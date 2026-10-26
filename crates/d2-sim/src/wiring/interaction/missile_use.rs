//! A mod game's missile bodies on the skill use pipeline: the calls of
//! [`crate::missiles::MissileBodies`] that need the skill use view
//! ([`UseView`]) reach it through [`Pending::missile_use`]. A seam value
//! that also implements [`UseRest`] routes it here:
//!
//! ```text
//! fn missile_use(h: &mut ActionHooks<Self>, sim: &mut Sim<'_>, call: MissileUse) -> bool {
//!     d2_sim::wiring::interaction::missile_use::run(h, sim, call)
//! }
//! ```

use crate::combat::events::{self, EventTables};
use crate::missiles::CastAt;
use crate::units::hooks::Sim;
use crate::wiring::action::combat::CombatView;
use crate::wiring::action::{ActionHooks, MissileUse, Pending, View};

use super::{UseRest, UseView};

/// Runs `call` on the skill use view: the unit filter's answer, or
/// whether the cast happened.
pub fn run<X: Pending + UseRest>(
    h: &mut ActionHooks<X>,
    sim: &mut Sim<'_>,
    call: MissileUse,
) -> bool {
    let t = h.tables.clone();
    let mut w = UseView {
        cv: CombatView {
            game: sim.game,
            v: View::of(sim.units, sim.stats, sim.data, h),
        },
    };
    match call {
        MissileUse::Accepts {
            source,
            unit,
            filter,
        } => crate::skills::use_::bodies::helpers::accepts(
            &mut w, &t.skills, &t.combat, source, unit, filter,
        ),
        MissileUse::Cast { caster, cast } => {
            let tb = EventTables {
                skills: &t.skills,
                combat: &t.combat,
            };
            let (s, l) = (cast.skill, cast.level);
            let r = match cast.at {
                CastAt::Unit(u) => events::cast(&mut w, tb, Some(caster), s, l, Some(u), cast.aim),
                CastAt::Point(x, y) => {
                    events::cast_point(&mut w, tb, Some(caster), s, l, (x, y), cast.aim)
                }
            };
            r != 0
        }
    }
}
