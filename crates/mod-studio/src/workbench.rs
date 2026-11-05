//! Studio snapshots compiled by the engine, and deterministic hosts for its item,
//! treasure and cube code. No UI state or file writes enter a simulation.
use anyhow::{bail, Context, Result};
use d2_data::{
    bin::{BinSet, BinTable, DataRules},
    patch::PatchData,
    strings::StringTables,
    tables::*,
};
use d2_item_text::{
    item_tip::{ItemTips, TipLine},
    item_tip_build::{TipCtx, TipUnit},
    StringLookup,
};
use d2_sim::{
    items::{self, Item, ItemGame, ItemStats, ItemTables, ListKey, UniqueBits},
    rng::Seed,
    treasure::*,
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

#[derive(Clone)]
pub struct Snapshot {
    pub data: PatchData,
    pub native: Option<PathBuf>,
    pub strings: StringTables,
    pub drop_rules: d2_sim::treasure::rules::DropRules,
}
impl Snapshot {
    pub fn from_tables(t: &crate::tables::Tables) -> Self {
        Self {
            data: t.edited.clone(),
            native: t.native.clone(),
            strings: StringTables::default(),
            drop_rules: t.drop_rules,
        }
    }
    pub fn compile(&self) -> Result<Engine> {
        let mut strings = self.strings.clone();
        if let Some(native) = &self.native {
            let source = d2_native::source::NativeSource::open_with(native, Some(&[]))?;
            for (name, slot) in [
                ("string.tbl", &mut strings.base),
                ("patchstring.tbl", &mut strings.patch),
                ("expansionstring.tbl", &mut strings.expansion),
            ] {
                match source.read_native(&format!("data/local/lng/eng/{name}")) {
                    Some(Ok(d2_native::source::NativeAsset::Tbl(t))) => *slot = Some(t),
                    Some(Err(e)) => return Err(anyhow::Error::msg(e)),
                    _ => bail!("Missing English string table {name}"),
                }
            }
        }
        for edit in &self.data.strings {
            strings.apply(&edit.edit)?;
        }
        let compiled = d2_data::compile_set::compile_all_under(
            &mut |file| {
                Ok(self
                    .data
                    .table(file.trim_end_matches(".txt").to_ascii_lowercase().as_str())
                    .map(|t| ("Studio snapshot".into(), t.render())))
            },
            &strings,
            &d2_data::calc::NO_KEYWORDS,
            DataRules::Mod,
        )?;
        let tables = compiled
            .tables
            .into_iter()
            .map(|t| BinTable {
                name: t.name,
                source: t.txt_source,
                count: t.compiled.count,
                record_size: t.compiled.record_size,
                records: t.compiled.records,
            })
            .collect();
        let empty = || d2_data::txt::TxtTable {
            header: vec![],
            records: vec![],
            removed_lines: vec![],
        };
        let mut set = BinSet {
            lod: true,
            rules: DataRules::Mod,
            strings,
            tables,
            wide: BTreeMap::new(),
            code: compiled
                .buffers
                .into_iter()
                .map(|(buffer, bytes)| {
                    (
                        buffer,
                        d2_data::bin::CodeFile {
                            buffer,
                            bytes,
                            source: "Studio".into(),
                        },
                    )
                })
                .collect(),
            code_reports: BTreeMap::new(),
            hitclass: BinTable {
                name: "hitclass".into(),
                source: "Studio".into(),
                count: 0,
                record_size: 4,
                records: vec![],
            },
            sounds: empty(),
            soundenviron: empty(),
        };
        let fixed = d2_data::fixup::apply(
            &set,
            &d2_formats::animdata::AnimData {
                buckets: vec![vec![]; 256],
            },
        )?;
        let items = Arc::new(ItemTables::from_fixed(&fixed)?);
        set.tables = fixed.tables.clone();
        let tips = ItemTips::new(items.clone(), &set, Arc::new(Names::new(&set.strings)))
            .map_err(anyhow::Error::msg)?;
        let weapons = typed::<Weapons>(&set)?;
        let armor = typed::<Armor>(&set)?;
        let misc = typed::<Misc>(&set)?;
        let drop_items = item_list(&weapons, &armor, &misc);
        let types = typed::<Itemtypes>(&set)?;
        let tcs = TreasureClasses::build(&TcSources {
            treasureclassex: &typed::<Treasureclassex>(&set)?,
            items: &drop_items,
            itemtypes: &types,
            equiv: &fixed.itemtypes_equiv,
            uniqueitems: &typed::<Uniqueitems>(&set)?,
            setitems: &typed::<Setitems>(&set)?,
        })?;
        let cube = d2_sim::world::cube::CubeData::from_fixed(&fixed)?;
        let row_keys = self
            .data
            .tables
            .iter()
            .map(|t| {
                (
                    t.name().to_owned(),
                    (0..t.rows.len())
                        .map(|r| crate::tables::text(t.key(r)))
                        .collect(),
                )
            })
            .collect();
        let names = Names::new(&set.strings);
        let item_names = self
            .data
            .tables
            .iter()
            .filter(|t| ["weapons", "armor", "misc"].contains(&t.name()))
            .flat_map(|t| {
                let col = t
                    .header
                    .iter()
                    .position(|h| h.eq_ignore_ascii_case(b"namestr"));
                (0..t.rows.len())
                    .map(|r| {
                        let code = crate::tables::text(t.key(r));
                        let key = col
                            .map(|c| crate::tables::text(&t.rows[r].cells[c]))
                            .unwrap_or_else(|| code.clone());
                        let name = names.get(&key).map(String::from_utf16_lossy).unwrap_or(key);
                        (code, name)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        Ok(Engine {
            item_names,
            drop_rules: self.drop_rules,
            row_keys,
            set,
            items,
            tips,
            tcs,
            drop_items,
            cube,
        })
    }
}
fn typed<T: Record>(set: &BinSet) -> Result<Vec<T>> {
    Ok(decode_all(set.table(T::TABLE).context(T::TABLE)?)?)
}
pub struct Engine {
    pub item_names: BTreeMap<String, String>,
    pub drop_rules: d2_sim::treasure::rules::DropRules,
    pub row_keys: BTreeMap<String, Vec<String>>,
    pub set: BinSet,
    pub items: Arc<ItemTables>,
    pub tips: ItemTips,
    pub tcs: TreasureClasses,
    pub drop_items: Vec<ItemData>,
    pub cube: d2_sim::world::cube::CubeData,
}
struct Names {
    ids: BTreeMap<u16, Vec<u16>>,
    keys: BTreeMap<String, Vec<u16>>,
}
impl Names {
    fn new(s: &StringTables) -> Self {
        let mut ids = BTreeMap::new();
        let mut keys = BTreeMap::new();
        for id in 0..=u16::MAX {
            if let Ok(text) = s.by_index(u32::from(id)) {
                ids.insert(id, text);
            }
        }
        for t in [&s.base, &s.patch, &s.expansion].into_iter().flatten() {
            for e in &t.entries {
                if e.used != 0 {
                    let key = String::from_utf8_lossy(&e.key).into_owned();
                    let id = s.strkey(&e.key);
                    if let Some(text) = ids.get(&id) {
                        keys.insert(key, text.clone());
                    }
                }
            }
        }
        for (key, id, text) in s.layer.entries() {
            keys.insert(String::from_utf8_lossy(key).into_owned(), text.to_vec());
            ids.insert(id as u16, text.to_vec());
        }
        Self { ids, keys }
    }
}
impl StringLookup for Names {
    fn get(&self, key: &str) -> Option<&[u16]> {
        self.keys.get(key).map(Vec::as_slice)
    }
    fn get_id(&self, id: u16) -> Option<&[u16]> {
        self.ids.get(&id).map(Vec::as_slice)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub base: BTreeMap<(u16, u16), i32>,
    pub lists: BTreeMap<ListKey, BTreeMap<(u16, u16), i32>>,
}
impl ItemStats for Stats {
    fn has_stats(&self) -> bool {
        true
    }
    fn stat(&self, id: u16, l: u16) -> i32 {
        self.base(id, l)
            + self
                .lists
                .values()
                .map(|m| m.get(&(id, l)).copied().unwrap_or(0))
                .sum::<i32>()
    }
    fn base(&self, id: u16, l: u16) -> i32 {
        self.base.get(&(id, l)).copied().unwrap_or(0)
    }
    fn set_base(&mut self, id: u16, l: u16, v: i32) {
        self.base.insert((id, l), v);
    }
    fn has_list(&self, k: ListKey) -> bool {
        self.lists.contains_key(&k)
    }
    fn list_set(&mut self, k: ListKey, id: u16, l: u16, v: i32) {
        self.lists.entry(k).or_default().insert((id, l), v);
    }
    fn list_add(&mut self, k: ListKey, id: u16, l: u16, v: i32) {
        *self.lists.entry(k).or_default().entry((id, l)).or_default() += v;
    }
    fn list_get(&self, k: ListKey, id: u16, l: u16) -> i32 {
        self.lists
            .get(&k)
            .and_then(|m| m.get(&(id, l)))
            .copied()
            .unwrap_or(0)
    }
}
/// Neutral viewer: enables the engine's damage lines without player bonuses.
pub struct PreviewViewer;
impl TipUnit for PreviewViewer {
    fn unit_type(&self) -> u8 {
        0
    }
    fn class(&self) -> u32 {
        0
    }
    fn stat(&self, _id: u16, _layer: u16) -> i32 {
        0
    }
}
pub struct Game {
    pub seed: Seed,
    pub difficulty: u8,
    pub uniques: UniqueBits,
}
impl Game {
    pub fn new(seed: u32, difficulty: u8) -> Self {
        Self {
            seed: Seed::init_low(seed),
            difficulty,
            uniques: UniqueBits::default(),
        }
    }
}
impl ItemGame for Game {
    fn seed(&mut self) -> &mut Seed {
        &mut self.seed
    }
    fn difficulty(&self) -> u8 {
        self.difficulty
    }
    fn expansion(&self) -> bool {
        true
    }
    fn ladder_flags(&self) -> (bool, bool) {
        (true, true)
    }
    fn tick_rate(&self) -> d2_sim::tick::TickRate {
        d2_sim::tick::TickRate::GAME
    }
    fn uniques(&mut self) -> &mut UniqueBits {
        &mut self.uniques
    }
}

impl Engine {
    pub fn preview(&self, table: &str, row: usize, base: usize, seed: u32) -> Result<Vec<TipLine>> {
        anyhow::ensure!(
            base < self.items.items.len(),
            "Choose a valid base item for this row"
        );
        let mut item = Item::new(base, 101, Stats::default());
        item.ilvl = 99;
        item.quality = 2;
        item.flags = items::flag::IDENTIFIED;
        item.item_seed = Seed::init_low(seed);
        items::create::init_item_stats(
            &self.items,
            &mut Game::new(seed, 0),
            &mut item,
            None,
            false,
        )?;
        let mut fillers = Vec::new();
        match table {
            "uniqueitems" => {
                item.file_index = row as i32;
                item.quality = 7;
                items::props::apply_unique(&self.items, &mut item);
            }
            "setitems" => {
                item.file_index = row as i32;
                item.quality = 5;
                items::props::apply_set_item(&self.items, &mut item);
            }
            "magicprefix" | "magicsuffix" | "automagic" => {
                let id = match table {
                    "magicsuffix" => row,
                    "magicprefix" => self.items.n_suffix + row,
                    _ => self.items.n_suffix + self.items.n_prefix + row,
                } + 1;
                items::props::apply_affix(&self.items, &mut item, id as u16);
                match table {
                    "magicsuffix" => {
                        item.quality = 4;
                        item.suffix[0] = row as u16 + 1;
                    }
                    "magicprefix" => {
                        item.quality = 4;
                        item.prefix[0] = items::bitstream::PREFIX_OFFSET + row as u16 + 1;
                    }
                    _ => item.auto_affix = items::bitstream::AUTO_OFFSET + row as u16 + 1,
                }
            }
            "runes" => {
                let rune = self.items.runes.get(row).context("runeword row")?;
                let records: Vec<_> = rune
                    .runes
                    .iter()
                    .copied()
                    .take_while(|&id| id >= 1)
                    .map(|id| id as usize)
                    .collect();
                anyhow::ensure!(!records.is_empty(), "Runeword has no rune inputs");
                item.flags |= items::flag::SOCKETED;
                item.stats.set_base(194, 0, records.len() as i32);
                anyhow::ensure!(items::props::runeword_match(&self.items,&item,&records)==Some(row), "Choose a compatible base for this enabled runeword (an earlier matching row may take precedence)");
                let apply_type = self.items.items[base].gemapplytype;
                let mut rune_items = Vec::new();
                for (i, record) in records.into_iter().enumerate() {
                    self.items.item(record).context("Runeword rune item")?;
                    let mut filler = Item::new(record, 101, Stats::default());
                    filler.flags = items::flag::IDENTIFIED;
                    filler.quality = 2;
                    filler.ilvl = 99;
                    filler.item_seed = Seed::init_low(seed.wrapping_add(i as u32 + 1));
                    items::create::init_item_stats(
                        &self.items,
                        &mut Game::new(seed, 0),
                        &mut filler,
                        None,
                        false,
                    )?;
                    items::props::apply_socket_filler_into(
                        &self.items,
                        &mut filler,
                        apply_type,
                        Some(&mut item.stats),
                    );
                    rune_items.push(filler);
                }
                anyhow::ensure!(
                    items::props::activate_runeword(
                        &self.items,
                        rune_items.last_mut().context("Last rune")?,
                        &mut item.stats,
                        row,
                        true
                    ),
                    "Runeword did not activate"
                );
                item.flags |= items::flag::RUNEWORD;
                item.file_index = i32::from(rune.name_id);
                fillers = rune_items
                    .iter()
                    .map(|f| self.bits(f))
                    .collect::<Result<_>>()?;
            }
            _ => {}
        }
        let bits = self.bits_with_fillers(&item, fillers.len() as u32)?;
        Ok(self.tips.tip_lines_of(
            &bits,
            &TipCtx {
                player: Some(&PreviewViewer),
                fillers,
                ..Default::default()
            },
        ))
    }
    pub fn bits(&self, item: &Item<Stats>) -> Result<d2_proto::item_bits::ItemBits> {
        self.bits_with_fillers(item, 0)
    }
    fn bits_with_fillers(
        &self,
        item: &Item<Stats>,
        filled: u32,
    ) -> Result<d2_proto::item_bits::ItemBits> {
        use items::bitstream::{self, Kind, StatEntry, StreamItem};
        let record = self.items.item(item.record).context("Item record")?;
        let is = |ty| self.items.is_type(item.record, ty);
        let list = |key| {
            item.stats.lists.get(&key).map(|m| {
                m.iter()
                    .map(|(&(stat, param), &value)| StatEntry { stat, param, value })
                    .collect()
            })
        };
        let stream = StreamItem {
            flags: item.flags,
            version: 101,
            filled,
            code: record.code,
            base_code: record.normcode,
            ilvl: item.ilvl,
            quality: item.quality,
            file_index: item.file_index,
            auto_affix: item.auto_affix,
            prefix: item.prefix,
            suffix: item.suffix,
            rare_prefix: item.rare_prefix,
            rare_suffix: item.rare_suffix,
            kind: Kind {
                armor: is(50),
                weapon: is(45),
                gold: is(4),
                charm: is(13),
                body_part: is(40) && !is(7),
                scroll_or_book: is(22) || is(18),
            },
            compact: record.compactsave != 0,
            stackable: record.stackable != 0,
            varinvgfx: self
                .items
                .itype_of(item.record)
                .is_some_and(|ty| ty.varinvgfx != 0),
            quest_diff: record.quest != 0 && record.questdiffcheck != 0,
            total_gold: item.stats.stat(14, 0),
            total_quest_diff: item.stats.stat(356, 0),
            base_defense: item.stats.base(31, 0),
            base_max_dur: item.stats.base(73, 0),
            base_sockets: item.stats.base(194, 0),
            total_dur: item.stats.stat(72, 0),
            total_quantity: item.stats.stat(70, 0),
            main: list(ListKey::ITEM),
            sets: std::array::from_fn(|k| {
                list(ListKey {
                    state: 165 + k as u16,
                    flags: 0x2040,
                })
            }),
            runeword_list: list(ListKey {
                state: 171,
                flags: 0x40,
            }),
            runeword: if item.flags & items::flag::RUNEWORD != 0 {
                item.file_index as u16
            } else {
                0xffff
            },
            ..Default::default()
        };
        let (bytes, _) = bitstream::write(&stream, &self.items.isc)?;
        d2_proto::item_bits::decode(&bytes, &d2_item_text::lookup::TablesLookup(&self.items))
            .map_err(Into::into)
    }
    pub fn drops(
        &self,
        tc: u16,
        difficulty: u8,
        n: u32,
        seed: u32,
        level: i32,
    ) -> Result<BTreeMap<(String, u8), u32>> {
        self.drops_with_options(tc, difficulty, n, seed, level, &DropOptions::default())
    }
    pub fn drops_with_options(
        &self,
        tc: u16,
        difficulty: u8,
        n: u32,
        seed: u32,
        level: i32,
        options: &DropOptions,
    ) -> Result<BTreeMap<(String, u8), u32>> {
        anyhow::ensure!((1..=8).contains(&options.players), "Players must be 1–8");
        anyhow::ensure!(n <= 1_000_000, "At most 1,000,000 trials");
        anyhow::ensure!(difficulty < 3, "Difficulty must be 0–2");
        let types = typed::<Itemtypes>(&self.set)?;
        let ratios = typed::<Itemratio>(&self.set)?;
        let data = TreasureData {
            tcs: &self.tcs,
            items: &self.drop_items,
            itemtypes: &types,
            equiv: &self.items.equiv,
            itemratio: &ratios,
        };
        let game = GameFacts {
            expansion: true,
            difficulty,
            game_type: 3,
            living_players: 1,
            players_setting: options.players,
            item_format: 101,
        };
        let dropper = Dropper {
            kind: DropperKind::Other { area_level: level },
            x: 0,
            y: 0,
        };
        let mut rng = Seed::init_low(seed);
        let mut sink = Counter {
            engine: self,
            game: Game::new(seed, difficulty),
            counts: BTreeMap::new(),
            nothing: 0,
            created: 0,
        };
        for _ in 0..n {
            let before = sink.created;
            walk(
                &data,
                &game,
                &dropper,
                &mut rng,
                Some(&Recipient {
                    party: Some(1),
                    magic_find: options.magic_find,
                    gold_find: 0,
                }),
                &WalkArgs {
                    tc: Some(tc),
                    quality: 0,
                    level,
                    find_item: false,
                    list: false,
                    max: 6,
                },
                &mut sink,
            )?;
            if sink.created == before {
                sink.nothing += 1;
            }
        }
        sink.counts.insert((String::new(), 0), sink.nothing);
        Ok(sink.counts)
    }
}
struct Counter<'a> {
    engine: &'a Engine,
    game: Game,
    counts: BTreeMap<(String, u8), u32>,
    nothing: u32,
    created: u32,
}
impl DropSink for Counter<'_> {
    fn drop_rules(&self) -> d2_sim::treasure::rules::DropRules {
        self.engine.drop_rules
    }
    type Spot = ();
    type Item = ();
    fn place(&mut self, _: i32, _: i32) -> Option<()> {
        Some(())
    }
    fn create(&mut self, r: DropRequest<()>) -> Option<()> {
        let mut request = items::ItemRequest {
            item: i32::from(r.id),
            ilvl: r.item_level,
            quality: r.quality,
            index: r.index,
            format: 101,
            ..Default::default()
        };
        let made = items::create::create_item(
            &self.engine.items,
            &mut self.game,
            &mut request,
            false,
            Stats::default(),
            0,
        )
        .ok()?;
        let code = String::from_utf8_lossy(&self.engine.items.items[made.item.record].code)
            .trim()
            .to_owned();
        *self.counts.entry((code, made.item.quality)).or_default() += 1;
        self.created += 1;
        Some(())
    }
    fn gold(&self, _: ()) -> i32 {
        0
    }
    fn set_gold(&mut self, _: (), _: i32) {}
}

pub mod cube;

#[derive(Clone, Debug)]
pub struct DropOptions {
    pub players: i32,
    pub magic_find: i32,
}
impl Default for DropOptions {
    fn default() -> Self {
        Self {
            players: 1,
            magic_find: 0,
        }
    }
}
pub fn quality_name(q: u8) -> &'static str {
    match q {
        1 => "Low",
        2 => "Normal",
        3 => "Superior",
        4 => "Magic",
        5 => "Set",
        6 => "Rare",
        7 => "Unique",
        8 => "Crafted",
        9 => "Tempered",
        _ => "Nothing dropped",
    }
}
impl Engine {
    pub fn item_label(&self, code: &str) -> String {
        format!(
            "{} ({code})",
            self.item_names
                .get(code)
                .map(String::as_str)
                .unwrap_or(code)
        )
    }
    /// Percentages are item occurrences per trial; multiple picks may exceed 100%.
    pub fn format_drops(&self, counts: &BTreeMap<(String, u8), u32>, trials: u32) -> String {
        let mut out = format!("{trials} trials · percentages per trial\n");
        let mut groups: BTreeMap<u8, Vec<(&str, u32)>> = BTreeMap::new();
        for ((code, q), count) in counts {
            groups.entry(*q).or_default().push((code, *count));
        }
        for (q, mut rows) in groups {
            rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            let subtotal: u32 = rows.iter().map(|r| r.1).sum();
            out.push_str(&format!(
                "\n{} · {subtotal} · {:.2}%\n",
                quality_name(q),
                100.0 * subtotal as f64 / trials.max(1) as f64
            ));
            let max = rows.first().map_or(1, |r| r.1.max(1));
            for (code, count) in rows {
                if q == 0 {
                    continue;
                }
                out.push_str(&format!(
                    "{} · {} · {count} · {:.2}%\n{}\n",
                    self.item_label(code),
                    quality_name(q),
                    100.0 * count as f64 / trials.max(1) as f64,
                    "━".repeat((count as u64 * 40 / max as u64) as usize)
                ));
            }
        }
        out
    }
}
