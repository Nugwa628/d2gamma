//! [`crate::rules::HostDamage`] through a mod body.

use crate::combat::hit::result;
use crate::combat::DamageRecord;
use crate::missiles::unit_flag;
use crate::rules::HostDamage;
use crate::stats::stat as sst;
use crate::units::{UnitId, UnitType};

use super::super::skill_use::skills;
use super::{fx_with, probe, Fx};

const SKILL: i32 = 0;
const TOHIT: u16 = 19;
const LEVEL_STAT: u16 = 12;
const ARMORCLASS: u16 = 31;
const LIFE: i32 = 25600;

/// The fixture's skill with physical damage 4-8 and fire damage 3-6;
/// a player at (10, 10) and a monster next to it at (11, 10) with 100
/// life.
fn fixture() -> (Fx, UnitId, UnitId) {
    let mut t = skills();
    let r = &mut t.skills[SKILL as usize];
    r.mindam = 4;
    r.maxdam = 8;
    r.hitshift = 8;
    r.etype = 1;
    r.emin = 3;
    r.emax = 6;
    let mut fx = fx_with(t);
    let p = fx.spawn(UnitType::Player, 10, 10);
    let m = fx.spawn(UnitType::Monster, 11, 10);
    fx.set(p, &[(TOHIT, 3000), (LEVEL_STAT, 30)]);
    fx.set(
        m,
        &[
            (ARMORCLASS, 1),
            (LEVEL_STAT, 1),
            (sst::MAXHP, LIFE),
            (sst::HITPOINTS, LIFE),
        ],
    );
    fx.sim.sys.units.get_mut(m).unwrap().flags |=
        unit_flag::IS_VALID_TARGET | unit_flag::CAN_BE_ATTACKED;
    (fx, p, m)
}

/// A skill hit of `a` on `d`: the rolls, the skill's damage, a weapon
/// fill, and the record applied at once; every answer as text.
fn hit<W: HostDamage + ?Sized>(w: &mut W, a: UnitId, d: UnitId) -> String {
    let roll = w.hit_roll(a, d, 0, true);
    let defend = w.defend_roll(a, d, true, false);
    let melee = w.melee_result(a, d, 0, 0);
    let mut rec = DamageRecord {
        result: result::HIT,
        ..DamageRecord::default()
    };
    w.roll_skill_physical(a, SKILL, 3, &mut rec);
    let fire = w.roll_skill_element(a, SKILL, 3, &mut rec);
    let cold = w.add_element(a, &mut rec, 3, 512, 25);
    w.fill_damage(a, d, &mut rec, false, 64);
    let ev = w.unit_event(7, a, Some(d), Some(&mut rec));
    w.apply_damage(a, d, true, &mut rec);
    format!("{:?}", (roll, defend, melee, fire, cold, ev, rec))
}

#[test]
fn a_mod_body_rolls_and_applies_a_skill_hit() {
    // The mod body's answers are the view's, seed draw for seed draw.
    let (mut fx, p, m) = fixture();
    let got = probe(&mut fx, p, SKILL, 3, move |w, u, _, _| hit(w, u, m));
    let life = fx.sim.with(&mut fx.game, |_, v| v.stat(m, sst::HITPOINTS));
    let (mut fy, q, n) = fixture();
    let want = fy.sim.skill_use(&mut fy.game, |w| hit(w, q, n));
    assert_eq!(got, Some(want));
    assert!(life < LIFE, "the record took life");
    fx.assert_clean();
}

#[test]
fn a_mod_body_fills_the_skill_damage_into_a_record() {
    let (mut fx, p, _) = fixture();
    let got = probe(&mut fx, p, SKILL, 3, |w, u, skill, lvl| {
        let mut rec = DamageRecord::default();
        w.roll_skill_physical(u, skill, lvl, &mut rec);
        let v = w.roll_skill_element(u, skill, lvl, &mut rec);
        let (pmin, pmax) = w.skill_phys_damage(u, skill, lvl, false);
        let (emin, emax) = w.skill_elem_damage(u, skill, lvl, true);
        let ok = (pmin..=pmax).contains(&rec.physical)
            && (emin..=emax).contains(&v)
            && rec.fire == v
            && rec.physical > 0
            && v > 0;
        format!("{ok}")
    });
    assert_eq!(got.as_deref(), Some("true"));
    fx.assert_clean();
}

#[test]
fn a_mod_body_queues_a_combat_and_applies_it() {
    let (mut fx, p, m) = fixture();
    let got = probe(&mut fx, p, SKILL, 3, move |w, u, skill, lvl| {
        let mut rec = DamageRecord {
            result: result::HIT,
            hit_flags: crate::combat::damage::hitflag::SKIP_ROLL,
            ..DamageRecord::default()
        };
        w.roll_skill_physical(u, skill, lvl, &mut rec);
        w.start_combat(u, m, &mut rec, 0);
        format!("{}", rec.physical)
    });
    let queued = fx.sim.sys.hooks.combat_lists.get(&p).map_or(0, Vec::len);
    assert_eq!(queued, 1, "the record waits on the attacker's list");
    let dmg: i32 = got.unwrap().parse().unwrap();
    assert!(dmg > 0);
    probe(&mut fx, p, SKILL, 3, move |w, u, _, _| {
        w.apply_queued(u, m);
        String::new()
    });
    // The record is consumed (applied in melee range, else dropped), as
    // the view's own apply does on a twin game.
    let life = fx.sim.with(&mut fx.game, |_, v| v.stat(m, sst::HITPOINTS));
    let (mut fy, q, n) = fixture();
    fy.sim.skill_use(&mut fy.game, |w| {
        let mut rec = DamageRecord {
            result: result::HIT,
            hit_flags: crate::combat::damage::hitflag::SKIP_ROLL,
            ..DamageRecord::default()
        };
        w.roll_skill_physical(q, SKILL, 3, &mut rec);
        w.start_combat(q, n, &mut rec, 0);
        w.apply_queued(q, n);
    });
    let twin_life = fy.sim.with(&mut fy.game, |_, v| v.stat(n, sst::HITPOINTS));
    assert_eq!(life, twin_life);
    assert!(fx
        .sim
        .sys
        .hooks
        .combat_lists
        .get(&p)
        .is_none_or(Vec::is_empty));
    fx.assert_clean();
}
