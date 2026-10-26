// D2RUST's own: the shared stash's moves on a local player's items.
//! What single player's shared stash does to the character in the
//! in-process game: an item of the inventory (page 0, never equipped,
//! never the cube) or the cursor lifted off as its save bytes, an item
//! placed back from save bytes, fresh plain stackables made, and the
//! gold. Each call queues the inventory model's item messages with the
//! next tick's sends, as the dev tools' item gift does; the store keeps
//! the goods between the calls.

use d2_sim::items::bitstream::read::read_save_entry;
use d2_sim::items::bitstream::write_save;
use d2_sim::items::inventory::{grid::search, grid_id, mode, page_grid_size, InvWorld};
use d2_sim::items::{flag, stat};
use d2_sim::units::lifecycle::LifecycleHooks;
use d2_sim::units::UnitId;
use d2_sim::wiring::economy::quest_reward::create_reward;
use d2_sim::wiring::inventory::{InvDesk, InvRest};

use crate::adapters::character::save::item_entry;
use crate::adapters::handlers::world::free_item_tree;
use crate::game::Sim;

/// The inventory page.
const INVENTORY: u8 = 0;
/// The save format's quality number of a normal item.
const NORMAL: u8 = 2;

/// An item the player holds where the stash may take it from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeldItem {
    pub guid: u32,
    pub code: String,
    /// The item type's code (`rune`, `gem`, `key`, ...).
    pub item_type: String,
    /// Width and height in cells.
    pub size: (u8, u8),
    /// On the cursor (else in the inventory, at `cell`).
    pub cursor: bool,
    pub cell: (i32, i32),
    /// The save format's quality number.
    pub quality: u8,
    pub ilvl: u8,
    pub level_req: u8,
    pub sockets: u8,
    pub ethereal: bool,
    /// The class the item type is restricted to.
    pub class: Option<u8>,
    /// How many of its kind it is when it is a plain item (normal quality,
    /// nothing socketed, not ethereal): its quantity for an item the game
    /// stacks (keys), else 1. `None` for any other item.
    pub plain_count: Option<u32>,
}

/// An item lifted off the player: its save entry and what it was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiftedItem {
    pub held: HeldItem,
    /// The save entry (the item, then its socketed items).
    pub bytes: Vec<u8>,
    /// Its realm item ID, in a realm game (taken when the entry was
    /// written).
    pub id: Option<u32>,
}

/// Where a placed item goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceTo {
    /// The inventory: at the cell when free, else the first free place.
    Inventory(Option<(i32, i32)>),
    Cursor,
}

fn text(code: [u8; 4]) -> String {
    String::from_utf8_lossy(&code).trim_end().to_owned()
}

fn code_key(code: &str) -> Option<[u8; 4]> {
    let b = code.as_bytes();
    if b.is_empty() || b.len() > 4 {
        return None;
    }
    let mut key = [b' '; 4];
    key[..b.len()].copy_from_slice(b);
    Some(key)
}

/// Runs `$body` with `$d` the inventory desk (`&mut InvDesk`), then
/// queues the model's messages. `None` without an inventory model.
macro_rules! with_desk {
    ($s:expr, |$d:ident| $body:expr) => {{
        let s: &mut Sim = $s;
        let out = s
            .world
            .with_economy(&mut s.game, &mut s.events, |econ, parts| {
                let inv = parts.inventory.as_deref_mut()?;
                let mut desk = inv.desk(econ);
                let out = {
                    let $d = &mut desk;
                    #[allow(clippy::redundant_closure_call)]
                    (|| $body)()
                };
                desk.sync_out();
                Some(out)
            });
        let sent = s.world.take_inventory_sent(&mut s.game, &mut s.events);
        s.world.rest.sent.extend(sent);
        out
    }};
}

/// The held item with `guid`: on the player's cursor or in its inventory.
fn held<H, R>(d: &InvDesk<'_, '_, H, R>, player: UnitId, guid: u32) -> Option<HeldItem>
where
    H: LifecycleHooks,
    R: InvRest + ?Sized,
{
    let u = d.item_unit(guid)?;
    let it = d.state.items.get(&u)?;
    let cursor = d.state.cursor_of(player) == Some(u);
    let stored = it.mode == mode::STORED && it.page == INVENTORY && d.state.holds(player, u);
    if !cursor && !stored {
        return None;
    }
    let rec = d.tables.item(it.record)?;
    let ty = d.tables.itype_of(it.record);
    let item = d.econ.items.get(u)?;
    let total = |s: u16| d.econ.stats.unit_total(u, s, 0);
    let sockets = total(stat::NUMSOCKETS).clamp(0, 255) as u8;
    let ethereal = item.flags & flag::ETHEREAL != 0;
    let plain = item.quality == NORMAL && !ethereal && d.state.fillers(u).is_empty();
    let count = if rec.stackable != 0 {
        total(stat::QUANTITY).max(1) as u32
    } else {
        1
    };
    Some(HeldItem {
        guid,
        code: text(rec.code),
        item_type: ty.map(|t| text(t.code)).unwrap_or_default(),
        size: (rec.invwidth, rec.invheight),
        cursor,
        cell: (it.x, it.y),
        quality: item.quality,
        ilvl: item.ilvl.clamp(0, 255) as u8,
        level_req: rec.levelreq,
        sockets,
        ethereal,
        class: ty.map(|t| t.class).filter(|&c| c < 7),
        plain_count: plain.then_some(count),
    })
}

/// The save entry `bytes` written again with realm item ID `id`: a
/// hostile test build's invented item. Only an item with an ID of its own
/// (not compact) and nothing socketed.
pub fn with_realm_id(s: &mut Sim, bytes: &[u8], id: u32) -> Result<Vec<u8>, String> {
    with_desk!(s, |d| {
        let mut rec =
            read_save_entry(bytes, d.econ.tables).map_err(|e| format!("unreadable item: {e:?}"))?;
        let it = &mut rec.item.item;
        if rec.item.failed || it.compact || it.alt || !rec.children.is_empty() {
            return Err("not an item with an ID of its own".into());
        }
        it.unit28 = id;
        write_save(it, &d.econ.tables.isc)
            .map(|(b, _)| b)
            .map_err(|e| format!("not written: {e:?}"))
    })
    .unwrap_or_else(|| Err("no inventory model in this game".into()))
}

/// What the held item `guid` is, without moving it.
pub fn held_item(s: &mut Sim, player: UnitId, guid: u32) -> Option<HeldItem> {
    with_desk!(s, |d| held(d, player, guid)).flatten()
}

/// Every inventory item of the player (page 0, not the cube, not worn).
pub fn inventory_items(s: &mut Sim, player: UnitId) -> Vec<HeldItem> {
    with_desk!(s, |d| {
        d.state
            .items_of(player)
            .into_iter()
            .filter_map(|u| held(d, player, d.guid_of(u)))
            .filter(|h| !h.cursor)
            .collect()
    })
    .unwrap_or_default()
}

/// Lifts the held item `guid` off the player as its save entry: the item
/// and its socketed items leave the game (the client is told).
pub fn lift_item(s: &mut Sim, player: UnitId, guid: u32) -> Result<LiftedItem, String> {
    with_desk!(s, |d| {
        let held = held(d, player, guid).ok_or("no such item in the inventory or on the cursor")?;
        let u = d.item_unit(guid).ok_or("no item unit")?;
        let isc = d.econ.tables.isc.clone();
        let entry = item_entry(&*d, u, &isc).map_err(|e| e.to_string())?;
        d.apply_write_backs();
        let id = d.econ.items.ids().id_of(u);
        remove(d, player, u);
        Ok(LiftedItem {
            held,
            bytes: entry.bytes,
            id,
        })
    })
    .unwrap_or_else(|| Err("no inventory model in this game".into()))
}

/// Removes held item `u` and its socketed items from the game.
fn remove<H, R>(d: &mut InvDesk<'_, '_, H, R>, player: UnitId, u: UnitId)
where
    H: LifecycleHooks,
    R: InvRest + ?Sized,
{
    let fillers = d.state.fillers(u);
    d.delete_held_item(player, u);
    for f in fillers {
        free_item_tree(d, f);
    }
}

/// Removes the held item `guid` (a placement taken back). False when the
/// player holds no such item.
pub fn remove_item(s: &mut Sim, player: UnitId, guid: u32) -> bool {
    with_desk!(s, |d| {
        if held(d, player, guid).is_none() {
            return false;
        }
        let Some(u) = d.item_unit(guid) else {
            return false;
        };
        remove(d, player, u);
        true
    })
    .unwrap_or(false)
}

/// Makes the item of a save entry and puts it in the inventory or on the
/// cursor. Returns its GUID; on any failure nothing is left in the game.
pub fn place_item(s: &mut Sim, player: UnitId, bytes: &[u8], to: PlaceTo) -> Result<u32, String> {
    with_desk!(s, |d| {
        if to == PlaceTo::Cursor && d.state.cursor_of(player).is_some() {
            return Err("the cursor holds an item".to_string());
        }
        let owner = d.owner_of(player).ok_or("the player has no inventory")?;
        let rec =
            read_save_entry(bytes, d.econ.tables).map_err(|e| format!("unreadable item: {e:?}"))?;
        let u = d
            .make_entry(&rec)
            .map_err(|e| format!("no item made: {e:?}"))?;
        if let Some(it) = d.state.items.get_mut(&u) {
            it.mode = mode::CURSOR;
            it.page = INVENTORY;
        }
        d.sync_out();
        let guid = d.guid_of(u);
        let ok = match to {
            PlaceTo::Inventory(at) => {
                let (xy, free) = match at {
                    Some(xy) => (xy, false),
                    None => ((0, 0), true),
                };
                d.place(player, u, xy, free, true)
                    || (!free && d.place(player, u, (0, 0), true, true))
            }
            PlaceTo::Cursor => {
                d2_sim::items::moves::handlers::load_to_cursor(d, owner, guid);
                true
            }
        };
        if !ok {
            for f in d.state.fillers(u) {
                free_item_tree(d, f);
            }
            d.free(u);
            return Err("no room in the inventory".into());
        }
        Ok(guid)
    })
    .unwrap_or_else(|| Err("no inventory model in this game".into()))
}

/// The inventory's free place for one item of `size`.
fn fits<H, R>(d: &InvDesk<'_, '_, H, R>, player: UnitId, size: (u8, u8)) -> Option<(i32, i32)>
where
    H: LifecycleHooks,
    R: InvRest + ?Sized,
{
    let inv = d.state.of(player)?;
    match inv.grid(grid_id::PAGE + usize::from(INVENTORY)) {
        Some(g) => search(g, size.0, size.1, true),
        None => {
            let (w, h) =
                page_grid_size(d.tables, inv.owner_kind, INVENTORY, InvWorld::expansion(d))?;
            (size.0 <= w && size.1 <= h).then_some((0, 0))
        }
    }
}

/// Whether the item of save entry `bytes` fits there now (the cursor:
/// when it is free).
pub fn entry_fits(s: &mut Sim, player: UnitId, bytes: &[u8], to: PlaceTo) -> bool {
    with_desk!(s, |d| {
        let Ok(rec) = read_save_entry(bytes, d.econ.tables) else {
            return false;
        };
        let Some(size) = d.tables.size(rec.item.record).filter(|_| !rec.item.failed) else {
            return false;
        };
        match to {
            PlaceTo::Cursor => d.state.cursor_of(player).is_none(),
            PlaceTo::Inventory(_) => fits(d, player, size).is_some(),
        }
    })
    .unwrap_or(false)
}

/// Whether an item of `size` fits in the inventory now.
pub fn inventory_fits(s: &mut Sim, player: UnitId, size: (u8, u8)) -> bool {
    with_desk!(s, |d| fits(d, player, size).is_some()).unwrap_or(false)
}

/// The free inventory cells.
fn free_cells<H, R>(d: &InvDesk<'_, '_, H, R>, player: UnitId) -> u64
where
    H: LifecycleHooks,
    R: InvRest + ?Sized,
{
    let Some(inv) = d.state.of(player) else {
        return 0;
    };
    match inv.grid(grid_id::PAGE + usize::from(INVENTORY)) {
        Some(g) => g.cells.iter().filter(|c| c.is_none()).count() as u64,
        None => page_grid_size(d.tables, inv.owner_kind, INVENTORY, InvWorld::expansion(d))
            .map_or(0, |(w, h)| u64::from(w) * u64::from(h)),
    }
}

/// The record and the per-item count of a plain stackable `code` (keys:
/// the game's stack size; anything else 1). Stackable kinds are one cell.
fn plain_kind<H, R>(d: &InvDesk<'_, '_, H, R>, code: &str) -> Option<(usize, u32)>
where
    H: LifecycleHooks,
    R: InvRest + ?Sized,
{
    kind_of(d, code)
        .filter(|k| k.2 == (1, 1))
        .map(|(i, per, _)| (i, per))
}

/// `code`'s items row, how many one item holds and its size.
fn kind_of<H, R>(d: &InvDesk<'_, '_, H, R>, code: &str) -> Option<(usize, u32, (u8, u8))>
where
    H: LifecycleHooks,
    R: InvRest + ?Sized,
{
    let key = code_key(code)?;
    let (i, r) = d
        .tables
        .items
        .iter()
        .enumerate()
        .find(|(_, r)| r.code == key)?;
    let per = if r.stackable != 0 {
        r.maxstack.max(1)
    } else {
        1
    };
    Some((i, per, (r.invwidth, r.invheight)))
}

/// Whether the game makes plain `code` items at all (a one-cell kind of
/// its item data).
pub fn plain_kind_known(s: &mut Sim, code: &str) -> bool {
    with_desk!(s, |d| plain_kind(d, code).is_some()).unwrap_or(false)
}

/// How many plain `code` items fit in the inventory now (keys: their count).
pub fn plain_room(s: &mut Sim, player: UnitId, code: &str) -> u64 {
    with_desk!(s, |d| {
        let Some((_, per)) = plain_kind(d, code) else {
            return 0;
        };
        free_cells(d, player) * u64::from(per)
    })
    .unwrap_or(0)
}

/// Makes `count` plain `code` items (normal quality; a stackable kind in
/// full stacks) in the inventory: all or none. Returns their GUIDs.
pub fn give_plain(s: &mut Sim, player: UnitId, code: &str, count: u32) -> Result<Vec<u32>, String> {
    let key = code_key(code).ok_or("bad item code")?;
    let made = s
        .world
        .with_economy(&mut s.game, &mut s.events, |econ, parts| {
            let inv = parts
                .inventory
                .as_deref_mut()
                .ok_or("no inventory model in this game")?;
            let (_, per, _) = kind_of(&inv.desk(econ), code).ok_or("no such item kind")?;
            let mut left = count;
            let mut made: Vec<UnitId> = Vec::new();
            let mut fault = None;
            while left > 0 {
                let n = left.min(per);
                let item = match create_reward(econ, player, key, 0, NORMAL) {
                    Ok(Some(u)) => u,
                    Ok(None) => {
                        fault = Some("the game refused to make the item".to_string());
                        break;
                    }
                    Err(e) => {
                        fault = Some(e.to_string());
                        break;
                    }
                };
                if per > 1 {
                    econ.stats
                        .unit_set(&mut *econ.hooks, item, stat::QUANTITY, n as i32, 0);
                }
                if let Some(i) = econ.items.get_mut(item) {
                    i.flags |= flag::IDENTIFIED;
                }
                let mut d = inv.desk(econ);
                if !d.place(player, item, (0, 0), true, true) {
                    d.free(item);
                    fault = Some("no room in the inventory".into());
                    break;
                }
                d.sync_out();
                made.push(item);
                left -= n;
            }
            let mut d = inv.desk(econ);
            if let Some(f) = fault {
                for u in made {
                    d.delete_held_item(player, u);
                }
                d.sync_out();
                return Err(f);
            }
            Ok(made.into_iter().map(|u| d.guid_of(u)).collect())
        });
    let sent = s.world.take_inventory_sent(&mut s.game, &mut s.events);
    s.world.rest.sent.extend(sent);
    made
}

/// The player's gold.
pub fn gold(s: &Sim, player: UnitId) -> u64 {
    s.events
        .action
        .sys
        .stats
        .unit_base(player, stat::GOLD, 0)
        .max(0) as u64
}

/// The most gold the player carries: 10,000 a level.
pub fn gold_max(s: &Sim, player: UnitId) -> u64 {
    let level = s
        .events
        .action
        .sys
        .stats
        .unit_base(player, stat::LEVEL, 0)
        .max(1) as u64;
    level * 10_000
}

/// Sets the player's gold (kept within 0 and [`gold_max`]).
pub fn set_gold(s: &mut Sim, player: UnitId, value: u64) {
    let v = value.min(gold_max(s, player)).min(i32::MAX as u64) as i32;
    let sys = &mut s.events.action.sys;
    sys.stats.unit_set(&mut sys.hooks, player, stat::GOLD, v, 0);
}
