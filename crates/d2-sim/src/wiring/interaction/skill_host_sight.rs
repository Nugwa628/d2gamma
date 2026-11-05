//! Shared state application and the caster-centered no-aura-aware scan.
use super::{skill_use::UseView, UseRest};
use crate::rules::CursePulse;
use crate::skills::use_::bodies::callback;
use crate::skills::use_::bodies::helpers::{apply_state, scan_unit, StateRequest};
use crate::units::UnitId;
use crate::wiring::action::Pending;
pub(super) fn targets<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    u: UnitId,
    r: i32,
    f: u32,
) -> Vec<UnitId> {
    let t = w.cv.v.h.tables.clone();
    let mut found = Vec::new();
    scan_unit(
        w,
        &t.skills,
        &t.combat,
        u,
        (0, 0),
        r,
        f,
        true,
        &mut |_, x| {
            if found.len() < crate::rules::data_mod::MAX_AREA_TARGETS {
                found.push(x);
            }
            1
        },
    );
    found
}
pub(super) fn state<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    u: UnitId,
    x: UnitId,
    p: &CursePulse<'_>,
) {
    let t = w.cv.v.h.tables.clone();
    let (stat, value) = p.stats[0];
    apply_state(
        w,
        &t.combat,
        StateRequest {
            source: u,
            target: x,
            skill: p.skill,
            level: p.level,
            duration: p.frames,
            stat: i32::from(stat),
            value,
            state: i32::from(p.state),
            callback: callback::DEFAULT,
        },
    );
}
