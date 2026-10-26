//! A mod game's level backdrop scaled to cover the frame
//! (`crate::mods::ui_hooks::LevelBackdrop`): the grid is scaled by the
//! larger of the frame-to-grid ratios, so the frame has no bare sides, and
//! centred; what overhangs (rows at the top and bottom of a frame wider
//! than the grid, columns at the sides of a taller one) is cropped. The
//! frames are resampled nearest-neighbour (palette indices) once per frame
//! size; a grid of the frame's own size draws unscaled.

use super::background_view::BackdropCell;
use super::ViewAssets;
use crate::frames::{FrameError, FrameSet, FrameSetKey, IndexFrame};

/// The cover scale of a grid on a frame: `num / den`, exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cover {
    pub num: i64,
    pub den: i64,
}

impl Cover {
    /// The scale that covers a `frame` (w, h) with a `grid` (w, h);
    /// `None` for an empty grid.
    pub fn of(grid: (i32, i32), frame: (i32, i32)) -> Option<Cover> {
        let (gw, gh) = (i64::from(grid.0), i64::from(grid.1));
        let (fw, fh) = (i64::from(frame.0), i64::from(frame.1));
        if gw <= 0 || gh <= 0 {
            return None;
        }
        // fw / gw ≥ fh / gh: the width sets the scale.
        Some(if fw * gh >= fh * gw {
            Cover { num: fw, den: gw }
        } else {
            Cover { num: fh, den: gh }
        })
    }

    /// No scaling.
    pub fn is_one(self) -> bool {
        self.num == self.den
    }

    /// Grid coordinate `v` scaled (rounded down).
    pub fn scale(self, v: i32) -> i32 {
        (i64::from(v) * self.num).div_euclid(self.den) as i32
    }

    /// The grid coordinate under the centre of scaled pixel `v`.
    fn source(self, v: i32) -> i32 {
        ((2 * i64::from(v) + 1) * self.den).div_euclid(2 * self.num) as i32
    }

    /// The scaled grid's top-left on the frame: centred (negative where
    /// it is cropped).
    pub fn origin(self, grid: (i32, i32), frame: (i32, i32)) -> (i32, i32) {
        (
            (frame.0 - self.scale(grid.0)) / 2,
            (frame.1 - self.scale(grid.1)) / 2,
        )
    }
}

/// Cell `c`'s frame `image` resampled for `cover`: the scaled cell's
/// top-left in the scaled grid and its frame (offsets 0, top row first).
/// Neighbouring cells meet with no gap or overlap.
pub fn scale_cell(
    c: &BackdropCell,
    image: &IndexFrame,
    cover: Cover,
) -> Result<((i32, i32), IndexFrame), FrameError> {
    let (x0, x1) = (cover.scale(c.x), cover.scale(c.x + c.w));
    let (y0, y1) = (cover.scale(c.y), cover.scale(c.y + c.h));
    let (iw, ih) = (image.width as i32, image.height as i32);
    let (w, h) = ((x1 - x0).max(0), (y1 - y0).max(0));
    let mut pixels = Vec::with_capacity((w * h) as usize);
    if iw > 0 && ih > 0 {
        for y in y0..y1 {
            let sy = (cover.source(y) - c.y).clamp(0, ih - 1);
            let row = &image.pixels[(sy * iw) as usize..((sy + 1) * iw) as usize];
            pixels.extend((x0..x1).map(|x| row[(cover.source(x) - c.x).clamp(0, iw - 1) as usize]));
        }
    } else {
        pixels.resize((w * h) as usize, 0);
    }
    IndexFrame::new(w as u32, h as u32, 0, 0, pixels).map(|f| ((x0, y0), f))
}

/// The frame set of `key`'s `cells` scaled for `cover` on a `frame`
/// (w, h), frame i for cell i: made on first use for that frame size and
/// kept with the game's frames.
pub fn scaled_set(
    key: &FrameSetKey,
    cells: &[BackdropCell],
    cover: Cover,
    frame: (i32, i32),
    assets: &mut ViewAssets,
) -> Result<FrameSetKey, String> {
    let path = format!("{}/cover-{}x{}", key.path(), frame.0, frame.1);
    let scaled = FrameSetKey::new(path, key.part()).map_err(|e| e.to_string())?;
    if assets.frames.contains(&scaled) {
        return Ok(scaled);
    }
    let frames = cells
        .iter()
        .map(|c| {
            let image = assets.frame(key, c.frame).map_err(|e| e.to_string())?;
            scale_cell(c, image, cover)
                .map(|(_, f)| f)
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    assets
        .frames
        .insert(scaled.clone(), FrameSet { frames })
        .map_err(|e| e.to_string())?;
    Ok(scaled)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1024 × 768 grid on the 1280 × 720 player frame: scale 1.25, 120
    /// rows cut at the top and at the bottom, no bare columns.
    #[test]
    fn a_4_3_grid_covers_the_player_frame_cropping_rows() {
        let c = Cover::of((1024, 768), (1280, 720)).unwrap();
        assert_eq!((c.scale(1024), c.scale(768)), (1280, 960));
        assert_eq!(c.origin((1024, 768), (1280, 720)), (0, -120));
        // A tall frame crops columns instead.
        let t = Cover::of((1024, 768), (600, 800)).unwrap();
        assert_eq!(t.scale(768), 800);
        assert_eq!(t.origin((1024, 768), (600, 800)).1, 0);
        assert!(t.origin((1024, 768), (600, 800)).0 < 0);
        assert!(Cover::of((800, 600), (800, 600)).unwrap().is_one());
        assert_eq!(Cover::of((0, 600), (800, 600)), None);
    }

    /// A 2 × 2 cell doubled: each pixel becomes 2 × 2; the next cell
    /// starts where this one ends.
    #[test]
    fn cells_resample_nearest_and_meet() {
        let image = IndexFrame::new(2, 2, 0, 0, vec![1, 2, 3, 4]).unwrap();
        let cover = Cover { num: 2, den: 1 };
        let cell = |x| BackdropCell {
            frame: 0,
            x,
            y: 0,
            w: 2,
            h: 2,
        };
        let ((x, y), f) = scale_cell(&cell(0), &image, cover).unwrap();
        assert_eq!((x, y, f.width, f.height), (0, 0, 4, 4));
        assert_eq!(
            f.pixels,
            vec![1, 1, 2, 2, 1, 1, 2, 2, 3, 3, 4, 4, 3, 3, 4, 4]
        );
        let ((x, _), _) = scale_cell(&cell(2), &image, cover).unwrap();
        assert_eq!(x, 4);
        // 1.25: a 4-wide cell at 4 spans 5..10.
        let image = IndexFrame::new(4, 1, 0, 0, vec![1, 2, 3, 4]).unwrap();
        let c = BackdropCell {
            frame: 0,
            x: 4,
            y: 0,
            w: 4,
            h: 1,
        };
        let ((x, _), f) = scale_cell(&c, &image, Cover { num: 5, den: 4 }).unwrap();
        assert_eq!((x, f.width), (5, 5));
        assert_eq!(f.pixels, vec![1, 2, 3, 3, 4]);
    }
}
