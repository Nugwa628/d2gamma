// D2RUST's own: the market's wire messages as the window's types.
//! The realm's market speaks the contract's messages
//! (`d2_net::msg::market`); the window ([`super::source`]) has its own
//! types with the client's reading of every item record beside it. This
//! is the translation both ways: an ask goes out as a wire [`w::Ask`],
//! an answer (a page the online client assembled from its Begin, rows and
//! item messages) comes in as a [`MarketEvent`]. The realm source
//! (`super::realm`, online builds) does the sending; this part needs no
//! connection, so the default suite tests it.
//!
//! The realm sends an item as its save-format record: [`Reading`] reads
//! it with the game data (`ui::market::tip::read_item`). The group of a
//! listing is the realm's; an offer's goods, a delivery's item and an
//! offered item get the group the record reads as.

use d2_net::msg::market as w;
use d2_net::msg::{ErrorCode, ErrorMsg};

use super::prices::{self, PriceHistory, PriceOf};
use super::source::{
    Facets, Goods, Group, ItemView, ListingDetail, ListingMode, ListingRow, ListingState,
    MarketAsk, MarketCode, MarketDone, MarketEvent, MarketFees, MarketQuery, MarketSort,
    MarketState, OffReason, OfferShort, PostGoods, Price, PriceMode, Req, ResultsPage, ShopOp,
    ShopPage, ShopRow, ShopSort, ShopTab, Sold, Tri,
};
use super::trade::{
    AnswerHow, DeliveriesPage, DeliveryReason, DeliveryRow, DeliverySource, NamedItem, Notice,
    NoticeWhat, OfferRow, OfferState, OffersPage, OffersWhich,
};

/// An item record's group and view; `None` for a record that does not
/// read.
pub type ReadRecord<'a> = dyn Fn(&[u8]) -> Option<(Group, ItemView)> + 'a;

/// How the client reads what the realm sends: an item record's group and
/// view, a stackable's name by code.
pub struct Reading<'a> {
    pub item: &'a ReadRecord<'a>,
    pub name: &'a dyn Fn(&str) -> Option<String>,
}

impl Reading<'_> {
    /// Reads nothing: records show as unknown items, stackables by code.
    pub const NONE: Reading<'static> = Reading {
        item: &|_| None,
        name: &|_| None,
    };

    /// The view of `goods` (`None` for gold) with the group its record
    /// reads as; an unreadable record is an unknown item.
    pub fn goods(&self, goods: &Goods) -> Option<(Group, ItemView)> {
        match goods {
            Goods::Item { record, .. } => Some((self.item)(record).unwrap_or_else(|| {
                (
                    Group::Other,
                    ItemView {
                        name: UNKNOWN.into(),
                        identified: true,
                        ..ItemView::default()
                    },
                )
            })),
            Goods::Stack { code, .. } => {
                let name = (self.name)(code).unwrap_or_else(|| code.clone());
                Some((
                    Group::Stackable,
                    ItemView {
                        base: name.clone(),
                        name,
                        code: code.clone(),
                        identified: true,
                        ..ItemView::default()
                    },
                ))
            }
            Goods::Gold(_) => None,
        }
    }
}

/// The name of an item whose record does not read.
pub const UNKNOWN: &str = "Unknown item";

/// Gold's place where an item's group and view go.
fn other() -> (Group, ItemView) {
    (Group::Other, ItemView::default())
}

// ---- asks ----

/// An ask as the wire's.
pub fn ask(a: &MarketAsk) -> w::Ask {
    match a {
        MarketAsk::Hello => w::Ask::Hello,
        MarketAsk::Search(q) => w::Ask::Search(query(q)),
        MarketAsk::ListingGet(id) => w::Ask::ListingGet { listing: *id },
        MarketAsk::Buy {
            listing,
            price_hash,
            fee_rev,
        } => w::Ask::Buy {
            listing: *listing,
            price_hash: *price_hash,
            fee_rev: *fee_rev,
        },
        MarketAsk::Offer {
            listing,
            price,
            fee_rev,
        } => w::Ask::Offer {
            listing: *listing,
            price: price_out(price),
            fee_rev: *fee_rev,
        },
        MarketAsk::Answer {
            offer,
            how,
            counter,
            fee_rev,
        } => w::Ask::Answer {
            offer: *offer,
            how: match how {
                AnswerHow::Accept => w::AnswerHow::Accept,
                AnswerHow::Decline => w::AnswerHow::Decline,
                AnswerHow::Counter => w::AnswerHow::Counter,
                AnswerHow::Withdraw => w::AnswerHow::Withdraw,
            },
            counter: counter.as_ref().map(price_out),
            fee_rev: *fee_rev,
        },
        MarketAsk::OffersOpen {
            which,
            live_only,
            page,
        } => w::Ask::OffersOpen {
            which: match which {
                OffersWhich::OnMyListings => w::OffersWhich::OnMyListings,
                OffersWhich::MadeByMe => w::OffersWhich::MadeByMe,
            },
            live_only: *live_only,
            page: *page,
        },
        MarketAsk::DeliveriesOpen { page } => w::Ask::DeliveriesOpen { page: *page },
        MarketAsk::Claim { all, deliveries } => w::Ask::Claim {
            all: *all,
            deliveries: deliveries.clone(),
        },
        MarketAsk::Post(p) => w::Ask::Post(w::Post {
            goods: match &p.goods {
                PostGoods::Item(id) => w::PostGoods::Item { item_id: *id },
                PostGoods::Stack { code, count } => w::PostGoods::Stack {
                    code: code.clone(),
                    count: *count,
                },
            },
            mode: mode_out(p.mode),
            price: p.price.as_ref().map(price_out),
            days: p.days,
            note: p.note.clone(),
            hidden: p.hidden,
            fee_rev: p.fee_rev,
        }),
        MarketAsk::ShopEdit {
            op,
            listings,
            fee_rev,
        } => w::Ask::ShopEdit {
            op: match op {
                ShopOp::Cancel => w::ShopOp::Cancel,
                ShopOp::Relist { days } => w::ShopOp::Relist { days: *days },
                ShopOp::Hide => w::ShopOp::Hide,
                ShopOp::Show => w::ShopOp::Show,
                ShopOp::Note(n) => w::ShopOp::Note(n.clone()),
                ShopOp::Reprice { mode, price } => w::ShopOp::Reprice {
                    mode: mode_out(*mode),
                    price: price.as_ref().map(price_out),
                },
            },
            listings: listings.clone(),
            fee_rev: *fee_rev,
        },
        MarketAsk::ShopOpen {
            tab,
            page,
            sort,
            desc,
        } => w::Ask::ShopOpen {
            tab: tab_out(*tab),
            page: *page,
            sort: match sort {
                ShopSort::Posted => w::ShopSort::Posted,
                ShopSort::Ends => w::ShopSort::Ends,
                ShopSort::Gold => w::ShopSort::GoldPrice,
                ShopSort::Name => w::ShopSort::Name,
            },
            desc: *desc,
        },
        MarketAsk::Prices { of, days } => w::Ask::Prices {
            of: match of {
                PriceOf::Listing(id) => w::PriceOf::Listing(*id),
                PriceOf::StashItem(id) => w::PriceOf::StashItem(*id),
                PriceOf::Stackable(c) => w::PriceOf::Stackable(c.clone()),
                PriceOf::Key(k) => w::PriceOf::Key(k.clone()),
            },
            days: *days,
        },
    }
}

fn price_out(p: &Price) -> w::Price {
    w::Price {
        gold: p.gold,
        stacks: p.stacks.clone(),
        items: p.items.clone(),
    }
}

fn mode_out(m: ListingMode) -> w::ListingMode {
    match m {
        ListingMode::Price => w::ListingMode::Price,
        ListingMode::OffersOnly => w::ListingMode::OffersOnly,
        ListingMode::PriceAndOffers => w::ListingMode::PriceAndOffers,
    }
}

fn tab_out(t: ShopTab) -> w::ShopTab {
    match t {
        ShopTab::Shown => w::ShopTab::Shown,
        ShopTab::Hidden => w::ShopTab::Hidden,
        ShopTab::Sold => w::ShopTab::Sold,
        ShopTab::Expired => w::ShopTab::Expired,
    }
}

fn group_out(g: Group) -> w::Group {
    match g {
        Group::Unique => w::Group::Unique,
        Group::Set => w::Group::Set,
        Group::Runeword => w::Group::Runeword,
        Group::Rare => w::Group::Rare,
        Group::Crafted => w::Group::Crafted,
        Group::Magic => w::Group::Magic,
        Group::Other => w::Group::Other,
        Group::Stackable => w::Group::Stackable,
    }
}

fn tri_out(t: Tri) -> w::TriState {
    match t {
        Tri::Any => w::TriState::Any,
        Tri::Yes => w::TriState::Yes,
        Tri::No => w::TriState::No,
    }
}

/// A search as the wire's query.
pub fn query(q: &MarketQuery) -> w::MarketQuery {
    w::MarketQuery {
        text: q.text.clone(),
        groups: q.groups.iter().map(|g| group_out(*g)).collect(),
        types: q.types.clone(),
        ethereal: tri_out(q.ethereal),
        socketed: tri_out(q.socketed),
        identified: tri_out(q.identified),
        stats: q
            .stats
            .iter()
            .map(|&(stat, min, max)| w::StatRange { stat, min, max })
            .collect(),
        level: q.level,
        sockets: q.sockets,
        class: q.class,
        price_mode: match q.price_mode {
            PriceMode::Any => w::PriceMode::Any,
            PriceMode::Fixed => w::PriceMode::HasPrice,
            PriceMode::Offers => w::PriceMode::TakesOffers,
            PriceMode::RunesOnly => w::PriceMode::CurrenciesOnly,
            PriceMode::GoldOnly => w::PriceMode::GoldOnly,
        },
        caps: q.caps.clone(),
        sort: w::Sort {
            by: match q.sort {
                MarketSort::Posted => w::SortBy::Posted,
                MarketSort::Price => w::SortBy::Price,
                MarketSort::Stat => w::SortBy::Stat,
                MarketSort::Level => w::SortBy::Level,
                MarketSort::Name => w::SortBy::Name,
                MarketSort::Seller => w::SortBy::Seller,
            },
            stat: q.sort_stat,
            currency: q.sort_currency.clone(),
            desc: q.desc,
        },
        group_first: q.group_first,
        page: q.page,
        page_size: q.page_size,
        facets: q.facets,
    }
}

// ---- answers ----

fn group_in(g: w::Group) -> Group {
    match g {
        w::Group::Unique => Group::Unique,
        w::Group::Set => Group::Set,
        w::Group::Runeword => Group::Runeword,
        w::Group::Rare => Group::Rare,
        w::Group::Crafted => Group::Crafted,
        w::Group::Magic => Group::Magic,
        w::Group::Other => Group::Other,
        w::Group::Stackable => Group::Stackable,
    }
}

fn price_in(p: w::Price) -> Price {
    Price {
        gold: p.gold,
        stacks: p.stacks,
        items: p.items,
    }
}

fn goods_in(g: w::Goods) -> Goods {
    match g {
        w::Goods::Item { item_id, record } => Goods::Item { item_id, record },
        w::Goods::Stack { code, count } => Goods::Stack { code, count },
        w::Goods::Gold(g) => Goods::Gold(g),
    }
}

fn mode_in(m: w::ListingMode) -> ListingMode {
    match m {
        w::ListingMode::Price => ListingMode::Price,
        w::ListingMode::OffersOnly => ListingMode::OffersOnly,
        w::ListingMode::PriceAndOffers => ListingMode::PriceAndOffers,
    }
}

fn listing_state_in(s: w::ListingState) -> ListingState {
    match s {
        w::ListingState::Active => ListingState::Active,
        w::ListingState::Sold => ListingState::Sold,
        w::ListingState::Cancelled => ListingState::Cancelled,
        w::ListingState::Expired => ListingState::Expired,
    }
}

fn offer_state_in(s: w::OfferState) -> OfferState {
    match s {
        w::OfferState::Open => OfferState::Open,
        w::OfferState::Countered => OfferState::Countered,
        w::OfferState::Accepted => OfferState::Accepted,
        w::OfferState::Declined => OfferState::Declined,
        w::OfferState::Withdrawn => OfferState::Withdrawn,
        w::OfferState::Returned => OfferState::Returned,
    }
}

fn code_in(c: ErrorCode) -> MarketCode {
    MarketCode::of(c.0)
}

/// The realm's market state.
pub fn state(s: w::MarketState) -> MarketEvent {
    MarketEvent::State {
        req: s.req,
        state: MarketState {
            on: s.on,
            off_reason: match s.off_reason {
                w::OffReason::On => OffReason::On,
                w::OffReason::Switch => OffReason::Switch,
                w::OffReason::NoCharacter => OffReason::NoCharSelected,
            },
            mod_id: s.mod_id,
            hardcore: s.hardcore,
            ladder: s.ladder,
            expansion: s.expansion,
            max_listings: s.limits.max_listings,
            max_offers: s.limits.max_offers,
            max_offer_items: s.limits.max_offer_items,
            max_days: s.limits.max_days,
            fees: MarketFees {
                sale_fee_pct: s.fees.sale_fee_pct,
                listing_fee: s.fees.listing_fee,
                buyer_fee: s.fees.buyer_fee,
                fee_rev: s.fees.fee_rev,
            },
            active_listings: s.counts.active_listings,
            deliveries_waiting: s.counts.deliveries_waiting,
            live_offers_to_me: s.counts.live_offers_to_me,
            my_live_offers: s.counts.my_live_offers,
            last_req: s.last_req,
            currencies: s.stackables,
        },
    }
}

/// A listing's row with the client's reading of its goods; the group is
/// the realm's.
pub fn row(r: w::ListingRow, read: &Reading) -> ListingRow {
    let goods = goods_in(r.goods);
    let item = read.goods(&goods).map(|(_, v)| v).unwrap_or_default();
    ListingRow {
        listing: r.listing,
        group: group_in(r.group),
        goods,
        mode: mode_in(r.mode),
        price: r.price.map(price_in),
        price_hash: r.price_hash,
        seller: r.seller,
        seller_sales: r.seller_sales,
        posted_ms: r.posted_ms,
        expires_ms: r.expires_ms,
        note: r.note,
        offers: r.offers,
        mine: r.mine,
        item,
    }
}

fn facets_in(f: w::Facets) -> Facets {
    Facets {
        groups: f
            .groups
            .into_iter()
            .map(|(g, n)| (group_in(g), n))
            .collect(),
        types: f.types.into_iter().collect(),
        ethereal: f.ethereal,
        socketed: f.socketed,
        identified: f.identified,
        price_modes: f.price_modes,
    }
}

/// A page of search results.
pub fn results(
    req: Req,
    head: w::ResultsHead,
    facets: Option<w::Facets>,
    rows: Vec<w::ListingRow>,
    read: &Reading,
) -> MarketEvent {
    MarketEvent::Results {
        req,
        page: ResultsPage {
            total: head.total,
            page: head.page,
            page_size: head.page_size,
            now_ms: head.now_ms,
            rows: rows.into_iter().map(|r| row(r, read)).collect(),
            facets: facets.map(facets_in),
        },
    }
}

/// A listing looked up, with the asker's offer on it.
pub fn listing(req: Req, d: w::ListingDetail, read: &Reading) -> MarketEvent {
    MarketEvent::Listing {
        req,
        detail: Box::new(ListingDetail {
            state: d.state as u8,
            row: row(d.row, read),
            buyer_fee: d.buyer_fee,
            fee_rev: d.fee_rev,
            offer: d.my_offer.map(|o| OfferShort {
                offer: o.offer,
                price: price_in(o.offered),
                counter: o.counter.map(price_in),
                state: o.state as u8,
                made_ms: o.at_ms,
                answered_ms: o.answered_ms,
            }),
        }),
    }
}

/// A page of My shop.
pub fn shop(req: Req, head: w::ShopHead, rows: Vec<w::ShopRow>, read: &Reading) -> MarketEvent {
    let tab = match head.tab {
        w::ShopTab::Shown => ShopTab::Shown,
        w::ShopTab::Hidden => ShopTab::Hidden,
        w::ShopTab::Sold => ShopTab::Sold,
        w::ShopTab::Expired => ShopTab::Expired,
    };
    MarketEvent::Shop {
        req,
        page: ShopPage {
            tab,
            page: head.page,
            counts: head.counts,
            slots_used: head.slots_used,
            slots_max: head.slots_max,
            now_ms: head.now_ms,
            rows: rows
                .into_iter()
                .map(|r| ShopRow {
                    row: row(r.row, read),
                    state: listing_state_in(r.state),
                    hidden: r.hidden,
                    live_offers: r.live_offers,
                    newest_offer_ms: r.newest_offer_ms,
                    sold: r.sold.map(|s| Sold {
                        buyer: s.buyer,
                        paid: price_in(s.paid),
                        sale_fee: s.sale_fee,
                        closed_ms: s.closed_ms,
                    }),
                    relistable: r.relistable,
                })
                .collect(),
        },
    }
}

/// A page of offers; each row names the offered (and countered) items
/// whose records came with the page.
pub fn offers(
    req: Req,
    head: w::OffersHead,
    rows: Vec<w::OfferRow>,
    items: Vec<w::ItemRecord>,
    read: &Reading,
) -> MarketEvent {
    let named = |id: u32| -> Option<NamedItem> {
        let rec = items.iter().find(|i| i.item_id == id)?;
        let (group, item) = read
            .goods(&Goods::Item {
                item_id: id,
                record: rec.record.clone(),
            })
            .unwrap_or_else(other);
        Some(NamedItem {
            item_id: id,
            record: rec.record.clone(),
            group,
            item,
        })
    };
    let rows = rows
        .into_iter()
        .map(|r| {
            let goods = goods_in(r.goods);
            let (group, item) = read.goods(&goods).unwrap_or_else(other);
            let offered = price_in(r.offered);
            let counter = r.counter.map(price_in);
            let mut ids: Vec<u32> = offered.items.clone();
            for id in counter.iter().flat_map(|c| c.items.iter()) {
                if !ids.contains(id) {
                    ids.push(*id);
                }
            }
            OfferRow {
                offer: r.offer,
                listing: r.listing,
                goods,
                group,
                item,
                price: r.price.map(price_in),
                buyer: r.buyer,
                seller: r.seller,
                offered,
                items: ids.into_iter().filter_map(named).collect(),
                counter,
                state: offer_state_in(r.state),
                at_ms: r.at_ms,
                answered_ms: r.answered_ms,
            }
        })
        .collect();
    MarketEvent::Offers {
        req,
        page: OffersPage {
            which: match head.which {
                w::OffersWhich::OnMyListings => OffersWhich::OnMyListings,
                w::OffersWhich::MadeByMe => OffersWhich::MadeByMe,
            },
            page: head.page,
            total: head.total,
            now_ms: head.now_ms,
            rows,
        },
    }
}

/// A page of Deliveries, with why each refund came back.
pub fn deliveries(
    req: Req,
    head: w::DeliveriesHead,
    rows: Vec<w::DeliveryRow>,
    read: &Reading,
) -> MarketEvent {
    MarketEvent::Deliveries {
        req,
        page: DeliveriesPage {
            page: head.page,
            total: head.total,
            now_ms: head.now_ms,
            rows: rows
                .into_iter()
                .map(|d| {
                    let goods = goods_in(d.goods);
                    let item = match goods {
                        Goods::Item { .. } => read.goods(&goods),
                        _ => None,
                    };
                    DeliveryRow {
                        delivery: d.delivery,
                        source: DeliverySource::of_code(d.source),
                        reason: DeliveryReason::of_code(d.reason),
                        listing: d.listing,
                        offer: d.offer,
                        about: d.about,
                        goods,
                        item,
                        at_ms: d.at_ms,
                    }
                })
                .collect(),
        },
    }
}

/// A price check's answer; `days` is the asked span (the head does not
/// repeat it).
pub fn prices(
    req: Req,
    head: w::PricesHead,
    series: Vec<w::PriceSeries>,
    recent: Vec<w::Sale>,
    days: u16,
) -> MarketEvent {
    let series = series
        .into_iter()
        .map(|s| prices::PriceSeries {
            index: s.index,
            shape: match s.shape {
                w::PriceShape::One => prices::PriceShape::One,
                w::PriceShape::Bundle => prices::PriceShape::Bundle,
                w::PriceShape::WithItems => prices::PriceShape::WithItems,
            },
            currencies: s.currencies,
            sold: s.sold,
            listed_now: s.listed_now,
            lowest_now: s.lowest_now,
            median_now: s.median_now,
            buckets: s
                .buckets
                .into_iter()
                .map(|b| prices::Bucket {
                    index: b.index,
                    sales: b.sales,
                    values: b
                        .values
                        .into_iter()
                        .map(|v| prices::Spread {
                            min: v.min,
                            median: v.median,
                            max: v.max,
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    MarketEvent::Prices {
        req,
        history: Box::new(PriceHistory {
            key: head.key,
            label: head.label,
            now_ms: head.now_ms,
            bucket_ms: head.bucket_ms,
            days,
            listed_now: head.listed_now,
            sold: head.sold,
            series,
            recent: recent
                .into_iter()
                .map(|s| prices::Sale {
                    at_ms: s.at_ms,
                    price: price_in(s.price),
                })
                .collect(),
        }),
    }
}

/// A request's result.
pub fn done(d: w::Done) -> MarketEvent {
    MarketEvent::Done(MarketDone {
        req: d.req,
        code: code_in(d.code),
        wait_s: d.wait_s,
        repeated: d.repeated,
        id: d.id,
        rows: d.rows.into_iter().map(|(id, c)| (id, code_in(c))).collect(),
        text: d.text,
    })
}

/// A refusal before the market (an `ERROR` for a market request: not
/// logged in, rate limited, malformed) as the request's result.
pub fn error(req: Req, e: ErrorMsg) -> MarketEvent {
    MarketEvent::Done(MarketDone {
        req,
        code: code_in(e.code),
        wait_s: e.wait_s,
        repeated: false,
        id: None,
        rows: Vec::new(),
        text: e.text,
    })
}

/// A notice pushed to the account.
pub fn notice(n: w::Notice) -> MarketEvent {
    MarketEvent::Notice(Notice {
        what: NoticeWhat::of_code(n.what),
        listing: n.listing,
        offer: n.offer,
        about: n.about,
        who: n.who,
        price: n.price.map(price_in),
        deliveries_waiting: n.deliveries_waiting,
        at_ms: n.at_ms,
    })
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
