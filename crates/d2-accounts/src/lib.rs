//! `d2-accounts`: register, log in, and the online characters an account
//! owns (contracts section 4). Only the main server uses it: a game server
//! never opens the accounts database or the character folder.
//!
//! - [`characters`]: the character folder. Online characters are created
//!   on the server and stored here in D2RUST's own versioned format. They
//!   are never written to a client.
//! - [`backup`]: validated snapshots of the server characters, offline and
//!   on a running server's request.
//! - [`denied`]: a refused request (code, wait, words), the answer of
//!   accounts, the lobby and games alike.
//! - [`social`] and [`ladder`]: the realm lobby's guilds, friends,
//!   privacy, direct messages, mutes, seasons and ladder rows, in the same
//!   database ([`Accounts::store`]).
//!
//! - **Storage:** one SQLite database, `<data>/accounts.sqlite`, with the
//!   accounts and a summary row per character ([`db`]), and one file per
//!   character, `<data>/characters/<account id>/<name>.d2rc`, in the
//!   character envelope of [`crate::characters`]. The file is the truth;
//!   the row is what character select and the lobby read, written after
//!   every save ([`Accounts::saved`]) and rebuilt by [`Accounts::reindex`].
//! - **Passwords** are hashed with argon2id ([`hash`]); at most two hashes
//!   run at a time, off the network threads. Every failed login is
//!   answered after the same fixed delay, so the answer's timing says
//!   nothing about why it failed.
//! - **Limits** on registering and logging in are in [`limits`].
//! - **Characters:** names are unique on the server in any case. A new
//!   character is only a row until its first game makes and saves its
//!   file. A delete marks the row and moves the file to
//!   `<data>/deleted/<account id>/`; the name stays taken for
//!   [`NAME_HOLD`], then it is free again.
//! - Sessions (who is logged in on which connection) live with the
//!   connections, not here: a server restart ends them all.
//!
//! Logs never carry passwords or hashes.

pub mod backup;
pub mod characters;
pub mod db;
pub mod denied;
pub mod hash;
pub mod ladder;
pub mod limits;
pub mod records;
pub mod rules;
pub mod social;
pub mod summary;

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use d2_formats::d2s::status;

use d2_net::msg::account::{CharSummary, CLASSES, MAX_CHARACTERS_LIMIT};
use d2_net::msg::ErrorCode;

use crate::characters::{CharacterStore, EXTENSION};
pub use crate::denied::Denied;
pub use d2_service::character::{AccountId, NewChar};
use d2_service::log::ServerLog;
use db::Db;
pub use hash::HashCost;
use limits::Limits;
pub use records::RecordFormat;

/// How long a deleted character's name stays taken.
pub const NAME_HOLD: Duration = Duration::from_secs(7 * 24 * 3600);
/// Characters per account unless the config says otherwise.
pub const DEFAULT_MAX_CHARACTERS: u8 = 16;
/// How long every failed login waits before its answer.
pub const FAIL_DELAY: Duration = Duration::from_millis(500);
/// The failed-login delay in the test limits mode (`limits = test`).
pub const TEST_FAIL_DELAY: Duration = Duration::from_millis(10);
/// Password hashes that may run at once.
const HASHES_AT_ONCE: usize = 2;
/// The database file in the data folder.
pub const DATABASE: &str = "accounts.sqlite";

/// A logged-in account: its id and its name as stored, with its case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
}

/// What a game needs to seat a character: its summary, its file, its
/// stored record, and how to make it when it has never been played.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharSeat {
    pub summary: CharSummary,
    pub file: PathBuf,
    /// The record in the file (unwrapped from the character envelope);
    /// `None` while the character has no file yet.
    pub record: Option<Vec<u8>>,
    /// `Some` while the character has no file yet.
    pub new: Option<NewChar>,
}

/// The accounts part of the server config.
///
/// ```text
/// data           = data        # the database and the character files
/// argon2         = normal      # or `test`, for test servers only
/// limits         = normal      # or `test`, for test servers only
/// max_characters = 16          # per account, 1 to 32
/// registration   = open        # or `closed`: no new accounts
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountsConfig {
    pub data: PathBuf,
    pub cost: HashCost,
    /// `test` divides every rate-limit window and lock by 60 and makes the
    /// failed-login delay [`TEST_FAIL_DELAY`].
    pub limits: LimitMode,
    pub max_characters: u8,
    pub registration_open: bool,
    /// The wait before every failed login's answer ([`FAIL_DELAY`]; not a
    /// file key). Ignored in the test limits mode, which uses
    /// [`TEST_FAIL_DELAY`].
    pub fail_delay: Duration,
    /// The format of the stored records (`records`, [`RecordFormat`]).
    pub records: RecordFormat,
}

impl AsRef<AccountsConfig> for AccountsConfig {
    fn as_ref(&self) -> &AccountsConfig {
        self
    }
}

/// The rate limits' mode (`limits`, contracts 5 and 10.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitMode {
    /// The normal windows.
    Normal,
    /// The test windows: every window and lock divided by 60, and the
    /// failed-login delay 10 ms.
    Test,
}

impl AccountsConfig {
    /// The config keys this part reads.
    pub const KEYS: &'static [&'static str] = &[
        "data",
        "argon2",
        "limits",
        "max_characters",
        "registration",
        "records",
    ];

    /// The defaults for a server folder at `base`.
    pub fn new(base: &Path) -> Self {
        Self {
            data: base.join("data"),
            cost: HashCost::Normal,
            limits: LimitMode::Normal,
            max_characters: DEFAULT_MAX_CHARACTERS,
            registration_open: true,
            fail_delay: FAIL_DELAY,
            records: RecordFormat::D2s,
        }
    }

    /// Sets one of [`Self::KEYS`] from its text; a relative path is taken
    /// from `base`.
    pub fn set(&mut self, key: &str, value: &str, base: &Path) -> Result<(), String> {
        match key {
            "data" => self.data = base.join(value),
            "argon2" => {
                self.cost = match value {
                    "normal" => HashCost::Normal,
                    "test" => HashCost::Test,
                    _ => return Err("must be `normal` or `test`".into()),
                }
            }
            "limits" => {
                self.limits = match value {
                    "normal" => LimitMode::Normal,
                    "test" => LimitMode::Test,
                    _ => return Err("must be `normal` or `test`".into()),
                }
            }
            "max_characters" => {
                let v = value.parse::<u8>().map_err(|e| e.to_string())?;
                if v == 0 || usize::from(v) > MAX_CHARACTERS_LIMIT {
                    return Err(format!("must be 1 to {MAX_CHARACTERS_LIMIT}"));
                }
                self.max_characters = v;
            }
            "registration" => {
                self.registration_open = match value {
                    "open" => true,
                    "closed" => false,
                    _ => return Err("must be `open` or `closed`".into()),
                }
            }
            "records" => self.records = RecordFormat::parse(value)?,
            _ => return Err(format!("`{key}` is not an accounts key")),
        }
        Ok(())
    }

    pub fn database(&self) -> PathBuf {
        self.data.join(DATABASE)
    }

    pub fn characters_dir(&self) -> PathBuf {
        self.data.join("characters")
    }

    pub fn deleted_dir(&self) -> PathBuf {
        self.data.join("deleted")
    }
}

/// Opening the accounts failed.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum AccountError {
    #[error(transparent)]
    Db(#[from] db::DbError),
    #[error("{0}")]
    Io(String),
}

/// What a reindex did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reindexed {
    /// Rows written from character files.
    pub rows: usize,
    /// Files left out, each with why.
    pub skipped: Vec<String>,
}

/// The accounts service. Cheap to clone: clones share one database.
#[derive(Clone)]
pub struct Accounts {
    inner: Arc<Inner>,
}

struct Inner {
    cfg: AccountsConfig,
    db: Mutex<Db>,
    /// How long callers waited for the database and held it.
    db_use: DbUse,
    limits: Mutex<Limits>,
    hashing: tokio::sync::Semaphore,
    /// A hash checked for names with no account, so an unknown name costs
    /// the same as a wrong password.
    dummy: String,
    log: Option<Arc<ServerLog>>,
    /// Other databases in the data folder (relative paths) that every
    /// backup holds too ([`Accounts::with_backup_db`]).
    backup_dbs: Vec<PathBuf>,
}

/// The database's use: one connection behind one lock, so a caller that
/// finds it taken waits. The realm's `stats` line reports it.
#[derive(Debug, Default)]
struct DbUse {
    locks: AtomicU64,
    waited_us: AtomicU64,
    max_wait_us: AtomicU64,
    held_us: AtomicU64,
    max_held_us: AtomicU64,
}

/// What [`Accounts::db_use`] reports: how often the database was taken,
/// how long callers waited for it and how long they held it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DbUseStats {
    pub locks: u64,
    pub waited_us: u64,
    pub max_wait_us: u64,
    pub held_us: u64,
    pub max_held_us: u64,
}

fn micros(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

/// The database while a caller holds it; dropping it counts the time held.
struct DbGuard<'a> {
    db: MutexGuard<'a, Db>,
    since: Instant,
    use_: &'a DbUse,
}

impl std::ops::Deref for DbGuard<'_> {
    type Target = Db;
    fn deref(&self) -> &Db {
        &self.db
    }
}

impl std::ops::DerefMut for DbGuard<'_> {
    fn deref_mut(&mut self) -> &mut Db {
        &mut self.db
    }
}

impl Drop for DbGuard<'_> {
    fn drop(&mut self) {
        let held = micros(self.since.elapsed());
        self.use_.held_us.fetch_add(held, Ordering::Relaxed);
        self.use_.max_held_us.fetch_max(held, Ordering::Relaxed);
    }
}

impl Inner {
    /// Takes the database, counting the wait.
    fn lock_db(&self) -> DbGuard<'_> {
        let asked = Instant::now();
        let db = self.db.lock().unwrap_or_else(|p| p.into_inner());
        let since = Instant::now();
        let waited = micros(since - asked);
        let u = &self.db_use;
        u.locks.fetch_add(1, Ordering::Relaxed);
        u.waited_us.fetch_add(waited, Ordering::Relaxed);
        u.max_wait_us.fetch_max(waited, Ordering::Relaxed);
        DbGuard { db, since, use_: u }
    }
}

impl std::fmt::Debug for Accounts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Accounts")
            .field("data", &self.inner.cfg.data)
            .finish_non_exhaustive()
    }
}

/// Seconds since the Unix epoch.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn io(e: std::io::Error) -> AccountError {
    AccountError::Io(e.to_string())
}

impl Accounts {
    /// Opens the accounts of a program's config (its accounts part).
    pub fn open(cfg: &(impl AsRef<AccountsConfig> + ?Sized)) -> Result<Self, AccountError> {
        Self::open_with(cfg.as_ref().clone())
    }

    /// Opens the accounts in `cfg.data`, creating the folder and the
    /// database if needed.
    pub fn open_with(cfg: AccountsConfig) -> Result<Self, AccountError> {
        std::fs::create_dir_all(cfg.characters_dir()).map_err(io)?;
        let db = Db::open(&cfg.database())?;
        let dummy = hash::hash(cfg.cost, &format!("{:x}", unix_now()));
        let limits = match cfg.limits {
            LimitMode::Normal => Limits::new(),
            LimitMode::Test => Limits::test(),
        };
        Ok(Self {
            inner: Arc::new(Inner {
                cfg,
                db: Mutex::new(db),
                db_use: DbUse::default(),
                limits: Mutex::new(limits),
                hashing: tokio::sync::Semaphore::new(HASHES_AT_ONCE),
                dummy,
                log: None,
                backup_dbs: Vec::new(),
            }),
        })
    }

    /// Every backup also holds the database `rel` of the data folder
    /// (`realms/d2rust/realm.sqlite`), copied consistently while it is in
    /// use; one that doesn't exist yet is skipped. Call before cloning: it
    /// does nothing once the accounts are shared.
    pub fn with_backup_db(mut self, rel: impl Into<PathBuf>) -> Self {
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.backup_dbs.push(rel.into());
        }
        self
    }

    /// Writes failures and account events to `log`, and warns about the
    /// test-only settings in it. Call before cloning: it does nothing once
    /// `self` has been cloned.
    pub fn with_log(mut self, log: Arc<ServerLog>) -> Self {
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            if inner.cfg.cost == HashCost::Test {
                log.line(
                    "accounts: WARNING: argon2 = test: weak password hashing (test servers only)",
                );
            }
            if inner.cfg.limits == LimitMode::Test {
                log.line(
                    "accounts: WARNING: limits = test: shortened rate limits and 10 ms failed-login delay (test servers only)",
                );
            }
            inner.log = Some(log);
        }
        self
    }

    /// The wait before a failed login's answer: the configured one, or
    /// [`TEST_FAIL_DELAY`] in the test limits mode.
    fn fail_delay(&self) -> Duration {
        match self.inner.cfg.limits {
            LimitMode::Normal => self.inner.cfg.fail_delay,
            LimitMode::Test => TEST_FAIL_DELAY,
        }
    }

    pub fn config(&self) -> &AccountsConfig {
        &self.inner.cfg
    }

    fn log(&self, line: &str) {
        if let Some(log) = &self.inner.log {
            log.line(&format!("accounts: {line}"));
        }
    }

    /// Runs `f` on the database off the network threads. A database
    /// failure is logged and answered as [`ErrorCode::OTHER`].
    async fn db<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Db) -> Result<T, db::DbError> + Send + 'static,
    ) -> Result<T, Denied> {
        let inner = self.inner.clone();
        let r = tokio::task::spawn_blocking(move || {
            let mut db = inner.lock_db();
            f(&mut db)
        })
        .await
        .unwrap_or_else(|e| Err(db::DbError(format!("task failed: {e}"))));
        r.map_err(|e| self.store_failed(&e))
    }

    /// Logs a database failure and gives its answer.
    fn store_failed(&self, e: &db::DbError) -> Denied {
        self.log(&e.to_string());
        Denied::because(
            ErrorCode::OTHER,
            "The account store failed. Try again later.",
        )
    }

    /// Runs `f` on the database off the network threads: the realm's way
    /// to the [`social`] and [`ladder`] stores. A broken rule is answered
    /// with its code; a database failure is logged and answered as
    /// [`ErrorCode::OTHER`].
    pub async fn store<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Db) -> social::SocialResult<T> + Send + 'static,
    ) -> Result<T, Denied> {
        match self.db(move |db| Ok(f(db))).await? {
            Ok(v) => Ok(v),
            Err(social::SocialError::Refused(r)) => Err(r.into()),
            Err(social::SocialError::Db(e)) => Err(self.store_failed(&e)),
        }
    }

    /// The database's use since the start: how often it was taken, how
    /// long callers waited for it and held it.
    pub fn db_use(&self) -> DbUseStats {
        let u = &self.inner.db_use;
        DbUseStats {
            locks: u.locks.load(Ordering::Relaxed),
            waited_us: u.waited_us.load(Ordering::Relaxed),
            max_wait_us: u.max_wait_us.load(Ordering::Relaxed),
            held_us: u.held_us.load(Ordering::Relaxed),
            max_held_us: u.max_held_us.load(Ordering::Relaxed),
        }
    }

    /// Checks `password` against `phc` (or the dummy hash), at most
    /// [`HASHES_AT_ONCE`] at a time, off the network threads.
    async fn verify(&self, phc: Option<String>, password: &str) -> bool {
        let _permit = self.inner.hashing.acquire().await;
        let inner = self.inner.clone();
        let password = password.to_owned();
        tokio::task::spawn_blocking(move || {
            let phc = phc.as_deref().unwrap_or(&inner.dummy);
            hash::verify(phc, &password)
        })
        .await
        .unwrap_or(false)
    }

    async fn hash(&self, password: &str) -> String {
        let _permit = self.inner.hashing.acquire().await;
        let cost = self.inner.cfg.cost;
        let password = password.to_owned();
        tokio::task::spawn_blocking(move || hash::hash(cost, &password))
            .await
            .expect("hashing never panics")
    }

    fn limits(&self) -> std::sync::MutexGuard<'_, Limits> {
        self.inner.limits.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Makes an account and logs it in.
    pub async fn register(
        &self,
        name: &str,
        password: &str,
        ip: IpAddr,
    ) -> Result<Account, Denied> {
        if !self.inner.cfg.registration_open {
            return Err(ErrorCode::REGISTRATION_CLOSED.into());
        }
        rules::check_account_name(name)
            .map_err(|t| Denied::because(ErrorCode::BAD_ACCOUNT_NAME, t))?;
        rules::check_password(password).map_err(|t| Denied::because(ErrorCode::BAD_PASSWORD, t))?;
        let now = Instant::now();
        // Check and reserve the slot in one step: registrations that run
        // at the same time must not all pass a check made before any of
        // them is counted.
        self.limits()
            .reserve_register(ip, now)
            .map_err(|w| Denied::wait(ErrorCode::RATE_LIMITED, w))?;
        let key = rules::name_key(name);
        // A taken name answers before any hashing. Probing names costs
        // login attempts.
        let taken = {
            let key = key.clone();
            self.db(move |db| db.account_by_key(&key)).await?.is_some()
        };
        if taken {
            self.limits().release_register(ip, now);
            let _ = self.limits().take_login(ip, now);
            return Err(ErrorCode::ACCOUNT_NAME_TAKEN.into());
        }
        let pw_hash = self.hash(password).await;
        let (stored, at) = (name.to_owned(), unix_now());
        let made = self
            .db(move |db| {
                let id = db.insert_account(&stored, &key, &pw_hash, at)?;
                if let Some(id) = id {
                    db.set_last_login(id, at)?;
                }
                Ok(id)
            })
            .await;
        let id = match made {
            Ok(Some(id)) => id,
            Ok(None) => {
                self.limits().release_register(ip, now);
                return Err(Denied::new(ErrorCode::ACCOUNT_NAME_TAKEN));
            }
            Err(d) => {
                self.limits().release_register(ip, now);
                return Err(d);
            }
        };
        self.log(&format!("registered `{name}` ({})", AccountId(id)));
        Ok(Account {
            id: AccountId(id),
            name: name.to_owned(),
        })
    }

    /// Logs in. Every failure is answered the configured failed-login
    /// delay after the call began.
    pub async fn login(&self, name: &str, password: &str, ip: IpAddr) -> Result<Account, Denied> {
        let start = tokio::time::Instant::now();
        let r = self.try_login(name, password, ip).await;
        if r.is_err() {
            tokio::time::sleep_until(start + self.fail_delay()).await;
        }
        r
    }

    async fn try_login(&self, name: &str, password: &str, ip: IpAddr) -> Result<Account, Denied> {
        let key = rules::name_key(name);
        {
            let now = Instant::now();
            let mut limits = self.limits();
            limits
                .take_login(ip, now)
                .map_err(|w| Denied::wait(ErrorCode::RATE_LIMITED, w))?;
            if let Some(w) = limits.locked(&key, now) {
                return Err(Denied::wait(ErrorCode::ACCOUNT_LOCKED, w));
            }
        }
        let row = if rules::check_account_name(name).is_ok() {
            let key = key.clone();
            self.db(move |db| db.account_by_key(&key)).await?
        } else {
            None
        };
        let good = rules::check_password(password).is_ok()
            && self
                .verify(row.as_ref().map(|r| r.pw_hash.clone()), password)
                .await
            && row.is_some();
        let Some(row) = row.filter(|_| good) else {
            self.limits().note_failure(&key, Instant::now());
            return Err(ErrorCode::WRONG_LOGIN.into());
        };
        // The name may have locked while this password was being checked
        // (attempts that passed the first check at the same time). The
        // lock wins over a good password, so a queued guess can't slip in.
        if let Some(w) = self.limits().locked(&key, Instant::now()) {
            return Err(Denied::wait(ErrorCode::ACCOUNT_LOCKED, w));
        }
        if row.disabled {
            return Err(ErrorCode::ACCOUNT_DISABLED.into());
        }
        self.limits().clear_failures(&key);
        let id = row.id;
        self.db(move |db| db.set_last_login(id, unix_now())).await?;
        Ok(Account {
            id: AccountId(row.id),
            name: row.name,
        })
    }

    /// The account's characters, oldest first. A database failure is
    /// logged and gives an empty list.
    pub async fn characters(&self, a: AccountId) -> Vec<CharSummary> {
        self.db(move |db| db.characters(a.0))
            .await
            .unwrap_or_default()
    }

    /// Creates a non-ladder character on the account. It is a summary row
    /// until its first game saves it.
    pub async fn create_character(
        &self,
        a: AccountId,
        new: NewChar,
    ) -> Result<CharSummary, Denied> {
        self.create_character_in(a, new, ladder::NON_LADDER).await
    }

    /// Creates a character of ladder season `season` on the account
    /// ([`ladder::NON_LADDER`]: a non-ladder one). A ladder character's
    /// status word has the ladder bit; the caller checks that the season is
    /// the open one.
    pub async fn create_character_in(
        &self,
        a: AccountId,
        new: NewChar,
        season: u16,
    ) -> Result<CharSummary, Denied> {
        if let Err(e) = characters::check_name(&new.name) {
            let text = match e {
                characters::CharacterError::BadName { reason, .. } => reason,
                _ => "That character name isn't allowed.",
            };
            return Err(Denied::because(ErrorCode::BAD_CHAR_NAME, text));
        }
        if new.class >= CLASSES {
            return Err(Denied::because(
                ErrorCode::OTHER,
                "That class doesn't exist.",
            ));
        }
        // Druid (5) and Assassin (6) are expansion classes.
        if new.class >= 5 && !new.expansion {
            return Err(ErrorCode::EXPANSION_CLASS.into());
        }
        if new.progression > 0x1F {
            return Err(Denied::because(
                ErrorCode::OTHER,
                "That progression doesn't exist.",
            ));
        }
        let mut s = summary::of_new(&new);
        if season != ladder::NON_LADDER {
            s.status |= status::LADDER;
            s.season = season;
        }
        let max = usize::from(self.inner.cfg.max_characters);
        let row = s.clone();
        let hold = i64::try_from(NAME_HOLD.as_secs()).unwrap_or(i64::MAX);
        let made = self
            .db(move |db| {
                if db.count_characters(a.0)? >= max {
                    return Ok(Err(ErrorCode::CHAR_LIMIT));
                }
                let now = unix_now();
                Ok(if db.insert_character(a.0, &row, now, now - hold)? {
                    Ok(())
                } else {
                    Err(ErrorCode::CHAR_NAME_TAKEN)
                })
            })
            .await?;
        made.map_err(Denied::new)?;
        self.log(&format!("{a} created character `{}`", s.name));
        Ok(s)
    }

    /// Deletes a character of the account; `password` is the account's,
    /// to confirm. A wrong password counts as a failed login and is
    /// answered after the same delay. The caller first refuses a
    /// character that is in a game.
    pub async fn delete_character(
        &self,
        a: AccountId,
        name: &str,
        password: &str,
    ) -> Result<(), Denied> {
        let start = tokio::time::Instant::now();
        let r = self.try_delete(a, name, password).await;
        if matches!(&r, Err(d) if d.code == ErrorCode::WRONG_LOGIN) {
            tokio::time::sleep_until(start + self.fail_delay()).await;
        }
        r
    }

    async fn try_delete(&self, a: AccountId, name: &str, password: &str) -> Result<(), Denied> {
        let account = self
            .db(move |db| db.account_by_id(a.0))
            .await?
            .ok_or(Denied::new(ErrorCode::NOT_LOGGED_IN))?;
        let key = rules::name_key(&account.name);
        if let Some(w) = self.limits().locked(&key, Instant::now()) {
            return Err(Denied::wait(ErrorCode::ACCOUNT_LOCKED, w));
        }
        if !self.verify(Some(account.pw_hash), password).await {
            self.limits().note_failure(&key, Instant::now());
            return Err(ErrorCode::WRONG_LOGIN.into());
        }
        // The name may have locked while this password was being checked.
        if let Some(w) = self.limits().locked(&key, Instant::now()) {
            return Err(Denied::wait(ErrorCode::ACCOUNT_LOCKED, w));
        }
        let char_key = rules::name_key(name);
        let row = {
            let char_key = char_key.clone();
            self.db(move |db| db.character(&char_key)).await?
        }
        .filter(|r| r.account_id == a.0 && r.deleted.is_none())
        .ok_or(Denied::new(ErrorCode::UNKNOWN_CHAR))?;
        let now = unix_now();
        let deleted = self
            .db(move |db| db.mark_deleted(a.0, &char_key, now))
            .await?;
        if !deleted {
            return Err(ErrorCode::UNKNOWN_CHAR.into());
        }
        let stored = row.summary.name;
        let file = self.file_of(a, &stored);
        if file.exists() {
            let dir = self.inner.cfg.deleted_dir().join(a.0.to_string());
            let moved = std::fs::create_dir_all(&dir).and_then(|()| {
                std::fs::rename(&file, dir.join(format!("{stored}-{now}.{EXTENSION}")))
            });
            if let Err(e) = moved {
                // The row is deleted; the file stays where the reindex
                // would bring it back, so say so.
                self.log(&format!(
                    "{a}: moving deleted `{stored}`'s file failed: {e}"
                ));
            }
        }
        self.log(&format!("{a} deleted character `{stored}`"));
        Ok(())
    }

    /// One of the account's live characters.
    pub async fn character(&self, a: AccountId, name: &str) -> Result<CharSummary, Denied> {
        let key = rules::name_key(name);
        self.db(move |db| db.character(&key))
            .await?
            .filter(|r| r.account_id == a.0 && r.deleted.is_none())
            .map(|r| r.summary)
            .ok_or(Denied::new(ErrorCode::UNKNOWN_CHAR))
    }

    /// What a game needs to seat one of the account's characters: its
    /// stored record is read now. A file that can't be read or is damaged
    /// refuses the seat (logged).
    pub async fn seat(&self, a: AccountId, name: &str) -> Result<CharSeat, Denied> {
        let summary = self.character(a, name).await?;
        let file = self.file_of(a, &summary.name);
        let (stored, path) = (summary.name.clone(), file.clone());
        let record = tokio::task::spawn_blocking(move || characters::load_record(&stored, &path))
            .await
            .map_err(|_| Denied::new(ErrorCode::OTHER))?
            .map_err(|e| {
                self.log(&format!("{a}: {e}"));
                Denied::new(ErrorCode::OTHER)
            })?;
        let new = record.is_none().then(|| NewChar {
            name: summary.name.clone(),
            class: summary.class,
            hardcore: summary.status & d2_formats::d2s::status::HARDCORE != 0,
            expansion: summary.status & d2_formats::d2s::status::EXPANSION != 0,
            progression: status::progression(summary.status),
        });
        Ok(CharSeat {
            summary,
            file,
            record,
            new,
        })
    }

    fn file_of(&self, a: AccountId, stored: &str) -> PathBuf {
        self.inner
            .cfg
            .characters_dir()
            .join(a.0.to_string())
            .join(format!("{stored}.{EXTENSION}"))
    }

    /// Where the game loads and saves this character's record. `name`
    /// matches the stored name in any case; the path uses the stored one.
    /// Only a live character of this account has a file: an unknown name
    /// (or one of another account's, or a deleted one) is `UnknownChar`,
    /// never a path built from unchecked text.
    pub fn character_file(&self, a: AccountId, name: &str) -> Result<PathBuf, Denied> {
        let key = rules::name_key(name);
        let row = {
            let db = self.inner.lock_db();
            db.character(&key).ok().flatten()
        }
        .filter(|r| r.account_id == a.0 && r.deleted.is_none())
        .ok_or(Denied::new(ErrorCode::UNKNOWN_CHAR))?;
        Ok(self.file_of(a, &row.summary.name))
    }

    /// The account's character folder, created if needed (a game calls it
    /// before its first save of the account's character).
    pub fn character_store(
        &self,
        a: AccountId,
    ) -> Result<CharacterStore, characters::CharacterError> {
        CharacterStore::open(self.inner.cfg.characters_dir().join(a.0.to_string()))
    }

    /// Stores a save the main server accepted (contracts 11.6): writes
    /// `record` to the character's file (write-then-rename), then gives
    /// its row the record's summary ([`Accounts::saved`]). Refused with
    /// [`ErrorCode::UNKNOWN_CHAR`] when the account has no such live
    /// character, and [`ErrorCode::SAVE_INVALID`] when the record's header
    /// doesn't read or names another character.
    pub async fn store_record(
        &self,
        a: AccountId,
        name: &str,
        record: Vec<u8>,
        last_played: u64,
    ) -> Result<CharSummary, Denied> {
        self.store_save(a, name, record, None, last_played).await
    }

    /// A game's save of character `name`: [`Accounts::store_record`], but
    /// a realm whose records are in the game's own format
    /// ([`RecordFormat::Game`]) takes the summary the game server
    /// `reported` instead of reading the record. A `.d2s` realm ignores
    /// `reported`: it always reads the record itself.
    pub async fn store_played(
        &self,
        a: AccountId,
        name: &str,
        record: Vec<u8>,
        reported: &CharSummary,
        last_played: u64,
    ) -> Result<CharSummary, Denied> {
        self.store_save(a, name, record, Some(reported), last_played)
            .await
    }

    async fn store_save(
        &self,
        a: AccountId,
        name: &str,
        record: Vec<u8>,
        reported: Option<&CharSummary>,
        last_played: u64,
    ) -> Result<CharSummary, Denied> {
        let summary = self
            .inner
            .cfg
            .records
            .summary(&record, reported, last_played)
            .map_err(|why| Denied::because(ErrorCode::SAVE_INVALID, why))?;
        if rules::name_key(&summary.name) != rules::name_key(name) {
            return Err(Denied::because(
                ErrorCode::SAVE_INVALID,
                "the record names another character",
            ));
        }
        let path = self.character_file(a, name)?;
        let written = tokio::task::spawn_blocking(move || {
            characters::write_record(&path, &record)
                .map_err(|e| format!("writing {}: {e}", path.display()))
        })
        .await
        .unwrap_or_else(|e| Err(format!("task failed: {e}")));
        if let Err(e) = written {
            self.log(&format!("{a}: storing `{name}`: {e}"));
            return Err(Denied::because(
                ErrorCode::OTHER,
                "The character store failed. Try again later.",
            ));
        }
        Ok(self.saved(a, summary).await)
    }

    /// Called by a game after it saved a character: its row takes the
    /// saved summary. A progression set before the character's first game
    /// ([`Accounts::set_progression`]) is seeded into the record here: the
    /// game makes a new record without one (a `.d2s` record only: a record
    /// in the game's own format keeps it in the row). Returns the summary
    /// as the row now has it (with the character's ladder season).
    pub async fn saved(&self, a: AccountId, mut summary: CharSummary) -> CharSummary {
        let name = summary.name.clone();
        let row = {
            let db = self.inner.lock_db();
            db.character(&rules::name_key(&summary.name))
                .ok()
                .flatten()
                .filter(|r| r.account_id == a.0 && r.deleted.is_none())
        };
        if let Some(r) = &row {
            summary.season = r.summary.season;
        }
        let seeded = row
            .filter(|r| r.summary.last_played == 0)
            .map(|r| (status::progression(r.summary.status), r.summary.name));
        if let Some((p, stored)) = seeded {
            if p > 0 && status::progression(summary.status) < p {
                let st = (summary.status & !(0x1F << 8)) | (u16::from(p) << 8);
                let file = self.file_of(a, &stored);
                // A record in the game's own format is never edited: the
                // row carries the progression.
                if file.exists() && self.inner.cfg.records.is_d2s() {
                    let write = (|| -> Result<(), String> {
                        let store = CharacterStore::open(file.parent().unwrap_or(Path::new(".")))
                            .map_err(|e| e.to_string())?;
                        let mut record = store.load(&stored).map_err(|e| e.to_string())?;
                        if !summary::set_status(&mut record, st) {
                            return Err("the record has no status word".into());
                        }
                        store.save(&stored, &record).map_err(|e| e.to_string())
                    })();
                    if let Err(e) = write {
                        self.log(&format!("{a}: seeding `{name}`'s progression failed: {e}"));
                    }
                }
                summary.status = st;
                summary.difficulties = status::difficulties(st);
            }
        }
        let row = summary.clone();
        match self.db(move |db| db.save_summary(a.0, &row)).await {
            Ok(true) => {}
            Ok(false) => self.log(&format!("{a}: saved `{name}` has no live row")),
            Err(_) => {}
        }
        summary
    }

    // ------------------------------------------------- admin (blocking)

    /// The id of the account named `name` (any case), if there is one.
    pub fn account_id(&self, name: &str) -> Result<Option<AccountId>, String> {
        let db = self.inner.lock_db();
        Ok(db
            .account_by_key(&rules::name_key(name))
            .map_err(|e| e.to_string())?
            .map(|row| AccountId(row.id)))
    }

    /// Sets an account's password (the admin's reset; there is no email).
    pub fn set_password(&self, name: &str, password: &str) -> Result<(), String> {
        rules::check_password(password)?;
        let h = hash::hash(self.inner.cfg.cost, password);
        let db = self.inner.lock_db();
        if db
            .set_password(&rules::name_key(name), &h)
            .map_err(|e| e.to_string())?
        {
            self.log(&format!("admin set the password of `{name}`"));
            Ok(())
        } else {
            Err(format!("no account named `{name}`"))
        }
    }

    /// Disables (or enables again) an account: a disabled account can't
    /// log in.
    pub fn set_disabled(&self, name: &str, disabled: bool) -> Result<(), String> {
        let db = self.inner.lock_db();
        if db
            .set_disabled(&rules::name_key(name), disabled)
            .map_err(|e| e.to_string())?
        {
            let what = if disabled { "disabled" } else { "enabled" };
            self.log(&format!("admin {what} `{name}`"));
            Ok(())
        } else {
            Err(format!("no account named `{name}`"))
        }
    }

    /// Sets the progression bits (0-31) of the online character `name`
    /// (any case, any account) in its summary row and, when it has one and
    /// it is a `.d2s` ([`RecordFormat::D2s`]), in its stored record: what a test server's `--set-progression
    /// CHARACTER N` and stdin line do (contracts 10.1). A character that
    /// has never been played has only a row; its first save seeds the set
    /// progression into the new record ([`Accounts::saved`]). A record in
    /// the game's own format is never edited: only the row changes.
    pub fn set_progression(&self, name: &str, progression: u8) -> Result<(), String> {
        if progression > 0x1F {
            return Err("the progression must be 0 to 31".into());
        }
        let key = rules::name_key(name);
        let db = self.inner.lock_db();
        let row = db
            .character(&key)
            .map_err(|e| e.to_string())?
            .filter(|r| r.deleted.is_none())
            .ok_or_else(|| format!("no character named `{name}`"))?;
        let status = (row.summary.status & !(0x1F << 8)) | (u16::from(progression) << 8);
        let file = self.file_of(AccountId(row.account_id), &row.summary.name);
        if file.exists() && self.inner.cfg.records.is_d2s() {
            let store = CharacterStore::open(file.parent().unwrap_or(Path::new(".")))
                .map_err(|e| e.to_string())?;
            let mut record = store.load(&row.summary.name).map_err(|e| e.to_string())?;
            if !summary::set_status(&mut record, status) {
                return Err(format!("{}: the record has no status word", file.display()));
            }
            store
                .save(&row.summary.name, &record)
                .map_err(|e| e.to_string())?;
        }
        let saved = CharSummary {
            status,
            difficulties: status::difficulties(status),
            ..row.summary
        };
        if !db
            .save_summary(row.account_id, &saved)
            .map_err(|e| e.to_string())?
        {
            return Err(format!("no live character named `{name}`"));
        }
        self.log(&format!(
            "admin set the progression of `{}` to {progression}",
            saved.name
        ));
        Ok(())
    }

    /// Rebuilds the character rows from the character files. Rows of
    /// characters that were never played (no file yet) stay as they are.
    pub fn reindex(&self) -> Result<Reindexed, String> {
        let mut out = Reindexed::default();
        let root = self.inner.cfg.characters_dir();
        let mut db = self.inner.lock_db();
        for entry in std::fs::read_dir(&root).map_err(|e| e.to_string())? {
            let dir = entry.map_err(|e| e.to_string())?.path();
            let Some(id) = dir
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.parse::<i64>().ok())
            else {
                out.skipped
                    .push(format!("{}: not an account folder", dir.display()));
                continue;
            };
            if db.account_by_id(id).map_err(|e| e.to_string())?.is_none() {
                out.skipped
                    .push(format!("{}: no account {id}", dir.display()));
                continue;
            }
            let store = CharacterStore::open(&dir).map_err(|e| e.to_string())?;
            for name in store.list().map_err(|e| e.to_string())? {
                let file = dir.join(format!("{name}.{EXTENSION}"));
                let played = std::fs::metadata(&file)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                let s = match store.load(&name) {
                    Ok(record) => summary::of_record(&record, played),
                    Err(e) => {
                        out.skipped.push(format!("{}: {e}", file.display()));
                        continue;
                    }
                };
                let s = match s {
                    Ok(s) if s.name.eq_ignore_ascii_case(&name) => CharSummary {
                        name: name.clone(),
                        ..s
                    },
                    Ok(s) => {
                        out.skipped.push(format!(
                            "{}: the record names `{}`",
                            file.display(),
                            s.name
                        ));
                        continue;
                    }
                    Err(e) => {
                        out.skipped.push(format!("{}: {e}", file.display()));
                        continue;
                    }
                };
                if db
                    .upsert_from_file(id, &s, unix_now())
                    .map_err(|e| e.to_string())?
                {
                    out.rows += 1;
                } else {
                    out.skipped
                        .push(format!("{}: the name is another account's", file.display()));
                }
            }
        }
        self.log(&format!(
            "reindex: {} rows, {} files skipped",
            out.rows,
            out.skipped.len()
        ));
        Ok(out)
    }

    /// Writes a backup into the new folder `dest` while the server runs: a
    /// consistent copy of the database (`accounts.sqlite`) and every
    /// account's character files (`characters/<account id>/`), each
    /// checked before it is copied. The backup is usable only once
    /// [`crate::backup::COMPLETE`] is in it. Returns how many character
    /// files it holds.
    pub fn backup_into(&self, dest: &Path) -> Result<usize, String> {
        let root = self.inner.cfg.characters_dir();
        if let (Ok(d), Ok(r)) = (
            dest.parent().unwrap_or(Path::new(".")).canonicalize(),
            self.inner.cfg.data.canonicalize(),
        ) {
            if d.starts_with(&r) {
                return Err("the backup must be outside the data folder".into());
            }
        }
        std::fs::create_dir(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
        {
            let db = self.inner.lock_db();
            db.vacuum_into(&dest.join(DATABASE))
                .map_err(|e| e.to_string())?;
        }
        for rel in &self.inner.backup_dbs {
            let from = self.inner.cfg.data.join(rel);
            if from.exists() {
                vacuum_copy(&from, &dest.join(rel))?;
            }
        }
        let mut count = 0;
        for entry in std::fs::read_dir(&root).map_err(|e| e.to_string())? {
            let dir = entry.map_err(|e| e.to_string())?.path();
            if !dir.is_dir() {
                continue;
            }
            let Some(id) = dir.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let to = dest.join("characters").join(id);
            std::fs::create_dir_all(&to).map_err(|e| e.to_string())?;
            let store = CharacterStore::open(&dir).map_err(|e| e.to_string())?;
            for name in store.list().map_err(|e| e.to_string())? {
                let file = format!("{name}.{EXTENSION}");
                let bytes = std::fs::read(dir.join(&file)).map_err(|e| e.to_string())?;
                characters::decode(&name, &bytes).map_err(|e| e.to_string())?;
                write_new(&to.join(file), &bytes)?;
                count += 1;
            }
        }
        let marker = dest.join(format!("{}.tmp", crate::backup::COMPLETE));
        write_new(&marker, b"D2RUST server backup v1\n")?;
        std::fs::rename(&marker, dest.join(crate::backup::COMPLETE)).map_err(|e| e.to_string())?;
        self.log(&format!(
            "backup {}: {count} character files",
            dest.display()
        ));
        Ok(count)
    }
}

/// A consistent copy of the SQLite database `from` (another connection may
/// be writing to it) at `to`.
fn vacuum_copy(from: &Path, to: &Path) -> Result<(), String> {
    use rusqlite::{Connection, OpenFlags};
    let fail = |e: rusqlite::Error| format!("{}: {e}", from.display());
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let to = to
        .to_str()
        .ok_or_else(|| "the backup path is not text".to_string())?;
    let conn = Connection::open_with_flags(from, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(fail)?;
    conn.busy_timeout(Duration::from_secs(10)).map_err(fail)?;
    conn.execute("VACUUM INTO ?1", [to]).map_err(fail)?;
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_ladder;
#[cfg(test)]
mod tests_social;
