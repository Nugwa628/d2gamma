//! [`crate::rules::HostSkills`] through a mod body.

use crate::rules::HostSkills;
use crate::skills::list::{ListOwner, SkillList};
use crate::skills::{levels, SkillEntry};
use crate::units::{UnitId, UnitType};

use super::super::skill_use::skills;
use super::{fx_with, probe};

/// The test skill (the fixture's skill 0).
const SKILL: i32 = 0;
/// `push i16 v; end` at offset 0 (`v` = 42) and 4 (`v` = 9).
const CALC_42: u32 = 0;
const CALC_9: u32 = 4;

fn entry(base: i32) -> SkillEntry {
    SkillEntry {
        skill: SKILL,
        base,
        owner_guid: -1,
        ..SkillEntry::default()
    }
}

/// The fixture's skill with an attack rating bonus (10, +5 a level), a
/// physical damage range, an elemental damage and length, and two
/// formulas; the player knows it at level 3 and uses it.
fn fx() -> (super::Fx, UnitId) {
    let mut t = skills();
    let r = &mut t.skills[SKILL as usize];
    r.tohit = 10;
    r.levtohit = 5;
    r.mindam = 4;
    r.maxdam = 8;
    r.hitshift = 8;
    r.etype = 1;
    r.emin = 3;
    r.emax = 6;
    r.elen = 25;
    r.prgcalc1 = CALC_9;
    t.skills_code = [42i16, 9]
        .iter()
        .flat_map(|v| {
            let [a, b] = v.to_le_bytes();
            [0x08, a, b, 0x00]
        })
        .collect();
    let rows = t.skills.clone();
    let mut fx = fx_with(t);
    let p = fx.spawn(UnitType::Player, 3, 4);
    let mut l = SkillList::default();
    l.set_base(&rows, ListOwner::player(0), SKILL, 3);
    l.current = l.native(SKILL);
    l.fx.clear();
    fx.sim.sys.hooks.skill_lists.insert(p, l);
    (fx, p)
}

/// Every skill read of `u`, as text.
fn read<W: HostSkills + ?Sized>(w: &mut W, u: UnitId) -> String {
    format!(
        "{:?}",
        (
            (w.skill_list(u), w.owns_skill(u, SKILL), w.owns_skill(u, 5)),
            (
                w.skill_entry(u, SKILL),
                w.skill_level(u, SKILL),
                w.used_skill(u)
            ),
            (w.entry_flags(u, SKILL), w.used_entry_param(u, 1)),
            (
                w.eval_skill_calc(u, CALC_42, SKILL, 3),
                w.prog_count(u, SKILL, 3)
            ),
            (
                w.skill_to_hit(u, SKILL, 3),
                w.skill_phys_damage(u, SKILL, 3, false)
            ),
            (
                w.skill_elem_damage(u, SKILL, 3, true),
                w.skill_elem_len(u, SKILL, 3)
            ),
            (w.melee_range(u), w.minion_spawn_class(u)),
        )
    )
}

#[test]
fn a_mod_body_reads_the_skill_list_and_formulas() {
    let (mut fx, p) = fx();
    let got = probe(&mut fx, p, SKILL, 3, |w, u, _, _| read(w, u));
    let want = fx.sim.skill_use(&mut fx.game, |w| read(w, p));
    assert_eq!(got, Some(want));
    // The answers are D2RUST's formulas on the game's rows.
    let t = fx.sim.sys.hooks.tables.skills.clone();
    fx.sim.skill_use(&mut fx.game, |w| {
        assert_eq!(HostSkills::skill_level(w, p, SKILL), 3);
        assert_eq!(HostSkills::used_skill(w, p), Some(entry(3)));
        assert_eq!(HostSkills::eval_skill_calc(w, p, CALC_42, SKILL, 3), 42);
        assert_eq!(HostSkills::eval_skill_calc(w, p, u32::MAX, SKILL, 3), 0);
        assert_eq!(HostSkills::prog_count(w, p, SKILL, 3), 9);
        assert_eq!(HostSkills::skill_to_hit(w, p, SKILL, 3), 20);
        let phys = (
            levels::phys_min(w, &t, Some(p), SKILL, 3, false),
            levels::phys_max(w, &t, Some(p), SKILL, 3, false),
        );
        assert_eq!(HostSkills::skill_phys_damage(w, p, SKILL, 3, false), phys);
        assert!(phys.1 > 0);
        let len = levels::elem_len(w, &t, Some(p), SKILL, 3);
        assert_eq!(HostSkills::skill_elem_len(w, p, SKILL, 3), len);
        assert!(len > 0);
    });
    fx.assert_clean();
}

#[test]
fn a_mod_body_sets_entry_flags_and_the_used_entry_param() {
    let (mut fx, p) = fx();
    let got = probe(&mut fx, p, SKILL, 3, |w, u, skill, _| {
        let before = (w.entry_flags(u, skill), w.used_entry_param(u, 1));
        w.set_entry_flags(u, skill, 0x100);
        w.set_used_entry_param(u, 1, 7);
        format!("{before:?}")
    });
    assert_eq!(got.as_deref(), Some("(0, 0)"));
    let e = &fx.sim.sys.hooks.skill_lists[&p].entries[0];
    assert_eq!((e.flags, e.params[0]), (0x100, 7));
    fx.assert_clean();
}
