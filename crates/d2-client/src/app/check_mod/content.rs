//! Data-driven portions of the engine's item and treasure test sweeps.
//! Inputs come from the effective mod tables, never a separate test install.

use super::{Report, Severity};
use crate::app::single_player::LiveData;
use anyhow::Result;
use d2_data::tables::{Itemratio, Itemtypes, Levels, Monlvl, Monstats};
use d2_sim::items::{
    create_item, q, ty, CreateError, Fatal, ItemGame, ItemRequest, ItemStats, ListKey,
};
use d2_sim::rng::Seed;
use d2_sim::treasure::{
    monster_tc, select_entry, walk, DropRequest, DropSink, Dropper, DropperKind, GameFacts,
    MonsterRank, Recipient, TreasureData, WalkArgs,
};
use d2_sim::wiring::economy::GameFields;
use d2_sim::world::cube::{self, CubeData, Recipe};
use std::collections::BTreeMap;

/// The cube matches active input slots, treating zero quantity as one. Extra
/// unmatched items are allowed by the runtime; conditional/portal outputs are
/// legal too. Only impossible input counts are errors here.
fn recipe_errors(recipe: &Recipe) -> Vec<String> {
    if recipe.enabled == 0 {
        return Vec::new();
    }
    let mut errors = Vec::new();
    let quantity: usize = recipe
        .inputs
        .iter()
        .filter(|s| s.flags & 3 != 0)
        .map(|s| usize::from(s.quantity.max(1)))
        .sum();
    if recipe.numinputs == 0 || recipe.numinputs > 12 || quantity > usize::from(recipe.numinputs) {
        errors.push(format!(
            "numinputs is {} (cube capacity 12); active inputs require at least {quantity} items",
            recipe.numinputs
        ));
    }
    if recipe.outputs.iter().all(|o| o.kind == cube::kind::NONE) {
        errors.push("enabled recipe has no output".into());
    }
    errors
}

pub(super) fn check(live: &LiveData, report: &mut Report) -> Result<()> {
    let cubes = CubeData::from_fixed(&live.tables.fixed)?;
    for (row, recipe) in cubes.recipes.iter().enumerate() {
        let errors = recipe_errors(recipe);
        if !errors.is_empty() {
            report.finding(
                Severity::Error,
                "broken cube recipes",
                &format!("cubemain row {row}"),
                errors.join("; "),
            );
        }
    }
    report.completed.push("cube recipe structure sweep".into());

    // game_treasure::sweep_every_tc_resolves_and_picks, using boundary rolls
    // instead of iterating every probability (mods may use large weights).
    let tcs = &live.drops.tcs;
    for (i, tc) in tcs.tcs.iter().enumerate().skip(1) {
        let at = format!("treasure class {i} {}", String::from_utf8_lossy(&tc.name));
        for level in 0..=120 {
            if tcs.get(i as u16, level).is_none() {
                report.finding(
                    Severity::Error,
                    "broken treasure classes",
                    &at,
                    format!("cannot resolve at level {level}"),
                );
                break;
            }
        }
        for expansion in [false, true] {
            let total = tc.total(expansion);
            if total > 0
                && [0, total - 1]
                    .iter()
                    .any(|&r| select_entry(tc, r, expansion).is_none())
            {
                report.finding(
                    Severity::Error,
                    "broken treasure classes",
                    &at,
                    format!("cannot select a weighted entry (expansion={expansion})"),
                );
            }
        }
    }
    // game_treasure::sweep_monster_tcs_resolve: all difficulty/rank/quest
    // combinations, and the actual superunique records.
    let monsters = live.tables.rows::<Monstats>()?;
    for (row, monster) in monsters.iter().enumerate() {
        for difficulty in 0..3 {
            for rank in [
                MonsterRank::Normal,
                MonsterRank::Champion,
                MonsterRank::Unique,
                MonsterRank::Superunique(None),
            ] {
                for quest in [false, true] {
                    let id = monster_tc(monster, rank, difficulty, true, |_| quest)?;
                    if id != 0
                        && [1, 50, 99, 120]
                            .iter()
                            .any(|&level| tcs.get(id, level).is_none())
                    {
                        report.finding(Severity::Error, "broken monster drops", &format!("monstats row {row}"), format!("treasure class {id} cannot resolve at difficulty {difficulty}, quest={quest}"));
                    }
                }
            }
        }
    }
    for (row, unique) in live.drops.superuniques.iter().enumerate() {
        let Some(monster) = monsters.get(unique.class as usize) else {
            report.finding(
                Severity::Error,
                "broken monster drops",
                &format!("superuniques row {row}"),
                "monster class does not exist",
            );
            continue;
        };
        for difficulty in 0..3 {
            let id = monster_tc(
                monster,
                MonsterRank::Superunique(Some(unique)),
                difficulty,
                false,
                |_| false,
            )?;
            if id != 0 && tcs.get(id, 99).is_none() {
                report.finding(
                    Severity::Error,
                    "broken monster drops",
                    &format!("superuniques row {row}"),
                    format!("treasure class {id} cannot resolve at difficulty {difficulty}"),
                );
            }
        }
    }
    report
        .completed
        .push("monster and treasure-class resolution sweeps".into());
    monster_stats(live, &monsters, report)?;
    treasure(live, report)?;
    items(live, report);
    Ok(())
}

fn monster_stats(live: &LiveData, monsters: &[Monstats], report: &mut Report) -> Result<()> {
    use d2_sim::monsters::init::{a1_damage, monster_level, stats_by_level, GameInfo};
    let levels = live.tables.rows::<Levels>()?;
    let monlvl = live.tables.rows::<Monlvl>()?;
    let mut combinations = 0usize;
    // Arithmetic portion of game_monsters' all-class/area/difficulty sweep.
    // Physical spawning and AI/animation behaviour remain scenario/play checks.
    for (row, monster) in monsters.iter().enumerate() {
        if !monster.enabled {
            continue;
        }
        let mut observations = Vec::new();
        for expansion in [false, true] {
            for difficulty in 0..3 {
                let info = GameInfo {
                    expansion,
                    difficulty,
                    game_type: 3,
                    ..Default::default()
                };
                let d = if [271, 338, 359, 560, 561].contains(&row) {
                    0
                } else {
                    usize::from(difficulty)
                };
                let mut observed = false;
                for area in 0..=levels.len() {
                    combinations += 1;
                    let level = monster_level(monster, &levels, &info, d, area as i32);
                    let stats = stats_by_level(monster, &monlvl, info.l_flag(), d, level);
                    let (min, max) = a1_damage(monster, &monlvl, info.l_flag(), d, level);
                    if !observed && (stats.min_hp > stats.max_hp || min > max) {
                        observations.push(format!("difficulty={difficulty}, expansion={expansion}, area={area}, level={level}: HP {}..{}, A1 damage {min}..{max}", stats.min_hp, stats.max_hp));
                        observed = true;
                    }
                }
            }
        }
        if !observations.is_empty() {
            // The runtime uses fixed minimum HP for an inverted HP range
            // (game_monsters::sweep_init_every_class_every_area tests this).
            // Such rows are observations, not invalid content.
            report.finding(
                Severity::Info,
                "inverted monster stat ranges",
                &format!("monstats row {row}"),
                format!(
                    "{}; HP with a lower maximum uses the fixed minimum",
                    observations.join("; ")
                ),
            );
        }
    }
    report.finding(
        Severity::Info,
        "monster sweep summaries",
        "enabled monster rows",
        format!("{combinations} class/area/difficulty/game-mode stat calculations"),
    );
    report
        .completed
        .push("monster stat arithmetic sweep".into());
    Ok(())
}

#[derive(Default)]
struct Sink {
    requests: Vec<DropRequest<()>>,
    gold: Vec<i32>,
}
impl DropSink for Sink {
    type Spot = ();
    type Item = usize;
    fn place(&mut self, _: i32, _: i32) -> Option<()> {
        Some(())
    }
    fn create(&mut self, request: DropRequest<()>) -> Option<usize> {
        self.requests.push(request);
        self.gold.push(100);
        Some(self.requests.len() - 1)
    }
    fn gold(&self, item: usize) -> i32 {
        self.gold[item]
    }
    fn set_gold(&mut self, item: usize, gold: i32) {
        self.gold[item] = gold;
    }
}

fn treasure(live: &LiveData, report: &mut Report) -> Result<()> {
    let itemtypes = live.tables.rows::<Itemtypes>()?;
    let ratios = live.tables.rows::<Itemratio>()?;
    let data = TreasureData {
        tcs: &live.drops.tcs,
        items: &live.drops.treasure_items,
        itemtypes: &itemtypes,
        itemratio: &ratios,
        equiv: &live.tables.fixed.itemtypes_equiv,
    };
    let mut walks = 0;
    // game_treasure::sweep_walk_every_tc, with the engine's accepting sink.
    for row in 1..data.tcs.len() {
        let mut errors = Vec::new();
        for expansion in [false, true] {
            for players in [1, 8] {
                for magic_find in [0, 500] {
                    for level in [1, 50, 99] {
                        for low in [1, 0x5EED] {
                            walks += 1;
                            let game = GameFacts {
                                expansion,
                                difficulty: 0,
                                game_type: 3,
                                living_players: 1,
                                players_setting: players,
                                item_format: if expansion { 101 } else { 2 },
                            };
                            let dropper = Dropper {
                                kind: DropperKind::Monster {
                                    class: 0,
                                    level,
                                    playercount: 8,
                                },
                                x: 0,
                                y: 0,
                            };
                            let recipient = Recipient {
                                party: None,
                                magic_find,
                                gold_find: 0,
                            };
                            let args = WalkArgs {
                                tc: Some(row as u16),
                                quality: 0,
                                level,
                                find_item: false,
                                list: false,
                                max: 6,
                            };
                            let mut seed = Seed::init_low(low);
                            let mut sink = Sink::default();
                            match walk(
                                &data,
                                &game,
                                &dropper,
                                &mut seed,
                                Some(&recipient),
                                &args,
                                &mut sink,
                            ) {
                                Err(e) => errors.push(e.to_string()),
                                Ok(out) => {
                                    if out.len() > 6
                                        || sink.requests.iter().any(|r| {
                                            data.items.get(usize::from(r.id)).is_none_or(|item| {
                                                !expansion && item.version >= 100
                                            }) || !(1..=7).contains(&r.quality)
                                        })
                                    {
                                        errors.push("drop walk produced an invalid item, quality or item count".into());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        errors.sort();
        errors.dedup();
        if !errors.is_empty() {
            report.finding(
                Severity::Error,
                "broken treasure classes",
                &format!(
                    "treasure class {row} {}",
                    String::from_utf8_lossy(&data.tcs.tcs[row].name)
                ),
                errors.join("; "),
            );
        }
    }
    report.finding(Severity::Info, "treasure sweep summaries", "all treasure classes", format!("{walks} seeded drop walks, classic/expansion, 1/8 players, 0/500 magic find, levels 1/50/99"));
    report
        .completed
        .push("treasure-class drop walk sweep".into());
    Ok(())
}

/// Ordered stat storage for isolated creations. It implements the real item
/// seam, including affix lists; each request gets fresh game and stat state.
#[derive(Default)]
struct Stats {
    base: BTreeMap<(u16, u16), i32>,
    lists: BTreeMap<ListKey, BTreeMap<(u16, u16), i32>>,
}
impl ItemStats for Stats {
    fn has_stats(&self) -> bool {
        !self.base.is_empty() || !self.lists.is_empty()
    }
    fn stat(&self, id: u16, layer: u16) -> i32 {
        self.base(id, layer)
            .wrapping_add(self.lists.values().fold(0i32, |sum, l| {
                sum.wrapping_add(l.get(&(id, layer)).copied().unwrap_or(0))
            }))
    }
    fn base(&self, id: u16, layer: u16) -> i32 {
        self.base.get(&(id, layer)).copied().unwrap_or(0)
    }
    fn set_base(&mut self, id: u16, layer: u16, value: i32) {
        self.base.insert((id, layer), value);
    }
    fn has_list(&self, key: ListKey) -> bool {
        self.lists.contains_key(&key)
    }
    fn list_set(&mut self, key: ListKey, id: u16, layer: u16, value: i32) {
        self.lists
            .entry(key)
            .or_default()
            .insert((id, layer), value);
    }
    fn list_add(&mut self, key: ListKey, id: u16, layer: u16, value: i32) {
        let cell = self
            .lists
            .entry(key)
            .or_default()
            .entry((id, layer))
            .or_default();
        *cell = cell.wrapping_add(value);
    }
    fn list_get(&self, key: ListKey, id: u16, layer: u16) -> i32 {
        self.lists
            .get(&key)
            .and_then(|l| l.get(&(id, layer)))
            .copied()
            .unwrap_or(0)
    }
}

fn items(live: &LiveData, report: &mut Report) {
    let tables = &live.drops.items;
    let mut requests = 0;
    let mut expected_refusals = 0;
    let mut reserved = 0;
    // game_items::sweep_create_every_item_every_quality. Deterministic seeds,
    // every quality, classic/expansion, normal/hell and four item levels.
    for (row, record) in tables.items.iter().enumerate() {
        // Removed/disabled rows retain their slot but have no item body.
        if record.type_ == 0 && record.invwidth == 0 && record.invheight == 0 {
            reserved += 1;
            continue;
        }
        let mut failures = Vec::new();
        for expansion in [false, true] {
            for difficulty in [0, 2] {
                for quality in 0..=9 {
                    for ilvl in [1, 30, 60, 99] {
                        requests += 1u32;
                        let mut game = GameFields::new(
                            Seed::init_low(requests.wrapping_mul(0x9E37_79B9)),
                            expansion,
                        );
                        game.difficulty = difficulty;
                        let mut request = ItemRequest {
                            item: row as i32,
                            quality,
                            ilvl,
                            format: game.item_format(),
                            ..Default::default()
                        };
                        let result = create_item(
                            tables,
                            &mut game,
                            &mut request,
                            false,
                            Stats::default(),
                            0,
                        );
                        let error = match result {
                            Err(CreateError::Classic) if !expansion && record.version >= 100 => {
                                expected_refusals += 1;
                                None
                            }
                            Err(CreateError::Fatal(Fatal::NullAffixGroup))
                                if quality == q::CRAFTED =>
                            {
                                expected_refusals += 1;
                                None
                            }
                            Err(CreateError::NotPlayer) if record.type_ == ty::PLAY as i16 => {
                                expected_refusals += 1;
                                None
                            }
                            Err(e) => Some(e.to_string()),
                            Ok(_) if !expansion && record.version >= 100 => {
                                Some("created an expansion item in a classic game".into())
                            }
                            Ok(created) if !(1..=9).contains(&created.item.quality) => Some(
                                format!("invalid resulting quality {}", created.item.quality),
                            ),
                            Ok(_) => None,
                        };
                        if let Some(error) = error {
                            failures.push(format!("expansion={expansion}, difficulty={difficulty}, quality={quality}, ilvl={ilvl}: {error}"));
                        }
                    }
                }
            }
        }
        if !failures.is_empty() {
            report.finding(
                Severity::Error,
                "broken item generation rows",
                &format!("item {row} {}", String::from_utf8_lossy(&record.code)),
                failures.join("; "),
            );
        }
    }
    report.finding(
        Severity::Info,
        "item generation summaries",
        "all item rows",
        format!(
            "{requests} seeded requests; {expected_refusals} expected classic/crafted/ear refusals; {reserved} empty reserved rows skipped"
        ),
    );
    report.completed.push("item generation sweep".into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impossible_recipe_is_reported_and_disabled_recipe_is_ignored() {
        let mut recipe = Recipe {
            enabled: 1,
            numinputs: 0,
            ..Default::default()
        };
        assert_eq!(recipe_errors(&recipe).len(), 2);
        recipe.inputs[0].quantity = 3;
        recipe.inputs[0].flags = cube::input_flags::ITEMCODE;
        recipe.numinputs = 3;
        recipe.outputs[0].kind = cube::kind::COW_PORTAL;
        assert!(
            recipe_errors(&recipe).is_empty(),
            "portal outputs are legal"
        );
        recipe.inputs[0].quantity = 0;
        recipe.numinputs = 1;
        assert!(
            recipe_errors(&recipe).is_empty(),
            "zero input quantity means one"
        );
        recipe.numinputs = 2;
        assert!(
            recipe_errors(&recipe).is_empty(),
            "unmatched extra items are legal"
        );
        recipe.enabled = 0;
        recipe.numinputs = 99;
        assert!(recipe_errors(&recipe).is_empty());
    }
}
