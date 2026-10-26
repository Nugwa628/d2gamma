// D2RUST's own: the fake market's selling side and My shop.
//! Posts, shop edits and shop pages of [`super::FakeSource`], with the
//! contract's rules (`docs/plans/server/market-contract.md` 3, 3.3): a
//! post takes a stash item the fake knows into escrow; an edit acts on
//! each named listing and answers per row; hidden listings leave the
//! search and keep their escrow; a relist of a cancelled or expired
//! listing lists its goods anew. [`FakeSource::seed_shop`] gives the fake
//! a made-up shop in every tab, for offscreen shots.
//!
//! The shop's rows are the asker's listings. The trading side
//! ([`super::offers`]) keeps the active ones as its own listings, for
//! offers on them and sales; a cancelled or expired listing's goods wait
//! in its Deliveries until claimed or listed again.

use std::cmp::Ordering;

use super::offers::ME;
use super::{browse, FakeSource};
use crate::market::source::{
    Goods, Group, ItemView, ListingId, ListingMode, ListingRow, ListingState, MarketCode,
    MarketEvent, Post, PostGoods, Price, Req, ShopOp, ShopPage, ShopRow, ShopSort, ShopTab, Sold,
    NOTE_MAX, SHOP_PAGE,
};
use crate::market::trade::{DeliverySource, OfferState};

/// The first listing ID a post gets (the browse listings and the trading
/// side's own listings are below).
pub const FIRST_LISTING: u64 = 9100;

/// The most listings one edit names.
pub const EDIT_MAX: usize = 50;

const DAY_MS: u64 = 24 * 3600 * 1000;

/// A made-up shop row: the browse listing it copies, its state, hidden,
/// mode, price, note and live offers.
type Seed<'a> = (
    usize,
    ListingState,
    bool,
    ListingMode,
    Option<Price>,
    &'a str,
    u16,
);

/// Whether `note` is a note the realm takes: 0-60 bytes of printable
/// text.
pub fn note_ok(note: &str) -> bool {
    note.len() <= NOTE_MAX && !note.chars().any(char::is_control)
}

impl FakeSource {
    /// Whether `price` fits `mode`: a priced mode needs gold or currencies
    /// of the realm's (no items); offers only takes none.
    fn price_ok(&self, mode: ListingMode, price: Option<&Price>) -> bool {
        match price {
            None => !mode.has_price(),
            Some(p) => {
                mode.has_price()
                    && !p.is_empty()
                    && p.items.is_empty()
                    && p.stacks.len() <= 16
                    && p.stacks
                        .iter()
                        .all(|(c, n)| *n > 0 && self.state.currencies.contains(c))
            }
        }
    }

    fn days_ok(&self, days: u8) -> bool {
        (1..=self.state.max_days.max(1)).contains(&days)
    }

    /// Active listings, shown and hidden.
    fn slots_used(&self) -> usize {
        self.shop
            .iter()
            .filter(|r| r.state == ListingState::Active)
            .count()
    }

    /// The copies of shop row `i` follow it: the trading side's own
    /// listing while active (its offer count stays the trading side's), and
    /// the browse copy while active and shown. The trading side's seeded
    /// listings (below [`FIRST_LISTING`]) stay out of the search, as made.
    fn sync(&mut self, i: usize) {
        let r = &self.shop[i];
        let id = r.row.listing;
        let active = r.state == ListingState::Active;
        let shown = active && !r.hidden;
        let mut row = r.row.clone();
        let own = &mut self.trade.own;
        match own.iter().position(|l| l.listing == id) {
            Some(k) if active => {
                row.offers = own[k].offers;
                own[k] = row.clone();
            }
            Some(k) => {
                own.remove(k);
            }
            None if active => own.push(row.clone()),
            None => {}
        }
        if id >= FIRST_LISTING {
            self.listings.retain(|l| l.listing != id);
            if shown {
                self.listings.push(row);
            }
        }
        self.sync_counts();
    }

    /// The trading side's own listings become shop rows (at the fake's
    /// start).
    pub(super) fn adopt_own(&mut self) {
        for l in &self.trade.own {
            if self.shop.iter().any(|r| r.row.listing == l.listing) {
                continue;
            }
            self.shop.push(ShopRow {
                row: l.clone(),
                state: ListingState::Active,
                hidden: false,
                live_offers: 0,
                newest_offer_ms: 0,
                sold: None,
                relistable: false,
            });
        }
    }

    /// The trading side sold the asker's listing `id` to `buyer` for
    /// `paid`: its shop row moves to Sold.
    pub(super) fn own_sold(&mut self, id: ListingId, buyer: &str, paid: &Price) {
        let Some(r) = self.shop.iter_mut().find(|r| r.row.listing == id) else {
            return;
        };
        r.state = ListingState::Sold;
        r.sold = Some(Sold {
            buyer: buyer.into(),
            paid: paid.clone(),
            sale_fee: 0,
            closed_ms: browse::NOW_MS,
        });
        self.listings.retain(|l| l.listing != id);
    }

    /// The Deliveries row holding closed listing `id`'s goods, if they wait
    /// there still.
    fn closed_goods(&self, id: ListingId) -> Option<usize> {
        self.trade.deliveries.iter().position(|d| {
            d.listing == Some(id)
                && matches!(
                    d.source,
                    DeliverySource::Cancelled | DeliverySource::Expired
                )
        })
    }

    /// Closed shop row `i`'s goods go to Deliveries (`source`: cancelled or
    /// expired), `hours` ago.
    fn close_to_deliveries(&mut self, i: usize, source: DeliverySource, hours: u64) {
        let l = &self.shop[i].row;
        let item = matches!(l.goods, Goods::Item { .. }).then(|| (l.group, l.item.clone()));
        let (id, about, goods) = (l.listing, l.item.name.clone(), l.goods.clone());
        self.trade
            .deliver(source, Some(id), None, &about, goods, item, hours);
    }

    fn new_listing(&mut self) -> ListingId {
        let id = self.next_listing;
        self.next_listing += 1;
        id
    }

    /// 0xBB: the goods leave the stash for escrow; the answer names the
    /// new listing.
    pub(super) fn post(&mut self, req: Req, p: &Post) -> MarketEvent {
        if p.fee_rev != self.state.fees.fee_rev {
            return Self::done(req, MarketCode::FeeChanged, None);
        }
        if !self.price_ok(p.mode, p.price.as_ref()) || !self.days_ok(p.days) || !note_ok(&p.note) {
            return Self::done(req, MarketCode::Other(99), None);
        }
        if self.slots_used() >= usize::from(self.state.max_listings) {
            return Self::done(req, MarketCode::MarketLimit, None);
        }
        let (goods, group, item) = match &p.goods {
            // From the stash the sell slot showed, or the trading side's.
            PostGoods::Item(id) => {
                let held = &mut self.trade.stash.items;
                let named = held
                    .iter()
                    .position(|i| i.item_id == *id)
                    .map(|k| held.remove(k));
                let seen = self.stash.remove(id);
                if named.is_some() || seen.is_some() {
                    self.trade.room += 1;
                }
                match (seen, named) {
                    (Some(g), _) => (
                        Goods::Item {
                            item_id: g.item_id,
                            record: g.record,
                        },
                        g.group,
                        g.item,
                    ),
                    (None, Some(n)) => (
                        Goods::Item {
                            item_id: n.item_id,
                            record: n.record,
                        },
                        n.group,
                        n.item,
                    ),
                    (None, None) => return Self::done(req, MarketCode::ItemNotFound, None),
                }
            }
            PostGoods::Stack { code, count } => {
                let held = self.trade.stash.stacks.get(code).copied().unwrap_or(0);
                if *count == 0 || !self.state.currencies.contains(code) || held < *count {
                    return Self::done(req, MarketCode::ItemNotFound, None);
                }
                self.trade.stash.stacks.insert(code.clone(), held - count);
                let item = ItemView {
                    name: code.clone(),
                    base: code.clone(),
                    code: code.clone(),
                    item_type: "rune".into(),
                    identified: true,
                    ..ItemView::default()
                };
                let goods = Goods::Stack {
                    code: code.clone(),
                    count: *count,
                };
                (goods, Group::Stackable, item)
            }
        };
        let listing = self.new_listing();
        let now = browse::NOW_MS;
        let row = ListingRow {
            listing,
            group,
            goods,
            mode: p.mode,
            price_hash: p.price.as_ref().map(browse::price_hash).unwrap_or_default(),
            price: p.price.clone(),
            seller: ME.into(),
            seller_sales: 0,
            posted_ms: now,
            expires_ms: now + u64::from(p.days) * DAY_MS,
            note: p.note.clone(),
            offers: 0,
            mine: true,
            item,
        };
        self.shop.push(ShopRow {
            row,
            state: ListingState::Active,
            hidden: p.hidden,
            live_offers: 0,
            newest_offer_ms: 0,
            sold: None,
            relistable: false,
        });
        self.sync(self.shop.len() - 1);
        Self::done(req, MarketCode::Done, Some(listing))
    }

    /// 0xBC: `op` on each named listing; a row that fails gets its code,
    /// the others still happen.
    pub(super) fn shop_edit(
        &mut self,
        req: Req,
        op: &ShopOp,
        listings: &[ListingId],
        fee_rev: u32,
    ) -> MarketEvent {
        if fee_rev != self.state.fees.fee_rev {
            return Self::done(req, MarketCode::FeeChanged, None);
        }
        let bad = match op {
            ShopOp::Relist { days } => !self.days_ok(*days),
            ShopOp::Note(n) => !note_ok(n),
            ShopOp::Reprice { mode, price } => !self.price_ok(*mode, price.as_ref()),
            _ => false,
        };
        if bad || listings.is_empty() || listings.len() > EDIT_MAX {
            return Self::done(req, MarketCode::Other(99), None);
        }
        let rows = listings.iter().map(|id| self.edit_row(op, *id)).collect();
        let MarketEvent::Done(mut d) = Self::done(req, MarketCode::Done, None) else {
            unreachable!("done is a Done");
        };
        d.rows = rows;
        MarketEvent::Done(d)
    }

    /// `op` on listing `id`: its result (a relisted closed listing: the
    /// new listing's ID).
    fn edit_row(&mut self, op: &ShopOp, id: ListingId) -> (u64, MarketCode) {
        let Some(i) = self.shop.iter().position(|r| r.row.listing == id) else {
            let code = if self.listing(id).is_some() {
                MarketCode::NotYours
            } else {
                MarketCode::ListingGone
            };
            return (id, code);
        };
        let now = browse::NOW_MS;
        let active = self.shop[i].state == ListingState::Active;
        let r = &mut self.shop[i];
        match op {
            _ if !active && !matches!(op, ShopOp::Relist { .. }) => {
                return (id, MarketCode::ListingGone)
            }
            ShopOp::Cancel => {
                r.state = ListingState::Cancelled;
                r.live_offers = 0;
                r.row.offers = 0;
                // Its live offers go back to their buyers.
                for o in self.trade.offers.iter_mut() {
                    if o.listing == id && o.state.live() {
                        o.state = OfferState::Returned;
                        o.answered_ms = now;
                    }
                }
                self.close_to_deliveries(i, DeliverySource::Cancelled, 0);
            }
            ShopOp::Relist { days } if active => {
                r.row.expires_ms = now + u64::from(*days) * DAY_MS;
            }
            ShopOp::Relist { days } => {
                // Sold, or its goods claimed from Deliveries: nothing to list.
                let sold = r.state == ListingState::Sold;
                let Some(waiting) = self.closed_goods(id).filter(|_| !sold) else {
                    return (id, MarketCode::ItemNotFound);
                };
                if self.slots_used() >= usize::from(self.state.max_listings) {
                    return (id, MarketCode::MarketLimit);
                }
                self.trade.deliveries.remove(waiting);
                let mut row = self.shop[i].row.clone();
                let listing = self.new_listing();
                row.listing = listing;
                row.posted_ms = now;
                row.expires_ms = now + u64::from(*days) * DAY_MS;
                row.offers = 0;
                self.shop.push(ShopRow {
                    row,
                    state: ListingState::Active,
                    hidden: false,
                    live_offers: 0,
                    newest_offer_ms: 0,
                    sold: None,
                    relistable: false,
                });
                self.sync(self.shop.len() - 1);
                return (listing, MarketCode::Done);
            }
            ShopOp::Hide => r.hidden = true,
            ShopOp::Show => r.hidden = false,
            ShopOp::Note(n) => r.row.note = n.clone(),
            ShopOp::Reprice { mode, price } => {
                r.row.mode = *mode;
                r.row.price_hash = price.as_ref().map(browse::price_hash).unwrap_or_default();
                r.row.price = price.clone();
            }
        }
        self.sync(i);
        (id, MarketCode::Done)
    }

    /// Which tab shop row `r` is in.
    fn tab_of(r: &ShopRow) -> ShopTab {
        match r.state {
            ListingState::Active if r.hidden => ShopTab::Hidden,
            ListingState::Active => ShopTab::Shown,
            ListingState::Sold => ShopTab::Sold,
            ListingState::Cancelled | ListingState::Expired => ShopTab::Expired,
        }
    }

    /// 0xBD: page `page` of tab `tab`, with the four counts and the slots.
    pub(super) fn shop_page(
        &self,
        tab: ShopTab,
        page: u16,
        sort: ShopSort,
        desc: bool,
    ) -> ShopPage {
        let mut counts = [0u32; 4];
        for r in &self.shop {
            counts[usize::from(Self::tab_of(r).code())] += 1;
        }
        let mut rows: Vec<&ShopRow> = self
            .shop
            .iter()
            .filter(|r| Self::tab_of(r) == tab)
            .collect();
        rows.sort_by(|a, b| {
            let o = shop_order(sort, a, b);
            let o = if desc { o.reverse() } else { o };
            o.then(a.row.listing.cmp(&b.row.listing))
        });
        ShopPage {
            tab,
            page,
            counts,
            slots_used: self.slots_used() as u16,
            slots_max: self.state.max_listings,
            now_ms: browse::NOW_MS,
            rows: rows
                .into_iter()
                .skip(usize::from(page) * SHOP_PAGE)
                .take(SHOP_PAGE)
                .map(|r| self.shop_row(r))
                .collect(),
        }
    }

    /// Shop row `r` as a page shows it: its live offers counted from the
    /// trading side (the made-up ones of the seed kept), relistable while
    /// its goods wait in Deliveries.
    fn shop_row(&self, r: &ShopRow) -> ShopRow {
        let mut r = r.clone();
        let id = r.row.listing;
        let live = self
            .trade
            .offers
            .iter()
            .filter(|o| o.listing == id && o.state.live());
        for o in live {
            r.live_offers += 1;
            r.newest_offer_ms = r.newest_offer_ms.max(o.at_ms);
        }
        r.relistable = matches!(r.state, ListingState::Cancelled | ListingState::Expired)
            && self.closed_goods(id).is_some();
        r
    }

    /// A made-up shop in every tab (offscreen shots and the app's fake
    /// mode): listings shown, one hidden, some sold, expired and
    /// cancelled.
    pub fn seed_shop(&mut self, names: &dyn Fn(&str) -> Option<String>) {
        let all = browse::listings(names);
        let now = browse::NOW_MS;
        let rune = |c: &str, n: u32| (c.to_owned(), n);
        let plan: [Seed; 10] = [
            (
                0,
                ListingState::Active,
                false,
                ListingMode::OffersOnly,
                None,
                "no lowballs",
                2,
            ),
            (
                1,
                ListingState::Active,
                false,
                ListingMode::Price,
                Some(Price {
                    stacks: vec![rune("r20", 1), rune("r23", 2)],
                    ..Price::default()
                }),
                "",
                0,
            ),
            (
                9,
                ListingState::Active,
                false,
                ListingMode::PriceAndOffers,
                Some(Price::gold(60_000)),
                "",
                1,
            ),
            (
                16,
                ListingState::Active,
                false,
                ListingMode::Price,
                Some(Price {
                    gold: 25_000,
                    stacks: vec![rune("r15", 1)],
                    ..Price::default()
                }),
                "pick up in Harrogath",
                0,
            ),
            (
                4,
                ListingState::Active,
                true,
                ListingMode::Price,
                Some(Price {
                    stacks: vec![rune("r24", 1)],
                    ..Price::default()
                }),
                "",
                0,
            ),
            (
                12,
                ListingState::Sold,
                false,
                ListingMode::Price,
                Some(Price {
                    stacks: vec![rune("r25", 1)],
                    ..Price::default()
                }),
                "",
                0,
            ),
            (
                14,
                ListingState::Sold,
                false,
                ListingMode::Price,
                Some(Price::gold(95_000)),
                "",
                0,
            ),
            (
                18,
                ListingState::Sold,
                false,
                ListingMode::PriceAndOffers,
                Some(Price::gold(40_000)),
                "",
                0,
            ),
            (
                10,
                ListingState::Expired,
                false,
                ListingMode::Price,
                Some(Price::gold(150_000)),
                "",
                0,
            ),
            (
                20,
                ListingState::Cancelled,
                false,
                ListingMode::OffersOnly,
                None,
                "",
                0,
            ),
        ];
        for (k, (t, state, hidden, mode, price, note, offers)) in plan.into_iter().enumerate() {
            let mut row = all[t].clone();
            row.listing = self.new_listing();
            row.mine = true;
            row.seller = ME.into();
            row.mode = mode;
            row.price_hash = price.as_ref().map(browse::price_hash).unwrap_or_default();
            row.price = price.clone();
            row.note = note.into();
            row.offers = offers;
            row.posted_ms = now - (k as u64 + 1) * 7 * 3600 * 1000;
            row.expires_ms = match state {
                ListingState::Active => row.posted_ms + 7 * DAY_MS - (k as u64 % 3) * 2 * DAY_MS,
                _ => now - (k as u64) * 3600 * 1000,
            };
            if k == 3 {
                // One that runs out today.
                row.expires_ms = now + 5 * 3600 * 1000;
            }
            let sold = (state == ListingState::Sold).then(|| Sold {
                buyer: ["Varkas", "Ilse", "Orab"][k % 3].into(),
                paid: price.clone().unwrap_or_default(),
                sale_fee: 0,
                closed_ms: now - (k as u64) * 5 * 3600 * 1000,
            });
            self.shop.push(ShopRow {
                row,
                state,
                hidden,
                live_offers: offers,
                newest_offer_ms: if offers > 0 { now - 3600 * 1000 } else { 0 },
                sold,
                relistable: false,
            });
            let i = self.shop.len() - 1;
            // A closed one's goods wait in Deliveries.
            match state {
                ListingState::Cancelled => {
                    self.close_to_deliveries(i, DeliverySource::Cancelled, k as u64)
                }
                ListingState::Expired => {
                    self.close_to_deliveries(i, DeliverySource::Expired, k as u64)
                }
                _ => {}
            }
            self.sync(i);
        }
    }
}

/// Two shop rows in `sort`'s order (ascending; ties are the caller's).
pub fn shop_order(sort: ShopSort, a: &ShopRow, b: &ShopRow) -> Ordering {
    match sort {
        ShopSort::Posted => a.row.posted_ms.cmp(&b.row.posted_ms),
        ShopSort::Ends => a.row.expires_ms.cmp(&b.row.expires_ms),
        ShopSort::Gold => {
            let g = |r: &ShopRow| r.row.price.as_ref().map_or(0, |p| p.gold);
            g(a).cmp(&g(b))
        }
        ShopSort::Name => a.row.item.name.cmp(&b.row.item.name),
    }
}

#[cfg(test)]
mod tests {
    use super::super::FEE_REV;
    use super::*;
    use crate::market::source::{MarketAsk, MarketDone, MarketSource, StashGoods};

    fn fake() -> FakeSource {
        FakeSource::new(&|_| None, false, 0, true)
    }

    fn answer(src: &mut FakeSource, ask: MarketAsk) -> MarketDone {
        src.send(ask);
        match src.poll().pop() {
            Some(MarketEvent::Done(d)) => d,
            e => panic!("not a Done: {e:?}"),
        }
    }

    fn goods(id: u32) -> StashGoods {
        StashGoods {
            item_id: id,
            group: Group::Unique,
            record: Vec::new(),
            item: ItemView {
                name: "The Gnasher".into(),
                ..ItemView::default()
            },
        }
    }

    fn post(id: u32, hidden: bool) -> MarketAsk {
        MarketAsk::Post(Post {
            goods: PostGoods::Item(id),
            mode: ListingMode::Price,
            price: Some(Price::gold(1_000)),
            days: 7,
            note: String::new(),
            hidden,
            fee_rev: FEE_REV,
        })
    }

    #[test]
    fn fake_post_takes_the_stash_item_into_escrow() {
        let mut src = fake();
        let own = src.state.active_listings;
        // An item the fake was never shown is not in the stash.
        assert_eq!(
            answer(&mut src, post(7, false)).code,
            MarketCode::ItemNotFound
        );
        src.stash_item_seen(&goods(7));
        src.stash_item_seen(&goods(8));
        let d = answer(&mut src, post(7, false));
        assert_eq!(d.code, MarketCode::Done);
        let shown = d.id.unwrap();
        assert!(src.listing(shown).is_some_and(|l| l.mine));
        assert!(!src.stash.contains_key(&7), "in escrow");
        // Twice: gone from the stash.
        assert_eq!(
            answer(&mut src, post(7, false)).code,
            MarketCode::ItemNotFound
        );
        // Posted hidden: in My shop, not in the search.
        let hidden = answer(&mut src, post(8, true)).id.unwrap();
        assert!(src.listing(hidden).is_none());
        assert_eq!(src.state.active_listings, own + 2);
        // Fees changed since the asker looked: refused.
        src.stash_item_seen(&goods(9));
        let MarketAsk::Post(mut p) = post(9, false) else {
            unreachable!()
        };
        p.fee_rev = FEE_REV + 1;
        assert_eq!(
            answer(&mut src, MarketAsk::Post(p)).code,
            MarketCode::FeeChanged
        );
        // Offers only with a price, or a bad length: refused.
        let MarketAsk::Post(mut p) = post(9, false) else {
            unreachable!()
        };
        p.mode = ListingMode::OffersOnly;
        assert_eq!(
            answer(&mut src, MarketAsk::Post(p.clone())).code,
            MarketCode::Other(99)
        );
        p.price = None;
        p.days = 8;
        assert_eq!(
            answer(&mut src, MarketAsk::Post(p.clone())).code,
            MarketCode::Other(99)
        );
        p.days = 1;
        assert_eq!(answer(&mut src, MarketAsk::Post(p)).code, MarketCode::Done);
    }

    #[test]
    fn fake_shop_edits_answer_per_row() {
        let mut src = fake();
        src.seed_shop(&|_| None);
        let id_of = |src: &FakeSource, s: ListingState| {
            src.shop.iter().find(|r| r.state == s).unwrap().row.listing
        };
        let expired = id_of(&src, ListingState::Expired);
        let sold = id_of(&src, ListingState::Sold);
        let shown = src
            .shop
            .iter()
            .find(|r| r.state == ListingState::Active && !r.hidden)
            .unwrap()
            .row
            .listing;
        // Someone else's listing in the search.
        let theirs = src.listings.iter().find(|l| !l.mine).unwrap().listing;
        let waiting = src.state.deliveries_waiting;
        let d = answer(
            &mut src,
            MarketAsk::ShopEdit {
                op: ShopOp::Relist { days: 3 },
                listings: vec![expired, sold, theirs, shown],
                fee_rev: FEE_REV,
            },
        );
        assert_eq!(d.code, MarketCode::Done);
        assert_eq!(d.rows.len(), 4);
        // The expired one is listed anew under a new ID, its goods taken
        // back from Deliveries.
        let (new, code) = d.rows[0].clone();
        assert_eq!(code, MarketCode::Done);
        assert_ne!(new, expired);
        assert!(src.listing(new).is_some());
        assert_eq!(src.state.deliveries_waiting, waiting - 1);
        assert_eq!(d.rows[1], (sold, MarketCode::ItemNotFound));
        assert_eq!(d.rows[2], (theirs, MarketCode::NotYours));
        // The shown one runs 3 more days from now.
        assert_eq!(d.rows[3], (shown, MarketCode::Done));
        // Cancel: the goods go to Deliveries, the listing leaves the search.
        let d = answer(
            &mut src,
            MarketAsk::ShopEdit {
                op: ShopOp::Cancel,
                listings: vec![shown],
                fee_rev: FEE_REV,
            },
        );
        assert_eq!(d.rows, [(shown, MarketCode::Done)]);
        assert!(src.listing(shown).is_none());
        // More than 50 rows, or none: the whole edit is refused.
        let d = answer(
            &mut src,
            MarketAsk::ShopEdit {
                op: ShopOp::Hide,
                listings: vec![shown; EDIT_MAX + 1],
                fee_rev: FEE_REV,
            },
        );
        assert_eq!(d.code, MarketCode::Other(99));
    }

    #[test]
    fn fake_shop_page_sorts_and_counts() {
        let mut src = fake();
        let own = src.trade.own.len();
        src.seed_shop(&|_| None);
        let p = src.shop_page(ShopTab::Shown, 0, ShopSort::Gold, false);
        assert_eq!(p.counts, [4 + own as u32, 1, 3, 2]);
        assert_eq!(usize::from(p.slots_used), 5 + own);
        assert_eq!(p.rows.len(), 4 + own);
        for w in p.rows.windows(2) {
            assert_ne!(shop_order(ShopSort::Gold, &w[0], &w[1]), Ordering::Greater);
        }
        let names = src.shop_page(ShopTab::Sold, 0, ShopSort::Name, true);
        for w in names.rows.windows(2) {
            assert_ne!(shop_order(ShopSort::Name, &w[0], &w[1]), Ordering::Less);
        }
    }

    /// My shop and the trading side tell one story: the trading side's own
    /// listings are shop rows with their live offers, a sale there moves
    /// the row to Sold, a cancel's goods are a Deliveries row, and once
    /// that row is claimed the listing can't be listed again.
    #[test]
    fn fake_shop_and_trading_side_agree() {
        let mut src = fake();
        let own: Vec<ListingId> = src.trade.own.iter().map(|l| l.listing).collect();
        assert!(!own.is_empty());
        let shown = src.shop_page(ShopTab::Shown, 0, ShopSort::Posted, false);
        for id in &own {
            assert!(shown.rows.iter().any(|r| r.row.listing == *id));
        }
        let offered = shown.rows.iter().filter(|r| r.live_offers > 0).count();
        assert!(offered > 0, "the trading side's offers count");
        assert_eq!(usize::from(src.state.active_listings), own.len());
        // Sold through the trading side: Sold in My shop.
        src.sell_mine(own[0], "Orab");
        let sold = src.shop_page(ShopTab::Sold, 0, ShopSort::Posted, false);
        let row = sold.rows.iter().find(|r| r.row.listing == own[0]).unwrap();
        assert_eq!(row.sold.as_ref().map(|s| s.buyer.as_str()), Some("Orab"));
        assert_eq!(usize::from(src.state.active_listings), own.len() - 1);
        // Cancelled: its goods wait in Deliveries, relistable.
        let waiting = src.state.deliveries_waiting;
        let d = answer(
            &mut src,
            MarketAsk::ShopEdit {
                op: ShopOp::Cancel,
                listings: vec![own[1]],
                fee_rev: FEE_REV,
            },
        );
        assert_eq!(d.rows, [(own[1], MarketCode::Done)]);
        assert_eq!(src.state.deliveries_waiting, waiting + 1);
        assert!(src.trade.own.iter().all(|l| l.listing != own[1]));
        let gone = src.shop_page(ShopTab::Expired, 0, ShopSort::Posted, false);
        let row = gone.rows.iter().find(|r| r.row.listing == own[1]).unwrap();
        assert!(row.relistable);
        // Claimed into the stash: nothing left to list.
        let delivery = src
            .trade
            .deliveries
            .iter()
            .find(|d| d.listing == Some(own[1]))
            .unwrap()
            .delivery;
        let d = answer(
            &mut src,
            MarketAsk::Claim {
                all: false,
                deliveries: vec![delivery],
            },
        );
        assert_eq!(d.rows, [(delivery, MarketCode::Done)]);
        let d = answer(
            &mut src,
            MarketAsk::ShopEdit {
                op: ShopOp::Relist { days: 7 },
                listings: vec![own[1]],
                fee_rev: FEE_REV,
            },
        );
        assert_eq!(d.rows, [(own[1], MarketCode::ItemNotFound)]);
    }
}
