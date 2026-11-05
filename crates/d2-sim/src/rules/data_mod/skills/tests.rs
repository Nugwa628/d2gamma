//! What `skills.toml` says and refuses.

use super::*;

const FILE: &str = r#"
version = 1

[[skill]]
id = 390
name = "Ember Burst"

[[skill.part]]
kind = "missile"
missile = 20

[[skill.part]]
kind = "area_hit"
radius = "6 + lvl"
around = "caster"
[skill.part.damage]
element = "fire"
min = "10 + lvl * 4"
max = "ln12"
bonus_stat = 450

[[skill.part]]
kind = "state"
op = "add_stack"
state = 380
on = "area"
radius = 12
seconds = 8

[[skill.part]]
kind = "melee_hit"
weapon_percent = 150
attack_rating = 25

[[skill.part]]
kind = "summon"
class = 5
pet_type = 2
pet_max = "min(lvl, 4)"

[[skill.part]]
kind = "aura"
state = 30
range = 12
seconds = 10
[[skill.part.stat]]
stat = 31
value = "par1 * lvl"
"#;

fn setting(text: &str) -> String {
    match SkillRules::from_toml(text) {
        Err(SkillsError::Setting(s)) => s,
        Err(SkillsError::Toml(e)) => e.to_string(),
        Ok(_) => panic!("accepted"),
    }
}

#[test]
fn the_file_reads_into_parts() {
    let rules = SkillRules::from_toml(FILE).unwrap();
    assert_eq!(rules.defs().len(), 1);
    let d = rules.get(390).unwrap();
    assert_eq!(d.name, "Ember Burst");
    assert_eq!(d.parts.len(), 6);
    assert!(matches!(&d.parts[0], Part::Missile(m) if m.missile == 20 && !m.lob));
    let Part::AreaHit(a) = &d.parts[1] else {
        panic!("area")
    };
    assert_eq!((a.around, a.affects), (Around::Caster, Affects::Enemies));
    assert_eq!(a.damage.element, Element::Fire);
    assert_eq!(a.damage.bonus_stat, Some(450));
    assert_eq!(a.radius.text(), "6 + lvl");
    let Part::State(s) = &d.parts[2] else {
        panic!("state")
    };
    assert_eq!(
        (s.op, s.target, s.state),
        (StateOp::AddStack, StateTarget::Area, 380)
    );
    assert_eq!(s.radius.as_ref().unwrap().text(), "12");
    let Part::MeleeHit(m) = &d.parts[3] else {
        panic!("melee")
    };
    assert_eq!(m.weapon_percent, 150);
    let Part::Aura(a) = &d.parts[5] else {
        panic!("aura")
    };
    assert_eq!((a.affects, a.stats.len()), (Affects::Allies, 1));
    assert!(rules.get(391).is_none());
}

#[test]
fn only_the_block_slot_is_the_providers() {
    let rules = SkillRules::from_toml(FILE).unwrap();
    assert!(rules.has(SkillFn::Do, BLOCK_SLOT));
    assert!(!rules.has(SkillFn::Do, BLOCK_SLOT + 1));
    assert!(!rules.has(SkillFn::Start, BLOCK_SLOT));
    assert!(!SkillRules::none().has(SkillFn::Do, BLOCK_SLOT));
}

#[test]
fn an_empty_file_defines_nothing() {
    assert!(SkillRules::from_toml("").unwrap().is_empty());
    assert!(SkillRules::from_toml("version = 1\n").unwrap().is_empty());
}

#[test]
fn a_bad_formula_names_the_key_and_the_text() {
    let e = setting(
        "[[skill]]\nid = 1\n[[skill.part]]\nkind = \"area_hit\"\nradius = \"6 + bogus\"\n\
         [skill.part.damage]\nmin = 1\nmax = 2\n",
    );
    assert!(e.contains("skill[0].part[0]"), "{e}");
    assert!(e.contains("6 + bogus"), "{e}");
    assert!(e.contains("skillcalc"), "{e}");
    let e = setting(
        "[[skill]]\nid = 1\n[[skill.part]]\nkind = \"area_hit\"\nradius = 5\n\
         [skill.part.damage]\nmin = \"(1 +\"\nmax = 2\n",
    );
    assert!(e.contains("skill[0].part[0]") && e.contains("(1 +"), "{e}");
}

#[test]
fn formulas_use_the_engines_names_and_functions() {
    for ok in [
        "lvl * 5",
        "ln12 + dm34",
        "par1 + par8",
        "min(lvl, 10)",
        "max(1, lvl - 3) * 2",
        "rand(5, 9)",
        "lvl > 3 ? 10 : 20",
        "7",
    ] {
        Formula::compile(ok).unwrap_or_else(|e| panic!("{ok}: {e}"));
    }
    for bad in ["", "lvl +", "nosuchname + 1", "skill(fireball.par1)"] {
        assert!(Formula::compile(bad).is_err(), "{bad}");
    }
}

#[test]
fn a_part_the_file_does_not_know_is_refused() {
    let e = setting("[[skill]]\nid = 1\n[[skill.part]]\nkind = \"nuke\"\n");
    assert!(e.contains("skill[0].part[0]") && e.contains("nuke"), "{e}");
    let e =
        setting("[[skill]]\nid = 1\n[[skill.part]]\nkind = \"missile\"\nmissile = 1\nspeed = 3\n");
    assert!(e.contains("skill[0].part[0]") && e.contains("speed"), "{e}");
}

#[test]
fn a_skill_needs_parts_but_not_too_many() {
    let e = setting("[[skill]]\nid = 1\n");
    assert!(e.contains("part"), "{e}");
    let parts = "[[skill.part]]\nkind = \"missile\"\nmissile = 1\n".repeat(MAX_PARTS + 1);
    let e = setting(&format!("[[skill]]\nid = 1\n{parts}"));
    assert!(e.contains("skill[0].part") && e.contains("16"), "{e}");
}

#[test]
fn two_entries_for_one_skill_are_refused() {
    let one = "[[skill]]\nid = 7\n[[skill.part]]\nkind = \"missile\"\nmissile = 1\n";
    let e = setting(&format!("{one}{one}"));
    assert!(e.contains("skill[1].id") && e.contains("twice"), "{e}");
}

#[test]
fn an_area_state_needs_a_radius_and_an_aura_at_most_six_stats() {
    let e = setting("[[skill]]\nid = 1\n[[skill.part]]\nkind = \"state\"\nstate = 3\non = \"area\"\nseconds = 4\n");
    assert!(e.contains("radius"), "{e}");
    let stats = "[[skill.part.stat]]\nstat = 1\nvalue = 1\n".repeat(7);
    let e = setting(&format!(
        "[[skill]]\nid = 1\n[[skill.part]]\nkind = \"aura\"\nstate = 3\nrange = 5\nseconds = 4\n{stats}"
    ));
    assert!(e.contains("stat") && e.contains('6'), "{e}");
}

#[test]
fn an_unsupported_version_is_refused() {
    let e = setting("version = 2\n");
    assert!(e.contains("version"), "{e}");
}

#[test]
fn a_state_part_needs_a_time_from_somewhere() {
    let skills = SkillRules::from_toml(
        "[[skill]]\nid = 1\n[[skill.part]]\nkind = \"state\"\nstate = 380\nop = \"add_stack\"\n",
    )
    .unwrap();
    let none = StateRules::none();
    let e = skills.check_states(&none).unwrap_err().to_string();
    assert!(
        e.contains("skill[0].part[0].seconds") && e.contains("380"),
        "{e}"
    );
    let timed = StateRules::from_toml("[[state]]\nid = 380\nduration_seconds = 5\n").unwrap();
    skills.check_states(&timed).unwrap();
    let untimed = StateRules::from_toml("[[state]]\nid = 380\n").unwrap();
    assert!(skills.check_states(&untimed).is_err());
    // Ending or unstacking needs no time.
    let remove = SkillRules::from_toml(
        "[[skill]]\nid = 1\n[[skill.part]]\nkind = \"state\"\nstate = 380\nop = \"remove\"\n",
    )
    .unwrap();
    remove.check_states(&none).unwrap();
}

#[test]
fn a_curve_is_the_engines_own_evaluation() {
    use crate::skills::fake::{blank, skill_tables, FUnit, Fake};
    use crate::skills::{dm, eval_desc, ln};
    use crate::units::UnitType;
    let params: SkillParams = [3, 40, 7, 90, 0, 0, 5, 12];
    // The same formula run the way the skill host runs it: on a unit, in a
    // skills table whose row 0 carries the same Param columns.
    let mut row: d2_data::tables::Skills = blank();
    row.param1 = 3;
    row.param2 = 40;
    row.param3 = 7;
    row.param4 = 90;
    row.param7 = 5;
    row.param8 = 12;
    let tables = skill_tables(vec![row]);
    let mut world = Fake::default();
    let unit = world.add(FUnit::new(UnitType::Player, 0));
    for text in [
        "10 + lvl * 4",
        "ln12 + par8",
        "dm34 / 2",
        "min(lvl * 7, par4) + max(par1, 2)",
        "lvl > 5 ? par7 : par3",
    ] {
        let f = Formula::compile(text).unwrap();
        let curve = f.curve(&params, 1..=30);
        assert_eq!(curve.len(), 30);
        for (lvl, value) in curve {
            let engine = eval_desc(&mut world, &tables, f.code(), Some(unit), 0, 0, lvl);
            assert_eq!(value, engine, "`{text}` at level {lvl}");
        }
    }
    // And the named specials are the engine's `ln` and `dm` of those params.
    let ln12 = Formula::compile("ln12").unwrap().curve(&params, 1..=30);
    assert!(ln12.iter().all(|&(l, v)| v == ln(l, 3, 40)));
    let dm12 = Formula::compile("dm12").unwrap().curve(&params, 1..=30);
    assert!(dm12.iter().all(|&(l, v)| v == dm(l, 3, 40)));
}
