//! A mod's window icon: the picture the window frame and the taskbar show
//! while the mod is played (`files/mod/icon.png`, [`crate::mod_look`]).
//!
//! The exe's own icon is a resource written at build time; a mod cannot
//! change the exe, so the icon is put on each window as it opens. D2Gamma
//! itself (and a mod with no icon) never adds this plugin, so its windows
//! keep the exe's icon exactly as before.

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::WindowCreated;
use bevy::winit::WINIT_WINDOWS;

use crate::mod_menu::Rgba;

/// Puts the mod's icon on every window that opens.
pub struct WindowIconPlugin;

impl Plugin for WindowIconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_icon_on_new_windows);
    }
}

/// The plugin when this run's mod has an icon.
pub fn plugin() -> Option<WindowIconPlugin> {
    crate::mod_look::icon().map(|_| WindowIconPlugin)
}

/// The icon in the form windows take; `None` for a picture the window system
/// refuses (the window keeps the exe's icon).
pub fn window_icon(icon: &Rgba) -> Option<winit::window::Icon> {
    winit::window::Icon::from_rgba(icon.rgba.clone(), icon.w, icon.h).ok()
}

fn set_icon_on_new_windows(mut opened: MessageReader<WindowCreated>, _main_thread: NonSendMarker) {
    let Some(icon) = crate::mod_look::icon().and_then(window_icon) else {
        opened.clear();
        return;
    };
    for e in opened.read() {
        WINIT_WINDOWS.with_borrow(|windows| {
            if let Some(window) = windows.get_window(e.window) {
                window.set_window_icon(Some(icon.clone()));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_becomes_a_window_icon() {
        let icon = Rgba {
            w: 2,
            h: 2,
            rgba: vec![255; 16],
        };
        assert!(window_icon(&icon).is_some());
    }

    #[test]
    fn a_picture_the_window_system_refuses_is_no_icon() {
        // The pixel count does not match the size.
        let bad = Rgba {
            w: 3,
            h: 3,
            rgba: vec![0; 8],
        };
        assert!(window_icon(&bad).is_none());
    }

    #[test]
    fn without_a_mod_there_is_no_icon_plugin() {
        assert!(plugin().is_none());
    }
}
