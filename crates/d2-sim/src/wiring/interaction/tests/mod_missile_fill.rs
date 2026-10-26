//! The reads of a mod game's missile creation fill
//! ([`crate::missiles::fill::MissileFill`]) and the skill formula columns
//! its bodies evaluate ([`crate::missiles::SkillCalc`]) on the wired
//! world: the same values D2RUST's own fill reads.

use std::sync::Arc;

use d2_data::tables::Skills;

use crate::missiles::fill::{DamageLine, MissileFill};
use crate::missiles::{MissileBodies, SkillCalc};
use crate::skills::fake::skill_tables;
use crate::skills::{self, use_::bodies, use_::bodies::BodyTables};
use crate::units::UnitType;

use super::skill_bodies::body_rec;
use super::skill_use::Fx;
use super::stat_data;

/// A skill with an elemental line, `calc3` and two aura stat formulas.
const SKILL: i32 = 1;
/// Formula offsets in the code buffer: `push i16 v; end`.
const F_200: u32 = 0;
const F_25: u32 = 4;
const F_77: u32 = 8;

fn code() -> Vec<u8> {
    let mut c = Vec::new();
    for v in [200i16, 25, 77] {
        c.push(0x08);
        c.extend(v.to_le_bytes());
        c.push(0x00);
    }
    c
}

fn tables() -> crate::skills::SkillTables {
    let mut v: Vec<Skills> = (0..2).map(|_| body_rec()).collect();
    let k = &mut v[SKILL as usize];
    k.calc3 = F_77;
    k.aurastatcalc1 = F_25;
    k.aurastatcalc2 = F_200;
    k.emin = 3;
    k.emax = 5;
    k.hitshift = 8;
    let mut t = skill_tables(v);
    t.skills_code = code();
    t
}

fn fx() -> (Fx, crate::units::UnitId) {
    let mut fx = Fx::with(stat_data(), tables());
    fx.sim.sys.hooks.bodies = Some(Arc::new(BodyTables {
        stats: vec![bodies::BodyStat::default(); 359],
        ..BodyTables::default()
    }));
    let p = fx.spawn(UnitType::Player, 10, 10);
    (fx, p)
}

#[test]
fn skill_calc_reads_calc3_and_the_aura_stat_formulas() {
    let (mut fx, p) = fx();
    let got = fx
        .sim
        .missiles(&mut fx.game, |g, cx| {
            let w = &mut *cx.world;
            [
                SkillCalc::Calc3,
                SkillCalc::AuraStat(1),
                SkillCalc::AuraStat(2),
                SkillCalc::AuraStat(0),
                SkillCalc::AuraStat(7),
            ]
            .map(|c| w.skill_calc(g, Some(p), SKILL, c, 1))
        })
        .expect("missile store");
    assert_eq!(got, [77, 25, 200, 0, 0]);
}

#[test]
fn fill_reads_answer_what_d2rust_fill_reads() {
    let (mut fx, p) = fx();
    let (line, direct, weapon, mastery) = fx
        .sim
        .missiles(&mut fx.game, |g, cx| {
            let w = &mut *cx.world;
            let line = w.skill_damage_line(g, Some(p), SKILL, 1, false);
            let weapon = w.fill_weapon(g, p);
            let mastery = w.weapon_mastery(g, p, None, Some(0), 1);
            let t = w.h.tables.clone();
            let mut cv = w.combat(g);
            let s = &t.skills;
            let direct = DamageLine {
                phys_min: skills::phys_min(&mut cv, s, Some(p), SKILL, 1, false),
                phys_max: skills::phys_max(&mut cv, s, Some(p), SKILL, 1, false),
                elem_min: skills::elem_min(&mut cv, s, Some(p), SKILL, 1, false),
                elem_max: skills::elem_max(&mut cv, s, Some(p), SKILL, 1, false),
                elem_len: skills::elem_len(&mut cv, s, Some(p), SKILL, 1),
            };
            (line, direct, weapon, mastery)
        })
        .expect("missile store");
    assert_eq!(line, direct);
    assert_eq!((line.elem_min, line.elem_max), (3 << 8, 5 << 8));
    // No weapon: no weapon and no item-type mastery.
    assert_eq!((weapon, mastery), (None, 0));
}
