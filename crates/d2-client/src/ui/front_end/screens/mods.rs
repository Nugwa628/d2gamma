//! D2Gamma Mods: the list of installed mods, opened from the main menu.
//!
//! Each row shows a mod's picture (when it has one), its name, author and
//! version, its description, and, when it cannot be started, why in red: the
//! words of the mod kit ("this mod needs updating", "needs a newer
//! D2Gamma"). A click selects a row (a second click on it, PLAY or Enter
//! starts it); Up / Down move the selection, the wheel and Page Up / Down
//! turn the pages, Esc and BACK return to the main menu.
//!
//! Starting a mod does not load it into this run: the mod is fixed for a
//! process, so the pick is handed to the host ([`ModsHost::switch`]) and the
//! front end ends; the game then starts again as picked ([`crate::mod_menu`]).

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use crate::kit_mod;
use crate::mod_menu::{self, ModEntry, ModStatus, Switch};
use crate::ui::front_end::control::vk;
use crate::ui::front_end::flow::Trigger;
use crate::ui::front_end::screens::ids::MODS;
use crate::ui::front_end::screens::main_menu;
use crate::ui::front_end::screens::online::lobby::wrap;
use crate::ui::front_end::screens::online::widgets::{
    button, centred, fit, modal, popup_text, push_message, push_popup_art, FONT16, FONT30, FONT8,
    GOLD, GREEN, GREY, MEDIUM, RED, SKY_POPUP, WHITE,
};
use crate::ui::front_end::{Control, ControlKind, DrawItem, FrontCtx, Registry, Screen};
use crate::ui::geom::{Point, Rect};
use crate::ui::ui_ids::UiElement;

/// What the main menu's third button is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuButton {
    /// Other Multiplayer, as it always was.
    Stock,
    /// D2GAMMA MODS: plain D2Gamma with mods installed.
    Mods,
    /// BACK TO D2GAMMA: inside a mod.
    Back,
}

/// Where the screens get the mods and hand a pick to (the disk and the
/// restart in the game, a fixed list in tests).
pub trait ModsHost {
    /// The installed mods, as the list shows them.
    fn installed(&self) -> Vec<ModEntry>;
    /// What the main menu's third button is.
    fn menu_button(&self) -> MenuButton;
    /// A mod (or plain D2Gamma) was picked.
    fn switch(&self, switch: Switch);
    fn characters_to_copy(&self) -> Vec<String> {
        Vec::new()
    }
    fn copy_character(&self, _id: &str, _name: &str) -> Result<(), String> {
        Err("Character copying is unavailable.".into())
    }
    fn download_offer(&self) -> Option<crate::mod_download::Prompt> {
        None
    }
    fn request_download(&self, _request: crate::mod_download::Confirmed) -> Result<(), String> {
        Err("No launcher download handler is available.".into())
    }
}

/// The game's host: the mods folder, and a restart through
/// [`mod_menu::request_switch`].
#[derive(Clone, Debug)]
pub struct DiskHost {
    pub files: Arc<dyn crate::ui::front_end::character_files::CharacterFiles>,
}

impl ModsHost for DiskHost {
    fn characters_to_copy(&self) -> Vec<String> {
        if kit_mod::active().is_some() {
            return Vec::new();
        }
        crate::character_copy::characters(&crate::user_dir::d2gamma_root())
    }
    fn copy_character(&self, id: &str, name: &str) -> Result<(), String> {
        if kit_mod::active().is_some() {
            return Err("Copy from plain D2Gamma.".into());
        }
        crate::character_copy::copy(&crate::user_dir::d2gamma_root(), id, name, |bytes| {
            self.files.copy_to_mod(bytes)
        })
        .map(|_| ())
    }
    fn installed(&self) -> Vec<ModEntry> {
        mod_menu::list()
    }

    fn menu_button(&self) -> MenuButton {
        if kit_mod::active().is_some() {
            MenuButton::Back
        } else if crate::identity::current().is_d2gamma_folder()
            && (!self.installed().is_empty() || self.download_offer().is_some())
        {
            MenuButton::Mods
        } else {
            MenuButton::Stock
        }
    }

    fn switch(&self, switch: Switch) {
        mod_menu::request_switch(switch);
    }
    fn download_offer(&self) -> Option<crate::mod_download::Prompt> {
        crate::mod_download::pending().ok().flatten()
    }
    fn request_download(&self, request: crate::mod_download::Confirmed) -> Result<(), String> {
        request.queue(&crate::user_dir::d2gamma_root())
    }
}

/// No mods anywhere: the list is empty and the main menu is stock.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoMods;

impl ModsHost for NoMods {
    fn installed(&self) -> Vec<ModEntry> {
        Vec::new()
    }

    fn menu_button(&self) -> MenuButton {
        MenuButton::Stock
    }

    fn switch(&self, _: Switch) {}
}

/// Virtual keys the list answers to.
mod key {
    pub const PRIOR: u16 = 33;
    pub const NEXT: u16 = 34;
    pub const END: u16 = 35;
    pub const HOME: u16 = 36;
    pub const UP: u16 = 38;
    pub const DOWN: u16 = 40;
}

/// Custom action ids.
pub mod act {
    pub const BACK: u32 = 1;
    pub const PLAY: u32 = 2;
    pub const DOWNLOAD: u32 = 3;
    pub const DOWNLOAD_CANCEL: u32 = 4;
    pub const DOWNLOAD_OK: u32 = 5;
    pub const COPY: u32 = 6;
    pub const COPY_CONFIRM: u32 = 7;
    pub const COPY_CANCEL: u32 = 8;
    pub const COPY_PREV: u32 = 9;
    pub const COPY_NEXT: u32 = 10;
}

/// The layout, in the 800 x 600 frame.
const LIST_X: i32 = 80;
const LIST_Y: i32 = 108;
const LIST_W: i32 = 640;
const ROW_H: i32 = 88;
const ROW_STEP: i32 = 92;
/// Rows a page shows.
pub const ROWS: usize = 4;
/// The picture's box, in a row.
const PIC_W: u32 = 104;
const PIC_H: u32 = 78;
const TEXT_X: i32 = 120 + PIC_W as i32;
/// A second click on the selected row within this many milliseconds starts it.
const DOUBLE_CLICK_MS: u64 = 600;

/// A picture ready to draw.
#[derive(Clone, Debug)]
struct Pic {
    key: u64,
    w: u16,
    h: u16,
    rgb: Arc<Vec<u8>>,
}

pub struct ModsScreen {
    host: Rc<dyn ModsHost>,
    entries: Vec<ModEntry>,
    /// The selected entry, and the first one on the page.
    selected: usize,
    top: usize,
    /// Pictures by file, `None` for one that could not be read.
    pictures: HashMap<PathBuf, Option<Pic>>,
    /// The last row clicked and when, for a double click.
    last_click: Option<(usize, u64)>,
    now_ms: u64,
    download_prompt: Option<crate::mod_download::Prompt>,
    download_seen: bool,
    download_notice: Option<String>,
    copy_target: Option<(String, String)>,
    copy_names: Vec<String>,
    copy_index: usize,
}

impl ModsScreen {
    pub fn new(host: Rc<dyn ModsHost>) -> Self {
        ModsScreen {
            host,
            entries: Vec::new(),
            selected: 0,
            top: 0,
            pictures: HashMap::new(),
            last_click: None,
            now_ms: 0,
            download_prompt: None,
            download_seen: false,
            download_notice: None,
            copy_target: None,
            copy_names: Vec::new(),
            copy_index: 0,
        }
    }

    /// The selected mod, when it can be started.
    fn pickable(&self) -> Option<&ModEntry> {
        self.entries
            .get(self.selected)
            .filter(|e| e.status.is_ready())
    }

    fn copying_allowed(&self) -> bool {
        self.pickable()
            .is_some_and(|e| e.accepts_d2gamma_characters)
    }

    fn busy(&self) -> bool {
        self.copy_target.is_some()
            || self.download_prompt.is_some()
            || self.download_notice.is_some()
    }

    fn start(&mut self) -> Option<Trigger> {
        let id = self.pickable()?.id.clone();
        self.host.switch(Switch::Mod(id));
        Some(Trigger::SwitchMod)
    }

    fn select(&mut self, i: usize) {
        if i < self.entries.len() {
            self.selected = i;
            if i < self.top {
                self.top = i;
            } else if i >= self.top + ROWS {
                self.top = i + 1 - ROWS;
            }
        }
    }

    fn page(&mut self, forward: bool) {
        let last_top = self.entries.len().saturating_sub(ROWS);
        self.top = if forward {
            (self.top + ROWS).min(last_top)
        } else {
            self.top.saturating_sub(ROWS)
        };
        self.selected = self.selected.clamp(
            self.top,
            (self.top + ROWS - 1).min(self.entries.len().saturating_sub(1)),
        );
    }

    fn row_rect(slot: usize) -> Rect {
        Rect::new(
            LIST_X,
            LIST_Y + slot as i32 * ROW_STEP,
            LIST_W as u16,
            ROW_H as u16,
        )
    }

    /// The entry index of the visible row under `p`.
    fn row_at(&self, p: Point) -> Option<usize> {
        (0..ROWS)
            .filter(|&slot| self.top + slot < self.entries.len())
            .find(|&slot| Self::row_rect(slot).contains(p))
            .map(|slot| self.top + slot)
    }

    fn picture(&mut self, e: &ModEntry) -> Option<Pic> {
        let path = e.picture.clone()?;
        self.pictures
            .entry(path.clone())
            .or_insert_with(|| {
                let (w, h, rgb) = mod_menu::load_picture(&path, PIC_W, PIC_H)?;
                // The same file is the same picture: its path names it.
                let mut key = 0xcbf2_9ce4_8422_2325u64;
                for b in path.to_string_lossy().bytes() {
                    key = (key ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
                }
                Some(Pic {
                    key,
                    w: w as u16,
                    h: h as u16,
                    rgb: Arc::new(rgb),
                })
            })
            .clone()
    }
}

impl Screen for ModsScreen {
    fn build(&mut self, ctx: &mut FrontCtx) -> Vec<Control> {
        self.now_ms = ctx.now_ms;
        if !self.download_seen {
            self.download_prompt = self.host.download_offer();
            self.download_seen = true;
        }
        self.entries = self.host.installed();
        self.pictures.clear();
        self.last_click = None;
        self.top = 0;
        // Open on the first mod that can be started.
        self.selected = self
            .entries
            .iter()
            .position(|e| e.status.is_ready())
            .unwrap_or(0);
        self.select(self.selected);
        let bg = if ctx.expansion {
            main_menu::BACKGROUND
        } else {
            main_menu::BACKGROUND_CLASSIC
        };
        let mut title = centred(0, 78, 800, 40, FONT30, "D2GAMMA MODS");
        title.color = GOLD;
        let mut play = button(627, 572, 128, 35, MEDIUM, "PLAY", act::PLAY).with_hotkey(vk::ENTER);
        play.enabled = self.pickable().is_some();
        let mut controls = vec![
            Control::new(ControlKind::Image, 0, 599, 800, 600).with_art(bg),
            title,
            button(33, 572, 128, 35, MEDIUM, "BACK", act::BACK).with_hotkey(vk::ESC),
            play,
        ];
        let mut copy_button = button(280, 572, 240, 35, MEDIUM, "COPY CHARACTER", act::COPY);
        copy_button.enabled = self.copying_allowed();
        copy_button.visible = self.copying_allowed();
        // Keep the unopted list's control descriptors unchanged.
        if self.entries.iter().any(|e| e.accepts_d2gamma_characters) {
            controls.push(copy_button);
        }
        if let Some((_, target_name)) = &self.copy_target {
            push_popup_art(&mut controls, SKY_POPUP);
            let name = &self.copy_names[self.copy_index];
            controls.push(popup_text(format!("Copy {name} to {target_name}?\nThe D2Gamma original stays unchanged.\nThe mod may play differently.")).with_font(FONT8, 2));
            controls.push(
                button(279, 303, 50, 26, SKY_POPUP.button, "<", act::COPY_PREV).with_on_press(),
            );
            controls.push(
                button(469, 303, 50, 26, SKY_POPUP.button, ">", act::COPY_NEXT).with_on_press(),
            );
            controls.push(
                button(
                    279,
                    337,
                    112,
                    32,
                    SKY_POPUP.button,
                    "COPY",
                    act::COPY_CONFIRM,
                )
                .with_on_press(),
            );
            controls.push(
                button(
                    407,
                    337,
                    112,
                    32,
                    SKY_POPUP.button,
                    "CANCEL",
                    act::COPY_CANCEL,
                )
                .with_hotkey(vk::ESC)
                .with_on_press(),
            );
        } else if let Some(prompt) = &self.download_prompt {
            push_popup_art(&mut controls, SKY_POPUP);
            controls.push(popup_text(prompt.text()).with_font(FONT8, 2));
            controls.push(
                button(
                    279,
                    337,
                    112,
                    32,
                    SKY_POPUP.button,
                    "DOWNLOAD",
                    act::DOWNLOAD,
                )
                .with_on_press(),
            );
            controls.push(
                button(
                    407,
                    337,
                    112,
                    32,
                    SKY_POPUP.button,
                    "CANCEL",
                    act::DOWNLOAD_CANCEL,
                )
                .with_hotkey(vk::ESC)
                .with_on_press(),
            );
        } else if let Some(notice) = &self.download_notice {
            push_message(&mut controls, SKY_POPUP, notice, act::DOWNLOAD_OK);
        }
        modal(&mut controls);
        controls
    }

    fn action(&mut self, _ctx: &mut FrontCtx, id: u32) -> Option<Trigger> {
        if let Some((target, _)) = &self.copy_target {
            match id {
                act::COPY_PREV => {
                    self.copy_index =
                        (self.copy_index + self.copy_names.len() - 1) % self.copy_names.len()
                }
                act::COPY_NEXT => self.copy_index = (self.copy_index + 1) % self.copy_names.len(),
                act::COPY_CANCEL => self.copy_target = None,
                act::COPY_CONFIRM => {
                    self.download_notice = Some(
                        match self
                            .host
                            .copy_character(target, &self.copy_names[self.copy_index])
                        {
                            Ok(()) => {
                                "Character copied. The D2Gamma original remains unchanged.".into()
                            }
                            Err(error) => error,
                        },
                    );
                    self.copy_target = None;
                }
                _ => return None,
            }
            return Some(Trigger::Rebuild);
        }
        if self.download_prompt.is_some() {
            match id {
                act::DOWNLOAD => {
                    let request = self.download_prompt.take()?.confirm();
                    self.download_notice = Some(match self.host.request_download(request) { Ok(()) => "Download requested. The launcher will verify the author and package before installation.".into(), Err(error) => error });
                }
                act::DOWNLOAD_CANCEL => {
                    self.download_prompt = None;
                }
                _ => return None,
            }
            return Some(Trigger::Rebuild);
        }
        if self.download_notice.is_some() {
            if id == act::DOWNLOAD_OK {
                self.download_notice = None;
                return Some(Trigger::Rebuild);
            }
            return None;
        }
        match id {
            act::BACK => Some(Trigger::Exit),
            act::PLAY => self.start(),
            act::COPY if self.copying_allowed() => {
                let entry = self.pickable()?;
                let target = (entry.id.clone(), entry.name.clone());
                self.copy_names = self.host.characters_to_copy();
                self.copy_index = 0;
                if self.copy_names.is_empty() {
                    self.download_notice =
                        Some("No D2Gamma characters are available to copy.".into());
                } else {
                    self.copy_target = Some(target);
                }
                Some(Trigger::Rebuild)
            }
            _ => None,
        }
    }

    fn sync(&mut self, controls: &mut [Control]) {
        if self.busy() {
            return;
        }
        for control in controls {
            if control.action == crate::ui::front_end::control::Action::Custom(act::PLAY) {
                control.enabled = self.pickable().is_some();
            } else if control.action == crate::ui::front_end::control::Action::Custom(act::COPY) {
                control.visible = self.copying_allowed();
                control.enabled = self.copying_allowed();
            }
        }
    }

    fn mouse(&mut self, ctx: &mut FrontCtx, down: bool, p: Point) -> Option<Trigger> {
        if !down || self.busy() {
            return None;
        }
        let i = self.row_at(p)?;
        let again = self
            .last_click
            .is_some_and(|(row, at)| row == i && ctx.now_ms.saturating_sub(at) <= DOUBLE_CLICK_MS);
        self.select(i);
        self.last_click = Some((i, ctx.now_ms));
        if again {
            self.last_click = None;
            return self.start();
        }
        None
    }

    fn wheel(&mut self, _ctx: &mut FrontCtx, delta: i32) -> Option<Trigger> {
        if self.busy() {
            return None;
        }
        if delta != 0 {
            let step = delta.signum() as isize;
            let last_top = self.entries.len().saturating_sub(ROWS);
            self.top = (self.top as isize - step).clamp(0, last_top as isize) as usize;
        }
        None
    }

    fn key(&mut self, _ctx: &mut FrontCtx, k: u16) -> Option<Trigger> {
        if self.busy() {
            return None;
        }
        match k {
            key::UP => self.select(self.selected.saturating_sub(1)),
            key::DOWN => self.select(self.selected + 1),
            key::PRIOR => self.page(false),
            key::NEXT => self.page(true),
            key::HOME => self.select(0),
            key::END => self.select(self.entries.len().saturating_sub(1)),
            _ => {}
        }
        None
    }

    fn overlay(&mut self, _now_ms: u64, adv: &dyn Fn(u16, &[u16]) -> i32) -> Vec<DrawItem> {
        let width = |font: u16| move |t: &str| adv(font, &t.encode_utf16().collect::<Vec<u16>>());
        let text = |font: u16, x: i32, y: i32, color: i32, t: &str| DrawItem::Text {
            string_id: 0,
            text: t.to_owned(),
            font,
            at: Point::new(x, y),
            label: None,
            color,
            boxed: None,
        };
        let mut out = Vec::new();
        if self.entries.is_empty() {
            let rect = Self::row_rect(0);
            out.push(DrawItem::Rect {
                at: Point::new(rect.x, rect.y),
                w: LIST_W,
                h: ROW_H,
            });
            out.push(DrawItem::Border {
                at: Point::new(rect.x, rect.y),
                w: LIST_W,
                h: ROW_H,
            });
            for (n, line) in [
                "No mods are installed.",
                "A mod's folder goes in Documents\\D2Gamma\\mods.",
            ]
            .iter()
            .enumerate()
            {
                let w = width(FONT8)(line);
                out.push(text(
                    FONT8,
                    LIST_X + (LIST_W - w) / 2,
                    rect.y + 38 + n as i32 * 16,
                    GREY,
                    line,
                ));
            }
            return out;
        }
        let shown: Vec<(usize, ModEntry)> = self
            .entries
            .iter()
            .enumerate()
            .skip(self.top)
            .take(ROWS)
            .map(|(i, e)| (i, e.clone()))
            .collect();
        // One panel under every row, so the gaps between rows are dark too.
        out.push(DrawItem::Rect {
            at: Point::new(LIST_X, LIST_Y),
            w: LIST_W,
            h: (ROWS as i32 - 1) * ROW_STEP + ROW_H,
        });
        for (slot, (i, e)) in shown.iter().enumerate() {
            let rect = Self::row_rect(slot);
            let (x, y) = (rect.x, rect.y);
            out.push(DrawItem::Rect {
                at: Point::new(x, y),
                w: LIST_W,
                h: ROW_H,
            });
            if *i == self.selected {
                for inset in 0..2 {
                    out.push(DrawItem::Border {
                        at: Point::new(x + inset, y + inset),
                        w: LIST_W - 2 * inset,
                        h: ROW_H - 2 * inset,
                    });
                }
            }
            // The picture's box: the mod's own picture, centred, or an empty
            // frame.
            let (bx, by) = (x + 8, y + 5);
            out.push(DrawItem::Border {
                at: Point::new(bx - 1, by - 1),
                w: PIC_W as i32 + 2,
                h: PIC_H as i32 + 2,
            });
            if let Some(p) = self.picture(e) {
                out.push(DrawItem::Picture {
                    key: p.key,
                    w: p.w,
                    h: p.h,
                    rgb: p.rgb.clone(),
                    at: Point::new(
                        bx + (PIC_W as i32 - i32::from(p.w)) / 2,
                        by + (PIC_H as i32 - i32::from(p.h)) / 2,
                    ),
                });
            }
            let room = LIST_W - (TEXT_X - LIST_X) - 10;
            let ready = e.status.is_ready();
            let name = fit(&e.name, room, &width(FONT16));
            out.push(text(
                FONT16,
                x + (TEXT_X - LIST_X),
                y + 24,
                if ready { GOLD } else { GREY },
                &name,
            ));
            let by_line = match (e.author.is_empty(), e.version.is_empty()) {
                (true, true) => String::new(),
                (false, true) => format!("by {}", e.author),
                (true, false) => format!("v{}", e.version),
                (false, false) => format!("by {}    v{}", e.author, e.version),
            };
            out.push(text(
                FONT8,
                x + (TEXT_X - LIST_X),
                y + 42,
                WHITE,
                &fit(&by_line, room, &width(FONT8)),
            ));
            let tx = x + (TEXT_X - LIST_X);
            match &e.status {
                ModStatus::Ready => {
                    let describe = fit(&e.description, room, &width(FONT8));
                    out.push(text(FONT8, tx, y + 60, GREY, &describe));
                    out.push(text(FONT8, tx, y + 78, GREEN, "Ready to play"));
                }
                // A mod that cannot start says why, over two lines, in place
                // of its description.
                other => {
                    let why = other.problem().unwrap_or_default();
                    let mut lines = wrap(why, room, &width(FONT8));
                    if lines.len() > 2 {
                        let rest = lines.split_off(1).join(" ");
                        lines.push(fit(&rest, room, &width(FONT8)));
                    }
                    for (n, line) in lines.iter().enumerate() {
                        out.push(text(FONT8, tx, y + 60 + n as i32 * 16, RED, line));
                    }
                }
            }
        }
        // Where the page is, under the list's right edge. The small font
        // draws a 6 like a 0, so the count is in the larger one.
        let last = (self.top + ROWS).min(self.entries.len());
        let pos = format!("{}-{} of {}", self.top + 1, last, self.entries.len());
        let w = width(FONT16)(&pos);
        out.push(text(
            FONT16,
            LIST_X + LIST_W - w,
            LIST_Y + ROWS as i32 * ROW_STEP + 14,
            WHITE,
            &pos,
        ));
        out
    }

    fn ui_elements(&self) -> Vec<UiElement> {
        (0..ROWS)
            .filter_map(|slot| {
                let e = self.entries.get(self.top + slot)?;
                Some(UiElement::new(
                    format!("mods.mod({})", e.id),
                    Self::row_rect(slot),
                ))
            })
            .collect()
    }
}

/// The screen with no mods (tests and the screens' default).
pub fn register(reg: &mut Registry) {
    register_with(reg, Rc::new(NoMods));
}

/// The screen on `host`.
pub fn register_with(reg: &mut Registry, host: Rc<dyn ModsHost>) {
    reg.register(MODS, Box::new(ModsScreen::new(host)));
}

#[cfg(test)]
#[path = "mods_tests.rs"]
mod tests;
