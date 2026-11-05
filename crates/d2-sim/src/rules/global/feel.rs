//! Crowd-control numbers and animation breakpoints. Durations use 25 Hz data frames;
//! the host continues to simulate at 60 Hz, including the optional classic animation feel.

use serde::Deserialize;

use super::{HitRecovery, RulesError};

/// Numbers applied independently to actual player and monster targets.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ControlRules {
    pub stun_max_frames: i32,
    pub hireling_stun_max_frames: i32,
    pub unique_stun_resist_percent: u32,
    pub freeze_max_frames: Option<i32>,
    pub freeze_divisors: Option<[i32; 3]>,
    pub knockback_velocity: i32,
    pub knockback_distance: u8,
    pub sandleaper_knockback_distance: u8,
}

impl Default for ControlRules {
    fn default() -> Self {
        Self::defaults()
    }
}

impl ControlRules {
    pub const fn defaults() -> Self {
        Self {
            stun_max_frames: 250,
            hireling_stun_max_frames: 13,
            unique_stun_resist_percent: 90,
            freeze_max_frames: None,
            freeze_divisors: None,
            knockback_velocity: 4096,
            knockback_distance: 5,
            sandleaper_knockback_distance: 10,
        }
    }

    pub(super) fn validate(&self, name: &str) -> Result<(), RulesError> {
        for (key, n) in [
            ("stun_max_frames", self.stun_max_frames),
            ("hireling_stun_max_frames", self.hireling_stun_max_frames),
            ("freeze_max_frames", self.freeze_max_frames.unwrap_or(1)),
        ] {
            range(&format!("{name}.control.{key}"), n, 1, 1_000_000)?;
        }
        range(
            &format!("{name}.control.unique_stun_resist_percent"),
            self.unique_stun_resist_percent as i32,
            0,
            100,
        )?;
        range(
            &format!("{name}.control.knockback_velocity"),
            self.knockback_velocity,
            1,
            32767,
        )?;
        range(
            &format!("{name}.control.knockback_distance"),
            i32::from(self.knockback_distance),
            1,
            255,
        )?;
        range(
            &format!("{name}.control.sandleaper_knockback_distance"),
            i32::from(self.sandleaper_knockback_distance),
            1,
            255,
        )?;
        if let Some(ds) = self.freeze_divisors {
            for n in ds {
                range(&format!("{name}.control.freeze_divisors"), n, 1, 1_000_000)?;
            }
        }
        Ok(())
    }
}

/// The five diminishing-return rows are attack, recovery, cast, block and movement.
/// Mode selection and the integer formulas stay engine code; their numbers are data.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnimationRules {
    pub classic_25hz: bool,
    pub diminishing: [i32; 5],
    pub attack_percent: [i32; 2],
    pub cast_base_percent: i32,
    pub cast_max_percent: i32,
    pub recovery_base_percent: i32,
    pub block_base_percent: i32,
    pub holy_shield_block_percent: i32,
    pub block_min_speed: i32,
    pub movement_min_percent: i32,
    pub movement_max_percent: Option<i32>,
    pub other_percent: [i32; 2],
    pub speed_cap: i32,
    pub sequence_attack_penalty: i32,
    pub player_walk_speed: i32,
    pub player_run_speed: i32,
    pub frame_bonus: [[i32; 7]; 12],
}

impl Default for AnimationRules {
    fn default() -> Self {
        Self::defaults()
    }
}

impl AnimationRules {
    pub const fn defaults() -> Self {
        Self {
            classic_25hz: false,
            diminishing: [120, 120, 120, 120, 150],
            attack_percent: [15, 175],
            cast_base_percent: 100,
            cast_max_percent: 175,
            recovery_base_percent: 50,
            block_base_percent: 50,
            holy_shield_block_percent: 100,
            block_min_speed: 1,
            movement_min_percent: 25,
            movement_max_percent: None,
            other_percent: [15, 175],
            speed_cap: 32767,
            sequence_attack_penalty: 30,
            player_walk_speed: 213,
            player_run_speed: 101,
            frame_bonus: [
                [1, 1, 0, 0, 0, 0, 0],
                [0; 7],
                [2, 2, 0, 0, 0, 0, 0],
                [2, 2, 0, 0, 0, 0, 0],
                [2, 2, 0, 0, 0, 0, 0],
                [2, 2, 0, 0, 0, 0, 0],
                [2, 2, 0, 0, 0, 0, 0],
                [0; 7],
                [0; 7],
                [0; 7],
                [0; 7],
                [0; 7],
            ],
        }
    }

    /// Preserve the path table's coefficient unless a mod overrides the movement row.
    pub fn movement_coefficient(&self, table: i32) -> i32 {
        if self.diminishing[4] == 150 {
            table
        } else {
            self.diminishing[4]
        }
    }

    pub fn movement_percent(&self, n: i32) -> i32 {
        n.max(self.movement_min_percent)
            .min(self.movement_max_percent.unwrap_or(i32::MAX))
    }

    pub(super) fn validate(&self) -> Result<(), RulesError> {
        for n in self.diminishing {
            range("animation.diminishing", n, 1, 10000)?;
        }
        for (key, n) in [
            ("cast_base_percent", self.cast_base_percent),
            ("cast_max_percent", self.cast_max_percent),
            ("recovery_base_percent", self.recovery_base_percent),
            ("block_base_percent", self.block_base_percent),
            ("holy_shield_block_percent", self.holy_shield_block_percent),
            ("movement_min_percent", self.movement_min_percent),
            (
                "movement_max_percent",
                self.movement_max_percent
                    .unwrap_or(self.movement_min_percent),
            ),
            ("player_walk_speed", self.player_walk_speed),
            ("player_run_speed", self.player_run_speed),
        ] {
            range(&format!("animation.{key}"), n, 1, 10000)?;
        }
        for (key, bounds) in [
            ("attack_percent", self.attack_percent),
            ("other_percent", self.other_percent),
        ] {
            if bounds[0] < 1 || bounds[0] > bounds[1] || bounds[1] > 10000 {
                return Err(RulesError::Setting(format!(
                    "animation.{key} must be ordered bounds in 1..=10000"
                )));
            }
        }
        range("animation.speed_cap", self.speed_cap, 1, 32767)?;
        range(
            "animation.block_min_speed",
            self.block_min_speed,
            1,
            self.speed_cap,
        )?;
        range(
            "animation.sequence_attack_penalty",
            self.sequence_attack_penalty,
            0,
            10000,
        )?;
        if self
            .movement_max_percent
            .is_some_and(|n| n < self.movement_min_percent)
        {
            return Err(RulesError::Setting(
                "animation.movement_max_percent must be at least movement_min_percent".into(),
            ));
        }
        for row in self.frame_bonus {
            for n in row {
                // A bonus larger than the animation can cause repeated wrapping.
                range("animation.frame_bonus", n, 0, 2)?;
            }
        }
        Ok(())
    }
}

impl HitRecovery {
    pub(super) fn validate(&self, name: &str) -> Result<(), RulesError> {
        range(
            &format!("{name}.hit_recovery.minimum_damage"),
            self.minimum_damage,
            0,
            i32::MAX,
        )?;
        for n in self.divisors {
            range(&format!("{name}.hit_recovery.divisors"), n, 4, 1_000_000)?;
        }
        for n in self.band_denominators {
            range(
                &format!("{name}.hit_recovery.band_denominators"),
                n,
                1,
                *self.divisors.iter().min().unwrap(),
            )?;
        }
        if self.band_denominators[0] > self.band_denominators[1] {
            return Err(RulesError::Setting(format!(
                "{name}.hit_recovery.band_denominators must be ordered"
            )));
        }
        for n in self.band_roll_sides {
            if n == 0 || n > 256 || !n.is_power_of_two() {
                return Err(RulesError::Setting(format!(
                    "{name}.hit_recovery.band_roll_sides must be powers of two in 1..=256"
                )));
            }
        }
        Ok(())
    }
}

fn range(key: &str, n: i32, min: i32, max: i32) -> Result<(), RulesError> {
    if !(min..=max).contains(&n) {
        return Err(RulesError::Setting(format!("{key} must be {min}..={max}")));
    }
    Ok(())
}
