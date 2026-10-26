// D2RUST's own: the market window in the play UI.
//! The market window ([`crate::ui::market`]) installed in the play UI: a
//! panel open for good that draws and takes input only while the window
//! is shown, above the game's panels; and the stash panel's Market button
//! (greyed with its reason when the market cannot open). The window's
//! state lives in the shared UI state, so the Market button, the market
//! key ([`super::OriginalUi::market_key`]) and the app's pump reach it.

use super::SharedRef;
use crate::bridge::items;
use crate::bridge::world::ClientWorld;
use crate::ui::draw::UiDrawSink;
use crate::ui::geom::{Point, Rect};
use crate::ui::item_tip::{draw_tip_at, ItemTips, TipAnchor, TipLine};
use crate::ui::market::details::{slot_of, Compare};
use crate::ui::market::notices;
use crate::ui::market::price_graph;
use crate::ui::market::sell::{self, FormDraw, FormPlaces, SellItem};
use crate::ui::market::window::{self, DrawIn, Pen, Places, GREY};
use crate::ui::market::MarketUi;
use crate::ui::panel::{Panel, PanelId, UiCtx, UiEvent, UiResponse, WidgetId};
use crate::ui::panels::stash_cube::UI_STASH;
use crate::ui::panels::PanelOutput;
use crate::ui::shared_stash::{self as shared, layout::CELL, MarketPick, ShownItem};
use crate::ui::ui_ids::UiElement;
use crate::ui::PointerButton;

/// The adapter's id: not a UI state, open for good.
pub const MARKET_PANEL: PanelId = PanelId(0x117);

/// The kit's drawing helpers, `None` before the palette is known.
fn pen(sh: &super::Shared) -> Option<Pen<'_>> {
    Some(Pen {
        kit: sh.stash_kit?,
        fonts: sh.fonts.as_ref(),
        mouse: sh.mouse,
    })
}

/// The item the player wears in the shown listing's slot, with its tip.
fn compare(sh: &super::Shared, world: &ClientWorld, ui: &MarketUi) -> Option<Compare> {
    let l = ui.selected.as_ref()?;
    let (body, slot) = slot_of(&l.item.item_type)?;
    let tips = sh.items.tips.as_ref()?;
    let worn = items::local_items(world)
        .into_iter()
        .find(|i| i.mode == items::mode::BODY && i.body == body)?;
    let stream = items::stream(world, worn.key)?;
    let lines = tips.lines(stream);
    (!lines.is_empty()).then(|| Compare {
        slot: slot.into(),
        lines,
    })
}

/// The market window's panel.
pub(super) struct MarketPanelUi {
    pub(super) sh: SharedRef,
}

impl Panel for MarketPanelUi {
    fn id(&self) -> PanelId {
        MARKET_PANEL
    }

    fn rect(&self) -> Rect {
        self.sh.borrow().config.screen.frame_rect()
    }

    fn draw(&self, ctx: &UiCtx, out: &mut dyn UiDrawSink) {
        let sh = self.sh.borrow();
        let ui = &sh.market;
        let Some(pen) = pen(&sh) else { return };
        if !ui.shown() {
            if sell::form_shown(ui, selling(&sh)) {
                draw_form(&sh, pen, out);
                if ui.prices.open {
                    draw_form_check(&sh, pen, out);
                }
            }
            return;
        }
        let compare = compare(&sh, ctx.world, ui);
        let i = DrawIn {
            ui,
            places: Places::of(&sh.config.screen),
            pen,
            tips: sh.items.tips.as_ref(),
            compare: compare.as_ref(),
        };
        window::draw(&i, out);
        if let Some((lines, anchor)) = window::hover_tip(&i) {
            let s = sh.config.screen;
            draw_tip_at(&lines, anchor, (s.w, s.h), sh.fonts.as_ref(), out);
        }
    }

    fn hit(&self, _p: Point) -> Option<WidgetId> {
        None
    }

    fn event(&mut self, e: UiEvent, _ctx: &UiCtx) -> UiResponse {
        let mut sh = self.sh.borrow_mut();
        let esc = matches!(e, UiEvent::Action(a)
            if a == crate::ui::ActionId(crate::controls::Action::GameMenu.index() as u16));
        if sell::form_shown(&sh.market, selling(&sh)) {
            let form = FormPlaces::of(&sh.config.screen);
            let was_open = sh.market.shown();
            let taken = if sh.market.prices.open {
                let places = Places::of(&sh.config.screen);
                price_graph::form_event(&mut sh.market, &places, e, esc)
            } else {
                sell::event(&mut sh.market, &form, e, esc)
            };
            if !was_open && sh.market.shown() {
                sh.outputs.push(PanelOutput::Sound(4));
            }
            return if taken {
                UiResponse::Consumed
            } else {
                UiResponse::Ignored
            };
        }
        let places = Places::of(&sh.config.screen);
        let was_open = sh.market.shown();
        let taken = window::event(&mut sh.market, &places, e, esc);
        if was_open && !sh.market.shown() {
            sh.outputs.push(PanelOutput::Sound(4));
        }
        if taken {
            UiResponse::Consumed
        } else {
            UiResponse::Ignored
        }
    }

    fn ui_elements(&self, _ctx: &UiCtx) -> Vec<UiElement> {
        let sh = self.sh.borrow();
        if sell::form_shown(&sh.market, selling(&sh)) {
            if sh.market.prices.open {
                let places = Places::of(&sh.config.screen);
                return price_graph::spots(&sh.market, &places)
                    .into_iter()
                    .map(|(s, r)| UiElement::new(s.id(&sh.market), r))
                    .collect();
            }
            return sell::ui_elements(&sh.market, &FormPlaces::of(&sh.config.screen));
        }
        window::ui_elements(&sh.market, &Places::of(&sh.config.screen))
    }
}

/// The stash panel is open on the shared stash's Market category: the New
/// listing form stands beside it.
fn selling(sh: &super::Shared) -> bool {
    sh.shared_stash.selling() && sh.states.is_open(UI_STASH)
}

/// Draws the New listing form, and the tip of the item on its Sell slot
/// under the mouse.
fn draw_form(sh: &super::Shared, pen: Pen, out: &mut dyn UiDrawSink) {
    let places = FormPlaces::of(&sh.config.screen);
    let picture = |it: &SellItem, at: Point, out: &mut dyn UiDrawSink| {
        let code = shared::draw::code_key(&it.code);
        let pic = sh.items.picture(
            &sh.tables.files,
            it.look.as_deref(),
            code,
            (at.x, at.y),
            (CELL, CELL),
            false,
        );
        match pic {
            Some((d, _)) => out.push(d),
            None => {
                let (w, h) = (CELL * i32::from(it.size.0), CELL * i32::from(it.size.1));
                let r = Rect::new(at.x, at.y, w as u16, h as u16);
                pen.label(r, &it.code, window::SMALL, window::WHITE, out);
            }
        }
    };
    let i = FormDraw {
        ui: &sh.market,
        places,
        pen,
        picture: &picture,
    };
    sell::draw(&i, out);
    let f = &sh.market.sell;
    let slot = places.slot();
    let (Some(it), None, false) = (&f.item, &f.held, sh.market.prices.open) else {
        return;
    };
    let s = sh.config.screen;
    // The "Similar active" line: all of it.
    let line = places.similar();
    if let Some(h) = price_graph::sell_history(&sh.market).filter(|_| line.contains(sh.mouse)) {
        let lines: Vec<TipLine> = price_graph::similar_tip(&sh.market, h)
            .into_iter()
            .map(|l| TipLine::new(l, window::WHITE))
            .collect();
        let anchor = TipAnchor {
            x: sh.mouse.x,
            top: line.y,
            bottom: line.y + i32::from(line.h),
        };
        draw_tip_at(&lines, anchor, (s.w, s.h), sh.fonts.as_ref(), out);
        return;
    }
    if !slot.contains(sh.mouse) {
        return;
    }
    let mut lines = match (&sh.items.tips, it.look.as_deref()) {
        (Some(t), Some(look)) => t.lines(look),
        _ => Vec::new(),
    };
    if lines.is_empty() {
        lines.push(TipLine::new(&it.name, window::WHITE));
    }
    lines.push(TipLine::new("Left: back to the stash page", window::GREY));
    let anchor = TipAnchor {
        x: slot.x + i32::from(slot.w) / 2,
        top: slot.y,
        bottom: slot.y + i32::from(slot.h),
    };
    draw_tip_at(&lines, anchor, (s.w, s.h), sh.fonts.as_ref(), out);
}

/// Draws the price check box over the New listing form (its Price check),
/// and the tip of the graph point under the mouse.
fn draw_form_check(sh: &super::Shared, pen: Pen, out: &mut dyn UiDrawSink) {
    let i = DrawIn {
        ui: &sh.market,
        places: Places::of(&sh.config.screen),
        pen,
        tips: sh.items.tips.as_ref(),
        compare: None,
    };
    price_graph::draw(&i, out);
    if let Some((lines, anchor)) = price_graph::hover_tip(&i) {
        let s = sh.config.screen;
        draw_tip_at(&lines, anchor, (s.w, s.h), sh.fonts.as_ref(), out);
    }
}

/// A shared stash item as the Sell slot shows it: its tip's lines after
/// the name are its stats.
fn sell_item(it: &ShownItem, tips: Option<&ItemTips>) -> SellItem {
    let lines = match (tips, it.look.as_deref()) {
        (Some(t), Some(look)) => t.lines(look),
        _ => Vec::new(),
    };
    // A store record without a name shows its code; the base name reads
    // better on the form and in My shop.
    let name = match tips {
        Some(t) if it.name == it.code => t.base_name(&it.code).unwrap_or_else(|| it.name.clone()),
        _ => it.name.clone(),
    };
    let stats = lines
        .iter()
        .map(|l| String::from_utf16_lossy(&l.text))
        .filter(|l| !l.trim().is_empty() && *l != it.name && *l != name)
        .collect();
    SellItem {
        id: it.id.0,
        name,
        code: it.code.clone(),
        group: sell::group_of_quality(it.quality),
        size: it.size,
        look: it.look.clone(),
        level_req: it.level_req,
        sockets: it.sockets,
        ethereal: it.ethereal,
        stats,
    }
}

/// A press on the Market category's page (`shared::press` noted it): the
/// item is picked up for the Sell slot, or with Ctrl put straight on it; a
/// character's item on the cursor is refused; an empty cell puts the held
/// item back.
pub(super) fn take_pick(sh: &mut super::Shared) {
    let Some(pick) = sh.shared_stash.market_pick.take() else {
        return;
    };
    let f = &mut sh.market.sell;
    match pick {
        MarketPick::Cursor => f.refuse_cursor(),
        MarketPick::Empty => f.drop_held(),
        MarketPick::Item { id, ctrl } => {
            if f.escrowed.contains(&id.0) {
                return;
            }
            let Some(shown) = sh.shared_stash.items.iter().find(|i| i.id == id) else {
                return;
            };
            let it = sell_item(shown, sh.items.tips.as_ref());
            if ctrl {
                f.put(it);
            } else if f.held.as_ref().is_some_and(|h| h.id == it.id) {
                f.drop_held();
            } else {
                f.hold(it);
            }
            sh.outputs.push(PanelOutput::Sound(4));
        }
    }
}

/// The stash panel's Market button: below the shared stash's category
/// buttons, on the panel's left edge.
pub(super) fn button_rect(sh: &super::Shared) -> Rect {
    crate::ui::shared_stash::Places::of(&sh.config.screen).category(4)
}

/// Draws the Market button: greyed when the market cannot open, marked
/// with the count while Deliveries wait.
pub(super) fn draw_button(sh: &super::Shared, out: &mut dyn UiDrawSink) {
    let Some(pen) = pen(sh) else { return };
    let r = button_rect(sh);
    if sh.market.availability().why_not().is_some() {
        pen.fill(r, pen.kit.field, out);
        pen.frame(r, pen.kit.edge, out);
        pen.label(r, "Market", window::LABEL, GREY, out);
    } else {
        let on = sh.market.open || sh.shared_stash.selling();
        pen.button(r, "Market", on, out);
    }
    if let Some(n) = notices::mark(&sh.market) {
        let s = if n > 99 {
            "99+".to_owned()
        } else {
            n.to_string()
        };
        let w = (pen.width(window::SMALL, &s) + 6).max(12);
        let m = Rect::new(r.x + i32::from(r.w) - w + 3, r.y - 4, w as u16, 11);
        pen.fill(m, pen.kit.on, out);
        pen.frame(m, pen.kit.gold, out);
        pen.label(m, &s, window::SMALL, window::GOLD, out);
    }
}

/// The Market button's tip under the mouse: why the market cannot open,
/// or what waits in Deliveries.
pub(super) fn button_tip(sh: &super::Shared) -> Option<(Vec<TipLine>, TipAnchor)> {
    let r = button_rect(sh);
    if !r.contains(sh.mouse) {
        return None;
    }
    let why = match sh.market.availability().why_not() {
        Some(why) => why.to_owned(),
        None => {
            let n = notices::mark(&sh.market)?;
            format!("{n} waiting in Deliveries")
        }
    };
    let anchor = TipAnchor {
        x: r.x + i32::from(r.w) / 2,
        top: r.y,
        bottom: r.y + i32::from(r.h),
    };
    Some((vec![TipLine::new(why, 0)], anchor))
}

/// A press on the Market button: with the shared stash there it shows its
/// Market category (selling, the New listing form beside it) or goes back
/// to the item pages; without one it opens or closes the window. A greyed
/// button takes the press and does nothing.
pub(super) fn button_event(sh: &mut super::Shared, e: UiEvent) -> bool {
    let UiEvent::Press {
        button: PointerButton::Left,
        at,
    } = e
    else {
        return false;
    };
    if !button_rect(sh).contains(at) {
        return false;
    }
    let open = sh.market.availability().why_not().is_none();
    if open && super::stash_ui::shared_on(sh) && sh.states.is_open(UI_STASH) {
        let st = &mut sh.shared_stash;
        st.category.0 = if st.selling() {
            sh.market.sell.drop_held();
            shared::Category::Stash
        } else {
            shared::Category::Market
        };
        st.ask(shared::StashAsk::Refresh);
        sh.market.close();
        sh.outputs.push(PanelOutput::Sound(4));
        return true;
    }
    if sh.market.toggle().is_ok() {
        sh.outputs.push(PanelOutput::Sound(4));
    }
    true
}

/// The Market button's UI element, greyed with its reason.
pub(super) fn button_element(sh: &super::Shared) -> UiElement {
    let e = UiElement::new("stash.market", button_rect(sh));
    match sh.market.availability().why_not() {
        Some(why) => e.idle_if(true, why),
        None => e,
    }
}
