//! The economy's side of the world host seams: [`UnitQuery`] on the
//! [`Economy`] (the unit records, stat lists and the unit hooks' rooms and
//! skill lists), which the cube, NPC, item use and quest hosts hand out;
//! and [`EconomyMessages`], the [`MessageHost`] a mod game's own messages
//! run on.

use std::collections::BTreeMap;

use super::Economy;
use crate::game::Game;
use crate::items::inventory::Inventory;
use crate::rng::Seed;
use crate::rules::{skill_level_in, EventHost, ItemFacts, MessageHost, UnitQuery};
use crate::units::lifecycle::LifecycleHooks;
use crate::units::{UnitId, UnitType};
use crate::wiring::action::{ActionHooks, Pending};

impl<H: LifecycleHooks> Economy<'_, H> {
    fn unit_room(&self, unit: UnitId) -> Option<crate::units::RoomId> {
        self.game.lists.unit(unit)?.room()
    }
}

impl<H: LifecycleHooks> UnitQuery for Economy<'_, H> {
    fn stat(&self, unit: UnitId, stat: u16, layer: u16) -> i32 {
        self.stats.unit_total(unit, stat, layer)
    }
    fn base_stat(&self, unit: UnitId, stat: u16, layer: u16) -> i32 {
        self.stats.unit_base(unit, stat, layer)
    }
    /// From the unit hooks' skill entries and tables; 0 without tables.
    fn skill_level(&self, unit: UnitId, skill: u16, total: bool) -> i32 {
        let Some(t) = self.hooks.skill_tables() else {
            return 0;
        };
        let player = self
            .units
            .get(unit)
            .filter(|r| r.ty == UnitType::Player)
            .map(|r| r.class as i32);
        let stats = &*self.stats;
        skill_level_in(
            t,
            &self.hooks.skill_entries(unit),
            skill,
            total,
            player,
            |s| stats.has_state(unit, u32::from(s)),
            |s, l| stats.unit_total(unit, s, l),
        )
    }
    fn has_state(&self, unit: UnitId, state: u16) -> bool {
        self.stats.has_state(unit, u32::from(state))
    }
    fn level_id(&self, unit: UnitId) -> Option<u32> {
        self.hooks.room_level(&*self.game, self.unit_room(unit)?)
    }
    fn act(&self, unit: UnitId) -> Option<u8> {
        Some(self.game.lists.room(self.unit_room(unit)?)?.act)
    }
    fn in_town(&self, unit: UnitId) -> bool {
        self.unit_room(unit)
            .is_some_and(|room| self.hooks.town_room(&*self.game, room))
    }
    fn class(&self, unit: UnitId) -> Option<(UnitType, u32)> {
        let ty = self.game.lists.unit(unit)?.ty;
        Some((ty, self.units.get(unit)?.class))
    }
    fn difficulty(&self) -> u8 {
        self.fields.difficulty
    }
}

/// The world a mod game's own messages run on, over the economy of the
/// action wiring: the event host's reads and writes, the unit query, and
/// the items of the inventory model when the host has one.
pub struct EconomyMessages<'e, 'a, X> {
    /// The economy.
    pub econ: &'e mut Economy<'a, ActionHooks<X>>,
    /// The inventory model's inventories; `None`: a host without one (no
    /// unit holds items).
    pub inventories: Option<&'e BTreeMap<UnitId, Inventory>>,
}

impl<X: Pending> EventHost for EconomyMessages<'_, '_, X> {
    fn game(&mut self) -> &mut Game {
        self.econ.game
    }
    fn unit_type(&self, unit: UnitId) -> Option<UnitType> {
        self.econ.game.lists.unit(unit).map(|e| e.ty)
    }
    fn class_id(&self, unit: UnitId) -> i32 {
        self.econ.units.get(unit).map_or(0, |r| r.class as i32)
    }
    fn stat(&self, unit: UnitId, stat: u16, layer: u16) -> i32 {
        self.econ.stats.unit_total(unit, stat, layer)
    }
    fn set_stat(&mut self, unit: UnitId, stat: u16, value: i32) {
        let e = &mut *self.econ;
        e.stats.unit_set(e.hooks, unit, stat, value, 0);
    }
    fn has_state(&self, unit: UnitId, state: u16) -> bool {
        self.econ.stats.has_state(unit, u32::from(state))
    }
    fn seed(&mut self, unit: UnitId) -> Option<&mut Seed> {
        self.econ.units.get_mut(unit).map(|r| &mut r.seed)
    }
    fn level(&self, unit: UnitId) -> Option<u32> {
        UnitQuery::level_id(&*self.econ, unit)
    }
    fn send(&mut self, player: UnitId, bytes: &[u8]) {
        self.econ.hooks.x.send(player, bytes);
    }
    fn unit_query(&self) -> Option<&dyn UnitQuery> {
        Some(&*self.econ)
    }
}

impl<X: Pending> MessageHost for EconomyMessages<'_, '_, X> {
    fn items(&self, owner: UnitId) -> Vec<UnitId> {
        self.inventories
            .and_then(|m| m.get(&owner))
            .map(|i| i.items().to_vec())
            .unwrap_or_default()
    }
    fn item(&self, item: UnitId) -> Option<ItemFacts> {
        let i = self.econ.items.get(item)?;
        Some(ItemFacts {
            record: i.record,
            quality: i.quality,
            page: i.inv_page,
            flags: i.flags,
        })
    }
}
