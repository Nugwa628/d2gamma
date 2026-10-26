//! One inventory item from an item code, made the way the game makes an
//! item for a tool: the code lookup, the create wrapper with the
//! start-item arguments (source = the player, spawn mode 4, quality 2,
//! no sockets, never ethereal, ilvl = the player's level, < 2 → 1, no
//! seeds: the seeds derive from the game seed), the flag-0x40 list
//! removed, stackable → quantity := total max stack, and the inventory
//! placement (find free, or at a given cell), then the durability /
//! quiver stats. The item is written with
//! `d2_sim::items::bitstream::write_save` (save on, children on) in mode 0
//! (stored), body location 0, on its page.
//!
//! Not settled yet (written as noted, never guessed silently):
//! - the save trailer: which items carry the 1-bit-1 form is not known;
//!   every item is written with the 1-bit-0 form;
//! - item flags set by the inventory pass after a placement are not
//!   known; the flags are the creation's plus 0x10, without 0x2000
//!   (instore), which every load clears, so the file is what the game
//!   saves after loading it.

use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Result};
use d2_formats::d2s::ItemEntry;
use d2_proto::item_bits::Location;
use d2_sim::items::bitstream::{write_save, Kind, StatEntry, StreamItem};
use d2_sim::items::inventory::grid::{fits, in_bounds};
use d2_sim::items::inventory::{mode, page_grid_size, search, Grid, UnitKind};
use d2_sim::items::props::{activate_runeword, runeword_row, STATE_RUNEWORD};
use d2_sim::items::{
    create_item, flag, q, req, stat, ty, Item, ItemGame, ItemRequest, ItemStats, ListKey,
    PlayerInfo, RequestUnit, UniqueBits,
};
use d2_sim::rng::Seed;
use d2_sim::units::UnitId;

use crate::tables::Tables;

/// `--item <code>[@x,y][:page]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemSpec {
    /// Items `code`, space-padded to four bytes.
    pub code: [u8; 4],
    /// Top-left cell; `None` = find a free position.
    pub at: Option<(i32, i32)>,
    /// Page: 0 inventory, 3 cube, 4 stash.
    pub page: u8,
    /// `#Q`: quantity (stat 70) set after creation, for a part stack
    /// (a test set-up, not a step of item creation).
    pub qty: Option<i32>,
    /// `/` options (test set-ups beyond item creation, see [`ItemOpts`]).
    pub opts: ItemOpts,
}

/// Where an item goes when it is not stored on a page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Place {
    /// Mode 0 on `page` (the default).
    #[default]
    Stored,
    /// Mode 1 at body location 1..=12.
    Body(u8),
    /// Mode 2 in belt slot 0..=15 (the stream's x).
    Belt(u8),
}

/// `/q=QUALITY`, `/idx=ROW`, `/ilvl=N`, `/sock=N`, `/unid`, `/eth`,
/// `/body=LOC`, `/belt=SLOT`: the creation request's quality (forced),
/// set / unique row, item level, a socket count, and the flags /
/// placement written after creation. Test set-ups; the quality and
/// affix draws are the creation's own, and the row is the one creation
/// prefers when it fits the base and item level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemOpts {
    pub quality: Option<u8>,
    pub index: Option<i32>,
    pub ilvl: Option<i32>,
    pub sockets: Option<i32>,
    pub unidentified: bool,
    pub ethereal: bool,
    /// `/rw=ID`: the runeword flag (0x4000000) and record id, with no runes socketed: a
    /// runeword item that no longer matches.
    pub runeword: Option<u16>,
    /// `/dur=N`: current durability (stat 72) after creation: a damaged item for the repair checks.
    pub durability: Option<i32>,
    /// `/broken`: durability 0 and the broken flag 0x100 (a worn-out item for the repair checks).
    pub broken: bool,
    /// `/stat=ID_LAYER_VALUE` (up to three): a stat written to the item's own list after creation
    /// (test set-ups for stat ops no generated item carries, e.g. the by-time stats 268 / 269).
    pub stats: [Option<(u16, u16, i32)>; 3],
    /// `/runes=CODE+CODE..` (up to six): items made like the base and socketed into it in order
    /// (sockets: `/sock`, else one per filler). When the base and fillers spell a runeword row, its
    /// properties are activated as a socket fill does (ladder-only rows too: a test set-up).
    pub runes: [Option<[u8; 4]>; 6],
    pub place: Place,
}

impl ItemOpts {
    fn parse(text: &str, whole: &str) -> Result<Self> {
        let mut o = ItemOpts::default();
        for w in text.split('/').filter(|w| !w.is_empty()) {
            let (k, v) = w.split_once('=').unwrap_or((w, ""));
            let n = |v: &str| {
                v.parse::<i32>()
                    .map_err(|_| anyhow!("bad number {v:?} in {whole:?}"))
            };
            match k {
                "q" => {
                    o.quality = Some(match v {
                        "low" => q::LOW,
                        "normal" => q::NORMAL,
                        "superior" => q::SUPERIOR,
                        "magic" => q::MAGIC,
                        "set" => q::SET,
                        "rare" => q::RARE,
                        "unique" => q::UNIQUE,
                        "crafted" => q::CRAFTED,
                        _ => bail!("quality {v:?} in {whole:?}"),
                    })
                }
                "idx" => o.index = Some(n(v)?),
                "ilvl" => o.ilvl = Some(n(v)?),
                "sock" => o.sockets = Some(n(v)?),
                "unid" => o.unidentified = true,
                "eth" => o.ethereal = true,
                "rw" => o.runeword = Some(n(v)? as u16),
                "dur" => o.durability = Some(n(v)?),
                "broken" => o.broken = true,
                "stat" => {
                    let p: Vec<&str> = v.split('_').collect();
                    let [id, layer, val] = p[..] else {
                        bail!("/stat=ID_LAYER_VALUE in {whole:?}");
                    };
                    let e = (n(id)? as u16, n(layer)? as u16, n(val)?);
                    let Some(slot) = o.stats.iter_mut().find(|x| x.is_none()) else {
                        bail!("more than three /stat in {whole:?}");
                    };
                    *slot = Some(e);
                }
                "runes" => {
                    for (i, c) in v.split('+').enumerate() {
                        let Some(slot) = o.runes.get_mut(i) else {
                            bail!("more than six /runes in {whole:?}");
                        };
                        if c.is_empty() || c.len() > 4 || !c.is_ascii() {
                            bail!("filler code {c:?} in {whole:?}: 1 to 4 ASCII characters");
                        }
                        let mut code = [b' '; 4];
                        code[..c.len()].copy_from_slice(c.as_bytes());
                        *slot = Some(code);
                    }
                }
                "body" => {
                    let b = n(v)?;
                    if !(1..=12).contains(&b) {
                        bail!("body location {b} in {whole:?}: 1..=12");
                    }
                    o.place = Place::Body(b as u8);
                }
                "belt" => {
                    let b = n(v)?;
                    if !(0..=15).contains(&b) {
                        bail!("belt slot {b} in {whole:?}: 0..=15");
                    }
                    o.place = Place::Belt(b as u8);
                }
                _ => bail!("unknown item option {k:?} in {whole:?}"),
            }
        }
        Ok(o)
    }
}

impl std::str::FromStr for ItemSpec {
    type Err = anyhow::Error;

    fn from_str(whole: &str) -> Result<Self> {
        let (s, opts) = whole.split_once('/').unwrap_or((whole, ""));
        let opts = ItemOpts::parse(opts, whole)?;
        let (rest, page) = match s.rsplit_once(':') {
            Some((r, p)) => (
                r,
                p.parse::<u8>().map_err(|_| anyhow!("bad page in {s:?}"))?,
            ),
            None => (s, 0),
        };
        let (rest, qty) = match rest.split_once('#') {
            Some((c, q)) => {
                let (q, tail) = q.split_once('@').map_or((q, ""), |(a, b)| (a, b));
                let q = q
                    .parse::<i32>()
                    .map_err(|_| anyhow!("bad quantity in {s:?}"))?;
                (
                    if tail.is_empty() {
                        c.to_owned()
                    } else {
                        format!("{c}@{tail}")
                    },
                    Some(q),
                )
            }
            None => (rest.to_owned(), None),
        };
        let rest = rest.as_str();
        let (code, at) = match rest.split_once('@') {
            Some((c, xy)) => {
                let (x, y) = xy
                    .split_once(',')
                    .ok_or_else(|| anyhow!("want @x,y in {s:?}"))?;
                let p = |v: &str| v.parse::<i32>().map_err(|_| anyhow!("bad cell in {s:?}"));
                (c, Some((p(x)?, p(y)?)))
            }
            None => (rest, None),
        };
        if code.is_empty() || code.len() > 4 || !code.is_ascii() {
            bail!("item code {code:?}: 1 to 4 ASCII characters");
        }
        let mut c = [b' '; 4];
        c[..code.len()].copy_from_slice(code.as_bytes());
        if !matches!(page, 0 | 3 | 4) {
            bail!(
                "page {page}: only 0 (inventory), 3 (cube) and 4 (stash) are player storage pages"
            );
        }
        Ok(ItemSpec {
            code: c,
            at,
            page,
            qty,
            opts,
        })
    }
}

/// Item stats: base stats and keyed lists in ordered maps.
#[derive(Clone, Debug, Default)]
pub struct MapStats {
    any: bool,
    base: BTreeMap<(u16, u16), i32>,
    lists: BTreeMap<ListKey, BTreeMap<(u16, u16), i32>>,
}

impl ItemStats for MapStats {
    fn has_stats(&self) -> bool {
        self.any
    }
    fn stat(&self, id: u16, layer: u16) -> i32 {
        let lists: i32 = self
            .lists
            .values()
            .map(|l| l.get(&(id, layer)).copied().unwrap_or(0))
            .fold(0i32, i32::wrapping_add);
        self.base(id, layer).wrapping_add(lists)
    }
    fn base(&self, id: u16, layer: u16) -> i32 {
        self.base.get(&(id, layer)).copied().unwrap_or(0)
    }
    fn set_base(&mut self, id: u16, layer: u16, value: i32) {
        self.any = true;
        self.base.insert((id, layer), value);
    }
    fn has_list(&self, key: ListKey) -> bool {
        self.lists.contains_key(&key)
    }
    fn list_set(&mut self, key: ListKey, id: u16, layer: u16, value: i32) {
        self.any = true;
        self.lists
            .entry(key)
            .or_default()
            .insert((id, layer), value);
    }
    fn list_add(&mut self, key: ListKey, id: u16, layer: u16, value: i32) {
        self.any = true;
        let v = self
            .lists
            .entry(key)
            .or_default()
            .entry((id, layer))
            .or_default();
        *v = v.wrapping_add(value);
    }
    fn list_get(&self, key: ListKey, id: u16, layer: u16) -> i32 {
        self.lists
            .get(&key)
            .and_then(|l| l.get(&(id, layer)))
            .copied()
            .unwrap_or(0)
    }
}

/// The game fields creation reads: one game seed for every item of a run,
/// so the draws follow each other as in one game.
pub struct ToolGame {
    pub seed: Seed,
    pub expansion: bool,
    pub uniques: UniqueBits,
}

impl ToolGame {
    pub fn new(seed: u32, expansion: bool) -> Self {
        ToolGame {
            seed: Seed::init_low(seed),
            expansion,
            uniques: UniqueBits::default(),
        }
    }
}

impl ItemGame for ToolGame {
    /// Items made here go into a save file, outside any game, so their
    /// timers are stated at the base rate (the save keeps none of them).
    fn tick_rate(&self) -> d2_sim::tick::TickRate {
        d2_sim::tick::TickRate::BASE
    }
    fn seed(&mut self) -> &mut Seed {
        &mut self.seed
    }
    fn difficulty(&self) -> u8 {
        0
    }
    fn expansion(&self) -> bool {
        self.expansion
    }
    fn ladder_flags(&self) -> (bool, bool) {
        (false, false)
    }
    fn uniques(&mut self) -> &mut UniqueBits {
        &mut self.uniques
    }
}

/// The player the items are made for.
#[derive(Clone, Debug)]
pub struct Owner {
    pub class: u8,
    pub name: [u8; 16],
    pub level: i32,
    pub hardcore: bool,
    pub expansion: bool,
}

/// The page grids of one player, with the cells already taken.
pub struct Pages {
    grids: BTreeMap<u8, Grid>,
    next: u32,
}

impl Pages {
    pub fn new() -> Self {
        Pages {
            grids: BTreeMap::new(),
            next: 1,
        }
    }

    fn grid(&mut self, t: &Tables, o: &Owner, page: u8) -> Result<&mut Grid> {
        use std::collections::btree_map::Entry;
        Ok(match self.grids.entry(page) {
            Entry::Occupied(g) => g.into_mut(),
            Entry::Vacant(v) => {
                let (w, h) = page_grid_size(
                    &t.inv,
                    UnitKind::Player { class: o.class },
                    page,
                    o.expansion,
                )
                .ok_or_else(|| anyhow!("page {page}: no inventory.bin record"))?;
                v.insert(Grid::new(w, h))
            }
        })
    }

    /// Marks the w × h cells at (x, y) of `page` as taken.
    fn mark(
        &mut self,
        t: &Tables,
        o: &Owner,
        page: u8,
        at: (i32, i32),
        (w, h): (u8, u8),
    ) -> Result<()> {
        let (x, y) = at;
        let id = UnitId(self.next);
        self.next += 1;
        let g = self.grid(t, o, page)?;
        for yy in y..y + i32::from(h) {
            for xx in x..x + i32::from(w) {
                if xx >= 0 && yy >= 0 && xx < i32::from(g.width) && yy < i32::from(g.height) {
                    g.cells[yy as usize * usize::from(g.width) + xx as usize] = Some(id);
                }
            }
        }
        g.items.push(id);
        Ok(())
    }

    /// Takes the cells of an item already in a save (mode 0, body
    /// location 0): its page from the stream's page + 1.
    pub fn take_existing(&mut self, t: &Tables, o: &Owner, entry: &ItemEntry) -> Result<()> {
        let e = t
            .decode_entry(&entry.bytes)
            .map_err(|e| anyhow!("existing item: {e}"))?;
        if let Some(Location::Slot {
            body: 0,
            x,
            y,
            page1,
        }) = e.item.location
        {
            if e.item.mode == mode::STORED && page1 >= 1 {
                if let Some(r) = t.inv.items.iter().find(|r| r.code == e.item.code) {
                    let (w, h) = (r.invwidth, r.invheight);
                    self.mark(t, o, page1 - 1, (i32::from(x), i32::from(y)), (w, h))?;
                }
            }
        }
        Ok(())
    }

    /// Placement: find free (player owner) or place at
    /// (max(x, 0), max(y, 0)).
    fn place(&mut self, t: &Tables, o: &Owner, s: &ItemSpec, w: u8, h: u8) -> Result<(i32, i32)> {
        if w == 0 || h == 0 {
            bail!(
                "item {:?} has invwidth or invheight 0: never placed (inventory.md §2.2)",
                code_str(&s.code)
            );
        }
        let g = self.grid(t, o, s.page)?;
        let (x, y) = match s.at {
            None => search(g, w, h, true).ok_or_else(|| {
                anyhow!(
                    "no free {w}×{h} position on page {} for {:?}",
                    s.page,
                    code_str(&s.code)
                )
            })?,
            Some((x, y)) => {
                let (x, y) = (x.max(0), y.max(0));
                if !(in_bounds(g, x, y, w, h) && fits(g, x, y, w, h)) {
                    bail!(
                        "{:?} ({w}×{h}) does not fit at {x},{y} on page {} ({}×{})",
                        code_str(&s.code),
                        s.page,
                        g.width,
                        g.height
                    );
                }
                (x, y)
            }
        };
        self.mark(t, o, s.page, (x, y), (w, h))?;
        Ok((x, y))
    }
}

impl Default for Pages {
    fn default() -> Self {
        Self::new()
    }
}

pub fn code_str(c: &[u8; 4]) -> String {
    String::from_utf8_lossy(c).trim_end().to_owned()
}

/// The items row of `code` (the code map: the first combined row with the
/// code).
fn row_of(t: &Tables, code: &[u8; 4]) -> Result<usize> {
    t.items
        .items
        .iter()
        .position(|r| r.code == *code)
        .ok_or_else(|| anyhow!("unknown item code {:?}", code_str(code)))
}

/// The create wrapper with the start-item arguments for items row `idx`,
/// and the checks of what `opts` asked for (quality, set / unique row).
fn create(
    t: &Tables,
    game: &mut ToolGame,
    o: &Owner,
    idx: usize,
    opts: &ItemOpts,
) -> Result<Item<MapStats>> {
    let it = &t.items;
    let rec = &it.items[idx];
    let mut rq = ItemRequest {
        unit: Some(RequestUnit {
            class: i32::from(o.class),
            player: Some(PlayerInfo {
                name: o.name,
                level: o.level,
                hardcore: Some(o.hardcore),
            }),
        }),
        ilvl: opts.ilvl.unwrap_or(if o.level < 2 { 1 } else { o.level }),
        item: idx as i32,
        format: game.item_format(),
        quality: opts.quality.unwrap_or(q::NORMAL),
        // The row is the pick's preferred row (row + 1), not a forced
        // request: a forced one takes its seeds from the request (none
        // here), so every such item would get item ID 0 and the same rolls.
        index: opts.index.map_or(0, |i| i + 1),
        flags2: req::NO_SOCKETS | req::NEVER_ETHEREAL,
        ..ItemRequest::default()
    };
    let created = create_item(it, game, &mut rq, false, MapStats::default(), 0)
        .map_err(|e| anyhow!("creating {:?}: {e}", code_str(&rec.code)))?;
    let item = created.item;
    if let Some(want) = opts.quality {
        if item.quality != want {
            bail!(
                "{:?}: quality {} asked, creation gave {} (no row fits?)",
                code_str(&rec.code),
                want,
                item.quality
            );
        }
    }
    // A set / unique item's row must name this base (or no code check
    // is possible): a mismatch is reported, never written.
    let row_code = match item.quality {
        q::SET => usize::try_from(item.file_index)
            .ok()
            .and_then(|i| it.setitems.get(i))
            .map(|r| r.item),
        q::UNIQUE => usize::try_from(item.file_index)
            .ok()
            .and_then(|i| it.uniques.get(i))
            .map(|r| r.code),
        _ => None,
    };
    if let (Some(row), Some(_)) = (opts.index, row_code) {
        if item.file_index != row {
            bail!(
                "{:?}: creation chose row {}, not {row} (the row must fit the base and item level)",
                code_str(&rec.code),
                item.file_index
            );
        }
    }
    if let Some(c) = row_code {
        if c != rec.code && c != rec.normcode {
            bail!(
                "{:?}: creation chose {} row {} whose item is {:?}",
                code_str(&rec.code),
                if item.quality == q::SET {
                    "set"
                } else {
                    "unique"
                },
                item.file_index,
                code_str(&c)
            );
        }
    }
    Ok(item)
}

/// Where a stream view puts an item: mode, body location, (x, y), page.
type At = (u8, u8, (i32, i32), u8);

/// The writer's view of an item of items row `idx` (as
/// `wiring::inventory::bits` projects it), without children.
fn stream_view(t: &Tables, idx: usize, item: &Item<MapStats>, at: At, runeword: u16) -> StreamItem {
    let it = &t.items;
    let rec = &it.items[idx];
    let (imode, body_loc, (x, y), page) = at;
    let is = |k: u16| it.is_type(idx, k as i16);
    let st = &item.stats;
    let entries = |k: &ListKey| {
        st.lists.get(k).map(|l| {
            l.iter()
                .map(|(&(id, layer), &v)| StatEntry {
                    stat: id,
                    param: layer,
                    value: v,
                })
                .collect::<Vec<_>>()
        })
    };
    let main = entries(&ListKey::ITEM);
    // Set lists by state (flags 0x2040, else 0x40).
    let mut sets: [Option<Vec<StatEntry>>; 5] = Default::default();
    for (slot, &state) in sets
        .iter_mut()
        .zip(d2_sim::items::bitstream::SET_STATES.iter())
    {
        *slot = entries(&ListKey {
            state: state as u16,
            flags: 0x2040,
        })
        .or_else(|| {
            entries(&ListKey {
                state: state as u16,
                flags: ListKey::ITEM.flags,
            })
        });
    }
    // The runeword list (state 171, flags 0x40).
    let runeword_list = entries(&ListKey {
        state: STATE_RUNEWORD,
        flags: ListKey::ITEM.flags,
    });
    // The flags a game save of a
    // loaded character holds (0x2000 cleared by the load).
    StreamItem {
        flags: item.flags & !d2_formats::d2s::ITEM_FLAG_INSTORE,
        alt: false,
        compact: rec.compactsave != 0,
        version: item.format,
        mode: u32::from(imode),
        x,
        y,
        body_loc,
        page,
        code: rec.code,
        base_code: rec.normcode,
        ear_class: item.file_index,
        ear_level: item.ear_level,
        name: item.name,
        filled: 0,
        ilvl: item.ilvl,
        quality: item.quality,
        file_index: item.file_index,
        varinvgfx: it.itype_of(idx).is_some_and(|y| y.varinvgfx != 0),
        gfx: item.gfx,
        auto_affix: item.auto_affix,
        prefix: item.prefix,
        suffix: item.suffix,
        rare_prefix: item.rare_prefix,
        rare_suffix: item.rare_suffix,
        runeword,
        kind: Kind {
            armor: is(ty::ARMO),
            weapon: is(ty::WEAP),
            gold: is(ty::GOLD),
            charm: is(ty::CHAR),
            body_part: is(ty::BODY) && !is(ty::PLAY),
            scroll_or_book: is(ty::SCRO) || is(ty::BOOK),
        },
        stackable: rec.stackable != 0,
        quest_diff: rec.quest != 0 && rec.questdiffcheck != 0,
        base_defense: st.base(stat::ARMORCLASS, 0),
        base_max_dur: st.base(stat::MAXDURABILITY, 0),
        base_sockets: st.base(stat::NUMSOCKETS, 0),
        total_dur: st.stat(stat::DURABILITY, 0),
        total_gold: st.stat(stat::GOLD, 0),
        total_quantity: st.stat(stat::QUANTITY, 0),
        total_quest_diff: st.stat(stat::QUESTITEMDIFFICULTY, 0),
        main,
        sets,
        runeword_list,
        // Unit +0x28: the init seed.
        unit28: item.init_seed,
        // Not settled yet: see the save trailer note at the top.
        save_trailer: None,
        ..StreamItem::default()
    }
}

/// Creates one item (as for a tool) and returns its save entry.
pub fn make_item(
    t: &Tables,
    game: &mut ToolGame,
    o: &Owner,
    pages: &mut Pages,
    s: &ItemSpec,
) -> Result<ItemEntry> {
    let it = &t.items;
    let idx = row_of(t, &s.code)?;
    let rec = it.items[idx].clone();
    let mut item = create(t, game, o, idx, &s.opts)?;
    if s.opts.unidentified {
        item.flags &= !flag::IDENTIFIED;
    } else {
        item.flags |= flag::IDENTIFIED;
    }
    if s.opts.ethereal {
        item.flags |= flag::ETHEREAL;
    }
    if s.opts.runeword.is_some() {
        item.flags |= flag::RUNEWORD;
    }
    // The socket fillers, made like the base (after it, in order): normal
    // identified items without their creation list (start items).
    let mut fillers = Vec::new();
    for code in s.opts.runes.iter().flatten() {
        let f = row_of(t, code)?;
        let mut filler = create(t, game, o, f, &ItemOpts::default())?;
        filler.flags |= flag::IDENTIFIED;
        if filler.quality <= q::NORMAL {
            filler.stats.lists.remove(&ListKey::ITEM);
        }
        fillers.push((f, filler));
    }
    if !fillers.is_empty() && rec.hasinv == 0 {
        bail!(
            "{:?}: /runes: the base has no inventory (items hasinv 0)",
            code_str(&s.code)
        );
    }
    let sockets = s
        .opts
        .sockets
        .or((!fillers.is_empty()).then_some(fillers.len() as i32));
    if let Some(n) = sockets {
        // The game's load cuts a larger count to the base's `gemsockets`
        // (a buckler written with 2 is re-saved with 1).
        if n < 1 || n > i32::from(rec.gemsockets) {
            bail!(
                "{:?}: sock={n}: the base allows 1..={} (gemsockets)",
                code_str(&s.code),
                rec.gemsockets
            );
        }
        if fillers.len() > n as usize {
            bail!(
                "{:?}: {} /runes for {n} sockets",
                code_str(&s.code),
                fillers.len()
            );
        }
        item.flags |= flag::SOCKETED;
        item.stats.set_base(stat::NUMSOCKETS, 0, n);
    }
    // Step 2.2: the flag-0x40 list is removed (start items, normal
    // quality); a better item keeps its property list.
    if item.quality <= q::NORMAL {
        item.stats.lists.remove(&ListKey::ITEM);
    }
    // Step 2.4: stackable → quantity := total max stack.
    if rec.stackable != 0 {
        let extra = item.stats.stat(stat::EXTRA_STACK, 0);
        let max = (rec.maxstack as i32).wrapping_add(extra).min(511);
        item.stats.set_base(stat::QUANTITY, 0, max);
    }
    // Step 2.6: inventory placement (page := the page, find free or at);
    // a body or belt item takes no grid cell.
    let (w, h) = (rec.invwidth, rec.invheight);
    let at: At = match s.opts.place {
        Place::Stored => (mode::STORED, 0, pages.place(t, o, s, w, h)?, s.page),
        // A game save writes an equipped item's x as its body location
        // (seen in a game re-save of d2s-tool items).
        Place::Body(b) => (mode::EQUIPPED, b, (i32::from(b), 0), 0xFF),
        Place::Belt(b) => (mode::BELT, 0, (i32::from(b), 0), 0xFF),
    };
    item.inv_page = at.3;
    // Then: quantity 250 for itemtype 5, else durability := max
    // durability; then a quiver (itemtype `quiver` ≠ 0) gets 100.
    if it.is_type(idx, 5) {
        item.stats.set_base(stat::QUANTITY, 0, 250);
    } else {
        let maxdur = item.stats.stat(stat::MAXDURABILITY, 0);
        item.stats.set_base(stat::DURABILITY, 0, maxdur);
    }
    if it.itype_of(idx).is_some_and(|y| y.quiver != 0) {
        item.stats.set_base(stat::QUANTITY, 0, 100);
    }
    if let Some(q) = s.qty {
        item.stats.set_base(stat::QUANTITY, 0, q);
    }
    if let Some(d) = s.opts.durability {
        item.stats.set_base(stat::DURABILITY, 0, d);
    }
    for (id, layer, val) in s.opts.stats.into_iter().flatten() {
        item.stats.list_set(ListKey::ITEM, id, layer, val);
    }
    if s.opts.broken {
        item.stats.set_base(stat::DURABILITY, 0, 0);
        item.flags |= 0x100;
    }
    // The socket fill's runeword step: the first runes row the base and
    // its fillers match; its properties run with the last filler as I.
    let mut runeword = s.opts.runeword.unwrap_or(0xFFFF);
    let classes: Vec<usize> = fillers.iter().map(|&(f, _)| f).collect();
    if let Some((_, last)) = fillers.last_mut() {
        let n = item.stats.stat(stat::NUMSOCKETS, 0) as u8;
        if let Some(row) = runeword_row(it, idx, item.quality, n, &classes) {
            if activate_runeword(it, last, &mut item.stats, row, true) {
                item.flags |= flag::RUNEWORD;
                runeword = it.runes[row].name_id;
            }
        }
    }
    let mut view = stream_view(t, idx, &item, at, runeword);
    view.filled = fillers.len() as u32;
    view.children = fillers
        .iter()
        .enumerate()
        .map(|(i, (f, c))| stream_view(t, *f, c, (mode::SOCKETED, 0, (i as i32, 0), 0xFF), 0xFFFF))
        .collect();
    let (bytes, _) = write_save(&view, &it.isc)
        .map_err(|_| anyhow!("{:?}: item stream overflow", code_str(&s.code)))?;
    // The entry must decode back to its own length.
    let back = t.decode_entry(&bytes).map_err(|e| {
        anyhow!(
            "{:?}: written entry does not decode: {e}",
            code_str(&s.code)
        )
    })?;
    if back.len != bytes.len() {
        bail!(
            "{:?}: written entry is {} bytes, decodes as {}",
            code_str(&s.code),
            bytes.len(),
            back.len
        );
    }
    Ok(ItemEntry { bytes })
}
