//! Difficulty progress, quests and waypoints: only forward, in order.

use d2_formats::d2s::Waypoints;

use super::*;

const NORMAL: usize = 0;
const NIGHTMARE: usize = 1;

fn complete(s: &mut D2s, difficulty: usize, quest: usize) {
    let r = &mut s.body.as_mut().expect("a body").quests.records[difficulty];
    r[2 * quest] |= 1;
}

fn waypoint(s: &mut D2s, difficulty: usize, n: u8) {
    let (byte, mask) = Waypoints::bit(n).expect("a waypoint");
    s.body.as_mut().expect("a body").waypoints.records[difficulty][byte] |= mask;
}

/// Progression `p` (acts completed across the difficulties).
fn progressed(mut s: D2s, p: u16) -> D2s {
    s.header.status = (s.header.status & !0x1F00) | (p << 8);
    s
}

#[test]
fn quest_skip_fails() {
    // A Nightmare quest with only Normal open.
    let before = character();
    let mut after = character();
    complete(&mut after, NIGHTMARE, 1);
    let v = run(&before, &after);
    assert!(!v.accepted());
    assert!(failed(&v, "progress"));
    // The same quest in Normal passes.
    let mut after = character();
    complete(&mut after, NORMAL, 1);
    assert!(!failed(&run(&before, &after), "progress"));
    // Nightmare open (five acts, expansion): the Nightmare quest passes.
    let before = progressed(character(), 5);
    let mut after = before.clone();
    complete(&mut after, NIGHTMARE, 1);
    assert!(!failed(&run(&before, &after), "progress"));
}

#[test]
fn waypoint_in_closed_difficulty_fails() {
    let before = character();
    let mut after = character();
    waypoint(&mut after, NIGHTMARE, 3);
    assert!(failed(&run(&before, &after), "progress"));
    let mut after = character();
    waypoint(&mut after, NORMAL, 3);
    assert!(!failed(&run(&before, &after), "progress"));
}

#[test]
fn progress_from_another_difficulty_fails() {
    // Nightmare open, but the save comes from a Normal game.
    let before = progressed(character(), 5);
    let mut after = before.clone();
    complete(&mut after, NIGHTMARE, 2);
    let normal_game = SaveInfo {
        difficulty: Some(0),
        ..SaveInfo::default()
    };
    assert!(failed(
        &list().run(&before, &after, normal_game),
        "progress"
    ));
    let nightmare_game = SaveInfo {
        difficulty: Some(1),
        ..SaveInfo::default()
    };
    assert!(!failed(
        &list().run(&before, &after, nightmare_game),
        "progress"
    ));
}

#[test]
fn progress_going_back_fails() {
    let before = progressed(character(), 3);
    assert!(failed(
        &run(&before, &progressed(character(), 2)),
        "progress"
    ));
    assert!(!failed(
        &run(&before, &progressed(character(), 4)),
        "progress"
    ));
}

#[test]
fn quest_or_waypoint_taken_back_fails() {
    let mut before = character();
    complete(&mut before, NORMAL, 4);
    assert!(failed(&run(&before, &character()), "progress"));
    let mut before = character();
    waypoint(&mut before, NORMAL, 5);
    assert!(failed(&run(&before, &character()), "progress"));
    assert!(!failed(&run(&before, &before), "progress"));
}

#[test]
fn new_character_first_save_is_checked_from_creation() {
    // The stored record of a new character is the creation stub.
    let stub = crate::new_character_like(&character());
    let mut after = character();
    complete(&mut after, NORMAL, 1);
    waypoint(&mut after, NORMAL, 1);
    assert!(!failed(&run(&stub, &after), "progress"));
    let mut after = character();
    complete(&mut after, NIGHTMARE, 1);
    assert!(failed(&run(&stub, &after), "progress"));
}
