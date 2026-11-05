//! Data-selected placed sentries share pet ownership and the engine's mode requests.
use super::skill_use::UseView;
use super::UseRest;
use crate::rules::SentrySetup;
use crate::skills::use_::bodies::{self, BodyEffect, BodyWorld, Summon};
use crate::skills::use_::UseWorld;
use crate::units::{UnitId, UnitType};
use crate::wiring::action::Pending;

pub(super) fn place<X: Pending + UseRest>(
    w: &mut UseView<'_, X>,
    caster: UnitId,
    p: &SentrySetup,
) -> Option<UnitId> {
    if usize::from(p.class) >= w.cv.v.h.tables.combat.monstats.len() {
        return None;
    }
    let mut owner = caster;
    let mut seen = std::collections::BTreeSet::new();
    while w.cv.v.units.get(owner)?.ty == UnitType::Monster {
        if !seen.insert(owner) {
            return None;
        }
        owner = w.minion_owner(owner)?;
    }
    let (mut x, mut y) = p.at;
    if x == 0 || y == 0 {
        (x, y) = UseWorld::position(w, caster);
    }
    if x == 0 || y == 0 {
        (x, y) = w.target_position(owner)?;
    }
    let room = w.unit_room(owner).and_then(|r| w.room_at(r, x, y))?;
    if !p.in_town && w.room_in_town(room) {
        return None;
    }
    let pt = if i32::from(p.pet_type) < w.pettype_count() {
        i32::from(p.pet_type)
    } else {
        0
    };
    let pet = bodies::spawn(
        w,
        Summon {
            flags: 1,
            owner,
            class: i32::from(p.class),
            ai: 0,
            mode: i32::from(p.mode),
            x,
            y,
            pet_type: pt,
            pet_max: p.cap,
        },
    )?;
    bodies::base_stats(w, owner, pet, 0, p.level);
    let t = w.cv.v.h.tables.clone();
    bodies::skill_stats(w, &t.skills, owner, pet, p.skill, p.level, 0);
    w.effect(BodyEffect::Alignment { u: pet, a: 2, v: 1 });
    w.mode_request(pet, i32::from(p.mode), None);
    Some(pet)
}
