//! Curated, deterministic rules from a mod's `rules.toml`. The authoritative host
//! parses and installs these before creating a game; the simulation performs no I/O.

use serde::Deserialize;

use super::{ModRules, Ruleset};
use crate::units::UnitType;

mod feel;
mod progression;
use crate::treasure::rules::DropRules;
pub use feel::{AnimationRules, ControlRules};
pub(crate) use progression::percent;
pub use progression::ResistanceRules;

/// D2Gamma's unchanged settings, also used by worlds without a mod.
pub static DEFAULT: GlobalRules = GlobalRules::defaults();

/// Version 1 of `rules.toml`. Missing tables and keys retain D2Gamma's behaviour.
/// Percent multipliers use integer math: 100 is unchanged, 200 doubles duration.
///
/// | Setting | Default | Meaning |
/// | --- | --- | --- |
/// | `version` | 1 | File schema version; unsupported versions are rejected. |
/// | `drops.players_setting` | true | Honor the single-player /players floor. |
/// | `drops.party_bonus` | true | Nearby party members count fully for NoDrop. |
/// | `drops.other_player_divisor` | 2 | Other players contribute one divided by this integer (1..=8). |
/// | `drops.monster_spawn_cap` | true | Cap NoDrop exponent at the monster's spawn-time count. |
/// | `drops.magic_find_percent` | 100 | Scale MF before quality diminishing returns (0..=10000). |
/// | `drops.magic_find_cap` | omitted | Cap scaled MF (0..=10000); negative values remain negative. |
/// | `drops.nodrop_percent` | 100 | Scale raw TC NoDrop before player exponent (0..=10000). |
/// | `socket_level_bands` | [25, 40] | Inclusive upper levels for first and second itemtypes socket bands. |
/// | `runeword_max_runes` | 6 | Maximum matching rune count (0..=6); 0 disables table runewords. |
/// | `death_gold_percent` | 100 | Scale death gold loss before protection and allocation (0..=10000). |
/// | `hireling_experience_curve_percent` | 100 | Scale hireling thresholds and initial offer XP (1..=10000). |
/// | `resistances.elemental_cap` | 75 | Non-monster base cap for rows with a maximum-resistance stat (including magic), before its bonuses. |
/// | `resistances.elemental_max_cap` | 95 | Ceiling after maximum-resistance bonuses. |
/// | `resistances.physical_cap` | 50 | Non-monster physical damage resistance cap. |
/// | `resistances.other_cap` | 75 | Cap for remaining resistance rows without a maximum-resistance stat, including poison-length reduction. |
/// | `resistances.expansion_penalties` | omitted | Normal/Nightmare/Hell penalty overrides; omitted uses difficulty tables. |
/// | `resistances.classic_penalties` | omitted | Normal/Nightmare/Hell penalty overrides; omitted uses 0/-20/-50. |
/// | `experience_curve_percent` | 100 | Scale player XP thresholds, including formula-provided thresholds; does not change kill XP or hireling curves. |
/// | `death_experience_percent` | 100 | Scale the computed death XP loss before the current-level floor and corpse recovery. |
/// | `players.hit_recovery.enabled`, `monsters.hit_recovery.enabled` | true | Allow damage to interrupt with a get-hit animation, including stunned reactions. |
/// | `players.hit_recovery.threshold_denominator`, `monsters.hit_recovery.threshold_denominator` | omitted | Preserve hit-class divisors and random bands. Set N to require max life / N damage instead; tiny and poison-only hits still do not interrupt. |
/// | `players.hit_recovery.duration_percent`, `monsters.hit_recovery.duration_percent` | 100 | Scale get-hit animation duration via its animation speed. |
/// | `crowd_control` | "classic" | `"declared"`: stun, interrupt (get-hit), knockback and freeze happen only from sources that declare them (see `crowd_control.toml`). `"classic"` is D2Gamma's rule. |
/// | `players.crowd_control`, `monsters.crowd_control` | omitted | Override `crowd_control` for hits on that unit class. |
/// | `cast.uninterruptible_after_percent` | omitted | Percent (0–100) of a skill's cast animation after which a hit or a new action cannot cancel it; omitted or 100: cancellable until the cast ends. |
/// | `[[cast.skill]]` `id`, `uninterruptible_after_percent` | none | The same, for one skills row. |
/// | `block_animation` | true | Play block reactions; disabling leaves damage prevention intact. |
/// | `max_block_chance` | 75 | Maximum shield block chance in percent, before the running penalty. |
/// | `players.knockback.enabled`, `monsters.knockback.enabled` | true | Allow knockback reactions. |
/// | `players.knockback.duration_percent`, `monsters.knockback.duration_percent` | 100 | Scale knockback travel duration via velocity and animation speed; distance is unchanged. |
/// | `players.stun.enabled`, `monsters.stun.enabled` | true | Allow stun state application. |
/// | `players.stun.duration_percent`, `monsters.stun.duration_percent` | 100 | Scale stun duration after the existing duration caps. |
/// | `players.freeze.enabled`, `monsters.freeze.enabled` | true | Allow freeze application, including its existing fallback to cold slow on players and immune monster classes. Ordinary cold damage is unaffected. |
/// | `players.freeze.duration_percent`, `monsters.freeze.duration_percent` | 100 | Scale freeze duration, including its cold fallback, before difficulty scaling. |
/// | `animation.classic_25hz` | `false` | Opt into 40 ms animation steps; simulation and movement remain 60 Hz. |
/// | `animation.diminishing` | `[120,120,120,120,150]` | Coefficients for attack/recovery/cast/block/movement, each 1–10000. |
/// | `animation.attack_percent` | `[15,175]` | Inclusive lower/upper attack-rate percent bounds, ordered in 1–10000. |
/// | `animation.cast_base_percent` | `100` | Base cast percent before diminished bonus, 1–10000. |
/// | `animation.cast_max_percent` | `175` | Maximum cast percent, 1–10000. |
/// | `animation.recovery_base_percent` | `50` | Base hit-recovery percent, 1–10000. |
/// | `animation.block_base_percent` | `50` | Base block percent without Holy Shield, 1–10000. |
/// | `animation.holy_shield_block_percent` | `100` | Base block percent with Holy Shield, 1–10000. |
/// | `animation.block_min_speed` | `1` | Block animation speed floor, 1 through speed_cap. |
/// | `animation.movement_min_percent` | `25` | Walk/run movement percent floor, 1–10000. |
/// | `animation.movement_max_percent` | omitted | Optional walk/run movement percent cap, at least the floor and at most 10000. |
/// | `animation.other_percent` | `[15,175]` | Inclusive percent bounds for other animation modes, ordered in 1–10000. |
/// | `animation.speed_cap` | `32767` | Animation speed ceiling in 8.8 units, 1–32767. |
/// | `animation.sequence_attack_penalty` | `30` | Percent subtracted for player sequence attacks, 0–10000. |
/// | `animation.player_walk_speed` | `213` | Player walk animation base speed in 8.8 units, 1–10000. |
/// | `animation.player_run_speed` | `101` | Player run animation base speed in 8.8 units, 1–10000. |
/// | `animation.frame_bonus` | D2Gamma table | 12 weapon-type rows × 7 player-class columns, each 0–2 frames. |
/// | `target.hit_recovery.minimum_damage` | `256` | Minimum damage in 8.8 life units, 0–2147483647. |
/// | `target.hit_recovery.divisors` | `[8,16,32,64]` | Life divisors for light/default/heavy/long-sword hit classes, each 4–1000000. |
/// | `target.hit_recovery.band_denominators` | `[2,4]` | Divide the selected divisor by these ordered values for the seeded bands, each 1 through the smallest divisor. |
/// | `target.hit_recovery.band_roll_sides` | `[2,4]` | Reject each band when its seeded draw is zero; powers of two 1–256. |
/// | `target.control.stun_max_frames` | `250` | General stun cap before duration scaling, 1–1000000 data frames. |
/// | `monsters.control.hireling_stun_max_frames` | `13` | Hireling stun cap, 1–1000000 data frames; replaces the general cap once reached. |
/// | `monsters.control.unique_stun_resist_percent` | `90` | Unique monster stun resistance, 0–100 percent. Bosses and immobile monsters retain immunity. |
/// | `target.control.freeze_max_frames` | omitted | Optional freeze cap before duration scaling and difficulty; 1–1000000 data frames. Also caps the player/immune-monster cold fallback. |
/// | `monsters.control.freeze_divisors` | omitted | Normal/Nightmare/Hell freeze divisors, each 1–1000000. Omitted reads difficulty tables. Cold fallback keeps cold-divisor rules. |
/// | `target.control.knockback_velocity` | `4096` | Knockback path speed before duration scaling, 1–32767 fixed-point units per data frame. |
/// | `target.control.knockback_distance` | `5` | Knockback path distance budget, 1–255. |
/// | `monsters.control.sandleaper_knockback_distance` | `10` | Distance budget for the leaping monster family, 1–255. |
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GlobalRules {
    pub version: u32,
    pub drops: DropRules,
    pub animation: AnimationRules,
    pub socket_level_bands: [u32; 2],
    pub runeword_max_runes: u32,
    pub death_gold_percent: u32,
    pub hireling_experience_curve_percent: u32,
    pub resistances: ResistanceRules,
    pub experience_curve_percent: u32,
    pub death_experience_percent: u32,
    pub players: UnitRules,
    pub monsters: UnitRules,
    pub block_animation: bool,
    pub max_block_chance: u32,
    pub crowd_control: CrowdControl,
    pub cast: CastRules,
}

/// Which hits may stun, interrupt, knock back or freeze a unit class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CrowdControl {
    /// D2Gamma's rule: any hit may interrupt by its size, missile rows and skills
    /// stun, freeze and shove.
    #[default]
    Classic,
    /// Only hits whose source declared the effect (a skill part, a
    /// `crowd_control.toml` source, an item's stun property, a trigger).
    Declared,
}

/// When a skill's cast can no longer be cancelled by a hit. See [`GlobalRules`]'
/// setting table.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CastRules {
    /// The default for every skill, in percent of the cast animation; omitted (or
    /// 100): a cast can be cancelled until it ends, as D2Gamma does.
    pub uninterruptible_after_percent: Option<u32>,
    /// Per-skill points, `[[cast.skill]]`.
    #[serde(rename = "skill")]
    pub skills: Vec<CastSkillRule>,
}

/// One `[[cast.skill]]`: that skill's point.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastSkillRule {
    /// The skills row.
    pub id: u16,
    /// Percent of the cast animation, 0 (from the first frame) to 100 (never).
    pub uninterruptible_after_percent: u32,
}

/// Most `[[cast.skill]]` entries.
pub const MAX_CAST_SKILLS: usize = 512;

impl CastRules {
    /// The point of `skill` in percent of its cast: its own entry, else the default;
    /// `None` when the cast can be cancelled until it ends.
    pub fn point_for(&self, skill: i32) -> Option<u32> {
        let own = self
            .skills
            .iter()
            .find(|s| i32::from(s.id) == skill)
            .map(|s| s.uninterruptible_after_percent);
        own.or(self.uninterruptible_after_percent)
            .filter(|p| *p < 100)
    }

    /// `true` when no skill has a point (D2Gamma's rule).
    pub fn is_off(&self) -> bool {
        self.uninterruptible_after_percent.is_none() && self.skills.is_empty()
    }

    fn check(&self) -> Result<(), RulesError> {
        let bad = |key: String| RulesError::Setting(format!("{key} must be 0..=100"));
        if self.uninterruptible_after_percent.is_some_and(|p| p > 100) {
            return Err(bad("cast.uninterruptible_after_percent".into()));
        }
        if self.skills.len() > MAX_CAST_SKILLS {
            return Err(RulesError::Setting(format!(
                "cast.skill has {} entries; at most {MAX_CAST_SKILLS}",
                self.skills.len()
            )));
        }
        for (i, s) in self.skills.iter().enumerate() {
            if s.uninterruptible_after_percent > 100 {
                return Err(bad(format!(
                    "cast.skill[{i}].uninterruptible_after_percent"
                )));
            }
            if self.skills[..i].iter().any(|o| o.id == s.id) {
                return Err(RulesError::Setting(format!(
                    "cast.skill[{i}].id {} is listed twice",
                    s.id
                )));
            }
        }
        Ok(())
    }
}

/// Rules selected by the affected unit's actual type, including transformed players.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct UnitRules {
    pub hit_recovery: HitRecovery,
    pub control: ControlRules,
    pub knockback: TimedRule,
    pub stun: TimedRule,
    pub freeze: TimedRule,
    pub crowd_control: Option<CrowdControl>,
}

/// A crowd-control switch and duration multiplier. See [`GlobalRules`]' setting table.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TimedRule {
    pub enabled: bool,
    pub duration_percent: u32,
}

impl Default for TimedRule {
    fn default() -> Self {
        Self::defaults()
    }
}

impl TimedRule {
    const fn defaults() -> Self {
        Self {
            enabled: true,
            duration_percent: 100,
        }
    }

    /// Scale a duration with saturation; disabled effects return zero.
    pub fn duration(&self, ticks: i32) -> i32 {
        if !self.enabled {
            return 0;
        }
        (i64::from(ticks) * i64::from(self.duration_percent) / 100)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }

    /// Inverse duration scaling for animation and travel speeds. Zero duration
    /// disables the reaction; positive speeds remain at least one unit.
    pub fn speed(&self, speed: i32, maximum: i32) -> i32 {
        if self.duration_percent == 100 {
            return speed;
        }
        (i64::from(speed) * 100 / i64::from(self.duration_percent.max(1)))
            .clamp(1, i64::from(maximum)) as i32
    }

    pub fn active(&self) -> bool {
        self.enabled && self.duration_percent != 0
    }
}

/// Damage interruption settings; the omitted threshold keeps the seeded bands.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HitRecovery {
    pub enabled: bool,
    pub threshold_denominator: Option<u32>,
    pub minimum_damage: i32,
    pub divisors: [i32; 4],
    pub band_denominators: [i32; 2],
    pub band_roll_sides: [u32; 2],
    pub duration_percent: u32,
}

impl Default for HitRecovery {
    fn default() -> Self {
        Self::defaults()
    }
}

impl HitRecovery {
    const fn defaults() -> Self {
        Self {
            enabled: true,
            threshold_denominator: None,
            minimum_damage: 256,
            divisors: [8, 16, 32, 64],
            band_denominators: [2, 4],
            band_roll_sides: [2, 4],
            duration_percent: 100,
        }
    }

    pub fn timing(&self) -> TimedRule {
        TimedRule {
            enabled: self.enabled,
            duration_percent: self.duration_percent,
        }
    }
}

impl UnitRules {
    const fn defaults() -> Self {
        Self {
            hit_recovery: HitRecovery::defaults(),
            control: ControlRules::defaults(),
            knockback: TimedRule::defaults(),
            stun: TimedRule::defaults(),
            freeze: TimedRule::defaults(),
            crowd_control: None,
        }
    }
}

/// Invalid TOML or an unsupported/out-of-range rule. Diagnostics include the key.
#[derive(Debug, thiserror::Error)]
pub enum RulesError {
    #[error("invalid rules.toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid rules.toml setting: {0}")]
    Setting(String),
}

impl Default for GlobalRules {
    fn default() -> Self {
        Self::defaults()
    }
}

impl GlobalRules {
    const fn defaults() -> Self {
        Self {
            version: 1,
            drops: DropRules::defaults(),
            animation: AnimationRules::defaults(),
            socket_level_bands: [25, 40],
            runeword_max_runes: 6,
            death_gold_percent: 100,
            hireling_experience_curve_percent: 100,
            resistances: ResistanceRules::defaults(),
            experience_curve_percent: 100,
            death_experience_percent: 100,
            players: UnitRules::defaults(),
            monsters: UnitRules::defaults(),
            block_animation: true,
            max_block_chance: 75,
            crowd_control: CrowdControl::Classic,
            cast: CastRules {
                uninterruptible_after_percent: None,
                skills: Vec::new(),
            },
        }
    }

    pub fn from_toml(text: &str) -> Result<Self, RulesError> {
        let rules: Self = toml::from_str(text)?;
        if rules.version != 1 {
            return Err(RulesError::Setting("version must be 1".into()));
        }
        rules.cast.check()?;
        if rules.max_block_chance > 100 {
            return Err(RulesError::Setting(
                "max_block_chance must be 0..=100".into(),
            ));
        }
        for (key, value) in [
            ("experience_curve_percent", rules.experience_curve_percent),
            ("death_experience_percent", rules.death_experience_percent),
        ] {
            if value > 10_000 || (key == "experience_curve_percent" && value == 0) {
                return Err(RulesError::Setting(format!(
                    "{key} must be {}..=10000",
                    if key == "experience_curve_percent" {
                        1
                    } else {
                        0
                    }
                )));
            }
        }
        for (key, value, min, max) in [
            (
                "drops.other_player_divisor",
                rules.drops.other_player_divisor,
                1,
                8,
            ),
            (
                "drops.magic_find_percent",
                rules.drops.magic_find_percent,
                0,
                10000,
            ),
            ("drops.nodrop_percent", rules.drops.nodrop_percent, 0, 10000),
            ("death_gold_percent", rules.death_gold_percent, 0, 10000),
            (
                "hireling_experience_curve_percent",
                rules.hireling_experience_curve_percent,
                1,
                10000,
            ),
            ("runeword_max_runes", rules.runeword_max_runes, 0, 6),
        ] {
            if !(min..=max).contains(&value) {
                return Err(RulesError::Setting(format!("{key} must be {min}..={max}")));
            }
        }
        if rules.drops.magic_find_cap.is_some_and(|n| n > 10000) {
            return Err(RulesError::Setting(
                "drops.magic_find_cap must be 0..=10000".into(),
            ));
        }
        let [low, high] = rules.socket_level_bands;
        if low == 0 || low > high || high > i32::MAX as u32 {
            return Err(RulesError::Setting(
                "socket_level_bands must be ordered positive i32 levels".into(),
            ));
        }
        let r = &rules.resistances;
        for (key, value) in [
            ("elemental_cap", r.elemental_cap),
            ("elemental_max_cap", r.elemental_max_cap),
            ("physical_cap", r.physical_cap),
            ("other_cap", r.other_cap),
        ] {
            if !(0..=100).contains(&value) {
                return Err(RulesError::Setting(format!(
                    "resistances.{key} must be 0..=100"
                )));
            }
        }
        if r.elemental_cap > r.elemental_max_cap {
            return Err(RulesError::Setting(
                "resistances.elemental_cap must not exceed elemental_max_cap".into(),
            ));
        }
        for (key, penalties) in [
            ("expansion_penalties", r.expansion_penalties),
            ("classic_penalties", r.classic_penalties),
        ] {
            if penalties.is_some_and(|p| p.into_iter().any(|n| !(-1000..=1000).contains(&n))) {
                return Err(RulesError::Setting(format!(
                    "resistances.{key} entries must be -1000..=1000"
                )));
            }
        }
        for (name, unit) in [("players", &rules.players), ("monsters", &rules.monsters)] {
            if unit
                .hit_recovery
                .threshold_denominator
                .is_some_and(|n| n == 0 || n > i32::MAX as u32)
            {
                return Err(RulesError::Setting(format!(
                    "{name}.hit_recovery.threshold_denominator must be 1..=2147483647"
                )));
            }
            for (key, pct) in [
                ("hit_recovery", unit.hit_recovery.duration_percent),
                ("knockback", unit.knockback.duration_percent),
                ("stun", unit.stun.duration_percent),
                ("freeze", unit.freeze.duration_percent),
            ] {
                if pct > 10_000 {
                    return Err(RulesError::Setting(format!(
                        "{name}.{key}.duration_percent must be 0..=10000"
                    )));
                }
            }
        }
        rules.animation.validate()?;
        for (name, unit) in [("players", &rules.players), ("monsters", &rules.monsters)] {
            unit.control.validate(name)?;
            unit.hit_recovery.validate(name)?;
        }
        Ok(rules)
    }

    /// Install on the host's action hooks / server rules configuration before game
    /// creation. A host with other overrides can return this from `ModRules::global_rules`.
    pub fn into_ruleset(self) -> Ruleset {
        Ruleset::Mod(std::sync::Arc::new(self))
    }

    /// How crowd control on a unit of type `ty` works: its own override, else the
    /// global setting.
    pub fn crowd_control_for(&self, ty: UnitType) -> CrowdControl {
        self.unit(ty).crowd_control.unwrap_or(self.crowd_control)
    }

    pub fn unit(&self, ty: UnitType) -> &UnitRules {
        if ty == UnitType::Player {
            &self.players
        } else {
            &self.monsters
        }
    }
}

impl ModRules for GlobalRules {
    fn name(&self) -> &str {
        "global rules"
    }
    fn global_rules(&self) -> &GlobalRules {
        self
    }
}

#[cfg(test)]
mod tests;
