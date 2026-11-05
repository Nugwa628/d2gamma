//! A mod game's NPC interaction (`docs/plans/mods/seams.md` MS8c): which
//! NPC classes trade, gamble, heal, identify, sell hirelings and
//! resurrect, and the mod's own menu actions.
//!
//! The mod answers first and D2RUST's code runs when it doesn't:
//! - **Roles.** [`NpcProvider::roles`] gives a class's roles; `None` keeps
//!   D2RUST's lists ([`NpcRoles::d2rust`]). A mod's own NPC classes (any
//!   monstats row with `interact`) get roles this way, and a mod may change
//!   a D2RUST NPC's roles.
//!   [`NpcProvider::player_roles`] answers per player (roles a player's
//!   progress unlocks) where a player is at hand: the menu actions, heal,
//!   identify, the hire list and resurrect.
//! - **Menu actions.** [`NpcProvider::menu_action`] runs a C→S 0x38 menu
//!   action before D2RUST's (trade, gamble, hire list, services). `None`
//!   runs D2RUST's action. The message checks (size, unit check, the
//!   `interact` flag, an interaction list) stay D2RUST's and run first.
//!
//! Action numbers are message fields and classes are monstats rows, so
//! there is no slot count to extend. Providers get the NPC world as
//! `dyn NpcHost` and must stay deterministic: no I/O, no clock, no floats
//! in game state, draws only from the game's seeds (the NPC-control seed
//! in [`NpcControl`], item seeds through [`NpcWorld::item_seed`]).

use std::fmt::Debug;

use super::hire::{HireRow, HireSlot};
use super::npc_rules::{HireRequest, MercGrowth};
use super::{NpcControl, NpcError, NpcVendors, NpcWorld};
use super::{GAMBLERS, HEALERS, IDENTIFIERS, RESURRECTORS, SELLERS, TRADERS};
use crate::rng::Seed;
use crate::rules::Ruleset;
use crate::units::UnitId;
use crate::world::hirelings::rows::{HirelingRow, Offer};
use crate::world::quests::{QuestWorld, TextList};

/// Every seam an NPC interaction needs, as one object-safe trait.
pub trait NpcHost: NpcWorld + NpcVendors {}
impl<T: NpcWorld + NpcVendors> NpcHost for T {}

/// What an NPC class does at its menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NpcRoles {
    /// Menu action 1 opens its store.
    pub trades: bool,
    /// Menu action 2 opens its gamble list.
    pub gambles: bool,
    /// A chat open heals the player and the player's pets.
    pub heals: bool,
    /// C→S 0x34 identifies the player's items.
    pub identifies: bool,
    /// It sells mercenaries (hire list).
    pub hires: bool,
    /// It resurrects the player's mercenary.
    pub resurrects: bool,
}

impl NpcRoles {
    /// D2RUST's roles of `class`.
    pub fn d2rust(class: u16) -> Self {
        Self {
            trades: TRADERS.contains(&class),
            gambles: GAMBLERS.contains(&class),
            heals: HEALERS.contains(&class),
            identifies: IDENTIFIERS.contains(&class),
            hires: SELLERS.contains(&class),
            resurrects: RESURRECTORS.contains(&class),
        }
    }

    /// The roles of `class` under `rules`: the mod's, else D2RUST's.
    pub fn of(rules: &Ruleset, class: u16) -> Self {
        rules
            .npcs()
            .and_then(|p| p.roles(class))
            .unwrap_or_else(|| Self::d2rust(class))
    }

    /// The roles of `class` for `player` in world `w`: the mod's answer for
    /// the player, else [`NpcRoles::of`].
    pub fn for_player<W: NpcWorld>(w: &W, class: u16, player: UnitId) -> Self {
        let rules = w.rules();
        rules
            .npcs()
            .and_then(|p| p.player_roles(w, class, player))
            .unwrap_or_else(|| Self::of(rules, class))
    }
}

/// A mod game's NPC roles and menu actions. Every method defaults to "not
/// the mod's", so a provider only writes what its game adds.
pub trait NpcProvider: Send + Sync + Debug {
    /// The roles of NPC `class`; `None`: D2RUST's.
    fn roles(&self, class: u16) -> Option<NpcRoles> {
        let _ = class;
        None
    }

    /// The roles of NPC `class` for `player`, read from world `w` (roles a
    /// player's progress unlocks); `None`: the class's
    /// ([`NpcProvider::roles`]).
    fn player_roles(&self, w: &dyn NpcWorld, class: u16, player: UnitId) -> Option<NpcRoles> {
        let _ = (w, class, player);
        None
    }

    /// Run menu `action` (with the message's `item` GUID) of NPC `npc` of
    /// `class` for `player`. `Some`: the message result (or a fatal
    /// error); `None`: D2RUST's action runs.
    #[allow(clippy::too_many_arguments)]
    fn menu_action(
        &self,
        ctl: &mut NpcControl,
        w: &mut dyn NpcHost,
        player: UnitId,
        npc: UnitId,
        class: u16,
        action: u32,
        item: u32,
    ) -> Option<Result<u32, NpcError>> {
        let _ = (ctl, w, player, npc, class, action, item);
        None
    }

    /// The lines NPC `npc` of `class` says to `player` at a chat open or
    /// a text refresh ([`super::npc_rules`]): `list` (string id, menu)
    /// holds the quest records' lines, and the mod may add to it or change
    /// it (an intro by the player's class, gossip that rotates by
    /// [`QuestWorld::frame`]). The default leaves it.
    fn chat_text(
        &self,
        w: &mut dyn QuestWorld,
        player: UnitId,
        npc: UnitId,
        class: u16,
        list: &mut TextList,
    ) {
        let _ = (w, player, npc, class, list);
    }

    /// The hire list of seller `class`, whose Normal `hireling` row is
    /// `row`: `Some(slots)` replaces D2RUST's draw. Slot k must name
    /// `row.name_first + k` (a hire finds its slot that way); slots past
    /// [`super::hire::HIRE_SLOTS`] are dropped. Draws come from `seed` only
    /// (the NPC-control seed).
    fn hire_list(&self, class: u16, row: &HireRow, seed: &mut Seed) -> Option<Vec<HireSlot>> {
        let _ = (class, row, seed);
        None
    }

    /// The offer of a hire slot: `Some(Some(o))` replaces D2RUST's offer
    /// both at the hire (the price, the row, the level) and at the
    /// mercenary's init (its level and experience); `Some(None)` offers
    /// nothing. `o.row` is a `hireling` row index; an offer past the rows
    /// is no offer.
    fn hire_offer(&self, req: &HireRequest) -> Option<Option<Offer>> {
        let _ = req;
        None
    }

    /// The experience of hireling level `level` on `hireling` row `row`
    /// (the next-level experience, the level-ups, the restore level, the
    /// kill gain cap); `None`: D2RUST's.
    fn merc_exp(&self, row: &HirelingRow, level: i32) -> Option<i32> {
        let _ = (row, level);
        None
    }

    /// A hireling's stats and skills at `level` (a level-up or a new
    /// hire), from `hireling` row `row`: `Some` replaces D2RUST's stat
    /// growth and skills; the level, the experience and the level speech
    /// stay D2RUST's. `req_level` gives a skill's required level (`None`:
    /// no such skill).
    fn merc_growth(
        &self,
        row: &HirelingRow,
        level: i32,
        req_level: &dyn Fn(u32) -> Option<i16>,
    ) -> Option<MercGrowth> {
        let _ = (row, level, req_level);
        None
    }
}
