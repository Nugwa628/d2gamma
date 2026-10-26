//! Offers and barter (plan E2, module N31): a buyer offers a bundle of
//! gold, stackables and specific stash items on a listing that takes
//! offers; the bundle waits in the offer's escrow while the offer is open
//! (open question 27).
//!
//! - The seller **accepts** an open offer, **declines** it, or
//!   **counters** with a new bundle of gold and stackables, keeping any of
//!   the offered items. A counter is final: the buyer accepts it or
//!   withdraws.
//! - **Accepting** is one transaction: the bundle (or the counter, the
//!   difference taken from the buyer's stash or returned to their
//!   Deliveries) goes to the seller's Deliveries, the listing's goods to
//!   the buyer's, the listing closes and every other open offer on it is
//!   returned.
//! - A declined or withdrawn offer, and every open offer on a listing that
//!   is sold, cancelled or expires, goes back to the buyer's Deliveries.

use rusqlite::{params, OptionalExtension, Row};

use crate::db::{Act, Economy};
use crate::error::{Done, Error, Refusal};
use crate::key::{Owner, Req};
use crate::log::Transfer;
use crate::market::deliveries::Source;
use crate::market::notices::NoticeKind;
use crate::market::price::Price;
use crate::market::Listing;
use crate::registry::Place;
use crate::stash::Goods;

/// Migration 4: offers. Their goods are in `escrow`, `escrow_stacks` and
/// `econ_items` under the holder `offer`.
pub(crate) const SCHEMA: &str = "
CREATE TABLE offers(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    listing INTEGER NOT NULL REFERENCES listings(id),
    account INTEGER NOT NULL,
    economy TEXT NOT NULL,
    buyer TEXT NOT NULL,
    price BLOB NOT NULL,
    counter BLOB,
    state TEXT NOT NULL,
    at INTEGER NOT NULL,
    answered INTEGER
);
CREATE INDEX offers_listing ON offers(listing, state);
CREATE INDEX offers_owner ON offers(account, economy, state);
";

/// Where an offer is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferState {
    Open,
    /// The seller answered with a counter; the buyer accepts or withdraws.
    Countered,
    Accepted,
    Declined,
    Withdrawn,
    /// Its listing was sold to someone else, cancelled or expired.
    Returned,
}

impl OfferState {
    pub fn as_str(self) -> &'static str {
        match self {
            OfferState::Open => "open",
            OfferState::Countered => "countered",
            OfferState::Accepted => "accepted",
            OfferState::Declined => "declined",
            OfferState::Withdrawn => "withdrawn",
            OfferState::Returned => "returned",
        }
    }

    fn parse(s: &str) -> OfferState {
        match s {
            "open" => OfferState::Open,
            "countered" => OfferState::Countered,
            "accepted" => OfferState::Accepted,
            "declined" => OfferState::Declined,
            "withdrawn" => OfferState::Withdrawn,
            _ => OfferState::Returned,
        }
    }

    /// Open or countered: its goods are in escrow.
    pub fn is_live(self) -> bool {
        matches!(self, OfferState::Open | OfferState::Countered)
    }
}

/// An offer, as the seller's and the buyer's lists show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub id: i64,
    pub listing: i64,
    /// The buyer's account.
    pub account: i64,
    pub economy: String,
    /// The buyer's character name, as shown.
    pub buyer: String,
    /// What the buyer offered (held in escrow while live).
    pub price: Price,
    /// The seller's counter, if any.
    pub counter: Option<Price>,
    pub state: OfferState,
    pub at: i64,
    /// When the seller answered or it ended.
    pub answered: Option<i64>,
}

/// An answer to an offer (`MarketAnswer.how`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OfferAnswer {
    /// The seller accepts an open offer, or the buyer a counter.
    Accept,
    /// The seller declines.
    Decline,
    /// The seller counters: gold and stackables, and any of the offered
    /// items to keep.
    Counter(Price),
    /// The buyer takes the offer back.
    Withdraw,
}

pub(crate) const OFFER_COLUMNS: &str =
    "o.id, o.listing, o.account, o.economy, o.buyer, o.price, o.counter, o.state, o.at, \
     o.answered";

pub(crate) fn offer_of(r: &Row) -> rusqlite::Result<Offer> {
    let price: Vec<u8> = r.get(5)?;
    let counter: Option<Vec<u8>> = r.get(6)?;
    let state: String = r.get(7)?;
    Ok(Offer {
        id: r.get(0)?,
        listing: r.get(1)?,
        account: r.get(2)?,
        economy: r.get(3)?,
        buyer: r.get(4)?,
        price: Price::decode(&price).unwrap_or_default(),
        counter: counter.as_deref().and_then(Price::decode),
        state: OfferState::parse(&state),
        at: r.get(8)?,
        answered: r.get(9)?,
    })
}

impl Economy {
    /// The buyer's stash → a new offer's escrow on `listing`. The answer's
    /// `id` is the offer.
    pub fn offer(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        buyer: &str,
        price: &Price,
        now: i64,
    ) -> Result<Done, Error> {
        self.offer_as(owner, req, listing, buyer, price, None, now)
    }

    /// [`Economy::offer`] under the fees the player saw (`MarketOffer`).
    #[allow(clippy::too_many_arguments)]
    pub fn offer_with(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        buyer: &str,
        price: &Price,
        fee_rev: u32,
        now: i64,
    ) -> Result<Done, Error> {
        self.offer_as(owner, req, listing, buyer, price, Some(fee_rev), now)
    }

    #[allow(clippy::too_many_arguments)]
    fn offer_as(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        buyer: &str,
        price: &Price,
        fee_rev: Option<u32>,
        now: i64,
    ) -> Result<Done, Error> {
        self.act(req, now, |a| {
            a.check_market()?;
            a.check_key(&owner.economy)?;
            if let Some(rev) = fee_rev {
                a.check_fee_rev(rev)?;
            }
            let split = owner.economy.split();
            let account = owner.account.0;
            let l = a.active_listing(listing)?;
            if l.hidden {
                return Err(Refusal::ListingGone.into());
            }
            if l.economy != split {
                return Err(Refusal::EconomyMismatch.into());
            }
            if l.account == account
                || !l.mode.takes_offers()
                || buyer.is_empty()
                || buyer.len() > 64
            {
                return Err(Refusal::BadRequest.into());
            }
            let price = price.checked(a.config, true)?;
            if price.items.len() > a.config.market_offer_items as usize {
                return Err(Refusal::MarketLimit.into());
            }
            a.check_offer_limits(account, listing)?;
            a.exec(
                "INSERT INTO offers(listing, account, economy, buyer, price, state, at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 'open', ?6)",
                params![listing, account, split, buyer, price.encode(), a.now],
            )?;
            let offer = a.conn.last_insert_rowid();
            a.escrow_from_stash(account, &split, (listing, offer), &price)?;
            a.notify(
                l.account,
                NoticeKind::OfferReceived,
                &l,
                Some(offer),
                buyer,
                Some(&price),
            )?;
            Ok(Some(offer))
        })
    }

    /// Answers an offer: the seller accepts, declines or counters, the
    /// buyer accepts a counter or withdraws. Anything else, or an offer no
    /// longer live, is refused with [`Refusal::OfferGone`]. The answer's
    /// `id` is the offer.
    pub fn answer(
        &mut self,
        owner: &Owner,
        req: &Req,
        offer: i64,
        how: &OfferAnswer,
        now: i64,
    ) -> Result<Done, Error> {
        self.answer_as(owner, req, offer, how, None, now)
    }

    /// [`Economy::answer`] under the fees the player saw (`MarketAnswer`).
    pub fn answer_with(
        &mut self,
        owner: &Owner,
        req: &Req,
        offer: i64,
        how: &OfferAnswer,
        fee_rev: u32,
        now: i64,
    ) -> Result<Done, Error> {
        self.answer_as(owner, req, offer, how, Some(fee_rev), now)
    }

    fn answer_as(
        &mut self,
        owner: &Owner,
        req: &Req,
        offer: i64,
        how: &OfferAnswer,
        fee_rev: Option<u32>,
        now: i64,
    ) -> Result<Done, Error> {
        self.act(req, now, |a| {
            a.check_market()?;
            a.check_key(&owner.economy)?;
            if let Some(rev) = fee_rev {
                a.check_fee_rev(rev)?;
            }
            let o = a.live_offer(offer)?;
            if o.economy != owner.economy.split() {
                return Err(Refusal::OfferGone.into());
            }
            let seller_account: i64 = a.conn.query_row(
                "SELECT account FROM listings WHERE id = ?1",
                [o.listing],
                |r| r.get(0),
            )?;
            let seller = seller_account == owner.account.0;
            let buyer = o.account == owner.account.0;
            match (how, o.state) {
                (OfferAnswer::Accept, OfferState::Open) if seller => {
                    let l = a.active_listing(o.listing)?;
                    a.accept(&l, &o, OfferState::Open, &o.price)?;
                    let kind = NoticeKind::OfferAccepted;
                    a.notify(o.account, kind, &l, Some(o.id), &l.seller, Some(&o.price))?;
                }
                (OfferAnswer::Accept, OfferState::Countered) if buyer => {
                    let l = a.active_listing(o.listing)?;
                    if l.hidden {
                        return Err(Refusal::ListingGone.into());
                    }
                    let counter = o.counter.clone().ok_or(Refusal::OfferGone)?;
                    a.accept(&l, &o, OfferState::Countered, &counter)?;
                }
                (OfferAnswer::Decline, _) if seller => {
                    a.end_offer(&o, OfferState::Declined)?;
                    let l = a.any_listing(o.listing)?;
                    a.notify(
                        o.account,
                        NoticeKind::OfferDeclined,
                        &l,
                        Some(o.id),
                        &l.seller,
                        None,
                    )?;
                }
                (OfferAnswer::Withdraw, _) if buyer => {
                    a.end_offer(&o, OfferState::Withdrawn)?;
                    let l = a.any_listing(o.listing)?;
                    let kind = NoticeKind::OfferWithdrawn;
                    a.notify(l.account, kind, &l, Some(o.id), &o.buyer, None)?;
                }
                (OfferAnswer::Counter(p), OfferState::Open) if seller => {
                    let l = a.active_listing(o.listing)?;
                    let p = p.checked(a.config, true)?;
                    if p.items.iter().any(|i| !o.price.items.contains(i)) {
                        return Err(Refusal::BadRequest.into());
                    }
                    a.set_offer_state(&o, OfferState::Open, OfferState::Countered)?;
                    a.exec(
                        "UPDATE offers SET counter = ?2 WHERE id = ?1",
                        params![o.id, p.encode()],
                    )?;
                    a.notify(
                        o.account,
                        NoticeKind::Countered,
                        &l,
                        Some(o.id),
                        &l.seller,
                        Some(&p),
                    )?;
                }
                _ => return Err(Refusal::OfferGone.into()),
            }
            Ok(Some(o.id))
        })
    }

    /// An offer by its ID, in any state.
    pub fn find_offer(&self, id: i64) -> Result<Option<Offer>, Error> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {OFFER_COLUMNS} FROM offers o WHERE o.id = ?1"),
                [id],
                offer_of,
            )
            .optional()?)
    }

    /// The offers an account made, live ones first, newest first, at most
    /// `limit`.
    pub fn my_offers(&self, owner: &Owner, limit: u32) -> Result<Vec<Offer>, Error> {
        self.check_key(&owner.economy)?;
        let mut st = self.conn().prepare(&format!(
            "SELECT {OFFER_COLUMNS} FROM offers o WHERE o.account = ?1 AND o.economy = ?2 \
             ORDER BY (o.state IN ('open', 'countered')) DESC, o.id DESC LIMIT ?3"
        ))?;
        let rows = st.query_map(
            params![owner.account.0, owner.economy.split(), limit],
            offer_of,
        )?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The live offers on an account's listings, newest first.
    pub fn offers_to_me(&self, owner: &Owner) -> Result<Vec<Offer>, Error> {
        self.check_key(&owner.economy)?;
        let mut st = self.conn().prepare(&format!(
            "SELECT {OFFER_COLUMNS} FROM offers o JOIN listings l ON l.id = o.listing \
             WHERE l.account = ?1 AND l.economy = ?2 AND o.state IN ('open', 'countered') \
             ORDER BY o.id DESC"
        ))?;
        let rows = st.query_map(params![owner.account.0, owner.economy.split()], offer_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

impl Act<'_> {
    /// Open offers per account, and one live offer per account and
    /// listing.
    fn check_offer_limits(&self, account: i64, listing: i64) -> Result<(), Error> {
        let (live, here): (i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COUNT(*) FILTER (WHERE listing = ?2) FROM offers \
             WHERE account = ?1 AND state IN ('open', 'countered')",
            params![account, listing],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if live >= i64::from(self.config.market_max_offers) || here > 0 {
            return Err(Refusal::MarketLimit.into());
        }
        Ok(())
    }

    /// Moves a bundle from an account's stash into an offer's escrow.
    fn escrow_from_stash(
        &self,
        account: i64,
        split: &str,
        (listing, offer): (i64, i64),
        p: &Price,
    ) -> Result<(), Error> {
        let log = |goods: &Goods| {
            self.log(Transfer {
                kind: "offer",
                economy: split,
                goods,
                from: ("stash", Some(account)),
                to: ("offer", Some(offer)),
                character: None,
                game: None,
                lease: None,
                listing: Some(listing),
                offer: Some(offer),
            })
        };
        if p.gold > 0 {
            self.remove_gold(account, split, p.gold, Refusal::NotEnough)?;
            self.escrow_gold("offer", offer, p.gold)?;
            log(&Goods::Gold(p.gold))?;
        }
        for (code, count) in &p.stacks {
            self.remove_stack(account, split, code, *count, Refusal::NotEnough)?;
            self.escrow_stack("offer", offer, code, *count)?;
            log(&Goods::Stack {
                code: code.clone(),
                count: *count,
            })?;
        }
        for id in &p.items {
            self.check_in_stash(*id, account, split)?;
            self.registry.listable(self.conn, *id, self.now)?;
            self.exec(
                "UPDATE econ_items SET place = 'offer', place_id = ?2 WHERE item_id = ?1",
                params![id.0, offer],
            )?;
            self.moved(*id, Place::Escrow, Some(account), None, "offer")?;
            log(&Goods::Item(*id))?;
        }
        Ok(())
    }

    /// Adds gold to a holder's escrow (an offer).
    pub fn escrow_gold(&self, holder: &str, id: i64, gold: u64) -> Result<(), Error> {
        self.exec(
            "INSERT INTO escrow(holder, holder_id, gold) VALUES (?1, ?2, ?3) \
             ON CONFLICT(holder, holder_id) DO UPDATE SET gold = gold + excluded.gold",
            params![holder, id, gold as i64],
        )?;
        Ok(())
    }

    /// A live offer, for answering.
    fn live_offer(&self, id: i64) -> Result<Offer, Error> {
        let o = self
            .conn
            .query_row(
                &format!("SELECT {OFFER_COLUMNS} FROM offers o WHERE o.id = ?1"),
                [id],
                offer_of,
            )
            .optional()?
            .ok_or(Refusal::OfferGone)?;
        if !o.state.is_live() {
            return Err(Refusal::OfferGone.into());
        }
        Ok(o)
    }

    /// Moves an offer from `from` to `to`; refused if it moved meanwhile.
    fn set_offer_state(&self, o: &Offer, from: OfferState, to: OfferState) -> Result<(), Error> {
        let n = self.exec(
            "UPDATE offers SET state = ?3, answered = ?4 WHERE id = ?1 AND state = ?2",
            params![o.id, from.as_str(), to.as_str(), self.now],
        )?;
        if n != 1 {
            return Err(Refusal::OfferGone.into());
        }
        Ok(())
    }

    /// Ends a live offer without a sale: its goods back to the buyer's
    /// Deliveries.
    fn end_offer(&self, o: &Offer, to: OfferState) -> Result<(), Error> {
        self.set_offer_state(o, o.state, to)?;
        self.return_offer_goods(o, to.as_str())
    }

    fn return_offer_goods(&self, o: &Offer, why: &str) -> Result<(), Error> {
        for goods in self.drain_escrow("offer", o.id)? {
            self.deliver(
                o.account,
                &o.economy,
                &goods,
                &Source {
                    kind: why,
                    listing: Some(o.listing),
                    offer: Some(o.id),
                    from: ("offer", Some(o.id)),
                },
            )?;
        }
        Ok(())
    }

    /// Returns the goods of every live offer on a closed listing.
    pub fn return_open_offers(&self, listing: i64) -> Result<(), Error> {
        let live: Vec<Offer> = {
            let mut st = self.conn.prepare(&format!(
                "SELECT {OFFER_COLUMNS} FROM offers o \
                 WHERE o.listing = ?1 AND o.state IN ('open', 'countered') ORDER BY o.id"
            ))?;
            let rows = st.query_map([listing], offer_of)?;
            rows.collect::<Result<_, _>>()?
        };
        let l = if live.is_empty() {
            None
        } else {
            Some(self.any_listing(listing)?)
        };
        for o in &live {
            self.set_offer_state(o, o.state, OfferState::Returned)?;
            self.return_offer_goods(o, "returned")?;
            if let Some(l) = &l {
                let kind = NoticeKind::OfferReturned;
                self.notify(o.account, kind, l, Some(o.id), &l.seller, None)?;
            }
        }
        Ok(())
    }

    /// Sells listing `l` to offer `o` for `pay` (the offer itself, or the
    /// seller's counter): the escrow pays what it holds, the buyer's stash
    /// the rest; what the escrow holds beyond `pay` goes back to the
    /// buyer.
    fn accept(&self, l: &Listing, o: &Offer, from: OfferState, pay: &Price) -> Result<(), Error> {
        self.set_offer_state(o, from, OfferState::Accepted)?;
        let mut held = Price::default();
        let mut extra = Vec::new();
        for goods in self.drain_escrow("offer", o.id)? {
            match goods {
                Goods::Gold(g) => held.gold = g,
                Goods::Stack { code, count } => held.stacks.push((code, count)),
                Goods::Item(id) if pay.items.contains(&id) => held.items.push(id),
                item @ Goods::Item(_) => extra.push(item),
            }
        }
        // Gold and counts: the difference either way.
        let mut short = Price::default();
        let held_count = |code: &str| {
            held.stacks
                .iter()
                .find(|(c, _)| c == code)
                .map_or(0, |(_, n)| *n)
        };
        if pay.gold > held.gold {
            short.gold = pay.gold - held.gold;
        } else if held.gold > pay.gold {
            extra.push(Goods::Gold(held.gold - pay.gold));
        }
        for (code, count) in &pay.stacks {
            let have = held_count(code);
            if *count > have {
                short.stacks.push((code.clone(), count - have));
            }
        }
        for (code, have) in &held.stacks {
            let want = pay
                .stacks
                .iter()
                .find(|(c, _)| c == code)
                .map_or(0, |(_, n)| *n);
            if *have > want {
                extra.push(Goods::Stack {
                    code: code.clone(),
                    count: have - want,
                });
            }
        }
        if !short.is_empty() {
            self.escrow_from_stash(o.account, &o.economy, (o.listing, o.id), &short)?;
            self.drain_escrow("offer", o.id)?;
        }
        for goods in &extra {
            self.deliver(
                o.account,
                &o.economy,
                goods,
                &Source {
                    kind: "returned",
                    listing: Some(l.id),
                    offer: Some(o.id),
                    from: ("offer", Some(o.id)),
                },
            )?;
        }
        let fee = self.pay_seller(l, pay, Some(o.id), ("offer", Some(o.id)))?;
        self.sell_listing(l, o.account, &o.buyer, pay, fee, Some(o.id))
    }
}
