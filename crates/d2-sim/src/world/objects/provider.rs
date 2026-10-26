//! A mod game's object operate functions (`docs/plans/mods/seams.md` MS8b).
//!
//! An object's `objects.txt` row names its operate function by number. The
//! operate dispatch ([`super::dispatch`]) asks the mod first:
//! - [`ObjectProvider::has_operate`] claims a function number. D2RUST's
//!   table ends at [`super::OPERATE_FN_BOUND`]; a claimed number past it is
//!   the mod's own slot instead of a fatal error, so the mod sets its own
//!   slot count. A mod may also claim one of D2RUST's numbers (a chest,
//!   a quest object, a waypoint ...) to take it over.
//! - [`ObjectProvider::operate`] runs a claimed function. `None` falls
//!   through to D2RUST's route for the number (past the bound: nothing
//!   runs, the dispatch returns 0).
//!
//! Object timer events work the same way: [`ObjectProvider::has_event`]
//! claims an event type (D2RUST's or the mod's own, scheduled with
//! [`super::ObjectWorld::schedule`]) and [`ObjectProvider::event`] runs it;
//! `None` runs D2RUST's handler.
//!
//! The dispatch's own checks stay D2RUST's and run first: the player
//! refusals (interacting, busy, an item on the cursor away from the stash)
//! and the refused classes. Unclaimed numbers run exactly as under
//! `Original`.
//!
//! Providers get the object world as `dyn ObjectHost` and must stay
//! deterministic: no I/O, no clock, no floats in game state, draws only
//! from the game's seeds (the control seed in [`ObjectControl`], the unit
//! seeds through [`super::ObjectWorld::unit_seed`]).

use std::fmt::Debug;

use super::{ObjectControl, ObjectError, ObjectHost, ObjectTables, Operate};
use crate::units::UnitId;

/// A mod game's object operate functions. Every method defaults to "not
/// the mod's", so a provider only writes what its game adds.
pub trait ObjectProvider: Send + Sync + Debug {
    /// The mod runs operate function `n` (any number, D2RUST's or past
    /// [`super::OPERATE_FN_BOUND`]).
    fn has_operate(&self, n: u8) -> bool {
        let _ = n;
        false
    }

    /// Run claimed operate function `op.operate_fn` on `op.object`. `Some`:
    /// the function's return value (or a fatal error); `None`: D2RUST's
    /// route for the number runs instead.
    fn operate(
        &self,
        ctl: &mut ObjectControl,
        t: &ObjectTables,
        w: &mut dyn ObjectHost,
        op: &Operate,
    ) -> Option<Result<i32, ObjectError>> {
        let _ = (ctl, t, w, op);
        None
    }

    /// The mod runs object timer event type `ev` (D2RUST's, or one of the
    /// types 12–14 D2RUST has no object handler for; the timer queue holds
    /// types 0–14).
    fn has_event(&self, ev: u8) -> bool {
        let _ = ev;
        false
    }

    /// Run claimed timer event `ev` of `object`. `Some`: done (or a fatal
    /// error); `None`: D2RUST's handler for the type runs instead.
    fn event(
        &self,
        ctl: &mut ObjectControl,
        t: &ObjectTables,
        w: &mut dyn ObjectHost,
        object: UnitId,
        ev: u8,
    ) -> Option<Result<(), ObjectError>> {
        let _ = (ctl, t, w, object, ev);
        None
    }
}
