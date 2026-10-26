//! [`HostDamage`] on the skill use view.

use crate::combat::events::{self, EventTables};
use crate::combat::hit::BlockResult;
use crate::combat::DamageRecord;
use crate::combat::{damage, hit};
use crate::rules::{ElementAdded, HostDamage};
use crate::skills::levels;
use crate::units::UnitId;
use crate::wiring::action::Pending;
use crate::wiring::interaction::{UseRest, UseView};

impl<X: Pending + UseRest> HostDamage for UseView<'_, X> {
    fn hit_roll(&mut self, a: UnitId, d: UnitId, bonus: i32, missile: bool) -> bool {
        let t = self.cv.v.h.tables.clone();
        hit::hit_test(
            &mut self.cv,
            &t.skills,
            &t.combat,
            Some(a),
            Some(d),
            bonus,
            missile,
        )
    }
    fn defend_roll(&mut self, a: UnitId, d: UnitId, avoid: bool, block: bool) -> BlockResult {
        let t = self.cv.v.h.tables.clone();
        hit::block_or_dodge(&mut self.cv, &t.combat, a, d, avoid, block)
    }
    fn melee_result(&mut self, a: UnitId, d: UnitId, bonus: i32, range_offset: i32) -> u16 {
        let t = self.cv.v.h.tables.clone();
        hit::melee_result(
            &mut self.cv,
            &t.skills,
            &t.combat,
            Some(a),
            Some(d),
            bonus,
            range_offset,
        )
    }
    fn roll_skill_physical(&mut self, a: UnitId, skill: i32, lvl: i32, rec: &mut DamageRecord) {
        let t = self.cv.v.h.tables.clone();
        levels::roll_physical(self, &t.skills, a, rec, skill, lvl);
    }
    fn roll_skill_element(
        &mut self,
        a: UnitId,
        skill: i32,
        lvl: i32,
        rec: &mut DamageRecord,
    ) -> i32 {
        let t = self.cv.v.h.tables.clone();
        levels::roll_elemental(self, &t.skills, a, rec, skill, lvl)
    }
    fn add_element(
        &mut self,
        a: UnitId,
        rec: &mut DamageRecord,
        element: i32,
        value: i32,
        len: i32,
    ) -> ElementAdded {
        levels::add_element(self, a, rec, element, value, len)
    }
    fn fill_damage(
        &mut self,
        a: UnitId,
        d: UnitId,
        rec: &mut DamageRecord,
        offhand: bool,
        src_dam: i32,
    ) {
        let t = self.cv.v.h.tables.clone();
        damage::fill(
            &mut self.cv,
            &t.skills,
            &t.combat,
            a,
            d,
            rec,
            offhand,
            src_dam,
        );
    }
    fn start_combat(&mut self, a: UnitId, d: UnitId, rec: &mut DamageRecord, src_dam: i32) {
        let t = self.cv.v.h.tables.clone();
        damage::start_combat(
            &mut self.cv,
            &t.skills,
            &t.combat,
            Some(a),
            Some(d),
            rec,
            src_dam,
        );
    }
    fn apply_queued(&mut self, a: UnitId, d: UnitId) {
        let t = self.cv.v.h.tables.clone();
        damage::apply_melee(&mut self.cv, &t.combat, a, d);
    }
    fn apply_damage(&mut self, a: UnitId, d: UnitId, missile: bool, rec: &mut DamageRecord) {
        let t = self.cv.v.h.tables.clone();
        damage::apply(&mut self.cv, &t.combat, a, d, missile, rec);
    }
    fn unit_event(
        &mut self,
        event: u8,
        u: UnitId,
        other: Option<UnitId>,
        rec: Option<&mut DamageRecord>,
    ) -> i32 {
        let t = self.cv.v.h.tables.clone();
        let tb = EventTables {
            skills: &t.skills,
            combat: &t.combat,
        };
        events::run(self, tb, event, Some(u), other, rec)
    }
}
