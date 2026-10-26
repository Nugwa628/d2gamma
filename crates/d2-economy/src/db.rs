//! The realm database and the one way it changes: [`Economy::act`] runs a
//! request as one transaction and stores its answer with it.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::{Connection, ErrorCode, OptionalExtension, Params, TransactionBehavior};

use crate::config::EconomyConfig;
use crate::error::{Done, Error, Refusal, RowDone};
use crate::key::{EconomyKey, Req, Scope};
use crate::market::facets::FacetCache;
use crate::market::notices::Notice;
use crate::market::search::SearchLimit;
use crate::registry::{CharacterRecords, ItemRegistry, NoRecords, NoRegistry};
use crate::requests;

/// The schema, one numbered migration per step; `PRAGMA user_version`
/// says how many have run.
pub(crate) const MIGRATIONS: &[&str] = &[
    crate::stash::SCHEMA,
    crate::market::SCHEMA,
    crate::market::search::SCHEMA,
    crate::market::offers::SCHEMA,
    crate::market::SCHEMA_5,
];

/// The realm database of `mod_id` under the main server's data folder:
/// `<data>/realms/<mod_id>/realm.sqlite`. Mod IDs are 1 to 32 characters
/// of `a-z`, `0-9`, `-` and `_`.
pub fn realm_db_path(data: &Path, mod_id: &str) -> Result<PathBuf, Error> {
    check_mod_id(mod_id)?;
    Ok(data.join("realms").join(mod_id).join("realm.sqlite"))
}

fn check_mod_id(mod_id: &str) -> Result<(), Error> {
    let ok = (1..=32).contains(&mod_id.len())
        && mod_id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(Error::Db(format!("bad mod realm id {mod_id:?}")))
    }
}

/// One connection to one mod realm's economy. Several may be open on the
/// same file (one per thread); SQLite serialises their changes.
pub struct Economy {
    conn: Connection,
    mod_id: String,
    config: EconomyConfig,
    registry: Box<dyn ItemRegistry + Send>,
    records: Box<dyn CharacterRecords + Send>,
    searches: SearchLimit,
    faults: Faults,
    /// What committed calls had to tell, until [`Economy::take_notices`].
    notices: Vec<Notice>,
    facets: FacetCache,
}

impl std::fmt::Debug for Economy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Economy")
            .field("mod_id", &self.mod_id)
            .finish_non_exhaustive()
    }
}

/// How long opening waits for other connections setting up the same file,
/// and each change for the write lock.
const BUSY_WAIT: Duration = Duration::from_secs(5);

/// Runs `f` again while SQLite answers busy, for up to [`BUSY_WAIT`]. In a
/// few cases it answers busy without waiting, such as switching a new
/// file to WAL or starting a write while another connection sets the file
/// up.
fn retry_busy<T>(
    conn: &mut Connection,
    mut f: impl FnMut(&mut Connection) -> rusqlite::Result<T>,
) -> rusqlite::Result<T> {
    let until = Instant::now() + BUSY_WAIT;
    loop {
        match f(conn) {
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == ErrorCode::DatabaseBusy && Instant::now() < until =>
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            r => return r,
        }
    }
}

/// Runs the next migration, if any, in its own write transaction. Returns
/// the schema version it found.
fn migrate_step(conn: &mut Connection) -> rusqlite::Result<usize> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let done: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let done = usize::try_from(done).unwrap_or(usize::MAX);
    if let Some(sql) = MIGRATIONS.get(done) {
        tx.execute_batch(sql)?;
        tx.execute_batch(&format!("PRAGMA user_version = {}", done + 1))?;
        tx.commit()?;
    }
    Ok(done)
}

/// Marks a new file as `mod_id`'s. Returns the mod it already belonged to.
fn claim_mod(conn: &mut Connection, mod_id: &str) -> rusqlite::Result<Option<String>> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let owner: Option<String> = tx
        .query_row("SELECT value FROM meta WHERE key = 'mod_id'", [], |r| {
            r.get(0)
        })
        .optional()?;
    if owner.is_none() {
        tx.execute(
            "INSERT INTO meta(key, value) VALUES ('mod_id', ?1)",
            [mod_id],
        )?;
    }
    tx.commit()?;
    Ok(owner)
}

impl Economy {
    /// Opens (creating if needed) the database file of `mod_id`'s economy.
    /// A file that belongs to another mod is refused.
    pub fn open(path: &Path, mod_id: &str, config: EconomyConfig) -> Result<Self, Error> {
        check_mod_id(mod_id)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| Error::Db(format!("{}: {e}", dir.display())))?;
        }
        Self::setup(Connection::open(path)?, mod_id, config)
    }

    /// An economy in memory (tests).
    pub fn in_memory(mod_id: &str, config: EconomyConfig) -> Result<Self, Error> {
        check_mod_id(mod_id)?;
        Self::setup(Connection::open_in_memory()?, mod_id, config)
    }

    fn setup(mut conn: Connection, mod_id: &str, config: EconomyConfig) -> Result<Self, Error> {
        conn.busy_timeout(BUSY_WAIT)?;
        // An in-memory database answers "memory"; a file one "wal".
        retry_busy(&mut conn, |c| {
            c.query_row("PRAGMA journal_mode = WAL", [], |r| r.get::<_, String>(0))
        })?;
        conn.execute_batch("PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;")?;
        // One migration per write transaction, the version read inside it:
        // several connections may open a new file at once.
        let mut ran_5 = false;
        loop {
            let done = retry_busy(&mut conn, migrate_step)?;
            ran_5 |= done == 4;
            if done > MIGRATIONS.len() {
                return Err(Error::Db(format!(
                    "schema version {done} is newer than this build's {}",
                    MIGRATIONS.len()
                )));
            }
            if done == MIGRATIONS.len() {
                break;
            }
        }
        let owner = retry_busy(&mut conn, |c| claim_mod(c, mod_id))?;
        if let Some(m) = owner.filter(|m| m != mod_id) {
            return Err(Error::Db(format!(
                "this database holds the economy of mod {m:?}, not {mod_id:?}"
            )));
        }
        let mut e = Economy {
            conn,
            mod_id: mod_id.to_owned(),
            config,
            registry: Box::new(NoRegistry),
            records: Box::new(NoRecords),
            searches: SearchLimit::default(),
            faults: Faults::default(),
            notices: Vec::new(),
            facets: FacetCache::default(),
        };
        if ran_5 {
            // Migration 5 filled the index from the rows alone; a stack's
            // name and item types come from the realm's stackables.
            e.sweep(0, |a| a.reindex_stacks())?;
        }
        Ok(e)
    }

    /// Plugs in the realm's item registry (N19).
    pub fn with_registry(mut self, registry: Box<dyn ItemRegistry + Send>) -> Self {
        self.registry = registry;
        self
    }

    /// Plugs in the realm's character records (N25), stored in the same
    /// transaction as the moves that change them.
    pub fn with_records(mut self, records: Box<dyn CharacterRecords + Send>) -> Self {
        self.records = records;
        self
    }

    pub fn mod_id(&self) -> &str {
        &self.mod_id
    }

    pub fn config(&self) -> &EconomyConfig {
        &self.config
    }

    /// The connection, for reads.
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Refuses an economy key of another mod.
    pub(crate) fn check_key(&self, key: &EconomyKey) -> Result<(), Error> {
        if key.mod_id == self.mod_id {
            Ok(())
        } else {
            Err(Refusal::EconomyMismatch.into())
        }
    }

    /// Runs one request: in one transaction, answers a repeated `req` from
    /// the stored answer, or runs `f` and stores its answer (success or
    /// refusal) with its changes. A refusal undoes everything `f` wrote; a
    /// database failure undoes everything and stores nothing.
    pub(crate) fn act<F>(&mut self, req: &Req, now: i64, f: F) -> Result<Done, Error>
    where
        F: FnOnce(&Act<'_>) -> Result<Option<i64>, Error>,
    {
        self.act_rows(req, now, |a| f(a).map(|id| (id, Vec::new())))
            .map(|(done, _)| done)
    }

    /// [`Economy::act`] for a request on several rows: `f` answers each
    /// row (see [`Act::row`]) and the rows are stored with the answer.
    pub(crate) fn act_rows<F>(
        &mut self,
        req: &Req,
        now: i64,
        f: F,
    ) -> Result<(Done, Vec<RowDone>), Error>
    where
        F: FnOnce(&Act<'_>) -> Result<(Option<i64>, Vec<RowDone>), Error>,
    {
        let Economy {
            conn,
            config,
            registry,
            records,
            faults,
            mod_id,
            notices,
            facets,
            ..
        } = self;
        let scope = req.scope.to_string();
        // Request numbers are u64 on the wire; SQLite keeps the same bits.
        let n = req.n as i64;
        let mut tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(answer) = requests::find(&tx, &scope, n)? {
            return answer.replay();
        }
        let told = RefCell::new(Vec::new());
        let outcome = {
            let sp = tx.savepoint()?;
            let act = Act {
                conn: &sp,
                config,
                registry: registry.as_ref(),
                records: records.as_ref(),
                mod_id,
                now,
                req: n,
                faults,
                notices: &told,
            };
            match f(&act) {
                Ok(out) => {
                    sp.commit()?;
                    Ok(out)
                }
                // Dropping the savepoint rolls back what `f` wrote, and its
                // notices are dropped with it.
                Err(Error::Refused(r)) => {
                    told.borrow_mut().clear();
                    Err(r)
                }
                Err(e) => return Err(e),
            }
        };
        faults.hit()?;
        let answer = requests::Answer::of(&outcome);
        requests::store(&tx, &scope, n, &answer, now)?;
        faults.hit()?;
        tx.commit()?;
        notices.extend(told.into_inner());
        facets.clear();
        answer.replay().map(|(d, rows)| {
            let done = Done {
                repeated: false,
                ..d
            };
            (done, rows)
        })
    }

    /// Runs `f` in one transaction with no request number: the main
    /// server's own sweeps (expiry, undoing a lost game's transfers).
    pub(crate) fn sweep<T, F>(&mut self, now: i64, f: F) -> Result<T, Error>
    where
        F: FnOnce(&Act<'_>) -> Result<T, Error>,
    {
        let Economy {
            conn,
            config,
            registry,
            records,
            faults,
            mod_id,
            notices,
            facets,
            ..
        } = self;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let told = RefCell::new(Vec::new());
        let act = Act {
            conn: &tx,
            config,
            registry: registry.as_ref(),
            records: records.as_ref(),
            mod_id,
            now,
            req: 0,
            faults,
            notices: &told,
        };
        let out = f(&act)?;
        faults.hit()?;
        tx.commit()?;
        notices.extend(told.into_inner());
        facets.clear();
        Ok(out)
    }

    /// The highest request number the economy holds an answer for in
    /// `scope` (0 for none), so a client numbers its requests above it
    /// (`MarketState.last_req`).
    pub fn last_req(&self, scope: &Scope) -> Result<u64, Error> {
        requests::last(&self.conn, scope)
    }

    /// Forgets stored answers older than the keep time (24 h). Returns how
    /// many were dropped.
    pub fn prune_requests(&mut self, now: i64) -> Result<usize, Error> {
        let before = now - self.config.request_keep_ms;
        Ok(self
            .conn
            .execute("DELETE FROM requests WHERE at < ?1", [before])?)
    }

    pub(crate) fn search_limit(&self) -> &SearchLimit {
        &self.searches
    }

    pub(crate) fn facet_cache(&self) -> &FacetCache {
        &self.facets
    }

    /// The notices committed calls collected, oldest first, emptying the
    /// queue.
    pub(crate) fn drain_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    #[cfg(test)]
    pub(crate) fn faults(&self) -> &Faults {
        &self.faults
    }
}

/// One request's view of the open transaction.
pub(crate) struct Act<'a> {
    pub conn: &'a Connection,
    pub config: &'a EconomyConfig,
    pub registry: &'a dyn ItemRegistry,
    pub records: &'a dyn CharacterRecords,
    pub mod_id: &'a str,
    pub now: i64,
    /// The request number (0 in sweeps), for the transfer log.
    pub req: i64,
    faults: &'a Faults,
    /// What this call has to tell, kept only if it commits.
    pub notices: &'a RefCell<Vec<Notice>>,
}

impl Act<'_> {
    /// One write, followed by a crash point.
    pub fn exec<P: Params>(&self, sql: &str, params: P) -> Result<usize, Error> {
        let n = self.conn.execute(sql, params)?;
        self.faults.hit()?;
        Ok(n)
    }

    /// Runs one row of a request on several rows: a refusal undoes the
    /// row's writes (and drops its notices) and is the row's answer, the
    /// other rows still happen. A database failure fails the request.
    pub fn row<T>(
        &self,
        f: impl FnOnce() -> Result<T, Error>,
    ) -> Result<Result<T, Refusal>, Error> {
        self.conn.execute_batch("SAVEPOINT market_row")?;
        let told = self.notices.borrow().len();
        match f() {
            Ok(v) => {
                self.conn.execute_batch("RELEASE market_row")?;
                Ok(Ok(v))
            }
            Err(Error::Refused(r)) => {
                self.conn
                    .execute_batch("ROLLBACK TO market_row; RELEASE market_row")?;
                self.notices.borrow_mut().truncate(told);
                Ok(Err(r))
            }
            Err(e) => Err(e),
        }
    }

    /// Refuses a request made under other fees than the realm's
    /// ([`EconomyConfig::fee_rev`]).
    pub fn check_fee_rev(&self, seen: u32) -> Result<(), Error> {
        if seen == self.config.fee_rev() {
            Ok(())
        } else {
            Err(Refusal::FeeChanged.into())
        }
    }

    /// A crash point between steps that aren't a single write.
    pub fn step(&self) -> Result<(), Error> {
        self.faults.hit()
    }

    /// Refuses an economy key of another mod.
    pub fn check_key(&self, key: &EconomyKey) -> Result<(), Error> {
        if key.mod_id == self.mod_id {
            Ok(())
        } else {
            Err(Refusal::EconomyMismatch.into())
        }
    }
}

/// Crash injection for the tests: every write inside a request is a
/// numbered point, and a test can make the process die (or unwind) at any
/// one of them. Outside tests it does nothing.
#[derive(Debug, Default)]
pub(crate) struct Faults {
    #[cfg(test)]
    pub crash_at: std::cell::Cell<Option<u32>>,
    #[cfg(test)]
    pub seen: std::cell::Cell<u32>,
    /// Abort the process (no unwinding, no rollback) instead of panicking.
    #[cfg(test)]
    pub abort: std::cell::Cell<bool>,
}

impl Faults {
    fn hit(&self) -> Result<(), Error> {
        #[cfg(test)]
        {
            let n = self.seen.get();
            self.seen.set(n + 1);
            if self.crash_at.get() == Some(n) {
                if self.abort.get() {
                    std::process::abort();
                }
                panic!("injected crash at point {n}");
            }
        }
        Ok(())
    }
}
