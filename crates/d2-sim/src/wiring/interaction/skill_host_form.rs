//! Data-selected forms share the game's state group toggle and removal.
use super::skill_use::UseView;
use super::UseRest;
use crate::combat::CombatWorld;
use crate::rules::BuffPulse;
use crate::skills::use_::bodies::helpers::clear_group;
use crate::skills::use_::bodies::{self, BodyWorld};
use crate::skills::use_::set_delay;
use crate::skills::use_::UseWorld;
use crate::stats::ids as stat;
use crate::units::UnitId;
use crate::wiring::action::Pending;

pub(super) fn apply<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    u: UnitId,
    p: &BuffPulse<'_>,
    delay: i32,
) -> bool {
    let state = i32::from(p.state);
    if state >= w.state_count() {
        return false;
    }
    bodies::flag_40(w, u);
    if clear_group(w, u, state, true) {
        if delay > 0 {
            set_delay(w, u, delay);
        }
        return false;
    }
    if w.state_list(u, state).is_some() {
        return false;
    }
    let expires = bodies::after(w, p.frames);
    let Some(list) = w.alloc_list(2, expires, Some(u)) else {
        return false;
    };
    w.set_list_state(list, state);
    w.set_remove_callback(list, crate::wiring::action::units::MOD_FORM_REMOVED);
    w.attach(u, list);
    w.state_on(u, state, true);
    w.combat().schedule_timer(u, 12, expires);
    let stat_count = w.cv.v.h.tables.skills.stat_count;
    for &(stat, value) in p.stats {
        if i32::from(stat) < stat_count && value != 0 {
            w.list_set(list, i32::from(stat), value);
            if stat == stat::ATTACKRATE {
                w.list_set(list, i32::from(stat::OTHER_ANIMRATE), value);
            }
        }
    }
    w.combat().refresh_anim_rate(u);
    w.list_set(list, i32::from(stat::MODIFIERLIST_SKILL), p.skill);
    w.list_set(list, i32::from(stat::MODIFIERLIST_LEVEL), p.level);
    if let Some(entry) = w.find_entry(u, p.skill) {
        w.set_entry_mode(u, &entry, 10);
    }
    true
}

pub(super) fn removed<X: Pending + UseRest>(w: &mut UseView<'_, X>, u: UnitId, skill: i32) {
    if let Some(entry) = w.find_entry(u, skill) {
        let mode = w.disguise_mode(u, 10);
        w.set_entry_mode(u, &entry, mode);
    }
}
