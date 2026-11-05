//! The market link of a mod's own market window: a synthetic market and a
//! mod that asks through the link, and D2Gamma's window beside it.

use std::sync::{Arc, Mutex};

use super::*;
use crate::market::fake::FakeSource;
use crate::market::source::{
    MarketCode, MarketDone, MarketQuery, MarketSource, MarketState, OffReason,
};
use crate::market::trade::{Notice, NoticeWhat};

/// A market that answers every ask with a `Done` carrying its number,
/// `Hello` with its state, and sends `pushes` with the next poll.
struct Scripted {
    on: bool,
    next: Req,
    waiting: Vec<(Req, MarketAsk)>,
    pushes: Vec<MarketEvent>,
    asked: Arc<Mutex<Vec<MarketAsk>>>,
}

impl Scripted {
    fn new(on: bool) -> (Scripted, Arc<Mutex<Vec<MarketAsk>>>) {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let s = Scripted {
            on,
            next: 1,
            waiting: Vec::new(),
            pushes: Vec::new(),
            asked: asked.clone(),
        };
        (s, asked)
    }
}

fn done(req: Req) -> MarketEvent {
    MarketEvent::Done(MarketDone {
        req,
        code: MarketCode::Done,
        wait_s: 0,
        repeated: false,
        id: None,
        rows: Vec::new(),
        text: String::new(),
    })
}

impl MarketSource for Scripted {
    fn send(&mut self, ask: MarketAsk) -> Req {
        let req = self.next;
        self.next += 1;
        self.asked.lock().unwrap().push(ask.clone());
        self.waiting.push((req, ask));
        req
    }

    fn poll(&mut self) -> Vec<MarketEvent> {
        let mut out = std::mem::take(&mut self.pushes);
        for (req, ask) in self.waiting.drain(..) {
            out.push(match ask {
                MarketAsk::Hello => MarketEvent::State {
                    req,
                    state: MarketState {
                        on: self.on,
                        off_reason: if self.on {
                            OffReason::On
                        } else {
                            OffReason::Switch
                        },
                        mod_id: "mymod".into(),
                        ..MarketState::default()
                    },
                },
                _ => done(req),
            });
        }
        out
    }
}

fn boxed(s: impl MarketSource + 'static) -> Option<Box<dyn MarketSource>> {
    Some(Box::new(s))
}

/// One engine frame with no D2Gamma window.
fn frame(link: &ModMarket, online: bool, market: &mut Option<Box<dyn MarketSource>>) {
    link.frame(online, market, |_| {});
}

fn reqs(events: &[MarketEvent]) -> Vec<Req> {
    events.iter().map(MarketEvent::req).collect()
}

#[test]
fn the_mods_asks_reach_the_market_and_its_answers_come_back() {
    let link = ModMarket::new();
    let mods = ClientMods::new().with_market_link(link.clone());
    let (s, asked) = Scripted::new(true);
    let mut market = boxed(s);
    // The engine hands the game's market over and asks its state.
    frame(mods.market_link().unwrap(), true, &mut market);
    assert!(market.is_none(), "the link keeps the market");
    assert_eq!(link.availability(), Availability::Open);
    assert_eq!(link.state().unwrap().mod_id, "mymod");
    let first = link.take_events();
    assert!(matches!(first.as_slice(), [MarketEvent::State { .. }]));

    // A search through a clone the mod's panel holds.
    let panel = link.clone();
    let req = panel
        .send(MarketAsk::Search(MarketQuery::default()))
        .unwrap();
    assert!(matches!(
        asked.lock().unwrap().last(),
        Some(MarketAsk::Search(_))
    ));
    // Nothing before the engine's frame polls the market.
    assert!(panel.take_events().is_empty());
    frame(&link, true, &mut None);
    assert_eq!(reqs(&panel.take_events()), vec![req]);
}

#[test]
fn single_player_has_no_market_to_ask() {
    let link = ModMarket::new();
    let mut market = None;
    let mut window_got = Vec::new();
    link.frame(false, &mut market, |src| window_got.push(src.is_some()));
    assert_eq!(window_got, vec![false]);
    assert!(!link.online());
    assert_eq!(link.availability(), Availability::SinglePlayer);
    assert_eq!(link.availability().why_not(), Some("Online only"));
    assert_eq!(link.send(MarketAsk::Hello), None);
    assert!(link.holdings().is_none());
}

#[test]
fn a_realm_with_the_market_off_greys_the_mods_button() {
    let link = ModMarket::new();
    let (s, _) = Scripted::new(false);
    let mut market = boxed(s);
    link.frame(true, &mut market, |_| {});
    assert_eq!(link.availability(), Availability::Closed);
    let st = link.state().unwrap();
    assert!(!st.on);
    assert_eq!(st.off_reason, OffReason::Switch);
    assert_eq!(
        link.availability().why_not(),
        Some("The market is closed on this realm")
    );
    // Going offline forgets the state; back online asks it again.
    frame(&link, false, &mut None);
    assert_eq!(link.availability(), Availability::SinglePlayer);
    assert!(link.state().is_none());
    link.take_events();
    frame(&link, true, &mut None);
    assert!(link.state().is_some());
    assert!(matches!(
        link.take_events().as_slice(),
        [MarketEvent::State { .. }]
    ));
}

#[test]
fn d2gammas_window_and_the_mod_share_the_market_by_request() {
    let link = ModMarket::new();
    let (mut s, asked) = Scripted::new(true);
    let notice = MarketEvent::Notice(Notice {
        what: NoticeWhat::Sold,
        listing: 9,
        offer: None,
        about: "Ring".into(),
        who: "Buyer".into(),
        price: None,
        deliveries_waiting: 1,
        at_ms: 0,
    });
    s.pushes.push(notice.clone());
    let mut market = boxed(s);
    let mut window_events = Vec::new();
    let mut window_req = 0;
    link.frame(true, &mut market, |src| {
        let src = src.unwrap();
        window_req = src.send(MarketAsk::Hello);
        window_events = src.poll();
    });
    // The window's own Hello answer and the push reach the window; the
    // link's Hello answer (a state) and the push reach the mod.
    assert!(window_events.contains(&notice));
    assert!(window_events.iter().any(|e| e.req() == window_req));
    let mod_events = link.take_events();
    assert!(mod_events.contains(&notice));
    assert!(mod_events
        .iter()
        .all(|e| matches!(e, MarketEvent::State { .. } | MarketEvent::Notice(_))));

    // A mod ask and a window ask in one frame: each answer to its asker.
    let mine = link.send(MarketAsk::DeliveriesOpen { page: 0 }).unwrap();
    let mut theirs = 0;
    let mut got = Vec::new();
    link.frame(true, &mut None, |src| {
        let src = src.unwrap();
        theirs = src.send(MarketAsk::DeliveriesOpen { page: 1 });
        got = src.poll();
    });
    assert_eq!(reqs(&got), vec![theirs]);
    assert_eq!(reqs(&link.take_events()), vec![mine]);
    assert_eq!(asked.lock().unwrap().len(), 4);
}

#[test]
fn a_mod_that_never_takes_its_answers_keeps_only_the_newest() {
    let link = ModMarket::new();
    let (s, _) = Scripted::new(true);
    let mut market = boxed(s);
    frame(&link, true, &mut market);
    for _ in 0..MAX_WAITING + 10 {
        link.send(MarketAsk::DeliveriesOpen { page: 0 });
    }
    frame(&link, true, &mut None);
    let events = link.take_events();
    assert_eq!(events.len(), MAX_WAITING);
    assert_eq!(events.last().unwrap().req(), (MAX_WAITING + 11) as Req);
}

#[test]
fn the_fake_market_answers_through_the_link() {
    let link = ModMarket::new();
    let mut market = boxed(FakeSource::new(&|_| None, false, 0, true));
    frame(&link, true, &mut market);
    assert_eq!(link.availability(), Availability::Open);
    let req = link
        .send(MarketAsk::Search(MarketQuery::default()))
        .unwrap();
    frame(&link, true, &mut None);
    let events = link.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, MarketEvent::Results { req: r, .. } if *r == req)));
}

#[test]
fn d2gamma_registers_no_link() {
    let none = ClientMods::new();
    assert!(none.market_link().is_none());
    assert!(none.is_empty());
    let with = ClientMods::new().with_market_link(ModMarket::new());
    assert!(with.market_link().is_some());
    assert!(!with.is_empty());
}

#[test]
fn a_mod_that_draws_the_window_takes_its_open_asks() {
    let link = ModMarket::new();
    assert!(!link.owns_window());
    // Every clone is the same link.
    let mine = link.clone().draws_window();
    assert!(link.owns_window() && mine.owns_window());
    assert!(link.take_open_asks().is_empty());
    link.ask_open(OpenAsk::Key);
    link.ask_open(OpenAsk::Stash);
    assert_eq!(mine.take_open_asks(), [OpenAsk::Key, OpenAsk::Stash]);
    assert!(mine.take_open_asks().is_empty());
    // A mod that never takes them keeps only the newest.
    for _ in 0..MAX_OPENS + 3 {
        link.ask_open(OpenAsk::Key);
    }
    link.ask_open(OpenAsk::Stash);
    let asks = link.take_open_asks();
    assert_eq!(asks.len(), MAX_OPENS);
    assert_eq!(asks.last(), Some(&OpenAsk::Stash));
}

#[test]
fn the_link_says_whether_the_stash_is_open() {
    let link = ModMarket::new();
    assert!(!link.at_stash());
    link.set_at_stash(true);
    assert!(link.clone().at_stash());
    link.set_at_stash(false);
    assert!(!link.at_stash());
}
