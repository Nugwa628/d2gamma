//! One market request: the wire's [`ToRealm`] on the economy, and the
//! answers back (`d2_net::msg::market`; contract section 3). Each request
//! is one job on the economy worker; its notices go out once it is done
//! ([`Market::push_notices`]).

use std::collections::BTreeMap;
use std::sync::Arc;

use d2_economy as ec;
use d2_economy::Economy;
use d2_net::msg::market::{self as w, Ask, FromRealm, ToRealm};
use d2_net::msg::ErrorCode;

use d2_accounts::AccountId;

use super::{Market, Who};
use crate::session::ControlTx;

/// `s` cut to at most `max` bytes, on a character boundary (the wire's
/// bounds).
fn cut(s: &str, max: usize) -> String {
    let mut n = s.len().min(max);
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    s[..n].to_owned()
}

fn u16_of(n: u32) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

fn done(req: u64, code: ErrorCode) -> Vec<u8> {
    FromRealm::Done(w::Done::refused(req, code)).body()
}

fn done_of(req: u64, d: &ec::Done) -> Vec<u8> {
    FromRealm::Done(w::Done {
        repeated: d.repeated,
        id: d.id.map(|i| i as u64),
        ..w::Done::ok(req)
    })
    .body()
}

fn rows_done(req: u64, d: &ec::RowsDone) -> Vec<u8> {
    let rows = d
        .rows
        .iter()
        .map(|r| match r.outcome {
            // A relist's row names the new listing.
            Ok(Some(new)) => (new as u64, ErrorCode(0)),
            Ok(None) => (r.id as u64, ErrorCode(0)),
            Err(f) => (r.id as u64, ErrorCode(f.code())),
        })
        .collect();
    FromRealm::Done(w::Done {
        repeated: d.repeated,
        rows,
        ..w::Done::ok(req)
    })
    .body()
}

fn price_out(p: &ec::Price) -> w::Price {
    w::Price {
        gold: p.gold,
        stacks: p.stacks.clone(),
        items: p.items.iter().map(|i| i.0).collect(),
    }
}

fn price_in(p: &w::Price) -> ec::Price {
    ec::Price {
        gold: p.gold,
        stacks: p.stacks.clone(),
        items: p.items.iter().map(|i| ec::ItemId(*i)).collect(),
    }
}

fn goods_out(g: &ec::Goods, item: Option<&ec::Item>) -> w::Goods {
    match g {
        ec::Goods::Item(id) => w::Goods::Item {
            item_id: id.0,
            record: item.map(|i| i.record.clone()).unwrap_or_default(),
        },
        ec::Goods::Stack { code, count } => w::Goods::Stack {
            code: code.clone(),
            count: *count,
        },
        ec::Goods::Gold(g) => w::Goods::Gold(*g),
    }
}

fn group_out(g: ec::Group) -> w::Group {
    w::Group::from_u8(g as u8).unwrap_or(w::Group::Other)
}

fn group_in(g: w::Group) -> ec::Group {
    match g {
        w::Group::Other => ec::Group::Other,
        w::Group::Magic => ec::Group::Magic,
        w::Group::Rare => ec::Group::Rare,
        w::Group::Crafted => ec::Group::Crafted,
        w::Group::Set => ec::Group::Set,
        w::Group::Unique => ec::Group::Unique,
        w::Group::Runeword => ec::Group::Runeword,
        w::Group::Stackable => ec::Group::Stackable,
    }
}

fn mode_out(m: ec::ListingMode) -> w::ListingMode {
    match m {
        ec::ListingMode::Price => w::ListingMode::Price,
        ec::ListingMode::OffersOnly => w::ListingMode::OffersOnly,
        ec::ListingMode::PriceAndOffers => w::ListingMode::PriceAndOffers,
    }
}

fn mode_in(m: w::ListingMode) -> ec::ListingMode {
    match m {
        w::ListingMode::Price => ec::ListingMode::Price,
        w::ListingMode::OffersOnly => ec::ListingMode::OffersOnly,
        w::ListingMode::PriceAndOffers => ec::ListingMode::PriceAndOffers,
    }
}

fn listing_state(s: ec::ListingState) -> w::ListingState {
    match s {
        ec::ListingState::Active => w::ListingState::Active,
        ec::ListingState::Sold => w::ListingState::Sold,
        ec::ListingState::Cancelled => w::ListingState::Cancelled,
        ec::ListingState::Expired => w::ListingState::Expired,
    }
}

fn offer_state(s: ec::OfferState) -> w::OfferState {
    match s {
        ec::OfferState::Open => w::OfferState::Open,
        ec::OfferState::Countered => w::OfferState::Countered,
        ec::OfferState::Accepted => w::OfferState::Accepted,
        ec::OfferState::Declined => w::OfferState::Declined,
        ec::OfferState::Withdrawn => w::OfferState::Withdrawn,
        ec::OfferState::Returned => w::OfferState::Returned,
    }
}

fn tri(t: w::TriState) -> Option<bool> {
    match t {
        w::TriState::Any => None,
        w::TriState::Yes => Some(true),
        w::TriState::No => Some(false),
    }
}

/// A search on the wire as the economy takes it.
pub(super) fn query_in(q: &w::MarketQuery) -> ec::MarketQuery {
    let range = |r: (u8, u8)| (r != (0, u8::MAX)).then_some(r);
    ec::MarketQuery {
        filter: ec::ItemFilter {
            text: Some(q.text.trim().to_owned()).filter(|t| !t.is_empty()),
            item_type: None,
            quality: None,
            level: range(q.level),
            stats: q
                .stats
                .iter()
                .map(|s| ec::StatRange {
                    stat: s.stat,
                    min: s.min,
                    max: s.max,
                })
                .collect(),
            ethereal: tri(q.ethereal),
            sockets: range(q.sockets),
            class: q.class,
        },
        price: None,
        sort: match q.sort.by {
            w::SortBy::Posted => ec::MarketSort::Date,
            w::SortBy::Price => ec::MarketSort::Price,
            w::SortBy::Stat => ec::MarketSort::Stat(q.sort.stat),
            w::SortBy::Level => ec::MarketSort::Level,
            w::SortBy::Name => ec::MarketSort::Name,
            w::SortBy::Seller => ec::MarketSort::Seller,
        },
        desc: q.sort.desc,
        page: q.page,
        groups: q.groups.iter().map(|g| group_in(*g)).collect(),
        types: q.types.clone(),
        socketed: tri(q.socketed),
        identified: tri(q.identified),
        price_mode: match q.price_mode {
            w::PriceMode::Any => ec::PriceMode::Any,
            w::PriceMode::HasPrice => ec::PriceMode::HasPrice,
            w::PriceMode::TakesOffers => ec::PriceMode::TakesOffers,
            w::PriceMode::CurrenciesOnly => ec::PriceMode::CurrenciesOnly,
            w::PriceMode::GoldOnly => ec::PriceMode::GoldOnly,
        },
        caps: q
            .caps
            .iter()
            .map(|(code, max)| ec::PriceFilter {
                code: Some(code.clone()).filter(|c| !c.is_empty()),
                min: 0,
                max: *max,
            })
            .collect(),
        // "" is gold, as on the wire.
        sort_currency: (q.sort.by == w::SortBy::Price).then(|| q.sort.currency.clone()),
        group_first: q.group_first,
        page_size: q.page_size,
    }
}

fn facets_out(f: &ec::Facets) -> w::Facets {
    w::Facets {
        groups: f.groups.iter().map(|(g, n)| (group_out(*g), *n)).collect(),
        types: f
            .types
            .iter()
            .take(w::MAX_FACET_TYPES)
            .map(|(t, n)| (cut(t, w::MAX_TYPE_CODE), *n))
            .collect(),
        ethereal: f.ethereal,
        socketed: f.socketed,
        identified: f.identified,
        price_modes: f.price_modes,
    }
}

fn row_out(l: &ec::Listing, owner: &ec::Owner) -> w::ListingRow {
    w::ListingRow {
        listing: l.id as u64,
        group: group_out(l.group),
        goods: goods_out(&l.goods, l.item.as_ref()),
        mode: mode_out(l.mode),
        price: l.price.as_ref().map(price_out),
        price_hash: l.price_hash(),
        seller: cut(&l.seller, w::MAX_NAME),
        seller_sales: l.seller_sales,
        posted_ms: l.posted as u64,
        expires_ms: l.expires as u64,
        note: cut(&l.note, w::MAX_NOTE),
        offers: l.offers,
        mine: l.account == owner.account.0,
    }
}

fn item_out(i: &ec::Item) -> w::ItemRecord {
    w::ItemRecord {
        item_id: i.id.0,
        record: i.record.clone(),
    }
}

fn shop_row(r: &ec::ShopRow, owner: &ec::Owner) -> w::ShopRow {
    let l = &r.listing;
    w::ShopRow {
        row: row_out(l, owner),
        state: listing_state(l.state),
        hidden: l.hidden,
        live_offers: l.offers,
        newest_offer_ms: r.newest_offer.unwrap_or(0) as u64,
        sold: r.sold.as_ref().map(|s| w::Sold {
            buyer: cut(&s.buyer, w::MAX_NAME),
            paid: price_out(&s.paid),
            sale_fee: s.sale_fee,
            closed_ms: s.closed as u64,
        }),
        relistable: r.relistable,
    }
}

fn offer_row(r: &ec::OfferRow) -> w::OfferRow {
    let (o, l) = (&r.offer, &r.listing);
    w::OfferRow {
        offer: o.id as u64,
        listing: l.id as u64,
        goods: goods_out(&l.goods, l.item.as_ref()),
        price: l.price.as_ref().map(price_out),
        buyer: cut(&o.buyer, w::MAX_NAME),
        seller: cut(&l.seller, w::MAX_NAME),
        offered: price_out(&o.price),
        counter: o.counter.as_ref().map(price_out),
        state: offer_state(o.state),
        at_ms: o.at as u64,
        answered_ms: o.answered.unwrap_or(0) as u64,
    }
}

fn delivery_row(r: &ec::DeliveryRow) -> w::DeliveryRow {
    let d = &r.delivery;
    w::DeliveryRow {
        delivery: d.id as u64,
        source: d.source_code(),
        reason: r.reason as u8,
        listing: d.listing.map(|i| i as u64),
        offer: d.offer.map(|i| i as u64),
        about: cut(&r.about, w::MAX_ABOUT),
        goods: goods_out(&d.goods, d.item.as_ref()),
        at_ms: d.at as u64,
    }
}

pub(super) fn notice_out(n: &ec::Notice) -> w::Notice {
    w::Notice {
        what: n.what as u8,
        listing: n.listing as u64,
        offer: n.offer.map(|i| i as u64),
        about: cut(&n.about, w::MAX_ABOUT),
        who: cut(&n.who, w::MAX_NAME),
        price: n.price.as_ref().map(price_out),
        deliveries_waiting: u16_of(n.deliveries_waiting),
        at_ms: n.at as u64,
    }
}

fn series_out(index: usize, s: &ec::Series) -> w::PriceSeries {
    w::PriceSeries {
        index: index as u8,
        shape: match s.shape {
            ec::Shape::One => w::PriceShape::One,
            ec::Shape::Bundle => w::PriceShape::Bundle,
            ec::Shape::WithItems => w::PriceShape::WithItems,
        },
        currencies: s.currencies.clone(),
        sold: s.sold,
        listed_now: s.listed_now,
        lowest_now: s.lowest_now.clone(),
        median_now: s.median_now.clone(),
        buckets: s
            .buckets
            .iter()
            .map(|b| w::Bucket {
                index: b.index,
                sales: u16_of(b.sales),
                values: b
                    .values
                    .iter()
                    .map(|&(min, median, max)| w::Spread { min, median, max })
                    .collect(),
            })
            .collect(),
    }
}

/// A listing `owner` may see: its own, or a shown one of its economy.
fn visible(l: Option<ec::Listing>, owner: &ec::Owner) -> Option<ec::Listing> {
    l.filter(|l| l.economy == owner.economy.split() && (!l.hidden || l.account == owner.account.0))
}

impl Market {
    /// One request of `who` (on a task of its own).
    pub(super) async fn handle(self: Arc<Self>, who: Who, m: ToRealm) {
        let req = m.req;
        let hello = matches!(m.ask, Ask::Hello);
        let owner = who
            .character
            .as_ref()
            .map(|c| ec::Owner::new(who.account.0, self.economy_of(c.status)));
        // Seated on every request: a resumed session's stream is new.
        super::MarketService::seat(self.as_ref(), &who);
        let refused = match &owner {
            None if !hello => Some((ErrorCode::NO_CHAR_SELECTED, 0)),
            Some(_) if !hello && !self.config.market => Some((ErrorCode::MARKET_OFF, 0)),
            Some(_) if self.config.market => match &m.ask {
                Ask::Search(_) => Self::limited(&self.searches, who.account.0)
                    .map(|wait| (ErrorCode::RATE_LIMITED, wait)),
                Ask::Post(_) => Self::limited(&self.posts, who.account.0)
                    .map(|wait| (ErrorCode::RATE_LIMITED, wait)),
                _ => None,
            },
            _ => None,
        };
        if let Some((code, wait_s)) = refused {
            let d = w::Done {
                wait_s,
                ..w::Done::refused(req, code)
            };
            who.out.send(FromRealm::Done(d).body());
            return;
        }
        let me = self.clone();
        let account = who.account;
        let name = who.character.as_ref().map(|c| c.name.clone());
        let out = who.out.clone();
        let ran = self
            .bank
            .run(move |e| match (owner, name) {
                (Some(owner), Some(name)) => me.answer(e, &owner, &name, m, &out),
                _ => out.send(FromRealm::State(me.off_state(e, req, account, None)).body()),
            })
            .await;
        if let Err(e) = ran {
            self.log.line(&format!("market: {e}"));
            who.out.send(done(req, ErrorCode::OTHER));
        }
    }

    /// One request's answers on `out`, then its notices (on the economy
    /// worker, so the asker's answer comes before the state they change).
    fn answer(&self, e: &mut Economy, owner: &ec::Owner, name: &str, m: ToRealm, out: &ControlTx) {
        let req = m.req;
        if !self.config.market {
            let account = AccountId(owner.account.0);
            out.send(FromRealm::State(self.off_state(e, req, account, Some(owner))).body());
            return;
        }
        let changes = m.ask.changes();
        let mut bodies = Vec::new();
        if let Err(err) = self.run(e, owner, name, m, &mut bodies) {
            bodies.push(self.refusal(req, &err));
        }
        for b in bodies {
            out.send(b);
        }
        if changes {
            self.push_notices(e, Some(owner));
        }
    }

    /// A refusal or a failure as `MarketDone`.
    fn refusal(&self, req: u64, err: &ec::Error) -> Vec<u8> {
        match err.refusal() {
            Some(r) => {
                let wait_s = if r == ec::Refusal::RateLimited { 60 } else { 0 };
                FromRealm::Done(w::Done {
                    wait_s,
                    ..w::Done::refused(req, ErrorCode(r.code()))
                })
                .body()
            }
            None => {
                self.log.line(&format!("market: {err}"));
                done(req, ErrorCode::OTHER)
            }
        }
    }

    fn run(
        &self,
        e: &mut Economy,
        owner: &ec::Owner,
        name: &str,
        m: ToRealm,
        out: &mut Vec<Vec<u8>>,
    ) -> Result<(), ec::Error> {
        let req = m.req;
        let now = self.clock.now();
        let now_ms = now as u64;
        let rq = ec::Req::account(owner.account, req);
        match m.ask {
            Ask::Hello => out.push(FromRealm::State(self.state(e, req, owner)).body()),
            Ask::Search(q) => {
                let eq = query_in(&q);
                let page_size = eq.rows_a_page()? as u8;
                let page = e.search(&owner.economy, None, &eq, now)?;
                let facets = if q.facets {
                    Some(e.facets(&owner.economy, &eq, now)?)
                } else {
                    None
                };
                let head = w::ResultsHead {
                    total: page.total,
                    page: page.page,
                    page_size,
                    rows: page.listings.len() as u8,
                    now_ms,
                    facets: facets.is_some(),
                };
                out.push(FromRealm::ResultsBegin { req, head }.body());
                if let Some(f) = facets {
                    let facets = facets_out(&f);
                    out.push(FromRealm::Facets { req, facets }.body());
                }
                for l in &page.listings {
                    let row = row_out(l, owner);
                    out.push(FromRealm::Row { req, row }.body());
                }
                out.push(FromRealm::ListEnd { req }.body());
            }
            Ask::ListingGet { listing } => {
                let Some(l) = visible(e.listing(listing as i64)?, owner) else {
                    return Err(ec::Refusal::ListingGone.into());
                };
                let mine = self.my_offer(e, owner, l.id)?;
                if let Some((_, items)) = &mine {
                    for i in items {
                        let item = item_out(i);
                        out.push(FromRealm::Item { req, item }.body());
                    }
                }
                let detail = w::ListingDetail {
                    state: listing_state(l.state),
                    row: row_out(&l, owner),
                    buyer_fee: self.config.market_buyer_fee,
                    fee_rev: self.config.fee_rev(),
                    my_offer: mine.map(|(o, _)| w::ListingOffer {
                        offer: o.id as u64,
                        offered: price_out(&o.price),
                        counter: o.counter.as_ref().map(price_out),
                        state: offer_state(o.state),
                        at_ms: o.at as u64,
                        answered_ms: o.answered.unwrap_or(0) as u64,
                    }),
                };
                out.push(FromRealm::Listing { req, detail }.body());
            }
            Ask::Buy {
                listing,
                price_hash,
                fee_rev,
            } => {
                let d = e.buy_with(owner, &rq, listing as i64, name, &price_hash, fee_rev, now)?;
                out.push(done_of(req, &d));
            }
            Ask::Offer {
                listing,
                price,
                fee_rev,
            } => {
                let p = price_in(&price);
                let d = e.offer_with(owner, &rq, listing as i64, name, &p, fee_rev, now)?;
                out.push(done_of(req, &d));
            }
            Ask::Answer {
                offer,
                how,
                counter,
                fee_rev,
            } => {
                let how = match (how, counter) {
                    (w::AnswerHow::Accept, _) => ec::OfferAnswer::Accept,
                    (w::AnswerHow::Decline, _) => ec::OfferAnswer::Decline,
                    (w::AnswerHow::Withdraw, _) => ec::OfferAnswer::Withdraw,
                    (w::AnswerHow::Counter, Some(p)) => ec::OfferAnswer::Counter(price_in(&p)),
                    (w::AnswerHow::Counter, None) => return Err(ec::Refusal::BadRequest.into()),
                };
                let d = e.answer_with(owner, &rq, offer as i64, &how, fee_rev, now)?;
                out.push(done_of(req, &d));
            }
            Ask::Post(p) => {
                let goods = match p.goods {
                    w::PostGoods::Item { item_id } => ec::Goods::Item(ec::ItemId(item_id)),
                    w::PostGoods::Stack { code, count } => ec::Goods::Stack { code, count },
                };
                let facts = match &goods {
                    ec::Goods::Item(id) => self.read_facts(e, owner, *id)?,
                    _ => None,
                };
                let post = ec::Post {
                    seller: name.to_owned(),
                    goods,
                    mode: mode_in(p.mode),
                    price: p.price.as_ref().map(price_in),
                    days: p.days,
                };
                let opts = ec::PostOptions {
                    note: p.note,
                    hidden: p.hidden,
                    fee_rev: Some(p.fee_rev),
                    facts,
                };
                let d = e.post_with(owner, &rq, &post, &opts, now)?;
                out.push(done_of(req, &d));
            }
            Ask::ShopEdit {
                op,
                listings,
                fee_rev,
            } => {
                let op = match op {
                    w::ShopOp::Cancel => ec::ShopOp::Cancel,
                    w::ShopOp::Relist { days } => ec::ShopOp::Relist { days },
                    w::ShopOp::Hide => ec::ShopOp::Hide,
                    w::ShopOp::Show => ec::ShopOp::Show,
                    w::ShopOp::Note(n) => ec::ShopOp::Note(n),
                    w::ShopOp::Reprice { mode, price } => ec::ShopOp::Reprice {
                        mode: mode_in(mode),
                        price: price.as_ref().map(price_in),
                    },
                };
                let ids: Vec<i64> = listings.iter().map(|i| *i as i64).collect();
                let d = e.shop_edit(owner, &rq, &op, &ids, fee_rev, now)?;
                out.push(rows_done(req, &d));
            }
            Ask::ShopOpen {
                tab,
                page,
                sort,
                desc,
            } => {
                let t = match tab {
                    w::ShopTab::Shown => ec::ShopTab::Shown,
                    w::ShopTab::Hidden => ec::ShopTab::Hidden,
                    w::ShopTab::Sold => ec::ShopTab::Sold,
                    w::ShopTab::Expired => ec::ShopTab::Expired,
                };
                let s = match sort {
                    w::ShopSort::Posted => ec::ShopSort::Posted,
                    w::ShopSort::Ends => ec::ShopSort::Ends,
                    w::ShopSort::GoldPrice => ec::ShopSort::Gold,
                    w::ShopSort::Name => ec::ShopSort::Name,
                };
                let p = e.shop_page(owner, t, page, s, desc, now)?;
                let head = w::ShopHead {
                    tab,
                    page: p.page,
                    rows: p.rows.len() as u8,
                    counts: p.counts,
                    slots_used: u16_of(p.slots_used),
                    slots_max: u16_of(p.slots_max),
                    now_ms,
                };
                out.push(FromRealm::ShopBegin { req, head }.body());
                for r in &p.rows {
                    let row = shop_row(r, owner);
                    out.push(FromRealm::ShopRow { req, row }.body());
                }
                out.push(FromRealm::ListEnd { req }.body());
            }
            Ask::OffersOpen {
                which,
                live_only,
                page,
            } => {
                let wh = match which {
                    w::OffersWhich::OnMyListings => ec::OffersWhich::OnMine,
                    w::OffersWhich::MadeByMe => ec::OffersWhich::MadeByMe,
                };
                let p = e.offers_page(owner, wh, live_only, page)?;
                let head = w::OffersHead {
                    which,
                    page: p.page,
                    rows: p.rows.len() as u8,
                    total: p.total,
                    now_ms,
                };
                out.push(FromRealm::OffersBegin { req, head }.body());
                for r in &p.rows {
                    for i in &r.items {
                        let item = item_out(i);
                        out.push(FromRealm::Item { req, item }.body());
                    }
                    let row = offer_row(r);
                    out.push(FromRealm::OfferRow { req, row }.body());
                }
                out.push(FromRealm::ListEnd { req }.body());
            }
            Ask::DeliveriesOpen { page } => {
                let p = e.deliveries_page(owner, page)?;
                let head = w::DeliveriesHead {
                    page: p.page,
                    rows: p.rows.len() as u8,
                    total: p.total,
                    now_ms,
                };
                out.push(FromRealm::DeliveriesBegin { req, head }.body());
                for r in &p.rows {
                    let row = delivery_row(r);
                    out.push(FromRealm::DeliveryRow { req, row }.body());
                }
                out.push(FromRealm::ListEnd { req }.body());
            }
            Ask::Claim { all, deliveries } => {
                let which = if all {
                    ec::Claim::All
                } else {
                    ec::Claim::Ids(deliveries.iter().map(|i| *i as i64).collect())
                };
                let d = e.claim_many(owner, &rq, &which, now)?;
                out.push(rows_done(req, &d));
            }
            Ask::Prices { of, days } => {
                let key = match of {
                    w::PriceOf::Listing(id) => {
                        if visible(e.listing(id as i64)?, owner).is_none() {
                            return Err(ec::Refusal::ListingGone.into());
                        }
                        e.listing_price_key(id as i64)?
                            .ok_or(ec::Refusal::ListingGone)?
                    }
                    w::PriceOf::StashItem(id) => e
                        .item_price_key(owner, ec::ItemId(id))?
                        .ok_or(ec::Refusal::ItemNotFound)?,
                    w::PriceOf::Stackable(code) => {
                        if e.config().stackable(&code).is_none() {
                            return Err(ec::Refusal::ItemNotFound.into());
                        }
                        ec::price_key(None, &code)
                    }
                    w::PriceOf::Key(k) => k,
                };
                let h = e.price_history(&owner.economy, &key, days, now)?;
                let head = w::PricesHead {
                    key: cut(&h.key, w::MAX_PRICE_KEY),
                    label: cut(&h.label, w::MAX_ABOUT),
                    now_ms: h.now as u64,
                    bucket_ms: h.bucket_ms as u64,
                    listed_now: h.listed_now,
                    sold: h.sold,
                    series: h.series.len().min(u8::MAX as usize) as u8,
                };
                out.push(FromRealm::PricesBegin { req, head }.body());
                for (i, s) in h.series.iter().enumerate().take(u8::MAX as usize) {
                    for series in series_out(i, s).split_to_fit() {
                        out.push(FromRealm::PriceSeries { req, series }.body());
                    }
                }
                let sales = h
                    .recent
                    .iter()
                    .take(w::MAX_RECENT)
                    .map(|(at, p)| w::Sale {
                        at_ms: *at as u64,
                        price: price_out(p),
                    })
                    .collect();
                out.push(FromRealm::PriceRecent { req, sales }.body());
                out.push(FromRealm::ListEnd { req }.body());
            }
        }
        Ok(())
    }

    /// The facts of the asker's stash item `id`, read from its record with
    /// the realm's game data: the post stores them in place of the ones it
    /// had. `None`: no reader, or a record it can't read (the stored facts
    /// stay).
    fn read_facts(
        &self,
        e: &Economy,
        owner: &ec::Owner,
        id: ec::ItemId,
    ) -> Result<Option<ec::ItemFacts>, ec::Error> {
        let Some(reader) = &self.reader else {
            return Ok(None);
        };
        let Some(record) = e.stash_record(owner, id)? else {
            return Ok(None);
        };
        match reader.facts(&record) {
            Ok(f) => Ok(Some(f)),
            Err(err) => {
                self.log.line(&format!("market: item {}: {err}", id.0));
                Ok(None)
            }
        }
    }

    /// The asker's live offer on `listing`, with its items' records.
    fn my_offer(
        &self,
        e: &Economy,
        owner: &ec::Owner,
        listing: i64,
    ) -> Result<Option<(ec::Offer, Vec<ec::Item>)>, ec::Error> {
        let mut page = 0;
        loop {
            let p = e.offers_page(owner, ec::OffersWhich::MadeByMe, true, page)?;
            if let Some(r) = p.rows.iter().find(|r| r.listing.id == listing) {
                return Ok(Some((r.offer.clone(), r.items.clone())));
            }
            if p.rows.is_empty() || (u32::from(page) + 1) * ec::PAGE >= p.total {
                return Ok(None);
            }
            page += 1;
        }
    }

    fn limits(&self) -> (w::MarketLimits, w::Fees) {
        let c = &self.config;
        (
            w::MarketLimits {
                max_listings: u16_of(c.market_max_listings),
                max_offers: u16_of(c.market_max_offers),
                max_days: c.market_days,
                max_offer_items: c.market_offer_items.min(u8::MAX as u32) as u8,
                searches_per_min: u16_of(c.market_searches_per_min),
                posts_per_min: u16_of(c.market_posts_per_min),
            },
            w::Fees {
                sale_fee_pct: c.market_fee_pct,
                listing_fee: c.market_listing_fee,
                buyer_fee: c.market_buyer_fee,
                fee_rev: c.fee_rev(),
            },
        )
    }

    fn last_req(&self, e: &Economy, account: AccountId) -> u64 {
        e.last_req(&ec::Scope::Account(ec::AccountId(account.0)))
            .unwrap_or_else(|err| {
                self.log.line(&format!("market: last request: {err}"));
                0
            })
    }

    /// `owner`'s `MarketState` (`req` 0 when pushed).
    pub(super) fn state(&self, e: &Economy, req: u64, owner: &ec::Owner) -> w::MarketState {
        let c = e.counts(owner).unwrap_or_else(|err| {
            self.log.line(&format!("market: counts: {err}"));
            ec::MarketCounts {
                active_listings: 0,
                deliveries_waiting: 0,
                live_offers_to_me: 0,
                my_live_offers: 0,
            }
        });
        let (limits, fees) = self.limits();
        let k = &owner.economy;
        w::MarketState {
            req,
            on: true,
            off_reason: w::OffReason::On,
            mod_id: cut(&k.mod_id, w::MAX_MOD_ID),
            hardcore: k.hardcore,
            ladder: k.ladder,
            expansion: k.expansion,
            limits,
            fees,
            counts: w::MarketCounts {
                active_listings: u16_of(c.active_listings),
                deliveries_waiting: u16_of(c.deliveries_waiting),
                live_offers_to_me: u16_of(c.live_offers_to_me),
                my_live_offers: u16_of(c.my_live_offers),
            },
            last_req: self.last_req(e, AccountId(owner.account.0)),
            stackables: self.stackable_codes(),
        }
    }

    /// The state of a market that is off for the asker: the switch, or no
    /// character selected (`owner` `None`).
    fn off_state(
        &self,
        e: &Economy,
        req: u64,
        account: AccountId,
        owner: Option<&ec::Owner>,
    ) -> w::MarketState {
        let (limits, fees) = self.limits();
        let key = owner.map(|o| o.economy.clone());
        w::MarketState {
            req,
            on: false,
            off_reason: if owner.is_some() {
                w::OffReason::Switch
            } else {
                w::OffReason::NoCharacter
            },
            mod_id: cut(&self.mod_id, w::MAX_MOD_ID),
            hardcore: key.as_ref().is_some_and(|k| k.hardcore),
            ladder: key.as_ref().map_or(0, |k| k.ladder),
            expansion: key.as_ref().is_none_or(|k| k.expansion),
            limits,
            fees,
            counts: w::MarketCounts::default(),
            last_req: self.last_req(e, account),
            stackables: self.stackable_codes(),
        }
    }

    fn stackable_codes(&self) -> Vec<String> {
        self.config
            .stackables
            .iter()
            .take(w::MAX_CURRENCIES * 4)
            .map(|s| cut(&s.code, w::MAX_CODE))
            .collect()
    }

    /// Hands the economy's notices to their accounts' seated sessions in
    /// the same economy, then a fresh `MarketState` to each of them and to
    /// `asker` (its counts changed).
    pub(super) fn push_notices(&self, e: &mut Economy, asker: Option<&ec::Owner>) {
        let notices = e.take_notices().unwrap_or_else(|err| {
            self.log.line(&format!("market: notices: {err}"));
            Vec::new()
        });
        let mut touched: BTreeMap<(i64, String), ()> = BTreeMap::new();
        if let Some(o) = asker {
            touched.insert((o.account.0, o.economy.split()), ());
        }
        for n in &notices {
            let body = FromRealm::Notice(notice_out(n)).body();
            self.seats.send(n.account.0, &n.economy, &body);
            touched.insert((n.account.0, n.economy.clone()), ());
        }
        for (account, split) in touched.into_keys() {
            let seats = self.seats.of(account, &split);
            let Some(first) = seats.first() else {
                continue;
            };
            let owner = ec::Owner::new(account, first.key.clone());
            let body = FromRealm::State(self.state(e, 0, &owner)).body();
            for s in &seats {
                s.out.send(body.clone());
            }
        }
    }
}
