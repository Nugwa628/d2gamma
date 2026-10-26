// D2RUST's own: the market window.
//! The market window (module K2 of `docs/plans/market-ui.md`): a window
//! over the game with four tabs, Browse, My shop, Offers and Deliveries.
//! Browse is the browser: filters with live counts on the left
//! ([`filters`]), the results as compact coloured names grouped by quality
//! ([`grid`]) or as a sortable table ([`table`]) in the middle, and the
//! picked listing's tip, its comparison with the equipped item and Buy /
//! Offer... on the right ([`details`]), with its price check: what such
//! items sold for lately, as a graph per currency ([`price_graph`]). My
//! shop lists the seller's listings and edits them in bulk ([`shop`]);
//! items are listed from the shared stash's Market category with the New
//! listing form ([`sell`]). Offers and Deliveries come later ([`offers`],
//! [`deliveries`]).
//!
//! - It opens with the market key (Ctrl+I by default, a Controls setting,
//!   [`MarketKey`]) and with the stash panel's Market button, only online
//!   with the realm's market on: in single player the button is greyed
//!   ("Online only") and nothing opens ([`Availability`]).
//! - It decides nothing: searches and buys are [`MarketAsk`]s to the
//!   realm's market ([`crate::market::source::MarketSource`]); what the
//!   market gives goes to Deliveries and from there to the shared stash,
//!   never to the inventory.
//! - Every price is shown exactly, in the currencies it is asked in; the
//!   buy confirm adds the realm's buyer fee (none by default) and the buy
//!   carries the price's hash and the fee revision the player saw.
//!
//! It is drawn like the shared stash's panel: on the game's palette
//! surface, in the Display v3 kit's colours, scaled with the UI
//! ([`window`]); its parts are named UI elements (`market.tab(Browse)`,
//! `market.filter.quality(Unique)`, `market.result(3)`, `market.buy`), laid
//! out once for the draw, the clicks and the test driver.

pub mod deliveries;
pub mod details;
pub mod filters;
pub mod grid;
pub mod notices;
pub mod offer_form;
pub mod offers;
pub mod price_graph;
pub mod sell;
pub mod shop;
pub mod table;
pub mod tip;
pub mod window;

use std::collections::{BTreeMap, BTreeSet};

pub use filters::{Filters, Paging};

use crate::market::source::{
    Facets, ListingDetail, ListingId, ListingRow, MarketAsk, MarketCode, MarketDone, MarketEvent,
    MarketFees, MarketSort, MarketSource, MarketState, Price, Req, ResultsPage,
};
use crate::market::trade::Holdings;

pub use deliveries::DeliveriesTab;
pub use offer_form::BundleForm;
pub use offers::OffersTab;

/// Whether the window can open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Availability {
    /// A single-player game: the market is online only.
    #[default]
    SinglePlayer,
    /// Online, but the realm's market is off (or has not answered yet).
    Closed,
    Open,
}

impl Availability {
    /// The Market button's tip when the window cannot open (contract 4).
    pub fn why_not(self) -> Option<&'static str> {
        match self {
            Availability::SinglePlayer => Some("Online only"),
            Availability::Closed => Some("The market is closed on this realm"),
            Availability::Open => None,
        }
    }
}

/// The window's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarketTab {
    #[default]
    Browse,
    MyShop,
    Offers,
    Deliveries,
}

impl MarketTab {
    pub const ALL: [MarketTab; 4] = [
        MarketTab::Browse,
        MarketTab::MyShop,
        MarketTab::Offers,
        MarketTab::Deliveries,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MarketTab::Browse => "Browse",
            MarketTab::MyShop => "My shop",
            MarketTab::Offers => "Offers",
            MarketTab::Deliveries => "Deliveries",
        }
    }

    /// The word of its UI id (`market.tab(MyShop)`).
    pub fn key(self) -> &'static str {
        match self {
            MarketTab::Browse => "Browse",
            MarketTab::MyShop => "MyShop",
            MarketTab::Offers => "Offers",
            MarketTab::Deliveries => "Deliveries",
        }
    }
}

/// How the results are shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResultsView {
    /// Compact coloured names, grouped by quality.
    #[default]
    Grid,
    /// One row per listing with its key stats, price and seller.
    Table,
}

impl ResultsView {
    pub fn key(self) -> &'static str {
        match self {
            ResultsView::Grid => "grid",
            ResultsView::Table => "table",
        }
    }
}

/// A text box of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextBox {
    /// The name search.
    Search,
    /// The type tree's "find a type".
    TypeFind,
    /// The offer form's gold.
    Gold,
    /// My shop's Set price... box's gold.
    ShopGold,
    /// My shop's Note... box.
    ShopNote,
}

/// The key that opens and closes the window: Ctrl held with a letter, a
/// Controls setting (`[controls] market_key`, the index of [`Self::ALL`]).
/// The game's bindings are single keys; this chord is read before them,
/// so the letter's own binding (I: the inventory) does not run with Ctrl
/// held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarketKey {
    #[default]
    CtrlI,
    CtrlM,
    CtrlK,
    CtrlJ,
    CtrlU,
    Off,
}

impl MarketKey {
    pub const ALL: [MarketKey; 6] = [
        MarketKey::CtrlI,
        MarketKey::CtrlM,
        MarketKey::CtrlK,
        MarketKey::CtrlJ,
        MarketKey::CtrlU,
        MarketKey::Off,
    ];

    /// The names the settings row shows, in [`Self::ALL`] order.
    pub const NAMES: [&'static str; 6] = ["Ctrl+I", "Ctrl+M", "Ctrl+K", "Ctrl+J", "Ctrl+U", "Off"];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }

    pub fn of_index(i: i64) -> MarketKey {
        Self::ALL[(i.max(0) as usize).min(Self::ALL.len() - 1)]
    }

    pub fn name(self) -> &'static str {
        Self::NAMES[self.index()]
    }

    /// The letter held with Ctrl, `None` when off.
    pub fn letter(self) -> Option<char> {
        match self {
            MarketKey::CtrlI => Some('I'),
            MarketKey::CtrlM => Some('M'),
            MarketKey::CtrlK => Some('K'),
            MarketKey::CtrlJ => Some('J'),
            MarketKey::CtrlU => Some('U'),
            MarketKey::Off => None,
        }
    }

    /// The letter's key.
    pub fn key(self) -> Option<crate::controls::Key> {
        use crate::controls::Key;
        Some(match self.letter()? {
            'I' => Key::I,
            'M' => Key::M,
            'K' => Key::K,
            'J' => Key::J,
            _ => Key::U,
        })
    }

    /// Whether `key` pressed with Ctrl held (`ctrl`) is this chord.
    pub fn is_chord(self, key: crate::controls::Key, ctrl: bool) -> bool {
        ctrl && self.key() == Some(key)
    }
}

/// The page sizes offered (the realm takes 25, 50 and 100).
pub const PAGE_SIZES: [u8; 3] = crate::market::source::PAGE_SIZES;

/// The buy confirm: the exact price, the realm's buyer fee and what the
/// buy sends back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuyConfirm {
    pub listing: ListingId,
    pub name: String,
    pub price: Price,
    pub price_hash: [u8; 32],
    /// Gold added to the price (0 today).
    pub buyer_fee: u64,
    pub fee_rev: u32,
}

/// The market window's state.
#[derive(Debug, Default)]
pub struct MarketUi {
    /// The game is online (a realm game, or the fake market's test
    /// config).
    pub online: bool,
    /// The realm's last stated market.
    pub state: Option<MarketState>,
    pub open: bool,
    pub tab: MarketTab,
    pub filters: Filters,
    /// "Save as default": what Reset and a new window start from.
    pub default_filters: Filters,
    /// The type tree's open parents.
    pub tree_open: BTreeSet<usize>,
    /// The type tree's first shown row (the wheel scrolls it).
    pub tree_top: usize,
    pub type_find: String,
    pub view: ResultsView,
    pub grid_size: u8,
    pub table_size: u8,
    pub sort: MarketSort,
    pub desc: bool,
    pub page: u16,
    pub results: Option<ResultsPage>,
    /// The results' first shown row (the wheel scrolls it).
    pub scroll: usize,
    /// The last facet counts (a page flip does not ask for new ones).
    pub facets: Facets,
    /// The search an answer is awaited for: queued, then numbered.
    pub search_queued: bool,
    pub search_req: Option<Req>,
    pub get_req: Option<Req>,
    pub buy_req: Option<Req>,
    /// The listing shown on the right, and its looked-up detail.
    pub selected: Option<ListingRow>,
    pub detail: Option<ListingDetail>,
    pub confirm: Option<BuyConfirm>,
    /// The offer form (Offer... on a listing, Counter... on an offer).
    pub form: Option<BundleForm>,
    /// The Offers tab.
    pub offers: OffersTab,
    /// The Deliveries tab.
    pub deliveries: DeliveriesTab,
    /// What the shared stash holds, for the offer form.
    pub holdings: Option<Holdings>,
    /// The player stands at the stash (its panel is open): claims go in.
    pub at_stash: bool,
    pub focus: Option<TextBox>,
    /// The names the search box completes, from game data: base items,
    /// unique and set items, sorted.
    pub names: Vec<String>,
    /// The currencies' names by stackable code, from game data.
    pub coin_names: BTreeMap<String, String>,
    /// The status line under the results.
    pub note: Option<String>,
    /// Lines for the game's message area (market closed ...), taken by the
    /// panel.
    pub chat: Vec<String>,
    /// The market key in force.
    pub key: MarketKey,
    /// The price check box.
    pub prices: price_graph::PriceCheck,
    /// Asks since the app last sent them, in order.
    pub asks: Vec<MarketAsk>,
    /// The New listing form beside the stash's Market category.
    pub sell: sell::SellForm,
    /// The My shop tab.
    pub shop: shop::ShopUi,
}

/// The most names the search box offers at once.
pub const SUGGESTIONS: usize = 6;

impl MarketUi {
    pub fn new() -> Self {
        MarketUi {
            desc: true,
            grid_size: 100,
            table_size: 50,
            ..MarketUi::default()
        }
    }

    pub fn availability(&self) -> Availability {
        match (&self.state, self.online) {
            (_, false) => Availability::SinglePlayer,
            (Some(s), true) if s.on => Availability::Open,
            _ => Availability::Closed,
        }
    }

    /// The game went online (or back to single player): online, the
    /// market's state is asked for.
    pub fn set_online(&mut self, online: bool) {
        if online && !self.online {
            self.asks.push(MarketAsk::Hello);
        }
        if !online {
            self.state = None;
            self.close();
        }
        self.online = online;
    }

    /// The page size of the shown view.
    pub fn page_size(&self) -> u8 {
        let n = match self.view {
            ResultsView::Grid => self.grid_size,
            ResultsView::Table => self.table_size,
        };
        if PAGE_SIZES.contains(&n) {
            n
        } else {
            50
        }
    }

    /// The market key or the Market button. Opens or closes the window
    /// when the market is open; returns why not otherwise (the caller
    /// shows it).
    pub fn toggle(&mut self) -> Result<(), &'static str> {
        if let Some(why) = self.availability().why_not() {
            self.close();
            return Err(why);
        }
        if self.open {
            self.close();
        } else {
            self.open = true;
            self.tab = MarketTab::Browse;
            // The sell form's price check box stays with the form.
            self.prices.open = false;
            if self.results.is_none() {
                self.filters = self.default_filters.clone();
                self.search(true);
            }
        }
        Ok(())
    }

    pub fn close(&mut self) {
        self.open = false;
        self.focus = None;
        self.confirm = None;
        self.form = None;
        self.offers.deal = None;
        self.shop.boxed = None;
        self.close_prices();
    }

    /// The seller's listings changed (a post): My shop, shown, reads its
    /// page again; hidden, it does when next shown.
    pub fn shop_changed(&mut self) {
        if self.shown() && self.tab == MarketTab::MyShop {
            shop::refresh(self);
        }
    }

    /// The window is open and shows something (a single-player game never
    /// does).
    pub fn shown(&self) -> bool {
        self.open && self.availability() == Availability::Open
    }

    /// A box is open over the window (the buy confirm, the offer form, an
    /// answer's confirm or the price check): it alone takes clicks.
    pub fn boxed(&self) -> bool {
        self.confirm.is_some()
            || self.form.is_some()
            || self.offers.deal.is_some()
            || self.prices.open
    }

    /// Typing goes to a box of the window: the game's key bindings wait.
    pub fn text_focus(&self) -> bool {
        (self.shown() && self.focus.is_some()) || self.sell.focus.is_some()
    }

    /// Asks for the results of the filters, sort and page in force; with
    /// `facets` the counts too (a filter changed).
    pub fn search(&mut self, facets: bool) {
        let paging = Paging {
            page: self.page,
            page_size: self.page_size(),
            group_first: self.view == ResultsView::Grid,
            facets,
        };
        let q = self.filters.query(self.sort, self.desc, paging);
        // A search not yet sent is replaced: only the newest counts.
        self.asks.retain(|a| !matches!(a, MarketAsk::Search(_)));
        self.asks.push(MarketAsk::Search(q));
        self.search_queued = true;
        self.search_req = None;
    }

    /// A filter changed: back to the first page, with new counts.
    pub fn refilter(&mut self) {
        self.page = 0;
        self.search(true);
    }

    /// The number of pages of the shown results.
    pub fn pages(&self) -> u16 {
        let Some(r) = &self.results else { return 1 };
        let per = u32::from(r.page_size.max(1));
        r.total.div_ceil(per).clamp(1, u32::from(u16::MAX)) as u16
    }

    /// Sorts by `s`; a second click on the same sort turns it round.
    pub fn sort_by(&mut self, s: MarketSort) {
        if self.sort == s {
            self.desc = !self.desc;
        } else {
            self.sort = s;
            // Newest first; names, sellers, levels and prices lowest first.
            self.desc = s == MarketSort::Posted;
        }
        self.page = 0;
        self.search(false);
    }

    pub fn set_view(&mut self, v: ResultsView) {
        if self.view != v {
            self.view = v;
            self.page = 0;
            self.search(false);
        }
    }

    pub fn set_page_size(&mut self, n: u8) {
        if !PAGE_SIZES.contains(&n) {
            return;
        }
        match self.view {
            ResultsView::Grid => self.grid_size = n,
            ResultsView::Table => self.table_size = n,
        }
        self.page = 0;
        self.search(false);
    }

    pub fn go_page(&mut self, page: u16) {
        let page = page.min(self.pages() - 1);
        if page != self.page {
            self.page = page;
            self.search(false);
        }
    }

    /// The shown listing at `index` of the page.
    pub fn listing(&self, index: usize) -> Option<&ListingRow> {
        self.results.as_ref()?.rows.get(index)
    }

    /// Shows listing `index` of the page on the right and looks it up
    /// afresh (its state now and the fees a buy pays).
    pub fn select(&mut self, index: usize) {
        let Some(l) = self.listing(index).cloned() else {
            return;
        };
        self.asks.push(MarketAsk::ListingGet(l.listing));
        self.get_req = None;
        self.selected = Some(l);
        self.detail = None;
        self.confirm = None;
        self.form = None;
    }

    /// The fees a buy of the shown listing pays: the lookup's, else the
    /// state's.
    fn buy_fees(&self) -> (u64, u32) {
        if let Some(d) = &self.detail {
            return (d.buyer_fee, d.fee_rev);
        }
        let f = self.state.as_ref().map(|s| s.fees).unwrap_or_default();
        (f.buyer_fee, f.fee_rev)
    }

    /// Whether the shown listing can be bought (has a price, not the
    /// asker's own).
    pub fn can_buy(&self) -> bool {
        self.selected
            .as_ref()
            .is_some_and(|l| l.price.is_some() && !l.mine)
    }

    /// Whether the shown listing takes offers from the asker.
    pub fn can_offer(&self) -> bool {
        self.selected
            .as_ref()
            .is_some_and(|l| l.mode.takes_offers() && !l.mine)
    }

    /// Buy on the shown listing: opens the confirm with its exact price
    /// and the realm's buyer fee.
    pub fn ask_buy(&mut self) {
        if !self.can_buy() {
            return;
        }
        let (buyer_fee, fee_rev) = self.buy_fees();
        let Some(l) = &self.selected else { return };
        let Some(price) = l.price.clone() else { return };
        self.confirm = Some(BuyConfirm {
            listing: l.listing,
            name: l.item.name.clone(),
            price,
            price_hash: l.price_hash,
            buyer_fee,
            fee_rev,
        });
        self.form = None;
    }

    /// The confirm's Buy: the ask goes to the market.
    pub fn confirm_buy(&mut self) {
        if let Some(b) = self.confirm.take() {
            self.note = Some(format!("Buying {}...", b.name));
            self.asks.push(MarketAsk::Buy {
                listing: b.listing,
                price_hash: b.price_hash,
                fee_rev: b.fee_rev,
            });
            self.buy_req = None;
        }
    }

    /// Shows tab `t`; My shop, Offers and Deliveries ask for their lists
    /// afresh.
    pub fn set_tab(&mut self, t: MarketTab) {
        if self.tab != t {
            self.note = None;
        }
        self.tab = t;
        self.focus = None;
        self.form = None;
        self.confirm = None;
        self.offers.deal = None;
        match t {
            MarketTab::Offers => self.open_offers(),
            MarketTab::Deliveries => self.open_deliveries(),
            MarketTab::MyShop => {
                let st = self.shop.tab;
                shop::set_tab(self, st);
            }
            _ => {}
        }
    }

    /// The realm's fees as last stated (none before the state).
    pub fn fees(&self) -> MarketFees {
        self.state.as_ref().map(|s| s.fees).unwrap_or_default()
    }

    /// Saves the filters as the ones Reset gives.
    pub fn save_default(&mut self) {
        self.default_filters = self.filters.clone();
        self.note = Some("These filters are the default now.".into());
    }

    pub fn reset(&mut self) {
        self.filters = self.default_filters.clone();
        self.type_find.clear();
        self.refilter();
    }

    /// The names the search box offers for its text: names starting with
    /// it first, then names holding it, at most [`SUGGESTIONS`]. None for
    /// fewer than two letters or a name typed out in full.
    pub fn suggestions(&self) -> Vec<&str> {
        let t = self.filters.text.trim().to_lowercase();
        if self.focus != Some(TextBox::Search) || t.chars().count() < 2 {
            return Vec::new();
        }
        let lower: Vec<(String, &str)> = self
            .names
            .iter()
            .map(|n| (n.to_lowercase(), n.as_str()))
            .collect();
        if lower.iter().any(|(l, _)| *l == t) {
            return Vec::new();
        }
        let starts = lower.iter().filter(|(l, _)| l.starts_with(&t));
        let holds = lower
            .iter()
            .filter(|(l, _)| !l.starts_with(&t) && l.contains(&t));
        starts
            .chain(holds)
            .map(|(_, n)| *n)
            .take(SUGGESTIONS)
            .collect()
    }

    /// The name search box takes suggestion `i`.
    pub fn pick_suggestion(&mut self, i: usize) {
        if let Some(n) = self.suggestions().get(i).map(|s| (*s).to_owned()) {
            self.filters.text = n;
            self.focus = None;
            self.refilter();
        }
    }

    /// Sets the names the search box completes (sorted, each once).
    pub fn set_names(&mut self, mut names: Vec<String>) {
        names.retain(|n| !n.trim().is_empty());
        names.sort();
        names.dedup();
        self.names = names;
    }

    /// Ask `ask` went out as request `req`.
    pub fn sent(&mut self, ask: &MarketAsk, req: Req) {
        match ask {
            MarketAsk::Search(_) => {
                self.search_queued = false;
                self.search_req = Some(req);
            }
            MarketAsk::ListingGet(_) => self.get_req = Some(req),
            MarketAsk::Buy { .. } => self.buy_req = Some(req),
            MarketAsk::Post(_) => {
                self.sell.posting = false;
                self.sell.post_req = Some(req);
            }
            MarketAsk::ShopEdit { .. } | MarketAsk::ShopOpen { .. } => shop::sent(self, ask, req),
            MarketAsk::Prices { .. } => self.prices_sent(req),
            MarketAsk::Hello => {}
            MarketAsk::Offer { .. } | MarketAsk::Answer { .. } | MarketAsk::OffersOpen { .. } => {
                self.sent_offer(ask, req);
            }
            MarketAsk::DeliveriesOpen { .. } | MarketAsk::Claim { .. } => {
                self.sent_delivery(ask, req);
            }
        }
    }

    /// Sends the queued asks to `src` and shows what it answered: the
    /// app's step each frame.
    pub fn pump(&mut self, src: &mut dyn MarketSource) {
        // The Sell slot's item gets its "Similar active" line.
        if self.availability() == Availability::Open {
            self.check_sell_line();
        }
        for g in std::mem::take(&mut self.sell.seen) {
            src.stash_item_seen(&g);
        }
        for ask in self.take_asks() {
            let req = src.send(ask.clone());
            self.sent(&ask, req);
        }
        for e in src.poll() {
            self.apply(e);
        }
        // The offer form offers what the stash holds now.
        if self.shown() {
            self.holdings = src.holdings();
        }
    }

    /// Shows the market's answer.
    pub fn apply(&mut self, e: MarketEvent) {
        match e {
            MarketEvent::State { state, .. } => {
                let off = !state.on;
                self.state = Some(state);
                if off && self.open {
                    self.close();
                    self.chat.push("The market is closed on this realm.".into());
                }
            }
            MarketEvent::Results { req, page } => {
                // An answer to an older search is late: a newer one is out.
                if self.search_queued || self.search_req != Some(req) {
                    return;
                }
                self.search_req = None;
                if let Some(f) = &page.facets {
                    self.facets = f.clone();
                }
                // The shown listing follows the new page.
                if let Some(sel) = &self.selected {
                    if let Some(l) = page.rows.iter().find(|l| l.listing == sel.listing) {
                        self.selected = Some(l.clone());
                    }
                }
                self.page = page.page;
                self.results = Some(page);
                self.scroll = 0;
            }
            MarketEvent::Listing { req, detail } => {
                if self.get_req != Some(req) {
                    return;
                }
                self.get_req = None;
                if self
                    .selected
                    .as_ref()
                    .is_some_and(|l| l.listing == detail.row.listing)
                {
                    self.selected = Some(detail.row.clone());
                    self.detail = Some(*detail);
                }
            }
            MarketEvent::Shop { req, page } => shop::on_page(self, req, page),
            MarketEvent::Prices { req, history } => self.prices_answer(req, *history),
            MarketEvent::Done(d) => {
                if self.prices_done(&d) || sell::on_done(self, &d) || shop::on_done(self, &d) {
                    return;
                }
                let ours = |r: Option<Req>| r == Some(d.req);
                if ours(self.buy_req) {
                    self.buy_req = None;
                    if d.code == MarketCode::Done {
                        let name = self
                            .selected
                            .as_ref()
                            .filter(|l| Some(l.listing) == d.id)
                            .map_or_else(|| "the item".to_owned(), |l| l.item.name.clone());
                        self.note = Some(format!("Bought {name}. It waits in Deliveries."));
                        self.selected = None;
                        self.detail = None;
                    } else {
                        self.refused(&d);
                    }
                    self.search(true);
                } else if ours(self.get_req) {
                    self.get_req = None;
                    self.refused(&d);
                } else if ours(self.search_req) {
                    self.search_req = None;
                    self.note = Some(d.line());
                } else if !self.done_offer(&d) {
                    self.done_claim(&d);
                }
            }
            MarketEvent::Offers { req, page } => self.apply_offers(req, page),
            MarketEvent::Deliveries { req, page } => self.apply_deliveries(req, page),
            MarketEvent::Notice(n) => self.notice(n),
        }
    }

    /// A buy or lookup refused: the reason shows; the listing is looked up
    /// again (a new price or fee) or leaves the panel (gone).
    fn refused(&mut self, d: &MarketDone) {
        self.note = Some(d.line());
        match d.code {
            MarketCode::ListingGone => {
                self.selected = None;
                self.detail = None;
            }
            MarketCode::PriceChanged | MarketCode::FeeChanged => {
                if let Some(l) = &self.selected {
                    self.asks.push(MarketAsk::ListingGet(l.listing));
                    self.detail = None;
                }
                if d.code == MarketCode::FeeChanged {
                    self.asks.push(MarketAsk::Hello);
                }
            }
            _ => {}
        }
    }

    pub fn take_asks(&mut self) -> Vec<MarketAsk> {
        std::mem::take(&mut self.asks)
    }
}

#[cfg(test)]
mod shop_tests;
#[cfg(test)]
mod tests;
