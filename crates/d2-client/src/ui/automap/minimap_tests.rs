//! The minimap's place, zoom, picture and UI cover, with synthetic cels.

use std::collections::BTreeMap;

use d2_formats::palette::{Palette, Rgb};

use super::*;
use crate::rules::camera::SIDE_PANEL_WIDTH as SIDE_PANEL_W;
use crate::ui::automap::cells::Cell;
use crate::ui::automap::markers::MarkerSubject;

const FRAMES: [FrameSize; 4] = [
    FrameSize::PLAY,
    FrameSize::D2RUST,
    FrameSize::LOW,
    FrameSize {
        width: 1068,
        height: 600,
    },
];

/// The side panels open in `mode` as rectangles `[left, right)` across the
/// panel area (a panel runs from the top to the control panel).
fn panels(frame: FrameSize, mode: u8) -> Vec<(i32, i32)> {
    let span = frame.side_panel_span();
    let left = (0, span);
    let right = (frame.width - span, frame.width);
    match mode {
        1 => vec![right],
        2 => vec![left],
        3 => vec![left, right],
        _ => vec![],
    }
}

/// At every size, frame and open mode the widget sits inside the frame,
/// above the control panel, clear of every open side panel; with no panel
/// open it is in the top-right corner, and with both open it hides.
#[test]
fn the_minimap_never_covers_the_control_panel_or_an_open_side_panel() {
    for frame in FRAMES {
        for size in 0..3u8 {
            for mode in 0..=3u8 {
                let p = place(UiSpace::identity(frame), OpenMode::new(mode).unwrap(), size);
                if mode == 3 {
                    assert_eq!(p, None, "{frame:?} size {size}: both panels open");
                    continue;
                }
                let p = p.unwrap_or_else(|| panic!("{frame:?} size {size} mode {mode}"));
                assert_eq!(p.side, SIZES[usize::from(size)]);
                assert!(p.left >= MARGIN && p.right() <= frame.width - MARGIN);
                assert_eq!(p.top, MARGIN);
                assert!(
                    p.bottom() <= frame.play_height() - MARGIN,
                    "{frame:?} size {size}: reaches the control panel"
                );
                for (l, r) in panels(frame, mode) {
                    assert!(
                        p.right() <= l || p.left >= r,
                        "{frame:?} size {size} mode {mode}: {p:?} over the panel {l}..{r}"
                    );
                }
                let right = match mode {
                    1 => frame.width - frame.side_panel_span(),
                    _ => frame.width,
                };
                assert_eq!(p.right(), right - MARGIN, "the world's top-right corner");
            }
        }
    }
    // At the 1280 x 720 view: medium is 240 units, 10 in from the corner.
    assert_eq!(
        place(UiSpace::identity(FrameSize::PLAY), OpenMode::NONE, 1),
        Some(Place {
            left: 1030,
            top: 10,
            side: 240
        })
    );
    // An open right panel (320 wide) moves it to the panel's left.
    assert_eq!(
        place(
            UiSpace::identity(FrameSize::PLAY),
            OpenMode::new(1).unwrap(),
            1
        )
        .map(|p| p.right()),
        Some(950)
    );
    // An out-of-range size reads as the largest.
    assert_eq!(side(9), SIZES[2]);
}

/// Under UI Scale the side panels and the control panel are drawn from the
/// UI frame, so an open right panel takes more (or fewer) of the frame's
/// columns: the widget still sits left of the panel as drawn, above the
/// control panel as drawn, and in the frame's corner with no panel open.
#[test]
fn the_minimap_clears_the_right_panel_as_ui_scale_draws_it() {
    let right = OpenMode::new(1).unwrap();
    // The UI frames of 1280 x 720 at 90, 110, 120, 125 and 133 %.
    for (width, height) in [
        (1422, 800),
        (1164, 655),
        (1067, 600),
        (1024, 576),
        (962, 541),
    ] {
        let space = UiSpace {
            frame: FrameSize::PLAY,
            ui: FrameSize { width, height },
        };
        let ui = (width, height);
        // The frame column where the right panel's first UI column is shown.
        let panel_left = space.x_to_frame(space.ui.width - SIDE_PANEL_W);
        let hud_top = space.y_to_frame(space.ui.play_height());
        for size in 0..3u8 {
            let p = place(space, right, size).unwrap_or_else(|| panic!("{ui:?} size {size}"));
            assert_eq!(p.right(), panel_left - MARGIN, "{ui:?} size {size}: {p:?}");
            assert!(p.bottom() <= hud_top - MARGIN, "{ui:?} size {size}");
            let none = place(space, OpenMode::NONE, size).unwrap();
            assert_eq!(none.right(), FrameSize::PLAY.width - MARGIN);
            assert_eq!(place(space, OpenMode::new(3).unwrap(), size), None);
        }
    }
    // At 125 % the 1024-wide UI frame's panel starts at UI column 704, frame
    // column 880; the medium widget ends 10 columns before it.
    let at125 = UiSpace {
        frame: FrameSize::PLAY,
        ui: FrameSize {
            width: 1024,
            height: 576,
        },
    };
    let p = place(at125, right, 1).unwrap();
    assert_eq!((p.left, p.right()), (630, 870));
}

/// A map pixel is a whole number of output pixels: 2 at 2160p (3 per view
/// unit), 1 at 1440p and 1080p, never 0.
#[test]
fn map_pixels_are_whole_output_pixels() {
    for (scale, n) in [
        (3.0, 2),
        (2.0, 1),
        (1.5, 1),
        (1.0, 1),
        (0.5, 1),
        (4.5, 3),
        (0.0, 1),
        (f32::NAN, 1),
    ] {
        assert_eq!(zoom(scale), n, "scale {scale}");
    }
    // A 720-pixel box at 2: 360 map pixels; an odd box gets one more.
    assert_eq!(picture_side(720, 2), 360);
    assert_eq!(picture_side(721, 2), 361);
    assert_eq!(picture_side(0, 0), 1);
}

struct Cels(BTreeMap<(CelFile, i16), IndexFrame>);

impl MinimapCels for Cels {
    fn frame(&self, file: CelFile, cel: i16) -> Option<&IndexFrame> {
        self.0.get(&(file, cel))
    }
}

/// Greys (i, i, i), with index 9 pure blue (the local player's marker)
/// and 40 green.
fn palette() -> Palette {
    let mut colors = [Rgb::default(); 256];
    for (i, c) in colors.iter_mut().enumerate() {
        *c = Rgb {
            r: i as u8,
            g: i as u8,
            b: i as u8,
        };
    }
    colors[9] = Rgb { r: 0, g: 0, b: 255 };
    colors[40] = Rgb { r: 0, g: 200, b: 0 };
    Palette { colors }
}

fn square(index: u8) -> IndexFrame {
    IndexFrame::new(2, 2, 0, 0, vec![index; 4]).unwrap()
}

fn local_player(at: ClientPos) -> MarkerUnit {
    MarkerUnit {
        subject: MarkerSubject::Player {
            local: true,
            mode: 1,
            party: -1,
            own_inventory: true,
            state_7: false,
            name: Vec::new(),
        },
        pos: at,
    }
}

/// The picture is the automap around the player: the local player's cross
/// in the middle, a wall cell where the full automap puts it relative to
/// the player, town art from the town's own file, nothing elsewhere.
#[test]
fn the_picture_is_the_automap_centred_on_the_local_player() {
    let player = ClientPos { x: 1000, y: 2000 };
    // The marker sits at (100 + 8, 200 − 8) map pixels: the middle of a
    // 64 x 64 picture, origin (76, 160).
    assert_eq!(origin(player, 64, 64), (76, 160));
    let mut cells = LayerCells::default();
    let cell = |cel, x, y| Cell {
        saved: false,
        cel,
        x,
        y,
    };
    cells.tree_mut(TreeKind::Wall).insert(cell(3, 96, 170));
    cells.tree_mut(TreeKind::Town).insert(cell(1, 130, 210));
    // Far outside the picture: not drawn, never a panic.
    cells.tree_mut(TreeKind::Floor).insert(cell(3, -4000, 9000));
    let cels = Cels(BTreeMap::from([
        ((CelFile::MaxiMap, 3), square(200)),
        ((CelFile::Act2Map, 1), square(40)),
        // The automap file's cel 1 is not the town's art.
        ((CelFile::MaxiMap, 1), square(77)),
    ]));
    let pal = palette();
    let markers = [local_player(player)];
    let pic = picture(&PictureInput {
        cells: &cells,
        town: TownKind::LutGholein,
        player,
        markers: &markers,
        palette: &pal,
        cels: &cels,
        width: 64,
        height: 64,
    });
    assert_eq!((pic.width, pic.height), (64, 64));
    // The wall: (96 − 76, 170 − 160).
    assert_eq!(pic.at(20, 10), [200, 200, 200, 255]);
    assert_eq!(pic.at(21, 11), [200, 200, 200, 255]);
    // The town art, from the Lut Gholein file: (130 − 76, 210 − 160).
    assert_eq!(pic.at(54, 50), [0, 200, 0, 255]);
    // The cross's right tip (+8, −2 from the marker at the middle).
    assert_eq!(pic.at(40, 30), [0, 0, 255, 255]);
    assert_eq!(pic.at(24, 30), [0, 0, 255, 255]);
    // Nothing drawn: transparent.
    assert_eq!(pic.at(5, 60), [0, 0, 0, 0]);
    // Every drawn pixel is one of the two squares or the cross (8 by 4
    // map pixels each way from the marker).
    for y in 0..64 {
        for x in 0..64 {
            let known = (20..22).contains(&x) && (10..12).contains(&y)
                || (54..56).contains(&x) && (50..52).contains(&y)
                || (24..=40).contains(&x) && (28..=36).contains(&y);
            assert!(known || pic.at(x, y)[3] == 0, "stray pixel at ({x}, {y})");
        }
    }
}

/// The cover is kept per 8-unit square: a UI item one square away from the
/// box leaves it shown, one reaching a single pixel into it hides it.
#[test]
fn ui_reaching_into_the_box_covers_it() {
    let p = Place {
        left: 1032,
        top: 8,
        side: 240,
    };
    let mut c = UiCover::new(FrameSize::PLAY);
    assert!(!c.covers(&p));
    // Up to x 1032 (exclusive), and below the box: clear.
    c.mark(900, 100, 1032, 200);
    c.mark(1100, 248, 1200, 400);
    // Off the frame: ignored.
    c.mark(-50, -50, -1, -1);
    c.mark(2000, 0, 2100, 50);
    assert!(!c.covers(&p));
    c.mark(1000, 100, 1033, 101);
    assert!(c.covers(&p));
    // An empty rectangle marks nothing.
    let mut e = UiCover::new(FrameSize::PLAY);
    e.mark(1100, 50, 1100, 60);
    assert!(!e.covers(&p));
}
