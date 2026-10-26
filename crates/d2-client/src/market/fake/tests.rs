use super::browse::{flag_count, group_counts, LISTINGS};
use super::*;
use crate::market::source::{Facets, Group, MarketQuery, MarketSort, PriceMode, Tri};

fn fake() -> FakeSource {
    FakeSource::new(&|_| None, false, 0, true)
}

fn q() -> MarketQuery {
    MarketQuery {
        page_size: 50,
        ..MarketQuery::default()
    }
}

fn results(f: &mut FakeSource, q: MarketQuery) -> crate::market::source::ResultsPage {
    let req = f.send(MarketAsk::Search(q));
    match f.poll().pop() {
        Some(MarketEvent::Results { req: r, page }) if r == req => page,
        e => panic!("{e:?}"),
    }
}

#[test]
fn facets_leave_out_their_own_filter() {
    let mut f = fake();
    let all = results(&mut f, q());
    assert_eq!(all.total as usize, LISTINGS);
    let fa = all.facets.clone().unwrap();
    assert_eq!(fa.groups, group_counts(&f.listings));
    let uniques = results(
        &mut f,
        MarketQuery {
            groups: vec![Group::Unique],
            ..q()
        },
    );
    let fu = uniques.facets.unwrap();
    // The group counts stay; the others are the uniques'.
    assert_eq!(fu.groups, fa.groups);
    let n = fu.groups[&Group::Unique];
    assert_eq!(uniques.total, n);
    assert_eq!(Facets::flag(fu.ethereal, Tri::Any), n);
    assert!(uniques.rows.iter().all(|l| l.group == Group::Unique));
    // A type counts at its ancestors: every armor piece under `armo`.
    let helms = fa.types["helm"];
    assert!(helms > fa.types.get("circ").copied().unwrap_or(0));
    assert!(fa.types["armo"] >= helms);
    let eth = flag_count(&f.listings, |l| l.item.ethereal, Tri::Yes);
    assert_eq!(fa.ethereal, [eth, LISTINGS as u32 - eth]);
}

#[test]
fn pages_sort_types_and_text_find_listings() {
    let mut f = fake();
    let by_name = MarketQuery {
        sort: MarketSort::Name,
        desc: false,
        ..q()
    };
    let p0 = results(&mut f, by_name.clone());
    let p1 = results(&mut f, MarketQuery { page: 1, ..by_name });
    assert_eq!((p0.rows.len(), p1.rows.len()), (50, 50));
    assert!(p0.rows.windows(2).all(|w| w[0].item.name <= w[1].item.name));
    assert!(p0.rows.last().unwrap().item.name <= p1.rows[0].item.name);
    let ashen = results(
        &mut f,
        MarketQuery {
            text: "ash cro".into(),
            ..q()
        },
    );
    assert!(ashen.total > 0);
    assert!(ashen.rows.iter().all(|l| l.item.name == "Ashen Crown"));
    let armor = results(
        &mut f,
        MarketQuery {
            types: vec!["helm".into()],
            ..q()
        },
    );
    assert!(armor
        .rows
        .iter()
        .all(|l| matches!(l.item.item_type.as_str(), "helm" | "circ")));
    let eth = results(
        &mut f,
        MarketQuery {
            ethereal: Tri::Yes,
            price_mode: PriceMode::GoldOnly,
            ..q()
        },
    );
    assert!(eth
        .rows
        .iter()
        .all(|l| l.item.ethereal && l.price.as_ref().is_some_and(|p| p.gold_only())));
    let grouped = results(
        &mut f,
        MarketQuery {
            group_first: true,
            page_size: 100,
            ..q()
        },
    );
    assert!(grouped.rows.windows(2).all(|w| w[0].group <= w[1].group));
}

#[test]
fn a_buy_checks_the_hash_and_fees_and_takes_the_listing() {
    let mut f = fake();
    let l = f.listings[0].clone();
    let waiting = f.state.deliveries_waiting;
    let buy = |hash, fee_rev| MarketAsk::Buy {
        listing: l.listing,
        price_hash: hash,
        fee_rev,
    };
    let codes = |f: &mut FakeSource| -> Vec<MarketCode> {
        f.poll()
            .into_iter()
            .map(|e| match e {
                MarketEvent::Done(d) => d.code,
                e => panic!("{e:?}"),
            })
            .collect()
    };
    f.send(buy([0; 32], FEE_REV));
    f.send(buy(l.price_hash, FEE_REV + 1));
    f.send(buy(l.price_hash, FEE_REV));
    f.send(buy(l.price_hash, FEE_REV));
    assert_eq!(
        codes(&mut f),
        [
            MarketCode::PriceChanged,
            MarketCode::FeeChanged,
            MarketCode::Done,
            MarketCode::ListingGone
        ]
    );
    assert!(f.listing(l.listing).is_none());
    assert_eq!(f.state.deliveries_waiting, waiting + 1);
    f.set_on(false);
    f.send(MarketAsk::Search(q()));
    let e = f.poll();
    assert!(matches!(&e[0], MarketEvent::State { req: 0, state } if !state.on));
    assert!(matches!(&e[1], MarketEvent::Done(d) if d.code == MarketCode::MarketOff));
}
