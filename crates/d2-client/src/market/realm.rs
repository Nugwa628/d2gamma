// D2RUST's own: the realm's market behind the window's seam.
//! [`RealmSource`]: the window's [`MarketSource`] on a realm, over the
//! online client's [`MarketClient`] and the account's main-server session
//! (contract section 8). It never waits on the realm: in a game it reads
//! the market's queue of the realm reader's inbox and queues its asks in
//! the reader's outbox ([`crate::app::realm_pump`]), which goes out on the
//! next frame the session is free. The pages the client assembled come
//! back as the window's events ([`super::wire`]), every item record read
//! with the game data. Outside a game ([`RealmSource::new`]: tests, tools)
//! the source runs a reader of its own on the session.
//!
//! - A change (buy, offer, answer, post, shop edit, claim) before the
//!   realm's first state is answered here: the market isn't ready yet.
//! - A failed session loses what was in flight; once the reader has a
//!   session again (a reconnect: a new one) the client starts over and
//!   asks the realm's state again (whose last request number it goes on
//!   from). A market body that doesn't read is handled the same way.
//! - The holdings are the online stash's ([`super::holdings`]); a change
//!   the realm answered marks them stale so the stash reads them again.
//! - What else arrives on the session goes to its own reader's queue
//!   (chat, ladder, the lobby's list); a stash answer stays queued in the
//!   session for the stash's own request.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use bevy::log::warn;
use d2_net::msg::market::{self as w, ToRealm};
use d2_online::{MarketAnswer, MarketClient, Session};

use crate::app::realm_pump::{Owner, RealmInbox, RealmOutbox, RealmPump};

use super::holdings::SharedBook;
use super::prices::DEFAULT_DAYS;
use super::source::{
    Group, ItemView, MarketAsk, MarketCode, MarketDone, MarketEvent, MarketSource, Req,
};
use super::trade::Holdings;
use super::wire::{self, Reading};
use crate::app::online::OnlineSession;
use crate::ui::item_tip::ItemTips;
use crate::ui::market::tip::read_item;

/// What a change before the realm's first state reads.
pub const NOT_READY: &str = "The market isn't ready yet; try again.";
/// What an ask the session lost reads.
pub const LOST: &str = "The realm didn't get that; try again.";

/// How long after a failure the market starts over.
const RETRY: Duration = Duration::from_secs(1);

/// The numbers of the answers made here, far above the realm's.
const LOCAL_REQ: Req = 1 << 62;

/// The window's market on the realm.
pub struct RealmSource {
    inbox: RealmInbox,
    outbox: RealmOutbox,
    /// The reader of its own outside a game ([`RealmSource::new`]).
    own: Option<RealmPump<Session>>,
    /// The reader's failures seen so far.
    losses: u64,
    client: MarketClient,
    /// Numbered asks waiting for the session.
    out: VecDeque<ToRealm>,
    /// Answers made here, for the next poll.
    local: Vec<MarketEvent>,
    next_local: Req,
    tips: Option<ItemTips>,
    book: Option<SharedBook>,
    /// The window's asks in flight: a failed session answers them here.
    asked: BTreeSet<Req>,
    /// The changes in flight: their answer marks the holdings stale.
    changes: BTreeSet<Req>,
    /// The span of each price check in flight.
    days: BTreeMap<Req, u16>,
    /// The session failed (or sent a market body that doesn't read): when
    /// to start over, once the reader has a session again.
    failed: Option<Instant>,
}

impl RealmSource {
    /// The market on `session`'s realm with a reader of its own, polled
    /// with the source (outside a game: tests, tools); `tips` reads the
    /// item records, `book` holds what the online stash read.
    pub fn new(session: OnlineSession, tips: Option<ItemTips>, book: Option<SharedBook>) -> Self {
        let pump = RealmPump::new(session.0.clone());
        let mut s = Self::on_reader(pump.inbox().clone(), pump.outbox().clone(), tips, book);
        s.own = Some(pump);
        s
    }

    /// The market of a game: it reads the market's queue of the game's
    /// realm reader (`inbox`) and sends through its `outbox`.
    pub fn on_reader(
        inbox: RealmInbox,
        outbox: RealmOutbox,
        tips: Option<ItemTips>,
        book: Option<SharedBook>,
    ) -> Self {
        let losses = inbox.link().losses;
        RealmSource {
            inbox,
            outbox,
            own: None,
            losses,
            client: MarketClient::new(),
            out: VecDeque::new(),
            local: Vec::new(),
            next_local: LOCAL_REQ,
            tips,
            book,
            asked: BTreeSet::new(),
            changes: BTreeSet::new(),
            days: BTreeMap::new(),
            failed: None,
        }
    }

    /// Answers request `req` here with `text`.
    fn refuse(&mut self, req: Req, text: &str) {
        self.local.push(MarketEvent::Done(MarketDone {
            req,
            code: MarketCode::Other(u16::MAX),
            wait_s: 0,
            repeated: false,
            id: None,
            rows: Vec::new(),
            text: text.into(),
        }));
    }

    /// The session failed (or the realm broke the market's framing): what
    /// is in flight is lost.
    fn fail(&mut self) {
        self.client.reset();
        self.out.clear();
        self.changes.clear();
        self.days.clear();
        for req in std::mem::take(&mut self.asked) {
            self.refuse(req, LOST);
        }
        self.failed = Some(Instant::now() + RETRY);
    }

    /// One frame on the reader's queues: a failure the reader saw loses
    /// what is in flight; a while later, with a session again (a
    /// reconnect: a new one), the client starts over and asks the realm's
    /// state (it numbers on from the state's last request).
    fn step(&mut self) {
        let link = self.inbox.link();
        if link.losses != self.losses {
            self.losses = link.losses;
            if self.failed.is_none() {
                self.fail();
            }
        }
        if let Some(at) = self.failed {
            // What arrives meanwhile answers asks that are lost.
            self.inbox.take(Owner::Market);
            if Instant::now() < at || link.down {
                return;
            }
            self.failed = None;
            self.client.reset();
            if let Some(m) = self.client.prepare(w::Ask::Hello) {
                self.out.push_back(m);
            }
        }
        for m in self.out.drain(..) {
            self.outbox.send(m.body());
        }
        for body in self.inbox.take(Owner::Market) {
            if let Err(e) = self.client.feed(&body) {
                warn!("market: the realm broke the market's messages: {e}");
                self.fail();
                return;
            }
        }
    }

    fn next_req_here(&mut self) -> Req {
        self.next_local += 1;
        self.next_local
    }

    /// The window's event for an answer the client assembled.
    fn event(&mut self, a: MarketAnswer) -> MarketEvent {
        if let Some(req) = a.req() {
            self.asked.remove(&req);
            if self.changes.remove(&req) {
                if let Some(b) = &self.book {
                    b.lock().unwrap_or_else(|e| e.into_inner()).touch();
                }
            }
        }
        let tips = self.tips.as_ref();
        let item = |r: &[u8]| tips.and_then(|t| read_item(t, r));
        let name = |c: &str| tips.and_then(|t| t.base_name(c));
        let read = Reading {
            item: &item,
            name: &name,
        };
        match a {
            MarketAnswer::State(s) => wire::state(s),
            MarketAnswer::Notice(n) => wire::notice(n),
            MarketAnswer::Done(d) => {
                self.days.remove(&d.req);
                wire::done(d)
            }
            MarketAnswer::Error { req, error } => {
                let req = req.unwrap_or(0);
                self.days.remove(&req);
                wire::error(req, error)
            }
            MarketAnswer::Results {
                req,
                head,
                facets,
                rows,
            } => wire::results(req, head, facets, rows, &read),
            MarketAnswer::Listing { req, detail, .. } => wire::listing(req, *detail, &read),
            MarketAnswer::Shop { req, head, rows } => wire::shop(req, head, rows, &read),
            MarketAnswer::Offers {
                req,
                head,
                rows,
                items,
            } => wire::offers(req, head, rows, items, &read),
            MarketAnswer::Deliveries { req, head, rows } => {
                wire::deliveries(req, head, rows, &read)
            }
            MarketAnswer::Prices {
                req,
                head,
                series,
                recent,
            } => {
                let days = self.days.remove(&req).unwrap_or(DEFAULT_DAYS);
                wire::prices(req, head, series, recent, days)
            }
        }
    }
}

impl MarketSource for RealmSource {
    fn send(&mut self, ask: MarketAsk) -> Req {
        if self.failed.is_some() {
            let req = self.next_req_here();
            self.refuse(req, LOST);
            return req;
        }
        let days = match &ask {
            MarketAsk::Prices { days, .. } => Some(*days),
            _ => None,
        };
        let a = wire::ask(&ask);
        let changes = a.changes();
        match self.client.prepare(a) {
            Some(m) => {
                let req = m.req;
                self.asked.insert(req);
                if changes {
                    self.changes.insert(req);
                }
                if let Some(d) = days {
                    self.days.insert(req, d);
                }
                self.out.push_back(m);
                req
            }
            None => {
                let req = self.next_req_here();
                self.refuse(req, NOT_READY);
                req
            }
        }
    }

    fn poll(&mut self) -> Vec<MarketEvent> {
        // Outside a game the source runs its own reader: what it queued
        // goes out, then what arrived is read.
        if let Some(mut p) = self.own.take() {
            self.step();
            p.step(Instant::now());
            self.own = Some(p);
        }
        self.step();
        let mut events = std::mem::take(&mut self.local);
        for a in self.client.take() {
            let e = self.event(a);
            events.push(e);
        }
        events.append(&mut self.local);
        events
    }

    fn holdings(&self) -> Option<Holdings> {
        let book = self.book.as_ref()?;
        let tips = self.tips.as_ref();
        let item = |r: &[u8]| -> Option<(Group, ItemView)> { tips.and_then(|t| read_item(t, r)) };
        let name = |c: &str| tips.and_then(|t| t.base_name(c));
        let read = Reading {
            item: &item,
            name: &name,
        };
        let b = book.lock().unwrap_or_else(|e| e.into_inner());
        b.holdings(&read)
    }
}
