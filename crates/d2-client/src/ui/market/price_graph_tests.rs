use super::*;
use crate::market::fake::{prices as fake_prices, FakeSource};
use crate::market::prices::{Bucket, Sale, DAY_MS};
use crate::market::source::{Group, MarketAsk, MarketSource};
use crate::ui::geom::Point;
use crate::ui::layout::Screen;
use crate::ui::market::window::{self, Places};
use crate::ui::shared_stash::Kit;

const KIT: Kit = Kit {
    panel: 1,
    edge: 2,
    gold: 3,
    on: 4,
    field: 5,
    hit: 6,
};

const INKS: Inks = Inks {
    series: [100, 101, 102, 103, 104, 105],
    muted: 106,
};

fn places() -> Places {
    Places::of(&Screen::R800)
}

/// A one-currency series with a bucket per `(index, min, median, max)`.
fn one(cur: &str, buckets: &[(u32, u64, u64, u64)], sales: u16) -> PriceSeries {
    PriceSeries {
        index: 0,
        shape: PriceShape::One,
        currencies: vec![cur.to_owned()],
        sold: u32::from(sales) * buckets.len() as u32,
        listed_now: 0,
        lowest_now: Vec::new(),
        median_now: Vec::new(),
        buckets: buckets
            .iter()
            .map(|&(index, min, median, max)| Bucket {
                index,
                sales,
                values: vec![Spread { min, median, max }],
            })
            .collect(),
    }
}

fn bundle(buckets: &[u32]) -> PriceSeries {
    PriceSeries {
        index: 0,
        shape: PriceShape::Bundle,
        currencies: vec!["r23".into(), "r24".into()],
        sold: buckets.len() as u32,
        listed_now: 0,
        lowest_now: Vec::new(),
        median_now: Vec::new(),
        buckets: buckets
            .iter()
            .map(|&index| Bucket {
                index,
                sales: 1,
                values: vec![
                    Spread {
                        min: 1,
                        median: 1,
                        max: 1,
                    },
                    Spread {
                        min: 2,
                        median: 2,
                        max: 2,
                    },
                ],
            })
            .collect(),
    }
}

fn with_items(sold: u32) -> PriceSeries {
    PriceSeries {
        index: 0,
        shape: PriceShape::WithItems,
        currencies: Vec::new(),
        sold,
        listed_now: 0,
        lowest_now: Vec::new(),
        median_now: Vec::new(),
        buckets: vec![Bucket {
            index: 3,
            sales: sold as u16,
            values: Vec::new(),
        }],
    }
}

fn history(series: Vec<PriceSeries>) -> PriceHistory {
    let mut series = series;
    for (i, s) in series.iter_mut().enumerate() {
        s.index = i as u8;
    }
    PriceHistory {
        key: "u:Ashen Crown".into(),
        label: "Ashen Crown".into(),
        now_ms: 100 * DAY_MS,
        bucket_ms: DAY_MS,
        days: 30,
        listed_now: 0,
        sold: series.iter().map(|s| s.sold).sum(),
        series,
        recent: vec![Sale {
            at_ms: 100 * DAY_MS - 3 * 3_600_000,
            price: Price::gold(250_000),
        }],
    }
}

/// The texts of a draw.
fn texts(out: &[UiDraw]) -> Vec<String> {
    out.iter()
        .filter_map(|d| match d {
            UiDraw::Text(t) => Some(String::from_utf16_lossy(&t.text)),
            _ => None,
        })
        .collect()
}

/// Draws the window (the price box over it) with the mouse at `mouse`.
fn draw_at(ui: &MarketUi, mouse: Point) -> (Vec<UiDraw>, Option<Vec<String>>) {
    let i = DrawIn {
        ui,
        places: places(),
        pen: Pen {
            kit: KIT,
            fonts: None,
            mouse,
        },
        tips: None,
        compare: None,
    };
    let mut out = Vec::new();
    window::draw(&i, &mut out);
    let tip = window::hover_tip(&i).map(|(lines, _)| {
        lines
            .iter()
            .map(|l| String::from_utf16_lossy(&l.text))
            .collect()
    });
    (out, tip)
}

/// An online window over the fake with listing `index` of the first page
/// shown on the right.
fn shown(index: usize) -> (MarketUi, FakeSource) {
    let mut ui = MarketUi::new();
    let mut src = FakeSource::new(&|_| None, false, 0, true);
    ui.set_online(true);
    ui.pump(&mut src);
    ui.toggle().unwrap();
    ui.pump(&mut src);
    ui.select(index);
    ui.pump(&mut src);
    assert!(ui.selected.is_some());
    (ui, src)
}

/// The first result of group `g` on the page.
fn first_of(ui: &MarketUi, g: Group) -> usize {
    ui.results
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|l| l.group == g)
        .unwrap_or_else(|| panic!("no {g:?} on the page"))
}

fn click(ui: &mut MarketUi, src: &mut FakeSource, s: Spot) {
    let p = places();
    let (_, r) = window::spots(ui, &p)
        .into_iter()
        .find(|(x, _)| *x == s)
        .unwrap_or_else(|| panic!("{s:?} is not shown"));
    let at = Point::new(r.x + i32::from(r.w) / 2, r.y + i32::from(r.h) / 2);
    let e = crate::ui::panel::UiEvent::Press {
        button: crate::ui::panel::PointerButton::Left,
        at,
    };
    assert!(window::event(ui, &p, e, false), "{s:?} takes the click");
    ui.pump(src);
}

#[test]
fn series_per_currency() {
    let h = history(vec![
        one(
            "",
            &[
                (2, 200_000, 250_000, 300_000),
                (9, 220_000, 220_000, 220_000),
            ],
            2,
        ),
        one("r24", &[(1, 1, 2, 3), (4, 2, 2, 2)], 1),
        bundle(&[0, 5]),
        with_items(3),
    ]);
    let pl = plot(&h, MAX_STRIPS);
    // A strip per series paid in currencies; the sales with items counted.
    assert_eq!(pl.strips.len(), 3);
    assert_eq!(pl.with_items, 3);
    assert_eq!(pl.more, 0);
    let marks: Vec<(Mark, &[String])> = pl
        .strips
        .iter()
        .map(|s| (s.mark, h.series[s.series].currencies.as_slice()))
        .collect();
    assert_eq!(marks[0], (Mark::Line, &[String::new()][..]));
    assert_eq!(marks[1], (Mark::Line, &["r24".to_owned()][..]));
    assert_eq!(marks[2].0, Mark::Points);
    // Each strip its own colour.
    let slots: Vec<usize> = pl.strips.iter().map(|s| s.slot).collect();
    assert_eq!(slots, [0, 1, 2]);
    // Room for two: the third is counted, not drawn.
    let pl = plot(&h, 2);
    assert_eq!((pl.strips.len(), pl.more), (2, 1));

    // The fake's mixed item: its sales give one series per currency set,
    // currencies sorted, the sales adding up.
    let src = FakeSource::new(&|_| None, false, 0, true);
    let l = src
        .listings
        .iter()
        .find(|l| l.group == Group::Unique)
        .unwrap();
    let key = fake_prices::price_key(l);
    let h = fake_prices::history(&src.sales, &src.listings, &key, "x", 90, NOW);
    let sets: Vec<(PriceShape, Vec<String>)> = h
        .series
        .iter()
        .filter(|s| s.sold > 0)
        .map(|s| (s.shape, s.currencies.clone()))
        .collect();
    for want in [
        (PriceShape::One, vec![String::new()]),
        (PriceShape::One, vec!["r24".to_owned()]),
        (PriceShape::Bundle, vec!["r23".to_owned(), "r24".to_owned()]),
        (PriceShape::WithItems, Vec::new()),
    ] {
        assert!(sets.contains(&want), "{want:?} in {sets:?}");
    }
    assert_eq!(h.series.iter().map(|s| s.sold).sum::<u32>(), h.sold);
    for s in &h.series {
        let w = if s.shape == PriceShape::WithItems {
            0
        } else {
            s.currencies.len()
        };
        for b in &s.buckets {
            assert_eq!(b.values.len(), w);
            for v in &b.values {
                assert!(v.min <= v.median && v.median <= v.max, "{v:?}");
            }
        }
    }
}

const NOW: u64 = crate::market::fake::browse::NOW_MS;

#[test]
fn bundle_points_not_lines() {
    let h = history(vec![
        one("r24", &[(1, 1, 2, 3), (4, 2, 2, 2), (7, 1, 1, 1)], 2),
        bundle(&[0, 5, 9]),
    ]);
    let pl = plot(&h, MAX_STRIPS);
    let r = Rect::new(0, 0, 300, 60);
    let line = shapes(&h, &pl.strips[0], r);
    let joins = line
        .iter()
        .filter(|s| matches!(s, Shape::Join { .. }))
        .count();
    let dots = line
        .iter()
        .filter(|s| matches!(s, Shape::Dot { .. }))
        .count();
    assert_eq!((joins, dots), (2, 3), "{line:?}");
    // Only the bucket with a spread has a whisker.
    assert_eq!(
        line.iter()
            .filter(|s| matches!(s, Shape::Whisker { .. }))
            .count(),
        1
    );
    let points = shapes(&h, &pl.strips[1], r);
    assert_eq!(points.len(), 3);
    assert!(
        points.iter().all(|s| matches!(s, Shape::Dot { .. })),
        "{points:?}"
    );
    // Oldest left, newest right at now.
    let xs: Vec<i32> = points
        .iter()
        .map(|s| match s {
            Shape::Dot { x, .. } => *x,
            _ => 0,
        })
        .collect();
    assert!(xs[0] < xs[1] && xs[1] < xs[2], "{xs:?}");
    assert!(xs[2] > 290);
    // A point's tip gives the bundle's full price.
    let ui = MarketUi::new();
    let tip = bucket_tip(&ui, &h, 1, 0, 1);
    let tip: Vec<String> = tip
        .iter()
        .map(|l| String::from_utf16_lossy(&l.text))
        .collect();
    assert_eq!(tip[0], "r23 + r24");
    assert_eq!(tip[2], "1 r23 + 2 r24");
}

#[test]
fn not_enough_data_under_5() {
    let mut ui = MarketUi::new();
    ui.online = true;
    ui.state = Some(crate::market::source::MarketState {
        on: true,
        ..Default::default()
    });
    ui.open = true;
    ui.prices.open = true;
    ui.prices.inks = Some(INKS);
    ui.prices.req = Some(1);
    let four = history(vec![one("", &[(1, 5, 5, 5), (2, 5, 5, 5)], 2)]);
    assert_eq!(four.sold, 4);
    assert!(!four.enough());
    ui.prices_answer(1, four);
    let (out, _) = draw_at(&ui, Point::new(0, 0));
    let t = texts(&out);
    assert!(t.iter().any(|s| s == "Not enough data"), "{t:?}");
    assert!(
        t.iter()
            .any(|s| s.starts_with("4 sales in the last 30 days")),
        "{t:?}"
    );
    // Nothing drawn, nothing to hover; the recent sales still listed.
    assert!(!out
        .iter()
        .any(|d| matches!(d, UiDraw::Line(l) if l.color == INKS.series[0])));
    assert!(targets(&ui, &places()).is_empty());
    assert!(
        t.iter().any(|s| s.starts_with("250,000 gold · 3h ago")),
        "{t:?}"
    );

    // Five sales draw.
    ui.prices.req = Some(2);
    ui.prices_answer(
        2,
        history(vec![
            one("", &[(1, 5, 5, 5), (2, 5, 5, 5), (3, 5, 5, 5)], 1),
            one("r24", &[(4, 1, 1, 1), (6, 1, 1, 1)], 1),
        ]),
    );
    assert!(ui.prices.history.as_ref().unwrap().enough());
    let (out, _) = draw_at(&ui, Point::new(0, 0));
    let t = texts(&out);
    assert!(!t.iter().any(|s| s == "Not enough data"), "{t:?}");
    assert!(t.iter().any(|s| s == "now"), "{t:?}");
    assert!(out
        .iter()
        .any(|d| matches!(d, UiDraw::Line(l) if l.color == INKS.series[0])));
    assert_eq!(targets(&ui, &places()).len(), 5);
}

#[test]
fn axis_scales_per_currency() {
    let mut gold = one(
        "",
        &[
            (2, 200_000, 250_000, 300_000),
            (9, 220_000, 400_000, 410_000),
        ],
        2,
    );
    gold.buckets[0].values[0].min = 180_000;
    gold.lowest_now = vec![295_000];
    let h = history(vec![
        gold,
        one("r24", &[(1, 1, 2, 3), (4, 2, 2, 2)], 1),
        one("r23", &[(1, 2, 2, 2)], 5),
    ]);
    let pl = plot(&h, MAX_STRIPS);
    // Each axis spans its own currency's sales; nothing is converted.
    let ends: Vec<(u64, u64)> = pl.strips.iter().map(|s| (s.lo, s.hi)).collect();
    assert_eq!(ends[0], (180_000, 410_000));
    assert_eq!(ends[1], (1, 3));
    // One value: widened around it.
    assert_eq!(ends[2], (1, 3));
    // Each strip maps its own ends to its own top and bottom.
    let r = Rect::new(10, 20, 200, 41);
    for st in &pl.strips {
        assert_eq!(y_of(st, r, st.hi), 20);
        assert_eq!(y_of(st, r, st.lo), 60);
    }
    assert_eq!(y_of(&pl.strips[1], r, 2), 40);
    assert_eq!(y_of(&pl.strips[0], r, 295_000), 40);
    // The lowest listed now is a rule across the gold strip, when it falls
    // inside it; an odd listing does not stretch the axis.
    let s = shapes(&h, &pl.strips[0], r);
    assert!(s.contains(&Shape::Now { y: 40 }), "{s:?}");
    let mut odd = h.clone();
    odd.series[0].lowest_now = vec![5_000];
    assert_eq!(plot(&odd, MAX_STRIPS).strips[0].lo, 180_000);
    let s = shapes(&odd, &pl.strips[0], r);
    assert!(!s.iter().any(|x| matches!(x, Shape::Now { .. })), "{s:?}");
    // Both axes' numbers are drawn, each in its own unit.
    let mut ui = MarketUi::new();
    ui.online = true;
    ui.state = Some(crate::market::source::MarketState {
        on: true,
        ..Default::default()
    });
    ui.open = true;
    ui.prices.open = true;
    ui.prices.history = Some(h);
    let (out, _) = draw_at(&ui, Point::new(0, 0));
    let t = texts(&out);
    for want in [
        "410,000",
        "180,000",
        "3",
        "1",
        "gold · 4 sold",
        "r24 · 2 sold",
    ] {
        assert!(t.iter().any(|s| s == want), "{want} in {t:?}");
    }
}

#[test]
fn price_check_opens_from_details_and_follows_the_span() {
    let (mut ui, mut src) = shown(0);
    let i = first_of(&ui, Group::Unique);
    ui.select(i);
    ui.pump(&mut src);
    let id = ui.selected.as_ref().unwrap().listing;
    assert!(window::ui_elements(&ui, &places())
        .iter()
        .any(|e| e.id == "market.price_check"));
    click(&mut ui, &mut src, Spot::PriceCheck);
    assert_eq!(
        src.asks.last().unwrap().1,
        MarketAsk::Prices {
            of: PriceOf::Listing(id),
            days: DEFAULT_DAYS
        }
    );
    let h = ui.prices.history.clone().expect("the answer");
    assert!(h.key.starts_with("u:") && h.enough(), "{h:?}");
    // The box is modal: its own parts only, and the graph's points.
    let ids: Vec<String> = window::ui_elements(&ui, &places())
        .into_iter()
        .map(|e| e.id)
        .collect();
    let points = ids
        .iter()
        .filter(|i| i.starts_with("market.price.point("))
        .count();
    assert_eq!(points, targets(&ui, &places()).len());
    assert!(points > 0);
    assert_eq!(
        ids[..5],
        [
            "market.price.close",
            "market.price.days(2)",
            "market.price.days(7)",
            "market.price.days(30)",
            "market.price.days(90)",
        ]
    );
    // Another span asks by the answer's key.
    click(&mut ui, &mut src, Spot::PriceDays(7));
    assert_eq!(
        src.asks.last().unwrap().1,
        MarketAsk::Prices {
            of: PriceOf::Key(h.key.clone()),
            days: 7
        }
    );
    let h7 = ui.prices.history.as_ref().unwrap();
    assert_eq!((h7.days, h7.bucket_ms), (7, DAY_MS));
    assert!(h7.sold < h.sold);
    click(&mut ui, &mut src, Spot::PriceDays(2));
    assert_eq!(ui.prices.history.as_ref().unwrap().bucket_ms, HOUR_MS);
    // A point of the graph shows its tip; Esc closes the box, the span stays.
    click(&mut ui, &mut src, Spot::PriceDays(90));
    let (r, ..) = targets(&ui, &places())[0];
    let (_, tip) = draw_at(&ui, Point::new(r.x + 3, r.y + 3));
    let tip = tip.expect("a point's tip");
    assert!(tip[1].contains("sale"), "{tip:?}");
    let esc = crate::ui::panel::UiEvent::Action(crate::ui::ActionId(0));
    assert!(window::event(&mut ui, &places(), esc, true));
    assert!(!ui.prices.open && ui.shown());
    assert_eq!(ui.prices.span(), 90);
}

#[test]
fn a_late_or_refused_check_is_handled() {
    let (mut ui, mut src) = shown(0);
    // Asked, then closed before the answer: the answer is dropped.
    ui.check_selected();
    for a in ui.take_asks() {
        let req = src.send(a.clone());
        ui.sent(&a, req);
    }
    ui.close_prices();
    for e in src.poll() {
        ui.apply(e);
    }
    assert!(ui.prices.history.is_none());
    // Two asks before the first answer: only the newest shows.
    ui.check_selected();
    ui.price_days(7);
    let asks = ui.take_asks();
    assert_eq!(asks.len(), 1, "{asks:?}");
    // A stash item the fake has not: the reason shows in the box.
    ui.check_prices(PriceOf::StashItem(7), "Grim Crown", 2, "Set");
    ui.pump(&mut src);
    assert_eq!(
        ui.prices.error.as_deref(),
        Some("That isn't in your stash any more.")
    );
    let (out, _) = draw_at(&ui, Point::new(0, 0));
    assert!(texts(&out)
        .iter()
        .any(|s| s == "That isn't in your stash any more."));
}

#[test]
fn a_buy_adds_its_sale_to_the_check() {
    let (mut ui, mut src) = shown(0);
    let i = ui
        .results
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|l| l.group == Group::Unique && l.price.is_some())
        .unwrap();
    ui.select(i);
    ui.pump(&mut src);
    ui.check_selected();
    ui.pump(&mut src);
    let before = ui.prices.history.clone().unwrap();
    ui.close_prices();
    let price = ui.selected.as_ref().unwrap().price.clone().unwrap();
    ui.ask_buy();
    ui.confirm_buy();
    ui.pump(&mut src);
    ui.check_prices(PriceOf::Key(before.key.clone()), "x", 0, "");
    ui.pump(&mut src);
    let after = ui.prices.history.as_ref().unwrap();
    assert_eq!(after.sold, before.sold + 1);
    assert_eq!(after.listed_now + 1, before.listed_now);
    assert_eq!(after.recent[0].price, price);
    assert_eq!(after.recent[0].at_ms, NOW);
}

#[test]
fn listed_now_gives_lowest_and_median_per_series() {
    let src = FakeSource::new(&|_| None, false, 0, true);
    let l = src.listings.iter().find(|l| l.group == Group::Set).unwrap();
    let key = fake_prices::price_key(l);
    let h = fake_prices::history(&src.sales, &src.listings, &key, "x", 30, NOW);
    let listed: Vec<_> = src
        .listings
        .iter()
        .filter(|x| fake_prices::price_key(x) == key)
        .collect();
    assert_eq!(h.listed_now as usize, listed.len());
    let priced: u32 = h.series.iter().map(|s| s.listed_now).sum();
    let offers_only = listed.iter().filter(|x| x.price.is_none()).count() as u32;
    assert_eq!(priced + offers_only, h.listed_now);
    for s in h.series.iter().filter(|s| s.listed_now > 0) {
        assert_eq!(s.lowest_now.len(), s.currencies.len());
        assert_eq!(s.median_now.len(), s.currencies.len());
        for (lo, med) in s.lowest_now.iter().zip(&s.median_now) {
            assert!(lo <= med);
        }
    }
    // Set items sold for one rune only.
    let sold: Vec<&PriceSeries> = h.series.iter().filter(|s| s.sold > 0).collect();
    assert_eq!(sold.len(), 1);
    assert_eq!(sold[0].currencies, ["r23"]);
    // The sell form's line.
    let mut ui = MarketUi::new();
    ui.coin_names.insert("r23".into(), "Rune 23".into());
    let line = similar_line(&ui, &h);
    assert!(
        line.starts_with(&format!("Similar active: {} listing", h.listed_now)),
        "{line}"
    );
    assert!(
        line.contains(" · last sold: ") && line.contains("Rune 23"),
        "{line}"
    );
}

#[test]
fn price_keys_and_buckets_follow_the_contract() {
    let src = FakeSource::new(&|_| None, false, 0, true);
    let key_of =
        |g: Group| fake_prices::price_key(src.listings.iter().find(|l| l.group == g).unwrap());
    assert_eq!(key_of(Group::Unique), "u:Ashen Crown");
    assert_eq!(key_of(Group::Stackable), "s:r24");
    assert_eq!(key_of(Group::Rare), "b:uap:2");
    // Hours for two days, days beyond; bucket 0 the newest.
    let h = fake_prices::history(&src.sales, &src.listings, "u:Ashen Crown", "x", 2, NOW);
    assert_eq!(h.bucket_ms, HOUR_MS);
    assert!(h
        .series
        .iter()
        .flat_map(|s| &s.buckets)
        .all(|b| b.index < 48));
    let h = fake_prices::history(&src.sales, &src.listings, "u:Ashen Crown", "x", 90, NOW);
    assert_eq!(h.bucket_ms, DAY_MS);
    assert!(h.recent.len() == 10);
    assert!(h.recent.windows(2).all(|w| w[0].at_ms >= w[1].at_ms));
    // A sparse key is not enough to draw at any span.
    let h = fake_prices::history(
        &src.sales,
        &src.listings,
        &key_of(Group::Rare),
        "x",
        90,
        NOW,
    );
    assert!(h.sold > 0 && !h.enough(), "{}", h.sold);
    // Days out of range are refused.
    let mut src = src;
    let req = src.send(MarketAsk::Prices {
        of: PriceOf::Stackable("r24".into()),
        days: 91,
    });
    let e = src.poll();
    assert!(
        matches!(&e[..], [crate::market::source::MarketEvent::Done(d)] if d.req == req && d.code.code() == 99),
        "{e:?}"
    );
}
