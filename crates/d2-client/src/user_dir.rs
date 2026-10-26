//! The player's D2Gamma folder, `Documents\D2Gamma`: characters (`saves\`,
//! with the shared stash files beside them), `settings.toml`,
//! `controls.toml`, the logs (`logs\`) and the baked animations
//! (`cache\`). The game never writes into the Diablo II install.
//!
//! Earlier builds used other names for the same folder: `Documents\D2RUST`
//! (the game's name before D2Gamma) and, before that, `Documents\d2rs`;
//! `controls.toml` once lived in `%APPDATA%\D2RUST` or `%APPDATA%\d2rs`.
//! [`migrate`] moves the newest old folder to `Documents\D2Gamma` when no
//! D2Gamma folder exists yet: one rename, so players keep their characters,
//! stash and settings. When the rename fails (a file held open, a synced
//! Documents folder) the old folder is copied instead and left as it was;
//! only a complete copy takes the new name. An old `controls.toml` moves in
//! whenever the folder has none. A move that fails leaves the old folder
//! in use ([`resolve`]), and an existing file is never overwritten, so
//! nothing is ever lost or hidden.
//!
//! `D2RUST_HOME` names another folder to use instead (tests, portable
//! installs); no migration happens then.
//!
//! A mod game names its own folder ([`crate::identity`]): its files live in
//! `Documents\<folder>` and nothing is migrated into it.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// D2Gamma's folder name under Documents (a mod game's own is
/// [`crate::identity::GameIdentity::folder`]).
pub const FOLDER: &str = "D2Gamma";

/// The names earlier builds gave the folder, newest first: `D2RUST`, then
/// the test builds' `d2rs` (under Documents and under `%APPDATA%`).
pub const LEGACY: [&str; 2] = ["D2RUST", "d2rs"];

/// Where a copy is built before it takes [`FOLDER`]'s name (with the
/// process id after it, so starts at the same moment never share one), so
/// a copy cut short never looks like a finished folder.
const STAGING: &str = "D2Gamma.moving";

/// The variable that overrides the folder (the older `D2RS_HOME` is read
/// too, [`crate::launch::env_os`]).
pub const HOME_VAR: &str = "D2RUST_HOME";

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// What [`migrate`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Migration {
    /// Nothing to move (a D2Gamma folder exists, or there was no old one,
    /// or another start of the game moved it first).
    None,
    /// The old folder `from` was renamed to `Documents\D2Gamma`;
    /// `controls` says whether an old `%APPDATA%` `controls.toml` came
    /// along too.
    Moved { from: PathBuf, controls: bool },
    /// The rename failed, so `from` was copied to `Documents\D2Gamma` and
    /// kept as it was.
    Copied { from: PathBuf, controls: bool },
    /// Only an old `controls.toml` was moved in.
    Controls,
    /// The rename and the copy failed; the old folder stays in use.
    Failed(String),
}

/// The user's Documents folder: `%USERPROFILE%\Documents` on Windows,
/// `$HOME/Documents` elsewhere.
pub fn documents() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .filter(|h| !h.is_empty())
        .map(|h| PathBuf::from(h).join("Documents"))
}

/// The folders earlier builds kept `controls.toml` in, newest first:
/// `%APPDATA%\D2RUST`, then `%APPDATA%\d2rs` (on Windows; the platform
/// config folder elsewhere).
pub fn legacy_config_dirs() -> Vec<PathBuf> {
    let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let base = if cfg!(windows) {
        var("APPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Application Support"))
    } else {
        var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|h| h.join(".config")))
    };
    base.map(|b| LEGACY.iter().map(|l| b.join(l)).collect())
        .unwrap_or_default()
}

/// The newest old folder under `documents`, when one exists.
fn legacy_folder(documents: &Path) -> Option<PathBuf> {
    LEGACY
        .iter()
        .map(|l| documents.join(l))
        .find(|d| d.is_dir())
}

/// Moves the old files into `documents\D2Gamma`: the newest old folder is
/// renamed (else copied, keeping the old one) when the D2Gamma folder does
/// not exist yet, then the first `controls.toml` found in `legacy_config`
/// is moved in unless the folder already has one.
pub fn migrate(documents: &Path, legacy_config: &[PathBuf]) -> Migration {
    migrate_with(documents, legacy_config, |from, to| {
        std::fs::rename(from, to)
    })
}

/// [`migrate`] with the folder rename given (tests make it fail).
fn migrate_with(
    documents: &Path,
    legacy_config: &[PathBuf],
    rename: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> Migration {
    let new = documents.join(FOLDER);
    let staging = documents.join(format!("{STAGING}-{}", std::process::id()));
    let mut moved = None;
    if !new.is_dir() {
        if let Some(old) = legacy_folder(documents) {
            moved = match rename(&old, &new) {
                Ok(()) => Some((old, true)),
                // Another start (the launcher, a second game) moved it first.
                Err(_) if new.is_dir() => None,
                Err(rename) => match copy_into_place(&old, &staging, &new) {
                    Ok(()) => Some((old, false)),
                    Err(_) if new.is_dir() => None,
                    Err(copy) => {
                        return Migration::Failed(format!(
                            "moving {} to {}: {rename}; copying it: {copy}",
                            old.display(),
                            new.display()
                        ))
                    }
                },
            };
        }
    }
    let controls = legacy_config
        .iter()
        .map(|d| d.join("controls.toml"))
        .find(|c| c.is_file())
        .is_some_and(|c| {
            let to = new.join("controls.toml");
            !to.exists() && std::fs::create_dir_all(&new).is_ok() && move_file(&c, &to)
        });
    match (moved, controls) {
        (Some((from, true)), controls) => Migration::Moved { from, controls },
        (Some((from, false)), controls) => Migration::Copied { from, controls },
        (None, true) => Migration::Controls,
        (None, false) => Migration::None,
    }
}

/// Copies `from` to `staging`, then renames `staging` to `to`. A copy cut
/// short is removed (only the copy: `from` is never touched).
fn copy_into_place(from: &Path, staging: &Path, to: &Path) -> std::io::Result<()> {
    // A copy an earlier start of the same process id left unfinished.
    if staging.exists() {
        std::fs::remove_dir_all(staging)?;
    }
    let done = copy_dir(from, staging).and_then(|()| std::fs::rename(staging, to));
    if done.is_err() {
        let _ = std::fs::remove_dir_all(staging);
    }
    done
}

/// Copies the folder `from` to `to` (made), with everything under it.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

/// The folder to use after [`migrate`]: `documents\D2Gamma`, unless it does
/// not exist while an old folder does (a move that failed).
pub fn resolve(documents: &Path) -> PathBuf {
    let new = documents.join(FOLDER);
    if new.is_dir() {
        return new;
    }
    legacy_folder(documents).unwrap_or(new)
}

/// `path` with an old folder's prefix replaced by `root`'s when `root` is
/// `documents\D2Gamma`: a path into `Documents\D2RUST` that a launcher or a
/// shortcut still passes finds what moved. Other paths stay as they are.
pub fn follow_move_in(documents: &Path, root: &Path, path: &Path) -> PathBuf {
    if root != documents.join(FOLDER) {
        return path.to_path_buf();
    }
    LEGACY
        .iter()
        .find_map(|l| path.strip_prefix(documents.join(l)).ok())
        .map_or_else(|| path.to_path_buf(), |rest| root.join(rest))
}

/// [`follow_move_in`] for this run's folder ([`root`]).
pub fn follow_move(path: &Path) -> PathBuf {
    match documents() {
        Some(docs) => follow_move_in(&docs, &root(), path),
        None => path.to_path_buf(),
    }
}

/// Runs the one-time move and fixes the folder for this run. Call once at
/// start, before anything reads or writes a user file. Returns the folder
/// and what the move did.
pub fn init() -> (PathBuf, Migration) {
    if let Some(home) = crate::launch::env_os(HOME_VAR).filter(|h| !h.is_empty()) {
        let root = PathBuf::from(home);
        let _ = ROOT.set(root.clone());
        return (root, Migration::None);
    }
    let id = crate::identity::current();
    let (root, m) = match documents() {
        // A mod game's own folder: nothing old to move in.
        Some(docs) if !id.is_d2gamma_folder() => (docs.join(&id.folder), Migration::None),
        Some(docs) => {
            let m = migrate(&docs, &legacy_config_dirs());
            (resolve(&docs), m)
        }
        None => (PathBuf::from(&id.folder), Migration::None),
    };
    let _ = ROOT.set(root.clone());
    (root, m)
}

/// The game's folder ([`init`]'s answer; resolved without moving anything
/// when `init` has not run, as in tests and tools).
pub fn root() -> PathBuf {
    ROOT.get().cloned().unwrap_or_else(|| {
        let id = crate::identity::current();
        crate::launch::env_os(HOME_VAR)
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                documents().map(|d| match id.is_d2gamma_folder() {
                    true => resolve(&d),
                    false => d.join(&id.folder),
                })
            })
            .unwrap_or_else(|| PathBuf::from(&id.folder))
    })
}

/// Where characters are saved.
pub fn saves() -> PathBuf {
    root().join("saves")
}

/// Where the logs go.
pub fn logs() -> PathBuf {
    root().join("logs")
}

/// `controls.toml`, next to `settings.toml`.
pub fn controls_file() -> PathBuf {
    root().join("controls.toml")
}

/// Moves a file: a rename, else copy and delete (another drive).
fn move_file(from: &Path, to: &Path) -> bool {
    std::fs::rename(from, to).is_ok()
        || (std::fs::copy(from, to).is_ok() && std::fs::remove_file(from).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("d2rust-user-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A D2RUST folder as a player has it: a character, the shared stash
    /// beside it, settings, a log and a finished animation bake.
    fn d2rust_folder(docs: &Path) {
        let old = docs.join("D2RUST");
        for d in ["saves", "logs", "cache/animation/baked"] {
            std::fs::create_dir_all(old.join(d)).unwrap();
        }
        std::fs::write(old.join("saves/Amy.d2s"), b"character").unwrap();
        std::fs::write(old.join("saves/shared-stash-sc.sqlite"), b"stash").unwrap();
        std::fs::write(old.join("settings.toml"), b"version = 1\n").unwrap();
        std::fs::write(old.join("logs/game.log"), b"log").unwrap();
        std::fs::write(old.join("cache/animation/baked/index.toml"), b"bake").unwrap();
    }

    /// Whether `root` holds everything [`d2rust_folder`] wrote.
    fn holds_the_player_files(root: &Path) -> bool {
        [
            ("saves/Amy.d2s", &b"character"[..]),
            ("saves/shared-stash-sc.sqlite", b"stash"),
            ("settings.toml", b"version = 1\n"),
            ("logs/game.log", b"log"),
            ("cache/animation/baked/index.toml", b"bake"),
        ]
        .iter()
        .all(|(f, bytes)| std::fs::read(root.join(f)).ok().as_deref() == Some(*bytes))
    }

    #[test]
    fn the_d2rust_folder_moves_once_with_saves_stash_and_bake() {
        let docs = scratch("move");
        d2rust_folder(&docs);
        assert_eq!(
            migrate(&docs, &[]),
            Migration::Moved {
                from: docs.join("D2RUST"),
                controls: false
            }
        );
        assert!(holds_the_player_files(&docs.join("D2Gamma")));
        assert!(!docs.join("D2RUST").exists());
        assert_eq!(resolve(&docs), docs.join("D2Gamma"));
        // Second start: nothing left to do.
        assert_eq!(migrate(&docs, &[]), Migration::None);
        let _ = std::fs::remove_dir_all(docs);
    }

    #[test]
    fn the_test_builds_folder_moves_when_no_d2rust_one_exists() {
        let docs = scratch("move-d2rs");
        std::fs::create_dir_all(docs.join("d2rs/saves")).unwrap();
        std::fs::write(docs.join("d2rs/saves/Amy.d2s"), b"x").unwrap();
        assert_eq!(
            migrate(&docs, &[]),
            Migration::Moved {
                from: docs.join("d2rs"),
                controls: false
            }
        );
        assert!(docs.join("D2Gamma/saves/Amy.d2s").is_file());
        assert!(!docs.join("d2rs").exists());
        let _ = std::fs::remove_dir_all(docs);
    }

    #[test]
    fn the_newest_old_folder_wins_and_the_older_one_stays() {
        let docs = scratch("newest");
        d2rust_folder(&docs);
        std::fs::create_dir_all(docs.join("d2rs/saves")).unwrap();
        std::fs::write(docs.join("d2rs/saves/Old.d2s"), b"x").unwrap();
        assert!(
            matches!(migrate(&docs, &[]), Migration::Moved { from, .. } if from == docs.join("D2RUST"))
        );
        assert!(holds_the_player_files(&docs.join("D2Gamma")));
        assert!(docs.join("d2rs/saves/Old.d2s").is_file());
        assert_eq!(resolve(&docs), docs.join("D2Gamma"));
        let _ = std::fs::remove_dir_all(docs);
    }

    #[test]
    fn a_failed_rename_copies_and_keeps_the_old_folder() {
        let docs = scratch("copy");
        d2rust_folder(&docs);
        let (old, new) = (docs.join("D2RUST"), docs.join("D2Gamma"));
        // An unfinished copy from an earlier start is replaced.
        let staging = docs.join(format!("{STAGING}-{}", std::process::id()));
        std::fs::create_dir_all(staging.join("saves")).unwrap();
        std::fs::write(staging.join("saves/half.d2s"), b"").unwrap();
        let held_open = |_: &Path, _: &Path| Err(std::io::Error::other("held open"));
        assert_eq!(
            migrate_with(&docs, &[], held_open),
            Migration::Copied {
                from: old.clone(),
                controls: false
            }
        );
        assert!(holds_the_player_files(&new));
        assert!(!new.join("saves/half.d2s").exists());
        assert!(holds_the_player_files(&old), "the old folder is kept");
        assert!(!staging.exists());
        assert_eq!(resolve(&docs), new);
        let _ = std::fs::remove_dir_all(docs);
    }

    #[test]
    fn a_start_that_loses_the_race_uses_the_moved_folder() {
        // Two starts at once: the other one renames the folder between
        // this start's look and its own rename.
        let docs = scratch("race");
        d2rust_folder(&docs);
        let (old, new) = (docs.join("D2RUST"), docs.join("D2Gamma"));
        let other_start_first = |from: &Path, to: &Path| {
            std::fs::rename(from, to).unwrap();
            std::fs::rename(from, to)
        };
        assert_eq!(migrate_with(&docs, &[], other_start_first), Migration::None);
        assert!(holds_the_player_files(&new));
        assert!(!old.exists());
        assert_eq!(resolve(&docs), new);
        let _ = std::fs::remove_dir_all(docs);
    }

    #[test]
    fn a_copy_that_cannot_finish_leaves_no_d2gamma_folder() {
        let docs = scratch("copy-fail");
        d2rust_folder(&docs);
        let (old, new) = (docs.join("D2RUST"), docs.join("D2Gamma"));
        // The copy cannot take the name: a file already holds it.
        std::fs::write(&new, b"in the way").unwrap();
        assert!(copy_into_place(&old, &docs.join(STAGING), &new).is_err());
        assert!(!docs.join(STAGING).exists());
        assert!(holds_the_player_files(&old));
        // The old folder stays in use.
        assert_eq!(resolve(&docs), old);
        assert!(matches!(migrate(&docs, &[]), Migration::Failed(_)));
        assert!(holds_the_player_files(&old));
        assert_eq!(resolve(&docs), old);
        let _ = std::fs::remove_dir_all(docs);
    }

    #[test]
    fn an_existing_d2gamma_folder_is_never_overwritten() {
        let docs = scratch("keep");
        std::fs::create_dir_all(docs.join("D2Gamma/saves")).unwrap();
        d2rust_folder(&docs);
        assert_eq!(migrate(&docs, &[]), Migration::None);
        assert!(holds_the_player_files(&docs.join("D2RUST")));
        assert!(!docs.join("D2Gamma/saves/Amy.d2s").exists());
        assert_eq!(resolve(&docs), docs.join("D2Gamma"));
        let _ = std::fs::remove_dir_all(docs);
    }

    #[test]
    fn the_old_controls_file_comes_along() {
        let docs = scratch("ctl-docs");
        let cfg = scratch("ctl-cfg");
        std::fs::create_dir_all(docs.join("D2RUST")).unwrap();
        std::fs::write(cfg.join("controls.toml"), b"version = 1\n").unwrap();
        assert_eq!(
            migrate(&docs, std::slice::from_ref(&cfg)),
            Migration::Moved {
                from: docs.join("D2RUST"),
                controls: true
            }
        );
        assert!(docs.join("D2Gamma/controls.toml").is_file());
        assert!(!cfg.join("controls.toml").exists());
        // Without an old Documents folder, the controls file alone moves.
        let docs2 = scratch("ctl-docs2");
        std::fs::write(cfg.join("controls.toml"), b"version = 1\n").unwrap();
        assert_eq!(
            migrate(&docs2, std::slice::from_ref(&cfg)),
            Migration::Controls
        );
        assert!(docs2.join("D2Gamma/controls.toml").is_file());
        for d in [docs, cfg, docs2] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn the_newer_controls_file_joins_an_existing_folder() {
        // Documents\D2Gamma exists (saves) and controls.toml sits in the
        // newer %APPDATA%\D2RUST; the older %APPDATA%\d2rs one stays.
        let docs = scratch("ho-docs");
        let newer = scratch("ho-newer");
        let older = scratch("ho-older");
        std::fs::create_dir_all(docs.join("D2Gamma/saves")).unwrap();
        std::fs::write(newer.join("controls.toml"), b"version = 1\n# new\n").unwrap();
        std::fs::write(older.join("controls.toml"), b"version = 1\n# old\n").unwrap();
        let dirs = [newer.clone(), older.clone()];
        assert_eq!(migrate(&docs, &dirs), Migration::Controls);
        assert_eq!(
            std::fs::read(docs.join("D2Gamma/controls.toml")).unwrap(),
            b"version = 1\n# new\n"
        );
        assert!(!newer.join("controls.toml").exists());
        // The folder has one now: the older file is never moved over it.
        assert_eq!(migrate(&docs, &dirs), Migration::None);
        assert!(older.join("controls.toml").is_file());
        for d in [docs, newer, older] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn a_fresh_install_uses_the_d2gamma_folder() {
        let docs = scratch("fresh");
        assert_eq!(migrate(&docs, &[]), Migration::None);
        assert_eq!(resolve(&docs), docs.join("D2Gamma"));
        // A failed move (simulated: only an old folder exists) keeps it.
        std::fs::create_dir_all(docs.join("d2rs")).unwrap();
        assert_eq!(resolve(&docs), docs.join("d2rs"));
        std::fs::create_dir_all(docs.join("D2RUST")).unwrap();
        assert_eq!(resolve(&docs), docs.join("D2RUST"));
        let _ = std::fs::remove_dir_all(docs);
    }

    #[test]
    fn paths_into_the_old_folder_follow_the_move() {
        let docs = Path::new("/h/Documents");
        let root = docs.join("D2Gamma");
        let mod_saves = docs.join("D2RUST").join("mods").join("m1").join("saves");
        assert_eq!(
            follow_move_in(docs, &root, &mod_saves),
            root.join("mods").join("m1").join("saves")
        );
        assert_eq!(
            follow_move_in(docs, &root, &docs.join("d2rs").join("saves")),
            root.join("saves")
        );
        // Elsewhere, or while the old folder is still in use: unchanged.
        let other = Path::new("/elsewhere/saves");
        assert_eq!(follow_move_in(docs, &root, other), other);
        assert_eq!(
            follow_move_in(docs, &docs.join("D2RUST"), &mod_saves),
            mod_saves
        );
        assert_eq!(
            follow_move_in(docs, Path::new("/portable"), &mod_saves),
            mod_saves
        );
    }
}
