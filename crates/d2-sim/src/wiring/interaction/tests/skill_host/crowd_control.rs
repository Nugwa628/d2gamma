//! Declared crowd control from the sources a mod can write in `skills.toml` and
//! `triggers.toml`, on the real wiring: a melee or area part with `cc`, and the
//! `crowd_control` trigger action, stun the player they hit; a part without `cc`
//! under declared rules does not.

use crate::rules::{DataMod, DataModFiles, HostDamage};
use crate::stats::stat as sst;
use crate::units::UnitId;

use super::blocks::{cast, fixture, LEVEL_STAT, LIFE, STATES};
use super::Fx;

const STUNNED: u32 = 21;
const TOHIT: u16 = 19;

const MELEE: &str = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"melee_hit\"\nweapon_percent = 0\n\
                     [skill.part.damage]\nmin = 20\nmax = 20\n";
const AREA: &str = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"area_hit\"\nradius = 8\n\
                    around = \"caster\"\n[skill.part.damage]\nmin = 20\nmax = 20\n";
const STUN_HALF_SECOND: &str = "[[skill.part.cc]]\nkind = \"stun\"\nseconds = 0.5\n";
const DECLARED: &str = "crowd_control = \"declared\"\n";

fn install(fx: &mut Fx, skills: &str, triggers: &str, rules: &str) {
    let files = DataModFiles {
        skills: Some(("skills.toml", skills)),
        states: Some(("states.toml", STATES)),
        triggers: Some(("triggers.toml", triggers)),
        rules: Some(("rules.toml", rules)),
        ..DataModFiles::default()
    };
    fx.sim.sys.hooks.rules = DataMod::from_files("cc", &files).unwrap().into_ruleset();
}

/// The monster `m` attacks the player `p` with the block skill under `rules`;
/// `p` is stunned afterwards or not.
fn stunned_by(skills: &str, rules: &str) -> bool {
    let (mut fx, p, m, _) = fixture_with_life(skills);
    install(&mut fx, skills, "", rules);
    fx.sim.sys.hooks.x.in_reach = true;
    fx.sim.sys.hooks.x.targets.insert(m, p);
    cast(&mut fx, m, 1);
    fx.sim.sys.stats.has_state(p, STUNNED)
}

#[test]
fn a_melee_part_with_cc_stuns_and_one_without_does_not() {
    let with = format!("{MELEE}{STUN_HALF_SECOND}");
    for rules in ["", DECLARED] {
        assert!(stunned_by(&with, rules), "rules {rules:?}");
    }
    // Without a declaration there is no stun under declared rules.
    assert!(!stunned_by(MELEE, DECLARED));
}

/// As [`stunned_by`], with the mod's immunity state `immune_state` (state 30
/// of the test states) and the player holding it or not.
fn stunned_by_with_immunity(holds: bool, immune_state: Option<u16>) -> bool {
    let skills = format!("{MELEE}{STUN_HALF_SECOND}");
    let (mut fx, p, m, _) = fixture_with_life(&skills);
    let cc = immune_state.map(|s| {
        format!(
            "immune_state = {s}
"
        )
    });
    let files = DataModFiles {
        skills: Some(("skills.toml", skills.as_str())),
        states: Some(("states.toml", STATES)),
        crowd_control: cc.as_deref().map(|t| ("crowd_control.toml", t)),
        ..DataModFiles::default()
    };
    fx.sim.sys.hooks.rules = DataMod::from_files("cc", &files).unwrap().into_ruleset();
    fx.sim.sys.hooks.x.in_reach = true;
    fx.sim.sys.hooks.x.targets.insert(m, p);
    if holds {
        fx.sim.with(&mut fx.game, |_, v| v.set_state(p, 30, true));
    }
    cast(&mut fx, m, 1);
    fx.sim.sys.stats.has_state(p, STUNNED)
}

#[test]
fn a_unit_in_the_immunity_state_is_not_stunned_even_by_a_declared_stun() {
    // Without the state, or when the mod names none, the declared stun lands.
    assert!(stunned_by_with_immunity(false, Some(30)));
    assert!(stunned_by_with_immunity(true, None));
    // Holding the named state, it does not.
    assert!(!stunned_by_with_immunity(true, Some(30)));
    // A different state is no protection.
    assert!(stunned_by_with_immunity(true, Some(31)));
}

#[test]
fn an_area_part_with_cc_stuns_what_it_hits() {
    let with = format!("{AREA}{STUN_HALF_SECOND}");
    assert!(stunned_by(&with, DECLARED));
    assert!(!stunned_by(AREA, DECLARED));
}

#[test]
fn the_stun_lasts_the_declared_time() {
    let with = format!("{AREA}{STUN_HALF_SECOND}");
    let (mut fx, p, m, _) = fixture_with_life(&with);
    install(&mut fx, &with, "", DECLARED);
    cast(&mut fx, m, 1);
    assert!(fx.sim.sys.stats.has_state(p, STUNNED));
    // 0.5 seconds are 13 base ticks.
    let ticks = fx.game.ticks(13) as usize;
    for _ in 0..ticks - 1 {
        fx.frame();
    }
    assert!(fx.sim.sys.stats.has_state(p, STUNNED));
    fx.frame();
    assert!(!fx.sim.sys.stats.has_state(p, STUNNED));
}

#[test]
fn the_trigger_action_stuns_the_other_unit() {
    const ATTACK: u8 = 7;
    let trigger = "[[trigger]]\non = \"attack\"\nwhile_stat = 12\n\
                   [[trigger.action]]\nkind = \"crowd_control\"\n\
                   effect = { kind = \"stun\", seconds = 0.5 }\n";
    for rules in ["", DECLARED] {
        let (mut fx, p, m, _) = fixture_with_life("");
        install(&mut fx, "", trigger, rules);
        let fire = |fx: &mut Fx, u: UnitId, other: UnitId| {
            fx.sim.skill_use(&mut fx.game, |w| {
                HostDamage::unit_event(w, ATTACK, u, Some(other), None);
            });
        };
        // The monster holds the trigger (its level is stat 12 too) and stuns
        // the player it attacks.
        fire(&mut fx, m, p);
        assert!(fx.sim.sys.stats.has_state(p, STUNNED), "rules {rules:?}");
        fx.assert_clean();
    }
}

/// The fixture, with a player who has life to lose.
fn fixture_with_life(skills: &str) -> (Fx, UnitId, UnitId, UnitId) {
    let (mut fx, p, m, far) = fixture(skills);
    // The monster out-levels the player and has the rating to hit every time.
    fx.set(
        p,
        &[(sst::MAXHP, LIFE), (sst::HITPOINTS, LIFE), (LEVEL_STAT, 1)],
    );
    fx.set(m, &[(LEVEL_STAT, 30), (TOHIT, 3000)]);
    (fx, p, m, far)
}

#[test]
fn a_timed_immunity_state_from_the_ai_protects_until_it_runs_out() {
    use crate::monsters::ai::AiUnits;
    // The state the mod names is put on the unit by the AI for 50 ticks
    // (two seconds) through the real state host, then it expires.
    let skills = format!("{MELEE}{STUN_HALF_SECOND}");
    let (mut fx, p, m, _) = fixture_with_life(&skills);
    let cc = "immune_state = 30\n";
    let files = DataModFiles {
        skills: Some(("skills.toml", skills.as_str())),
        states: Some(("states.toml", STATES)),
        crowd_control: Some(("crowd_control.toml", cc)),
        ..DataModFiles::default()
    };
    fx.sim.sys.hooks.rules = DataMod::from_files("cc", &files).unwrap().into_ruleset();
    fx.sim.sys.hooks.x.in_reach = true;
    fx.sim.sys.hooks.x.targets.insert(m, p);
    let landed = fx
        .sim
        .with(&mut fx.game, |g, v| AiUnits::timed_state(v, g, p, 30, 50));
    assert!(landed);
    assert!(fx.sim.sys.stats.has_state(p, 30));
    cast(&mut fx, m, 1);
    assert!(!fx.sim.sys.stats.has_state(p, STUNNED));
    // Two seconds later the state is gone and the same hit stuns.
    for _ in 0..fx.game.ticks(50) {
        fx.frame();
    }
    assert!(!fx.sim.sys.stats.has_state(p, 30));
    cast(&mut fx, m, 1);
    assert!(fx.sim.sys.stats.has_state(p, STUNNED));
}

mod template {
    //! `templates/boss-interrupts-only`, loaded as a mod host loads it.

    use crate::combat::cc::{CcEffect, CcKind, CcSource};
    use crate::rules::global::CrowdControl;
    use crate::rules::{DataMod, DataModFiles, ModRules};
    use crate::units::modes::monster_mode;
    use crate::units::UnitType;

    const RULES: &str =
        include_str!("../../../../../../../templates/boss-interrupts-only/files/rules.toml");
    const SKILLS: &str =
        include_str!("../../../../../../../templates/boss-interrupts-only/files/skills.toml");
    const CC: &str = include_str!(
        "../../../../../../../templates/boss-interrupts-only/files/crowd_control.toml"
    );

    #[test]
    fn declares_the_boss_attacks_and_leaves_the_player_undeclared_otherwise() {
        let files = DataModFiles {
            rules: Some(("rules.toml", RULES)),
            skills: Some(("skills.toml", SKILLS)),
            crowd_control: Some(("crowd_control.toml", CC)),
            ..DataModFiles::default()
        };
        let m = DataMod::from_files("boss-interrupts-only", &files).unwrap();
        let g = m.global_rules();
        assert_eq!(
            g.crowd_control_for(UnitType::Player),
            CrowdControl::Declared
        );
        assert_eq!(
            g.crowd_control_for(UnitType::Monster),
            CrowdControl::Classic
        );
        // Teleport is committed from the first frame; every other skill at 40%.
        assert_eq!(g.cast.point_for(54), Some(0));
        assert_eq!(g.cast.point_for(36), Some(40));
        let boss = |mode| CcSource::Monster { class: 156, mode };
        assert_eq!(
            m.declared_cc(boss(monster_mode::A1)),
            [CcEffect {
                kind: CcKind::Interrupt,
                chance: 100,
                ticks: 0
            }]
        );
        // 1.5 seconds are 38 base ticks (37.5 rounded).
        assert_eq!(
            m.declared_cc(boss(monster_mode::S1)),
            [CcEffect {
                kind: CcKind::Stun,
                chance: 100,
                ticks: 38
            }]
        );
        assert!(m.declared_cc(boss(monster_mode::A2)).is_empty());
        assert!(m
            .declared_cc(CcSource::Monster {
                class: 7,
                mode: monster_mode::A1
            })
            .is_empty());
        assert_eq!(
            m.declared_cc(CcSource::Missile { row: 214, skill: 1 }),
            [CcEffect {
                kind: CcKind::Knockback,
                chance: 25,
                ticks: 0
            }]
        );
    }
}
