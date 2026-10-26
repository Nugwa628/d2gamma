//! A mod game's treasure drop from its quest code: a treasure class walked
//! at a unit, the items made in the unit's room as the chest drop of
//! [`super::chest_drop`] makes them (the unit is the dropper, its unit seed
//! draws).

use d2_data::tables::Levels;

use super::death::Spots;
use super::{dropper, recipient, DeathDrops, Economy, FreeSpot, GameFields, ItemDrops};
use crate::treasure::drop::area_level;
use crate::treasure::walk::{walk, WalkArgs};
use crate::treasure::TreasureData;
use crate::units::hooks::Sim;
use crate::units::UnitId;
use crate::wiring::action::{ActionHooks, Pending};

/// Drops treasure class `tc` (its row) at `unit` for `recipient`: the walk
/// runs on the unit's seed with item level `level` and forced quality
/// `quality` (0: rolled), from the unit's place as
/// [`super::object_chest_drop`] drops. Returns the items made; they are also
/// appended to [`DeathDrops::placed`].
#[allow(clippy::too_many_arguments)]
pub fn unit_tc_drop<X: Pending, F: FreeSpot>(
    h: &mut ActionHooks<X>,
    sim: &mut Sim<'_>,
    d: &mut DeathDrops,
    levels: &[Levels],
    spots: &mut F,
    unit: UnitId,
    recipient_unit: Option<UnitId>,
    tc: u16,
    level: i32,
    quality: u8,
) -> Vec<UnitId> {
    let Some(mut seed) = sim.units.get(unit).map(|r| r.seed) else {
        return Vec::new();
    };
    let room = sim.game.lists.unit(unit).and_then(|e| e.room());
    let (x, y) = h.path_position(unit);
    let level_id = room
        .and_then(|rm| h.drlg.level_id(sim.game, rm))
        .map_or(0, |l| i32::try_from(l).unwrap_or(i32::MAX));
    let t = d.tables.clone();
    let mut fields = GameFields::from_action(
        h.game_seed,
        &h.ai_info,
        sim.data.expansion,
        std::mem::take(&mut h.uniques),
    );
    let facts = fields.treasure_facts(super::death::living_players(sim), d.players_setting);
    let area = area_level(levels, level_id, fields.difficulty, fields.expansion);
    let drop_by = dropper(sim.units, sim.stats, Some(unit), area, x, y);
    let rec = recipient_unit.map(|r| {
        recipient(
            sim.units,
            sim.stats,
            r,
            h.minion_owner_of(sim.game, r),
            h.x.party_size(r),
        )
    });
    // Start at the offset (+2, +3) when a room exists there.
    let start_room = room.and_then(|rm| h.drlg.find_room(sim.game, rm, x + 2, y + 3));
    let start = match start_room {
        Some(_) => (x + 2, y + 3),
        None => (x, y),
    };
    let data = TreasureData {
        tcs: &t.tcs,
        items: &t.treasure_items,
        itemtypes: &t.items.itemtypes,
        equiv: &t.items.equiv,
        itemratio: &t.items.itemratio,
    };
    let args = WalkArgs {
        tc: Some(tc),
        quality,
        level,
        find_item: false,
        list: true,
        max: 6,
    };
    let mut items = std::mem::take(&mut h.items);
    let (out, placed, failures) = {
        let mut econ = Economy {
            game: &mut *sim.game,
            units: &mut *sim.units,
            stats: &mut *sim.stats,
            data: sim.data,
            hooks: &mut *h,
            fields: &mut fields,
            tables: &t.items,
            items: &mut items,
        };
        let mut sink = ItemDrops::new(
            &mut econ,
            Spots {
                inner: spots,
                room,
                start_room,
                start,
            },
        )
        .with_unit(unit);
        let out = walk(
            &data,
            &facts,
            &drop_by,
            &mut seed,
            rec.as_ref(),
            &args,
            &mut sink,
        );
        (out, sink.placed, sink.failures)
    };
    h.items = items;
    h.game_seed = fields.seed;
    h.uniques = std::mem::take(&mut fields.uniques);
    d.fields = fields;
    if let Some(r) = sim.units.get_mut(unit) {
        r.seed = seed;
    }
    d.placed.extend(placed);
    d.failures.extend(failures);
    out.unwrap_or_else(|e| {
        d.errors.push(e);
        Vec::new()
    })
}
