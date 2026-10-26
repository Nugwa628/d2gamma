// D2Gamma's own: a mod game's stash on the player's items.
//! [`ItemHost`] on the inventory model: what a mod game's own stash
//! (`d2_sim::rules::stash`) does to a player's items in the running game.
//! Each change goes through the inventory desk as the game's own item
//! moves do, so the clients are told; the world host queues the messages
//! with the next tick's sends ([`WorldHost::mod_items`]).

use d2_sim::items::bitstream::read::read_save_entry;
use d2_sim::items::flag;
use d2_sim::items::inventory::{grid::search, grid_id, mode, page, page_grid_size, InvWorld};
use d2_sim::rules::stash::{HeldItem, ItemError, ItemHost, ItemPlace, PlaceAt};
use d2_sim::units::lifecycle::LifecycleHooks;
use d2_sim::units::UnitId;
use d2_sim::wiring::economy::item_units::Economy;
use d2_sim::wiring::economy::quest_reward::create_reward;
use d2_sim::wiring::inventory::{InvDesk, InvRest};

use crate::adapters::character::save::item_entry;
use crate::adapters::handlers::items::moves::{InvParts, MoveRest};
use crate::adapters::handlers::world::{free_item_tree, WorldHost};
use crate::game::Sim;

/// The save format's quality number of a normal item.
const NORMAL: u8 = 2;
/// The most fresh items [`ItemHost::room`] counts (a grid holds far fewer).
const ROOM_MAX: u32 = 1024;

/// The player items of one call on the host's economy and inventory model.
pub struct DeskItems<'c, 'a, H: LifecycleHooks> {
    pub econ: &'c mut Economy<'a, H>,
    pub inv: &'c mut InvParts,
}

/// Where item `u` of `player` is, and its cell.
fn place_of<H, R>(d: &InvDesk<'_, '_, H, R>, player: UnitId, u: UnitId) -> Option<HeldItem>
where
    H: LifecycleHooks,
    R: InvRest + ?Sized,
{
    let it = d.state.items.get(&u)?;
    let place = if d.state.cursor_of(player) == Some(u) {
        ItemPlace::Cursor
    } else {
        match (it.mode, it.page) {
            (mode::STORED, page::INVENTORY) => ItemPlace::Inventory,
            (mode::STORED, page::CUBE) => ItemPlace::Cube,
            (mode::STORED, page::STASH) => ItemPlace::Stash,
            (mode::EQUIPPED, _) => ItemPlace::Equipped,
            (mode::BELT, _) => ItemPlace::Belt,
            _ => return None,
        }
    };
    let rec = d.tables.item(it.record)?;
    Some(HeldItem {
        guid: d.guid_of(u),
        code: rec.code,
        size: (rec.invwidth, rec.invheight),
        place,
        cell: (it.x, it.y),
    })
}

/// The unit of item `guid` when `player` holds it.
fn held_unit<H, R>(d: &InvDesk<'_, '_, H, R>, player: UnitId, guid: u32) -> Option<UnitId>
where
    H: LifecycleHooks,
    R: InvRest + ?Sized,
{
    d.item_unit(guid).filter(|&u| d.state.holds(player, u))
}

/// Puts item `u` (made, in cursor mode, not yet placed) there; on failure
/// it is freed with its socketed items.
fn put<H, R>(d: &mut InvDesk<'_, '_, H, R>, player: UnitId, u: UnitId, at: PlaceAt) -> bool
where
    H: LifecycleHooks,
    R: InvRest + ?Sized,
{
    let ok = match (at, d.owner_of(player)) {
        (_, None) => false,
        (PlaceAt::Inventory(at), Some(_)) => {
            let (xy, free) = at.map_or(((0, 0), true), |xy| (xy, false));
            d.place(player, u, xy, free, true) || (!free && d.place(player, u, (0, 0), true, true))
        }
        (PlaceAt::Cursor, Some(owner)) => {
            let guid = d.guid_of(u);
            d2_sim::items::moves::handlers::load_to_cursor(d, owner, guid);
            true
        }
    };
    if !ok {
        for f in d.state.fillers(u) {
            free_item_tree(d, f);
        }
        d.free(u);
    }
    ok
}

/// How many items of `size` fit in the player's inventory, placed one
/// after another as the game places them.
fn fit_count<H, R>(d: &InvDesk<'_, '_, H, R>, player: UnitId, size: (u8, u8)) -> u32
where
    H: LifecycleHooks,
    R: InvRest + ?Sized,
{
    let Some(inv) = d.state.of(player) else {
        return 0;
    };
    let Some(g) = inv.grid(grid_id::PAGE + usize::from(page::INVENTORY)) else {
        // No grid yet: an empty page of the owner's size.
        let Some((w, h)) = page_grid_size(
            d.tables,
            inv.owner_kind,
            page::INVENTORY,
            InvWorld::expansion(d),
        ) else {
            return 0;
        };
        let (iw, ih) = (size.0.max(1), size.1.max(1));
        return u32::from(w / iw) * u32::from(h / ih);
    };
    let mut g = g.clone();
    let mut n = 0;
    while n < ROOM_MAX {
        let Some((x, y)) = search(&g, size.0, size.1, true) else {
            break;
        };
        for dy in 0..i32::from(size.1) {
            for dx in 0..i32::from(size.0) {
                let i = (y + dy) * i32::from(g.width) + x + dx;
                g.cells[i as usize] = Some(UnitId(u32::MAX));
            }
        }
        n += 1;
    }
    n
}

impl<'a, H: LifecycleHooks> DeskItems<'_, 'a, H> {
    fn desk(&mut self) -> InvDesk<'_, 'a, H, dyn MoveRest + Send + Sync> {
        self.inv.desk(&mut *self.econ)
    }
}

impl<H: LifecycleHooks> ItemHost for DeskItems<'_, '_, H> {
    fn items(&mut self, player: UnitId) -> Vec<HeldItem> {
        let d = self.desk();
        let cursor = d.state.cursor_of(player);
        cursor
            .into_iter()
            .chain(
                d.state
                    .items_of(player)
                    .into_iter()
                    .filter(|&u| Some(u) != cursor),
            )
            .filter_map(|u| place_of(&d, player, u))
            .collect()
    }

    fn item_stat(&mut self, player: UnitId, guid: u32, stat: u16) -> Option<i32> {
        let d = self.desk();
        let u = held_unit(&d, player, guid)?;
        Some(d.econ.stats.unit_total(u, stat, 0))
    }

    fn in_town(&mut self, player: UnitId) -> bool {
        let game = &*self.econ.game;
        game.lists
            .unit(player)
            .and_then(|e| e.room())
            .is_some_and(|r| self.econ.hooks.town_room(game, r))
    }

    fn cursor_free(&mut self, player: UnitId) -> bool {
        self.inv.state.cursor_of(player).is_none()
    }

    fn size_of(&self, code: [u8; 4]) -> Option<(u8, u8)> {
        self.inv
            .tables
            .items
            .iter()
            .find(|r| r.code == code)
            .map(|r| (r.invwidth, r.invheight))
    }

    fn room(&mut self, player: UnitId, code: [u8; 4]) -> u32 {
        let Some(size) = self.size_of(code) else {
            return 0;
        };
        let d = self.desk();
        fit_count(&d, player, size)
    }

    fn record(&mut self, player: UnitId, guid: u32) -> Option<Vec<u8>> {
        let mut d = self.desk();
        let u = held_unit(&d, player, guid)?;
        let isc = d.econ.tables.isc.clone();
        let entry = item_entry(&d, u, &isc).ok()?;
        // The writer's changes (an ID it gave) stay on the item, as a save.
        d.apply_write_backs();
        Some(entry.bytes)
    }

    fn destroy(&mut self, player: UnitId, guid: u32) -> bool {
        let mut d = self.desk();
        let Some(u) = held_unit(&d, player, guid) else {
            return false;
        };
        let cursor = d.state.cursor_of(player) == Some(u);
        let stored = d
            .state
            .items
            .get(&u)
            .is_some_and(|i| i.mode == mode::STORED);
        // Worn and belt items stay: the stash never takes them.
        if !cursor && !stored {
            return false;
        }
        let fillers = d.state.fillers(u);
        d.delete_held_item(player, u);
        for f in fillers {
            free_item_tree(&mut d, f);
        }
        d.sync_out();
        true
    }

    fn place(&mut self, player: UnitId, record: &[u8], at: PlaceAt) -> Result<u32, ItemError> {
        let mut d = self.desk();
        if at == PlaceAt::Cursor && d.state.cursor_of(player).is_some() {
            return Err(ItemError::CursorBusy);
        }
        if d.owner_of(player).is_none() {
            return Err(ItemError::Refused("the player has no inventory".into()));
        }
        let rec = read_save_entry(record, d.econ.tables)
            .map_err(|e| ItemError::Refused(format!("unreadable item: {e:?}")))?;
        let u = d
            .make_entry(&rec)
            .map_err(|e| ItemError::Refused(format!("no item made: {e:?}")))?;
        if let Some(it) = d.state.items.get_mut(&u) {
            it.mode = mode::CURSOR;
            it.page = page::INVENTORY;
        }
        d.sync_out();
        let guid = d.guid_of(u);
        if !put(&mut d, player, u, at) {
            return Err(ItemError::NoRoom);
        }
        d.sync_out();
        Ok(guid)
    }

    fn create(
        &mut self,
        player: UnitId,
        code: [u8; 4],
        stats: &[(u16, i32)],
        at: PlaceAt,
    ) -> Result<u32, ItemError> {
        if self.size_of(code).is_none() {
            return Err(ItemError::UnknownCode(code));
        }
        if at == PlaceAt::Cursor && !self.cursor_free(player) {
            return Err(ItemError::CursorBusy);
        }
        let u = match create_reward(self.econ, player, code, 0, NORMAL) {
            Ok(Some(u)) => u,
            Ok(None) => return Err(ItemError::Refused("the game made no item".into())),
            Err(e) => return Err(ItemError::Refused(e.to_string())),
        };
        for &(stat, value) in stats {
            self.econ
                .stats
                .unit_set(&mut *self.econ.hooks, u, stat, value, 0);
        }
        if let Some(i) = self.econ.items.get_mut(u) {
            i.flags |= flag::IDENTIFIED;
        }
        let mut d = self.desk();
        if let Some(it) = d.state.items.get_mut(&u) {
            it.mode = mode::CURSOR;
            it.page = page::INVENTORY;
        }
        let guid = d.guid_of(u);
        if !put(&mut d, player, u, at) {
            return Err(ItemError::NoRoom);
        }
        d.sync_out();
        Ok(guid)
    }
}
/// Runs `f` with the players' items of a running game as a mod game's
/// stash sees them ([`WorldHost::mod_items`] on the game's world).
pub fn with_items<T>(s: &mut Sim, f: impl FnOnce(&mut dyn ItemHost) -> T) -> Option<T> {
    s.world.mod_items(&mut s.game, &mut s.events, f)
}
