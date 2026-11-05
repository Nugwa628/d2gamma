//! The vocabulary tables against the reader they describe.

use super::*;
use crate::rules::data_mod::ai::AiRules;
use std::collections::BTreeSet;

fn literal(v: &toml::Value) -> String {
    v.to_string()
}

/// `{ kind = "...", key = value, ... }` for the given pairs.
fn item(kind: &str, pairs: &[(&str, toml::Value)]) -> String {
    let mut out = format!("{{ kind = \"{kind}\"");
    for (k, v) in pairs {
        out.push_str(&format!(", {k} = {}", literal(v)));
    }
    out.push_str(" }");
    out
}

/// A file with a state `a`, a state `b`, a cooldown `c`, and the given
/// condition as the only condition of a transition from `a` to `b`.
fn with_condition(c: &str) -> String {
    format!(
        "[[ai]]\nindex = 1\n[[ai.cooldown]]\nname = \"c\"\nseconds = 1\n\
         [[ai.state]]\nname = \"a\"\n[[ai.state.transition]]\nto = \"b\"\nwhen = [{c}]\n\
         [[ai.state]]\nname = \"b\"\n"
    )
}

fn with_action(a: &str) -> String {
    format!(
        "[[ai]]\nindex = 1\n[[ai.cooldown]]\nname = \"c\"\nseconds = 1\n\
         [[ai.state]]\nname = \"a\"\n{a}\n"
    )
}

fn action_text(kind: &str, pairs: &[(&str, toml::Value)]) -> String {
    let mut out = format!("[[ai.state.action]]\nkind = \"{kind}\"\n");
    for (k, v) in pairs {
        out.push_str(&format!("{k} = {}\n", literal(v)));
    }
    out
}

/// The value a field takes in a test: a declared cooldown for a cooldown key.
fn test_value(f: &Field) -> toml::Value {
    match f.kind {
        FieldKind::Cooldown => toml::Value::String("c".into()),
        FieldKind::Name => toml::Value::String("roar".into()),
        _ => f.default_value(),
    }
}

fn required(k: &KindInfo) -> Vec<(&'static str, toml::Value)> {
    k.fields
        .iter()
        .filter(|f| f.required)
        .map(|f| (f.key, test_value(f)))
        .collect()
}

fn all(k: &KindInfo) -> Vec<(&'static str, toml::Value)> {
    k.fields.iter().map(|f| (f.key, test_value(f))).collect()
}

/// The variant names serde lists when it meets a kind it does not know.
fn serde_kinds(text: String) -> BTreeSet<String> {
    let e = AiRules::from_toml(&text).unwrap_err().to_string();
    let list = e.split("expected one of ").nth(1).expect(&e);
    list.lines()
        .next()
        .unwrap()
        .split(", ")
        .map(|w| w.trim_matches(|c| c == '`' || c == ' ').to_owned())
        .collect()
}

#[test]
fn every_condition_the_reader_knows_is_described() {
    let known = serde_kinds(with_condition(&item("zzz", &[])));
    let described: BTreeSet<String> = CONDITIONS.iter().map(|k| k.name.to_owned()).collect();
    assert_eq!(known, described);
}

#[test]
fn every_action_the_reader_knows_is_described() {
    let known = serde_kinds(with_action(&action_text("zzz", &[])));
    let described: BTreeSet<String> = ACTIONS.iter().map(|k| k.name.to_owned()).collect();
    assert_eq!(known, described);
}

#[test]
fn a_kind_is_described_once() {
    for list in [CONDITIONS, ACTIONS] {
        let names: BTreeSet<_> = list.iter().map(|k| k.name).collect();
        assert_eq!(names.len(), list.len());
        for k in list {
            let keys: BTreeSet<_> = k.fields.iter().map(|f| f.key).collect();
            assert_eq!(keys.len(), k.fields.len(), "{}", k.name);
        }
    }
}

#[test]
fn every_condition_loads_with_its_required_keys_and_with_all_keys() {
    for k in CONDITIONS {
        for pairs in [required(k), all(k)] {
            let text = with_condition(&item(k.name, &pairs));
            let rules = AiRules::from_toml(&text).unwrap_or_else(|e| panic!("{}: {e}", k.name));
            let t = &rules.defs()[0].states[0].transitions[0];
            assert_eq!(t.when[0].kind_name(), k.name);
        }
    }
}

#[test]
fn every_action_loads_with_its_required_keys_and_with_all_keys() {
    for k in ACTIONS {
        for pairs in [required(k), all(k)] {
            let text = with_action(&action_text(k.name, &pairs));
            let rules = AiRules::from_toml(&text).unwrap_or_else(|e| panic!("{}: {e}", k.name));
            let a = &rules.defs()[0].states[0].actions[0];
            assert_eq!(a.kind.kind_name(), k.name);
            assert_eq!(a.kind.is_instant(), k.is_instant(), "{}", k.name);
        }
    }
}

#[test]
fn a_required_key_left_out_is_refused() {
    for k in ACTIONS {
        for f in k.fields.iter().filter(|f| f.required) {
            let pairs: Vec<_> = required(k)
                .into_iter()
                .filter(|(n, _)| *n != f.key)
                .collect();
            let text = with_action(&action_text(k.name, &pairs));
            assert!(
                AiRules::from_toml(&text).is_err(),
                "{} without {}",
                k.name,
                f.key
            );
        }
    }
    for k in CONDITIONS {
        for f in k.fields.iter().filter(|f| f.required) {
            let pairs: Vec<_> = required(k)
                .into_iter()
                .filter(|(n, _)| *n != f.key)
                .collect();
            let text = with_condition(&item(k.name, &pairs));
            assert!(
                AiRules::from_toml(&text).is_err(),
                "{} without {}",
                k.name,
                f.key
            );
        }
    }
}

#[test]
fn the_stated_bounds_are_the_readers() {
    let probe = |k: &KindInfo, f: &Field, v: i64, is_action: bool| -> bool {
        let mut pairs = required(k);
        pairs.retain(|(n, _)| *n != f.key);
        pairs.push((f.key, toml::Value::Integer(v)));
        let text = if is_action {
            with_action(&action_text(k.name, &pairs))
        } else {
            with_condition(&item(k.name, &pairs))
        };
        AiRules::from_toml(&text).is_ok()
    };
    for (list, is_action) in [(CONDITIONS, false), (ACTIONS, true)] {
        for k in list {
            for f in k.fields {
                let FieldKind::Int { min, max, default } = f.kind else {
                    continue;
                };
                assert!((min..=max).contains(&default), "{}.{}", k.name, f.key);
                let (min, max) = (i64::from(min), i64::from(max));
                assert!(probe(k, f, min, is_action), "{}.{} at {min}", k.name, f.key);
                assert!(probe(k, f, max, is_action), "{}.{} at {max}", k.name, f.key);
                assert!(
                    !probe(k, f, max + 1, is_action),
                    "{}.{} at {}",
                    k.name,
                    f.key,
                    max + 1
                );
                // The lowest value is refused when it is not the lowest a
                // number of that width can hold.
                if min > 0 {
                    assert!(
                        !probe(k, f, min - 1, is_action),
                        "{}.{} at {}",
                        k.name,
                        f.key,
                        min - 1
                    );
                }
            }
        }
    }
}

#[test]
fn a_choice_default_is_one_of_its_options() {
    for list in [CONDITIONS, ACTIONS] {
        for k in list {
            for f in k.fields {
                if let FieldKind::Choice { options, default } = f.kind {
                    assert!(options.iter().any(|(w, _)| *w == default), "{}", k.name);
                }
            }
        }
    }
}

#[test]
fn the_target_words_are_the_readers() {
    for (word, _) in TARGETS {
        let text = format!("[[ai]]\nindex = 1\ntarget = \"{word}\"\n[[ai.state]]\nname = \"a\"\n");
        assert!(AiRules::from_toml(&text).is_ok(), "{word}");
    }
    let bad = concat!(
        "[[ai]]\nindex = 1\ntarget = \"zzz\"\n",
        "[[ai.state]]\nname = \"a\"\n"
    );
    let known = serde_kinds(bad.to_owned());
    let described: BTreeSet<String> = TARGETS.iter().map(|(w, _)| (*w).to_owned()).collect();
    assert_eq!(known, described);
}

#[test]
fn lookups_find_kinds_by_name() {
    assert_eq!(
        condition_kind("life_below").unwrap().fields[0].key,
        "percent"
    );
    assert_eq!(action_kind("summon").unwrap().fields.len(), 3);
    assert!(condition_kind("summon").is_none());
    assert!(action_kind("life_below").is_none());
}
