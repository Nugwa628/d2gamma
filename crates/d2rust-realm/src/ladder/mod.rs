//! The realm's ladder (lobby contract section 6; plan `lobby-ui.md` L6):
//! seasons, each character's ladder row, the rank tables and the
//! `LADDER` requests (kinds 0x50-0x57).
//!
//! - **Seasons:** `season-start NAME [test]` opens a season (ending the
//!   open one), `season-end` ends it ([`Ladder::season_start`],
//!   [`Ladder::season_end`]). A ladder character is created in the open
//!   season ([`Ladder::current_season`]); a non-ladder one is in season 0,
//!   which has no row and is always listed. With `ladder = off` only
//!   season 0 is listed and no ladder character can be made.
//! - **Season end** ([`season_end`]): an ended season keeps its rows as
//!   its final ranks; its characters become non-ladder when next loaded
//!   for a game.
//! - **Rows:** after every stored save the character's row is written
//!   ([`save_hook`]); a deleted character leaves every season
//!   ([`Ladder::deleted`]); `ladder-seed` writes a row and the summary as a
//!   save would ([`Ladder::seed`], test setups only).
//! - **Ranks:** each (season, split) table is read from the store and kept
//!   for `ladder_refresh_s` ([`ranks`]); a class view ranks that class
//!   alone.
//! - **Requests:** `LadderSeasons` and `LadderQuery`, 30 a minute per
//!   account together, answered with their rows and `LadderEnd`. They need
//!   a logged-in account, not a selected character.
//!
//! Store calls run off the network threads ([`Accounts::store`]).

pub mod ranks;
pub mod save_hook;
pub mod season_end;

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use d2_accounts::ladder::{Season, Split, NON_LADDER};
use d2_accounts::social::SocialResult;
use d2_accounts::{rules, summary, AccountId, Accounts, LimitMode};
use d2_formats::d2s::status;
use d2_limits::{Limiter, Rate};
use d2_net::msg::ladder::{
    Ask, FromRealm, LadderEnd, LadderHead, LadderQuery, LadderRow, SeasonRow, ToRealm,
    MAX_SEASON_NAME,
};
use d2_net::msg::ErrorCode;
use d2_save_checks::CheckTables;
use tokio::sync::mpsc;

use crate::denied::Denied;
use crate::log::ServerLog;
use crate::session::ControlTx;
use ranks::{Page, Table};

/// How often the rank tables are computed again, at most, by default.
pub const DEFAULT_REFRESH_S: u32 = 60;

/// `LadderSeasons` and `LadderQuery` per account and minute, together.
pub const QUERIES_PER_MIN: u32 = 30;

/// The name season 0 is listed with.
pub const NON_LADDER_NAME: &str = "Non-ladder";

/// The ladder's config keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LadderConfig {
    /// `ladder = on`: seasons run.
    pub on: bool,
    /// `ladder_refresh_s`: how long a rank table is kept.
    pub refresh: Duration,
}

impl Default for LadderConfig {
    fn default() -> Self {
        Self {
            on: false,
            refresh: Duration::from_secs(DEFAULT_REFRESH_S.into()),
        }
    }
}

/// Who asks: the session's account, its selected character and where its
/// answers go.
#[derive(Clone, Debug)]
pub struct Asker {
    pub account: AccountId,
    pub character: Option<String>,
    pub out: ControlTx,
}

/// Whether the character (account, name) plays now: in a game, or
/// selected by its account's session.
pub type Online<'a> = &'a (dyn Fn(i64, &str) -> bool + Sync);

/// A rank table and when it was read.
struct Cached {
    table: Arc<Table>,
    read: Instant,
}

/// The realm's ladder of one game (mod).
pub struct Ladder {
    accounts: Accounts,
    game: String,
    cfg: LadderConfig,
    /// The save tables the records' experience is read with; without them
    /// no row is written after a save.
    tables: Option<Arc<CheckTables>>,
    cache: Mutex<HashMap<(u16, u8), Cached>>,
    queries: Mutex<Limiter<i64>>,
    saves: mpsc::UnboundedSender<save_hook::Saved>,
    saves_rx: Mutex<Option<mpsc::UnboundedReceiver<save_hook::Saved>>>,
    log: Arc<ServerLog>,
}

impl std::fmt::Debug for Ladder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ladder")
            .field("game", &self.game)
            .field("cfg", &self.cfg)
            .finish_non_exhaustive()
    }
}

/// Unix milliseconds now.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn ms(t: i64) -> u64 {
    u64::try_from(t).unwrap_or(0)
}

impl Ladder {
    /// The ladder of `game` on `accounts`' store; `tables` read the saved
    /// records' experience.
    pub fn new(
        accounts: Accounts,
        game: &str,
        cfg: LadderConfig,
        tables: Option<Arc<CheckTables>>,
        log: Arc<ServerLog>,
    ) -> Arc<Self> {
        let mut rate = Rate::per_secs(QUERIES_PER_MIN, 60);
        if accounts.config().limits == LimitMode::Test {
            rate = rate.test();
        }
        let (saves, rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            accounts,
            game: game.to_owned(),
            cfg,
            tables,
            cache: Mutex::new(HashMap::new()),
            queries: Mutex::new(Limiter::new(rate)),
            saves,
            saves_rx: Mutex::new(Some(rx)),
            log,
        })
    }

    pub fn is_on(&self) -> bool {
        self.cfg.on
    }

    fn forget_ranks(&self) {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    // ------------------------------------------------------------ seasons

    /// The open season, if the ladder is on and one is open: where a new
    /// ladder character goes.
    pub async fn current_season(&self) -> Result<Option<u16>, Denied> {
        if !self.cfg.on {
            return Ok(None);
        }
        let game = self.game.clone();
        let s = self
            .accounts
            .store(move |db| Ok(db.ladder().current(&game)?))
            .await?;
        Ok(s.map(|s| s.id))
    }

    /// `season-start NAME [test]`: opens the next season, ending the open
    /// one.
    pub async fn season_start(&self, name: &str, test: bool) -> Result<String, String> {
        if !self.cfg.on {
            return Err("the ladder is off (`ladder = on` in the config)".into());
        }
        if name.is_empty() || name.len() > MAX_SEASON_NAME || name.chars().any(char::is_control) {
            return Err(format!(
                "a season name is 1-{MAX_SEASON_NAME} printable bytes"
            ));
        }
        let (game, name) = (self.game.clone(), name.to_owned());
        let s = self
            .accounts
            .store(move |db| Ok(db.ladder().start_season(&game, &name, test, now_ms())?))
            .await
            .map_err(|d| d.text)?;
        self.forget_ranks();
        let what = format!(
            "season {} `{}`{} started",
            s.id,
            s.name,
            if test { " (test)" } else { "" }
        );
        self.log.line(&format!("ladder: {what}"));
        Ok(what)
    }

    /// `season-end`: ends the open season. Its rows stay, browsable, as
    /// its final ranks; its characters become non-ladder when next loaded
    /// ([`season_end`]).
    pub async fn season_end(&self) -> Result<String, String> {
        let game = self.game.clone();
        let (s, ranked) = self
            .accounts
            .store(move |db| {
                let Some(s) = db.ladder().end_season(&game, now_ms())? else {
                    return Ok(None);
                };
                let ranked = season_end::ranked(db, &game, s.id)?;
                Ok(Some((s, ranked)))
            })
            .await
            .map_err(|d| d.text)?
            .ok_or("no season is open")?;
        self.forget_ranks();
        let what = format!(
            "season {} `{}` ended; its final ranks stay ({ranked} ranked characters); \
             its characters become non-ladder when next loaded",
            s.id, s.name
        );
        self.log.line(&format!("ladder: {what}"));
        Ok(what)
    }

    /// The seasons as listed: newest first, then season 0. With the ladder
    /// off, season 0 alone.
    async fn season_rows(&self) -> Result<Vec<SeasonRow>, Denied> {
        let seasons: Vec<Season> = if self.cfg.on {
            let game = self.game.clone();
            self.accounts
                .store(move |db| Ok(db.ladder().seasons(&game)?))
                .await?
        } else {
            Vec::new()
        };
        let open = seasons.iter().any(|s| s.ended_ms.is_none());
        let mut rows: Vec<SeasonRow> = seasons
            .into_iter()
            .map(|s| SeasonRow {
                season: s.id,
                name: s.name,
                started_ms: ms(s.started_ms),
                ended_ms: s.ended_ms.map_or(0, ms),
                test: s.test,
                current: s.ended_ms.is_none(),
            })
            .collect();
        rows.push(SeasonRow {
            season: NON_LADDER,
            name: NON_LADDER_NAME.into(),
            started_ms: 0,
            ended_ms: 0,
            test: false,
            current: !open,
        });
        Ok(rows)
    }

    // --------------------------------------------------------------- rows

    /// A save of `account`'s character `name` was stored: its row is
    /// written by [`Ladder::run_saves`].
    pub fn saved(&self, account: AccountId, name: &str) {
        let _ = self.saves.send((account.0, name.to_owned()));
    }

    /// Writes the rows of the stored saves, in order, until the ladder is
    /// gone. The server runs it once.
    pub async fn run_saves(self: Arc<Self>) {
        let rx = self
            .saves_rx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let Some(mut rx) = rx else {
            return;
        };
        while let Some((account, name)) = rx.recv().await {
            if let Err(e) = self.write_row(AccountId(account), &name).await {
                self.log
                    .line(&format!("ladder: `{name}`'s row after its save: {e}"));
            }
        }
    }

    /// The row of `name` from its stored record.
    async fn write_row(&self, account: AccountId, name: &str) -> Result<(), String> {
        let Some(tables) = self.tables.clone() else {
            return Ok(());
        };
        let seat = self
            .accounts
            .seat(account, name)
            .await
            .map_err(|d| d.text)?;
        let Some(record) = seat.record else {
            return Ok(());
        };
        // An ended season's ranks are final: a save of one of its
        // characters (still in a game when it ended) doesn't change them.
        if self.frozen(seat.summary.season).await.map_err(|d| d.text)? {
            return Ok(());
        }
        let expansion = seat.summary.status & status::EXPANSION != 0;
        let experience = tokio::task::spawn_blocking(move || {
            summary::experience(&record, &tables.reading(expansion))
        })
        .await
        .map_err(|e| e.to_string())??;
        let entry = save_hook::entry_of(account.0, &seat.summary, experience, now_ms());
        let game = self.game.clone();
        self.accounts
            .store(move |db| Ok(db.ladder().upsert(&game, &entry)?))
            .await
            .map_err(|d| d.text)
    }

    /// A character was deleted: it leaves every season's ladder.
    pub async fn deleted(&self, name: &str) {
        let (game, name) = (self.game.clone(), name.to_owned());
        if let Err(d) = self
            .accounts
            .store(move |db| Ok(db.ladder().remove(&game, &name)?))
            .await
        {
            self.log
                .line(&format!("ladder: removing a row: {}", d.text));
        }
        self.forget_ranks();
    }

    /// `ladder-seed CHARACTER LEVEL EXPERIENCE [dead]` (test setups): the
    /// character's row and summary as a save of that level and experience
    /// would write them (`dead`: a hardcore character that died).
    pub async fn seed(
        &self,
        character: &str,
        level: u8,
        experience: u64,
        dead: bool,
    ) -> Result<String, String> {
        let (game, key) = (self.game.clone(), rules::name_key(character));
        let seeded = self
            .accounts
            .store(move |db| -> SocialResult<Option<(u16, String)>> {
                let Some(row) = db.character(&key)?.filter(|r| r.deleted.is_none()) else {
                    return Ok(None);
                };
                let mut s = row.summary;
                s.level = level;
                if dead {
                    s.status |= status::DEAD;
                }
                db.save_summary(row.account_id, &s)?;
                let entry = save_hook::entry_of(row.account_id, &s, experience, now_ms());
                db.ladder().upsert(&game, &entry)?;
                Ok(Some((s.season, s.name)))
            })
            .await
            .map_err(|d| d.text)?;
        let (season, name) = seeded.ok_or_else(|| format!("no character named `{character}`"))?;
        self.forget_ranks();
        Ok(format!(
            "{name}: level {level}, experience {experience} in season {season}"
        ))
    }

    // ------------------------------------------------------------ queries

    /// The (season, split) table, read again when older than
    /// `ladder_refresh_s`.
    async fn table(&self, season: u16, split: Split) -> Result<Arc<Table>, Denied> {
        let key = (season, split.bits());
        {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(c) = cache
                .get(&key)
                .filter(|c| c.read.elapsed() < self.cfg.refresh)
            {
                return Ok(c.table.clone());
            }
        }
        let game = self.game.clone();
        let rows = self
            .accounts
            .store(move |db| Ok(db.ladder().split(&game, season, split)?))
            .await?;
        let table = Arc::new(Table::new(rows, ms(now_ms())));
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).insert(
            key,
            Cached {
                table: table.clone(),
                read: Instant::now(),
            },
        );
        Ok(table)
    }

    /// Whether `season` can be asked for.
    async fn season_exists(&self, season: u16) -> Result<bool, Denied> {
        if season == NON_LADDER {
            return Ok(true);
        }
        if !self.cfg.on {
            return Ok(false);
        }
        let game = self.game.clone();
        Ok(self
            .accounts
            .store(move |db| Ok(db.ladder().season(&game, season)?))
            .await?
            .is_some())
    }

    /// One ladder request of `who`, answered on its control stream.
    pub async fn request(&self, who: &Asker, msg: ToRealm, online: Online<'_>) {
        let req = msg.req;
        let end =
            |code: ErrorCode, wait_s: u32| FromRealm::End(LadderEnd { req, code, wait_s }).body();
        let limited = {
            let mut l = self.queries.lock().unwrap_or_else(|e| e.into_inner());
            l.check(&who.account.0, Instant::now()).err()
        };
        if let Some(r) = limited {
            who.out.send(end(ErrorCode::RATE_LIMITED, r.wait_s()));
            return;
        }
        let answer = match msg.ask {
            Ask::Seasons => self.seasons(req).await,
            Ask::Query(q) => self.query(who, req, &q, online).await,
        };
        match answer {
            Ok(bodies) => {
                for b in bodies {
                    who.out.send(b);
                }
                who.out.send(end(ErrorCode(0), 0));
            }
            Err(d) => who.out.send(end(d.code, d.wait_s)),
        }
    }

    async fn seasons(&self, req: u32) -> Result<Vec<Vec<u8>>, Denied> {
        Ok(self
            .season_rows()
            .await?
            .into_iter()
            .map(|row| FromRealm::SeasonRow { req, row }.body())
            .collect())
    }

    async fn query(
        &self,
        who: &Asker,
        req: u32,
        q: &LadderQuery,
        online: Online<'_>,
    ) -> Result<Vec<Vec<u8>>, Denied> {
        if !self.season_exists(q.season).await? {
            return Err(ErrorCode::NO_SUCH_SEASON.into());
        }
        let table = self.table(q.season, Split::from_bits(q.split)).await?;
        let me = who.character.as_deref();
        let page = table.page(q, me).map_err(Denied::new)?;
        let tags = self.tags(&page).await?;
        let row = |r: &ranks::Ranked, pinned: bool| {
            let e = r.entry;
            let mut flags = ranks::flags(e, who.account.0, me, online(e.account, &e.name));
            if pinned {
                flags |= d2_net::msg::ladder::row_flags::PINNED;
            }
            FromRealm::Row {
                req,
                row: LadderRow {
                    rank: r.rank,
                    char: e.name.clone(),
                    class: e.class,
                    level: e.level,
                    experience: e.experience,
                    tag: tags.get(&e.account).cloned(),
                    flags,
                },
            }
            .body()
        };
        let mut out = vec![FromRealm::Head(LadderHead {
            req,
            season: q.season,
            split: q.split,
            class: q.class,
            total: page.total,
            first_rank: page.first_rank,
            rows: u8::try_from(page.rows.len()).unwrap_or(u8::MAX),
            my_rank: page.my_rank,
            updated_ms: table.updated_ms,
            now_ms: ms(now_ms()),
        })
        .body()];
        out.extend(page.rows.iter().map(|r| row(r, false)));
        out.extend(page.pinned.iter().map(|r| row(r, true)));
        Ok(out)
    }

    /// The guild tags of the accounts on `page`.
    async fn tags(&self, page: &Page<'_>) -> Result<BTreeMap<i64, String>, Denied> {
        let accounts: Vec<i64> = page
            .rows
            .iter()
            .chain(&page.pinned)
            .map(|r| r.entry.account)
            .collect();
        self.accounts
            .store(move |db| {
                let mut tags = BTreeMap::new();
                for a in accounts {
                    if tags.contains_key(&a) {
                        continue;
                    }
                    if let Some(g) = db.guilds().of(a)? {
                        tags.insert(a, g.tag);
                    }
                }
                Ok(tags)
            })
            .await
    }
}

#[cfg(test)]
mod tests;
