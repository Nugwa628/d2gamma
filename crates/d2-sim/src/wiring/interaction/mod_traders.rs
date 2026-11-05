//! The vendor records of a mod game: which NPC classes keep a store.
//!
//! D2Gamma's NPC table marks its own traders. A mod game may add NPC
//! classes the table does not know, or give a class a trade or gamble role
//! ([`crate::world::npc::NpcProvider::roles`]); such a class has a store
//! record like any trader, so its store is generated at a trade open (from
//! the mod's [`crate::world::vendors::StoreProvider`] stock) and its gamble
//! list is made at a gamble open.

use super::InteractionState;
use crate::rules::Ruleset;
use crate::world::npc::NpcControl;
use crate::world::vendors::GlobalLists;

/// Whether the vendor record of `class` is a trader's: the NPC table
/// says so (`table_trader`), or the mod's roles for the class trade or
/// gamble. Under [`Ruleset::Original`], and for a class the mod gives no
/// roles, it is the table's answer alone.
pub fn keeps_store(rules: &Ruleset, class: u16, table_trader: bool) -> bool {
    table_trader
        || rules
            .npcs()
            .and_then(|p| p.roles(class))
            .is_some_and(|r| r.trades || r.gambles)
}

impl InteractionState {
    /// Replaces the vendor records with the ones `rules` makes: the store
    /// roles and traders of a mod game ([`InteractionState::new_for`]).
    /// For a host that created the state under D2Gamma's rules before it
    /// knew the game's rules; `globals` is [`GlobalLists::build_for`]
    /// under the same rules. Every other part of the state is kept.
    pub fn use_rules(&mut self, rules: &Ruleset, ctl: &NpcControl, globals: &GlobalLists) {
        self.vendors = Self::new_for(rules, ctl, globals).vendors;
    }
}
