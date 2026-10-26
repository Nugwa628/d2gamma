//! The overlays a mod game shows on a monster from its creation
//! ([`crate::mods::ClientMods::with_monster_overlays`]): a monster's look
//! by class and kind (a unique's or a boss's aura, a class's glow), not a
//! state's.
//!
//! D2RUST's own monsters have none: without a hook nothing is made.

use std::fmt;
use std::sync::Arc;

use super::world::{ClientWorld, ModelInputs, MonsterData, UnitKey};

/// Kind of the overlay create a monster overlay is made with: the same
/// as a state's own overlay (it stays until removed).
const OVERLAY_KIND: u8 = 3;

/// A mod game's monster overlays.
pub trait MonsterOverlays: Send + Sync {
    /// The `overlay` ids monster class `class` shows from its creation,
    /// in creation order. `flags` are its type flags (superunique 0x2,
    /// champion 0x4, unique 0x8, minion 0x10) and `umods` its unique
    /// modifiers (0: none). Empty: none.
    fn overlays(&self, class: u16, flags: u8, umods: &[u8]) -> Vec<u16>;
}

/// The registered [`MonsterOverlays`] hook, as the model inputs hold it.
#[derive(Clone)]
pub struct MonsterOverlayHook(pub Arc<dyn MonsterOverlays>);

impl fmt::Debug for MonsterOverlayHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MonsterOverlayHook")
    }
}

/// Makes the hook's overlays on the monster just added (the monster
/// assign): each one as a state's overlay is made. No hook: nothing.
pub(super) fn start(
    w: &mut ClientWorld,
    inputs: &ModelInputs,
    key: UnitKey,
    class: u16,
    data: &MonsterData,
) {
    let Some(hook) = &inputs.monster_overlays else {
        return;
    };
    let umods: Vec<u8> = data.umods.iter().copied().filter(|&m| m != 0).collect();
    for id in hook.0.overlays(class, data.flags, &umods) {
        super::state_overlays::create(w, inputs, key, i32::from(id), OVERLAY_KIND, [0; 3]);
    }
}
