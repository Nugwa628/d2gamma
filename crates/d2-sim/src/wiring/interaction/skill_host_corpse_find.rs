//! Corpse finding uses the real corpse selectors, update queue and item economy.
use super::skill_use::UseView;
use super::UseRest;
use crate::skills::use_::bodies::helpers2::st;
use crate::skills::use_::bodies::{self, BodyEffect, BodyWorld};
use crate::skills::SkillUnits;
use crate::units::UnitId;
use crate::wiring::action::Pending;
pub(super) fn consume<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    corpse: UnitId,
    soft: bool,
) -> bool {
    let t = w.cv.v.h.tables.clone();
    let eligible = if soft {
        bodies::starts2::corpse_soft(w, &t.combat, corpse)
    } else {
        bodies::starts2::corpse_any(w, &t.combat, corpse)
    };
    if !eligible || w.has_state(corpse, st::CORPSE_NOSELECT as u16) {
        return false;
    }
    w.state_on(corpse, st::CORPSE_NOSELECT, true);
    w.queue_update(corpse);
    true
}
pub(super) fn item<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    source: UnitId,
    corpse: UnitId,
    quality: i32,
) {
    w.effect(BodyEffect::TreasureDrop {
        corpse,
        killer: source,
        q: quality,
    });
}
pub(super) fn potion<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    source: UnitId,
    corpse: UnitId,
    bands: [i32; 2],
) {
    if let Some(code) = bodies::helpers3::potion_code_with_bands(w, source, bands) {
        w.effect(BodyEffect::DropItem {
            at: corpse,
            code,
            quality: 2,
        });
    }
}
