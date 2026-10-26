//! The character folder the tool's rollbacks read and write, and the replay
//! engine of this build.

use std::path::PathBuf;

use d2_accounts::characters::CharacterStore as Store;
use d2_audit::{CharacterStore, ReplayEngine};

/// `data/characters/<account>/<name>.d2rc`: a character is found in
/// whichever account folder holds it.
pub struct Folder {
    root: PathBuf,
}

impl Folder {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn holder(&self, character: &str) -> Result<Store, String> {
        let entries =
            std::fs::read_dir(&self.root).map_err(|e| format!("{}: {e}", self.root.display()))?;
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if !path.is_dir() {
                continue;
            }
            let store = Store::open(&path).map_err(|e| e.to_string())?;
            let names = store.list().map_err(|e| e.to_string())?;
            if names.iter().any(|n| n.eq_ignore_ascii_case(character)) {
                return Ok(store);
            }
        }
        Err(format!("no character named {character}"))
    }
}

impl CharacterStore for Folder {
    fn load(&self, character: &str) -> Result<Vec<u8>, String> {
        self.holder(character)?
            .load(character)
            .map_err(|e| e.to_string())
    }

    fn store(&self, character: &str, record: &[u8]) -> Result<(), String> {
        self.holder(character)?
            .save(character, record)
            .map_err(|e| e.to_string())
    }
}

/// The replay engine of this build: none until the journal replay (N17)
/// lands, so `run` says so and leaves the queue as it is.
pub fn engine() -> Option<Box<dyn ReplayEngine>> {
    None
}
