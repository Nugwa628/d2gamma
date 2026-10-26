//! The UI frame as a mod game's panels see it: its size, the game's UI
//! frame floor, and the panel slots at its edges.
//!
//! Panels lay themselves out in box space (`ui::layout::Screen`): x 0 is
//! the first column of the 800-wide UI box, centred in the UI frame, so
//! the UI frame's first column has a negative x in a wider frame; y 0 is
//! the UI frame's top row. [`UiFrame`] gives a mod game's panels the
//! rectangles they need in that space without the 800-wide box's own
//! layout:
//!
//! - the whole UI frame ([`UiFrame::rect`]);
//! - the floor box: the game's floor (`ui_space::ui_floor`, the size its
//!   art is laid out for) centred across the frame and standing on its
//!   bottom row ([`UiFrame::floor_rect`]);
//! - slots at the frame's edges ([`UiFrame::slot`]): a left slot at the
//!   frame's first column, a right slot ending at its last, a centred slot
//!   in the middle. Each starts on the floor box's top row, so when the UI
//!   frame has more rows than the floor (a smaller UI Scale) the panels
//!   stay on the bottom bar and the world shows above them.
//!
//! [`UiFrame::to_local`] and [`UiFrame::to_box`] convert between box space
//! and points counted from the UI frame's top-left corner.
//!
//! A panel reads both from its context: [`UiCtx::ui_frame`], and
//! [`UiCtx::pointer`], the pointer as the UI root last routed it (a panel
//! only gets the pointer events inside its own rectangle).

use std::sync::atomic::{AtomicU64, Ordering};

use super::geom::{Point, Rect};
use super::panel::UiCtx;
use crate::rules::camera::FrameSize;

/// The pointer the UI root last routed, packed (x, y); [`NO_POINTER`]
/// while it is off the frame.
static POINTER: AtomicU64 = AtomicU64::new(NO_POINTER);
const NO_POINTER: u64 = u64::MAX;

/// Records the pointer the UI root routes (box space; `None` when it
/// left the frame).
pub(crate) fn set_pointer(p: Option<Point>) {
    let v = p.map_or(NO_POINTER, |p| {
        (u64::from(p.x as u32) << 32) | u64::from(p.y as u32)
    });
    POINTER.store(v, Ordering::Relaxed);
}

/// The UI rows a mod game's own HUD covers ([`mod_hud_rows`]); −1: the
/// game's control panel.
static HUD_ROWS: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

/// Records the rows a mod game's own HUD covers (`None`: the game's
/// control panel), at UI install.
pub(crate) fn set_mod_hud_rows(rows: Option<i32>) {
    HUD_ROWS.store(rows.map_or(-1, |r| r.max(0)), Ordering::Relaxed);
}

/// The UI rows at the bottom of the UI frame a mod game's own HUD covers
/// (`crate::mods::ClientMods::with_hud`); `None` with the game's control
/// panel.
pub fn mod_hud_rows() -> Option<i32> {
    let r = HUD_ROWS.load(Ordering::Relaxed);
    (r >= 0).then_some(r)
}

/// The last frame row a press or hover still reaches the world from: the
/// control panel's `H − 49` (`FrameSize::skill_y_limit`), or the row above
/// a mod game's own HUD, both mapped from the UI frame of `space`.
pub fn world_click_limit(space: super::ui_space::UiSpace) -> i32 {
    match mod_hud_rows() {
        Some(rows) => space.y_to_frame(space.ui.height - rows) - 1,
        None => space.y_to_frame(space.ui.skill_y_limit()),
    }
}

fn pointer() -> Option<Point> {
    let v = POINTER.load(Ordering::Relaxed);
    (v != NO_POINTER).then(|| Point::new((v >> 32) as u32 as i32, v as u32 as i32))
}

impl UiCtx<'_> {
    /// The UI frame the panel lays itself out on, with the game's floor
    /// and its slots ([`UiFrame::current`]).
    pub fn ui_frame(&self) -> UiFrame {
        UiFrame::current()
    }

    /// The pointer in box space as the UI root last routed it, wherever it
    /// is on the frame (`None` once it left the frame).
    /// [`UiFrame::to_local`] counts it from the UI frame's corner.
    pub fn pointer(&self) -> Option<Point> {
        pointer()
    }
}

/// Where a slot sits across the UI frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotSide {
    /// At the frame's first column.
    Left,
    /// Ending at the frame's last column.
    Right,
    /// Centred across the frame (an odd spare column goes right).
    Centre,
}

/// The UI frame and the game's floor, for laying out panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiFrame {
    /// The UI frame the UI is laid out on (`ui_space::UiSpace::ui`).
    pub size: FrameSize,
    /// The smallest UI frame the game's layout holds.
    pub floor: FrameSize,
}

impl UiFrame {
    /// The play UI's frame now ([`super::ui_space::current`]) and the
    /// game's floor.
    pub fn current() -> UiFrame {
        UiFrame {
            size: super::ui_space::current().ui,
            floor: super::ui_space::ui_floor(),
        }
    }

    /// The UI frame's first column in box space (0 when the frame is the
    /// box's width or narrower).
    pub fn left(&self) -> i32 {
        -self.size.ui_left()
    }

    /// One past the UI frame's last column in box space.
    pub fn right(&self) -> i32 {
        self.left() + self.size.width
    }

    /// The whole UI frame in box space.
    pub fn rect(&self) -> Rect {
        Rect::new(
            self.left(),
            0,
            clamp_u16(self.size.width),
            clamp_u16(self.size.height),
        )
    }

    /// The floor box's top row: the rows the UI frame has above the floor
    /// (0 when it has no more).
    pub fn floor_top(&self) -> i32 {
        (self.size.height - self.floor.height).max(0)
    }

    /// The floor box in box space: the floor's size, centred across the
    /// frame, standing on its bottom row.
    pub fn floor_rect(&self) -> Rect {
        self.slot(SlotSide::Centre, self.floor.width, self.floor.height)
    }

    /// A `w × h` slot on `side`, its top on the floor box's top row.
    pub fn slot(&self, side: SlotSide, w: i32, h: i32) -> Rect {
        let x = match side {
            SlotSide::Left => self.left(),
            SlotSide::Right => self.right() - w,
            SlotSide::Centre => self.left() + (self.size.width - w) / 2,
        };
        Rect::new(x, self.floor_top(), clamp_u16(w), clamp_u16(h))
    }

    /// The bottom `rows` rows across the whole UI frame: where a HUD of
    /// that height goes.
    pub fn bottom_band(&self, rows: i32) -> Rect {
        let rows = rows.clamp(0, self.size.height);
        Rect::new(
            self.left(),
            self.size.height - rows,
            clamp_u16(self.size.width),
            clamp_u16(rows),
        )
    }

    /// A box-space point counted from the UI frame's top-left corner.
    pub fn to_local(&self, p: Point) -> Point {
        Point::new(p.x - self.left(), p.y)
    }

    /// A point counted from the UI frame's top-left corner, in box space
    /// ([`Self::to_local`]'s inverse).
    pub fn to_box(&self, p: Point) -> Point {
        Point::new(p.x + self.left(), p.y)
    }
}

fn clamp_u16(v: i32) -> u16 {
    v.clamp(0, i32::from(u16::MAX)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::ui_space::{UiSpace, MIN_UI};

    const VIEW: FrameSize = FrameSize::PLAY;
    const FLOOR: FrameSize = FrameSize {
        width: 1024,
        height: 768,
    };

    /// A 1024 × 768 floor on the 1280 × 720 frame: the 1366 × 768 UI frame,
    /// 512 × 704 slots at its edges, the floor box centred 171 columns in.
    #[test]
    fn slots_open_at_the_ui_frames_edges_on_the_floor_box() {
        let f = UiFrame {
            size: UiSpace::with_floor(VIEW, 100, FLOOR).ui,
            floor: FLOOR,
        };
        assert_eq!(f.size.width, 1366);
        assert_eq!(f.left(), -283);
        assert_eq!(f.rect(), Rect::new(-283, 0, 1366, 768));
        assert_eq!(
            f.slot(SlotSide::Left, 512, 704),
            Rect::new(-283, 0, 512, 704)
        );
        let right = f.slot(SlotSide::Right, 512, 704);
        assert_eq!(right.right(), i64::from(f.right()));
        assert_eq!(f.to_local(right.origin()), Point::new(1366 - 512, 0));
        assert_eq!(f.to_local(f.floor_rect().origin()), Point::new(171, 0));
        let p = Point::new(40, 300);
        assert_eq!(f.to_box(f.to_local(p)), p);
    }

    /// A UI frame taller than the floor (UI Scale 75 %): the slots move
    /// down onto the bottom rows and the world shows above them.
    #[test]
    fn slots_stand_on_the_bottom_rows_of_a_taller_ui_frame() {
        let f = UiFrame {
            size: UiSpace::with_floor(VIEW, 75, FLOOR).ui,
            floor: FLOOR,
        };
        assert_eq!(f.size.height, 960);
        assert_eq!(f.floor_top(), 192);
        let left = f.slot(SlotSide::Left, 512, 704);
        assert_eq!((left.x, left.y), (f.left(), 192));
        assert_eq!(f.floor_rect().bottom(), 960);
    }

    /// D2Gamma's own frame and floor: the box is the frame's middle 800
    /// columns, and the floor box is the 800 × 600 box on the bottom rows.
    #[test]
    fn the_default_floor_box_is_the_ui_box() {
        let f = UiFrame {
            size: VIEW,
            floor: MIN_UI,
        };
        assert_eq!(f.left(), -240);
        assert_eq!(f.floor_rect(), Rect::new(0, 120, 800, 600));
        let at_800 = UiFrame {
            size: FrameSize::D2RUST,
            floor: MIN_UI,
        };
        assert_eq!(at_800.rect(), Rect::new(0, 0, 800, 600));
        assert_eq!(at_800.floor_top(), 0);
    }

    /// The pointer the root routes reaches every panel's context, off its
    /// own rectangle and left of the box too.
    #[test]
    fn panels_read_the_pointer_the_root_routed() {
        use crate::bridge::world::ClientWorld;
        use crate::ui::panel::{NoStrings, UiEvent};
        use crate::ui::{NoPanelRules, UiRoot};
        let world = ClientWorld::default();
        let ctx = UiCtx {
            tick: 0,
            world: &world,
            strings: &NoStrings,
        };
        let mut root = UiRoot::new(Box::new(NoPanelRules));
        assert_eq!(ctx.pointer(), None);
        root.dispatch(UiEvent::CursorMoved(Point::new(-200, 700)), &ctx);
        assert_eq!(ctx.pointer(), Some(Point::new(-200, 700)));
        root.dispatch(UiEvent::CursorLeft, &ctx);
        assert_eq!(ctx.pointer(), None);
        assert_eq!(ctx.ui_frame(), UiFrame::current());
    }
}
