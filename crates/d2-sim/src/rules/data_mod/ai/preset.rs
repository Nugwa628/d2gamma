//! The presets of `ai.toml`: D2Gamma's own monster AIs by name, with their
//! parameters (`aip1` to `aip8` of the monstats row) as named fields.
//!
//! The catalogue is read once from the AI table's description
//! (`tables/monsters/ai-functions.tsv`): the `monai` column names the preset
//! and the `aip_meaning` column names its knobs. A knob's field name is its
//! description in lower case with every run of other characters turned into
//! one `_` (`"att1/att2?"` is `att1_att2`, `"hurt%"` is `hurt_percent`). A
//! knob the table does not describe has no name and is set as `aip3` and so
//! on; `aip1` to `aip8` also work for the described ones.

use std::sync::OnceLock;

use crate::monsters::ai::table::AI_FUNCTIONS_TSV;

/// A parameter of a preset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Knob {
    /// The parameter, 1 to 8.
    pub aip: u8,
    /// The field name in `[ai.params]`.
    pub name: String,
}

/// One of D2Gamma's AIs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    /// The monstats `AI` value (the AI table row) it is.
    pub index: u16,
    /// The name used in `preset = "..."`.
    pub name: String,
    /// The described parameters, in `aip` order.
    pub knobs: Vec<Knob>,
}

impl Preset {
    /// The parameter (1 to 8) that `field` stands for: a knob name or
    /// `aip1` to `aip8`.
    pub fn aip_of(&self, field: &str) -> Option<u8> {
        if let Some(k) = self.knobs.iter().find(|k| k.name == field) {
            return Some(k.aip);
        }
        let n: u8 = field.strip_prefix("aip")?.parse().ok()?;
        (1..=8).contains(&n).then_some(n)
    }

    /// The field names, for messages: `approach (aip1), stall_time (aip2)`.
    pub fn field_list(&self) -> String {
        if self.knobs.is_empty() {
            return "aip1 to aip8".to_owned();
        }
        let named: Vec<String> = self
            .knobs
            .iter()
            .map(|k| format!("{} (aip{})", k.name, k.aip))
            .collect();
        format!("{}; or aip1 to aip8", named.join(", "))
    }
}

/// Every preset, by AI table row.
pub fn presets() -> &'static [Preset] {
    static ALL: OnceLock<Vec<Preset>> = OnceLock::new();
    ALL.get_or_init(|| parse(AI_FUNCTIONS_TSV))
}

/// The preset called `name` (any case).
pub fn find(name: &str) -> Option<&'static Preset> {
    presets().iter().find(|p| p.name.eq_ignore_ascii_case(name))
}

/// A knob description as a field name.
pub fn slug(meaning: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in meaning.trim().chars() {
        if c == '%' {
            out.push_str("_percent");
            gap = true;
        } else if c.is_ascii_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('_');
            }
            gap = false;
            out.push(c.to_ascii_lowercase());
        } else {
            gap = true;
        }
    }
    out
}

fn parse(tsv: &str) -> Vec<Preset> {
    let mut lines = tsv.lines();
    let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').collect();
    let col = |name: &str| header.iter().position(|h| *h == name);
    let (Some(index), Some(monai), Some(meaning)) =
        (col("index"), col("monai"), col("aip_meaning"))
    else {
        return Vec::new();
    };
    lines
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            let knobs = cells
                .get(meaning)
                .copied()
                .unwrap_or("-")
                .split(';')
                .filter_map(|part| {
                    let (key, text) = part.trim().split_once('=')?;
                    let aip = key.strip_prefix("aip")?.parse().ok()?;
                    let name = slug(text);
                    (!name.is_empty()).then_some(Knob { aip, name })
                })
                .collect();
            Some(Preset {
                index: cells.get(index)?.parse().ok()?,
                name: (*cells.get(monai)?).to_owned(),
                knobs,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monsters::ai::AI_TABLE;

    #[test]
    fn a_description_becomes_a_field_name() {
        assert_eq!(slug("approach?"), "approach");
        assert_eq!(slug("att1/att2?"), "att1_att2");
        assert_eq!(slug("hurt%"), "hurt_percent");
        assert_eq!(slug("stall time"), "stall_time");
        assert_eq!(slug("rez?/cmd?"), "rez_cmd");
        assert_eq!(slug("#of eggs"), "of_eggs");
        assert_eq!(slug("?"), "");
    }

    #[test]
    fn there_is_a_preset_for_every_ai_table_row() {
        let all = presets();
        assert_eq!(all.len(), AI_TABLE.len());
        for (row, p) in all.iter().enumerate() {
            assert_eq!(usize::from(p.index), row, "{}", p.name);
        }
    }

    #[test]
    fn preset_names_are_unique_and_plain() {
        let all = presets();
        for (i, p) in all.iter().enumerate() {
            assert!(
                p.name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "{}",
                p.name
            );
            assert!(
                !all[..i]
                    .iter()
                    .any(|o| o.name.eq_ignore_ascii_case(&p.name)),
                "{} is named twice",
                p.name
            );
        }
    }

    #[test]
    fn knob_names_are_unique_within_a_preset_and_never_look_like_raw_fields() {
        for p in presets() {
            for (i, k) in p.knobs.iter().enumerate() {
                assert!((1..=8).contains(&k.aip), "{}: aip{}", p.name, k.aip);
                assert!(
                    !p.knobs[..i]
                        .iter()
                        .any(|o| o.name == k.name || o.aip == k.aip),
                    "{}: {} is described twice",
                    p.name,
                    k.name
                );
                let raw = k
                    .name
                    .strip_prefix("aip")
                    .and_then(|n| n.parse::<u8>().ok());
                assert!(raw.is_none(), "{}: {}", p.name, k.name);
            }
        }
    }

    #[test]
    fn a_preset_is_found_by_name_and_its_fields_by_name_or_number() {
        let zombie = find("zombie").unwrap();
        assert_eq!(zombie.index, 3);
        assert_eq!(zombie.aip_of("approach"), Some(1));
        assert_eq!(zombie.aip_of("aware_dist"), Some(2));
        assert_eq!(zombie.aip_of("att1_att2"), Some(4));
        assert_eq!(zombie.aip_of("aip3"), Some(3));
        assert_eq!(zombie.aip_of("aip9"), None);
        assert_eq!(zombie.aip_of("speed"), None);
        assert!(zombie.field_list().contains("approach (aip1)"));
        assert!(find("no such ai").is_none());
        assert_eq!(find("UberBaal").unwrap().knobs.len(), 0);
    }

    #[test]
    fn a_catalogue_without_the_expected_columns_has_no_presets() {
        assert!(parse("a\tb\n1\t2").is_empty());
        assert!(parse("").is_empty());
    }
}
