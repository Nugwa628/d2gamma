//! A mod game's own stash panel on the game's stash (ui 0x19)
//! ([`ClientMods::with_stash_panel`](super::ClientMods::with_stash_panel)).
//!
//! The mod draws its panel (its art, its grid, book buttons, page and
//! category tabs) over the game's stash panel, and gets the panel's input
//! as [`StashInput`]s; the engine's stash logic keeps running around it
//! (opening and closing with the stash, the close button, the gold
//! dialogs, the item on the cursor). D2RUST's shared stash parts (its tabs,
//! pages and the inventory's send button) are not shown while a mod panel
//! is registered: the mod's panel takes their place.
//!
//! The mod's hooks say where things are ([`StashPanelMod::spot`]); the
//! engine turns pointer presses and keys into inputs:
//! - Left on a book, page tab or category: [`StashInput::Book`],
//!   [`StashInput::Page`], [`StashInput::Category`].
//! - Right on a page tab the mod names ([`StashPanelMod::page_name`]):
//!   the engine's text field (at most [`PAGE_NAME_MAX`] characters), then
//!   [`StashInput::Rename`] on Enter.
//! - The send-to-stash and sort buttons: [`StashInput::SendToStash`],
//!   [`StashInput::Sort`].
//! - A slot of counted goods: Left takes one to the cursor, Shift+Left
//!   one to the inventory, Ctrl+Left as many as fit, Alt+Left a pack, and
//!   Right opens the engine's amount field ([`StashPanelMod::amount_max`])
//!   ([`StashInput::Withdraw`], [`Withdraw`]).
//! - A cell of the mod's grid and any other spot: [`StashInput::Cell`],
//!   [`StashInput::Other`], with the button, keys and the cursor item.
//! - Ctrl+Left on an inventory item while the stash is open:
//!   [`StashInput::Store`]; when the mod answers nothing, the game's own
//!   Ctrl+Left runs.
//!
//! The panel decides nothing about goods. Its answers ([`StashOut`]) are
//! mod messages to the mod game's server (an envelope each, `ModRules`
//! decides), UI sounds, or asks of D2RUST's shared stash store.

use crate::bridge::world::ClientWorld;
use crate::ui::draw::UiDrawSink;
use crate::ui::geom::{Point, Rect};
use crate::ui::layout::Screen;
use crate::ui::original::FontMeasure;
use crate::ui::panels::UiFiles;
use crate::ui::shared_stash::StashAsk;

/// The longest page name the rename field takes, in characters.
pub const PAGE_NAME_MAX: usize = 12;
/// The most digits the amount field takes (a `u32` count).
pub const AMOUNT_DIGITS: usize = 10;

/// What a mod stash panel reads when it draws or answers.
pub struct StashCtx<'a> {
    pub world: &'a ClientWorld,
    pub screen: Screen,
    /// The art file ids (`ImageRef::file`).
    pub files: &'a UiFiles,
    pub fonts: Option<&'a FontMeasure>,
    pub mouse: Point,
    /// The GUID of the item on the cursor, if any.
    pub cursor: Option<u32>,
}

/// A place on the mod's panel ([`StashPanelMod::spot`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StashSpot {
    /// A book button (the mod numbers its books).
    Book(u8),
    /// A page tab.
    Page(u8),
    /// A category tab.
    Category(u8),
    SendToStash,
    Sort,
    /// A slot of counted goods, by the mod's id.
    Slot(u32),
    /// A cell of the mod's item grid.
    Cell {
        x: u8,
        y: u8,
    },
    /// Any other part of the panel, by the mod's id.
    Other(u16),
}

/// The modifier keys of a press.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// How a slot is withdrawn from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Withdraw {
    /// Left: one onto the cursor.
    One,
    /// Shift+Left: one into the inventory.
    ToInventory,
    /// Ctrl+Left: as many as fit in the inventory.
    AllThatFit,
    /// Right, then the amount field: this many.
    Amount(u32),
    /// Alt+Left: one pack (the mod's own container of the goods).
    Pack,
}

/// One input of the mod's panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StashInput {
    Book(u8),
    Page(u8),
    Category(u8),
    /// Page `page` renamed to `name` (at most [`PAGE_NAME_MAX`]
    /// characters, may be empty).
    Rename {
        page: u8,
        name: String,
    },
    SendToStash,
    Sort,
    Withdraw {
        slot: u32,
        how: Withdraw,
    },
    /// A press on a cell of the mod's grid.
    Cell {
        x: u8,
        y: u8,
        right: bool,
        keys: Keys,
        cursor: Option<u32>,
    },
    /// Ctrl+Left on inventory item `guid` while the stash is open.
    Store {
        guid: u32,
    },
    /// A press on another spot of the panel.
    Other {
        id: u16,
        right: bool,
        keys: Keys,
        cursor: Option<u32>,
    },
}

/// What the mod's panel answers an input with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StashOut {
    /// A mod message to the server (sent as one C→S envelope).
    Send(Vec<u8>),
    /// A UI sound by id.
    Sound(i32),
    /// An ask of D2RUST's shared stash store (when one is open).
    Ask(StashAsk),
}

/// Which engine text field is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// Renaming page `page`.
    Rename { page: u8 },
    /// An amount of slot `slot`, at most `max`.
    Amount { slot: u32, max: u32 },
}

/// The engine's text field over the mod's panel: the page rename or the
/// withdraw amount.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StashField {
    pub kind: FieldKind,
    pub text: String,
}

/// A mod game's stash panel, made by its [`StashPanelFactory`] when the UI
/// installs. Only `draw` and `spot` are required.
pub trait StashPanelMod {
    /// The part of the stash panel the mod's panel covers: there the
    /// game's own stash grid and gold are neither drawn nor pressed.
    /// `None`: nothing (the mod draws over the game's panel and grid).
    /// Default: the whole left half ([`Screen::left_side_rect`]); the
    /// game's close button still works where the game draws it.
    fn cover(&self, ctx: &StashCtx) -> Option<Rect> {
        Some(ctx.screen.left_side_rect())
    }
    /// Draws the panel, over the game's stash panel art.
    fn draw(&self, ctx: &StashCtx, out: &mut dyn UiDrawSink);
    /// What is at `at` (frame pixels).
    fn spot(&self, ctx: &StashCtx, at: Point) -> Option<StashSpot>;
    /// An input; the answers go out in order.
    fn input(&mut self, ctx: &StashCtx, input: StashInput) -> Vec<StashOut> {
        let _ = (ctx, input);
        Vec::new()
    }
    /// Page `page`'s name, for the rename field; `None`: the page can't be
    /// renamed (Right on its tab does nothing).
    fn page_name(&self, page: u8) -> Option<String> {
        let _ = page;
        None
    }
    /// The most the amount field may ask of slot `slot`; `None` (or 0):
    /// Right on the slot opens no field.
    fn amount_max(&self, slot: u32) -> Option<u32> {
        let _ = slot;
        None
    }
    /// Draws the open text field in the mod's own art. `false`: the
    /// engine draws its plain box.
    fn draw_field(&self, ctx: &StashCtx, field: &StashField, out: &mut dyn UiDrawSink) -> bool {
        let _ = (ctx, field, out);
        false
    }
}

/// Makes a mod game's stash panel. The registry holds the factory; the UI
/// makes the panel when it installs (panels hold UI-thread state).
pub trait StashPanelFactory: Send + Sync {
    /// The art files the panel draws, added to the UI's file list.
    fn files(&self) -> Vec<String> {
        Vec::new()
    }
    fn make(&self, files: &UiFiles) -> Box<dyn StashPanelMod>;
}

/// The engine's side of a mod stash panel: the panel, the open text field
/// and the Alt key.
pub struct ModStash {
    pub panel: Box<dyn StashPanelMod>,
    pub field: Option<StashField>,
    /// Alt held (set by the host each frame).
    pub alt: bool,
}

impl std::fmt::Debug for ModStash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModStash")
            .field("field", &self.field)
            .field("alt", &self.alt)
            .finish_non_exhaustive()
    }
}

/// What a press on the panel did.
#[derive(Debug, PartialEq, Eq)]
pub enum Pressed {
    /// Nothing of the mod's there.
    Missed,
    /// Taken; the answers to send on.
    Taken(Vec<StashOut>),
}

impl ModStash {
    pub fn new(panel: Box<dyn StashPanelMod>) -> ModStash {
        ModStash {
            panel,
            field: None,
            alt: false,
        }
    }

    /// The press of `right` or left at `at` with `keys` held. A press
    /// closes an open field first.
    pub fn press(&mut self, ctx: &StashCtx, at: Point, right: bool, keys: Keys) -> Pressed {
        self.field = None;
        let Some(spot) = self.panel.spot(ctx, at) else {
            return Pressed::Missed;
        };
        let cursor = ctx.cursor;
        let input = match (spot, right) {
            (StashSpot::Book(b), false) => StashInput::Book(b),
            (StashSpot::Page(p), false) => StashInput::Page(p),
            (StashSpot::Category(c), false) => StashInput::Category(c),
            (StashSpot::SendToStash, false) => StashInput::SendToStash,
            (StashSpot::Sort, false) => StashInput::Sort,
            (StashSpot::Slot(slot), false) => StashInput::Withdraw {
                slot,
                how: withdraw_of(keys),
            },
            (StashSpot::Page(page), true) => {
                if let Some(name) = self.panel.page_name(page) {
                    self.field = Some(StashField {
                        kind: FieldKind::Rename { page },
                        text: name.chars().take(PAGE_NAME_MAX).collect(),
                    });
                }
                return Pressed::Taken(Vec::new());
            }
            (StashSpot::Slot(slot), true) => {
                if let Some(max) = self.panel.amount_max(slot).filter(|&m| m > 0) {
                    self.field = Some(StashField {
                        kind: FieldKind::Amount { slot, max },
                        text: String::new(),
                    });
                }
                return Pressed::Taken(Vec::new());
            }
            (StashSpot::Cell { x, y }, right) => StashInput::Cell {
                x,
                y,
                right,
                keys,
                cursor,
            },
            (StashSpot::Other(id), right) => StashInput::Other {
                id,
                right,
                keys,
                cursor,
            },
            // Right on a book, category or button: nothing.
            (_, true) => return Pressed::Taken(Vec::new()),
        };
        Pressed::Taken(self.panel.input(ctx, input))
    }

    /// A typed UTF-16 unit while a field is open: `None` when no field is
    /// open (the key goes on), else the answers of a finished field.
    pub fn key(&mut self, ctx: &StashCtx, unit: u16) -> Option<Vec<StashOut>> {
        const BACKSPACE: u16 = 0x08;
        const ENTER: u16 = 0x0D;
        const ESCAPE: u16 = 0x1B;
        let f = self.field.as_mut()?;
        match unit {
            ENTER => {
                let f = self.field.take()?;
                let input = match f.kind {
                    FieldKind::Rename { page } => StashInput::Rename { page, name: f.text },
                    FieldKind::Amount { slot, max } => {
                        let n = f.text.parse::<u64>().unwrap_or(0).min(u64::from(max)) as u32;
                        if n == 0 {
                            return Some(Vec::new());
                        }
                        StashInput::Withdraw {
                            slot,
                            how: Withdraw::Amount(n),
                        }
                    }
                };
                return Some(self.panel.input(ctx, input));
            }
            ESCAPE => self.field = None,
            BACKSPACE => {
                f.text.pop();
            }
            u => match f.kind {
                FieldKind::Rename { .. } => {
                    if let Some(c) = char::from_u32(u32::from(u)).filter(|c| !c.is_control()) {
                        if f.text.chars().count() < PAGE_NAME_MAX {
                            f.text.push(c);
                        }
                    }
                }
                FieldKind::Amount { .. } => {
                    if (u16::from(b'0')..=u16::from(b'9')).contains(&u)
                        && f.text.len() < AMOUNT_DIGITS
                    {
                        f.text.push(char::from(u as u8));
                    }
                }
            },
        }
        Some(Vec::new())
    }

    /// Ctrl+Left on inventory item `guid` with the stash open: the mod's
    /// answers, `None` when it gave none (the game's own move runs).
    pub fn store(&mut self, ctx: &StashCtx, guid: u32) -> Option<Vec<StashOut>> {
        let out = self.panel.input(ctx, StashInput::Store { guid });
        (!out.is_empty()).then_some(out)
    }
}

/// The withdraw of a Left press with `keys` held: Alt before Ctrl before
/// Shift.
fn withdraw_of(keys: Keys) -> Withdraw {
    if keys.alt {
        Withdraw::Pack
    } else if keys.ctrl {
        Withdraw::AllThatFit
    } else if keys.shift {
        Withdraw::ToInventory
    } else {
        Withdraw::One
    }
}

#[cfg(test)]
#[path = "stash_tests.rs"]
pub(crate) mod tests;
