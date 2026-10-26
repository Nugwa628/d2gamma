//! The play UI's side of the item-moving shortcuts
//! ([`crate::ui::item_moves`]): which windows are open, the player's grid
//! sizes, the outcome of a modifier-click on an item (its messages queued,
//! or a short screen note when it can't move the item) and the shortcut
//! lines at the bottom of the hovered item's tool tip.

use super::{msg_ui, Shared};
use crate::bridge::items::ItemView;
use crate::bridge::world::ClientWorld;
use crate::ui::item_moves::{Button, Mods, Open, Shortcut, Trade};
use crate::ui::item_tip::TipLine;
use crate::ui::panels::inv_items::moves::PageSizes;
use crate::ui::states::id;

impl Shared {
    /// The windows a shortcut reads: the stash, the cube, the player trade
    /// (with its lock) and a vendor's trade window.
    pub(super) fn shortcut_open(&self) -> Open {
        Open {
            stash: self.states.is_open(id::STASH),
            cube: self.states.is_open(id::CUBE),
            trade: self
                .states
                .is_open(msg_ui::UI_MPTRADE as u8)
                .then_some(Trade {
                    locked: self.items.trade_locked,
                }),
            store: self.items.store_npc.is_some(),
        }
    }

    /// The modifier keys held (set by the host each frame).
    fn shortcut_mods(&self) -> Mods {
        Mods {
            ctrl: self.items.ctrl,
            shift: self.items.shift,
        }
    }

    /// The player's grid sizes; none without a local player's layout.
    fn page_sizes(&self) -> Option<PageSizes> {
        let s = &self.config.screen;
        let l = self.items.layout(self.facts.class, s)?;
        Some(self.items.page_sizes(&l.grid, self.env().exp, s))
    }

    /// The free cells of the local player's inventory; none without its
    /// layout.
    pub(super) fn free_inventory_cells(&self, world: &ClientWorld) -> Option<u64> {
        let sizes = self.page_sizes()?;
        Some(self.items.free_inventory_cells(world, sizes.inventory))
    }

    /// A press of `button` on stored item `it` with the modifiers held:
    /// true when it was a shortcut (its messages are queued, or the note
    /// saying why it moved nothing is shown); false when the plain click
    /// is to run.
    pub(super) fn item_shortcut(
        &mut self,
        world: &ClientWorld,
        it: Option<&ItemView>,
        button: Button,
    ) -> bool {
        let (Some(it), Some(sizes)) = (it, self.page_sizes()) else {
            return false;
        };
        let (mods, open) = (self.shortcut_mods(), self.shortcut_open());
        let Some(outcome) = self
            .items
            .shortcut_press(world, &sizes, it, mods, button, open)
        else {
            return false;
        };
        match outcome {
            Ok(out) => self.outputs.extend(out),
            Err(why) => self.messages.add_note(why.text()),
        }
        true
    }

    /// The grey shortcut lines at the bottom of item `it`'s tool tip.
    pub(super) fn shortcut_hints(&self, world: &ClientWorld, it: &ItemView) -> Vec<TipLine> {
        let mut lines = self.items.shortcut_hints(world, it, self.shortcut_open());
        // The shared stash's own Ctrl+Left (to a stack slot or a shared
        // page) replaces the move to the game's stash.
        if let Some(l) = super::stash_ui::inventory_hint(self, world, it) {
            let ctrl_left: Vec<u16> = Shortcut::Stash.keys().encode_utf16().collect();
            lines.retain(|t| !t.text.starts_with(&ctrl_left));
            lines.insert(0, l);
        }
        lines
    }
}
