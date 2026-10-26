//! The stash (ui 0x19) installed in the play UI: the [`StashPanel`] art and
//! close button, the page-4 items and grid click ([`ItemsUi::draw_stash`],
//! [`ItemsUi::press_stash`]) and the close press / release rules
//! ([`StashCubeInput`]). S→C 0x77 0x10 opens the state (`msg_ui`); the C→S
//! intents leave through the root. The client decides nothing: the server
//! validates every move.
//!
//! Preview fills (D2RUST's own, unverified): the GoldMax line reads the
//! string table by id (`ctx.strings`); the stash gold button opens the
//! withdraw dialog (kind 4) and the inventory gold button the deposit
//! (kind 3, `gold_dialog`, whose frame-wide panel sees the right half).
//!
//! D2RUST's own: the shared stash ([`crate::ui::shared_stash`]) is drawn on
//! the panel and takes its presses first; on a shared page it covers the
//! game's grid. The inventory gets one button, "Send stackables to stash".
//! The panel has the Market button too ([`super::market_ui`]).
//!
//! A mod game's own stash panel (`crate::mods::stash`) takes the shared
//! stash's place: it draws over the game's panel, gets the panel's presses
//! and keys first (all but the close button's) and Ctrl+Left on inventory
//! items; its answers go out as mod messages, sounds or store asks.

use super::SharedRef;
use crate::bridge::items;
use crate::bridge::items::ItemView;
use crate::bridge::world::ClientWorld;
use crate::mods::stash::{FieldKind, Keys, ModStash, Pressed, StashCtx, StashField, StashOut};
use crate::ui::draw::UiDrawSink;
use crate::ui::geom::{Point, Rect};
use crate::ui::gold::GoldKind;
use crate::ui::item_moves::{self, Button, Shortcut};
use crate::ui::item_tip::{draw_tip_at, TipAnchor, TipLine};
use crate::ui::panel::{ClientIntent, Panel, PanelId, UiCtx, UiEvent, UiResponse, WidgetId};
use crate::ui::panels::stash_cube::{StashPanel, UI_STASH};
use crate::ui::panels::stash_input::{stash_gold_rect_hit, Pointer, StashCubeInput, GOLD_MAX_FONT};
use crate::ui::panels::stash_items::STASH_PAGE;
use crate::ui::panels::PanelOutput;
use crate::ui::shared_stash::{self as shared, draw::DrawIn, Mods, Places, StashAsk};
use crate::ui::PointerButton;
use d2_sim::world::stash::STASH_CAP;

/// D2RUST's shared stash parts are shown and pressed: a store is there
/// and no mod game's stash panel takes their place.
pub(super) fn shared_on(sh: &super::Shared) -> bool {
    sh.shared_stash.on && sh.mod_stash.is_none()
}

/// Runs `f` on the mod game's stash panel with its context; `None`
/// without one.
fn with_mod<R>(
    sh: &mut super::Shared,
    world: &ClientWorld,
    f: impl FnOnce(&mut ModStash, &StashCtx) -> R,
) -> Option<R> {
    let ctx = StashCtx {
        world,
        screen: sh.config.screen,
        files: &sh.tables.files,
        fonts: sh.fonts.as_ref(),
        mouse: sh.mouse,
        cursor: items::cursor_item(world).map(|i| i.key.guid),
    };
    let m = sh.mod_stash.as_mut()?;
    Some(f(m, &ctx))
}

fn mod_ctx<'a>(sh: &'a super::Shared, world: &'a ClientWorld) -> StashCtx<'a> {
    StashCtx {
        world,
        screen: sh.config.screen,
        files: &sh.tables.files,
        fonts: sh.fonts.as_ref(),
        mouse: sh.mouse,
        cursor: items::cursor_item(world).map(|i| i.key.guid),
    }
}

/// The mod panel's answers: mod messages out as C->S envelopes (one too
/// large for an envelope is dropped), sounds, store asks.
fn send_outs(sh: &mut super::Shared, outs: Vec<StashOut>) {
    for o in outs {
        match o {
            StashOut::Send(payload) => {
                if let Ok(bytes) = crate::bridge::intent::mod_message(&payload) {
                    sh.outputs.push(PanelOutput::Intent(ClientIntent(bytes)));
                }
            }
            StashOut::Sound(id) => sh.outputs.push(PanelOutput::Sound(id)),
            StashOut::Ask(a) => sh.shared_stash.ask(a),
        }
    }
}

/// The mod panel's cover rectangle (`StashPanelMod::cover`): `None`
/// without a mod panel or when it covers nothing.
fn mod_cover(sh: &super::Shared, world: &ClientWorld) -> Option<Rect> {
    let m = sh.mod_stash.as_ref()?;
    m.panel.cover(&mod_ctx(sh, world))
}

/// The engine's plain box of an open mod panel field, centred on `area`,
/// in the shared stash's colours.
fn draw_field_box(sh: &super::Shared, f: &StashField, area: Rect, out: &mut dyn UiDrawSink) {
    let Some(k) = sh.stash_kit else {
        return;
    };
    let w = area.w.min(220);
    let r = Rect::new(
        area.x + (i32::from(area.w) - i32::from(w)) / 2,
        area.y + i32::from(area.h) / 2 - 20,
        w,
        40,
    );
    shared::draw::fill(r, k.on, out);
    shared::draw::frame(r, k.gold, out);
    let title = match f.kind {
        FieldKind::Rename { .. } => "Page name".to_string(),
        FieldKind::Amount { max, .. } => format!("How many? (1-{max})"),
    };
    // Tan title, white text (the string tables' colours 7 and 0).
    for (row, (t, color)) in [(title, 7), (format!("{}_", f.text), 0)]
        .into_iter()
        .enumerate()
    {
        let units: Vec<u16> = t.encode_utf16().collect();
        let tw = sh
            .fonts
            .as_ref()
            .and_then(|m| m.width_a(1, &units))
            .unwrap_or(units.len() as i32 * 6);
        let x = r.x + (i32::from(r.w) - tw) / 2;
        let y = r.y + 15 + 18 * row as i32;
        out.push(crate::ui::panels::text(units, x, y, 1, color));
    }
}

/// The shared stash's drawing inputs, `None` before the palette is known.
fn draw_in<'a>(sh: &'a super::Shared, ctx: &UiCtx) -> Option<DrawIn<'a>> {
    Some(DrawIn {
        st: &sh.shared_stash,
        places: Places::of(&sh.config.screen),
        kit: sh.stash_kit?,
        items: &sh.items,
        files: &sh.tables.files,
        fonts: sh.fonts.as_ref(),
        mouse: sh.mouse,
        holding: items::cursor_item(ctx.world).is_some(),
        shortcuts: sh.items.shortcut_set(),
        skip: &sh.market.sell.escrowed,
        faded: &sh.market.sell.faded,
    })
}

/// What the shared stash makes of a Ctrl+Left on inventory item `it` with
/// the stash open (`item_moves`): `StackSlot` (4) sends a stackable with a
/// slot to that slot; with a shared page or stackable page shown, `Stash`
/// (1) puts any other item on the shared item page (the game's own stash
/// is covered then). `None`: the framework's own Ctrl+Left runs.
pub(super) fn inventory_move(
    sh: &super::Shared,
    world: &ClientWorld,
    it: &ItemView,
) -> Option<(Shortcut, StashAsk)> {
    let st = &sh.shared_stash;
    if !shared_on(sh)
        || !sh.states.is_open(UI_STASH)
        || it.mode != items::mode::STORED
        || it.page != item_moves::page::INVENTORY
        || items::cursor_item(world).is_some()
    {
        return None;
    }
    let guid = it.key.guid;
    let code = it
        .code
        .map(|c| String::from_utf8_lossy(&c).trim_end().to_owned());
    let slotted = code
        .as_deref()
        .zip(st.catalog.as_ref())
        .is_some_and(|(code, cat)| cat.slot(code).is_some());
    if slotted && sh.items.shortcut_on(Shortcut::StackSlot) {
        return Some((Shortcut::StackSlot, StashAsk::PutStack { guid }));
    }
    if st.covers() && sh.items.shortcut_on(Shortcut::Stash) {
        let ask = StashAsk::PutItem {
            guid,
            page: st.page(),
            at: None,
        };
        return Some((Shortcut::Stash, ask));
    }
    None
}

/// A Ctrl+Left on inventory item `it` the shared stash takes
/// ([`inventory_move`]): true when its ask is queued.
pub(super) fn inventory_shortcut(
    sh: &mut super::Shared,
    world: &ClientWorld,
    it: Option<&ItemView>,
) -> bool {
    if !(sh.items.ctrl && !sh.items.shift) {
        return false;
    }
    if sh.mod_stash.is_some() {
        return mod_store(sh, world, it);
    }
    let Some((_, ask)) = it.and_then(|it| inventory_move(sh, world, it)) else {
        return false;
    };
    sh.shared_stash.ask(ask);
    sh.outputs.push(PanelOutput::Sound(4));
    true
}

/// Ctrl+Left on inventory item `it` with a mod stash panel: the mod's
/// [`StashInput::Store`](crate::mods::stash::StashInput::Store) while the
/// stash is open and nothing is held. True when the mod answered.
fn mod_store(sh: &mut super::Shared, world: &ClientWorld, it: Option<&ItemView>) -> bool {
    let Some(it) =
        it.filter(|it| it.mode == items::mode::STORED && it.page == item_moves::page::INVENTORY)
    else {
        return false;
    };
    if !sh.states.is_open(UI_STASH) || items::cursor_item(world).is_some() {
        return false;
    }
    let guid = it.key.guid;
    match with_mod(sh, world, |m, c| m.store(c, guid)).flatten() {
        Some(outs) => {
            send_outs(sh, outs);
            true
        }
        None => false,
    }
}

/// The Ctrl+Left tip line of inventory item `it` when the shared stash
/// takes that click ([`inventory_move`]).
pub(super) fn inventory_hint(
    sh: &super::Shared,
    world: &ClientWorld,
    it: &ItemView,
) -> Option<TipLine> {
    let (s, ask) = inventory_move(sh, world, it)?;
    let words = match ask {
        StashAsk::PutStack { .. } => "Move to its stash slot",
        _ => "Move to shared stash",
    };
    Some(item_moves::hint_line(s, words))
}

/// The shared stash's tips (on the stash panel) and the send button's (on
/// the inventory), over the panels.
pub(super) fn draw_tips(sh: &super::Shared, ctx: &UiCtx, out: &mut dyn UiDrawSink) {
    if !shared_on(sh) {
        return;
    }
    let s = sh.config.screen;
    let tip = if sh.states.is_open(UI_STASH) {
        draw_in(sh, ctx).and_then(|i| shared::draw::hover_tip(&i))
    } else {
        None
    };
    let tip = tip.or_else(|| {
        let r = send_button_rect(sh, ctx)?;
        (r.contains(sh.mouse) && items::cursor_item(ctx.world).is_none()).then(|| {
            let lines = vec![
                TipLine::new("Send stackables to stash", 0),
                TipLine::new("Runes, gems and the other kinds with a slot", 5),
            ];
            let anchor = TipAnchor {
                x: r.x + i32::from(r.w) / 2,
                top: r.y,
                bottom: r.y + i32::from(r.h),
            };
            (lines, anchor)
        })
    });
    if let Some((lines, anchor)) = tip {
        draw_tip_at(&lines, anchor, (s.w, s.h), sh.fonts.as_ref(), out);
    }
}

/// The send button's side in the panel's pixels: the game's 32 px button
/// frame.
const SEND_SIZE: i32 = 32;

/// The game's button frames the send button is drawn on: frame 0 up,
/// frame 1 pressed.
const SEND_FRAME_FILE: &str = "panel\\buysellbtn";

/// The inventory's "Send stackables to stash" button (D2RUST's own): right
/// of the gold row, under the grid's last column.
pub(super) fn send_button_rect(sh: &super::Shared, ctx: &UiCtx) -> Option<Rect> {
    if !shared_on(sh)
        || !(sh
            .states
            .is_open(crate::ui::panels::inventory::UI_INVENTORY)
            || sh.states.is_open(UI_STASH))
    {
        return None;
    }
    let l = sh
        .items
        .layout(super::Facts::of(ctx.world).class, &sh.config.screen)?;
    let g = l.grid;
    Some(Rect::new(
        g.right - SEND_SIZE,
        g.bottom + 8,
        SEND_SIZE as u16,
        SEND_SIZE as u16,
    ))
}

/// Draws the send button: the game's button frame, and our chest-and-arrow
/// cel on it ([`crate::ui::own_art::SEND_STASH`]). Hover draws the cel one
/// step brighter; pressed, the pressed frame with the cel darker and 1 px
/// down-right.
pub(super) fn draw_send_button(sh: &super::Shared, ctx: &UiCtx, out: &mut dyn UiDrawSink) {
    use crate::ui::own_art::{send_frame, SEND_STASH};
    let Some(r) = send_button_rect(sh, ctx) else {
        return;
    };
    let (Some(frame), Some(icon)) = (
        sh.tables.files.id(SEND_FRAME_FILE),
        sh.tables.files.id(SEND_STASH),
    ) else {
        return;
    };
    // Held down and still on the button (a release off the inventory
    // never reaches it).
    let pressed = sh.shared_stash.send_pressed && r.contains(sh.mouse);
    let (look, d) = if pressed {
        (send_frame::PRESSED, 1)
    } else if r.contains(sh.mouse) {
        (send_frame::HOVER, 0)
    } else {
        (send_frame::NORMAL, 0)
    };
    // A cel draw's point is its bottom-left pixel.
    let bottom = r.y + SEND_SIZE - 1;
    out.push(crate::ui::panels::cel(
        frame,
        u32::from(pressed),
        r.x,
        bottom,
    ));
    out.push(crate::ui::panels::cel(icon, look, r.x + d, bottom + d));
}

/// A press on the send button asks the store to send the stackables of the
/// inventory grid (the store moves only the kinds with a slot).
/// The button shows pressed until the release (or the next press).
pub(super) fn send_button_event(sh: &mut super::Shared, e: UiEvent, ctx: &UiCtx) -> bool {
    let at = match e {
        UiEvent::Press {
            button: PointerButton::Left,
            at,
        } => at,
        UiEvent::Release {
            button: PointerButton::Left,
            ..
        } => return std::mem::take(&mut sh.shared_stash.send_pressed),
        _ => return false,
    };
    sh.shared_stash.send_pressed = false;
    if !send_button_rect(sh, ctx).is_some_and(|r| r.contains(at)) {
        return false;
    }
    sh.shared_stash.send_pressed = true;
    let guids = items::local_items(ctx.world)
        .into_iter()
        .filter(|i| i.mode == 0 && i.page == 0)
        .map(|i| i.key.guid)
        .collect::<Vec<_>>();
    sh.shared_stash.ask(StashAsk::SendStackables { guids });
    sh.outputs.push(PanelOutput::Sound(4));
    true
}

/// The stash adapter (ui 0x19, left half above the control panel).
pub(super) struct StashUi {
    pub(super) sh: SharedRef,
    pub(super) input: StashCubeInput,
}

impl Panel for StashUi {
    /// Drawn before the inventory.
    fn draw_before(&self) -> Option<PanelId> {
        Some(PanelId(u16::from(
            crate::ui::panels::inventory::UI_INVENTORY,
        )))
    }

    fn id(&self) -> PanelId {
        PanelId(u16::from(UI_STASH))
    }

    /// The left half above the control panel, plus the inventory close
    /// rectangle the stash / cube mouse handlers test first: the frame
    /// above the control panel, and [`Self::event`] lets
    /// every other point go on.
    fn rect(&self) -> Rect {
        let s = self.sh.borrow().config.screen;
        s.frame_rect_h(s.h - 48)
    }

    fn draw(&self, ctx: &UiCtx, out: &mut dyn UiDrawSink) {
        let sh = self.sh.borrow();
        let env = sh.env();
        let panel = StashPanel {
            close_pressed: self.input.stash_close_pressed,
        };
        // The cap is the fixed stash gold limit.
        panel.draw(&sh.tables, &env, ctx.strings, STASH_CAP, GOLD_MAX_FONT, out);
        let shared_in = draw_in(&sh, ctx).filter(|_| shared_on(&sh));
        if let Some(i) = &shared_in {
            shared::draw::draw(i, out);
        }
        // D2RUST's own: the Market button (`market_ui`).
        super::market_ui::draw_button(&sh, out);
        if shared_in.as_ref().is_some_and(|i| i.st.covers()) {
            Self::draw_close_tip(&sh, ctx, out);
            return;
        }
        if sh.mod_stash.is_some() {
            Self::draw_mod(&sh, ctx, out);
            Self::draw_close_tip(&sh, ctx, out);
            return;
        }
        let g = sh.items.stash_grid(env.exp, &sh.config.screen);
        let screen_h = sh.config.screen.h;
        sh.items.draw_placement_tint(
            ctx.world,
            &sh.tables.files,
            &g,
            crate::ui::panels::stash_items::STASH_PAGE,
            sh.mouse,
            screen_h,
            out,
        );
        sh.items.draw_stash(ctx.world, &sh.tables.files, &g, out);
        // After the stash grid:
        // stat 15 `goldbank`, layer 0.
        let gold = match (&sh.fonts, crate::ui::original::local_player(ctx.world)) {
            (Some(_), Some((key, _))) => Some(ctx.world.total(key, 15, 0)),
            _ => None,
        };
        crate::ui::panels::inv_gold::draw_stash_gold(
            &sh.tables,
            &env,
            sh.gold.buttons.stash_pressed,
            gold,
            out,
        );
        Self::draw_close_tip(&sh, ctx, out);
    }

    fn ui_elements(&self, ctx: &UiCtx) -> Vec<crate::ui::ui_ids::UiElement> {
        let sh = self.sh.borrow();
        let s = sh.config.screen;
        let covered = mod_cover(&sh, ctx.world).is_some();
        let mut out = if (shared_on(&sh) && sh.shared_stash.covers()) || covered {
            Vec::new()
        } else {
            sh.items.stash_grid(sh.env().exp, &s).ui_elements("stash")
        };
        out.push(crate::ui::ui_ids::UiElement::new(
            "stash.close",
            crate::ui::panels::stash_input::stash_close_rect(&s, sh.env().exp),
        ));
        if shared_on(&sh) {
            out.extend(shared::layout::ui_elements(
                &sh.shared_stash,
                &Places::of(&s),
            ));
        }
        out.push(super::market_ui::button_element(&sh));
        out
    }

    fn hit(&self, _p: Point) -> Option<WidgetId> {
        None
    }

    fn event(&mut self, e: UiEvent, ctx: &UiCtx) -> UiResponse {
        if super::market_ui::button_event(&mut self.sh.borrow_mut(), e) {
            return UiResponse::Consumed;
        }
        if let Some(r) = self.mod_event(e, ctx) {
            return r;
        }
        if let Some(r) = self.shared_event(e, ctx) {
            return r;
        }
        self.game_event(e, ctx)
    }
}

impl StashUi {
    /// The close button's "Close" pop-up.
    fn draw_close_tip(sh: &super::Shared, ctx: &UiCtx, out: &mut dyn UiDrawSink) {
        let env = sh.env();
        // In the inclusive close rectangle,
        // `strClose` queued at (X + 12 − w / 2, Y − 35), pop-up text in
        // font 1, centre 0 (as the cube's tips).
        let s = sh.config.screen;
        if crate::ui::panels::stash_cube::stash_close_hover(&s, env.exp, sh.mouse) {
            if let Some(t) = ctx.strings.get_id(super::cube_ui::STR_CLOSE) {
                let fonts = sh.fonts.as_ref();
                let w = fonts.and_then(|f| f.width_a(1, t)).unwrap_or(0);
                let (x, y) = crate::ui::panels::stash_cube::stash_close_pos(&s, env.exp);
                let at = Point::new(x + 12 - w / 2, y - 35);
                super::hud_tips::push_popup(t.to_vec(), at, 0, false, (s.w, s.h), fonts, out);
            }
        }
    }

    /// A mod stash panel over the game's: the game's grid and gold where it
    /// covers nothing, then the mod's panel and its open field.
    fn draw_mod(sh: &super::Shared, ctx: &UiCtx, out: &mut dyn UiDrawSink) {
        let Some(m) = sh.mod_stash.as_ref() else {
            return;
        };
        let c = mod_ctx(sh, ctx.world);
        let cover = m.panel.cover(&c);
        if cover.is_none() {
            let env = sh.env();
            let g = sh.items.stash_grid(env.exp, &sh.config.screen);
            sh.items.draw_stash(ctx.world, &sh.tables.files, &g, out);
            let gold = match (&sh.fonts, crate::ui::original::local_player(ctx.world)) {
                (Some(_), Some((key, _))) => Some(ctx.world.total(key, 15, 0)),
                _ => None,
            };
            crate::ui::panels::inv_gold::draw_stash_gold(
                &sh.tables,
                &env,
                sh.gold.buttons.stash_pressed,
                gold,
                out,
            );
        }
        m.panel.draw(&c, out);
        if let Some(f) = &m.field {
            if !m.panel.draw_field(&c, f, out) {
                let area = cover.unwrap_or_else(|| sh.config.screen.left_side_rect());
                draw_field_box(sh, f, area, out);
            }
        }
    }

    /// A mod stash panel's part of an event: `None` lets the game's panel
    /// have it (the close button's presses always go on).
    fn mod_event(&mut self, e: UiEvent, ctx: &UiCtx) -> Option<UiResponse> {
        let mut sh = self.sh.borrow_mut();
        let sh = &mut *sh;
        let field_open = sh.mod_stash.as_ref()?.field.is_some();
        let (button, at, down) = match e {
            UiEvent::Char(unit) => {
                let outs = with_mod(sh, ctx.world, |m, c| m.key(c, unit)).flatten()?;
                send_outs(sh, outs);
                return Some(UiResponse::Consumed);
            }
            // The game's key bindings wait while the field has the typing.
            UiEvent::Action(_) => return field_open.then_some(UiResponse::Consumed),
            UiEvent::Press { button, at } => (button, at, true),
            UiEvent::Release { button, at } => (button, at, false),
            _ => return None,
        };
        let s = sh.config.screen;
        if crate::ui::panels::stash_input::stash_close_rect(&s, sh.env().exp).contains(at) {
            return None;
        }
        let right = match button {
            PointerButton::Left => false,
            PointerButton::Right => true,
            PointerButton::Middle => return None,
        };
        let covered = mod_cover(sh, ctx.world).is_some_and(|r| r.contains(at));
        if !down {
            return covered.then_some(UiResponse::Consumed);
        }
        let keys = Keys {
            ctrl: sh.items.ctrl,
            shift: sh.items.shift,
            alt: sh.mod_stash.as_ref().is_some_and(|m| m.alt),
        };
        match with_mod(sh, ctx.world, |m, c| m.press(c, at, right, keys))? {
            Pressed::Taken(outs) => {
                send_outs(sh, outs);
                Some(UiResponse::Consumed)
            }
            Pressed::Missed => covered.then_some(UiResponse::Consumed),
        }
    }

    /// The shared stash's part of an event: `None` lets the game's panel
    /// have it.
    fn shared_event(&mut self, e: UiEvent, ctx: &UiCtx) -> Option<UiResponse> {
        let mut sh = self.sh.borrow_mut();
        if !shared_on(&sh) {
            return None;
        }
        let places = Places::of(&sh.config.screen);
        let taken = match e {
            UiEvent::Char(unit) => shared::key(&mut sh.shared_stash, unit),
            // The game's key bindings wait while a box has the typing.
            UiEvent::Action(_) => sh.shared_stash.text_focus(),
            UiEvent::Press {
                button: PointerButton::Left,
                at,
            } => {
                let m = Mods {
                    on: sh.items.shortcut_set(),
                    ..Mods::keys(sh.items.ctrl, sh.items.shift)
                };
                let cursor = items::cursor_item(ctx.world).map(|i| i.key.guid);
                let taken = shared::press(&mut sh.shared_stash, &places, at, m, cursor);
                // On the Market category the press is for the Sell slot.
                super::market_ui::take_pick(&mut sh);
                taken
            }
            // On a shared page the game's grid is covered: its releases
            // and right presses do nothing there.
            UiEvent::Press { at, .. } | UiEvent::Release { at, .. } => {
                sh.shared_stash.covers() && places.in_cover(at)
            }
            _ => false,
        };
        taken.then_some(UiResponse::Consumed)
    }

    /// The game's stash panel: the close button, the gold button and the
    /// character's own grid.
    fn game_event(&mut self, e: UiEvent, ctx: &UiCtx) -> UiResponse {
        let (down, at) = match e {
            UiEvent::Press {
                button: PointerButton::Left,
                at,
            } => (true, at),
            UiEvent::Release {
                button: PointerButton::Left,
                at,
            } => (false, at),
            // Ctrl+Shift+Right on a stash item puts it in the cube
            // (`item_moves`); every other right press goes on as before.
            UiEvent::Press {
                button: PointerButton::Right,
                at,
            } => {
                let mut sh = self.sh.borrow_mut();
                let g = sh.items.stash_grid(sh.env().exp, &sh.config.screen);
                let under = sh.items.grid_item_at(ctx.world, &g, STASH_PAGE, at);
                return if sh.item_shortcut(ctx.world, under.as_ref(), Button::Right) {
                    UiResponse::Consumed
                } else {
                    UiResponse::Ignored
                };
            }
            _ => return UiResponse::Ignored,
        };
        let mut sh = self.sh.borrow_mut();
        let s = sh.config.screen;
        let exp = sh.env().exp;
        // The inventory close rectangle (the belt popup
        // covering it is not tested: D2RUST's own, the popup is the HUD's).
        let in_inv_close = crate::ui::panels::inventory::close_rect(&sh.tables, &s)
            .is_some_and(|r| r.contains(at));
        let left = s.left_side_rect();
        if !in_inv_close && !left.contains(at) {
            return UiResponse::Ignored;
        }
        let ptr = Pointer {
            at,
            in_inv_close,
            cursor_item: items::cursor_item(ctx.world).is_some(),
        };
        let eff = if down {
            self.input.stash_down(&s, exp, &ptr)
        } else {
            self.input.stash_up(&s, exp, &ptr)
        };
        // The stash gold button opens the withdraw dialog (kind 4) on a
        // release inside it.
        if !down && std::mem::take(&mut self.input.stash_gold) {
            if stash_gold_rect_hit(&s, exp, at) {
                let cursor = ptr.cursor_item;
                super::gold_dialog::open_dialog(&mut sh, ctx.world, GoldKind::Withdraw, cursor);
            }
            return UiResponse::Consumed;
        }
        if eff.sound4 {
            sh.outputs.push(PanelOutput::Sound(4));
        }
        sh.outputs.extend(eff.outputs);
        if eff.consumed {
            return UiResponse::Consumed;
        }
        if down && !sh.shared_stash.covers() {
            let g = sh.items.stash_grid(exp, &s);
            // Ctrl+Left moves a stash item to the inventory (`item_moves`).
            let under = sh.items.grid_item_at(ctx.world, &g, STASH_PAGE, at);
            if sh.item_shortcut(ctx.world, under.as_ref(), Button::Left) {
                return UiResponse::Consumed;
            }
            let out = sh.items.press_stash(ctx.world, &sh.tables.files, &g, at);
            sh.outputs.extend(out);
        }
        UiResponse::Consumed
    }
}
