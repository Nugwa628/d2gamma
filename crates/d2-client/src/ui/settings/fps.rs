// D2RUST's own: the FPS counter.
//! The FPS counter (Interface, `[hud] fps_counter`): frames per second and
//! the mean frame time, e.g. "60 FPS - 16.7 ms", in the window's top-left
//! corner, drawn at the window's resolution over everything the game
//! draws, in game and in the menus.
//!
//! The reading is taken the way the perf report takes it
//! (`app::perf_summary`): every frame's interval is its start to the next
//! frame's start (the app's real clock), and the frames per second over a
//! span are its frame count over its length. The counter shows the last
//! whole span of half a second, so it settles instead of flickering, and
//! its mean frame time is that span's length over its frames.
//!
//! The counter only reads the clock: it never changes what the game or
//! its frame does.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::text::{MenuTextSource, TextCache, TextCtx, TextRole};
use super::{window_metrics, z, Metrics};
use crate::app::config::ConfigRes;

/// The span each reading covers, in microseconds.
pub const SPAN_US: u64 = 500_000;

/// The counter's place: its left edge and the middle of its line, view
/// units from the window's top-left.
pub const LEFT: f32 = 14.0;
pub const MIDDLE: f32 = 26.0;
/// The space between the number and "FPS", view units.
pub const GAP: f32 = 6.0;

/// The number's colour (green) and the rest's (light parchment).
pub const NUMBER: [u8; 3] = [150, 220, 130];
pub const REST: [u8; 3] = [225, 216, 198];

/// One reading: the frames per second, rounded, and the mean frame time
/// in tenths of a millisecond, rounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FpsReading {
    pub fps: u32,
    pub tenth_ms: u32,
}

impl FpsReading {
    /// The reading of `frames` frames over `us` microseconds.
    pub fn of(frames: u32, us: u64) -> Option<Self> {
        if frames == 0 || us == 0 {
            return None;
        }
        let fps = (f64::from(frames) * 1e6 / us as f64).round() as u32;
        let tenth_ms = (us as f64 / 100.0 / f64::from(frames)).round() as u32;
        Some(FpsReading { fps, tenth_ms })
    }

    /// The counter's two parts: the number, and the rest of the line.
    pub fn parts(&self) -> (String, String) {
        (
            self.fps.to_string(),
            format!("FPS - {}.{} ms", self.tenth_ms / 10, self.tenth_ms % 10),
        )
    }
}

/// Counts frames into half-second spans.
#[derive(Clone, Debug, Default)]
pub struct FpsMeter {
    span_us: u64,
    frames: u32,
    reading: Option<FpsReading>,
}

impl FpsMeter {
    /// One frame `interval_us` long; a span that is full becomes the
    /// reading.
    pub fn frame(&mut self, interval_us: u64) {
        self.span_us += interval_us;
        self.frames += 1;
        if self.span_us >= SPAN_US {
            self.reading = FpsReading::of(self.frames, self.span_us);
            self.span_us = 0;
            self.frames = 0;
        }
    }

    /// The last whole span's reading; `None` before the first one.
    pub fn reading(&self) -> Option<FpsReading> {
        self.reading
    }
}

/// The counter's state: the meter, and what is on screen.
#[derive(Resource, Default)]
pub struct FpsCounter {
    pub meter: FpsMeter,
    shown: Option<(FpsReading, Metrics, u64)>,
}

/// The counter's root node.
#[derive(Component)]
pub struct FpsRoot;

pub(super) fn register(app: &mut App) {
    app.init_resource::<FpsCounter>()
        .add_systems(Update, fps_counter);
}

/// Feeds the meter every frame and shows the reading while the setting
/// is on.
#[allow(clippy::too_many_arguments)]
fn fps_counter(
    mut commands: Commands,
    time: Res<Time<Real>>,
    cfg: Option<Res<ConfigRes>>,
    mut counter: ResMut<FpsCounter>,
    roots: Query<Entity, With<FpsRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    source: Option<Res<MenuTextSource>>,
    mut cache: ResMut<TextCache>,
    images: Option<ResMut<Assets<Image>>>,
) {
    counter.meter.frame(time.delta().as_micros() as u64);
    let on = cfg.is_some_and(|c| c.settings.fps_counter != 0);
    let (Some(metrics), Some(reading), Some(mut images), true) = (
        window_metrics(&windows),
        counter.meter.reading(),
        images,
        on,
    ) else {
        for e in &roots {
            commands.entity(e).despawn();
        }
        counter.shown = None;
        return;
    };
    let (source, text_key) = super::text::current(source.as_deref());
    let shown = (reading, metrics, text_key);
    let root = roots.iter().next();
    if root.is_some() && counter.shown == Some(shown) {
        return;
    }
    counter.shown = Some(shown);
    let root = root.unwrap_or_else(|| commands.spawn(FpsRoot).id());
    let mut ctx = TextCtx {
        source,
        cache: &mut cache,
        images: &mut images,
        metrics,
    };
    let (number, rest) = reading.parts();
    commands
        .entity(root)
        .despawn_children()
        .insert((
            Node {
                position_type: PositionType::Absolute,
                left: metrics.px(LEFT),
                top: metrics.px(MIDDLE - 10.0),
                height: metrics.px(20.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: metrics.px(GAP),
                ..default()
            },
            GlobalZIndex(z::FPS),
        ))
        .with_children(|p| {
            ctx.spawn(p, TextRole::Label, &number, NUMBER);
            ctx.spawn(p, TextRole::Label, &rest, REST);
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::perf_summary::{Frame, Report, Summary};

    fn meter_of(intervals: &[u32]) -> FpsMeter {
        let mut m = FpsMeter::default();
        for &i in intervals {
            m.frame(u64::from(i));
        }
        m
    }

    #[test]
    fn a_steady_60_hz_reads_60_fps_and_16_7_ms() {
        let m = meter_of(&[16_667; 60]);
        let r = m.reading().expect("a whole span");
        assert_eq!(
            r,
            FpsReading {
                fps: 60,
                tenth_ms: 167
            }
        );
        assert_eq!(r.parts(), ("60".into(), "FPS - 16.7 ms".into()));
    }

    #[test]
    fn nothing_is_read_before_the_first_whole_span() {
        assert_eq!(meter_of(&[16_667; 29]).reading(), None);
        assert!(meter_of(&[16_667; 30]).reading().is_some());
        assert_eq!(FpsReading::of(0, 1), None);
        assert_eq!(FpsReading::of(1, 0), None);
    }

    /// The counter matches the frame-time route (the perf report's
    /// summary over the same frames) within 1 FPS, steady or uneven.
    #[test]
    fn the_counter_matches_the_perf_reports_fps_within_one() {
        let uneven: Vec<u32> = (0..240u32)
            .map(|i| match i % 7 {
                0 => 33_000,
                3 => 9_000,
                _ => 14_500,
            })
            .collect();
        for intervals in [
            vec![16_667; 120],
            vec![6_944; 288],
            vec![41_000; 48],
            uneven,
        ] {
            // Whole spans only, as the counter shows them.
            let mut span = Vec::new();
            let mut us = 0u64;
            for &i in &intervals {
                span.push(i);
                us += u64::from(i);
                if us >= SPAN_US {
                    break;
                }
            }
            let mut t = 0u64;
            let frames = span
                .iter()
                .map(|&interval| {
                    let f = Frame {
                        interval,
                        start_us: t,
                        ..Frame::default()
                    };
                    t += u64::from(interval);
                    f
                })
                .collect();
            let report = Report {
                version: 2,
                frames,
                ..Report::default()
            };
            let route = Summary::of(&report, 50).fps.expect("frames");
            let shown = meter_of(&span).reading().expect("a whole span").fps;
            assert!(
                (f64::from(shown) - route).abs() <= 1.0,
                "counter {shown} vs perf route {route}"
            );
        }
    }

    #[test]
    fn the_counter_shows_only_while_the_setting_is_on() {
        let mut app = App::new();
        app.init_resource::<Time<Real>>()
            .init_resource::<TextCache>()
            .init_resource::<Assets<Image>>()
            .insert_resource(ConfigRes {
                dir: std::env::temp_dir(),
                settings: crate::app::config::Settings::default(),
                bindings: crate::controls::Preset::Original
                    .bindings()
                    .expect("original preset"),
                sound_device: false,
            });
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        register(&mut app);
        // A whole span read already.
        for _ in 0..30 {
            app.world_mut()
                .resource_mut::<FpsCounter>()
                .meter
                .frame(16_667);
        }
        let roots = |app: &mut App| {
            app.world_mut()
                .query_filtered::<Entity, With<FpsRoot>>()
                .iter(app.world())
                .count()
        };
        app.update();
        assert_eq!(roots(&mut app), 0, "off by default");
        app.world_mut()
            .resource_mut::<ConfigRes>()
            .settings
            .fps_counter = 1;
        // Built in `Update`, before the UI is laid out in `PostUpdate`: a
        // new reading is laid out on the frame it appears, not shown
        // blank for one frame.
        app.world_mut().run_schedule(Update);
        assert_eq!(roots(&mut app), 1);
        app.update();
        assert_eq!(roots(&mut app), 1);
        app.world_mut()
            .resource_mut::<ConfigRes>()
            .settings
            .fps_counter = 0;
        app.update();
        assert_eq!(roots(&mut app), 0);
    }
}
