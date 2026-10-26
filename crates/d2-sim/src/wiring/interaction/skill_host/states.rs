//! [`HostStates`] on the skill use view.

use crate::combat::CombatWorld;
use crate::rules::{HostStates, StateRemove, StateSpec, StateStat};
use crate::skills::use_::bodies::{self, callback, group, BodyWorld};
use crate::skills::SkillUnits;
use crate::stats::lists::ListId;
use crate::stats::{key_layer, key_stat};
use crate::units::UnitId;
use crate::wiring::action::units::MOD_STATE_REMOVED;
use crate::wiring::action::Pending;
use crate::wiring::interaction::{UseRest, UseView};

/// The remove callback id of `r`.
fn callback_of(r: StateRemove) -> u32 {
    match r {
        StateRemove::Default => callback::DEFAULT,
        StateRemove::Buff => callback::BUFF,
        StateRemove::StateOff => callback::CHARGE,
        StateRemove::Mod => MOD_STATE_REMOVED,
    }
}

/// Runs the remove callback `cb` of the unit's list `l` of `state`: the
/// mod game's for [`MOD_STATE_REMOVED`], else the skill bodies'
/// ([`bodies::remove_callback`]).
pub(in crate::wiring::interaction) fn run_remove_callback<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    u: UnitId,
    state: i32,
    cb: u32,
    l: ListId,
) {
    if cb == MOD_STATE_REMOVED {
        let rules = w.cv.v.h.rules.clone();
        if let (Some(p), Ok(s)) = (rules.skills(), u16::try_from(state)) {
            p.state_removed(w, u, s);
        }
        return;
    }
    let t = w.cv.v.h.tables.clone();
    bodies::remove_callback(w, &t.skills, &t.combat, u, state, cb, l);
}

fn entries(e: Vec<(i32, i32)>) -> Vec<StateStat> {
    e.into_iter()
        .map(|(k, value)| StateStat {
            stat: key_stat(k),
            layer: key_layer(k),
            value,
        })
        .collect()
}

impl<X: Pending + UseRest> HostStates for UseView<'_, X> {
    fn set_stat_layer(&mut self, u: UnitId, stat: u16, layer: u16, value: i32) {
        let v = &mut self.cv.v;
        v.stats.unit_set(&mut *v.h, u, stat, value, layer);
    }
    fn base_stat_entries(&self, u: UnitId) -> Vec<StateStat> {
        let st = &self.cv.v.stats;
        st.unit_list(u)
            .map_or_else(Vec::new, |l| entries(st.base_entries(l)))
    }
    /// The full array of the unit's list; a plain list's base.
    fn stat_entries(&self, u: UnitId) -> Vec<StateStat> {
        let st = &self.cv.v.stats;
        let Some(l) = st.unit_list(u) else {
            return Vec::new();
        };
        let full = st.full_entries(l);
        entries(if full.is_empty() {
            st.base_entries(l)
        } else {
            full
        })
    }
    fn state_stat(&self, u: UnitId, state: u16, stat: u16) -> Option<i32> {
        SkillUnits::state_stat(self, u, state, stat)
    }
    fn has_state_list(&self, u: UnitId, state: u16) -> bool {
        BodyWorld::state_list(self, u, i32::from(state)).is_some()
    }
    /// A new list is made as the skill bodies' state apply makes one:
    /// flags 2 when it expires, 0x800 for a state of the experience
    /// group, 8 for an aura state; an expiry schedules the list's type-12
    /// check at that frame.
    fn apply_state(&mut self, u: UnitId, spec: &StateSpec) -> bool {
        let s = i32::from(spec.state);
        if !bodies::state_ok(self, s) {
            return false;
        }
        let l = match BodyWorld::state_list(self, u, s) {
            Some(l) => l,
            None => {
                let mut flags = 0;
                if spec.expire.is_some() {
                    flags |= 2;
                }
                if BodyWorld::state_flag(self, s, group::EXP) {
                    flags |= 0x800;
                }
                if BodyWorld::state_is_aura(self, s) {
                    flags |= 8;
                }
                BodyWorld::queue_update(self, u);
                BodyWorld::state_on(self, u, s, true);
                let Some(l) =
                    BodyWorld::alloc_list(self, flags, spec.expire.unwrap_or(0), spec.source)
                else {
                    return false;
                };
                BodyWorld::set_list_state(self, l, s);
                BodyWorld::attach(self, u, l);
                BodyWorld::set_remove_callback(self, l, callback_of(spec.on_remove));
                l
            }
        };
        BodyWorld::set_list_skill(self, l, spec.skill, spec.level);
        if let Some(e) = spec.expire {
            BodyWorld::set_list_expire(self, l, e);
            self.combat().schedule_timer(u, 12, e);
        }
        for st in &spec.stats {
            BodyWorld::list_set_layer(self, l, i32::from(st.stat), st.value, st.layer);
        }
        if spec.markers {
            BodyWorld::list_set(self, l, bodies::passive::PASSIVE_SKILL, spec.skill);
            BodyWorld::list_set(self, l, bodies::passive::PASSIVE_LEVEL, spec.level);
        }
        BodyWorld::queue_update(self, u);
        true
    }
    fn clear_state(&mut self, u: UnitId, state: u16) {
        let s = i32::from(state);
        if let Some(l) = BodyWorld::state_list(self, u, s) {
            BodyWorld::detach_free(self, u, l);
        }
        BodyWorld::state_on(self, u, s, false);
    }
    fn clear_state_group(&mut self, u: UnitId, state: u16, include_self: bool) -> bool {
        bodies::clear_group(self, u, i32::from(state), include_self)
    }
    fn state_on(&mut self, u: UnitId, state: u16, on: bool) {
        BodyWorld::state_on(self, u, i32::from(state), on);
    }
    /// Keyed by the state (key type 1), as the default remove callback
    /// unregisters them.
    fn add_state_event(
        &mut self,
        u: UnitId,
        event: u8,
        func: i32,
        skill: i32,
        lvl: i32,
        state: u16,
    ) -> bool {
        bodies::register(
            self,
            u,
            i32::from(event),
            skill,
            lvl,
            func,
            1,
            i32::from(state),
        )
    }
    fn clear_state_events(&mut self, u: UnitId, state: u16) {
        bodies::unregister(self, u, 1, i32::from(state));
    }
}
