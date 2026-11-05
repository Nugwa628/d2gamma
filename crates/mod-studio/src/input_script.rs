//! Black-box Studio input playback. Scripts can send input and inspect painted
//! text; they cannot access editor models or write project files.
use anyhow::{bail, Context as _, Result};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, Rect};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "do", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    Click {
        text: String,
        #[serde(default)]
        nth: usize,
        #[serde(default)]
        offset: [f32; 2],
    },
    Fill {
        text: String,
        value: String,
        #[serde(default)]
        nth: usize,
        #[serde(default = "field_offset")]
        offset: [f32; 2],
    },
    ClickAt {
        point: [f32; 2],
    },
    Type {
        value: String,
    },
    Key {
        key: String,
        #[serde(default)]
        command: bool,
    },
    Scroll {
        point: [f32; 2],
        delta: [f32; 2],
    },
    ScrollTo {
        text: String,
        point: [f32; 2],
        delta: [f32; 2],
        #[serde(default = "scroll_limit")]
        max_scrolls: usize,
    },
    WaitText {
        text: String,
        #[serde(default = "timeout")]
        seconds: u64,
    },
    Capture {
        name: String,
    },
    Wait {
        frames: usize,
    },
}
fn field_offset() -> [f32; 2] {
    [110.0, 0.0]
}
fn scroll_limit() -> usize {
    32
}
fn timeout() -> u64 {
    120
}

#[derive(Clone, Debug, Serialize)]
pub struct PaintedText {
    pub text: String,
    pub bounds: [f32; 4],
    #[serde(skip)]
    foreground: bool,
}
impl PaintedText {
    fn center(&self) -> Pos2 {
        Pos2::new(
            (self.bounds[0] + self.bounds[2]) * 0.5,
            (self.bounds[1] + self.bounds[3]) * 0.5,
        )
    }
}

/// Observe the actual painted window, including clipping. No widget is activated
/// directly: targets become ordinary pointer and keyboard events next frame.
pub fn painted_text(ctx: &egui::Context) -> Vec<PaintedText> {
    fn collect(shape: &egui::Shape, clip: Rect, out: &mut Vec<PaintedText>) {
        match shape {
            egui::Shape::Text(text) => {
                // Wrapped labels may include a leading indentation in `rect`.
                // Hit the painted glyphs, rather than that empty layout space.
                let bounds = if text.galley.mesh_bounds.is_positive() {
                    text.galley.mesh_bounds
                } else {
                    text.galley.rect
                };
                let rect = bounds.translate(text.pos.to_vec2());
                if clip.contains(rect.center()) {
                    out.push(PaintedText {
                        text: text.galley.text().into(),
                        bounds: [rect.min.x, rect.min.y, rect.max.x, rect.max.y],
                        foreground: false,
                    });
                }
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, clip, out);
                }
            }
            _ => {}
        }
    }
    let shapes = ctx.graphics(|g| g.clone().drain(&[], &Default::default()));
    let mut out = Vec::new();
    for shape in shapes {
        collect(&shape.shape, shape.clip_rect, &mut out);
    }
    // Menu choices can have the same label as the selected control underneath.
    // Preserve normal top-to-bottom targeting within each interaction layer.
    let layers = ctx.memory(|m| {
        m.layer_ids()
            .filter(|l| l.order == egui::Order::Foreground)
            .collect::<Vec<_>>()
    });
    let foreground = ctx.graphics(|g| {
        let mut texts = Vec::new();
        for layer in layers {
            if let Some(list) = g.get(layer) {
                for shape in list.all_entries() {
                    collect(&shape.shape, shape.clip_rect, &mut texts);
                }
            }
        }
        texts
    });
    for text in &mut out {
        text.foreground = foreground
            .iter()
            .any(|p| p.text == text.text && p.bounds == text.bounds);
    }
    out.sort_by(|a, b| {
        a.bounds[1]
            .total_cmp(&b.bounds[1])
            .then(a.bounds[0].total_cmp(&b.bounds[0]))
    });
    out
}

pub struct Runner {
    script: Script,
    output: PathBuf,
    pub step: usize,
    queue: VecDeque<Vec<Event>>,
    quiet: usize,
    scrolls: usize,
    since: Instant,
    observed_time: f64,
    ready_at: f64,
    visible: Vec<PaintedText>,
    capture: Option<String>,
    pub finished: bool,
    pub error: Option<String>,
}
impl Runner {
    pub fn visible(&self) -> &[PaintedText] {
        &self.visible
    }
    pub fn load(path: &Path, output: &Path) -> Result<Self> {
        let script =
            serde_json::from_slice(&std::fs::read(path)?).context("invalid Studio input script")?;
        Self::new(script, output)
    }
    pub fn new(script: Script, output: &Path) -> Result<Self> {
        std::fs::create_dir_all(output)?;
        for step in &script.steps {
            if let Step::Capture { name } = step {
                if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
                    bail!("capture name must be a file name");
                }
            }
            if let Step::Key { key, .. } = step {
                parse_key(key)?;
            }
        }
        Ok(Self {
            script,
            output: output.into(),
            step: 0,
            queue: VecDeque::new(),
            quiet: 3,
            scrolls: 0,
            since: Instant::now(),
            observed_time: 0.,
            ready_at: 0.,
            visible: Vec::new(),
            capture: None,
            finished: false,
            error: None,
        })
    }
    fn point(&self, text: &str, nth: usize, offset: [f32; 2]) -> Option<Pos2> {
        self.visible
            .iter()
            .filter(|p| p.text == text)
            .filter(|p| {
                !self
                    .visible
                    .iter()
                    .any(|other| other.text == text && other.foreground)
                    || p.foreground
            })
            .nth(nth)
            .map(|p| p.center() + egui::vec2(offset[0], offset[1]))
    }
    fn click(&mut self, point: Pos2) {
        self.queue.push_back(vec![
            Event::PointerMoved(point),
            Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        self.queue.push_back(vec![Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
    }
    fn advance(&mut self) {
        self.step += 1;
        self.scrolls = 0;
        self.quiet = 3;
        self.since = Instant::now();
        self.ready_at = self.observed_time + 0.05;
    }
    pub fn input(&mut self, raw: &mut egui::RawInput) {
        if let Some(events) = self.queue.pop_front() {
            raw.events.extend(events);
        }
    }
    fn finish(&mut self, error: Option<String>) -> Result<()> {
        self.error = error;
        self.finished = true;
        std::fs::write(
            self.output.join("result.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"completed_steps":self.step,"total_steps":self.script.steps.len(),"error":self.error,"visible":self.visible}),
            )?,
        )?;
        Ok(())
    }
    pub fn observe(&mut self, ctx: &egui::Context) -> Result<()> {
        self.observed_time = ctx.input(|i| i.time);
        self.visible = painted_text(ctx);
        if self.finished {
            return Ok(());
        }
        if let Some(name) = &self.capture {
            for event in ctx.input(|i| i.events.clone()) {
                if let Event::Screenshot { image, .. } = event {
                    let bytes: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    image::save_buffer(
                        self.output.join(format!("{name}.png")),
                        &bytes,
                        image.size[0] as u32,
                        image.size[1] as u32,
                        image::ColorType::Rgba8,
                    )?;
                    std::fs::write(
                        self.output.join(format!("{name}.json")),
                        serde_json::to_vec_pretty(&self.visible)?,
                    )?;
                    self.capture = None;
                    self.advance();
                    break;
                }
            }
            return Ok(());
        }
        if !self.queue.is_empty() {
            return Ok(());
        }
        if self.quiet > 0 {
            self.quiet -= 1;
            return Ok(());
        }
        // Native repaint frequency can far exceed 60 Hz under Xvfb. Frame counts
        // alone let the next click race egui's scrolling/layout animations.
        if self.observed_time < self.ready_at {
            return Ok(());
        }
        let Some(step) = self.script.steps.get(self.step).cloned() else {
            return self.finish(None);
        };
        let mut missing = None;
        match step {
            Step::Click { text, nth, offset }
            | Step::Fill {
                text, nth, offset, ..
            } => {
                if let Some(point) = self.point(&text, nth, offset) {
                    self.click(point);
                    if let Step::Fill { value, .. } = &self.script.steps[self.step] {
                        let modifiers = Modifiers::COMMAND;
                        self.queue.push_back(vec![Event::Key {
                            key: Key::A,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers,
                        }]);
                        self.queue.push_back(vec![
                            Event::Key {
                                key: Key::A,
                                physical_key: None,
                                pressed: false,
                                repeat: false,
                                modifiers,
                            },
                            if value.is_empty() {
                                Event::Key {
                                    key: Key::Backspace,
                                    physical_key: None,
                                    pressed: true,
                                    repeat: false,
                                    modifiers: Modifiers::NONE,
                                }
                            } else {
                                Event::Text(value.clone())
                            },
                        ]);
                        if value.is_empty() {
                            self.queue.push_back(vec![Event::Key {
                                key: Key::Backspace,
                                physical_key: None,
                                pressed: false,
                                repeat: false,
                                modifiers: Modifiers::NONE,
                            }]);
                        }
                    }
                    self.advance();
                } else {
                    missing = Some(text);
                }
            }
            Step::ClickAt { point } => {
                self.click(Pos2::new(point[0], point[1]));
                self.advance();
            }
            Step::Type { value } => {
                self.queue.push_back(vec![Event::Text(value)]);
                self.advance();
            }
            Step::Key { key, command } => {
                let key = parse_key(&key)?;
                let modifiers = if command {
                    Modifiers::COMMAND
                } else {
                    Modifiers::NONE
                };
                for pressed in [true, false] {
                    self.queue.push_back(vec![Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers,
                    }]);
                }
                self.advance();
            }
            Step::Scroll { point, delta } => {
                self.queue.push_back(vec![
                    Event::PointerMoved(Pos2::new(point[0], point[1])),
                    Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(delta[0], delta[1]),
                        modifiers: Modifiers::NONE,
                    },
                ]);
                self.advance();
                self.ready_at = self.observed_time + 0.35;
            }
            Step::ScrollTo {
                text,
                point,
                delta,
                max_scrolls,
            } => {
                if self.visible.iter().any(|p| p.text == text) {
                    self.advance();
                } else if self.scrolls >= max_scrolls {
                    return self.finish(Some(format!(
                        "step {}: {text:?} not visible after {max_scrolls} scrolls",
                        self.step
                    )));
                } else {
                    self.queue.push_back(vec![
                        Event::PointerMoved(Pos2::new(point[0], point[1])),
                        Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(delta[0], delta[1]),
                            modifiers: Modifiers::NONE,
                        },
                    ]);
                    self.scrolls += 1;
                    self.quiet = 3;
                    self.ready_at = self.observed_time + 0.35;
                }
            }
            Step::WaitText { text, seconds } => {
                if self.visible.iter().any(|p| p.text.contains(&text)) {
                    self.advance();
                } else if self.since.elapsed() > Duration::from_secs(seconds) {
                    return self.finish(Some(format!(
                        "step {}: timed out waiting for {text:?}",
                        self.step
                    )));
                }
            }
            Step::Capture { name } => {
                self.capture = Some(name);
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            Step::Wait { frames } => {
                self.advance();
                self.quiet = frames;
                self.ready_at = self.observed_time + (frames as f64 / 60.).max(0.05);
            }
        }
        if let Some(text) = missing {
            if self.since.elapsed() > Duration::from_secs(10) {
                self.finish(Some(format!(
                    "step {}: visible target {text:?} was not found",
                    self.step
                )))?;
            }
        }
        Ok(())
    }
}
fn parse_key(key: &str) -> Result<Key> {
    Key::from_name(key).with_context(|| format!("unknown input key {key:?}"))
}

/// Eframe supplies real pointer routing, focus, text editing and framebuffer
/// capture. The wrapper only injects events before Studio consumes its input.
pub struct ScriptApp {
    pub studio: crate::ui::Studio,
    pub runner: Runner,
    pub failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl eframe::App for ScriptApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.runner.input(raw);
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.draw(ctx);
    }
}
impl ScriptApp {
    /// A finished automation run must close without the interactive unsaved-map
    /// dialog cancelling its close request on every subsequent frame.
    pub fn draw(&mut self, ctx: &egui::Context) {
        if self.runner.finished {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.studio.draw(ctx);
        if let Err(error) = self.runner.observe(ctx) {
            self.runner.error = Some(format!("{error:#}"));
            self.runner.finished = true;
        }
        if self.runner.finished {
            if let Some(error) = &self.runner.error {
                eprintln!("Studio input script failed: {error}");
                self.failed
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}
