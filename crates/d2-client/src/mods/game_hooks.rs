//! The client hooks of a mod game's own game traffic and map (G7): its
//! mod messages ([`ClientMods::with_mod_messages`]) and its automap
//! markers ([`AutomapMarkers`]).

use std::fmt;
use std::sync::Arc;

use crate::bridge::world::ClientWorld;
use crate::bridge::ModMessages;

use super::{ClientMods, MakeModMessages};

/// The hooks of this file, held by [`ClientMods`].
#[derive(Clone, Default)]
pub(super) struct GameHooks {
    mod_messages: Option<Arc<MakeModMessages>>,
    automap_markers: Vec<Arc<dyn AutomapMarkers>>,
}

impl GameHooks {
    pub(super) fn is_empty(&self) -> bool {
        self.mod_messages.is_none() && self.automap_markers.is_empty()
    }
}

impl fmt::Debug for GameHooks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GameHooks")
            .field("mod_messages", &self.mod_messages.is_some())
            .field("automap_markers", &self.automap_markers.len())
            .finish()
    }
}

impl ClientMods {
    /// The client side of the mod game's mod messages (MS13): `make` is
    /// called once for each game the app starts, and the game's bridge
    /// then speaks the mod wire, handing the hook every S→C mod message
    /// (`crate::bridge::Bridge::set_mod_messages`). Without it the bridge
    /// stays on D2RUST's own wire and refuses mod messages. A second call
    /// replaces the first.
    pub fn with_mod_messages(
        mut self,
        make: impl Fn() -> Box<dyn ModMessages> + Send + Sync + 'static,
    ) -> ClientMods {
        self.game.mod_messages = Some(Arc::new(make));
        self
    }

    /// A new mod message hook for a starting game; `None` without one.
    pub fn mod_messages(&self) -> Option<Box<dyn ModMessages>> {
        self.game.mod_messages.as_ref().map(|make| make())
    }

    /// Adds a source of automap markers.
    pub fn with_automap_markers(mut self, hook: impl AutomapMarkers + 'static) -> ClientMods {
        self.game.automap_markers.push(Arc::new(hook));
        self
    }

    /// Some automap marker source is registered.
    pub fn has_automap_markers(&self) -> bool {
        !self.game.automap_markers.is_empty()
    }

    /// This frame's markers of every source, in registration order.
    pub fn automap_markers(&self, world: &ClientWorld) -> Vec<AutomapMarker> {
        let mut out = Vec::new();
        for hook in &self.game.automap_markers {
            hook.markers(world, &mut out);
        }
        out
    }
}

/// One mod marker on the automap: a cross like a unit's, with an optional
/// name above it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutomapMarker {
    /// The marked spot in subtiles (a static unit's position).
    pub at: (i32, i32),
    /// The cross colour; the nearest colour of the act palette is drawn.
    pub rgb: (u8, u8, u8),
    /// UTF-16 name drawn above the cross (empty: none).
    pub name: Vec<u16>,
    /// The name's text colour index (`ui::item_tip::color`).
    pub name_color: u16,
}

/// A source of automap markers (G7): spots a mod game marks on the map
/// (its own objects, goals). Called once per drawn frame while the automap
/// shows; the markers are drawn after the units' and inside the same
/// marker rectangle.
pub trait AutomapMarkers: Send + Sync {
    /// Appends this frame's markers, read from the client's model of the
    /// game.
    fn markers(&self, world: &ClientWorld, out: &mut Vec<AutomapMarker>);
}

#[cfg(test)]
#[path = "game_hooks_tests.rs"]
mod tests;
