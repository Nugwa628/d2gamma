//! Collide types past the engine's table, and the area search a mod
//! game's bodies run with a collide mask.
//!
//! A `missiles.txt` `CollideType` picks an entry of [`super::COLLIDE_MODES`]
//! (types 0-8). D2RUST's data clamps the column to 8 at load; a mod
//! game's data keeps it (`d2_data::fixup`), and the mod's
//! [`MissileProvider::collide_mode`] gives the entries of its own types.
//! A type neither answers reads as type 8, the entry the clamp gave it.
//!
//! [`MissileProvider::collide_mode`]: super::MissileProvider::collide_mode

use crate::game::Game;
use crate::rules::Ruleset;
use crate::units::UnitId;

use super::{collide_mode, CollideMode, Ctx, MissileWorld, COLLIDE_MODES};

/// The last collide type of the engine's own table.
pub const LAST_COLLIDE_TYPE: u8 = (COLLIDE_MODES.len() - 1) as u8;

/// The collide entry of `mode` under `rules`: the mod's entry first, then
/// the engine's table, a type past it as [`LAST_COLLIDE_TYPE`].
pub fn collide_entry(rules: &Ruleset, mode: u8) -> Option<CollideMode> {
    if let Some(c) = rules.missiles().and_then(|p| p.collide_mode(mode)) {
        return Some(c);
    }
    collide_mode(mode.min(LAST_COLLIDE_TYPE))
}

/// The units [`super::MissileBodies::area_units`] gives for `(owner, at,
/// r, f)` that nothing in `mask` blocks on the line from `source` to them
/// (the line test from `source`'s room). `mask` 0 or a `source` without a
/// room: the area units as they are.
#[allow(clippy::too_many_arguments)]
pub fn area_units_in_sight<W: MissileWorld + ?Sized>(
    game: &mut Game,
    cx: &mut Ctx<'_, W>,
    owner: UnitId,
    source: UnitId,
    at: (i32, i32),
    r: i32,
    f: u32,
    mask: u16,
) -> Vec<UnitId> {
    let units = cx.world.area_units(game, owner, at, r, f);
    let room = game.lists.unit(source).and_then(|e| e.room());
    let Some(room) = room.filter(|_| mask != 0) else {
        return units;
    };
    let from = cx.world.position(source);
    units
        .into_iter()
        .filter(|&u| {
            let to = cx.world.position(u);
            !cx.world.line_hits(game, room, from, to, mask)
        })
        .collect()
}
