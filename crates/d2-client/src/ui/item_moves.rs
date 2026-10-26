// D2RUST's own: the item-moving shortcuts.
//! Modifier-clicks that move an item in one click where the plain game
//! needs a lift and a drop: Ctrl+Left between the inventory and the open
//! stash, cube or trade box, Ctrl+Right to drop an item at your feet,
//! Ctrl+Shift+Right into the Horadric Cube without opening it, and
//! Ctrl+Left to sell while a vendor is open. The stash's stackable pages
//! add their own (a stack to its fixed slot, take some or take all, drop
//! the held item on a page tab); they use the same pieces from here.
//!
//! The rules every shortcut keeps:
//! - **Input only.** A shortcut sends the same C→S messages a drag sends
//!   (lift 0x19, then place 0x18, drop 0x17 or into the cube 0x2A), so the
//!   server checks and performs every move, online and in single player.
//!   The client trusts nothing new.
//! - **All or nothing.** The client checks first that the item fits where
//!   it goes ([`first_free_spot`]); a move that can't happen sends nothing
//!   and says why in a short screen message ([`Refusal`]), so no item is
//!   left on the cursor.
//! - **The plain game stays as it is.** A click without the modifiers, a
//!   drag, the belt and the inventory's own size and layout are unchanged.
//!   Each shortcut has its own switch on the settings menu's Controls page
//!   ([`ShortcutSet`]), all on by default; a mod game may switch any of
//!   them off (`crate::mods::ClientMods::without_item_shortcuts`).
//! - **Shown where it applies.** An item's tool tip lists the shortcuts
//!   that work on it right now as grey lines at its bottom ([`hint_lines`]).

use d2_proto::client::{DropItem, InsertItemInBuffer, ItemToCube, RemoveItemFromBuffer};

use super::item_tip::{color, TipLine};
use super::panel::ClientIntent;

/// One item-moving shortcut. The number in each doc line is the
/// shortcut's place in the settings menu's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Shortcut {
    /// 1. Ctrl+Left: inventory ↔ the open stash (its current page).
    Stash,
    /// 2. Ctrl+Left: inventory ↔ the open Horadric Cube.
    Cube,
    /// 3. Ctrl+Left: inventory ↔ your own trade box.
    Trade,
    /// 4. Ctrl+Left on a stackable with the stash open: to its fixed slot
    ///    on the stash's Runes / Gems / Misc page (the stash's pages).
    StackSlot,
    /// 5. Shift+Left on a stack of a stackable stash page: take a chosen
    ///    number (the stash's pages).
    TakeCount,
    /// 6. Ctrl+Left on a stack of a stackable stash page: take as many as
    ///    fit in the inventory (the stash's pages).
    TakeAll,
    /// 7. Ctrl+Right on an inventory item: drop it at your feet.
    Drop,
    /// 8. Ctrl+Shift+Right on an inventory or stash item: into the cube in
    ///    your inventory, without opening it.
    IntoCube,
    /// 9. Ctrl+Left with a vendor open: sell the item.
    Sell,
    /// 10. Shift+Left on a stash page tab while holding an item: drop it
    ///     onto that page (the stash's pages).
    TabDrop,
}

impl Shortcut {
    /// Every shortcut, in the settings menu's order.
    pub const ALL: [Shortcut; 10] = [
        Shortcut::Stash,
        Shortcut::Cube,
        Shortcut::Trade,
        Shortcut::StackSlot,
        Shortcut::TakeCount,
        Shortcut::TakeAll,
        Shortcut::Drop,
        Shortcut::IntoCube,
        Shortcut::Sell,
        Shortcut::TabDrop,
    ];

    /// The shortcut's bit in a [`ShortcutSet`].
    pub const fn bit(self) -> u16 {
        1 << (self as u16)
    }

    /// The key of its switch in `settings.toml`'s `[controls]`.
    pub const fn key(self) -> &'static str {
        match self {
            Shortcut::Stash => "move_stash",
            Shortcut::Cube => "move_cube",
            Shortcut::Trade => "move_trade",
            Shortcut::StackSlot => "stack_to_slot",
            Shortcut::TakeCount => "take_count",
            Shortcut::TakeAll => "take_all",
            Shortcut::Drop => "drop_item",
            Shortcut::IntoCube => "into_cube",
            Shortcut::Sell => "quick_sell",
            Shortcut::TabDrop => "tab_drop",
        }
    }

    /// The mouse and keys that do it, as the tool tip and the settings
    /// menu write them.
    pub const fn keys(self) -> &'static str {
        match self {
            Shortcut::Stash
            | Shortcut::Cube
            | Shortcut::Trade
            | Shortcut::StackSlot
            | Shortcut::TakeAll
            | Shortcut::Sell => "Ctrl+Left",
            Shortcut::TakeCount | Shortcut::TabDrop => "Shift+Left",
            Shortcut::Drop => "Ctrl+Right",
            Shortcut::IntoCube => "Ctrl+Shift+Right",
        }
    }

    /// The settings menu's row label.
    pub const fn label(self) -> &'static str {
        match self {
            Shortcut::Stash => "Move to Stash",
            Shortcut::Cube => "Move to Cube",
            Shortcut::Trade => "Move to Trade",
            Shortcut::StackSlot => "Stack to Slot",
            Shortcut::TakeCount => "Take Some",
            Shortcut::TakeAll => "Take All",
            Shortcut::Drop => "Drop Item",
            Shortcut::IntoCube => "Put in Cube",
            Shortcut::Sell => "Quick Sell",
            Shortcut::TabDrop => "Drop on Page Tab",
        }
    }

    /// The settings menu's one-line description.
    pub const fn description(self) -> &'static str {
        match self {
            Shortcut::Stash => {
                "Ctrl+Left-click moves an item between the inventory and the open stash."
            }
            Shortcut::Cube => {
                "Ctrl+Left-click moves an item between the inventory and the open Horadric Cube."
            }
            Shortcut::Trade => {
                "Ctrl+Left-click moves an item between the inventory and your trade box."
            }
            Shortcut::StackSlot => {
                "With the stash open, Ctrl+Left-click sends a rune, gem or other stackable to its slot."
            }
            Shortcut::TakeCount => {
                "Shift+Left-click on a stash stack asks how many to take."
            }
            Shortcut::TakeAll => {
                "Ctrl+Left-click on a stash stack takes as many as fit in the inventory."
            }
            Shortcut::Drop => "Ctrl+Right-click drops an inventory item at your feet.",
            Shortcut::IntoCube => {
                "Ctrl+Shift+Right-click puts an item into the cube in your inventory without opening it."
            }
            Shortcut::Sell => "Ctrl+Left-click sells an item while a vendor's trade window is open.",
            Shortcut::TabDrop => {
                "Shift+Left-click on a stash page tab drops the held item onto that page."
            }
        }
    }
}

/// Which shortcuts are on: one bit per [`Shortcut`]. The default is every
/// shortcut on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShortcutSet(pub u16);

impl ShortcutSet {
    /// Every shortcut on.
    pub const ALL: ShortcutSet = ShortcutSet((1 << Shortcut::ALL.len()) - 1);
    /// None on.
    pub const NONE: ShortcutSet = ShortcutSet(0);

    pub const fn has(self, s: Shortcut) -> bool {
        self.0 & s.bit() != 0
    }

    pub fn set(&mut self, s: Shortcut, on: bool) {
        if on {
            self.0 |= s.bit();
        } else {
            self.0 &= !s.bit();
        }
    }

    /// The shortcuts on in both sets.
    pub const fn and(self, other: ShortcutSet) -> ShortcutSet {
        ShortcutSet(self.0 & other.0)
    }

    /// This set without the shortcuts of `off`.
    pub const fn without(self, off: ShortcutSet) -> ShortcutSet {
        ShortcutSet(self.0 & !off.0)
    }
}

impl Default for ShortcutSet {
    fn default() -> Self {
        ShortcutSet::ALL
    }
}

/// The game's item pages.
pub mod page {
    pub const INVENTORY: u8 = 0;
    /// The local player's own trade box.
    pub const TRADE: u8 = 2;
    pub const CUBE: u8 = 3;
    pub const STASH: u8 = 4;
}

/// The trade window's state as the client knows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Trade {
    /// The offer is locked (a side accepted): no item may go in or out.
    pub locked: bool,
}

/// The windows open around a click.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Open {
    pub stash: bool,
    pub cube: bool,
    pub trade: Option<Trade>,
    /// A vendor's trade window.
    pub store: bool,
}

/// The mouse button of a click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
}

/// The modifier keys held at a click.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
}

/// What a shortcut does with the clicked item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// To the first free spot of page `to`.
    Move { to: u8 },
    /// On the ground at the player's feet.
    Drop,
    /// Into the cube in the inventory.
    IntoCube,
    /// Sold to the open vendor (the grid click's own sell).
    Sell,
}

impl Action {
    /// The words of the tool tip line.
    pub fn words(self) -> &'static str {
        match self {
            Action::Move { to: page::STASH } => "Move to stash",
            Action::Move { to: page::CUBE } => "Move to cube",
            Action::Move { to: page::TRADE } => "Move to trade",
            Action::Move { .. } => "Move to inventory",
            Action::Drop => "Drop",
            Action::IntoCube => "Put in cube",
            Action::Sell => "Sell",
        }
    }
}

/// The shortcut a click with `mods` and `button` on a stored item of page
/// `from` makes, with the windows `open` and the shortcuts `on`; `None`:
/// no shortcut, the plain click runs. Ctrl+Left reads the open windows in
/// the order vendor, trade, cube, stash: the one window that takes items
/// from the inventory at a time.
pub fn shortcut_for(
    on: ShortcutSet,
    mods: Mods,
    button: Button,
    from: u8,
    open: Open,
) -> Option<(Shortcut, Action)> {
    let pick = |s: Shortcut, a: Action| on.has(s).then_some((s, a));
    match (button, mods.ctrl, mods.shift) {
        (Button::Left, true, false) => match from {
            page::INVENTORY => {
                if open.store {
                    pick(Shortcut::Sell, Action::Sell)
                } else if open.trade.is_some() {
                    pick(Shortcut::Trade, Action::Move { to: page::TRADE })
                } else if open.cube {
                    pick(Shortcut::Cube, Action::Move { to: page::CUBE })
                } else if open.stash {
                    pick(Shortcut::Stash, Action::Move { to: page::STASH })
                } else {
                    None
                }
            }
            page::STASH if open.stash => pick(
                Shortcut::Stash,
                Action::Move {
                    to: page::INVENTORY,
                },
            ),
            page::CUBE if open.cube => pick(
                Shortcut::Cube,
                Action::Move {
                    to: page::INVENTORY,
                },
            ),
            page::TRADE if open.trade.is_some() => pick(
                Shortcut::Trade,
                Action::Move {
                    to: page::INVENTORY,
                },
            ),
            _ => None,
        },
        (Button::Right, true, false) if from == page::INVENTORY => {
            pick(Shortcut::Drop, Action::Drop)
        }
        (Button::Right, true, true) if from == page::INVENTORY || from == page::STASH => {
            pick(Shortcut::IntoCube, Action::IntoCube)
        }
        _ => None,
    }
}

/// Why a shortcut sent nothing: the short screen message it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// No free spot big enough on page `0`.
    NoRoom(u8),
    /// The trade offer is locked.
    TradeLocked,
    /// The cube can't go into itself.
    CubeInCube,
    /// No Horadric Cube stored in the inventory.
    NoCube,
    /// Items can't be dropped during a trade.
    Trading,
}

impl Refusal {
    /// The message's text.
    pub fn text(self) -> &'static str {
        match self {
            Refusal::NoRoom(page::STASH) => "Not enough room in the stash",
            Refusal::NoRoom(page::CUBE) => "Not enough room in the cube",
            Refusal::NoRoom(page::TRADE) => "Not enough room in the trade box",
            Refusal::NoRoom(_) => "Not enough room in the inventory",
            Refusal::TradeLocked => "The trade is locked",
            Refusal::CubeInCube => "The cube can't go into itself",
            Refusal::NoCube => "No Horadric Cube in the inventory",
            Refusal::Trading => "Can't drop items during a trade",
        }
    }
}

/// A footprint on a grid: left column, top row, width and height in cells.
pub type Rect = (i32, i32, i32, i32);

/// The first free spot of a `w` × `h` item on a `cols` × `rows` grid with
/// the footprints `taken`: from the top-left, column by column, each
/// column top to bottom (the game's own top-left search order). `None`
/// when it fits nowhere.
pub fn first_free_spot(
    (cols, rows): (i32, i32),
    taken: &[Rect],
    (w, h): (i32, i32),
) -> Option<(u32, u32)> {
    let (w, h) = (w.max(1), h.max(1));
    let overlaps = |x: i32, y: i32| {
        taken
            .iter()
            .any(|&(tx, ty, tw, th)| tx < x + w && x < tx + tw && ty < y + h && y < ty + th)
    };
    (0..=cols - w)
        .flat_map(|x| (0..=rows - h).map(move |y| (x, y)))
        .find(|&(x, y)| !overlaps(x, y))
        .map(|(x, y)| (x as u32, y as u32))
}

/// Where a moved item goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dest {
    /// Cell (`x`, `y`) of page `page`.
    Page { page: u8, x: u32, y: u32 },
    /// The ground at the player's feet.
    Ground,
    /// Into the cube item `cube`.
    Cube { cube: u32 },
}

/// The C→S messages that move stored item `item` to `dest`, the ones a drag
/// sends: the lift (0x19) to the cursor, then the place (0x18), the drop
/// (0x17) or the cube's put-in (0x2A). The server runs them in order.
pub fn move_intents(item: u32, dest: Dest) -> Vec<ClientIntent> {
    let lift = ClientIntent::from_message(&RemoveItemFromBuffer { item });
    let put = match dest {
        Dest::Page { page, x, y } => ClientIntent::from_message(&InsertItemInBuffer {
            item,
            x,
            y,
            page: u32::from(page),
        }),
        Dest::Ground => ClientIntent::from_message(&DropItem { item }),
        Dest::Cube { cube } => ClientIntent::from_message(&ItemToCube { item, cube }),
    };
    vec![lift, put]
}

/// A grey tool tip line "`keys`: `words`".
pub fn hint_line(s: Shortcut, words: &str) -> TipLine {
    TipLine::new(format!("{}: {words}", s.keys()), color::GREY)
}

/// What the tool tip's lines read about the hovered item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HintFacts {
    /// The item is the Horadric Cube.
    pub is_cube: bool,
    /// The open vendor buys it.
    pub sellable: bool,
    /// A Horadric Cube is stored in the inventory.
    pub cube_in_inventory: bool,
}

/// The grey lines at the bottom of a stored item's tool tip on page `from`:
/// the shortcuts that work on it now, in the order Ctrl+Left, Ctrl+Right,
/// Ctrl+Shift+Right. A shortcut that would refuse for a reason the client
/// can see without a fit search (a locked trade, the cube into itself, no
/// cube, a drop during a trade, an item the vendor doesn't buy) is left
/// out.
pub fn hint_lines(on: ShortcutSet, from: u8, open: Open, facts: HintFacts) -> Vec<TipLine> {
    let clicks = [
        (
            Mods {
                ctrl: true,
                shift: false,
            },
            Button::Left,
        ),
        (
            Mods {
                ctrl: true,
                shift: false,
            },
            Button::Right,
        ),
        (
            Mods {
                ctrl: true,
                shift: true,
            },
            Button::Right,
        ),
    ];
    let locked = open.trade.is_some_and(|t| t.locked);
    clicks
        .iter()
        .filter_map(|&(m, b)| shortcut_for(on, m, b, from, open))
        .filter(|&(s, a)| match a {
            Action::Sell => facts.sellable,
            Action::Move { to } => {
                !(s == Shortcut::Trade && locked) && !(to == page::CUBE && facts.is_cube)
            }
            Action::Drop => open.trade.is_none(),
            Action::IntoCube => facts.cube_in_inventory && !facts.is_cube,
        })
        .map(|(s, a)| hint_line(s, a.words()))
        .collect()
}

#[cfg(test)]
#[path = "item_moves_tests.rs"]
mod tests;
