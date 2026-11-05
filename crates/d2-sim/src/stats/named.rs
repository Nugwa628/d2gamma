//! Named mod stats and simultaneous integer transformations. Each behavior reads
//! the native evaluated totals before any behavior, so conversions cannot recurse
//! or depend on declaration order. The base game never installs this registry.

use super::{ids, StatInfo, StatTable, NO_STAT};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub const FIRST_CUSTOM_STAT: u16 = 32768;
const MAX_STATS: usize = 256;
const MAX_BEHAVIORS: usize = 512;

#[derive(Debug, thiserror::Error)]
#[error("invalid stats.toml: {0}")]
pub struct StatsError(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    name: String,
    #[serde(default)]
    id: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Add,
    Convert,
    PercentOf,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Behavior {
    kind: Kind,
    source: String,
    target: String,
    #[serde(default)]
    of: Option<String>,
    #[serde(default = "hundred")]
    percent: i32,
}
fn hundred() -> i32 {
    100
}
fn version() -> u32 {
    1
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default = "version")]
    version: u32,
    #[serde(default)]
    stat: Vec<Definition>,
    #[serde(default)]
    behavior: Vec<Behavior>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Operation {
    pub kind: Kind,
    pub source: u16,
    pub target: u16,
    pub of: Option<u16>,
    pub percent: i32,
}

/// Names and operations of a mod; ids are stable when explicitly assigned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatRules {
    custom: BTreeMap<String, u16>,
    pub(crate) operations: Vec<Operation>,
    targets: Vec<u16>,
}

fn normalized(name: &str) -> String {
    name.to_ascii_lowercase()
}
fn builtin(name: &str) -> Option<u16> {
    ids::NAMED_STATS
        .iter()
        .chain(ids::ALIASES.iter())
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, id)| *id)
}

impl StatRules {
    pub fn from_toml(text: &str) -> Result<Self, StatsError> {
        let file: File = toml::from_str(text).map_err(|e| StatsError(e.to_string()))?;
        if file.version != 1 {
            return Err(StatsError("version must be 1".into()));
        }
        if file.stat.len() > MAX_STATS || file.behavior.len() > MAX_BEHAVIORS {
            return Err(StatsError(
                "at most 256 stats and 512 behaviors are allowed".into(),
            ));
        }
        let mut result = Self::default();
        let mut definitions = file.stat;
        definitions.sort_by(|a, b| a.name.cmp(&b.name));
        let mut used = BTreeSet::new();
        for definition in &definitions {
            if let Some(id) = definition.id {
                if !(FIRST_CUSTOM_STAT..NO_STAT).contains(&id) || !used.insert(id) {
                    return Err(StatsError(format!(
                        "stat {}: id must be unique and in 32768..65534",
                        definition.name
                    )));
                }
            }
        }
        for definition in definitions {
            let name = normalized(&definition.name);
            if name.is_empty()
                || name.len() > 63
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                || name.as_bytes()[0].is_ascii_digit()
            {
                return Err(StatsError(format!("stat {}: use an ASCII name of 1..63 letters, digits or underscores, starting with a letter or underscore", definition.name)));
            }
            if builtin(&name).is_some() || result.custom.contains_key(&name) {
                return Err(StatsError(format!("stat {name}: name is already defined")));
            }
            let id = match definition.id {
                Some(id) => id,
                None => (FIRST_CUSTOM_STAT..NO_STAT)
                    .find(|id| !used.contains(id))
                    .ok_or_else(|| StatsError("no custom stat ids left".into()))?,
            };
            used.insert(id);
            result.custom.insert(name, id);
        }
        let mut converted = BTreeMap::<u16, i32>::new();
        for (index, behavior) in file.behavior.into_iter().enumerate() {
            let resolve = |name: &str| {
                result
                    .id(name)
                    .ok_or_else(|| StatsError(format!("behavior[{index}]: unknown stat {name}")))
            };
            let source = resolve(&behavior.source)?;
            let target = resolve(&behavior.target)?;
            let of = behavior.of.as_deref().map(resolve).transpose()?;
            if behavior.percent < 0
                || behavior.percent > 10000
                || (behavior.kind == Kind::PercentOf) != of.is_some()
            {
                return Err(StatsError(format!(
                    "behavior[{index}]: percent must be 0..10000; only percent_of requires of"
                )));
            }
            if behavior.kind == Kind::Convert {
                let total = converted.entry(source).or_default();
                *total += behavior.percent;
                if source == target || *total > 100 {
                    return Err(StatsError(format!("behavior[{index}]: conversion needs different stats and total outgoing percent <= 100")));
                }
                result.targets.push(source);
            }
            result.targets.push(target);
            result.operations.push(Operation {
                kind: behavior.kind,
                source,
                target,
                of,
                percent: behavior.percent,
            });
        }
        result.targets.sort_unstable();
        result.targets.dedup();
        Ok(result)
    }
    pub fn id(&self, name: &str) -> Option<u16> {
        self.custom
            .get(&normalized(name))
            .copied()
            .or_else(|| builtin(name))
    }
    pub fn custom_stats(&self) -> impl Iterator<Item = (&str, u16)> {
        self.custom.iter().map(|(n, id)| (n.as_str(), *id))
    }
    pub fn is_empty(&self) -> bool {
        self.custom.is_empty() && self.operations.is_empty()
    }
    pub(crate) fn targets(&self) -> &[u16] {
        &self.targets
    }
    /// Resolve named stat fields in skills, states and triggers before their
    /// existing schemas validate the file. Numeric fields retain their meaning.
    pub fn resolve_fields(&self, text: &str) -> Result<String, StatsError> {
        let mut value: toml::Value = toml::from_str(text).map_err(|e| StatsError(e.to_string()))?;
        fn walk(rules: &StatRules, value: &mut toml::Value) -> Result<(), StatsError> {
            match value {
                toml::Value::Table(table) => {
                    for (key, child) in table {
                        if matches!(key.as_str(), "stat" | "bonus_stat" | "while_stat") {
                            if let toml::Value::String(name) = child {
                                let id = rules.id(name).ok_or_else(|| {
                                    StatsError(format!("{key}: unknown stat {name}"))
                                })?;
                                *child = toml::Value::Integer(i64::from(id));
                            }
                        }
                        walk(rules, child)?;
                    }
                }
                toml::Value::Array(values) => {
                    for child in values {
                        walk(rules, child)?;
                    }
                }
                _ => {}
            }
            Ok(())
        }
        walk(self, &mut value)?;
        toml::to_string(&value).map_err(|e| StatsError(e.to_string()))
    }
    /// Validate against the effective base table; never replace a table row.
    pub fn install(&self, table: &mut StatTable) -> Result<(), StatsError> {
        let mut next = table.clone();
        for (name, id) in self.custom_stats() {
            if usize::from(id) < next.stats.len() {
                return Err(StatsError(format!(
                    "stat {name}: id {id} collides with itemstatcost"
                )));
            }
            next.custom.insert(id, StatInfo::custom());
        }
        for operation in &self.operations {
            for id in [Some(operation.source), Some(operation.target), operation.of]
                .into_iter()
                .flatten()
            {
                if next.get(id).is_none() {
                    return Err(StatsError(format!(
                        "behavior stat {id} is missing from itemstatcost"
                    )));
                }
            }
        }
        next.named = self.clone();
        *table = next;
        Ok(())
    }
}

/// Saturating new-rule math, with one truncation toward zero. `source_shift`
/// converts raw source points into target fixed-point units.
pub(crate) fn scaled(source: i32, percent: i32, source_shift: u8, target_shift: u8) -> i32 {
    let value = i128::from(source) * i128::from(percent) * (1i128 << target_shift.min(31))
        / (100i128 * (1i128 << source_shift.min(31)));
    value.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests;
