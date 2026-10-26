// D2RUST's own: the settings screens.
//! D2RUST's settings screens and the small widgets beside the game: drawn
//! as UI nodes over the window at its physical resolution, never into the
//! game frame, so frame hashes and the mouse mapping are untouched.
//!
//! - [`text`]: how they draw a string (by role; the font follows the HD
//!   Text setting).
//! - [`fps`]: the FPS counter in the top-left corner (`[hud]
//!   fps_counter`).
//! - [`table`]: the one table of every setting the menus show.
//! - [`info`]: the Display and Picture lines (what the screen shows now).
//! - [`state`]: the menu's state and input rules, without drawing.
//! - [`widgets`]: the widget kit (panels, rows, sliders, switches,
//!   segmented choices, drop-downs, tabs, key caps, buttons).
//! - [`menu`]: the full settings menu, from the main menu and the Esc
//!   menu.
//! - [`overlay`]: the Ctrl+O quick settings overlay over the running game.
//! - [`minimap`]: the minimap in the top-right corner of the world (`[hud]
//!   minimap`, `minimap_size`, `minimap_opacity`).
//!
//! Every size is in view units ([`Metrics`]): one unit is one pixel of the
//! 1280 x 720 game view, so the screens keep their proportions on every
//! monitor and are drawn sharp at its own resolution.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub mod fps;
pub mod info;
pub mod menu;
pub mod minimap;
pub mod overlay;
pub mod state;
pub mod table;
pub mod text;
pub mod widgets;

/// The view the layout is measured in: 1280 x 720 units.
pub const VIEW_W: f32 = 1280.0;
pub const VIEW_H: f32 = 720.0;

/// Draw order of the settings screens' UI over the game's (the dev tools
/// stay on top at 2000).
pub mod z {
    /// The minimap, under the menus.
    pub const MINIMAP: i32 = 900;
    /// The full settings menu.
    pub const MENU: i32 = 1000;
    /// The Ctrl+O overlay.
    pub const OVERLAY: i32 = 1100;
    /// The FPS counter.
    pub const FPS: i32 = 1500;
}

/// How view units map onto the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Output (physical) pixels per view unit: the window's size over the
    /// 1280 x 720 view, the smaller of the two axes (3 at 2160p, 2 at
    /// 1440p, 1.5 at 1080p).
    pub scale: f32,
    /// The window's physical pixels per logical pixel (UI node sizes are
    /// logical).
    pub scale_factor: f32,
}

impl Default for Metrics {
    fn default() -> Self {
        Metrics {
            scale: 1.0,
            scale_factor: 1.0,
        }
    }
}

impl Metrics {
    /// The metrics of a window `w` x `h` physical pixels at `scale_factor`.
    pub fn new(w: u32, h: u32, scale_factor: f32) -> Self {
        let scale = (w as f32 / VIEW_W).min(h as f32 / VIEW_H);
        Metrics {
            scale: if scale > 0.0 { scale } else { 1.0 },
            scale_factor: if scale_factor > 0.0 {
                scale_factor
            } else {
                1.0
            },
        }
    }

    /// The metrics of `window`.
    pub fn of(window: &Window) -> Self {
        Self::new(
            window.resolution.physical_width(),
            window.resolution.physical_height(),
            window.resolution.scale_factor(),
        )
    }

    /// `u` view units as a UI length (logical pixels).
    pub fn px(&self, u: f32) -> Val {
        Val::Px(self.logical(u))
    }

    /// `u` view units in logical pixels.
    pub fn logical(&self, u: f32) -> f32 {
        u * self.scale / self.scale_factor
    }
}

/// The primary window's metrics, when there is one.
pub fn window_metrics(windows: &Query<&Window, With<PrimaryWindow>>) -> Option<Metrics> {
    windows.single().ok().map(Metrics::of)
}

/// Adds the settings screens' systems (once per app).
pub fn add(app: &mut impl crate::app::session::Setup) {
    app.register("settings-ui", |app| {
        app.init_resource::<text::TextCache>()
            .add_systems(Update, text::install_source);
        fps::register(app);
        menu::register(app);
        overlay::register(app);
        minimap::register(app);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_unit_is_the_whole_scale_at_common_monitors() {
        for (w, h, s) in [(3840, 2160, 3.0), (2560, 1440, 2.0), (1920, 1080, 1.5)] {
            assert_eq!(Metrics::new(w, h, 1.0).scale, s);
        }
        // A window narrower than 16:9 follows its width.
        assert_eq!(Metrics::new(1280, 1024, 1.0).scale, 1.0);
        // Logical pixels divide by the scale factor.
        assert_eq!(Metrics::new(3840, 2160, 1.5).logical(10.0), 20.0);
        // A minimized (0 x 0) window keeps a usable scale.
        assert_eq!(Metrics::new(0, 0, 0.0), Metrics::default());
    }
}
