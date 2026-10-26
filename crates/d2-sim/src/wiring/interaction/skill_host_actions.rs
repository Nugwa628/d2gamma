//! [`SkillActions`] on the skill view: the world actions a mod game's
//! skill bodies get, on the unit placement, auto pickup, portal pair and
//! town portal code, the unit sound and "can't do that" message, the aura
//! callback D2RUST's own aura bodies run, and D2RUST's own bodies by slot.

use super::skill_use::UseView;
use super::UseRest;
use crate::rules::{AuraPulse, SkillActions, SkillFn};
use crate::skills::use_::bodies::dos::{aura_unit, AuraCtx};
use crate::skills::use_::bodies::{self, BodyEffect, BodyWorld};
use crate::skills::use_::{table, ServerMsg, UseWorld};
use crate::units::UnitId;
use crate::wiring::action::Pending;

impl<X: Pending + UseRest> SkillActions for UseView<'_, X> {
    fn teleport(&mut self, u: UnitId, to: (i32, i32)) -> bool {
        self.place_unit(u, None, to)
    }

    fn auto_pickup(&mut self, u: UnitId, item: UnitId) {
        self.effect(BodyEffect::AutoPickup { u, item });
    }

    fn open_portal(&mut self, u: UnitId, level: u32, class: u32) -> bool {
        let at = self.position(u);
        let cv = &mut self.cv;
        let room = cv.game.lists.unit(u).and_then(|e| e.room());
        let (made, _) =
            cv.v.create_portal_pair(&mut *cv.game, Some(u), room, at, level, class, false, false);
        made == 1
    }

    fn open_town_portal(&mut self, u: UnitId) -> bool {
        let cv = &mut self.cv;
        cv.v.town_portal_cast(&mut *cv.game, u).0 == 1
    }

    fn unit_sound(&mut self, u: UnitId, id: i32) {
        self.effect(BodyEffect::Sound { u, id });
    }

    fn cant_do(&mut self, u: UnitId) {
        self.send(u, ServerMsg::CantDo);
    }

    fn aura_pulse(&mut self, u: UnitId, pulse: &AuraPulse<'_>) -> i32 {
        let t = self.cv.v.h.tables.clone();
        let mut cx = AuraCtx {
            source_state: i32::from(pulse.state),
            skill: pulse.skill,
            lvl: pulse.level,
            duration: self.tick_rate().ticks(pulse.frames),
            stats: [0; 6],
            values: [0; 6],
            count: 0,
            list: None,
            passivestate: 0,
            callback: 0,
        };
        for (i, &(s, v)) in pulse.stats.iter().take(6).enumerate() {
            cx.stats[i] = i32::from(s);
            cx.values[i] = v;
        }
        // The caster first, as an aura's own state; then the units around.
        aura_unit(self, &t.skills, &t.combat, u, &mut cx, u);
        let mut cb = |w: &mut Self, x: UnitId| aura_unit(w, &t.skills, &t.combat, u, &mut cx, x);
        bodies::scan_unit(
            self,
            &t.skills,
            &t.combat,
            u,
            (0, 0),
            pulse.range,
            pulse.filter,
            true,
            &mut cb,
        );
        cx.count
    }

    fn engine_body(
        &mut self,
        kind: SkillFn,
        index: u16,
        u: UnitId,
        skill: i32,
        level: i32,
    ) -> Option<i32> {
        let t = self.cv.v.h.tables.clone();
        match kind {
            SkillFn::Start if index < table::START_SLOTS => Some(
                match bodies::run_start(self, &t.skills, &t.combat, index, u, skill, level) {
                    Some(v) => v,
                    None => self.xm().srvst(index, u, skill, level),
                },
            ),
            SkillFn::Do if index < table::DO_SLOTS => Some(
                match bodies::run_do(self, &t.skills, &t.combat, index, u, skill, level) {
                    Some(v) => v,
                    None => self.xm().srvdo(index, u, skill, level, false, false, false),
                },
            ),
            _ => None,
        }
    }
}
