//! The uninterruptible cast point: the one rule the server and the client both
//! read.
//!
//! With `cast.uninterruptible_after_percent` (or a `[[cast.skill]]` entry) in
//! `rules.toml`, a cast that has played that fraction of its animation cannot be
//! cancelled: a hit does not interrupt it and a new action does not replace it.
//! The server decides with [`committed`] on the unit's own animation; the client
//! asks the same function on its model of the player (`bridge::player_anim::cast_committed`),
//! so both sides move from "can be cancelled" to "cannot" on the same frame and a
//! cast the client shows going off is not taken back by the server.

use crate::rules::global::CastRules;
use crate::units::UnitType;

/// Player modes that play a used skill: attack 1 and 2, cast, throw, kick and the
/// four skill modes.
const PLAYER_SKILL_MODES: [u32; 9] = [7, 8, 10, 11, 12, 13, 14, 15, 16];
/// Monster modes that play a used skill: attack 1 and 2, skill cast and the four
/// special attacks.
const MONSTER_SKILL_MODES: [u32; 7] = [4, 5, 7, 8, 9, 10, 11];

/// Whether `mode` of a unit of type `ty` is one in which a used skill plays.
pub fn is_skill_mode(ty: UnitType, mode: u32) -> bool {
    match ty {
        UnitType::Player => PLAYER_SKILL_MODES.contains(&mode),
        UnitType::Monster => MONSTER_SKILL_MODES.contains(&mode),
        _ => false,
    }
}

/// Whether `frame` of an animation of `frame_count` frames (both 8.8, as the unit
/// stores them) has reached `percent` of it.
pub fn reached(frame: i32, frame_count: i32, percent: u32) -> bool {
    frame_count > 0 && i64::from(frame.max(0)) * 100 >= i64::from(frame_count) * i64::from(percent)
}

/// The first frame (8.8) that has reached `percent` of `frame_count`.
pub fn frame_at(frame_count: i32, percent: u32) -> i32 {
    let n = i64::from(frame_count.max(0)) * i64::from(percent.min(100));
    ((n + 99) / 100) as i32
}

/// Whether the cast of `skill`, at `frame` of its `frame_count`, can no longer be
/// cancelled under `rules`. Always `false` when the skill has no point.
pub fn committed(rules: &CastRules, skill: i32, frame: i32, frame_count: i32) -> bool {
    rules
        .point_for(skill)
        .is_some_and(|p| reached(frame, frame_count, p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::global::CastSkillRule;

    fn rules(default: Option<u32>, skills: &[(u16, u32)]) -> CastRules {
        CastRules {
            uninterruptible_after_percent: default,
            skills: skills
                .iter()
                .map(|&(id, p)| CastSkillRule {
                    id,
                    uninterruptible_after_percent: p,
                })
                .collect(),
        }
    }

    #[test]
    fn a_skill_uses_its_own_point_else_the_default_and_100_is_never() {
        let r = rules(Some(60), &[(54, 25), (36, 100)]);
        assert_eq!(r.point_for(54), Some(25));
        assert_eq!(r.point_for(1), Some(60));
        assert_eq!(r.point_for(36), None, "100 is never");
        assert_eq!(rules(None, &[]).point_for(54), None);
        assert!(rules(None, &[]).is_off());
        assert!(!r.is_off());
    }

    #[test]
    fn the_point_is_the_first_frame_at_or_past_the_fraction() {
        // 8 frames, 8.8: 2048 is the count. 25% is frame 2 exactly.
        assert_eq!(frame_at(2048, 25), 512);
        assert!(!reached(511, 2048, 25));
        assert!(reached(512, 2048, 25));
        // A fraction between frames rounds up; the frame reached is where it flips.
        assert_eq!(frame_at(2048, 33), 676);
        assert!(!reached(675, 2048, 33) && reached(676, 2048, 33));
        for p in [0, 1, 33, 60, 99, 100] {
            let at = frame_at(2048, p);
            assert!(reached(at, 2048, p), "{p}");
            assert!(at == 0 || !reached(at - 1, 2048, p), "{p}");
        }
        assert!(reached(0, 2048, 0), "0% is committed from the first frame");
        assert!(!reached(0, 0, 0), "no animation, no point");
    }

    #[test]
    fn only_skill_modes_count() {
        assert!(is_skill_mode(UnitType::Player, 10));
        assert!(is_skill_mode(UnitType::Monster, 8));
        assert!(!is_skill_mode(UnitType::Player, 1), "neutral");
        assert!(!is_skill_mode(UnitType::Player, 4), "get-hit");
        assert!(!is_skill_mode(UnitType::Monster, 3), "get-hit");
        assert!(!is_skill_mode(UnitType::Missile, 7));
    }
}
