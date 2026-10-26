//! The item rules: every item is one the realm's game data can make
//! (legality), its properties lie within the ranges of its affixes, quality
//! row, unique, set or runeword (ranges), no two items share an item ID
//! (with the registry hook of plan C3), and an item ID of the stored record
//! that is still there still names the same item (kept IDs).
//!
//! The rules read the realm's tables through `d2-sim`'s item code: the
//! affix filter of the roller, the socket limits, the runeword match and
//! the property functions, so a mod's tables and property functions are
//! checked with the same code that makes its items.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use d2_formats::d2s::D2s;
use d2_sim::items::affixes::{affix, alvl, magic_fits};
use d2_sim::items::bitstream::{StatEntry, StreamItem};
use d2_sim::items::create::max_sockets_at;
use d2_sim::items::props::{
    activate_runeword, apply_affix, apply_quality_row, apply_set_item, apply_unique, runeword_row,
    FUNCS, STATE_ITEMSET1, STATE_RUNEWORD,
};
use d2_sim::items::quality::superior_fits;
use d2_sim::items::tables::{ItemRec, ItemTables, PropRec};
use d2_sim::items::{flag, q, stat, ty, Item, ItemStats, ListKey};

use crate::decoded::FoundItem;
use crate::{CheckFail, SaveCheck, SaveCtx};

/// Property functions whose stat or layer the record does not bound (a
/// skill and level in the layer, a rolled layer, packed charges): their
/// stats are not range-checked.
pub const UNBOUNDED_FUNCS: [u8; 5] = [11, 12, 18, 19, 36];
/// The highest item level.
pub const MAX_ILVL: i32 = 99;

/// The realm's item registry (plan C3, N19): who else holds an item ID.
pub trait ItemRegistry {
    /// The holder of item `id` when it is a character other than
    /// `character` (a name for the failure message), else `None`.
    fn other_holder(&self, character: &[u8], id: u32) -> Option<String>;
}

fn fail(it: &FoundItem, why: impl std::fmt::Display) -> Result<(), CheckFail> {
    Err(CheckFail::new(format!("{}: {why}", it.at())))
}

fn code_str(code: &[u8; 4]) -> String {
    String::from_utf8_lossy(code).trim_end().to_owned()
}

/// The item's codes along the upgrade chain (normal, exceptional, elite):
/// a cube upgrade keeps the unique or set row.
fn same_base(t: &ItemTables, rec: &ItemRec, code: [u8; 4]) -> bool {
    let blank = |c: &[u8; 4]| c.iter().all(|&b| b == 0 || b == b' ');
    rec.code == code
        || [rec.normcode, rec.ubercode, rec.ultracode]
            .iter()
            .any(|c| !blank(c) && *c == code)
        || t.find_code(code).and_then(|i| t.item(i)).is_some_and(|r| {
            [r.normcode, r.ubercode, r.ultracode]
                .iter()
                .any(|c| !blank(c) && *c == rec.code)
        })
}

// ------------------------------------------------------------------ legality

/// Every item decodes with the realm's tables (a known code, a valid
/// quality, set row and stat list) and is one the game can make: an item
/// level of 1–99; affixes of the right part that the affix roller can pick
/// for the item, its level and its quality; a low-quality, superior,
/// unique or set row that exists and fits the item; a runeword its runes
/// spell in an item of the right type; sockets within the base's and the
/// item level's limits; a stack within the base's maximum; a defense
/// within the base's range.
#[derive(Clone, Copy, Debug, Default)]
pub struct ItemLegality;

impl SaveCheck for ItemLegality {
    fn check(&self, _: &D2s, _: &D2s, ctx: &SaveCtx) -> Result<(), CheckFail> {
        if let Some(e) = ctx.after.errors.first() {
            return Err(CheckFail::new(format!("an item does not decode: {e}")));
        }
        let t = &ctx.tables.items;
        for it in &ctx.after.items {
            legal(t, it)?;
        }
        Ok(())
    }
}

fn legal(t: &ItemTables, it: &FoundItem) -> Result<(), CheckFail> {
    let s = &it.item;
    if it.failed {
        return fail(it, "the record is not valid for the realm's tables");
    }
    let Some(rec) = t.item(it.record) else {
        return fail(it, format!("unknown item code `{}`", code_str(&s.code)));
    };
    if rec.code != s.code {
        return fail(
            it,
            format!("code `{}` is not row {}", code_str(&s.code), it.record),
        );
    }
    if s.stackable && rec.stackable != 0 {
        let extra = stat_sum(s.main.as_deref(), stat::EXTRA_STACK);
        let cap = i64::from(rec.maxstack) + extra;
        if !(0..=cap).contains(&i64::from(s.total_quantity)) {
            return fail(it, format!("quantity {} (most {cap})", s.total_quantity));
        }
    }
    if s.compact || s.flags & flag::EAR != 0 {
        return Ok(());
    }
    if !(1..=MAX_ILVL).contains(&s.ilvl) {
        return fail(it, format!("item level {}", s.ilvl));
    }
    if !(q::LOW..=q::TEMPERED).contains(&s.quality) {
        return fail(it, format!("quality {}", s.quality));
    }
    if t.is_type(it.record, ty::ARMO as i16) && rec.maxac != 0 {
        let mut most = i64::from(rec.maxac) + 1;
        if s.flags & flag::ETHEREAL != 0 {
            most = most * 3 / 2;
        }
        if !(0..=most).contains(&i64::from(s.base_defense)) {
            return fail(it, format!("defense {} (base most {most})", s.base_defense));
        }
    }
    sockets(t, it, rec)?;
    if s.version >= 1 {
        quality(t, it, rec)?;
    }
    runeword(t, it)
}

fn stat_sum(list: Option<&[StatEntry]>, id: u16) -> i64 {
    list.unwrap_or_default()
        .iter()
        .filter(|e| e.stat == id)
        .map(|e| i64::from(e.value))
        .sum()
}

fn sockets(t: &ItemTables, it: &FoundItem, rec: &ItemRec) -> Result<(), CheckFail> {
    let s = &it.item;
    let n = if s.flags & flag::SOCKETED != 0 {
        s.base_sockets
    } else {
        0
    };
    if n > 0 {
        let grid = (i32::from(rec.invwidth) * i32::from(rec.invheight)).min(6);
        let most = max_sockets_at(t, it.record, s.ilvl).min(grid);
        if n > most {
            return fail(
                it,
                format!("{n} sockets (most {most} at item level {})", s.ilvl),
            );
        }
    }
    if it.fillers.len() as i32 > n.max(0) {
        return fail(it, format!("{} items in {n} sockets", it.fillers.len()));
    }
    Ok(())
}

fn quality(t: &ItemTables, it: &FoundItem, rec: &ItemRec) -> Result<(), CheckFail> {
    let s = &it.item;
    let fi = usize::try_from(s.file_index).ok();
    let ilvl = s.ilvl;
    match s.quality {
        q::LOW => {
            if fi.is_none_or(|i| i >= t.n_lowquality) {
                return fail(it, format!("low-quality row {}", s.file_index));
            }
        }
        q::SUPERIOR => {
            let probe = probe(it);
            if fi.is_none_or(|i| !superior_fits(t, &probe, i)) {
                return fail(it, format!("superior row {} does not fit", s.file_index));
            }
        }
        q::UNIQUE => {
            let Some(u) = fi.and_then(|i| t.uniques.get(i)) else {
                return fail(it, format!("unique row {} does not exist", s.file_index));
            };
            if !u.enabled {
                return fail(it, format!("unique row {} is disabled", s.file_index));
            }
            if !same_base(t, rec, u.code) {
                return fail(
                    it,
                    format!("unique row {} is a `{}`", s.file_index, code_str(&u.code)),
                );
            }
            if rec.quest == 0 && i32::from(u.lvl) > ilvl {
                return fail(
                    it,
                    format!("unique row {} needs item level {}", s.file_index, u.lvl),
                );
            }
        }
        q::SET => {
            let Some(si) = fi.and_then(|i| t.setitems.get(i)) else {
                return fail(it, format!("set row {} does not exist", s.file_index));
            };
            if !same_base(t, rec, si.item) {
                return fail(
                    it,
                    format!("set row {} is a `{}`", s.file_index, code_str(&si.item)),
                );
            }
            if i32::from(si.lvl) > ilvl {
                return fail(
                    it,
                    format!("set row {} needs item level {}", s.file_index, si.lvl),
                );
            }
        }
        _ => {}
    }
    affixes(t, it, rec)
}

/// An `Item` with the stream item's fields, for `d2-sim`'s fit tests.
fn probe(it: &FoundItem) -> Item<Recorder> {
    let s = &it.item;
    let mut item = Item::new(it.record, s.version, Recorder::default());
    item.ilvl = s.ilvl;
    item.quality = s.quality;
    item.file_index = s.file_index;
    item.prefix = s.prefix;
    item.suffix = s.suffix;
    item.rare_prefix = s.rare_prefix;
    item.rare_suffix = s.rare_suffix;
    item.auto_affix = s.auto_affix;
    item.flags = s.flags;
    if s.flags & flag::SOCKETED != 0 {
        item.stats
            .base
            .insert((stat::NUMSOCKETS, 0), s.base_sockets);
    }
    item
}

/// The parts of the magic array: suffixes, prefixes, automagic.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Suffix,
    Prefix,
    Auto,
}

fn part_of(t: &ItemTables, id: u16) -> Option<Part> {
    let i = usize::from(id).checked_sub(1)?;
    if i >= t.magic.len() {
        None
    } else if i < t.n_suffix {
        Some(Part::Suffix)
    } else if i < t.n_suffix + t.n_prefix {
        Some(Part::Prefix)
    } else if i >= t.first_auto() {
        Some(Part::Auto)
    } else {
        None
    }
}

fn affixes(t: &ItemTables, it: &FoundItem, rec: &ItemRec) -> Result<(), CheckFail> {
    let s = &it.item;
    let books = t.is_type(it.record, ty::SCRO as i16) || t.is_type(it.record, ty::BOOK as i16);
    let suffixes: &[u16] = if books { &s.suffix[1..] } else { &s.suffix };
    let used = |slots: &[u16]| slots.iter().filter(|&&a| a != 0).count();
    let (np, ns) = (used(&s.prefix), used(suffixes));
    let rare = matches!(s.quality, q::RARE | q::CRAFTED | q::TEMPERED);
    match s.quality {
        q::MAGIC => {
            if np + ns == 0 {
                return fail(it, "a magic item without affixes");
            }
            if np > 1 || ns > 1 {
                return fail(it, "a magic item with more than one prefix or suffix");
            }
        }
        q::RARE | q::CRAFTED | q::TEMPERED => {
            for (id, prefix) in [(s.rare_prefix, true), (s.rare_suffix, false)] {
                let first = if prefix { t.n_rare_suffix } else { 0 };
                let end = if prefix {
                    t.rare.len()
                } else {
                    t.n_rare_suffix
                };
                let i = usize::from(id).wrapping_sub(1);
                if !(first..end).contains(&i) {
                    return fail(it, format!("rare name {id}"));
                }
            }
        }
        _ => {
            if np + used(&s.suffix) > 0 && !books {
                return fail(it, format!("quality {} with magic affixes", s.quality));
            }
        }
    }
    let probe = probe(it);
    let a = alvl(ilvl_of(s), i32::from(rec.level), i32::from(rec.magic_lvl));
    let class = t.itype_of(it.record).map_or(7, |x| x.class.min(7));
    let mut groups = BTreeSet::new();
    let slots = s
        .prefix
        .iter()
        .map(|&id| (id, Part::Prefix))
        .chain(suffixes.iter().map(|&id| (id, Part::Suffix)));
    for (id, want) in slots.filter(|&(id, _)| id != 0) {
        if part_of(t, id) != Some(want) {
            return fail(it, format!("affix {id} is not a {}", part_name(want)));
        }
        let Some(row) = affix(t, id) else {
            return fail(it, format!("affix {id} does not exist"));
        };
        let why = if row.spawnable == 0 {
            Some("is not spawnable".to_owned())
        } else if row.version >= 100 && s.version < 100 {
            Some("is an expansion affix on a classic item".to_owned())
        } else if row.level > a || (row.maxlevel != 0 && row.maxlevel < a) {
            Some(format!(
                "has levels {}–{} and the item's affix level is {a}",
                row.level, row.maxlevel
            ))
        } else if rare && row.rare == 0 {
            Some("is not a rare affix".to_owned())
        } else if !magic_fits(t, &probe, row) {
            Some("does not fit the item's type".to_owned())
        } else if row.frequency == 0 {
            Some("has frequency 0".to_owned())
        } else if !(row.classspecific == 0xFF || class == 7 || class == row.classspecific) {
            Some(format!("is for class {}", row.classspecific))
        } else if !groups.insert(row.group) {
            Some(format!("repeats affix group {}", row.group))
        } else {
            None
        };
        if let Some(why) = why {
            return fail(it, format!("affix {id} {why}"));
        }
    }
    if s.auto_affix != 0 {
        let id = s.auto_affix;
        let Some(row) = affix(t, id).filter(|_| part_of(t, id) == Some(Part::Auto)) else {
            return fail(it, format!("automagic affix {id} does not exist"));
        };
        if row.group != i32::from(rec.auto_prefix) {
            return fail(
                it,
                format!(
                    "automagic affix {id} is not of the base's group {}",
                    rec.auto_prefix
                ),
            );
        }
        if row.level > a || (row.maxlevel != 0 && row.maxlevel < a) || !magic_fits(t, &probe, row) {
            return fail(it, format!("automagic affix {id} does not fit the item"));
        }
    }
    Ok(())
}

fn ilvl_of(s: &StreamItem) -> i32 {
    s.ilvl.max(1)
}

fn part_name(p: Part) -> &'static str {
    match p {
        Part::Prefix => "prefix",
        Part::Suffix => "suffix",
        Part::Auto => "automagic affix",
    }
}

fn runeword(t: &ItemTables, it: &FoundItem) -> Result<(), CheckFail> {
    let s = &it.item;
    if s.flags & flag::RUNEWORD == 0 {
        return Ok(());
    }
    let sockets = s.base_sockets.clamp(0, 255) as u8;
    let Some(row) = runeword_row(t, it.record, s.quality, sockets, &it.fillers) else {
        return fail(it, "a runeword its runes and base do not make");
    };
    let name = t.runes[row].name_id;
    if s.runeword != 0xFFFF && s.runeword != name {
        return fail(
            it,
            format!("runeword {} but its runes spell {name}", s.runeword),
        );
    }
    Ok(())
}

// -------------------------------------------------------------------- ranges

/// An `ItemStats` that keeps what the property functions write.
#[derive(Clone, Debug, Default)]
pub(crate) struct Recorder {
    base: BTreeMap<(u16, u16), i32>,
    lists: BTreeMap<(u16, u16, u16), i32>,
    keys: BTreeSet<u16>,
}

impl Recorder {
    fn get(&self, state: u16, id: u16, layer: u16) -> i64 {
        i64::from(self.lists.get(&(state, id, layer)).copied().unwrap_or(0))
    }
}

impl ItemStats for Recorder {
    fn has_stats(&self) -> bool {
        true
    }
    fn stat(&self, id: u16, layer: u16) -> i32 {
        self.base(id, layer)
            .wrapping_add(self.lists.get(&(0, id, layer)).copied().unwrap_or(0))
    }
    fn base(&self, id: u16, layer: u16) -> i32 {
        self.base.get(&(id, layer)).copied().unwrap_or(0)
    }
    fn set_base(&mut self, id: u16, layer: u16, value: i32) {
        self.base.insert((id, layer), value);
    }
    fn has_list(&self, key: ListKey) -> bool {
        self.keys.contains(&key.state)
    }
    fn list_set(&mut self, key: ListKey, id: u16, layer: u16, value: i32) {
        self.keys.insert(key.state);
        self.lists.insert((key.state, id, layer), value);
    }
    fn list_add(&mut self, key: ListKey, id: u16, layer: u16, value: i32) {
        self.keys.insert(key.state);
        let v = self.lists.entry((key.state, id, layer)).or_insert(0);
        *v = v.wrapping_add(value);
    }
    fn list_get(&self, key: ListKey, id: u16, layer: u16) -> i32 {
        self.lists
            .get(&(key.state, id, layer))
            .copied()
            .unwrap_or(0)
    }
}

/// Every property of an item lies between the lowest and the highest value
/// its sources give with their rolls at their ends
/// ([`crate::CheckTables::corners`]): the affixes (and the automagic
/// affix), the superior row, the unique or set row (with the set's partial
/// lists) and the runeword its runes spell. Stats of
/// property functions whose values the records do not bound (skills on
/// hit, charges, rolled layers), the class skill mods and the quest
/// difficulty are not checked. Crafted and tempered items (their recipe's
/// fixed properties are not in the item) and format-0 items are skipped.
#[derive(Clone, Copy, Debug, Default)]
pub struct ItemRanges;

impl SaveCheck for ItemRanges {
    fn check(&self, _: &D2s, _: &D2s, ctx: &SaveCtx) -> Result<(), CheckFail> {
        for it in &ctx.after.items {
            ranges(ctx, it)?;
        }
        Ok(())
    }
}

/// Runs the item's sources on `t` into a recorder.
fn record_sources(t: &ItemTables, it: &FoundItem, fillers: &[usize]) -> Recorder {
    let s = &it.item;
    let mut item = probe(it);
    item.stats = Recorder::default();
    if s.auto_affix != 0 {
        apply_affix(t, &mut item, s.auto_affix);
    }
    match s.quality {
        q::MAGIC | q::RARE => {
            for k in 0..3 {
                for id in [s.prefix[k], s.suffix[k]] {
                    if id != 0 {
                        apply_affix(t, &mut item, id);
                    }
                }
            }
        }
        q::SUPERIOR => {
            if let Ok(row) = usize::try_from(s.file_index) {
                apply_quality_row(t, &mut item, row);
            }
        }
        q::UNIQUE => apply_unique(t, &mut item),
        q::SET => apply_set_item(t, &mut item),
        _ => {}
    }
    if s.flags & flag::RUNEWORD != 0 {
        let sockets = s.base_sockets.clamp(0, 255) as u8;
        if let (Some(row), Some(&last)) = (
            runeword_row(t, it.record, s.quality, sockets, fillers),
            fillers.last(),
        ) {
            let mut filler = Item::new(last, s.version, Recorder::default());
            filler.ilvl = s.ilvl;
            activate_runeword(t, &mut filler, &mut item.stats, row, true);
        }
    }
    item.stats
}

/// The stats the item's sources write through an unbounded function.
fn unbounded(t: &ItemTables, it: &FoundItem) -> BTreeSet<u16> {
    let s = &it.item;
    let mut recs: Vec<PropRec> = Vec::new();
    let mut add = |rs: &[PropRec]| recs.extend(rs.iter().copied().filter(|r| r.code >= 0));
    for id in s.prefix.iter().chain(&s.suffix).chain([&s.auto_affix]) {
        if let Some(a) = affix(t, *id) {
            add(&a.mods);
        }
    }
    let fi = usize::try_from(s.file_index).ok();
    match s.quality {
        q::SUPERIOR => {
            if let Some(r) = fi.and_then(|i| t.qualityitems.get(i)) {
                add(&r.mods);
            }
        }
        q::UNIQUE => {
            if let Some(r) = fi.and_then(|i| t.uniques.get(i)) {
                add(&r.props);
            }
        }
        q::SET => {
            if let Some(r) = fi.and_then(|i| t.setitems.get(i)) {
                add(&r.props);
                add(&r.aprops);
            }
        }
        _ => {}
    }
    if s.flags & flag::RUNEWORD != 0 {
        let sockets = s.base_sockets.clamp(0, 255) as u8;
        if let Some(w) = runeword_row(t, it.record, s.quality, sockets, &it.fillers)
            .and_then(|row| t.runes.get(row))
        {
            add(&w.props);
        }
    }
    let mut free = BTreeSet::from([stat::QUESTITEMDIFFICULTY]);
    for r in recs {
        let Some(p) = usize::try_from(r.code)
            .ok()
            .and_then(|c| t.properties.get(c))
        else {
            continue;
        };
        for slot in &p.slots {
            let known = FUNCS.iter().any(|f| f.func == slot.func);
            if !known || UNBOUNDED_FUNCS.contains(&slot.func) {
                free.insert(slot.stat);
            }
            if !known {
                break;
            }
        }
    }
    if t.itype_of(it.record).is_some_and(|x| x.staffmods < 7) {
        free.insert(stat::ITEM_SINGLESKILL);
    }
    free
}

fn ranges(ctx: &SaveCtx, it: &FoundItem) -> Result<(), CheckFail> {
    let s = &it.item;
    if s.compact
        || s.version < 1
        || it.failed
        || matches!(s.quality, q::CRAFTED | q::TEMPERED)
        || s.flags & flag::EAR != 0
    {
        return Ok(());
    }
    let t = &ctx.tables.items;
    let runs = ctx
        .tables
        .corners()
        .each_ref()
        .map(|c| record_sources(c, it, &it.fillers));
    let free = unbounded(t, it);
    let lists = std::iter::once((0u16, s.main.as_ref()))
        .chain(
            s.sets
                .iter()
                .enumerate()
                .map(|(k, l)| (STATE_ITEMSET1 + k as u16, l.as_ref())),
        )
        .chain(std::iter::once((STATE_RUNEWORD, s.runeword_list.as_ref())));
    for (state, list) in lists {
        for e in list.into_iter().flatten() {
            if free.contains(&e.stat) {
                continue;
            }
            let got = runs.each_ref().map(|r| r.get(state, e.stat, e.param));
            let (lo, hi) = got
                .iter()
                .fold((i64::MAX, i64::MIN), |(l, h), &v| (l.min(v), h.max(v)));
            let v = i64::from(e.value);
            if !(lo..=hi).contains(&v) {
                let list = match state {
                    0 => "its properties".to_owned(),
                    STATE_RUNEWORD => "its runeword".to_owned(),
                    s => format!("set list {}", s - STATE_ITEMSET1 + 1),
                };
                return fail(
                    it,
                    format!(
                        "stat {} (layer {}) is {v} in {list}; its sources give {lo}–{hi}",
                        e.stat, e.param
                    ),
                );
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ item IDs

/// No two items of the save share an item ID, and (with the realm's item
/// registry, plan C3) no item ID is held by another character.
#[derive(Clone, Copy, Debug, Default)]
pub struct ItemDuplicates;

impl SaveCheck for ItemDuplicates {
    fn check(&self, _: &D2s, after: &D2s, ctx: &SaveCtx) -> Result<(), CheckFail> {
        let mut seen: HashMap<u32, &FoundItem> = HashMap::new();
        for it in &ctx.after.items {
            let Some(id) = it.id() else {
                continue;
            };
            if let Some(first) = seen.insert(id, it) {
                return fail(it, format!("item ID {id:#010x} is also {}", first.at()));
            }
            if let Some(holder) = ctx
                .info
                .registry
                .and_then(|r| r.other_holder(after.header.name_bytes(), id))
            {
                return fail(it, format!("item ID {id:#010x} is held by {holder}"));
            }
        }
        Ok(())
    }
}

/// An item ID of the stored record that is still in the save names the
/// same item: the same quality, unique, set or quality row, affixes, rare
/// names and item level (an item is never changed into another under its
/// ID; the first item-ID rule of plan N6).
#[derive(Clone, Copy, Debug, Default)]
pub struct ItemIdsKept;

impl SaveCheck for ItemIdsKept {
    fn check(&self, _: &D2s, _: &D2s, ctx: &SaveCtx) -> Result<(), CheckFail> {
        let before: HashMap<u32, &FoundItem> = ctx
            .before
            .items
            .iter()
            .filter_map(|it| it.id().map(|id| (id, it)))
            .collect();
        for it in &ctx.after.items {
            let Some(old) = it.id().and_then(|id| before.get(&id)) else {
                continue;
            };
            let (a, b) = (&old.item, &it.item);
            let same = a.quality == b.quality
                && a.file_index == b.file_index
                && a.prefix == b.prefix
                && a.suffix == b.suffix
                && a.auto_affix == b.auto_affix
                && a.rare_prefix == b.rare_prefix
                && a.rare_suffix == b.rare_suffix
                && a.ilvl == b.ilvl;
            if !same {
                return fail(
                    it,
                    format!(
                        "item ID {:#010x} was {} and is now another item",
                        b.unit28,
                        old.at()
                    ),
                );
            }
        }
        Ok(())
    }
}
