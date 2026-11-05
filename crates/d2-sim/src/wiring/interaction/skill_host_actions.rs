//! [`SkillActions`] on the skill view: the world actions a mod game's
//! skill bodies get, on the unit placement, auto pickup, portal pair and
//! town portal code, the unit sound and "can't do that" message, the aura
//! callback D2RUST's own aura bodies run, and D2RUST's own bodies by slot.

use super::skill_use::UseView;
use super::UseRest;
use crate::combat::CombatWorld;
use crate::rules::{AuraPulse, CursePulse, SkillActions, SkillFn};
use crate::skills::use_::bodies::dos::{aura_unit, curse_pulse, AuraCtx};
use crate::skills::use_::bodies::{self, BodyEffect, BodyWorld};
use crate::skills::use_::{table, ServerMsg, UseWorld};
use crate::skills::SkillUnits;
use crate::units::{UnitId, UnitType};
use crate::wiring::action::Pending;

impl<X: Pending + UseRest> SkillActions for UseView<'_, X> {
    fn sight_rows_valid(&self, state: u16, stat: u16) -> bool {
        i32::from(state) < self.state_count()
            && i32::from(stat) < self.cv.v.h.tables.skills.stat_count
    }
    fn sight_targets(&mut self, u: UnitId, r: i32, f: u32) -> Vec<UnitId> {
        super::skill_host_sight::targets(self, u, r, f)
    }
    fn sight_state(&mut self, u: UnitId, x: UnitId, p: &CursePulse<'_>) {
        super::skill_host_sight::state(self, u, x, p);
    }
    fn ai_curse_eligible(&mut self, u: UnitId, x: UnitId, a: bool) -> bool {
        super::skill_host_ai_curse::eligible(self, u, x, a)
    }
    fn ai_curse_duration(&mut self, n: i32) -> i32 {
        super::skill_host_ai_curse::duration(self, n)
    }
    fn ai_curse_targets(&mut self, u: UnitId, r: i32, f: u32) -> Option<Vec<UnitId>> {
        super::skill_host_ai_curse::targets(self, u, r, f)
    }
    fn ai_curse_state(&mut self, u: UnitId, x: UnitId, p: &CursePulse<'_>, a: bool) -> bool {
        super::skill_host_ai_curse::state(self, u, x, p, a)
    }
    fn ai_curse_align(&mut self, x: UnitId, a: bool) {
        super::skill_host_ai_curse::align(self, x, a);
    }
    fn ai_curse_redirect(&mut self, x: UnitId, t: Option<UnitId>, n: i32) {
        super::skill_host_ai_curse::redirect(self, x, t, n);
    }
    fn martial_charge_valid(&self, state: u16, counter: u16) -> bool {
        state > 0
            && i32::from(state) < self.state_count()
            && i32::from(counter) < self.cv.v.h.tables.skills.stat_count
    }
    fn martial_claw_frame(&mut self, u: UnitId) {
        super::skill_host_martial::claws(self, u);
    }
    fn martial_charge(&mut self, u: UnitId, counter: u16, pulse: &crate::rules::BuffPulse<'_>) {
        super::skill_host_martial::add(self, u, counter, pulse);
    }
    fn consume_find_corpse(&mut self, corpse: UnitId, soft: bool) -> bool {
        super::skill_host_corpse_find::consume(self, corpse, soft)
    }
    fn corpse_item_drop(&mut self, source: UnitId, corpse: UnitId, quality: i32) {
        super::skill_host_corpse_find::item(self, source, corpse, quality);
    }
    fn corpse_potion_drop(&mut self, source: UnitId, corpse: UnitId, bands: [i32; 2]) {
        super::skill_host_corpse_find::potion(self, source, corpse, bands);
    }
    fn passive_values(
        &mut self,
        u: UnitId,
        state: u16,
        skill: i32,
        level: i32,
        stats: &[crate::rules::StateStat],
    ) {
        super::skill_host_passive::values(self, u, state, skill, level, stats);
    }
    fn aura_state_valid(&self, state: u16) -> bool {
        i32::from(state) < self.state_count()
    }
    fn aura_period_frames(&self, base: i32) -> i32 {
        super::skill_host_damage_aura::period(self, base)
    }
    fn aura_state(&mut self, source: UnitId, target: UnitId, pulse: &AuraPulse<'_>, freeze: bool) {
        super::skill_host_damage_aura::state(self, source, target, pulse, freeze);
    }
    fn aura_targets(&mut self, source: UnitId, radius: i32, filter: u32) -> Vec<UnitId> {
        super::skill_host_damage_aura::targets(self, source, radius, filter)
    }
    fn freeze_aura_target(&self, target: UnitId) -> bool {
        super::skill_host_damage_aura::freeze_target(self, target)
    }
    fn shift_form(&mut self, u: UnitId, pulse: &crate::rules::BuffPulse<'_>, delay: i32) -> bool {
        super::skill_host_form::apply(self, u, pulse, delay)
    }
    fn clear_form_entry(&mut self, u: UnitId, skill: i32) {
        super::skill_host_form::removed(self, u, skill)
    }
    fn warcry_state(
        &mut self,
        source: UnitId,
        target: UnitId,
        pulse: &crate::rules::BuffPulse<'_>,
    ) -> bool {
        super::skill_host_warcry::apply(self, source, target, pulse)
    }
    fn timed_buff(&mut self, u: UnitId, pulse: &crate::rules::BuffPulse<'_>) -> bool {
        super::skill_host_buff::apply(self, u, pulse)
    }
    fn unit_overlay(&mut self, u: UnitId, id: i32) {
        BodyWorld::combat(self).overlay(u, id);
    }
    fn run_speed(&self, u: UnitId) -> i32 {
        let t = &self.cv.v.h.tables;
        match crate::rules::HostUnits::unit_type(self, u) {
            Some(UnitType::Player) => t
                .combat
                .charstats(self.class_id(u))
                .map_or(0, |c| i32::from(c.runvelocity)),
            Some(UnitType::Monster) => t
                .combat
                .monstats(self.class_id(u))
                .map_or(0, |m| i32::from(m.run as i16)),
            _ => 0,
        }
    }

    fn in_reach(&mut self, u: UnitId, target: UnitId) -> bool {
        BodyWorld::combat(self).in_melee_range(u, target, 0)
    }

    fn skill_move(&mut self, u: UnitId, on: bool) {
        bodies::set_uninterruptable(self, u, on);
        BodyWorld::state_on(self, u, bodies::st::SKILL_MOVE, on);
    }

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
        // The caster first, as an aura's own state; then the units around,
        // with the state the aura gives them.
        aura_unit(self, &t.skills, &t.combat, u, &mut cx, u);
        cx.source_state = i32::from(pulse.target_state);
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

    fn nest(&mut self, u: UnitId, overlay: i32) -> i32 {
        bodies::b4_mon::nest_hatch(self, u, overlay)
    }

    fn resurrect(&mut self, u: UnitId, overlay: i32) -> i32 {
        let t = self.cv.v.h.tables.clone();
        bodies::b4_mon::resurrect_target(self, &t.skills, &t.combat, u, overlay)
    }

    fn lay_wall(&mut self, u: UnitId, missile: i32, centre: i32, skill: i32, lvl: i32) -> i32 {
        let t = self.cv.v.h.tables.clone();
        bodies::b3_lvl18::wall(self, &t.skills, u, skill, lvl, missile, centre)
    }

    fn mode_damage(&mut self, u: UnitId, mode: i32) {
        let t = self.cv.v.h.tables.clone();
        bodies::mode_damage(self, &t.skills, &t.combat, u, mode);
    }

    fn heal(&mut self, u: UnitId, target: UnitId, percent: i32) -> bool {
        bodies::flag_40(self, u);
        bodies::b4_mon::heal_percent(self, target, percent);
        true
    }

    fn curse_pulse(&mut self, u: UnitId, pulse: &CursePulse<'_>) -> i32 {
        let t = self.cv.v.h.tables.clone();
        let stats: Vec<(i32, i32)> = pulse
            .stats
            .iter()
            .map(|&(s, v)| (i32::from(s), v))
            .collect();
        curse_pulse(
            self,
            &t.skills,
            &t.combat,
            u,
            pulse.skill,
            pulse.level,
            i32::from(pulse.state),
            self.tick_rate().ticks(pulse.frames),
            pulse.range,
            pulse.filter,
            &stats,
        )
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
