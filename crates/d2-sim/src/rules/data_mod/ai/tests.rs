//! Reading `ai.toml`: what a good file gives and what a bad one is told.

use super::*;
use crate::monsters::provider::MonsterProvider;

const GOOD: &str = r#"
version = 1

[[ai]]
index = 200
name = "Stalker"
target = "any"
start = "stalk"

[[ai.cooldown]]
name = "leap"
seconds = 6

[[ai.cooldown]]
name = "roar"
seconds = 1.5

[[ai.state]]
name = "stalk"

[[ai.state.transition]]
to = "strike"
when = [{ kind = "distance_below", value = 3 }, { kind = "line_of_sight" }]

[[ai.state.action]]
kind = "use_skill"
slot = 2
when = [{ kind = "cooldown_ready", name = "leap" }]
starts_cooldown = "leap"

[[ai.state.action]]
kind = "move_to"
run = true

[[ai.state]]
name = "strike"

[[ai.state.transition]]
to = "stalk"
when = [{ kind = "distance_above", value = 8 }]

[[ai.state.action]]
kind = "wind_up"
seconds = 0.5
event = "slam"

[[ai.state.action]]
kind = "attack"
attack = 2
"#;

fn err(text: &str) -> String {
    AiRules::from_toml(text).unwrap_err().to_string()
}

/// One graph with one state whose only action is `action` (a TOML table body).
fn with_action(action: &str) -> String {
    format!("[[ai]]\nindex = 1\n[[ai.state]]\nname = \"a\"\n[[ai.state.action]]\n{action}\n")
}

#[test]
fn a_good_file_is_read_and_resolved() {
    let rules = AiRules::from_toml(GOOD).unwrap();
    let d = rules.get(200).unwrap();
    assert_eq!(d.name, "Stalker");
    assert_eq!(d.target, Some(Targeting::Any));
    assert_eq!(d.start_state, 0);
    assert_eq!(d.states.len(), 2);
    // Seconds are kept as base ticks (25 a second).
    assert_eq!(d.cooldowns[0].ticks, 150);
    assert_eq!(d.cooldowns[1].ticks, 38);
    let stalk = &d.states[0];
    assert_eq!(stalk.transitions[0].target, 1);
    assert_eq!(stalk.actions[0].cooldown, Some(0));
    assert_eq!(
        stalk.actions[0].when,
        [Condition::CooldownReady {
            name: "leap".to_owned(),
            index: 0
        }]
    );
    let strike = &d.states[1];
    assert_eq!(strike.actions[0].ticks, Some(13));
    assert_eq!(strike.actions[0].event_index, Some(0));
    assert_eq!(d.events, ["slam"]);
    assert_eq!(rules.state_name(200, 1), Some("strike"));
    assert_eq!(rules.event_name(200, 0), Some("slam"));
    assert_eq!(rules.state_name(7, 0), None);
}

#[test]
fn an_empty_file_has_no_graphs() {
    let rules = AiRules::from_toml("version = 1").unwrap();
    assert!(rules.is_empty());
    assert!(rules.get(200).is_none());
}

#[test]
fn the_start_state_defaults_to_the_first() {
    let rules = AiRules::from_toml(&with_action("kind = \"idle\"")).unwrap();
    assert_eq!(rules.get(1).unwrap().start_state, 0);
    assert_eq!(rules.get(1).unwrap().target_or_default(), Targeting::Enemy);
}

#[test]
fn the_think_budget_is_the_sum_of_the_limits() {
    assert_eq!(THINK_BUDGET, 4 * 6 + 4 * 8 * 6 + 8 * 7);
}

#[test]
fn a_wrong_version_is_refused() {
    assert!(err("version = 2").contains("`version`: 2 is not supported"));
}

#[test]
fn an_unknown_key_is_refused() {
    assert!(err("[[ai]]\nindex = 1\nspeed = 3\n[[ai.state]]\nname = \"a\"").contains("speed"));
    assert!(err(&with_action("kind = \"idle\"\nspeed = 3")).contains("speed"));
}

#[test]
fn a_graph_needs_a_state_and_a_name_for_each() {
    assert!(err("[[ai]]\nindex = 1\nstate = []").contains("at least one state"));
    let two = "[[ai]]\nindex = 1\n[[ai.state]]\nname = \"a\"\n[[ai.state]]\nname = \"a\"";
    assert!(err(two).contains("`ai[0].state[1].name`"));
}

#[test]
fn two_graphs_cannot_share_an_index() {
    let one = "[[ai]]\nindex = 5\n[[ai.state]]\nname = \"a\"\n";
    assert!(err(&format!("{one}{one}")).contains("`ai[1].index`: 5 is used by another graph"));
}

#[test]
fn names_must_exist() {
    let start = "[[ai]]\nindex = 1\nstart = \"nope\"\n[[ai.state]]\nname = \"a\"";
    assert!(err(start).contains("`ai[0].start`: there is no state `nope`"));
    let to = "[[ai]]\nindex = 1\n[[ai.state]]\nname = \"a\"\n[[ai.state.transition]]\nto = \"b\"";
    assert!(err(to).contains("`ai[0].state[0].transition[0].to`"));
    let cooldown = with_action("kind = \"idle\"\nstarts_cooldown = \"x\"");
    assert!(err(&cooldown).contains("`ai[0].state[0].action[0].starts_cooldown`"));
    let cond = with_action("kind = \"idle\"\nwhen = [{ kind = \"cooldown_ready\", name = \"x\" }]");
    assert!(err(&cond).contains("`ai[0].state[0].action[0].when[0].name`"));
}

#[test]
fn a_transition_cannot_lead_nowhere() {
    let own = "[[ai]]\nindex = 1\n[[ai.state]]\nname = \"a\"\n[[ai.state.transition]]\nto = \"a\"";
    assert!(err(own).contains("its own state"));
}

#[test]
fn an_action_reads_only_its_own_keys() {
    assert!(err(&with_action("kind = \"attack\"\nsteps = 3")).contains("`steps` does not apply"));
    assert!(err(&with_action("kind = \"move_to\"\nslot = 1")).contains("`slot` does not apply"));
    assert!(err(&with_action("kind = \"idle\"\non = true")).contains("`on` does not apply"));
}

#[test]
fn an_action_needs_its_required_keys() {
    let e = err(&with_action("kind = \"use_skill\""));
    assert!(e.contains("`ai[0].state[0].action[0].slot`"), "{e}");
    assert!(err(&with_action("kind = \"wind_up\"")).contains("needs `seconds`"));
    assert!(err(&with_action("kind = \"summon\"")).contains("needs `class`"));
    assert!(err(&with_action("kind = \"set_state\"")).contains("needs `state`"));
}

#[test]
fn values_are_bounded() {
    assert!(err(&with_action("kind = \"use_skill\"\nslot = 9")).contains("1 to 8"));
    assert!(err(&with_action("kind = \"attack\"\nattack = 3")).contains("is 1 or 2"));
    assert!(err(&with_action("kind = \"flee\"\nsteps = 0")).contains("1 to 40"));
    assert!(err(&with_action("kind = \"summon\"\nclass = 4\ncount = 5")).contains("1 to 4"));
    assert!(err(&with_action("kind = \"idle\"\nseconds = 601")).contains("out of range"));
    assert!(err(&with_action("kind = \"idle\"\nseconds = -1")).contains("out of range"));
    let pct = with_action("kind = \"idle\"\nwhen = [{ kind = \"random\", percent = 101 }]");
    assert!(err(&pct).contains("0 to 100"));
    let far = with_action("kind = \"idle\"\nwhen = [{ kind = \"distance_below\", value = 999 }]");
    assert!(err(&far).contains("subtiles"));
    let radius = with_action("kind = \"idle\"\nwhen = [{ kind = \"allies_within\", radius = 0 }]");
    assert!(err(&radius).contains("`ai[0].state[0].action[0].when[0].radius`"));
}

#[test]
fn the_limits_are_enforced() {
    let states = |n: usize| {
        let mut s = String::from("[[ai]]\nindex = 1\n");
        for i in 0..n {
            s.push_str(&format!("[[ai.state]]\nname = \"s{i}\"\n"));
        }
        s
    };
    assert!(AiRules::from_toml(&states(MAX_STATES)).is_ok());
    assert!(err(&states(MAX_STATES + 1)).contains("at most 24"));
    let mut many = String::new();
    for i in 0..=MAX_AIS {
        many.push_str(&format!(
            "[[ai]]\nindex = {i}\n[[ai.state]]\nname = \"a\"\n"
        ));
    }
    assert!(err(&many).contains("at most 32"));
    let acts = format!(
        "[[ai]]\nindex = 1\n[[ai.state]]\nname = \"a\"\n{}",
        "[[ai.state.action]]\nkind = \"idle\"\n".repeat(MAX_ACTIONS + 1)
    );
    assert!(err(&acts).contains("at most 8"));
    let conds = with_action(&format!(
        "kind = \"idle\"\nwhen = [{}]",
        ["{ kind = \"has_target\" }"; MAX_CONDITIONS + 1].join(", ")
    ));
    assert!(err(&conds).contains("at most 6"));
    let events = (0..=MAX_EVENTS)
        .map(|i| {
            format!("[[ai.state.action]]\nkind = \"wind_up\"\nseconds = 1\nevent = \"e{i}\"\n")
        })
        .collect::<String>();
    let many_events = format!("[[ai]]\nindex = 1\n[[ai.state]]\nname = \"a\"\n{events}");
    // Nine actions are one over the action limit; the event limit is the
    // next check, so spread them over two states.
    assert!(err(&many_events).contains("at most 8"));
    let two = format!(
        "[[ai]]\nindex = 1\n[[ai.state]]\nname = \"a\"\n{}[[ai.state]]\nname = \"b\"\n{}",
        (0..MAX_EVENTS)
            .map(|i| format!(
                "[[ai.state.action]]\nkind = \"wind_up\"\nseconds = 1\nevent = \"e{i}\"\n"
            ))
            .collect::<String>(),
        "[[ai.state.action]]\nkind = \"wind_up\"\nseconds = 1\nevent = \"extra\"\n",
    );
    assert!(err(&two).contains("different event names"));
}

#[test]
fn a_cooldown_list_is_bounded() {
    let mut s = String::from("[[ai]]\nindex = 1\n[[ai.state]]\nname = \"a\"\n");
    for i in 0..=MAX_COOLDOWNS {
        s.push_str(&format!("[[ai.cooldown]]\nname = \"c{i}\"\nseconds = 1\n"));
    }
    assert!(err(&s).contains("at most 8"));
}

fn preset(body: &str) -> String {
    format!("[[ai]]\nindex = 201\n{body}")
}

#[test]
fn a_preset_entry_is_read_with_its_parameters() {
    let rules = AiRules::from_toml(&preset(
        "preset = \"zombie\"\n[ai.params]\napproach = 40\naware_dist = [10, 12, 14]\naip4 = 20\n",
    ))
    .unwrap();
    let d = rules.get(201).unwrap();
    assert!(d.is_preset());
    assert_eq!(d.preset_row, Some(3));
    assert!(d.states.is_empty());
    // One number is for every difficulty, a list is Normal, Nightmare, Hell.
    assert_eq!((d.param(1, 0), d.param(1, 2)), (Some(40), Some(40)));
    assert_eq!(
        [d.param(2, 0), d.param(2, 1), d.param(2, 2)],
        [Some(10), Some(12), Some(14)]
    );
    assert_eq!(d.param(4, 1), Some(20));
    // What it does not set stays with the monstats columns.
    assert_eq!(
        (d.param(3, 0), d.param(8, 0), d.param(0, 0)),
        (None, None, None)
    );
    assert_eq!(MonsterProvider::aip(&rules, 201, 1, 0), Some(40));
    assert_eq!(MonsterProvider::aip(&rules, 3, 1, 0), None);
}

#[test]
fn a_preset_without_parameters_is_just_that_ai() {
    let rules = AiRules::from_toml(&preset("preset = \"Skeleton\"")).unwrap();
    let d = rules.get(201).unwrap();
    assert_eq!(d.preset_row, Some(2));
    assert!(d.knobs.iter().all(Option::is_none));
}

#[test]
fn a_preset_entry_is_refused_when_it_does_not_add_up() {
    let e = err(&preset("preset = \"Nope\""));
    assert!(
        e.contains("`ai[0].preset`: there is no AI preset called `Nope`"),
        "{e}"
    );
    let e = err(&preset("preset = \"Zombie\"\n[ai.params]\nspeed = 3"));
    assert!(e.contains("`ai[0].params.speed`"), "{e}");
    assert!(e.contains("approach (aip1)"), "{e}");
    let e = err(&preset("preset = \"Zombie\"\n[ai.params]\nap = 70000"));
    assert!(e.contains("`ai[0].params.ap`"), "{e}");
    let e = err(&preset(
        "preset = \"Zombie\"\n[ai.params]\napproach = 70000",
    ));
    assert!(e.contains("out of range"), "{e}");
    let e = err(&preset(
        "preset = \"Zombie\"\n[ai.params]\napproach = [1, 2, 99999]",
    ));
    assert!(e.contains("out of range"), "{e}");
    let e = err(&preset(
        "preset = \"Zombie\"\n[ai.params]\napproach = 1\naip1 = 2",
    ));
    assert!(e.contains("set twice"), "{e}");
    let e = err(&preset(
        "preset = \"Zombie\"\n[ai.params]\napproach = [1, 2]",
    ));
    assert!(e.contains("`ai[0]`"), "{e}");
    let e = err(&preset("preset = \"Zombie\"\n[[ai.state]]\nname = \"a\""));
    assert!(e.contains("`ai[0].state`"), "{e}");
    let e = err(&preset("preset = \"Zombie\"\ntarget = \"any\""));
    assert!(e.contains("`ai[0].target`"), "{e}");
    let e = err(&preset("preset = \"Zombie\"\nstart = \"a\""));
    assert!(e.contains("`ai[0].start`"), "{e}");
    let e = err(&preset(
        "preset = \"Zombie\"\n[[ai.cooldown]]\nname = \"a\"\nseconds = 1",
    ));
    assert!(e.contains("`ai[0].cooldown`"), "{e}");
}

#[test]
fn a_graph_has_no_parameters() {
    let e =
        err(&with_action("kind = \"idle\"").replace("index = 1", "index = 1\n[ai.params]\nx = 1"));
    assert!(e.contains("`ai[0].params.x`"), "{e}");
}

#[test]
fn presets_and_graphs_share_one_file_and_one_index_space() {
    let both = format!(
        "{}{}",
        preset("preset = \"Zombie\"\n"),
        "[[ai]]\nindex = 202\n[[ai.state]]\nname = \"a\"\n"
    );
    let rules = AiRules::from_toml(&both).unwrap();
    assert!(rules.get(201).unwrap().is_preset());
    assert!(!rules.get(202).unwrap().is_preset());
    let twice = format!("{0}{0}", preset("preset = \"Zombie\"\n"));
    assert!(err(&twice).contains("`ai[1].index`: 201 is used by another graph"));
}

#[test]
fn the_docs_name_every_preset_and_knob() {
    let docs = include_str!("../../../../../../docs/modkit/ai.md");
    for p in presets() {
        assert!(
            docs.contains(&format!("`{}`", p.name)),
            "{} is not in docs/modkit/ai.md",
            p.name
        );
        for k in &p.knobs {
            assert!(
                docs.contains(&k.name),
                "{}: {} is not in docs/modkit/ai.md",
                p.name,
                k.name
            );
        }
    }
}

const PHASED: &str = r#"
[[ai]]
index = 1
[[ai.phase]]
name = "enrage"
when = [{ kind = "life_below", percent = 50 }, { kind = "alive_for", seconds = 1.5 }]
state = "rage"
immune_seconds = 2.5
[[ai.phase]]
name = "rout"
graph = 2
[[ai.state]]
name = "calm"
[[ai.state]]
name = "rage"

[[ai]]
index = 2
[[ai.state]]
name = "flee"
"#;

#[test]
fn phases_are_read_and_resolved() {
    let rules = AiRules::from_toml(PHASED).unwrap();
    let d = rules.get(1).unwrap();
    assert_eq!(d.phases.len(), 2);
    assert_eq!(d.phases[0].state_index, Some(1));
    // Seconds are kept as base ticks.
    assert_eq!(d.phases[0].immune_ticks, Some(63));
    assert_eq!(d.phases[0].when[1], Condition::AliveFor { ticks: 38 });
    assert_eq!(d.phases[1].graph_position, Some(1));
    assert_eq!(rules.phase_name(1, 1), Some("rout"));
    assert_eq!(rules.wants_immunity(), Some((0, 0)));
    let calm = AiRules::from_toml(&with_action("kind = \"idle\"")).unwrap();
    assert_eq!(calm.wants_immunity(), None);
}

fn phased(body: &str) -> String {
    format!("[[ai]]\nindex = 1\n[[ai.phase]]\n{body}\n[[ai.state]]\nname = \"a\"\n")
}

#[test]
fn a_bad_phase_is_told_where() {
    let e = err(&phased("name = \"x\"\nstate = \"nope\""));
    assert!(e.contains("`ai[0].phase[0].state`"), "{e}");
    let e = err(&phased("name = \"x\"\ngraph = 9"));
    assert!(e.contains("`ai[0].phase[0].graph`"), "{e}");
    let e = err(&phased("name = \"x\"\ngraph = 1"));
    assert!(e.contains("the graph itself"), "{e}");
    let e = err(&phased("name = \"x\"\nstate = \"a\"\ngraph = 2"));
    assert!(e.contains("not both"), "{e}");
    let e = err(&phased("name = \"\""));
    assert!(e.contains("`ai[0].phase[0].name`"), "{e}");
    let e = err(&phased(
        "name = \"x\"\nwhen = [{ kind = \"cooldown_ready\", name = \"c\" }]",
    ));
    assert!(e.contains("`ai[0].phase[0].when[0].name`"), "{e}");
    let e = err(&phased("name = \"x\"\nimmune_seconds = 700"));
    assert!(e.contains("out of range"), "{e}");
    let e = err(&phased("name = \"x\"\nspeed = 3"));
    assert!(e.contains("speed"), "{e}");
}

#[test]
fn a_switch_must_lead_to_a_graph_not_a_preset() {
    let text = format!(
        "{}[[ai]]\nindex = 2\npreset = \"Zombie\"\n",
        phased("name = \"x\"\ngraph = 2")
    );
    let e = err(&text);
    assert!(e.contains("`ai[0].phase[0].graph`"), "{e}");
}

#[test]
fn phase_names_and_counts_are_checked() {
    let two = "[[ai]]\nindex = 1\n[[ai.phase]]\nname = \"x\"\n[[ai.phase]]\nname = \"x\"\n[[ai.state]]\nname = \"a\"\n";
    assert!(err(two).contains("`ai[0].phase[1].name`"));
    let mut many = String::from("[[ai]]\nindex = 1\n[[ai.state]]\nname = \"a\"\n");
    for n in 0..=MAX_PHASES {
        many.push_str(&format!("[[ai.phase]]\nname = \"p{n}\"\n"));
    }
    assert!(err(&many).contains("at most 8"));
    // A preset has no phases.
    let preset = "[[ai]]\nindex = 1\npreset = \"Zombie\"\n[[ai.phase]]\nname = \"x\"\n";
    assert!(err(preset).contains("`ai[0].phase`"));
}

fn with_condition(condition: &str) -> String {
    format!(
        "[[ai]]\nindex = 1\n[[ai.state]]\nname = \"a\"\n[[ai.state.transition]]\nto = \"b\"\nwhen = [{condition}]\n[[ai.state]]\nname = \"b\"\n"
    )
}

#[test]
fn a_head_count_of_none_is_refused() {
    for kind in ["allies_within", "enemies_within"] {
        let e = err(&with_condition(&format!(
            "{{ kind = \"{kind}\", radius = 5, at_least = 0 }}"
        )));
        assert!(
            e.contains("`ai[0].state[0].transition[0].when[0].at_least`"),
            "{e}"
        );
        assert!(AiRules::from_toml(&with_condition(&format!(
            "{{ kind = \"{kind}\", radius = 5, at_least = 1 }}"
        )))
        .is_ok());
    }
    let e = err(&with_condition(
        "{ kind = \"stacks_at_least\", state = 3, count = 0 }",
    ));
    assert!(
        e.contains("`ai[0].state[0].transition[0].when[0].count`"),
        "{e}"
    );
}

#[test]
fn a_move_to_may_stop_short_and_change_its_speed() {
    let rules = AiRules::from_toml(&with_action(
        "kind = \"move_to\"\nsteps = 9\nspeed = 10\nrun = true",
    ))
    .unwrap();
    let a = &rules.get(1).unwrap().states[0].actions[0];
    assert_eq!((a.steps, a.speed, a.run), (Some(9), Some(10), Some(true)));
    // Other kinds do not read `speed`; the value is bounded and is not 0.
    assert!(err(&with_action("kind = \"attack\"\nspeed = 10")).contains("`speed` does not apply"));
    assert!(err(&with_action("kind = \"move_to\"\nspeed = 0")).contains("not 0"));
    assert!(err(&with_action("kind = \"move_to\"\nspeed = 101")).contains("-100 to 100"));
    assert!(err(&with_action("kind = \"move_to\"\nspeed = -101")).contains("-100 to 100"));
    assert!(err(&with_action("kind = \"move_to\"\nsteps = 0")).contains("1 to"));
}

#[test]
fn a_hop_and_an_aim_have_their_own_keys() {
    let rules = AiRules::from_toml(&with_action(
        "kind = \"hop_to\"
steps = 9
keep = 4
speed = 10
at = \"second\"",
    ))
    .unwrap();
    let a = &rules.get(1).unwrap().states[0].actions[0];
    assert_eq!(
        (a.steps, a.keep, a.speed, a.at),
        (Some(9), Some(4), Some(10), Some(Whose::Second))
    );
    assert!(err(&with_action(
        "kind = \"hop_to\"
run = true"
    ))
    .contains("`run` does not apply"));
    assert!(err(&with_action(
        "kind = \"move_to\"
keep = 1"
    ))
    .contains("`keep` does not apply"));
    assert!(err(&with_action(
        "kind = \"hop_to\"
keep = 201"
    ))
    .contains("0 to 200"));
    assert!(err(&with_action(
        "kind = \"attack\"
at = \"self\""
    ))
    .contains("`self` does not apply"));
    assert!(AiRules::from_toml(&with_action(
        "kind = \"attack\"
at = \"second\""
    ))
    .is_ok());
    let far = with_condition("{ kind = \"second_distance_below\", value = 999 }");
    assert!(err(&far).contains("subtiles"));
}

#[test]
fn scans_counts_and_thinks_have_bounded_values() {
    for bad in [
        "{ kind = \"undead_within\", radius = 0 }",
        "{ kind = \"undead_within\", radius = 5, at_least = 0 }",
        "{ kind = \"corpse_within\", radius = 201 }",
        "{ kind = \"hurt_within\", radius = 0 }",
        "{ kind = \"class_count_below\", class = 1, count = 0 }",
        "{ kind = \"class_count_at_least\", class = 1, count = 0 }",
        "{ kind = \"thinks_at_least\", count = 0 }",
    ] {
        assert!(AiRules::from_toml(&with_condition(bad)).is_err(), "{bad}");
    }
    for good in [
        "{ kind = \"undead_within\", radius = 24 }",
        "{ kind = \"corpse_within\", radius = 24 }",
        "{ kind = \"hurt_within\", radius = 24 }",
        "{ kind = \"class_count_below\", class = 453, also = 461, count = 25 }",
        "{ kind = \"class_count_at_least\", class = 453, count = 1 }",
        "{ kind = \"thinks_at_least\", count = 18 }",
    ] {
        assert!(AiRules::from_toml(&with_condition(good)).is_ok(), "{good}");
    }
    let a = AiRules::from_toml(&with_action(
        "kind = \"use_skill\"\nslot = 1\nat = \"corpse\"\nradius = 12",
    ))
    .unwrap();
    let a = &a.get(1).unwrap().states[0].actions[0];
    assert_eq!((a.at, a.radius), (Some(Whose::Corpse), Some(12)));
    assert!(AiRules::from_toml(&with_action("kind = \"attack\"\nat = \"hurt\"")).is_ok());
}

#[test]
fn a_follow_has_its_own_keys() {
    let rules = AiRules::from_toml(&with_action(
        "kind = \"follow_owner\"
steps = 8
quiet = true
at = \"owner_target\"",
    ))
    .unwrap();
    let a = &rules.get(1).unwrap().states[0].actions[0];
    assert_eq!(
        (a.steps, a.quiet, a.at),
        (Some(8), Some(true), Some(Whose::OwnerTarget))
    );
    assert!(err(&with_action(
        "kind = \"move_to\"
quiet = true"
    ))
    .contains("`quiet` does not apply"));
    assert!(err(&with_action(
        "kind = \"follow_owner\"
run = true"
    ))
    .contains("`run` does not apply"));
    assert!(err(&with_action(
        "kind = \"follow_owner\"
at = \"self\""
    ))
    .contains("`self` does not apply"));
    for good in [
        "{ kind = \"has_owner\" }",
        "{ kind = \"no_owner\" }",
        "{ kind = \"owner_target\" }",
        "{ kind = \"owner_distance_below\", value = 8 }",
        "{ kind = \"owner_distance_above\", value = 12 }",
    ] {
        assert!(AiRules::from_toml(&with_condition(good)).is_ok(), "{good}");
    }
    let far = with_condition("{ kind = \"owner_distance_above\", value = 999 }");
    assert!(err(&far).contains("subtiles"));
}
