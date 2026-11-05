//! Declared crowd control: a mod's `files/crowd_control.toml`.
//!
//! With `crowd_control = "declared"` in `rules.toml`, a hit stuns, interrupts, knocks
//! back or freezes only when its source says so. This file names the sources the
//! engine cannot read from a skill part (a monster's attack, a missile row, a skill row);
//! a skill made of parts declares in its own `skills.toml` (`[[skill.part.cc]]`).
//!
//! ```toml
//! version = 1
//!
//! [[source]]
//! monster = 156                 # the monstats row
//! attack = "a1"                 # a1, a2, s1..s4; leave out for every attack
//! [[source.effect]]
//! kind = "stun"                 # interrupt, knockback, stun or freeze
//! seconds = 1.5                 # stun and freeze only
//! chance = 100                  # percent, default 100
//!
//! [[source]]
//! missile = 214                 # or `skill = N`: hits of missiles that skill made
//! [[source.effect]]
//! kind = "interrupt"
//! ```
//!
//! A mod may also name a state whose holders cannot be crowd-controlled at all:
//!
//! ```toml
//! immune_state = 900            # a unit with this state takes no stun, freeze,
//!                               # interrupt or knockback, declared or not
//! ```
//!
//! Monster AI graphs give it for a while at a phase change (`ai.toml`, `immune_seconds`).
//!
//! The same declaration works under `crowd_control = "classic"`: it adds the effect to
//! what D2Gamma's rules already do.

use serde::Deserialize;

use crate::combat::cc::{CcEffect, CcKind, CcSource, MAX_EFFECTS, MAX_TICKS};
use crate::units::modes::monster_mode;

/// The one version of `crowd_control.toml` this build reads.
pub const VERSION: u32 = 1;
/// Most sources in one file.
pub const MAX_SOURCES: usize = 512;

/// A `crowd_control.toml` or a `cc` entry that cannot be used. The message names
/// the entry and the key.
#[derive(Debug, thiserror::Error)]
pub enum CcError {
    #[error("invalid crowd_control.toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid crowd_control.toml setting: {0}")]
    Setting(String),
}

/// One `[[...effect]]` / `[[skill.part.cc]]` as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEffect {
    kind: Kind,
    #[serde(default)]
    seconds: Option<f64>,
    #[serde(default)]
    chance: Option<u32>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Interrupt,
    Knockback,
    Stun,
    Freeze,
}

/// A length in seconds as base ticks (25 a second, rounded, at least 1); `None`
/// when it is not above 0 or is over the engine's stun cap. The one float step is
/// at load time, IEEE-exact, and leaves an integer: nothing in the sim runs on it.
#[allow(clippy::float_arithmetic)]
fn seconds_to_ticks(s: f64) -> Option<i32> {
    let ticks = s * 25.0;
    (s > 0.0 && ticks <= f64::from(MAX_TICKS)).then(|| (ticks.round() as i32).max(1))
}

impl RawEffect {
    fn into_effect(self) -> Result<CcEffect, String> {
        let chance = self.chance.unwrap_or(100);
        if !(1..=100).contains(&chance) {
            return Err(format!("`chance`: {chance} is not in 1..=100"));
        }
        let timed = matches!(self.kind, Kind::Stun | Kind::Freeze);
        let ticks = match (timed, self.seconds.map(|s| (s, seconds_to_ticks(s)))) {
            (true, Some((_, Some(t)))) => t,
            (true, Some((s, None))) => {
                return Err(format!(
                    "`seconds`: {s} is not in 0..={} (the engine's stun cap)",
                    MAX_TICKS / 25
                ))
            }
            (true, None) => return Err("`seconds`: a stun or freeze needs a length".into()),
            (false, Some(_)) => {
                return Err("`seconds`: only a stun or freeze has a length".into());
            }
            (false, None) => 0,
        };
        Ok(CcEffect {
            kind: match self.kind {
                Kind::Interrupt => CcKind::Interrupt,
                Kind::Knockback => CcKind::Knockback,
                Kind::Stun => CcKind::Stun,
                Kind::Freeze => CcKind::Freeze,
            },
            chance: chance as u8,
            ticks,
        })
    }
}

/// Reads a list of effects (used by `skills.toml` parts for their `cc` key).
pub fn effects<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<CcEffect>, D::Error> {
    use serde::de::Error;
    let raw = Vec::<RawEffect>::deserialize(d)?;
    if raw.len() > MAX_EFFECTS {
        return Err(D::Error::custom(format!(
            "{} effects; a source declares at most {MAX_EFFECTS}",
            raw.len()
        )));
    }
    raw.into_iter()
        .enumerate()
        .map(|(i, e)| {
            e.into_effect()
                .map_err(|m| D::Error::custom(format!("cc[{i}].{m}")))
        })
        .collect()
}

/// Reads one effect (used by the `crowd_control` trigger action).
pub fn effect<'de, D: serde::Deserializer<'de>>(d: D) -> Result<CcEffect, D::Error> {
    use serde::de::Error;
    RawEffect::deserialize(d)?
        .into_effect()
        .map_err(D::Error::custom)
}

/// Where a `crowd_control.toml` source comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum From {
    /// `attack` is the monster mode; `None`: every attack mode.
    Monster {
        class: u16,
        mode: Option<u32>,
    },
    Missile(u16),
    Skill(u16),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Source {
    from: From,
    effects: Vec<CcEffect>,
}

/// All the declared sources of a mod.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CcRules {
    sources: Vec<Source>,
    immune_state: Option<u16>,
}

impl CcRules {
    /// No declarations.
    pub fn none() -> CcRules {
        CcRules::default()
    }

    /// `true` when the file declares nothing.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.immune_state.is_none()
    }

    /// The state that makes its holder immune to crowd control, if the mod names one.
    pub fn immune_state(&self) -> Option<u16> {
        self.immune_state
    }

    /// How many sources the file has.
    pub fn len(&self) -> usize {
        self.sources.len()
    }

    /// Reads a `crowd_control.toml`.
    pub fn from_toml(text: &str) -> Result<CcRules, CcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawSource {
            #[serde(default)]
            monster: Option<u16>,
            #[serde(default)]
            attack: Option<String>,
            #[serde(default)]
            missile: Option<u16>,
            #[serde(default)]
            skill: Option<u16>,
            #[serde(default, rename = "effect")]
            effects: Vec<toml::Value>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct File {
            #[serde(default = "version")]
            version: u32,
            #[serde(default)]
            immune_state: Option<u16>,
            #[serde(default, rename = "source")]
            sources: Vec<RawSource>,
        }
        fn version() -> u32 {
            VERSION
        }
        let file: File = toml::from_str(text)?;
        if file.version != VERSION {
            return Err(CcError::Setting(format!(
                "`version`: {} is not supported (this build reads version {VERSION})",
                file.version
            )));
        }
        if file.sources.len() > MAX_SOURCES {
            return Err(CcError::Setting(format!(
                "{} sources; a file has at most {MAX_SOURCES}",
                file.sources.len()
            )));
        }
        let mut sources = Vec::with_capacity(file.sources.len());
        for (i, raw) in file.sources.into_iter().enumerate() {
            let bad =
                |key: &str, msg: String| CcError::Setting(format!("`source[{i}].{key}`: {msg}"));
            let named = [
                raw.monster.is_some(),
                raw.missile.is_some(),
                raw.skill.is_some(),
            ]
            .iter()
            .filter(|b| **b)
            .count();
            if named != 1 {
                return Err(bad(
                    "monster",
                    "give exactly one of `monster`, `missile` or `skill`".into(),
                ));
            }
            if raw.attack.is_some() && raw.monster.is_none() {
                return Err(bad(
                    "attack",
                    "only a `monster` source has an attack".into(),
                ));
            }
            let from = if let Some(class) = raw.monster {
                let mode = match raw.attack.as_deref() {
                    None | Some("any") => None,
                    Some("a1") => Some(monster_mode::A1),
                    Some("a2") => Some(monster_mode::A2),
                    Some("s1") => Some(monster_mode::S1),
                    Some("s2") => Some(monster_mode::S2),
                    Some("s3") => Some(monster_mode::S3),
                    Some("s4") => Some(monster_mode::S4),
                    Some(other) => {
                        return Err(bad(
                            "attack",
                            format!("`{other}` is not a1, a2, s1, s2, s3, s4 or any"),
                        ))
                    }
                };
                From::Monster { class, mode }
            } else if let Some(row) = raw.missile {
                From::Missile(row)
            } else {
                From::Skill(raw.skill.unwrap_or(0))
            };
            if raw.effects.is_empty() || raw.effects.len() > MAX_EFFECTS {
                return Err(bad(
                    "effect",
                    format!(
                        "{} effects; a source has 1 to {MAX_EFFECTS}",
                        raw.effects.len()
                    ),
                ));
            }
            let mut effects = Vec::with_capacity(raw.effects.len());
            for (j, value) in raw.effects.into_iter().enumerate() {
                let effect = RawEffect::deserialize(value)
                    .map_err(|e| e.message().to_owned())
                    .and_then(RawEffect::into_effect)
                    .map_err(|m| CcError::Setting(format!("`source[{i}].effect[{j}]`: {m}")))?;
                effects.push(effect);
            }
            sources.push(Source { from, effects });
        }
        Ok(CcRules {
            sources,
            immune_state: file.immune_state,
        })
    }

    /// The effects declared for `source`, in file order.
    pub fn declared(&self, source: CcSource) -> Vec<CcEffect> {
        let mut out = Vec::new();
        for s in &self.sources {
            let hit = match (s.from, source) {
                (From::Monster { class, mode }, CcSource::Monster { class: c, mode: m }) => {
                    u32::from(class) == c && mode.is_none_or(|x| x == m)
                }
                (From::Missile(row), CcSource::Missile { row: r, .. }) => row == r,
                (From::Skill(id), CcSource::Missile { skill, .. }) => i32::from(id) == skill,
                _ => false,
            };
            if hit {
                out.extend_from_slice(&s.effects);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;
