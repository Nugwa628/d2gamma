//! The server programs' shared base game data.
//!
//! A main server and a game server read Diablo II's own tables, strings
//! and maps from a converted native folder. Each machine makes that folder
//! itself, from the owner's own Diablo II: the folder of its MPQs is the
//! config's `diablo2` (or [`detect`] finds one), and [`ensure`] converts
//! what a server reads ([`d2_convert::kinds::server`], from
//! [`d2_convert::convert::SERVER_ARCHIVES`]) into
//! `<base home>/<format key>/`, by default `~/d2rust-server/base/`.
//!
//! - **One base for every server on the machine.** The folder is keyed by
//!   [`format_key`], which changes when the native format or a server
//!   kind's version does, so servers of one engine version share a folder
//!   and a newer version converts its own next to it.
//! - **Converted once, checked, then swapped in.** The conversion goes to
//!   `<key>.new`, is checked by the program's own check (a game server
//!   plays its probe game on it), and only then becomes `<key>`. An old or
//!   refused folder stays until the new one passed; a format no running
//!   server holds (`<key>.in-use`, a shared lock each server keeps) is
//!   removed after.
//! - **One conversion at a time.** Servers that start together wait on
//!   `<base home>/.lock`; the second finds the folder made.
//! - **The same bytes everywhere.** A realm compares its game data's hash
//!   with each game server's, and each machine converts its own copy, so
//!   the folder holds only what the conversion makes the same on every
//!   machine (its `report.txt` goes next to it).
//! - **A mod's layer.** A mod server's package ships only its own layer,
//!   `mods/<name>/` (or a pack file) in its `game_data` folder; [`resolve`]
//!   puts it over the shared base in a per-server view folder of links.
//!
//! [`resolve`] also keeps an older setup working: a `game_data` folder that
//! is a game install is read as before, and one that is a converted copy of
//! its own is used (with [`IMPORT_HINT`]) until Diablo II is imported.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use d2_convert::convert::SERVER_ARCHIVES;
use sha2::{Digest, Sha256};

pub mod view;

/// The server package's folder name in the home folder (`~/d2rust-server`).
pub const SERVER_HOME: &str = "d2rust-server";
/// The base folder's name in it.
pub const BASE_DIR: &str = "base";
/// Where `import-d2` puts the MPQs, in it.
pub const IMPORT_DIR: &str = "diablo2";
/// The lock file servers take while one of them converts.
pub const LOCK_FILE: &str = ".lock";

/// The one step that fixes a missing or old base, as the servers say it.
pub const IMPORT_HINT: &str = "import your own Diablo II once: `server-ctl import-d2 <Diablo II folder or zip>` \
     (on a Windows game server: `node-ctl.ps1 import-d2 <folder>`), or set `diablo2 = <folder>` in the config";

/// The key of the base folder this build reads: the native format version
/// and a short hash over the server kinds' versions and the archives they
/// come from. Another key means a folder this build would not read as is.
pub fn format_key() -> String {
    let kinds: Vec<(String, u32)> = d2_convert::kinds::server()
        .iter()
        .map(|k| (k.name().to_owned(), k.native_version()))
        .collect();
    key_of(
        d2_native::manifest::FORMAT_VERSION,
        &kinds,
        &SERVER_ARCHIVES,
    )
}

/// [`format_key`] for a given format, kinds and archives.
pub fn key_of(format: u32, kinds: &[(String, u32)], archives: &[&str]) -> String {
    let mut h = Sha256::new();
    h.update(format!("format {format}\n"));
    let mut kinds = kinds.to_vec();
    kinds.sort();
    for (k, v) in kinds {
        h.update(format!("kind {k} {v}\n"));
    }
    for a in archives {
        h.update(format!("archive {}\n", a.to_ascii_lowercase()));
    }
    let hex: String = h.finalize()[..6]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("f{format}-{hex}")
}

/// The user's home folder (`HOME`, else `USERPROFILE` on Windows).
pub fn user_home() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find(|p| !p.as_os_str().is_empty())
}

/// `~/d2rust-server/base`.
pub fn default_base_home() -> Option<PathBuf> {
    user_home().map(|h| h.join(SERVER_HOME).join(BASE_DIR))
}

/// `~/d2rust-server/diablo2`, where `import-d2` puts the MPQs.
pub fn import_dir() -> Option<PathBuf> {
    user_home().map(|h| h.join(SERVER_HOME).join(IMPORT_DIR))
}

/// The archives a server conversion needs that `dir` lacks (names matched
/// case-insensitively, as the converter matches them).
pub fn missing_archives(dir: &Path) -> Vec<&'static str> {
    let names: Vec<String> = fs::read_dir(dir)
        .map(|r| {
            r.filter_map(Result::ok)
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();
    SERVER_ARCHIVES
        .iter()
        .copied()
        .filter(|a| !names.iter().any(|n| n == a))
        .collect()
}

/// Whether `dir` holds a file named `name` (in any case).
fn holds_file(dir: &Path, name: &str) -> bool {
    fs::read_dir(dir).is_ok_and(|r| {
        r.filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
    })
}

/// Whether `dir` holds every archive a server conversion needs.
pub fn has_server_archives(dir: &Path) -> bool {
    dir.is_dir() && missing_archives(dir).is_empty()
}

/// Where a Diablo II install is looked for when the config names none, in
/// order: the imported copy, then the usual install places (on Windows
/// also the install path the Diablo II installer wrote to the registry).
pub fn candidates() -> Vec<PathBuf> {
    let mut c = Vec::new();
    c.extend(import_dir());
    let home = user_home();
    if cfg!(windows) {
        c.extend(registry_install_paths());
        for root in [
            "C:\\Program Files (x86)",
            "C:\\Program Files",
            "C:\\Games",
            "C:\\",
        ] {
            c.push(Path::new(root).join("Diablo II"));
        }
    } else {
        for p in [
            "/opt/diablo2",
            "/opt/Diablo II",
            "/usr/local/games/diablo2",
            "/usr/share/games/diablo2",
        ] {
            c.push(PathBuf::from(p));
        }
        if let Some(h) = &home {
            for rel in [
                "Diablo II",
                "Games/Diablo II",
                "games/diablo2",
                ".wine/drive_c/Program Files (x86)/Diablo II",
                ".wine/drive_c/Program Files/Diablo II",
                "Games/diablo-ii/drive_c/Program Files (x86)/Diablo II",
            ] {
                c.push(h.join(rel));
            }
        }
    }
    c
}

/// The first of `candidates` with every archive a server needs.
pub fn detect_in(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|d| has_server_archives(d)).cloned()
}

/// A Diablo II install found on this machine ([`candidates`]).
pub fn detect() -> Option<PathBuf> {
    detect_in(&candidates())
}

/// The `InstallPath` values the Diablo II installer writes (current user,
/// then machine, 32- and 64-bit views), read with Windows' own `reg`.
fn registry_install_paths() -> Vec<PathBuf> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let keys = [
        r"HKCU\Software\Blizzard Entertainment\Diablo II",
        r"HKLM\SOFTWARE\WOW6432Node\Blizzard Entertainment\Diablo II",
        r"HKLM\SOFTWARE\Blizzard Entertainment\Diablo II",
    ];
    keys.iter()
        .filter_map(|k| {
            let out = std::process::Command::new("reg")
                .args(["query", k, "/v", "InstallPath"])
                .output()
                .ok()?;
            parse_reg_value(&String::from_utf8_lossy(&out.stdout))
        })
        .collect()
}

/// The value of a `reg query ... /v NAME` answer:
/// `    InstallPath    REG_SZ    C:\Games\Diablo II`.
fn parse_reg_value(text: &str) -> Option<PathBuf> {
    text.lines().find_map(|l| {
        let (_, v) = l.split_once("REG_SZ")?;
        let v = v.trim();
        (!v.is_empty()).then(|| PathBuf::from(v))
    })
}

/// Why the base could not be made.
#[derive(Debug)]
pub enum BaseError {
    /// No Diablo II to convert from (`why`: what was wrong with the one
    /// named, or with the folder there was).
    NoDiablo2 { why: String },
    /// The folder or the lock could not be made.
    Io { what: String, source: io::Error },
    /// The conversion failed.
    Convert(String),
    /// The converted folder failed the program's check.
    Check(String),
}

impl std::fmt::Display for BaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BaseError::NoDiablo2 { why } => write!(f, "no game data: {why}; {IMPORT_HINT}"),
            BaseError::Io { what, source } => write!(f, "game data: {what}: {source}"),
            BaseError::Convert(e) => write!(f, "game data: converting Diablo II: {e}"),
            BaseError::Check(e) => {
                write!(
                    f,
                    "game data: the converted folder failed its check (it was not kept): {e}"
                )
            }
        }
    }
}

impl std::error::Error for BaseError {}

fn io_err(what: impl Into<String>) -> impl FnOnce(io::Error) -> BaseError {
    let what = what.into();
    move |source| BaseError::Io { what, source }
}

/// What [`ensure`] does with a folder; the programs pass [`Steps::real`]
/// with their own check, the tests their stand-ins.
pub struct Steps<'a> {
    /// Converts the Diablo II folder (first) into a new folder (second).
    pub convert: &'a dyn Fn(&Path, &Path) -> Result<(), String>,
    /// Whether this build reads a folder as it is (a cheap check: the
    /// manifest, its kinds and versions, the file list).
    pub loadable: &'a dyn Fn(&Path) -> Result<(), String>,
    /// The program's own check of a newly converted folder.
    pub check: &'a dyn Fn(&Path) -> Result<(), String>,
    /// Progress lines.
    pub say: &'a dyn Fn(&str),
}

impl<'a> Steps<'a> {
    /// The converter, the native reader, and the program's `check`.
    pub fn real(
        check: &'a dyn Fn(&Path) -> Result<(), String>,
        say: &'a dyn Fn(&str),
    ) -> Steps<'a> {
        Steps {
            convert: &convert_server,
            loadable: &loadable,
            check,
            say,
        }
    }
}

/// The server conversion of `diablo2` into `out`: the server kinds from the
/// server archives, on half the machine's cores (a live server may share
/// it).
pub fn convert_server(diablo2: &Path, out: &Path) -> Result<(), String> {
    let mut o = d2_convert::Options::new(diablo2, out);
    o.archives = Some(SERVER_ARCHIVES.iter().map(|a| (*a).to_owned()).collect());
    o.threads = std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).max(1));
    let s = d2_convert::convert(&o, &d2_convert::kinds::server()).map_err(|e| e.to_string())?;
    if let Some((path, check, detail)) = s.failures.first() {
        return Err(format!(
            "{} files failed, the first {path} ({check}: {detail})",
            s.failures.len()
        ));
    }
    Ok(())
}

/// Whether this build reads the converted folder `dir` (its base alone).
pub fn loadable(dir: &Path) -> Result<(), String> {
    let base_only: &[String] = &[];
    d2_native::source::NativeSource::open_with(dir, Some(base_only))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The shared base a server reads, held for as long as it runs.
#[derive(Debug)]
pub struct Base {
    /// `<base home>/<key>`.
    pub dir: PathBuf,
    pub key: String,
    /// How long the conversion took, when this start made it.
    pub converted: Option<Duration>,
    /// The shared lock on `<key>.in-use`: no other server removes the
    /// folder while it is held.
    _in_use: File,
}

/// The base folder's state, without changing anything (`--check` hints).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// `<key>` is there and was checked when it was made.
    Ready(PathBuf),
    /// Only folders of other formats are there (an older engine's).
    Stale(Vec<String>),
    /// Nothing converted yet.
    Missing,
}

/// The base folder's state in `home` for this build's key.
pub fn state(home: &Path) -> State {
    let key = format_key();
    if is_ready(home, &key) {
        return State::Ready(home.join(&key));
    }
    let others = formats_in(home);
    if others.is_empty() {
        State::Missing
    } else {
        State::Stale(others)
    }
}

fn ok_file(home: &Path, key: &str) -> PathBuf {
    home.join(format!("{key}.ok"))
}

fn in_use_file(home: &Path, key: &str) -> PathBuf {
    home.join(format!("{key}.in-use"))
}

fn is_ready(home: &Path, key: &str) -> bool {
    home.join(key).is_dir() && ok_file(home, key).is_file()
}

/// The finished format folders in `home` (no `.new`, `.old`, dot files).
fn formats_in(home: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(home)
        .map(|r| {
            r.filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.starts_with('.') && !n.contains('.'))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Takes `<home>/.lock`, waiting (and saying so) while another server
/// holds it. The OS drops it with the process, so a killed server never
/// leaves it taken.
pub fn lock(home: &Path, say: &dyn Fn(&str)) -> Result<File, BaseError> {
    let path = home.join(LOCK_FILE);
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(io_err(format!("opening {}", path.display())))?;
    match f.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            say("game data: another server is converting it; waiting");
            f.lock()
                .map_err(io_err(format!("locking {}", path.display())))?;
        }
        Err(TryLockError::Error(e)) => {
            return Err(io_err(format!("locking {}", path.display()))(e))
        }
    }
    Ok(f)
}

fn hold_in_use(home: &Path, key: &str) -> Result<File, BaseError> {
    let path = in_use_file(home, key);
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(io_err(format!("opening {}", path.display())))?;
    f.lock_shared()
        .map_err(io_err(format!("locking {}", path.display())))?;
    Ok(f)
}

/// The base of this build in `home` (made if needed: see the crate docs),
/// converted from `diablo2` when there is none yet or this build refuses
/// the one there.
pub fn ensure(home: &Path, diablo2: Option<&Path>, steps: &Steps) -> Result<Base, BaseError> {
    ensure_key(home, &format_key(), diablo2, steps)
}

/// [`ensure`] for a given key.
pub fn ensure_key(
    home: &Path,
    key: &str,
    diablo2: Option<&Path>,
    steps: &Steps,
) -> Result<Base, BaseError> {
    // Nothing to convert from and nothing made: leave the disk alone.
    if !is_ready(home, key) {
        source(diablo2)?;
    }
    fs::create_dir_all(home).map_err(io_err(format!("making {}", home.display())))?;
    let _lock = lock(home, steps.say)?;
    let dir = home.join(key);
    let refused = if is_ready(home, key) {
        (steps.loadable)(&dir).err()
    } else if dir.is_dir() {
        Some("it was not finished or not checked".to_owned())
    } else {
        Some(String::new())
    };
    let mut converted = None;
    if let Some(why) = refused {
        if !why.is_empty() {
            (steps.say)(&format!(
                "game data: {} is refused ({why}); converting it again",
                dir.display()
            ));
        }
        converted = Some(convert_into(home, key, diablo2, steps)?);
    }
    let in_use = hold_in_use(home, key)?;
    drop_unused(home, key, steps.say);
    Ok(Base {
        dir,
        key: key.to_owned(),
        converted,
        _in_use: in_use,
    })
}

/// The Diablo II folder to convert from, or why there is none.
fn source(diablo2: Option<&Path>) -> Result<PathBuf, BaseError> {
    match diablo2 {
        Some(d) if has_server_archives(d) => Ok(d.to_path_buf()),
        Some(d) if d.is_dir() => Err(BaseError::NoDiablo2 {
            why: format!("{} has no {}", d.display(), missing_archives(d).join(", ")),
        }),
        Some(d) => Err(BaseError::NoDiablo2 {
            why: format!("no Diablo II folder at {}", d.display()),
        }),
        None => Err(BaseError::NoDiablo2 {
            why: "no Diablo II found on this machine".into(),
        }),
    }
}

/// Converts into `<key>.new`, checks it, and swaps it in for `<key>`.
fn convert_into(
    home: &Path,
    key: &str,
    diablo2: Option<&Path>,
    steps: &Steps,
) -> Result<Duration, BaseError> {
    let src = source(diablo2)?;
    let dir = home.join(key);
    let new = home.join(format!("{key}.new"));
    if new.exists() {
        fs::remove_dir_all(&new).map_err(io_err(format!("removing {}", new.display())))?;
    }
    (steps.say)(&format!(
        "game data: converting {} into {} (once per engine format; about a minute at most)",
        src.display(),
        dir.display()
    ));
    let t = Instant::now();
    if let Err(e) = (steps.convert)(&src, &new) {
        let _ = fs::remove_dir_all(&new);
        return Err(BaseError::Convert(e));
    }
    // Only what every machine's conversion makes the same stays in it.
    let report = new.join("report.txt");
    if report.is_file() {
        let _ = fs::rename(&report, home.join(format!("{key}.report.txt")));
    }
    let _ = fs::remove_dir_all(new.join(".work"));
    if let Err(e) = (steps.loadable)(&new).and_then(|()| (steps.check)(&new)) {
        let _ = fs::remove_dir_all(&new);
        return Err(BaseError::Check(e));
    }
    let _ = fs::remove_file(ok_file(home, key));
    let old = home.join(format!("{key}.old"));
    if dir.exists() {
        if old.exists() {
            fs::remove_dir_all(&old).map_err(io_err(format!("removing {}", old.display())))?;
        }
        fs::rename(&dir, &old).map_err(io_err(format!("moving {} aside", dir.display())))?;
    }
    fs::rename(&new, &dir).map_err(io_err(format!("moving {} in", new.display())))?;
    fs::write(ok_file(home, key), format!("{key}\n"))
        .map_err(io_err(format!("writing {}", ok_file(home, key).display())))?;
    if old.exists() {
        let _ = fs::remove_dir_all(&old);
    }
    let took = t.elapsed();
    (steps.say)(&format!(
        "game data: converted and checked in {:.1} s",
        took.as_secs_f64()
    ));
    Ok(took)
}

/// Removes the folders of other formats that no running server holds
/// (whoever holds one keeps it; a later start removes it).
fn drop_unused(home: &Path, keep: &str, say: &dyn Fn(&str)) {
    for other in formats_in(home) {
        if other == keep {
            continue;
        }
        let Ok(f) = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(in_use_file(home, &other))
        else {
            continue;
        };
        if f.try_lock().is_err() {
            continue;
        }
        let _ = fs::remove_file(ok_file(home, &other));
        if fs::remove_dir_all(home.join(&other)).is_ok() {
            let _ = fs::remove_file(home.join(format!("{other}.report.txt")));
            say(&format!("game data: removed the old format {other}"));
        }
        drop(f);
        let _ = fs::remove_file(in_use_file(home, &other));
    }
}

/// What a server's config says about its game data.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    /// `game_data`: an install, a converted folder of its own (an older
    /// setup), or a mod's layer folder (`mods/`). It may not exist.
    pub game_data: &'a Path,
    /// `diablo2`, when set.
    pub diablo2: Option<&'a Path>,
    /// `base_data`, when set; else [`default_base_home`].
    pub base_home: Option<&'a Path>,
    /// The server's own folder for the view of a mod's layer over the base.
    pub view: &'a Path,
}

/// Where a server reads its game data from.
#[derive(Debug)]
pub struct Resolved {
    /// The folder the program loads (and hashes).
    pub root: PathBuf,
    /// One line for `--check` and the log.
    pub what: String,
    /// A line telling the owner what to do once, when there is something.
    pub hint: Option<String>,
    /// The shared base, held while the server runs.
    pub base: Option<Base>,
}

/// The game data a server reads ([`Request`]): `game_data` itself when it
/// is a game install, or a converted folder of its own while no Diablo II
/// is configured or imported; else the shared base, converted if needed,
/// with `game_data`'s `mods/` over it in a view.
pub fn resolve(req: &Request, steps: &Steps) -> Result<Resolved, BaseError> {
    let gd = req.game_data;
    let mut own_refused = None;
    if req.diablo2.is_none() {
        if holds_file(gd, "d2data.mpq") {
            return Ok(Resolved {
                root: gd.to_path_buf(),
                what: format!("{} (a game install, read as is)", gd.display()),
                hint: None,
                base: None,
            });
        }
        let imported = import_dir().is_some_and(|d| has_server_archives(&d));
        if gd.join("base").is_dir() && !imported {
            match (steps.loadable)(gd) {
                Ok(()) => {
                    return Ok(Resolved {
                        root: gd.to_path_buf(),
                        what: format!("{} (a converted copy of its own)", gd.display()),
                        hint: Some(format!(
                            "game data: {} is a copy made elsewhere; the next engine format will refuse it. {IMPORT_HINT}",
                            gd.display()
                        )),
                        base: None,
                    })
                }
                Err(e) => own_refused = Some(format!("{} is refused ({e})", gd.display())),
            }
        }
    }
    let diablo2 = req.diablo2.map(Path::to_path_buf).or_else(detect);
    let home = match req.base_home {
        Some(h) => h.to_path_buf(),
        None => default_base_home().ok_or_else(|| BaseError::NoDiablo2 {
            why: "no home folder (HOME) for the base game data; set `base_data`".into(),
        })?,
    };
    let own_refused = own_refused.or_else(|| {
        (!gd.exists()).then(|| format!("the game data folder {} doesn't exist", gd.display()))
    });
    let base = match ensure(&home, diablo2.as_deref(), steps) {
        Ok(b) => b,
        // Nothing to make the base from, and a folder of its own that this
        // build doesn't refuse: read it as before (as the older versions
        // did), and say how to set it up.
        Err(BaseError::NoDiablo2 { why })
            if req.diablo2.is_none() && own_refused.is_none() && gd.is_dir() =>
        {
            return Ok(Resolved {
                root: gd.to_path_buf(),
                what: format!("{} (read as is: {why})", gd.display()),
                hint: Some(format!(
                    "game data: {} holds no Diablo II data yet. {IMPORT_HINT}",
                    gd.display()
                )),
                base: None,
            });
        }
        Err(BaseError::NoDiablo2 { why }) => {
            return Err(BaseError::NoDiablo2 {
                why: match &own_refused {
                    Some(own) => format!("{own}, and {why}"),
                    None => why,
                },
            })
        }
        Err(e) => return Err(e),
    };
    let mods = gd.join("mods");
    let from = diablo2.map_or_else(String::new, |d| format!(", from {}", d.display()));
    if mods.is_dir() {
        let root = view::make(req.view, &base.key, &base.dir, &mods)
            .map_err(io_err(format!("making the view {}", req.view.display())))?;
        view::drop_stale(req.view, &formats_in(&home));
        Ok(Resolved {
            what: format!(
                "{} (the shared base {}{from}, with {})",
                root.display(),
                base.dir.display(),
                mods.display()
            ),
            root,
            hint: None,
            base: Some(base),
        })
    } else {
        Ok(Resolved {
            root: base.dir.clone(),
            what: format!("{} (the shared base{from})", base.dir.display()),
            hint: None,
            base: Some(base),
        })
    }
}

#[cfg(test)]
mod tests;
