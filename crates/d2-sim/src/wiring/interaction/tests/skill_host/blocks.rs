//! Building-block skills (the mod kit's `skills.toml`) on the real wiring:
//! the data mod host answers the block slot of the skills table, each part
//! reaches the engine's own missile, aura, summon, state and damage code,
//! and a game with no mod has no such slot.

use std::sync::Arc;

use crate::rules::{DataMod, DataModFiles, Ruleset, BLOCK_SLOT};
use crate::skills::use_::bodies::{BodyStat, BodyTables};
use crate::skills::use_::SkillFunctions;
use crate::stats::stat as sst;
use crate::units::{UnitId, UnitType};

use super::{fx, Fx};

pub(super) const SKILL: i32 = 0;
pub(super) const STATE: u16 = 30;
pub(super) const ARMORCLASS: u16 = 31;
pub(super) const LIFE: i32 = 25600;
pub(super) const LEVEL_STAT: u16 = 12;
const TOHIT: u16 = 19;

pub(super) const STATES: &str =
    "[[state]]\nid = 30\nname = \"Charge\"\nduration_seconds = 2\nmax_stacks = 3\n\
                      reapply = \"add_stack\"\n[[state.effect]]\nstat = 31\nper_stack = 5\n";

/// A player at (10, 10), a monster at (11, 10) with 100 life and one at
/// (30, 10); the data mod host with `skills.toml` = `skills`.
pub(super) fn fixture(skills: &str) -> (Fx, UnitId, UnitId, UnitId) {
    let mut fx = fx();
    let p = fx.spawn(UnitType::Player, 10, 10);
    let m = fx.spawn(UnitType::Monster, 11, 10);
    let far = fx.spawn(UnitType::Monster, 30, 10);
    fx.set(p, &[(TOHIT, 3000), (LEVEL_STAT, 30)]);
    for u in [p, m, far] {
        if u != p {
            fx.set(
                u,
                &[
                    (ARMORCLASS, 1),
                    (LEVEL_STAT, 1),
                    (sst::MAXHP, LIFE),
                    (sst::HITPOINTS, LIFE),
                ],
            );
        }
        fx.sim.sys.units.get_mut(u).unwrap().flags |= crate::missiles::unit_flag::IS_VALID_TARGET
            | crate::missiles::unit_flag::CAN_BE_ATTACKED;
    }
    install(&mut fx, skills, STATES);
    (fx, p, m, far)
}

pub(super) fn install(fx: &mut Fx, skills: &str, states: &str) {
    let files = DataModFiles {
        skills: Some(("skills.toml", skills)),
        states: Some(("states.toml", states)),
        ..DataModFiles::default()
    };
    fx.sim.sys.hooks.rules = DataMod::from_files("blocks", &files)
        .unwrap()
        .into_ruleset();
}

/// Casts the block skill [`SKILL`] at `lvl` as unit `u`: the do core's result.
pub(super) fn cast(fx: &mut Fx, u: UnitId, lvl: i32) -> i32 {
    fx.sim.skill_use(&mut fx.game, |w| {
        w.srvdo(BLOCK_SLOT, u, SKILL, lvl, false, false, false)
    })
}

pub(super) fn life(fx: &mut Fx, u: UnitId) -> i32 {
    fx.sim.with(&mut fx.game, |_, v| v.stat(u, sst::HITPOINTS))
}

pub(super) fn stacks(fx: &Fx, u: UnitId) -> u16 {
    let Some(r) = fx.sim.sys.stats.unit_list(u) else {
        return 0;
    };
    fx.sim
        .sys
        .stats
        .list_of_state(r, u32::from(STATE))
        .map_or(0, |l| fx.sim.sys.stats.stacks(l))
}

pub(super) fn armor(fx: &Fx, u: UnitId) -> i32 {
    fx.sim.sys.stats.unit_total(u, ARMORCLASS, 0)
}

const AREA: &str = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"area_hit\"\nradius = 6\n\
                    [skill.part.damage]\nelement = \"fire\"\nmin = \"lvl * 10\"\nmax = \"lvl * 10\"\n";

#[test]
fn an_area_hit_damages_what_is_in_the_radius_by_the_skill_level() {
    let (mut fx, p, m, far) = fixture(AREA);
    fx.sim.sys.hooks.x.aim_at = (11, 10);
    fx.sim.sys.hooks.x.targets.insert(p, m);
    assert_eq!(cast(&mut fx, p, 1), 1);
    assert_eq!(life(&mut fx, m), LIFE - 10 * 256);
    // The monster 19 sub-tiles away and the caster are untouched.
    assert_eq!(life(&mut fx, far), LIFE);
    cast(&mut fx, p, 3);
    assert_eq!(life(&mut fx, m), LIFE - 10 * 256 - 30 * 256);
    fx.assert_clean();
}

#[test]
fn an_area_around_the_caster_and_a_bonus_stat() {
    let toml =
        "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"area_hit\"\nradius = 6\naround = \"caster\"\n\
                [skill.part.damage]\nelement = \"fire\"\nmin = 20\nmax = 20\nbonus_stat = 31\n";
    let (mut fx, p, m, _) = fixture(toml);
    // +50% from the caster's stat 31 (armor class).
    fx.set(p, &[(ARMORCLASS, 50)]);
    cast(&mut fx, p, 1);
    assert_eq!(life(&mut fx, m), LIFE - 30 * 256);
    fx.assert_clean();
}

#[test]
fn damage_rolls_between_min_and_max_from_the_casters_seed() {
    let toml =
        "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"area_hit\"\nradius = 6\naround = \"caster\"\n\
                [skill.part.damage]\nelement = \"fire\"\nmin = 10\nmax = 40\n";
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..8 {
        let (mut fx, p, m, _) = fixture(toml);
        for _ in 0..3 {
            cast(&mut fx, p, 1);
        }
        let lost = (LIFE - life(&mut fx, m)) / 256;
        assert!((30..=120).contains(&lost), "{lost}");
        seen.insert(lost);
    }
    // The same inputs give the same game.
    let run = || {
        let (mut fx, p, m, _) = fixture(toml);
        for _ in 0..3 {
            cast(&mut fx, p, 1);
        }
        life(&mut fx, m)
    };
    assert_eq!(run(), run());
    assert!(!seen.is_empty());
}

#[test]
fn state_parts_stack_unstack_and_end_by_the_state_rules() {
    let toml = "[[skill]]\nid = 0\n\
        [[skill.part]]\nkind = \"state\"\nop = \"add_stack\"\nstate = 30\ncount = 2\n";
    let (mut fx, p, _, _) = fixture(toml);
    assert_eq!(cast(&mut fx, p, 1), 1);
    assert_eq!((stacks(&fx, p), armor(&fx, p)), (2, 10));
    // Past the maximum of 3 the count stops.
    cast(&mut fx, p, 1);
    assert_eq!((stacks(&fx, p), armor(&fx, p)), (3, 15));

    let off =
        "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"state\"\nop = \"remove_stack\"\nstate = 30\n";
    install(&mut fx, off, STATES);
    cast(&mut fx, p, 1);
    assert_eq!((stacks(&fx, p), armor(&fx, p)), (2, 10));
    cast(&mut fx, p, 1);
    cast(&mut fx, p, 1);
    assert_eq!((stacks(&fx, p), armor(&fx, p)), (0, 0));
    assert!(!fx.sim.sys.stats.has_state(p, u32::from(STATE)));
    // Nothing to take off: the cast did nothing.
    assert_eq!(cast(&mut fx, p, 1), 0);

    let apply = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"state\"\nstate = 30\n";
    install(&mut fx, apply, STATES);
    cast(&mut fx, p, 1);
    cast(&mut fx, p, 1);
    // Apply follows the state's `add_stack` policy; the duration is the state's own.
    assert_eq!(stacks(&fx, p), 2);
    let end = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"state\"\nop = \"remove\"\nstate = 30\n";
    install(&mut fx, end, STATES);
    assert_eq!(cast(&mut fx, p, 1), 1);
    assert!(!fx.sim.sys.stats.has_state(p, u32::from(STATE)));
    fx.assert_clean();
}

#[test]
fn a_state_part_on_the_target_and_with_its_own_time() {
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"state\"\nstate = 31\non = \"target\"\nseconds = \"lvl\"\n";
    let (mut fx, p, _, _) = fixture(toml);
    let friend = fx.spawn(UnitType::Player, 12, 10);
    // No target: nothing happens, the cast is not spent.
    assert_eq!(cast(&mut fx, p, 2), 0);
    fx.sim.sys.hooks.x.targets.insert(p, friend);
    assert_eq!(cast(&mut fx, p, 2), 1);
    assert!(fx.sim.sys.stats.has_state(friend, 31));
    // 2 seconds are 50 ticks.
    for _ in 0..49 {
        fx.frame();
    }
    assert!(fx.sim.sys.stats.has_state(friend, 31));
    fx.frame();
    assert!(!fx.sim.sys.stats.has_state(friend, 31));
    fx.assert_clean();
}

#[test]
fn a_melee_hit_attacks_the_target_and_needs_one() {
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"melee_hit\"\nweapon_percent = 0\n\
                [skill.part.damage]\nelement = \"physical\"\nmin = 40\nmax = 40\n";
    let (mut fx, p, m, _) = fixture(toml);
    fx.sim.sys.hooks.x.in_reach = true;
    // State 54 (uninterruptible): the defender takes the hit but plays no
    // get-hit animation, which the fixture's monsters have no record for.
    fx.sim.with(&mut fx.game, |_, v| v.set_state(m, 54, true));
    assert_eq!(cast(&mut fx, p, 1), 0, "no target");
    assert_eq!(life(&mut fx, m), LIFE);
    fx.sim.sys.hooks.x.targets.insert(p, m);
    assert_eq!(cast(&mut fx, p, 1), 1);
    assert!(life(&mut fx, m) < LIFE, "the hit landed");
    fx.assert_clean();
}

#[test]
fn a_missile_part_fires_an_existing_missile_row() {
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"missile\"\nmissile = 0\n";
    let (mut fx, p, _, _) = fixture(toml);
    assert_eq!(cast(&mut fx, p, 1), 0, "no aim point");
    fx.sim.sys.hooks.x.aim_at = (16, 10);
    let before = fx.game.lists.units_of_type(UnitType::Missile).len();
    assert_eq!(cast(&mut fx, p, 1), 1);
    assert_eq!(
        fx.game.lists.units_of_type(UnitType::Missile).len(),
        before + 1
    );
    fx.assert_clean();
}

#[test]
fn an_aura_part_puts_its_state_and_stats_on_the_units_around() {
    let toml =
        "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"aura\"\nstate = 30\nrange = 5\nseconds = 2\n\
                affects = \"everyone\"\n[[skill.part.stat]]\nstat = 31\nvalue = \"lvl * 7\"\n";
    let (mut fx, p, m, far) = fixture(toml);
    {
        let at = Arc::make_mut(&mut fx.sim.sys.hooks.tables);
        at.combat.monstats2[0].isatt = true;
    }
    fx.sim.sys.hooks.bodies = Some(Arc::new(BodyTables {
        stats: vec![BodyStat::default(); 359],
        state_group: vec![0; super::super::N_STATES],
        state_aura: vec![false; super::super::N_STATES],
        ..BodyTables::default()
    }));
    install(&mut fx, toml, "");
    assert_eq!(cast(&mut fx, p, 3), 1);
    for u in [p, m] {
        assert!(fx.sim.sys.stats.has_state(u, u32::from(STATE)));
    }
    assert!(!fx.sim.sys.stats.has_state(far, u32::from(STATE)));
    assert!(fx.sim.with(&mut fx.game, |_, v| v.stat(m, ARMORCLASS)) >= 21);
    fx.assert_clean();
}

#[test]
fn a_summon_part_makes_pets_up_to_the_pet_maximum() {
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"summon\"\nclass = 0\npet_type = 2\n\
                pet_max = 2\ncount = \"lvl\"\n";
    let (mut fx, p, _, _) = fixture(toml);
    fx.sim.sys.hooks.bodies = Some(Arc::new(BodyTables {
        stats: vec![BodyStat::default(); 359],
        state_group: vec![0; super::super::N_STATES],
        state_aura: vec![false; super::super::N_STATES],
        pettype_count: 3,
        pettype_group: vec![0; 3],
        ..BodyTables::default()
    }));
    fx.set(p, &[(LEVEL_STAT, 10)]);
    fx.sim.sys.hooks.x.aim_at = (14, 10);
    let before = fx.game.lists.units_of_type(UnitType::Monster).len();
    assert_eq!(cast(&mut fx, p, 3), 1);
    // Three were asked for; two is the maximum, so the oldest went.
    let pets = fx
        .sim
        .sys
        .hooks
        .pet_lists
        .get(&p)
        .map_or(0, |l| l.entries[2].nodes.len());
    assert_eq!(pets, 2);
    assert!(fx.game.lists.units_of_type(UnitType::Monster).len() > before);
    fx.assert_clean();
}

#[test]
fn parts_run_in_order_and_one_that_acts_spends_the_cast() {
    let toml = "[[skill]]\nid = 0\n\
        [[skill.part]]\nkind = \"state\"\nop = \"add_stack\"\nstate = 30\non = \"target\"\n\
        [[skill.part]]\nkind = \"area_hit\"\nradius = 6\naround = \"caster\"\n\
        [skill.part.damage]\nelement = \"fire\"\nmin = 10\nmax = 10\n";
    let (mut fx, p, m, _) = fixture(toml);
    // The first part finds no target and does nothing; the area part still hits.
    assert_eq!(cast(&mut fx, p, 1), 1);
    assert_eq!(life(&mut fx, m), LIFE - 10 * 256);
    assert_eq!(stacks(&fx, p), 0);
    fx.assert_clean();
}

#[test]
fn a_row_with_no_entry_does_nothing() {
    let (mut fx, p, m, _) = fixture(AREA);
    let r = fx.sim.skill_use(&mut fx.game, |w| {
        w.srvdo(BLOCK_SLOT, p, 7, 1, false, false, false)
    });
    assert_eq!(r, 0);
    assert_eq!(life(&mut fx, m), LIFE);
}

#[test]
fn without_a_mod_the_block_slot_is_not_the_games() {
    let (mut fx, p, m, _) = fixture(AREA);
    fx.sim.sys.hooks.rules = Ruleset::Original;
    assert!(!fx
        .sim
        .sys
        .hooks
        .rules
        .has_skill_slot(crate::rules::SkillFn::Do, BLOCK_SLOT));
    fx.sim.sys.hooks.x.aim_at = (11, 10);
    assert_eq!(cast(&mut fx, p, 1), 0);
    assert_eq!(life(&mut fx, m), LIFE);
    // A mod whose skills.toml is empty does not answer it either.
    install(&mut fx, "", "");
    assert!(!fx
        .sim
        .sys
        .hooks
        .rules
        .has_skill_slot(crate::rules::SkillFn::Do, BLOCK_SLOT));
}
