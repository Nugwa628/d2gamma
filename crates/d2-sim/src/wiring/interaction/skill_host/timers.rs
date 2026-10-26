//! [`HostTimers`] on the skill use view.

use crate::rules::HostTimers;
use crate::skills::use_::UseWorld;
use crate::units::UnitId;
use crate::wiring::action::Pending;
use crate::wiring::interaction::{UseRest, UseView};

impl<X: Pending + UseRest> HostTimers for UseView<'_, X> {
    fn schedule_timer(&mut self, u: UnitId, kind: u8, frame: i32, arg1: i32, arg2: i32) {
        UseWorld::schedule(self, u, kind, frame, arg1, arg2);
    }
    fn remove_timers(&mut self, u: UnitId, kind: u8, arg1: Option<i32>) {
        self.cv
            .game
            .timers
            .cancel_unit_events(u, kind, arg1.map(|a| a as u32));
    }
    fn timers(&self, u: UnitId, kind: u8) -> Vec<(i32, i32, i32)> {
        let t = &self.cv.game.timers;
        t.unit_timers(u)
            .into_iter()
            .filter_map(|id| {
                let (e, a1, a2) = t.event(id)?;
                (e == kind).then_some((t.expire(id)?, a1 as i32, a2 as i32))
            })
            .collect()
    }
}
