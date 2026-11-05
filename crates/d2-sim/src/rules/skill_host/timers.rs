//! Timer events for a mod game's skill bodies.

use crate::units::UnitId;

/// One pending run of a block skill part over time (a channel's next pulse,
/// a leap's next step). The host keeps it on the game's timer list and hands
/// it back to [`crate::rules::SkillProvider::tick`] when it is due.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tick {
    /// The skill whose part started it, and the level it was cast at.
    pub skill: i32,
    pub level: i32,
    /// Which part of the skill's list this tick belongs to.
    pub part: u16,
    /// Runs still to come, this one included.
    pub left: u16,
    /// Runs the part was started with.
    pub of: u16,
    /// Where the caster was when the part started.
    pub from: (i32, i32),
    /// The point a leap is heading for.
    pub to: (i32, i32),
}

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
    /// Asks for `tick` to come back to the provider's
    /// [`crate::rules::SkillProvider::tick`] for `u` after `frames` base
    /// ticks (at least 1). False when the host has no tick seam (nothing is
    /// scheduled then).
    fn schedule_tick(&mut self, u: UnitId, frames: i32, tick: Tick) -> bool {
        let _ = (u, frames, tick);
        false
    }
    /// Drops the unit's pending ticks of `skill`, or all of them for `None`.
    fn cancel_ticks(&mut self, u: UnitId, skill: Option<i32>) {
        let _ = (u, skill);
    }
    /// The unit's pending ticks as `(frame, tick)`, soonest first.
    fn ticks(&self, u: UnitId) -> Vec<(i32, Tick)> {
        let _ = u;
        Vec::new()
    }
}
