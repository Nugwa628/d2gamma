//! Display names and the supported difficulty count of a data mod.
//! Numeric ids are row indexes in difficultylevels; existing three-column
//! tables use their last column beyond Hell.
use serde::Deserialize;

pub const MAX_DIFFICULTIES: usize = 255;
pub const BASE_NAMES: [&str; 3] = ["Normal", "Nightmare", "Hell"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difficulties {
    names: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct DifficultyError(String);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    names: Vec<String>,
}

impl Default for Difficulties {
    fn default() -> Self {
        Self {
            names: BASE_NAMES.map(str::to_owned).into(),
        }
    }
}

impl Difficulties {
    pub fn from_toml(text: &str) -> Result<Self, DifficultyError> {
        let file: File = toml::from_str(text).map_err(|e| DifficultyError(e.to_string()))?;
        if file.version != 1 || file.names.is_empty() || file.names.len() > MAX_DIFFICULTIES {
            return Err(DifficultyError(
                "version must be 1; names needs 1..255 entries".into(),
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for name in &file.names {
            if name.trim() != name
                || name.is_empty()
                || name.chars().count() > 63
                || name.chars().any(char::is_control)
                || !seen.insert(name.to_lowercase())
            {
                return Err(DifficultyError(
                    "names must be unique, nonempty text of at most 63 characters".into(),
                ));
            }
        }
        Ok(Self { names: file.names })
    }
    pub fn names(&self) -> &[String] {
        &self.names
    }
    pub fn count(&self) -> usize {
        self.names.len()
    }
    pub fn contains(&self, difficulty: u8) -> bool {
        usize::from(difficulty) < self.count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_three_and_data_can_add_a_fourth() {
        assert_eq!(Difficulties::default().names(), BASE_NAMES);
        assert!(crate::rules::Ruleset::Original.difficulties().is_none());
        assert!(
            crate::rules::DataMod::from_files("empty", &crate::rules::DataModFiles::default())
                .unwrap()
                .into_ruleset()
                .difficulties()
                .is_none()
        );
        let d = Difficulties::from_toml("version=1\nnames=['Dawn','Day','Dusk','Night']").unwrap();
        assert!(d.contains(3));
        assert!(!d.contains(4));
        assert_eq!(d.names()[3], "Night");
    }
    #[test]
    fn bad_versions_counts_names_and_unknown_fields_fail() {
        for text in [
            "version=2\nnames=['A']",
            "version=1\nnames=[]",
            "version=1\nnames=['a','A']",
            "version=1\nnames=[' a']",
            "version=1\nnames=['A']\nunknown=1",
        ] {
            assert!(Difficulties::from_toml(text).is_err(), "{text}");
        }
        let error = crate::rules::DataMod::from_files(
            "bad",
            &crate::rules::DataModFiles {
                difficulties: Some(("mods/bad/files/difficulties.toml", "version=2\nnames=['A']")),
                ..crate::rules::DataModFiles::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("mods/bad/files/difficulties.toml"));
        let names = vec!["a"; MAX_DIFFICULTIES + 1];
        assert!(Difficulties::from_toml(&format!("version=1\nnames={names:?}")).is_err());
    }
}
