//! Seat leases and the save desk (plan A3, contracts 11.6): the main
//! server owns every character's save lifecycle, so a closed, crashed or
//! orphaned game can't write a character again.
//!
//! - [`LeaseTable`]: the live leases. A character takes a seat in a game
//!   on a game server: the main server grants a [`Lease`] (at most one
//!   live lease per character, else `CharInGame`). The lease ends on
//!   `Left`, `GameClosed`, `CloseGame`, a session takeover (`Kick`) or the
//!   game server's loss.
//! - [`SaveDesk::accept`]: a game server's save is stored only while its
//!   lease is live and is that game server's, for that game, with a `seq`
//!   higher than the lease's last stored one, and its record reads as the
//!   leased character's. Anything else is refused (`LeaseRevoked`,
//!   `SaveInvalid`) and logged, and nothing is written. With a
//!   [`SaveGuard`] the record then goes through the save checks and the
//!   item registry ([`super::guard`]). A realm whose records are in its
//!   game's own format (`records = game`, [`d2_accounts::RecordFormat`])
//!   skips those `.d2s` steps and takes the game server's summary.
//! - [`SavePairs`]: a `Save` message and its record arrive apart (the
//!   record on its own stream); they are paired by lease and `seq`. So
//!   are a stash transfer's `ItemOut` or placed `ItemInDone` and the
//!   record of the save that goes with it ([`TransferHalf`]).
//! - [`SaveDesk::seat`]: a seat's stored record, loaded for the game
//!   server, goes through the load hook first ([`SeatLoad`]: the ladder
//!   makes a character of an ended season non-ladder there).

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use d2_accounts::rules::name_key;
use d2_accounts::{AccountId, Accounts, CharSeat};
use d2_link::Lease;
use d2_net::msg::account::CharSummary;
use d2_net::msg::node::SaveInfo;
use d2_net::msg::transfer::{ItemInDone, ItemOut};
use d2_net::msg::ErrorCode;

use super::guard::SaveGuard;
use crate::denied::Denied;
use crate::log::ServerLog;

/// Who holds a lease: a character's seat in one game on one game server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeaseGrant {
    pub account: AccountId,
    /// The character's name as stored.
    pub character: String,
    pub game: u32,
    /// The game server's id.
    pub node: String,
}

/// Why a lease ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseEnd {
    /// The player left, after its last save (`Left`).
    Left,
    /// The game closed (`GameClosed`).
    GameClosed,
    /// The main server closed the game (`CloseGame`).
    Closed,
    /// The account logged in again elsewhere (`Kick`).
    Takeover,
    /// The game server was lost.
    NodeLost,
    /// The seat was never taken (`SeatRefused`).
    Refused,
}

impl std::fmt::Display for LeaseEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Left => "the player left",
            Self::GameClosed => "the game closed",
            Self::Closed => "the main server closed the game",
            Self::Takeover => "the account logged in again elsewhere",
            Self::NodeLost => "the game server was lost",
            Self::Refused => "the seat was refused",
        })
    }
}

/// A live lease: whose, and the `seq` of its last stored save (0 before
/// the first).
#[derive(Clone, Debug)]
struct Live {
    grant: LeaseGrant,
    seq: u32,
}

/// How many ended leases are remembered, to say why a late save is
/// refused.
const ENDED_KEPT: usize = 4096;

/// The live leases (contracts 11.6).
#[derive(Debug)]
pub struct LeaseTable {
    next: u64,
    live: BTreeMap<Lease, Live>,
    /// A character's live lease, by its name key.
    by_character: BTreeMap<String, Lease>,
    /// The last [`ENDED_KEPT`] ended leases and why they ended.
    ended: BTreeMap<Lease, LeaseEnd>,
}

impl LeaseTable {
    /// A table whose first lease is `first`.
    pub fn new(first: u64) -> Self {
        Self {
            next: first.max(1),
            live: BTreeMap::new(),
            by_character: BTreeMap::new(),
            ended: BTreeMap::new(),
        }
    }

    /// A table for a main server starting now. Its leases start at the
    /// clock in milliseconds times 1024, above every lease of an earlier
    /// run that granted fewer than 1024 a millisecond: leases are never
    /// reused, even across restarts.
    pub fn starting_now() -> Self {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        Self::new(ms.saturating_mul(1024))
    }

    /// Grants a lease for `grant`. Refused with `CharInGame` while the
    /// character holds another live lease.
    pub fn grant(&mut self, grant: LeaseGrant) -> Result<Lease, Denied> {
        let key = name_key(&grant.character);
        if self.by_character.contains_key(&key) {
            return Err(Denied::new(ErrorCode::CHAR_IN_GAME));
        }
        let lease = Lease(self.next);
        self.next += 1;
        self.by_character.insert(key, lease);
        self.live.insert(lease, Live { grant, seq: 0 });
        Ok(lease)
    }

    /// Who holds `lease`, while it is live.
    pub fn get(&self, lease: Lease) -> Option<&LeaseGrant> {
        self.live.get(&lease).map(|l| &l.grant)
    }

    /// The character's live lease.
    pub fn of_character(&self, character: &str) -> Option<Lease> {
        self.by_character.get(&name_key(character)).copied()
    }

    /// Whether game server `node` holds a live lease in game `game`.
    pub fn holds_game(&self, node: &str, game: u32) -> bool {
        self.live
            .values()
            .any(|l| l.grant.node == node && l.grant.game == game)
    }

    /// How many leases are live.
    pub fn len(&self) -> usize {
        self.live.len()
    }

    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    /// Ends `lease` because of `why`; returns who held it (`None` when it
    /// wasn't live).
    pub fn end(&mut self, lease: Lease, why: LeaseEnd) -> Option<LeaseGrant> {
        let live = self.live.remove(&lease)?;
        self.by_character.remove(&name_key(&live.grant.character));
        self.ended.insert(lease, why);
        while self.ended.len() > ENDED_KEPT {
            self.ended.pop_first();
        }
        Some(live.grant)
    }

    /// Ends every lease that matches `which`.
    fn end_where(
        &mut self,
        why: LeaseEnd,
        which: impl Fn(&LeaseGrant) -> bool,
    ) -> Vec<(Lease, LeaseGrant)> {
        let leases: Vec<Lease> = self
            .live
            .iter()
            .filter(|(_, l)| which(&l.grant))
            .map(|(k, _)| *k)
            .collect();
        leases
            .into_iter()
            .filter_map(|k| self.end(k, why).map(|g| (k, g)))
            .collect()
    }

    /// Ends every lease of game `game` on game server `node`.
    pub fn end_game(&mut self, node: &str, game: u32, why: LeaseEnd) -> Vec<(Lease, LeaseGrant)> {
        self.end_where(why, |g| g.node == node && g.game == game)
    }

    /// Ends every lease on game server `node`.
    pub fn end_node(&mut self, node: &str, why: LeaseEnd) -> Vec<(Lease, LeaseGrant)> {
        self.end_where(why, |g| g.node == node)
    }

    /// Ends every lease of account `a` (a session takeover: the account
    /// logged in again elsewhere).
    pub fn end_account(&mut self, a: AccountId, why: LeaseEnd) -> Vec<(Lease, LeaseGrant)> {
        self.end_where(why, |g| g.account == a)
    }

    /// Whether `from` may store `save` now: the lease is live, `from`'s,
    /// for that game, and `seq` rises. On success the lease's `seq` is
    /// taken, so a save handed in twice is stored once.
    pub(crate) fn take(&mut self, from: &str, save: &SaveInfo) -> Result<LeaseGrant, Denied> {
        let lease = Lease(save.lease);
        let Some(live) = self.live.get_mut(&lease) else {
            let why = match self.ended.get(&lease) {
                Some(end) => format!("{lease} ended: {end}"),
                None => format!("{lease} is unknown"),
            };
            return Err(Denied::because(ErrorCode::LEASE_REVOKED, &why));
        };
        if live.grant.node != from {
            return Err(Denied::because(
                ErrorCode::LEASE_REVOKED,
                &format!("{lease} is game server {}'s", live.grant.node),
            ));
        }
        if live.grant.game != save.game {
            return Err(Denied::because(
                ErrorCode::LEASE_REVOKED,
                &format!("{lease} is for game {}", live.grant.game),
            ));
        }
        if save.seq <= live.seq {
            return Err(Denied::because(
                ErrorCode::SAVE_INVALID,
                &format!("save {} after save {}", save.seq, live.seq),
            ));
        }
        live.seq = save.seq;
        Ok(live.grant.clone())
    }
}

/// A game server's save: the `Save` message and its record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeSave {
    pub info: SaveInfo,
    pub record: Vec<u8>,
}

/// `SaveAck`'s code for a stored save (contracts 11.3).
pub const STORED: ErrorCode = ErrorCode(0);

/// A stored save's answer (`SaveAck` with code [`STORED`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveAck {
    pub lease: Lease,
    pub seq: u32,
}

/// A stored save: its answer, and whose character it was with the
/// character's new summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    pub ack: SaveAck,
    pub account: AccountId,
    pub summary: CharSummary,
}

/// Told of every lease that ends (the online stash undoes its transfers).
#[derive(Clone)]
pub struct LeaseEnded(pub Arc<dyn Fn(Lease) + Send + Sync>);

impl std::fmt::Debug for LeaseEnded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LeaseEnded")
    }
}

/// What [`SeatLoad::loaded`] returns.
pub type SeatLoading<'a> = Pin<Box<dyn Future<Output = Result<(), Denied>> + Send + 'a>>;

/// Sees every seat's stored record after it is loaded and before it goes
/// to the game server, and may change it (and the stored character with
/// it). A refusal refuses the seat.
pub trait SeatLoad: Send + Sync {
    fn loaded<'a>(&'a self, account: AccountId, seat: &'a mut CharSeat) -> SeatLoading<'a>;
}

/// The load hook a [`SaveDesk`] runs ([`SaveDesk::on_seat_load`]).
#[derive(Clone)]
pub struct SeatLoaded(pub Arc<dyn SeatLoad>);

impl std::fmt::Debug for SeatLoaded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SeatLoaded")
    }
}

/// Where game servers' saves are checked and stored.
#[derive(Debug)]
pub struct SaveDesk {
    leases: Mutex<LeaseTable>,
    accounts: Accounts,
    guard: Option<Arc<SaveGuard>>,
    on_end: Option<LeaseEnded>,
    on_load: Option<SeatLoaded>,
    log: Arc<ServerLog>,
}

impl SaveDesk {
    pub fn new(leases: LeaseTable, accounts: Accounts, log: Arc<ServerLog>) -> Self {
        Self {
            leases: Mutex::new(leases),
            accounts,
            guard: None,
            on_end: None,
            on_load: None,
            log,
        }
    }

    /// `f` is told of every lease that ends, after it ended.
    pub fn on_lease_end(&mut self, f: LeaseEnded) {
        self.on_end = Some(f);
    }

    /// Every seat's stored record goes through `hook` ([`SaveDesk::seat`]).
    pub fn on_seat_load(mut self, hook: SeatLoaded) -> Self {
        self.on_load = Some(hook);
        self
    }

    /// What a game needs to seat the account's character `name`
    /// ([`Accounts::seat`]), after the load hook.
    pub async fn seat(&self, a: AccountId, name: &str) -> Result<CharSeat, Denied> {
        let mut seat = self.accounts.seat(a, name).await?;
        if let Some(hook) = &self.on_load {
            hook.0.loaded(a, &mut seat).await?;
        }
        Ok(seat)
    }

    /// The accounts the saves are stored to.
    pub fn accounts(&self) -> &Accounts {
        &self.accounts
    }

    /// Every save also goes through `guard` (the save checks and the item
    /// registry), and leases that end tell its registry.
    pub fn guarded(mut self, guard: Arc<SaveGuard>) -> Self {
        self.guard = Some(guard);
        self
    }

    /// The lease table (grant, end).
    pub fn leases(&self) -> MutexGuard<'_, LeaseTable> {
        self.leases.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Ends `lease` if it is `from`'s, and logs it; who held it.
    pub fn end(&self, from: &str, lease: Lease, why: LeaseEnd) -> Option<LeaseGrant> {
        let mut t = self.leases();
        if !t.get(lease).is_some_and(|g| g.node == from) {
            return None;
        }
        let g = t.end(lease, why)?;
        drop(t);
        self.ended(lease, &g, why);
        if let (Some(guard), LeaseEnd::Left) = (&self.guard, why) {
            guard.left(g.game, &g.character);
        }
        Some(g)
    }

    /// Ends every lease of `from`'s game `game`, and logs them. The game
    /// is over.
    pub fn end_game(&self, from: &str, game: u32, why: LeaseEnd) {
        let ended = self.leases().end_game(from, game, why);
        for (lease, g) in ended {
            self.ended(lease, &g, why);
        }
        if let Some(guard) = &self.guard {
            guard.close_game(game);
        }
    }

    /// Ends every lease of account `a`, and logs them; the ended ones.
    pub fn end_account(&self, a: AccountId, why: LeaseEnd) -> Vec<(Lease, LeaseGrant)> {
        let ended = self.leases().end_account(a, why);
        for (lease, g) in &ended {
            self.ended(*lease, g, why);
        }
        ended
    }

    /// Ends `lease` wherever it is, and logs it; who held it.
    pub fn end_any(&self, lease: Lease, why: LeaseEnd) -> Option<LeaseGrant> {
        let g = self.leases().end(lease, why)?;
        self.ended(lease, &g, why);
        Some(g)
    }

    /// Ends every lease on `from`, and logs them. Their games are over.
    pub fn end_node(&self, from: &str, why: LeaseEnd) {
        let ended = self.leases().end_node(from, why);
        let mut games: Vec<u32> = ended.iter().map(|(_, g)| g.game).collect();
        for (lease, g) in ended {
            self.ended(lease, &g, why);
        }
        games.sort_unstable();
        games.dedup();
        if let Some(guard) = &self.guard {
            for game in games {
                guard.close_game(game);
            }
        }
    }

    fn ended(&self, lease: Lease, g: &LeaseGrant, why: LeaseEnd) {
        self.log.line(&format!(
            "{lease} of `{}` (game {} on node {}) ended: {why}",
            g.character, g.game, g.node
        ));
        if let Some(f) = &self.on_end {
            (f.0)(lease);
        }
    }

    /// [`LeaseTable::take`] for `from`'s save `info`, logged when refused.
    pub(crate) fn take_lease(&self, from: &str, info: &SaveInfo) -> Result<LeaseGrant, Denied> {
        let taken = self.leases().take(from, info);
        if let Err(d) = &taken {
            self.log.line(&format!(
                "save {} of {} (`{}`, game {}) from node {from} refused: {}: {}",
                info.seq,
                Lease(info.lease),
                info.summary.name,
                info.game,
                d.code,
                d.text
            ));
        }
        taken
    }

    /// Stores game server `from`'s save if its lease allows it (see the
    /// module notes). A refusal is logged; nothing is written then.
    pub async fn accept(&self, from: &str, save: NodeSave) -> Result<Stored, Denied> {
        let (lease, seq) = (Lease(save.info.lease), save.info.seq);
        let taken = self.leases().take(from, &save.info);
        let stored = match taken {
            Ok(grant) => match self.guard(&grant, &save).await {
                Ok(()) => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs());
                    // A realm of the game's own records takes the game
                    // server's summary ([`d2_accounts::RecordFormat`]).
                    self.accounts
                        .store_played(
                            grant.account,
                            &grant.character,
                            save.record,
                            &save.info.summary,
                            now,
                        )
                        .await
                        .map(|summary| (grant.account, summary))
                }
                Err(d) => Err(d),
            },
            Err(d) => Err(d),
        };
        match stored {
            Ok((account, summary)) => Ok(Stored {
                ack: SaveAck { lease, seq },
                account,
                summary,
            }),
            Err(d) => {
                self.log.line(&format!(
                    "save {seq} of {lease} (`{}`, game {}) from node {from} refused: {}: {}",
                    save.info.summary.name, save.info.game, d.code, d.text
                ));
                Err(d)
            }
        }
    }
}

impl SaveDesk {
    /// Whether the item registry knows item ID `id` of `game` (see
    /// [`SaveGuard::knows`]); true without a guard.
    pub(crate) async fn knows_item(&self, game: u32, id: u32) -> Result<bool, String> {
        let Some(guard) = self.guard.clone() else {
            return Ok(true);
        };
        tokio::task::spawn_blocking(move || guard.knows(game, id))
            .await
            .unwrap_or_else(|e| Err(format!("task failed: {e}")))
    }

    /// The guard's judgement of a leased save: flagged findings are
    /// logged; a refusal is `SaveInvalid`. A realm whose records are in its
    /// game's own format can't read them: the save checks and the item
    /// registry (`.d2s` only) skip.
    pub(crate) async fn guard(&self, grant: &LeaseGrant, save: &NodeSave) -> Result<(), Denied> {
        let Some(guard) = self.guard.clone() else {
            return Ok(());
        };
        if !self.accounts.config().records.is_d2s() {
            return Ok(());
        }
        let path = self
            .accounts
            .character_file(grant.account, &grant.character)?;
        let (character, game, after) = (grant.character.clone(), grant.game, save.record.clone());
        let j = tokio::task::spawn_blocking(move || {
            let before = load_record(&character, &path)?;
            Ok::<_, String>(guard.judge(game, &character, before.as_deref(), &after))
        })
        .await
        .unwrap_or_else(|e| Err(format!("task failed: {e}")))
        .map_err(|e| {
            self.log
                .line(&format!("`{}`: the stored record: {e}", grant.character));
            Denied::new(ErrorCode::OTHER)
        })?;
        let (lease, seq) = (Lease(save.info.lease), save.info.seq);
        for f in &j.flagged {
            self.log.line(&format!(
                "save {seq} of {lease} (`{}`, game {}) flagged: {f}",
                grant.character, grant.game
            ));
        }
        match j.refused {
            Some(why) => Err(Denied::because(ErrorCode::SAVE_INVALID, &why)),
            None => Ok(()),
        }
    }
}

/// The stored record of `character` at `path` (`None`: never saved).
fn load_record(character: &str, path: &std::path::Path) -> Result<Option<Vec<u8>>, String> {
    d2_accounts::characters::load_record(character, path).map_err(|e| e.to_string())
}

/// How long half a save waits for its other half.
pub const PAIR_WAIT: Duration = Duration::from_secs(10);

/// The most halves waiting on one link; past it the oldest is dropped.
pub const PAIRS_MAX: usize = 64;

/// A stash transfer's message that goes with a save: its record comes on
/// its own stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferHalf {
    /// Goods taken off the character, with its record without them.
    Out(Box<ItemOut>),
    /// Goods placed on the character, with its record holding them.
    In(ItemInDone),
}

impl TransferHalf {
    /// The save's (lease, seq).
    pub fn key(&self) -> Option<(u64, u32)> {
        let info = match self {
            Self::Out(o) => &o.save,
            Self::In(d) => d.save.as_ref()?,
        };
        Some((info.lease, info.seq))
    }

    /// The transfer's (lease, req).
    pub fn transfer(&self) -> (u64, u64) {
        match self {
            Self::Out(o) => (o.save.lease, o.req),
            Self::In(d) => (d.lease, d.req),
        }
    }
}

/// Pairs a link's `Save` messages with their records, by lease and `seq`.
#[derive(Debug, Default)]
pub struct SavePairs {
    infos: BTreeMap<(u64, u32), (SaveInfo, Instant)>,
    records: BTreeMap<(u64, u32), (Vec<u8>, Instant)>,
    transfers: BTreeMap<(u64, u32), (TransferHalf, Instant)>,
}

impl SavePairs {
    /// A `Save` message: its save when its record is here.
    pub fn info(&mut self, info: SaveInfo, now: Instant) -> Option<NodeSave> {
        let key = (info.lease, info.seq);
        match self.records.remove(&key) {
            Some((record, _)) => Some(NodeSave { info, record }),
            None => {
                self.infos.insert(key, (info, now));
                bound(&mut self.infos);
                None
            }
        }
    }

    /// A record (from its stream): its save when its message is here.
    pub fn record(
        &mut self,
        lease: u64,
        seq: u32,
        record: Vec<u8>,
        now: Instant,
    ) -> Option<NodeSave> {
        let key = (lease, seq);
        match self.infos.remove(&key) {
            Some((info, _)) => Some(NodeSave { info, record }),
            None => {
                self.records.insert(key, (record, now));
                bound(&mut self.records);
                None
            }
        }
    }

    /// Drops the halves older than [`PAIR_WAIT`]; returns the `Save`
    /// messages whose record never came (they are answered `SaveInvalid`).
    pub fn expire(&mut self, now: Instant) -> Vec<SaveInfo> {
        let old = |t: &Instant| now.saturating_duration_since(*t) >= PAIR_WAIT;
        self.records.retain(|_, (_, t)| !old(t));
        let gone: Vec<(u64, u32)> = self
            .infos
            .iter()
            .filter(|(_, (_, t))| old(t))
            .map(|(k, _)| *k)
            .collect();
        gone.into_iter()
            .filter_map(|k| self.infos.remove(&k).map(|(i, _)| i))
            .collect()
    }

    /// A transfer's message: it with its record when the record is here.
    /// A message without a save is returned at once.
    pub fn transfer(
        &mut self,
        t: TransferHalf,
        now: Instant,
    ) -> Option<(TransferHalf, Option<Vec<u8>>)> {
        let Some(key) = t.key() else {
            return Some((t, None));
        };
        match self.records.remove(&key) {
            Some((record, _)) => Some((t, Some(record))),
            None => {
                self.transfers.insert(key, (t, now));
                bound(&mut self.transfers);
                None
            }
        }
    }

    /// A record that a transfer's message waits for: the two. Any other
    /// record is handed back (it is a save's: [`SavePairs::record`]).
    pub fn transfer_record(
        &mut self,
        lease: u64,
        seq: u32,
        record: Vec<u8>,
    ) -> Result<(TransferHalf, Vec<u8>), Vec<u8>> {
        match self.transfers.remove(&(lease, seq)) {
            Some((t, _)) => Ok((t, record)),
            None => Err(record),
        }
    }

    /// The transfers' messages whose record didn't come in [`PAIR_WAIT`]
    /// (they are refused `SaveInvalid`).
    pub fn expire_transfers(&mut self, now: Instant) -> Vec<TransferHalf> {
        let gone: Vec<(u64, u32)> = self
            .transfers
            .iter()
            .filter(|(_, (_, t))| now.saturating_duration_since(*t) >= PAIR_WAIT)
            .map(|(k, _)| *k)
            .collect();
        gone.into_iter()
            .filter_map(|k| self.transfers.remove(&k).map(|(t, _)| t))
            .collect()
    }

    /// How many halves are waiting.
    pub fn waiting(&self) -> usize {
        self.infos.len() + self.records.len() + self.transfers.len()
    }
}

/// Keeps at most [`PAIRS_MAX`] halves, dropping the oldest.
fn bound<T>(m: &mut BTreeMap<(u64, u32), (T, Instant)>) {
    while m.len() > PAIRS_MAX {
        let oldest = m.iter().min_by_key(|(_, (_, t))| *t).map(|(k, _)| *k);
        match oldest {
            Some(k) => m.remove(&k),
            None => break,
        };
    }
}

#[cfg(test)]
#[path = "saves_tests.rs"]
mod tests;
