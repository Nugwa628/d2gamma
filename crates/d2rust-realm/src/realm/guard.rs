//! The save checks and the item registry on the save path (plan N6 and
//! N19; contracts 11.6 and 11.13).
//!
//! Every save a game server sends goes through [`SaveGuard::judge`] before
//! it is stored:
//!
//! - **The save checks** (`d2-save-checks`): the record against the stored
//!   one, with the realm's item, stat and experience tables. `save_checks
//!   = flag` (the default) runs every check as `flag`: a failure is logged
//!   and the save is stored. `standard` runs the checks in their standard
//!   modes (the table rules refuse), `off` runs none; `check.NAME = MODE`
//!   lines set one check's mode on top.
//! - **The item registry** (`d2-item-registry`): the realm database
//!   `data/realms/<mod>/realm.sqlite` holds every item ID and where it is.
//!   Item ID blocks for the games are granted from it ([`SaveGuard::grant`]),
//!   and each save's IDs are judged. With `item_registry = flag` (the
//!   default) an ID not granted to the game is adopted and logged, and a
//!   refused ID (held elsewhere, twice in the save) is logged and the save
//!   stored all the same; `refuse` refuses such a save (`SaveInvalid`);
//!   `off` only grants blocks. A character seen for the first time has its
//!   stored record's IDs registered first ([`d2_item_registry::Registry::adopt`]).
//!
//! The registry is written before the record is: a record store that
//! fails afterwards leaves the registry one save ahead, which the next
//! save of the same game finds as kept IDs. When a character leaves a
//! game its items leave the game ([`SaveGuard::left`]); when a game ends
//! its duplicates are found ([`SaveGuard::close_game`]). Game ids start
//! again at each start of the main server, so the games a former run left
//! open are closed when the guard opens.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use d2_accounts::rules::name_key;
use d2_data::bin;
use d2_data::fixup::{self, FixedSet};
use d2_data::strings::StringTables;
use d2_formats::d2s::{self, D2s};
use d2_formats::mpq::ArchiveSet;
use d2_item_registry::{save_ids, Config, Registry, SaveItems, Split, UnknownIds};
use d2_native::source::{NativeAsset, NativeSource};
use d2_net::msg::node::ItemIdBlock;
use d2_save_checks::{CheckList, CheckTables, ItemRegistry, Mode, SaveInfo};
use rusqlite::{Connection, OptionalExtension};

use crate::log::ServerLog;

/// How the save checks run (`save_checks`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChecksMode {
    /// None runs.
    Off,
    /// Every check runs as `flag`.
    #[default]
    Flag,
    /// The checks' standard modes ([`d2_save_checks::STANDARD`]).
    Standard,
}

impl ChecksMode {
    /// `off`, `flag` or `standard`.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "off" => Self::Off,
            "flag" => Self::Flag,
            "standard" => Self::Standard,
            _ => return None,
        })
    }
}

/// How the item registry judges saves (`item_registry`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RegistryMode {
    /// Blocks are granted; saves are not judged.
    Off,
    /// Unknown IDs are adopted, refusals logged; the save is stored.
    #[default]
    Flag,
    /// A save with a refused ID is refused.
    Refuse,
}

impl RegistryMode {
    /// `off`, `flag` or `refuse`.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "off" => Self::Off,
            "flag" => Self::Flag,
            "refuse" => Self::Refuse,
            _ => return None,
        })
    }
}

/// The guard's settings, from the main server's config.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuardConfig {
    pub checks: ChecksMode,
    /// `check.NAME = MODE` lines, in order.
    pub check_modes: Vec<(String, Mode)>,
    pub registry: RegistryMode,
}

impl GuardConfig {
    /// The tables are needed: a check runs, or saves are judged.
    pub fn needs_tables(&self) -> bool {
        self.checks != ChecksMode::Off || self.registry != RegistryMode::Off
    }
}

/// The realm database of mod `mod_id` in the data folder `data`.
pub fn realm_db(data: &Path, mod_id: &str) -> PathBuf {
    data.join("realms").join(mod_id).join("realm.sqlite")
}

/// The save checks' tables from the game data: a converted native folder
/// (it has `base/`, read without mods) or a game install's archives.
pub fn load_tables(dir: &Path) -> Result<CheckTables, String> {
    load_game_data(dir).map(|d| d.tables)
}

/// What the main server reads from the game data: the save checks'
/// tables, and the fixed-up tables with their strings (the market's item
/// names).
pub struct GameData {
    pub tables: CheckTables,
    pub fixed: FixedSet,
    pub strings: StringTables,
}

/// [`load_tables`] with the fixed-up tables and strings kept.
pub fn load_game_data(dir: &Path) -> Result<GameData, String> {
    let (fixed, strings) =
        fixed_set(dir).map_err(|e| format!("game data {}: {e}", dir.display()))?;
    let tables = CheckTables::from_fixed(&fixed).map_err(|e| format!("save check tables: {e}"))?;
    Ok(GameData {
        tables,
        fixed,
        strings,
    })
}

fn fixed_set(dir: &Path) -> Result<(FixedSet, StringTables), String> {
    if dir.join("base").is_dir() {
        let base_only: &[String] = &[];
        let src = NativeSource::open_with(dir, Some(base_only)).map_err(|e| e.to_string())?;
        let tables = src
            .tables(bin::DEFAULT_LANGUAGE)
            .map_err(|e| e.to_string())?;
        let data = bin::load_from(&tables, bin::DEFAULT_LANGUAGE).map_err(|e| e.to_string())?;
        let path = "data/global/animdata.d2";
        let anim = match src.read_native(path) {
            Some(Ok(NativeAsset::AnimData(a))) => a,
            Some(Ok(_)) => return Err(format!("{path}: not animation data")),
            Some(Err(e)) => return Err(format!("{path}: {e}")),
            None => return Err(format!("{path}: no native file")),
        };
        let fixed = fixup::apply(&data, &anim).map_err(|e| e.to_string())?;
        Ok((fixed, data.strings))
    } else {
        let set = ArchiveSet::open_dir(dir).map_err(|e| e.to_string())?;
        let data = bin::load(&set, bin::DEFAULT_LANGUAGE).map_err(|e| e.to_string())?;
        let anim = fixup::read_animdata(&set).map_err(|e| e.to_string())?;
        let fixed = fixup::apply(&data, &anim).map_err(|e| e.to_string())?;
        Ok((fixed, data.strings))
    }
}

/// What the guard says about one save.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Judgement {
    /// Why the save is refused (`SaveInvalid`); `None` stores it.
    pub refused: Option<String>,
    /// What was flagged: logged, and stored all the same.
    pub flagged: Vec<String>,
}

/// The save checks and the item registry of one main server.
pub struct SaveGuard {
    config: GuardConfig,
    tables: Option<Arc<CheckTables>>,
    checks: Option<CheckList>,
    registry: Registry,
    db: Mutex<Connection>,
    log: Arc<ServerLog>,
}

impl std::fmt::Debug for SaveGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SaveGuard")
            .field("config", &self.config)
            .field("checks", &self.checks)
            .field("registry", &self.registry)
            .finish()
    }
}

/// The item registry as the save checks' duplicate check asks it, for
/// one save.
struct Holders<'a> {
    registry: &'a Registry,
    conn: &'a Connection,
    game: u32,
    split: Split,
}

impl ItemRegistry for Holders<'_> {
    fn other_holder(&self, character: &[u8], id: u32) -> Option<String> {
        let key = name_key(&String::from_utf8_lossy(character));
        self.registry
            .other_holder(self.conn, self.game, &key, self.split, id)
            .ok()
            .flatten()
            .map(|w| w.to_string())
    }
}

/// The split of the economy a record's items belong to. Ladder seasons
/// don't run yet: every character is in season 0.
fn split_of(status: u16) -> Split {
    Split {
        hardcore: status & d2s::status::HARDCORE != 0,
        ladder: 0,
        expansion: status & d2s::status::EXPANSION != 0,
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// The characters whose stored record the registry has seen (the guard's
/// own table in the realm database).
const SEEN: &str = "CREATE TABLE IF NOT EXISTS realm_registry_seen(
    character TEXT PRIMARY KEY,
    at INTEGER NOT NULL
)";

impl SaveGuard {
    /// Opens the registry in `db` for mod `mod_id` and closes the games a
    /// former run left open. `tables` are needed when
    /// [`GuardConfig::needs_tables`].
    pub fn open(
        db: &Path,
        mod_id: &str,
        tables: Option<Arc<CheckTables>>,
        config: GuardConfig,
        log: Arc<ServerLog>,
    ) -> Result<Self, String> {
        if config.needs_tables() && tables.is_none() {
            return Err("the save checks and the item registry need the game tables".into());
        }
        if let Some(dir) = db.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let mut conn = d2_item_registry::open_file(db).map_err(|e| e.to_string())?;
        let registry = Registry::open(
            &conn,
            mod_id,
            Config {
                unknown: if config.registry == RegistryMode::Refuse {
                    UnknownIds::Refuse
                } else {
                    UnknownIds::Adopt
                },
                ..Config::default()
            },
        )
        .map_err(|e| e.to_string())?;
        conn.execute_batch(SEEN)
            .map_err(|e| format!("{}: {e}", db.display()))?;
        let checks = match (config.checks, &tables) {
            (ChecksMode::Off, _) | (_, None) => None,
            (mode, Some(t)) => {
                let mut list = CheckList::standard(t.clone());
                if mode == ChecksMode::Flag {
                    let names: Vec<String> =
                        list.modes().iter().map(|(n, _)| (*n).to_owned()).collect();
                    for n in names {
                        list.set_mode(&n, Mode::Flag).map_err(|e| e.to_string())?;
                    }
                }
                for (name, m) in &config.check_modes {
                    list.set_mode(name, *m).map_err(|e| e.to_string())?;
                }
                Some(list)
            }
        };
        let left = d2_item_registry::write(&mut conn, |c| {
            let games: Vec<u32> = {
                let mut q = c.prepare(
                    "SELECT game FROM item_blocks WHERE open = 1
                     UNION SELECT game FROM item_claims",
                )?;
                let rows = q.query_map([], |r| r.get(0))?;
                rows.collect::<Result<_, _>>()?
            };
            let mut dupes = Vec::new();
            for g in &games {
                dupes.extend(registry.close_game(c, *g, now_ms())?);
            }
            Ok((games.len(), dupes))
        })
        .map_err(|e| e.to_string())?;
        if left.0 > 0 {
            log.line(&format!(
                "item registry: closed {} games of a former run",
                left.0
            ));
        }
        for d in left.1 {
            log.line(&format!(
                "item registry: duplicate item ID {:#010x}: kept on {}, also on {}",
                d.id,
                d.kept,
                d.also.join(", ")
            ));
        }
        Ok(Self {
            config,
            tables,
            checks,
            registry,
            db: Mutex::new(conn),
            log,
        })
    }

    pub fn config(&self) -> &GuardConfig {
        &self.config
    }

    /// The item registry (the online stash moves stash items' IDs).
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// The realm's game tables, when loaded.
    pub fn tables(&self) -> Option<&Arc<CheckTables>> {
        self.tables.as_ref()
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A block of item IDs for `game` (`StartGame`, `ItemIds`).
    pub fn grant(&self, game: u32) -> Result<ItemIdBlock, String> {
        let mut conn = self.conn();
        let b = d2_item_registry::write(&mut conn, |c| self.registry.grant(c, game, now_ms()))
            .map_err(|e| e.to_string())?;
        Ok(ItemIdBlock {
            first: b.first,
            count: b.count,
        })
    }

    /// Judges `after`, the save of `character` from `game`, against its
    /// stored record `before` (`None`: never saved), and registers its
    /// item IDs when it may be stored. Blocking: the checks decode both
    /// records and the registry writes the realm database.
    pub fn judge(
        &self,
        game: u32,
        character: &str,
        before: Option<&[u8]>,
        after: &[u8],
    ) -> Judgement {
        let mut j = Judgement::default();
        let Some(tables) = &self.tables else {
            return j;
        };
        let key = name_key(character);
        let status = d2s::header_status(after).unwrap_or(0);
        let split = split_of(status);
        let mut conn = self.conn();
        if let Some(list) = &self.checks {
            let holders = Holders {
                registry: &self.registry,
                conn: &conn,
                game,
                split,
            };
            let v = list.run_records(
                before,
                after,
                // The game time between two saves and the game's difficulty
                // don't come with a save yet: the checks that read them
                // skip.
                SaveInfo {
                    played_ms: None,
                    difficulty: None,
                    registry: (self.config.registry != RegistryMode::Off)
                        .then_some(&holders as &dyn ItemRegistry),
                },
            );
            j.flagged
                .extend(v.flagged.iter().map(|f| format!("check {f}")));
            if let Some(f) = v.refused.first() {
                j.refused = Some(format!("check {f}"));
                return j;
            }
        }
        if self.config.registry == RegistryMode::Off {
            return j;
        }
        let ids_of = |bytes: &[u8]| -> Result<Vec<u32>, String> {
            let save: D2s = d2_save_checks::decode(tables, bytes).map_err(|e| e.to_string())?;
            let ids = save_ids(&save, &tables.items);
            match ids.errors.first() {
                Some(e) => Err(e.clone()),
                None => Ok(ids.ids),
            }
        };
        let after_ids = match ids_of(after) {
            Ok(ids) => ids,
            Err(e) => {
                let why = format!("item IDs: the save's items don't read: {e}");
                if self.config.registry == RegistryMode::Refuse {
                    j.refused = Some(why);
                } else {
                    j.flagged.push(why);
                }
                return j;
            }
        };
        let before_ids = before.map(ids_of);
        let now = now_ms();
        let refuse = self.config.registry == RegistryMode::Refuse;
        let done = d2_item_registry::write(&mut conn, |c| {
            let mut notes = Vec::new();
            let seen: Option<i64> = c
                .query_row(
                    "SELECT at FROM realm_registry_seen WHERE character = ?1",
                    [&key],
                    |r| r.get(0),
                )
                .optional()?;
            if seen.is_none() {
                match &before_ids {
                    Some(Ok(ids)) => {
                        let a = self.registry.adopt(
                            c,
                            &SaveItems {
                                game,
                                character: &key,
                                split,
                                ids,
                                now,
                            },
                        )?;
                        for (id, w) in a.dupes {
                            notes.push(format!(
                                "item ID {id:#010x} of the stored record is held by {w}"
                            ));
                        }
                    }
                    Some(Err(e)) => notes.push(format!(
                        "item IDs: the stored record's items don't read: {e}"
                    )),
                    None => {}
                }
                c.execute(
                    "INSERT INTO realm_registry_seen(character, at) VALUES (?1, ?2)",
                    rusqlite::params![key, now],
                )?;
            }
            let v = self.registry.accept_save(
                c,
                &SaveItems {
                    game,
                    character: &key,
                    split,
                    ids: &after_ids,
                    now,
                },
            )?;
            Ok((notes, v))
        });
        match done {
            Ok((notes, v)) => {
                j.flagged.extend(notes);
                if !v.adopted.is_empty() {
                    j.flagged.push(format!(
                        "{} item IDs not granted to game {game}, adopted",
                        v.adopted.len()
                    ));
                }
                if !v.accepted() {
                    let why: Vec<String> = v
                        .refused
                        .iter()
                        .map(|(id, r)| format!("item ID {id:#010x} {r}"))
                        .collect();
                    let why = why.join("; ");
                    if refuse {
                        j.refused = Some(why);
                    } else {
                        j.flagged.push(format!("{why} (not registered)"));
                    }
                }
            }
            Err(e) => {
                self.log.line(&format!("item registry: {e}"));
                if refuse {
                    j.refused = Some("the item registry failed".into());
                }
            }
        }
        j
    }

    /// Whether the registry knows item ID `id`, or `game` made it from
    /// its own blocks: an ID never granted is a game server's invention.
    /// True when the registry is off.
    pub fn knows(&self, game: u32, id: u32) -> Result<bool, String> {
        if self.config.registry == RegistryMode::Off {
            return Ok(true);
        }
        let conn = self.conn();
        let placed = self
            .registry
            .place_of(&conn, id)
            .map_err(|e| e.to_string())?;
        let granted = self
            .registry
            .blocks(&conn, game)
            .map_err(|e| e.to_string())?
            .iter()
            .any(|b| b.contains(id));
        Ok(placed.is_some() || granted)
    }

    /// `character`'s last save in `game` is in: its items leave the game.
    pub fn left(&self, game: u32, character: &str) {
        let r = self.registry.left(&self.conn(), game, &name_key(character));
        if let Err(e) = r {
            self.log.line(&format!("item registry: {e}"));
        }
    }

    /// `game` is over: its duplicates are found and logged, and its blocks
    /// close.
    pub fn close_game(&self, game: u32) {
        let mut conn = self.conn();
        match d2_item_registry::write(&mut conn, |c| self.registry.close_game(c, game, now_ms())) {
            Ok(dupes) => {
                for d in dupes {
                    self.log.line(&format!(
                        "item registry: game {game}: duplicate item ID {:#010x}: kept on {}, also on {}",
                        d.id,
                        d.kept,
                        d.also.join(", ")
                    ));
                }
            }
            Err(e) => self.log.line(&format!("item registry: {e}")),
        }
    }
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
