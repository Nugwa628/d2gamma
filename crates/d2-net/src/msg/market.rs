//! The market's control-stream messages (kinds 0xB0-0xCF,
//! `docs/plans/server/market-contract.md` sections 1-6): search, the
//! listing's details, buying, offers and their answers, posting, the shop,
//! Deliveries and claiming, the price history, the answers and the pushed
//! state and notices.
//!
//! - Every C→S message is a [`ToRealm`]: a request number (`req`, rising
//!   per account) and an [`Ask`]. Every answer, and every row of a list,
//!   repeats the `req` ([`FromRealm::req`]), so several requests can be in
//!   flight.
//! - A body is at most [`crate::wire::MAX_FRAME`] bytes, so a list is sent
//!   as a Begin message, one message per row, then [`FromRealm::ListEnd`].
//!   Item records that rows name by ID ([`FromRealm::Item`]) come before
//!   the row that names them.
//! - The decoders enforce the wire bounds that keep every row inside one
//!   frame (record size, names, currencies, codes); a body that breaks one
//!   is malformed. The market's own rules (note length, page size, days,
//!   what a price may hold) are the realm's to answer with a refusal.

use super::{ErrorCode, Reader, Writer, MAX_ERROR_TEXT};
use crate::NetError;

/// The market's kinds.
pub mod kind {
    pub const HELLO: u8 = 0xB0;
    pub const STATE: u8 = 0xB1;
    pub const SEARCH: u8 = 0xB2;
    pub const RESULTS_BEGIN: u8 = 0xB3;
    pub const FACETS: u8 = 0xB4;
    pub const ROW: u8 = 0xB5;
    pub const LISTING_GET: u8 = 0xB6;
    pub const LISTING: u8 = 0xB7;
    pub const BUY: u8 = 0xB8;
    pub const OFFER: u8 = 0xB9;
    pub const ANSWER: u8 = 0xBA;
    pub const POST: u8 = 0xBB;
    pub const SHOP_EDIT: u8 = 0xBC;
    pub const SHOP_OPEN: u8 = 0xBD;
    pub const SHOP_BEGIN: u8 = 0xBE;
    pub const SHOP_ROW: u8 = 0xBF;
    pub const OFFERS_OPEN: u8 = 0xC0;
    pub const OFFERS_BEGIN: u8 = 0xC1;
    pub const OFFER_ROW: u8 = 0xC2;
    pub const DELIVERIES_OPEN: u8 = 0xC3;
    pub const DELIVERIES_BEGIN: u8 = 0xC4;
    pub const DELIVERY_ROW: u8 = 0xC5;
    pub const CLAIM: u8 = 0xC6;
    pub const ITEM: u8 = 0xC7;
    pub const PRICES: u8 = 0xC8;
    pub const PRICES_BEGIN: u8 = 0xC9;
    pub const PRICE_SERIES: u8 = 0xCA;
    pub const PRICE_RECENT: u8 = 0xCB;
    pub const DONE: u8 = 0xCC;
    pub const NOTICE: u8 = 0xCD;
    pub const LIST_END: u8 = 0xCE;
}

/// An item record (the save format's): at most 2,048 bytes, sent as
/// `bytes16` (a socketed item with its children can pass 255 bytes).
pub const MAX_RECORD: usize = 2048;
/// Currencies (stackable codes) in one price.
pub const MAX_CURRENCIES: usize = 16;
/// A stackable code ("r24"): at most 8 bytes.
pub const MAX_CODE: usize = 8;
/// Stash items one price may name (offers and counters; the realm's
/// `max_offer_items` is at most this).
pub const MAX_PRICE_ITEMS: usize = 16;
/// A character name.
pub const MAX_NAME: usize = 32;
/// A listing's public note, as the realm sends it. A posted note is up to
/// 255 bytes on the wire; the realm refuses one over this.
pub const MAX_NOTE: usize = 60;
/// The search text.
pub const MAX_QUERY_TEXT: usize = 60;
/// An item type code (`weap`, `helm`): four bytes in the game data.
pub const MAX_TYPE_CODE: usize = 4;
/// Type codes in one query.
pub const MAX_QUERY_TYPES: usize = 16;
/// Stat ranges in one query.
pub const MAX_QUERY_STATS: usize = 8;
/// Price caps in one query.
pub const MAX_CAPS: usize = 4;
/// Type counts in one facets message.
pub const MAX_FACET_TYPES: usize = 400;
/// The name of goods ("Grim Crown") in a delivery, notice or price check.
pub const MAX_ABOUT: usize = 64;
/// A mod realm's id.
pub const MAX_MOD_ID: usize = 32;
/// A price key (`u:<name>`, `b:<code>:<group>`, `s:<code>`).
pub const MAX_PRICE_KEY: usize = 80;
/// Sales in a [`FromRealm::PriceRecent`].
pub const MAX_RECENT: usize = 10;

macro_rules! wire_enum {
    ($(#[$m:meta])* $name:ident { $($(#[$vm:meta])* $v:ident = $n:literal,)* }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vm])* $v = $n,)*
        }

        impl $name {
            /// Every value, in wire order.
            pub const ALL: &'static [Self] = &[$(Self::$v),*];

            /// The value of a wire byte; `None` for one this build doesn't
            /// know.
            pub fn from_u8(v: u8) -> Option<Self> {
                match v {
                    $($n => Some(Self::$v),)*
                    _ => None,
                }
            }

            fn read(r: &mut Reader) -> Result<Self, NetError> {
                Self::from_u8(r.u8()?).ok_or(NetError::Malformed)
            }
        }
    };
}

wire_enum! {
    /// The quality group the browser lists and groups by.
    Group {
        /// Low, normal, superior without a runeword.
        Other = 0,
        Magic = 1,
        Rare = 2,
        Crafted = 3,
        Set = 4,
        Unique = 5,
        Runeword = 6,
        /// Runes, gems and the realm's other stackables.
        Stackable = 7,
    }
}

impl Group {
    /// The grid's order (and `group_first`'s).
    pub const GRID_ORDER: [Group; 8] = [
        Group::Unique,
        Group::Set,
        Group::Runeword,
        Group::Rare,
        Group::Crafted,
        Group::Magic,
        Group::Other,
        Group::Stackable,
    ];
}

wire_enum! {
    /// How a listing sells.
    ListingMode {
        /// At its price.
        Price = 0,
        /// Offers only: no price.
        OffersOnly = 1,
        /// At its price, or by an accepted offer.
        PriceAndOffers = 2,
    }
}

wire_enum! {
    /// A yes/no filter that can be left open.
    TriState {
        Any = 0,
        Yes = 1,
        No = 2,
    }
}

wire_enum! {
    /// The search's price mode chips.
    PriceMode {
        Any = 0,
        HasPrice = 1,
        TakesOffers = 2,
        /// No gold in the price ("runes only").
        CurrenciesOnly = 3,
        GoldOnly = 4,
    }
}

wire_enum! {
    /// The search's sorts.
    SortBy {
        Posted = 0,
        /// In [`Sort::currency`] ("" gold); only listings priced in it.
        Price = 1,
        /// By [`Sort::stat`]; only items with it.
        Stat = 2,
        Level = 3,
        Name = 4,
        Seller = 5,
    }
}

wire_enum! {
    /// What a [`Ask::Answer`] does to an offer.
    AnswerHow {
        /// The seller an open offer, or the buyer a counter.
        Accept = 0,
        /// The seller.
        Decline = 1,
        /// The seller, with the counter price.
        Counter = 2,
        /// The buyer takes the offer back.
        Withdraw = 3,
    }
}

wire_enum! {
    /// My shop's tabs.
    ShopTab {
        Shown = 0,
        Hidden = 1,
        Sold = 2,
        /// Expired and cancelled.
        Expired = 3,
    }
}

wire_enum! {
    /// My shop's sorts.
    ShopSort {
        Posted = 0,
        Ends = 1,
        GoldPrice = 2,
        Name = 3,
    }
}

wire_enum! {
    /// Which offers a [`Ask::OffersOpen`] lists.
    OffersWhich {
        OnMyListings = 0,
        MadeByMe = 1,
    }
}

wire_enum! {
    /// Why [`MarketState::on`] is false.
    OffReason {
        /// The market is on.
        On = 0,
        /// The realm's `market` switch is off.
        Switch = 1,
        NoCharacter = 2,
    }
}

wire_enum! {
    /// A listing's state (the shop's rows, the listing's details).
    ListingState {
        Active = 0,
        Sold = 1,
        Cancelled = 2,
        Expired = 3,
    }
}

wire_enum! {
    /// An offer's state.
    OfferState {
        Open = 0,
        Countered = 1,
        Accepted = 2,
        Declined = 3,
        Withdrawn = 4,
        /// Its listing closed (sold to someone else, cancelled, expired).
        Returned = 5,
    }
}

wire_enum! {
    /// The currencies a price-history series was paid in.
    PriceShape {
        /// One currency: drawn as a line.
        One = 0,
        /// A bundle of currencies: drawn as points.
        Bundle = 1,
        /// Sales that included items: counted, values left out.
        WithItems = 2,
    }
}

/// Where a delivery came from ([`DeliveryRow::source`]); the realm may add
/// more, so the field stays a byte.
pub mod source {
    pub const BOUGHT: u8 = 1;
    /// A sale's proceeds.
    pub const SOLD: u8 = 2;
    pub const CANCELLED: u8 = 3;
    pub const EXPIRED: u8 = 4;
    /// An offer's goods back.
    pub const RETURNED: u8 = 5;
    /// What a counter didn't take.
    pub const CHANGE: u8 = 6;
    pub const OTHER: u8 = 255;
}

/// Why a delivery's goods came back ([`DeliveryRow::reason`]), for the
/// client's refund line; the realm may add more, so the field stays a
/// byte.
pub mod reason {
    /// Not a refund: bought goods or a sale's proceeds.
    pub const NONE: u8 = 0;
    /// The seller declined the offer.
    pub const DECLINED: u8 = 1;
    /// The buyer took the offer back.
    pub const WITHDRAWN: u8 = 2;
    /// The listing was sold to someone else.
    pub const OUTBID: u8 = 3;
    pub const EXPIRED: u8 = 4;
    pub const CANCELLED: u8 = 5;
    /// What an accepted offer or counter held beyond the price.
    pub const CHANGE: u8 = 6;
}

/// What a [`Notice`] says ([`Notice::what`]); a client ignores one it
/// doesn't know.
pub mod notice {
    /// To the seller: bought at its price, or an offer or counter accepted.
    pub const SOLD: u8 = 1;
    /// To the seller: a new offer on their listing.
    pub const OFFER_RECEIVED: u8 = 2;
    /// To the buyer: the seller countered their offer.
    pub const COUNTERED: u8 = 3;
    /// To the buyer: accepted; the goods are in Deliveries.
    pub const OFFER_ACCEPTED: u8 = 4;
    /// To the buyer: declined; their goods are back in Deliveries.
    pub const OFFER_DECLINED: u8 = 5;
    /// To the buyer: the listing sold to someone else, was cancelled or
    /// expired.
    pub const OFFER_RETURNED: u8 = 6;
    /// To the seller: the goods are back in Deliveries.
    pub const LISTING_EXPIRED: u8 = 7;
    /// To the seller: the buyer took their offer back.
    pub const OFFER_WITHDRAWN: u8 = 8;
}

fn read_vec<T>(
    r: &mut Reader,
    max: usize,
    mut get: impl FnMut(&mut Reader) -> Result<T, NetError>,
) -> Result<Vec<T>, NetError> {
    let n = r.u8()? as usize;
    if n > max {
        return Err(NetError::Malformed);
    }
    (0..n).map(|_| get(r)).collect()
}

/// Writes a `u8` count; the caller keeps lists inside their bound.
fn write_len(w: &mut Writer, n: usize) {
    w.u8(n.min(u8::MAX as usize) as u8);
}

fn write_i32(w: &mut Writer, v: i32) {
    w.u32(v as u32);
}

fn read_i32(r: &mut Reader) -> Result<i32, NetError> {
    Ok(r.u32()? as i32)
}

/// A price: gold, stackables (the realm's currencies), and specific stash
/// items (offers and counters only; a listing's price never names items).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Price {
    pub gold: u64,
    /// `(stackable code, count)`.
    pub stacks: Vec<(String, u32)>,
    /// The asker's stash item IDs.
    pub items: Vec<u32>,
}

impl Price {
    /// A price in gold only.
    pub fn gold(gold: u64) -> Self {
        Self {
            gold,
            ..Self::default()
        }
    }

    fn encode(&self, w: &mut Writer) {
        w.u64(self.gold);
        write_len(w, self.stacks.len());
        for (code, count) in &self.stacks {
            w.str8(code).u32(*count);
        }
        write_len(w, self.items.len());
        for id in &self.items {
            w.u32(*id);
        }
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            gold: r.u64()?,
            stacks: read_vec(r, MAX_CURRENCIES, |r| Ok((r.str8(MAX_CODE)?, r.u32()?)))?,
            items: read_vec(r, MAX_PRICE_ITEMS, |r| r.u32())?,
        })
    }
}

fn put_price(w: &mut Writer, p: Option<&Price>) {
    w.opt(p, |w, p| p.encode(w));
}

fn get_price(r: &mut Reader) -> Result<Option<Price>, NetError> {
    r.opt(Price::decode)
}

/// What a listing, delivery or offer moves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Goods {
    /// Kind 0: an item and its save-format record.
    Item { item_id: u32, record: Vec<u8> },
    /// Kind 1: a count of a stackable.
    Stack { code: String, count: u32 },
    /// Kind 2: gold (deliveries only).
    Gold(u64),
}

impl Goods {
    fn encode(&self, w: &mut Writer) {
        match self {
            Self::Item { item_id, record } => {
                w.u8(0).u32(*item_id).bytes16(record);
            }
            Self::Stack { code, count } => {
                w.u8(1).str8(code).u32(*count);
            }
            Self::Gold(g) => {
                w.u8(2).u64(*g);
            }
        }
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(match r.u8()? {
            0 => Self::Item {
                item_id: r.u32()?,
                record: r.bytes16(MAX_RECORD)?,
            },
            1 => Self::Stack {
                code: r.str8(MAX_CODE)?,
                count: r.u32()?,
            },
            2 => Self::Gold(r.u64()?),
            _ => return Err(NetError::Malformed),
        })
    }

    /// Goods a listing can hold: an item or a stackable, not gold.
    fn decode_listed(r: &mut Reader) -> Result<Self, NetError> {
        match Self::decode(r)? {
            Self::Gold(_) => Err(NetError::Malformed),
            g => Ok(g),
        }
    }
}

/// One listing as the search, the shop and the details show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListingRow {
    pub listing: u64,
    pub group: Group,
    /// An item or a stackable.
    pub goods: Goods,
    pub mode: ListingMode,
    /// `None` for offers only.
    pub price: Option<Price>,
    /// The realm's hash of the price; [`Ask::Buy`] echoes it.
    pub price_hash: [u8; 32],
    /// The seller's character name when posted.
    pub seller: String,
    /// The seller account's sold listings in this economy.
    pub seller_sales: u32,
    pub posted_ms: u64,
    pub expires_ms: u64,
    /// The seller's public note, may be empty.
    pub note: String,
    /// Live offers on it.
    pub offers: u16,
    /// The asker's own listing (no Buy or Offer).
    pub mine: bool,
}

impl ListingRow {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.listing).u8(self.group as u8);
        self.goods.encode(w);
        w.u8(self.mode as u8);
        put_price(w, self.price.as_ref());
        w.array(&self.price_hash)
            .str8(&self.seller)
            .u32(self.seller_sales)
            .u64(self.posted_ms)
            .u64(self.expires_ms)
            .str8(&self.note)
            .u16(self.offers)
            .bool(self.mine);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            listing: r.u64()?,
            group: Group::read(r)?,
            goods: Goods::decode_listed(r)?,
            mode: ListingMode::read(r)?,
            price: get_price(r)?,
            price_hash: r.array()?,
            seller: r.str8(MAX_NAME)?,
            seller_sales: r.u32()?,
            posted_ms: r.u64()?,
            expires_ms: r.u64()?,
            note: r.str8(MAX_NOTE)?,
            offers: r.u16()?,
            mine: r.bool()?,
        })
    }
}

/// The asker's live offer on a listing, in its details
/// ([`ListingDetail::my_offer`]). The listing, its goods and seller are the
/// details' row; the buyer is the asker. The offered items' records come as
/// [`FromRealm::Item`]s before the details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListingOffer {
    pub offer: u64,
    pub offered: Price,
    pub counter: Option<Price>,
    pub state: OfferState,
    pub at_ms: u64,
    /// 0: not answered yet.
    pub answered_ms: u64,
}

impl ListingOffer {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.offer);
        self.offered.encode(w);
        put_price(w, self.counter.as_ref());
        w.u8(self.state as u8).u64(self.at_ms).u64(self.answered_ms);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            offer: r.u64()?,
            offered: Price::decode(r)?,
            counter: get_price(r)?,
            state: OfferState::read(r)?,
            at_ms: r.u64()?,
            answered_ms: r.u64()?,
        })
    }
}

/// One offer, on the asker's listing or made by the asker. The offered
/// items' records come as [`FromRealm::Item`]s before the row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfferRow {
    pub offer: u64,
    pub listing: u64,
    /// The listing's goods.
    pub goods: Goods,
    /// The listing's price.
    pub price: Option<Price>,
    pub buyer: String,
    pub seller: String,
    pub offered: Price,
    pub counter: Option<Price>,
    pub state: OfferState,
    pub at_ms: u64,
    /// 0: not answered yet.
    pub answered_ms: u64,
}

impl OfferRow {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.offer).u64(self.listing);
        self.goods.encode(w);
        put_price(w, self.price.as_ref());
        w.str8(&self.buyer).str8(&self.seller);
        self.offered.encode(w);
        put_price(w, self.counter.as_ref());
        w.u8(self.state as u8).u64(self.at_ms).u64(self.answered_ms);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            offer: r.u64()?,
            listing: r.u64()?,
            goods: Goods::decode_listed(r)?,
            price: get_price(r)?,
            buyer: r.str8(MAX_NAME)?,
            seller: r.str8(MAX_NAME)?,
            offered: Price::decode(r)?,
            counter: get_price(r)?,
            state: OfferState::read(r)?,
            at_ms: r.u64()?,
            answered_ms: r.u64()?,
        })
    }
}

/// How a shop's listing sold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sold {
    pub buyer: String,
    /// The listing price, or the accepted offer or counter.
    pub paid: Price,
    /// Gold the realm kept (0 while fees are off).
    pub sale_fee: u64,
    pub closed_ms: u64,
}

/// One row of My shop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShopRow {
    pub row: ListingRow,
    pub state: ListingState,
    pub hidden: bool,
    pub live_offers: u16,
    pub newest_offer_ms: u64,
    pub sold: Option<Sold>,
    /// Cancelled or expired, and its goods are still in the seller's
    /// Deliveries or stash.
    pub relistable: bool,
}

impl ShopRow {
    fn encode(&self, w: &mut Writer) {
        self.row.encode(w);
        w.u8(self.state as u8)
            .bool(self.hidden)
            .u16(self.live_offers)
            .u64(self.newest_offer_ms);
        w.opt(self.sold.as_ref(), |w, s| {
            w.str8(&s.buyer);
            s.paid.encode(w);
            w.u64(s.sale_fee).u64(s.closed_ms);
        });
        w.bool(self.relistable);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            row: ListingRow::decode(r)?,
            state: ListingState::read(r)?,
            hidden: r.bool()?,
            live_offers: r.u16()?,
            newest_offer_ms: r.u64()?,
            sold: r.opt(|r| {
                Ok(Sold {
                    buyer: r.str8(MAX_NAME)?,
                    paid: Price::decode(r)?,
                    sale_fee: r.u64()?,
                    closed_ms: r.u64()?,
                })
            })?,
            relistable: r.bool()?,
        })
    }
}

/// One row of Deliveries: what the market gave the account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryRow {
    pub delivery: u64,
    /// [`source`].
    pub source: u8,
    /// [`reason`].
    pub reason: u8,
    pub listing: Option<u64>,
    pub offer: Option<u64>,
    /// The name of the listing's goods.
    pub about: String,
    pub goods: Goods,
    pub at_ms: u64,
}

impl DeliveryRow {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.delivery).u8(self.source).u8(self.reason);
        w.opt(self.listing.as_ref(), |w, v| {
            w.u64(*v);
        });
        w.opt(self.offer.as_ref(), |w, v| {
            w.u64(*v);
        });
        w.str8(&self.about);
        self.goods.encode(w);
        w.u64(self.at_ms);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            delivery: r.u64()?,
            source: r.u8()?,
            reason: r.u8()?,
            listing: r.opt(|r| r.u64())?,
            offer: r.opt(|r| r.u64())?,
            about: r.str8(MAX_ABOUT)?,
            goods: Goods::decode(r)?,
            at_ms: r.u64()?,
        })
    }
}

/// An item a following row names by ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemRecord {
    pub item_id: u32,
    pub record: Vec<u8>,
}

/// A stat range of a query, both ends included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatRange {
    pub stat: u16,
    pub min: i32,
    pub max: i32,
}

/// A query's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sort {
    pub by: SortBy,
    /// For [`SortBy::Stat`].
    pub stat: u16,
    /// For [`SortBy::Price`]: "" gold, else a stackable code.
    pub currency: String,
    pub desc: bool,
}

/// What a search asks for; every set field must match (contract 3.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketQuery {
    /// Word prefixes of any name.
    pub text: String,
    /// Any of these; empty = all.
    pub groups: Vec<Group>,
    /// Item type codes of the type tree; a node matches its descendants;
    /// empty = all.
    pub types: Vec<String>,
    pub ethereal: TriState,
    pub socketed: TriState,
    pub identified: TriState,
    pub stats: Vec<StatRange>,
    /// Level requirement, both ends included; `(0, 255)` = any.
    pub level: (u8, u8),
    /// Sockets, both ends included; `(0, 255)` = any.
    pub sockets: (u8, u8),
    /// Items restricted to that class.
    pub class: Option<u8>,
    pub price_mode: PriceMode,
    /// `(currency, max)`: a listing priced only in that currency ("" gold)
    /// and at most `max`; caps are OR'd; empty = no cap.
    pub caps: Vec<(String, u64)>,
    pub sort: Sort,
    /// Order by [`Group::GRID_ORDER`] first (the grid view).
    pub group_first: bool,
    /// From 0.
    pub page: u16,
    /// 25, 50 or 100 (the realm refuses others).
    pub page_size: u8,
    /// Also send [`Facets`].
    pub facets: bool,
}

impl Default for MarketQuery {
    /// Everything, newest first, 50 a page.
    fn default() -> Self {
        Self {
            text: String::new(),
            groups: Vec::new(),
            types: Vec::new(),
            ethereal: TriState::Any,
            socketed: TriState::Any,
            identified: TriState::Any,
            stats: Vec::new(),
            level: (0, 255),
            sockets: (0, 255),
            class: None,
            price_mode: PriceMode::Any,
            caps: Vec::new(),
            sort: Sort {
                by: SortBy::Posted,
                stat: 0,
                currency: String::new(),
                desc: true,
            },
            group_first: false,
            page: 0,
            page_size: 50,
            facets: false,
        }
    }
}

impl MarketQuery {
    fn encode(&self, w: &mut Writer) {
        w.str8(&self.text);
        write_len(w, self.groups.len());
        for g in &self.groups {
            w.u8(*g as u8);
        }
        write_len(w, self.types.len());
        for t in &self.types {
            w.str8(t);
        }
        w.u8(self.ethereal as u8)
            .u8(self.socketed as u8)
            .u8(self.identified as u8);
        write_len(w, self.stats.len());
        for s in &self.stats {
            w.u16(s.stat);
            write_i32(w, s.min);
            write_i32(w, s.max);
        }
        w.u8(self.level.0)
            .u8(self.level.1)
            .u8(self.sockets.0)
            .u8(self.sockets.1);
        w.opt(self.class.as_ref(), |w, c| {
            w.u8(*c);
        });
        w.u8(self.price_mode as u8);
        write_len(w, self.caps.len());
        for (currency, max) in &self.caps {
            w.str8(currency).u64(*max);
        }
        w.u8(self.sort.by as u8)
            .u16(self.sort.stat)
            .str8(&self.sort.currency)
            .bool(self.sort.desc)
            .bool(self.group_first)
            .u16(self.page)
            .u8(self.page_size)
            .bool(self.facets);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            text: r.str8(MAX_QUERY_TEXT)?,
            groups: read_vec(r, Group::ALL.len(), Group::read)?,
            types: read_vec(r, MAX_QUERY_TYPES, |r| r.str8(MAX_TYPE_CODE))?,
            ethereal: TriState::read(r)?,
            socketed: TriState::read(r)?,
            identified: TriState::read(r)?,
            stats: read_vec(r, MAX_QUERY_STATS, |r| {
                Ok(StatRange {
                    stat: r.u16()?,
                    min: read_i32(r)?,
                    max: read_i32(r)?,
                })
            })?,
            level: (r.u8()?, r.u8()?),
            sockets: (r.u8()?, r.u8()?),
            class: r.opt(|r| r.u8())?,
            price_mode: PriceMode::read(r)?,
            caps: read_vec(r, MAX_CAPS, |r| Ok((r.str8(MAX_CODE)?, r.u64()?)))?,
            sort: Sort {
                by: SortBy::read(r)?,
                stat: r.u16()?,
                currency: r.str8(MAX_CODE)?,
                desc: r.bool()?,
            },
            group_first: r.bool()?,
            page: r.u16()?,
            page_size: r.u8()?,
            facets: r.bool()?,
        })
    }
}

/// The search's counts: each is the listings that match the query with
/// that facet's own filter left out (contract 3.2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facets {
    pub groups: Vec<(Group, u32)>,
    /// Every type code with at least one listing, counted at each ancestor
    /// too.
    pub types: Vec<(String, u32)>,
    /// `[yes, no]`.
    pub ethereal: [u32; 2],
    pub socketed: [u32; 2],
    pub identified: [u32; 2],
    /// By [`PriceMode`] 0-4.
    pub price_modes: [u32; 5],
}

impl Facets {
    fn encode(&self, w: &mut Writer) {
        write_len(w, self.groups.len());
        for (g, n) in &self.groups {
            w.u8(*g as u8).u32(*n);
        }
        w.u16(self.types.len().min(u16::MAX as usize) as u16);
        for (t, n) in &self.types {
            w.str8(t).u32(*n);
        }
        for v in self
            .ethereal
            .iter()
            .chain(&self.socketed)
            .chain(&self.identified)
            .chain(&self.price_modes)
        {
            w.u32(*v);
        }
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let groups = read_vec(r, Group::ALL.len(), |r| Ok((Group::read(r)?, r.u32()?)))?;
        let n = r.u16()? as usize;
        if n > MAX_FACET_TYPES {
            return Err(NetError::Malformed);
        }
        let types = (0..n)
            .map(|_| Ok((r.str8(MAX_TYPE_CODE)?, r.u32()?)))
            .collect::<Result<_, NetError>>()?;
        let mut two = || -> Result<[u32; 2], NetError> { Ok([r.u32()?, r.u32()?]) };
        let (ethereal, socketed, identified) = (two()?, two()?, two()?);
        let mut price_modes = [0; 5];
        for v in &mut price_modes {
            *v = r.u32()?;
        }
        Ok(Self {
            groups,
            types,
            ethereal,
            socketed,
            identified,
            price_modes,
        })
    }
}

/// The realm's market limits for the account's economy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarketLimits {
    pub max_listings: u16,
    pub max_offers: u16,
    pub max_days: u8,
    pub max_offer_items: u8,
    pub searches_per_min: u16,
    pub posts_per_min: u16,
}

/// The realm's fees (all 0 today) and their revision, which every paying
/// request echoes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fees {
    /// Share of a sale's gold the realm keeps.
    pub sale_fee_pct: u8,
    /// Gold per listing and per relist.
    pub listing_fee: u64,
    /// Gold a buyer pays on top of the price.
    pub buyer_fee: u64,
    pub fee_rev: u32,
}

/// What waits for the account.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarketCounts {
    pub active_listings: u16,
    pub deliveries_waiting: u16,
    pub live_offers_to_me: u16,
    pub my_live_offers: u16,
}

/// S→C 0xB1: the answer to [`Ask::Hello`], and pushed with `req` 0 when
/// anything in it changes (contract section 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketState {
    /// 0 when pushed.
    pub req: u64,
    pub on: bool,
    pub off_reason: OffReason,
    /// The economy: the selected character's mod realm and split.
    pub mod_id: String,
    pub hardcore: bool,
    pub ladder: u16,
    pub expansion: bool,
    pub limits: MarketLimits,
    pub fees: Fees,
    pub counts: MarketCounts,
    /// The highest `req` the realm has stored for this account: the client
    /// numbers its requests above it.
    pub last_req: u64,
    /// The realm's stackable codes: the currencies a price may hold.
    pub stackables: Vec<String>,
}

impl MarketState {
    fn encode(&self, w: &mut Writer) {
        let (l, f, c) = (&self.limits, &self.fees, &self.counts);
        w.u64(self.req)
            .bool(self.on)
            .u8(self.off_reason as u8)
            .str8(&self.mod_id)
            .bool(self.hardcore)
            .u16(self.ladder)
            .bool(self.expansion)
            .u16(l.max_listings)
            .u16(l.max_offers)
            .u8(l.max_days)
            .u8(l.max_offer_items)
            .u16(l.searches_per_min)
            .u16(l.posts_per_min)
            .u8(f.sale_fee_pct)
            .u64(f.listing_fee)
            .u64(f.buyer_fee)
            .u32(f.fee_rev)
            .u16(c.active_listings)
            .u16(c.deliveries_waiting)
            .u16(c.live_offers_to_me)
            .u16(c.my_live_offers)
            .u64(self.last_req);
        write_len(w, self.stackables.len());
        for s in &self.stackables {
            w.str8(s);
        }
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            req: r.u64()?,
            on: r.bool()?,
            off_reason: OffReason::read(r)?,
            mod_id: r.str8(MAX_MOD_ID)?,
            hardcore: r.bool()?,
            ladder: r.u16()?,
            expansion: r.bool()?,
            limits: MarketLimits {
                max_listings: r.u16()?,
                max_offers: r.u16()?,
                max_days: r.u8()?,
                max_offer_items: r.u8()?,
                searches_per_min: r.u16()?,
                posts_per_min: r.u16()?,
            },
            fees: Fees {
                sale_fee_pct: r.u8()?,
                listing_fee: r.u64()?,
                buyer_fee: r.u64()?,
                fee_rev: r.u32()?,
            },
            counts: MarketCounts {
                active_listings: r.u16()?,
                deliveries_waiting: r.u16()?,
                live_offers_to_me: r.u16()?,
                my_live_offers: r.u16()?,
            },
            last_req: r.u64()?,
            stackables: read_vec(r, u8::MAX as usize, |r| r.str8(MAX_CODE))?,
        })
    }
}

/// S→C 0xCC: a request's answer when it is not a list (or a list that was
/// refused): done (code 0) or the refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Done {
    pub req: u64,
    /// [`ErrorCode`]; 0 done.
    pub code: ErrorCode,
    /// For [`ErrorCode::RATE_LIMITED`].
    pub wait_s: u32,
    /// A repeat of a stored request: the stored answer, nothing changed.
    pub repeated: bool,
    /// The listing, offer or new listing.
    pub id: Option<u64>,
    /// Per-row results of a shop edit or a claim: `(id, code)`.
    pub rows: Vec<(u64, ErrorCode)>,
    /// English, for a code the client doesn't know.
    pub text: String,
}

impl Done {
    /// A request carried out.
    pub fn ok(req: u64) -> Self {
        Self::refused(req, ErrorCode(0))
    }

    /// A request refused with the code's own words.
    pub fn refused(req: u64, code: ErrorCode) -> Self {
        Self {
            req,
            code,
            wait_s: 0,
            repeated: false,
            id: None,
            rows: Vec::new(),
            text: if code.0 == 0 {
                String::new()
            } else {
                code.text().to_owned()
            },
        }
    }

    pub fn is_ok(&self) -> bool {
        self.code.0 == 0
    }

    /// What the player reads for a refusal: the market's words for a code
    /// this build knows, the realm's text for one it doesn't.
    pub fn message(&self) -> String {
        refusal_text(self.code, self.wait_s, &self.text)
    }

    fn encode(&self, w: &mut Writer) {
        w.u64(self.req)
            .u16(self.code.0)
            .u32(self.wait_s)
            .bool(self.repeated);
        w.opt(self.id.as_ref(), |w, v| {
            w.u64(*v);
        });
        write_len(w, self.rows.len());
        for (id, code) in &self.rows {
            w.u64(*id).u16(code.0);
        }
        w.str8(&self.text);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            req: r.u64()?,
            code: ErrorCode(r.u16()?),
            wait_s: r.u32()?,
            repeated: r.bool()?,
            id: r.opt(|r| r.u64())?,
            rows: read_vec(r, u8::MAX as usize, |r| Ok((r.u64()?, ErrorCode(r.u16()?))))?,
            text: r.str8(MAX_ERROR_TEXT)?,
        })
    }
}

/// The market's words for a refusal code (contract section 6): the code's
/// own text, the wait for [`ErrorCode::RATE_LIMITED`], and the realm's
/// `text` for a code this build doesn't know.
pub fn refusal_text(code: ErrorCode, wait_s: u32, text: &str) -> String {
    match code {
        ErrorCode::RATE_LIMITED => format!("Too many market requests; wait {wait_s} s."),
        c if c.0 == 0 => String::new(),
        c if ErrorCode::ALL.contains(&c) && c != ErrorCode::OTHER => c.text().to_owned(),
        _ => text.to_owned(),
    }
}

/// S→C 0xCD: something happened to the account's listing or offer
/// (contract section 5). Pushed, not stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// [`notice`].
    pub what: u8,
    pub listing: u64,
    pub offer: Option<u64>,
    /// The goods' name.
    pub about: String,
    /// The other character.
    pub who: String,
    /// Sold for, offered, countered.
    pub price: Option<Price>,
    pub deliveries_waiting: u16,
    pub at_ms: u64,
}

impl Notice {
    fn encode(&self, w: &mut Writer) {
        w.u8(self.what).u64(self.listing);
        w.opt(self.offer.as_ref(), |w, v| {
            w.u64(*v);
        });
        w.str8(&self.about).str8(&self.who);
        put_price(w, self.price.as_ref());
        w.u16(self.deliveries_waiting).u64(self.at_ms);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            what: r.u8()?,
            listing: r.u64()?,
            offer: r.opt(|r| r.u64())?,
            about: r.str8(MAX_ABOUT)?,
            who: r.str8(MAX_NAME)?,
            price: get_price(r)?,
            deliveries_waiting: r.u16()?,
            at_ms: r.u64()?,
        })
    }
}

/// S→C 0xB3: a search's page; `rows` [`FromRealm::Row`]s (and the facets
/// when asked) follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResultsHead {
    pub total: u32,
    pub page: u16,
    pub page_size: u8,
    pub rows: u8,
    pub now_ms: u64,
    /// A [`FromRealm::Facets`] follows.
    pub facets: bool,
}

/// S→C 0xB7: one listing's details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListingDetail {
    pub state: ListingState,
    pub row: ListingRow,
    /// Gold added to the price (0 today).
    pub buyer_fee: u64,
    pub fee_rev: u32,
    /// The asker's live offer on it.
    pub my_offer: Option<ListingOffer>,
}

/// S→C 0xBE: a page of My shop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShopHead {
    pub tab: ShopTab,
    pub page: u16,
    pub rows: u8,
    /// Rows in each [`ShopTab`].
    pub counts: [u32; 4],
    /// Active listings, shown and hidden.
    pub slots_used: u16,
    pub slots_max: u16,
    pub now_ms: u64,
}

/// S→C 0xC1: a page of offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OffersHead {
    pub which: OffersWhich,
    pub page: u16,
    pub rows: u8,
    pub total: u32,
    pub now_ms: u64,
}

/// S→C 0xC4: a page of Deliveries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeliveriesHead {
    pub page: u16,
    pub rows: u8,
    pub total: u32,
    pub now_ms: u64,
}

/// S→C 0xC9: a price check; `series` series follow, each in one or more
/// [`FromRealm::PriceSeries`], then [`FromRealm::PriceRecent`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PricesHead {
    /// The price key, for a later [`PriceOf::Key`].
    pub key: String,
    /// "Harlequin Crest".
    pub label: String,
    pub now_ms: u64,
    /// One hour for 2 days or less, else one day.
    pub bucket_ms: u64,
    pub listed_now: u32,
    pub sold: u32,
    pub series: u8,
}

/// The min, median and max price in one currency over a bucket's sales.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spread {
    pub min: u64,
    pub median: u64,
    pub max: u64,
}

/// One time bucket with sales.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bucket {
    /// 0 is the newest, ending at the head's `now_ms`.
    pub index: u32,
    pub sales: u16,
    /// One per currency of the series; none for [`PriceShape::WithItems`].
    pub values: Vec<Spread>,
}

/// Sales paid in one set of currencies (contract 3.4). No conversion
/// between currencies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriceSeries {
    pub index: u8,
    pub shape: PriceShape,
    /// Sorted; "" gold, else a stackable code.
    pub currencies: Vec<String>,
    pub sold: u32,
    pub listed_now: u32,
    /// Per currency, the cheapest active listing of this shape; empty when
    /// none is listed.
    pub lowest_now: Vec<u64>,
    /// Per currency, the median (the lower middle) of those listings;
    /// empty when none is listed.
    pub median_now: Vec<u64>,
    pub buckets: Vec<Bucket>,
}

/// Bytes one bucket takes with `values` spreads.
const fn bucket_len(values: usize) -> usize {
    4 + 2 + values * 24
}

impl PriceSeries {
    /// Spreads each bucket carries.
    fn values(&self) -> usize {
        match self.shape {
            PriceShape::WithItems => 0,
            _ => self.currencies.len(),
        }
    }

    fn encode(&self, w: &mut Writer) {
        w.u8(self.index).u8(self.shape as u8);
        write_len(w, self.currencies.len());
        for c in &self.currencies {
            w.str8(c);
        }
        w.u32(self.sold).u32(self.listed_now);
        write_len(w, self.lowest_now.len());
        for v in &self.lowest_now {
            w.u64(*v);
        }
        write_len(w, self.median_now.len());
        for v in &self.median_now {
            w.u64(*v);
        }
        w.u16(self.buckets.len().min(u16::MAX as usize) as u16);
        for b in &self.buckets {
            w.u32(b.index).u16(b.sales);
            for s in &b.values {
                w.u64(s.min).u64(s.median).u64(s.max);
            }
        }
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let index = r.u8()?;
        let shape = PriceShape::read(r)?;
        let currencies = read_vec(r, MAX_CURRENCIES, |r| r.str8(MAX_CODE))?;
        let sold = r.u32()?;
        let listed_now = r.u32()?;
        let lowest_now = read_vec(r, MAX_CURRENCIES, |r| r.u64())?;
        let median_now = read_vec(r, MAX_CURRENCIES, |r| r.u64())?;
        let mut s = Self {
            index,
            shape,
            currencies,
            sold,
            listed_now,
            lowest_now,
            median_now,
            buckets: Vec::new(),
        };
        let per_currency = |v: &[u64]| v.is_empty() || v.len() == s.currencies.len();
        if !per_currency(&s.lowest_now) || !per_currency(&s.median_now) {
            return Err(NetError::Malformed);
        }
        let values = s.values();
        for _ in 0..r.u16()? {
            let index = r.u32()?;
            let sales = r.u16()?;
            let values = (0..values)
                .map(|_| {
                    Ok(Spread {
                        min: r.u64()?,
                        median: r.u64()?,
                        max: r.u64()?,
                    })
                })
                .collect::<Result<_, NetError>>()?;
            s.buckets.push(Bucket {
                index,
                sales,
                values,
            });
        }
        Ok(s)
    }

    /// The series in parts whose [`FromRealm::PriceSeries`] bodies each fit
    /// a frame (the realm sends a long series as several messages with the
    /// same index; the client appends their buckets). Always at least one
    /// part.
    pub fn split_to_fit(&self) -> Vec<PriceSeries> {
        let empty = PriceSeries {
            buckets: Vec::new(),
            ..self.clone()
        };
        let head = FromRealm::PriceSeries {
            req: 0,
            series: empty.clone(),
        }
        .body()
        .len();
        let per = bucket_len(self.values());
        let room = crate::wire::MAX_FRAME.saturating_sub(head) / per;
        let room = room.clamp(1, u16::MAX as usize);
        let mut parts: Vec<PriceSeries> = self
            .buckets
            .chunks(room)
            .map(|c| PriceSeries {
                buckets: c.to_vec(),
                ..empty.clone()
            })
            .collect();
        if parts.is_empty() {
            parts.push(empty);
        }
        parts
    }
}

/// One recent sale of a price check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sale {
    pub at_ms: u64,
    pub price: Price,
}

/// What a [`Ask::Post`] lists, from the shared stash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PostGoods {
    Item { item_id: u32 },
    Stack { code: String, count: u32 },
}

/// C→S 0xBB: a new listing. The seller name is the selected character's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Post {
    pub goods: PostGoods,
    pub mode: ListingMode,
    /// Gold and stackables; `None` for offers only.
    pub price: Option<Price>,
    /// 1 to the realm's `max_days`.
    pub days: u8,
    /// 0-60 bytes.
    pub note: String,
    /// Post straight to the Hidden tab.
    pub hidden: bool,
    pub fee_rev: u32,
}

/// What a [`Ask::ShopEdit`] does to each named listing (contract 3.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShopOp {
    Cancel,
    Relist {
        days: u8,
    },
    Hide,
    Show,
    Note(String),
    Reprice {
        mode: ListingMode,
        price: Option<Price>,
    },
}

/// What a [`Ask::Prices`] checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PriceOf {
    Listing(u64),
    /// One of the asker's stash items.
    StashItem(u32),
    Stackable(String),
    /// A key from an earlier answer.
    Key(String),
}

/// What a C→S market message asks (the request number is [`ToRealm::req`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// 0xB0: entering a game or the lobby with a character, and after a
    /// reconnect; answered with [`MarketState`].
    Hello,
    /// 0xB2.
    Search(MarketQuery),
    /// 0xB6.
    ListingGet { listing: u64 },
    /// 0xB8: buy at the price whose hash the row carried.
    Buy {
        listing: u64,
        price_hash: [u8; 32],
        fee_rev: u32,
    },
    /// 0xB9: gold, stackables, the buyer's stash items.
    Offer {
        listing: u64,
        price: Price,
        fee_rev: u32,
    },
    /// 0xBA.
    Answer {
        offer: u64,
        how: AnswerHow,
        /// The counter: gold, stackables and any of the offered items to
        /// keep.
        counter: Option<Price>,
        fee_rev: u32,
    },
    /// 0xBB.
    Post(Post),
    /// 0xBC: one op on 1-50 of the asker's listings.
    ShopEdit {
        op: ShopOp,
        listings: Vec<u64>,
        fee_rev: u32,
    },
    /// 0xBD: 50 rows a page.
    ShopOpen {
        tab: ShopTab,
        page: u16,
        sort: ShopSort,
        desc: bool,
    },
    /// 0xC0: 50 a page.
    OffersOpen {
        which: OffersWhich,
        live_only: bool,
        page: u16,
    },
    /// 0xC3: 50 a page.
    DeliveriesOpen { page: u16 },
    /// 0xC6: into the shared stash; `deliveries` is ignored when `all`.
    Claim { all: bool, deliveries: Vec<u64> },
    /// 0xC8: `days` 1-90.
    Prices { of: PriceOf, days: u16 },
}

impl Ask {
    pub fn kind(&self) -> u8 {
        match self {
            Self::Hello => kind::HELLO,
            Self::Search(_) => kind::SEARCH,
            Self::ListingGet { .. } => kind::LISTING_GET,
            Self::Buy { .. } => kind::BUY,
            Self::Offer { .. } => kind::OFFER,
            Self::Answer { .. } => kind::ANSWER,
            Self::Post(_) => kind::POST,
            Self::ShopEdit { .. } => kind::SHOP_EDIT,
            Self::ShopOpen { .. } => kind::SHOP_OPEN,
            Self::OffersOpen { .. } => kind::OFFERS_OPEN,
            Self::DeliveriesOpen { .. } => kind::DELIVERIES_OPEN,
            Self::Claim { .. } => kind::CLAIM,
            Self::Prices { .. } => kind::PRICES,
        }
    }

    /// A changing request: the realm stores its answer under the `req`
    /// for 24 h, and a repeat gets that answer. Reads are not stored.
    pub fn changes(&self) -> bool {
        matches!(
            self,
            Self::Buy { .. }
                | Self::Offer { .. }
                | Self::Answer { .. }
                | Self::Post(_)
                | Self::ShopEdit { .. }
                | Self::Claim { .. }
        )
    }
}

/// A C→S market message: the request number and what it asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToRealm {
    pub req: u64,
    pub ask: Ask,
}

impl ToRealm {
    pub fn kind(&self) -> u8 {
        self.ask.kind()
    }

    pub fn body(&self) -> Vec<u8> {
        let mut w = Writer::new(self.kind());
        w.u64(self.req);
        match &self.ask {
            Ask::Hello => {}
            Ask::Search(q) => q.encode(&mut w),
            Ask::ListingGet { listing } => {
                w.u64(*listing);
            }
            Ask::Buy {
                listing,
                price_hash,
                fee_rev,
            } => {
                w.u64(*listing).array(price_hash).u32(*fee_rev);
            }
            Ask::Offer {
                listing,
                price,
                fee_rev,
            } => {
                w.u64(*listing);
                price.encode(&mut w);
                w.u32(*fee_rev);
            }
            Ask::Answer {
                offer,
                how,
                counter,
                fee_rev,
            } => {
                w.u64(*offer).u8(*how as u8);
                put_price(&mut w, counter.as_ref());
                w.u32(*fee_rev);
            }
            Ask::Post(p) => {
                match &p.goods {
                    PostGoods::Item { item_id } => w.u8(0).u32(*item_id),
                    PostGoods::Stack { code, count } => w.u8(1).str8(code).u32(*count),
                };
                w.u8(p.mode as u8);
                put_price(&mut w, p.price.as_ref());
                w.u8(p.days).str8(&p.note).bool(p.hidden).u32(p.fee_rev);
            }
            Ask::ShopEdit {
                op,
                listings,
                fee_rev,
            } => {
                let code = match op {
                    ShopOp::Cancel => 0,
                    ShopOp::Relist { .. } => 1,
                    ShopOp::Hide => 2,
                    ShopOp::Show => 3,
                    ShopOp::Note(_) => 4,
                    ShopOp::Reprice { .. } => 5,
                };
                w.u8(code);
                write_len(&mut w, listings.len());
                for l in listings {
                    w.u64(*l);
                }
                match op {
                    ShopOp::Cancel | ShopOp::Hide | ShopOp::Show => {}
                    ShopOp::Relist { days } => {
                        w.u8(*days);
                    }
                    ShopOp::Note(n) => {
                        w.str8(n);
                    }
                    ShopOp::Reprice { mode, price } => {
                        w.u8(*mode as u8);
                        put_price(&mut w, price.as_ref());
                    }
                }
                w.u32(*fee_rev);
            }
            Ask::ShopOpen {
                tab,
                page,
                sort,
                desc,
            } => {
                w.u8(*tab as u8).u16(*page).u8(*sort as u8).bool(*desc);
            }
            Ask::OffersOpen {
                which,
                live_only,
                page,
            } => {
                w.u8(*which as u8).bool(*live_only).u16(*page);
            }
            Ask::DeliveriesOpen { page } => {
                w.u16(*page);
            }
            Ask::Claim { all, deliveries } => {
                w.bool(*all);
                write_len(&mut w, deliveries.len());
                for d in deliveries {
                    w.u64(*d);
                }
            }
            Ask::Prices { of, days } => {
                match of {
                    PriceOf::Listing(l) => w.u8(0).u64(*l),
                    PriceOf::StashItem(i) => w.u8(1).u32(*i),
                    PriceOf::Stackable(c) => w.u8(2).str8(c),
                    PriceOf::Key(k) => w.u8(3).str8(k),
                };
                w.u16(*days);
            }
        }
        w.finish()
    }

    /// Reads a C→S market body (kind byte included). Wire bounds only: a
    /// note of up to 255 bytes, any page size or day count reads, and the
    /// realm answers with `BadRequest`.
    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut rest) = Reader::new(body)?;
        let r = &mut rest;
        let req = r.u64()?;
        let ask = match k {
            kind::HELLO => Ask::Hello,
            kind::SEARCH => Ask::Search(MarketQuery::decode(r)?),
            kind::LISTING_GET => Ask::ListingGet { listing: r.u64()? },
            kind::BUY => Ask::Buy {
                listing: r.u64()?,
                price_hash: r.array()?,
                fee_rev: r.u32()?,
            },
            kind::OFFER => Ask::Offer {
                listing: r.u64()?,
                price: Price::decode(r)?,
                fee_rev: r.u32()?,
            },
            kind::ANSWER => Ask::Answer {
                offer: r.u64()?,
                how: AnswerHow::read(r)?,
                counter: get_price(r)?,
                fee_rev: r.u32()?,
            },
            kind::POST => Ask::Post(Post {
                goods: match r.u8()? {
                    0 => PostGoods::Item { item_id: r.u32()? },
                    1 => PostGoods::Stack {
                        code: r.str8(MAX_CODE)?,
                        count: r.u32()?,
                    },
                    _ => return Err(NetError::Malformed),
                },
                mode: ListingMode::read(r)?,
                price: get_price(r)?,
                days: r.u8()?,
                note: r.str8(u8::MAX as usize)?,
                hidden: r.bool()?,
                fee_rev: r.u32()?,
            }),
            kind::SHOP_EDIT => {
                let code = r.u8()?;
                let listings = read_vec(r, u8::MAX as usize, |r| r.u64())?;
                let op = match code {
                    0 => ShopOp::Cancel,
                    1 => ShopOp::Relist { days: r.u8()? },
                    2 => ShopOp::Hide,
                    3 => ShopOp::Show,
                    4 => ShopOp::Note(r.str8(u8::MAX as usize)?),
                    5 => ShopOp::Reprice {
                        mode: ListingMode::read(r)?,
                        price: get_price(r)?,
                    },
                    _ => return Err(NetError::Malformed),
                };
                Ask::ShopEdit {
                    op,
                    listings,
                    fee_rev: r.u32()?,
                }
            }
            kind::SHOP_OPEN => Ask::ShopOpen {
                tab: ShopTab::read(r)?,
                page: r.u16()?,
                sort: ShopSort::read(r)?,
                desc: r.bool()?,
            },
            kind::OFFERS_OPEN => Ask::OffersOpen {
                which: OffersWhich::read(r)?,
                live_only: r.bool()?,
                page: r.u16()?,
            },
            kind::DELIVERIES_OPEN => Ask::DeliveriesOpen { page: r.u16()? },
            kind::CLAIM => Ask::Claim {
                all: r.bool()?,
                deliveries: read_vec(r, u8::MAX as usize, |r| r.u64())?,
            },
            kind::PRICES => Ask::Prices {
                of: match r.u8()? {
                    0 => PriceOf::Listing(r.u64()?),
                    1 => PriceOf::StashItem(r.u32()?),
                    2 => PriceOf::Stackable(r.str8(MAX_CODE)?),
                    3 => PriceOf::Key(r.str8(MAX_PRICE_KEY)?),
                    _ => return Err(NetError::Malformed),
                },
                days: r.u16()?,
            },
            _ => return Err(NetError::Malformed),
        };
        rest.finish()?;
        Ok(Self { req, ask })
    }
}

/// An S→C market message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FromRealm {
    /// 0xB1.
    State(MarketState),
    /// 0xB3.
    ResultsBegin { req: u64, head: ResultsHead },
    /// 0xB4.
    Facets { req: u64, facets: Facets },
    /// 0xB5.
    Row { req: u64, row: ListingRow },
    /// 0xB7.
    Listing { req: u64, detail: ListingDetail },
    /// 0xBE.
    ShopBegin { req: u64, head: ShopHead },
    /// 0xBF.
    ShopRow { req: u64, row: ShopRow },
    /// 0xC1.
    OffersBegin { req: u64, head: OffersHead },
    /// 0xC2.
    OfferRow { req: u64, row: OfferRow },
    /// 0xC4.
    DeliveriesBegin { req: u64, head: DeliveriesHead },
    /// 0xC5.
    DeliveryRow { req: u64, row: DeliveryRow },
    /// 0xC7.
    Item { req: u64, item: ItemRecord },
    /// 0xC9.
    PricesBegin { req: u64, head: PricesHead },
    /// 0xCA.
    PriceSeries { req: u64, series: PriceSeries },
    /// 0xCB: the last sales, newest first (at most [`MAX_RECENT`]).
    PriceRecent { req: u64, sales: Vec<Sale> },
    /// 0xCC.
    Done(Done),
    /// 0xCD.
    Notice(Notice),
    /// 0xCE: the end of a list.
    ListEnd { req: u64 },
}

impl FromRealm {
    pub fn kind(&self) -> u8 {
        match self {
            Self::State(_) => kind::STATE,
            Self::ResultsBegin { .. } => kind::RESULTS_BEGIN,
            Self::Facets { .. } => kind::FACETS,
            Self::Row { .. } => kind::ROW,
            Self::Listing { .. } => kind::LISTING,
            Self::ShopBegin { .. } => kind::SHOP_BEGIN,
            Self::ShopRow { .. } => kind::SHOP_ROW,
            Self::OffersBegin { .. } => kind::OFFERS_BEGIN,
            Self::OfferRow { .. } => kind::OFFER_ROW,
            Self::DeliveriesBegin { .. } => kind::DELIVERIES_BEGIN,
            Self::DeliveryRow { .. } => kind::DELIVERY_ROW,
            Self::Item { .. } => kind::ITEM,
            Self::PricesBegin { .. } => kind::PRICES_BEGIN,
            Self::PriceSeries { .. } => kind::PRICE_SERIES,
            Self::PriceRecent { .. } => kind::PRICE_RECENT,
            Self::Done(_) => kind::DONE,
            Self::Notice(_) => kind::NOTICE,
            Self::ListEnd { .. } => kind::LIST_END,
        }
    }

    /// The request this answers; `None` for a notice (pushed), and 0 for a
    /// pushed state.
    pub fn req(&self) -> Option<u64> {
        Some(match self {
            Self::State(s) => s.req,
            Self::Done(d) => d.req,
            Self::Notice(_) => return None,
            Self::ResultsBegin { req, .. }
            | Self::Facets { req, .. }
            | Self::Row { req, .. }
            | Self::Listing { req, .. }
            | Self::ShopBegin { req, .. }
            | Self::ShopRow { req, .. }
            | Self::OffersBegin { req, .. }
            | Self::OfferRow { req, .. }
            | Self::DeliveriesBegin { req, .. }
            | Self::DeliveryRow { req, .. }
            | Self::Item { req, .. }
            | Self::PricesBegin { req, .. }
            | Self::PriceSeries { req, .. }
            | Self::PriceRecent { req, .. }
            | Self::ListEnd { req } => *req,
        })
    }

    pub fn body(&self) -> Vec<u8> {
        let mut w = Writer::new(self.kind());
        let w = &mut w;
        match self {
            Self::State(s) => s.encode(w),
            Self::Done(d) => d.encode(w),
            Self::Notice(n) => n.encode(w),
            Self::ResultsBegin { req, head } => {
                w.u64(*req)
                    .u32(head.total)
                    .u16(head.page)
                    .u8(head.page_size)
                    .u8(head.rows)
                    .u64(head.now_ms)
                    .bool(head.facets);
            }
            Self::Facets { req, facets } => {
                w.u64(*req);
                facets.encode(w);
            }
            Self::Row { req, row } => {
                w.u64(*req);
                row.encode(w);
            }
            Self::Listing { req, detail } => {
                w.u64(*req).u8(detail.state as u8);
                detail.row.encode(w);
                w.u64(detail.buyer_fee).u32(detail.fee_rev);
                w.opt(detail.my_offer.as_ref(), |w, o| o.encode(w));
            }
            Self::ShopBegin { req, head } => {
                w.u64(*req).u8(head.tab as u8).u16(head.page).u8(head.rows);
                for c in head.counts {
                    w.u32(c);
                }
                w.u16(head.slots_used).u16(head.slots_max).u64(head.now_ms);
            }
            Self::ShopRow { req, row } => {
                w.u64(*req);
                row.encode(w);
            }
            Self::OffersBegin { req, head } => {
                w.u64(*req)
                    .u8(head.which as u8)
                    .u16(head.page)
                    .u8(head.rows)
                    .u32(head.total)
                    .u64(head.now_ms);
            }
            Self::OfferRow { req, row } => {
                w.u64(*req);
                row.encode(w);
            }
            Self::DeliveriesBegin { req, head } => {
                w.u64(*req)
                    .u16(head.page)
                    .u8(head.rows)
                    .u32(head.total)
                    .u64(head.now_ms);
            }
            Self::DeliveryRow { req, row } => {
                w.u64(*req);
                row.encode(w);
            }
            Self::Item { req, item } => {
                w.u64(*req).u32(item.item_id).bytes16(&item.record);
            }
            Self::PricesBegin { req, head } => {
                w.u64(*req)
                    .str8(&head.key)
                    .str8(&head.label)
                    .u64(head.now_ms)
                    .u64(head.bucket_ms)
                    .u32(head.listed_now)
                    .u32(head.sold)
                    .u8(head.series);
            }
            Self::PriceSeries { req, series } => {
                w.u64(*req);
                series.encode(w);
            }
            Self::PriceRecent { req, sales } => {
                w.u64(*req);
                write_len(w, sales.len());
                for s in sales {
                    w.u64(s.at_ms);
                    s.price.encode(w);
                }
            }
            Self::ListEnd { req } => {
                w.u64(*req);
            }
        }
        w.finish()
    }

    /// Reads an S→C market body (kind byte included).
    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut rest) = Reader::new(body)?;
        let r = &mut rest;
        let m = match k {
            kind::STATE => Self::State(MarketState::decode(r)?),
            kind::DONE => Self::Done(Done::decode(r)?),
            kind::NOTICE => Self::Notice(Notice::decode(r)?),
            kind::RESULTS_BEGIN => Self::ResultsBegin {
                req: r.u64()?,
                head: ResultsHead {
                    total: r.u32()?,
                    page: r.u16()?,
                    page_size: r.u8()?,
                    rows: r.u8()?,
                    now_ms: r.u64()?,
                    facets: r.bool()?,
                },
            },
            kind::FACETS => Self::Facets {
                req: r.u64()?,
                facets: Facets::decode(r)?,
            },
            kind::ROW => Self::Row {
                req: r.u64()?,
                row: ListingRow::decode(r)?,
            },
            kind::LISTING => Self::Listing {
                req: r.u64()?,
                detail: ListingDetail {
                    state: ListingState::read(r)?,
                    row: ListingRow::decode(r)?,
                    buyer_fee: r.u64()?,
                    fee_rev: r.u32()?,
                    my_offer: r.opt(ListingOffer::decode)?,
                },
            },
            kind::SHOP_BEGIN => Self::ShopBegin {
                req: r.u64()?,
                head: ShopHead {
                    tab: ShopTab::read(r)?,
                    page: r.u16()?,
                    rows: r.u8()?,
                    counts: [r.u32()?, r.u32()?, r.u32()?, r.u32()?],
                    slots_used: r.u16()?,
                    slots_max: r.u16()?,
                    now_ms: r.u64()?,
                },
            },
            kind::SHOP_ROW => Self::ShopRow {
                req: r.u64()?,
                row: ShopRow::decode(r)?,
            },
            kind::OFFERS_BEGIN => Self::OffersBegin {
                req: r.u64()?,
                head: OffersHead {
                    which: OffersWhich::read(r)?,
                    page: r.u16()?,
                    rows: r.u8()?,
                    total: r.u32()?,
                    now_ms: r.u64()?,
                },
            },
            kind::OFFER_ROW => Self::OfferRow {
                req: r.u64()?,
                row: OfferRow::decode(r)?,
            },
            kind::DELIVERIES_BEGIN => Self::DeliveriesBegin {
                req: r.u64()?,
                head: DeliveriesHead {
                    page: r.u16()?,
                    rows: r.u8()?,
                    total: r.u32()?,
                    now_ms: r.u64()?,
                },
            },
            kind::DELIVERY_ROW => Self::DeliveryRow {
                req: r.u64()?,
                row: DeliveryRow::decode(r)?,
            },
            kind::ITEM => Self::Item {
                req: r.u64()?,
                item: ItemRecord {
                    item_id: r.u32()?,
                    record: r.bytes16(MAX_RECORD)?,
                },
            },
            kind::PRICES_BEGIN => Self::PricesBegin {
                req: r.u64()?,
                head: PricesHead {
                    key: r.str8(MAX_PRICE_KEY)?,
                    label: r.str8(MAX_ABOUT)?,
                    now_ms: r.u64()?,
                    bucket_ms: r.u64()?,
                    listed_now: r.u32()?,
                    sold: r.u32()?,
                    series: r.u8()?,
                },
            },
            kind::PRICE_SERIES => Self::PriceSeries {
                req: r.u64()?,
                series: PriceSeries::decode(r)?,
            },
            kind::PRICE_RECENT => Self::PriceRecent {
                req: r.u64()?,
                sales: read_vec(r, MAX_RECENT, |r| {
                    Ok(Sale {
                        at_ms: r.u64()?,
                        price: Price::decode(r)?,
                    })
                })?,
            },
            kind::LIST_END => Self::ListEnd { req: r.u64()? },
            _ => return Err(NetError::Malformed),
        };
        rest.finish()?;
        Ok(m)
    }
}

#[cfg(test)]
#[path = "market_tests.rs"]
mod tests;
