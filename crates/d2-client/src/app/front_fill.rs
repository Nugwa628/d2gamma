//! The fill beside the front end's picture on a wide screen.
//!
//! The menus, character screens, credits and loading picture are drawn at
//! 800 × 600, a 4:3 picture. A 16:9 window shows it centred at its own
//! shape ([`super::front_host::front_presentation`]), leaving the window's
//! sides empty. With `[video] menu_sides = "black"` (the default) they stay
//! black. With `"blur"` they show a dim, blurred copy of the picture,
//! stretched to cover the whole window behind it, so the art seems to
//! carry on past its edges instead of stopping at a hard bar.
//!
//! The blur is made on the CPU and costs almost nothing: the picture is
//! averaged into a 40 × 30 grid of 20 × 20 blocks (25 samples each),
//! softened once more with a 3 × 3 box over the grid and darkened. The
//! grid is shown as an ordinary sprite with linear filtering, which every
//! GPU draws, so there is no shader of our own. The picture is sampled
//! before the pointer is drawn, so the pointer leaves no smudge in the
//! fill.
//!
//! A window of 4:3 or narrower never shows the fill: its bars stay black
//! and its output is exactly what it was without the fill.
//!
//! `"extend"` ([`extend`]) fills the sides from the picture's own edges
//! instead: each side shows the picture's outer columns smeared out to the
//! window's edge, softened and shaded darker toward it, so the art's edge
//! colours carry on without repeating any of its shapes. It is made the
//! same way, as a grid stretched over the window. The picture's own outer
//! 28 columns on each side are feathered into that smear ([`feather`]), so
//! the picture's edge melts into it instead of cutting across it. A mod game may lay its
//! front end out on its own canvas (`super::front_host::set_front_canvas`):
//! the fill then follows that picture's shape.

use crate::app::config::MenuSides;

use super::front_host::{front_presentation, HEIGHT, WIDTH};

/// The fill grid's size: one cell per [`BLOCK`] × [`BLOCK`] picture pixels.
pub const FILL_W: u32 = WIDTH / BLOCK;
pub const FILL_H: u32 = HEIGHT / BLOCK;

/// Picture pixels per grid cell, on each axis.
const BLOCK: u32 = 20;

/// Distance between the samples of a cell, on each axis (5 × 5 samples).
const STEP: u32 = 4;

/// The fill's brightness, `DIM_NUM / DIM_DEN` of the blurred picture: dark
/// enough that the picture itself stays the clear centre of the screen.
const DIM_NUM: u32 = 7;
const DIM_DEN: u32 = 20;

/// Columns of the picture's edge each side's [`extend`] fill is made of.
const EDGE_COLS: u32 = 12;

/// How much darker [`extend`] makes a side at the window's edge than at
/// the picture's, in percent.
const EDGE_SHADE: f32 = 45.0;

/// Whether the window shows the fill: the player chose one, the window is
/// wider than the picture's shape (4:3, or a mod game's front canvas) and
/// it holds the picture.
pub fn shown(sides: MenuSides, window_w: u32, window_h: u32) -> bool {
    let (pw, ph) = super::front_host::picture_shape();
    matches!(sides, MenuSides::Blur | MenuSides::Extend)
        && u64::from(window_w) * u64::from(ph) > u64::from(window_h) * u64::from(pw)
        && front_presentation(window_w, window_h).is_ok()
}

/// The scale (physical pixels per grid cell) that stretches the grid over
/// the whole `window_w × window_h` window, keeping its 4:3 shape and
/// centred: the larger of the two axis ratios, so it covers the window and
/// overflows on the other axis.
pub fn cover_scale(window_w: u32, window_h: u32) -> f32 {
    (window_w as f32 / FILL_W as f32).max(window_h as f32 / FILL_H as f32)
}

/// The fill grid (RGBA, `FILL_W × FILL_H`, opaque) of the picture whose
/// pixel (x, y) has colour `colour(x, y)`.
pub fn blur(colour: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
    let (w, h) = (FILL_W as usize, FILL_H as usize);
    // Block averages.
    let mut avg = vec![[0u32; 3]; w * h];
    let per = BLOCK / STEP;
    for (i, cell) in avg.iter_mut().enumerate() {
        let (bx, by) = ((i % w) as u32 * BLOCK, (i / w) as u32 * BLOCK);
        let mut sum = [0u32; 3];
        for sy in 0..per {
            for sx in 0..per {
                let c = colour(bx + STEP / 2 + sx * STEP, by + STEP / 2 + sy * STEP);
                for k in 0..3 {
                    sum[k] += u32::from(c[k]);
                }
            }
        }
        *cell = sum.map(|s| s / (per * per));
    }
    // A 3 × 3 box over the grid (edges repeated), then dimmed.
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 3];
            for dy in [-1, 0, 1] {
                for dx in [-1, 0, 1] {
                    let cx = (x as i64 + dx).clamp(0, w as i64 - 1) as usize;
                    let cy = (y as i64 + dy).clamp(0, h as i64 - 1) as usize;
                    for k in 0..3 {
                        sum[k] += avg[cy * w + cx][k];
                    }
                }
            }
            for s in sum {
                out.push((s * DIM_NUM / (9 * DIM_DEN)) as u8);
            }
            out.push(255);
        }
    }
    out
}

/// [`blur`] of the 800 × 600 picture centred in an index frame `width`
/// pixels wide (800, or a wide screen's canvas, [`super::front_host`]),
/// shown with `palette` (black without one).
pub fn blur_index(
    index: &[u8],
    width: u32,
    palette: Option<&d2_formats::palette::Palette>,
) -> Vec<u8> {
    let left = width.saturating_sub(WIDTH) / 2;
    blur(|x, y| {
        let i = index
            .get((y * width + left + x) as usize)
            .copied()
            .unwrap_or_default();
        palette.map_or([0; 3], |p| {
            let c = p.colors[usize::from(i)];
            [c.r, c.g, c.b]
        })
    })
}

/// [`blur`] of a whole `w × h` picture whose pixel (x, y) has colour
/// `colour(x, y)`: the 800 × 600 grid's samples spread over it.
pub fn blur_scaled(colour: impl Fn(u32, u32) -> [u8; 3], w: u32, h: u32) -> Vec<u8> {
    blur(|x, y| colour(x * w / WIDTH, y * h / HEIGHT))
}

/// The `"extend"` fill grid (RGBA, `FILL_W × FILL_H`, opaque) for a
/// `pic.0 × pic.1` picture whose pixel (x, y) has colour `colour(x, y)`,
/// shown at `place` (left, top, width, height in window pixels) in a
/// `win.0 × win.1` window, the grid stretched over the window as
/// [`cover_scale`] places it. A cell beside the picture takes the average
/// of the picture's [`EDGE_COLS`] outer columns on that side over the rows
/// beside it, shaded by up to [`EDGE_SHADE`] % toward the window's edge;
/// a cell under the picture takes the picture there. The grid is then
/// softened once more with a 3 × 3 box.
pub fn extend(
    colour: impl Fn(u32, u32) -> [u8; 3],
    pic: (u32, u32),
    place: (u32, u32, u32, u32),
    win: (u32, u32),
) -> Vec<u8> {
    let (pw, ph) = (pic.0.max(1), pic.1.max(1));
    let (left, top, iw, ih) = (
        place.0 as f32,
        place.1 as f32,
        place.2.max(1) as f32,
        place.3.max(1) as f32,
    );
    let (ww, wh) = (win.0 as f32, win.1 as f32);
    let s = cover_scale(win.0, win.1);
    let (ox, oy) = (
        (ww - FILL_W as f32 * s) / 2.0,
        (wh - FILL_H as f32 * s) / 2.0,
    );
    let right = left + iw;
    // Picture rows per grid cell.
    let span = ((s / ih) * ph as f32).max(1.0) as u32;
    let edge = EDGE_COLS.min(pw);
    let (w, h) = (FILL_W as usize, FILL_H as usize);
    let mut grid = vec![[0f32; 3]; w * h];
    for (i, cell) in grid.iter_mut().enumerate() {
        let wx = ox + ((i % w) as f32 + 0.5) * s;
        let wy = oy + ((i / w) as f32 + 0.5) * s;
        let py = (((wy - top) / ih) * ph as f32).clamp(0.0, (ph - 1) as f32) as u32;
        let (cols, shade) = if wx < left {
            let t = ((left - wx) / left.max(1.0)).clamp(0.0, 1.0);
            ((0, edge), t)
        } else if wx >= right {
            let t = ((wx - right) / (ww - right).max(1.0)).clamp(0.0, 1.0);
            ((pw - edge, pw), t)
        } else {
            let px = (((wx - left) / iw) * pw as f32).clamp(0.0, (pw - 1) as f32) as u32;
            ((px, px + 1), 0.0)
        };
        let mut sum = [0u32; 3];
        let mut n = 0u32;
        let rows = py.saturating_sub(span / 2)..(py + span / 2 + 1).min(ph);
        for y in rows.step_by((span / 5).max(1) as usize) {
            for x in (cols.0..cols.1).step_by(3) {
                let c = colour(x, y);
                for k in 0..3 {
                    sum[k] += u32::from(c[k]);
                }
                n += 1;
            }
        }
        let keep = 1.0 - EDGE_SHADE / 100.0 * shade;
        *cell = sum.map(|v| v as f32 / n.max(1) as f32 * keep);
    }
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0f32; 3];
            for dy in [-1, 0, 1] {
                for dx in [-1, 0, 1] {
                    let cx = (x as i64 + dx).clamp(0, w as i64 - 1) as usize;
                    let cy = (y as i64 + dy).clamp(0, h as i64 - 1) as usize;
                    for k in 0..3 {
                        sum[k] += grid[cy * w + cx][k];
                    }
                }
            }
            for v in sum {
                out.push((v / 9.0).round().clamp(0.0, 255.0) as u8);
            }
            out.push(255);
        }
    }
    out
}

/// Columns of the picture at each side that [`feather`] dissolves into the
/// [`extend`] fill.
pub const FEATHER_COLS: u32 = 28;

/// Rows above and below a row that [`edge_colours`] averages.
const EDGE_ROWS: i32 = 10;

/// The colour each side of the picture's rows hands on to the [`extend`]
/// fill: for each row of a `pic.0 × pic.1` picture, the average of its
/// [`EDGE_COLS`] outer columns at the left and at the right (as `[left,
/// right]`), over the rows around it, so the fill's smear and this agree
/// where they meet.
pub fn edge_colours(colour: impl Fn(u32, u32) -> [u8; 3], pic: (u32, u32)) -> Vec<[[u8; 3]; 2]> {
    let (pw, ph) = (pic.0.max(1), pic.1.max(1));
    let edge = EDGE_COLS.min(pw);
    let side = |y: u32, cols: std::ops::Range<u32>| {
        let mut sum = [0u32; 3];
        let mut n = 0u32;
        for dy in (-EDGE_ROWS..=EDGE_ROWS).step_by(5) {
            let row = (y as i32 + dy).clamp(0, ph as i32 - 1) as u32;
            for x in cols.clone().step_by(3) {
                let c = colour(x, row);
                for k in 0..3 {
                    sum[k] += u32::from(c[k]);
                }
                n += 1;
            }
        }
        sum.map(|v| (v / n.max(1)) as u8)
    };
    (0..ph)
        .map(|y| [side(y, 0..edge), side(y, pw - edge..pw)])
        .collect()
}

/// Feathers the picture's own edge into the [`extend`] fill: over the
/// outer [`FEATHER_COLS`] columns of each side of the RGBA picture `px`
/// (`size`), each pixel is blended toward that row's [`edge_colours`], by
/// a share of the pixel that grows linearly from the picture's edge inward
/// (the outermost column keeps a 56th of itself, the innermost 55/56ths),
/// so the picture's hard edge melts into the smear beside it instead of
/// cutting across it.
pub fn feather(px: &mut [u8], size: (u32, u32), edges: &[[[u8; 3]; 2]]) {
    let (w, h) = (size.0 as usize, size.1 as usize);
    if px.len() != w * h * 4 || edges.len() != h {
        return;
    }
    let cols = (FEATHER_COLS as usize).min(w / 2);
    for (y, edge) in edges.iter().enumerate() {
        for c in 0..cols {
            let keep = (c as f32 + 0.5) / cols as f32;
            for (x, to) in [(c, edge[0]), (w - 1 - c, edge[1])] {
                let o = (y * w + x) * 4;
                for k in 0..3 {
                    let v = f32::from(px[o + k]) * keep + f32::from(to[k]) * (1.0 - keep);
                    px[o + k] = v.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

/// [`blur`] of an 800 × 600 RGBA picture.
pub fn blur_rgba(px: &[u8]) -> Vec<u8> {
    blur(|x, y| {
        let o = ((y * WIDTH + x) * 4) as usize;
        px.get(o..o + 3).map_or([0; 3], |c| [c[0], c[1], c[2]])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Owner bug B1 on the front end: under Integer the menus stay the
    /// 800 × 600 picture at the largest whole scale, centred, on every
    /// window, a maximized one with an odd inner size included.
    #[test]
    fn integer_menus_keep_their_picture_at_the_largest_whole_scale() {
        crate::ui::set_scale_mode(crate::ui::ScaleMode::Integer);
        for ((w, h), n) in [
            ((3840, 2160), 3),
            ((3840, 2097), 3),
            ((2560, 1440), 2),
            ((1920, 1080), 1),
            ((1919, 1079), 1),
            ((1366, 768), 1),
        ] {
            let p = front_presentation(w, h).unwrap();
            assert_eq!((p.frame_w, p.frame_h), (WIDTH, HEIGHT), "{w}x{h}");
            assert_eq!((p.image_w, p.image_h), (WIDTH * n, HEIGHT * n), "{w}x{h}");
            assert_eq!((p.left, p.top), ((w - WIDTH * n) / 2, (h - HEIGHT * n) / 2));
            assert!(shown(MenuSides::Blur, w, h), "{w}x{h}");
        }
        crate::ui::set_scale_mode(crate::ui::ScaleMode::Fit);
    }

    #[test]
    fn the_fill_shows_only_when_chosen_and_the_window_is_wider_than_4_3() {
        for (w, h) in [(1920, 1080), (2560, 1440), (3840, 2160), (1366, 768)] {
            assert!(shown(MenuSides::Blur, w, h), "{w}x{h}");
            assert!(!shown(MenuSides::Black, w, h), "{w}x{h}");
        }
        // 4:3 (and taller) windows keep their black bars.
        for (w, h) in [
            (800, 600),
            (1024, 768),
            (1600, 1200),
            (2400, 1800),
            (1280, 1024),
        ] {
            assert!(!shown(MenuSides::Blur, w, h), "{w}x{h}");
        }
        // A window with no height presents nothing.
        assert!(!shown(MenuSides::Blur, 1920, 0));
    }

    #[test]
    fn the_grid_covers_the_window_and_keeps_its_shape() {
        for (w, h) in [(1920, 1080), (2560, 1440), (3840, 2160), (800, 600)] {
            let s = cover_scale(w, h);
            let (gw, gh) = (FILL_W as f32 * s, FILL_H as f32 * s);
            assert!(gw >= w as f32 && gh >= h as f32, "{w}x{h}");
            // One axis fits exactly.
            assert!(gw == w as f32 || gh == h as f32, "{w}x{h}");
        }
        // 16:9: the height overflows, the width fits.
        assert_eq!(cover_scale(1920, 1080), 48.0);
    }

    #[test]
    fn a_flat_picture_blurs_to_its_colour_dimmed() {
        let fill = blur(|_, _| [200, 100, 40]);
        assert_eq!(fill.len(), (FILL_W * FILL_H * 4) as usize);
        for c in fill.chunks(4) {
            assert_eq!(c, [70, 35, 14, 255]);
        }
        assert!(blur(|_, _| [0; 3]).chunks(4).all(|c| c == [0, 0, 0, 255]));
    }

    #[test]
    fn the_blur_follows_the_picture_and_softens_its_edges() {
        // Left half white, right half black.
        let fill = blur(|x, _| if x < WIDTH / 2 { [255; 3] } else { [0; 3] });
        let at = |x: u32, y: u32| fill[((y * FILL_W + x) * 4) as usize];
        let mid = FILL_W / 2;
        assert_eq!(at(0, 5), at(mid - 2, 5));
        assert!(at(mid - 1, 5) < at(mid - 2, 5));
        assert!(at(mid, 5) > 0);
        assert_eq!(at(FILL_W - 1, 5), 0);
    }

    #[test]
    fn index_and_rgba_pictures_blur_alike() {
        let mut pal = d2_formats::palette::Palette {
            colors: [Default::default(); 256],
        };
        pal.colors[7] = d2_formats::palette::Rgb {
            r: 90,
            g: 180,
            b: 30,
        };
        let index: Vec<u8> = (0..WIDTH * HEIGHT)
            .map(|i| if (i % WIDTH) < 300 { 7 } else { 0 })
            .collect();
        let rgba: Vec<u8> = index
            .iter()
            .flat_map(|&i| {
                let c = pal.colors[usize::from(i)];
                [c.r, c.g, c.b, 255]
            })
            .collect();
        assert_eq!(blur_index(&index, WIDTH, Some(&pal)), blur_rgba(&rgba));
        assert!(blur_index(&index, WIDTH, None)
            .chunks(4)
            .all(|c| c == [0, 0, 0, 255]));
    }

    /// The fill of a wide canvas (the 16:9 lobby) is the blur of its
    /// centred 800 × 600 picture, the same grid as the 4:3 one.
    #[test]
    fn a_wide_frame_blurs_its_centred_picture() {
        let mut pal = d2_formats::palette::Palette {
            colors: [Default::default(); 256],
        };
        pal.colors[7] = d2_formats::palette::Rgb {
            r: 200,
            g: 40,
            b: 90,
        };
        pal.colors[9] = d2_formats::palette::Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        let narrow: Vec<u8> = (0..WIDTH * HEIGHT)
            .map(|i| if (i / WIDTH) < 200 { 7 } else { 0 })
            .collect();
        let wide_w = 1068;
        let left = (wide_w - WIDTH) / 2;
        let wide: Vec<u8> = (0..wide_w * HEIGHT)
            .map(|i| {
                let (x, y) = (i % wide_w, i / wide_w);
                match x.checked_sub(left).filter(|&x| x < WIDTH) {
                    Some(x) => narrow[(y * WIDTH + x) as usize],
                    // Beside the picture: another colour, never sampled.
                    None => 9,
                }
            })
            .collect();
        let want = blur_index(&narrow, WIDTH, Some(&pal));
        assert_eq!(blur_index(&wide, wide_w, Some(&pal)), want);
        assert_eq!(want.len(), (FILL_W * FILL_H * 4) as usize);
    }

    /// The extend fill carries each side's edge colours out to the
    /// window's edge, row by row, darker toward it, and never black.
    #[test]
    fn extend_smears_the_pictures_edges_and_shades_them_outward() {
        // A 1024 x 768 picture: red left half, blue right half, its top
        // half lighter than its bottom; shown at the full height of a
        // 1920 x 1080 window.
        let colour = |x: u32, y: u32| {
            let v = if y < 384 { 240 } else { 120 };
            if x < 512 {
                [v, 0, 0]
            } else {
                [0, 0, v]
            }
        };
        let (iw, ih) = (1440, 1080);
        let left = (1920 - iw) / 2;
        let g = extend(colour, (1024, 768), (left, 0, iw, ih), (1920, 1080));
        assert_eq!(g.len(), (FILL_W * FILL_H * 4) as usize);
        let at = |x: u32, y: u32| {
            let o = ((y * FILL_W + x) * 4) as usize;
            [g[o], g[o + 1], g[o + 2], g[o + 3]]
        };
        // The grid covers the window at 48 px a cell: the picture spans
        // cells 5..35; rows 3..26 show.
        let (outer, inner) = (at(0, 8), at(3, 8));
        assert!(outer[0] > 0 && outer[2] == 0, "{outer:?}");
        assert!(inner[0] > outer[0], "darker toward the edge");
        assert!(at(38, 8)[2] > 0 && at(38, 8)[0] == 0);
        // Rows follow the picture's rows.
        assert!(at(3, 6)[0] > at(3, 22)[0]);
        // Opaque, and no black anywhere beside the picture.
        for y in 3..27 {
            for x in (0..5).chain(35..FILL_W) {
                let c = at(x, y);
                assert_eq!(c[3], 255);
                assert!(c[0] > 0 || c[2] > 0, "black at {x},{y}");
            }
        }
    }

    /// A flat picture has nothing to feather: its edge is the smear's
    /// colour. A picture with a hard edge melts into it, the outermost
    /// column most, and the picture's inside stays as it is.
    #[test]
    fn feather_melts_the_pictures_edge_into_the_smear() {
        let (w, h) = (200u32, 60u32);
        let flat = |_: u32, _: u32| [90, 120, 150];
        let edges = edge_colours(flat, (w, h));
        assert_eq!(edges.len(), h as usize);
        let mut px: Vec<u8> = (0..w * h).flat_map(|_| [90, 120, 150, 255]).collect();
        let before = px.clone();
        feather(&mut px, (w, h), &edges);
        assert_eq!(px, before);

        // Dark rim, bright middle: the left 3 columns are dark.
        let art = |x: u32, _: u32| {
            if x < 3 || x >= w - 3 {
                [10; 3]
            } else {
                [250; 3]
            }
        };
        let edges = edge_colours(art, (w, h));
        let mut px: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let c = art(i % w, i / w);
                [c[0], c[1], c[2], 255]
            })
            .collect();
        let before = px.clone();
        feather(&mut px, (w, h), &edges);
        let at = |px: &[u8], x: u32, y: u32| px[((y * w + x) * 4) as usize];
        // The smear is mostly bright (3 dark of 12 columns): the dark rim
        // moves toward it, and the same at the right side.
        assert!(at(&px, 0, 30) > at(&before, 0, 30) + 100);
        assert!(at(&px, w - 1, 30) > at(&before, w - 1, 30) + 100);
        // Inside the rim the feathered columns climb back to the picture.
        for x in 4..FEATHER_COLS {
            assert!(at(&px, x, 30) >= at(&px, x - 1, 30), "{x}");
        }
        // Past the feathered columns, and in the alpha, nothing changes.
        let rest = |p: &[u8]| {
            (0..h)
                .flat_map(|y| (FEATHER_COLS..w - FEATHER_COLS).map(move |x| (x, y)))
                .map(|(x, y)| p[((y * w + x) * 4) as usize..][..4].to_vec())
                .collect::<Vec<_>>()
        };
        assert_eq!(rest(&px), rest(&before));
        assert!(px.chunks(4).all(|c| c[3] == 255));
        // A buffer that is not the picture's size is left alone.
        let mut short = vec![7u8; 16];
        feather(&mut short, (w, h), &edges);
        assert_eq!(short, vec![7u8; 16]);
    }

    /// Extend is a fill the window shows, as blur is; black is none.
    #[test]
    fn extend_shows_on_a_wide_window_only() {
        assert!(shown(MenuSides::Extend, 1920, 1080));
        assert!(!shown(MenuSides::Extend, 1024, 768));
        assert!(!shown(MenuSides::Black, 1920, 1080));
    }
}
