//! Small aura primitives keep state callbacks, period alignment and filters native.
use super::skill_use::UseView;
use super::UseRest;
use crate::combat::CombatWorld;
use crate::rules::AuraPulse;
use crate::skills::use_::bodies;
use crate::skills::use_::bodies::dos::{aura_unit, AuraCtx};
use crate::skills::use_::UseWorld;
use crate::skills::SkillUnits;
use crate::units::{UnitId, UnitType};
use crate::wiring::action::Pending;

pub(super) fn state<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    source: UnitId,
    target: UnitId,
    p: &AuraPulse<'_>,
    freeze: bool,
) {
    let t = w.cv.v.h.tables.clone();
    let mut cx = AuraCtx {
        source_state: i32::from(p.state),
        skill: p.skill,
        lvl: p.level,
        duration: p.frames,
        stats: [-1; 6],
        values: [0; 6],
        count: 0,
        list: None,
        passivestate: 0,
        callback: if freeze {
            bodies::callback::HOLY_FREEZE
        } else {
            0
        },
    };
    for (i, &(s, v)) in p.stats.iter().take(6).enumerate() {
        cx.stats[i] = i32::from(s);
        cx.values[i] = v;
    }
    aura_unit(w, &t.skills, &t.combat, source, &mut cx, target);
}
pub(super) fn targets<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    u: UnitId,
    radius: i32,
    filter: u32,
) -> Vec<UnitId> {
    let t = w.cv.v.h.tables.clone();
    let mut found = Vec::new();
    bodies::scan_unit(
        w,
        &t.skills,
        &t.combat,
        u,
        (0, 0),
        radius,
        filter,
        false,
        &mut |_, x| {
            found.push(x);
            1
        },
    );
    found
}
pub(super) fn period<X: Pending + UseRest>(w: &UseView<'_, X>, base: i32) -> i32 {
    let d = w.tick_rate().ticks(base.max(5));
    let f = w.frame();
    (f.wrapping_add(d).wrapping_sub(1) / d)
        .wrapping_mul(d)
        .wrapping_add(1)
        .wrapping_sub(f)
        .wrapping_add(1)
}
pub(super) fn freeze_target<X: Pending + UseRest>(w: &UseView<'_, X>, u: UnitId) -> bool {
    if SkillUnits::unit_type(w, u) != UnitType::Monster {
        return true;
    }
    let diff = w.cv.difficulty();
    w.cv.v
        .h
        .tables
        .combat
        .monstats(SkillUnits::class_id(w, u))
        .is_some_and(|m| {
            (match diff {
                0 => m.coldeffect,
                1 => m.coldeffect_n,
                _ => m.coldeffect_h,
            } as i8)
                < 0
        })
}
