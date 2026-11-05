//! The mod's named, typed network contract. No raw packet layouts are authored.
use crate::ui::Action;
use std::collections::BTreeSet;
use toml::{Table, Value};
pub const MAX_DOCUMENT: usize = 256 * 1024;
pub const MAX_VALUES: usize = 256;
pub const MAX_ACTIONS: usize = 256;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Definition {
    pub values: Vec<SyncedValue>,
    pub actions: Vec<ServerAction>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncedValue {
    pub id: u16,
    pub name: String,
    pub source: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerAction {
    pub id: u16,
    pub rule: Action,
}
fn string(t: &mut Table, key: &str) -> Result<String, String> {
    match t.remove(key) {
        Some(Value::String(s)) => Ok(s),
        _ => Err(format!("net.{key}: expected string")),
    }
}
fn id(t: &mut Table) -> Result<u16, String> {
    match t.remove("id") {
        Some(Value::Integer(n)) if (1..65535).contains(&n) => Ok(n as u16),
        _ => Err("net.id: expected 1..65534".into()),
    }
}
fn named(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
fn source(s: &str) -> bool {
    s.split_once('.').is_some_and(|(kind, key)| {
        matches!(kind, "stat" | "state" | "var")
            && !key.is_empty()
            && key.len() <= 128
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(&b))
    })
}
fn array(t: &mut Table, key: &str, cap: usize) -> Result<Vec<Table>, String> {
    match t.remove(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(a)) if a.len() <= cap => a
            .into_iter()
            .map(|v| match v {
                Value::Table(t) => Ok(t),
                _ => Err(format!("net.{key}: expected tables")),
            })
            .collect(),
        _ => Err(format!("net.{key}: expected at most {cap} entries")),
    }
}
impl Definition {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_DOCUMENT {
            return Err("files/net.toml exceeds 256 KiB".into());
        }
        let mut t: Table = text.parse().map_err(|e| format!("files/net.toml: {e}"))?;
        if t.remove("version") != Some(Value::Integer(1)) {
            return Err("net.version: expected 1; this mod needs updating".into());
        }
        let mut values = Vec::new();
        let (mut ids, mut names) = (BTreeSet::new(), BTreeSet::new());
        for mut v in array(&mut t, "values", MAX_VALUES)? {
            let id = id(&mut v)?;
            let name = string(&mut v, "name")?;
            let source = string(&mut v, "source")?;
            if !named(&name)
                || !self::source(&source)
                || !ids.insert(id)
                || !names.insert(name.clone())
                || !v.is_empty()
            {
                return Err(format!(
                    "net.values.{name}: invalid/duplicate id, name, source or unknown field"
                ));
            }
            values.push(SyncedValue { id, name, source });
        }
        let mut actions = Vec::new();
        ids.clear();
        names.clear();
        for mut v in array(&mut t, "actions", MAX_ACTIONS)? {
            let id = id(&mut v)?;
            let rule = Action::parse(v)?;
            if !ids.insert(id) || !names.insert(rule.name.clone()) {
                return Err("net.actions: duplicate id or name".into());
            }
            actions.push(ServerAction { id, rule });
        }
        if let Some(key) = t.keys().next() {
            return Err(format!("net: unknown field {key}"));
        }
        values.sort_by_key(|v| v.id);
        actions.sort_by_key(|v| v.id);
        Ok(Self { values, actions })
    }
    pub fn empty(&self) -> bool {
        self.values.is_empty() && self.actions.is_empty()
    }
}
/// A semantic contract fingerprint salted with the ordered mod IDs/versions.
/// Parsed declarations are sorted, so TOML formatting does not change it.
pub fn fingerprint(definition: &Definition, versions: &[(String, String)]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"LCUI/net/1");
    h.update(format!("{definition:?}"));
    for (id, version) in versions {
        h.update((id.len() as u64).to_le_bytes());
        h.update(id);
        h.update((version.len() as u64).to_le_bytes());
        h.update(version);
    }
    h.finalize().into()
}

impl Definition {
    /// Resolve authored names to transport IDs only in the runtime copy.
    pub fn resolve_panel(&self, panel: &mut crate::ui::Panel) -> Result<(), String> {
        for e in &mut panel.elements {
            if let Some(b) = &e.binding {
                for reference in std::iter::once(&b.source).chain(b.maximum_source.iter()) {
                    if let Some(name) = reference.strip_prefix("sync.") {
                        if !self.values.iter().any(|v| v.name == name) {
                            return Err(format!("unknown synced value {name}"));
                        }
                    }
                }
            }
            if let Some(action) = &mut e.action {
                if let Some(name) = action.strip_prefix("net.") {
                    let a = self
                        .actions
                        .iter()
                        .find(|a| a.rule.name == name)
                        .ok_or_else(|| format!("unknown network action {name}"))?;
                    *action = format!("net.{}", a.id);
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn net_schema_is_strict_bounded_and_empty_by_default() {
        assert!(Definition::default().empty());
        assert!(Definition::parse("version=1").unwrap().empty());
        let valid="version=1\n[[values]]\nid=2\nname='rage'\nsource='var.combat/rage'\n[[actions]]\nid=1\nname='allocate'\nstat='strength'\ndelta=1";
        let d = Definition::parse(valid).unwrap();
        assert_eq!(d.values[0].source, "var.combat/rage");
        assert_eq!(d.actions[0].rule.delta, 1);
        for bad in [
            valid.replace("version=1", "version=2"),
            valid.replace("id=2", "id=0"),
            valid.replace("id=2", "id=65535"),
            valid.replace("source='var.combat/rage'", "source='client.any'"),
            valid.replace("delta=1", "delta=0"),
            format!("{valid}\nunknown=true"),
            format!("{valid}\n[[values]]\nid=2\nname='other'\nsource='stat.strength'"),
            " ".repeat(MAX_DOCUMENT + 1),
            format!(
                "version=1\n{}",
                "[[values]]\nid=1\nname='x'\nsource='stat.x'\n".repeat(MAX_VALUES + 1)
            ),
        ] {
            assert!(Definition::parse(&bad).is_err(), "{bad:.100}");
        }
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;
    #[test]
    fn net_schema_bounds_actions_and_rejects_unknown_fields_and_duplicate_names() {
        let action = "[[actions]]\nid=1\nname='allocate'\nstat='strength'\ndelta=1\n";
        for body in [action.repeat(MAX_ACTIONS+1),format!("{action}{action}"),format!("{action}unknown=true\n"),"[[values]]\nid=1\nname='x'\nsource='stat.x'\n[[values]]\nid=2\nname='x'\nsource='stat.y'".into()]{assert!(Definition::parse(&format!("version=1\n{body}")).is_err());}
        let text="version=1\n[[values]]\nid=2\nname='two'\nsource='state.frozen'\n[[values]]\nid=1\nname='one'\nsource='stat.strength'";
        assert_eq!(Definition::parse(text).unwrap().values[0].id, 1);
    }
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;
    #[test]
    fn network_identity_ignores_toml_formatting_and_checks_version_source_and_ids() {
        let a = Definition::parse("version=1\n[[values]]\nid=1\nname='a'\nsource='stat.strength'")
            .unwrap();
        let b = Definition::parse(
            "# formatting\nversion = 1\n[[values]]\nsource='stat.strength'\nname='a'\nid=1",
        )
        .unwrap();
        let versions = vec![("alpha".into(), "1.0.0".into())];
        assert_eq!(fingerprint(&a, &versions), fingerprint(&b, &versions));
        assert_ne!(
            fingerprint(&a, &versions),
            fingerprint(&a, &[("alpha".into(), "2.0.0".into())])
        );
        let mut other = a.clone();
        other.values[0].id = 2;
        assert_ne!(fingerprint(&a, &versions), fingerprint(&other, &versions));
        other = a.clone();
        other.values[0].source = "state.frozen".into();
        assert_ne!(fingerprint(&a, &versions), fingerprint(&other, &versions));
    }
}
