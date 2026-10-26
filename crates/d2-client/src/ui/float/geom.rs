//! The geometry of floating panels: clamping a rect to the canvas, snapping
//! its edges to the canvas edges and to other panels, and rects anchored to
//! the nearest canvas corner, so a saved layout lands in the same corner
//! after the view changes size (the lobby's 800 × 600, a game's 1280 × 720).

use crate::ui::geom::Rect;

/// Edges closer than this (pixels) snap together.
pub const SNAP: i32 = 8;

/// The area panels live in: the lobby's frame or the game view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Canvas {
    pub w: u16,
    pub h: u16,
}

impl Canvas {
    pub const fn new(w: u16, h: u16) -> Self {
        Self { w, h }
    }
}

/// A panel's smallest size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MinSize {
    pub w: u16,
    pub h: u16,
}

impl MinSize {
    pub const fn new(w: u16, h: u16) -> Self {
        Self { w, h }
    }

    /// The minimum, never larger than the canvas.
    pub fn within(self, c: Canvas) -> Self {
        Self::new(self.w.min(c.w), self.h.min(c.h))
    }
}

/// The rect of the edges `l..r`, `t..b` (empty when inverted).
pub(super) fn from_edges(l: i32, t: i32, r: i32, b: i32) -> Rect {
    let size = |a: i32, b: i32| (b - a).clamp(0, i32::from(u16::MAX)) as u16;
    Rect::new(l, t, size(l, r), size(t, b))
}

/// Exclusive right edge.
pub(super) fn right(r: Rect) -> i32 {
    r.x + i32::from(r.w)
}

/// Exclusive bottom edge.
pub(super) fn bottom(r: Rect) -> i32 {
    r.y + i32::from(r.h)
}

/// `r` moved (and shrunk only when larger than the canvas) to lie wholly
/// inside the canvas.
pub fn clamp(r: Rect, c: Canvas) -> Rect {
    let w = r.w.min(c.w);
    let h = r.h.min(c.h);
    Rect::new(
        r.x.clamp(0, i32::from(c.w - w)),
        r.y.clamp(0, i32::from(c.h - h)),
        w,
        h,
    )
}

/// `r` grown to `min` (keeping its top-left corner), then clamped.
pub fn at_least(r: Rect, min: MinSize, c: Canvas) -> Rect {
    let m = min.within(c);
    clamp(Rect::new(r.x, r.y, r.w.max(m.w), r.h.max(m.h)), c)
}

/// The spans `a0..a1` and `b0..b1` overlap, or come within [`SNAP`].
fn near_span(a0: i32, a1: i32, b0: i32, b1: i32) -> bool {
    a0 < b1 + SNAP && b0 < a1 + SNAP
}

/// The x lines a panel at `r` snaps to: the canvas' sides and the left and
/// right edges of every other panel beside it (overlapping rows).
pub(super) fn x_lines(r: Rect, c: Canvas, others: &[Rect]) -> Vec<i32> {
    let mut v = vec![0, i32::from(c.w)];
    for o in others {
        if near_span(r.y, bottom(r), o.y, bottom(*o)) {
            v.extend([o.x, right(*o)]);
        }
    }
    v
}

/// The y lines a panel at `r` snaps to (as [`x_lines`], by columns).
pub(super) fn y_lines(r: Rect, c: Canvas, others: &[Rect]) -> Vec<i32> {
    let mut v = vec![0, i32::from(c.h)];
    for o in others {
        if near_span(r.x, right(r), o.x, right(*o)) {
            v.extend([o.y, bottom(*o)]);
        }
    }
    v
}

/// The shortest move (within [`SNAP`]) that puts one of `edges` on one of
/// `lines`; the first line wins a tie.
pub(super) fn snap_shift(edges: &[i32], lines: &[i32]) -> Option<i32> {
    let mut best: Option<i32> = None;
    for &e in edges {
        for &t in lines {
            let d = t - e;
            if d.abs() <= SNAP && best.is_none_or(|b| d.abs() < b.abs()) {
                best = Some(d);
            }
        }
    }
    best
}

/// `r` after a move: shifted so its nearest edge on each axis lies on a
/// canvas edge or another panel's edge when within [`SNAP`].
pub fn snap_move(r: Rect, c: Canvas, others: &[Rect]) -> Rect {
    let dx = snap_shift(&[r.x, right(r)], &x_lines(r, c, others)).unwrap_or(0);
    let dy = snap_shift(&[r.y, bottom(r)], &y_lines(r, c, others)).unwrap_or(0);
    Rect::new(r.x + dx, r.y + dy, r.w, r.h)
}

/// The canvas corner a rect is anchored to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub const ALL: [Corner; 4] = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomRight,
    ];

    /// The name the layout file uses.
    pub fn key(self) -> &'static str {
        match self {
            Corner::TopLeft => "top-left",
            Corner::TopRight => "top-right",
            Corner::BottomLeft => "bottom-left",
            Corner::BottomRight => "bottom-right",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == s)
    }

    fn left(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::BottomLeft)
    }

    fn top(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::TopRight)
    }
}

/// A rect as the layout stores it: its size and its distance from the
/// nearest canvas corner (`dx` from the left or right side, `dy` from the
/// top or bottom).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchored {
    pub corner: Corner,
    pub dx: i32,
    pub dy: i32,
    pub w: u16,
    pub h: u16,
}

impl Anchored {
    /// `r` anchored to the canvas corner nearest its centre (left and top
    /// win a tie).
    pub fn of(r: Rect, c: Canvas) -> Self {
        let left = 2 * r.x + i32::from(r.w) <= i32::from(c.w);
        let top = 2 * r.y + i32::from(r.h) <= i32::from(c.h);
        let corner = match (left, top) {
            (true, true) => Corner::TopLeft,
            (false, true) => Corner::TopRight,
            (true, false) => Corner::BottomLeft,
            (false, false) => Corner::BottomRight,
        };
        Self {
            corner,
            dx: if left { r.x } else { i32::from(c.w) - right(r) },
            dy: if top { r.y } else { i32::from(c.h) - bottom(r) },
            w: r.w,
            h: r.h,
        }
    }

    /// The rect on canvas `c`: the same distance from the same corner,
    /// clamped to the canvas.
    pub fn place(&self, c: Canvas) -> Rect {
        let x = if self.corner.left() {
            self.dx
        } else {
            i32::from(c.w) - self.dx - i32::from(self.w)
        };
        let y = if self.corner.top() {
            self.dy
        } else {
            i32::from(c.h) - self.dy - i32::from(self.h)
        };
        clamp(Rect::new(x, y, self.w, self.h), c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOBBY: Canvas = Canvas::new(800, 600);
    const GAME: Canvas = Canvas::new(1280, 720);

    #[test]
    fn clamp_keeps_the_rect_inside() {
        assert_eq!(
            clamp(Rect::new(-30, 590, 100, 50), LOBBY),
            Rect::new(0, 550, 100, 50)
        );
        // Larger than the canvas: shrunk to it.
        assert_eq!(
            clamp(Rect::new(10, 10, 900, 700), LOBBY),
            Rect::new(0, 0, 800, 600)
        );
    }

    #[test]
    fn anchors_survive_view_change() {
        // Each corner of the lobby: the same gaps to the same corner in the
        // game view.
        let cases = [
            (Rect::new(48, 100, 352, 150), Rect::new(48, 100, 352, 150)),
            // The lobby's chat pane: its centre is below the middle.
            (Rect::new(48, 179, 352, 326), Rect::new(48, 299, 352, 326)),
            (Rect::new(446, 312, 320, 192), Rect::new(926, 432, 320, 192)),
            (Rect::new(20, 400, 200, 150), Rect::new(20, 520, 200, 150)),
            (Rect::new(600, 30, 180, 100), Rect::new(1080, 30, 180, 100)),
        ];
        for (at_lobby, at_game) in cases {
            let a = Anchored::of(at_lobby, LOBBY);
            assert_eq!(a.place(GAME), at_game, "{a:?}");
            // And back.
            assert_eq!(Anchored::of(at_game, GAME).place(LOBBY), at_lobby);
        }
        assert_eq!(
            Anchored::of(Rect::new(446, 312, 320, 192), LOBBY).corner,
            Corner::BottomRight
        );
    }

    #[test]
    fn a_large_anchored_rect_is_clamped_on_a_small_canvas() {
        let a = Anchored::of(Rect::new(900, 500, 360, 200), GAME);
        assert_eq!(a.place(LOBBY), Rect::new(420, 380, 360, 200));
        let wide = Anchored::of(Rect::new(0, 0, 1000, 100), GAME);
        assert_eq!(wide.place(LOBBY), Rect::new(0, 0, 800, 100));
    }

    #[test]
    fn corner_keys_round_trip() {
        for c in Corner::ALL {
            assert_eq!(Corner::from_key(c.key()), Some(c));
        }
        assert_eq!(Corner::from_key("middle"), None);
    }

    #[test]
    fn snap_move_picks_the_nearest_line() {
        // 5 px from the left side, 7 px from the top: both snap.
        assert_eq!(
            snap_move(Rect::new(5, 7, 100, 100), LOBBY, &[]),
            Rect::new(0, 0, 100, 100)
        );
        // 9 px away: no snap.
        assert_eq!(
            snap_move(Rect::new(9, 9, 100, 100), LOBBY, &[]),
            Rect::new(9, 9, 100, 100)
        );
        // A panel far below doesn't pull on x.
        let other = Rect::new(300, 400, 100, 100);
        assert_eq!(
            snap_move(Rect::new(404, 50, 100, 100), LOBBY, &[other]),
            Rect::new(404, 50, 100, 100)
        );
    }
}
