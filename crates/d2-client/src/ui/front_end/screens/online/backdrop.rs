//! The realm lobby's backgrounds: places in the game for the picture
//! behind the lobby's panels, and the picker at the lobby's top left that
//! chooses one. Until a place has a picture of its own the lobby shows
//! the main menu's art (`app::lobby_picture`). The choice is saved in the
//! settings (`[video] lobby_background`); the Worldstone Chamber is the
//! default.
//!
//! - **Picker**: the menus' text box with the chosen background's name and
//!   a down arrow; a click opens the list under it (gold names on black, a
//!   gold edge, the chosen one white). A click on a name picks it, a click
//!   anywhere else or Esc closes the list.
//! - **Places** ([`Place`]): the level a scene of the place warps to, the
//!   time of day it is held at, the camera's moves from where the warp
//!   lands, and the scene's own brightness (night scenes are drawn brighter).

use crate::ui::front_end::{Action, Control, ControlKind, DrawItem};
use crate::ui::geom::{Point, Rect};

use super::widgets::{field_text, fit, FIELD_H, FIELD_W, FONT16, GOLD, SCROLL, TEXTBOX, WHITE};

/// The picker's field is as wide as its list: the text box's art drawn in
/// two halves that meet in the middle ([`Picker::underlay`]).
pub const BOX_W: u16 = LIST_W as u16;

/// The lobby's backgrounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Backdrop {
    #[default]
    WorldstoneChamber,
    RogueEncampment,
    PandemoniumFortress,
    ArreatSummit,
    LutGholein,
    Tristram,
}

/// Night, dusk and day as the `time` poke sets them: the environment's
/// period and its ticks into the day.
const NIGHT: (u8, u32) = (5, 33280);
const DUSK: (u8, u32) = (4, 24320);
const DAY: (u8, u32) = (2, 6400);

/// Where and when a background is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    /// The level the scene warps to.
    pub level: u16,
    /// The time of day it is held at: the environment's period and ticks.
    pub time: (u8, u32),
    /// The camera's moves from where the warp lands, in subtiles, one
    /// after another (a long way goes in two steps, the rooms on the way
    /// loaded by the first).
    pub moves: &'static [(i32, i32)],
    /// The scene's brightness, a gamma as the settings' (155: as drawn).
    pub gamma: u16,
}

impl Backdrop {
    pub const ALL: [Backdrop; 6] = [
        Backdrop::WorldstoneChamber,
        Backdrop::RogueEncampment,
        Backdrop::PandemoniumFortress,
        Backdrop::ArreatSummit,
        Backdrop::LutGholein,
        Backdrop::Tristram,
    ];

    /// The settings text (`[video] lobby_background`).
    pub fn key(self) -> &'static str {
        match self {
            Backdrop::WorldstoneChamber => "worldstone-chamber",
            Backdrop::RogueEncampment => "rogue-encampment",
            Backdrop::PandemoniumFortress => "pandemonium-fortress",
            Backdrop::ArreatSummit => "arreat-summit",
            Backdrop::LutGholein => "lut-gholein",
            Backdrop::Tristram => "tristram",
        }
    }

    /// The background of a settings text; `None` for an unknown one.
    pub fn from_key(key: &str) -> Option<Backdrop> {
        Backdrop::ALL.into_iter().find(|b| b.key() == key)
    }

    /// The name the picker's field shows: [`Self::name`] without the time
    /// of day, which the list shows.
    pub fn short(self) -> &'static str {
        match self {
            Backdrop::RogueEncampment => "Rogue Encampment",
            Backdrop::LutGholein => "Lut Gholein",
            Backdrop::Tristram => "Tristram",
            b => b.name(),
        }
    }

    /// The name the picker's list shows.
    pub fn name(self) -> &'static str {
        match self {
            Backdrop::WorldstoneChamber => "Worldstone Chamber",
            Backdrop::RogueEncampment => "Rogue Encampment (night)",
            Backdrop::PandemoniumFortress => "Pandemonium Fortress",
            Backdrop::ArreatSummit => "Arreat Summit",
            Backdrop::LutGholein => "Lut Gholein (dusk)",
            Backdrop::Tristram => "Tristram (burning)",
        }
    }

    pub fn place(self) -> Place {
        let (level, time, moves, gamma): (u16, _, &'static [(i32, i32)], u16) = match self {
            Backdrop::WorldstoneChamber => (132, NIGHT, &[(-10, 10)], 175),
            Backdrop::RogueEncampment => (1, NIGHT, &[(4, 16)], 200),
            Backdrop::PandemoniumFortress => (103, NIGHT, &[(-10, 10)], 185),
            Backdrop::ArreatSummit => (120, DAY, &[(10, -10)], 155),
            Backdrop::LutGholein => (40, DUSK, &[(-12, -12)], 185),
            Backdrop::Tristram => (38, DUSK, &[(-26, 26), (-6, 6)], 215),
        };
        Place {
            level,
            time,
            moves,
            gamma,
        }
    }
}

/// The picker's field: its left, its cel bottom (the lobby's top left,
/// over the chat's left edge).
pub const FIELD: (i32, i32) = (46, 54);
/// The list's rows under the field: its width, a row's height, and the
/// text's baseline in a row.
const LIST_W: i32 = 236;
const ROW_H: i32 = 20;
const ROW_BASE: i32 = 16;
/// The names' left inside the list.
const PAD: i32 = 8;
/// The arrow (`joingamescrollbars` cel 1, 12 × 14) inside the field's
/// right end.
const ARROW: (i32, i32) = (BOX_W as i32 - 19, -6);

/// The background picker's state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Picker {
    /// The list is open.
    pub open: bool,
    /// The row under the pointer while open.
    hover: Option<usize>,
}

impl Picker {
    /// The field's rect (top-left, size).
    pub fn field() -> Rect {
        Rect::new(FIELD.0, FIELD.1 - FIELD_H as i32, BOX_W, FIELD_H)
    }

    /// The open list's rect, under the field.
    pub fn list() -> Rect {
        Rect::new(
            FIELD.0,
            FIELD.1 + 2,
            LIST_W as u16,
            (ROW_H * Backdrop::ALL.len() as i32 + 4) as u16,
        )
    }

    /// Row `i` of the open list.
    pub fn row(i: usize) -> Rect {
        let r = Self::list();
        Rect::new(r.x, r.y + 2 + ROW_H * i as i32, r.w, ROW_H as u16)
    }

    /// The field's text box, [`BOX_W`] wide: the art's left and right
    /// halves.
    pub fn underlay() -> Vec<DrawItem> {
        let (x, y) = FIELD;
        let half = i32::from(BOX_W) / 2;
        debug_assert!(half <= i32::from(FIELD_W));
        [(x, 0), (x + half, -half)]
            .map(|(at, from)| DrawItem::Strip {
                file: TEXTBOX,
                frame: 0,
                at: Point::new(at, y),
                from,
                w: half,
                mode: 5,
            })
            .to_vec()
    }

    /// The field's controls: the `chosen` background's name (in the text
    /// boxes' own font) on the box ([`Self::underlay`]), the arrow and the
    /// click area that opens or closes the list (`toggle`).
    pub fn controls(&self, v: &mut Vec<Control>, chosen: Backdrop, toggle: u32) {
        let (x, y) = FIELD;
        let mut name = field_text(x, y);
        name.text = Some(chosen.short().to_string());
        name.w = ARROW.0 as u16;
        v.push(name);
        let mut arrow =
            Control::new(ControlKind::Image, x + ARROW.0, y + ARROW.1, 12, 14).with_art(SCROLL);
        arrow.cel = Some(1);
        v.push(arrow);
        v.push(
            Control::new(ControlKind::Image, x, y, BOX_W, FIELD_H)
                .with_action(Action::Custom(toggle)),
        );
    }

    /// The open list, over everything: black rows in a gold edge, the
    /// names in gold, the `chosen` one and the one under the pointer in
    /// white. Nothing while closed.
    pub fn overlay(&self, chosen: Backdrop, adv: &dyn Fn(u16, &[u16]) -> i32) -> Vec<DrawItem> {
        let width = |s: &str| adv(FONT16, &s.encode_utf16().collect::<Vec<_>>());
        if !self.open {
            return Vec::new();
        }
        let r = Self::list();
        let (w, h) = (i32::from(r.w), i32::from(r.h));
        let mut out = vec![DrawItem::Rect {
            at: r.origin(),
            w,
            h,
        }];
        out.push(DrawItem::Border {
            at: r.origin(),
            w,
            h,
        });
        for (i, b) in Backdrop::ALL.into_iter().enumerate() {
            let lit = b == chosen || self.hover == Some(i);
            let top = r.y + 2 + ROW_H * i as i32;
            out.push(text(
                r.x + PAD,
                top + ROW_BASE,
                if lit { WHITE } else { GOLD },
                &fit(b.name(), w - 2 * PAD, &width),
            ));
        }
        out
    }

    /// The list row at `p` while open.
    pub fn row_at(&self, p: Point) -> Option<usize> {
        let r = Self::list();
        if !self.open || !r.contains(p) {
            return None;
        }
        let dy = p.y - r.y - 2;
        let rows = ROW_H * Backdrop::ALL.len() as i32;
        (0..rows).contains(&dy).then_some((dy / ROW_H) as usize)
    }

    /// The pointer moved: the row it is on lights up.
    pub fn pointer(&mut self, p: Point) {
        self.hover = self.row_at(p);
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.hover = None;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.hover = None;
    }
}

fn text(x: i32, y: i32, color: i32, text: &str) -> DrawItem {
    DrawItem::Text {
        string_id: 0,
        text: text.to_string(),
        font: FONT16,
        at: Point::new(x, y),
        label: None,
        color,
        boxed: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_background_has_its_own_key_and_place() {
        assert_eq!(Backdrop::default(), Backdrop::WorldstoneChamber);
        for b in Backdrop::ALL {
            assert_eq!(Backdrop::from_key(b.key()), Some(b));
            assert!(!b.place().moves.is_empty());
        }
        assert_eq!(Backdrop::from_key("kurast-docks"), None);
        let levels: std::collections::BTreeSet<u16> =
            Backdrop::ALL.iter().map(|b| b.place().level).collect();
        assert_eq!(levels.len(), Backdrop::ALL.len());
        assert_eq!(Backdrop::WorldstoneChamber.place().level, 132);
        assert_eq!(Backdrop::RogueEncampment.place().time, NIGHT);
        assert_eq!(Backdrop::Tristram.place().moves.len(), 2);
    }

    #[test]
    fn the_picker_lists_every_background_under_its_field() {
        let adv = |_: u16, s: &[u16]| 8 * s.len() as i32;
        let mut p = Picker::default();
        assert!(p.overlay(Backdrop::ArreatSummit, &adv).is_empty(), "closed");
        let mut field = Vec::new();
        p.controls(&mut field, Backdrop::Tristram, 9);
        let name: Vec<_> = field.iter().filter_map(|c| c.text.as_deref()).collect();
        assert_eq!(name, ["Tristram"], "the field names the chosen one");
        assert!(field
            .iter()
            .any(|c| c.action == Action::Custom(9) && c.w == BOX_W));
        let halves = Picker::underlay();
        assert!(matches!(
            halves.as_slice(),
            [DrawItem::Strip { w: a, .. }, DrawItem::Strip { w: b, .. }]
                if a + b == i32::from(BOX_W) && *a <= i32::from(FIELD_W)
        ));
        p.toggle();
        let list = Picker::list();
        assert!(list.y > Picker::field().y + i32::from(Picker::field().h));
        let open = p.overlay(Backdrop::ArreatSummit, &adv);
        let rows: Vec<(String, i32)> = open
            .iter()
            .skip(2)
            .map(|i| match i {
                DrawItem::Text { text, color, .. } => (text.clone(), *color),
                other => panic!("a row is text: {other:?}"),
            })
            .collect();
        assert_eq!(rows.len(), Backdrop::ALL.len());
        assert_eq!(rows[3].1, WHITE, "the chosen one is lit");
        assert_eq!(rows[0].1, GOLD);
        // Each row takes the clicks on it, and nothing past the last.
        for i in 0..Backdrop::ALL.len() {
            let y = list.y + 2 + ROW_H * i as i32 + 5;
            assert_eq!(p.row_at(Point::new(list.x + 10, y)), Some(i));
        }
        let below = list.y + i32::from(list.h) + 1;
        assert_eq!(p.row_at(Point::new(list.x + 10, below)), None);
        p.pointer(Point::new(list.x + 10, list.y + 2 + 5));
        let lit = p.overlay(Backdrop::ArreatSummit, &adv);
        assert!(matches!(&lit[2], DrawItem::Text { color: WHITE, .. }));
        p.close();
        assert_eq!(p.row_at(Point::new(list.x + 10, list.y + 7)), None);
    }
}
