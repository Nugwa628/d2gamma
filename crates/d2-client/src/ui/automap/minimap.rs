// D2RUST's own: the minimap widget's picture and place (Display v3 H10).
//! The minimap: the current automap layer around the local player, always
//! on in the top-right corner of the world (`[hud] minimap`,
//! `minimap_size`, `minimap_opacity`). This module is its plain-Rust half:
//! where the widget sits in the 1280 x 720 view, how many output pixels a
//! map pixel takes, and the picture itself, drawn from the automap's cells
//! and unit markers into straight RGBA. `ui::settings::minimap` shows the
//! picture over the window at its physical resolution.
//!
//! Rules:
//! - The map is the full-size automap (the same cels, cell places and
//!   marker crosses the Tab map draws), centred on the local player's
//!   marker, so it reads exactly like the automap, only smaller.
//! - Each map pixel is a whole number of output pixels ([`zoom`]): 2 at
//!   2160p, 1 at 1440p and 1080p, so the lines stay sharp at every size.
//! - It sits in the top-right corner of the world the side panels leave
//!   visible ([`place`]): an open right panel moves it left of the panel
//!   (as wide as UI Scale draws it); with both panels open there is no
//!   corner left and it hides.
//! - It never covers the game's own UI: any UI the frame draws over its
//!   box (a tooltip, a skill list, a menu) hides it ([`UiCover`]).

use d2_formats::palette::Palette;

use super::cells::{LayerCells, TreeKind};
use super::draw::AutomapDraw;
use super::markers::{unit_markers, MarkerCtx, MarkerPalette, MarkerUnit};
use super::options::CelFile;
use super::town::TownKind;
use super::view::{Bounds, View, DIV_FULL};
use crate::frames::IndexFrame;
use crate::rules::blend::gdi_line_pixels;
use crate::rules::camera::{ClientPos, FrameSize, OpenMode};
use crate::rules::placement::draw_position;
use crate::ui::ui_space::UiSpace;

/// The widget's side for each `[hud] minimap_size` (small, medium,
/// large), in view units (pixels of the 1280 x 720 view).
pub const SIZES: [i32; 3] = [180, 240, 300];
/// The size names, in `minimap_size` order.
pub const SIZE_NAMES: [&str; 3] = ["Small", "Medium", "Large"];
/// The gap between the widget and the edges of the visible world, view
/// units.
pub const MARGIN: i32 = 10;
/// Output pixels per view unit that give one more output pixel per map
/// pixel ([`zoom`]).
const ZOOM_STEP: f32 = 1.5;
/// How far past the picture a cel may start and still reach into it, map
/// pixels (the largest town art cels are 180 x 170).
const CEL_REACH: i32 = 192;

/// The widget's box in view units: its top-left corner and its side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Place {
    pub left: i32,
    pub top: i32,
    pub side: i32,
}

impl Place {
    pub fn right(&self) -> i32 {
        self.left + self.side
    }

    pub fn bottom(&self) -> i32 {
        self.top + self.side
    }
}

/// The widget's side in view units for `[hud] minimap_size` `size`
/// (out of range: the largest).
pub fn side(size: u8) -> i32 {
    SIZES[usize::from(size).min(SIZES.len() - 1)]
}

/// Where the widget sits in the frame of `space` with the side panels of
/// `mode` open: [`MARGIN`] in from the top-right corner of the world the
/// panels leave visible, above the control panel. The panels and the
/// control panel are laid out on the UI frame (UI Scale draws them larger
/// or smaller), so their edges are mapped to the frame. `None` when that
/// world has no room for it (both panels open, or a frame too small).
pub fn place(space: UiSpace, mode: OpenMode, size: u8) -> Option<Place> {
    let side = side(size);
    let frame = space.frame;
    let span = space.x_to_frame(space.ui.side_panel_span());
    let (world_left, world_right) = match mode.get() {
        1 => (0, frame.width - span),
        2 => (span, frame.width),
        3 => return None,
        _ => (0, frame.width),
    };
    let p = Place {
        left: world_right - MARGIN - side,
        top: MARGIN,
        side,
    };
    let play_bottom = space.y_to_frame(space.ui.play_height());
    (p.left >= world_left + MARGIN && p.bottom() <= play_bottom - MARGIN).then_some(p)
}

/// Output pixels per map pixel at `scale` output pixels per view unit: the
/// nearest whole number to `scale / 1.5`, at least 1 (2 at 2160p, 1 at
/// 1440p and 1080p).
pub fn zoom(scale: f32) -> u32 {
    let n = (scale / ZOOM_STEP).round();
    if n.is_finite() && n >= 1.0 {
        n as u32
    } else {
        1
    }
}

/// The picture's size in map pixels for a box `side_px` output pixels
/// wide at `zoom` output pixels per map pixel: enough map pixels to fill
/// the box (the last one may be cut).
pub fn picture_side(side_px: u32, zoom: u32) -> u32 {
    side_px.div_ceil(zoom.max(1)).max(1)
}

/// The drawing origin (Ax, Ay) that puts the local player's marker at the
/// middle of a `w` x `h` picture: the full automap's marker place
/// (`markers` `screen`: client position / 10, then 8 right and 8 up) minus
/// half the picture.
pub fn origin(player: ClientPos, w: u32, h: u32) -> (i32, i32) {
    (
        player.x / DIV_FULL + 8 - (w / 2) as i32,
        player.y / DIV_FULL - 8 - (h / 2) as i32,
    )
}

/// The automap cels the picture draws, by file and frame number.
pub trait MinimapCels {
    fn frame(&self, file: CelFile, cel: i16) -> Option<&IndexFrame>;
}

/// A drawn picture: straight RGBA8, top row first; a pixel no cel or
/// marker drew is transparent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Picture {
    fn new(width: u32, height: u32) -> Self {
        Picture {
            width,
            height,
            rgba: vec![0; width as usize * height as usize * 4],
        }
    }

    fn put(&mut self, x: i32, y: i32, rgb: [u8; 3]) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.rgba[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }

    /// The pixel at (x, y).
    pub fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    fn cel(&mut self, frame: &IndexFrame, x: i32, y: i32, palette: &Palette) {
        let (left, top) = draw_position(frame, x, y);
        let w = frame.width as usize;
        for (row, line) in frame.pixels.chunks(w.max(1)).enumerate() {
            for (col, &index) in line.iter().enumerate() {
                if index != 0 {
                    let c = palette.colors[usize::from(index)];
                    self.put(left + col as i32, top + row as i32, [c.r, c.g, c.b]);
                }
            }
        }
    }
}

/// What one picture is drawn from.
pub struct PictureInput<'a> {
    pub cells: &'a LayerCells,
    pub town: TownKind,
    /// The local player's client position (the picture's middle).
    pub player: ClientPos,
    /// The near rooms' marker units ([`MarkerUnit`]), the local player
    /// among them.
    pub markers: &'a [MarkerUnit],
    pub palette: &'a Palette,
    pub cels: &'a dyn MinimapCels,
    pub width: u32,
    pub height: u32,
}

/// The cel file each tree draws from: the town tree reads its town's own
/// file, the others the automap file.
fn tree_file(tree: TreeKind, town: TownKind) -> CelFile {
    match tree {
        TreeKind::Town => CelFile::of_town(town),
        _ => CelFile::MaxiMap,
    }
}

/// The picture: floors, walls, units and town art in the automap's draw
/// order, then the unit markers' crosses, all opaque in the act palette.
pub fn picture(p: &PictureInput) -> Picture {
    let mut out = Picture::new(p.width, p.height);
    let a = origin(p.player, p.width, p.height);
    let view = full_view();
    let window = Bounds {
        left: -CEL_REACH,
        top: -CEL_REACH,
        right: p.width as i32 + CEL_REACH,
        bottom: p.height as i32 + CEL_REACH,
    };
    for tree in TreeKind::ALL {
        let file = tree_file(tree, p.town);
        for (cel, x, y) in super::draw::walk(p.cells.tree(tree), &view, a, &window) {
            if let Some(frame) = p.cels.frame(file, cel) {
                out.cel(frame, x, y, p.palette);
            }
        }
    }
    for d in marker_lines(p, a) {
        let AutomapDraw::Line { from, to, color } = d else {
            continue;
        };
        let c = p.palette.colors[usize::from(color)];
        if let Ok(pixels) = gdi_line_pixels(from.0, from.1, to.0, to.1) {
            for (x, y) in pixels {
                out.put(x, y, [c.r, c.g, c.b]);
            }
        }
    }
    out
}

/// The full-size view (divisor 10) the automap's own cell walk reads.
fn full_view() -> View {
    let mut v = View::new(&super::options::Options::default());
    v.mini = false;
    v.div = DIV_FULL;
    v
}

/// The marker crosses of the picture, coloured as the automap colours them
/// (no names: the picture has no text).
fn marker_lines(p: &PictureInput, a: (i32, i32)) -> Vec<AutomapDraw> {
    let palette = MarkerPalette::new(|r, g, b| {
        crate::rules::shading::nearest(p.palette, r.into(), g.into(), b.into())
    });
    let ctx = MarkerCtx {
        local_party: -1,
        party: true,
        names: false,
        player_gate: false,
        mini: false,
        div: DIV_FULL,
        a,
        rect: Bounds {
            left: -16,
            top: -16,
            right: p.width as i32 + 16,
            bottom: p.height as i32 + 16,
        },
        palette,
    };
    let mut out = Vec::new();
    unit_markers(p.markers, &ctx, &mut out);
    out
}

/// Which parts of the view the game's UI drew over this frame, on a grid of
/// [`UiCover::CELL`]-unit squares: a square is set when any UI item (a
/// panel, the control panel, a tooltip, text, a menu) reaches into it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UiCover {
    cols: i32,
    rows: i32,
    set: Vec<bool>,
}

impl UiCover {
    /// The grid square's side, view units.
    pub const CELL: i32 = 8;

    /// Nothing covered, for a `frame`-sized view.
    pub fn new(frame: FrameSize) -> Self {
        let cols = (frame.width + Self::CELL - 1) / Self::CELL;
        let rows = (frame.height + Self::CELL - 1) / Self::CELL;
        UiCover {
            cols,
            rows,
            set: vec![false; (cols.max(0) * rows.max(0)) as usize],
        }
    }

    /// The squares of `[left, right) x [top, bottom)`, cut to the grid.
    fn squares(&self, left: i32, top: i32, right: i32, bottom: i32) -> Option<[i32; 4]> {
        if right <= left || bottom <= top {
            return None;
        }
        let c0 = left.div_euclid(Self::CELL).max(0);
        let r0 = top.div_euclid(Self::CELL).max(0);
        let c1 = (right - 1).div_euclid(Self::CELL).min(self.cols - 1);
        let r1 = (bottom - 1).div_euclid(Self::CELL).min(self.rows - 1);
        (c0 <= c1 && r0 <= r1).then_some([c0, r0, c1, r1])
    }

    /// Marks `[left, right) x [top, bottom)` covered.
    pub fn mark(&mut self, left: i32, top: i32, right: i32, bottom: i32) {
        let Some([c0, r0, c1, r1]) = self.squares(left, top, right, bottom) else {
            return;
        };
        for r in r0..=r1 {
            for c in c0..=c1 {
                self.set[(r * self.cols + c) as usize] = true;
            }
        }
    }

    /// Whether any UI reaches into `place`.
    pub fn covers(&self, place: &Place) -> bool {
        let Some([c0, r0, c1, r1]) =
            self.squares(place.left, place.top, place.right(), place.bottom())
        else {
            return false;
        };
        (r0..=r1).any(|r| (c0..=c1).any(|c| self.set[(r * self.cols + c) as usize]))
    }
}

/// What the world view hands the widget each frame it draws the world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimapFacts {
    /// The local player's client position.
    pub player: ClientPos,
    pub open_mode: OpenMode,
    /// The near rooms' marker units.
    pub markers: Vec<MarkerUnit>,
    /// The game UI drawn this frame.
    pub ui: UiCover,
}

#[cfg(test)]
#[path = "minimap_tests.rs"]
mod tests;
