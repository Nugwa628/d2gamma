//! A mod game's save hooks: its own data in a character save.
//!
//! A mod game keeps per-player state the engine's save model has no place
//! for (counters, unlocks, extra stash pages). Through [`SaveHooks`],
//! handed out by [`super::ModRules::saves`], it writes that state into the
//! save as tagged [`ModSection`]s each time the game saves the player, and
//! gets the sections back when the character joins a game. The engine
//! never reads the sections' bytes; the mod game's own save codec stores
//! them (a `.d2s` file has no place for them). Under
//! [`super::Ruleset::Original`] nothing is asked and saves carry no
//! sections.

use std::fmt::Debug;

pub use d2_formats::d2s::ModSection;

use super::{EventHost, Ruleset};
use crate::units::UnitId;

/// A mod game's save hooks. Every hook does nothing by default.
pub trait SaveHooks: Send + Sync + Debug {
    /// The mod sections of `player`'s save, read when the game saves the
    /// player. They replace the sections the character was loaded with,
    /// so a section the answer leaves out is dropped from the save.
    fn save_sections(&self, h: &mut dyn EventHost, player: UnitId) -> Vec<ModSection> {
        let _ = (h, player);
        Vec::new()
    }
    /// `player` joined the game from a save holding `sections` (empty for
    /// a new character), after the engine's own load of the save and
    /// before the player enters the game.
    fn load_sections(&self, h: &mut dyn EventHost, player: UnitId, sections: &[ModSection]) {
        let _ = (h, player, sections);
    }
}

impl Ruleset {
    /// The mod game's save hooks, `None` under [`Ruleset::Original`] or
    /// when the mod has none.
    pub fn saves(&self) -> Option<&dyn SaveHooks> {
        self.mod_rules()?.saves()
    }
}
