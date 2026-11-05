//! Small shadow stat-list adapter; equipment and timers use the shared host paths.
use super::skill_use::UseView;
use super::UseRest;
use crate::rules::StateStat;
use crate::skills::levels::ManaUnits;
use crate::skills::use_::bodies::{BodyEffect, BodyWorld};
use crate::stats::ids;
use crate::units::UnitId;
use crate::wiring::action::Pending;

pub(super) fn stats<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    pet: UnitId,
    level: i32,
    percent: i32,
    stats: &[StateStat],
    umod: Option<u8>,
) {
    if level <= 1 {
        return;
    }
    let hp = w.stat_max(pet, ids::HITPOINTS);
    let hp = hp.wrapping_add(crate::combat::pct(hp, percent, 100));
    w.set_stat(pet, ids::MAXHP, hp);
    w.set_stat(pet, ids::HITPOINTS, hp);
    let Some(list) = w.alloc_list(0, 0, Some(pet)) else {
        return;
    };
    w.attach(pet, list);
    for s in stats {
        w.list_set(list, i32::from(s.stat), s.value);
    }
    if let Some(umod) = umod {
        w.effect(BodyEffect::Umod {
            m: pet,
            umod: i32::from(umod),
            arg: 1,
        });
    }
}
