//! The rules of `states.toml`: what the file says, what it refuses, and the
//! stack arithmetic its policies give.

use super::*;

const FILE: &str = r#"
version = 1

[[state]]
id = 380
name = "Smolder"
duration_seconds = 8
max_stacks = 5
reapply = "add_stack"
decay = "lose_one"
icon = "smolder"
overlay = 17

[[state.effect]]
stat = 25
per_stack = 5

[[state.effect]]
stat = 31
per_stack = -2

[[state]]
id = 381
"#;

#[test]
fn the_file_reads_into_definitions() {
    let rules = StateRules::from_toml(FILE).unwrap();
    assert_eq!(rules.defs().len(), 2);
    let d = rules.get(380).unwrap();
    assert_eq!(d.name, "Smolder");
    assert_eq!(d.duration_base_ticks(), Some(200));
    assert_eq!(
        (d.max_stacks, d.reapply, d.decay),
        (5, Reapply::AddStack, Decay::LoseOne)
    );
    assert_eq!((d.icon.as_deref(), d.overlay), (Some("smolder"), Some(17)));
    assert_eq!(
        d.effects,
        [
            StackEffect {
                stat: 25,
                per_stack: 5,
                min_stacks: 1,
                flat: false,
            },
            StackEffect {
                stat: 31,
                per_stack: -2,
                min_stacks: 1,
                flat: false,
            }
        ]
    );
    // A bare entry is a plain state: one stack, refresh, the skill's duration.
    let plain = rules.get(381).unwrap();
    assert_eq!(
        (
            plain.max_stacks,
            plain.reapply,
            plain.decay,
            plain.duration_base_ticks()
        ),
        (1, Reapply::Refresh, Decay::LoseAll, None)
    );
    assert!(plain.effects.is_empty() && plain.icon.is_none());
    assert!(rules.get(382).is_none() && rules.get(-1).is_none());
}

#[test]
fn an_empty_file_defines_nothing() {
    assert!(StateRules::from_toml("").unwrap().is_empty());
    assert!(StateRules::from_toml("version = 1\n").unwrap().is_empty());
}

fn message(text: &str) -> String {
    StateRules::from_toml(text).unwrap_err().to_string()
}

#[test]
fn a_bad_value_names_the_entry_and_the_key() {
    let m = message("[[state]]\nid = 9\nmax_stacks = 0\n");
    assert!(
        m.contains("state[0].max_stacks") && m.contains("1 and 255"),
        "{m}"
    );
    let m = message("[[state]]\nid = 9\n[[state]]\nid = 10\nmax_stacks = 256\n");
    assert!(m.contains("state[1].max_stacks"), "{m}");
    let m = message("[[state]]\nid = 9\nduration_seconds = 0\n");
    assert!(m.contains("state[0].duration_seconds"), "{m}");
    let m = message("[[state]]\nid = 9\ndecay = \"lose_one\"\n");
    assert!(
        m.contains("state[0].decay") && m.contains("duration_seconds"),
        "{m}"
    );
    let m = message("[[state]]\nid = 9\n[[state]]\nid = 9\n");
    assert!(m.contains("state[1].id") && m.contains("twice"), "{m}");
    let m = message("[[state]]\nid = 9\nicon = \"  \"\n");
    assert!(m.contains("state[0].icon"), "{m}");
    let m = message("version = 2\n");
    assert!(m.contains("version") && m.contains("2"), "{m}");
}

#[test]
fn an_unknown_key_or_policy_is_refused() {
    for text in [
        "[[state]]\nid = 9\nmax_stack = 3\n",
        "[[state]]\nid = 9\nreapply = \"stack\"\n",
        "[[state]]\nid = 9\ndecay = \"fade\"\n",
        "[[state]]\nid = 9\n[[state.effect]]\nstat = 1\nper_stack = 1\nextra = 2\n",
        "[[state]]\nname = \"no id\"\n",
        "unknown = 1\n",
    ] {
        let m = message(text);
        assert!(m.starts_with("invalid states.toml"), "{text}: {m}");
    }
}

fn def(max: u32, reapply: Reapply, decay: Decay) -> StateDef {
    StateDef {
        id: 1,
        name: String::new(),
        duration_seconds: Some(4),
        max_stacks: max,
        reapply,
        decay,
        icon: None,
        overlay: None,
        effects: Vec::new(),
        watched: false,
    }
}

#[test]
fn apply_refreshes_stacks_or_ignores() {
    // First application: one stack, whatever the policy.
    for r in [Reapply::Refresh, Reapply::AddStack, Reapply::Ignore] {
        assert_eq!(def(3, r, Decay::LoseAll).stacks_after_apply(0), Some(1));
    }
    // Refresh: the count stays, the clock restarts.
    let refresh = def(3, Reapply::Refresh, Decay::LoseAll);
    assert_eq!(refresh.stacks_after_apply(2), None);
    assert!(refresh.reapply_restarts_clock());
    // Add: one more up to the maximum, then nothing changes.
    let add = def(3, Reapply::AddStack, Decay::LoseAll);
    assert_eq!(add.stacks_after_apply(1), Some(2));
    assert_eq!(add.stacks_after_apply(2), Some(3));
    assert_eq!(add.stacks_after_apply(3), None);
    assert!(add.reapply_restarts_clock());
    // Ignore: a running state is left alone, clock included.
    let ignore = def(3, Reapply::Ignore, Decay::LoseAll);
    assert_eq!(ignore.stacks_after_apply(1), None);
    assert!(!ignore.reapply_restarts_clock());
}

#[test]
fn expiry_loses_all_or_one() {
    let all = def(5, Reapply::AddStack, Decay::LoseAll);
    assert_eq!(all.stacks_after_expiry(4), 0);
    let one = def(5, Reapply::AddStack, Decay::LoseOne);
    assert_eq!(one.stacks_after_expiry(4), 3);
    assert_eq!(one.stacks_after_expiry(1), 0);
    assert_eq!(one.stacks_after_expiry(0), 0);
}

#[test]
fn effects_scale_with_the_stack_count() {
    let e = StackEffect {
        stat: 25,
        per_stack: 5,
        min_stacks: 1,
        flat: false,
    };
    let values: Vec<i32> = (0..=4).map(|n| StateDef::effect_value(&e, n)).collect();
    assert_eq!(values, [0, 5, 10, 15, 20]);
    let neg = StackEffect {
        stat: 31,
        per_stack: -2,
        min_stacks: 1,
        flat: false,
    };
    assert_eq!(StateDef::effect_value(&neg, 3), -6);
    let big = StackEffect {
        stat: 1,
        per_stack: i32::MAX,
        min_stacks: 1,
        flat: false,
    };
    assert_eq!(StateDef::effect_value(&big, 255), i32::MAX);
}

#[test]
fn an_effect_can_wait_for_a_stack_count_and_be_flat() {
    let from_three = StackEffect {
        stat: 25,
        per_stack: 5,
        min_stacks: 3,
        flat: false,
    };
    let values: Vec<i32> = (0..=4)
        .map(|n| StateDef::effect_value(&from_three, n))
        .collect();
    assert_eq!(values, [0, 0, 0, 15, 20], "nothing below 3, then per stack");
    let flat = StackEffect {
        flat: true,
        ..from_three
    };
    let values: Vec<i32> = (0..=4).map(|n| StateDef::effect_value(&flat, n)).collect();
    assert_eq!(values, [0, 0, 0, 5, 5], "the same value from 3 on");
}

#[test]
fn min_stacks_and_flat_are_read_and_checked() {
    let ok = "[[state]]\nid = 5\nmax_stacks = 4\n[[state.effect]]\nstat = 9\nper_stack = 20\n\
              min_stacks = 3\nflat = true\n";
    let rules = StateRules::from_toml(ok).unwrap();
    let e = rules.get(5).unwrap().effects[0];
    assert_eq!((e.min_stacks, e.flat), (3, true));
    for (min, why) in [(0, "0"), (5, "5")] {
        let bad = ok.replace("min_stacks = 3", &format!("min_stacks = {min}"));
        let err = StateRules::from_toml(&bad).unwrap_err().to_string();
        assert!(
            err.contains("state[0].effect[0].min_stacks") && err.contains(why),
            "{err}"
        );
    }
}
