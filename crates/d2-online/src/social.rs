//! The realm's chat, guilds and friends on a [`Session`]
//! (`docs/plans/server/lobby-contract.md` sections 3-5 and 12):
//! [`SocialClient`] numbers the requests, sends them, and puts each answer
//! back together from the bodies the session polls: a list's head, its
//! rows and its end become one [`SocialAnswer`], keyed by the request
//! number, while pushes (room lines, live DMs, presence, notices) pass
//! straight through.
//!
//! It never waits: requests go out with [`Session::send`], answers come in
//! with [`Session::poll`] (or [`SocialClient::feed`], for a caller that
//! drains the control stream itself), and several requests can be in
//! flight with their lists and the pushes interleaved. Bodies of other
//! areas (lobby, market, ladder) go back to the caller untouched.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use d2_net::msg::guild::{self, FriendNotice, FriendRow, GuildInfo, GuildMember, GuildNotice};
use d2_net::msg::social::{
    self, DmConversationRow, DmLine, Done, ListHead, RoomLine, RoomPresence, SocialState, Who,
};
use d2_net::msg::{self, ErrorMsg};

use crate::connect::Session;
use crate::OnlineError;

/// The request numbers of one session, rising from 1. The social and the
/// ladder client of a session share one ([`crate::LadderClient::sharing`]),
/// so every request of the session has its own number.
#[derive(Clone, Debug)]
pub struct ReqCounter(Arc<AtomicU32>);

impl Default for ReqCounter {
    fn default() -> Self {
        Self(Arc::new(AtomicU32::new(1)))
    }
}

impl ReqCounter {
    /// The next number; never 0 (a push's).
    pub fn next(&self) -> u32 {
        loop {
            let n = self.0.fetch_add(1, Ordering::Relaxed);
            if n != 0 {
                return n;
            }
        }
    }
}

/// A C→S message of the social service: chat, or guilds and friends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocialAsk {
    Social(social::Ask),
    Guild(guild::Ask),
}

impl SocialAsk {
    pub fn kind(&self) -> u8 {
        match self {
            Self::Social(a) => a.kind(),
            Self::Guild(a) => a.kind(),
        }
    }

    /// The body of this ask numbered `req`.
    pub fn body(self, req: u32) -> Vec<u8> {
        match self {
            Self::Social(ask) => social::ToRealm { req, ask }.body(),
            Self::Guild(ask) => guild::ToRealm { req, ask }.body(),
        }
    }
}

impl From<social::Ask> for SocialAsk {
    fn from(a: social::Ask) -> Self {
        Self::Social(a)
    }
}

impl From<guild::Ask> for SocialAsk {
    fn from(a: guild::Ask) -> Self {
        Self::Guild(a)
    }
}

/// A user list's row: who, and where they are (`social::place`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserRow {
    pub who: Who,
    pub place: u8,
}

/// One answer or push of the social service, put together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocialAnswer {
    /// The answer to a hello, or a push (`req` 0).
    State(SocialState),
    /// Pushed: a line of a room.
    RoomLine(RoomLine),
    /// Pushed: a live DM line (to or from this account, from any of its
    /// sessions).
    Dm(DmLine),
    /// Pushed for the watched room.
    Presence(RoomPresence),
    GuildNotice(GuildNotice),
    FriendNotice(FriendNotice),
    /// A request carried out or refused (a refused list too).
    Done(Done),
    /// The realm answered with `ERROR` (it doesn't serve the area:
    /// `NotSupported`; not logged in). `req` is the oldest request in
    /// flight of that kind.
    Error {
        req: Option<u32>,
        error: ErrorMsg,
    },
    /// A page of DM conversations.
    DmConversations {
        req: u32,
        head: ListHead,
        rows: Vec<DmConversationRow>,
    },
    /// DM lines with one peer, oldest first.
    DmHistory {
        req: u32,
        head: ListHead,
        lines: Vec<DmLine>,
    },
    /// A room's user list, sorted by character name.
    Users {
        req: u32,
        head: ListHead,
        rows: Vec<UserRow>,
    },
    /// The asker's guild with its members.
    Guild {
        req: u32,
        info: GuildInfo,
        head: ListHead,
        members: Vec<GuildMember>,
    },
    /// The guilds that invited the asker.
    GuildInvites {
        req: u32,
        head: ListHead,
        invites: Vec<GuildInfo>,
    },
    /// A guild's public facts, by tag.
    GuildByTag {
        req: u32,
        info: GuildInfo,
    },
    /// The friends or the ignore list.
    Friends {
        req: u32,
        head: ListHead,
        rows: Vec<FriendRow>,
    },
}

impl SocialAnswer {
    /// The request this answers; `None` for a push, 0 for a pushed state.
    pub fn req(&self) -> Option<u32> {
        match self {
            Self::State(s) => Some(s.req),
            Self::Done(d) => Some(d.req),
            Self::Error { req, .. } => *req,
            Self::RoomLine(_)
            | Self::Dm(_)
            | Self::Presence(_)
            | Self::GuildNotice(_)
            | Self::FriendNotice(_) => None,
            Self::DmConversations { req, .. }
            | Self::DmHistory { req, .. }
            | Self::Users { req, .. }
            | Self::Guild { req, .. }
            | Self::GuildInvites { req, .. }
            | Self::GuildByTag { req, .. }
            | Self::Friends { req, .. } => Some(*req),
        }
    }
}

/// A request in flight: its kind, and for a guild get which.
#[derive(Clone, Copy, Debug)]
struct Pending {
    kind: u8,
    guild_get: u8,
}

/// A list being put together: the answer it becomes and the rows its head
/// announced.
#[derive(Debug)]
struct Partial {
    answer: SocialAnswer,
    rows: usize,
}

impl Partial {
    fn rows_so_far(&self) -> usize {
        match &self.answer {
            SocialAnswer::DmConversations { rows, .. } => rows.len(),
            SocialAnswer::DmHistory { lines, .. } => lines.len(),
            SocialAnswer::Users { rows, .. } => rows.len(),
            SocialAnswer::Guild { members, .. } => members.len(),
            SocialAnswer::GuildInvites { invites, .. } => invites.len(),
            SocialAnswer::Friends { rows, .. } => rows.len(),
            _ => 0,
        }
    }
}

/// The client side of the social service: request numbers, the requests
/// in flight, and the lists being put together.
#[derive(Debug, Default)]
pub struct SocialClient {
    reqs: ReqCounter,
    in_flight: BTreeMap<u32, Pending>,
    lists: BTreeMap<u32, Partial>,
    /// The guild of a "my guild" answer, waiting for its member list.
    guild_heads: BTreeMap<u32, GuildInfo>,
    answers: VecDeque<SocialAnswer>,
}

impl SocialClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// The session's request numbers, for a [`crate::LadderClient`] of the
    /// same session.
    pub fn counter(&self) -> &ReqCounter {
        &self.reqs
    }

    /// Requests sent and not answered yet.
    pub fn in_flight(&self) -> usize {
        self.in_flight.len()
    }

    /// Forgets what was in flight (a new connection: send the hello
    /// again). Request numbers keep rising.
    pub fn reset(&mut self) {
        self.in_flight.clear();
        self.lists.clear();
        self.guild_heads.clear();
    }

    /// Numbers `ask` and records it as in flight: its number and the body
    /// to send.
    pub fn prepare(&mut self, ask: impl Into<SocialAsk>) -> (u32, Vec<u8>) {
        let ask = ask.into();
        let req = self.reqs.next();
        let guild_get = match &ask {
            SocialAsk::Guild(guild::Ask::GuildGet { what, .. }) => *what,
            _ => 0,
        };
        self.in_flight.insert(
            req,
            Pending {
                kind: ask.kind(),
                guild_get,
            },
        );
        (req, ask.body(req))
    }

    /// Sends `ask` on `session`: its request number.
    pub fn send(
        &mut self,
        session: &mut Session,
        ask: impl Into<SocialAsk>,
    ) -> Result<u32, OnlineError> {
        let (req, body) = self.prepare(ask);
        if let Err(e) = session.send(body) {
            self.in_flight.remove(&req);
            return Err(e);
        }
        Ok(req)
    }

    /// Drains what has arrived on `session`: social, guild and friends
    /// bodies are taken (their answers wait in [`SocialClient::take`]); the
    /// others are returned in order, for their owners.
    pub fn poll(&mut self, session: &mut Session) -> Result<Vec<Vec<u8>>, OnlineError> {
        let mut others = Vec::new();
        while let Some(body) = session.poll()? {
            if !self.feed(&body)? {
                others.push(body);
            }
        }
        Ok(others)
    }

    /// The answers and pushes put together so far, oldest first.
    pub fn take(&mut self) -> Vec<SocialAnswer> {
        self.answers.drain(..).collect()
    }

    /// Whether a body is one of this client's: a social, guild or friends
    /// kind, or an `ERROR` answering one.
    pub fn owns(body: &[u8]) -> bool {
        match body.first() {
            Some(&msg::kind::ERROR) => {
                ErrorMsg::parse(body).is_ok_and(|e| is_social_kind(e.request_kind))
            }
            Some(&k) => is_social_kind(k),
            None => false,
        }
    }

    /// Takes one S→C control body: `false` if it isn't the social
    /// service's (the caller keeps it). A social body that doesn't read,
    /// or a list that breaks its framing, is [`OnlineError::Malformed`]:
    /// the realm broke the protocol. Answers to requests not in flight are
    /// dropped; pushes always pass.
    pub fn feed(&mut self, body: &[u8]) -> Result<bool, OnlineError> {
        let Some(&k) = body.first() else {
            return Ok(false);
        };
        if k == msg::kind::ERROR {
            let e = ErrorMsg::parse(body).map_err(|_| OnlineError::Malformed)?;
            if !is_social_kind(e.request_kind) {
                return Ok(false);
            }
            let req = self
                .in_flight
                .iter()
                .find(|(_, p)| p.kind == e.request_kind)
                .map(|(req, _)| *req);
            if let Some(req) = req {
                self.finish(req);
            }
            self.answers
                .push_back(SocialAnswer::Error { req, error: e });
            return Ok(true);
        }
        if msg::kind::SOCIAL.contains(&k) {
            let m = social::FromRealm::parse(body).map_err(|_| OnlineError::Malformed)?;
            self.take_social(m)?;
            return Ok(true);
        }
        if msg::kind::GUILD.contains(&k) || msg::kind::FRIENDS.contains(&k) {
            let m = guild::FromRealm::parse(body).map_err(|_| OnlineError::Malformed)?;
            self.take_guild(m)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn finish(&mut self, req: u32) {
        self.in_flight.remove(&req);
        self.lists.remove(&req);
        self.guild_heads.remove(&req);
    }

    /// The list `req` is building; a row with no head is malformed.
    fn list(&mut self, req: u32) -> Result<&mut Partial, OnlineError> {
        let p = self.lists.get_mut(&req).ok_or(OnlineError::Malformed)?;
        if p.rows_so_far() >= p.rows {
            return Err(OnlineError::Malformed);
        }
        Ok(p)
    }

    fn take_social(&mut self, m: social::FromRealm) -> Result<(), OnlineError> {
        use social::FromRealm as F;
        use SocialAnswer as A;
        match m {
            F::RoomLine(l) => {
                self.answers.push_back(A::RoomLine(l));
                return Ok(());
            }
            F::RoomPresence(p) => {
                self.answers.push_back(A::Presence(p));
                return Ok(());
            }
            F::State(s) => {
                self.in_flight.remove(&s.req);
                self.answers.push_back(A::State(s));
                return Ok(());
            }
            F::DmLine(l) if l.req == 0 => {
                self.answers.push_back(A::Dm(l));
                return Ok(());
            }
            _ => {}
        }
        let req = m.req().unwrap_or(0);
        let Some(pending) = self.in_flight.get(&req).copied() else {
            return Ok(());
        };
        match m {
            F::RoomLine(_) | F::RoomPresence(_) | F::State(_) => unreachable!("handled above"),
            F::Done(d) => {
                self.finish(req);
                self.answers.push_back(A::Done(d));
            }
            F::ListBegin(head) => {
                if self.lists.contains_key(&req) {
                    return Err(OnlineError::Malformed);
                }
                let answer = match (pending.kind, pending.guild_get) {
                    (social::kind::DM_CONVERSATIONS, _) => A::DmConversations {
                        req,
                        head,
                        rows: Vec::new(),
                    },
                    (social::kind::DM_HISTORY, _) => A::DmHistory {
                        req,
                        head,
                        lines: Vec::new(),
                    },
                    (social::kind::USER_LIST, _) => A::Users {
                        req,
                        head,
                        rows: Vec::new(),
                    },
                    (guild::kind::GUILD_GET, guild::guild_get::MINE) => A::Guild {
                        req,
                        info: self
                            .guild_heads
                            .remove(&req)
                            .ok_or(OnlineError::Malformed)?,
                        head,
                        members: Vec::new(),
                    },
                    (guild::kind::GUILD_GET, guild::guild_get::INVITES) => A::GuildInvites {
                        req,
                        head,
                        invites: Vec::new(),
                    },
                    (guild::kind::FRIENDS_GET, _) => A::Friends {
                        req,
                        head,
                        rows: Vec::new(),
                    },
                    _ => return Err(OnlineError::Malformed),
                };
                self.lists.insert(
                    req,
                    Partial {
                        answer,
                        rows: head.rows.into(),
                    },
                );
            }
            F::DmConversationRow { row, .. } => match &mut self.list(req)?.answer {
                A::DmConversations { rows, .. } => rows.push(row),
                _ => return Err(OnlineError::Malformed),
            },
            F::DmLine(line) => match &mut self.list(req)?.answer {
                A::DmHistory { lines, .. } => lines.push(line),
                _ => return Err(OnlineError::Malformed),
            },
            F::UserRow { who, place, .. } => match &mut self.list(req)?.answer {
                A::Users { rows, .. } => rows.push(UserRow { who, place }),
                _ => return Err(OnlineError::Malformed),
            },
            F::ListEnd { .. } => {
                let p = self.lists.remove(&req).ok_or(OnlineError::Malformed)?;
                if p.rows_so_far() != p.rows {
                    return Err(OnlineError::Malformed);
                }
                self.finish(req);
                self.answers.push_back(p.answer);
            }
        }
        Ok(())
    }

    fn take_guild(&mut self, m: guild::FromRealm) -> Result<(), OnlineError> {
        use guild::FromRealm as F;
        use SocialAnswer as A;
        let req = match m {
            F::GuildNotice(n) => {
                self.answers.push_back(A::GuildNotice(n));
                return Ok(());
            }
            F::FriendNotice(n) => {
                self.answers.push_back(A::FriendNotice(n));
                return Ok(());
            }
            ref m => m.req().unwrap_or(0),
        };
        let Some(pending) = self.in_flight.get(&req).copied() else {
            return Ok(());
        };
        match m {
            F::GuildNotice(_) | F::FriendNotice(_) => unreachable!("handled above"),
            F::GuildInfo { info, .. } => {
                if pending.kind != guild::kind::GUILD_GET {
                    return Err(OnlineError::Malformed);
                }
                match pending.guild_get {
                    guild::guild_get::MINE => {
                        if self.lists.contains_key(&req)
                            || self.guild_heads.insert(req, info).is_some()
                        {
                            return Err(OnlineError::Malformed);
                        }
                    }
                    guild::guild_get::BY_TAG => {
                        self.finish(req);
                        self.answers.push_back(A::GuildByTag { req, info });
                    }
                    _ => match &mut self.list(req)?.answer {
                        A::GuildInvites { invites, .. } => invites.push(info),
                        _ => return Err(OnlineError::Malformed),
                    },
                }
            }
            F::GuildMember { member, .. } => match &mut self.list(req)?.answer {
                A::Guild { members, .. } => members.push(member),
                _ => return Err(OnlineError::Malformed),
            },
            F::FriendRow { row, .. } => match &mut self.list(req)?.answer {
                A::Friends { rows, .. } => rows.push(row),
                _ => return Err(OnlineError::Malformed),
            },
        }
        Ok(())
    }
}

/// A kind of the social service (social, guilds, friends).
fn is_social_kind(k: u8) -> bool {
    msg::kind::SOCIAL.contains(&k)
        || msg::kind::GUILD.contains(&k)
        || msg::kind::FRIENDS.contains(&k)
}

#[cfg(test)]
#[path = "social_tests.rs"]
mod tests;
