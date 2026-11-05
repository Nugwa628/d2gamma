//! `triggers.toml`: reading, refusing and checking against the states.

use super::*;
use crate::rules::data_mod::StateRules;

const STATES: &str =
    "[[state]]\nid = 380\nduration_seconds = 4\nmax_stacks = 3\nreapply = \"add_stack\"\n\
                      [[state]]\nid = 381\nduration_seconds = 1\n";

const PROC: &str =
    "[[trigger]]\nname = \"proc\"\non = \"attack\"\nwhile_skill = 391\nchance = \"4 + lvl\"\n\
                    [[trigger.condition]]\nkind = \"lacks_state\"\nstate = 381\n\
                    [[trigger.action]]\nkind = \"apply_state\"\nstate = 381\n";

fn err(text: &str) -> String {
    TriggerRules::from_toml(text).unwrap_err().to_string()
}

#[test]
fn the_file_reads_into_triggers() {
    let r = TriggerRules::from_toml(PROC).unwrap();
    let [d] = r.defs() else { panic!("one trigger") };
    assert_eq!(d.name, "proc");
    assert_eq!(d.on, On::Attack);
    assert_eq!(d.while_skill, Some(391));
    assert_eq!(
        d.conditions,
        [Condition::LacksState {
            state: 381,
            who: Who::Holder
        }]
    );
    assert!(matches!(
        d.actions[0],
        Action::ApplyState { state: 381, .. }
    ));
}

#[test]
fn an_empty_file_has_no_triggers_and_a_version_is_checked() {
    assert!(TriggerRules::from_toml("").unwrap().is_empty());
    assert!(err("version = 2").contains("`version`: 2 is not supported"));
}

#[test]
fn a_trigger_needs_exactly_one_source() {
    let none =
        "[[trigger]]\non = \"kill\"\n[[trigger.action]]\nkind = \"remove_state\"\nstate = 1\n";
    assert!(err(none).contains("exactly one source"), "{}", err(none));
    let two = format!("{none}while_skill = 1\nwhile_stat = 2\n").replace(
        "[[trigger.action]]\nkind = \"remove_state\"\nstate = 1\nwhile_skill = 1\nwhile_stat = 2\n",
        "while_skill = 1\nwhile_stat = 2\n[[trigger.action]]\nkind = \"remove_state\"\nstate = 1\n",
    );
    assert!(err(&two).contains("exactly one source"), "{}", err(&two));
}

#[test]
fn state_events_name_their_state_and_others_do_not() {
    let a = "[[trigger.action]]\nkind = \"remove_state\"\nstate = 1\n";
    let start = format!("[[trigger]]\non = \"state_start\"\nwhile_skill = 1\n{a}");
    assert!(err(&start).contains("names its `state`"));
    let hit = format!("[[trigger]]\non = \"hit\"\nwhile_skill = 1\nstate = 380\n{a}");
    assert!(err(&hit).contains("only state_start / state_end"));
    let skills = format!("[[trigger]]\non = \"hit\"\nwhile_skill = 1\nskills = [2]\n{a}");
    assert!(err(&skills).contains("only skill_used triggers filter"));
}

#[test]
fn an_error_names_the_trigger_and_the_key() {
    let bad = "[[trigger]]\non = \"attack\"\nwhile_skill = 1\nchance = \"lvl *\"\n\
               [[trigger.action]]\nkind = \"remove_state\"\nstate = 1\n";
    let e = err(bad);
    assert!(e.contains("`trigger[0]`") && e.contains("lvl *"), "{e}");
    let unknown = "[[trigger]]\non = \"attack\"\nwhile_skill = 1\nchanse = 5\n";
    assert!(err(unknown).contains("`trigger[0]`"), "{}", err(unknown));
    let kind =
        "[[trigger]]\non = \"attack\"\nwhile_skill = 1\n[[trigger.action]]\nkind = \"explode\"\n";
    assert!(err(kind).contains("`trigger[0]`"), "{}", err(kind));
}

#[test]
fn a_trigger_is_bounded() {
    let none = "[[trigger]]\non = \"attack\"\nwhile_skill = 1\n";
    assert!(err(none).contains("0 actions"), "{}", err(none));
    let action = "[[trigger.action]]\nkind = \"remove_state\"\nstate = 1\n";
    let many = format!(
        "[[trigger]]\non = \"attack\"\nwhile_skill = 1\n{}",
        action.repeat(MAX_ACTIONS + 1)
    );
    assert!(err(&many).contains("actions"), "{}", err(&many));
    let cond = "[[trigger.condition]]\nkind = \"has_state\"\nstate = 1\n";
    let conds = format!(
        "[[trigger]]\non = \"attack\"\nwhile_skill = 1\n{}{action}",
        cond.repeat(MAX_CONDITIONS + 1)
    );
    assert!(err(&conds).contains("conditions"), "{}", err(&conds));
    let life = "[[trigger]]\non = \"attack\"\nwhile_skill = 1\n[[trigger.condition]]\nkind = \"life_below\"\npercent = 120\n";
    assert!(err(&format!("{life}{action}")).contains("more than 100"));
    let all =
        format!("[[trigger]]\non = \"attack\"\nwhile_skill = 1\n{action}").repeat(MAX_TRIGGERS + 1);
    assert!(err(&all).contains("at most"), "{}", err(&all));
}

#[test]
fn states_a_trigger_uses_are_checked_and_watched() {
    let mut states = StateRules::from_toml(STATES).unwrap();
    let r = TriggerRules::from_toml(PROC).unwrap();
    r.check_states(&mut states).unwrap();
    // A trigger that only applies a state does not watch it.
    assert!(!states.get(381).unwrap().watched);

    let end = "[[trigger]]\non = \"state_end\"\nstate = 381\nwhile_skill = 1\n\
               [[trigger.action]]\nkind = \"remove_stack\"\nstate = 380\n";
    let r = TriggerRules::from_toml(end).unwrap();
    r.check_states(&mut states).unwrap();
    assert!(states.get(381).unwrap().watched);
    assert!(!states.get(380).unwrap().watched);
}

#[test]
fn a_state_without_rules_cannot_be_watched_or_stacked() {
    let mut states = StateRules::from_toml(STATES).unwrap();
    let watch = "[[trigger]]\non = \"state_end\"\nstate = 999\nwhile_skill = 1\n\
                 [[trigger.action]]\nkind = \"remove_state\"\nstate = 380\n";
    let e = TriggerRules::from_toml(watch)
        .unwrap()
        .check_states(&mut states)
        .unwrap_err()
        .to_string();
    assert!(e.contains("`trigger[0].state`") && e.contains("999"), "{e}");
    let stack = "[[trigger]]\non = \"kill\"\nwhile_skill = 1\n\
                 [[trigger.action]]\nkind = \"add_stack\"\nstate = 999\nseconds = 2\n";
    let e = TriggerRules::from_toml(stack)
        .unwrap()
        .check_states(&mut states)
        .unwrap_err()
        .to_string();
    assert!(e.contains("action[0].state"), "{e}");
    let count = "[[trigger]]\non = \"kill\"\nwhile_skill = 1\n[[trigger.condition]]\nkind = \"stacks_at_least\"\n\
                 state = 999\ncount = 2\n[[trigger.action]]\nkind = \"remove_state\"\nstate = 380\n";
    let e = TriggerRules::from_toml(count)
        .unwrap()
        .check_states(&mut states)
        .unwrap_err()
        .to_string();
    assert!(e.contains("condition[0].state"), "{e}");
}

#[test]
fn an_action_that_turns_a_state_on_needs_a_time() {
    let mut states = StateRules::from_toml("[[state]]\nid = 50\nmax_stacks = 2\n").unwrap();
    let t = "[[trigger]]\non = \"kill\"\nwhile_skill = 1\n[[trigger.action]]\nkind = \"apply_state\"\nstate = 50\n";
    let e = TriggerRules::from_toml(t)
        .unwrap()
        .check_states(&mut states)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("action[0].seconds") && e.contains("never end"),
        "{e}"
    );
    let ok = format!("{t}seconds = 3\n");
    TriggerRules::from_toml(&ok)
        .unwrap()
        .check_states(&mut states)
        .unwrap();
}

#[test]
fn by_skill_filters_attack_hit_and_kill_only() {
    let a = "[[trigger.action]]\nkind = \"remove_state\"\nstate = 1\n";
    for on in ["attack", "hit", "kill"] {
        let t = format!("[[trigger]]\non = \"{on}\"\nwhile_skill = 1\nby_skill = 5\n{a}");
        let r = TriggerRules::from_toml(&t).unwrap();
        assert_eq!(r.defs()[0].by_skill, Some(5), "{on}");
    }
    for on in ["being_hit", "skill_used"] {
        let t = format!("[[trigger]]\non = \"{on}\"\nwhile_skill = 1\nby_skill = 5\n{a}");
        assert!(err(&t).contains("by_skill"), "{on}: {}", err(&t));
    }
    let plain = format!("[[trigger]]\non = \"hit\"\nwhile_skill = 1\n{a}");
    assert_eq!(
        TriggerRules::from_toml(&plain).unwrap().defs()[0].by_skill,
        None
    );
}
