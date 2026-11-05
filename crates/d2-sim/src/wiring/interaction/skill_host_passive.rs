//! Passive refresh writes real layered state lists without a cast.
use super::skill_use::UseView;
use super::UseRest;
use crate::rules::StateStat;
use crate::skills::use_::bodies::BodyWorld;
use crate::units::UnitId;
use crate::wiring::action::Pending;
pub(super) fn values<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    u: UnitId,
    state: u16,
    skill: i32,
    level: i32,
    stats: &[StateStat],
) {
    let state = i32::from(state);
    if state <= 0 || state >= w.state_count() {
        return;
    }
    let found = w.state_list(u, state);
    if level <= 0 {
        if let Some(l) = found {
            w.detach_free(u, l);
        }
        return;
    }
    let l = match found {
        Some(l) => l,
        None => {
            let Some(l) = w.alloc_list(0, 0, Some(u)) else {
                return;
            };
            w.set_list_state(l, state);
            w.attach(u, l);
            l
        }
    };
    if w.list_get(l, 351) == level {
        return;
    }
    for s in stats {
        w.list_set_layer(l, i32::from(s.stat), s.value, s.layer);
    }
    w.list_set(l, 350, skill);
    w.list_set(l, 351, level);
    w.mark_state_changed(u, state);
    w.queue_update(u);
}
