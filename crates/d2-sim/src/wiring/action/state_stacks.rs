//! The expiry of states with stack rules (the mod kit's `states.toml`): a
//! state whose stacks decay one at a time keeps its list when its duration
//! runs out, with one stack fewer and the duration started over, until the
//! last stack is gone and the ordinary expiry frees the list.

use crate::rules::Decay;
use crate::units::hooks::Sim;
use crate::units::UnitId;

use super::{ActionHooks, Pending};

impl<X: Pending> ActionHooks<X> {
    /// Gives every list of `unit` that is due at `frame` and loses one stack
    /// at a time its next stack count, effect values and expiry. Runs before
    /// the expiry walk, which then finds the list no longer due.
    pub(crate) fn decay_stacks(&mut self, sim: &mut Sim<'_>, unit: UnitId, frame: i32) {
        let rules = self.rules.clone();
        let Some(rules) = rules.state_rules() else {
            return;
        };
        for l in sim.stats.due_state_lists(unit, frame) {
            let state = sim.stats.state(l);
            let Some(def) = i32::try_from(state).ok().and_then(|s| rules.get(s)) else {
                continue;
            };
            let current = sim.stats.stacks(l);
            if def.decay != Decay::LoseOne || current == 0 {
                continue;
            }
            let left = def.stacks_after_expiry(current);
            let Some(base) = def.duration_base_ticks().filter(|_| left > 0) else {
                // The last stack: the ordinary expiry frees the list.
                continue;
            };
            sim.stats.set_stacks(l, left);
            for e in &def.effects {
                let v = crate::rules::StateDef::effect_value(e, left);
                sim.stats.set(self, l, e.stat, v, 0, None);
            }
            let until = frame.wrapping_add(sim.game.tick_rate.ticks(base));
            sim.stats.set_expire(l, until);
            if let Err(e) = sim.game.schedule_event(unit, 12, until, None, 0, 0) {
                self.errors.push(super::WiringError::Unit(
                    crate::units::modes::UnitError::Game(e),
                ));
            }
            sim.stats.set_state_changed(unit, state, true);
            let _ = sim.game.lists.queue_update(unit);
        }
    }
}
