//! Resistance and progression settings. Omitted penalties keep the loaded tables;
//! percentage 100 keeps the existing arithmetic, including its rounding.

use serde::Deserialize;

/// Non-monster resistance caps; monsters retain their uncapped immunity rules.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResistanceRules {
    pub elemental_cap: i32,
    pub elemental_max_cap: i32,
    pub physical_cap: i32,
    pub other_cap: i32,
    pub expansion_penalties: Option<[i32; 3]>,
    pub classic_penalties: Option<[i32; 3]>,
}

impl Default for ResistanceRules {
    fn default() -> Self {
        Self::defaults()
    }
}

impl ResistanceRules {
    pub(super) const fn defaults() -> Self {
        Self {
            elemental_cap: 75,
            elemental_max_cap: 95,
            physical_cap: 50,
            other_cap: 75,
            expansion_penalties: None,
            classic_penalties: None,
        }
    }

    pub fn penalty(&self, difficulty: usize, expansion: bool) -> Option<i32> {
        let penalties = if expansion {
            self.expansion_penalties
        } else {
            self.classic_penalties
        };
        penalties.and_then(|p| p.get(difficulty).copied())
    }
}

/// Scale an unsigned amount without overflow. The default returns the input exactly.
pub(crate) fn percent(value: u32, multiplier: u32) -> u32 {
    (u64::from(value) * u64::from(multiplier) / 100).min(u64::from(u32::MAX)) as u32
}
