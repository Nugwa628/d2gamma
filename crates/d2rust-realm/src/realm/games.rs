//! The games on the game servers, as the main server runs them (plan N4;
//! contracts 11.3):
//!
//! - [`Realm::start_game`]: `StartGame` with a fresh seed and the game's
//!   first block of item ids, answered `GameStarted` or `StartRefused`
//!   (within [`START_WAIT`]).
//! - [`Realm::offer_seat`]: the character's record on a stream of its own,
//!   then `SeatOffer`. The client's ticket for the seat is
//!   [`super::tickets::RealmTickets::join_node`]'s.
//! - [`Realm::kick`] and [`Realm::close_game`].
//! - What the game servers report goes to the lobby as [`GameNews`]
//!   ([`Realm::set_news`]): seats taken and refused, player counts, players
//!   who left, stored saves, closed games and lost game servers.
//!
//! Item ids come from the item registry ([`super::guard::SaveGuard::grant`]),
//! which never grants an id twice, even across restarts.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use d2_link::streams::{self, What};
use d2_link::Lease;
use d2_net::msg::account::CharSummary;
use d2_net::msg::node::{NewCharInfo, NodeGameSpec, RealmToNode};
use d2_net::msg::ErrorCode;
use tokio::sync::{mpsc, oneshot};

use super::nodes::Linked;
use super::Realm;
use crate::accounts::AccountId;

/// How long a game server has to answer `StartGame`.
pub const START_WAIT: Duration = Duration::from_secs(10);

/// What the games on the game servers tell the lobby.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameNews {
    /// A client took its seat with its ticket.
    SeatTaken { game: u32, lease: Lease },
    /// A seat ended before its player entered (its offer expired, the game
    /// refused it, or the game refused its 0x67 join: 45).
    SeatRefused {
        game: u32,
        lease: Lease,
        account: AccountId,
        character: String,
        code: ErrorCode,
    },
    /// The number of players of a game changed.
    Players { game: u32, players: u8 },
    /// A player left, after its last save was stored.
    Left {
        game: u32,
        lease: Lease,
        account: AccountId,
        character: String,
    },
    /// A save was stored: the character's new summary.
    Saved {
        account: AccountId,
        summary: CharSummary,
    },
    /// A game is over.
    Closed { game: u32 },
    /// A game server's link was lost: its games are over.
    NodeLost { node: String },
}

/// Where a `StartGame`'s answer goes.
type StartAnswer = oneshot::Sender<Result<(), ErrorCode>>;

/// `StartGame`s waiting for their answer, by (game server, game).
#[derive(Debug, Default)]
pub struct Starts(Mutex<HashMap<(String, u32), StartAnswer>>);

impl Starts {
    fn lock(&self) -> MutexGuard<'_, HashMap<(String, u32), StartAnswer>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Game server `node` answered the start of `game`.
    pub fn answered(&self, node: &str, game: u32, r: Result<(), ErrorCode>) {
        if let Some(tx) = self.lock().remove(&(node.to_owned(), game)) {
            let _ = tx.send(r);
        }
    }
}

/// A seat to offer.
#[derive(Clone, Debug)]
pub struct SeatOffer {
    pub game: u32,
    pub lease: Lease,
    /// The player's id in the game (opaque: no account id).
    pub player: u64,
    pub character: String,
    pub new: Option<NewCharInfo>,
    /// The stored record; `None` for a new character.
    pub record: Option<Vec<u8>>,
}

/// Where the news goes.
#[derive(Debug, Default)]
pub struct News(OnceLock<mpsc::UnboundedSender<GameNews>>);

impl News {
    pub fn send(&self, n: GameNews) {
        if let Some(tx) = self.0.get() {
            let _ = tx.send(n);
        }
    }
}

/// A random number from the operating system.
pub fn random_u64() -> u64 {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).expect("the operating system has a random source");
    u64::from_le_bytes(b)
}

impl Realm {
    /// One line in the main server's log.
    pub fn log_line(&self, line: &str) {
        self.log.line(line);
    }

    /// Sets where the games' news goes (the lobby's pump). Set once.
    pub fn set_news(&self, tx: mpsc::UnboundedSender<GameNews>) {
        let _ = self.news.0.set(tx);
    }

    /// Starts `spec` on `node`: a fresh seed and an item-id block.
    pub async fn start_game(&self, node: &Linked, spec: NodeGameSpec) -> Result<(), ErrorCode> {
        let game = spec.game;
        let item_ids = self.guard.grant(game).map_err(|e| {
            self.log.line(&format!("game {game}: item ids: {e}"));
            ErrorCode::OTHER
        })?;
        let (tx, rx) = oneshot::channel();
        self.starts.lock().insert((node.entry.id.clone(), game), tx);
        let sent = node.tx.send_msg(&RealmToNode::StartGame {
            spec,
            seed: random_u64() as u32,
            item_ids,
        });
        if sent.is_err() {
            self.starts.lock().remove(&(node.entry.id.clone(), game));
            return Err(ErrorCode::NODE_UNAVAILABLE);
        }
        let answer = tokio::time::timeout(START_WAIT, rx).await;
        self.starts.lock().remove(&(node.entry.id.clone(), game));
        match answer {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err(ErrorCode::NODE_UNAVAILABLE),
            Err(_) => {
                self.log.line(&format!(
                    "game {game}: node {} didn't answer its start",
                    node.entry.id
                ));
                Err(ErrorCode::NODE_UNAVAILABLE)
            }
        }
    }

    /// Offers seat `o` to `node`: the record first (on its own stream),
    /// then `SeatOffer`.
    pub async fn offer_seat(&self, node: &Linked, o: SeatOffer) -> Result<(), ErrorCode> {
        if let Some(record) = &o.record {
            streams::send(node.conn(), What::Record { lease: o.lease.0 }, 0, record)
                .await
                .map_err(|e| {
                    self.log.line(&format!(
                        "{}: sending `{}`'s record to node {}: {e}",
                        o.lease, o.character, node.entry.id
                    ));
                    ErrorCode::NODE_UNAVAILABLE
                })?;
        }
        node.tx
            .send_msg(&RealmToNode::SeatOffer {
                game: o.game,
                lease: o.lease.0,
                player: o.player,
                character: o.character,
                new: o.new,
                has_record: o.record.is_some(),
                ticket: None,
            })
            .map_err(|_| ErrorCode::NODE_UNAVAILABLE)
    }

    /// Asks `node` to take seat `lease` of `game` out (its character is
    /// saved and leaves, or its offer is withdrawn).
    pub fn kick(&self, node: &str, game: u32, lease: Lease) {
        if let Some(l) = self.nodes.get(node) {
            let _ = l.tx.send_msg(&RealmToNode::Kick {
                game,
                lease: lease.0,
            });
        }
    }

    /// Asks `node` to close `game`: `how` 0 saves its players, 1 doesn't.
    pub fn close_game(&self, node: &str, game: u32, how: u8) {
        if let Some(l) = self.nodes.get(node) {
            let _ = l.tx.send_msg(&RealmToNode::CloseGame { game, how });
        }
    }
}
