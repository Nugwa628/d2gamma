//! The state, skill and trigger editors read and write the files the engine reads.
mod kit_support;
use d2_sim::rules::data_mod::{
    Objective, QuestRules, Reward, SkillRules, StateRules, TriggerRules,
};
use mod_studio::kit::*;
use mod_studio::kit_ui::{apply_template, lift};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "mod-studio-kit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(p.join("files")).unwrap();
        Self(p)
    }
    fn write_template(&self) {
        fs::write(self.0.join(STATES_FILE), kit_support::STATES).unwrap();
        fs::write(self.0.join(SKILLS_FILE), kit_support::SKILLS).unwrap();
        fs::write(self.0.join(TRIGGERS_FILE), kit_support::TRIGGERS).unwrap();
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn engine_reads(dir: &std::path::Path) -> (StateRules, SkillRules, TriggerRules) {
    let read = |rel: &str| fs::read_to_string(dir.join(rel)).unwrap();
    (
        StateRules::from_toml(&read(STATES_FILE)).unwrap(),
        SkillRules::from_toml(&read(SKILLS_FILE)).unwrap(),
        TriggerRules::from_toml(&read(TRIGGERS_FILE)).unwrap(),
    )
}

#[test]
fn each_file_round_trips_to_what_the_engine_reads() {
    let temp = Temp::new();
    temp.write_template();
    let before = (
        StateRules::from_toml(kit_support::STATES).unwrap(),
        SkillRules::from_toml(kit_support::SKILLS).unwrap(),
        TriggerRules::from_toml(kit_support::TRIGGERS).unwrap(),
    );
    let mut kit = Kit::load(&temp.0).unwrap();
    assert_eq!(
        (kit.states.len(), kit.skills.len(), kit.triggers.len()),
        (2, 3, 3)
    );
    kit.dirty = true;
    kit.save().unwrap();
    assert_eq!(engine_reads(&temp.0), before);
    // And the editor reads its own output back to the same entries.
    let again = Kit::load(&temp.0).unwrap();
    assert_eq!(
        (&again.states, &again.skills, &again.triggers),
        (&kit.states, &kit.skills, &kit.triggers)
    );
    assert!(kit.problems().is_empty(), "{:?}", kit.problems());
}

#[test]
fn a_file_with_every_part_and_option_round_trips() {
    let temp = Temp::new();
    let skills = r#"
version = 1
[[skill]]
id = 5
name = "Everything"
[[skill.part]]
kind = "missile"
missile = 3
lob = true
at_aim = true
[[skill.part]]
kind = "resurrect"
overlay = 9
[[skill.part]]
kind = "nest"
overlay = 4
[[skill.part]]
kind = "wall"
missile = 3
centre = 4
[[skill.part]]
kind = "heal"
min = "10 + lvl"
max = "20 + lvl * 2"
[[skill.part]]
kind = "aura"
state = 30
range = "12 + lvl"
seconds = 10
affects = "everyone"
[[skill.part.stat]]
stat = 31
value = "par1 * lvl"
[[skill.part]]
kind = "curse"
state = 31
range = "6 + lvl"
seconds = "lvl * 2"
affects = "everyone"
[[skill.part.stat]]
stat = 31
value = "0 - lvl * 3"
[[skill.part]]
kind = "summon"
class = 5
pet_type = 2
pet_max = "min(lvl, 4)"
count = 2
level = 7
[[skill.part]]
kind = "state"
state = 30
op = "add_stack"
on = "area"
radius = 12
affects = "allies"
seconds = 8
count = 2
[[skill.part]]
kind = "melee_hit"
enhance = "20 + lvl"
own_element = true
stun_frames = "10"
monster_mode = "s1"
weapon_percent = 150
attack_rating = 25
[skill.part.damage]
element = "cold"
min = 1
max = "lvl"
seconds = 3
bonus_stat = 450
[[skill.part]]
kind = "area_hit"
radius = 6
around = "caster"
affects = "everyone"
[skill.part.damage]
min = 1
max = 2
"#;
    let states = "[[state]]\nid = 30\nduration_seconds = 4\nmax_stacks = 3\nreapply = \"add_stack\"\ndecay = \"lose_one\"\nicon = \"x\"\noverlay = 2\n[[state.effect]]\nstat = 31\nper_stack = -4\nmin_stacks = 2\nflat = true\n";
    let triggers = r#"
[[trigger]]
name = "all"
on = "state_start"
state = 30
while_stat = 99
chance = "lvl"
[[trigger.condition]]
kind = "stacks_at_least"
state = 30
count = 2
who = "other"
[[trigger.condition]]
kind = "life_below"
percent = 40
[[trigger.condition]]
kind = "life_above"
percent = 10
who = "other"
[[trigger.action]]
kind = "add_stack"
state = 30
count = 2
seconds = 5
who = "other"
[[trigger.action]]
kind = "cast_skill"
skill = 5
level = "lvl + 1"
at = "self"
[[trigger]]
on = "skill_used"
skills = [5]
while_state = 30
[[trigger.action]]
kind = "remove_stack"
state = 30
count = 1
[[trigger.action]]
kind = "remove_state"
state = 30
who = "other"
"#;
    fs::write(temp.0.join(STATES_FILE), states).unwrap();
    fs::write(temp.0.join(SKILLS_FILE), skills).unwrap();
    fs::write(temp.0.join(TRIGGERS_FILE), triggers).unwrap();
    let before = engine_reads(&temp.0);
    let mut kit = Kit::load(&temp.0).unwrap();
    kit.dirty = true;
    kit.save().unwrap();
    assert_eq!(engine_reads(&temp.0), before);
}

#[test]
fn an_edit_reaches_the_file_and_the_engine() {
    let temp = Temp::new();
    temp.write_template();
    let mut kit = Kit::load(&temp.0).unwrap();
    kit.states[1].max_stacks = 9;
    kit.states[1].effects[0].per_stack = 40;
    kit.states[1].decay = Some("lose_one".into());
    kit.skills[0].parts.push(PartEntry::new("missile").unwrap());
    kit.triggers[0].chance = Some(Fm::new("4 + lvl"));
    kit.triggers.swap(0, 2);
    kit.save().unwrap();
    let (states, skills, triggers) = engine_reads(&temp.0);
    let charges = states.get(191).unwrap();
    assert_eq!(charges.max_stacks, 9);
    assert_eq!(charges.effects[0].per_stack, 40);
    assert_eq!(skills.get(390).unwrap().parts.len(), 2);
    assert_eq!(triggers.defs()[2].chance.text(), "4 + lvl");
    assert_eq!(triggers.defs()[0].name, "Missed the window");
}

#[test]
fn a_file_changed_elsewhere_is_not_overwritten() {
    let temp = Temp::new();
    temp.write_template();
    let mut kit = Kit::load(&temp.0).unwrap();
    fs::write(temp.0.join(STATES_FILE), "version = 1\n").unwrap();
    kit.states.clear();
    assert!(kit
        .save()
        .unwrap_err()
        .to_string()
        .contains("changed outside Studio"));
    // A file the editor cannot read stops the load and is left alone.
    fs::write(temp.0.join(SKILLS_FILE), "[[skill]]\nid = 1\nspeed = 3\n").unwrap();
    let error = Kit::load(&temp.0).unwrap_err();
    assert!(format!("{error:#}").contains("skills.toml"), "{error:#}");
}

#[test]
fn an_empty_kit_writes_nothing() {
    let temp = Temp::new();
    let mut kit = Kit::load(&temp.0).unwrap();
    kit.dirty = true;
    kit.save().unwrap();
    assert!(!temp.0.join(STATES_FILE).exists());
    kit.states.push(StateEntry::new(5, "Smolder"));
    kit.save().unwrap();
    assert!(temp.0.join(STATES_FILE).exists());
    assert!(!temp.0.join(SKILLS_FILE).exists());
}

#[test]
fn formula_errors_are_the_engines() {
    for bad in ["6 + bogus", "lvl +", "", "skill(fireball.par1)"] {
        let error = Fm::new(bad).check().unwrap_err();
        assert!(error.contains("formula"), "{bad}: {error}");
    }
    assert!(Fm::new("min(lvl * 3, par1)").check().is_ok());
    let mut kit = Kit::default();
    let mut skill = SkillEntry::new(390, "Ember");
    if let PartEntry::AreaHit { damage, .. } = &mut skill.parts[0] {
        damage.max = Fm::new("10 + nosuchname");
    }
    kit.skills.push(skill);
    let problems = kit.problems();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].file, SKILLS_FILE);
    assert!(
        problems[0].message.contains("skill[0].part[0]"),
        "{}",
        problems[0].message
    );
    assert!(
        problems[0].message.contains("10 + nosuchname"),
        "{}",
        problems[0].message
    );
}

#[test]
fn cross_file_checks_are_the_engines() {
    let mut kit = Kit::default();
    kit.states.push(StateEntry::new(7, "Burn"));
    kit.states[0].duration_seconds = None;
    let mut skill = SkillEntry::new(1, "Torch");
    skill.parts = vec![PartEntry::State {
        state: 7,
        op: None,
        target: None,
        radius: None,
        affects: None,
        seconds: None,
        count: None,
    }];
    kit.skills.push(skill);
    let problems = kit.problems();
    assert!(
        problems
            .iter()
            .any(|p| p.file == SKILLS_FILE && p.message.contains("never end")),
        "{problems:?}"
    );
    let mut trigger = TriggerEntry::new("t");
    trigger.on = "state_end".into();
    trigger.state = Some(99);
    kit.triggers.push(trigger);
    assert!(kit
        .problems()
        .iter()
        .any(|p| p.file == TRIGGERS_FILE && p.message.contains("99")));
}

#[test]
fn the_curve_is_the_engines_evaluation() {
    use d2_sim::skills::{dm, ln};
    let params = [3, 40, 0, 0, 0, 0, 0, 0];
    let line = Fm::new("10 + lvl * 4").curve(&params, CURVE_LEVELS);
    assert_eq!(line.len(), 30);
    assert!(line.iter().all(|&(l, v)| v == 10 + 4 * l));
    assert!(Fm::new("ln12")
        .curve(&params, CURVE_LEVELS)
        .iter()
        .all(|&(l, v)| v == ln(l, 3, 40)));
    assert!(Fm::new("dm12")
        .curve(&params, CURVE_LEVELS)
        .iter()
        .all(|&(l, v)| v == dm(l, 3, 40)));
    let capped = Fm::new("min(lvl * 7, par2)").curve(&params, CURVE_LEVELS);
    assert_eq!(capped[0], (1, 7));
    assert_eq!(capped[29], (30, 40));
    assert!(Fm::new("bogus").curve(&params, CURVE_LEVELS).is_empty());
}

#[test]
fn states_and_triggers_read_as_sentences() {
    let temp = Temp::new();
    temp.write_template();
    let kit = Kit::load(&temp.0).unwrap();
    let names = Names::default();
    assert_eq!(
        kit.trigger_sentence(&names, &kit.triggers[0]),
        "When you attack (8%), if you are not in Reaction window: apply Reaction window"
    );
    assert_eq!(
        kit.trigger_sentence(&names, &kit.triggers[1]),
        "When you use Ember Lash or Frost Nova or Quick Strike, if you are in Reaction window: add 1 stack of Charges, then end Reaction window"
    );
    assert_eq!(
        kit.trigger_sentence(&names, &kit.triggers[2]),
        "When Reaction window runs out, if you are in Charges: remove 1 stack of Charges"
    );
    assert_eq!(
        kit.trigger_source(&names, &kit.triggers[0]),
        "while you have skill 393 (its level is lvl)"
    );
    let names = Names {
        stats: vec![(340, "item_damagepercent".into())],
        ..Names::default()
    };
    let preview = kit.state_preview(&names, &kit.states[1]);
    assert_eq!(
        preview,
        [
            "at 1 stack: no bonus",
            "at 3 stacks: +25% damage",
            "at 5 stacks: +25% damage"
        ]
    );
    assert_eq!(
        effect_text("item_fastercastrate", -10),
        "-10 fastercastrate"
    );
    assert_eq!(effect_text("stat 7", 3), "+3 stat 7");
    assert!(kit.state_summary(&kit.states[1]).contains("up to 5 stacks"));
}

#[test]
fn a_template_is_added_with_new_ids_and_new_table_rows() {
    let temp = Temp::new();
    let mut kit = Kit::load(&temp.0).unwrap();
    let mut tables = kit_support::tables(&temp.0);
    let skills_before = tables
        .edited
        .tables
        .iter()
        .find(|t| t.name() == "skills")
        .unwrap()
        .rows
        .len();
    apply_template(&mut kit, Some(&mut tables), &TEMPLATES[0], &[]).unwrap();
    let skills = tables
        .edited
        .tables
        .iter()
        .find(|t| t.name() == "skills")
        .unwrap();
    assert_eq!(skills.rows.len(), skills_before + 4);
    let slot = skills
        .header
        .iter()
        .position(|h| h.eq_ignore_ascii_case(b"srvdofunc"))
        .unwrap();
    // The three block skills got the block slot; the passive did not.
    assert_eq!(&skills.rows[skills_before].cells[slot], b"400");
    assert_eq!(&skills.rows[skills_before + 2].cells[slot], b"400");
    assert_ne!(&skills.rows[skills_before + 3].cells[slot], b"400");
    let first = skills_before as u16;
    assert_eq!(kit.skills[0].id, first);
    assert_eq!(kit.triggers[0].while_skill, Some(first + 3));
    let ids = kit.ids();
    assert!(
        ids.states.iter().all(|s| *s >= 192) && ids.stats.iter().all(|s| *s >= 341),
        "{ids:?}"
    );
    assert!(kit.problems().is_empty(), "{:?}", kit.problems());
    // A second copy with the same ids is refused.
    let same = [192, 193, 341, first, first + 1, first + 2, first + 3];
    let error = apply_template(&mut kit, None, &TEMPLATES[0], &same).unwrap_err();
    assert!(error.to_string().contains("already defined"), "{error}");
    // Without tables the ids are the ones given.
    let mut other = Kit::default();
    apply_template(
        &mut other,
        None,
        &TEMPLATES[0],
        &[600, 601, 700, 800, 801, 802, 803],
    )
    .unwrap();
    assert_eq!(other.states[0].id, 600);
    assert_eq!(other.triggers[1].skills, [800, 801, 802]);
    assert_eq!(other.skills[0].parts.len(), 1);
    assert!(other.problems().is_empty());
}

#[test]
fn the_patch_for_the_new_rows_is_valid() {
    let temp = Temp::new();
    let mut kit = Kit::default();
    let mut tables = kit_support::tables(&temp.0);
    apply_template(&mut kit, Some(&mut tables), &TEMPLATES[0], &[]).unwrap();
    assert!(tables.problems.is_empty(), "{:?}", tables.problems);
    let bytes = tables.diff().unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("table skills") && text.contains("table states"),
        "{text}"
    );
}

#[test]
fn every_block_studio_offers_is_in_the_engine_vocabulary() {
    use d2_sim::rules::data_mod::blocks::Family;
    for (family, list) in [
        (Family::SkillPart, PART_KINDS),
        (Family::Event, EVENTS),
        (Family::Condition, CONDITION_KINDS),
        (Family::Action, ACTION_KINDS),
    ] {
        for (name, _) in list {
            assert!(
                !block_help(family, name).is_empty(),
                "{} `{name}` is not in the block vocabulary",
                family.noun()
            );
        }
    }
    assert_eq!(block_help(Family::SkillPart, "no_such_block"), "");
}

#[test]
fn the_block_reference_lists_the_vocabulary_for_each_file() {
    let skills = block_reference(SKILLS_FILE);
    let titles: Vec<_> = skills.iter().map(|(t, _)| *t).collect();
    assert_eq!(titles, ["Skill parts", "Targets", "Values"]);
    assert!(skills[0]
        .1
        .iter()
        .any(|r| r.name == "area_hit" && !r.label.is_empty() && !r.about.is_empty()));
    let triggers = block_reference(TRIGGERS_FILE);
    assert_eq!(triggers.len(), 4);
    assert!(block_reference(STATES_FILE).is_empty());
}

#[test]
fn every_part_the_vocabulary_lists_can_be_added_saved_and_read_by_the_engine() {
    use d2_sim::rules::data_mod::blocks::{self, Family};
    // The editor offers exactly the skill parts the engine's vocabulary has.
    let mut offered: Vec<&str> = PART_KINDS.iter().map(|(k, _)| *k).collect();
    let mut listed: Vec<&str> = blocks::family(Family::SkillPart)
        .iter()
        .map(|b| b.name)
        .collect();
    offered.sort_unstable();
    listed.sort_unstable();
    assert_eq!(offered, listed);

    let names = Names::default();
    for (n, (kind, _)) in PART_KINDS.iter().enumerate() {
        let temp = Temp::new();
        let mut kit = Kit::load(&temp.0).unwrap();
        let mut skill = SkillEntry::new(390 + n as u16, "Probe");
        let mut part = PartEntry::new(kind).expect("a new part");
        assert_eq!(part.kind(), *kind);
        // A minion order that attacks names the skill it casts.
        if let PartEntry::MinionOrder { order, skill, .. } = &mut part {
            *order = "attack".into();
            *skill = Some(5);
        }
        // A part that runs another skill's parts names one of the file.
        let helper = 800 + n as u16;
        let timed = match &mut part {
            PartEntry::Repeat { skill, .. } => {
                *skill = Some(helper);
                true
            }
            PartEntry::Leap { land, .. } => {
                *land = Some(helper);
                true
            }
            _ => false,
        };
        skill.parts = vec![part];
        assert!(!kit.part_summary(&names, &skill.parts[0]).is_empty());
        kit.skills.push(skill);
        if timed {
            kit.skills.push(SkillEntry::new(helper, "Helper"));
        }
        assert!(kit.problems().is_empty(), "{kind}: {:?}", kit.problems());
        kit.dirty = true;
        kit.save().unwrap();
        let text = fs::read_to_string(temp.0.join(SKILLS_FILE)).unwrap();
        assert!(text.contains(&format!("kind = \"{kind}\"")), "{text}");
        let rules = SkillRules::from_toml(&text).unwrap();
        assert_eq!(rules.defs().len(), kit.skills.len(), "{kind}");
        // And Studio reads back what it wrote.
        let again = Kit::load(&temp.0).unwrap();
        assert_eq!(again.skills[0].parts, kit.skills[0].parts, "{kind}");
    }
}

#[test]
fn a_charge_up_and_its_state_are_edited_and_read_by_the_engine() {
    let temp = Temp::new();
    let mut kit = Kit::load(&temp.0).unwrap();
    let mut skill = SkillEntry::new(390, "Release");
    if let PartEntry::AreaHit { damage, .. } = &mut skill.parts[0] {
        damage.charge = Some(ChargeEntry {
            state: 191,
            percent: Fm::new("lvl * 2"),
            consume: true,
        });
    }
    kit.skills.push(skill);
    assert!(kit.problems().is_empty(), "{:?}", kit.problems());
    kit.dirty = true;
    kit.save().unwrap();
    let rules =
        SkillRules::from_toml(&fs::read_to_string(temp.0.join(SKILLS_FILE)).unwrap()).unwrap();
    let d = rules.get(390).unwrap();
    assert!(format!("{d:?}").contains("consume: true"));
    // Renumbering a state follows into the charge.
    let mut seen = Vec::new();
    kit.visit_ids(&mut |kind, id| {
        if matches!(kind, IdKind::State) {
            seen.push(*id);
        }
    });
    assert_eq!(seen, [191]);
}

#[test]
fn a_trigger_by_one_skill_is_edited_read_and_renumbered() {
    let temp = Temp::new();
    let mut kit = Kit::load(&temp.0).unwrap();
    let mut t = TriggerEntry::new("Only the bash");
    t.on = "hit".into();
    t.by_skill = Some(7);
    kit.triggers.push(t);
    assert!(kit.problems().is_empty(), "{:?}", kit.problems());
    kit.dirty = true;
    kit.save().unwrap();
    let text = fs::read_to_string(temp.0.join(TRIGGERS_FILE)).unwrap();
    assert!(text.contains("by_skill = 7"), "{text}");
    let rules = TriggerRules::from_toml(&text).unwrap();
    assert_eq!(rules.defs()[0].by_skill, Some(7));
    let again = Kit::load(&temp.0).unwrap();
    assert_eq!(again.triggers[0].by_skill, Some(7));
    assert!(again
        .trigger_sentence(&Names::default(), &again.triggers[0])
        .starts_with("When you hit something with"));
    let mut seen = Vec::new();
    kit.visit_ids(&mut |kind, id| {
        if matches!(kind, IdKind::Skill) {
            seen.push(*id);
        }
    });
    assert!(seen.contains(&7));
}

/// Sets a cell of a base table in the baseline and the edited copy.
fn base_cell(t: &mut mod_studio::tables::Tables, table: &str, row: usize, col: &str, v: &str) {
    let ti = t
        .edited
        .tables
        .iter()
        .position(|x| x.name() == table)
        .unwrap();
    let c = t.edited.tables[ti]
        .header
        .iter()
        .position(|h| h.eq_ignore_ascii_case(col.as_bytes()))
        .unwrap();
    t.baseline.tables[ti].rows[row].cells[c] = v.as_bytes().to_vec();
    t.edited.tables[ti].rows[row].cells[c] = v.as_bytes().to_vec();
}

fn edited_cell(t: &mod_studio::tables::Tables, table: &str, row: usize, col: &str) -> String {
    let x = t.edited.tables.iter().find(|x| x.name() == table).unwrap();
    let c = x
        .header
        .iter()
        .position(|h| h.eq_ignore_ascii_case(col.as_bytes()))
        .unwrap();
    String::from_utf8_lossy(&x.rows[row].cells[c]).into_owned()
}

#[test]
fn a_base_skill_is_copied_as_parts_and_compared_with_its_base() {
    let temp = Temp::new();
    let mut kit = Kit::load(&temp.0).unwrap();
    let mut tables = kit_support::tables(&temp.0);
    // Base skill 5 is a nova of missile 3 (firebolt, velocity 10 + 8 a level).
    base_cell(&mut tables, "skills", 5, "srvdofunc", "22");
    base_cell(&mut tables, "skills", 5, "srvmissilea", "firebolt");
    base_cell(&mut tables, "skills", 5, "calc1", "lvl");
    base_cell(&mut tables, "missiles", 3, "vel", "10");
    base_cell(&mut tables, "missiles", 3, "vellev", "8");
    tables.use_baseline_as_reference();

    let rows = lift::liftable(&tables);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!((rows[0].id, rows[0].family.name()), (5, "nova"));

    let lifted = lift::copy_base(&mut kit, &mut tables, 5).unwrap();
    assert!(lifted.toml.contains("count = 64"));
    assert_eq!(kit.skills.len(), 1);
    assert_eq!(kit.skills[0].id, 5);
    assert!(matches!(
        kit.skills[0].parts[..],
        [PartEntry::MissileRing { .. }]
    ));
    // The row is the block slot's now; the base row keeps its other cells.
    assert_eq!(edited_cell(&tables, "skills", 5, "srvdofunc"), "400");
    assert_eq!(edited_cell(&tables, "skills", 5, "calc1"), "lvl");
    assert!(kit.problems().is_empty(), "{:?}", kit.problems());

    // Compared with its base, the copy is the same until it is edited.
    let same = lift::diff_against_base(&kit, &tables, 5).unwrap();
    assert!(!lift::differs(&same), "{same:?}");
    if let PartEntry::MissileRing { count, .. } = &mut kit.skills[0].parts[0] {
        *count = Some(Fm::new("32"));
    }
    let lines = lift::diff_against_base(&kit, &tables, 5).unwrap();
    let changed: Vec<_> = lines
        .iter()
        .filter(|(c, _)| *c != lift::Change::Same)
        .collect();
    assert_eq!(changed.len(), 2, "{changed:?}");
    assert_eq!(changed[0].0, lift::Change::Removed);
    assert!(changed[0].1.contains("64"));
    assert_eq!(changed[1].0, lift::Change::Added);
    assert!(changed[1].1.contains("32"));

    // The engine reads what was saved.
    kit.dirty = true;
    kit.save().unwrap();
    let text = fs::read_to_string(temp.0.join(kit_support_skills())).unwrap();
    assert!(SkillRules::from_toml(&text).unwrap().get(5).is_some());

    // The same skill cannot be copied twice, nor a row that is not there or
    // is not one of the families.
    let twice = lift::copy_base(&mut kit, &mut tables, 5).unwrap_err();
    assert!(twice.to_string().contains("already in this kit"), "{twice}");
    let none = lift::copy_base(&mut kit, &mut tables, 6000).unwrap_err();
    assert!(none.to_string().contains("no skills row"), "{none}");
    let rust = lift::copy_base(&mut kit, &mut tables, 7).unwrap_err();
    assert!(rust.to_string().contains("not lifted"), "{rust}");
    assert_eq!(kit.skills.len(), 1);
}

fn kit_support_skills() -> &'static str {
    SKILLS_FILE
}

#[test]
fn the_line_diff_keeps_order_and_marks_each_side() {
    use lift::Change::{Added, Removed, Same};
    let d = lift::diff(
        "a
b
c", "a
x
c
d",
    );
    let kinds: Vec<_> = d.iter().map(|(c, t)| (*c, t.as_str())).collect();
    assert_eq!(
        kinds,
        [
            (Same, "a"),
            (Removed, "b"),
            (Added, "x"),
            (Same, "c"),
            (Added, "d")
        ]
    );
    assert!(lift::diff("", "").is_empty());
    assert!(!lift::differs(&lift::diff("a", "a")));
}

#[test]
fn call_script_entries_round_trip_through_studio_and_engine() {
    let dir = Temp::new();
    let skills = "[[skill]]\nid=0\n[[skill.part]]\nkind='call_script'\nmodule='main.luau'\nfunction='run'\nargument='lvl * 2'\n";
    let triggers = include_str!("../../../templates/script-reaction/files/triggers.toml");
    fs::write(dir.0.join(SKILLS_FILE), skills).unwrap();
    fs::write(dir.0.join(TRIGGERS_FILE), triggers).unwrap();
    let mut kit = Kit::load(&dir.0).unwrap();
    assert_eq!(kit.skills[0].parts[0].kind(), "call_script");
    assert_eq!(kit.triggers[0].actions[0].kind(), "call_script");
    kit.save().unwrap();
    let saved = fs::read_to_string(dir.0.join(SKILLS_FILE)).unwrap();
    assert_eq!(SkillRules::from_toml(&saved).unwrap().defs().len(), 1);
    let saved = fs::read_to_string(dir.0.join(TRIGGERS_FILE)).unwrap();
    assert_eq!(TriggerRules::from_toml(&saved).unwrap().defs().len(), 1);
}

#[test]
fn a_channel_is_edited_checked_and_read_by_the_engine() {
    let temp = Temp::new();
    let mut kit = Kit::load(&temp.0).unwrap();
    let mut channel = SkillEntry::new(390, "Channel");
    channel.parts = vec![PartEntry::Repeat {
        skill: Some(391),
        every: Some(Fm::new("10")),
        times: Some(Fm::new("lvl + 2")),
        now: false,
        while_state: None,
        stop_on_move: true,
    }];
    kit.skills.push(channel);
    // The skill it repeats is not in the file yet: the game's own check says so.
    let problems = kit.problems();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].message.contains("not defined"), "{problems:?}");
    kit.skills.push(SkillEntry::new(391, "Pulse"));
    assert!(kit.problems().is_empty(), "{:?}", kit.problems());
    kit.dirty = true;
    kit.save().unwrap();
    let text = fs::read_to_string(temp.0.join(SKILLS_FILE)).unwrap();
    assert!(text.contains("stop_on_move = true"), "{text}");
    assert!(text.contains("now = false"), "{text}");
    let rules = SkillRules::from_toml(&text).unwrap();
    rules.check_ticks().unwrap();
    let again = Kit::load(&temp.0).unwrap();
    assert_eq!(again.skills[0].parts, kit.skills[0].parts);
    // The skills it names follow a renumbering.
    let mut seen = Vec::new();
    kit.visit_ids(&mut |kind, id| {
        if matches!(kind, IdKind::Skill) {
            seen.push(*id);
        }
    });
    assert!(seen.contains(&391));
}

#[test]
fn a_channel_of_the_parts_after_the_repeat_needs_no_second_skill() {
    let temp = Temp::new();
    let mut kit = Kit::load(&temp.0).unwrap();
    let mut channel = SkillEntry::new(390, "Channel");
    let repeat = PartEntry::Repeat {
        skill: None,
        every: Some(Fm::new("10")),
        times: Some(Fm::new("lvl + 2")),
        now: true,
        while_state: None,
        stop_on_move: false,
    };
    channel.parts = vec![repeat.clone()];
    kit.skills.push(channel);
    // Nothing follows the repeat: the game's own check says so.
    let problems = kit.problems();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].message.contains("no part after"),
        "{problems:?}"
    );
    kit.skills[0].parts = vec![
        repeat,
        PartEntry::Missile {
            missile: 5,
            lob: false,
            at_aim: false,
            channel: true,
            cc: Vec::new(),
        },
    ];
    assert!(kit.problems().is_empty(), "{:?}", kit.problems());
    kit.dirty = true;
    kit.save().unwrap();
    let text = fs::read_to_string(temp.0.join(SKILLS_FILE)).unwrap();
    assert!(!text.contains("skill = "), "{text}");
    assert!(text.contains("channel = true"), "{text}");
    let again = Kit::load(&temp.0).unwrap();
    assert_eq!(again.skills[0].parts, kit.skills[0].parts);
}

const QUESTS: &str = r#"
version = 1
[[quest]]
id = 1
name = "Pest Control"
text = "Kill three rats."
giver = 7
[[quest.objective]]
kind = "kill"
monster = 12
count = 3
[[quest.objective]]
kind = "enter"
level = 4
[[quest.reward]]
kind = "stat"
stat = 14
value = 500
[[quest.reward]]
kind = "message"
text = "Thank you."

[[quest]]
id = 2
name = "Report Back"
requires = [1]
[[quest.objective]]
kind = "talk"
npc = 7
[[quest.objective]]
kind = "reach_stat"
stat = 14
value = 100
[[quest.objective]]
kind = "use_skill"
skill = 36
"#;

#[test]
fn quests_round_trip_to_what_the_engine_reads() {
    let temp = Temp::new();
    fs::write(temp.0.join(QUESTS_FILE), QUESTS).unwrap();
    let mut kit = Kit::load(&temp.0).unwrap();
    assert_eq!(kit.quests.len(), 2);
    assert_eq!(kit.quests[1].requires, [1]);
    assert!(kit.problems().is_empty(), "{:?}", kit.problems());
    kit.dirty = true;
    kit.save().unwrap();
    let text = fs::read_to_string(temp.0.join(QUESTS_FILE)).unwrap();
    let engine = QuestRules::from_toml(&text).unwrap();
    assert_eq!(engine, QuestRules::from_toml(QUESTS).unwrap());
    assert_eq!(Kit::load(&temp.0).unwrap().quests, kit.quests);
    // The other files are not made for a project that has none.
    assert!(!temp.0.join(STATES_FILE).exists());
}

#[test]
fn a_quest_the_engine_refuses_is_reported_and_blocks_the_save() {
    let temp = Temp::new();
    let mut kit = Kit::load(&temp.0).unwrap();
    assert!(kit.quests.is_empty());
    kit.quests.push(QuestEntry::new(1, "First"));
    assert!(kit.problems().is_empty(), "{:?}", kit.problems());
    // A second quest with the same id, and one that requires a quest not there.
    let mut again = QuestEntry::new(1, "Again");
    again.requires = vec![9];
    kit.quests.push(again);
    let problems = kit.problems();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].file, QUESTS_FILE);
    kit.quests[1].id = QuestEntry::free_id(&kit.quests);
    assert_eq!(kit.quests[1].id, 2);
    assert!(
        kit.problems()[0].message.contains("quest 9"),
        "{:?}",
        kit.problems()
    );
    kit.quests[1].requires = vec![1];
    kit.quests[1].objectives.clear();
    assert!(
        kit.problems()[0].message.contains("objective"),
        "{:?}",
        kit.problems()
    );
    // The editor does not write a file the engine refuses.
    let mut editor = mod_studio::kit_ui::KitEditor::default();
    editor.open(&temp.0);
    let k = editor.kit.as_mut().unwrap();
    k.quests = kit.quests.clone();
    k.dirty = true;
    let err = editor.save().unwrap_err().to_string();
    assert!(err.contains(QUESTS_FILE), "{err}");
    assert!(!temp.0.join(QUESTS_FILE).exists());
}

#[test]
fn every_registered_objective_and_reward_can_be_added_from_the_editor() {
    use d2_sim::rules::data_mod::blocks::{self, Family};
    for b in blocks::family(Family::Objective) {
        let o = new_objective(b.name).unwrap_or_else(|| panic!("no new objective `{}`", b.name));
        assert_eq!(objective_kind(&o), b.name);
    }
    for b in blocks::family(Family::Reward) {
        let r = new_reward(b.name).unwrap_or_else(|| panic!("no new reward `{}`", b.name));
        assert_eq!(reward_kind(&r), b.name);
    }
    assert!(new_objective("zzz").is_none());
    // A quest made of one of each is something the engine reads.
    let mut q = QuestEntry::new(1, "All");
    q.objectives = blocks::family(Family::Objective)
        .iter()
        .filter_map(|b| new_objective(b.name))
        .collect();
    q.rewards = blocks::family(Family::Reward)
        .iter()
        .filter_map(|b| new_reward(b.name))
        .collect();
    let mut kit = Kit::default();
    kit.quests.push(q);
    assert!(kit.problems().is_empty(), "{:?}", kit.problems());
}

#[test]
fn a_quest_reads_as_a_sentence_with_row_names() {
    let names = Names {
        monsters: vec![(12, "Rat".into())],
        stats: vec![(14, "Gold".into())],
        ..Names::default()
    };
    let mut q = QuestEntry::new(1, "Pest Control");
    q.objectives = vec![Objective::Kill {
        monster: 12,
        count: 5,
    }];
    assert_eq!(q.summary(&names), "Pest Control: Kill 5 x Rat");
    assert_eq!(
        q.summary(&Names::default()),
        "Pest Control: Kill 5 x monster 12"
    );
    assert_eq!(
        reward_text(
            &names,
            &Reward::Stat {
                stat: 14,
                value: 500
            }
        ),
        "+500 Gold"
    );
}

#[test]
fn item_blocks_read_as_sentences_and_round_trip() {
    let o = Objective::Collect {
        item: "hp1".into(),
        count: 3,
        consume: false,
    };
    let r = Reward::Item {
        item: "mp1".into(),
        count: 2,
    };
    assert_eq!(objective_text(&Names::default(), &o), "Collect 3 x hp1");
    let handed = Objective::Collect {
        item: "hp1".into(),
        count: 3,
        consume: true,
    };
    assert_eq!(
        objective_text(&Names::default(), &handed),
        "Collect 3 x hp1 (handed in)"
    );
    assert_eq!(reward_text(&Names::default(), &r), "Give 2 x mp1");
    let temp = Temp::new();
    let mut kit = Kit::load(&temp.0).unwrap();
    let mut q = QuestEntry::new(1, "Fetch");
    q.objectives = vec![o, handed];
    q.rewards = vec![r];
    kit.quests.push(q);
    kit.dirty = true;
    kit.save().unwrap();
    let text = fs::read_to_string(temp.0.join(QUESTS_FILE)).unwrap();
    // Only the objective that sets the flag writes it.
    assert_eq!(text.matches("consume").count(), 1);
    assert!(QuestRules::from_toml(&text).unwrap().wants_items());
    assert_eq!(Kit::load(&temp.0).unwrap().quests, kit.quests);
}

#[test]
fn the_quest_log_panel_is_the_engines_and_binds_the_published_variables() {
    use mod_studio::quest_panel;
    let temp = Temp::new();
    fs::write(temp.0.join(QUESTS_FILE), QUESTS).unwrap();
    let kit = Kit::load(&temp.0).unwrap();
    let names = Names::default();
    let made = quest_panel::write(&temp.0, &kit.quests, &names).unwrap();
    assert_eq!(made.left_out, 0);
    let text = fs::read_to_string(temp.0.join(quest_panel::PANEL_FILE)).unwrap();
    let panel = d2_modkit::ui::Panel::parse(&text).unwrap();
    assert_eq!(panel.replace.as_deref(), Some("quest_log"));
    let sources: Vec<&str> = panel
        .elements
        .iter()
        .filter_map(|e| e.binding.as_ref().map(|b| b.source.as_str()))
        .collect();
    let first = &kit.quests[0];
    assert!(sources.contains(
        &format!(
            "var.{}",
            d2_sim::rules::data_mod::quests::variable_name(first.id, 0)
        )
        .as_str()
    ));
    assert!(sources.contains(
        &format!(
            "var.{}",
            d2_sim::rules::data_mod::quests::variable_name(first.id, 1)
        )
        .as_str()
    ));
    // Writing again replaces it; a file of the modder's own is left alone.
    quest_panel::write(&temp.0, &kit.quests, &names).unwrap();
    fs::write(
        temp.0.join(quest_panel::PANEL_FILE),
        "version = 1
",
    )
    .unwrap();
    assert!(quest_panel::write(&temp.0, &kit.quests, &names).is_err());
    assert_eq!(
        fs::read_to_string(temp.0.join(quest_panel::PANEL_FILE)).unwrap(),
        "version = 1
"
    );
}

#[test]
fn a_quest_log_panel_keeps_to_the_screen_and_the_element_budget() {
    use mod_studio::quest_panel;
    let quests: Vec<QuestEntry> = (1..=100)
        .map(|id| {
            let mut q = QuestEntry::new(id, "Q");
            q.objectives = vec![Objective::Enter { level: 1 }; 3];
            q
        })
        .collect();
    let made = quest_panel::make(&quests, &Names::default());
    d2_modkit::ui::Panel::parse(&made.text).unwrap();
    assert!(made.left_out > 0 && made.left_out < 100);
    let (used, cap) = quest_panel::variable_use(&quests);
    assert_eq!(
        (used, cap),
        (400, d2_sim::rules::data_mod::quests::MAX_VARIABLES)
    );
}

#[test]
fn declared_hit_controls_survive_studio_save_for_every_supported_part() {
    let temp = Temp::new();
    let mut kit = Kit::load(&temp.0).unwrap();
    let mut skill = SkillEntry::new(1, "Controlled hit");
    skill.parts.clear();
    for kind in ["missile", "melee_hit", "area_hit", "chain", "bounce"] {
        let mut part = PartEntry::new(kind).unwrap();
        *part.controls_mut().unwrap() = vec![
            ControlEntry {
                kind: "interrupt".into(),
                seconds: None,
                chance: None,
            },
            ControlEntry {
                kind: "freeze".into(),
                seconds: Some(1.5),
                chance: Some(25),
            },
        ];
        skill.parts.push(part);
    }
    kit.skills.push(skill);
    kit.dirty = true;
    kit.save().unwrap();
    let saved = fs::read_to_string(temp.0.join(SKILLS_FILE)).unwrap();
    let before = SkillRules::from_toml(&saved).unwrap();
    let mut again = Kit::load(&temp.0).unwrap();
    assert_eq!(again.skills, kit.skills);
    again.dirty = true;
    again.save().unwrap();
    assert_eq!(
        SkillRules::from_toml(&fs::read_to_string(temp.0.join(SKILLS_FILE)).unwrap()).unwrap(),
        before
    );
}

#[test]
fn showcase_skill_is_editable_without_losing_interrupt() {
    let temp = Temp::new();
    let showcase = include_str!("../../../templates/showcase/files/skills.toml");
    fs::write(temp.0.join(SKILLS_FILE), showcase).unwrap();
    let expected = SkillRules::from_toml(showcase).unwrap();
    let mut kit = Kit::load(&temp.0).unwrap();
    assert_eq!(
        kit.skills[0].parts[2].controls_mut().unwrap()[0].kind,
        "interrupt"
    );
    kit.dirty = true;
    kit.save().unwrap();
    assert_eq!(
        SkillRules::from_toml(&fs::read_to_string(temp.0.join(SKILLS_FILE)).unwrap()).unwrap(),
        expected
    );
}
