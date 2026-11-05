//! State effects supplied by a data warcry, on its caster or a missile hit.
use super::skill_use::UseView;
use super::UseRest;
use crate::combat::CombatWorld;
use crate::rules::BuffPulse;
use crate::skills::use_::bodies::{self, callback, BodyWorld};
use crate::units::UnitId;
use crate::wiring::action::Pending;

pub(super) fn apply<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    source: UnitId,
    target: UnitId,
    p: &BuffPulse<'_>,
) -> bool {
    let state = i32::from(p.state);
    if state >= w.state_count() {
        return false;
    }
    let expires = bodies::after(w, p.frames);
    let list = match w.state_list(target, state) {
        Some(list) => list,
        None => {
            let Some(list) = w.alloc_list(2, expires, Some(source)) else {
                return false;
            };
            w.set_list_state(list, state);
            w.set_remove_callback(list, callback::DEFAULT);
            w.attach(target, list);
            w.state_on(target, state, true);
            list
        }
    };
    w.mark_state_changed(target, state);
    w.set_list_expire(list, expires);
    w.combat().schedule_timer(target, 12, expires);
    let stat_count = w.cv.v.h.tables.skills.stat_count;
    for &(stat, value) in p.stats {
        if i32::from(stat) < stat_count && value != 0 {
            w.list_set(list, i32::from(stat), value);
        }
    }
    w.buff_refresh(target);
    true
}
