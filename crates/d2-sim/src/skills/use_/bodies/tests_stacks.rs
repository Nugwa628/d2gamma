//! Applying a state that has stack rules: first application, refresh, add a
//! stack, the maximum, ignore, and the stat effects at 1..max stacks.

use super::fake::BodyFake;
use super::helpers::{apply_state, StateRequest};
use super::tests2::world;
use crate::rules::{Decay, Reapply, StackEffect, StateDef};
use crate::skills::fake::{combat_tables, monster_rec};
use crate::skills::SkillUnits;

const STATE: i32 = 7;

fn def(max: u32, reapply: Reapply, seconds: Option<u32>) -> StateDef {
    StateDef {
        id: STATE as u16,
        name: "Smolder".into(),
        duration_seconds: seconds,
        max_stacks: max,
        reapply,
        decay: Decay::LoseAll,
        icon: None,
        overlay: None,
        effects: vec![
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
            },
        ],
        watched: false,
    }
}

fn request(u: usize, skill: i32, duration: i32) -> StateRequest<usize> {
    StateRequest {
        source: u,
        target: u,
        skill,
        level: 1,
        duration,
        stat: -1,
        value: 0,
        state: STATE,
        callback: 0,
    }
}

fn setup(d: StateDef) -> (BodyFake, usize, crate::combat::CombatTables) {
    let (mut f, u) = world();
    f.state_defs.insert(STATE, d);
    f.c.frame = 100;
    (f, u, combat_tables(vec![monster_rec()]))
}

fn stats(f: &BodyFake, u: usize) -> (u16, Option<i32>, Option<i32>, i32) {
    let l = f.list_of(u, STATE).expect("the state's list");
    (
        l.stacks,
        l.stats.get(&25).copied(),
        l.stats.get(&31).copied(),
        l.expire,
    )
}

#[test]
fn the_first_application_is_one_stack_with_the_states_own_duration() {
    let (mut f, u, ct) = setup(def(5, Reapply::AddStack, Some(8)));
    // The request asks for 30 ticks; the state's own 8 seconds (200 ticks) win.
    apply_state(&mut f, &ct, request(u, 5, 30)).unwrap();
    assert_eq!(stats(&f, u), (1, Some(5), Some(-2), 300));
    assert!(SkillUnits::has_state(&f, u, STATE as u16));
    assert!(f.c.log.iter().any(|l| l == &format!("timer {u} 12 300")));
}

#[test]
fn without_a_duration_of_its_own_the_skills_is_used() {
    let (mut f, u, ct) = setup(def(5, Reapply::AddStack, None));
    apply_state(&mut f, &ct, request(u, 5, 30)).unwrap();
    assert_eq!(stats(&f, u).3, 130);
}

#[test]
fn add_stack_adds_up_to_the_maximum_and_scales_the_effects() {
    let (mut f, u, ct) = setup(def(3, Reapply::AddStack, Some(4)));
    let mut seen = Vec::new();
    for i in 0..5 {
        f.c.frame = 100 + i * 10;
        apply_state(&mut f, &ct, request(u, 5, 0)).unwrap();
        let (n, a, b, e) = stats(&f, u);
        seen.push((n, a.unwrap(), b.unwrap(), e));
    }
    // 1, 2, 3 stacks, then the maximum holds: the values are one stack's
    // times the count and the clock restarts each time.
    assert_eq!(
        seen,
        [
            (1, 5, -2, 200),
            (2, 10, -4, 210),
            (3, 15, -6, 220),
            (3, 15, -6, 230),
            (3, 15, -6, 240),
        ]
    );
    // One list only: a stack is not a second state.
    assert_eq!(f.lists.iter().filter(|l| !l.freed).count(), 1);
}

#[test]
fn a_stack_change_tells_the_client_and_a_plain_refresh_does_not() {
    let (mut f, u, ct) = setup(def(3, Reapply::AddStack, Some(4)));
    apply_state(&mut f, &ct, request(u, 5, 0)).unwrap();
    f.take_log();
    apply_state(&mut f, &ct, request(u, 5, 0)).unwrap();
    assert!(f
        .take_log()
        .iter()
        .any(|l| l == &format!("changed {u} {STATE}")));
    // At the maximum nothing changes for the client: no update.
    apply_state(&mut f, &ct, request(u, 5, 0)).unwrap();
    f.take_log();
    apply_state(&mut f, &ct, request(u, 5, 0)).unwrap();
    assert!(!f.take_log().iter().any(|l| l.starts_with("changed")));
}

#[test]
fn refresh_restarts_the_clock_and_keeps_the_count() {
    let (mut f, u, ct) = setup(def(5, Reapply::Refresh, Some(4)));
    apply_state(&mut f, &ct, request(u, 5, 0)).unwrap();
    f.c.frame = 150;
    apply_state(&mut f, &ct, request(u, 5, 0)).unwrap();
    assert_eq!(stats(&f, u), (1, Some(5), Some(-2), 250));
}

#[test]
fn ignore_leaves_a_running_state_alone() {
    let (mut f, u, ct) = setup(def(5, Reapply::Ignore, Some(4)));
    apply_state(&mut f, &ct, request(u, 5, 0)).unwrap();
    f.c.frame = 150;
    f.take_log();
    apply_state(&mut f, &ct, request(u, 5, 0)).unwrap();
    assert_eq!(stats(&f, u), (1, Some(5), Some(-2), 200));
    assert!(f.take_log().is_empty());
}

#[test]
fn any_skill_stacks_the_same_state() {
    let (mut f, u, ct) = setup(def(5, Reapply::AddStack, Some(4)));
    apply_state(&mut f, &ct, request(u, 5, 0)).unwrap();
    // Another skill (and a lower level) adds to the same state.
    let other = StateRequest {
        level: 0,
        ..request(u, 9, 0)
    };
    apply_state(&mut f, &ct, other).unwrap();
    assert_eq!(stats(&f, u).0, 2);
}

#[test]
fn a_state_without_rules_plays_as_before() {
    let (mut f, u, ct) = setup(def(5, Reapply::AddStack, Some(4)));
    f.state_defs.clear();
    let l = apply_state(&mut f, &ct, request(u, 5, 30)).unwrap();
    // The skill's duration, no stack count, no effects.
    assert_eq!((f.lists[l].stacks, f.lists[l].expire), (0, 130));
    assert!(f.lists[l].stats.is_empty());
}
