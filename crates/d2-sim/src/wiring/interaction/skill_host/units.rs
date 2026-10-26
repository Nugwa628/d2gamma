//! [`HostUnits`] on the skill use view.

use crate::combat::{CombatWorld, RoomKind};
use crate::rules::HostUnits;
use crate::skills::use_::bodies::BodyWorld;
use crate::skills::use_::UseWorld;
use crate::units::{UnitId, UnitType};
use crate::wiring::action::Pending;
use crate::wiring::interaction::{UseRest, UseView};

impl<X: Pending + UseRest> HostUnits for UseView<'_, X> {
    fn unit_type(&self, u: UnitId) -> Option<UnitType> {
        self.cv.v.units.get(u).map(|r| r.ty)
    }
    fn is_hireling(&self, u: UnitId) -> bool {
        CombatWorld::is_hireling(&self.cv, u)
    }
    fn has_path(&self, u: UnitId) -> bool {
        BodyWorld::has_path(self, u)
    }
    fn position(&self, u: UnitId) -> (i32, i32) {
        UseWorld::position(self, u)
    }
    fn target(&self, u: UnitId) -> Option<UnitId> {
        UseWorld::target(self, u)
    }
    fn target_position(&self, u: UnitId) -> Option<(i32, i32)> {
        UseWorld::target_position(self, u)
    }
    fn room_kind(&self, u: UnitId) -> RoomKind {
        UseWorld::room(self, u)
    }
    fn level_id(&self, u: UnitId) -> Option<u32> {
        let r = BodyWorld::unit_room(self, u)?;
        self.cv.v.h.drlg.level_id(&*self.cv.game, r)
    }
    /// The level's `MonLvl` at the game's difficulty; `None` for level 0.
    fn area_level(&self, u: UnitId) -> Option<i32> {
        let id = i32::try_from(self.level_id(u)?).ok().filter(|&i| i > 0)?;
        Some(crate::monsters::init::area_level(
            &self.cv.v.h.tables.levels,
            id,
            CombatWorld::difficulty(&self.cv),
            CombatWorld::expansion(&self.cv),
        ))
    }
    /// The level's row, else the room's answer of the skill bodies.
    fn level_teleport(&self, u: UnitId) -> Option<i32> {
        let row = self
            .level_id(u)
            .and_then(|id| self.cv.v.h.tables.levels.get(usize::try_from(id).ok()?));
        match row {
            Some(row) => Some(i32::from(row.teleport)),
            None => BodyWorld::room_teleport(self, BodyWorld::unit_room(self, u)?),
        }
    }
    fn is_dead(&self, u: UnitId) -> bool {
        CombatWorld::is_dead(&self.cv, u)
    }
    /// The owner link the skill pipeline reads, else the monster's AI
    /// control owner.
    fn owner(&self, u: UnitId) -> Option<UnitId> {
        UseWorld::owner(self, u).or_else(|| BodyWorld::pack_owner(self, u))
    }
    fn unit_flags(&self, u: UnitId) -> u32 {
        UseWorld::unit_flags(self, u)
    }
    fn set_unit_flags(&mut self, u: UnitId, flags: u32) {
        UseWorld::set_unit_flags(self, u, flags);
    }
    fn difficulty(&self) -> usize {
        CombatWorld::difficulty(&self.cv)
    }
    fn expansion(&self) -> bool {
        CombatWorld::expansion(&self.cv)
    }
}
