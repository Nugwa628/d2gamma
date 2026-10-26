// D2RUST's own: the fake market's offers, Deliveries and claims.
//! The trading side of [`super::FakeSource`], with the contract's rules
//! (`docs/plans/server/market-contract.md` 3, 3.3 and 5): offers are paid
//! into escrow from the account's stash and checked against it; the seller
//! accepts, declines or counters an open offer (a counter keeps only some
//! of the offered items); the buyer accepts a counter or withdraws; what
//! moves goes to Deliveries, one row per thing; a claim moves deliveries
//! into the stash row by row, and a row that does not fit stays with
//! `StashFull`. Notices go to the asker for what happens to its own
//! listings and offers, and the state is pushed when its counts change.
//!
//! The asker's character is [`ME`]; its own listings ([`Trade::own`]) are
//! not in the search. Everything is made up in D2RUST's own words; game
//! data, when there is some, names the base items and the runes.

use crate::market::source::{
    Goods, Group, ItemView, ListingId, ListingMode, ListingRow, MarketAsk, MarketCode, MarketEvent,
    Price, Req,
};
use crate::market::trade::{
    AnswerHow, DeliveriesPage, DeliveryReason, DeliveryRow, DeliverySource, Holdings, NamedItem,
    Notice, NoticeWhat, OfferRow, OfferState, OffersPage, OffersWhich, CLAIM_MAX, PAGE_ROWS,
};

use super::browse::{price_hash, NOW_MS};
use super::FakeSource;

/// The asker's character.
pub const ME: &str = "Wanderer";

/// Items one offer may name (the realm's `market_offer_items`).
pub const OFFER_ITEMS: usize = 10;

/// Gold the stash holds at most (the realm's `stash_gold_max`).
pub const GOLD_MAX: u64 = 2_500_000;

const HOUR_MS: u64 = 3600 * 1000;

/// The trading side's state.
#[derive(Clone, Debug)]
pub struct Trade {
    /// The asker's listings (`mine`), not in the search.
    pub own: Vec<ListingRow>,
    pub offers: Vec<OfferRow>,
    pub deliveries: Vec<DeliveryRow>,
    /// The account's online shared stash.
    pub stash: Holdings,
    /// Free item places left in the stash.
    pub room: usize,
    next_id: u64,
}

/// A made-up item.
#[allow(clippy::too_many_arguments)]
fn named(
    id: u32,
    name: &str,
    code: &str,
    base: &str,
    item_type: &str,
    group: Group,
    req: u8,
    stats: &[&str],
    names: &dyn Fn(&str) -> Option<String>,
) -> NamedItem {
    let base = names(code).unwrap_or_else(|| base.to_owned());
    NamedItem {
        item_id: id,
        record: Vec::new(),
        group,
        item: ItemView {
            name: if group == Group::Other {
                base.clone()
            } else {
                name.to_owned()
            },
            base,
            code: code.to_owned(),
            item_type: item_type.to_owned(),
            level_req: req,
            sockets: 0,
            ethereal: false,
            identified: true,
            stats: stats.iter().map(|s| (*s).to_owned()).collect(),
        },
    }
}

fn stacks(list: &[(&str, u32)]) -> Vec<(String, u32)> {
    let mut v: Vec<(String, u32)> = list.iter().map(|(c, n)| ((*c).to_owned(), *n)).collect();
    v.sort();
    v
}

fn price(gold: u64, list: &[(&str, u32)], items: &[u32]) -> Price {
    Price {
        gold,
        stacks: stacks(list),
        items: items.to_vec(),
    }
}

/// A listing of the asker's.
fn own_listing(
    id: ListingId,
    item: NamedItem,
    mode: ListingMode,
    p: Option<Price>,
    note: &str,
) -> ListingRow {
    let posted_ms = NOW_MS - 20 * HOUR_MS - id % 7 * HOUR_MS;
    ListingRow {
        listing: id,
        group: item.group,
        goods: Goods::Item {
            item_id: item.item_id,
            record: item.record,
        },
        mode,
        price_hash: p.as_ref().map(price_hash).unwrap_or_default(),
        price: p,
        seller: ME.into(),
        seller_sales: 12,
        posted_ms,
        expires_ms: posted_ms + 7 * 24 * HOUR_MS,
        note: note.into(),
        offers: 0,
        mine: true,
        item: item.item,
    }
}

impl Trade {
    /// The fake's trading side over the browse listings `browse` (the
    /// asker's offers are on two of them).
    pub fn new(browse: &[ListingRow], names: &dyn Fn(&str) -> Option<String>) -> Trade {
        let n = |id, name, code, base, ty, g, req, stats: &[&str]| {
            named(id, name, code, base, ty, g, req, stats, names)
        };
        let stash = Holdings {
            gold: 600_000,
            stacks: stacks(&[
                ("r15", 4),
                ("r20", 2),
                ("r23", 3),
                ("r24", 5),
                ("r25", 1),
                ("r26", 2),
                ("r29", 1),
            ])
            .into_iter()
            .collect(),
            items: vec![
                n(
                    7001,
                    "Azure Jewel of Haste",
                    "jew",
                    "Jewel",
                    "jewl",
                    Group::Magic,
                    30,
                    &["+15% Increased Attack Speed", "Cold Resist +12%"],
                ),
                n(
                    7002,
                    "Gloom Band",
                    "rin",
                    "Ring",
                    "ring",
                    Group::Rare,
                    41,
                    &["+10% Faster Cast Rate", "+22 To Mana", "Fire Resist +19%"],
                ),
                n(
                    7003,
                    "Small Charm of Vigor",
                    "cm1",
                    "Small Charm",
                    "scha",
                    Group::Magic,
                    42,
                    &["+20 To Life", "All Resistances +5"],
                ),
                n(
                    7004,
                    "Shield",
                    "uit",
                    "Shield",
                    "shie",
                    Group::Other,
                    54,
                    &["Defense: 148", "4 Sockets"],
                ),
                n(
                    7005,
                    "Korrin's Grip",
                    "ulg",
                    "Gloves",
                    "glov",
                    Group::Set,
                    50,
                    &["+30% Increased Attack Speed", "5% Life Stolen Per Hit"],
                ),
            ],
        };
        let own = vec![
            own_listing(
                9001,
                n(
                    6001,
                    "Spidersilk Sash",
                    "zvb",
                    "Belt",
                    "belt",
                    Group::Unique,
                    67,
                    &[
                        "+1 To All Skills",
                        "+20% Faster Cast Rate",
                        "+5% Maximum Mana",
                    ],
                ),
                ListingMode::OffersOnly,
                None,
                "no lowballs",
            ),
            own_listing(
                9002,
                n(
                    6002,
                    "Grimward Visage",
                    "uhm",
                    "Helm",
                    "helm",
                    Group::Unique,
                    69,
                    &["+2 To Necromancer Skill Levels", "30% Faster Cast Rate"],
                ),
                ListingMode::PriceAndOffers,
                Some(price(0, &[("r26", 2), ("r29", 1)], &[])),
                "",
            ),
            own_listing(
                9003,
                n(
                    6003,
                    "Korrin's Coil",
                    "zvb",
                    "Belt",
                    "belt",
                    Group::Set,
                    60,
                    &["+10% Faster Hit Recovery", "+40 To Life"],
                ),
                ListingMode::Price,
                Some(Price::gold(60_000)),
                "",
            ),
        ];
        let mut t = Trade {
            own,
            offers: Vec::new(),
            deliveries: Vec::new(),
            stash,
            room: 10,
            next_id: 100,
        };
        // Offers on the asker's listings.
        let jewel = n(
            8101,
            "Sapphire Jewel of Fervor",
            "jew",
            "Jewel",
            "jewl",
            Group::Magic,
            39,
            &["+15% Increased Attack Speed", "+30% Enhanced Damage"],
        );
        t.offer_on_mine(
            9001,
            "Necrolord",
            price(0, &[("r15", 3), ("r24", 1)], &[8101]),
            vec![jewel],
            2,
        );
        t.offer_on_mine(9001, "Ilse", Price::gold(120_000), Vec::new(), 5);
        t.offer_on_mine(
            9002,
            "Varkas",
            price(0, &[("r23", 1), ("r26", 1)], &[]),
            Vec::new(),
            26,
        );
        // The asker's offers: one countered, one open.
        let mut takes = browse.iter().filter(|l| l.mode.takes_offers() && !l.mine);
        if let Some(l) = takes.next() {
            let id = t.made(l, price(0, &[("r24", 1)], &[]), 9);
            let o = t.offers.iter_mut().find(|o| o.offer == id).expect("made");
            o.state = OfferState::Countered;
            o.counter = Some(price(0, &[("r20", 1), ("r24", 1)], &[]));
            o.answered_ms = NOW_MS - 3 * HOUR_MS;
        }
        if let Some(l) = takes.next() {
            t.made(l, Price::gold(50_000), 30);
        }
        // Deliveries: a sale's two currencies, a buy, an expired listing
        // and a declined offer's refund.
        let mantle = n(
            6010,
            "Duskweave Mantle",
            "uea",
            "Hide",
            "tors",
            Group::Unique,
            58,
            &["+1 To All Skills", "15% Faster Hit Recovery"],
        );
        for (code, count) in [("r29", 1), ("r26", 2)] {
            let goods = Goods::Stack {
                code: code.into(),
                count,
            };
            t.deliver(
                DeliverySource::Sold,
                Some(8800),
                None,
                &mantle.item.name,
                goods,
                None,
                1,
            );
        }
        let crown = n(
            6011,
            "Ashen Crown",
            "uap",
            "Cap",
            "helm",
            Group::Unique,
            62,
            &["+2 To All Skills", "Damage Reduced By 10%"],
        );
        t.deliver_item(DeliverySource::Bought, Some(8801), None, crown, 3);
        let vigil = n(
            6012,
            "Vessa's Vigil",
            "ci3",
            "Circlet",
            "circ",
            Group::Set,
            65,
            &["+1 To All Skills", "Cold Resist +30%"],
        );
        t.deliver_item(DeliverySource::Expired, Some(8802), None, vigil, 24);
        let goods = Goods::Stack {
            code: "r24".into(),
            count: 1,
        };
        t.deliver(
            DeliverySource::Returned,
            Some(8803),
            Some(77),
            "Stormcall",
            goods,
            None,
            48,
        );
        t
    }

    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Someone's offer on the asker's listing `listing`, `hours` ago.
    fn offer_on_mine(
        &mut self,
        listing: ListingId,
        buyer: &str,
        offered: Price,
        items: Vec<NamedItem>,
        hours: u64,
    ) -> u64 {
        let id = self.id();
        let Some(l) = self.own.iter_mut().find(|l| l.listing == listing) else {
            return id;
        };
        l.offers += 1;
        self.offers.push(OfferRow {
            offer: id,
            listing,
            goods: l.goods.clone(),
            group: l.group,
            item: l.item.clone(),
            price: l.price.clone(),
            buyer: buyer.into(),
            seller: ME.into(),
            offered,
            items,
            counter: None,
            state: OfferState::Open,
            at_ms: NOW_MS - hours * HOUR_MS,
            answered_ms: 0,
        });
        id
    }

    /// The asker's offer on `l`, `hours` ago, paid from the stash.
    fn made(&mut self, l: &ListingRow, offered: Price, hours: u64) -> u64 {
        let items = self.take(&offered);
        let id = self.id();
        self.offers.push(OfferRow {
            offer: id,
            listing: l.listing,
            goods: l.goods.clone(),
            group: l.group,
            item: l.item.clone(),
            price: l.price.clone(),
            buyer: ME.into(),
            seller: l.seller.clone(),
            offered,
            items,
            counter: None,
            state: OfferState::Open,
            at_ms: NOW_MS - hours * HOUR_MS,
            answered_ms: 0,
        });
        id
    }

    /// Takes `p` out of the stash (it holds it); the items taken.
    fn take(&mut self, p: &Price) -> Vec<NamedItem> {
        self.stash.gold -= p.gold;
        for (c, n) in &p.stacks {
            if let Some(have) = self.stash.stacks.get_mut(c) {
                *have -= n;
            }
        }
        self.stash.stacks.retain(|_, n| *n > 0);
        let mut taken = Vec::new();
        self.stash.items.retain(|i| {
            if p.items.contains(&i.item_id) {
                taken.push(i.clone());
                false
            } else {
                true
            }
        });
        self.room += taken.len();
        taken
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn deliver(
        &mut self,
        source: DeliverySource,
        listing: Option<ListingId>,
        offer: Option<u64>,
        about: &str,
        goods: Goods,
        item: Option<(Group, ItemView)>,
        hours: u64,
    ) {
        if matches!(goods, Goods::Gold(0)) {
            return;
        }
        let delivery = self.id();
        let reason = self.reason_of(source, offer);
        self.deliveries.push(DeliveryRow {
            delivery,
            source,
            reason,
            listing,
            offer,
            about: about.into(),
            goods,
            item,
            at_ms: NOW_MS - hours * HOUR_MS,
        });
    }

    /// Why a delivery of `source` came back, as the realm reads it: from the
    /// source, and for an offer's goods from the offer's state.
    fn reason_of(&self, source: DeliverySource, offer: Option<u64>) -> DeliveryReason {
        match source {
            DeliverySource::Cancelled => DeliveryReason::Cancelled,
            DeliverySource::Expired => DeliveryReason::Expired,
            DeliverySource::Change => DeliveryReason::Change,
            DeliverySource::Returned => {
                match self
                    .offers
                    .iter()
                    .find(|o| Some(o.offer) == offer)
                    .map(|o| o.state)
                {
                    Some(OfferState::Declined) => DeliveryReason::Declined,
                    Some(OfferState::Withdrawn) => DeliveryReason::Withdrawn,
                    Some(OfferState::Returned) => DeliveryReason::Outbid,
                    _ => DeliveryReason::None,
                }
            }
            _ => DeliveryReason::None,
        }
    }

    fn deliver_item(
        &mut self,
        source: DeliverySource,
        listing: Option<ListingId>,
        offer: Option<u64>,
        it: NamedItem,
        hours: u64,
    ) {
        let about = it.item.name.clone();
        let goods = Goods::Item {
            item_id: it.item_id,
            record: it.record,
        };
        self.deliver(
            source,
            listing,
            offer,
            &about,
            goods,
            Some((it.group, it.item)),
            hours,
        );
    }

    /// `p` (its gold, currencies and the items of `items` it names) into
    /// Deliveries, one row each.
    fn deliver_price(
        &mut self,
        source: DeliverySource,
        o: (ListingId, Option<u64>),
        about: &str,
        p: &Price,
        items: &[NamedItem],
    ) {
        self.deliver(source, Some(o.0), o.1, about, Goods::Gold(p.gold), None, 0);
        for (code, count) in &p.stacks {
            let goods = Goods::Stack {
                code: code.clone(),
                count: *count,
            };
            if *count > 0 {
                self.deliver(source, Some(o.0), o.1, about, goods, None, 0);
            }
        }
        for it in items.iter().filter(|i| p.items.contains(&i.item_id)) {
            self.deliver_item(source, Some(o.0), o.1, it.clone(), 0);
        }
    }

    /// What `counter` leaves of `offered` (the change), and what it asks
    /// on top (paid from the stash): gold and currencies; items it does
    /// not keep go back as change.
    fn split(offered: &Price, counter: &Price) -> (Price, Price) {
        let mut change = Price::default();
        let mut extra = Price::default();
        if offered.gold >= counter.gold {
            change.gold = offered.gold - counter.gold;
        } else {
            extra.gold = counter.gold - offered.gold;
        }
        let mut codes: Vec<&String> = offered
            .stacks
            .iter()
            .chain(&counter.stacks)
            .map(|(c, _)| c)
            .collect();
        codes.sort();
        codes.dedup();
        let count =
            |p: &Price, c: &str| p.stacks.iter().find(|(x, _)| x == c).map_or(0, |(_, n)| *n);
        for c in codes {
            let (o, k) = (count(offered, c), count(counter, c));
            if o > k {
                change.stacks.push((c.clone(), o - k));
            } else if k > o {
                extra.stacks.push((c.clone(), k - o));
            }
        }
        change.items = offered
            .items
            .iter()
            .copied()
            .filter(|i| !counter.items.contains(i))
            .collect();
        (change, extra)
    }
}

impl FakeSource {
    /// The state's counts from the trading side.
    pub(super) fn sync_counts(&mut self) {
        let t = &self.trade;
        let live = |w: &str| {
            t.offers
                .iter()
                .filter(|o| {
                    o.state.live()
                        && (if w == "to" {
                            o.seller == ME
                        } else {
                            o.buyer == ME
                        })
                })
                .count() as u16
        };
        self.state.deliveries_waiting = t.deliveries.len() as u16;
        self.state.live_offers_to_me = live("to");
        self.state.my_live_offers = live("by");
        self.state.active_listings = t.own.len() as u16;
    }

    /// The counts changed: the state is pushed (`req` 0).
    fn push_state(&mut self) {
        self.sync_counts();
        self.events.push_back(MarketEvent::State {
            req: 0,
            state: self.state.clone(),
        });
    }

    /// A notice to the asker.
    fn notify(&mut self, what: NoticeWhat, o: &OfferRow, who: &str, price: Option<Price>) {
        self.sync_counts();
        self.events.push_back(MarketEvent::Notice(Notice {
            what,
            listing: o.listing,
            offer: Some(o.offer),
            about: o.item.name.clone(),
            who: who.into(),
            price,
            deliveries_waiting: self.state.deliveries_waiting,
            at_ms: NOW_MS,
        }));
    }

    /// A buy of the browse listing `l` went through: it waits in
    /// Deliveries.
    pub(super) fn bought(&mut self, l: &ListingRow) {
        let item = matches!(l.goods, Goods::Item { .. }).then(|| (l.group, l.item.clone()));
        let t = &mut self.trade;
        t.deliver(
            DeliverySource::Bought,
            Some(l.listing),
            None,
            &l.item.name,
            l.goods.clone(),
            item,
            0,
        );
        self.sync_counts();
    }

    /// Someone buys the asker's listing `id` at its price: the proceeds go
    /// to Deliveries, its offers are returned, and the asker hears of it.
    pub fn sell_mine(&mut self, id: ListingId, buyer: &str) {
        let Some(i) = self.trade.own.iter().position(|l| l.listing == id) else {
            return;
        };
        let l = self.trade.own.remove(i);
        let paid = l.price.clone().unwrap_or_default();
        self.own_sold(id, buyer, &paid);
        self.trade
            .deliver_price(DeliverySource::Sold, (id, None), &l.item.name, &paid, &[]);
        for o in self.trade.offers.iter_mut() {
            if o.listing == id && o.state.live() {
                o.state = OfferState::Returned;
                o.answered_ms = NOW_MS;
            }
        }
        self.sync_counts();
        self.events.push_back(MarketEvent::Notice(Notice {
            what: NoticeWhat::Sold,
            listing: id,
            offer: None,
            about: l.item.name.clone(),
            who: buyer.into(),
            price: Some(paid),
            deliveries_waiting: self.state.deliveries_waiting,
            at_ms: NOW_MS,
        }));
    }

    /// Someone makes an offer on the asker's listing `id`: the asker hears
    /// of it.
    pub fn offer_arrives(&mut self, id: ListingId, buyer: &str, offered: Price) {
        let offer = self
            .trade
            .offer_on_mine(id, buyer, offered.clone(), Vec::new(), 0);
        if let Some(o) = self.trade.offers.iter().find(|o| o.offer == offer).cloned() {
            self.notify(NoticeWhat::OfferReceived, &o, buyer, Some(offered));
        }
    }

    /// The answer to a trading ask.
    pub(super) fn answer_trade(&mut self, req: Req, ask: &MarketAsk) -> MarketEvent {
        match ask {
            MarketAsk::OffersOpen {
                which,
                live_only,
                page,
            } => MarketEvent::Offers {
                req,
                page: self.offers_page(*which, *live_only, *page),
            },
            MarketAsk::DeliveriesOpen { page } => MarketEvent::Deliveries {
                req,
                page: self.deliveries_page(*page),
            },
            MarketAsk::Offer {
                listing,
                price,
                fee_rev,
            } => {
                let (code, id) = self.make_offer(*listing, price, *fee_rev);
                if code == MarketCode::Done {
                    self.push_state();
                }
                FakeSource::done(req, code, id)
            }
            MarketAsk::Answer {
                offer,
                how,
                counter,
                fee_rev,
            } => {
                let code = self.answer_offer(*offer, *how, counter.as_ref(), *fee_rev);
                if code == MarketCode::Done {
                    self.push_state();
                }
                FakeSource::done(req, code, Some(*offer))
            }
            MarketAsk::Claim { all, deliveries } => {
                if !*all && deliveries.len() > CLAIM_MAX {
                    return FakeSource::done(req, MarketCode::Other(99), None);
                }
                let rows = self.claim(*all, deliveries);
                let mut e = FakeSource::done(req, MarketCode::Done, None);
                if let MarketEvent::Done(d) = &mut e {
                    d.rows = rows;
                }
                self.push_state();
                e
            }
            _ => FakeSource::done(req, MarketCode::Other(99), None),
        }
    }

    fn offers_page(&self, which: OffersWhich, live_only: bool, page: u16) -> OffersPage {
        let mut rows: Vec<&OfferRow> = self
            .trade
            .offers
            .iter()
            .filter(|o| match which {
                OffersWhich::OnMyListings => o.seller == ME,
                OffersWhich::MadeByMe => o.buyer == ME,
            })
            .filter(|o| !live_only || o.state.live())
            .collect();
        rows.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then(b.offer.cmp(&a.offer)));
        OffersPage {
            which,
            page,
            total: rows.len() as u32,
            now_ms: NOW_MS,
            rows: rows
                .into_iter()
                .skip(usize::from(page) * PAGE_ROWS)
                .take(PAGE_ROWS)
                .cloned()
                .collect(),
        }
    }

    fn deliveries_page(&self, page: u16) -> DeliveriesPage {
        let mut rows: Vec<&DeliveryRow> = self.trade.deliveries.iter().collect();
        rows.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then(b.delivery.cmp(&a.delivery)));
        DeliveriesPage {
            page,
            total: rows.len() as u32,
            now_ms: NOW_MS,
            rows: rows
                .into_iter()
                .skip(usize::from(page) * PAGE_ROWS)
                .take(PAGE_ROWS)
                .cloned()
                .collect(),
        }
    }

    fn make_offer(
        &mut self,
        listing: ListingId,
        p: &Price,
        fee_rev: u32,
    ) -> (MarketCode, Option<u64>) {
        if fee_rev != self.state.fees.fee_rev {
            return (MarketCode::FeeChanged, Some(listing));
        }
        let Some(l) = self.listing(listing).cloned() else {
            return (MarketCode::ListingGone, Some(listing));
        };
        if !l.mode.takes_offers() || l.mine || p.is_empty() {
            return (MarketCode::Other(99), Some(listing));
        }
        let mine_live = self
            .trade
            .offers
            .iter()
            .filter(|o| o.buyer == ME && o.state.live())
            .count();
        if p.items.len() > OFFER_ITEMS || mine_live >= usize::from(self.state.max_offers) {
            return (MarketCode::MarketLimit, Some(listing));
        }
        if !p
            .items
            .iter()
            .all(|id| self.trade.stash.item(*id).is_some())
        {
            return (MarketCode::ItemNotFound, Some(listing));
        }
        if !self.trade.stash.holds(p) {
            return (MarketCode::NotEnough, Some(listing));
        }
        let id = self.trade.made(&l, p.clone(), 0);
        if let Some(b) = self.listings.iter_mut().find(|b| b.listing == listing) {
            b.offers += 1;
        }
        (MarketCode::Done, Some(id))
    }

    fn answer_offer(
        &mut self,
        id: u64,
        how: AnswerHow,
        counter: Option<&Price>,
        fee_rev: u32,
    ) -> MarketCode {
        if fee_rev != self.state.fees.fee_rev {
            return MarketCode::FeeChanged;
        }
        let Some(i) = self.trade.offers.iter().position(|o| o.offer == id) else {
            return MarketCode::OfferGone;
        };
        let o = self.trade.offers[i].clone();
        if !o.state.live() {
            return MarketCode::OfferGone;
        }
        let seller = o.seller == ME;
        let ok_role = match how {
            AnswerHow::Decline | AnswerHow::Counter => seller,
            AnswerHow::Withdraw => !seller,
            AnswerHow::Accept => true,
        };
        if !ok_role {
            return MarketCode::NotYours;
        }
        // The seller answers open offers; the buyer accepts a counter.
        let waits_on_me = (o.state == OfferState::Open) == seller;
        if !waits_on_me && how != AnswerHow::Withdraw {
            return MarketCode::Other(99);
        }
        let key = (o.listing, Some(o.offer));
        let about = o.item.name.clone();
        match how {
            AnswerHow::Counter => {
                let Some(c) = counter.filter(|c| !c.is_empty()) else {
                    return MarketCode::Other(99);
                };
                if !c.items.iter().all(|x| o.offered.items.contains(x)) {
                    return MarketCode::Other(99);
                }
                let r = &mut self.trade.offers[i];
                r.state = OfferState::Countered;
                r.counter = Some(c.clone());
                r.answered_ms = NOW_MS;
            }
            AnswerHow::Decline => {
                let r = &mut self.trade.offers[i];
                r.state = OfferState::Declined;
                r.answered_ms = NOW_MS;
            }
            AnswerHow::Withdraw => {
                let r = &mut self.trade.offers[i];
                r.state = OfferState::Withdrawn;
                r.answered_ms = NOW_MS;
                self.trade.deliver_price(
                    DeliverySource::Returned,
                    key,
                    &about,
                    &o.offered,
                    &o.items,
                );
            }
            AnswerHow::Accept if seller => {
                // The asker sells to the offer: the offered goods are its.
                let r = &mut self.trade.offers[i];
                r.state = OfferState::Accepted;
                r.answered_ms = NOW_MS;
                self.trade.own.retain(|l| l.listing != o.listing);
                self.own_sold(o.listing, &o.buyer, &o.offered);
                self.trade
                    .deliver_price(DeliverySource::Sold, key, &about, &o.offered, &o.items);
                for x in self.trade.offers.iter_mut() {
                    if x.listing == o.listing && x.state.live() {
                        x.state = OfferState::Returned;
                        x.answered_ms = NOW_MS;
                    }
                }
                self.notify(NoticeWhat::Sold, &o, &o.buyer, Some(o.offered.clone()));
            }
            AnswerHow::Accept => {
                // The asker takes the seller's counter: it pays what the
                // counter asks on top, and gets the change and the goods.
                let c = o.counter.clone().unwrap_or_default();
                let (change, extra) = Trade::split(&o.offered, &c);
                if !self.trade.stash.holds(&extra) {
                    return MarketCode::NotEnough;
                }
                self.trade.take(&extra);
                let r = &mut self.trade.offers[i];
                r.state = OfferState::Accepted;
                r.answered_ms = NOW_MS;
                self.trade
                    .deliver_price(DeliverySource::Change, key, &about, &change, &o.items);
                let item = matches!(o.goods, Goods::Item { .. }).then(|| (o.group, o.item.clone()));
                self.trade.deliver(
                    DeliverySource::Bought,
                    Some(o.listing),
                    Some(o.offer),
                    &about,
                    o.goods.clone(),
                    item,
                    0,
                );
                self.listings.retain(|l| l.listing != o.listing);
            }
        }
        MarketCode::Done
    }

    /// Claims into the stash: one result per delivery, in order; a row
    /// that does not fit stays.
    fn claim(&mut self, all: bool, ids: &[u64]) -> Vec<(u64, MarketCode)> {
        let want: Vec<u64> = if all {
            let mut d: Vec<&DeliveryRow> = self.trade.deliveries.iter().collect();
            d.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then(b.delivery.cmp(&a.delivery)));
            d.into_iter().map(|d| d.delivery).take(CLAIM_MAX).collect()
        } else {
            ids.to_vec()
        };
        let mut rows = Vec::new();
        for id in want {
            let t = &mut self.trade;
            let Some(i) = t.deliveries.iter().position(|d| d.delivery == id) else {
                rows.push((id, MarketCode::ItemNotFound));
                continue;
            };
            let fits = match &t.deliveries[i].goods {
                Goods::Item { .. } => t.room > 0,
                Goods::Gold(g) => t.stash.gold + g <= GOLD_MAX,
                Goods::Stack { .. } => true,
            };
            if !fits {
                rows.push((id, MarketCode::StashFull));
                continue;
            }
            let d = t.deliveries.remove(i);
            match d.goods {
                Goods::Item { item_id, record } => {
                    t.room -= 1;
                    let (group, item) = d.item.unwrap_or((Group::Other, ItemView::default()));
                    t.stash.items.push(NamedItem {
                        item_id,
                        record,
                        group,
                        item,
                    });
                }
                Goods::Gold(g) => t.stash.gold += g,
                Goods::Stack { code, count } => *t.stash.stacks.entry(code).or_default() += count,
            }
            rows.push((id, MarketCode::Done));
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::fake::FEE_REV;
    use crate::market::source::MarketSource;

    fn fake() -> FakeSource {
        FakeSource::new(&|_| None, false, 0, true)
    }

    /// Sends `ask` and gives its `MarketDone`.
    fn done(f: &mut FakeSource, ask: MarketAsk) -> crate::market::source::MarketDone {
        let req = f.send(ask);
        f.poll()
            .into_iter()
            .find_map(|e| match e {
                MarketEvent::Done(d) if d.req == req => Some(d),
                _ => None,
            })
            .expect("a MarketDone")
    }

    fn offer(listing: ListingId, price: Price, fee_rev: u32) -> MarketAsk {
        MarketAsk::Offer {
            listing,
            price,
            fee_rev,
        }
    }

    #[test]
    fn offers_are_checked_against_the_stash() {
        let mut f = fake();
        let l = f
            .listings
            .iter()
            .find(|l| l.mode.takes_offers() && !l.mine)
            .unwrap()
            .listing;
        let priced = f
            .listings
            .iter()
            .find(|l| !l.mode.takes_offers())
            .unwrap()
            .listing;
        let gold = |g| Price::gold(g);
        let code = |f: &mut FakeSource, a| done(f, a).code;
        assert_eq!(
            code(&mut f, offer(l, gold(10), FEE_REV + 1)),
            MarketCode::FeeChanged
        );
        assert_eq!(
            code(&mut f, offer(999_999_999, gold(10), FEE_REV)),
            MarketCode::ListingGone
        );
        assert_eq!(
            code(&mut f, offer(priced, gold(10), FEE_REV)),
            MarketCode::Other(99)
        );
        assert_eq!(
            code(&mut f, offer(l, Price::default(), FEE_REV)),
            MarketCode::Other(99)
        );
        let many = Price {
            items: (0..=OFFER_ITEMS as u32).collect(),
            ..gold(1)
        };
        assert_eq!(
            code(&mut f, offer(l, many, FEE_REV)),
            MarketCode::MarketLimit
        );
        let stranger = Price {
            items: vec![1],
            ..gold(1)
        };
        assert_eq!(
            code(&mut f, offer(l, stranger, FEE_REV)),
            MarketCode::ItemNotFound
        );
        let held = f.trade.stash.gold;
        assert_eq!(
            code(&mut f, offer(l, gold(held + 1), FEE_REV)),
            MarketCode::NotEnough
        );
        // Paid into escrow: the stash no longer holds it.
        let p = Price {
            gold: 1_000,
            stacks: vec![("r15".into(), 4)],
            items: vec![7001],
        };
        let d = done(&mut f, offer(l, p, FEE_REV));
        assert_eq!(d.code, MarketCode::Done);
        let s = &f.trade.stash;
        assert_eq!(s.gold, held - 1_000);
        assert!(!s.stacks.contains_key("r15"));
        assert!(s.item(7001).is_none());
        let o = f
            .trade
            .offers
            .iter()
            .find(|o| Some(o.offer) == d.id)
            .unwrap();
        assert_eq!(o.items.len(), 1);
        assert_eq!(f.state.my_live_offers, 3);
    }

    #[test]
    fn answers_follow_the_roles() {
        let mut f = fake();
        let to_me = |f: &FakeSource, who: &str| {
            f.trade
                .offers
                .iter()
                .find(|o| o.buyer == who)
                .unwrap()
                .clone()
        };
        let necro = to_me(&f, "Necrolord");
        let answer = |offer, how, counter| MarketAsk::Answer {
            offer,
            how,
            counter,
            fee_rev: FEE_REV,
        };
        // A counter keeps only offered items.
        let foreign = Price {
            items: vec![8101, 7001],
            ..Price::gold(5)
        };
        let a = answer(necro.offer, AnswerHow::Counter, Some(foreign));
        assert_eq!(done(&mut f, a).code, MarketCode::Other(99));
        // The seller cannot withdraw nor accept its own counter.
        let a = answer(necro.offer, AnswerHow::Withdraw, None);
        assert_eq!(done(&mut f, a).code, MarketCode::NotYours);
        let kept = Price {
            items: vec![8101],
            ..Price::gold(5_000)
        };
        let a = answer(necro.offer, AnswerHow::Counter, Some(kept));
        assert_eq!(done(&mut f, a).code, MarketCode::Done);
        let a = answer(necro.offer, AnswerHow::Accept, None);
        assert_eq!(done(&mut f, a).code, MarketCode::Other(99));
        // Accepting Ilse's gold sells the listing: the gold waits, the
        // other offers on it come back to their buyers.
        let ilse = to_me(&f, "Ilse");
        let waiting = f.state.deliveries_waiting;
        let req = f.send(answer(ilse.offer, AnswerHow::Accept, None));
        let events = f.poll();
        let ok = |e: &MarketEvent| matches!(e, MarketEvent::Done(d) if d.req == req && d.code == MarketCode::Done);
        assert!(events.iter().any(ok));
        // The seller hears of the sale, with the count waiting.
        let sold = |e: &MarketEvent| {
            matches!(e, MarketEvent::Notice(n)
                if n.what == NoticeWhat::Sold && n.deliveries_waiting == waiting + 1)
        };
        assert!(events.iter().any(sold));
        assert_eq!(f.state.deliveries_waiting, waiting + 1);
        let back = f.trade.offers.iter().find(|o| o.offer == necro.offer);
        assert_eq!(back.unwrap().state, OfferState::Returned);
        assert!(f.trade.own.iter().all(|l| l.listing != ilse.listing));
        // An answered offer is gone.
        let a = answer(ilse.offer, AnswerHow::Decline, None);
        assert_eq!(done(&mut f, a).code, MarketCode::OfferGone);
    }

    #[test]
    fn claims_go_row_by_row() {
        let mut f = fake();
        let too_many = MarketAsk::Claim {
            all: false,
            deliveries: (0..=CLAIM_MAX as u64).collect(),
        };
        assert_eq!(done(&mut f, too_many).code, MarketCode::Other(99));
        // Gold over the stash's most stays; a delivery not there is named.
        f.trade.stash.gold = GOLD_MAX;
        f.trade.deliver(
            DeliverySource::Change,
            Some(1),
            Some(2),
            "Coil",
            Goods::Gold(1),
            None,
            0,
        );
        let gold = f.trade.deliveries.last().unwrap().delivery;
        let a = MarketAsk::Claim {
            all: false,
            deliveries: vec![gold, 999_999],
        };
        let d = done(&mut f, a);
        assert_eq!(
            d.rows,
            [
                (gold, MarketCode::StashFull),
                (999_999, MarketCode::ItemNotFound)
            ]
        );
        // The rest goes in; the gold stays.
        let all = MarketAsk::Claim {
            all: true,
            deliveries: Vec::new(),
        };
        let d = done(&mut f, all);
        let ok = d
            .rows
            .iter()
            .filter(|(_, c)| *c == MarketCode::Done)
            .count();
        assert_eq!(ok, 5);
        assert_eq!(f.trade.deliveries.len(), 1);
        assert_eq!(f.state.deliveries_waiting, 1);
    }
}
