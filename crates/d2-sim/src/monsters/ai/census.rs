//! Looking around, for the data mod host: the scans the built-in bodies make
//! (undead near a mummy, the minions near a spawner), as plain answers.

use super::bodies2::{mummy_scan, MummyScan};
use super::bodies6::{owner_view, pet_follow};
use super::common::{base_id, dying, is_monster, scan_units, unit_base};
use super::seams::AiHost;
use super::Ctx;
use crate::game::Game;
use crate::units::UnitId;

/// What the undead scan found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UndeadScan {
    /// Undead (of the monster's side) within the radius.
    pub seen: i32,
    /// The last fallen one that can be raised.
    pub corpse: Option<UnitId>,
    /// The last living one that is hurt.
    pub hurt: Option<UnitId>,
}

/// The Greater Mummy's scan: the monsters of its side that are undead,
/// within `radius` subtiles of `unit`.
pub fn undead_scan<W: AiHost + ?Sized>(
    game: &Game,
    cx: &Ctx<'_, W>,
    unit: UnitId,
    radius: i32,
) -> UndeadScan {
    let mut r = MummyScan {
        max: radius.wrapping_mul(radius),
        ..MummyScan::default()
    };
    mummy_scan(game, cx, unit, &mut r);
    UndeadScan {
        seen: r.seen,
        corpse: r.corpse,
        hurt: r.hurt,
    }
}

/// How many living monsters other than `unit`, in the rooms near it, are of
/// the same family (monstats `BaseId`) as one of `classes`: the minion
/// spawner's count.
pub fn count_of_classes<W: AiHost + ?Sized>(
    game: &Game,
    cx: &Ctx<'_, W>,
    unit: UnitId,
    classes: &[u16],
) -> usize {
    let bases: Vec<i32> = classes.iter().map(|&c| base_id(cx, i32::from(c))).collect();
    scan_units(game, unit)
        .into_iter()
        .filter(|&v| {
            v != unit && is_monster(game, v) && !dying(cx, v) && bases.contains(&unit_base(cx, v))
        })
        .count()
}

/// The enemy the owner `o` of the pet `unit` is fighting: the owner's own
/// target search, dropped when the target is flagged to be left alone.
pub fn owner_target<W: AiHost + ?Sized>(
    game: &mut Game,
    cx: &mut Ctx<'_, W>,
    unit: UnitId,
    o: UnitId,
) -> Option<UnitId> {
    owner_view(game, cx, unit, o).0
}

/// The pet follow: `s` is the enemy the pet has in mind, `combat` whether
/// it is within reach of it, `quiet` to stay put when it is where it
/// should be, `n` how far it may stray from `o`. Whether it did anything.
#[allow(clippy::too_many_arguments)]
pub fn follow_owner<W: AiHost + ?Sized>(
    game: &mut Game,
    cx: &mut Ctx<'_, W>,
    unit: UnitId,
    s: Option<UnitId>,
    o: UnitId,
    combat: bool,
    quiet: bool,
    n: i32,
) -> bool {
    pet_follow(game, cx, unit, s, o, combat, quiet, n)
}
