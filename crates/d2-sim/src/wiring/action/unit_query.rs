//! [`UnitQuery`] on the action wiring: the unit facts a mod game's rules
//! read, from the unit records, stat lists, skill lists and rooms. The
//! event host ([`CombatView`]) answers it, and the economy hosts (cube,
//! NPCs, item use, quests) answer it through their unit hooks
//! ([`crate::units::lifecycle::LifecycleHooks::skill_entries`]).

use crate::rules::{skill_level_in, UnitQuery};
use crate::skills::SkillEntry;
use crate::units::{UnitId, UnitType};

use super::combat::CombatView;
use super::{ActionHooks, Pending};

impl<X> ActionHooks<X> {
    /// The unit's skill entries, in list order: a player's list, else a
    /// monster's (its set skills, else its natural ones, all native
    /// entries). Empty for a unit with none.
    pub fn skill_entries_of(&self, unit: UnitId) -> Vec<SkillEntry> {
        if let Some(l) = self.skill_lists.get(&unit) {
            return l.view();
        }
        let Some(l) = self
            .monster_skills
            .get(&unit)
            .or_else(|| self.natural_skills.get(&unit))
        else {
            return Vec::new();
        };
        l.iter()
            .map(|(&skill, &base)| SkillEntry {
                skill,
                base,
                level_bonus: 0,
                owner_guid: -1,
                charges: 0,
                has_charges: false,
            })
            .collect()
    }
}

impl<X: Pending> UnitQuery for CombatView<'_, X> {
    fn stat(&self, unit: UnitId, stat: u16, layer: u16) -> i32 {
        self.v.stats.unit_total(unit, stat, layer)
    }
    fn base_stat(&self, unit: UnitId, stat: u16, layer: u16) -> i32 {
        self.v.stats.unit_base(unit, stat, layer)
    }
    fn skill_level(&self, unit: UnitId, skill: u16, total: bool) -> i32 {
        let player = self
            .v
            .units
            .get(unit)
            .filter(|r| r.ty == UnitType::Player)
            .map(|r| r.class as i32);
        let stats = &*self.v.stats;
        skill_level_in(
            &self.v.h.tables.skills,
            &self.v.h.skill_entries_of(unit),
            skill,
            total,
            player,
            |s| stats.has_state(unit, u32::from(s)),
            |s, l| stats.unit_total(unit, s, l),
        )
    }
    fn has_state(&self, unit: UnitId, state: u16) -> bool {
        self.v.stats.has_state(unit, u32::from(state))
    }
    fn level_id(&self, unit: UnitId) -> Option<u32> {
        let room = self.game.lists.unit(unit)?.room()?;
        self.v.h.drlg.level_id(&*self.game, room)
    }
    fn act(&self, unit: UnitId) -> Option<u8> {
        let room = self.game.lists.unit(unit)?.room()?;
        Some(self.game.lists.room(room)?.act)
    }
    fn in_town(&self, unit: UnitId) -> bool {
        self.game
            .lists
            .unit(unit)
            .and_then(|e| e.room())
            .is_some_and(|room| self.v.h.drlg.in_town(&*self.game, room))
    }
    fn class(&self, unit: UnitId) -> Option<(UnitType, u32)> {
        let ty = self.game.lists.unit(unit)?.ty;
        Some((ty, self.v.units.get(unit)?.class))
    }
    fn difficulty(&self) -> u8 {
        self.v.data.difficulty
    }
}
