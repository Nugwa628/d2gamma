//! Notices (market contract 5): what a market call has to tell an
//! account at the moment it happens: a sale, an offer and its answers, a
//! listing that expired. Each changing call collects its notices; they
//! are kept only if the call commits, and the main server takes them
//! after the call ([`Economy::take_notices`]) to push them. They are not
//! stored: an account that was away sees the counts ([`Economy::counts`])
//! when it next opens the market.

use rusqlite::params;

use crate::db::{Act, Economy};
use crate::error::Error;
use crate::key::{AccountId, Owner};
use crate::market::price::Price;
use crate::market::Listing;

/// What happened (`MarketNotice.what`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoticeKind {
    /// To the seller: bought at its price, or an offer or counter was
    /// accepted.
    Sold = 1,
    /// To the seller: a new offer on their listing.
    OfferReceived = 2,
    /// To the buyer: the seller countered their offer.
    Countered = 3,
    /// To the buyer: the seller accepted their offer; the goods are in
    /// Deliveries.
    OfferAccepted = 4,
    /// To the buyer: declined; their goods are back in Deliveries.
    OfferDeclined = 5,
    /// To the buyer: the listing was sold to someone else, cancelled or
    /// expired.
    OfferReturned = 6,
    /// To the seller: the goods are back in Deliveries.
    ListingExpired = 7,
    /// To the seller: the buyer took their offer back.
    OfferWithdrawn = 8,
}

/// One notice for one account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub account: AccountId,
    /// The split of the economy it happened in: the main server pushes it
    /// to the account's sessions whose character is in that split.
    pub economy: String,
    pub what: NoticeKind,
    pub listing: i64,
    pub offer: Option<i64>,
    /// The name of the listing's goods ("Grim Crown", "2 Ber Rune").
    pub about: String,
    /// The other character: the buyer or the seller.
    pub who: String,
    /// What it sold for, was offered or countered.
    pub price: Option<Price>,
    /// The account's deliveries waiting when the notice was taken.
    pub deliveries_waiting: u32,
    pub at: i64,
}

/// What `MarketState` counts for an account in one economy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarketCounts {
    /// Active listings, shown and hidden: the shop's slots used.
    pub active_listings: u32,
    pub deliveries_waiting: u32,
    /// Live offers (open or countered) on the account's listings.
    pub live_offers_to_me: u32,
    /// Live offers the account made.
    pub my_live_offers: u32,
}

impl Act<'_> {
    /// Collects a notice for `account` about listing `l`.
    pub fn notify(
        &self,
        account: i64,
        what: NoticeKind,
        l: &Listing,
        offer: Option<i64>,
        who: &str,
        price: Option<&Price>,
    ) -> Result<(), Error> {
        let about = self.goods_about(l)?;
        self.notices.borrow_mut().push(Notice {
            account: AccountId(account),
            economy: l.economy.clone(),
            what,
            listing: l.id,
            offer,
            about,
            who: who.to_owned(),
            price: price.cloned(),
            deliveries_waiting: 0,
            at: self.now,
        });
        Ok(())
    }

    /// The name of a listing's goods, as notices and the Deliveries tab
    /// show it.
    pub fn goods_about(&self, l: &Listing) -> Result<String, Error> {
        crate::market::goods_name(self.conn, self.config, &l.goods)
    }
}

impl Economy {
    /// The notices of the calls that committed since the last take, oldest
    /// first, each with its account's deliveries waiting now.
    pub fn take_notices(&mut self) -> Result<Vec<Notice>, Error> {
        let mut out = self.drain_notices();
        for n in &mut out {
            n.deliveries_waiting = self.conn().query_row(
                "SELECT COUNT(*) FROM deliveries WHERE account = ?1 AND economy = ?2",
                params![n.account.0, n.economy],
                |r| r.get(0),
            )?;
        }
        Ok(out)
    }

    /// The counts `MarketState` shows: listings, deliveries waiting and
    /// live offers both ways.
    pub fn counts(&self, owner: &Owner) -> Result<MarketCounts, Error> {
        self.check_key(&owner.economy)?;
        Ok(self.conn().query_row(
            "SELECT \
             (SELECT COUNT(*) FROM listings WHERE account = ?1 AND economy = ?2 \
              AND state = 'active'), \
             (SELECT COUNT(*) FROM deliveries WHERE account = ?1 AND economy = ?2), \
             (SELECT COUNT(*) FROM offers o JOIN listings l ON l.id = o.listing \
              WHERE l.account = ?1 AND l.economy = ?2 AND o.state IN ('open', 'countered')), \
             (SELECT COUNT(*) FROM offers WHERE account = ?1 AND economy = ?2 \
              AND state IN ('open', 'countered'))",
            params![owner.account.0, owner.economy.split()],
            |r| {
                Ok(MarketCounts {
                    active_listings: r.get(0)?,
                    deliveries_waiting: r.get(1)?,
                    live_offers_to_me: r.get(2)?,
                    my_live_offers: r.get(3)?,
                })
            },
        )?)
    }
}
