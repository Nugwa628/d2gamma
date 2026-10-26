//! A mod game's quest-routed object operates
//! ([`QuestProvider::object_operate`]): the operate the object module hands
//! to the quest code runs the mod's function with the quest control.

use std::sync::Arc;

use super::tests::{control, Fake, P1};
use super::*;
use crate::rules::{ModRules, QuestProvider, Ruleset};
use crate::wiring::action::{ObjectRoute, QuestObjectCall};
use crate::wiring::economy::quest_objects::run;
use crate::wiring::economy::QuestObjectRun;
use crate::world::objects::{Dispatch, Operate};

/// A quest-routed operate number (D2RUST's: the Act V portal operate).
const FN: u8 = 70;
/// The chain whose record the mod's operate moves to [`STATE`].
const CHAIN: u8 = 3;
const STATE: u8 = 9;

/// Operate [`FN`] with an operator: chain [`CHAIN`] to [`STATE`]; without
/// one: declined.
#[derive(Debug)]
struct Operates;

impl QuestProvider for Operates {
    fn has_object_operate(&self, n: u8) -> bool {
        n == FN
    }
    fn object_operate(
        &self,
        ctl: &mut QuestControl,
        _w: &mut dyn QuestWorld,
        op: &Operate,
    ) -> bool {
        if op.operator.is_none() {
            return false;
        }
        let i = ctl.find(CHAIN).expect("record");
        ctl.records[i].state = STATE;
        true
    }
}

#[derive(Debug)]
struct Game;

impl ModRules for Game {
    fn name(&self) -> &str {
        "quest-operates"
    }
    fn quests(&self) -> Option<&dyn QuestProvider> {
        Some(&Operates)
    }
}

fn call(operator: Option<UnitId>) -> QuestObjectCall {
    QuestObjectCall {
        route: ObjectRoute::Operate(Dispatch::Quest(Operate {
            object: UnitId(90),
            operator,
            class: 1,
            operate_fn: FN,
        })),
        class: 1,
        room: None,
        x: 0,
        y: 0,
    }
}

fn state(ctl: &QuestControl) -> u8 {
    ctl.record(CHAIN).expect("record").state
}

#[test]
fn the_mod_runs_a_quest_routed_operate() {
    let (mut ctl, _) = control();
    let mut f = Fake::new();
    f.rules = Ruleset::Mod(Arc::new(Game));
    assert_eq!(run(&mut ctl, &mut f, &call(Some(P1))), QuestObjectRun::Ran);
    assert_eq!(state(&ctl), STATE);
    // Declined: handed back to the host as under D2RUST's rules.
    let (mut ctl, _) = control();
    let c = call(None);
    assert_eq!(run(&mut ctl, &mut f, &c), QuestObjectRun::HandBack(c.route));
    assert_ne!(state(&ctl), STATE);
}

/// Under D2RUST's rules its own operate runs, and without an operator the
/// route is handed back.
#[test]
fn original_rules_run_d2rusts_operate() {
    let (mut ctl, _) = control();
    let mut f = Fake::new();
    assert_eq!(run(&mut ctl, &mut f, &call(Some(P1))), QuestObjectRun::Ran);
    assert_ne!(state(&ctl), STATE);
    let c = call(None);
    assert_eq!(run(&mut ctl, &mut f, &c), QuestObjectRun::HandBack(c.route));
}
