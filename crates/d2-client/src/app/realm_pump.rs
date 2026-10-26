// D2RUST's own: the in-game reader of the realm session.
//! In a game the account's main-server session (the realm) stays open
//! next to the game stream: the market, the online stash, the chat, the
//! ladder and the lobby's game list all talk to the realm on it. One
//! reader serves them all, the [`RealmPump`]: once per frame, when the
//! session is free (`try_lock`, never waiting: the stash's worker, the
//! connection watch or a reconnect may hold it), it sends what the owners
//! queued in the [`RealmOutbox`] and drains `Session::poll`, putting each
//! body in its owner's queue of the [`RealmInbox`] by its kind
//! ([`owner_of`]):
//!
//! - market kinds to the market (`market::realm::RealmSource`);
//! - social, guild, friends kinds and the old lobby chat line to the chat;
//! - ladder kinds to the ladder;
//! - the other lobby kinds (the game list) to the lobby, so its list is
//!   the realm's latest when the player leaves the game;
//! - an `ERROR` to the owner of the request kind it answers.
//!
//! Stash answers never come out of `Session::poll`: they stay queued in
//! the session for the stash request that waits for them, as before. A
//! body no owner reads in a game (an account answer) is counted and
//! dropped.
//!
//! When the session fails (a send or a read errs), what the owners queued
//! is dropped, the inbox counts a loss, and the pump tries the session
//! again now and then: once it reads, it is a new session (the game's
//! reconnect put it in place) and the inbox counts it, so each owner
//! starts over. A queue nobody reads is capped and keeps its newest
//! bodies; the lobby's starts again at every new game list.
//!
//! The routing core works on any [`RealmWire`]; the session's own
//! ([`d2_online::Session`]) and the game's frame system are online-only.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, TryLockError};
use std::time::{Duration, Instant};

use d2_net::msg::{self, ErrorMsg};

/// The kind ranges of the realm lobby's areas (lobby contract section 1).
/// The wire lane adds them to `d2_net::msg::kind`; these go once it has.
pub mod kinds {
    use std::ops::RangeInclusive;

    /// Ladder seasons and pages, inside the lobby's range.
    pub const LADDER: RangeInclusive<u8> = 0x50..=0x57;
    /// Hello and state, rooms, DMs, user lists.
    pub const SOCIAL: RangeInclusive<u8> = 0x70..=0x7F;
    /// Guilds.
    pub const GUILD: RangeInclusive<u8> = 0xF0..=0xF7;
    /// Friends, ignore, privacy.
    pub const FRIENDS: RangeInclusive<u8> = 0xF8..=0xFF;
}

/// Who reads a realm body in a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Owner {
    /// The market window's source.
    Market,
    /// The chat module: rooms, DMs, user lists, guilds, friends, and the
    /// old lobby chat line.
    Social,
    /// The ladder.
    Ladder,
    /// The lobby's game list.
    Lobby,
}

impl Owner {
    pub const ALL: [Owner; 4] = [Owner::Market, Owner::Social, Owner::Ladder, Owner::Lobby];

    fn index(self) -> usize {
        self as usize
    }
}

/// The owner of a message of `kind` (a body's own kind, or the request
/// kind an `ERROR` answers); `None`: no reader in a game.
pub fn owner_of_kind(kind: u8) -> Option<Owner> {
    use d2_net::msg::lobby::kind as lobby;
    match kind {
        k if msg::kind::MARKET.contains(&k) => Some(Owner::Market),
        k if kinds::LADDER.contains(&k) => Some(Owner::Ladder),
        k if kinds::SOCIAL.contains(&k)
            || kinds::GUILD.contains(&k)
            || kinds::FRIENDS.contains(&k) =>
        {
            Some(Owner::Social)
        }
        // The old lobby chat (contract 7.1): a chat line, the chat's.
        lobby::CHAT | lobby::CHAT_LINE => Some(Owner::Social),
        k if msg::kind::LOBBY.contains(&k) => Some(Owner::Lobby),
        _ => None,
    }
}

/// The owner of the S→C `body`: by its kind, an `ERROR` by the kind of
/// the request it answers. `None`: no reader in a game (or an empty or
/// unreadable `ERROR`).
pub fn owner_of(body: &[u8]) -> Option<Owner> {
    match *body.first()? {
        msg::kind::ERROR => owner_of_kind(ErrorMsg::parse(body).ok()?.request_kind),
        k => owner_of_kind(k),
    }
}

/// How many bodies one owner's queue keeps while nobody reads it.
pub const QUEUE_MAX: usize = 1024;

/// How many bodies one frame reads at most.
pub const READ_MAX: usize = 4096;

/// How often a failed session is tried again.
pub const RETRY: Duration = Duration::from_secs(1);

/// The realm link as the owners see it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkState {
    /// The session failed and is not back yet.
    pub down: bool,
    /// Failures so far: an owner that sees this change lost what it had in
    /// flight.
    pub losses: u64,
    /// The bodies dropped with no reader (a full queue, or no owner).
    pub dropped: u64,
}

#[derive(Default)]
struct Queues {
    by_owner: [VecDeque<Vec<u8>>; 4],
    link: LinkState,
}

impl Queues {
    fn put(&mut self, owner: Owner, body: Vec<u8>) {
        let q = &mut self.by_owner[owner.index()];
        // A new game list makes the bodies before it old.
        if owner == Owner::Lobby && body.first() == Some(&d2_net::msg::lobby::kind::GAME_LIST_BEGIN)
        {
            self.link.dropped += q.len() as u64;
            q.clear();
        }
        if q.len() >= QUEUE_MAX {
            q.pop_front();
            self.link.dropped += 1;
        }
        q.push_back(body);
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the realm sent in a game, one queue per [`Owner`] (and the link's
/// state). Each owner holds a clone and takes its own bodies.
#[derive(Clone, Default, bevy::prelude::Resource)]
pub struct RealmInbox(Arc<Mutex<Queues>>);

impl RealmInbox {
    /// `owner`'s bodies, oldest first, taken out of the queue.
    pub fn take(&self, owner: Owner) -> Vec<Vec<u8>> {
        lock(&self.0).by_owner[owner.index()].drain(..).collect()
    }

    /// How many bodies wait for `owner`.
    pub fn waiting(&self, owner: Owner) -> usize {
        lock(&self.0).by_owner[owner.index()].len()
    }

    /// The link's state now.
    pub fn link(&self) -> LinkState {
        lock(&self.0).link
    }

    /// Puts `body` in its owner's queue; a body with no owner is counted
    /// and dropped. Its owner, if it has one.
    pub fn route(&self, body: Vec<u8>) -> Option<Owner> {
        let mut q = lock(&self.0);
        let owner = owner_of(&body);
        match owner {
            Some(o) => q.put(o, body),
            None => q.link.dropped += 1,
        }
        owner
    }

    fn lost(&self) {
        let mut q = lock(&self.0);
        q.link.down = true;
        q.link.losses += 1;
    }

    fn back(&self) {
        lock(&self.0).link.down = false;
    }
}

/// What the owners send to the realm in a game: C→S bodies, in order, for
/// the pump to send when the session is free.
#[derive(Clone, Default, bevy::prelude::Resource)]
pub struct RealmOutbox(Arc<Mutex<VecDeque<Vec<u8>>>>);

impl RealmOutbox {
    /// Queues `body` for the realm.
    pub fn send(&self, body: Vec<u8>) {
        lock(&self.0).push_back(body);
    }

    /// Bodies not sent yet.
    pub fn waiting(&self) -> usize {
        lock(&self.0).len()
    }

    fn clear(&self) {
        lock(&self.0).clear();
    }

    /// The oldest body not sent yet, taken out: the pump's, and the
    /// lobby link's for the lobby's market.
    pub fn pop(&self) -> Option<Vec<u8>> {
        lock(&self.0).pop_front()
    }
}

/// The realm connection the pump reads: the session's control stream.
pub trait RealmWire {
    type Error: std::fmt::Display;

    /// Sends one C→S body.
    fn send(&mut self, body: Vec<u8>) -> Result<(), Self::Error>;

    /// The next S→C body that arrived, or `None`; never waits.
    fn poll(&mut self) -> Result<Option<Vec<u8>>, Self::Error>;
}

/// What one [`RealmPump::step`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pumped {
    /// Someone else holds the session: nothing this frame.
    Busy,
    /// No session (logged out).
    NoSession,
    /// The session failed; it is tried again later.
    Waiting,
    /// The session failed this frame.
    Failed,
    /// Sent and read: how many bodies were sent and read.
    Done { sent: usize, read: usize },
}

/// The one reader of the realm session in a game.
pub struct RealmPump<W> {
    slot: Arc<Mutex<Option<W>>>,
    inbox: RealmInbox,
    outbox: RealmOutbox,
    /// The session failed: when to try it again.
    retry_at: Option<Instant>,
}

impl<W: RealmWire> RealmPump<W> {
    /// A pump on the session in `slot` with fresh queues.
    pub fn new(slot: Arc<Mutex<Option<W>>>) -> Self {
        Self::with(slot, RealmInbox::default(), RealmOutbox::default())
    }

    /// A pump on the session in `slot` that fills `inbox` and empties
    /// `outbox`.
    pub fn with(slot: Arc<Mutex<Option<W>>>, inbox: RealmInbox, outbox: RealmOutbox) -> Self {
        Self {
            slot,
            inbox,
            outbox,
            retry_at: None,
        }
    }

    pub fn inbox(&self) -> &RealmInbox {
        &self.inbox
    }

    pub fn outbox(&self) -> &RealmOutbox {
        &self.outbox
    }

    /// One frame: sends what waits and routes what arrived, if the session
    /// is free; never waits for it.
    pub fn step(&mut self, now: Instant) -> Pumped {
        let shared = self.slot.clone();
        let mut slot = match shared.try_lock() {
            Ok(s) => s,
            Err(TryLockError::WouldBlock) => return Pumped::Busy,
            Err(TryLockError::Poisoned(e)) => e.into_inner(),
        };
        let Some(wire) = slot.as_mut() else {
            return Pumped::NoSession;
        };
        if let Some(at) = self.retry_at {
            // What the owners sent meanwhile is lost with the session.
            self.outbox.clear();
            if now < at {
                return Pumped::Waiting;
            }
        }
        let mut read = 0;
        if self.retry_at.is_some() {
            // Whether the session reads again: a new one (a reconnect).
            match wire.poll() {
                Ok(b) => {
                    self.retry_at = None;
                    self.inbox.back();
                    if let Some(b) = b {
                        self.inbox.route(b);
                        read += 1;
                    }
                }
                Err(_) => {
                    self.retry_at = Some(now + RETRY);
                    return Pumped::Waiting;
                }
            }
        }
        let mut sent = 0;
        while let Some(body) = self.outbox.pop() {
            if let Err(e) = wire.send(body) {
                return self.fail(now, &e);
            }
            sent += 1;
        }
        while read < READ_MAX {
            match wire.poll() {
                Ok(Some(b)) => {
                    self.inbox.route(b);
                    read += 1;
                }
                Ok(None) => break,
                Err(e) => return self.fail(now, &e),
            }
        }
        Pumped::Done { sent, read }
    }

    fn fail(&mut self, now: Instant, e: &impl std::fmt::Display) -> Pumped {
        bevy::log::warn!("realm: the session failed: {e}");
        self.outbox.clear();
        self.inbox.lost();
        self.retry_at = Some(now + RETRY);
        Pumped::Failed
    }
}

#[cfg(feature = "online")]
pub use online::*;

#[cfg(feature = "online")]
mod online {
    use super::*;
    use bevy::prelude::{ResMut, Resource, World};
    use d2_online::{OnlineError, Session};

    use crate::app::online::OnlineSession;

    impl RealmWire for Session {
        type Error = OnlineError;

        fn send(&mut self, body: Vec<u8>) -> Result<(), OnlineError> {
            Session::send(self, body)
        }

        fn poll(&mut self) -> Result<Option<Vec<u8>>, OnlineError> {
            Session::poll(self)
        }
    }

    /// The running game's pump on the account's session.
    #[derive(Resource)]
    pub struct GamePump(pub RealmPump<Session>);

    /// Installs the pump of a game on `session`: the inbox and outbox go
    /// into `world` as resources (and back to the caller, for the owners
    /// made with the game).
    pub fn install(world: &mut World, session: &OnlineSession) -> (RealmInbox, RealmOutbox) {
        let pump = RealmPump::new(session.0.clone());
        let queues = (pump.inbox().clone(), pump.outbox().clone());
        world.insert_resource(queues.0.clone());
        world.insert_resource(queues.1.clone());
        world.insert_resource(GamePump(pump));
        queues
    }

    /// The game's frame system: one pump step.
    pub fn pump(mut p: ResMut<GamePump>) {
        p.0.step(Instant::now());
    }
}

#[cfg(test)]
#[path = "realm_pump_tests.rs"]
mod tests;
