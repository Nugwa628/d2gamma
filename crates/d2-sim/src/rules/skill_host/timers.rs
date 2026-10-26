//! Timer events for a mod game's skill bodies.

use crate::units::UnitId;

/// What a skill body schedules on the game's timer list: a unit event of
/// a type at a frame with two arguments (a skill delay end carries the
/// skill and level), and their removal.
pub trait HostTimers {
    /// Schedules timer event `kind` (the game's event types: 1 end of
    /// mode, 5 skill delay end, 12 stat list expiry, ...) on the unit at
    /// frame `frame` with `arg1` and `arg2`.
    fn schedule_timer(&mut self, u: UnitId, kind: u8, frame: i32, arg1: i32, arg2: i32) {
        let _ = (u, kind, frame, arg1, arg2);
    }
    /// Removes the unit's pending timers of `kind` whose first argument
    /// is `arg1` (a skill), or all of them for `None`.
    fn remove_timers(&mut self, u: UnitId, kind: u8, arg1: Option<i32>) {
        let _ = (u, kind, arg1);
    }
    /// The unit's pending timers of `kind` as `(frame, arg1, arg2)`, in
    /// list order.
    fn timers(&self, u: UnitId, kind: u8) -> Vec<(i32, i32, i32)> {
        let _ = (u, kind);
        Vec::new()
    }
}
