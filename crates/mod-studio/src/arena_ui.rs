//! The test arena panel of the monster AI tab: the options, the run, and
//! what the run showed (a strip of the states over time, the events, and
//! the ticks each state of the graph ran for).
use crate::ai::Entry;
use crate::arena::{marks, occupancy, stretches, ArenaOptions, ArenaRun, Stretch};
use crate::kit::Names;
use crate::kit_ui::{drag, labelled, pick, RED};
use d2_sim::rules::data_mod::ai::trace::Trace;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use std::path::Path;
use std::time::Duration;

const WINDUP: Color32 = Color32::from_rgb(0xe0, 0x6a, 0x4c);
const PHASE: Color32 = Color32::from_rgb(0xe0, 0xa4, 0x4c);
const OUTSIDE: Color32 = Color32::from_rgb(0x4a, 0x4a, 0x48);
const STRIP_HEIGHT: f32 = 26.0;

/// What a run needs from Studio's settings and the open project.
#[derive(Clone, Copy)]
pub struct ArenaTools<'a> {
    pub game: &'a Path,
    pub native: &'a Path,
    pub root: &'a Path,
}

impl ArenaTools<'_> {
    /// Why a run cannot start, if it cannot.
    pub fn missing(&self) -> Option<&'static str> {
        if self.game.as_os_str().is_empty() {
            Some("Set the D2Gamma-Game executable in the settings to run the arena.")
        } else if self.native.as_os_str().is_empty() {
            Some("Set the converted native install in the settings to run the arena.")
        } else {
            None
        }
    }
}

#[derive(Default)]
pub struct ArenaPanel {
    options: ArenaOptions,
    class: u16,
    run: Option<ArenaRun>,
    /// The last trace, and the index of the graph it was of.
    pub result: Option<Trace>,
    error: String,
}

impl ArenaPanel {
    pub fn running(&self) -> bool {
        self.run.is_some()
    }

    /// Starts a run of the graph `ai`; the caller has saved the file.
    pub fn start(&mut self, tools: &ArenaTools, ai: u16) {
        self.error.clear();
        let mut options = self.options.clone();
        options.class = u32::from(self.class);
        self.run = Some(ArenaRun::start(
            tools.game.into(),
            tools.root.into(),
            tools.native.into(),
            options,
            ai,
        ));
    }

    /// Takes the result of a finished run.
    pub fn poll(&mut self) {
        let Some(done) = self.run.as_ref().and_then(ArenaRun::poll) else {
            return;
        };
        self.run = None;
        match done {
            Ok(trace) => {
                self.result = Some(trace);
                self.error.clear();
            }
            Err(e) => self.error = e,
        }
    }

    /// Ticks each state of `e` ran for in the last trace, if it is of this
    /// graph as it is now.
    pub fn ticks_for(&self, e: &Entry) -> Option<Vec<u32>> {
        occupancy(self.result.as_ref()?, e.index, &e.state_names())
    }

    /// Draws the panel; true when the run button was pressed (the caller saves
    /// the file and calls [`ArenaPanel::start`]).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        names: &Names,
        tools: &ArenaTools,
        e: &Entry,
        blocked: Option<&str>,
    ) -> bool {
        self.poll();
        let mut run = false;
        ui.strong("Test arena");
        ui.weak("Spawns a monster beside a test character and records what the graph does.");
        labelled(ui, "Monster", |ui| {
            pick(ui, "arena-class", &mut self.class, &names.monsters)
        });
        labelled(ui, "Ticks", |ui| {
            drag(ui, &mut self.options.ticks, 1..=20_000)
        });
        labelled(ui, "Seed", |ui| {
            drag(ui, &mut self.options.seed, 0..=u32::MAX)
        });
        labelled(ui, "Distance (sub-tiles)", |ui| {
            drag(ui, &mut self.options.distance, 1..=2000)
        });
        labelled(ui, "Level", |ui| {
            let c = drag(ui, &mut self.options.level, 0..=136);
            ui.weak("0 stays in town, where monsters do not fight");
            c
        });
        ui.horizontal(|ui| {
            let why = tools.missing().or(blocked);
            let button = ui.add_enabled(
                why.is_none() && !self.running(),
                egui::Button::new("Run in arena"),
            );
            if button.clicked() {
                run = true;
            }
            if let Some(why) = why {
                ui.weak(why);
            }
            if let Some(r) = &self.run {
                ui.spinner();
                ui.label(format!("running {:.0}s", r.started.elapsed().as_secs_f32()));
                if ui.button("Cancel").clicked() {
                    r.cancel();
                }
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            }
        });
        if !self.error.is_empty() {
            ui.colored_label(RED, &self.error);
        }
        if let Some(t) = &self.result {
            ui.separator();
            result_view(ui, t, self.ticks_for(e).is_some());
        }
        run
    }
}

/// A colour for state number `n`; the same state always gets the same one.
pub fn state_color(n: Option<u8>) -> Color32 {
    let Some(n) = n else { return OUTSIDE };
    let hue = (f32::from(n) * 0.381_966).fract();
    egui::ecolor::Hsva::new(hue, 0.55, 0.85, 1.0).into()
}

/// Where each stretch lies on a strip `width` wide: (left, right, state).
/// Ticks count from 1.
pub fn strip_spans(stretches: &[Stretch], total: u32, width: f32) -> Vec<(f32, f32, Option<u8>)> {
    let per = width / total.max(1) as f32;
    stretches
        .iter()
        .map(|s| {
            (
                s.from.saturating_sub(1) as f32 * per,
                s.to as f32 * per,
                s.state,
            )
        })
        .collect()
}

fn result_view(ui: &mut egui::Ui, t: &Trace, current: bool) {
    if !current {
        ui.colored_label(
            RED,
            "This trace is of another graph, or of this one before it changed. Run again.",
        );
    }
    ui.label(format!(
        "{} (class {}), seed {}, {} ticks",
        t.monster, t.class, t.seed, t.ticks
    ));
    ui.add(egui::Label::new(egui::RichText::new(t.summary_text()).monospace()).wrap());
    let width = ui.available_width().clamp(200.0, 1200.0);
    let (response, painter) =
        ui.allocate_painter(Vec2::new(width, STRIP_HEIGHT + 8.0), Sense::hover());
    let top = response.rect.min + Vec2::new(0.0, 8.0);
    let spans = strip_spans(&stretches(t), t.ticks, width);
    for (l, r, state) in &spans {
        painter.rect_filled(
            Rect::from_min_max(top + Vec2::new(*l, 0.0), top + Vec2::new(*r, STRIP_HEIGHT)),
            0.0,
            state_color(*state),
        );
    }
    let per = width / t.ticks.max(1) as f32;
    for (tick, is_phase) in marks(t) {
        let x = response.rect.min.x + tick.saturating_sub(1) as f32 * per;
        let colour = if is_phase { PHASE } else { WINDUP };
        painter.line_segment(
            [
                Pos2::new(x, response.rect.min.y),
                Pos2::new(x, top.y + STRIP_HEIGHT),
            ],
            Stroke::new(1.5_f32, colour),
        );
    }
    ui.horizontal_wrapped(|ui| {
        for (i, name) in t.states.iter().enumerate() {
            let state = u8::try_from(i).ok();
            ui.colored_label(state_color(state), format!("■ {name}"));
        }
        ui.colored_label(OUTSIDE, "■ outside the graph");
        ui.colored_label(WINDUP, "| wind-up");
        ui.colored_label(PHASE, "| phase");
    });
    egui::CollapsingHeader::new(format!("{} events", t.events.len()))
        .id_salt("arena-events")
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .id_salt("arena-event-list")
                .show(ui, |ui| {
                    for ev in &t.events {
                        ui.monospace(t.describe(ev));
                    }
                });
        });
}
