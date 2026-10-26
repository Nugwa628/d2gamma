// D2RUST's own: offers, Deliveries and notices as the market window reads them.
//! The trading half of the market's seam (`docs/plans/server/market-contract.md`,
//! v1): offers and counters (`OfferRow`, `MarketOffersOpen` 0xC0,
//! `MarketAnswer` 0xBA), Deliveries and claims (`MarketDeliveryRow`,
//! `MarketClaim` 0xC6) and the pushed notices (`MarketNotice` 0xCD),
//! already decoded, with the client's reading of every item they name.
//!
//! The offer form also needs what the account's online shared stash holds
//! ([`Holdings`]): the realm source reads it from the online stash, the
//! fake from its own ([`super::source::MarketSource::holdings`]). It is not
//! a market message: the stash has its own (0xA0-0xAF).

use std::collections::BTreeMap;

use super::source::{Goods, Group, ItemView, ListingId, Price};

/// An offer's number on the realm.
pub type OfferId = u64;

/// A delivery's number on the realm.
pub type DeliveryId = u64;

/// Rows a page of offers or Deliveries holds (contract: 50 a page).
pub const PAGE_ROWS: usize = 50;

/// The most deliveries one claim names (contract: 0-50).
pub const CLAIM_MAX: usize = 50;

/// An offer's state (contract `OfferRow.state`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OfferState {
    #[default]
    Open,
    /// The seller answered with a counter; the buyer accepts or withdraws.
    Countered,
    Accepted,
    Declined,
    Withdrawn,
    /// Its listing closed: sold to someone else, cancelled or expired.
    Returned,
}

impl OfferState {
    pub const ALL: [OfferState; 6] = [
        OfferState::Open,
        OfferState::Countered,
        OfferState::Accepted,
        OfferState::Declined,
        OfferState::Withdrawn,
        OfferState::Returned,
    ];

    pub fn code(self) -> u8 {
        OfferState::ALL.iter().position(|s| *s == self).unwrap_or(0) as u8
    }

    pub fn of_code(c: u8) -> Option<OfferState> {
        OfferState::ALL.get(usize::from(c)).copied()
    }

    /// Still waiting on an answer: open or countered.
    pub fn live(self) -> bool {
        matches!(self, OfferState::Open | OfferState::Countered)
    }

    pub fn label(self) -> &'static str {
        match self {
            OfferState::Open => "open",
            OfferState::Countered => "countered",
            OfferState::Accepted => "accepted",
            OfferState::Declined => "declined",
            OfferState::Withdrawn => "withdrawn",
            OfferState::Returned => "returned",
        }
    }
}

/// What an answer does to an offer (contract `MarketAnswer.how`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnswerHow {
    /// The seller an open offer, or the buyer a counter.
    Accept,
    /// The seller.
    Decline,
    /// The seller, with the counter price.
    Counter,
    /// The buyer takes the offer back.
    Withdraw,
}

impl AnswerHow {
    pub fn code(self) -> u8 {
        match self {
            AnswerHow::Accept => 0,
            AnswerHow::Decline => 1,
            AnswerHow::Counter => 2,
            AnswerHow::Withdraw => 3,
        }
    }
}

/// Which offers a page lists (contract `MarketOffersOpen.which`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OffersWhich {
    /// Offers others made on the asker's listings.
    OnMyListings,
    /// Offers the asker made.
    MadeByMe,
}

impl OffersWhich {
    pub const ALL: [OffersWhich; 2] = [OffersWhich::OnMyListings, OffersWhich::MadeByMe];

    pub fn code(self) -> u8 {
        match self {
            OffersWhich::OnMyListings => 0,
            OffersWhich::MadeByMe => 1,
        }
    }

    /// The word of its UI ids (`market.offers.mine(0)`).
    pub fn key(self) -> &'static str {
        match self {
            OffersWhich::OnMyListings => "mine",
            OffersWhich::MadeByMe => "made",
        }
    }
}

/// An item a row names by ID (contract `MarketItem`, sent before the row),
/// with the client's reading of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedItem {
    pub item_id: u32,
    /// The save-format record (the game's own tip is built from it).
    pub record: Vec<u8>,
    pub group: Group,
    pub item: ItemView,
}

/// One offer (contract `OfferRow`) with the client's reading of the
/// listing's goods and of the offered items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfferRow {
    pub offer: OfferId,
    pub listing: ListingId,
    /// The listing's goods, its group and the client's reading of them.
    pub goods: Goods,
    pub group: Group,
    pub item: ItemView,
    /// The listing's price; `None` for an offers-only listing.
    pub price: Option<Price>,
    pub buyer: String,
    pub seller: String,
    /// What the buyer offers: gold, currencies and stash items (by ID).
    pub offered: Price,
    /// The offered items, as the item messages before the row named them.
    pub items: Vec<NamedItem>,
    /// The seller's counter: gold, currencies and the offered items kept.
    pub counter: Option<Price>,
    pub state: OfferState,
    pub at_ms: u64,
    /// 0: not answered yet.
    pub answered_ms: u64,
}

impl OfferRow {
    /// The offered item `id`, when the row named it.
    pub fn named(&self, id: u32) -> Option<&NamedItem> {
        self.items.iter().find(|i| i.item_id == id)
    }
}

/// A page of offers (contract `MarketOffersBegin`, its items and rows).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OffersPage {
    pub which: OffersWhich,
    pub page: u16,
    pub total: u32,
    pub now_ms: u64,
    pub rows: Vec<OfferRow>,
}

/// Where a delivery came from (contract `MarketDeliveryRow.source`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeliverySource {
    /// What the asker bought.
    Bought,
    /// A sale's proceeds.
    Sold,
    /// A cancelled listing's goods.
    Cancelled,
    /// An expired listing's goods.
    Expired,
    /// An offer's goods back (declined, withdrawn, its listing closed).
    Returned,
    /// What a counter didn't take of an offer.
    Change,
    Other(u8),
}

impl DeliverySource {
    pub fn code(self) -> u8 {
        match self {
            DeliverySource::Bought => 1,
            DeliverySource::Sold => 2,
            DeliverySource::Cancelled => 3,
            DeliverySource::Expired => 4,
            DeliverySource::Returned => 5,
            DeliverySource::Change => 6,
            DeliverySource::Other(c) => c,
        }
    }

    pub fn of_code(c: u8) -> DeliverySource {
        match c {
            1 => DeliverySource::Bought,
            2 => DeliverySource::Sold,
            3 => DeliverySource::Cancelled,
            4 => DeliverySource::Expired,
            5 => DeliverySource::Returned,
            6 => DeliverySource::Change,
            c => DeliverySource::Other(c),
        }
    }
}

/// Why a delivery's goods came back (contract `MarketDeliveryRow.reason`,
/// v2): the Refund line says it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeliveryReason {
    /// Not goods coming back (a sale, a buy).
    #[default]
    None,
    /// The seller declined the offer.
    Declined,
    /// The buyer took the offer back.
    Withdrawn,
    /// The listing sold to someone else.
    Outbid,
    Expired,
    Cancelled,
    /// What a counter didn't take.
    Change,
    Other(u8),
}

impl DeliveryReason {
    pub fn code(self) -> u8 {
        match self {
            DeliveryReason::None => 0,
            DeliveryReason::Declined => 1,
            DeliveryReason::Withdrawn => 2,
            DeliveryReason::Outbid => 3,
            DeliveryReason::Expired => 4,
            DeliveryReason::Cancelled => 5,
            DeliveryReason::Change => 6,
            DeliveryReason::Other(c) => c,
        }
    }

    pub fn of_code(c: u8) -> DeliveryReason {
        match c {
            0 => DeliveryReason::None,
            1 => DeliveryReason::Declined,
            2 => DeliveryReason::Withdrawn,
            3 => DeliveryReason::Outbid,
            4 => DeliveryReason::Expired,
            5 => DeliveryReason::Cancelled,
            6 => DeliveryReason::Change,
            c => DeliveryReason::Other(c),
        }
    }

    /// The Refund line's word for it; `None` when there is nothing to say.
    pub fn label(self) -> Option<&'static str> {
        Some(match self {
            DeliveryReason::Declined => "declined",
            DeliveryReason::Withdrawn => "withdrawn",
            DeliveryReason::Outbid => "sold to another",
            DeliveryReason::Expired => "listing expired",
            DeliveryReason::Cancelled => "listing cancelled",
            DeliveryReason::None | DeliveryReason::Change | DeliveryReason::Other(_) => {
                return None
            }
        })
    }
}

/// One delivery (contract `MarketDeliveryRow`): one thing the market gave
/// the account, waiting to be claimed into the shared stash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryRow {
    pub delivery: DeliveryId,
    pub source: DeliverySource,
    /// Why its goods came back.
    pub reason: DeliveryReason,
    pub listing: Option<ListingId>,
    pub offer: Option<OfferId>,
    /// The name of the listing's goods ("Grim Crown").
    pub about: String,
    /// An item, a stackable's count or gold.
    pub goods: Goods,
    /// For an item: its group and the client's reading of its record.
    pub item: Option<(Group, ItemView)>,
    pub at_ms: u64,
}

/// A page of Deliveries (contract `MarketDeliveriesBegin` and its rows).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeliveriesPage {
    pub page: u16,
    pub total: u32,
    pub now_ms: u64,
    pub rows: Vec<DeliveryRow>,
}

/// What a notice tells (contract `MarketNotice.what`, section 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeWhat {
    /// To the seller: bought at its price, or an offer or counter accepted.
    Sold,
    /// To the seller: a new offer on their listing.
    OfferReceived,
    /// To the buyer: the seller countered their offer.
    Countered,
    /// To the buyer: their offer was accepted; the goods are in Deliveries.
    OfferAccepted,
    /// To the buyer: declined; their goods are back in Deliveries.
    OfferDeclined,
    /// To the buyer: the listing sold to someone else, was cancelled or
    /// expired.
    OfferReturned,
    /// To the seller: the goods are back in Deliveries.
    ListingExpired,
    /// To the seller: the buyer took their offer back.
    OfferWithdrawn,
    Other(u8),
}

impl NoticeWhat {
    pub fn code(self) -> u8 {
        match self {
            NoticeWhat::Sold => 1,
            NoticeWhat::OfferReceived => 2,
            NoticeWhat::Countered => 3,
            NoticeWhat::OfferAccepted => 4,
            NoticeWhat::OfferDeclined => 5,
            NoticeWhat::OfferReturned => 6,
            NoticeWhat::ListingExpired => 7,
            NoticeWhat::OfferWithdrawn => 8,
            NoticeWhat::Other(c) => c,
        }
    }

    pub fn of_code(c: u8) -> NoticeWhat {
        match c {
            1 => NoticeWhat::Sold,
            2 => NoticeWhat::OfferReceived,
            3 => NoticeWhat::Countered,
            4 => NoticeWhat::OfferAccepted,
            5 => NoticeWhat::OfferDeclined,
            6 => NoticeWhat::OfferReturned,
            7 => NoticeWhat::ListingExpired,
            8 => NoticeWhat::OfferWithdrawn,
            c => NoticeWhat::Other(c),
        }
    }
}

/// Something happened to the account's listing or offer (contract
/// `MarketNotice`), pushed the moment it happens; not stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub what: NoticeWhat,
    pub listing: ListingId,
    pub offer: Option<OfferId>,
    /// The goods' name.
    pub about: String,
    /// The other character.
    pub who: String,
    /// Sold for, offered, countered.
    pub price: Option<Price>,
    /// Deliveries waiting after it.
    pub deliveries_waiting: u16,
    pub at_ms: u64,
}

/// What the account's online shared stash holds: what an offer can be
/// made of.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Holdings {
    pub gold: u64,
    /// Stackable counts by code.
    pub stacks: BTreeMap<String, u32>,
    pub items: Vec<NamedItem>,
}

impl Holdings {
    /// Whether the stash holds `p`: its gold, every count and every item.
    pub fn holds(&self, p: &Price) -> bool {
        self.gold >= p.gold
            && p.stacks
                .iter()
                .all(|(c, n)| self.stacks.get(c).copied().unwrap_or(0) >= *n)
            && p.items
                .iter()
                .all(|id| self.items.iter().any(|i| i.item_id == *id))
    }

    pub fn item(&self, id: u32) -> Option<&NamedItem> {
        self.items.iter().find(|i| i.item_id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_codes_follow_the_contract() {
        let states: Vec<u8> = OfferState::ALL.iter().map(|s| s.code()).collect();
        assert_eq!(states, [0, 1, 2, 3, 4, 5]);
        for s in OfferState::ALL {
            assert_eq!(OfferState::of_code(s.code()), Some(s));
        }
        assert_eq!(OfferState::of_code(6), None);
        let hows: Vec<u8> = [
            AnswerHow::Accept,
            AnswerHow::Decline,
            AnswerHow::Counter,
            AnswerHow::Withdraw,
        ]
        .iter()
        .map(|h| h.code())
        .collect();
        assert_eq!(hows, [0, 1, 2, 3]);
        for c in [1, 2, 3, 4, 5, 6, 255] {
            assert_eq!(DeliverySource::of_code(c).code(), c);
        }
        assert_eq!(DeliverySource::of_code(255), DeliverySource::Other(255));
        for c in [0, 1, 2, 3, 4, 5, 6, 9] {
            assert_eq!(DeliveryReason::of_code(c).code(), c);
        }
        assert_eq!(DeliveryReason::Outbid.label(), Some("sold to another"));
        assert_eq!(DeliveryReason::None.label(), None);
        for c in 1..=9 {
            assert_eq!(NoticeWhat::of_code(c).code(), c);
        }
        assert_eq!(OffersWhich::MadeByMe.code(), 1);
    }

    #[test]
    fn holdings_hold_a_price() {
        let h = Holdings {
            gold: 100,
            stacks: [("r24".to_owned(), 2)].into_iter().collect(),
            items: Vec::new(),
        };
        let p = |gold, n, items: Vec<u32>| Price {
            gold,
            stacks: vec![("r24".into(), n)],
            items,
        };
        assert!(h.holds(&p(100, 2, vec![])));
        assert!(!h.holds(&p(101, 2, vec![])));
        assert!(!h.holds(&p(0, 3, vec![])));
        assert!(!h.holds(&p(0, 0, vec![7])));
    }
}
