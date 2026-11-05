//! Applying a state that has stack rules (the mod kit's `states.toml`,
//! [`crate::rules::StateDef`]): the first application puts the state on with
//! one stack, a later one refreshes, adds a stack or changes nothing as the
//! state's re-apply policy says, and the stat effects of the state's list are
//! always one stack's value times the stack count.
//!
//! The expiry side (a stack lost at a time) is
//! `crate::wiring::action::state_stacks`.

use super::helpers::{group, StateRequest};
use super::{callback, BodyWorld};
use crate::combat::CombatWorld;
use crate::rules::StateDef;
use crate::skills::use_::UseWorld;

/// The list's stat values for `stacks` stacks of `def`.
pub fn write_effects<W: BodyWorld>(w: &mut W, l: W::List, def: &StateDef, stacks: u16) {
    for e in &def.effects {
        w.list_set(l, i32::from(e.stat), StateDef::effect_value(e, stacks));
    }
}

/// The duration of the application in ticks: the state's own when its rules
/// give one, else the request's (already in ticks).
fn duration<W: BodyWorld>(w: &W, def: &StateDef, requested: i32) -> i32 {
    match def.duration_base_ticks() {
        Some(base) => UseWorld::tick_rate(w).ticks(base),
        None => requested,
    }
}

/// [`super::helpers::apply_state_ticks`] for a state with stack rules, after
/// its target checks. Returns the state's list, `None` when the list could
/// not be made.
pub fn apply_stacked<W: BodyWorld>(
    w: &mut W,
    def: &StateDef,
    q: StateRequest<W::Unit>,
) -> Option<W::List> {
    let (tg, s) = (q.target, q.state);
    let dur = duration(w, def, q.duration);
    let frame = w.frame();
    if let Some(o) = w.state_list(tg, s) {
        // A list the rules did not make (the state was put on another way)
        // counts as one stack.
        let current = w.list_stacks(o).max(1);
        if !def.reapply_restarts_clock() {
            return Some(o);
        }
        if dur != 0 {
            let e = frame.wrapping_add(dur);
            w.set_list_expire(o, e);
            w.combat().schedule_timer(tg, 12, e);
        }
        let next = def.stacks_after_apply(current).unwrap_or(current);
        if next != w.list_stacks(o) {
            w.set_list_stacks(o, next);
            write_effects(w, o, def, next);
            // The client gets the new count and values with the next update.
            w.mark_state_changed(tg, s);
        }
        return Some(o);
    }
    w.queue_update(tg);
    w.state_on(tg, s, true);
    let mut e = 0;
    let mut flags = 0;
    if dur != 0 {
        e = frame.wrapping_add(dur);
        w.combat().schedule_timer(tg, 12, e);
        flags |= 2;
    }
    if w.state_flag(s, group::EXP) {
        flags |= 0x800;
    }
    if w.state_is_aura(s) {
        flags |= 8;
    }
    let l = w.alloc_list(flags, e, Some(q.source))?;
    w.set_list_state(l, s);
    w.set_list_skill(l, q.skill, q.level);
    if q.stat != -1 {
        w.list_set(l, q.stat, q.value);
    }
    w.set_list_stacks(l, 1);
    write_effects(w, l, def, 1);
    w.attach(tg, l);
    let cb = if q.callback == 0 {
        callback::DEFAULT
    } else {
        q.callback
    };
    w.set_remove_callback(l, cb);
    if def.watched {
        w.state_started(tg, s);
    }
    Some(l)
}
