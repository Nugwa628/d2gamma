//! The game's identity: the name players see, its version lines, the
//! window title, the folder under Documents that holds its saves, settings,
//! logs and cache, and its credits text.
//!
//! D2Gamma's exe never sets one: [`current`] is then D2Gamma's own
//! ([`GameIdentity::d2gamma`]), and every line, title and folder is what
//! the exe has always shown and used. A mod game's binary calls
//! [`install`] once at the start of `main`, before
//! [`crate::user_dir::init`] and before any window opens, so it brands
//! itself without forking the client (`docs/plans/mods/seams.md` E1).
//!
//! The identity is one value for the whole process: the client reads the
//! name and the folder in many places (the log, the crash dialog, every
//! window, the menus), long before a game is configured.

use std::sync::{Arc, OnceLock};

/// A game's credits text, as the credits screen reads its file (the bytes
/// of `Credits.txt`, or `ExpansionCredits.txt` when the argument is true):
/// `None` falls through to the text the game files carry.
pub type CreditsText = Arc<dyn Fn(bool) -> Option<Vec<u8>> + Send + Sync>;

/// Who the running game is ([`current`]).
#[derive(Clone)]
pub struct GameIdentity {
    /// The game's name, as players see it: the version line, `--version`,
    /// dialogs, the dev console and the crash log's first line.
    pub name: String,
    /// The game's version, shown after the name (the main menu's and the
    /// automap's version line, `--version`).
    pub version: String,
    /// The smaller line under the main menu's version line.
    pub data_version: String,
    /// The title of every window the game opens.
    pub window_title: String,
    /// The folder's name under Documents: the game's saves, settings,
    /// controls, logs and cache ([`crate::user_dir`]).
    pub folder: String,
    /// The game's own credits text, read before the game files' text.
    pub credits: Option<CreditsText>,
}

impl std::fmt::Debug for GameIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameIdentity")
            .field("name", &self.name)
            .field("version", &self.version)
            .field("data_version", &self.data_version)
            .field("window_title", &self.window_title)
            .field("folder", &self.folder)
            .field("credits", &self.credits.is_some())
            .finish()
    }
}

impl GameIdentity {
    /// D2Gamma's own identity: the exe's name, build version, title and
    /// `Documents\D2Gamma` folder, and the credits from the game files.
    pub fn d2gamma() -> GameIdentity {
        GameIdentity {
            name: crate::launch::NAME.into(),
            version: crate::launch::VERSION.into(),
            data_version: crate::ui::front_end::screens::main_menu::GAME_DATA_VERSION.into(),
            window_title: crate::launch::NAME.into(),
            folder: crate::user_dir::FOLDER.into(),
            credits: None,
        }
    }

    /// A mod game called `name` at `version`: its window titled `name`,
    /// its files in `Documents\<folder>`, no data version line and the
    /// credits from its game files.
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        folder: impl Into<String>,
    ) -> GameIdentity {
        let name = name.into();
        GameIdentity {
            window_title: name.clone(),
            name,
            version: version.into(),
            data_version: String::new(),
            folder: folder.into(),
            credits: None,
        }
    }

    /// The same identity with its own window title.
    pub fn with_window_title(mut self, title: impl Into<String>) -> GameIdentity {
        self.window_title = title.into();
        self
    }

    /// The same identity with the line under the version line.
    pub fn with_data_version(mut self, line: impl Into<String>) -> GameIdentity {
        self.data_version = line.into();
        self
    }

    /// The same identity with its own credits text.
    pub fn with_credits(mut self, credits: CreditsText) -> GameIdentity {
        self.credits = Some(credits);
        self
    }

    /// The name and version, as the version lines show them.
    pub fn version_line(&self) -> String {
        format!("{} {}", self.name, self.version)
    }

    /// Whether this is D2Gamma's own folder (the one old folders migrate
    /// into).
    pub fn is_d2gamma_folder(&self) -> bool {
        self.folder == crate::user_dir::FOLDER
    }
}

static IDENTITY: OnceLock<GameIdentity> = OnceLock::new();

/// Sets the game's identity for this process. Call once, first thing in a
/// mod game's `main`. A second call (or one after [`current`] was read)
/// changes nothing and hands the identity back.
pub fn install(identity: GameIdentity) -> Result<(), Box<GameIdentity>> {
    IDENTITY.set(identity).map_err(Box::new)
}

/// The game's identity: the installed one, else D2Gamma's.
pub fn current() -> &'static GameIdentity {
    IDENTITY.get_or_init(GameIdentity::d2gamma)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d2gammas_identity_is_the_exes_name_version_title_and_folder() {
        let id = GameIdentity::d2gamma();
        assert_eq!(id.name, "D2Gamma");
        assert_eq!(id.window_title, "D2Gamma");
        assert_eq!(id.folder, "D2Gamma");
        assert_eq!(id.version, crate::launch::VERSION);
        assert_eq!(id.data_version, "v 1.14d");
        assert!(id.credits.is_none());
        assert!(id.is_d2gamma_folder());
        assert_eq!(
            id.version_line(),
            format!("D2Gamma {}", crate::launch::VERSION)
        );
    }

    #[test]
    fn a_mod_game_names_its_title_folder_and_credits() {
        let id = GameIdentity::new("Example Mod", "2.0", "ExampleMod")
            .with_window_title("Example Mod (test)")
            .with_data_version("data 7")
            .with_credits(Arc::new(|expansion| {
                expansion.then(|| b"Team\r\n".to_vec())
            }));
        assert_eq!(id.version_line(), "Example Mod 2.0");
        assert_eq!(id.window_title, "Example Mod (test)");
        assert_eq!(id.data_version, "data 7");
        assert!(!id.is_d2gamma_folder());
        let credits = id.credits.as_ref().unwrap();
        assert_eq!(credits(true).as_deref(), Some(&b"Team\r\n"[..]));
        assert_eq!(credits(false), None);
        // A new mod identity titles its window with its name.
        assert_eq!(GameIdentity::new("M", "1", "M").window_title, "M");
    }

    #[test]
    fn without_an_install_the_game_is_d2gamma() {
        // The unit tests never install one.
        assert_eq!(current().name, "D2Gamma");
        assert!(install(GameIdentity::new("Late", "1", "Late")).is_err());
        assert_eq!(current().folder, "D2Gamma");
    }
}
