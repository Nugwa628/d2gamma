//! Posting, buying, cancelling and expiring listings.

use rusqlite::params;

use crate::db::{Act, Economy};
use crate::error::{Done, Error, Refusal};
use crate::key::{Owner, Req};
use crate::log::Transfer;
use crate::market::deliveries::Source;
use crate::market::notices::NoticeKind;
use crate::market::price::Price;
use crate::market::{fill_item, listing_of, Listing, ListingMode, ListingState, LISTING_COLUMNS};
use crate::registry::Place;
use crate::stash::{Goods, ItemFacts};

/// One day in milliseconds.
pub(crate) const DAY_MS: i64 = 24 * 3600 * 1000;
/// The longest note, in bytes.
pub const NOTE_MAX: usize = 60;
/// The window of the posts-per-minute limit.
const MINUTE_MS: i64 = 60 * 1000;

/// A new listing (`MarketPost`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Post {
    /// The seller's character name, shown with the listing.
    pub seller: String,
    /// An item or a count of a stackable from the seller's stash.
    pub goods: Goods,
    pub mode: ListingMode,
    /// Gold and stackables, no items; none for [`ListingMode::OffersOnly`].
    pub price: Option<Price>,
    /// How many days it stays up, 1 to the realm's `market_days`.
    pub days: u8,
}

/// What a post carries besides its goods and price (`MarketPost`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PostOptions {
    /// The public note, 0 to [`NOTE_MAX`] bytes of printable text.
    pub note: String,
    /// Post straight to My shop's Hidden tab.
    pub hidden: bool,
    /// The fees the player saw ([`crate::EconomyConfig::fee_rev`]); `None`
    /// for callers that show no fees.
    pub fee_rev: Option<u32>,
    /// The main server's reading of a posted item's record. It replaces
    /// the facts stored with the item, in the same transaction, before the
    /// listing is indexed: the realm's game data is the source of truth
    /// for facts. `None` keeps the stored ones.
    pub facts: Option<ItemFacts>,
}

/// Refuses a note that is too long or not printable text.
pub(crate) fn check_note(note: &str) -> Result<(), Refusal> {
    if note.len() > NOTE_MAX || note.chars().any(char::is_control) {
        return Err(Refusal::BadRequest);
    }
    Ok(())
}

/// A new listing's row, as [`Act::insert_listing`] writes it.
pub(crate) struct NewListing<'a> {
    pub account: i64,
    pub split: &'a str,
    pub seller: &'a str,
    pub goods: &'a Goods,
    pub mode: ListingMode,
    pub price: Option<&'a Price>,
    pub days: u8,
    pub note: &'a str,
    pub hidden: bool,
    pub relisted_from: Option<i64>,
    /// Where the goods come from, for the log; they have already left it.
    pub from: (&'a str, Option<i64>),
}

impl Economy {
    /// Stash → a new listing's escrow. The answer's `id` is the listing.
    pub fn post(&mut self, owner: &Owner, req: &Req, post: &Post, now: i64) -> Result<Done, Error> {
        self.post_with(owner, req, post, &PostOptions::default(), now)
    }

    /// [`Economy::post`] with a note, straight to hidden, under the fees
    /// the player saw.
    pub fn post_with(
        &mut self,
        owner: &Owner,
        req: &Req,
        post: &Post,
        opts: &PostOptions,
        now: i64,
    ) -> Result<Done, Error> {
        self.act(req, now, |a| {
            a.check_market()?;
            a.check_key(&owner.economy)?;
            if let Some(rev) = opts.fee_rev {
                a.check_fee_rev(rev)?;
            }
            let split = owner.economy.split();
            let account = owner.account.0;
            a.check_days(post.days)?;
            let price = a.checked_listing_price(post.mode, post.price.as_ref())?;
            if post.seller.is_empty() || post.seller.len() > 64 {
                return Err(Refusal::BadRequest.into());
            }
            check_note(&opts.note)?;
            a.check_post_limits(account)?;
            match &post.goods {
                Goods::Item(id) => {
                    a.check_in_stash(*id, account, &split)?;
                    a.registry.listable(a.conn, *id, a.now)?;
                    if let Some(f) = &opts.facts {
                        a.set_facts(*id, f)?;
                    }
                }
                Goods::Stack { code, count } => {
                    a.check_stackable(code, *count)?;
                    a.remove_stack(account, &split, code, *count, Refusal::ItemNotFound)?;
                }
                Goods::Gold(_) => return Err(Refusal::BadRequest.into()),
            }
            let listing = a.insert_listing(&NewListing {
                account,
                split: &split,
                seller: &post.seller,
                goods: &post.goods,
                mode: post.mode,
                price: price.as_ref(),
                days: post.days,
                note: &opts.note,
                hidden: opts.hidden,
                relisted_from: None,
                from: ("stash", Some(account)),
            })?;
            a.take_listing_fee(account, &split, listing)?;
            Ok(Some(listing))
        })
    }

    /// Buys a listing at its price. `price_hash` is the hash of the price
    /// the buyer was shown ([`Price::hash`]); a different price is refused
    /// with [`Refusal::PriceChanged`].
    pub fn buy(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        price_hash: &[u8; 32],
        now: i64,
    ) -> Result<Done, Error> {
        self.buy_as(owner, req, listing, "", price_hash, None, now)
    }

    /// [`Economy::buy`] by the character `buyer` (shown to the seller),
    /// under the fees the player saw (`MarketBuy`).
    #[allow(clippy::too_many_arguments)]
    pub fn buy_with(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        buyer: &str,
        price_hash: &[u8; 32],
        fee_rev: u32,
        now: i64,
    ) -> Result<Done, Error> {
        self.buy_as(owner, req, listing, buyer, price_hash, Some(fee_rev), now)
    }

    #[allow(clippy::too_many_arguments)]
    fn buy_as(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        buyer: &str,
        price_hash: &[u8; 32],
        fee_rev: Option<u32>,
        now: i64,
    ) -> Result<Done, Error> {
        self.act(req, now, |a| {
            a.check_market()?;
            a.check_key(&owner.economy)?;
            if let Some(rev) = fee_rev {
                a.check_fee_rev(rev)?;
            }
            if buyer.len() > 64 {
                return Err(Refusal::BadRequest.into());
            }
            let l = a.active_listing(listing)?;
            if l.economy != owner.economy.split() || l.hidden {
                // Another split's or a hidden listing: not there for them.
                return Err(if l.hidden {
                    Refusal::ListingGone
                } else {
                    Refusal::EconomyMismatch
                }
                .into());
            }
            if l.account == owner.account.0 {
                return Err(Refusal::BadRequest.into());
            }
            let price = match (&l.price, l.mode.has_price()) {
                (Some(p), true) => p.clone(),
                _ => return Err(Refusal::BadRequest.into()),
            };
            if &price.hash() != price_hash {
                return Err(Refusal::PriceChanged.into());
            }
            a.pay_from_stash(owner.account.0, &l.economy, &price)?;
            let fee = a.pay_seller(&l, &price, None, ("stash", Some(owner.account.0)))?;
            a.sell_listing(&l, owner.account.0, buyer, &price, fee, None)?;
            Ok(Some(listing))
        })
    }

    /// The seller takes a listing down: its goods go to their Deliveries.
    pub fn cancel(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        now: i64,
    ) -> Result<Done, Error> {
        self.act(req, now, |a| {
            a.check_key(&owner.economy)?;
            let l = a.active_listing(listing)?;
            if l.account != owner.account.0 || l.economy != owner.economy.split() {
                return Err(Refusal::ListingGone.into());
            }
            a.close_listing(&l, ListingState::Cancelled, None)?;
            a.return_listing_goods(&l, "cancelled")?;
            Ok(Some(listing))
        })
    }

    /// The main server's sweep: every listing past its expiry closes and
    /// its goods go to the seller's Deliveries. Returns how many.
    pub fn expire(&mut self, now: i64) -> Result<usize, Error> {
        self.sweep(now, |a| {
            let due: Vec<Listing> = {
                let mut st = a.conn.prepare(&format!(
                    "SELECT {LISTING_COLUMNS} FROM listings l \
                     WHERE l.state = 'active' AND l.expires <= ?1 ORDER BY l.id"
                ))?;
                let rows = st.query_map([a.now], listing_of)?;
                rows.collect::<Result<_, _>>()?
            };
            for l in &due {
                a.close_listing(l, ListingState::Expired, None)?;
                a.return_listing_goods(l, "expired")?;
                a.notify(l.account, NoticeKind::ListingExpired, l, None, "", None)?;
            }
            Ok(due.len())
        })
    }

    /// An account's listings, newest first: active ones and the last
    /// closed ones (sold, cancelled, expired), at most `limit`.
    pub fn my_listings(&self, owner: &Owner, limit: u32) -> Result<Vec<Listing>, Error> {
        self.check_key(&owner.economy)?;
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {LISTING_COLUMNS} FROM listings l WHERE l.account = ?1 AND l.economy = ?2 \
             ORDER BY (l.state = 'active') DESC, l.id DESC LIMIT ?3"
        ))?;
        let rows = st.query_map(
            params![owner.account.0, owner.economy.split(), limit],
            listing_of,
        )?;
        let mut out: Vec<Listing> = rows.collect::<Result<_, _>>()?;
        for l in &mut out {
            fill_item(conn, l)?;
        }
        Ok(out)
    }
}

impl Act<'_> {
    /// Active listings and posts in the last minute, against the realm's
    /// limits.
    fn check_post_limits(&self, account: i64) -> Result<(), Error> {
        let (active, recent): (i64, i64) = self.conn.query_row(
            "SELECT COUNT(*) FILTER (WHERE state = 'active'), \
                    COUNT(*) FILTER (WHERE posted > ?2) \
             FROM listings WHERE account = ?1",
            params![account, self.now - MINUTE_MS],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if recent >= i64::from(self.config.market_posts_per_min) {
            return Err(Refusal::RateLimited.into());
        }
        if active >= i64::from(self.config.market_max_listings) {
            return Err(Refusal::MarketLimit.into());
        }
        Ok(())
    }

    /// Active listings against `market_max_listings` (a relist: not a new
    /// post for the per-minute limit).
    pub fn check_listing_slots(&self, account: i64) -> Result<(), Error> {
        let active: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM listings WHERE account = ?1 AND state = 'active'",
            [account],
            |r| r.get(0),
        )?;
        if active >= i64::from(self.config.market_max_listings) {
            return Err(Refusal::MarketLimit.into());
        }
        Ok(())
    }

    pub fn check_days(&self, days: u8) -> Result<(), Error> {
        if days == 0 || days > self.config.market_days {
            return Err(Refusal::BadRequest.into());
        }
        Ok(())
    }

    /// A listing's price for `mode`: gold and stackables when it has one,
    /// none for offers only. Returns it normalized.
    pub fn checked_listing_price(
        &self,
        mode: ListingMode,
        price: Option<&Price>,
    ) -> Result<Option<Price>, Error> {
        match (mode.has_price(), price) {
            (true, Some(p)) => Ok(Some(p.checked(self.config, false)?)),
            (false, None) => Ok(None),
            _ => Err(Refusal::BadRequest.into()),
        }
    }

    /// The listing fee, from the seller's stash (per listing and relist).
    pub fn take_listing_fee(&self, account: i64, split: &str, listing: i64) -> Result<(), Error> {
        let fee = self.config.market_listing_fee;
        if fee > 0 {
            self.remove_gold(account, split, fee, Refusal::NotEnough)?;
            self.log(Transfer {
                kind: "listing_fee",
                economy: split,
                goods: &Goods::Gold(fee),
                from: ("stash", Some(account)),
                to: ("fees", None),
                character: None,
                game: None,
                lease: None,
                listing: Some(listing),
                offer: None,
            })?;
        }
        Ok(())
    }

    /// Writes a new listing, puts its goods (already out of their source)
    /// into its escrow, indexes it unless hidden and logs the move.
    /// Returns its ID.
    pub fn insert_listing(&self, n: &NewListing<'_>) -> Result<i64, Error> {
        let (item, code, count) = match n.goods {
            Goods::Item(id) => (Some(id.0), None, None),
            Goods::Stack { code, count } => (None, Some(code.as_str()), Some(*count)),
            Goods::Gold(_) => return Err(Refusal::BadRequest.into()),
        };
        self.exec(
            "INSERT INTO listings(account, economy, seller, item_id, code, count, mode, price, \
             price_gold, state, posted, expires, hidden, note, relisted_from) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'active', ?10, ?11, ?12, ?13, ?14)",
            params![
                n.account,
                n.split,
                n.seller,
                item,
                code,
                count,
                n.mode as u8,
                n.price.map(Price::encode),
                n.price.map(|p| p.gold as i64),
                self.now,
                self.now + i64::from(n.days) * DAY_MS,
                n.hidden,
                n.note,
                n.relisted_from,
            ],
        )?;
        let listing = self.conn.last_insert_rowid();
        match n.goods {
            Goods::Item(id) => {
                self.exec(
                    "UPDATE econ_items SET place = 'listing', place_id = ?2 WHERE item_id = ?1",
                    params![id.0, listing],
                )?;
                self.moved(*id, Place::Escrow, Some(n.account), None, "list")?;
            }
            Goods::Stack { code, count } => self.escrow_stack("listing", listing, code, *count)?,
            Goods::Gold(_) => unreachable!("refused above"),
        }
        self.index_listing(listing)?;
        self.log(Transfer {
            kind: if n.relisted_from.is_some() {
                "relist"
            } else {
                "list"
            },
            economy: n.split,
            goods: n.goods,
            from: n.from,
            to: ("listing", Some(listing)),
            character: None,
            game: None,
            lease: None,
            listing: Some(listing),
            offer: None,
        })?;
        Ok(listing)
    }

    /// Takes a price out of a buyer's stash; [`Refusal::NotEnough`] if it
    /// doesn't hold all of it (items: [`Refusal::ItemNotFound`]).
    pub fn pay_from_stash(&self, account: i64, split: &str, price: &Price) -> Result<(), Error> {
        if price.gold > 0 {
            self.remove_gold(account, split, price.gold, Refusal::NotEnough)?;
        }
        for (code, count) in &price.stacks {
            self.remove_stack(account, split, code, *count, Refusal::NotEnough)?;
        }
        for id in &price.items {
            self.check_in_stash(*id, account, split)?;
        }
        Ok(())
    }

    /// Puts a price paid for listing `l` into the seller's Deliveries, the
    /// fee taken from its gold. Items named by the price move there from
    /// wherever they are. Returns the fee.
    pub fn pay_seller(
        &self,
        l: &Listing,
        price: &Price,
        offer: Option<i64>,
        from: (&str, Option<i64>),
    ) -> Result<u64, Error> {
        let mut taken = 0;
        let src = Source {
            kind: "sold",
            listing: Some(l.id),
            offer,
            from,
        };
        if price.gold > 0 {
            let fee = price.gold * u64::from(self.config.market_fee_pct) / 100;
            taken = fee;
            if fee > 0 {
                self.log(Transfer {
                    kind: "sale_fee",
                    economy: &l.economy,
                    goods: &Goods::Gold(fee),
                    from,
                    to: ("fees", None),
                    character: None,
                    game: None,
                    lease: None,
                    listing: Some(l.id),
                    offer,
                })?;
            }
            if price.gold > fee {
                self.deliver(l.account, &l.economy, &Goods::Gold(price.gold - fee), &src)?;
            }
        }
        for (code, count) in &price.stacks {
            let goods = Goods::Stack {
                code: code.clone(),
                count: *count,
            };
            self.deliver(l.account, &l.economy, &goods, &src)?;
        }
        for id in &price.items {
            self.deliver(l.account, &l.economy, &Goods::Item(*id), &src)?;
        }
        Ok(taken)
    }

    /// Closes a sold listing: its goods go to the buyer's Deliveries, the
    /// sale (`paid`, the fee taken, the buyer's character) is kept with
    /// it and in the price history, and the seller is told.
    pub fn sell_listing(
        &self,
        l: &Listing,
        buyer: i64,
        buyer_name: &str,
        paid: &Price,
        fee: u64,
        offer: Option<i64>,
    ) -> Result<(), Error> {
        self.close_listing(l, ListingState::Sold, Some(buyer))?;
        self.exec(
            "UPDATE listings SET buyer_name = ?2, sold_price = ?3, sale_fee = ?4 WHERE id = ?1",
            params![l.id, buyer_name, paid.encode(), fee as i64],
        )?;
        self.record_sale(l, paid)?;
        self.notify(
            l.account,
            NoticeKind::Sold,
            l,
            offer,
            buyer_name,
            Some(paid),
        )?;
        for goods in self.drain_escrow("listing", l.id)? {
            self.deliver(
                buyer,
                &l.economy,
                &goods,
                &Source {
                    kind: "bought",
                    listing: Some(l.id),
                    offer,
                    from: ("listing", Some(l.id)),
                },
            )?;
        }
        Ok(())
    }

    /// Marks a listing closed and takes it out of the search index; open
    /// offers on it are returned (and their buyers told).
    pub fn close_listing(
        &self,
        l: &Listing,
        state: ListingState,
        buyer: Option<i64>,
    ) -> Result<(), Error> {
        let n = self.exec(
            "UPDATE listings SET state = ?2, buyer = ?3, closed = ?4 WHERE id = ?1 AND state = 'active'",
            params![l.id, state.as_str(), buyer, self.now],
        )?;
        if n != 1 {
            return Err(Refusal::ListingGone.into());
        }
        self.unindex_listing(l.id)?;
        self.return_open_offers(l.id)?;
        Ok(())
    }

    /// A closed listing's goods back to the seller's Deliveries.
    pub fn return_listing_goods(&self, l: &Listing, why: &str) -> Result<(), Error> {
        for goods in self.drain_escrow("listing", l.id)? {
            self.deliver(
                l.account,
                &l.economy,
                &goods,
                &Source {
                    kind: why,
                    listing: Some(l.id),
                    offer: None,
                    from: ("listing", Some(l.id)),
                },
            )?;
        }
        Ok(())
    }
}
