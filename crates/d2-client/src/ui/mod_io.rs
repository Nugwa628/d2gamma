//! What a mod game's panels read beyond the client model, and what they
//! ask the client to do (`crate::mods`).
//!
//! A panel only answers events with a [`UiResponse`]; the game's own
//! panels also hand the UI their state changes, sounds and the game's
//! requests through the UI's shared state, which a mod game's panel does
//! not see. This module gives mod panels both sides, through their
//! context:
//!
//! - [`UiCtx::keys`]: Shift, Ctrl and Alt as the host last saw them;
//! - [`UiCtx::hire_offers`]: the hire list an NPC offers, for a mod game
//!   that shows it in its own NPC menu
//!   ([`crate::mods::ClientMods::with_hire_list_in_npc_menu`]);
//! - [`UiCtx::request`]: a [`ModRequest`], carried out by the client after
//!   the event that asked for it, in order: open or close a UI state, the
//!   game's own add-skill-point and select-skill requests, a hire, a chat
//!   line, a hireling item move, a UI sound, a row of the NPC menu;
//! - for always-on overlays (a speech box, recent chat lines, a chat
//!   prompt): [`UiCtx::speech`], the overhead texts with their units' points;
//!   [`UiCtx::chat_lines`], the game's recent screen messages;
//!   [`UiCtx::unit_point`], a unit's point on the UI; [`UiCtx::npc_menu_npc`],
//!   the NPC whose menu is up; [`UiCtx::npc_menu`], that menu's box as the
//!   game built it. These are filled only for a game with mod panels.
//!
//! The game's rules and pop-ups mod panels read and draw with are in
//! [`crate::ui::original::mod_reads`].
//!
//! D2Gamma's own panels never ask, so its UI does exactly what it did.

use std::cell::RefCell;

use super::geom::Point;
use super::hire_list::Offer;
use super::panel::{ClientIntent, UiCtx};
use super::root::UiRoot;
use crate::bridge::world::UnitKey;
use crate::rules::camera::FrameAnchor;
use crate::ui::original::{OriginalUi, OriginalUiError};

/// The modifier keys held this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// The hire list an NPC offers: the offers the server sent (S→C 0x4F
/// resets the list, each 0x4E adds one), while the list is up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HireOffers {
    /// The NPC the list is up for (its GUID).
    pub npc: u32,
    /// The offers, in the order the server sent them.
    pub offers: Vec<Offer>,
}

/// One overhead text: a player's chat line or a unit's speech (S→C 0x26
/// type 5, 0x27), as the game shows it over its unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Speech {
    pub unit: UnitKey,
    /// The text as the bubble shows it (UTF-16, colour codes kept).
    pub text: Vec<u16>,
    /// The unit's feet on the UI, box space (as [`UiCtx::pointer`]).
    pub at: Point,
}

/// One of the game's recent screen messages (chat lines, quest messages,
/// notes), oldest first, while it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatLine {
    /// The wrapped lines of the message (UTF-16).
    pub lines: Vec<Vec<u16>>,
    /// Text colour index (`ui::item_tip::color`).
    pub color: u32,
    /// When the message leaves the screen (UI clock ms): with the text it
    /// tells a message from an earlier one with the same words.
    pub expiry: u32,
}

/// The NPC menu that is up ([`UiCtx::npc_menu`]), as the game built it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NpcMenu {
    /// The NPC (GUID) and its monster class.
    pub guid: u32,
    pub class: u32,
    /// The box's items top down: the title (the NPC's name), the options
    /// with their captions as the game filled them in (costs, the
    /// hireling's name) and the cancel item. Empty while talking.
    pub items: Vec<NpcMenuItem>,
    /// The selectable rows in order: the option each runs (`None`: cancel),
    /// its string id and cost.
    pub rows: Vec<super::original::npc_box::Row>,
    /// Talk was chosen: the box is closed, the game's talk is up.
    pub talking: bool,
    /// The menu waits for the server (an identify, a resurrect, a hire):
    /// the game shows its waiting note.
    pub waiting: bool,
}

/// One item of the NPC menu's box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NpcMenuItem {
    /// The caption (UTF-16).
    pub text: Vec<u16>,
    /// Text colour index (`ui::item_tip::color`).
    pub color: u8,
    /// The selectable row it is ([`ModRequest::NpcRow`]); `None` for the
    /// title.
    pub row: Option<usize>,
}

/// A request of a mod game's panel ([`UiCtx::request`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModRequest {
    /// Opens (`open`) or closes UI state `state`, through the game's own
    /// UI state rules (the open can be refused; a close runs the state's
    /// close hooks).
    SetUi { state: u8, open: bool },
    /// The game's add-skill-point request (C→S 0x3B) for `skill`.
    AddSkillPoint { skill: u16 },
    /// The game's select-skill request (C→S 0x3C): `skill` on the left
    /// hand (`left`) or the right, from the skill list entry of `owner`
    /// (an item skill's item GUID, else `u32::MAX`, the player's own).
    SelectSkill { skill: u16, left: bool, owner: u32 },
    /// The game's hire request (C→S 0x36) for offer `name` of `npc`'s
    /// list; the list closes.
    Hire { npc: u32, name: u16 },
    /// Opens the hire list of `npc` (`Some`) or closes it (`None`).
    HireList(Option<u32>),
    /// A UI sound by id.
    Sound(i32),
    /// The game's chat line said to every player of the game (C→S 0x15).
    Say(String),
    /// The game's hireling item request (C→S 0x61): with an item on the
    /// cursor, the hireling puts it on (its worn item there comes to the
    /// cursor); with none, the hireling's item at body location `bodyloc`
    /// comes to the cursor.
    MercItem { bodyloc: u16 },
    /// Runs selectable row `n` of the NPC menu that is up
    /// ([`NpcMenu::rows`]) as a click on it does in the game's box: Talk
    /// opens the talk, Trade and Gamble the shop, Hire the hire list,
    /// Identify and Resurrect wait for the server, Cancel ends the chat.
    /// Run at the start of the next UI frame.
    NpcRow(usize),
}

#[derive(Default)]
struct Io {
    keys: Keys,
    hire: Option<HireOffers>,
    requests: Vec<ModRequest>,
    speech: Vec<Speech>,
    chat: Vec<ChatLine>,
    npc_menu: Option<u32>,
    npc_menu_box: Option<NpcMenu>,
    /// The camera inputs of [`UiCtx::unit_point`]: the host's frame anchor
    /// and the open mode.
    anchor: Option<FrameAnchor>,
    open_mode: u8,
}

thread_local! {
    static IO: RefCell<Io> = RefCell::new(Io::default());
}

/// Records the modifier keys held this frame (the host, each frame).
pub(crate) fn set_keys(keys: Keys) {
    IO.with(|io| io.borrow_mut().keys = keys);
}

/// Records this frame's overhead texts (the bubbles' pass).
pub(crate) fn set_speech(speech: Vec<Speech>) {
    IO.with(|io| io.borrow_mut().speech = speech);
}

/// Records the screen messages showing now (the messages' pass).
pub(crate) fn set_chat_lines(chat: Vec<ChatLine>) {
    IO.with(|io| io.borrow_mut().chat = chat);
}

impl UiCtx<'_> {
    /// Shift, Ctrl and Alt as the host last saw them.
    pub fn keys(&self) -> Keys {
        IO.with(|io| io.borrow().keys)
    }

    /// The hire list that is up, with its offers; `None` while none is.
    /// Filled only for a mod game that shows the list in its NPC menu.
    pub fn hire_offers(&self) -> Option<HireOffers> {
        IO.with(|io| io.borrow().hire.clone())
    }

    /// Asks the client for `r`, carried out after the event being handled
    /// (requests made while drawing wait for the next event).
    pub fn request(&self, r: ModRequest) {
        IO.with(|io| io.borrow_mut().requests.push(r));
    }

    /// The overhead texts showing now, players first, with their units'
    /// points (filled by the game's bubble pass, which draws before the
    /// mod game's panels; it draws them too unless the mod game turned
    /// the bubbles off, `ClientMods::without_overhead_bubbles`).
    pub fn speech(&self) -> Vec<Speech> {
        IO.with(|io| io.borrow().speech.clone())
    }

    /// The game's screen messages showing now, oldest first (filled by the
    /// messages' pass, which draws before the mod game's panels; it draws
    /// them too unless the mod game turned them off,
    /// `ClientMods::without_chat_lines`). Empty in a game on a realm,
    /// whose chat window holds them.
    pub fn chat_lines(&self) -> Vec<ChatLine> {
        IO.with(|io| io.borrow().chat.clone())
    }

    /// The NPC (GUID) whose menu is up.
    pub fn npc_menu_npc(&self) -> Option<u32> {
        IO.with(|io| io.borrow().npc_menu)
    }

    /// The NPC menu that is up, with its box's items and rows as the game
    /// built them; `None` while none is.
    pub fn npc_menu(&self) -> Option<NpcMenu> {
        IO.with(|io| io.borrow().npc_menu_box.clone())
    }

    /// Where `unit` stands on the UI now (its feet, box space, as
    /// [`UiCtx::pointer`]), under the frame's camera; `None` for a unit
    /// the model has no position for, or without a local player.
    pub fn unit_point(&self, unit: UnitKey) -> Option<Point> {
        let (anchor, open_mode) = IO.with(|io| {
            let io = io.borrow();
            (io.anchor, io.open_mode)
        });
        let w = self.world;
        let cell = w.units.get(&unit)?.position?;
        let cam = super::original::overhead_ui::camera(w, anchor, open_mode)?;
        let (x, y) = super::original::overhead_ui::unit_point(w, &cam, anchor, unit, cell);
        Some(super::ui_space::current().frame_to_box(Point::new(x, y)))
    }
}

/// The requests made since the last call, oldest first.
pub(crate) fn take_requests() -> Vec<ModRequest> {
    IO.with(|io| std::mem::take(&mut io.borrow_mut().requests))
}

/// The client message of a request that is one; `None` for the others.
pub fn request_intent(r: &ModRequest) -> Option<ClientIntent> {
    use d2_proto::client::{AddSkillPoint, MercItem};
    match *r {
        ModRequest::AddSkillPoint { skill } => {
            Some(ClientIntent::from_message(&AddSkillPoint { skill }))
        }
        ModRequest::SelectSkill { skill, left, owner } => Some(
            super::original::hud_hotkeys::select(i32::from(skill), left, owner),
        ),
        ModRequest::Hire { npc, name } => Some(super::hire_list::hire_intent(npc, name)),
        ModRequest::MercItem { bodyloc } => Some(ClientIntent::from_message(&MercItem { bodyloc })),
        ModRequest::Say(ref text) => Some(ClientIntent(
            super::original::chat_window::game_chat_body(text),
        )),
        ModRequest::SetUi { .. }
        | ModRequest::HireList(_)
        | ModRequest::Sound(_)
        | ModRequest::NpcRow(_) => None,
    }
}

impl OriginalUi {
    /// Fills what mod panels read this frame ([`UiCtx::hire_offers`],
    /// [`UiCtx::npc_menu_npc`], [`UiCtx::npc_menu`], [`UiCtx::unit_point`]'s
    /// camera). Does
    /// nothing for a game without mod panels.
    pub fn refresh_mod_io(&self) {
        if !self.mods_have_panels() {
            return;
        }
        let hire = if self.mods_hire_in_npc_menu() {
            let h = self.hire.borrow();
            h.up.map(|npc| HireOffers {
                npc,
                offers: h.offers.clone(),
            })
        } else {
            None
        };
        let npc_menu = self
            .is_open(super::states::id::NPC_MENU)
            .then(|| self.npc_menu_guid())
            .flatten();
        let npc_menu_box = self.npc_menu_read();
        let (anchor, open_mode) = self.camera_inputs();
        IO.with(|io| {
            let mut io = io.borrow_mut();
            io.hire = hire;
            io.npc_menu = npc_menu;
            io.npc_menu_box = npc_menu_box;
            io.anchor = anchor;
            io.open_mode = open_mode;
        });
    }

    /// Carries out the mod panels' requests ([`take_requests`]) and the
    /// closes of the panels a press or key outside them closed
    /// ([`UiRoot::take_outside_closes`]), in order.
    pub(super) fn apply_mod_requests(&mut self, root: &mut UiRoot) -> Result<(), OriginalUiError> {
        for id in root.take_outside_closes() {
            if usize::from(id.0) < super::layout::UI_STATE_COUNT {
                self.set_ui(u32::from(id.0), 1, false)?;
            } else {
                root.close(id)?;
            }
        }
        for r in take_requests() {
            if let Some(i) = request_intent(&r) {
                root.queue_intent(i);
            }
            match r {
                ModRequest::SetUi { state, open } => {
                    self.set_ui(u32::from(state), if open { 0 } else { 1 }, false)?;
                }
                ModRequest::Hire { .. } => {
                    let mut h = self.hire.borrow_mut();
                    h.hired = true;
                    h.up = None;
                }
                ModRequest::HireList(npc) => self.hire.borrow_mut().up = npc,
                ModRequest::Sound(id) => self.push_ui_sound(id),
                ModRequest::NpcRow(n) => self.npcm.borrow_mut().row_request = Some(n),
                ModRequest::AddSkillPoint { .. }
                | ModRequest::SelectSkill { .. }
                | ModRequest::MercItem { .. }
                | ModRequest::Say(_) => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "mod_io_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "mod_io_overlay_tests.rs"]
mod overlay_tests;
