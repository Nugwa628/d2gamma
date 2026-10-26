//! The gold Celtic frame of floating panels, as the lobby art frames its
//! panes: a 2 px gold rope around the panel and, on a chat frame, the
//! knotwork band down its left side between two ropes.
//!
//! The pieces are cut at run time from the lobby background the game loads
//! (`FrontEnd\realmbckg`, 800 × 600, [`BCKG_PIECES`]): the rope of the right
//! pane's frame and one 62-row repeat of the chat pane's knot band. Nothing
//! of that art is stored in D2RUST. A background without those frames (a
//! mod's own art) gets D2RUST's own pieces in the same layout instead
//! ([`own_art`]), made of the palette's gold.
//!
//! A frame of any size is the four corners, the ropes tiled along the
//! edges and, for [`Style::Knot`], the band tiled down the left side; the
//! last tile of a run is cut short. [`CelticArt::blits`] gives those copies
//! for a host that draws from the source image; [`CelticArt::render`] draws
//! them into a frame-sized index image (0 transparent).

use crate::ui::geom::{Point, Rect};

use super::geom::MinSize;

/// A palette-index image, top row first; 0 is transparent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexImage {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl IndexImage {
    /// A transparent `w × h` image.
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            px: vec![0; (w * h) as usize],
        }
    }

    /// A `w × h` image of tiles laid out left to right, a new row of tiles
    /// when a row is `w` wide (a background cut in 256 × 256 frames: the
    /// lobby's 4 × 3). A tile is (width, height, its indices top row first).
    pub fn from_tiles(tiles: &[(u32, u32, &[u8])], w: u32, h: u32) -> Self {
        let mut img = Self::new(w, h);
        let (mut x, mut y, mut row_h) = (0u32, 0u32, 0u32);
        for &(tw, th, px) in tiles {
            for r in 0..th.min(h.saturating_sub(y)) {
                for c in 0..tw.min(w.saturating_sub(x)) {
                    if let Some(&v) = px.get((r * tw + c) as usize) {
                        img.px[((y + r) * w + x + c) as usize] = v;
                    }
                }
            }
            x += tw;
            row_h = row_h.max(th);
            if x >= w {
                x = 0;
                y += row_h;
                row_h = 0;
            }
        }
        img
    }

    /// The index at (x, y); 0 outside.
    pub fn get(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x as u32 >= self.w || y as u32 >= self.h {
            return 0;
        }
        self.px[(y as u32 * self.w + x as u32) as usize]
    }

    fn set(&mut self, x: i32, y: i32, v: u8) {
        if x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h {
            self.px[(y as u32 * self.w + x as u32) as usize] = v;
        }
    }

    /// Copies the opaque pixels of `src`'s rect `b.src` to `b.at`.
    pub fn blit(&mut self, src: &IndexImage, b: &Blit) {
        for r in 0..i32::from(b.src.h) {
            for c in 0..i32::from(b.src.w) {
                let v = src.get(b.src.x + c, b.src.y + r);
                if v != 0 {
                    self.set(b.at.x + c, b.at.y + r, v);
                }
            }
        }
    }
}

/// One copy of a frame: the source rect and where its top-left goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blit {
    pub src: Rect,
    pub at: Point,
}

/// The frame's look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// The rope all round (the lobby's right pane).
    Rope,
    /// The rope with the knot band down the left side (the chat pane).
    Knot,
}

/// The rope's thickness.
pub const ROPE: i32 = 2;
/// The knot band's width, its two ropes included.
pub const KNOT_W: i32 = 18;

/// Where the pieces are in a source image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pieces {
    /// The ropes: horizontal ones [`ROPE`] high, vertical ones [`ROPE`]
    /// wide, each tiled along its edge.
    pub top: Rect,
    pub bottom: Rect,
    pub left: Rect,
    pub right: Rect,
    /// [`ROPE`] × [`ROPE`]: top-left, top-right, bottom-left, bottom-right.
    pub corners: [Rect; 4],
    /// One repeat of the knot band, [`KNOT_W`] wide, tiled down.
    pub knot: Rect,
}

/// The pieces in the lobby background (800 × 600): the right pane's frame
/// is the rope (outer rect 446, 312 to 765, 503); the chat pane's left
/// side (columns 48–65) is the knot band, repeating every 62 rows.
pub const BCKG_PIECES: Pieces = Pieces {
    top: Rect::new(448, 312, 316, 2),
    bottom: Rect::new(448, 502, 316, 2),
    left: Rect::new(446, 314, 2, 188),
    right: Rect::new(764, 314, 2, 188),
    corners: [
        Rect::new(446, 312, 2, 2),
        Rect::new(764, 312, 2, 2),
        Rect::new(446, 502, 2, 2),
        Rect::new(764, 502, 2, 2),
    ],
    knot: Rect::new(48, 246, 18, 62),
};

/// The size of the lobby background.
pub const BCKG_SIZE: (u32, u32) = (800, 600);

/// The smallest frame of `style`: the corners and, with the band, its
/// width.
pub fn min_size(style: Style) -> MinSize {
    match style {
        Style::Rope => MinSize::new(2 * ROPE as u16, 2 * ROPE as u16),
        Style::Knot => MinSize::new((KNOT_W + ROPE) as u16, 2 * ROPE as u16),
    }
}

/// The area inside the frame of `r` (the panel's body).
pub fn inner(r: Rect, style: Style) -> Rect {
    let left = match style {
        Style::Rope => ROPE,
        Style::Knot => KNOT_W,
    };
    let w = (i32::from(r.w) - left - ROPE).max(0) as u16;
    let h = (i32::from(r.h) - 2 * ROPE).max(0) as u16;
    Rect::new(r.x + left, r.y + ROPE, w, h)
}

/// Tiles `src` along a run from `from` to `to` (x when `across`, else y),
/// at `fixed` on the other axis; the last tile is cut short.
fn tile(out: &mut Vec<Blit>, src: Rect, across: bool, (from, to): (i32, i32), fixed: i32) {
    let step = i32::from(if across { src.w } else { src.h });
    if step == 0 {
        return;
    }
    let mut at = from;
    while at < to {
        let n = step.min(to - at) as u16;
        let (s, p) = if across {
            (Rect::new(src.x, src.y, n, src.h), Point::new(at, fixed))
        } else {
            (Rect::new(src.x, src.y, src.w, n), Point::new(fixed, at))
        };
        out.push(Blit { src: s, at: p });
        at += step;
    }
}

/// The copies that draw the frame of `r` from `p`, in draw order: edges,
/// then corners. A rect under [`min_size`] has no frame.
pub fn frame_blits(r: Rect, style: Style, p: &Pieces) -> Vec<Blit> {
    let min = min_size(style);
    if r.w < min.w || r.h < min.h {
        return Vec::new();
    }
    let (x, y) = (r.x, r.y);
    let (x1, y1) = (r.x + i32::from(r.w), r.y + i32::from(r.h));
    let mut out = Vec::new();
    tile(&mut out, p.top, true, (x + ROPE, x1 - ROPE), y);
    tile(&mut out, p.bottom, true, (x + ROPE, x1 - ROPE), y1 - ROPE);
    match style {
        Style::Rope => tile(&mut out, p.left, false, (y + ROPE, y1 - ROPE), x),
        Style::Knot => tile(&mut out, p.knot, false, (y + ROPE, y1 - ROPE), x),
    }
    tile(&mut out, p.right, false, (y + ROPE, y1 - ROPE), x1 - ROPE);
    let at = [
        Point::new(x, y),
        Point::new(x1 - ROPE, y),
        Point::new(x, y1 - ROPE),
        Point::new(x1 - ROPE, y1 - ROPE),
    ];
    for (src, at) in p.corners.into_iter().zip(at) {
        out.push(Blit { src, at });
    }
    out
}

/// The palette's golds the own pieces are made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gold {
    pub light: u8,
    pub mid: u8,
    pub dark: u8,
}

impl Gold {
    /// The palette indices nearest the rope's light, middle and dark gold
    /// (index 0, transparent, is never picked).
    pub fn from_palette(pal: &[[u8; 3]; 256]) -> Self {
        let nearest = |rgb: [i32; 3]| -> u8 {
            (1..=255u8)
                .min_by_key(|&i| {
                    let c = pal[usize::from(i)];
                    (0..3)
                        .map(|k| (i32::from(c[k]) - rgb[k]).pow(2))
                        .sum::<i32>()
                })
                .unwrap_or(1)
        };
        Self {
            light: nearest([236, 212, 140]),
            mid: nearest([180, 144, 76]),
            dark: nearest([96, 72, 36]),
        }
    }
}

/// D2RUST's own pieces in the lobby frames' style: a twisted 2 px rope of
/// three golds and a knot band of two strands crossing every 16 rows
/// between ropes. The image (40 × 32) and where its pieces are.
pub fn own_art(g: Gold) -> (IndexImage, Pieces) {
    let mut img = IndexImage::new(40, 32);
    // The rope: a twist every 4 px, the two rows offset by half a twist.
    let twist = [g.light, g.light, g.mid, g.dark];
    let rope = |i: i32, row: i32| twist[((i + 2 * row) & 3) as usize];
    for i in 0..32 {
        for k in 0..ROPE {
            // Horizontal at rows 0–1 (columns 0–31), vertical at columns
            // 32–33 (rows 0–31).
            img.set(i, k, rope(i, k));
            img.set(32 + k, i, rope(i, k));
        }
    }
    // Corners: a light knot at 36, 0.
    for (c, r) in [(36, 0), (37, 0), (36, 1), (37, 1)] {
        img.set(c, r, g.light);
    }
    // The knot band: 18 × 16 at 0, 16, a rope on each side.
    let band_y = 16;
    for r in 0..16 {
        for k in 0..ROPE {
            img.set(k, band_y + r, rope(r, k));
            img.set(KNOT_W - ROPE + k, band_y + r, rope(r, k));
        }
        // Two strands zigzag across columns 4–13, crossing mid-band; the
        // one going right passes over.
        let t = if r < 8 { r } else { 15 - r };
        let a = 4 + t + t / 4;
        let b = 13 - t - t / 4;
        for (col, over) in [(b, false), (b + 1, false), (a, true), (a + 1, true)] {
            img.set(col, band_y + r, if over { g.light } else { g.mid });
        }
    }
    let pieces = Pieces {
        top: Rect::new(0, 0, 32, 2),
        bottom: Rect::new(0, 0, 32, 2),
        left: Rect::new(32, 0, 2, 32),
        right: Rect::new(32, 0, 2, 32),
        corners: [Rect::new(36, 0, 2, 2); 4],
        knot: Rect::new(0, band_y, KNOT_W as u16, 16),
    };
    (img, pieces)
}

/// The frame art in use: a source image and its pieces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CelticArt {
    pub src: IndexImage,
    pub pieces: Pieces,
    /// Made of D2RUST's own pieces (no usable lobby background).
    pub own: bool,
}

impl CelticArt {
    /// The pieces of the lobby background `bckg` (see [`IndexImage::from_tiles`])
    /// when its frames are where [`BCKG_PIECES`] says, else D2RUST's own
    /// ([`own_art`]); `pal` is the background's palette.
    pub fn new(bckg: Option<IndexImage>, pal: &[[u8; 3]; 256]) -> Self {
        match bckg.filter(|b| has_frames(b, pal)) {
            Some(src) => Self {
                src,
                pieces: BCKG_PIECES,
                own: false,
            },
            None => {
                let (src, pieces) = own_art(Gold::from_palette(pal));
                Self {
                    src,
                    pieces,
                    own: true,
                }
            }
        }
    }

    /// The copies that draw the frame of `r` (see [`frame_blits`]).
    pub fn blits(&self, r: Rect, style: Style) -> Vec<Blit> {
        frame_blits(r, style, &self.pieces)
    }

    /// The frame of a `w × h` panel as an image of that size.
    pub fn render(&self, w: u16, h: u16, style: Style) -> IndexImage {
        let mut img = IndexImage::new(u32::from(w), u32::from(h));
        for b in self.blits(Rect::new(0, 0, w, h), style) {
            img.blit(&self.src, &b);
        }
        img
    }
}

/// `bckg` is the lobby background with its gold frames at
/// [`BCKG_PIECES`]: its size, and nine in ten pixels of every rope piece
/// and of the band's ropes bright (the panes inside are near black).
pub fn has_frames(bckg: &IndexImage, pal: &[[u8; 3]; 256]) -> bool {
    if (bckg.w, bckg.h) != BCKG_SIZE {
        return false;
    }
    let bright = |v: u8| {
        let c = pal[usize::from(v)];
        v != 0 && u32::from(c[0]) + u32::from(c[1]) + u32::from(c[2]) >= 60
    };
    let p = &BCKG_PIECES;
    let band_ropes = [
        Rect::new(p.knot.x, p.knot.y, ROPE as u16, p.knot.h),
        Rect::new(p.knot.x + KNOT_W - ROPE, p.knot.y, ROPE as u16, p.knot.h),
    ];
    let mut rects = vec![p.top, p.bottom, p.left, p.right];
    rects.extend(p.corners);
    rects.extend(band_ropes);
    rects.iter().all(|r| {
        let mut lit = 0;
        let mut all = 0;
        for y in r.y..r.y + i32::from(r.h) {
            for x in r.x..r.x + i32::from(r.w) {
                all += 1;
                lit += i32::from(bright(bckg.get(x, y)));
            }
        }
        all > 0 && lit * 10 >= all * 9
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grey ramp: index i is (i, i, i).
    fn ramp() -> [[u8; 3]; 256] {
        std::array::from_fn(|i| [i as u8; 3])
    }

    fn own() -> CelticArt {
        CelticArt::new(None, &ramp())
    }

    /// A background with bright pieces at [`BCKG_PIECES`] on a dark scene.
    fn fake_bckg() -> IndexImage {
        let mut b = IndexImage::new(800, 600);
        b.px.fill(4);
        let p = BCKG_PIECES;
        let mut rects = vec![p.top, p.bottom, p.left, p.right, p.knot];
        rects.extend(p.corners);
        for r in rects {
            for y in r.y..r.y + i32::from(r.h) {
                for x in r.x..r.x + i32::from(r.w) {
                    b.set(x, y, 200);
                }
            }
        }
        b
    }

    #[test]
    fn the_pieces_lie_inside_the_background() {
        let p = BCKG_PIECES;
        let mut rects = vec![p.top, p.bottom, p.left, p.right, p.knot];
        rects.extend(p.corners);
        for r in rects {
            assert!(r.x >= 0 && r.y >= 0, "{r:?}");
            assert!(r.right() <= 800 && r.bottom() <= 600, "{r:?}");
        }
        assert_eq!(i32::from(p.knot.w), KNOT_W);
        assert!(p.corners.iter().all(|c| (c.w, c.h) == (2, 2)));
    }

    #[test]
    fn the_background_is_used_only_with_its_frames() {
        let art = CelticArt::new(Some(fake_bckg()), &ramp());
        assert!(!art.own);
        assert_eq!(art.pieces, BCKG_PIECES);
        // A background without frames there, or of another size: own art.
        assert!(CelticArt::new(Some(IndexImage::new(800, 600)), &ramp()).own);
        assert!(CelticArt::new(Some(IndexImage::new(640, 480)), &ramp()).own);
        assert!(own().own);
    }

    #[test]
    fn tiles_lay_out_like_the_lobby_background() {
        let a = vec![1u8; 256 * 256];
        let b = vec![2u8; 32 * 256];
        let c = vec![3u8; 256 * 88];
        let d = vec![4u8; 32 * 88];
        let mut tiles: Vec<(u32, u32, &[u8])> = Vec::new();
        for row in 0..3 {
            for col in 0..4 {
                tiles.push(match (row == 2, col == 3) {
                    (false, false) => (256, 256, &a),
                    (false, true) => (32, 256, &b),
                    (true, false) => (256, 88, &c),
                    (true, true) => (32, 88, &d),
                });
            }
        }
        let img = IndexImage::from_tiles(&tiles, 800, 600);
        assert_eq!(img.get(0, 0), 1);
        assert_eq!(img.get(767, 511), 1);
        assert_eq!(img.get(768, 511), 2);
        assert_eq!(img.get(767, 512), 3);
        assert_eq!(img.get(799, 599), 4);
    }

    /// Every pixel of the frame's outer ring is drawn, the body is not, at
    /// sizes around the pieces' lengths, in both styles and both arts.
    #[test]
    fn the_frame_covers_its_ring_at_any_size() {
        for art in [own(), CelticArt::new(Some(fake_bckg()), &ramp())] {
            for style in [Style::Rope, Style::Knot] {
                for (w, h) in [(20, 4), (33, 17), (317, 190), (640, 189), (700, 500)] {
                    let img = art.render(w, h, style);
                    let body = inner(Rect::new(0, 0, w, h), style);
                    for y in 0..i32::from(h) {
                        for x in 0..i32::from(w) {
                            let ring = x < ROPE
                                || y < ROPE
                                || x >= i32::from(w) - ROPE
                                || y >= i32::from(h) - ROPE;
                            if ring {
                                assert_ne!(img.get(x, y), 0, "{style:?} {w}x{h} at {x},{y}");
                            }
                            if body.contains(Point::new(x, y)) {
                                assert_eq!(img.get(x, y), 0, "{style:?} {w}x{h} at {x},{y}");
                            }
                        }
                    }
                    for b in art.blits(Rect::new(10, 20, w, h), style) {
                        assert!(b.at.x >= 10 && b.at.y >= 20);
                        assert!(b.at.x + i32::from(b.src.w) <= 10 + i32::from(w));
                        assert!(b.at.y + i32::from(b.src.h) <= 20 + i32::from(h));
                    }
                }
            }
        }
    }

    #[test]
    fn the_knot_band_repeats_down_the_left_side() {
        let mut bckg = fake_bckg();
        // A marked knot repeat: each row its own index.
        let k = BCKG_PIECES.knot;
        for r in 0..i32::from(k.h) {
            for c in 0..KNOT_W {
                bckg.set(k.x + c, k.y + r, 100 + r as u8);
            }
        }
        let art = CelticArt::new(Some(bckg), &ramp());
        assert!(!art.own);
        let h = 2 * ROPE as u16 + 62 * 2 + 10;
        let img = art.render(120, h, Style::Knot);
        for y in 0..62 * 2 + 10 {
            let want = 100 + (y % 62) as u8;
            assert_eq!(img.get(5, ROPE + y), want, "row {y}");
        }
        // The body starts right of the band.
        assert_eq!(inner(Rect::new(0, 0, 120, h), Style::Knot).x, KNOT_W);
    }

    #[test]
    fn under_the_minimum_there_is_no_frame() {
        assert!(frame_blits(Rect::new(0, 0, 3, 40), Style::Rope, &BCKG_PIECES).is_empty());
        assert!(frame_blits(Rect::new(0, 0, 19, 40), Style::Knot, &BCKG_PIECES).is_empty());
        assert!(!frame_blits(Rect::new(0, 0, 20, 4), Style::Knot, &BCKG_PIECES).is_empty());
    }

    #[test]
    fn gold_is_picked_from_the_palette() {
        let g = Gold::from_palette(&ramp());
        assert!(g.light > g.mid && g.mid > g.dark && g.dark > 0);
    }
}
