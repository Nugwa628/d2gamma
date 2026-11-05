//! Parameterized defensive buffs for opted-in data skills.

use super::skill_use::UseView;
use super::UseRest;
use crate::combat::CombatWorld;
use crate::rules::BuffPulse;
use crate::skills::use_::bodies::helpers::{apply_state, clear_group, register, unregister};
use crate::skills::use_::bodies::{self, BodyWorld, StateRequest};
use crate::stats::ids as stat;
use crate::units::UnitId;
use crate::wiring::action::units::MOD_TIMED_BUFF_REMOVED;
use crate::wiring::action::Pending;

pub(super) fn apply<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    u: UnitId,
    p: &BuffPulse<'_>,
) -> bool {
    let state = i32::from(p.state);
    if state >= w.state_count() {
        return false;
    }
    let t = w.cv.v.h.tables.clone();
    bodies::flag_40(w, u);
    clear_group(w, u, state, true);
    let Some(list) = apply_state(
        w,
        &t.combat,
        StateRequest {
            source: u,
            target: u,
            skill: p.skill,
            level: p.level,
            duration: p.frames,
            stat: -1,
            value: 0,
            state,
            callback: MOD_TIMED_BUFF_REMOVED,
        },
    ) else {
        return false;
    };
    for &(s, value) in p.stats {
        if i32::from(s) < t.skills.stat_count && value != 0 {
            w.list_set(list, i32::from(s), value);
            if s == stat::ATTACKRATE {
                w.list_set(list, i32::from(stat::OTHER_ANIMRATE), value);
            }
        }
    }
    w.combat().refresh_anim_rate(u);
    w.list_set(list, i32::from(stat::MODIFIERLIST_SKILL), p.skill);
    w.list_set(list, i32::from(stat::MODIFIERLIST_LEVEL), p.level);
    if !p.events.is_empty() {
        unregister(w, u, 1, state);
        for &(event, callback) in p.events {
            register(
                w,
                u,
                i32::from(event),
                p.skill,
                p.level,
                i32::from(callback),
                1,
                state,
            );
        }
    }
    w.mark_state_changed(u, state);
    true
}
