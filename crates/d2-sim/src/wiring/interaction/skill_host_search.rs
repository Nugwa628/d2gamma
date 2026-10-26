//! [`SkillSearch`] on the skill view: the unit search and AI seams a mod
//! game's skill bodies get, on the same room scan, unit filter, line test,
//! curse test, state gate and AI state D2RUST's own skill bodies use
//! ([`crate::skills::use_::bodies`]).

use super::skill_use::UseView;
use super::UseRest;
use crate::combat::CombatWorld;
use crate::rules::SkillSearch;
use crate::skills::use_::bodies::{self, BodyWorld};
use crate::skills::use_::UseWorld;
use crate::skills::SkillUnits;
use crate::units::{UnitId, UnitType};
use crate::wiring::action::Pending;

/// The squared distance between two points (wrapping, as the scans
/// compute it).
fn dist2((ax, ay): (i32, i32), (bx, by): (i32, i32)) -> i32 {
    let (dx, dy) = (ax.wrapping_sub(bx), ay.wrapping_sub(by));
    dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy))
}

impl<X: Pending + UseRest> SkillSearch for UseView<'_, X> {
    fn units_near(&self, near: UnitId, at: (i32, i32), radius: i32) -> Vec<UnitId> {
        let Some(rooms) = self.scan_rooms(near, Some(at)) else {
            return Vec::new();
        };
        let r2 = radius.wrapping_mul(radius);
        rooms
            .into_iter()
            .flat_map(|r| r.units)
            .filter(|&u| dist2(self.position(u), at) <= r2)
            .collect()
    }

    fn passes_filter(&mut self, source: UnitId, target: UnitId, filter: u32) -> bool {
        let t = self.cv.v.h.tables.clone();
        bodies::accepts(self, &t.skills, &t.combat, source, target, filter)
    }

    fn los_blocked(&self, a: UnitId, b: UnitId, mask: u32) -> bool {
        !self.line_clear(a, self.position(b), mask)
    }

    fn curse_allowed(&mut self, u: UnitId) -> bool {
        if self.cv.alignment(u) == 1 {
            return false;
        }
        if SkillUnits::unit_type(self, u) == UnitType::Monster
            && (!self.cv.monster_has_mode(u, 2) || self.cv.monster_flag(u, 0x20))
        {
            return false;
        }
        self.unit_flags(u) & 0xE == 0xE && self.is_alive(u)
    }

    fn accepts_aura(&self, u: UnitId) -> bool {
        if SkillUnits::unit_type(self, u) != UnitType::Monster {
            return true;
        }
        let ct = &self.cv.v.h.tables.combat;
        let class = SkillUnits::class_id(self, u);
        ct.monstats(class).is_some_and(|m| !m.npc) && ct.monstats2(class).is_some_and(|m| m.isatt)
    }

    fn notify_ai(&mut self, u: UnitId, state: i32) {
        self.set_ai_state(u, state);
    }
}
