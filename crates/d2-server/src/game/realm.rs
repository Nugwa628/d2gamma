//! A realm game's inputs from the main server (plan C3): the seed the game
//! starts on and the blocks of realm item IDs its items take
//! (`d2_sim::wiring::economy::item_ids`). The game server passes them in
//! from `StartGame` and `ItemIds`, and sends `ItemIdsWanted` for each
//! request [`Sim::take_item_ids_wanted`] returns. Every other game keeps the
//! defaults: its own seed and the items' init seeds as their IDs.

pub use d2_sim::wiring::economy::{BlockIds, GrantError, IdBlock, ItemIdSource, ItemIds};

use super::Sim;

/// How a realm game starts (`StartGame`): the seed the main server picked
/// and the first block of item IDs it granted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RealmStart {
    pub seed: u32,
    pub item_ids: IdBlock,
}

impl Sim {
    /// The game's item IDs.
    pub fn item_ids(&mut self) -> &mut ItemIds {
        self.events.action.hooks().items.ids_mut()
    }

    /// Adds a block of item IDs the main server granted (`ItemIds`). Call
    /// between frames, at the frame the journal records it.
    pub fn grant_item_ids(&mut self, block: IdBlock) -> Result<(), GrantError> {
        self.item_ids().grant(block)
    }

    /// The block requests the game raised since the last call; each is
    /// one `ItemIdsWanted` to the main server.
    pub fn take_item_ids_wanted(&mut self) -> u32 {
        self.item_ids().take_wanted()
    }
}
