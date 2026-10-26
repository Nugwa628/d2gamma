//! The item-moving shortcuts on the player's grids ([`crate::ui::item_moves`]):
//! the item under a modifier-click, the fit check on the page it goes to
//! and the messages that move it, or the refusal; and the shortcut lines
//! of a stored item's tool tip.

use super::{ItemsUi, PanelOutput};
use crate::bridge::items::{self, mode, ItemView};
use crate::bridge::world::ClientWorld;
use crate::ui::geom::Point;
use crate::ui::inv_grid::GridRecord;
use crate::ui::item_moves::{
    self, first_free_spot, move_intents, page, Action, Button, Dest, HintFacts, Mods, Open, Rect,
    Refusal, Shortcut, ShortcutSet,
};
use crate::ui::item_tip::TipLine;

/// The column × row sizes of the grids a shortcut moves items between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSizes {
    pub inventory: (i32, i32),
    pub stash: (i32, i32),
    pub cube: (i32, i32),
    pub trade: (i32, i32),
}

impl PageSizes {
    fn of(&self, pg: u8) -> (i32, i32) {
        match pg {
            page::STASH => self.stash,
            page::CUBE => self.cube,
            page::TRADE => self.trade,
            _ => self.inventory,
        }
    }
}

/// The trade box's grid (`inventory.bin` record 7) without the table rows.
const TRADE_GRID: (i32, i32) = (10, 4);

/// What a modifier-click made of: the shortcut's messages, or why it sent
/// nothing.
pub type ShortcutOutcome = Result<Vec<PanelOutput>, Refusal>;

impl ItemsUi {
    /// The shortcuts in force: the settings' switches, less the ones a mod
    /// game switches off.
    pub fn shortcut_set(&self) -> ShortcutSet {
        self.shortcuts.and(self.mod_shortcuts)
    }

    /// Whether shortcut `s` is on.
    pub fn shortcut_on(&self, s: Shortcut) -> bool {
        self.shortcut_set().has(s)
    }

    /// The sizes of the player's grids: the inventory of `layout`, the
    /// stash and cube grids of this install, the trade box's record 7.
    pub fn page_sizes(
        &self,
        inventory: &GridRecord,
        expansion: bool,
        screen: &crate::ui::layout::Screen,
    ) -> PageSizes {
        let size = |g: &GridRecord| (i32::from(g.grid_x), i32::from(g.grid_y));
        let trade = self
            .layouts
            .as_ref()
            .and_then(|l| l.get(7))
            .map_or(TRADE_GRID, |l| size(&l.grid));
        PageSizes {
            inventory: size(inventory),
            stash: size(&self.stash_grid(expansion, screen)),
            cube: size(&self.cube_grid(screen)),
            trade,
        }
    }

    /// An item's footprint in cells from its art row (1 × 1 without one).
    pub(crate) fn footprint(&self, it: &ItemView) -> (i32, i32) {
        self.art.get(it.code.unwrap_or([0; 4])).map_or((1, 1), |r| {
            (i32::from(r.inv_w.max(1)), i32::from(r.inv_h.max(1)))
        })
    }

    /// The footprints of the local player's stored items on page `pg`.
    fn taken(&self, world: &ClientWorld, pg: u8) -> Vec<Rect> {
        items::local_items(world)
            .iter()
            .filter(|i| i.mode == mode::STORED && i.page == pg)
            .map(|i| {
                let (w, h) = self.footprint(i);
                (i32::from(i.x), i32::from(i.y), w, h)
            })
            .collect()
    }

    /// How many cells of the local player's inventory grid of `size` no
    /// stored item covers.
    pub fn free_inventory_cells(&self, world: &ClientWorld, (cols, rows): (i32, i32)) -> u64 {
        let (cols, rows) = (cols.max(0), rows.max(0));
        let mut free = vec![true; (cols * rows) as usize];
        for (x, y, w, h) in self.taken(world, page::INVENTORY) {
            for cy in y.max(0)..(y + h).min(rows) {
                for cx in x.max(0)..(x + w).min(cols) {
                    free[(cy * cols + cx) as usize] = false;
                }
            }
        }
        free.iter().filter(|f| **f).count() as u64
    }

    /// The stored item of page `pg` under `at` on grid `g`.
    pub fn grid_item_at(
        &self,
        world: &ClientWorld,
        g: &GridRecord,
        pg: u8,
        at: Point,
    ) -> Option<ItemView> {
        if g.cell_w == 0 || g.cell_h == 0 || !g.contains_mouse(at) {
            return None;
        }
        let (c, r) = g.mouse_cell(at);
        let (c, r) = (c as i32, r as i32);
        items::local_items(world).into_iter().find(|i| {
            let (w, h) = self.footprint(i);
            let (x, y) = (i32::from(i.x), i32::from(i.y));
            i.mode == mode::STORED
                && i.page == pg
                && (x..x + w).contains(&c)
                && (y..y + h).contains(&r)
        })
    }

    /// The first Horadric Cube stored in the inventory.
    fn inventory_cube(&self, world: &ClientWorld) -> Option<ItemView> {
        items::local_items(world).into_iter().find(|i| {
            i.mode == mode::STORED && i.page == page::INVENTORY && i.code == Some(*b"box ")
        })
    }

    /// A click with `mods` and `button` on stored item `it` with the windows
    /// `open`: `None` when it is no shortcut (or the vendor's sell, which
    /// the grid click sends), else the shortcut's messages or why it sent
    /// nothing. Nothing is sent while an item is on the cursor.
    pub fn shortcut_press(
        &self,
        world: &ClientWorld,
        sizes: &PageSizes,
        it: &ItemView,
        mods: Mods,
        button: Button,
        open: Open,
    ) -> Option<ShortcutOutcome> {
        if it.mode != mode::STORED || items::cursor_item(world).is_some() {
            return None;
        }
        let (s, action) =
            item_moves::shortcut_for(self.shortcut_set(), mods, button, it.page, open)?;
        let guid = it.key.guid;
        let is_cube = it.code == Some(*b"box ");
        let out = match action {
            Action::Sell => return None,
            Action::Move { to } => {
                let locked = open.trade.is_some_and(|t| t.locked);
                if s == Shortcut::Trade && locked {
                    return Some(Err(Refusal::TradeLocked));
                }
                if to == page::CUBE && is_cube {
                    return Some(Err(Refusal::CubeInCube));
                }
                let Some((x, y)) =
                    first_free_spot(sizes.of(to), &self.taken(world, to), self.footprint(it))
                else {
                    return Some(Err(Refusal::NoRoom(to)));
                };
                move_intents(guid, Dest::Page { page: to, x, y })
            }
            Action::Drop => {
                if open.trade.is_some() {
                    return Some(Err(Refusal::Trading));
                }
                move_intents(guid, Dest::Ground)
            }
            Action::IntoCube => {
                if is_cube {
                    return Some(Err(Refusal::CubeInCube));
                }
                let Some(cube) = self.inventory_cube(world) else {
                    return Some(Err(Refusal::NoCube));
                };
                let taken = self.taken(world, page::CUBE);
                if first_free_spot(sizes.cube, &taken, self.footprint(it)).is_none() {
                    return Some(Err(Refusal::NoRoom(page::CUBE)));
                }
                move_intents(
                    guid,
                    Dest::Cube {
                        cube: cube.key.guid,
                    },
                )
            }
        };
        self.clear_hover();
        Some(Ok(out.into_iter().map(PanelOutput::Intent).collect()))
    }

    /// The shortcut lines at the bottom of stored item `it`'s tool tip with
    /// the windows `open` (none for an equipped item).
    pub fn shortcut_hints(&self, world: &ClientWorld, it: &ItemView, open: Open) -> Vec<TipLine> {
        if it.mode != mode::STORED {
            return Vec::new();
        }
        let facts = HintFacts {
            is_cube: it.code == Some(*b"box "),
            sellable: self
                .tips
                .as_ref()
                .zip(it.code)
                .is_some_and(|(t, code)| super::facts::GridInfo::sellable(t, code)),
            cube_in_inventory: self.inventory_cube(world).is_some(),
        };
        item_moves::hint_lines(self.shortcut_set(), it.page, open, facts)
    }

    /// The tool tip lines of the stored item under `at` on the stash or cube
    /// grid `g` (page `pg`): the hovered item of the hover state, as the
    /// inventory's tip; none with a cursor item or without tips.
    pub fn grid_hover_lines(
        &self,
        world: &ClientWorld,
        g: &GridRecord,
        pg: u8,
        at: Point,
    ) -> Option<(ItemView, Vec<TipLine>, crate::ui::item_tip::TipAnchor)> {
        let tips = self.tips.as_ref()?;
        if items::cursor_item(world).is_some() {
            return None;
        }
        let it = self.grid_item_at(world, g, pg, at)?;
        let hs = self.hover.get();
        if !(hs.in_grid && hs.item == Some(it.key.guid)) {
            return None;
        }
        let me = crate::ui::item_tip_world::WorldUnit::local(world);
        let ctx = crate::ui::item_tip_world::hover_ctx(tips, world, me.as_ref(), &it);
        let lines = crate::ui::item_tip_world::model_bits(tips, world, it.key)
            .map_or_else(Vec::new, |b| tips.tip_lines_of(&b, &ctx));
        let cell = (i32::from(g.cell_w), i32::from(g.cell_h));
        let anchor = crate::ui::item_tip::TipAnchor::of_cell(
            (g.left, g.top),
            cell,
            (i32::from(it.x), i32::from(it.y)),
            self.footprint(&it),
        );
        Some((it, lines, anchor))
    }
}

#[cfg(test)]
#[path = "inv_items_moves_tests.rs"]
mod tests;
