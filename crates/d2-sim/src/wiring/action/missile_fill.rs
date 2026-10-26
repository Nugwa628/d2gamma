//! A mod game's missile creation fill reads
//! ([`crate::missiles::fill::MissileFill`]) on the combat view: the same
//! queries D2RUST's own fill ([`crate::missiles::damage::setup`]) makes.

use crate::combat::CombatWorld;
use crate::game::Game;
use crate::missiles::damage::SetupWorld;
use crate::missiles::fill::{DamageLine, MissileFill};
use crate::skills::{self, SkillUnits};
use crate::units::UnitId;

use super::{Pending, View};

impl<X: Pending> MissileFill for View<'_, X> {
    fn fill_weapon(&mut self, game: &mut Game, owner: UnitId) -> Option<UnitId> {
        self.combat(game).setup_weapon(owner)
    }
    fn has_inventory(&mut self, game: &mut Game, unit: UnitId) -> bool {
        SetupWorld::has_inventory(&self.combat(game), unit)
    }
    fn item_two_handed(&mut self, game: &mut Game, item: UnitId) -> bool {
        self.combat(game).two_handed(item)
    }
    fn item_throwable(&mut self, game: &mut Game, item: UnitId) -> bool {
        self.combat(game).item_flag_throw(item)
    }
    fn item_is_type(&mut self, game: &mut Game, item: UnitId, itype: i32) -> bool {
        self.combat(game).item_is(item, itype)
    }
    fn wield_type(&mut self, game: &mut Game, item: UnitId) -> i32 {
        self.combat(game).wield_type(item)
    }
    fn str_dex_bonus(&mut self, game: &mut Game, item: UnitId) -> (i32, i32) {
        self.combat(game).str_dex_bonus(item)
    }
    fn weapon_mastery(
        &mut self,
        game: &mut Game,
        unit: UnitId,
        item: Option<UnitId>,
        skill: Option<i32>,
        kind: u8,
    ) -> i32 {
        let t = self.h.tables.clone();
        let cv = self.combat(game);
        skills::weapon_mastery(&cv, &t.skills, Some(unit), item, skill, kind)
    }
    fn dual_wield_switch(&mut self, game: &mut Game, unit: UnitId, offhand: bool, on: bool) {
        self.combat(game).dual_wield_switch(unit, offhand, on);
    }
    fn missile_damage_line(
        &mut self,
        game: &mut Game,
        missile: Option<UnitId>,
        owner: Option<UnitId>,
        class: i32,
        level: i32,
    ) -> DamageLine {
        let t = self.h.tables.clone();
        let mut cv = self.combat(game);
        let s = &t.skills;
        DamageLine {
            phys_min: skills::miss_phys_min(&mut cv, s, missile, owner, class, level),
            phys_max: skills::miss_phys_max(&mut cv, s, missile, owner, class, level),
            elem_min: skills::miss_elem_min(&mut cv, s, missile, owner, class, level),
            elem_max: skills::miss_elem_max(&mut cv, s, missile, owner, class, level),
            elem_len: skills::miss_elem_len(&mut cv, s, missile, class, level),
        }
    }
    fn skill_damage_line(
        &mut self,
        game: &mut Game,
        owner: Option<UnitId>,
        skill: i32,
        level: i32,
        mastery: bool,
    ) -> DamageLine {
        let t = self.h.tables.clone();
        let mut cv = self.combat(game);
        let s = &t.skills;
        DamageLine {
            phys_min: skills::phys_min(&mut cv, s, owner, skill, level, false),
            phys_max: skills::phys_max(&mut cv, s, owner, skill, level, false),
            elem_min: skills::elem_min(&mut cv, s, owner, skill, level, mastery),
            elem_max: skills::elem_max(&mut cv, s, owner, skill, level, mastery),
            elem_len: skills::elem_len(&mut cv, s, owner, skill, level),
        }
    }
}
