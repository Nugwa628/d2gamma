//! The in-game menu (ui state 9, the Esc menu) in play: a modal box with
//! "Options", "Save and Exit Game" and "Return to Game".
//!
//! Esc (command 56) closes the menu when it is open, else closes the open
//! panels, else opens it; state 9 has a gate (no player, no menu; a dead
//! player respawns instead); the world takes no click while it is open.
//!
//! The rows, input and settings are `options_menu`; this panel draws it
//! and routes events. The DC6 art is `esc_art`; a row whose image is
//! missing is Font16 English text. Options opens the settings menu
//! (`ui::settings::menu`). Esc closes the whole menu
//! (`OriginalUi::game_menu_key`).

use super::options_menu::{MenuEvent, OptionsMenu};
use super::{left, SharedRef};
use crate::ui::draw::{TextRequest, TextStyle, UiDraw, UiDrawSink};
use crate::ui::geom::{Point, Rect};
use crate::ui::panel::{Panel, PanelId, UiCtx, UiEvent, UiResponse, WidgetId};
use crate::ui::panels::{utf16, PanelOutput};
use crate::ui::text::TextOpts;
use crate::ui::ui_ids::UiElement;

/// The panel's id: the UI state number.
pub const ESC_PANEL: PanelId = PanelId(9);

/// Menu keys as UI `Char` events (the menu's key handlers, keyed by VK
/// code): private-use code units, D2RUST's own.
pub const CHAR_ENTER: u16 = 0x0D;
pub const CHAR_LEFT: u16 = 0xF025;
pub const CHAR_UP: u16 = 0xF026;
pub const CHAR_RIGHT: u16 = 0xF027;
pub const CHAR_DOWN: u16 = 0xF028;

/// Font16, and the colors of a row at rest and selected.
const FONT: u16 = 1;
const COLOR_REST: u16 = 0;
const COLOR_SELECTED: u16 = 3;

/// The menu's own state.
#[derive(Clone, Debug, Default)]
pub struct EscState {
    /// "Save and Exit Game" was chosen and the host has not read it yet.
    pub exit_requested: bool,
    /// "Configure Controls" was chosen and the host has not read it yet.
    pub controls_requested: bool,
    /// "Options" opened the settings menu and the host has not read it yet.
    pub settings_requested: bool,
    /// The Configure Controls screen, while it is open over the menu
    /// (`controls_host`).
    pub controls: Option<super::controls_host::ControlsHost>,
    /// Bindings accepted on that screen, until the host reads them.
    pub accepted: Option<crate::controls::Bindings>,
    /// The menu's rows and the settings the game edits (`options_menu`).
    pub menu: OptionsMenu,
}

impl EscState {
    /// Leave the Controls screen for the menu (Return to Game selected).
    pub fn close_controls(&mut self, f: super::controls_host::Finished) {
        use super::controls_host::Finished;
        self.controls = None;
        self.menu.return_from_controls();
        if let Finished::Accept(b) = f {
            self.accepted = Some(b);
        }
    }
}

/// `s` in Font16, centred across a block `w` wide from `at`.
fn text(out: &mut dyn UiDrawSink, clip: Rect, s: &str, at: Point, color: u16, w: i32) {
    out.push(UiDraw::Text(TextRequest {
        text: utf16(s),
        at,
        style: TextStyle { font: FONT, color },
        opts: TextOpts::Draw {
            centered: true,
            block_w: Some(w),
            mode: 5,
        },
        clip,
    }));
}

/// The adapter (installed last: top-most, so it takes every click).
pub struct EscMenuUi {
    pub(super) sh: SharedRef,
    /// The pentagram counter, advanced by the draws ([`PentClock`]).
    pub(super) pent: std::cell::Cell<super::esc_art::PentClock>,
}

impl Panel for EscMenuUi {
    fn id(&self) -> PanelId {
        ESC_PANEL
    }

    // Modal: the whole screen.
    fn rect(&self) -> Rect {
        self.sh.borrow().config.screen.frame_rect()
    }

    /// The UI pass draws the menu first, before the inventory family and
    /// the control panel (`options` … `pentspin` under `800ctrlpnl7`).
    fn draw_first(&self) -> bool {
        true
    }

    fn draw(&self, ctx: &UiCtx, out: &mut dyn UiDrawSink) {
        {
            let mut sh = self.sh.borrow_mut();
            let border = sh.tables.files.id(crate::ui::box_border::BOXPIECES_FILE);
            let slider = sh.tables.files.id(super::msg_log_ui::TEXTSLID_FILE);
            let sh = &mut *sh;
            if let Some(c) = sh.esc.controls.as_mut() {
                let fonts = sh
                    .fonts
                    .as_ref()
                    .map(|f| f as &dyn crate::ui::panels::TextMeasure);
                c.draw(ctx, (border, slider), fonts, out);
                return;
            }
        }
        let sh = self.sh.borrow();
        let m = &sh.esc.menu;
        let clip = m.screen.rect();
        for (i, def) in m.rows().iter().enumerate() {
            if super::esc_art::draw_row(&sh.tables.files, m, i, out) {
                continue;
            }
            let color = if i == m.selected {
                COLOR_SELECTED
            } else {
                COLOR_REST
            };
            text(
                out,
                clip,
                def.label,
                Point::new(0, m.baseline(i) - 12),
                color,
                m.screen.w,
            );
        }
        let mut clock = self.pent.get();
        // Live play steps on the wall clock, which runs on through
        // the pause; a check run on the server tick (the pinned clock).
        // A fixed display clock (`play --display-clock`) stands in for the
        // wall clock, so shots of the open menu repeat.
        let now = sh
            .menu_wall_ms
            .get()
            .map(|wall| crate::world_view::display_clock::now_ms().unwrap_or(wall))
            .unwrap_or_else(|| {
                super::esc_art::PentClock::ms_of_tick(ctx.world.server_ticks, ctx.world.tick_rate)
            });
        let f = clock.draw(now);
        self.pent.set(clock);
        super::esc_art::draw_pents(&sh.tables.files, m, f, out);
    }

    fn hit(&self, _p: Point) -> Option<WidgetId> {
        None
    }

    /// The menu's rows, `options.game.<row>` ([`OptionsMenu::ui_elements`]),
    /// or Configure Controls.
    fn ui_elements(&self, _ctx: &UiCtx) -> Vec<UiElement> {
        let sh = self.sh.borrow();
        if let Some(c) = &sh.esc.controls {
            return c.ui_elements();
        }
        sh.esc.menu.ui_elements()
    }

    fn event(&mut self, e: UiEvent, _ctx: &UiCtx) -> UiResponse {
        let mut sh = self.sh.borrow_mut();
        if let Some(c) = sh.esc.controls.as_mut() {
            let done = match e {
                UiEvent::CursorMoved(p) => {
                    c.moved(p);
                    None
                }
                UiEvent::Wheel { steps, .. } => {
                    c.wheel(steps);
                    None
                }
                _ => match left(e) {
                    Some((true, at)) => c.press(at),
                    _ => None,
                },
            };
            if let Some(f) = done {
                sh.esc.close_controls(f);
            }
            return UiResponse::Consumed;
        }
        let m = &mut sh.esc.menu;
        let mut consumed = false;
        match e {
            UiEvent::CursorMoved(p) => m.moved(p),
            UiEvent::Char(c) => {
                consumed = true;
                match c {
                    CHAR_ENTER => m.key_enter(),
                    // Taken, so the game does not see them.
                    CHAR_LEFT | CHAR_RIGHT => {}
                    CHAR_UP => m.key_up(),
                    CHAR_DOWN => m.key_down(),
                    _ => consumed = false,
                }
            }
            _ => {
                if let Some((down, at)) = left(e) {
                    if down {
                        m.press(at);
                    } else {
                        m.release(at);
                    }
                }
            }
        }
        for ev in m.take_events() {
            match ev {
                // Sound 1 at an arrow move, 2 at an action entry.
                MenuEvent::CursorPass => sh.outputs.push(PanelOutput::Sound(1)),
                MenuEvent::CursorSelect => sh.outputs.push(PanelOutput::Sound(2)),
                MenuEvent::SaveAndExit => sh.esc.exit_requested = true,
                MenuEvent::Close => sh.outputs.push(PanelOutput::SetUi {
                    ui: 9,
                    mode: 1,
                    jump: false,
                }),
                MenuEvent::OpenSettings => sh.esc.settings_requested = true,
            }
        }
        match e {
            UiEvent::Press { .. } | UiEvent::Release { .. } => UiResponse::Consumed,
            _ if consumed => UiResponse::Consumed,
            _ => UiResponse::Ignored,
        }
    }
}
