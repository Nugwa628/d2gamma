//! A mod game's skill reset and resurrection hooks on the skill view:
//! [`SkillRespec`] on [`UseView`], and the two calls that hand the view to
//! the mod's [`crate::rules::SkillProvider`] ([`respec`] in place of
//! D2RUST's own skill reset, [`resurrected`] after a player's
//! resurrection). A host whose [`Pending`] value also implements
//! [`UseRest`] routes [`Pending::mod_respec`] and
//! [`Pending::mod_resurrected`] here.

use super::skill_use::UseView;
use super::UseRest;
use crate::rules::SkillRespec;
use crate::skills::use_::bodies::BodyWorld;
use crate::skills::use_::UseWorld;
use crate::units::hooks::Sim;
use crate::units::UnitId;
use crate::wiring::action::combat::CombatView;
use crate::wiring::action::{ActionHooks, Pending, View};

impl<X: Pending + UseRest> SkillRespec for UseView<'_, X> {
    fn hard_points(&self, u: UnitId, skill: i32) -> i32 {
        match self.cv.v.h.skill_lists.get(&u) {
            Some(l) => l.native(skill).map_or(0, |i| l.entries[i].base),
            None => self.find_entry(u, skill).map_or(0, |e| e.base),
        }
    }

    fn remove_skill(&mut self, u: UnitId, skill: i32) {
        if let Some(l) = self.cv.v.h.skill_lists.get_mut(&u) {
            if let Some(i) = l.native(skill) {
                l.remove(i);
            }
        }
    }

    fn skills_changed(&mut self, u: UnitId) {
        self.skill_resync(u);
        self.queue_update(u);
    }

    fn skill_states(&self, u: UnitId) -> Vec<(u16, i32)> {
        let stats = &*self.cv.v.stats;
        let Some(main) = stats.unit_list(u) else {
            return Vec::new();
        };
        stats
            .active_chain(main)
            .into_iter()
            .chain(stats.parked_chain(main))
            .filter_map(|l| {
                let state = u16::try_from(stats.state(l)).ok()?;
                let skill = stats.skill(l).0 as i32;
                (state != 0 && skill != 0).then_some((state, skill))
            })
            .collect()
    }
}

/// Runs `f` on the skill view of the action hooks and the units.
fn on_view<X: Pending + UseRest, T>(
    h: &mut ActionHooks<X>,
    sim: &mut Sim<'_>,
    f: impl FnOnce(&mut UseView<'_, X>) -> T,
) -> T {
    let mut w = UseView {
        cv: CombatView {
            game: &mut *sim.game,
            v: View::of(&mut *sim.units, &mut *sim.stats, sim.data, h),
        },
    };
    f(&mut w)
}

/// The mod game's skill reset of `player`, asked before D2RUST's own (the
/// NPC respec and the reset item): true when the mod did it. False under
/// D2RUST's own rules.
pub fn respec<X: Pending + UseRest>(
    h: &mut ActionHooks<X>,
    sim: &mut Sim<'_>,
    player: UnitId,
) -> bool {
    let rules = h.rules.clone();
    let Some(p) = rules.skills() else {
        return false;
    };
    on_view(h, sim, |w| p.respec(w, player))
}

/// Tells the mod game `player` was resurrected (after D2RUST's own
/// resurrection: passive states back on, life, mana and stamina full, in
/// town). Nothing under D2RUST's own rules.
pub fn resurrected<X: Pending + UseRest>(
    h: &mut ActionHooks<X>,
    sim: &mut Sim<'_>,
    player: UnitId,
) {
    let rules = h.rules.clone();
    if let Some(p) = rules.skills() {
        on_view(h, sim, |w| p.resurrected(w, player));
    }
}
