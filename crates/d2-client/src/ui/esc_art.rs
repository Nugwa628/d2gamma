//! The DC6 art of the Esc menu: which image each row's label is, and the
//! draw list of the menu (labels, `pentspin` pentagrams). The files go
//! through the same [`UiFiles`] / `archive_name` path as every other panel
//! image (no second loader); label images live under
//! `data\local\ui\eng\` ([`LOCAL_PREFIX`]), the pentagram under
//! `data\global\ui\`.
//!
//! The widths are fixed: the game reads them from the DC6 headers;
//! multi-frame images tile at 256 px.
//!
//! D2RUST's own, unverified: the `pentspin` frame follows the client clock
//! ([`PentClock`]: the game's > 50 ms rule).

use d2_sim::tick::TickRate;

use super::options_menu::{OptionsMenu, Row, ROWS};
use crate::ui::draw::{CelLook, ImageRef, ImageRequest, Remap, UiDraw, UiDrawSink};
use crate::ui::geom::{Point, Rect};
use crate::ui::panels::UiFiles;

/// Prefix of a `UiFiles` name under `data\local\ui\eng\`.
pub const LOCAL_PREFIX: &str = "*local\\";
const PENTSPIN: &str = "cursor\\pentspin";

/// Widths of the `pentspin` frames.
pub const PENT_WIDTHS: [i32; 8] = [51, 43, 27, 9, 23, 40, 50, 52];

/// An image and its total width.
pub type Art = (&'static str, i32);

/// The label image of a row.
pub fn label(row: Row) -> Art {
    match row {
        Row::Options => ("options", 160),
        Row::ExitGame => ("exit", 434),
        Row::ReturnToGame => ("returntogame", 356),
    }
}

/// Every file the menu names (lowercase `UiFiles` names).
pub fn esc_files() -> Vec<String> {
    std::iter::once(PENTSPIN.to_string())
        .chain(ROWS.iter().map(|d| local(label(d.row).0)))
        .collect()
}

/// Whether `name` is one of the menu's files (a missing one draws empty).
pub fn is_esc_file(name: &str) -> bool {
    esc_files().iter().any(|f| f == name)
}

/// Frames of an image `w` px wide (tiled at 256 px).
fn frames(w: i32) -> i32 {
    (w + 255) / 256
}

/// Every frame of `name` from cel position (x, y), drawn normally (mode 5).
fn art(files: &UiFiles, out: &mut dyn UiDrawSink, clip: Rect, name: &str, w: i32, x: i32, y: i32) {
    let Some(file) = files.id(name) else { return };
    for k in 0..frames(w) {
        out.push(UiDraw::Image(ImageRequest {
            image: ImageRef {
                file,
                frame: k as u32,
            },
            at: Point::new(x + 256 * k, y),
            clip,
            look: CelLook {
                mode: 5,
                remap: Remap::None,
            },
            call: crate::ui::draw::CelCall::Draw,
        }));
    }
}

fn local(name: &str) -> String {
    format!("{LOCAL_PREFIX}{name}")
}

/// The `pentspin` counter: it advances by one,
/// modulo 8, once per draw at which more than 50 ms have passed since the
/// last advance, after that draw's frames are drawn (the stamp starts at
/// zero, so the first draw advances at once but still draws frame 0). In
/// live play the clock is the wall clock; check runs use the server tick's
/// real time at the game's tick rate ([`Self::ms_of_tick`]). A draw every
/// 40 ms steps every 2nd draw, one every 16.67 ms every 4th.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PentClock {
    frame: u32,
    last_ms: Option<u64>,
}

impl PentClock {
    /// One draw at `now_ms`: returns the frame to draw, then advances when
    /// > 50 ms passed.
    pub fn draw(&mut self, now_ms: u64) -> u32 {
        let drawn = self.frame;
        if self
            .last_ms
            .is_none_or(|last| now_ms.saturating_sub(last) > 50)
        {
            self.frame = (self.frame + 1) % 8;
            self.last_ms = Some(now_ms);
        }
        drawn
    }

    /// The real time of server tick `tick` at the game's tick rate `rate`
    /// in milliseconds (rounded down): [`CLIENT_TICK_MS`] per base-rate
    /// tick, so 40 ms a tick at 25 Hz and 16.67 ms at 60 Hz.
    ///
    /// [`CLIENT_TICK_MS`]: crate::rules::camera::CLIENT_TICK_MS
    pub fn ms_of_tick(tick: u64, rate: TickRate) -> u64 {
        rate.progress(u64::from(crate::rules::camera::CLIENT_TICK_MS), tick)
    }
}

/// Draws the label image of row `i`, centred; `false`: the image is
/// missing and the caller draws its text.
pub fn draw_row(files: &UiFiles, m: &OptionsMenu, i: usize, out: &mut dyn UiDrawSink) -> bool {
    let (name, w) = label(m.rows()[i].row);
    let name = local(name);
    if files.id(&name).is_none() {
        return false;
    }
    let (half, clip) = (m.half(), m.screen.rect());
    art(
        files,
        out,
        clip,
        &name,
        w,
        half - 1 - (w >> 1),
        m.baseline(i),
    );
    true
}

/// The pentagrams at the selected row: the left one spins the
/// other way, drawn right-aligned in its 52 px cell.
pub fn draw_pents(files: &UiFiles, m: &OptionsMenu, f: u32, out: &mut dyn UiDrawSink) {
    let Some(file) = files.id(PENTSPIN) else {
        return;
    };
    let (half, clip) = (m.half(), m.screen.rect());
    let left = if f == 0 { 0 } else { 8 - f };
    let y = m.pentagram_y();
    for (frame, x) in [(left, half - 52 - 249), (f, half + 249)] {
        out.push(UiDraw::Image(ImageRequest {
            image: ImageRef { file, frame },
            at: Point::new(x, y),
            clip,
            look: crate::ui::CelLook::PLAIN,
            call: crate::ui::draw::CelCall::Draw,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> UiFiles {
        let mut f = UiFiles::new(&[]);
        f.extend(esc_files());
        f
    }

    /// (name, frame, x, y) of every image drawn.
    fn draws(f: &UiFiles, m: &OptionsMenu, frame: u32) -> Vec<(String, u32, i32, i32)> {
        let mut out: Vec<UiDraw> = Vec::new();
        for i in 0..m.rows().len() {
            assert!(draw_row(f, m, i, &mut out));
        }
        draw_pents(f, m, frame, &mut out);
        out.iter()
            .filter_map(|d| match d {
                UiDraw::Image(r) => Some((
                    f.name(r.image.file).unwrap().to_string(),
                    r.image.frame,
                    r.at.x,
                    r.at.y,
                )),
                _ => None,
            })
            .collect()
    }

    // D2RUST's own, unverified: draw list against the spec's positions
    #[test]
    fn game_menu_draws_centred_labels_and_pentspin_at_the_selected_row() {
        let f = files();
        let mut m = OptionsMenu::default();
        m.open();
        let d = draws(&f, &m, 0);
        // 160 / 434 / 356 px labels centred at 400 − 1 − w/2, baselines 224 / 274 / 324.
        assert_eq!(d[0], ("*local\\options".into(), 0, 319, 224));
        assert_eq!(d[1], ("*local\\exit".into(), 0, 182, 274));
        assert_eq!(d[2], ("*local\\exit".into(), 1, 438, 274));
        assert_eq!(d[3], ("*local\\returntogame".into(), 0, 221, 324));
        assert_eq!(d[4], ("*local\\returntogame".into(), 1, 477, 324));
        // Return to Game selected: pentagrams at (99, 336), (649, 336).
        assert_eq!(d[5], ("cursor\\pentspin".into(), 0, 99, 336));
        assert_eq!(d[6], ("cursor\\pentspin".into(), 0, 649, 336));
        // Later frame: the left one spins the other way.
        let d = draws(&f, &m, 3);
        assert_eq!((d[5].1, d[6].1), (5, 3));
    }

    #[test]
    fn pentagrams_and_clip_follow_the_screen() {
        use crate::ui::layout::Screen;
        let f = files();
        for (screen, h, left_y) in [(Screen::R800, 400, 336), (Screen::R640, 320, 276)] {
            let mut m = OptionsMenu::default();
            m.screen = screen;
            m.open();
            let mut out: Vec<UiDraw> = Vec::new();
            draw_pents(&f, &m, 0, &mut out);
            let at: Vec<(i32, i32)> = out
                .iter()
                .filter_map(|d| match d {
                    UiDraw::Image(r) => {
                        assert_eq!(r.clip, screen.rect());
                        Some((r.at.x, r.at.y))
                    }
                    _ => None,
                })
                .collect();
            // Pentagrams at h − 301 and h + 249; Game menu, Return to Game
            // selected: y_top + 51 (800: 99 / 649 at y 336, spec).
            let y = m.pentagram_y();
            assert_eq!(y, left_y);
            assert_eq!(at, [(h - 301, y), (h + 249, y)]);
        }
    }

    #[test]
    fn the_pentagram_steps_when_more_than_50_ms_passed() {
        // 25 Hz draws (40 ms): the first draw steps at once, then every 2nd.
        let mut c = PentClock::default();
        let frames: Vec<u32> = (0..7)
            .map(|i| c.draw(PentClock::ms_of_tick(i, TickRate::BASE)))
            .collect();
        assert_eq!(frames, [0, 1, 1, 2, 2, 3, 3]);
        // 60 Hz draws (16.67 ms): every 4th.
        let mut c = PentClock::default();
        let frames: Vec<u32> = (0..9).map(|i| c.draw(i * 50 / 3)).collect();
        assert_eq!(frames, [0, 1, 1, 1, 1, 2, 2, 2, 2]);
        // The counter wraps at 8.
        let mut c = PentClock::default();
        assert_eq!((0..40).map(|i| c.draw(i * 60)).last(), Some(39 % 8));
    }

    /// The server-tick clock runs in real time at the game's 60 Hz: a tick
    /// is 16.67 ms, not the base rate's 40 ms (which spun the pentagram
    /// 2.4× too fast), so a draw per tick steps every 4th tick.
    #[test]
    fn the_server_tick_clock_is_real_time_at_60_hz() {
        let hz60 = TickRate::new(60).unwrap();
        assert_eq!(PentClock::ms_of_tick(0, hz60), 0);
        assert_eq!(PentClock::ms_of_tick(3, hz60), 50);
        assert_eq!(PentClock::ms_of_tick(4, hz60), 66);
        assert_eq!(PentClock::ms_of_tick(60, hz60), 1_000);
        assert_eq!(PentClock::ms_of_tick(25, TickRate::BASE), 1_000);
        let mut c = PentClock::default();
        let frames: Vec<u32> = (0..9)
            .map(|i| c.draw(PentClock::ms_of_tick(i, hz60)))
            .collect();
        assert_eq!(frames, [0, 1, 1, 1, 1, 2, 2, 2, 2]);
        // One second of 60 Hz ticks: 15 steps (every 66.7 ms), not the 30
        // of a 40 ms tick.
        let mut c = PentClock::default();
        let mut steps = 0;
        let mut last = None;
        for t in 1..=60 {
            let f = c.draw(PentClock::ms_of_tick(t, hz60));
            if last.is_some_and(|l| l != f) {
                steps += 1;
            }
            last = Some(f);
        }
        assert_eq!(steps, 15, "steps seen over one second of draws");
    }
}
