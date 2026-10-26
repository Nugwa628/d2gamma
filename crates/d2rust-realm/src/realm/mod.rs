//! The main server's side of the game-server link (contracts 11.2).
//!
//! A game server's connection (ALPN `d2rust-node/1`) arrives on the
//! players' port ([`crate::net::Net::run`] hands it here). Its first
//! message is `NodeHello`; [`Realm::serve_node`] admits it when
//!
//! 1. it presented a certificate the game-server list names
//!    ([`nodes::NodeRegistry::official`]), under the id it says it is
//!    (else `NodeUnknown`);
//! 2. its version, protocol and tick rate are this main server's (else
//!    `NodeBuild`), its mod realm is (else `NodeGame`), and its game data
//!    hashes the same (else `NodeData`).
//!
//! Then `NodeWelcome` (with the join tickets' public keys,
//! [`tickets::RealmTickets`]), and the game server counts as linked until its link
//! closes or [`LOST_AFTER`] passes without a `Load`: then it is lost, and
//! every seat lease on it ends. While linked, its saves (a `Save` message
//! and the record on its own stream) go to the [`saves::SaveDesk`] and are
//! answered with `SaveAck`; `Left` and `GameClosed` end leases. The games
//! it runs for the lobby are [`games`]'s: their answers and reports go to
//! the lobby as [`games::GameNews`]. Its stash transfers (`ItemOut`,
//! `ItemInDone`, `ItemOutRefused`) go to the online stash ([`stash`]).
//! Player-hosted game servers (a hello with a host token) come with plan
//! N8; until then they are refused with `HostingOff`.
//!
//! Upgrades ([`crate::upgrade`]): a game server that links during a
//! countdown hears it right after its welcome; one refused for another
//! version is first sent `Update(catch-up)` with this main server's version
//! ([`crate::upgrade::Upgrade::catch_up`]); its `UpdateState` tells the
//! countdown when it restarts.
//!
//! - [`nodes`]: the game-server list and the linked game servers.
//! - [`bundle`]: adding an official game server (`--add-node`).
//! - [`saves`]: seat leases and the save desk.
//! - [`tickets`]: the join tickets' keys and the client's `JoinNode`.
//! - [`games`]: starting games and offering seats on game servers.
//! - [`listing`]: the game servers as the lobby lists them, and where a
//!   new game runs.
//! - [`guard`]: the save checks and the item registry.
//! - [`journals`]: the games' journals, as their chunks come.
//! - [`stash`]: the online shared stash and its moves.

pub mod bundle;
pub mod games;
pub mod guard;
pub mod journals;
pub mod listing;
pub mod nodes;
pub mod saves;
pub mod stash;
pub mod tickets;

use std::sync::Arc;
use std::time::{Duration, Instant};

use d2_link::streams::{self, Carried, What};
use d2_link::{Lease, LinkRx, LinkTx, Refusal, Versions, HEARTBEAT, HELLO_WAIT, LOST_AFTER};
use d2_net::msg::node::{
    update_state, NodeHello, NodeKind, NodeToRealm, NodeWelcome, RealmToNode, SaveInfo,
};
use d2_net::msg::ErrorCode;
use d2_quic::{FramedStream, QuicConn};
use tokio::sync::mpsc;

use crate::log::ServerLog;
use crate::upgrade::Upgrade;
use games::{GameNews, News, Starts};
use guard::SaveGuard;
use journals::Journals;
use nodes::{NodeEntry, NodeRegistry};
use saves::{LeaseEnd, NodeSave, SaveDesk, SavePairs, TransferHalf};
use stash::Stash;
use tickets::RealmTickets;

/// How long an orphaned game keeps its players when its game server's link
/// is gone (plan N13).
pub const ORPHAN_S: u32 = 30;

/// How many seconds of a game one journal chunk holds (plan N7).
pub const JOURNAL_CHUNK_S: u32 = 10;

/// How long a refused link stays open for its refusal to arrive.
const CLOSE_GRACE: Duration = Duration::from_secs(1);

/// How often a link's unpaired saves are checked for their wait.
const PAIR_CHECK: Duration = Duration::from_secs(1);

/// Records and journal chunks read from a link's streams, waiting to be
/// handled.
const STREAMS_QUEUE: usize = 16;

/// What the main server tells and asks of its game servers.
#[derive(Debug)]
pub struct Realm {
    pub nodes: NodeRegistry,
    /// What a game server must match; its `game` is the mod realm the
    /// welcome and the join tickets name.
    pub versions: Versions,
    pub save_interval_s: u32,
    /// The join tickets' keys.
    pub tickets: RealmTickets,
    /// The seat leases and where saves are stored.
    pub saves: SaveDesk,
    /// The save checks and the item registry (item id blocks).
    guard: Arc<SaveGuard>,
    /// Where the games' journals are kept.
    journals: Arc<Journals>,
    starts: Starts,
    news: News,
    /// The online stash, when served ([`Realm::with_stash`]).
    stash: Option<Arc<Stash>>,
    /// The countdown to an upgrade.
    pub upgrade: Arc<Upgrade>,
    log: Arc<ServerLog>,
}

impl Realm {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nodes: NodeRegistry,
        versions: Versions,
        save_interval_s: u32,
        tickets: RealmTickets,
        saves: SaveDesk,
        guard: Arc<SaveGuard>,
        journals: Journals,
        log: Arc<ServerLog>,
    ) -> Self {
        Self {
            nodes,
            save_interval_s,
            tickets,
            saves: saves.guarded(guard.clone()),
            guard,
            journals: Arc::new(journals),
            starts: Starts::default(),
            news: News::default(),
            stash: None,
            upgrade: Arc::new(Upgrade::new(&versions.version)),
            versions,
            log,
        }
    }

    /// One game server's link, from its hello to its loss.
    pub async fn serve_node(self: Arc<Self>, conn: QuicConn, control: FramedStream) {
        let peer = conn.peer();
        let (tx, mut rx) = d2_link::start::<RealmToNode, NodeToRealm>(control);
        let hello = match tokio::time::timeout(HELLO_WAIT, rx.recv()).await {
            Ok(Ok(NodeToRealm::Hello(h))) => h,
            _ => {
                self.log
                    .line(&format!("link from {peer}: no NodeHello; closed"));
                conn.close(0, "no hello");
                return;
            }
        };
        let entry = match self.admit(&conn, &hello) {
            Ok(e) => e,
            Err(mut r) => {
                // Another version: an updater on it catches up with this
                // main server's, unless it runs the one being restarted for.
                if r.code == ErrorCode::NODE_BUILD && hello.version != self.versions.version {
                    // Its certificate and id are an official game server's.
                    self.nodes
                        .refused_version(&hello.node, &hello.version, peer);
                    match self.upgrade.catch_up(&hello.version) {
                        Ok(m) => {
                            let _ = tx.send_msg(&m);
                        }
                        Err(text) => r.text = text,
                    }
                }
                self.log.line(&format!(
                    "link from {peer} (node {:?}) refused: {}: {}",
                    hello.node, r.code, r.text
                ));
                let _ = tx.send_msg(&RealmToNode::Refused {
                    code: r.code,
                    text: r.text,
                });
                tx.close();
                tokio::time::sleep(CLOSE_GRACE).await;
                conn.close(0, "refused");
                return;
            }
        };
        let welcome = NodeWelcome {
            realm: self.versions.game.clone(),
            node: entry.id.clone(),
            kind: NodeKind::Official,
            seen_addr: peer.to_string(),
            heartbeat_ms: HEARTBEAT.as_millis() as u32,
            orphan_s: ORPHAN_S,
            journal_chunk_s: JOURNAL_CHUNK_S,
            save_interval_s: self.save_interval_s,
            ticket_keys: self.tickets.public_keys(),
        };
        if tx.send_msg(&RealmToNode::Welcome(welcome)).is_err() {
            return;
        }
        if let Some(m) = self.upgrade.for_new_link(Instant::now()) {
            let _ = tx.send_msg(&m);
        }
        let id = entry.id.clone();
        let link = self.nodes.linked(entry, peer, tx.clone(), conn.clone());
        let (carried_tx, mut carried) = mpsc::channel(STREAMS_QUEUE);
        let reader = tokio::spawn(read_streams(conn.clone(), carried_tx));
        let why = self.watch(&id, link, &tx, &mut rx, &mut carried).await;
        reader.abort();
        tx.close();
        if self.nodes.lost(&id, link, &why) {
            self.saves.end_node(&id, LeaseEnd::NodeLost);
            self.journals.node_lost(&id);
            self.news.send(GameNews::NodeLost { node: id });
        }
    }

    /// Whether the game server that said `hello` over `conn` may link.
    fn admit(&self, conn: &QuicConn, hello: &NodeHello) -> Result<NodeEntry, Refusal> {
        if hello.host_token.is_some() {
            return Err(Refusal::new(
                ErrorCode::HOSTING_OFF,
                "this main server takes no player-hosted games yet",
            ));
        }
        let unknown = |text: String| Refusal::new(ErrorCode::NODE_UNKNOWN, text);
        let fp = conn
            .peer_fingerprint()
            .ok_or_else(|| unknown("no certificate".into()))?;
        let entry = match self.nodes.official(&fp) {
            Ok(Some(e)) => e,
            Ok(None) => {
                return Err(unknown(
                    "this certificate is not on the main server's game-server list".into(),
                ));
            }
            Err(e) => {
                self.log.line(&format!("the game-server list: {e}"));
                return Err(unknown(
                    "the main server can't read its game-server list".into(),
                ));
            }
        };
        if entry.id != hello.node {
            return Err(unknown(format!(
                "this certificate is game server {}'s, not {}'s",
                entry.id, hello.node
            )));
        }
        self.versions.check(hello)?;
        if entry.game != hello.game {
            return Err(Refusal::new(
                ErrorCode::NODE_GAME,
                format!("game server {} was added for {}", entry.id, entry.game),
            ));
        }
        Ok(entry)
    }

    /// Reads a linked game server's messages and streams until its link
    /// ends or no `Load` comes for [`LOST_AFTER`]; returns why it is lost.
    async fn watch(
        &self,
        id: &str,
        link: u64,
        tx: &LinkTx<RealmToNode>,
        rx: &mut LinkRx<NodeToRealm>,
        carried: &mut mpsc::Receiver<Carried>,
    ) -> String {
        let mut deadline = tokio::time::Instant::now() + LOST_AFTER;
        let mut pairs = SavePairs::default();
        let mut check = tokio::time::interval(PAIR_CHECK);
        loop {
            tokio::select! {
                got = rx.recv() => match got {
                    Ok(NodeToRealm::Load(l)) => {
                        deadline = tokio::time::Instant::now() + LOST_AFTER;
                        self.nodes.load(id, link, l);
                    }
                    Ok(NodeToRealm::Save(info)) => {
                        if let Some(save) = pairs.info(info, Instant::now()) {
                            self.store(id, tx, save).await;
                        }
                    }
                    Ok(NodeToRealm::Left { game, lease }) => {
                        let lease = Lease(lease);
                        if let Some(g) = self.saves.end(id, lease, LeaseEnd::Left) {
                            self.news.send(GameNews::Left {
                                game,
                                lease,
                                account: g.account,
                                character: g.character,
                            });
                        }
                    }
                    Ok(NodeToRealm::GameClosed { game, reason }) => {
                        self.log.line(&format!("node {id}: game {game} closed: {reason}"));
                        self.saves.end_game(id, game, LeaseEnd::GameClosed);
                        self.news.send(GameNews::Closed { game });
                    }
                    Ok(NodeToRealm::GameStarted { game }) => {
                        self.starts.answered(id, game, Ok(()));
                    }
                    Ok(NodeToRealm::StartRefused { game, code }) => {
                        self.log.line(&format!("node {id}: game {game} not started: {code}"));
                        self.starts.answered(id, game, Err(code));
                    }
                    Ok(NodeToRealm::SeatTaken { game, lease }) => {
                        self.news.send(GameNews::SeatTaken { game, lease: Lease(lease) });
                    }
                    Ok(NodeToRealm::SeatRefused { game, lease, code }) => {
                        let lease = Lease(lease);
                        self.log.line(&format!("node {id}: {lease} of game {game} refused: {code}"));
                        if let Some(g) = self.saves.end(id, lease, LeaseEnd::Refused) {
                            self.news.send(GameNews::SeatRefused {
                                game,
                                lease,
                                account: g.account,
                                character: g.character,
                                code,
                            });
                        }
                    }
                    Ok(NodeToRealm::Players { game, players }) => {
                        self.news.send(GameNews::Players { game, players });
                    }
                    Ok(NodeToRealm::ItemIdsWanted { game }) => {
                        self.item_ids_wanted(id, tx, game).await;
                    }
                    Ok(NodeToRealm::ItemOut(o)) => {
                        let half = TransferHalf::Out(Box::new(o));
                        if let Some((half, record)) = pairs.transfer(half, Instant::now()) {
                            self.transfer(id, tx, half, record).await;
                        }
                    }
                    Ok(NodeToRealm::ItemInDone(d)) => {
                        let half = TransferHalf::In(d);
                        if let Some((half, record)) = pairs.transfer(half, Instant::now()) {
                            self.transfer(id, tx, half, record).await;
                        }
                    }
                    Ok(NodeToRealm::ItemOutRefused { lease, req, code }) => {
                        self.transfer_refused(lease, req, code);
                    }
                    // The chunk is kept from its stream, which carries all
                    // this too (and may come first).
                    Ok(NodeToRealm::JournalChunk { .. }) => {}
                    Ok(NodeToRealm::UpdateState { version, state, detail }) => {
                        self.log.line(&format!(
                            "node {id}: update {version}: {}{}",
                            state_word(state),
                            if detail.is_empty() { String::new() } else { format!(" ({detail})") }
                        ));
                        self.nodes.update(id, link, &version, state);
                        if state == update_state::RESTARTING {
                            self.upgrade.node_restarting(id);
                        }
                    }
                    Ok(other) => self.log.line(&format!(
                        "node {id}: kind {:#04x} not handled yet",
                        other.kind()
                    )),
                    Err(e) => return format!("its link ended ({e})"),
                },
                Some(c) = carried.recv() => match c.what {
                    What::Record { lease } => match pairs.transfer_record(lease, c.seq, c.bytes) {
                        Ok((half, record)) => self.transfer(id, tx, half, Some(record)).await,
                        Err(bytes) => {
                            if let Some(save) = pairs.record(lease, c.seq, bytes, Instant::now()) {
                                self.store(id, tx, save).await;
                            }
                        }
                    },
                    What::Journal { game } => self.journal(id, game, c.seq, c.bytes).await,
                },
                _ = check.tick() => {
                    for info in pairs.expire(Instant::now()) {
                        self.refuse_unpaired(id, tx, &info);
                    }
                    for half in pairs.expire_transfers(Instant::now()) {
                        self.transfer_expired(id, tx, half).await;
                    }
                }
                () = tokio::time::sleep_until(deadline) => {
                    return format!("no heartbeat in {} s", LOST_AFTER.as_secs());
                }
            }
        }
    }

    /// Hands a paired save to the desk and answers it.
    async fn store(&self, id: &str, tx: &LinkTx<RealmToNode>, save: NodeSave) {
        let (lease, seq) = (save.info.lease, save.info.seq);
        let code = match self.saves.accept(id, save).await {
            Ok(s) => {
                if let Some(stash) = &self.stash {
                    stash.saved(&s.summary.name);
                }
                self.news.send(GameNews::Saved {
                    account: s.account,
                    summary: s.summary,
                });
                saves::STORED
            }
            Err(d) => d.code,
        };
        let _ = tx.send_msg(&RealmToNode::SaveAck { lease, seq, code });
    }

    /// Keeps chunk `seq` of game `game`'s journal from game server `id`.
    async fn journal(&self, id: &str, game: u32, seq: u32, gz: Vec<u8>) {
        let (journals, node) = (self.journals.clone(), id.to_owned());
        // A broken journal is logged by the store.
        let _ = tokio::task::spawn_blocking(move || journals.chunk(&node, game, seq, gz)).await;
    }

    /// Game `game` of game server `id` runs low on item IDs: one more block
    /// from the registry (`ItemIds`). Only a game server with seats in the
    /// game gets one.
    async fn item_ids_wanted(&self, id: &str, tx: &LinkTx<RealmToNode>, game: u32) {
        if !self.saves.leases().holds_game(id, game) {
            self.log.line(&format!(
                "node {id}: item ids wanted for game {game}, where it has no seats; refused"
            ));
            return;
        }
        let guard = self.guard.clone();
        let granted = tokio::task::spawn_blocking(move || guard.grant(game))
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        match granted {
            Ok(ids) => {
                self.log.line(&format!(
                    "node {id}: game {game}: item ids {}..+{}",
                    ids.first, ids.count
                ));
                let _ = tx.send_msg(&RealmToNode::ItemIds { game, ids });
            }
            Err(e) => self
                .log
                .line(&format!("node {id}: game {game}: item ids: {e}")),
        }
    }

    /// Sends `m` to every linked game server.
    pub fn send_all(&self, m: &RealmToNode) {
        for l in self.nodes.linked_now() {
            let _ = l.tx.send_msg(m);
        }
    }

    /// A `Save` whose record didn't come in time.
    fn refuse_unpaired(&self, id: &str, tx: &LinkTx<RealmToNode>, info: &SaveInfo) {
        self.log.line(&format!(
            "node {id}: save {} of lease {} came without its record; refused",
            info.seq, info.lease
        ));
        let _ = tx.send_msg(&RealmToNode::SaveAck {
            lease: info.lease,
            seq: info.seq,
            code: ErrorCode::SAVE_INVALID,
        });
    }
}

/// An `UpdateState`'s state in the log.
fn state_word(state: u8) -> &'static str {
    match state {
        update_state::IDLE => "idle",
        update_state::DOWNLOADING => "downloading",
        update_state::READY => "ready",
        update_state::FAILED => "failed",
        _ => "restarting",
    }
}

/// Reads the game server's streams (records, journal chunks) and queues
/// what they carry, each stream on its own task; a broken stream is
/// dropped. Ends with the connection.
async fn read_streams(conn: QuicConn, out: mpsc::Sender<Carried>) {
    while let Some(stream) = conn.accept_uni().await {
        let out = out.clone();
        tokio::spawn(async move {
            if let Ok(c) = streams::recv(stream).await {
                let _ = out.send(c).await;
            }
        });
    }
}
