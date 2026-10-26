//! Unit queries for a mod game's skill bodies.

use crate::combat::RoomKind;
use crate::units::{UnitId, UnitType};

/// What a skill body reads about a unit beyond its stats and states: its
/// type, place, target, life and owner, and the game's difficulty.
pub trait HostUnits {
    /// The unit's type; `None` for a unit not in the game.
    fn unit_type(&self, u: UnitId) -> Option<UnitType> {
        let _ = u;
        None
    }
    /// The unit is a hireling (a monster hired by a player).
    fn is_hireling(&self, u: UnitId) -> bool {
        let _ = u;
        false
    }
    /// The unit has a path: the units that stand somewhere (players,
    /// monsters, missiles, and objects and items on the ground).
    fn has_path(&self, u: UnitId) -> bool {
        let _ = u;
        false
    }
    /// The unit's position in subtiles (meaningful when
    /// [`HostUnits::has_path`]).
    fn position(&self, u: UnitId) -> (i32, i32) {
        let _ = u;
        (0, 0)
    }
    /// The unit the unit's current mode or skill targets.
    fn target(&self, u: UnitId) -> Option<UnitId> {
        let _ = u;
        None
    }
    /// The point the unit's skill aims at: its target unit's position,
    /// else the clicked point; `None` without one.
    fn target_position(&self, u: UnitId) -> Option<(i32, i32)> {
        let _ = u;
        None
    }
    /// The kind of room the unit stands in (none, town, field).
    fn room_kind(&self, u: UnitId) -> RoomKind {
        let _ = u;
        RoomKind::None
    }
    /// The id of the level (`levels` row) the unit stands in; `None`
    /// outside a room.
    fn level_id(&self, u: UnitId) -> Option<u32> {
        let _ = u;
        None
    }
    /// The monster level of the unit's level at the game's difficulty
    /// (`levels` `MonLvl`); `None` outside a level.
    fn area_level(&self, u: UnitId) -> Option<i32> {
        let _ = u;
        None
    }
    /// The `Teleport` rule of the unit's level (`levels`; 0 forbids);
    /// `None` outside a level.
    fn level_teleport(&self, u: UnitId) -> Option<i32> {
        let _ = u;
        None
    }
    /// The unit is dead or dying.
    fn is_dead(&self, u: UnitId) -> bool {
        let _ = u;
        false
    }
    /// The unit's owner: the player or monster a summon, hireling or
    /// minion belongs to.
    fn owner(&self, u: UnitId) -> Option<UnitId> {
        let _ = u;
        None
    }
    /// The unit's flags word (bit 0x40: the skill's do frame ran).
    fn unit_flags(&self, u: UnitId) -> u32 {
        let _ = u;
        0
    }
    /// Sets the unit's flags word.
    fn set_unit_flags(&mut self, u: UnitId, flags: u32) {
        let _ = (u, flags);
    }
    /// The game's difficulty (0 normal, 1 nightmare, 2 hell).
    fn difficulty(&self) -> usize {
        0
    }
    /// The game runs the expansion rules.
    fn expansion(&self) -> bool {
        true
    }
}
