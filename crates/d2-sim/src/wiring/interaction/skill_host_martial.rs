//! Charge-list lifecycle and dual-claw frame selection, without a skill-body dispatch.
use super::skill_use::UseView;
use super::UseRest;
use crate::rules::BuffPulse;
use crate::skills::use_::bodies::{callback, BodyWorld};
use crate::skills::use_::UseWorld;
use crate::units::UnitId;
use crate::wiring::action::Pending;

pub(super) fn claws<X: Pending + UseRest>(w: &mut UseView<'_, X>, u: UnitId) {
    let dual = BodyWorld::has_inventory(w, u)
        && match (
            crate::skills::SkillUnits::item_at(w, u, 4),
            crate::skills::SkillUnits::item_at(w, u, 5),
        ) {
            (Some(a), Some(b)) => {
                a != b
                    && BodyWorld::item_usable(w, a)
                    && BodyWorld::item_usable(w, b)
                    && crate::skills::SkillUnits::item_is(w, a, 45)
                    && crate::skills::SkillUnits::item_is(w, b, 45)
            }
            _ => false,
        };
    let flags = UseWorld::unit_flags(w, u);
    UseWorld::set_unit_flags(
        w,
        u,
        if dual && BodyWorld::frame_event_index(w, u) % 2 == 0 {
            flags & !0x40
        } else {
            flags | 0x40
        },
    );
}
pub(super) fn add<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    u: UnitId,
    counter: u16,
    p: &BuffPulse<'_>,
) {
    let expires = crate::skills::use_::bodies::after(w, p.frames);
    let list = match w.state_list(u, i32::from(p.state)) {
        Some(l) => l,
        None => {
            let Some(l) = w.alloc_list(2, expires, Some(u)) else {
                return;
            };
            w.set_list_state(l, i32::from(p.state));
            w.set_remove_callback(l, callback::CHARGE);
            w.attach(u, l);
            w.list_set(l, 350, p.skill);
            w.list_set(l, 351, p.level);
            l
        }
    };
    w.set_list_expire(list, expires);
    w.schedule(u, 12, expires, 0, 0);
    let n = w.list_get(list, i32::from(counter));
    let next = n.wrapping_add(1).min(3);
    if n == next {
        return;
    }
    w.list_set(list, i32::from(counter), next);
    for &(stat, value) in p.stats {
        w.list_add(list, i32::from(stat), value);
    }
    w.state_on(u, i32::from(p.state), true);
    w.mark_state_changed(u, i32::from(p.state));
}
