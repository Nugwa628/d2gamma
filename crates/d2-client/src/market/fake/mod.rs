// D2RUST's own: an in-process market for tests and offscreen shots.
//! A [`MarketSource`] with a fixed set of listings made up in D2RUST's own
//! words, answering every ask on the next poll the way the realm's market
//! does (`docs/plans/server/market-contract.md`): the state, searches with
//! their filters, sorts, pages and facet counts, listing lookups, and buys
//! that check the price hash and the fee revision and take the listing
//! away. Game data, when there is some, names the base items and the runes
//! ([`FakeSource::new`]'s `names`); without it the stand-in words stay.
//!
//! The app uses it when `D2RUST_MARKET=fake` (tests and shot runs); a real
//! realm never does. Browsing is [`browse`]; offers, Deliveries and claims
//! are [`offers`]; selling and My shop are [`shop`]; the price checks are
//! [`prices`]. The shop's rows are the asker's listings; the trading side's
//! own listings follow its active ones.

pub mod browse;
pub mod offers;
pub mod prices;
pub mod shop;

use std::collections::{BTreeMap, VecDeque};

use super::prices::PriceOf;
use super::source::{
    Goods, ListingDetail, ListingId, ListingRow, MarketAsk, MarketCode, MarketDone, MarketEvent,
    MarketFees, MarketSource, MarketState, Price, Req, ShopRow, StashGoods,
};
use crate::market::trade::Holdings;

/// The fee revision the fake states.
pub const FEE_REV: u32 = 1;

/// The in-process market.
pub struct FakeSource {
    pub(crate) listings: Vec<ListingRow>,
    pub state: MarketState,
    next_req: Req,
    events: VecDeque<MarketEvent>,
    /// The sales the price checks read (a buy adds one).
    pub sales: Vec<prices::FakeSale>,
    /// The asks it got with their numbers, in order (tests read them).
    pub asks: Vec<(Req, MarketAsk)>,
    /// Offers, Deliveries and the account's stash ([`offers`]).
    pub trade: offers::Trade,
    /// The asker's shared stash as far as the fake knows it (the items the
    /// sell slot showed), by item ID.
    pub stash: BTreeMap<u32, StashGoods>,
    /// The asker's listings in every state (My shop), newest last.
    pub shop: Vec<ShopRow>,
    /// The next listing ID a post gets.
    next_listing: u64,
}

impl FakeSource {
    /// The fake's market for a character of `hardcore` / `ladder` /
    /// `expansion`; `names` gives a base item's or a rune's name by code
    /// from game data, if there is some.
    pub fn new(
        names: &dyn Fn(&str) -> Option<String>,
        hardcore: bool,
        ladder: u16,
        expansion: bool,
    ) -> Self {
        let state = MarketState {
            on: true,
            mod_id: "d2rust".into(),
            hardcore,
            ladder,
            expansion,
            max_listings: 50,
            max_offers: 20,
            max_offer_items: offers::OFFER_ITEMS as u8,
            max_days: 7,
            fees: MarketFees {
                fee_rev: FEE_REV,
                ..MarketFees::default()
            },
            currencies: browse::RUNES.iter().map(|(c, _)| (*c).to_owned()).collect(),
            ..MarketState::default()
        };
        let listings = browse::listings(names);
        let trade = offers::Trade::new(&listings, names);
        let mut f = FakeSource {
            sales: prices::sales(&listings),
            listings,
            state,
            next_req: 1,
            events: VecDeque::new(),
            asks: Vec::new(),
            trade,
            stash: BTreeMap::new(),
            shop: Vec::new(),
            next_listing: shop::FIRST_LISTING,
        };
        f.adopt_own();
        f.sync_counts();
        f
    }

    /// The listing `id`, while it is up.
    pub fn listing(&self, id: ListingId) -> Option<&ListingRow> {
        self.listings.iter().find(|l| l.listing == id)
    }

    /// The realm turns the market on or off: a pushed state (`req` 0).
    pub fn set_on(&mut self, on: bool) {
        self.state.on = on;
        self.state.off_reason = if on {
            super::source::OffReason::On
        } else {
            super::source::OffReason::Switch
        };
        self.events.push_back(MarketEvent::State {
            req: 0,
            state: self.state.clone(),
        });
    }

    /// The seller reprices listing `id` (its hash changes).
    pub fn reprice(&mut self, id: ListingId, price: Price) {
        if let Some(l) = self.listings.iter_mut().find(|l| l.listing == id) {
            l.price_hash = browse::price_hash(&price);
            l.price = Some(price);
        }
    }

    /// The realm's fees change (a new revision).
    pub fn set_fees(&mut self, fees: MarketFees) {
        self.state.fees = fees;
    }

    fn done(req: Req, code: MarketCode, id: Option<u64>) -> MarketEvent {
        MarketEvent::Done(MarketDone {
            req,
            code,
            wait_s: 0,
            repeated: false,
            id,
            rows: Vec::new(),
            text: String::new(),
        })
    }

    fn answer(&mut self, req: Req, ask: &MarketAsk) -> MarketEvent {
        if !self.state.on && *ask != MarketAsk::Hello {
            return Self::done(req, MarketCode::MarketOff, None);
        }
        match ask {
            MarketAsk::Hello => MarketEvent::State {
                req,
                state: MarketState {
                    last_req: req,
                    ..self.state.clone()
                },
            },
            MarketAsk::Search(q) => MarketEvent::Results {
                req,
                page: browse::search(&self.listings, q),
            },
            MarketAsk::ListingGet(id) => match self.listing(*id) {
                Some(row) => MarketEvent::Listing {
                    req,
                    detail: Box::new(ListingDetail {
                        state: 0,
                        row: row.clone(),
                        buyer_fee: self.state.fees.buyer_fee,
                        fee_rev: self.state.fees.fee_rev,
                        offer: None,
                    }),
                },
                None => Self::done(req, MarketCode::ListingGone, Some(*id)),
            },
            MarketAsk::Buy {
                listing,
                price_hash,
                fee_rev,
            } => {
                let Some(i) = self.listings.iter().position(|l| l.listing == *listing) else {
                    return Self::done(req, MarketCode::ListingGone, Some(*listing));
                };
                if *fee_rev != self.state.fees.fee_rev {
                    return Self::done(req, MarketCode::FeeChanged, Some(*listing));
                }
                let l = &self.listings[i];
                if l.price.is_none() || l.mine {
                    return Self::done(req, MarketCode::Other(99), Some(*listing));
                }
                if l.price_hash != *price_hash {
                    return Self::done(req, MarketCode::PriceChanged, Some(*listing));
                }
                let l = self.listings.remove(i);
                self.sales.push(prices::FakeSale {
                    key: prices::price_key(&l),
                    label: prices::label_of(&l),
                    at_ms: browse::NOW_MS,
                    price: l.price.clone().unwrap_or_default(),
                });
                self.bought(&l);
                Self::done(req, MarketCode::Done, Some(*listing))
            }
            MarketAsk::Post(p) => self.post(req, p),
            MarketAsk::ShopEdit {
                op,
                listings,
                fee_rev,
            } => self.shop_edit(req, op, listings, *fee_rev),
            MarketAsk::ShopOpen {
                tab,
                page,
                sort,
                desc,
            } => MarketEvent::Shop {
                req,
                page: self.shop_page(*tab, *page, *sort, *desc),
            },
            MarketAsk::Prices { of, days } => self.prices(req, of, *days),
            _ => self.answer_trade(req, ask),
        }
    }
}

impl FakeSource {
    /// A price check: the key of what it is of, then its history.
    fn prices(&mut self, req: Req, of: &PriceOf, days: u16) -> MarketEvent {
        if !(1..=prices::MAX_DAYS).contains(&days) {
            return Self::done(req, MarketCode::Other(99), None);
        }
        let (key, label) = match of {
            PriceOf::Listing(id) => match self.listing(*id) {
                Some(l) => (prices::price_key(l), prices::label_of(l)),
                None => return Self::done(req, MarketCode::ListingGone, Some(*id)),
            },
            // A shared stash item the sell slot showed; one without sales
            // of its key gets made-up ones.
            PriceOf::StashItem(id) => {
                let Some(g) = self.stash.get(id) else {
                    return Self::done(req, MarketCode::ItemNotFound, None);
                };
                let key = prices::item_key(g.group, &g.item);
                let label = prices::item_label(g.group, &g.item);
                if !self.sales.iter().any(|s| s.key == key) {
                    let n = prices::STASH_SALES.max(prices::sales_of(g.group));
                    let made = prices::made_up(&key, &label, g.group, n);
                    self.sales.extend(made);
                }
                (key, label)
            }
            PriceOf::Stackable(code) => {
                let label = self
                    .listings
                    .iter()
                    .find(|l| prices::price_key(l) == format!("s:{code}"))
                    .map_or_else(|| code.clone(), prices::label_of);
                (format!("s:{code}"), label)
            }
            PriceOf::Key(k) => {
                let label = self
                    .sales
                    .iter()
                    .find(|s| s.key == *k)
                    .map(|s| s.label.clone())
                    .or_else(|| {
                        self.listings
                            .iter()
                            .find(|l| prices::price_key(l) == *k)
                            .map(prices::label_of)
                    })
                    .unwrap_or_else(|| k.clone());
                (k.clone(), label)
            }
        };
        MarketEvent::Prices {
            req,
            history: Box::new(prices::history(
                &self.sales,
                &self.listings,
                &key,
                &label,
                days,
                browse::NOW_MS,
            )),
        }
    }
}

impl MarketSource for FakeSource {
    fn send(&mut self, ask: MarketAsk) -> Req {
        let req = self.next_req;
        self.next_req += 1;
        let e = self.answer(req, &ask);
        self.asks.push((req, ask));
        self.events.push_back(e);
        req
    }

    fn poll(&mut self) -> Vec<MarketEvent> {
        self.events.drain(..).collect()
    }

    fn holdings(&self) -> Option<Holdings> {
        Some(self.trade.stash.clone())
    }

    fn stash_item_seen(&mut self, goods: &StashGoods) {
        self.stash.insert(goods.item_id, goods.clone());
    }
}

/// Whether a listing sells an item (not a stackable).
pub fn is_item(l: &ListingRow) -> bool {
    matches!(l.goods, Goods::Item { .. })
}

#[cfg(test)]
mod tests;
