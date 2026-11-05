//! [`HostTimers`] on the skill use view.

use crate::rules::{HostTimers, Tick};
use crate::skills::use_::UseWorld;
use crate::tick::events::event;
use crate::units::dispatch::MOD_TICK;
use crate::units::UnitId;
use crate::wiring::action::ModTickJob;
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
    fn schedule_tick(&mut self, u: UnitId, frames: i32, tick: Tick) -> bool {
        let h = &mut *self.cv.v.h;
        // Jobs of units that left the game have no timer any more.
        let game = &*self.cv.game;
        h.mod_ticks.retain(|_, j| game.lists.unit(j.unit).is_some());
        let due = game.after(frames.max(1));
        let job = h.mod_tick_next.max(1);
        let scheduled = self.cv.game.schedule_event(
            u,
            u32::from(event::REMOVE_SKILL_COOLDOWN),
            due,
            Some(MOD_TICK),
            job,
            0,
        );
        if !matches!(scheduled, Ok(Some(_))) {
            return false;
        }
        let h = &mut *self.cv.v.h;
        h.mod_tick_next = job.wrapping_add(1).max(1);
        h.mod_ticks.insert(job, ModTickJob { unit: u, due, tick });
        true
    }
    fn cancel_ticks(&mut self, u: UnitId, skill: Option<i32>) {
        let h = &mut *self.cv.v.h;
        let jobs: Vec<u32> = h
            .mod_ticks
            .iter()
            .filter(|(_, j)| j.unit == u && skill.is_none_or(|s| j.tick.skill == s))
            .map(|(&id, _)| id)
            .collect();
        for job in jobs {
            self.cv.v.h.mod_ticks.remove(&job);
            self.cv
                .game
                .timers
                .cancel_unit_events(u, event::REMOVE_SKILL_COOLDOWN, Some(job));
        }
    }
    fn ticks(&self, u: UnitId) -> Vec<(i32, Tick)> {
        let mut v: Vec<(i32, i32, Tick)> = self
            .cv
            .v
            .h
            .mod_ticks
            .iter()
            .filter(|(_, j)| j.unit == u)
            .map(|(&id, j)| (j.due, id as i32, j.tick))
            .collect();
        v.sort_by_key(|(due, id, _)| (*due, *id));
        v.into_iter().map(|(due, _, t)| (due, t)).collect()
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
