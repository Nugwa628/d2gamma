//! The Esc menu engine: the Game page ("Options", "Save and Exit Game",
//! "Return to Game"), the selected row, the input rules (wrap, click on
//! release, hover) and the settings the game edits from its own hotkeys.
//! No drawing here: [`super::esc_menu`] draws and routes events; this
//! module only decides.
//!
//! Options opens the settings menu (`ui::settings::menu`), which holds
//! every setting; the menu hands its changes back here
//! (`OriginalUi::edit_settings`), so the game saves and applies them on
//! one path ([`OptionsMenu::take_changed`]).

use crate::app::config::Settings;
use crate::ui::geom::{Point, Rect};

use crate::ui::layout::Screen;
use crate::ui::ui_ids::UiElement;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Options,
    ExitGame,
    ReturnToGame,
}

/// One row of the page: its row and label (the label image's English
/// text, drawn when the image is missing).
#[derive(Clone, Copy, Debug)]
pub struct RowDef {
    pub row: Row,
    pub label: &'static str,
}

/// The Game page's rows, top to bottom.
pub const ROWS: &[RowDef] = &[
    RowDef {
        row: Row::Options,
        label: "Options",
    },
    RowDef {
        row: Row::ExitGame,
        label: "Save and Exit Game",
    },
    RowDef {
        row: Row::ReturnToGame,
        label: "Return to Game",
    },
];

/// The row pitch, and the label baseline and pentagram offsets below a
/// row's top.
const PITCH: i32 = 50;
const LABEL_BASELINE: i32 = 39;
const PENTAGRAM: i32 = 51;

/// What the menu asks of its host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEvent {
    /// UI sound `cursor_pass` (1).
    CursorPass,
    /// UI sound `cursor_select` (2).
    CursorSelect,
    /// Save and Exit Game.
    SaveAndExit,
    /// Return to Game: close the whole menu.
    Close,
    /// Options: open the settings menu.
    OpenSettings,
}

/// The menu's state.
#[derive(Clone, Debug)]
pub struct OptionsMenu {
    /// W and H: the play frame.
    pub screen: Screen,
    pub selected: usize,
    /// The settings in force, as the game edits them (the control panel's
    /// text toggles) and the settings menu hands them over.
    pub settings: Settings,
    /// A setting changed and the host has not written it yet.
    pub changed: bool,
    dragging: bool,
    last_ptr: Option<Point>,
    pub events: Vec<MenuEvent>,
}

impl Default for OptionsMenu {
    fn default() -> Self {
        Self {
            screen: Screen::R800,
            selected: ROWS.len() - 1,
            settings: Settings::default(),
            changed: false,
            dragging: false,
            last_ptr: None,
            events: Vec::new(),
        }
    }
}

impl OptionsMenu {
    /// `h = W / 2` (400 at 800 × 600, 320 at 640 × 480).
    pub fn half(&self) -> i32 {
        self.screen.w / 2
    }

    /// Open with Return to Game selected.
    pub fn open(&mut self) {
        self.selected = ROWS.len() - 1;
        self.dragging = false;
    }

    /// Take the settings in force.
    pub fn set_settings(&mut self, s: Settings) {
        self.settings = s;
    }

    /// Back from Configure Controls: Return to Game selected.
    pub fn return_from_controls(&mut self) {
        self.open();
    }

    pub fn rows(&self) -> &'static [RowDef] {
        ROWS
    }

    // ---- geometry ----

    /// y of the first row's top: the rows are centred above the bottom 80
    /// pixels of the frame.
    pub fn y0(&self) -> i32 {
        (self.screen.h - 80) / 2 - PITCH * ROWS.len() as i32 / 2
    }

    pub fn y_top(&self, i: usize) -> i32 {
        self.y0() + PITCH * i as i32
    }

    /// Label baseline of row `i`.
    pub fn baseline(&self, i: usize) -> i32 {
        self.y_top(i) + LABEL_BASELINE
    }

    /// Pentagram y of the selected row.
    pub fn pentagram_y(&self) -> i32 {
        self.y_top(self.selected) + PENTAGRAM
    }

    /// The row under the pointer, by y only: none outside the rows' span.
    pub fn row_at(&self, y: i32) -> Option<usize> {
        let n = ROWS.len() as i32;
        let top = self.y0();
        let bottom = top + 2 * (PITCH * n / 2);
        if !(top < y && y < bottom) {
            return None;
        }
        (0..ROWS.len()).rev().find(|&i| y >= self.y_top(i) + n)
    }

    // ---- input ----

    fn push(&mut self, e: MenuEvent) {
        self.events.push(e);
    }

    /// Down (`step` = 1) or Up (`step` = −1): the next row, wrapping.
    fn step(&mut self, step: i32) {
        let n = ROWS.len() as i32;
        self.selected = (self.selected as i32 + step).rem_euclid(n) as usize;
        self.push(MenuEvent::CursorPass);
    }

    pub fn key_down(&mut self) {
        self.step(1);
    }

    pub fn key_up(&mut self) {
        self.step(-1);
    }

    pub fn key_enter(&mut self) {
        self.activate();
    }

    fn activate(&mut self) {
        self.push(MenuEvent::CursorSelect);
        self.push(match ROWS[self.selected].row {
            Row::Options => MenuEvent::OpenSettings,
            Row::ExitGame => MenuEvent::SaveAndExit,
            Row::ReturnToGame => MenuEvent::Close,
        });
    }

    /// Left button down.
    pub fn press(&mut self, p: Point) {
        self.last_ptr = Some(p);
        if let Some(i) = self.row_at(p.y) {
            self.selected = i;
        }
        self.dragging = true;
    }

    /// Left button up: activates when the pointer is still on the selected row.
    pub fn release(&mut self, p: Point) {
        self.last_ptr = Some(p);
        if self.dragging && self.row_at(p.y) == Some(self.selected) {
            self.activate();
        }
        self.dragging = false;
    }

    /// Pointer moved (checked at each draw): hover selects.
    pub fn moved(&mut self, p: Point) {
        if self.last_ptr == Some(p) {
            return;
        }
        self.last_ptr = Some(p);
        if !self.dragging {
            if let Some(i) = self.row_at(p.y) {
                self.selected = i;
            }
        }
    }

    pub fn take_events(&mut self) -> Vec<MenuEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn take_changed(&mut self) -> Option<Settings> {
        std::mem::take(&mut self.changed).then_some(self.settings)
    }

    /// The clickable elements of the page ([`crate::ui::ui_ids`]): its
    /// rows, `options.game.<row>` ([`row_id`]). Each is the row's band of
    /// the frame, as the menu's own hit test ([`OptionsMenu::row_at`])
    /// finds it.
    pub fn ui_elements(&self) -> Vec<UiElement> {
        let x = self.half() - ROW_ID_LEFT;
        let mut out: Vec<UiElement> = Vec::new();
        let mut y = 0;
        while y < self.screen.h {
            let Some(i) = self.row_at(y) else {
                y += 1;
                continue;
            };
            let top = y;
            while y < self.screen.h && self.row_at(y) == Some(i) {
                y += 1;
            }
            let h = u16::try_from(y - top).unwrap_or(u16::MAX);
            let id = format!("options.game.{}", row_id(ROWS[i].row));
            if !out.iter().any(|e| e.id == id) {
                out.push(UiElement::new(id, Rect::new(x, top, ROW_ID_W, h)));
            }
        }
        out
    }
}

/// The rows' click strip ([`OptionsMenu::ui_elements`]): from this far
/// left of the centre, this wide.
const ROW_ID_LEFT: i32 = 250;
const ROW_ID_W: u16 = 190;

/// A row's name in ui ids.
pub fn row_id(r: Row) -> &'static str {
    match r {
        Row::Options => "options",
        Row::ExitGame => "exit_game",
        Row::ReturnToGame => "return_to_game",
    }
}

#[cfg(test)]
#[path = "options_menu_tests.rs"]
mod tests;
