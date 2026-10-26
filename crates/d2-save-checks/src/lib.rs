//! Save checks of the main server (plan `multiplayer-v2.md` C1 and N6,
//! contracts section 11.6).
//!
//! Every save a game server sends is decoded with the `d2s` codec and
//! checked against the realm's game rules before the main server stores it.
//! A check compares the stored record (`before`) with the new one (`after`):
//! it catches impossible characters (an item the game can't make, a level
//! the experience doesn't reach, a quest done out of order), not
//! possible-but-unearned ones, which are the replay audit's job.
//!
//! The parts:
//! - [`SaveCheck`]: one rule. [`CheckFail`] says why a save breaks it.
//! - [`CheckList`]: the realm's list of named checks, each with its
//!   [`Mode`] (`refuse`, `flag` or `off`). A mod replaces, adds or removes
//!   checks by name; [`CheckList::configure`] reads `check.NAME = MODE`
//!   lines. [`CheckList::run`] answers a [`Verdict`].
//! - [`CheckTables`]: the realm's item, stat and experience tables (no
//!   maps), from the fixed-up table set.
//! - [`SaveCtx`]: what a check sees besides the two saves: the tables, the
//!   save's [`SaveInfo`] and both saves' decoded items ([`Decoded`]).
//! - The standard checks: [`character`] (name and class, hardcore and
//!   status bits, level and experience, experience per minute, stat and
//!   skill points, gold), [`progress`] (difficulty, quests and waypoints
//!   only forward and in order) and [`items`] (item legality, affix and
//!   property ranges, duplicate item IDs with the registry hook, item IDs
//!   kept from the stored record).
//!
//! The rules reuse `d2-sim`'s item and stat code and `d2-formats`' save
//! codec, so there is one rule set; a mod's tables and rules come in
//! through [`CheckTables`]. The library has no I/O and builds on every
//! platform the main server and the game servers run on.

pub mod character;
mod decoded;
pub mod items;
pub mod progress;
mod tables;
#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::Arc;

use d2_formats::d2s::{self, D2s, D2sError, ReadOptions};

pub use decoded::{Decoded, FoundItem, Place};
pub use items::ItemRegistry;
pub use tables::{CheckTables, TablesError};

/// One rule of the save checks (contracts 11.1).
pub trait SaveCheck {
    /// Checks the new save `after` against the stored one `before`.
    fn check(&self, before: &D2s, after: &D2s, ctx: &SaveCtx) -> Result<(), CheckFail>;
}

/// Why a save breaks a check.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct CheckFail {
    /// What is wrong, for the log and the audit queue.
    pub reason: String,
}

impl CheckFail {
    /// A failure with this reason.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

/// How a check's failure is answered (plan C1, open question 14).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The save is refused (`SaveInvalid`), the stored record is kept and an
    /// audit of the game is queued.
    Refuse,
    /// The save is stored; the failure only queues an audit. New checks
    /// start here while they bed in.
    Flag,
    /// The check does not run.
    Off,
}

impl Mode {
    /// `refuse`, `flag` or `off`.
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "refuse" => Some(Mode::Refuse),
            "flag" => Some(Mode::Flag),
            "off" => Some(Mode::Off),
            _ => None,
        }
    }

    /// The config word of the mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Refuse => "refuse",
            Mode::Flag => "flag",
            Mode::Off => "off",
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the main server knows about a save beyond its bytes.
#[derive(Clone, Copy, Default)]
pub struct SaveInfo<'a> {
    /// Milliseconds the character played since the stored record (the game
    /// time between the two saves); `None` when unknown. The experience
    /// rate check needs it.
    pub played_ms: Option<u64>,
    /// The difficulty of the game the save comes from (0 Normal, 1
    /// Nightmare, 2 Hell); `None` when unknown.
    pub difficulty: Option<u8>,
    /// The realm's item registry (N19); `None` until it exists.
    pub registry: Option<&'a dyn ItemRegistry>,
}

impl fmt::Debug for SaveInfo<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SaveInfo")
            .field("played_ms", &self.played_ms)
            .field("difficulty", &self.difficulty)
            .field("registry", &self.registry.is_some())
            .finish()
    }
}

/// What a check sees besides the two saves.
pub struct SaveCtx<'a> {
    /// The realm's tables.
    pub tables: &'a CheckTables,
    /// The save's facts.
    pub info: SaveInfo<'a>,
    /// The stored record's items, decoded.
    pub before: Decoded,
    /// The new save's items, decoded.
    pub after: Decoded,
}

impl<'a> SaveCtx<'a> {
    /// Decodes both saves' items once for every check.
    pub fn new(tables: &'a CheckTables, before: &D2s, after: &D2s, info: SaveInfo<'a>) -> Self {
        Self {
            tables,
            info,
            before: Decoded::of(before, tables),
            after: Decoded::of(after, tables),
        }
    }
}

/// One failed check of a [`Verdict`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// The check's name in the list.
    pub check: String,
    /// Why it failed.
    pub fail: CheckFail,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.check, self.fail)
    }
}

/// The answer of the check list for one save.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Verdict {
    /// Failures of checks set to `refuse`: the save is refused.
    pub refused: Vec<Finding>,
    /// Failures of checks set to `flag`: stored, an audit is queued.
    pub flagged: Vec<Finding>,
}

impl Verdict {
    /// The save may be stored (no `refuse` check failed).
    pub fn accepted(&self) -> bool {
        self.refused.is_empty()
    }

    /// An audit of the game is due (any check failed).
    pub fn audit(&self) -> bool {
        !self.refused.is_empty() || !self.flagged.is_empty()
    }
}

/// A check list change that names a check wrongly.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ListError {
    /// No check has this name.
    #[error("no save check named `{0}`")]
    Unknown(String),
    /// A check with this name is in the list already.
    #[error("a save check named `{0}` is in the list already")]
    Exists(String),
    /// The mode word is not `refuse`, `flag` or `off`.
    #[error("save check `{name}`: mode `{value}` is not refuse, flag or off")]
    BadMode {
        /// The check.
        name: String,
        /// The word given.
        value: String,
    },
}

/// A check in the list.
struct Entry {
    name: String,
    mode: Mode,
    check: Box<dyn SaveCheck + Send + Sync>,
}

/// The standard checks: name, default mode and the check. The modes are
/// the defaults picked for open question 14: the rules that read a field
/// against a table refuse; the experience rate, the affix and property
/// ranges and the kept item IDs start as `flag` (plan N6: new checks bed in
/// as `flag`).
pub const STANDARD: &[(&str, Mode)] = &[
    ("name-class", Mode::Refuse),
    ("status", Mode::Refuse),
    ("level-experience", Mode::Refuse),
    ("experience-rate", Mode::Flag),
    ("stat-points", Mode::Refuse),
    ("skill-points", Mode::Refuse),
    ("gold", Mode::Refuse),
    ("progress", Mode::Refuse),
    ("item-legality", Mode::Refuse),
    ("item-ranges", Mode::Flag),
    ("item-duplicates", Mode::Refuse),
    ("item-ids-kept", Mode::Flag),
];

fn standard_check(name: &str) -> Option<Box<dyn SaveCheck + Send + Sync>> {
    Some(match name {
        "name-class" => Box::new(character::NameClass),
        "status" => Box::new(character::Status),
        "level-experience" => Box::new(character::LevelExperience),
        "experience-rate" => Box::new(character::ExperienceRate::default()),
        "stat-points" => Box::new(character::StatPoints),
        "skill-points" => Box::new(character::SkillPoints),
        "gold" => Box::new(character::Gold),
        "progress" => Box::new(progress::Progress),
        "item-legality" => Box::new(items::ItemLegality),
        "item-ranges" => Box::new(items::ItemRanges),
        "item-duplicates" => Box::new(items::ItemDuplicates),
        "item-ids-kept" => Box::new(items::ItemIdsKept),
        _ => return None,
    })
}

/// A realm's save checks, in order, each with its mode.
pub struct CheckList {
    tables: Arc<CheckTables>,
    entries: Vec<Entry>,
}

impl fmt::Debug for CheckList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.entries.iter().map(|e| (&e.name, e.mode)))
            .finish()
    }
}

impl CheckList {
    /// An empty list over the realm's tables.
    pub fn new(tables: Arc<CheckTables>) -> Self {
        Self {
            tables,
            entries: Vec::new(),
        }
    }

    /// The standard checks ([`STANDARD`]) in their default modes.
    pub fn standard(tables: Arc<CheckTables>) -> Self {
        let mut list = Self::new(tables);
        for &(name, mode) in STANDARD {
            let check = standard_check(name).expect("every standard name has its check");
            list.entries.push(Entry {
                name: name.to_owned(),
                mode,
                check,
            });
        }
        list
    }

    /// The realm's tables.
    pub fn tables(&self) -> &CheckTables {
        &self.tables
    }

    /// The checks' names and modes, in order.
    pub fn modes(&self) -> Vec<(&str, Mode)> {
        self.entries
            .iter()
            .map(|e| (e.name.as_str(), e.mode))
            .collect()
    }

    /// Adds a check at the end (a mod's own rule).
    pub fn add(
        &mut self,
        name: &str,
        mode: Mode,
        check: Box<dyn SaveCheck + Send + Sync>,
    ) -> Result<(), ListError> {
        if self.entries.iter().any(|e| e.name == name) {
            return Err(ListError::Exists(name.to_owned()));
        }
        self.entries.push(Entry {
            name: name.to_owned(),
            mode,
            check,
        });
        Ok(())
    }

    /// Replaces the check named `name` (a mod's version of a standard
    /// rule); its place and mode stay.
    pub fn replace(
        &mut self,
        name: &str,
        check: Box<dyn SaveCheck + Send + Sync>,
    ) -> Result<(), ListError> {
        let e = self.entry(name)?;
        e.check = check;
        Ok(())
    }

    /// Removes the check named `name`; false when there is none.
    pub fn remove(&mut self, name: &str) -> bool {
        let n = self.entries.len();
        self.entries.retain(|e| e.name != name);
        self.entries.len() != n
    }

    /// Sets the mode of the check named `name`.
    pub fn set_mode(&mut self, name: &str, mode: Mode) -> Result<(), ListError> {
        self.entry(name)?.mode = mode;
        Ok(())
    }

    /// One config line `check.NAME = MODE`: `Ok(true)` when the key is a
    /// check key and was applied, `Ok(false)` when the key is not a check
    /// key (the caller reads it).
    pub fn configure(&mut self, key: &str, value: &str) -> Result<bool, ListError> {
        let Some(name) = key.strip_prefix("check.") else {
            return Ok(false);
        };
        let mode = Mode::parse(value).ok_or_else(|| ListError::BadMode {
            name: name.to_owned(),
            value: value.to_owned(),
        })?;
        self.set_mode(name, mode)?;
        Ok(true)
    }

    fn entry(&mut self, name: &str) -> Result<&mut Entry, ListError> {
        self.entries
            .iter_mut()
            .find(|e| e.name == name)
            .ok_or_else(|| ListError::Unknown(name.to_owned()))
    }

    /// Runs every check not `off` on the stored record `before` and the
    /// new save `after`.
    pub fn run(&self, before: &D2s, after: &D2s, info: SaveInfo) -> Verdict {
        let ctx = SaveCtx::new(&self.tables, before, after, info);
        let mut v = Verdict::default();
        for e in &self.entries {
            if e.mode == Mode::Off {
                continue;
            }
            if let Err(fail) = e.check.check(before, after, &ctx) {
                let f = Finding {
                    check: e.name.clone(),
                    fail,
                };
                match e.mode {
                    Mode::Refuse => v.refused.push(f),
                    _ => v.flagged.push(f),
                }
            }
        }
        v
    }

    /// [`CheckList::run`] on record bytes, as the main server holds them:
    /// `before` is the stored record, `None` for a character that was
    /// never saved (it is checked against a new character of the same
    /// name, class and status). A record that does not decode is refused
    /// under the check name `decode`.
    pub fn run_records(&self, before: Option<&[u8]>, after: &[u8], info: SaveInfo) -> Verdict {
        let refuse = |what: &str, e: D2sError| Verdict {
            refused: vec![Finding {
                check: "decode".into(),
                fail: CheckFail::new(format!("{what} does not decode: {e}")),
            }],
            flagged: Vec::new(),
        };
        // The reader cuts a record at `MAX_FILE` bytes (as the game's
        // buffer would) and never looks past it, so a longer record would
        // be checked on its first part and stored whole. The writer never
        // makes one.
        for (what, b) in [("the save", Some(after)), ("the stored record", before)] {
            if let Some(b) = b.filter(|b| b.len() > d2s::MAX_FILE) {
                return Verdict {
                    refused: vec![Finding {
                        check: "decode".into(),
                        fail: CheckFail::new(format!(
                            "{what} is {} bytes, over the {}-byte limit",
                            b.len(),
                            d2s::MAX_FILE
                        )),
                    }],
                    flagged: Vec::new(),
                };
            }
        }
        let after = match decode(&self.tables, after) {
            Ok(s) => s,
            Err(e) => return refuse("the save", e),
        };
        let before = match before {
            Some(b) => match decode(&self.tables, b) {
                Ok(s) => s,
                Err(e) => return refuse("the stored record", e),
            },
            None => new_character_like(&after),
        };
        self.run(&before, &after, info)
    }
}

/// Decodes a record with the realm's tables (an expansion record reads
/// the hireling and golem sections).
pub fn decode(tables: &CheckTables, bytes: &[u8]) -> Result<D2s, D2sError> {
    let expansion = d2s::header_status(bytes).is_some_and(|s| s & d2s::status::EXPANSION != 0);
    let opts = ReadOptions {
        expansion,
        game: None,
    };
    d2s::read(bytes, &opts, &tables.reading(expansion))
}

/// The new character `after` was made as: its name, class and the status
/// bits a creation sets (hardcore, expansion, ladder), level 1, no body.
pub fn new_character_like(after: &D2s) -> D2s {
    let h = &after.header;
    let keep = d2s::status::HARDCORE | d2s::status::EXPANSION | d2s::status::LADDER;
    let mut s = D2s::new_stub(
        h.name_bytes(),
        h.class,
        (h.status & keep) | d2s::status::NEW,
        0,
    )
    .unwrap_or_else(|| D2s {
        header: h.clone(),
        body: None,
    });
    s.header.level = 1;
    s
}
