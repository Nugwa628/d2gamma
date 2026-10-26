//! A game that stops in the middle of a move (here: a panic, then a fresh
//! open and a character loaded from its file, as the next start would)
//! neither loses nor doubles anything.

use std::panic::{catch_unwind, AssertUnwindSafe};

use super::*;
use crate::local::Crash;

#[derive(Clone, Copy, Debug)]
enum Move {
    PutItem,
    PutStacks,
    PutGold,
    TakeItem,
    TakeStack,
    TakeGold,
}

const MOVES: [Move; 6] = [
    Move::PutItem,
    Move::PutStacks,
    Move::PutGold,
    Move::TakeItem,
    Move::TakeStack,
    Move::TakeGold,
];

const POINTS: [Crash; 7] = [
    Crash::Lifted,
    Crash::InTransaction,
    Crash::Stored(1),
    Crash::Stored(2),
    Crash::Written,
    Crash::InFlight,
    Crash::Placed,
];

/// Whether a move passes `at` (and so must stop there).
fn reaches(m: Move, at: Crash) -> bool {
    let put = matches!(m, Move::PutItem | Move::PutStacks | Move::PutGold);
    match at {
        Crash::Lifted => put,
        Crash::Stored(2) => matches!(m, Move::PutStacks),
        Crash::InFlight | Crash::Placed => !put,
        Crash::InTransaction | Crash::Stored(_) | Crash::Written => true,
    }
}

/// A character with goods of every kind, and a stash holding some.
fn setup(dir: &Path) -> (LocalStash, FakeGame) {
    let mut s = open(dir, false);
    let mut g = FakeGame::new(dir, "Hero");
    g.gold = 5000;
    let ring = g.add("rin", None, (1, 1), "Ring");
    let runes: Vec<u32> = (0..4).map(|_| g.stackable("r12")).collect();
    g.save();
    s.put(
        &mut g,
        Put::Item {
            unit: ring,
            at: Some(Cell::new(1, 2, 2)),
        },
    )
    .unwrap();
    s.put(&mut g, Put::Stacks { units: runes }).unwrap();
    s.put(&mut g, Put::Gold(2000)).unwrap();
    g.add("lsd", None, (1, 3), "Sword");
    g.stackable("r01");
    g.stackable("gpr");
    g.add("key", Some(6), (1, 1), "key");
    g.save();
    (s, g)
}

fn run(s: &mut LocalStash, g: &mut FakeGame, m: Move) -> Result<(), StoreError> {
    match m {
        Move::PutItem => {
            let sword = g.items.iter().find(|i| i.name == "Sword").unwrap().unit;
            s.put(
                g,
                Put::Item {
                    unit: sword,
                    at: None,
                },
            )
            .map(drop)
        }
        Move::PutStacks => {
            let units = g.units();
            s.put(g, Put::Stacks { units }).map(drop)
        }
        Move::PutGold => s.put(g, Put::Gold(1000)).map(drop),
        Move::TakeItem => {
            let id = s.search("Ring").unwrap()[0].0;
            s.take(
                g,
                Take::Item {
                    id,
                    to: TakeTo::Cursor,
                },
            )
            .map(drop)
        }
        Move::TakeStack => s
            .take(
                g,
                Take::Stack {
                    code: "r12".into(),
                    count: 3,
                },
            )
            .map(drop),
        Move::TakeGold => s.take(g, Take::Gold(1500)).map(drop),
    }
}

#[test]
fn a_crash_at_any_step_of_any_move_loses_and_doubles_nothing() {
    for m in MOVES {
        for at in POINTS {
            let dir = folder(&format!("crash-{m:?}-{at:?}"));
            let (mut s, mut g) = setup(&dir);
            let before = world_totals(&g, &mut s);
            let gold_before = before["gold"];
            s.crash = Some(at);
            let stopped = catch_unwind(AssertUnwindSafe(move || {
                let r = run(&mut s, &mut g, m);
                // The game ends with the stash (a crash drops both mid-move).
                drop(s);
                r
            }));
            assert_eq!(
                stopped.is_err(),
                reaches(m, at),
                "{m:?} at {at:?}: stopped there"
            );
            if let Ok(r) = stopped {
                r.unwrap();
            }

            // The next start: the stash opens first, then the character.
            let mut s = open(&dir, false);
            let g = FakeGame::load(&dir, "Hero");
            assert_eq!(world_totals(&g, &mut s), before, "{m:?} at {at:?}");
            assert_eq!(world_totals(&g, &mut s)["gold"], gold_before);
            assert!(s.recovered().homeless.is_empty(), "{m:?} at {at:?}");
            drop(s);
            // Nothing is left to finish.
            let s = open(&dir, false);
            assert_eq!(
                s.recovered(),
                &Recovered::default(),
                "{m:?} at {at:?}: finished in one open"
            );
        }
    }
}

#[test]
fn a_move_after_recovery_works_from_the_recovered_state() {
    let dir = folder("crash-then-move");
    let (mut s, mut g) = setup(&dir);
    s.crash = Some(Crash::Stored(1));
    let units = g.units();
    let stopped = catch_unwind(AssertUnwindSafe(move || {
        let _ = s.put(&mut g, Put::Stacks { units });
    }));
    assert!(stopped.is_err());
    let mut s = open(&dir, false);
    assert_eq!(s.recovered().rewritten.len(), 1);
    let mut g = FakeGame::load(&dir, "Hero");
    let before = world_totals(&g, &mut s);
    let units = g.units();
    let done = s.put(&mut g, Put::Stacks { units }).unwrap();
    assert_eq!(done.moved.len(), 2, "the gem and the keys; the rune went");
    assert_eq!(done.left.len(), 1, "the sword");
    assert_eq!(world_totals(&g, &mut s), before);
    assert_eq!(FakeGame::load(&dir, "Hero").totals(), g.totals());
}
