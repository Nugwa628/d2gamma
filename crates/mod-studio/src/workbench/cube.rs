//! An in-memory host for the engine cube transmute. Placement always succeeds;
//! quest portal outputs are reported as events and no save or game is modified.
use super::{Engine, Game, Stats};
use anyhow::{Context, Result};
use d2_sim::{
    items::{self, Item, ItemStats, ItemTables},
    rng::Seed,
    units::UnitId,
    world::cube::{CraftProperty, CubeWorld, ItemRequest, StatRead},
};
use std::sync::Arc;
pub struct CubeHost {
    pub tables: Arc<ItemTables>,
    pub game: Game,
    pub entries: Vec<Option<Item<Stats>>>,
    pub inventory: Vec<UnitId>,
    pub level: i32,
    pub class: u8,
    pub day: u8,
    pub weekday: u8,
    pub portal: bool,
}
impl CubeHost {
    fn get(&self, id: UnitId) -> Option<&Item<Stats>> {
        self.entries.get(id.0.checked_sub(1)? as usize)?.as_ref()
    }
    fn get_mut(&mut self, id: UnitId) -> Option<&mut Item<Stats>> {
        self.entries
            .get_mut(id.0.checked_sub(1)? as usize)?
            .as_mut()
    }
    fn insert(&mut self, item: Item<Stats>) -> UnitId {
        self.entries.push(Some(item));
        UnitId(self.entries.len() as u32)
    }
}
#[allow(unused_variables)]
impl CubeWorld for CubeHost {
    fn expansion(&self) -> bool {
        true
    }
    fn game_type(&self) -> u8 {
        0
    }
    fn ladder(&self) -> bool {
        true
    }
    fn difficulty(&self) -> u8 {
        self.game.difficulty
    }
    fn item_format(&self) -> u16 {
        101
    }
    fn local_date(&self) -> (u8, u8) {
        (self.day, self.weekday)
    }
    fn game_seed(&mut self) -> &mut Seed {
        &mut self.game.seed
    }
    fn player_class(&self, player: UnitId) -> u8 {
        self.class
    }
    fn stat(&self, unit: UnitId, read: StatRead, stat: u16) -> i32 {
        if unit.0 == 0 {
            if stat == 12 {
                self.level
            } else {
                0
            }
        } else {
            self.get(unit).map_or(0, |i| match read {
                StatRead::Value => i.stats.stat(stat, 0),
                StatRead::Base => i.stats.base(stat, 0),
                StatRead::Bonus => i.stats.stat(stat, 0) - i.stats.base(stat, 0),
            })
        }
    }
    fn set_stat(&mut self, unit: UnitId, stat: u16, value: i32) {
        if let Some(i) = self.get_mut(unit) {
            i.stats.set_base(stat, 0, value);
        }
    }
    fn attach_sound(&mut self, player: UnitId, event: u8) {}
    fn send(&mut self, player: UnitId, msg: &[u8]) {}
    fn interaction(&self, player: UnitId) -> Option<(u8, u32)> {
        None
    }
    fn set_interaction(&mut self, player: UnitId, unit_type: u8, guid: u32) {}
    fn reset_interaction(&mut self, player: UnitId) {}
    fn inventory_pass(&mut self, player: UnitId) {}
    fn interacting_with_stash(&self, player: UnitId) -> bool {
        false
    }
    fn trading(&self, player: UnitId) -> bool {
        false
    }
    fn inventory(&self, player: UnitId) -> Vec<UnitId> {
        self.inventory.clone()
    }
    fn item_by_guid(&self, guid: u32) -> Option<UnitId> {
        self.get(UnitId(guid)).map(|_| UnitId(guid))
    }
    fn item_guid(&self, item: UnitId) -> u32 {
        item.0
    }
    fn item_page(&self, item: UnitId) -> u8 {
        self.get(item).map_or(255, |i| i.inv_page)
    }
    fn set_item_page(&mut self, item: UnitId, page: u8) {
        if let Some(i) = self.get_mut(item) {
            i.inv_page = page;
        }
    }
    fn item_mode(&self, item: UnitId) -> u8 {
        0
    }
    fn set_item_mode(&mut self, item: UnitId, mode: u8) {}
    fn item_class(&self, item: UnitId) -> Option<u32> {
        self.get(item).map(|i| i.record as u32)
    }
    fn set_item_class(&mut self, item: UnitId, class: u32) {
        if let Some(i) = self.get_mut(item) {
            i.record = class as usize;
        }
    }
    fn class_is_type(&self, class: u32, ty: u16) -> bool {
        self.tables.is_type(class as usize, ty as i16)
    }
    fn item_quality(&self, item: UnitId) -> u8 {
        self.get(item).map_or(2, |i| i.quality)
    }
    fn item_file_index(&self, item: UnitId) -> u32 {
        self.get(item).map_or(0, |i| i.file_index as u32)
    }
    fn item_level(&self, item: UnitId) -> i32 {
        self.get(item).map_or(1, |i| i.ilvl)
    }
    fn set_item_level(&mut self, item: UnitId, level: i32) {
        if let Some(i) = self.get_mut(item) {
            i.ilvl = level;
        }
    }
    fn item_flags(&self, item: UnitId) -> u32 {
        self.get(item).map_or(0, |i| i.flags)
    }
    fn set_item_flag(&mut self, item: UnitId, flag: u32) {
        if let Some(i) = self.get_mut(item) {
            i.flags |= flag;
        }
    }
    fn item_sockets(&self, item: UnitId) -> i32 {
        self.get(item).map_or(0, |i| i.stats.stat(194, 0))
    }
    fn max_sockets(&self, item: UnitId) -> i32 {
        self.get(item)
            .map_or(0, |i| items::create::max_sockets(&self.tables, i))
    }
    fn add_sockets(&mut self, item: UnitId, n: i32) {
        if let Some(i) = self.get_mut(item) {
            i.stats.set_base(194, 0, n);
        }
    }
    fn item_seed(&mut self, item: UnitId) -> &mut Seed {
        &mut self.entries[item.0 as usize - 1]
            .as_mut()
            .expect("live cube item")
            .item_seed
    }
    fn socketed(&self, item: UnitId) -> Vec<UnitId> {
        vec![]
    }
    fn duplicate(&mut self, item: UnitId, fillers: bool) -> Option<UnitId> {
        let copy = self.get(item)?.clone();
        Some(self.insert(copy))
    }
    fn item_init(&mut self, item: UnitId) -> Option<UnitId> {
        let tables = self.tables.clone();
        let mut value = self.get(item)?.clone();
        items::create::init_item_stats(&tables, &mut self.game, &mut value, None, false).ok()?;
        *self.entries[item.0 as usize - 1].as_mut()? = value;
        Some(item)
    }
    fn create_item(&mut self, request: &ItemRequest) -> Option<UnitId> {
        let mut rq = items::ItemRequest {
            item: request.class as i32,
            ilvl: request.level,
            format: request.item_format,
            quality: request.quality,
            index: i32::from(request.item_index),
            flags2: request.flags2,
            prefix: request.prefix.map(i32::from),
            suffix: request.suffix.map(i32::from),
            ..Default::default()
        };
        let created = items::create::create_item(
            &self.tables,
            &mut self.game,
            &mut rq,
            false,
            Stats::default(),
            0,
        )
        .ok()?;
        Some(self.insert(created.item))
    }
    fn tempered_affix(&mut self, item: UnitId, prefix: bool) -> u16 {
        let tables = self.tables.clone();
        self.get_mut(item)
            .map_or(0, |i| items::affixes::rare_name(&tables, i, prefix))
    }
    fn set_tempered(&mut self, item: UnitId, prefix: u16, suffix: u16) {
        if let Some(i) = self.get_mut(item) {
            i.quality = 9;
            i.rare_prefix = prefix;
            i.rare_suffix = suffix;
        }
    }
    fn unique_found(&self, index: u32) -> bool {
        self.game.uniques.get(index)
    }
    fn set_unique_found(&mut self, index: u32, found: bool) {
        if index <= items::UniqueBits::MAX {
            let word = &mut self.game.uniques.0[(index >> 5) as usize];
            let mask = 1 << (index & 31);
            if found {
                *word |= mask;
            } else {
                *word &= !mask;
            }
        }
    }
    fn drop_runeword_stats(&mut self, item: UnitId) {
        if let Some(i) = self.get_mut(item) {
            i.flags &= !items::flag::RUNEWORD;
            i.stats.lists.remove(&items::ListKey {
                state: 171,
                flags: 0x40,
            });
        }
    }
    fn add_craft_property(&mut self, item: UnitId, prop: &CraftProperty) {
        let tables = self.tables.clone();
        if let Some(i) = self.get_mut(item) {
            items::props::apply_property(
                &tables,
                i,
                &mut items::props::PropCtx::item(items::props::mode::CRAFT),
                &items::tables::PropRec {
                    code: prop.property,
                    param: prop.param,
                    min: prop.min,
                    max: prop.max,
                },
            );
        }
    }
    fn repair(&mut self, item: UnitId) {
        if let Some(i) = self.get_mut(item) {
            i.flags &= !items::flag::BROKEN;
            i.stats.set_base(72, 0, i.stats.stat(73, 0));
        }
    }
    fn recharge(&mut self, item: UnitId) {
        if let Some(i) = self.get_mut(item) {
            for list in i.stats.lists.values_mut() {
                let entries: Vec<_> = list
                    .iter()
                    .filter(|((id, _), _)| *id == 204)
                    .map(|((_, layer), value)| (*layer, *value))
                    .collect();
                items::recharge::recharge(&entries, |layer, value| {
                    list.insert((204, layer), value);
                });
            }
        }
    }
    fn place(&mut self, player: UnitId, item: UnitId) -> bool {
        if !self.inventory.contains(&item) {
            self.inventory.push(item);
        }
        true
    }
    fn free_item(&mut self, item: UnitId) {
        if let Some(i) = self.entries.get_mut(item.0 as usize - 1) {
            *i = None;
        }
    }
    fn remove_cube_item(&mut self, player: UnitId, item: UnitId) {
        self.inventory.retain(|&i| i != item);
        if let Some(i) = self.entries.get_mut(item.0 as usize - 1) {
            *i = None;
        }
    }
    fn targeting_reset(&mut self, player: UnitId) {}
    fn put_item_check(&self, player: UnitId, item: u32) -> u32 {
        0
    }
    fn cube_check(&self, player: UnitId, cube: u32) -> bool {
        false
    }
    fn quest_item_hook(&mut self, player: UnitId, item: UnitId, code: [u8; 4]) {}
    fn cow_portal(&mut self, player: UnitId) -> bool {
        self.portal = true;
        true
    }
}

#[derive(Clone, Debug)]
pub struct CubeInput {
    pub code: String,
    pub quality: u8,
    pub level: i32,
    pub quantity: i32,
    pub sockets: i32,
    pub ethereal: bool,
}
#[derive(Clone, Debug)]
pub struct CubeOptions {
    pub difficulty: u8,
    pub level: i32,
    pub class: u8,
    pub day: u8,
    pub weekday: u8,
    pub seed: u32,
}
impl Default for CubeOptions {
    fn default() -> Self {
        Self {
            difficulty: 0,
            level: 99,
            class: 0,
            day: 1,
            weekday: 1,
            seed: 1,
        }
    }
}
impl Engine {
    pub fn cube(&self, inputs: &[CubeInput], options: &CubeOptions) -> Result<String> {
        anyhow::ensure!(inputs.len() <= 48, "At most 48 cube inputs");
        let mut host = CubeHost {
            tables: self.items.clone(),
            game: Game::new(options.seed, options.difficulty),
            entries: vec![],
            inventory: vec![],
            level: options.level,
            class: options.class,
            day: options.day,
            weekday: options.weekday,
            portal: false,
        };
        anyhow::ensure!(options.difficulty < 3, "Difficulty must be 0–2");
        for input in inputs {
            anyhow::ensure!(
                (1..=9).contains(&input.quality)
                    && input.level > 0
                    && input.quantity > 0
                    && input.sockets >= 0,
                "Input quality must be 1–9; level and quantity positive; sockets nonnegative"
            );
            let code = d2_data::compile::code4(input.code.as_bytes());
            let record = self
                .items
                .find_code(code)
                .context(format!("Unknown input {}", input.code))?;
            let mut request = items::ItemRequest {
                item: record as i32,
                ilvl: input.level,
                quality: input.quality,
                format: 101,
                flags2: items::req::NO_SOCKETS
                    | if input.ethereal {
                        items::req::ALWAYS_ETHEREAL
                    } else {
                        items::req::NEVER_ETHEREAL
                    },
                ..Default::default()
            };
            let mut item = items::create::create_item(
                &self.items,
                &mut host.game,
                &mut request,
                false,
                Stats::default(),
                0,
            )?
            .item;
            item.inv_page = 3;
            item.flags |= items::flag::IDENTIFIED;
            if input.sockets > 0 {
                item.flags |= items::flag::SOCKETED;
            } else {
                item.flags &= !items::flag::SOCKETED;
            }
            item.stats.set_base(70, 0, input.quantity);
            item.stats.set_base(194, 0, input.sockets);
            let id = host.insert(item);
            host.inventory.push(id);
        }
        let result = self.cube.transmute(&mut host, UnitId(0));
        let Some(row) = result.record else {
            return Ok("No recipe matches these inputs and conditions.".into());
        };
        let label = self
            .row_keys
            .get("cubemain")
            .and_then(|names| names.get(row))
            .map(String::as_str)
            .unwrap_or("");
        let mut out = format!("Recipe #{row} · {label}\nCommitted: {}", result.committed);
        if host.portal {
            out.push_str("\nPortal event");
        }
        for id in result.outputs {
            if let Some(item) = host.get(id) {
                out.push_str(&format!(
                    "\n{} · quality {} · level {}",
                    self.item_label(
                        String::from_utf8_lossy(&self.items.items[item.record].code).trim()
                    ),
                    super::quality_name(item.quality),
                    item.ilvl
                ));
                for line in self.tips.tip_lines_of(
                    &self.bits(item)?,
                    &d2_item_text::item_tip_build::TipCtx {
                        player: Some(&super::PreviewViewer),
                        ..Default::default()
                    },
                ) {
                    out.push('\n');
                    out.push_str(&String::from_utf16_lossy(&line.text));
                }
            }
        }
        Ok(out)
    }
}
