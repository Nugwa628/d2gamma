//! A mod game's own bytes on a quest record ([`QuestRecord::mod_data`]):
//! the mod's callback keeps its state there between events.

use std::sync::Arc;

use super::tests::{control, Fake, P1, WARRIV_U};
use super::*;
use crate::rules::{ModRules, QuestProvider, Ruleset};

/// The mod counts each talk to Warriv on chain 0's record.
#[derive(Debug)]
struct Counts;

impl QuestProvider for Counts {
    fn callback(
        &self,
        ctl: &mut QuestControl,
        _w: &mut dyn QuestWorld,
        i: usize,
        args: EventArgs,
        _list: Option<&mut TextList>,
        _force: bool,
    ) -> Option<bool> {
        if (ctl.records[i].chain, args.event) != (0, event::NPC_ACTIVATE) {
            return None;
        }
        let data = &mut ctl.records[i].mod_data;
        match data.first_mut() {
            Some(n) => *n += 1,
            None => data.push(1),
        }
        Some(true)
    }
}

#[derive(Debug)]
struct Game;

impl ModRules for Game {
    fn name(&self) -> &str {
        "quest-record-data"
    }
    fn quests(&self) -> Option<&dyn QuestProvider> {
        Some(&Counts)
    }
}

fn data(ctl: &QuestControl) -> &[u8] {
    &ctl.record(0).expect("record").mod_data
}

#[test]
fn the_mod_keeps_its_bytes_on_the_record() {
    let (mut ctl, _) = control();
    assert!(data(&ctl).is_empty());
    let mut f = Fake::new();
    f.rules = Ruleset::Mod(Arc::new(Game));
    for _ in 0..2 {
        ctl.npc_activate(&mut f, P1, WARRIV_U, &mut TextList::new());
    }
    assert_eq!(data(&ctl), [2]);
}

/// Under D2RUST's rules the bytes stay empty.
#[test]
fn original_rules_leave_the_bytes_empty() {
    let (mut ctl, _) = control();
    let mut o = Fake::new();
    let mut list = TextList::new();
    ctl.npc_activate(&mut o, P1, WARRIV_U, &mut list);
    assert!(list.contains(&(0, 0)));
    assert!(ctl.records.iter().all(|r| r.mod_data.is_empty()));
}
