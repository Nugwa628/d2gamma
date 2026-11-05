//! The vocabulary tables agree with the readers they describe.

use std::collections::BTreeSet;

use super::*;
use crate::rules::data_mod::{QuestRules, SkillRules, TriggerRules};

/// The backticked names of a serde "expected one of `a`, `b`" message.
fn expected(message: &str) -> BTreeSet<String> {
    let tail = message
        .split("expected one of")
        .nth(1)
        .or_else(|| message.split("expected").nth(1))
        .unwrap_or_else(|| panic!("no list of names in: {message}"));
    tail.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

fn names(of: Family) -> BTreeSet<String> {
    family(of).iter().map(|b| b.name.to_owned()).collect()
}

fn skills_error(text: &str) -> String {
    match SkillRules::from_toml(text) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("accepted:\n{text}"),
    }
}

fn triggers_error(text: &str) -> String {
    match TriggerRules::from_toml(text) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("accepted:\n{text}"),
    }
}

fn quests_error(text: &str) -> String {
    match QuestRules::from_toml(text) {
        Err(e) => e.to_string(),
        Ok(_) => panic!(
            "accepted:
{text}"
        ),
    }
}

fn read(kind: FileKind, text: &str) -> Result<(), String> {
    match kind {
        FileKind::Skills => SkillRules::from_toml(text)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        FileKind::Triggers => TriggerRules::from_toml(text)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        FileKind::Quests => QuestRules::from_toml(text)
            .map(|_| ())
            .map_err(|e| e.to_string()),
    }
}

fn file_of(block: &Block) -> FileKind {
    match block.family {
        Family::SkillPart => FileKind::Skills,
        Family::Objective | Family::Reward => FileKind::Quests,
        _ => FileKind::Triggers,
    }
}

const SKILL_HEAD: &str = "[[skill]]\nid = 1\n[[skill.part]]\n";
const TRIGGER_HEAD: &str = "[[trigger]]\non = \"attack\"\nwhile_skill = 1\n";

#[test]
fn every_name_the_readers_accept_is_registered_and_no_other() {
    let part = skills_error(&format!("{SKILL_HEAD}kind = \"zzz\"\n"));
    assert_eq!(expected(&part), names(Family::SkillPart), "{part}");
    let on = triggers_error("[[trigger]]\non = \"zzz\"\nwhile_skill = 1\n");
    assert_eq!(expected(&on), names(Family::Event), "{on}");
    let cond = triggers_error(&format!(
        "{TRIGGER_HEAD}[[trigger.condition]]\nkind = \"zzz\"\n[[trigger.action]]\nkind = \"remove_state\"\nstate = 1\n"
    ));
    assert_eq!(expected(&cond), names(Family::Condition), "{cond}");
    let action = triggers_error(&format!(
        "{TRIGGER_HEAD}[[trigger.action]]\nkind = \"zzz\"\n"
    ));
    assert_eq!(expected(&action), names(Family::Action), "{action}");
    let quest = "[[quest]]
id = 1
name = \"Q\"
";
    let objective = quests_error(&format!(
        "{quest}[[quest.objective]]
kind = \"zzz\"
"
    ));
    assert_eq!(
        expected(&objective),
        names(Family::Objective),
        "{objective}"
    );
    let reward = quests_error(&format!(
        "{quest}[[quest.objective]]
kind = \"enter\"
level = 1
[[quest.reward]]
kind = \"zzz\"
"
    ));
    assert_eq!(expected(&reward), names(Family::Reward), "{reward}");
}

#[test]
fn every_target_the_readers_accept_is_registered_under_its_slot() {
    let slot = |slot: &str| -> BTreeSet<String> {
        family(Family::Target)
            .iter()
            .filter(|b| b.slot == slot)
            .map(|b| b.name.to_owned())
            .collect()
    };
    for (key, slot_name) in [("affects", "affects"), ("around", "around"), ("on", "on")] {
        let kind = if key == "around" { "area_hit" } else { "state" };
        let extra = if key == "around" {
            "radius = 1\ndamage = { min = 1, max = 2 }\n"
        } else {
            ""
        };
        let message = skills_error(&format!(
            "{SKILL_HEAD}kind = \"{kind}\"\n{}{extra}{key} = \"zzz\"\n",
            if kind == "state" { "state = 1\n" } else { "" }
        ));
        assert_eq!(expected(&message), slot(slot_name), "{key}: {message}");
    }
    let who = triggers_error(&format!(
        "{TRIGGER_HEAD}[[trigger.action]]\nkind = \"remove_state\"\nstate = 1\nwho = \"zzz\"\n"
    ));
    assert_eq!(expected(&who), slot("who"), "{who}");
}

#[test]
fn a_sample_built_from_each_block_reads_with_the_real_reader() {
    let mut checked = 0;
    for b in all().filter(|b| sample(b, true).is_some()) {
        for every_key in [false, true] {
            let text = sample(b, every_key).unwrap();
            read(file_of(b), &text).unwrap_or_else(|e| {
                panic!(
                    "{} `{}` (all keys: {every_key}): {e}\n{text}",
                    b.family.noun(),
                    b.name
                )
            });
        }
        checked += 1;
    }
    assert!(checked >= 25, "{checked}");
}

#[test]
fn a_required_key_is_required_and_no_key_is_missing_from_a_block() {
    for b in all().filter(|b| {
        matches!(
            b.family,
            Family::SkillPart
                | Family::Action
                | Family::Condition
                | Family::Objective
                | Family::Reward
        )
    }) {
        let file = file_of(b);
        let full = sample(b, true).unwrap();
        // Dropping a required key is refused.
        for f in b.fields.iter().filter(|f| f.required) {
            let line = format!("{} = {}\n", f.key, f.kind.sample());
            assert!(full.contains(&line), "{} {}: {line}", b.name, f.key);
            let without = full.replacen(&line, "", 1);
            assert!(
                read(file, &without).is_err(),
                "{} `{}` still reads without `{}`",
                b.family.noun(),
                b.name,
                f.key
            );
        }
        // A key the reader knows that the block does not list shows in the
        // reader's own list of keys.
        // The key goes first: the reader lists the keys it has not read yet.
        let marker = format!("kind = \"{}\"\n", b.name);
        assert!(full.contains(&marker), "{}", b.name);
        let unknown = full.replacen(&marker, &format!("{marker}A = 1\n"), 1);
        let message = read(file, &unknown).expect_err("an unknown key is refused");
        let mut known = expected(&message);
        known.remove("kind");
        let listed: BTreeSet<String> = b.fields.iter().map(|f| f.key.to_owned()).collect();
        assert_eq!(known, listed, "{} `{}`: {message}", b.family.noun(), b.name);
    }
}

#[test]
fn field_choices_are_the_registered_targets() {
    for b in all() {
        for f in b.fields {
            let FieldKind::Choice(options) = f.kind else {
                continue;
            };
            let slot = match f.key {
                "at" => "who",
                other => other,
            };
            let registered: BTreeSet<&str> = family(Family::Target)
                .iter()
                .filter(|t| t.slot == slot)
                .map(|t| t.name)
                .collect();
            if registered.is_empty() {
                continue;
            }
            let got: BTreeSet<&str> = options.iter().copied().collect();
            assert_eq!(
                got,
                registered,
                "{} `{}` key `{}`",
                b.family.noun(),
                b.name,
                f.key
            );
        }
    }
}

#[test]
fn the_tables_are_well_formed() {
    for b in all() {
        assert!(
            !b.name.is_empty() && !b.label.is_empty() && !b.about.is_empty(),
            "{b:?}"
        );
        assert!(b
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
        assert!(
            (1..=VOCABULARY_API).contains(&b.since),
            "{} since {}",
            b.name,
            b.since
        );
        assert_eq!(b.family == Family::Target, !b.slot.is_empty(), "{}", b.name);
        let keys: BTreeSet<&str> = b.fields.iter().map(|f| f.key).collect();
        assert_eq!(keys.len(), b.fields.len(), "{} repeats a key", b.name);
        if let Some(d) = b.deprecated {
            assert!(d.since >= b.since, "{}", b.name);
        }
    }
    for fam in Family::ALL {
        let mut seen = BTreeSet::new();
        for b in family(fam) {
            assert!(seen.insert((b.slot, b.name)), "{} listed twice", b.name);
            assert_eq!(b.family, fam, "{} is in the wrong table", b.name);
        }
        assert!(!family(fam).is_empty());
    }
    assert_eq!(
        all().count(),
        Family::ALL.iter().map(|f| family(*f).len()).sum::<usize>()
    );
}

#[test]
fn values_are_names_the_formula_compiler_accepts() {
    for v in family(Family::Value) {
        crate::rules::data_mod::Formula::compile(v.name)
            .unwrap_or_else(|e| panic!("value `{}`: {e}", v.name));
    }
}

#[test]
fn find_and_target_look_names_up() {
    assert_eq!(
        find(Family::SkillPart, "area_hit").unwrap().label,
        "Area hit"
    );
    assert!(find(Family::SkillPart, "area").is_none());
    assert_eq!(target("who", "other").unwrap().label, "The other unit");
    assert!(target("affects", "other").is_none());
}

#[test]
fn scan_lists_the_names_a_file_uses_in_file_order() {
    let text = "\
[[trigger]]
on = \"attack\"
while_skill = 1
[[trigger.condition]]
kind = \"has_state\"
state = 1
who = \"other\"
[[trigger.action]]
kind = \"cast_skill\"
skill = 2
at = \"self\"

[[trigger]]
on = \"kill\"
while_skill = 1
[[trigger.action]]
kind = \"remove_state\"
state = 1
";
    let uses: Vec<(Family, String)> = scan(FileKind::Triggers, text)
        .unwrap()
        .iter()
        .map(|u| (u.family, u.name.clone()))
        .collect();
    let want = [
        (Family::Event, "attack"),
        (Family::Condition, "has_state"),
        (Family::Target, "other"),
        (Family::Action, "cast_skill"),
        (Family::Target, "self"),
        (Family::Event, "kill"),
        (Family::Action, "remove_state"),
    ];
    assert_eq!(
        uses,
        want.iter()
            .map(|(f, n)| (*f, (*n).to_owned()))
            .collect::<Vec<_>>()
    );
    let at: Vec<String> = scan(FileKind::Triggers, text)
        .unwrap()
        .into_iter()
        .map(|u| u.at)
        .collect();
    assert_eq!(at[0], "trigger[0].on");
    assert_eq!(at[3], "trigger[0].action[0].kind");
    assert_eq!(at[5], "trigger[1].on");
}

#[test]
fn a_file_of_known_names_has_no_problem() {
    let text = format!(
        "{SKILL_HEAD}kind = \"area_hit\"\nradius = 4\naround = \"caster\"\naffects = \"everyone\"\ndamage = {{ min = 1, max = 2 }}\n"
    );
    assert_eq!(check(FileKind::Skills, &text, 1).unwrap(), vec![]);
}

#[test]
fn an_unknown_name_is_an_error_that_suggests_the_nearest_one() {
    let text = format!("{SKILL_HEAD}kind = \"aera_hit\"\n");
    let problems = check(FileKind::Skills, &text, 1).unwrap();
    assert_eq!(problems.len(), 1);
    let p = &problems[0];
    assert_eq!(
        (p.severity, p.at.as_str()),
        (Severity::Error, "skill[0].part[0].kind")
    );
    assert!(
        p.message.contains("unknown skill part `aera_hit`"),
        "{}",
        p.message
    );
    assert!(
        p.message.contains("did you mean `area_hit`?"),
        "{}",
        p.message
    );
    assert!(
        p.message.contains("missile"),
        "lists the known names: {}",
        p.message
    );

    let far = format!("{SKILL_HEAD}kind = \"explode_everything\"\n");
    let p = &check(FileKind::Skills, &far, 1).unwrap()[0];
    assert!(!p.message.contains("did you mean"), "{}", p.message);

    let target_text = format!("{SKILL_HEAD}kind = \"area_hit\"\naffects = \"enemy\"\n");
    let p = &check(FileKind::Skills, &target_text, 1).unwrap()[0];
    assert!(
        p.message.contains("unknown target `enemy`") && p.message.contains("`enemies`"),
        "{}",
        p.message
    );

    let event = "[[trigger]]\non = \"killed\"\nwhile_skill = 1\n";
    let p = &check(FileKind::Triggers, event, 1).unwrap()[0];
    assert!(p.message.contains("did you mean `kill`?"), "{}", p.message);
}

#[test]
fn a_block_newer_than_the_mods_api_is_an_error() {
    let newer = Block {
        since: 3,
        ..*find(Family::SkillPart, "missile").unwrap()
    };
    let u = Use {
        family: Family::SkillPart,
        slot: "",
        name: "missile".into(),
        at: "skill[0].part[0].kind".into(),
    };
    let problems = judge(&u, Some(&newer), 2);
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].severity, Severity::Error);
    assert!(problems[0].message.contains("API 3") && problems[0].message.contains("API 2"));
    assert!(judge(&u, Some(&newer), 3).is_empty());
}

#[test]
fn a_deprecated_block_is_a_warning_naming_what_to_use() {
    let old = Block {
        deprecated: Some(Deprecated {
            since: 2,
            use_instead: "area_hit",
            note: "It goes in a later API.",
        }),
        ..*find(Family::SkillPart, "missile").unwrap()
    };
    let u = Use {
        family: Family::SkillPart,
        slot: "",
        name: "missile".into(),
        at: "skill[0].part[0].kind".into(),
    };
    let problems = judge(&u, Some(&old), 2);
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].severity, Severity::Warning);
    assert!(
        problems[0].message.contains("use `area_hit` instead"),
        "{}",
        problems[0].message
    );
}

#[test]
fn a_text_that_is_not_toml_is_the_readers_error() {
    assert!(check(FileKind::Skills, "[[skill", 1).is_err());
}

#[test]
fn distance_counts_edits() {
    assert_eq!(distance("kill", "kill"), 0);
    assert_eq!(distance("killed", "kill"), 2);
    assert_eq!(distance("aera_hit", "area_hit"), 2);
    assert_eq!(distance("", "abc"), 3);
}

#[test]
fn registered_call_block_accepts_graphs_and_keeps_luau_compatible() {
    for module in ["main.luau", "combat/main.graph.toml"] {
        let fields =
            format!("kind='call_script'\nmodule='{module}'\nfunction='run'\nargument='lvl'");
        for (kind, source) in [
            (FileKind::Skills, format!("{SKILL_HEAD}{fields}")),
            (
                FileKind::Triggers,
                format!("{TRIGGER_HEAD}[[trigger.action]]\n{fields}"),
            ),
        ] {
            read(kind, &source).unwrap();
            assert!(check(kind, &source, 1).unwrap().is_empty());
        }
    }
    for module in [
        "../main.graph.toml",
        "main.toml",
        "/main.graph.toml",
        "a\\b.graph.toml",
    ] {
        let spec = crate::rules::scripts::CallSpec {
            module: module.into(),
            function: "run".into(),
            argument: crate::rules::data_mod::Formula::constant(0),
        };
        assert!(spec.check().is_err(), "{module}");
    }
}

#[test]
fn samples_use_declared_flag_defaults_for_exclusive_modes() {
    let block = family(Family::SkillPart)
        .iter()
        .find(|b| b.name == "ai_curse")
        .unwrap();
    let text = sample(block, true).unwrap();
    assert!(text.contains("attract = false"));
    let rules = SkillRules::from_toml(&text).unwrap();
    let crate::rules::data_mod::Part::AiCurse(part) = &rules.get(1).unwrap().parts[0] else {
        panic!("AI curse");
    };
    assert!(!part.attract);
    assert_eq!(part.stats.len(), 1);
    assert!(SkillRules::from_toml(&text.replace("attract = false", "attract = true")).is_err());
}
