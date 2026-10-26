//! A mod game's stat and skill point rules (`WorldHooks::stat_point`,
//! `WorldHooks::skill_point`) take C→S 0x3A / 0x3B ahead of the engine's;
//! a request the mod leaves runs the engine's rules.

use d2_sim::rules::{EventHost, ModRules, Ruleset, WorldHooks};

use super::*;

/// Stat points: each spend raises the stat by 2 and leaves the unspent
/// points alone. Skill [`MULTI`] is refused; other skills are the engine's.
#[derive(Debug)]
struct Points;

impl WorldHooks for Points {
    fn stat_point(
        &self,
        h: &mut dyn EventHost,
        player: UnitId,
        stat: u8,
        count: u8,
    ) -> Option<i32> {
        let s = u16::from(stat);
        let v = h.stat(player, s, 0);
        h.set_stat(player, s, v + 2 * (i32::from(count) + 1));
        Some(0)
    }
    fn skill_point(&self, _: &mut dyn EventHost, _: UnitId, skill: u16) -> Option<i32> {
        (i32::from(skill) == MULTI).then_some(1)
    }
}

#[derive(Debug)]
struct Rules;

impl ModRules for Rules {
    fn name(&self) -> &str {
        "test-points"
    }
    fn world_hooks(&self) -> Option<&dyn WorldHooks> {
        Some(&Points)
    }
}

#[test]
fn a_mod_game_takes_the_point_requests() {
    let book = Inner {
        list: vec![entry(LEARN, 1)],
        class_skills: vec![LEARN, MULTI],
        ..Inner::default()
    };
    let mut fx = Fx::new(1, book);
    fx.host.game.events.hooks().rules = Ruleset::Mod(Arc::new(Rules));
    fx.set_stats(&[(0, 30), (4, 5), (12, 1), (5, 1)]);
    let sk = |s: i32| {
        let mut m = vec![0x3B];
        m.extend((s as u16).to_le_bytes());
        m
    };
    let (codes, recv) = fx.frame(&[&[0x3A, 0x00, 0x02], &sk(MULTI), &sk(LEARN)]);
    assert_eq!((codes, recv), (vec![Done, Refused, Done], NONE));
    // The mod's rule: +6 strength, no point spent.
    assert_eq!((fx.stat(0), fx.stat(4)), (36, 5));
    // The refused skill never reached the engine; LEARN did.
    assert_eq!(fx.book().log, ["addskill 3 1", "after"]);
    assert!(fx.take_errors().is_empty());
}
