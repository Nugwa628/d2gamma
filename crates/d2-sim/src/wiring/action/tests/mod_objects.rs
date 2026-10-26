//! A mod game's object seams on the action wiring: a timer event type past
//! the handler table reaching the mod's event function, a sound id past
//! 255, a level warp from an object and a wide portal destination kept by
//! the inactive store.

use std::sync::Arc;

use d2_data::tables::Objects;

use super::*;
use crate::rules::{ModRules, ObjectProvider, Ruleset};
use crate::tick::TickHooks;
use crate::units::hooks::UnitHooks;
use crate::units::modes::UnitError;
use crate::world::objects::{self as obj, ObjectControl, ObjectError, ObjectHost, ObjectTables};

/// An object event type without a handler in the table (12–14 have none).
const EV: u8 = 13;
/// A sound id past 255.
const SOUND: u16 = 0x2710;
/// A destination level past 255.
const DEST: u32 = 300;

/// Event [`EV`]: mode 1 through D2RUST's mode helper on the `dyn` host,
/// sound [`SOUND`] and a warp of the player to [`DEST`].
#[derive(Debug)]
struct Events(UnitId);

impl ObjectProvider for Events {
    fn has_event(&self, ev: u8) -> bool {
        ev == EV
    }
    fn event(
        &self,
        ctl: &mut ObjectControl,
        t: &ObjectTables,
        w: &mut dyn ObjectHost,
        object: UnitId,
        _ev: u8,
    ) -> Option<Result<(), ObjectError>> {
        if let Err(e) = obj::set_object_mode(ctl, t, w, object, 1) {
            return Some(Err(e));
        }
        w.sound_id(object, SOUND, None, false);
        w.warp_to_level(self.0, DEST, 2);
        Some(Ok(()))
    }
}

#[derive(Debug)]
struct Game(Events);

impl ModRules for Game {
    fn name(&self) -> &str {
        "mod-objects"
    }
    fn objects(&self) -> Option<&dyn ObjectProvider> {
        Some(&self.0)
    }
}

/// One plain object row the inactive store keeps (`Restore` 1).
fn tables() -> Arc<ObjectTables> {
    let mut row = blank::<Objects>();
    row.restore = 1;
    Arc::new(ObjectTables {
        objects: vec![row],
        shrines: Vec::new(),
        levels: vec![blank(); 150],
        objgroup: Vec::new(),
        leveldefs: Vec::new(),
    })
}

/// The fixture with an object in mode 0 and a player in room A.
fn setup() -> (Fx, UnitId, UnitId) {
    let mut fx = Fx::new();
    fx.sim.create_objects(tables());
    let a = fx.a;
    let o = fx
        .sim
        .with(&mut fx.game, |g, v| v.create_object(g, a, 0, 20, 20, 0))
        .expect("allocated");
    let p = fx.spawn(UnitType::Player, 0, a, 22, 20);
    (fx, o, p)
}

fn run_event(fx: &mut Fx, o: UnitId) {
    let at = fx.game.frame + 1;
    fx.game
        .schedule_event(o, u32::from(EV), at, None, 0, 0)
        .expect("schedule");
    fx.frame();
}

/// The timer reaches the mod's event function: the mode is set, the wide
/// sound id queued and the warp queued as a level change.
#[test]
fn a_mod_event_runs_through_the_timer() {
    let (mut fx, o, p) = setup();
    fx.sim.hooks().rules = Ruleset::Mod(Arc::new(Game(Events(p))));
    assert!(fx.sim.hooks().mod_object_event(EV));
    assert!(!fx.sim.hooks().mod_object_event(EV + 1));
    run_event(&mut fx, o);
    assert!(fx.sim.sys.errors.is_empty(), "{:?}", fx.sim.sys.errors);
    assert_eq!(fx.sim.sys.units.get(o).unwrap().mode, 1);
    assert_eq!(fx.game.sounds.get(o).map(|s| s.event), Some(SOUND));
    assert_eq!(fx.sim.hooks().act_changes, [(p, DEST, 2)]);
}

/// Under D2RUST's rules the event has no handler: the null-handler error,
/// nothing runs.
#[test]
fn original_rules_keep_the_null_handler() {
    let (mut fx, o, _) = setup();
    assert!(!fx.sim.hooks().mod_object_event(EV));
    run_event(&mut fx, o);
    let errors: Vec<_> = fx.sim.sys.errors.iter().map(|(_, e)| e).collect();
    assert_eq!(
        errors,
        [&UnitError::NullHandler {
            ty: UnitType::Object,
            event: EV
        }]
    );
    assert_eq!(fx.sim.sys.units.get(o).unwrap().mode, 0);
    assert_eq!(fx.game.sounds.get(o), None);
    assert!(fx.sim.hooks().act_changes.is_empty());
}

/// The object seam's own sounds keep their byte ids.
#[test]
fn byte_sounds_are_unchanged() {
    let (mut fx, o, _) = setup();
    fx.sim
        .objects(&mut fx.game, |_, _, w| {
            obj::ObjectWorld::sound(w, o, 13, None, false)
        })
        .expect("object state");
    assert_eq!(fx.game.sounds.get(o).map(|s| s.event), Some(13));
}

/// A stored object keeps its wide destination through the inactive store.
#[test]
fn the_inactive_store_keeps_a_wide_destination() {
    let (mut fx, o, _) = setup();
    fx.sim.hooks().enable_inactive_store();
    fx.sim.sys.units.get_mut(o).unwrap().flags |= 0x200_0000;
    let st = fx.sim.hooks().objects.as_mut().unwrap();
    st.control.data.get_mut(&o).unwrap().dest_level = Some(DEST);
    let a = fx.a;
    let mut g = std::mem::take(&mut fx.game);
    fx.sim.compress_unit(&mut g, o);
    assert!(fx.sim.restore(&mut g, a));
    fx.game = g;
    let back: Vec<_> = fx
        .game
        .lists
        .room_units(a)
        .into_iter()
        .filter(|&u| {
            fx.game
                .lists
                .unit(u)
                .is_some_and(|e| e.ty == UnitType::Object)
        })
        .collect();
    assert_eq!(back.len(), 1);
    let st = fx.sim.hooks().objects.as_ref().unwrap();
    let d = st.control.data[&back[0]];
    assert_eq!((d.dest_level, d.destination()), (Some(DEST), DEST));
}
