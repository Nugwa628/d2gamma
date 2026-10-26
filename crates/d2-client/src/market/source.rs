// D2RUST's own: what the market window reads and asks, behind one trait.
//! The market's data seam (`docs/plans/server/market-contract.md`, v1).
//! The window never decides a sale: it sends [`MarketAsk`]s to a
//! [`MarketSource`] and shows the [`MarketEvent`]s it gets back. The source
//! is the realm's market over the online link (`RealmSource`, over the
//! wire messages 0xB0-0xCF), or the in-process [`super::fake::FakeSource`]
//! tests and offscreen shots run against.
//!
//! The source answers later, never in the call: `send` gives the request
//! its number (`req`) and every answer repeats it, so an answer to an older
//! search is dropped.
//!
//! The types carry the contract's names and fields (`MarketQuery`,
//! `ListingRow`, `MarketState`, `MarketDone` ...), already decoded; the
//! item of a listing comes as its save-format record, which the window
//! turns into the game's own tip (`ui::market::tip`).

use std::collections::BTreeMap;

use super::trade::{
    AnswerHow, DeliveriesPage, DeliveryId, Holdings, Notice, OfferId, OffersPage, OffersWhich,
};

/// A request's number: rising per account, repeated by every answer.
pub type Req = u64;

/// A listing's number on the realm.
pub type ListingId = u64;

/// The quality group the browser lists and groups by (contract
/// `Group`), declared in the grid's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Unique,
    Set,
    Runeword,
    Rare,
    Crafted,
    Magic,
    /// Low, normal and superior items without a runeword.
    Other,
    /// Runes, gems and the realm's other stackables.
    Stackable,
}

impl Group {
    /// Grid order (contract: 5, 4, 6, 2, 3, 1, 0, 7).
    pub const ALL: [Group; 8] = [
        Group::Unique,
        Group::Set,
        Group::Runeword,
        Group::Rare,
        Group::Crafted,
        Group::Magic,
        Group::Other,
        Group::Stackable,
    ];

    /// The wire value.
    pub fn code(self) -> u8 {
        match self {
            Group::Other => 0,
            Group::Magic => 1,
            Group::Rare => 2,
            Group::Crafted => 3,
            Group::Set => 4,
            Group::Unique => 5,
            Group::Runeword => 6,
            Group::Stackable => 7,
        }
    }

    pub fn of_code(c: u8) -> Option<Group> {
        Group::ALL.into_iter().find(|g| g.code() == c)
    }

    /// The name shown, and the word of its UI id
    /// (`market.filter.quality(Unique)`).
    pub fn label(self) -> &'static str {
        match self {
            Group::Unique => "Unique",
            Group::Set => "Set",
            Group::Runeword => "Runeword",
            Group::Rare => "Rare",
            Group::Crafted => "Crafted",
            Group::Magic => "Magic",
            Group::Other => "Normal",
            Group::Stackable => "Stackable",
        }
    }

    /// The UI id word.
    pub fn key(self) -> &'static str {
        match self {
            Group::Other => "Other",
            g => g.label(),
        }
    }

    /// The text colour (string tables' colour codes) the game names an
    /// item of this group in: gold, green, gold, yellow, orange, blue,
    /// white, orange for the stackables (runes).
    pub fn color(self) -> u16 {
        match self {
            Group::Unique => 4,
            Group::Set => 2,
            Group::Runeword => 4,
            Group::Rare => 9,
            Group::Crafted => 8,
            Group::Magic => 3,
            Group::Other => 0,
            Group::Stackable => 8,
        }
    }
}

/// A price (contract `Price`): gold, counts of the realm's currencies by
/// stackable code, and (offers and counters only) stash items by ID.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Price {
    pub gold: u64,
    /// `(stackable code, count)`, each code once, sorted.
    pub stacks: Vec<(String, u32)>,
    /// Stash item IDs; a listing's price never names items.
    pub items: Vec<u32>,
}

impl Price {
    pub fn gold(gold: u64) -> Self {
        Price {
            gold,
            ..Price::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.gold == 0 && self.stacks.iter().all(|(_, n)| *n == 0) && self.items.is_empty()
    }

    /// Only gold (price mode 4).
    pub fn gold_only(&self) -> bool {
        self.gold > 0 && self.stacks.is_empty() && self.items.is_empty()
    }

    /// Only currencies, no gold (price mode 3, "runes only").
    pub fn stacks_only(&self) -> bool {
        self.gold == 0 && !self.stacks.is_empty()
    }
}

/// What a listing sells (contract `Goods`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Goods {
    /// An item: its stash ID and its save-format record (with its socketed
    /// children after it).
    Item { item_id: u32, record: Vec<u8> },
    /// A count of a stackable.
    Stack { code: String, count: u32 },
    /// Gold (deliveries only).
    Gold(u64),
}

/// What a listing takes (contract `mode`, the economy's `ListingMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListingMode {
    /// Sold at its price.
    Price,
    /// No price: buyers make offers.
    OffersOnly,
    /// Sold at its price, or to an accepted offer.
    PriceAndOffers,
}

impl ListingMode {
    pub fn code(self) -> u8 {
        match self {
            ListingMode::Price => 0,
            ListingMode::OffersOnly => 1,
            ListingMode::PriceAndOffers => 2,
        }
    }

    pub fn has_price(self) -> bool {
        self != ListingMode::OffersOnly
    }

    pub fn takes_offers(self) -> bool {
        self != ListingMode::Price
    }
}

/// What the client read from a listing's goods for the browser: names,
/// type, requirement, flags and key stats. The realm source fills it from
/// the record with the game data; the fake makes it up.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemView {
    /// The name shown ("Ashen Crown"; a rare's two words; a stackable's
    /// name).
    pub name: String,
    /// The base item's name.
    pub base: String,
    /// The base item's code.
    pub code: String,
    /// The item type code (`helm`, `ring`, `rune` ...).
    pub item_type: String,
    pub level_req: u8,
    pub sockets: u8,
    pub ethereal: bool,
    pub identified: bool,
    /// The tip's stat lines, most telling first (the table's key stats;
    /// the tip when the record gives none).
    pub stats: Vec<String>,
}

/// One listing (contract `ListingRow`) with the client's reading of its
/// goods.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListingRow {
    pub listing: ListingId,
    pub group: Group,
    pub goods: Goods,
    pub mode: ListingMode,
    /// `None` for an offers-only listing.
    pub price: Option<Price>,
    /// The realm's hash of the price; a buy sends it back.
    pub price_hash: [u8; 32],
    /// The seller's character name when posted.
    pub seller: String,
    /// The seller account's sold listings in this economy.
    pub seller_sales: u32,
    pub posted_ms: u64,
    pub expires_ms: u64,
    /// The seller's public note.
    pub note: String,
    /// Live offers on it.
    pub offers: u16,
    /// The asker's own listing: no Buy, no Offer.
    pub mine: bool,
    pub item: ItemView,
}

impl ListingRow {
    /// The item's save-format record, for an item listing.
    pub fn record(&self) -> Option<&[u8]> {
        match &self.goods {
            Goods::Item { record, .. } => Some(record),
            _ => None,
        }
    }
}

/// A three-way flag filter (contract 0 any, 1 yes, 2 no).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tri {
    #[default]
    Any,
    Yes,
    No,
}

impl Tri {
    pub fn code(self) -> u8 {
        match self {
            Tri::Any => 0,
            Tri::Yes => 1,
            Tri::No => 2,
        }
    }

    /// The next state a click gives: any, yes, no, any.
    pub fn next(self) -> Tri {
        match self {
            Tri::Any => Tri::Yes,
            Tri::Yes => Tri::No,
            Tri::No => Tri::Any,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tri::Any => "any",
            Tri::Yes => "yes",
            Tri::No => "no",
        }
    }

    pub fn allows(self, v: bool) -> bool {
        match self {
            Tri::Any => true,
            Tri::Yes => v,
            Tri::No => !v,
        }
    }
}

/// The price chips (contract `price_mode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PriceMode {
    #[default]
    Any,
    /// Has a price.
    Fixed,
    /// Takes offers.
    Offers,
    /// Currencies only, no gold.
    RunesOnly,
    GoldOnly,
}

impl PriceMode {
    pub const ALL: [PriceMode; 5] = [
        PriceMode::Any,
        PriceMode::Fixed,
        PriceMode::Offers,
        PriceMode::RunesOnly,
        PriceMode::GoldOnly,
    ];

    pub fn code(self) -> u8 {
        PriceMode::ALL.iter().position(|m| *m == self).unwrap_or(0) as u8
    }

    pub fn label(self) -> &'static str {
        match self {
            PriceMode::Any => "Any",
            PriceMode::Fixed => "Fixed price",
            PriceMode::Offers => "Offers",
            PriceMode::RunesOnly => "Runes only",
            PriceMode::GoldOnly => "Gold only",
        }
    }

    /// The word of its UI id.
    pub fn key(self) -> &'static str {
        match self {
            PriceMode::Any => "any",
            PriceMode::Fixed => "fixed",
            PriceMode::Offers => "offers",
            PriceMode::RunesOnly => "runes",
            PriceMode::GoldOnly => "gold",
        }
    }

    pub fn allows(self, mode: ListingMode, price: Option<&Price>) -> bool {
        match self {
            PriceMode::Any => true,
            PriceMode::Fixed => mode.has_price(),
            PriceMode::Offers => mode.takes_offers(),
            PriceMode::RunesOnly => price.is_some_and(Price::stacks_only),
            PriceMode::GoldOnly => price.is_some_and(Price::gold_only),
        }
    }
}

/// What the results are sorted by (contract `sort`). Ties go by the
/// listing ID, so pages never overlap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarketSort {
    /// Date posted.
    #[default]
    Posted,
    /// The price in [`MarketQuery::sort_currency`] ("" gold); only listings
    /// priced in it.
    Price,
    /// A stat's value ([`MarketQuery::sort_stat`]); only items with it.
    Stat,
    /// The level requirement.
    Level,
    Name,
    Seller,
}

impl MarketSort {
    pub fn code(self) -> u8 {
        match self {
            MarketSort::Posted => 0,
            MarketSort::Price => 1,
            MarketSort::Stat => 2,
            MarketSort::Level => 3,
            MarketSort::Name => 4,
            MarketSort::Seller => 5,
        }
    }
}

/// The page sizes the realm takes.
pub const PAGE_SIZES: [u8; 3] = [25, 50, 100];

/// One search (contract `MarketQuery`, 3.1): every set field must match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketQuery {
    /// Word prefixes of any name (0-60 bytes).
    pub text: String,
    /// Any of these groups; empty = all.
    pub groups: Vec<Group>,
    /// Item type codes of the type tree; a code matches its descendants;
    /// empty = all.
    pub types: Vec<String>,
    pub ethereal: Tri,
    pub socketed: Tri,
    pub identified: Tri,
    /// `(stat, min, max)` ranges, both ends included.
    pub stats: Vec<(u16, i32, i32)>,
    /// Level requirement range; 0-255 = any.
    pub level: (u8, u8),
    /// Socket count range; 0-255 = any.
    pub sockets: (u8, u8),
    /// Items restricted to that class.
    pub class: Option<u8>,
    pub price_mode: PriceMode,
    /// `(currency, max)` caps, OR'd; "" is gold.
    pub caps: Vec<(String, u64)>,
    pub sort: MarketSort,
    pub sort_stat: u16,
    pub sort_currency: String,
    pub desc: bool,
    /// Order by group (grid order) first: the grid view.
    pub group_first: bool,
    /// From 0.
    pub page: u16,
    /// 25, 50 or 100.
    pub page_size: u8,
    /// Also send the facet counts (a filter changed, not a page flip).
    pub facets: bool,
}

impl Default for MarketQuery {
    fn default() -> Self {
        MarketQuery {
            text: String::new(),
            groups: Vec::new(),
            types: Vec::new(),
            ethereal: Tri::Any,
            socketed: Tri::Any,
            identified: Tri::Any,
            stats: Vec::new(),
            level: (0, 255),
            sockets: (0, 255),
            class: None,
            price_mode: PriceMode::Any,
            caps: Vec::new(),
            sort: MarketSort::Posted,
            sort_stat: 0,
            sort_currency: String::new(),
            desc: true,
            group_first: false,
            page: 0,
            page_size: 50,
            facets: true,
        }
    }
}

/// The facet counts (contract `MarketFacets`, 3.2): each the listings the
/// query matches with that facet's own filter left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facets {
    pub groups: BTreeMap<Group, u32>,
    /// Every type code with a listing, counted at each ancestor too.
    pub types: BTreeMap<String, u32>,
    /// `[yes, no]`.
    pub ethereal: [u32; 2],
    pub socketed: [u32; 2],
    pub identified: [u32; 2],
    /// By [`PriceMode::code`].
    pub price_modes: [u32; 5],
}

impl Facets {
    /// The count beside flag chip state `t` (any: both).
    pub fn flag(pair: [u32; 2], t: Tri) -> u32 {
        match t {
            Tri::Any => pair[0] + pair[1],
            Tri::Yes => pair[0],
            Tri::No => pair[1],
        }
    }
}

/// A page of results (contract `MarketResultsBegin`, its rows and
/// facets).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResultsPage {
    /// Listings matching, on every page.
    pub total: u32,
    pub page: u16,
    pub page_size: u8,
    /// The realm's clock, for "2 h ago".
    pub now_ms: u64,
    pub rows: Vec<ListingRow>,
    /// Sent when asked ([`MarketQuery::facets`]).
    pub facets: Option<Facets>,
}

/// The realm's market fees (contract 7): zero by default. Every confirm
/// computes the fee from them, and every paying request carries
/// `fee_rev`, so the fees taken are the ones the player saw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarketFees {
    /// Share of a sale's gold the realm keeps.
    pub sale_fee_pct: u8,
    /// Gold per listing and relist.
    pub listing_fee: u64,
    /// Gold a buyer pays on top of the price.
    pub buyer_fee: u64,
    pub fee_rev: u32,
}

/// Why the market is off (contract `off_reason`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OffReason {
    #[default]
    On,
    /// The realm's `market` switch is off.
    Switch,
    NoCharSelected,
}

/// The market as the realm states it (contract `MarketState`, 0xB1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarketState {
    pub on: bool,
    pub off_reason: OffReason,
    pub mod_id: String,
    /// The selected character's economy.
    pub hardcore: bool,
    /// The ladder season, 0 = non-ladder.
    pub ladder: u16,
    pub expansion: bool,
    pub max_listings: u16,
    pub max_offers: u16,
    /// The most stash items one offer may name (contract 3.4).
    pub max_offer_items: u8,
    /// The longest a listing runs, in days.
    pub max_days: u8,
    pub fees: MarketFees,
    pub active_listings: u16,
    pub deliveries_waiting: u16,
    pub live_offers_to_me: u16,
    pub my_live_offers: u16,
    pub last_req: Req,
    /// The realm's stackable codes: the currencies prices use.
    pub currencies: Vec<String>,
}

impl MarketState {
    /// "Softcore · Non-ladder · Expansion".
    pub fn split_line(&self) -> String {
        format!(
            "{} · {} · {}",
            if self.hardcore {
                "Hardcore"
            } else {
                "Softcore"
            },
            if self.ladder > 0 {
                "Ladder"
            } else {
                "Non-ladder"
            },
            if self.expansion {
                "Expansion"
            } else {
                "Classic"
            },
        )
    }
}

/// One listing looked up (contract `MarketListing`, 0xB7): its state, the
/// row as it is now, the fees a buy pays and the asker's offer on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListingDetail {
    /// 0 active, 1 sold, 2 cancelled, 3 expired.
    pub state: u8,
    pub row: ListingRow,
    /// Gold added to the price; 0 today.
    pub buyer_fee: u64,
    pub fee_rev: u32,
    /// The asker's live offer on the listing (the short row; the offered
    /// items' records come as item messages before it).
    pub offer: Option<OfferShort>,
}

/// An offer's short row in a listing's details (contract change 1 of
/// K1a): the offered price names stash items whose records came first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfferShort {
    pub offer: u64,
    pub price: Price,
    /// The seller's counter, when there is one.
    pub counter: Option<Price>,
    pub state: u8,
    pub made_ms: u64,
    /// 0: not answered yet.
    pub answered_ms: u64,
}

/// A result code of `MarketDone` (contract 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarketCode {
    Done,
    RateLimited,
    NotLoggedIn,
    NoCharSelected,
    StashFull,
    ItemNotFound,
    ListingGone,
    PriceChanged,
    NotEnough,
    MarketLimit,
    ItemHeld,
    OfferGone,
    EconomyMismatch,
    MarketOff,
    FeeChanged,
    NotYours,
    /// Any other code, with the realm's text.
    Other(u16),
}

impl MarketCode {
    pub fn of(code: u16) -> MarketCode {
        match code {
            0 => MarketCode::Done,
            24 => MarketCode::RateLimited,
            27 => MarketCode::NotLoggedIn,
            47 => MarketCode::NoCharSelected,
            80 => MarketCode::StashFull,
            82 => MarketCode::ItemNotFound,
            83 => MarketCode::ListingGone,
            84 => MarketCode::PriceChanged,
            85 => MarketCode::NotEnough,
            86 => MarketCode::MarketLimit,
            87 => MarketCode::ItemHeld,
            88 => MarketCode::OfferGone,
            89 => MarketCode::EconomyMismatch,
            90 => MarketCode::MarketOff,
            91 => MarketCode::FeeChanged,
            92 => MarketCode::NotYours,
            c => MarketCode::Other(c),
        }
    }

    pub fn code(&self) -> u16 {
        match self {
            MarketCode::Done => 0,
            MarketCode::RateLimited => 24,
            MarketCode::NotLoggedIn => 27,
            MarketCode::NoCharSelected => 47,
            MarketCode::StashFull => 80,
            MarketCode::ItemNotFound => 82,
            MarketCode::ListingGone => 83,
            MarketCode::PriceChanged => 84,
            MarketCode::NotEnough => 85,
            MarketCode::MarketLimit => 86,
            MarketCode::ItemHeld => 87,
            MarketCode::OfferGone => 88,
            MarketCode::EconomyMismatch => 89,
            MarketCode::MarketOff => 90,
            MarketCode::FeeChanged => 91,
            MarketCode::NotYours => 92,
            MarketCode::Other(c) => *c,
        }
    }

    /// The client's short text; `None` for "done" and the codes the
    /// realm's text explains.
    pub fn text(&self, wait_s: u32) -> Option<String> {
        Some(match self {
            MarketCode::Done | MarketCode::Other(_) => return None,
            MarketCode::RateLimited => format!("Too many market requests; wait {wait_s} s."),
            MarketCode::NotLoggedIn => "You are not logged in.".into(),
            MarketCode::NoCharSelected => "Select a character first.".into(),
            MarketCode::StashFull => "Your shared stash is full.".into(),
            MarketCode::ItemNotFound => "That isn't in your stash any more.".into(),
            MarketCode::ListingGone => "That listing was sold, cancelled or has expired.".into(),
            MarketCode::PriceChanged => "The price changed. Look again before buying.".into(),
            MarketCode::NotEnough => "Your shared stash doesn't hold the price.".into(),
            MarketCode::MarketLimit => "You have the most listings (or offers) allowed.".into(),
            MarketCode::ItemHeld => "That item is held.".into(),
            MarketCode::OfferGone => "That offer was answered, withdrawn or has expired.".into(),
            MarketCode::EconomyMismatch => {
                "That belongs to another realm or character type.".into()
            }
            MarketCode::MarketOff => "The market is closed on this realm.".into(),
            MarketCode::FeeChanged => "The market's fees changed. Look again.".into(),
            MarketCode::NotYours => "That isn't yours.".into(),
        })
    }
}

/// A request's end (contract `MarketDone`, 0xCC).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketDone {
    pub req: Req,
    pub code: MarketCode,
    pub wait_s: u32,
    /// A repeat of a stored request: nothing changed.
    pub repeated: bool,
    /// The listing, offer or new listing.
    pub id: Option<u64>,
    /// Per-row results of a shop edit or a claim: `(id, code)` (contract
    /// 3.3); empty otherwise.
    pub rows: Vec<(u64, MarketCode)>,
    /// The realm's English text, for an unknown code.
    pub text: String,
}

impl MarketDone {
    /// The line the window shows for a refusal.
    pub fn line(&self) -> String {
        self.code
            .text(self.wait_s)
            .unwrap_or_else(|| self.text.clone())
    }
}

/// What the window asks of the market (contract C→S).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarketAsk {
    /// 0xB0: the market's state for the selected character.
    Hello,
    /// 0xB2: answered with [`MarketEvent::Results`].
    Search(MarketQuery),
    /// 0xB6: answered with [`MarketEvent::Listing`] or `ListingGone`.
    ListingGet(ListingId),
    /// 0xB8: the hash of the price the buyer saw and the fees they saw.
    Buy {
        listing: ListingId,
        price_hash: [u8; 32],
        fee_rev: u32,
    },
    /// 0xB9: an offer of gold, currencies and the buyer's stash items;
    /// answered `MarketDone` with the offer's ID.
    Offer {
        listing: ListingId,
        price: Price,
        fee_rev: u32,
    },
    /// 0xBA: accept, decline, counter (with its price) or withdraw.
    Answer {
        offer: OfferId,
        how: AnswerHow,
        counter: Option<Price>,
        fee_rev: u32,
    },
    /// 0xC0: answered with [`MarketEvent::Offers`].
    OffersOpen {
        which: OffersWhich,
        live_only: bool,
        page: u16,
    },
    /// 0xC3: answered with [`MarketEvent::Deliveries`].
    DeliveriesOpen { page: u16 },
    /// 0xC6: deliveries into the shared stash (all, or these); answered
    /// `MarketDone` with one result per delivery.
    Claim {
        all: bool,
        deliveries: Vec<DeliveryId>,
    },
    /// 0xBB: goods of the shared stash go up for sale (into escrow);
    /// answered `MarketDone` with the new listing.
    Post(Post),
    /// 0xBC: one edit on up to 50 of the asker's listings; answered
    /// `MarketDone` with the rows that failed.
    ShopEdit {
        op: ShopOp,
        listings: Vec<ListingId>,
        fee_rev: u32,
    },
    /// 0xBD: a page of My shop; answered with [`MarketEvent::Shop`].
    ShopOpen {
        tab: ShopTab,
        page: u16,
        sort: ShopSort,
        desc: bool,
    },
    /// 0xC8: a price check over `days` (1-90); answered with
    /// [`MarketEvent::Prices`].
    Prices {
        of: super::prices::PriceOf,
        days: u16,
    },
}

/// What a post sells, from the shared stash (contract `MarketPost` kind).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PostGoods {
    /// A stash item by its ID.
    Item(u32),
    /// A count of a stackable.
    Stack { code: String, count: u32 },
}

/// A new listing (contract `MarketPost`, 0xBB).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Post {
    pub goods: PostGoods,
    pub mode: ListingMode,
    /// Gold and currencies; `None` for offers only.
    pub price: Option<Price>,
    /// 1 to the realm's `max_days`.
    pub days: u8,
    /// The public note, 0-60 bytes.
    pub note: String,
    /// Straight to the Hidden tab.
    pub hidden: bool,
    /// The fees the seller saw.
    pub fee_rev: u32,
}

/// The longest note the realm takes, in bytes.
pub const NOTE_MAX: usize = 60;

/// A shop edit (contract `MarketShopEdit` op).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShopOp {
    /// The goods go back to Deliveries; live offers are returned.
    Cancel,
    /// An active listing runs `days` from now; a cancelled or expired one
    /// is listed anew.
    Relist {
        days: u8,
    },
    /// Out of the search, still in escrow.
    Hide,
    Show,
    Note(String),
    Reprice {
        mode: ListingMode,
        price: Option<Price>,
    },
}

impl ShopOp {
    /// The wire value.
    pub fn code(&self) -> u8 {
        match self {
            ShopOp::Cancel => 0,
            ShopOp::Relist { .. } => 1,
            ShopOp::Hide => 2,
            ShopOp::Show => 3,
            ShopOp::Note(_) => 4,
            ShopOp::Reprice { .. } => 5,
        }
    }
}

/// My shop's tabs (contract `MarketShopOpen` tab).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShopTab {
    /// Active and in the search.
    #[default]
    Shown,
    /// Active, out of the search.
    Hidden,
    Sold,
    /// Expired and cancelled.
    Expired,
}

impl ShopTab {
    pub const ALL: [ShopTab; 4] = [
        ShopTab::Shown,
        ShopTab::Hidden,
        ShopTab::Sold,
        ShopTab::Expired,
    ];

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn label(self) -> &'static str {
        match self {
            ShopTab::Shown => "Shown in market",
            ShopTab::Hidden => "Hidden",
            ShopTab::Sold => "Sold",
            ShopTab::Expired => "Expired",
        }
    }

    /// The word of its UI id (`market.shop.tab(Sold)`).
    pub fn key(self) -> &'static str {
        match self {
            ShopTab::Shown => "Shown",
            ShopTab::Hidden => "Hidden",
            ShopTab::Sold => "Sold",
            ShopTab::Expired => "Expired",
        }
    }
}

/// My shop's sorts (contract `MarketShopOpen` sort).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShopSort {
    #[default]
    Posted,
    Ends,
    /// The gold of the price.
    Gold,
    Name,
}

impl ShopSort {
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// A listing's state (contract `ShopRow.state`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListingState {
    #[default]
    Active,
    Sold,
    Cancelled,
    Expired,
}

impl ListingState {
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// How a listing sold (contract `Sold`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sold {
    pub buyer: String,
    /// The listing's price, or the accepted offer or counter.
    pub paid: Price,
    /// Gold the realm kept.
    pub sale_fee: u64,
    pub closed_ms: u64,
}

/// One of the asker's listings (contract `ShopRow`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShopRow {
    pub row: ListingRow,
    pub state: ListingState,
    pub hidden: bool,
    pub live_offers: u16,
    pub newest_offer_ms: u64,
    pub sold: Option<Sold>,
    /// Cancelled or expired, and its goods are still the seller's: it can
    /// be listed again.
    pub relistable: bool,
}

/// The rows My shop shows a page.
pub const SHOP_PAGE: usize = 50;

/// A page of My shop (contract `MarketShopBegin`, its rows).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShopPage {
    pub tab: ShopTab,
    pub page: u16,
    /// The four tabs' counts, in [`ShopTab::ALL`] order.
    pub counts: [u32; 4],
    /// Active listings, shown and hidden.
    pub slots_used: u16,
    pub slots_max: u16,
    pub now_ms: u64,
    pub rows: Vec<ShopRow>,
}

/// A stash item the sell slot shows, as the client read it: what the fake
/// market lists when it is posted (the realm reads the account's stash
/// itself).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StashGoods {
    pub item_id: u32,
    pub group: Group,
    /// The item as the stash keeps it (its client look; empty when none).
    pub record: Vec<u8>,
    pub item: ItemView,
}

/// What the market sends (contract S→C), each with its request's number
/// (0 for a push).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarketEvent {
    State {
        req: Req,
        state: MarketState,
    },
    Results {
        req: Req,
        page: ResultsPage,
    },
    Listing {
        req: Req,
        detail: Box<ListingDetail>,
    },
    Offers {
        req: Req,
        page: OffersPage,
    },
    Deliveries {
        req: Req,
        page: DeliveriesPage,
    },
    /// Pushed (0xCD): something happened to a listing or offer.
    Notice(Notice),
    Shop {
        req: Req,
        page: ShopPage,
    },
    /// A price check (0xC9, its series and recent sales, assembled).
    Prices {
        req: Req,
        history: Box<super::prices::PriceHistory>,
    },
    Done(MarketDone),
}

/// The market as the window reaches it. `send` numbers and sends an ask;
/// `poll` gives what arrived since, in order. Neither blocks.
pub trait MarketSource: Send + Sync {
    fn send(&mut self, ask: MarketAsk) -> Req;
    fn poll(&mut self) -> Vec<MarketEvent>;

    /// What the account's online shared stash holds, for the offer form
    /// (not a market message: the stash's own view). `None` until known.
    fn holdings(&self) -> Option<Holdings> {
        None
    }
    /// The sell slot shows stash item `goods`. The realm knows the
    /// account's stash and ignores it; the fake market keeps it, so a post
    /// of it can be listed (its stand-in for the realm's `econ-seed`).
    fn stash_item_seen(&mut self, _goods: &StashGoods) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_codes_follow_the_contract() {
        let codes: Vec<u8> = Group::ALL.iter().map(|g| g.code()).collect();
        assert_eq!(codes, [5, 4, 6, 2, 3, 1, 0, 7]);
        for g in Group::ALL {
            assert_eq!(Group::of_code(g.code()), Some(g));
        }
        let modes: Vec<u8> = PriceMode::ALL.iter().map(|m| m.code()).collect();
        assert_eq!(modes, [0, 1, 2, 3, 4]);
        for c in [0, 24, 47, 80, 83, 84, 90, 91, 92, 99] {
            assert_eq!(MarketCode::of(c).code(), c);
        }
        assert!(MarketCode::Done.text(0).is_none());
        assert_eq!(
            MarketCode::RateLimited.text(3).unwrap(),
            "Too many market requests; wait 3 s."
        );
    }
}
