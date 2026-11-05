//! A mod game's link to the realm's market ([`ClientMods::with_market_link`]):
//! a mod that draws its own market window sends the realm's market
//! requests through a [`ModMarket`] and gets the answers back as events.
//!
//! The requests and answers are the market contract's
//! (`docs/plans/server/market-contract.md`), as the client's market seam
//! carries them ([`MarketAsk`], [`MarketEvent`]): browse and search, a
//! listing, buy, offer, answer, post, shop edits and pages, offers,
//! deliveries, claim and price checks. The market behind the link is the
//! one D2Gamma's own window asks: the realm's in an online game, the
//! in-process fake with `D2RUST_MARKET=fake`, none in single player.
//!
//! - The mod holds a clone of the [`ModMarket`] it registered (its panel
//!   factory hands one to each panel it makes); every clone is the same
//!   link.
//! - [`ModMarket::send`] numbers an ask and queues it for the realm at
//!   once; `None` when there is no market to ask (single player). The
//!   answers repeat the number; [`ModMarket::take_events`] gives them, with
//!   the realm's pushes (notices, state), in the order they came.
//! - The link asks the realm's state itself when the game goes online, so
//!   [`ModMarket::availability`] says whether the market is open before
//!   any window is: a mod's Market button greys with its
//!   [`Availability::why_not`] ("Online only", "The market is closed on
//!   this realm").
//! - D2Gamma's own window keeps working beside it: the engine shares the
//!   market between the two by request number (an answer goes to whoever
//!   asked; pushes and states go to both), so a mod may keep D2Gamma's
//!   window and add its own panels, or replace the window entirely.
//! - A mod that replaces the window says so ([`ModMarket::draws_window`]):
//!   the market key (Ctrl+I by default) and the stash's Market buttons
//!   (D2Gamma's, and a mod stash panel's `StashOut::OpenMarket`) then
//!   open nothing of D2Gamma's; each press is queued on the link as an
//!   [`OpenAsk`] for the mod's panel ([`ModMarket::take_open_asks`], read
//!   on its tick). [`ModMarket::at_stash`] says whether the stash panel is
//!   open (selling and claiming are at the stash only).
//!
//! Without a registered link nothing here runs: the market is pumped to
//! D2Gamma's window exactly as before (`app/market.rs`).

use std::collections::{BTreeSet, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::market::source::{MarketAsk, MarketEvent, MarketSource, MarketState, Req, StashGoods};
use crate::market::trade::Holdings;
use crate::ui::market::Availability;

use super::ClientMods;

/// The most answers a link keeps for a mod that doesn't take them; the
/// oldest go first.
pub const MAX_WAITING: usize = 4096;

/// A mod game's link to the market (see the module). Cheap to clone; every
/// clone is the same link.
#[derive(Clone, Default)]
pub struct ModMarket(Arc<Mutex<Link>>);

#[derive(Default)]
struct Link {
    /// The market of the game, once the engine handed it over.
    source: Option<Box<dyn MarketSource>>,
    /// The game is online (its link is a realm's, or the fake market is
    /// on).
    online: bool,
    /// The realm's last stated market state; `None` until it answers.
    state: Option<MarketState>,
    /// The link asked the realm's state for this source.
    hello: bool,
    /// The mod's requests waiting for their answer.
    mine: BTreeSet<Req>,
    /// Answers and pushes for the mod, oldest first.
    events: VecDeque<MarketEvent>,
    /// The mod draws the market window in place of D2Gamma's.
    draws_window: bool,
    /// The presses that ask the mod's window to open, oldest first.
    opens: Vec<OpenAsk>,
    /// The stash panel is open (set by the engine each frame).
    at_stash: bool,
}

/// A press that asks a mod's market window to open
/// ([`ModMarket::take_open_asks`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenAsk {
    /// The market key: opens the window, or closes an open one.
    Key,
    /// A Market button of the stash: opens the window at the stash.
    Stash,
}

/// The most open asks a link keeps for a mod that doesn't take them.
const MAX_OPENS: usize = 16;

impl fmt::Debug for ModMarket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let l = self.lock();
        f.debug_struct("ModMarket")
            .field("source", &l.source.is_some())
            .field("online", &l.online)
            .field("state", &l.state.is_some())
            .field("waiting", &l.mine.len())
            .field("events", &l.events.len())
            .field("draws_window", &l.draws_window)
            .field("at_stash", &l.at_stash)
            .finish()
    }
}

impl ModMarket {
    /// A link with no market yet: the engine hands it the game's market
    /// once the mod registered it ([`ClientMods::with_market_link`]).
    pub fn new() -> ModMarket {
        ModMarket::default()
    }

    fn lock(&self) -> MutexGuard<'_, Link> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The mod draws the market window itself (see the module): the market
    /// key and the stash's Market buttons ask the mod's window to open
    /// ([`ModMarket::take_open_asks`]) and D2Gamma's window never opens.
    /// Every clone of the link is changed.
    pub fn draws_window(self) -> ModMarket {
        self.lock().draws_window = true;
        self
    }

    /// Whether the mod draws the market window ([`ModMarket::draws_window`]).
    pub fn owns_window(&self) -> bool {
        self.lock().draws_window
    }

    /// Queues a press that asks the mod's window to open (the engine's
    /// market key and Market buttons, when the mod draws the window).
    pub fn ask_open(&self, ask: OpenAsk) {
        let mut l = self.lock();
        l.opens.push(ask);
        if l.opens.len() > MAX_OPENS {
            l.opens.remove(0);
        }
    }

    /// The presses that asked the mod's window to open since the last
    /// call, oldest first; the mod's panel opens (or, for
    /// [`OpenAsk::Key`] on an open window, closes) itself.
    pub fn take_open_asks(&self) -> Vec<OpenAsk> {
        std::mem::take(&mut self.lock().opens)
    }

    /// Whether the game's stash panel is open (deliveries are claimed and
    /// goods sold at the stash only), as of the engine's last frame.
    pub fn at_stash(&self) -> bool {
        self.lock().at_stash
    }

    /// The engine's frame: whether the stash panel is open.
    pub fn set_at_stash(&self, at: bool) {
        self.lock().at_stash = at;
    }

    /// The game is online and has a market to ask (a realm's game, or the
    /// fake market); single player is not.
    pub fn online(&self) -> bool {
        let l = self.lock();
        l.online && l.source.is_some()
    }

    /// The market as the realm last stated it (its switch, limits, fees,
    /// currencies, counts); `None` offline and until the realm answers.
    pub fn state(&self) -> Option<MarketState> {
        self.lock().state.clone()
    }

    /// Whether a market window can open, as D2Gamma's own window answers
    /// it: single player, closed on this realm (or not answered yet), or
    /// open.
    pub fn availability(&self) -> Availability {
        let l = self.lock();
        match (&l.state, l.online && l.source.is_some()) {
            (_, false) => Availability::SinglePlayer,
            (Some(s), true) if s.on => Availability::Open,
            _ => Availability::Closed,
        }
    }

    /// Numbers `ask` and queues it for the market (it goes out on the next
    /// frame); its answer comes from [`ModMarket::take_events`] with the
    /// same number. `None` when there is no market to ask: the game is
    /// offline.
    pub fn send(&self, ask: MarketAsk) -> Option<Req> {
        let mut l = self.lock();
        if !l.online {
            return None;
        }
        let req = l.source.as_mut()?.send(ask);
        l.mine.insert(req);
        Some(req)
    }

    /// The answers to the mod's asks and the market's pushes since the
    /// last call, in the order they came.
    pub fn take_events(&self) -> Vec<MarketEvent> {
        self.lock().events.drain(..).collect()
    }

    /// What the account's online shared stash holds, for an offer form;
    /// `None` until known (and offline).
    pub fn holdings(&self) -> Option<Holdings> {
        self.lock().source.as_ref()?.holdings()
    }

    /// The mod shows stash item `goods` for sale (the fake market keeps
    /// it so a post of it can be listed; the realm reads the stash
    /// itself).
    pub fn stash_item_seen(&self, goods: &StashGoods) {
        if let Some(s) = self.lock().source.as_mut() {
            s.stash_item_seen(goods);
        }
    }

    /// The engine's frame step: takes the game's market from `market` when
    /// it has one (the link keeps it from then on), asks the realm's state
    /// when the game went online, then runs `window` (D2Gamma's own
    /// window's pump) on the shared market, or, without a market, on
    /// none. What the market answered is shared out: the mod's answers to
    /// the mod, the window's to the window, pushes and states to both.
    pub fn frame(
        &self,
        online: bool,
        market: &mut Option<Box<dyn MarketSource>>,
        window: impl FnOnce(Option<&mut dyn MarketSource>),
    ) {
        let mut l = self.lock();
        if let Some(src) = market.take() {
            l.source = Some(src);
            l.hello = false;
            l.state = None;
            l.mine.clear();
        }
        if !online && l.online {
            l.state = None;
            l.hello = false;
        }
        l.online = online;
        if !online || l.source.is_none() {
            drop(l);
            window(None);
            return;
        }
        if !l.hello {
            l.hello = true;
            if let Some(req) = l.source.as_mut().map(|s| s.send(MarketAsk::Hello)) {
                l.mine.insert(req);
            }
        }
        let mut shared = Shared {
            link: &mut l,
            polled: false,
        };
        window(Some(&mut shared));
        if !shared.polled {
            // No window polled this frame: everything is the mod's.
            let _ = shared.poll();
        }
    }
}

/// The market as D2Gamma's window reaches it while a mod link shares it:
/// the window's asks go straight to the market; a poll keeps the mod's
/// answers back for the mod.
struct Shared<'a> {
    link: &'a mut Link,
    polled: bool,
}

impl MarketSource for Shared<'_> {
    fn send(&mut self, ask: MarketAsk) -> Req {
        match self.link.source.as_mut() {
            Some(s) => s.send(ask),
            None => 0,
        }
    }

    fn poll(&mut self) -> Vec<MarketEvent> {
        self.polled = true;
        let l = &mut *self.link;
        let Some(src) = l.source.as_mut() else {
            return Vec::new();
        };
        let mut window = Vec::new();
        for e in src.poll() {
            let req = e.req();
            let push = matches!(e, MarketEvent::State { .. } | MarketEvent::Notice(_)) || req == 0;
            if let MarketEvent::State { state, .. } = &e {
                l.state = Some(state.clone());
            }
            let mine = l.mine.remove(&req);
            if push {
                window.push(e.clone());
                l.events.push_back(e);
            } else if mine {
                l.events.push_back(e);
            } else {
                window.push(e);
            }
        }
        while l.events.len() > MAX_WAITING {
            l.events.pop_front();
        }
        window
    }

    fn holdings(&self) -> Option<Holdings> {
        self.link.source.as_ref()?.holdings()
    }

    fn stash_item_seen(&mut self, goods: &StashGoods) {
        if let Some(s) = self.link.source.as_mut() {
            s.stash_item_seen(goods);
        }
    }
}

impl ClientMods {
    /// Registers the mod game's market link (see [`ModMarket`]): from then
    /// on the game's market is shared between D2Gamma's window and the
    /// mod, which sends asks and takes answers through `link` (keep a
    /// clone of it for the mod's panels). A second call replaces the
    /// first.
    pub fn with_market_link(mut self, link: ModMarket) -> ClientMods {
        self.market = Some(link);
        self
    }

    /// The registered market link, when one is.
    pub fn market_link(&self) -> Option<&ModMarket> {
        self.market.as_ref()
    }
}

#[cfg(test)]
#[path = "market_link_tests.rs"]
mod tests;
