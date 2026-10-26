//! UI Scale: the HUD, the panels, their text and the cursor drawn larger or
//! smaller than the picture's own scale.
//!
//! The UI lays itself out on a UI frame of the play frame's size divided
//! by the scale (a 1280 × 720 frame at 110 % gives a 1164 × 655 UI frame)
//! and the native UI pass (`world_view::native_ui`) draws that UI frame
//! over the whole picture, so every UI element comes out `scale` times as
//! large while the edges still meet the picture's edges: panels anchored
//! at the sides stay at the sides, the HUD at the bottom. The world, its
//! camera, the hover pick and world clicks stay in frame space; labels
//! anchored on the world (unit names, item labels, the automap) stay at
//! the picture's scale.
//!
//! A larger UI goes up to 120 %: the side panels' art is laid out for
//! the 800 × 600 frame's 600 rows (`ui::layout::Screen::panel_dy`), so a
//! UI frame under 600 rows would cut their top rows off ([`MIN_UI`]). The
//! 1280 × 720 frame at 120 % is a 1067 × 600 UI frame.
//!
//! A smaller UI (75 % to 100 %) gives a UI frame larger than the frame:
//! the HUD then covers fewer than the control panel's 47 frame rows, so
//! the world is drawn under them too while it is ([`UiSpace::is_smaller`],
//! [`crate::scene::set_world_under_hud`]).
//!
//! A pointer position maps frame → UI frame at the pixel centre (as
//! [`crate::ui::Presentation`] maps the window to the frame), and back to
//! the first frame pixel of a UI pixel, so a round trip lands on the same
//! UI pixel at every scale.
//!
//! The scale applies only while the native UI pass draws the UI (the
//! frame shown enlarged, `[video] native_ui` on): otherwise the UI is part
//! of the frame's own pixels, which can't scale, and the UI frame is the
//! frame. It is also kept where the UI frame holds the UI's layout box
//! ([`MIN_UI`]), and the 640 × 480 frame (its own layout) never scales.
//!
//! A mod game whose panels are laid out for more rows than the frame has
//! raises the floor ([`set_ui_floor`], from
//! `crate::mods::ClientMods::with_ui_frame_min`): the UI frame never drops
//! below it, so a floor above the play frame draws the UI smaller than the
//! picture at every setting (a 1024 × 768 floor on the 1280 × 720 frame
//! gives a 1366 × 768 UI frame, 93.75 %), and the native UI pass draws it
//! even at 1× ([`floor_forces_native`]).

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU16, Ordering};

use super::geom::Point;
use crate::rules::camera::FrameSize;

/// `[video] ui_scale`, in percent.
pub const UI_SCALE_MIN: u16 = 75;
pub const UI_SCALE_MAX: u16 = 120;
/// The largest `ui_scale` a settings file may hold: earlier builds offered
/// up to 150 %; a value past [`UI_SCALE_MAX`] reads as it.
pub const UI_SCALE_SAVED_MAX: u16 = 150;
pub const UI_SCALE_DEFAULT: u16 = 100;

/// The smallest UI frame the layout holds: the 800-wide UI box, and the
/// 600 rows the side panels are laid out for (their art starts at the top
/// row of an 800 × 600 frame and sits on the control panel; with fewer
/// rows its top is cut off).
pub const MIN_UI: FrameSize = FrameSize {
    width: 800,
    height: 600,
};

static SETTING: AtomicU16 = AtomicU16::new(UI_SCALE_DEFAULT);
static NATIVE: AtomicBool = AtomicBool::new(false);
static FLOOR_W: AtomicI32 = AtomicI32::new(MIN_UI.width);
static FLOOR_H: AtomicI32 = AtomicI32::new(MIN_UI.height);

/// Sets the game's UI frame floor: the smallest UI frame its layout
/// holds ([`MIN_UI`] unless a mod game sets its own, at UI install).
pub fn set_ui_floor(floor: FrameSize) {
    FLOOR_W.store(floor.width.max(1), Ordering::Relaxed);
    FLOOR_H.store(floor.height.max(1), Ordering::Relaxed);
}

/// The game's UI frame floor ([`set_ui_floor`]).
pub fn ui_floor() -> FrameSize {
    FrameSize {
        width: FLOOR_W.load(Ordering::Relaxed),
        height: FLOOR_H.load(Ordering::Relaxed),
    }
}

/// Whether `frame` is too small for the game's UI floor: its UI is then
/// always drawn by the native UI pass, at 1× too, since the UI frame is
/// never the frame. Never for D2Gamma's own floor on its frames.
pub fn floor_forces_native(frame: FrameSize) -> bool {
    let floor = ui_floor();
    frame != FrameSize::LOW && (frame.width < floor.width || frame.height < floor.height)
}

/// The largest UI Scale the settings offer on `frame`: [`UI_SCALE_MAX`],
/// or under a mod game's floor the largest percent whose UI frame still
/// holds the floor (93 for a 1024 × 768 floor on 1280 × 720), never below
/// [`UI_SCALE_MIN`].
pub fn ui_scale_max(frame: FrameSize) -> u16 {
    let floor = ui_floor();
    if floor == MIN_UI {
        return UI_SCALE_MAX;
    }
    let cap = (i64::from(frame.width) * 100 / i64::from(floor.width))
        .min(i64::from(frame.height) * 100 / i64::from(floor.height));
    cap.clamp(i64::from(UI_SCALE_MIN), i64::from(UI_SCALE_MAX)) as u16
}

/// Sets the UI Scale setting (percent; from the settings, at once).
pub fn set_ui_scale(percent: u16) {
    SETTING.store(percent.clamp(UI_SCALE_MIN, UI_SCALE_MAX), Ordering::Relaxed);
}

/// The UI Scale setting, percent.
pub fn ui_scale() -> u16 {
    SETTING.load(Ordering::Relaxed)
}

/// Records whether the native UI pass draws the UI this frame (the game
/// view sets it from the presentation before the UI runs).
pub fn set_native(on: bool) {
    NATIVE.store(on, Ordering::Relaxed);
}

/// The UI space of the play frame now: the setting while the native UI
/// pass draws, else the frame itself.
pub fn current() -> UiSpace {
    let frame = FrameSize::play();
    if NATIVE.load(Ordering::Relaxed) {
        UiSpace::new(frame, ui_scale())
    } else {
        UiSpace::identity(frame)
    }
}

/// The play frame and the UI frame the UI is laid out on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiSpace {
    pub frame: FrameSize,
    pub ui: FrameSize,
}

impl UiSpace {
    /// No scaling: the UI frame is the frame.
    pub const fn identity(frame: FrameSize) -> Self {
        UiSpace { frame, ui: frame }
    }

    /// The UI space of `frame` at `percent` (clamped to the setting's
    /// range) under the game's floor ([`ui_floor`]): [`Self::with_floor`].
    pub fn new(frame: FrameSize, percent: u16) -> Self {
        Self::with_floor(frame, percent, ui_floor())
    }

    /// The UI space of `frame` at `percent` whose UI frame holds `floor`.
    /// On a frame that holds the floor, a larger UI is lowered to the
    /// largest scale whose UI frame still holds it (none at all for a
    /// frame that small, or the 640 × 480 frame); a smaller one always
    /// holds it. A frame smaller than the floor gets at least the floor:
    /// the smallest UI frame of the frame's shape that holds it (1366 ×
    /// 768 for a 1024 × 768 floor on 1280 × 720), whatever the setting.
    pub fn with_floor(frame: FrameSize, percent: u16, floor: FrameSize) -> Self {
        let percent = i64::from(percent.clamp(UI_SCALE_MIN, UI_SCALE_MAX));
        if frame == FrameSize::LOW {
            return Self::identity(frame);
        }
        let (fw, fh) = (i64::from(frame.width), i64::from(frame.height));
        let (mw, mh) = (
            i64::from(floor.width.max(1)),
            i64::from(floor.height.max(1)),
        );
        if fw < mw || fh < mh {
            return Self::under_floor(frame, percent, floor);
        }
        if percent == 100 {
            return Self::identity(frame);
        }
        // The largest percent whose UI frame keeps the box: fw·100/p ≥ mw.
        let max = (fw * 100 / mw).min(fh * 100 / mh);
        let percent = if percent > 100 {
            percent.min(max).max(100)
        } else {
            percent
        };
        if percent == 100 {
            return Self::identity(frame);
        }
        let size = |f: i64| ((f * 100 + percent / 2) / percent) as i32;
        UiSpace {
            frame,
            ui: FrameSize {
                width: size(fw),
                height: size(fh),
            },
        }
    }

    /// [`Self::with_floor`] on a frame smaller than `floor`: the UI frame
    /// at `percent` when it holds the floor, else the floor's fit.
    fn under_floor(frame: FrameSize, percent: i64, floor: FrameSize) -> Self {
        let (fw, fh) = (i64::from(frame.width), i64::from(frame.height));
        let (mw, mh) = (
            i64::from(floor.width.max(1)),
            i64::from(floor.height.max(1)),
        );
        let size = |f: i64| (f * 100 + percent / 2) / percent;
        let (w, h) = if size(fw) >= mw && size(fh) >= mh {
            (size(fw), size(fh))
        } else if mh * fw >= mw * fh {
            // The rows bind: the floor's height, the frame's shape.
            ((fw * mh + fh - 1) / fh, mh)
        } else {
            // The columns bind.
            (mw, (fh * mw + fw - 1) / fw)
        };
        UiSpace {
            frame,
            ui: FrameSize {
                width: w as i32,
                height: h as i32,
            },
        }
    }

    /// Whether the UI frame is the frame.
    pub fn is_identity(&self) -> bool {
        self.ui == self.frame
    }

    /// Whether the UI is drawn smaller than the picture (UI Scale below
    /// 100 %): the HUD then leaves the control panel's frame rows open,
    /// and the world is drawn under it.
    pub fn is_smaller(&self) -> bool {
        self.ui.height > self.frame.height
    }

    /// The scale on each axis, frame pixels per UI pixel.
    pub fn scale(&self) -> (f32, f32) {
        (
            self.frame.width as f32 / self.ui.width.max(1) as f32,
            self.frame.height as f32 / self.ui.height.max(1) as f32,
        )
    }

    /// The UI pixel under frame pixel `p` (its centre), on each axis.
    /// Points off the frame map proportionally.
    pub fn to_ui(&self, p: Point) -> Point {
        if self.is_identity() {
            return p;
        }
        let map = |v: i32, f: i32, u: i32| {
            (((2 * i64::from(v) + 1) * i64::from(u)).div_euclid(2 * i64::from(f.max(1)))) as i32
        };
        Point::new(
            map(p.x, self.frame.width, self.ui.width),
            map(p.y, self.frame.height, self.ui.height),
        )
    }

    /// The first frame pixel of UI pixel `p` on each axis: the inverse of
    /// [`Self::to_ui`] (`to_ui(to_frame(p)) == p`).
    pub fn to_frame(&self, p: Point) -> Point {
        if self.is_identity() {
            return p;
        }
        let first = |v: i32, f: i32, u: i32| {
            crate::ui::frame::first_pixel(v, f.max(1) as u32, u.max(1) as u32) as i32
        };
        Point::new(
            first(p.x, self.frame.width, self.ui.width),
            first(p.y, self.frame.height, self.ui.height),
        )
    }

    /// A frame point as a UI box point: into the UI frame, then the box's
    /// left offset off (the 800-wide box centred in the UI frame).
    pub fn frame_to_box(&self, p: Point) -> Point {
        let u = self.to_ui(p);
        Point::new(u.x - self.ui.ui_left(), u.y)
    }

    /// A UI box point as a frame point ([`Self::frame_to_box`]'s inverse).
    pub fn box_to_frame(&self, p: Point) -> Point {
        self.to_frame(Point::new(p.x + self.ui.ui_left(), p.y))
    }

    /// The frame column where UI column `x` starts.
    pub fn x_to_frame(&self, x: i32) -> i32 {
        self.to_frame(Point::new(x, 0)).x
    }

    /// The frame row where UI row `y` starts.
    pub fn y_to_frame(&self, y: i32) -> i32 {
        self.to_frame(Point::new(0, y)).y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: FrameSize = FrameSize {
        width: 1280,
        height: 720,
    };

    #[test]
    fn the_ui_frame_is_the_frame_divided_by_the_scale_within_the_layout_box() {
        assert_eq!(UiSpace::new(VIEW, 100), UiSpace::identity(VIEW));
        assert_eq!(
            UiSpace::new(VIEW, 110).ui,
            FrameSize {
                width: 1164,
                height: 655
            }
        );
        // The largest scale keeps the panels' 600 rows: 120 % (1067 × 600).
        assert_eq!(
            UiSpace::new(VIEW, UI_SCALE_MAX).ui,
            FrameSize {
                width: 1067,
                height: 600
            }
        );
        // The 600-row frames (1068 × 600, 800 × 600) and 640 × 480 never
        // grow their UI.
        assert!(UiSpace::new(FrameSize::WIDE, 120).is_identity());
        assert!(UiSpace::new(FrameSize::D2RUST, 120).is_identity());
        assert!(UiSpace::new(FrameSize::LOW, 120).is_identity());
        // Out-of-range settings are clamped to the setting's range.
        assert_eq!(UiSpace::new(VIEW, 50), UiSpace::new(VIEW, UI_SCALE_MIN));
        assert_eq!(UiSpace::new(VIEW, 900), UiSpace::new(VIEW, UI_SCALE_MAX));
    }

    /// Every step of the setting's range gives its own UI frame on the
    /// 1280 × 720 frame (no step past a cap that changes nothing), and at
    /// every step on every frame the UI frame keeps the side panels' 600
    /// rows: the panels' art is never moved above the top row.
    #[test]
    fn every_ui_scale_step_changes_the_ui_and_keeps_the_panels_whole() {
        let steps: Vec<FrameSize> = (UI_SCALE_MIN..=UI_SCALE_MAX)
            .step_by(5)
            .map(|p| UiSpace::new(VIEW, p).ui)
            .collect();
        for pair in steps.windows(2) {
            assert!(pair[1].height < pair[0].height, "{pair:?}");
        }
        assert_eq!(steps.len(), 10);
        for frame in [VIEW, FrameSize::WIDE, FrameSize::D2RUST] {
            for percent in UI_SCALE_MIN..=UI_SCALE_SAVED_MAX {
                let ui = UiSpace::new(frame, percent).ui;
                let screen = crate::ui::layout::Screen::for_frame(ui);
                assert!(ui.height >= MIN_UI.height, "{frame:?} {percent}%: {ui:?}");
                assert!(screen.panel_dy() >= 0, "{frame:?} {percent}%: {ui:?}");
            }
        }
    }

    #[test]
    fn a_smaller_ui_lays_out_on_a_larger_ui_frame_and_opens_the_hud_rows() {
        // 75 %: 1280 × 720 / 0.75 = 1707 × 960, the HUD's 47 UI rows on
        // 35 frame rows.
        let small = UiSpace::new(VIEW, 75);
        assert_eq!(
            small.ui,
            FrameSize {
                width: 1707,
                height: 960
            }
        );
        assert!(small.is_smaller() && !small.is_identity());
        assert_eq!(VIEW.height - small.y_to_frame(small.ui.height - 47), 35);
        // Every step below 100 % is kept as asked, on every frame that
        // scales; 100 % and larger never open the HUD rows.
        for percent in (UI_SCALE_MIN..100).step_by(5) {
            for frame in [VIEW, FrameSize::WIDE, FrameSize::D2RUST] {
                let s = UiSpace::new(frame, percent);
                assert!(s.is_smaller(), "{percent}% {frame:?}");
                assert_eq!(
                    s.ui.height,
                    (frame.height * 100 + i32::from(percent) / 2) / i32::from(percent)
                );
            }
        }
        for percent in 100..=UI_SCALE_MAX {
            assert!(!UiSpace::new(VIEW, percent).is_smaller());
        }
        assert!(!UiSpace::new(FrameSize::LOW, 75).is_smaller());
    }

    #[test]
    fn pointer_round_trips_land_on_the_same_ui_pixel_at_every_scale() {
        for percent in (UI_SCALE_MIN..=UI_SCALE_MAX).step_by(5) {
            for frame in [VIEW, FrameSize::WIDE, FrameSize::D2RUST] {
                let s = UiSpace::new(frame, percent);
                for (x, y) in [
                    (0, 0),
                    (5, 7),
                    (s.ui.width / 2, s.ui.height / 3),
                    (s.ui.width - 1, s.ui.height - 1),
                ] {
                    let p = Point::new(x, y);
                    let f = s.to_frame(p);
                    assert!(f.x >= 0 && f.x < frame.width && f.y >= 0 && f.y < frame.height);
                    if s.ui.width <= frame.width {
                        // A UI pixel covers one or more frame pixels: its
                        // first frame pixel maps back to it.
                        assert_eq!(s.to_ui(f), p, "{percent}% {frame:?} {p:?}");
                        let b = Point::new(x - s.ui.ui_left(), y);
                        assert_eq!(s.frame_to_box(s.box_to_frame(b)), b);
                    } else {
                        // A smaller UI: a frame pixel covers UI pixels, and
                        // the one under its centre maps back to it.
                        let q = Point::new(
                            x * frame.width / s.ui.width,
                            y * frame.height / s.ui.height,
                        );
                        assert_eq!(s.to_frame(s.to_ui(q)), q, "{percent}% {frame:?} {q:?}");
                    }
                }
                // Every frame pixel lands inside the UI frame, and the
                // frame's edges on the UI frame's edges.
                for (x, y) in [(0, 0), (frame.width - 1, frame.height - 1)] {
                    let u = s.to_ui(Point::new(x, y));
                    assert!(u.x >= 0 && u.x < s.ui.width && u.y >= 0 && u.y < s.ui.height);
                }
                assert_eq!(s.to_ui(Point::new(frame.width - 1, 0)).x, s.ui.width - 1);
            }
        }
    }

    #[test]
    fn the_scale_only_applies_while_the_native_pass_draws() {
        set_ui_scale(125);
        set_native(false);
        assert!(current().is_identity());
        set_native(true);
        assert_eq!(current(), UiSpace::new(FrameSize::play(), 125));
        set_ui_scale(500);
        assert_eq!(ui_scale(), UI_SCALE_MAX);
        set_ui_scale(UI_SCALE_DEFAULT);
        set_native(false);
    }

    /// A mod game's floor above the play frame (panels laid out for 768
    /// rows on the 1280 × 720 frame): the UI frame is the floor's fit at
    /// every setting that would make it smaller, the setting's own size
    /// when that is larger, and the native pass is forced on.
    #[test]
    fn a_floor_above_the_frame_draws_the_ui_smaller_at_every_setting() {
        let floor = FrameSize {
            width: 1024,
            height: 768,
        };
        let fit = FrameSize {
            width: 1366,
            height: 768,
        };
        for percent in [94, 100, 110, UI_SCALE_MAX] {
            let s = UiSpace::with_floor(VIEW, percent, floor);
            assert_eq!(s.ui, fit, "{percent}%");
            assert!(s.is_smaller());
        }
        assert_eq!(
            UiSpace::with_floor(VIEW, 75, floor).ui,
            FrameSize {
                width: 1707,
                height: 960
            }
        );
        for percent in UI_SCALE_MIN..=UI_SCALE_SAVED_MAX {
            let ui = UiSpace::with_floor(VIEW, percent, floor).ui;
            assert!(
                ui.width >= floor.width && ui.height >= floor.height,
                "{percent}%"
            );
        }
        // A floor wider than the frame's shape binds on its columns.
        let wide = FrameSize {
            width: 1600,
            height: 600,
        };
        assert_eq!(
            UiSpace::with_floor(FrameSize::D2RUST, 100, wide).ui,
            FrameSize {
                width: 1600,
                height: 1200
            }
        );
        // The 640 × 480 frame keeps its own layout.
        assert!(UiSpace::with_floor(FrameSize::LOW, 100, floor).is_identity());

        set_ui_floor(floor);
        assert_eq!(UiSpace::new(VIEW, 100).ui, fit);
        assert!(floor_forces_native(VIEW));
        assert_eq!(ui_scale_max(VIEW), 93);
        set_ui_floor(MIN_UI);
    }

    /// D2Gamma's own floor: the same UI frames as before the floor became
    /// a setting, no forced native pass, the full slider.
    #[test]
    fn the_default_floor_is_the_800_by_600_box() {
        assert_eq!(ui_floor(), MIN_UI);
        for frame in FrameSize::ALL {
            assert!(!floor_forces_native(frame), "{frame:?}");
            assert_eq!(ui_scale_max(frame), UI_SCALE_MAX);
            for percent in UI_SCALE_MIN..=UI_SCALE_SAVED_MAX {
                assert_eq!(
                    UiSpace::new(frame, percent),
                    UiSpace::with_floor(frame, percent, MIN_UI)
                );
            }
        }
        assert_eq!(
            UiSpace::new(VIEW, 110).ui,
            FrameSize {
                width: 1164,
                height: 655
            }
        );
    }
}
