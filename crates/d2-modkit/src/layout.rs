//! The folder a mod is edited in.
//!
//! A mod being made is a folder of readable files, friendly to version
//! control, and it is exactly the layer folder the layer stack reads
//! (`d2_native::layers`): a manifest and a `files/` tree.
//!
//! ```text
//! my-mod/
//!   mod.toml                the manifest (required)
//!   files/
//!     rules.toml            the global rules file
//!     patches/              table patches (*.d2patch)
//!     tables/               the mod's own side tables
//!     <native paths>        art, sounds, maps, fonts at their native paths
//!   README.md  LICENSE      free text, never packed, ignored by the game
//! ```
//!
//! What goes under `files/` follows the native file layout and is checked by
//! the layer stack when the mod is opened (no copies of game tables, sprite
//! sidecars complete, ...); this module checks the part every tool shares:
//! the shape of the folder and that every path is safe on every platform.
//!
//! Anything starting with `.` (git folders, editor files) and the usual
//! operating-system leftovers are left out. Anything else at the top level
//! is an error, so a typo (`file/`) is caught instead of silently dropped.
//! Installed mods also hold the game's saves, cache, logs and settings;
//! those known runtime entries are ignored and never enter a package.
//! Links are refused: a mod must be the files themselves.
//!
//! Paths inside a mod always use `/`, are relative, and are limited to what
//! every platform can store (no `..`, no `\ : * ? " < > |`, no names Windows
//! reserves), so a pack made on one machine unpacks on any other. Two paths
//! that differ only by letter case are refused too.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::ModError;
use crate::manifest::{Manifest, MANIFEST_FILE};

/// The folder that holds everything the game reads.
pub const FILES_DIR: &str = "files";
/// The act registry, under [`FILES_DIR`].
pub const ACTS_FILE: &str = "files/acts.toml";
/// Difficulty file.
pub const DIFFICULTIES_FILE: &str = "files/difficulties.toml";
/// The global rules file.
pub const RULES_FILE: &str = "files/rules.toml";
/// The states and stacks file, under [`FILES_DIR`].
pub const STATES_FILE: &str = "files/states.toml";
/// The building-block skills file, under [`FILES_DIR`].
pub const SKILLS_FILE: &str = "files/skills.toml";
/// The triggers file, under [`FILES_DIR`].
pub const TRIGGERS_FILE: &str = "files/triggers.toml";
/// The declared crowd control file, under [`FILES_DIR`].
pub const CROWD_CONTROL_FILE: &str = "files/crowd_control.toml";
/// The monster behaviour graphs file, under [`FILES_DIR`].
pub const AI_FILE: &str = "files/ai.toml";
/// The quests file, under [`FILES_DIR`].
pub const QUESTS_FILE: &str = "files/quests.toml";
/// Top-level files that are kept with the mod but never packed.
pub const EXTRA_FILES: &[&str] = &["README.md", "LICENSE"];

const PLAYER_DIRS: &[&str] = &["saves", "cache", "logs"];
const PLAYER_FILES: &[&str] = &["settings.toml", "controls.toml"];

const MAX_PATH_BYTES: usize = 240;
const JUNK: &[&str] = &["Thumbs.db", "desktop.ini", "ehthumbs.db"];
const WINDOWS_RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Whether `path` is a valid path inside a mod; the error says why not.
pub fn check_rel_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("is empty".into());
    }
    if path.len() > MAX_PATH_BYTES {
        return Err(format!("is longer than {MAX_PATH_BYTES} bytes"));
    }
    if path.starts_with('/') {
        return Err("must be relative (no leading `/`)".into());
    }
    for part in path.split('/') {
        if part.is_empty() {
            return Err("has an empty folder name (`//`) or ends with `/`".into());
        }
        if part == "." || part == ".." {
            return Err(format!("must not contain `{part}`"));
        }
        if let Some(c) = part
            .chars()
            .find(|c| c.is_control() || matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        {
            return Err(format!(
                "`{part}` contains `{}`, which is not allowed",
                c.escape_default()
            ));
        }
        if part.ends_with('.') || part.ends_with(' ') || part.starts_with(' ') {
            return Err(format!(
                "`{part}` must not start with a space or end with a space or dot"
            ));
        }
        let stem = part.split('.').next().unwrap_or(part).to_ascii_lowercase();
        if WINDOWS_RESERVED.contains(&stem.as_str()) {
            return Err(format!("`{part}` is a name Windows reserves"));
        }
    }
    Ok(())
}

/// One file of a mod: its path inside the mod folder and where it is on
/// disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModFile {
    /// `files/...`, `/`-separated.
    pub path: String,
    pub disk: PathBuf,
}

/// A mod folder, read and checked.
#[derive(Debug, Clone)]
pub struct ModFolder {
    pub root: PathBuf,
    /// The manifest file exactly as written.
    pub manifest_text: String,
    pub manifest: Manifest,
    /// Every file under `files/`, sorted by path: what a pack holds besides
    /// the manifest.
    pub files: Vec<ModFile>,
}

impl ModFolder {
    /// Reads `dir` as a mod folder: the manifest must be a valid mod-kit
    /// manifest and every file must sit where the layout allows.
    pub fn scan(dir: &Path) -> Result<ModFolder, ModError> {
        let manifest_path = dir.join(MANIFEST_FILE);
        let manifest_text = match fs::read_to_string(&manifest_path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ModError::layout(
                    dir.display().to_string(),
                    format!("this folder has no {MANIFEST_FILE}, so it is not a mod folder"),
                ))
            }
            Err(e) => return Err(ModError::io(manifest_path.display(), e)),
        };
        let manifest = Manifest::parse(&manifest_text)?;

        let mut files = Vec::new();
        for entry in read_dir_sorted(dir)? {
            let name = entry.name;
            if skipped(&name) || name == MANIFEST_FILE {
                continue;
            }
            match entry.kind {
                Kind::Dir if name == FILES_DIR => walk(&entry.path, FILES_DIR, &mut files)?,
                Kind::Dir if PLAYER_DIRS.contains(&name.as_str()) => {}
                Kind::File if PLAYER_FILES.contains(&name.as_str()) => {}
                Kind::File if EXTRA_FILES.contains(&name.as_str()) => {}
                Kind::Link => return Err(link_error(&entry.path)),
                _ => {
                    return Err(ModError::layout(
                        entry.path.display().to_string(),
                        format!(
                            "`{name}` is not part of the mod layout (a mod folder holds {MANIFEST_FILE}, \
                             the folder {FILES_DIR}/ and, unpacked, {})",
                            EXTRA_FILES.join(", ")
                        ),
                    ))
                }
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));

        let mut seen = BTreeSet::new();
        for f in &files {
            check_rel_path(&f.path).map_err(|m| {
                ModError::layout(
                    f.disk.display().to_string(),
                    format!("path `{}` {m}", f.path),
                )
            })?;
            if !seen.insert(f.path.to_lowercase()) {
                return Err(ModError::layout(
                    f.disk.display().to_string(),
                    format!(
                        "`{}` differs from another file only by letter case; that breaks on Windows and macOS",
                        f.path
                    ),
                ));
            }
        }
        Ok(ModFolder {
            root: dir.to_path_buf(),
            manifest_text,
            manifest,
            files,
        })
    }
}

/// Starts a new mod folder: the manifest, a rules placeholder and the
/// usual `files/` subfolders (each with a `.gitkeep`). `dir` must not exist
/// yet or must be empty.
pub fn init_folder(dir: &Path, manifest: &Manifest) -> Result<(), ModError> {
    if dir.exists() {
        let mut it = fs::read_dir(dir).map_err(|e| ModError::io(dir.display(), e))?;
        if it.next().is_some() {
            return Err(ModError::layout(
                dir.display().to_string(),
                "the folder is not empty",
            ));
        }
    }
    let files = dir.join(FILES_DIR);
    fs::create_dir_all(&files).map_err(|e| ModError::io(files.display(), e))?;
    let put = |path: PathBuf, text: &str| {
        fs::write(&path, text).map_err(|e| ModError::io(path.display(), e))
    };
    put(dir.join(MANIFEST_FILE), &manifest.to_toml())?;
    put(
        dir.join(RULES_FILE),
        "# The mod's global rules. Empty means \"play like D2Gamma\".\n",
    )?;
    for sub in ["patches", "tables"] {
        let p = files.join(sub);
        fs::create_dir_all(&p).map_err(|e| ModError::io(p.display(), e))?;
        put(p.join(".gitkeep"), "")?;
    }
    Ok(())
}

fn skipped(name: &str) -> bool {
    name.starts_with('.') || JUNK.contains(&name)
}

fn link_error(path: &Path) -> ModError {
    ModError::layout(
        path.display().to_string(),
        "links are not allowed in a mod folder; put the real files here",
    )
}

enum Kind {
    File,
    Dir,
    Link,
}

struct Entry {
    name: String,
    path: PathBuf,
    kind: Kind,
}

fn read_dir_sorted(dir: &Path) -> Result<Vec<Entry>, ModError> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).map_err(|e| ModError::io(dir.display(), e))? {
        let e = e.map_err(|e| ModError::io(dir.display(), e))?;
        let path = e.path();
        let Some(name) = e.file_name().to_str().map(str::to_owned) else {
            return Err(ModError::layout(
                path.display().to_string(),
                "the file name is not valid text",
            ));
        };
        let ft = e.file_type().map_err(|e| ModError::io(path.display(), e))?;
        let kind = if ft.is_symlink() {
            Kind::Link
        } else if ft.is_dir() {
            Kind::Dir
        } else {
            Kind::File
        };
        out.push(Entry { name, path, kind });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<ModFile>) -> Result<(), ModError> {
    for entry in read_dir_sorted(dir)? {
        if skipped(&entry.name) {
            continue;
        }
        let rel = format!("{prefix}/{}", entry.name);
        match entry.kind {
            Kind::Dir => walk(&entry.path, &rel, out)?,
            Kind::File => out.push(ModFile {
                path: rel,
                disk: entry.path,
            }),
            Kind::Link => return Err(link_error(&entry.path)),
        }
    }
    Ok(())
}

/// Named stat definitions and deterministic behaviors.
pub const STATS_FILE: &str = "files/stats.toml";

pub const LEVELS_FILE: &str = "files/levels.toml";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{sample, temp, MANIFEST};

    #[test]
    fn the_proc_charges_template_is_a_valid_mod_folder() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/proc-charges");
        let f = ModFolder::scan(&dir).unwrap();
        assert_eq!(f.manifest.id, "proc-charges");
        let paths: Vec<&str> = f.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [SKILLS_FILE, STATES_FILE, TRIGGERS_FILE],
            "the three rule files and no game files"
        );
    }

    #[test]
    fn the_monster_ai_graph_template_is_a_valid_mod_folder() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/monster-ai-graph");
        let f = ModFolder::scan(&dir).unwrap();
        assert_eq!(f.manifest.id, "monster-ai-graph");
        let paths: Vec<&str> = f.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, [AI_FILE], "the ai file and no game files");
    }

    #[test]
    fn the_boss_interrupts_only_template_is_a_valid_mod_folder() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/boss-interrupts-only");
        let f = ModFolder::scan(&dir).unwrap();
        assert_eq!(f.manifest.id, "boss-interrupts-only");
        let paths: Vec<&str> = f.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [CROWD_CONTROL_FILE, RULES_FILE, SKILLS_FILE],
            "the three rule files and no game files"
        );
    }
    #[test]
    fn script_reaction_template_is_a_valid_mod_folder() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/script-reaction");
        let folder = ModFolder::scan(&dir).unwrap();
        assert_eq!(folder.manifest.id, "script-reaction");
        let paths: Vec<_> = folder.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            ["files/scripts/reaction.luau", "files/triggers.toml"]
        );
    }
    #[test]
    fn scans_a_folder() {
        let d = sample("scan");
        fs::create_dir_all(d.join(".git")).unwrap();
        fs::write(d.join(".git/config"), "x").unwrap();
        fs::write(d.join(".gitignore"), "x").unwrap();
        fs::write(d.join("README.md"), "about").unwrap();
        fs::write(d.join("files/art/Thumbs.db"), "x").unwrap();
        let f = ModFolder::scan(&d).unwrap();
        assert_eq!(f.manifest.id, "test-mod");
        let paths: Vec<&str> = f.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "files/art/pic.bin",
                "files/patches/items.d2patch",
                "files/rules.toml",
                "files/tables/extra.txt"
            ]
        );
    }

    #[test]
    fn playing_an_installed_folder_does_not_change_its_package() {
        let d = sample("played");
        let before = crate::MemoryPack::from_folder(&ModFolder::scan(&d).unwrap()).unwrap();
        for name in ["saves", "cache", "logs"] {
            fs::create_dir_all(d.join(name).join("nested")).unwrap();
            fs::write(d.join(name).join("nested/private.bin"), "player data").unwrap();
        }
        for name in ["settings.toml", "controls.toml"] {
            fs::write(d.join(name), "player settings").unwrap();
        }
        let after = crate::MemoryPack::from_folder(&ModFolder::scan(&d).unwrap()).unwrap();
        assert_eq!(after.files, before.files);
        assert_eq!(
            fs::read_to_string(d.join("saves/nested/private.bin")).unwrap(),
            "player data"
        );
    }

    #[test]
    fn runtime_names_require_their_actual_file_or_directory_kind() {
        for name in PLAYER_DIRS {
            let d = sample(&format!("runtime-file-{name}"));
            fs::write(d.join(name), "not a runtime directory").unwrap();
            assert!(ModFolder::scan(&d).is_err(), "{name}");
        }
        for name in PLAYER_FILES {
            let d = sample(&format!("runtime-dir-{name}"));
            fs::create_dir(d.join(name)).unwrap();
            assert!(ModFolder::scan(&d).is_err(), "{name}");
        }
    }

    #[test]
    #[cfg(unix)]
    fn runtime_names_do_not_allow_links() {
        let d = sample("runtime-link");
        let outside = temp("runtime-outside");
        std::os::unix::fs::symlink(&outside, d.join("logs")).unwrap();
        let error = ModFolder::scan(&d).unwrap_err().to_string();
        assert!(error.contains("links are not allowed"), "{error}");
    }

    #[test]
    fn needs_a_manifest() {
        let d = temp("nomanifest");
        let e = ModFolder::scan(&d).unwrap_err().to_string();
        assert!(e.contains("no mod.toml"), "{e}");
    }

    #[test]
    fn reports_a_bad_manifest_by_field() {
        let d = sample("badmanifest");
        fs::write(d.join("mod.toml"), MANIFEST.replace("0.1.0", "x")).unwrap();
        let e = ModFolder::scan(&d).unwrap_err().to_string();
        assert!(e.contains("`version`"), "{e}");
    }

    #[test]
    fn refuses_unknown_top_level_items() {
        let d = sample("unknown");
        fs::create_dir_all(d.join("file")).unwrap();
        let e = ModFolder::scan(&d).unwrap_err().to_string();
        assert!(e.contains("`file` is not part of the mod layout"), "{e}");
        fs::remove_dir_all(d.join("file")).unwrap();
        fs::write(d.join("notes.txt"), "x").unwrap();
        assert!(ModFolder::scan(&d).is_err());
    }

    #[test]
    fn refuses_case_twins() {
        let d = sample("case");
        // Both exist only on a case-sensitive file system.
        fs::write(d.join("files/art/PIC.bin"), "x").unwrap();
        if fs::read_dir(d.join("files/art")).unwrap().count() == 2 {
            let e = ModFolder::scan(&d).unwrap_err().to_string();
            assert!(e.contains("letter case"), "{e}");
        }
    }

    #[test]
    fn path_rules() {
        for ok in [
            "files/a.tsv",
            "files/art/x/y/z.png",
            "files/sounds/Music Box.wav",
        ] {
            assert!(check_rel_path(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "/abs",
            "a//b",
            "a/",
            "../x",
            "a/../b",
            "a\\b",
            "c:/x",
            "a/b?",
            "a/nul.txt",
            "a/COM1",
            "a/trail.",
            "a/ lead",
            "a/b\u{7}",
        ] {
            assert!(check_rel_path(bad).is_err(), "{bad:?} should fail");
        }
        assert!(check_rel_path(&"a/".repeat(130)).is_err());
    }

    #[test]
    fn init_makes_a_scannable_folder() {
        let d = temp("init");
        let dir = d.join("new-mod");
        let m = Manifest::parse(MANIFEST).unwrap();
        init_folder(&dir, &m).unwrap();
        let f = ModFolder::scan(&dir).unwrap();
        assert_eq!(f.manifest, m);
        // rules.toml; the .gitkeep files are skipped.
        let paths: Vec<&str> = f.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, [RULES_FILE]);
        assert!(dir.join("files/patches").is_dir() && dir.join("files/tables").is_dir());
        // Not over a folder that has things in it.
        assert!(init_folder(&dir, &m).is_err());
    }
}
