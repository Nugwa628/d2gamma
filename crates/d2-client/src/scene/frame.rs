//! The persistent index framebuffer and the frame cycle of the D2RUST renderer: clear at the
//! start of a frame (the level's BlankScreen flag), draw, the optional post-draw clear, present
//! through one palette.
//!
//! The framebuffer keeps its pixels from frame to frame, so a frame only changes what it draws
//! over. The clears are expressed as a [`FramePlan`] that both compositors apply around the draws
//! (CPU: [`super::cpu::compose_frame`]; GPU: the `clear_rows` / `clear_after` params of
//! `gpu_compositor`), so the frame cycle is computed once and the two images stay byte-identical.
//!
//! There is no whole-surface clear every frame, no gamma ramp and no forced-black palette entry 0:
//! the palette is presented exactly as stored.

use std::sync::atomic::{AtomicBool, Ordering};

use d2_formats::palette::{Palette, Rgb};

use super::cpu;
use super::item::{DrawItem, FrameSource, MapTable};
use super::{Rect, SceneError};

/// Bottom rows a start-of-frame clear never touches: only the first `(H − 47) × W` bytes are
/// cleared. They are the control panel's rows, which the world leaves to the HUD
/// ([`hud_rows`]).
pub const UNCLEARED_ROWS: u32 = 47;

/// Whether the world is drawn under the control panel's rows too ([`set_world_under_hud`]).
static WORLD_UNDER_HUD: AtomicBool = AtomicBool::new(false);

/// Draws the world under the control panel's rows too (`on`), or leaves them to the HUD (the
/// default). A HUD drawn smaller than the picture (UI Scale below 100 %, `ui::ui_space`) no
/// longer covers them, so the game view turns it on for those frames: the tile clip, the
/// floor and roof cull, the weather passes and the start-of-frame clear then reach the
/// frame's bottom edge.
pub fn set_world_under_hud(on: bool) {
    WORLD_UNDER_HUD.store(on, Ordering::Relaxed);
}

/// The frame's bottom rows the world leaves to the control panel: [`UNCLEARED_ROWS`], or 0
/// while the world is drawn under the HUD ([`set_world_under_hud`]).
pub fn hud_rows() -> u32 {
    if WORLD_UNDER_HUD.load(Ordering::Relaxed) {
        0
    } else {
        UNCLEARED_ROWS
    }
}

/// Bytes of a PL2 file that hold the presented palette: 256 entries of R, G, B, x.
pub const PL2_PALETTE_BYTES: usize = 1024;

/// What one frame does around its draws, in view rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FramePlan {
    /// Rows `0..clear_rows` start the frame as index 0; the other rows
    /// keep the previous frame's index.
    pub clear_rows: u32,
    /// Every pixel becomes index 0 after all drawing; set when the post-draw clear counter is
    /// above 0.
    pub clear_after: bool,
}

impl FramePlan {
    /// No clear: draws go onto the base unchanged and stay.
    pub const NONE: FramePlan = FramePlan {
        clear_rows: 0,
        clear_after: false,
    };
}

/// The 8-bit index framebuffer and the frame-cycle state it carries between frames: the pixels
/// persist, and so does the post-draw clear counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameCycle {
    width: u32,
    height: u32,
    /// Row-major, top row first: pixel `(x, y)` is byte `y × W + x`.
    pixels: Vec<u8>,
    /// Frames still owed a post-draw clear.
    post_clear: u32,
}

impl FrameCycle {
    /// A `width × height` framebuffer of index 0 with the counter at 0.
    /// The height must exceed [`UNCLEARED_ROWS`]: the start-of-frame clear is only defined
    /// there (the screen heights in use are 480, 600 and 700 rows).
    pub fn new(width: u32, height: u32) -> Result<Self, SceneError> {
        Self::with_pixels(width, height, vec![0; width as usize * height as usize])
    }

    /// A framebuffer holding `pixels` (e.g. a previous frame kept for a test).
    pub fn with_pixels(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, SceneError> {
        if height <= UNCLEARED_ROWS {
            return Err(SceneError::FramebufferHeight { height });
        }
        if pixels.len() as u64 != u64::from(width) * u64::from(height) {
            return Err(SceneError::BaseSize {
                len: pixels.len(),
                pixels: u64::from(width) * u64::from(height),
            });
        }
        Ok(FrameCycle {
            width,
            height,
            pixels,
            post_clear: 0,
        })
    }

    /// Framebuffer width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Framebuffer height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The framebuffer as a view at the origin: what both compositors
    /// compose a frame of this cycle into.
    pub fn view(&self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    /// The framebuffer: after [`FrameCycle::compose`] or
    /// [`FrameCycle::commit`], the presented frame.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The post-draw clear counter: how many frames still end with a full clear.
    pub fn post_clear(&self) -> u32 {
        self.post_clear
    }

    /// Sets the counter. Which game event should set it is not decided in this module.
    pub fn set_post_clear(&mut self, counter: u32) {
        self.post_clear = counter;
    }

    /// The clears of the next frame. `blank_screen` is the BlankScreen flag of the player's
    /// current level (`Levels.txt`): when set, all but the bottom [`hud_rows`] rows start
    /// the frame as index 0.
    pub fn plan(&self, blank_screen: bool) -> FramePlan {
        FramePlan {
            clear_rows: if blank_screen {
                self.height - hud_rows()
            } else {
                0
            },
            clear_after: self.post_clear > 0,
        }
    }

    /// One whole frame on the CPU reference: [`FrameCycle::plan`], the
    /// draws onto the persistent framebuffer, then [`FrameCycle::commit`].
    /// Returns the presented index frame. On error nothing changes.
    pub fn compose<F: FrameSource + ?Sized>(
        &mut self,
        blank_screen: bool,
        items: &[DrawItem],
        frames: &F,
        maps: &MapTable,
    ) -> Result<&[u8], SceneError> {
        let plan = self.plan(blank_screen);
        let next = cpu::compose_frame(items, frames, maps, self.view(), &self.pixels, plan)?;
        self.commit(plan, next)?;
        Ok(&self.pixels)
    }

    /// Takes `presented` (a frame composed elsewhere, e.g. read back from
    /// the GPU compositor, with `plan` from [`FrameCycle::plan`]) as the
    /// new framebuffer and counts the post-draw clear down by one if `plan` had one.
    pub fn commit(&mut self, plan: FramePlan, presented: Vec<u8>) -> Result<(), SceneError> {
        if presented.len() != self.pixels.len() {
            return Err(SceneError::BaseSize {
                len: presented.len(),
                pixels: self.pixels.len() as u64,
            });
        }
        self.pass(plan)?;
        self.pixels = presented;
        Ok(())
    }

    /// Counts a frame of `plan` that was composed elsewhere without taking
    /// its pixels: the GPU composes the next frame over its own copy of
    /// them, so the framebuffer here is behind until the next
    /// [`FrameCycle::commit`]. The post-draw clear counts down as in
    /// `commit`, so every frame's plan is counted once.
    pub fn pass(&mut self, plan: FramePlan) -> Result<(), SceneError> {
        if !self.fits(plan) {
            return Err(SceneError::FramePlan(plan));
        }
        if plan.clear_after {
            self.post_clear -= 1;
        }
        Ok(())
    }

    /// Whether `plan` is one [`FrameCycle::plan`] makes for this cycle's
    /// next frame: the post-draw clear as the counter stands, and no
    /// start-of-frame clear or one that leaves the HUD's rows or not
    /// ([`hud_rows`]). The GPU counts a frame a pass or more after it was
    /// planned, and the HUD rows can change in between (UI Scale moved
    /// across 100 %), so a plan of either is still this cycle's.
    fn fits(&self, plan: FramePlan) -> bool {
        plan.clear_after == (self.post_clear > 0)
            && [0, self.height - UNCLEARED_ROWS, self.height].contains(&plan.clear_rows)
    }
}

/// The presented palette of an act: the first 1,024 bytes of its
/// `pal.pl2`, index `i` = `(pl2[4i], pl2[4i + 1], pl2[4i + 2])`, index 0
/// included; byte `4i + 3` is not a color.
pub fn present_palette(pl2: &[u8]) -> Result<Palette, SceneError> {
    let head = pl2
        .get(..PL2_PALETTE_BYTES)
        .ok_or(SceneError::Pl2Size { len: pl2.len() })?;
    let mut palette = Palette {
        colors: [Rgb::default(); 256],
    };
    for (c, e) in palette.colors.iter_mut().zip(head.as_chunks::<4>().0) {
        *c = Rgb {
            r: e[0],
            g: e[1],
            b: e[2],
        };
    }
    Ok(palette)
}
