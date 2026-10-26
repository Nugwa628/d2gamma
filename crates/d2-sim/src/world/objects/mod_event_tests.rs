//! A mod game's object timer events ([`ObjectProvider::event`]) on the
//! object fake: the mod's event type runs its function, which calls
//! D2RUST's mode helper on the `dyn` host and plays sounds by wide id; an
//! event the mod declines runs D2RUST's.

use std::sync::Arc;

use super::fake::*;
use super::*;
use crate::rules::{ModRules, Ruleset};

const O: UnitId = UnitId(1);
/// An event type D2RUST has no handler for (12–14 have none).
const EV: u8 = 13;
const SOUND: u8 = 9;

#[derive(Debug)]
struct Events;

impl ObjectProvider for Events {
    fn has_event(&self, ev: u8) -> bool {
        matches!(ev, EV | oevent::END_ANIM)
    }
    fn event(
        &self,
        ctl: &mut ObjectControl,
        t: &ObjectTables,
        w: &mut dyn ObjectHost,
        object: UnitId,
        ev: u8,
    ) -> Option<Result<(), ObjectError>> {
        if ev != EV {
            return None;
        }
        if let Err(e) = set_object_mode(ctl, t, w, object, 2) {
            return Some(Err(e));
        }
        clear_selectable(w, object);
        w.sound_id(object, u16::from(SOUND), None, false);
        // Past a byte: the default plays nothing.
        w.sound_id(object, 0x2710, None, false);
        Some(Ok(()))
    }
}

#[derive(Debug)]
struct Game;

impl ModRules for Game {
    fn name(&self) -> &str {
        "object-events"
    }
    fn objects(&self) -> Option<&dyn ObjectProvider> {
        Some(&Events)
    }
}

/// Object `O` (row 0, mode 1 with a mode 2, selectable) under `rules`.
fn world(rules: Ruleset) -> (ObjectControl, ObjectTables, Fake) {
    let mut row = blank_object();
    row.mode2 = 1;
    let t = ObjectTables {
        objects: vec![row],
        ..ObjectTables::default()
    };
    let (mut ctl, _) = ObjectControl::new(&mut Seed::init(), &t);
    ctl.data.insert(O, ObjectData::default());
    let mut f = Fake {
        rules,
        ..Fake::default()
    };
    f.modes.insert(O, 1);
    f.flags.insert(O, oflags::SELECTABLE);
    (ctl, t, f)
}

#[test]
fn the_mod_event_runs() {
    let (mut ctl, t, mut f) = world(Ruleset::Mod(Arc::new(Game)));
    assert_eq!(
        object_event(&mut ctl, &t, &mut f, O, EV),
        Ok(EventRun::Done)
    );
    assert_eq!(f.modes[&O], 2);
    assert_eq!(f.flags[&O] & oflags::SELECTABLE, 0);
    let sounds = f.calls_of(|c| matches!(c, Call::Sound(..)));
    assert_eq!(sounds, [Call::Sound(O, SOUND, None, false)]);
}

#[test]
fn original_rules_leave_the_event_uncovered() {
    let (mut ctl, t, mut f) = world(Ruleset::Original);
    assert_eq!(
        object_event(&mut ctl, &t, &mut f, O, EV),
        Ok(EventRun::NotCovered(EV))
    );
    assert_eq!((f.modes[&O], f.calls.len()), (1, 0));
}

/// A declined event type runs D2RUST's handler, as without the mod.
#[test]
fn a_declined_event_runs_d2rusts() {
    let (mut ctl, t, mut f) = world(Ruleset::Mod(Arc::new(Game)));
    let r = object_event(&mut ctl, &t, &mut f, O, oevent::END_ANIM);
    let (mut ctl0, t0, mut f0) = world(Ruleset::Original);
    let r0 = object_event(&mut ctl0, &t0, &mut f0, O, oevent::END_ANIM);
    assert_eq!(r, r0);
    assert_eq!((f.modes[&O], &f.calls), (f0.modes[&O], &f0.calls));
    assert_eq!(f.modes[&O], 2);
}
