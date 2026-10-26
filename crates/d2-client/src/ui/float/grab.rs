//! Grabbing a floating panel: the zones of a panel the pointer can take
//! (the title strip moves it, the edges and corners resize it) and the
//! rect a held grab gives for a pointer position: moved or resized, kept
//! at its minimum size, snapped and clamped to the canvas.

use crate::ui::geom::{Point, Rect};

use super::geom::{
    bottom, clamp, from_edges, right, snap_move, snap_shift, x_lines, y_lines, Canvas, MinSize,
};

/// Rows at the top of a panel that drag it (under the resize band).
pub const TITLE_H: i32 = 20;
/// The resize band along each edge, inside the panel.
pub const EDGE: i32 = 5;
/// Along an edge, this close to a corner resizes both edges of the corner.
pub const CORNER: i32 = 14;

/// The edges a resize moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl Edges {
    pub const fn any(self) -> bool {
        self.left || self.right || self.top || self.bottom
    }
}

/// What the pointer takes of a panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// The title strip: moves the panel.
    Title,
    /// An edge or a corner: resizes the panel.
    Resize(Edges),
}

/// The zone of panel `r` under `p`; `None` outside the panel and in its
/// body (the panel's own controls take those clicks). A panel that can't
/// be resized has only its title strip.
pub fn zone_at(r: Rect, p: Point, resizable: bool) -> Option<Zone> {
    if !r.contains(p) {
        return None;
    }
    if resizable {
        let from_l = p.x - r.x;
        let from_r = right(r) - 1 - p.x;
        let from_t = p.y - r.y;
        let from_b = bottom(r) - 1 - p.y;
        let side = from_l < EDGE || from_r < EDGE;
        let end = from_t < EDGE || from_b < EDGE;
        let mut e = Edges {
            left: from_l < EDGE || (end && from_l < CORNER),
            right: from_r < EDGE || (end && from_r < CORNER),
            top: from_t < EDGE || (side && from_t < CORNER),
            bottom: from_b < EDGE || (side && from_b < CORNER),
        };
        // A panel narrower than two bands: the nearer edge only.
        if e.left && e.right {
            e.right = from_r < from_l;
            e.left = !e.right;
        }
        if e.top && e.bottom {
            e.bottom = from_b < from_t;
            e.top = !e.bottom;
        }
        if e.any() {
            return Some(Zone::Resize(e));
        }
    }
    (p.y - r.y < TITLE_H).then_some(Zone::Title)
}

/// A held grab: the zone taken, where the pointer went down and the
/// panel's rect then.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grab {
    pub zone: Zone,
    pub start: Point,
    pub from: Rect,
}

impl Grab {
    pub fn new(zone: Zone, start: Point, from: Rect) -> Self {
        Self { zone, start, from }
    }

    /// The panel's rect with the pointer at `p`: moved or resized by the
    /// pointer's travel, snapped to the canvas edges and `others` (the
    /// other panels), inside the canvas and never under `min`.
    pub fn rect_at(&self, p: Point, min: MinSize, c: Canvas, others: &[Rect]) -> Rect {
        // A click without a move leaves the panel where it is.
        if p == self.start {
            return self.from;
        }
        let (dx, dy) = (p.x - self.start.x, p.y - self.start.y);
        match self.zone {
            Zone::Title => {
                let moved = Rect::new(self.from.x + dx, self.from.y + dy, self.from.w, self.from.h);
                clamp(snap_move(clamp(moved, c), c, others), c)
            }
            Zone::Resize(e) => resize(self.from, e, (dx, dy), min, c, others),
        }
    }
}

/// `from` with edges `e` moved by `(dx, dy)`: each moved edge stays on the
/// canvas and at least `min` from the fixed edge, then snaps to a line
/// within reach that keeps the minimum.
fn resize(
    from: Rect,
    e: Edges,
    (dx, dy): (i32, i32),
    min: MinSize,
    c: Canvas,
    others: &[Rect],
) -> Rect {
    let m = min.within(c);
    let (mw, mh) = (i32::from(m.w), i32::from(m.h));
    let (cw, ch) = (i32::from(c.w), i32::from(c.h));
    let (mut l, mut t, mut r, mut b) = (from.x, from.y, right(from), bottom(from));
    if e.left {
        l = (l + dx).min(r - mw).max(0);
    }
    if e.right {
        r = (r + dx).max(l + mw).min(cw);
    }
    if e.top {
        t = (t + dy).min(b - mh).max(0);
    }
    if e.bottom {
        b = (b + dy).max(t + mh).min(ch);
    }
    let now = from_edges(l, t, r, b);
    let xs = x_lines(now, c, others);
    let ys = y_lines(now, c, others);
    if e.left {
        if let Some(d) = snap_shift(&[l], &xs).filter(|d| r - (l + d) >= mw && l + d >= 0) {
            l += d;
        }
    }
    if e.right {
        if let Some(d) = snap_shift(&[r], &xs).filter(|d| r + d - l >= mw && r + d <= cw) {
            r += d;
        }
    }
    if e.top {
        if let Some(d) = snap_shift(&[t], &ys).filter(|d| b - (t + d) >= mh && t + d >= 0) {
            t += d;
        }
    }
    if e.bottom {
        if let Some(d) = snap_shift(&[b], &ys).filter(|d| b + d - t >= mh && b + d <= ch) {
            b += d;
        }
    }
    // A rect that was under its minimum (a layout from another canvas)
    // grows away from the moved edge.
    if r - l < mw {
        if e.left {
            l = (r - mw).max(0);
        }
        r = (l + mw).min(cw);
    }
    if b - t < mh {
        if e.top {
            t = (b - mh).max(0);
        }
        b = (t + mh).min(ch);
    }
    from_edges(l, t, r, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: Canvas = Canvas::new(800, 600);
    const MIN: MinSize = MinSize::new(160, 120);
    const PANEL: Rect = Rect::new(200, 150, 300, 200);

    fn resize_zone(left: bool, right: bool, top: bool, bottom: bool) -> Zone {
        Zone::Resize(Edges {
            left,
            right,
            top,
            bottom,
        })
    }

    #[test]
    fn zones_of_a_panel() {
        let at = |x, y| zone_at(PANEL, Point::new(x, y), true);
        assert_eq!(at(199, 160), None);
        assert_eq!(at(300, 160), Some(Zone::Title));
        assert_eq!(at(300, 200), None);
        assert_eq!(at(200, 250), Some(resize_zone(true, false, false, false)));
        assert_eq!(at(499, 250), Some(resize_zone(false, true, false, false)));
        assert_eq!(at(300, 150), Some(resize_zone(false, false, true, false)));
        assert_eq!(at(300, 349), Some(resize_zone(false, false, false, true)));
        // Within CORNER of a corner along an edge: both edges.
        assert_eq!(at(210, 349), Some(resize_zone(true, false, false, true)));
        assert_eq!(at(499, 160), Some(resize_zone(false, true, true, false)));
        // Not resizable: only the title strip.
        assert_eq!(
            zone_at(PANEL, Point::new(200, 160), false),
            Some(Zone::Title)
        );
        assert_eq!(zone_at(PANEL, Point::new(200, 250), false), None);
    }

    #[test]
    fn drag_clamps() {
        let g = Grab::new(Zone::Title, Point::new(300, 155), PANEL);
        // Far past the top-left corner.
        assert_eq!(
            g.rect_at(Point::new(-500, -500), MIN, C, &[]),
            Rect::new(0, 0, 300, 200)
        );
        // Far past the bottom-right corner.
        assert_eq!(
            g.rect_at(Point::new(5000, 5000), MIN, C, &[]),
            Rect::new(500, 400, 300, 200)
        );
        // An ordinary move.
        assert_eq!(
            g.rect_at(Point::new(330, 175), MIN, C, &[]),
            Rect::new(230, 170, 300, 200)
        );
    }

    #[test]
    fn resize_keeps_minimum() {
        // The bottom-right corner pulled far up-left: the minimum, top-left
        // fixed.
        let g = Grab::new(
            resize_zone(false, true, false, true),
            Point::new(499, 349),
            PANEL,
        );
        assert_eq!(
            g.rect_at(Point::new(0, 0), MIN, C, &[]),
            Rect::new(200, 150, 160, 120)
        );
        // The top-left corner pulled far down-right: the minimum,
        // bottom-right fixed.
        let g = Grab::new(
            resize_zone(true, false, true, false),
            Point::new(200, 150),
            PANEL,
        );
        assert_eq!(
            g.rect_at(Point::new(800, 600), MIN, C, &[]),
            Rect::new(340, 230, 160, 120)
        );
        // Grown past the canvas: stops at its edges.
        let g = Grab::new(
            resize_zone(true, false, false, false),
            Point::new(200, 250),
            PANEL,
        );
        assert_eq!(
            g.rect_at(Point::new(-400, 250), MIN, C, &[]).x,
            0,
            "the left edge stops at the canvas"
        );
        // A rect under its minimum grows to it on the first resize.
        let small = Rect::new(100, 100, 80, 60);
        let g = Grab::new(
            resize_zone(false, true, false, false),
            Point::new(179, 120),
            small,
        );
        assert_eq!(
            g.rect_at(Point::new(180, 120), MIN, C, &[]),
            Rect::new(100, 100, 160, 120)
        );
    }

    #[test]
    fn snaps_to_edges_and_panels() {
        let other = Rect::new(520, 150, 200, 200);
        // Moved to 6 px left of the other panel: its right edge snaps to
        // the other's left edge.
        let p = Rect::new(100, 150, 300, 200);
        let g = Grab::new(Zone::Title, Point::new(200, 160), p);
        assert_eq!(
            g.rect_at(Point::new(314, 163), MIN, C, &[other]),
            Rect::new(220, 150, 300, 200),
            "right edge on the other's left edge, top aligned with its top"
        );
        // Near the canvas' bottom-left: snaps into the corner.
        assert_eq!(
            g.rect_at(Point::new(105, 553), MIN, C, &[other]),
            Rect::new(0, 400, 300, 200)
        );
        // A resize snaps the moved edge to the other panel.
        let g = Grab::new(
            resize_zone(false, true, false, false),
            Point::new(399, 250),
            p,
        );
        assert_eq!(
            g.rect_at(Point::new(514, 250), MIN, C, &[other]),
            Rect::new(100, 150, 420, 200)
        );
        // And to the canvas' edge.
        let g = Grab::new(
            resize_zone(false, false, false, true),
            Point::new(200, 349),
            p,
        );
        assert_eq!(
            g.rect_at(Point::new(200, 594), MIN, C, &[]),
            Rect::new(100, 150, 300, 450)
        );
    }
}
