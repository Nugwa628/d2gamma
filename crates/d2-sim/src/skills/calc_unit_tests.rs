//! A mod game's formula functions and parameters read the formula's unit
//! ([`crate::rules::CalcUnit`] from [`CalcContext::unit`]) and answer
//! parameters before D2RUST's ([`crate::rules::CalcProvider::param`]).
//! D2RUST's own formulas evaluate as before.

use std::sync::Arc;

use d2_data::calc::{CalcRegistry, Family, ModKeyword};

use super::calc::CalcContext;
use super::fake::*;
use super::levels::{eval_missile, eval_skill};
use super::SkillEntry;
use crate::rules::{CalcProvider, ModRules, Ruleset};
use crate::units::UnitType;

/// The keywords, in registration order (skills indices 7.., missiles 4..).
const KEYWORDS: [(&str, u8); 11] = [
    ("ustate", 1),
    ("uclass", 0),
    ("uskill", 2),
    ("upets", 1),
    ("uitem", 2),
    ("ulevel", 0),
    ("umode", 0),
    ("ustat", 2),
    ("uspecial", 2),
    ("ueval", 1),
    ("utype", 0),
];

/// The parameter the test mod adds past D2RUST's rows.
const MOD_PARAM: u8 = 100;
/// D2RUST's `par1` parameter (skillcalc row 8), which the mod replaces
/// while a unit is in context.
const PAR1: u8 = 8;
/// D2RUST's `lvl` parameter (skillcalc row 16).
const LVL_PARAM: u8 = 16;

#[derive(Debug)]
struct Calc(CalcRegistry);

impl CalcProvider for Calc {
    fn registry(&self) -> &CalcRegistry {
        &self.0
    }
    fn call(&self, family: Family, index: u8, a: &[i32], ctx: &mut dyn CalcContext) -> i32 {
        let first = CalcRegistry::first_index(family);
        let Some(u) = ctx.unit() else {
            return -1;
        };
        match index - first {
            0 => i32::from(u.has_state(a[0] as u16)),
            1 => u.class_id(),
            2 => u.skill_level(a[0], a[1] != 0),
            3 => u.pet_count(a[0]),
            4 => u.item_stat(a[0] as u8, a[1] as u16, 0),
            5 => u.level_id(),
            6 => u.mode(),
            7 => u.stat(a[0] as u16, a[1] as u16),
            8 => u.special(a[0] as u8, a[1], 3),
            9 => u.eval(a[0] as u32, 0, 4),
            _ => i32::from(u.unit_type() == UnitType::Player),
        }
    }
    fn param(&self, _: Family, code: u8, ctx: &mut dyn CalcContext) -> Option<i32> {
        match code {
            MOD_PARAM => Some(4242),
            // Through the context's own functions: max(2, 7) + 70.
            PAR1 if ctx.unit().is_some() => Some(ctx.call(1, &[2, 7]) + 70),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Game(Calc);

impl ModRules for Game {
    fn name(&self) -> &str {
        "calc unit"
    }
    fn calc(&self) -> Option<&dyn CalcProvider> {
        Some(&self.0)
    }
}

fn registry() -> CalcRegistry {
    let mut r = CalcRegistry::new();
    for family in [Family::Skills, Family::Missiles] {
        for (name, arity) in KEYWORDS {
            let kw = ModKeyword {
                name: name.into(),
                arity,
                names_like: None,
            };
            r.add(family, kw).unwrap();
        }
    }
    r
}

fn modded() -> Ruleset {
    Ruleset::Mod(Arc::new(Game(Calc(registry()))))
}

/// Links for the test formulas: no names resolve.
struct NoLinks;

impl d2_data::calc::CalcLinks for NoLinks {
    fn skill(&self, _: &[u8]) -> Option<u32> {
        None
    }
    fn missile(&self, _: &[u8]) -> Option<u32> {
        None
    }
    fn stat(&self, _: &[u8]) -> Option<u32> {
        None
    }
    fn skillcalc(&self, _: u32) -> Option<u32> {
        None
    }
    fn misscalc(&self, _: u32) -> Option<u32> {
        None
    }
}

fn code(family: Family, text: &str) -> Vec<u8> {
    let r = registry();
    d2_data::calc::compile_with(family, &NoLinks, &r, text.as_bytes())
        .unwrap()
        .code
}

/// A world under `rules` with one player (class 3) and its tables.
fn world(rules: Ruleset) -> (Fake, crate::skills::SkillTables, usize) {
    let mut f = Fake {
        rules,
        ..Fake::default()
    };
    let mut p = FUnit::new(UnitType::Player, 3).with(39, 55);
    p.stats.insert((83, 2), 7);
    p.states.push(32);
    p.mode = 4;
    p.skills.push(SkillEntry {
        skill: 0,
        base: 5,
        level_bonus: 0,
        owner_guid: -1,
        charges: 0,
        has_charges: false,
    });
    let u = f.add(p);
    let helm = f.add_item(FItem::default());
    f.items[helm].stats.insert((31, 0), 120);
    f.units[u].items.insert(1, helm);
    f.pets.insert(u, vec![363, 363, 417]);
    f.level_ids.insert(u, 66);
    (f, skill_tables(vec![skill_rec()]), u)
}

/// `text` as a skills formula of the player at level `lvl`.
fn skills(rules: Ruleset, text: &str, lvl: i32) -> i32 {
    let (mut f, mut t, u) = world(rules);
    t.skills_code = code(Family::Skills, text);
    eval_skill(&mut f, &t, Some(u), 0, 0, lvl)
}

#[test]
fn mod_functions_read_the_formula_unit() {
    let m = modded;
    assert_eq!(skills(m(), "ustate(32) * 10 + ustate(33)", 1), 10);
    assert_eq!(skills(m(), "uclass()", 1), 3);
    assert_eq!(skills(m(), "uskill(0, 0) * 100 + uskill(1, 1)", 1), 500);
    assert_eq!(skills(m(), "upets(363) * 10 + upets(417)", 1), 21);
    assert_eq!(skills(m(), "uitem(1, 31) + uitem(2, 31)", 1), 120);
    assert_eq!(skills(m(), "ulevel()", 1), 66);
    assert_eq!(skills(m(), "umode()", 1), 4);
    assert_eq!(skills(m(), "ustat(39, 0) + ustat(83, 2)", 1), 62);
    assert_eq!(skills(m(), "utype()", 1), 1);
}

#[test]
fn mod_functions_evaluate_other_skills_and_levels() {
    // `uspecial(16, s)`: skillcalc row 16 (`lvl`) of skill `s` at level
    // 3, D2RUST's special for another level than the context's.
    assert_eq!(skills(modded(), "uspecial(16, 0)", 1), 3);
    // `ueval(f)`: formula field `f` (here the code at 0: `lvl` + 1)
    // evaluated at level 4 instead of the context's.
    let (mut f, mut t, u) = world(Ruleset::Original);
    f.rules = modded();
    // Field 0: `ueval(9)`; field 9: `lvl + 1`.
    let mut buf = code(Family::Skills, "ueval(9)");
    buf.resize(9, 0);
    buf.extend([0x04, LVL_PARAM, 0x07, 1, 0x10, 0x00]);
    t.skills_code = buf;
    assert_eq!(eval_skill(&mut f, &t, Some(u), 0, 0, 1), 5);
}

#[test]
fn mod_parameters_answer_first() {
    // Param 100 has no D2RUST row: 0, the mod's 4242. Param 8 is `par1`
    // (0 in the test skill), the mod's 77 while a unit is in context.
    let bytes = [0x04, MOD_PARAM, 0x00];
    let par1 = [0x04, PAR1, 0x00];
    let run = |rules: Ruleset, b: &[u8], unit: bool| {
        let (mut f, mut t, u) = world(rules);
        t.skills_code = b.to_vec();
        eval_skill(&mut f, &t, unit.then_some(u), 0, 0, 7)
    };
    assert_eq!(run(Ruleset::Original, &bytes, true), 0);
    assert_eq!(run(modded(), &bytes, true), 4242);
    assert_eq!(run(Ruleset::Original, &par1, true), 0);
    assert_eq!(run(modded(), &par1, true), 77);
    // No unit: the mod declines and D2RUST's `par1` stays.
    assert_eq!(run(modded(), &par1, false), 0);
}

#[test]
fn missile_formulas_read_the_owner() {
    let (mut f, mut t, u) = world(modded());
    t.miss_code = code(Family::Missiles, "uclass() * 10 + ulevel()");
    assert_eq!(eval_missile(&mut f, &t, None, Some(u), 0, 0, 1), 96);
    t.miss_code = vec![0x04, MOD_PARAM, 0x00];
    assert_eq!(eval_missile(&mut f, &t, None, Some(u), 0, 0, 1), 4242);
    // Without an owner the view is absent.
    t.miss_code = code(Family::Missiles, "uclass()");
    assert_eq!(eval_missile(&mut f, &t, None, None, 0, 0, 1), -1);
}

#[test]
fn d2rust_formulas_are_unchanged_under_a_mod() {
    // Formulas with no mod keyword or parameter evaluate the same under
    // D2RUST's rules and the mod's.
    let lvl_plus = vec![0x04, LVL_PARAM, 0x07, 4, 0x10, 0x00];
    let mins = code(Family::Skills, "min(9, 2) + max(1, 9)");
    for lvl_bytes in [lvl_plus, mins] {
        for lvl in [1, 3, 20] {
            let run = |rules: Ruleset| {
                let (mut f, mut t, u) = world(rules);
                t.skills_code = lvl_bytes.clone();
                eval_skill(&mut f, &t, Some(u), 0, 0, lvl)
            };
            assert_eq!(run(Ruleset::Original), run(Ruleset::Mod(Arc::new(Plain))));
        }
    }
}

/// A mod game with formula functions that answers no parameter.
#[derive(Debug)]
struct Plain;

impl CalcProvider for Plain {
    fn registry(&self) -> &CalcRegistry {
        static R: CalcRegistry = CalcRegistry::new();
        &R
    }
    fn call(&self, _: Family, _: u8, _: &[i32], _: &mut dyn CalcContext) -> i32 {
        unreachable!("nothing is registered")
    }
}

impl ModRules for Plain {
    fn name(&self) -> &str {
        "plain"
    }
    fn calc(&self) -> Option<&dyn CalcProvider> {
        Some(self)
    }
}
