//! The damage a missile carries from its creation, filled by a mod game.
//!
//! D2RUST fills a new missile's damage itself ([`super::damage::setup`]
//! through [`super::MissileCombat::damage_setup`]). A mod game's
//! [`MissileProvider::fill_damage`] can fill it instead; [`run_fill`] asks
//! it first. The reads a fill needs beyond the rest of the missile world
//! (the owner's weapon and its facts, the item-type masteries, the
//! damage lines of a missile row or a skill) are [`MissileFill`]. D2RUST's
//! own bodies never call them.
//!
//! [`MissileProvider::fill_damage`]: super::MissileProvider::fill_damage

use crate::game::Game;
use crate::units::UnitId;

use super::{Ctx, MissileWorld};

/// What a missile's creation fill is for: the missile `missile` of
/// `owner` (stats from `origin` when it has one), with its stored skill
/// and level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FillRequest {
    pub owner: UnitId,
    pub origin: Option<UnitId>,
    pub missile: UnitId,
    pub skill: i32,
    pub level: i32,
}

/// A damage line of a missile row or a skill at a level: physical and
/// elemental min/max (1/256 points) and the elemental length (frames).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DamageLine {
    pub phys_min: i32,
    pub phys_max: i32,
    pub elem_min: i32,
    pub elem_max: i32,
    pub elem_len: i32,
}

/// The reads of a mod game's creation fill. Every default is the
/// no-weapon, no-damage answer.
#[allow(unused_variables)]
pub trait MissileFill {
    /// The weapon the creation fill takes damage from: a player's attack
    /// weapon, a monster's when it has an inventory; else none.
    fn fill_weapon(&mut self, game: &mut Game, owner: UnitId) -> Option<UnitId> {
        None
    }
    /// The unit has an inventory.
    fn has_inventory(&mut self, game: &mut Game, unit: UnitId) -> bool {
        false
    }
    /// The item is two-handed.
    fn item_two_handed(&mut self, game: &mut Game, item: UnitId) -> bool {
        false
    }
    /// The item is a throwing weapon.
    fn item_throwable(&mut self, game: &mut Game, item: UnitId) -> bool {
        false
    }
    /// The item is of item type `itype` (or a type under it).
    fn item_is_type(&mut self, game: &mut Game, item: UnitId, itype: i32) -> bool {
        false
    }
    /// The item's wield type (2: wielded with both hands).
    fn wield_type(&mut self, game: &mut Game, item: UnitId) -> i32 {
        0
    }
    /// The weapon's strength and dexterity damage bonus (`StrBonus`,
    /// `DexBonus`); (0, 0) for an item that is not a weapon.
    fn str_dex_bonus(&mut self, game: &mut Game, item: UnitId) -> (i32, i32) {
        (0, 0)
    }
    /// The unit's item-type mastery of `kind` (0 to-hit, 1 damage, 2
    /// critical strike) for `item`: the best value of the mastery stats
    /// whose layer is an item type the item is (the throw masteries for a
    /// throwing weapon with a throwing skill); `skill` none reads the
    /// used skill. 0 without an item.
    fn weapon_mastery(
        &mut self,
        game: &mut Game,
        unit: UnitId,
        item: Option<UnitId>,
        skill: Option<i32>,
        kind: u8,
    ) -> i32 {
        0
    }
    /// Switches the unit's weapon stat lists for the damage reads
    /// (`offhand`: the second weapon; `on` false restores them).
    fn dual_wield_switch(&mut self, game: &mut Game, unit: UnitId, offhand: bool, on: bool) {}
    /// Missile row `class`'s own damage line at `level` (its ladders and
    /// calcs on `missile` and `owner`).
    fn missile_damage_line(
        &mut self,
        game: &mut Game,
        missile: Option<UnitId>,
        owner: Option<UnitId>,
        class: i32,
        level: i32,
    ) -> DamageLine {
        DamageLine::default()
    }
    /// Skill `skill`'s damage line at `level` for `owner`: the physical
    /// damage without the source-damage share, the elemental damage (with
    /// the owner's elemental mastery when `mastery`) and length.
    fn skill_damage_line(
        &mut self,
        game: &mut Game,
        owner: Option<UnitId>,
        skill: i32,
        level: i32,
        mastery: bool,
    ) -> DamageLine {
        DamageLine::default()
    }
}

/// The creation fill of a new missile: the mod game's
/// [`super::MissileProvider::fill_damage`] when it answers, else D2RUST's
/// [`super::MissileCombat::damage_setup`]. Returns the missile data flag
/// bits to set.
pub fn run_fill<W: MissileWorld + ?Sized>(
    game: &mut Game,
    cx: &mut Ctx<'_, W>,
    req: FillRequest,
) -> u32 {
    if !cx.world.rules().is_original() {
        let rules = cx.world.rules().clone();
        if let Some(bits) = rules
            .missiles()
            .and_then(|p| p.fill_damage(game, &mut cx.as_dyn(), req))
        {
            return bits;
        }
    }
    cx.world.damage_setup(
        game,
        req.owner,
        req.origin,
        req.missile,
        req.skill,
        req.level,
    )
}
