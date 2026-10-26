// D2RUST's own: the market window's frame step.
//! Each frame the market window's asks go to the market and its answers
//! come back ([`crate::ui::original::OriginalUi::pump_market`]). The game
//! is online for the window when its link is a realm's; the market itself
//! is the [`MarketLink`] resource: in a realm's game the realm's own
//! (`market::realm::RealmSource`, online builds, installed with the game),
//! whose state says whether the realm runs it; off, the window does not
//! open and the Market button says why.
//!
//! Tests and offscreen shots turn the in-process fake market on with
//! `D2RUST_MARKET=fake` ([`crate::market::fake::FakeSource`]): the game
//! counts as online and the fake answers.

use bevy::prelude::{App, Local, NonSendMut, Res, ResMut, Resource, Update};

use crate::bridge::link::ServerLink;
use crate::bridge::BridgeResource;
use crate::market::fake::FakeSource;
use crate::market::source::MarketSource;
use crate::market::trade::OffersWhich;
use crate::ui::market::{deliveries, notices, MarketUi};
use crate::world_view::present::WorldViewUi;

/// The environment variable that turns the fake market on.
pub const ENV: &str = "D2RUST_MARKET";

/// The market the window asks.
#[derive(Resource, Default)]
pub struct MarketLink(pub Option<Box<dyn MarketSource>>);

/// Whether `D2RUST_MARKET=fake` asks for the fake market.
pub fn fake_wanted() -> bool {
    crate::launch::env_os(ENV).is_some_and(|v| v == "fake")
}

/// The market's frame system, once per app.
pub fn register_market(app: &mut App) {
    app.init_resource::<MarketLink>();
    app.add_systems(Update, run_market);
}

fn run_market(
    mut link: ResMut<MarketLink>,
    bridge: Option<Res<BridgeResource>>,
    ui: Option<NonSendMut<WorldViewUi>>,
    mut fake: Local<Option<bool>>,
) {
    let Some(mut ui) = ui else {
        return;
    };
    let Some(o) = ui.original.as_mut() else {
        return;
    };
    let fake = *fake.get_or_insert_with(fake_wanted);
    if fake && link.0.is_none() {
        let mut f = FakeSource::new(&|_| None, false, 0, true);
        // A made-up shop in every My shop tab, for the shots.
        f.seed_shop(&|_| None);
        link.0 = Some(Box::new(f));
    }
    let remote = bridge.as_ref().is_some_and(|b| b.0.link().is_remote());
    o.pump_market(remote || fake, link.0.as_deref_mut());
}

/// The market window in a control `state` reply: what a driver checks
/// (open, view, page, counts, the shown listing, the confirm, the offer
/// form, the Offers and Deliveries tabs, the Market button's mark).
pub fn state(ui: &MarketUi) -> serde_json::Value {
    let r = ui.results.as_ref();
    let groups: serde_json::Map<String, serde_json::Value> = ui
        .facets
        .groups
        .iter()
        .map(|(g, n)| (g.key().to_owned(), (*n).into()))
        .collect();
    serde_json::json!({
        "online": ui.online,
        "why_not": ui.availability().why_not(),
        "open": ui.shown(),
        "tab": ui.tab.key(),
        "view": ui.view.key(),
        "page": ui.page,
        "pages": ui.pages(),
        "page_size": ui.page_size(),
        "total": r.map(|r| r.total),
        "rows": r.map(|r| r.rows.len()),
        "groups": groups,
        "selected": ui.selected.as_ref().map(|l| l.listing),
        "confirm": ui.confirm.as_ref().map(|c| serde_json::json!({
            "listing": c.listing, "buyer_fee": c.buyer_fee, "fee_rev": c.fee_rev,
        })),
        "note": ui.note,
        "deliveries_waiting": ui.state.as_ref().map(|s| s.deliveries_waiting),
        "live_offers_to_me": ui.state.as_ref().map(|s| s.live_offers_to_me),
        "mark": notices::mark(ui),
        "at_stash": ui.at_stash,
        "form": ui.form.as_ref().map(|f| serde_json::json!({
            "counter": f.is_counter(), "review": f.review, "gold": f.gold,
            "items": f.items.len(), "error": f.error,
        })),
        "offers": OffersWhich::ALL.iter().map(|w| {
            (w.key().to_owned(), ui.offers.page(*w).map(|p| p.rows.len()).into())
        }).collect::<serde_json::Map<String, serde_json::Value>>(),
        "picked_offer": ui.offers.picked.map(|(_, id)| id),
        "deal": ui.offers.deal.as_ref().map(|d| d.offer),
        "deliveries": ui.deliveries.page.as_ref().map(|p| p.rows.len()),
        "delivery_lines": ui.deliveries.page.as_ref().map(|p| deliveries::lines(p).len()),
        "active_listings": ui.state.as_ref().map(|s| s.active_listings),
        "sell": {
            "item": ui.sell.item.as_ref().map(|i| i.id),
            "held": ui.sell.held.as_ref().map(|i| i.id),
            "line": ui.sell.line.as_ref().map(|l| l.0.clone()),
            "escrowed": ui.sell.escrowed,
        },
        "shop": {
            "tab": ui.shop.tab.key(),
            "counts": ui.shop.view.as_ref().map(|v| v.counts),
            "slots_used": ui.shop.view.as_ref().map(|v| v.slots_used),
            "rows": ui.shop.rows().len(),
            "selected": ui.shop.selected.len(),
            "line": ui.shop.line.as_ref().map(|l| l.0.clone()),
        },
        "prices": prices(ui),
    })
}

/// The price check box in a `state` reply: open, the span, the Sell slot
/// item it is of and the sell form's line, and the answer's series (shape,
/// currencies, sales) and whether it is enough to draw.
fn prices(ui: &MarketUi) -> serde_json::Value {
    use crate::market::prices::PriceShape;
    use crate::ui::market::price_graph;
    let p = &ui.prices;
    let h = p.history.as_ref();
    serde_json::json!({
        "open": p.open,
        "days": p.span(),
        "waiting": p.queued || p.req.is_some(),
        "error": p.error,
        "stash": p.stash,
        "similar": price_graph::sell_history(ui).map(|h| price_graph::similar_line(ui, h)),
        "key": h.map(|h| h.key.clone()),
        "sold": h.map(|h| h.sold),
        "listed_now": h.map(|h| h.listed_now),
        "enough": h.map(|h| h.enough()),
        "strips": h.map(|h| price_graph::plot(h, price_graph::MAX_STRIPS).strips.len()),
        "series": h.map(|h| h.series.iter().map(|s| serde_json::json!({
            "shape": match s.shape {
                PriceShape::One => "one",
                PriceShape::Bundle => "bundle",
                PriceShape::WithItems => "items",
            },
            "currencies": s.currencies,
            "sold": s.sold,
            "listed_now": s.listed_now,
        })).collect::<Vec<_>>()),
    })
}
