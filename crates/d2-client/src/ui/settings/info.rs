// D2RUST's own: what the player is really looking at.
//! The Display and Picture lines the settings menu shows under the info
//! panel on its Display and Picture categories, read from the system every
//! frame ([`Facts`]): the monitor (or the window on it), and the game
//! picture with the scale it is shown at.

use super::table::Facts;
use crate::app::config::{ScaleMode, Settings, WindowMode, WINDOW_MODE_NAMES};

fn size_text((w, h): (u32, u32)) -> String {
    format!("{w}x{h}")
}

/// The monitor as "3840x2160 at 144 Hz" (the rate left out when unknown).
fn monitor_text(monitor: Option<(u32, u32)>, refresh_hz: Option<u32>) -> Option<String> {
    let m = size_text(monitor?);
    Some(match refresh_hz {
        Some(hz) => format!("{m} at {hz} Hz"),
        None => m,
    })
}

/// The first line: the monitor and how the window sits on it.
/// - Windowed: "Display: Window 1920x1080 on 3840x2160 at 144 Hz", the
///   window's real size in physical pixels;
/// - Windowed Borderless and Fullscreen, which cover the monitor:
///   "Display: 3840x2160 at 144 Hz, Fullscreen".
///
/// `mode` is the mode the window is in; a part the platform hasn't
/// reported is left out.
pub fn display_line(
    monitor: Option<(u32, u32)>,
    refresh_hz: Option<u32>,
    mode: WindowMode,
    window: Option<(u32, u32)>,
) -> String {
    let screen = monitor_text(monitor, refresh_hz);
    match (mode, window, screen) {
        (WindowMode::Windowed, Some(w), Some(s)) => {
            format!("Display: Window {} on {s}", size_text(w))
        }
        (WindowMode::Windowed, Some(w), None) => format!("Display: Window {}", size_text(w)),
        (_, w, s) => {
            let s = s.or_else(|| w.map(size_text)).unwrap_or_else(|| "?".into());
            let name = WindowMode::ALL
                .iter()
                .position(|&m| m == mode)
                .map_or("", |i| WINDOW_MODE_NAMES[i]);
            format!("Display: {s}, {name}")
        }
    }
}

/// A scale factor as the line shows it: "2x" when whole, else up to two
/// decimals ("1.8x", "1.28x").
pub fn scale_text(scale: f64) -> String {
    let hundredths = (scale * 100.0).round() as i64;
    let (whole, frac) = (hundredths / 100, hundredths % 100);
    match frac {
        0 => format!("{whole}x"),
        f if f % 10 == 0 => format!("{whole}.{}x", f / 10),
        f => format!("{whole}.{f:02}x"),
    }
}

/// The second line: the game frame, the scale it is shown at in the
/// window and how ("Sharp" for Fit, whose sharp scaler keeps every game
/// pixel a solid block; "Integer"), e.g. "Picture: 1280x720 shown at 3x
/// (Integer)". A window too small for Integer shows the picture as Fit
/// does, and the line says so.
pub fn picture_line(frame: (u32, u32), window: Option<(u32, u32)>, mode: ScaleMode) -> String {
    let mut out = format!("Picture: {}", size_text(frame));
    if let Some(p) =
        window.and_then(|(w, h)| crate::ui::edge::present_in(w, h, frame.0, frame.1, mode).ok())
    {
        let scale = f64::from(p.image_w) / f64::from(p.frame_w);
        let scale = scale.min(f64::from(p.image_h) / f64::from(p.frame_h));
        let how = match p.mode {
            ScaleMode::Fit => "Sharp",
            ScaleMode::Integer => "Integer",
        };
        out.push_str(&format!(" shown at {} ({how})", scale_text(scale)));
    }
    out
}

/// Both lines for settings `s` on the machine `f`: the window's own mode
/// when the platform reported it (Alt+Enter), and the frame in force (the
/// one player view until it is reported).
pub fn lines(s: &Settings, f: &Facts) -> [String; 2] {
    let w = &f.window;
    let mode = w.mode.unwrap_or(s.window_mode);
    let view = crate::rules::camera::FrameSize::PLAY;
    let frame = w.frame.unwrap_or((view.width as u32, view.height as u32));
    [
        display_line(f.monitor, f.refresh_hz, mode, w.size),
        picture_line(frame, w.size, s.scale_mode),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::settings::table::WindowFacts;

    #[test]
    fn the_display_line_shows_the_monitor_and_the_window_on_it() {
        use crate::app::config::WindowMode::*;
        let uhd = Some((3840, 2160));
        assert_eq!(
            display_line(uhd, Some(144), Fullscreen, Some((3840, 2160))),
            "Display: 3840x2160 at 144 Hz, Fullscreen"
        );
        assert_eq!(
            display_line(uhd, Some(144), WindowedBorderless, None),
            "Display: 3840x2160 at 144 Hz, Windowed Borderless"
        );
        assert_eq!(
            display_line(uhd, Some(144), Windowed, Some((1920, 1080))),
            "Display: Window 1920x1080 on 3840x2160 at 144 Hz"
        );
        // What the platform hasn't reported is left out.
        assert_eq!(
            display_line(uhd, None, Windowed, Some((1920, 1080))),
            "Display: Window 1920x1080 on 3840x2160"
        );
        assert_eq!(
            display_line(None, None, Windowed, Some((1920, 1080))),
            "Display: Window 1920x1080"
        );
        assert_eq!(
            display_line(None, None, Fullscreen, Some((2560, 1440))),
            "Display: 2560x1440, Fullscreen"
        );
        assert_eq!(
            display_line(None, None, Windowed, None),
            "Display: ?, Windowed"
        );
    }

    #[test]
    fn the_picture_line_shows_the_frame_its_scale_and_the_scaler() {
        use crate::app::config::ScaleMode::*;
        let play = (1280, 720);
        assert_eq!(
            picture_line(play, Some((3840, 2160)), Fit),
            "Picture: 1280x720 shown at 3x (Sharp)"
        );
        assert_eq!(
            picture_line(play, Some((1920, 1080)), Fit),
            "Picture: 1280x720 shown at 1.5x (Sharp)"
        );
        assert_eq!(
            picture_line(play, Some((2560, 1440)), Fit),
            "Picture: 1280x720 shown at 2x (Sharp)"
        );
        assert_eq!(
            picture_line((800, 600), Some((3840, 2160)), Integer),
            "Picture: 800x600 shown at 3x (Integer)"
        );
        assert_eq!(
            picture_line((800, 600), Some((1366, 768)), Fit),
            "Picture: 800x600 shown at 1.28x (Sharp)"
        );
        // Too small for Integer: shown as Fit is, and said so.
        assert_eq!(
            picture_line((800, 600), Some((640, 480)), Integer),
            "Picture: 800x600 shown at 0.8x (Sharp)"
        );
        // No window yet: the frame alone.
        assert_eq!(picture_line(play, None, Fit), "Picture: 1280x720");
        assert_eq!(scale_text(2.0), "2x");
        assert_eq!(scale_text(3.5955), "3.6x");
        assert_eq!(scale_text(1.28), "1.28x");
        assert_eq!(scale_text(1.05), "1.05x");
    }

    fn facts(mode: WindowMode, size: (u32, u32)) -> Facts {
        Facts {
            monitor: Some((3840, 2160)),
            refresh_hz: Some(144),
            window: WindowFacts {
                mode: Some(mode),
                size: Some(size),
                frame: Some((1068, 600)),
            },
            ..Facts::default()
        }
    }

    #[test]
    fn the_lines_follow_the_window_and_the_settings() {
        let mut s = Settings::default();
        let mut f = facts(WindowMode::Fullscreen, (3840, 2160));
        assert_eq!(
            lines(&s, &f),
            [
                "Display: 3840x2160 at 144 Hz, Fullscreen",
                "Picture: 1068x600 shown at 3.6x (Sharp)"
            ]
        );
        // Alt+Enter to a window, and Integer picked: both lines follow.
        f = facts(WindowMode::Windowed, (2560, 1377));
        s.scale_mode = ScaleMode::Integer;
        assert_eq!(
            lines(&s, &f),
            [
                "Display: Window 2560x1377 on 3840x2160 at 144 Hz",
                "Picture: 1068x600 shown at 2x (Integer)"
            ]
        );
        // No frame reported yet: the one player view (2x needs 1440 rows).
        f.window.frame = None;
        assert_eq!(lines(&s, &f)[1], "Picture: 1280x720 shown at 1x (Integer)");
        // No mode reported: the setting's.
        f.window.mode = None;
        s.window_mode = WindowMode::WindowedBorderless;
        assert_eq!(
            lines(&s, &f)[0],
            "Display: 3840x2160 at 144 Hz, Windowed Borderless"
        );
    }
}
