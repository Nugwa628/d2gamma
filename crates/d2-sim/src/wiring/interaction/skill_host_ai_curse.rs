//! Small AI eligibility, state-list and forced-target adapters.
use super::{skill_use::UseView, UseRest};
use crate::combat::{CombatWorld, RoomKind};
use crate::rules::CursePulse;
use crate::skills::use_::bodies::dos::can_switch;
use crate::skills::use_::bodies::helpers::{apply_state, scaled, scan_point, StateRequest};
use crate::skills::use_::bodies::helpers3::{leave_pack, node_prepend};
use crate::skills::use_::bodies::{self, callback, BodyEffect, BodyWorld};
use crate::skills::use_::UseWorld;
use crate::units::{UnitId, UnitType};
use crate::wiring::action::Pending;

pub(super) fn eligible<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    u: UnitId,
    x: UnitId,
    attract: bool,
) -> bool {
    let t = w.cv.v.h.tables.clone();
    crate::skills::SkillUnits::unit_type(w, x) == UnitType::Monster
        && w.combat().alignment(x) == 0
        && w.combat().hostile(u, x)
        && UseWorld::is_alive(w, x)
        && (!attract || UseWorld::room(w, x) != RoomKind::Town)
        && can_switch(w, &t.combat, x, if attract { 19 } else { 11 })
}
pub(super) fn duration<X: Pending + UseRest>(w: &mut UseView<'_, X>, frames: i32) -> i32 {
    let t = w.cv.v.h.tables.clone();
    let d = t
        .combat
        .difficulty(w.combat().difficulty())
        .map_or(0, |d| d.aicursedivisor as i32);
    if d == 0 {
        frames
    } else {
        frames / d
    }
}
pub(super) fn targets<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    u: UnitId,
    range: i32,
    filter: u32,
) -> Option<Vec<UnitId>> {
    let t = w.cv.v.h.tables.clone();
    let mut found = Vec::new();
    let scanned = scan_point(w, &t.skills, &t.combat, filter, u, range, &mut |_, x| {
        if found.len() < crate::rules::data_mod::MAX_AREA_TARGETS {
            found.push(x);
        }
        1
    });
    (scanned != 0).then_some(found)
}
pub(super) fn state<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    u: UnitId,
    x: UnitId,
    p: &CursePulse<'_>,
    attract: bool,
) -> bool {
    let t = w.cv.v.h.tables.clone();
    let Some(list) = apply_state(
        w,
        &t.combat,
        StateRequest {
            source: u,
            target: x,
            skill: p.skill,
            level: p.level,
            duration: p.frames,
            stat: -1,
            value: 0,
            state: i32::from(p.state),
            callback: if attract {
                callback::ATTRACT
            } else {
                callback::CONFUSE
            },
        },
    ) else {
        return false;
    };
    let mut update = false;
    for &(stat, value) in p.stats {
        update |= w
            .stat_info(i32::from(stat))
            .is_some_and(|s| s.updateanimrate);
        let value = scaled(w, x, i32::from(stat), value);
        if value != 0 {
            w.list_set(list, i32::from(stat), value);
        }
    }
    if update {
        w.combat().refresh_anim_rate(x);
    }
    true
}
pub(super) fn align<X: Pending + UseRest>(w: &mut UseView<'_, X>, x: UnitId, attract: bool) {
    w.effect(BodyEffect::Alignment { u: x, a: 1, v: 1 });
    node_prepend(w, x, 9);
    if attract {
        leave_pack(w, x);
    }
}
pub(super) fn redirect<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    x: UnitId,
    target: Option<UnitId>,
    frames: i32,
) {
    let (kind, guid) = target.map_or((3, 0), |t| {
        (
            if crate::skills::SkillUnits::unit_type(w, t) == UnitType::Player {
                1
            } else {
                2
            },
            bodies::helpers2::guid(w, t) as i32,
        )
    });
    w.effect(BodyEffect::TargetOverride { m: x, kind, guid });
    let expires = bodies::after(w, frames);
    w.schedule(x, 10, expires, 0, 0);
}
