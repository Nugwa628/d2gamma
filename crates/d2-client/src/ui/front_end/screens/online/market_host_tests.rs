use super::*;
use crate::ui::draw::{RectRequest, TextRequest, TextStyle};
use crate::ui::front_end::screens::online::{NoLink, Online};
use crate::ui::market::MarketTab;

/// A host on the fake market, its state answered.
fn fake_host() -> MarketHost {
    let online = Online::new(Box::<NoLink>::default(), Vec::new());
    online.borrow_mut().market_parts = Some(Rc::new(|| MarketParts {
        fake: true,
        ..MarketParts::default()
    }));
    let mut h = MarketHost::new();
    h.start(&online);
    h.tick();
    h
}

fn texts(items: &[DrawItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|i| match i {
            DrawItem::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Rule (plan L18): without a market on the link the button stays greyed
/// with the reason; nothing opens.
#[test]
fn no_market_on_the_link_stays_closed() {
    let online = Online::new(Box::<NoLink>::default(), Vec::new());
    let mut h = MarketHost::new();
    h.start(&online);
    h.tick();
    assert!(!h.can_open());
    assert_eq!(h.tip(), NO_MARKET_TIP);
    assert!(!h.toggle());
    assert!(!h.shown());
    assert!(h.draw().is_empty());
}

/// Rule (plan L18): the market asks its state on start; until it answers
/// the tip says so; then it opens on Browse with a search out.
#[test]
fn the_market_opens_once_its_state_answered() {
    let online = Online::new(Box::<NoLink>::default(), Vec::new());
    online.borrow_mut().market_parts = Some(Rc::new(|| MarketParts {
        fake: true,
        ..MarketParts::default()
    }));
    let mut h = MarketHost::new();
    h.start(&online);
    assert_eq!(h.tip(), ASKING_TIP);
    assert!(!h.toggle());
    h.tick();
    assert_eq!(h.tip(), OPEN_TIP);
    assert!(h.toggle());
    assert!(h.shown());
    assert_eq!(h.ui.tab, MarketTab::Browse);
    h.tick();
    assert!(h.ui.results.is_some(), "the fake answered the search");
    let shown = texts(&h.draw());
    assert!(shown.iter().any(|t| t == "Market"), "{shown:?}");
    assert!(h.ui_elements().iter().any(|e| e.id == "market.tab(Browse)"));
}

/// Rule (plan L18): Esc closes a box first, then the window; the toggle
/// closes it too.
#[test]
fn esc_and_toggle_close_the_window() {
    let mut h = fake_host();
    assert!(h.toggle());
    h.ui.focus = Some(crate::ui::market::TextBox::Search);
    assert!(h.esc());
    assert!(h.shown(), "the first Esc leaves the search box");
    assert!(h.esc());
    assert!(!h.shown());
    assert!(h.toggle());
    assert!(h.toggle());
    assert!(!h.shown());
}

/// Rule (plan L18, market decision 1): selling and claiming stay at the
/// stash: My shop and Deliveries say so under the window, Browse doesn't;
/// a claim is refused with the window's own reason.
#[test]
fn sell_and_claim_stay_at_the_stash() {
    let mut h = fake_host();
    h.toggle();
    h.tick();
    assert!(!texts(&h.draw()).iter().any(|t| t == STASH_NOTE));
    for tab in [MarketTab::MyShop, MarketTab::Deliveries] {
        h.ui.set_tab(tab);
        h.tick();
        assert!(h.stash_note());
        assert!(
            texts(&h.draw()).iter().any(|t| t == STASH_NOTE),
            "{tab:?}: {:?}",
            texts(&h.draw())
        );
    }
    assert!(!h.ui.at_stash);
    assert_eq!(
        h.ui.claim_blocked(),
        Some(crate::ui::market::deliveries::AT_STASH)
    );
    // The strip sits under the window, inside the lobby's canvas.
    let (w, n) = (places().window(), MarketHost::note_rect());
    assert_eq!(n.y, w.y + i32::from(w.h));
    assert!(n.y + i32::from(n.h) <= 600);
}

/// Rule (plan L18): typing goes to the window's focused box; the market
/// key needs Ctrl held.
#[test]
fn typing_and_the_market_key() {
    let mut h = fake_host();
    assert!(!h.is_chord(u16::from(b'I')));
    h.ctrl(true);
    assert!(h.is_chord(u16::from(b'I')));
    assert!(!h.is_chord(u16::from(b'K')));
    h.ctrl(false);
    h.toggle();
    h.ui.focus = Some(crate::ui::market::TextBox::Search);
    for u in "shako".encode_utf16() {
        assert!(h.char(u));
    }
    assert_eq!(h.ui.filters.text, "shako");
    assert!(h.char(8));
    assert_eq!(h.ui.filters.text, "shak");
}

/// A fresh lobby entry starts the market over but keeps the player's
/// views and default filters.
#[test]
fn a_fresh_entry_keeps_the_players_views() {
    let mut h = fake_host();
    h.toggle();
    h.ui.set_view(crate::ui::market::ResultsView::Table);
    h.ui.filters.text = "rune".into();
    h.ui.save_default();
    let online = Online::new(Box::<NoLink>::default(), Vec::new());
    h.start(&online);
    assert!(!h.shown());
    assert_eq!(h.ui.view, crate::ui::market::ResultsView::Table);
    assert_eq!(h.ui.default_filters.text, "rune");
    assert!(h.ui.state.is_none(), "the realm's state is asked again");
}

/// The window's draws become the front end's items: rectangles keep their
/// mode, straight lines are one shade, a slanted line a pixel per step,
/// text keeps pen, font and colour.
#[test]
fn draws_become_front_items() {
    let draws = vec![
        UiDraw::Rect(RectRequest::sized(10, 20, 30, 4, 7, 5)),
        UiDraw::Rect(RectRequest::sized(10, 20, 0, 4, 7, 5)),
        UiDraw::Line(LineRequest {
            x0: 5,
            y0: 9,
            x1: 1,
            y1: 9,
            color: 3,
        }),
        UiDraw::Line(LineRequest {
            x0: 4,
            y0: 2,
            x1: 4,
            y1: 6,
            color: 3,
        }),
        UiDraw::Line(LineRequest {
            x0: 0,
            y0: 0,
            x1: 3,
            y1: 3,
            color: 9,
        }),
        UiDraw::Text(TextRequest {
            text: "Buy".encode_utf16().collect(),
            at: Point::new(40, 50),
            style: TextStyle { font: 6, color: 4 },
            opts: Default::default(),
            clip: Rect::new(0, 0, 800, 600),
        }),
    ];
    let items = front_items(draws);
    let shade = |x, y, w, h, color, mode| DrawItem::Shade {
        at: Point::new(x, y),
        w,
        h,
        color,
        mode,
    };
    assert_eq!(items[0], shade(10, 20, 30, 4, 7, 5));
    // The empty rectangle is left out.
    assert_eq!(items[1], shade(1, 9, 5, 1, 3, 5));
    assert_eq!(items[2], shade(4, 2, 1, 5, 3, 5));
    for k in 0..4 {
        assert_eq!(items[3 + k], shade(k as i32, k as i32, 1, 1, 9, 5));
    }
    assert_eq!(
        items[7],
        DrawItem::Text {
            string_id: 0,
            text: "Buy".into(),
            font: 6,
            at: Point::new(40, 50),
            label: None,
            color: 4,
            boxed: None,
        }
    );
    assert_eq!(items.len(), 8);
}
