//! The monster AI editor: a node graph of a behaviour graph's states and the
//! forms to change it. States, transitions, conditions and actions are picked
//! from the engine's own lists.
use crate::ai::*;
use crate::arena_ui::{ArenaPanel, ArenaTools};
use crate::kit::Names;
use crate::kit_ui::{build_names, drag, labelled, move_buttons, pick, RED};
use crate::tables::Tables;
use d2_sim::rules::data_mod::ai::{
    presets, Field, FieldKind, KindInfo, ACTIONS, CONDITIONS, MAX_ACTIONS, MAX_AIS, MAX_CONDITIONS,
    MAX_COOLDOWNS, MAX_PHASES, MAX_STATES, MAX_TRANSITIONS, TARGETS, THINK_BUDGET,
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use std::collections::BTreeMap;
use std::path::Path;
use toml::Value;

const NODE: Vec2 = Vec2::new(168.0, 50.0);
const GAP: Vec2 = Vec2::new(86.0, 34.0);
const INK: Color32 = Color32::from_rgb(0xc3, 0xc2, 0xb7);
const PANEL: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x19);
const START: Color32 = Color32::from_rgb(0x6f, 0xc2, 0x7f);
const PHASE: Color32 = Color32::from_rgb(0xe0, 0xa4, 0x4c);

#[derive(Default)]
pub struct AiEditor {
    pub file: Option<AiFile>,
    pub error: String,
    pub entry: usize,
    pub state: usize,
    pub phase: Option<usize>,
    /// Node offsets dragged in this session, by (entry, state).
    offsets: BTreeMap<(usize, usize), Vec2>,
    new_preset: usize,
    message: String,
    pub arena: ArenaPanel,
}

impl AiEditor {
    pub fn open(&mut self, root: &Path) {
        *self = AiEditor::default();
        match AiFile::load(root) {
            Ok(file) => self.file = Some(file),
            Err(e) => self.error = format!("{e:#}"),
        }
    }

    pub fn dirty(&self) -> bool {
        self.file.as_ref().is_some_and(|f| f.dirty)
    }

    pub fn save(&mut self) -> anyhow::Result<()> {
        match &mut self.file {
            Some(f) if f.dirty => {
                if let Some(p) = f.problems().first() {
                    anyhow::bail!("Fix the {AI_FILE} problems before saving:\n{}", p.message);
                }
                f.save()
            }
            _ => Ok(()),
        }
    }

    pub fn draw(&mut self, ui: &mut egui::Ui, tables: Option<&Tables>, tools: &ArenaTools) {
        if !self.error.is_empty() {
            ui.colored_label(RED, &self.error);
            ui.label("Studio will not overwrite a file it cannot read. Fix it in a text editor, then reopen the project.");
            return;
        }
        let names = build_names(tables);
        let Some(file) = &mut self.file else {
            ui.label("Open a project to edit its monster AI.");
            return;
        };
        let mut changed = false;
        let mut run = None;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(260.0);
                ui.set_max_width(260.0);
                changed |= entry_list(ui, file, &mut self.entry, &mut self.new_preset);
                ui.separator();
                ui.strong("Phase immunity");
                let mut state = file.immune_state();
                let mut enabled = state.is_some();
                if ui.checkbox(&mut enabled, "Use an immunity state").changed() {
                    state = enabled.then_some(0);
                }
                if let Some(id) = &mut state {
                    labelled(ui, "Immunity state", |ui| pick(ui, "phase-immunity", id, &names.states));
                }
                file.set_immune_state(state);

            });
            ui.separator();
            ui.vertical(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt("ai-form")
                    .show(ui, |ui| {
                        self.entry = self.entry.min(file.entries.len().saturating_sub(1));
                        if file.entries.is_empty() {
                            ui.label("No monster AI yet. Add a behaviour graph, or start from one of D2Gamma's own AIs.");
                        } else {
                            let problems = file.problems();
                            let here = self.entry;
                            if file.entries[here].preset.is_some() {
                                changed |= preset_form(ui, &mut file.entries, here);
                            } else {
                                let ticks = self.arena.ticks_for(&file.entries[here]);
                                changed |= graph_form(
                                    ui,
                                    &mut file.entries,
                                    here,
                                    &mut self.state,
                                    &mut self.phase,
                                    &mut self.offsets,
                                    &names,
                                    &problems,
                                    ticks.as_deref(),
                                );
                                ui.separator();
                                let blocked = problems
                                    .first()
                                    .map(|_| "Fix the problems below before running.");
                                if self.arena.show(ui, &names, tools, &file.entries[here], blocked) {
                                    run = Some(file.entries[here].index);
                                }
                            }
                            problems_panel(ui, &problems, &mut self.entry, &mut self.state, &mut self.phase);
                        }
                    });
            });
        });
        if !self.message.is_empty() {
            ui.colored_label(RED, &self.message);
        }
        if changed {
            file.dirty = true;
        }
        if let Some(ai) = run {
            self.message.clear();
            match self.save() {
                Ok(()) => self.arena.start(tools, ai),
                Err(e) => self.message = format!("{e:#}"),
            }
        }
    }
}

// ------------------------------------------------------------------ list

fn entry_list(
    ui: &mut egui::Ui,
    file: &mut AiFile,
    sel: &mut usize,
    new_preset: &mut usize,
) -> bool {
    let mut changed = false;
    ui.heading("Monster AI");
    ui.weak(format!("{} of {MAX_AIS} entries", file.entries.len()));
    for (i, e) in file.entries.iter().enumerate() {
        let tag = if e.preset.is_some() {
            "preset"
        } else {
            "graph"
        };
        let label = format!(
            "{} · {}  [{tag}]",
            e.index,
            if e.name.is_empty() {
                "(unnamed)"
            } else {
                &e.name
            }
        );
        if ui.selectable_label(*sel == i, label).clicked() {
            *sel = i;
        }
    }
    ui.separator();
    let full = file.entries.len() >= MAX_AIS;
    if ui
        .add_enabled(!full, egui::Button::new("New behaviour graph"))
        .clicked()
    {
        let index = file.next_index();
        file.entries.push(Entry::new_graph(index, "New AI"));
        *sel = file.entries.len() - 1;
        changed = true;
    }
    let all = presets();
    if !all.is_empty() {
        *new_preset = (*new_preset).min(all.len() - 1);
        egui::ComboBox::from_id_salt("ai-new-preset")
            .width(230.0)
            .selected_text(all[*new_preset].name.clone())
            .show_ui(ui, |ui| {
                for (i, p) in all.iter().enumerate() {
                    ui.selectable_value(new_preset, i, &p.name);
                }
            });
        if ui
            .add_enabled(!full, egui::Button::new("Add from D2Gamma's AI"))
            .on_hover_text("A copy of one of the built-in AIs under a new index, with its settings ready to change")
            .clicked()
        {
            let index = file.next_index();
            file.entries.push(Entry::new_preset(index, &all[*new_preset].name));
            *sel = file.entries.len() - 1;
            changed = true;
        }
    }
    if !file.entries.is_empty() {
        ui.separator();
        let i = *sel;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!full, egui::Button::new("Duplicate"))
                .clicked()
            {
                let mut copy = file.entries[i].clone();
                copy.index = file.next_index();
                copy.name = format!("{} copy", copy.name);
                file.entries.push(copy);
                *sel = file.entries.len() - 1;
                changed = true;
            }
            let (to, remove) = move_buttons(ui, i, file.entries.len());
            if let Some(to) = to {
                file.entries.swap(i, to);
                *sel = to;
                changed = true;
            }
            if remove {
                file.entries.remove(i);
                *sel = i.saturating_sub(1);
                changed = true;
            }
        });
    }
    changed
}

// ---------------------------------------------------------------- preset

fn preset_form(ui: &mut egui::Ui, entries: &mut [Entry], at: usize) -> bool {
    let mut changed = false;
    let e = &mut entries[at];
    ui.heading(format!(
        "{} · a copy of D2Gamma's {}",
        e.index,
        e.preset.clone().unwrap_or_default()
    ));
    ui.label("Monsters whose monstats AI column holds this index run D2Gamma's AI with the settings below; every other monster is untouched. A setting you leave off keeps the monster row's own value.");
    changed |= labelled(ui, "AI index", |ui| drag(ui, &mut e.index, 0..=u16::MAX));
    changed |= labelled(ui, "Name", |ui| {
        ui.text_edit_singleline(&mut e.name).changed()
    });
    let current = e.preset.clone().unwrap_or_default();
    let Some(preset) = presets()
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(&current))
    else {
        ui.colored_label(RED, format!("There is no D2Gamma AI called `{current}`."));
        return changed;
    };
    ui.separator();
    ui.strong("Settings");
    let mut fields: Vec<(String, String)> = preset
        .knobs
        .iter()
        .map(|k| {
            (
                k.name.clone(),
                format!("{} (aip{})", k.name.replace('_', " "), k.aip),
            )
        })
        .collect();
    for n in 1..=8u8 {
        if !preset.knobs.iter().any(|k| k.aip == n) {
            fields.push((format!("aip{n}"), format!("aip{n} (not described)")));
        }
    }
    for (key, label) in fields {
        let pos = e.params.iter().position(|(k, _)| *k == key);
        ui.horizontal(|ui| {
            let mut set = pos.is_some();
            if ui
                .checkbox(&mut set, "")
                .on_hover_text("Change this setting")
                .changed()
            {
                if set {
                    e.params.push((key.clone(), Param::All(0)));
                } else if let Some(p) = pos {
                    e.params.remove(p);
                }
                changed = true;
            }
            ui.add_sized([210.0, 20.0], egui::Label::new(label.as_str()));
            if let Some(p) = e.params.iter().position(|(k, _)| *k == key) {
                let mut each = matches!(e.params[p].1, Param::Each(_));
                if ui.checkbox(&mut each, "per difficulty").changed() {
                    e.params[p].1 = match e.params[p].1 {
                        Param::All(n) if each => Param::Each([n; 3]),
                        Param::Each([n, ..]) if !each => Param::All(n),
                        other => other,
                    };
                    changed = true;
                }
                match &mut e.params[p].1 {
                    Param::All(n) => changed |= drag(ui, n, -32768..=32767),
                    Param::Each(v) => {
                        for (slot, name) in v.iter_mut().zip(["Normal", "Nightmare", "Hell"]) {
                            ui.label(name);
                            changed |= drag(ui, slot, -32768..=32767);
                        }
                    }
                }
            }
        });
    }
    changed
}

// ----------------------------------------------------------------- graph

#[allow(clippy::too_many_arguments)]
fn graph_form(
    ui: &mut egui::Ui,
    entries: &mut [Entry],
    at: usize,
    state: &mut usize,
    phase: &mut Option<usize>,
    offsets: &mut BTreeMap<(usize, usize), Vec2>,
    names: &Names,
    problems: &[Problem],
    ticks: Option<&[u32]>,
) -> bool {
    let mut changed = false;
    let graphs: Vec<u16> = entries.iter().map(|e| e.index).collect();
    let e = &mut entries[at];
    ui.heading(format!(
        "{} · {}",
        e.index,
        if e.name.is_empty() {
            "(unnamed)"
        } else {
            &e.name
        }
    ));
    changed |= labelled(ui, "AI index", |ui| {
        let c = drag(ui, &mut e.index, 0..=u16::MAX);
        ui.weak("the monstats AI value this graph answers");
        c
    });
    changed |= labelled(ui, "Name", |ui| {
        ui.text_edit_singleline(&mut e.name).changed()
    });
    changed |= labelled(ui, "Target", |ui| {
        let mut c = false;
        let shown = e.target.clone().unwrap_or_else(|| "enemy".into());
        egui::ComboBox::from_id_salt("ai-target")
            .width(260.0)
            .selected_text(
                TARGETS
                    .iter()
                    .find(|(k, _)| *k == shown)
                    .map_or(shown.clone(), |(k, l)| format!("{k}: {l}")),
            )
            .show_ui(ui, |ui| {
                for (k, l) in TARGETS {
                    if ui
                        .selectable_label(shown == *k, format!("{k}: {l}"))
                        .clicked()
                    {
                        e.target = (*k != "enemy").then(|| (*k).to_owned());
                        c = true;
                    }
                }
            });
        c
    });
    *state = (*state).min(e.states.len().saturating_sub(1));
    changed |= labelled(ui, "Starts in", |ui| {
        let mut c = false;
        let start = e.start_position();
        egui::ComboBox::from_id_salt("ai-start")
            .width(260.0)
            .selected_text(e.states.get(start).map_or("", |s| s.name.as_str()))
            .show_ui(ui, |ui| {
                for (i, s) in e.states.iter().enumerate() {
                    if ui.selectable_label(i == start, &s.name).clicked() {
                        e.start = (i != 0).then(|| s.name.clone());
                        c = true;
                    }
                }
            });
        c
    });
    changed |= cooldowns_form(ui, e);
    ui.separator();
    ui.strong("States");
    canvas(ui, e, at, state, phase, offsets, problems, ticks);
    ui.horizontal(|ui| {
        let full = e.states.len() >= MAX_STATES;
        if ui
            .add_enabled(!full, egui::Button::new("Add state"))
            .clicked()
        {
            let name = unique_name(&e.state_names(), "state");
            e.states.push(State {
                name,
                transitions: Vec::new(),
                actions: Vec::new(),
            });
            *state = e.states.len() - 1;
            *phase = None;
            changed = true;
        }
        ui.weak(format!("{} of {MAX_STATES}", e.states.len()));
    });
    ui.separator();
    if e.states.is_empty() {
        return changed;
    }
    changed |= state_form(ui, e, state, names);
    ui.separator();
    changed |= phases_form(ui, e, phase, &graphs, names);
    ui.weak(format!(
        "One think looks at no more than {THINK_BUDGET} conditions and actions, whatever the graph."
    ));
    changed
}

fn cooldowns_form(ui: &mut egui::Ui, e: &mut Entry) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new(format!("Cooldowns ({})", e.cooldowns.len()))
        .id_salt("ai-cooldowns")
        .show(ui, |ui| {
            let mut remove = None;
            let mut rename = None;
            for (i, c) in e.cooldowns.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let mut name = c.name.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut name).desired_width(140.0))
                        .changed()
                    {
                        rename = Some((i, name));
                    }
                    let mut s = as_f64(&c.seconds);
                    if ui
                        .add(
                            egui::DragValue::new(&mut s)
                                .range(0.0..=600.0)
                                .speed(0.1)
                                .suffix(" s"),
                        )
                        .changed()
                    {
                        c.seconds = seconds(s);
                        changed = true;
                    }
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some((i, name)) = rename {
                e.rename_cooldown(i, &name);
                changed = true;
            }
            if let Some(i) = remove {
                e.delete_cooldown(i);
                changed = true;
            }
            if ui
                .add_enabled(
                    e.cooldowns.len() < MAX_COOLDOWNS,
                    egui::Button::new("Add cooldown"),
                )
                .clicked()
            {
                let name = unique_name(&e.cooldown_names(), "cooldown");
                e.cooldowns.push(Cooldown {
                    name,
                    seconds: seconds(5.0),
                });
                changed = true;
            }
        });
    changed
}

// ---------------------------------------------------------------- canvas

fn bezier(p0: Pos2, c1: Pos2, c2: Pos2, p1: Pos2, t: f32) -> Pos2 {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    Pos2::new(
        w[0] * p0.x + w[1] * c1.x + w[2] * c2.x + w[3] * p1.x,
        w[0] * p0.y + w[1] * c1.y + w[2] * c2.y + w[3] * p1.y,
    )
}

fn arrow(painter: &egui::Painter, from: Pos2, to: Pos2, stroke: Stroke) {
    let dir = (to - from).normalized();
    let side = Vec2::new(-dir.y, dir.x);
    painter.line_segment([to, to - dir * 9.0 + side * 5.0], stroke);
    painter.line_segment([to, to - dir * 9.0 - side * 5.0], stroke);
}

fn short(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

/// The node graph. Clicking a state selects it; dragging moves it (for this
/// session only: the layout is worked out from the transitions).
#[allow(clippy::too_many_arguments)]
fn canvas(
    ui: &mut egui::Ui,
    e: &Entry,
    at: usize,
    selected: &mut usize,
    phase: &mut Option<usize>,
    offsets: &mut BTreeMap<(usize, usize), Vec2>,
    problems: &[Problem],
    ticks: Option<&[u32]>,
) {
    let cells = layout(e);
    let columns = cells.iter().map(|c| c.0).max().map_or(1, |c| c + 1);
    let rows = cells.iter().map(|c| c.1).max().map_or(1, |r| r + 1);
    let phase_band = if e.phases.is_empty() {
        0.0
    } else {
        NODE.y + GAP.y
    };
    let size = Vec2::new(
        (columns as f32 * (NODE.x + GAP.x) + 20.0).max(ui.available_width().min(900.0)),
        phase_band + rows as f32 * (NODE.y + GAP.y) + 40.0,
    );
    let mut picked = None;
    let mut drags: Vec<(usize, Vec2)> = Vec::new();
    let offs: Vec<Vec2> = (0..e.states.len())
        .map(|i| offsets.get(&(at, i)).copied().unwrap_or_default())
        .collect();
    egui::ScrollArea::horizontal()
        .id_salt("ai-canvas")
        .show(ui, |ui| {
            let (response, painter) = ui.allocate_painter(size, Sense::hover());
            painter.rect_filled(response.rect, 4.0, PANEL);
            let origin = response.rect.min + Vec2::new(14.0, 14.0 + phase_band);
            let rect_of = |i: usize| -> Rect {
                let (c, r) = cells[i];
                let base =
                    origin + Vec2::new(c as f32 * (NODE.x + GAP.x), r as f32 * (NODE.y + GAP.y));
                Rect::from_min_size(base + offs[i], NODE)
            };
            let bad = |i: usize| {
                problems
                    .iter()
                    .any(|p| p.at.is_some_and(|a| a.entry == at && a.state == Some(i)))
            };
            // Edges under the nodes.
            let mut back = 0.0_f32;
            for edge in edges(e) {
                let (a, b) = (rect_of(edge.from), rect_of(edge.to));
                let stroke = Stroke::new(1.5_f32, INK);
                let (p0, c1, c2, p1) = if b.center().x > a.center().x + 10.0 {
                    let (p0, p1) = (a.right_center(), b.left_center());
                    let d = ((p1.x - p0.x) / 2.0).max(30.0);
                    (p0, p0 + Vec2::new(d, 0.0), p1 - Vec2::new(d, 0.0), p1)
                } else {
                    let (p0, p1) = (a.center_bottom(), b.center_bottom());
                    back += 1.0;
                    let bulge = Vec2::new(0.0, 34.0 + 16.0 * back);
                    (p0, p0 + bulge, p1 + bulge, p1)
                };
                painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                    [p0, c1, c2, p1],
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                ));
                arrow(&painter, bezier(p0, c1, c2, p1, 0.92), p1, stroke);
                let mid = bezier(p0, c1, c2, p1, 0.5);
                let text = short(&edge.label, 34);
                let galley = painter.layout_no_wrap(text, FontId::proportional(10.5), INK);
                let label_rect = Rect::from_center_size(mid, galley.size() + Vec2::new(8.0, 4.0));
                painter.rect_filled(label_rect, 3.0, PANEL);
                painter.galley(label_rect.min + Vec2::new(4.0, 2.0), galley, INK);
            }
            // Phases along the top, each tied to the state it goes to.
            for (i, p) in e.phases.iter().enumerate() {
                let rect = Rect::from_min_size(
                    response.rect.min + Vec2::new(14.0 + i as f32 * (NODE.x + 14.0), 10.0),
                    Vec2::new(NODE.x, NODE.y - 8.0),
                );
                let chosen = *phase == Some(i);
                painter.rect(
                    rect,
                    6.0,
                    Color32::from_rgb(0x2b, 0x25, 0x1b),
                    Stroke::new(if chosen { 3.0_f32 } else { 1.5_f32 }, PHASE),
                    egui::StrokeKind::Inside,
                );
                let target = match (&p.state, p.graph) {
                    (Some(s), _) => format!("to {s}"),
                    (None, Some(g)) => format!("to graph {g}"),
                    _ => "event only".into(),
                };
                painter.text(
                    rect.center_top() + Vec2::new(0.0, 6.0),
                    Align2::CENTER_TOP,
                    short(&format!("★ {}", p.name), 22),
                    FontId::proportional(12.5),
                    PHASE,
                );
                painter.text(
                    rect.center_bottom() - Vec2::new(0.0, 5.0),
                    Align2::CENTER_BOTTOM,
                    short(&target, 24),
                    FontId::proportional(10.5),
                    INK,
                );
                if let Some(to) = p
                    .state
                    .as_ref()
                    .and_then(|s| e.states.iter().position(|x| &x.name == s))
                {
                    let t = rect_of(to);
                    let (p0, p1) = (rect.center_bottom(), t.center_top());
                    painter.line_segment([p0, p1], Stroke::new(1.0_f32, PHASE));
                    arrow(&painter, p0, p1, Stroke::new(1.0_f32, PHASE));
                }
                let response = ui.interact(rect, ui.id().with(("phase", at, i)), Sense::click());
                if response.clicked() {
                    picked = Some((None, Some(i)));
                }
            }
            // Nodes.
            for (i, s) in e.states.iter().enumerate() {
                let rect = rect_of(i);
                let response = ui.interact(
                    rect,
                    ui.id().with(("state", at, i)),
                    Sense::click_and_drag(),
                );
                if response.dragged() {
                    drags.push((i, response.drag_delta()));
                }
                if response.clicked() || response.drag_started() {
                    picked = Some((Some(i), None));
                }
                let chosen = *selected == i && phase.is_none();
                let edge = if bad(i) {
                    RED
                } else if i == e.start_position() {
                    START
                } else {
                    INK
                };
                painter.rect(
                    rect,
                    8.0,
                    if chosen {
                        Color32::from_rgb(0x2d, 0x33, 0x3a)
                    } else {
                        Color32::from_rgb(0x24, 0x24, 0x22)
                    },
                    Stroke::new(if chosen { 3.0_f32 } else { 1.5_f32 }, edge),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    rect.center_top() + Vec2::new(0.0, 8.0),
                    Align2::CENTER_TOP,
                    short(&s.name, 20),
                    FontId::proportional(14.0),
                    Color32::WHITE,
                );
                painter.text(
                    rect.center_bottom() - Vec2::new(0.0, 6.0),
                    Align2::CENTER_BOTTOM,
                    format!(
                        "{} transitions · {} actions",
                        s.transitions.len(),
                        s.actions.len()
                    ),
                    FontId::proportional(10.5),
                    INK,
                );
                if let Some(n) = ticks.and_then(|t| t.get(i)) {
                    let most = ticks
                        .and_then(|t| t.iter().max())
                        .copied()
                        .unwrap_or(1)
                        .max(1);
                    let bar = Rect::from_min_size(
                        rect.left_bottom() + Vec2::new(8.0, -4.0),
                        Vec2::new((NODE.x - 16.0) * *n as f32 / most as f32, 3.0),
                    );
                    painter.rect_filled(
                        bar,
                        1.0,
                        crate::arena_ui::state_color(u8::try_from(i).ok()),
                    );
                    painter.text(
                        rect.right_top() + Vec2::new(-6.0, -2.0),
                        Align2::RIGHT_BOTTOM,
                        format!("ran {n} ticks"),
                        FontId::proportional(10.0),
                        INK,
                    );
                }
                if i == e.start_position() {
                    painter.text(
                        rect.left_top() + Vec2::new(6.0, -2.0),
                        Align2::LEFT_BOTTOM,
                        "start",
                        FontId::proportional(10.0),
                        START,
                    );
                }
            }
        });
    for (i, delta) in drags {
        *offsets.entry((at, i)).or_default() += delta;
    }
    if let Some((s, p)) = picked {
        if let Some(s) = s {
            *selected = s;
        }
        *phase = p;
    }
}

// ----------------------------------------------------------------- forms

fn state_form(ui: &mut egui::Ui, e: &mut Entry, selected: &mut usize, names: &Names) -> bool {
    let mut changed = false;
    let i = *selected;
    let cooldowns = e.cooldown_names();
    let mut deleted = false;
    ui.horizontal(|ui| {
        ui.strong("State");
        let mut name = e.states[i].name.clone();
        if ui
            .add(egui::TextEdit::singleline(&mut name).desired_width(200.0))
            .changed()
        {
            e.rename_state(i, &name);
            changed = true;
        }
        if ui.button("Make start").clicked() {
            e.start = (i != 0).then(|| e.states[i].name.clone());
            changed = true;
        }
        if ui
            .add_enabled(e.states.len() > 1, egui::Button::new("Delete state"))
            .clicked()
        {
            e.delete_state(i);
            *selected = i.saturating_sub(1);
            changed = true;
            deleted = true;
        }
    });
    if deleted {
        return true;
    }
    let all_states = e.state_names();
    let s = &mut e.states[i];
    ui.add_space(4.0);
    ui.strong(format!(
        "Transitions ({} of {MAX_TRANSITIONS}) — checked in order, the first that holds wins",
        s.transitions.len()
    ));
    let mut t_move: Option<(usize, usize)> = None;
    let mut t_remove = None;
    let count = s.transitions.len();
    for (n, t) in s.transitions.iter_mut().enumerate() {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("Go to");
                egui::ComboBox::from_id_salt(("tr-to", n))
                    .width(160.0)
                    .selected_text(t.to.clone())
                    .show_ui(ui, |ui| {
                        for name in &all_states {
                            if *name != s_name(&all_states, i)
                                && ui.selectable_label(t.to == *name, name).clicked()
                            {
                                t.to = name.clone();
                                changed = true;
                            }
                        }
                    });
                let (to, remove) = move_buttons(ui, n, count);
                if let Some(to) = to {
                    t_move = Some((n, to));
                }
                if remove {
                    t_remove = Some(n);
                }
            });
            changed |= conditions_ui(ui, ("tr", n), &mut t.when, &cooldowns, names);
        });
    }
    if let Some((a, b)) = t_move {
        s.transitions.swap(a, b);
        changed = true;
    }
    if let Some(n) = t_remove {
        s.transitions.remove(n);
        changed = true;
    }
    if ui
        .add_enabled(
            s.transitions.len() < MAX_TRANSITIONS && all_states.len() > 1,
            egui::Button::new("Add transition"),
        )
        .clicked()
    {
        let to = all_states
            .iter()
            .enumerate()
            .find(|(k, _)| *k != i)
            .map(|(_, n)| n.clone())
            .unwrap_or_default();
        s.transitions.push(Transition {
            to,
            when: Vec::new(),
        });
        changed = true;
    }
    ui.add_space(6.0);
    ui.strong(format!(
        "Actions ({} of {MAX_ACTIONS}) — tried in order; the first that acts ends the turn",
        s.actions.len()
    ));
    let mut a_move: Option<(usize, usize)> = None;
    let mut a_remove = None;
    let count = s.actions.len();
    for (n, a) in s.actions.iter_mut().enumerate() {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                changed |= kind_picker(ui, ("act-kind", n), ACTIONS, &mut a.item, &cooldowns);
                if a.item.action_info().is_some_and(|k| k.is_instant()) {
                    ui.weak("instant: the turn goes on");
                }
                let (to, remove) = move_buttons(ui, n, count);
                if let Some(to) = to {
                    a_move = Some((n, to));
                }
                if remove {
                    a_remove = Some(n);
                }
            });
            if let Some(info) = a.item.action_info() {
                ui.weak(info.summary);
                changed |= fields_ui(ui, ("act", n), info, &mut a.item.keys, &cooldowns, names);
            } else {
                ui.colored_label(
                    RED,
                    format!("`{}` is not an action this build knows.", a.item.kind),
                );
            }
            ui.label("Only when");
            changed |= conditions_ui(ui, ("act-when", n), &mut a.when, &cooldowns, names);
            if !cooldowns.is_empty() {
                changed |= labelled(ui, "Starts cooldown", |ui| {
                    let mut c = false;
                    egui::ComboBox::from_id_salt(("act-cd", n))
                        .width(160.0)
                        .selected_text(a.starts_cooldown.clone().unwrap_or_else(|| "none".into()))
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(a.starts_cooldown.is_none(), "none")
                                .clicked()
                            {
                                a.starts_cooldown = None;
                                c = true;
                            }
                            for name in &cooldowns {
                                if ui
                                    .selectable_label(
                                        a.starts_cooldown.as_ref() == Some(name),
                                        name,
                                    )
                                    .clicked()
                                {
                                    a.starts_cooldown = Some(name.clone());
                                    c = true;
                                }
                            }
                        });
                    c
                });
            }
        });
    }
    if let Some((x, y)) = a_move {
        s.actions.swap(x, y);
        changed = true;
    }
    if let Some(n) = a_remove {
        s.actions.remove(n);
        changed = true;
    }
    changed |= add_picker(
        ui,
        "act-add",
        "Add action",
        ACTIONS,
        s.actions.len() < MAX_ACTIONS,
        |info| {
            s.actions.push(Action {
                item: Item::new(info, &cooldowns),
                when: Vec::new(),
                starts_cooldown: None,
            });
        },
    );
    changed
}

fn s_name(all: &[String], i: usize) -> String {
    all.get(i).cloned().unwrap_or_default()
}

/// A drop-down of kinds that adds the chosen one through `add`.
fn add_picker(
    ui: &mut egui::Ui,
    salt: &str,
    label: &str,
    kinds: &'static [KindInfo],
    room: bool,
    mut add: impl FnMut(&'static KindInfo),
) -> bool {
    let mut changed = false;
    ui.add_enabled_ui(room, |ui| {
        egui::ComboBox::from_id_salt(salt)
            .width(220.0)
            .selected_text(format!("+ {label}"))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show_ui(ui, |ui| {
                let query = kind_filter(ui);
                for info in kinds.iter().filter(|info| kind_matches(info, &query)) {
                    if ui
                        .selectable_label(false, info.label)
                        .on_hover_text(info.summary)
                        .clicked()
                    {
                        add(info);
                        changed = true;
                        ui.memory_mut(|m| m.close_popup());
                    }
                }
            });
    });
    changed
}

/// Keep long action/condition palettes usable without hunting through scroll rows.
fn kind_filter(ui: &mut egui::Ui) -> String {
    let id = ui.id().with("kind-search");
    let mut query = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
    ui.label("Find behaviour");
    ui.add(
        egui::TextEdit::singleline(&mut query)
            .hint_text("Name or label")
            .desired_width(210.),
    );
    ui.data_mut(|d| d.insert_temp(id, query.clone()));
    query.to_lowercase()
}
fn kind_matches(info: &KindInfo, query: &str) -> bool {
    info.label.to_lowercase().contains(query) || info.name.to_lowercase().contains(query)
}

/// Changes the kind of an item; the keys start over at the new kind's defaults.
fn kind_picker(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash,
    kinds: &'static [KindInfo],
    item: &mut Item,
    cooldowns: &[String],
) -> bool {
    let mut changed = false;
    let shown = kinds
        .iter()
        .find(|k| k.name == item.kind)
        .map_or(item.kind.clone(), |k| k.label.to_owned());
    egui::ComboBox::from_id_salt(salt)
        .width(200.0)
        .selected_text(shown)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            let query = kind_filter(ui);
            for info in kinds.iter().filter(|info| kind_matches(info, &query)) {
                if ui
                    .selectable_label(info.name == item.kind, info.label)
                    .on_hover_text(info.summary)
                    .clicked()
                    && info.name != item.kind
                {
                    *item = Item::new(info, cooldowns);
                    changed = true;
                    ui.memory_mut(|m| m.close_popup());
                }
            }
        });
    changed
}

fn conditions_ui(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + Copy,
    list: &mut Vec<Item>,
    cooldowns: &[String],
    names: &Names,
) -> bool {
    let mut changed = false;
    let mut remove = None;
    for (n, c) in list.iter_mut().enumerate() {
        ui.horizontal_wrapped(|ui| {
            changed |= kind_picker(ui, (salt, "kind", n), CONDITIONS, c, cooldowns);
            if let Some(info) = c.condition_info() {
                changed |= fields_inline(ui, (salt, n), info, &mut c.keys, cooldowns, names);
            } else {
                ui.colored_label(
                    RED,
                    format!("`{}` is not a condition this build knows.", c.kind),
                );
            }
            if ui
                .small_button("x")
                .on_hover_text("Remove this condition")
                .clicked()
            {
                remove = Some(n);
            }
        });
    }
    if let Some(n) = remove {
        list.remove(n);
        changed = true;
    }
    changed |= add_picker(
        ui,
        &format!("{:?}-add", salt_text(salt)),
        if list.is_empty() {
            "Always — add a condition"
        } else {
            "And also…"
        },
        CONDITIONS,
        list.len() < MAX_CONDITIONS,
        |info| list.push(Item::new(info, cooldowns)),
    );
    changed
}

fn salt_text(salt: impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    salt.hash(&mut h);
    h.finish()
}

/// All the keys of a kind, one labelled row each.
fn fields_ui(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + Copy,
    info: &KindInfo,
    keys: &mut BTreeMap<String, Value>,
    cooldowns: &[String],
    names: &Names,
) -> bool {
    let mut changed = false;
    for f in info.fields {
        ui.horizontal(|ui| {
            changed |= field_ui(ui, (salt, f.key), f, keys, cooldowns, names, true);
        });
    }
    changed
}

fn fields_inline(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + Copy,
    info: &KindInfo,
    keys: &mut BTreeMap<String, Value>,
    cooldowns: &[String],
    names: &Names,
) -> bool {
    let mut changed = false;
    for f in info.fields {
        changed |= field_ui(ui, (salt, f.key), f, keys, cooldowns, names, false);
    }
    changed
}

/// One key. A key the kind can do without has a box that turns it on.
fn field_ui(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + Copy,
    f: &Field,
    keys: &mut BTreeMap<String, Value>,
    cooldowns: &[String],
    names: &Names,
    labelled_row: bool,
) -> bool {
    let mut changed = false;
    if labelled_row {
        ui.add_sized([150.0, 20.0], egui::Label::new(f.label));
    } else {
        ui.label(f.label);
    }
    if !f.required {
        let mut on = keys.contains_key(f.key);
        if ui
            .checkbox(&mut on, "")
            .on_hover_text("Set this; left off the engine's default is used")
            .changed()
        {
            if on {
                keys.insert(f.key.to_owned(), f.default_value());
            } else {
                keys.remove(f.key);
            }
            changed = true;
        }
        if !on {
            return changed;
        }
    } else if !keys.contains_key(f.key) {
        keys.insert(f.key.to_owned(), f.default_value());
    }
    let value = keys.get_mut(f.key).expect("key present");
    match f.kind {
        FieldKind::Int { min, max, .. } => {
            let mut n = value.as_integer().unwrap_or(0);
            if ui
                .add(egui::DragValue::new(&mut n).range(i64::from(min)..=i64::from(max)))
                .changed()
            {
                *value = Value::Integer(n);
                changed = true;
            }
        }
        FieldKind::Seconds { .. } => {
            let mut s = as_f64(value);
            if ui
                .add(
                    egui::DragValue::new(&mut s)
                        .range(0.0..=600.0)
                        .speed(0.1)
                        .suffix(" s"),
                )
                .changed()
            {
                *value = seconds(s);
                changed = true;
            }
        }
        FieldKind::Bool { .. } => {
            let mut b = value.as_bool().unwrap_or(false);
            if ui.checkbox(&mut b, "").changed() {
                *value = Value::Boolean(b);
                changed = true;
            }
        }
        FieldKind::Choice { options, .. } => {
            let now = plain(value);
            egui::ComboBox::from_id_salt(salt)
                .width(150.0)
                .selected_text(
                    options
                        .iter()
                        .find(|(k, _)| *k == now)
                        .map_or(now.clone(), |(_, l)| (*l).to_owned()),
                )
                .show_ui(ui, |ui| {
                    for (k, l) in options {
                        if ui.selectable_label(now == *k, *l).clicked() {
                            *value = Value::String((*k).to_owned());
                            changed = true;
                        }
                    }
                });
        }
        FieldKind::State => {
            let mut id = value
                .as_integer()
                .and_then(|n| u16::try_from(n).ok())
                .unwrap_or(0);
            if pick(
                ui,
                &format!("{:?}", salt_text(salt)),
                &mut id,
                &names.states,
            ) {
                *value = Value::Integer(i64::from(id));
                changed = true;
            }
        }
        FieldKind::Class => {
            let mut id = value
                .as_integer()
                .and_then(|n| u16::try_from(n).ok())
                .unwrap_or(0);
            if pick(
                ui,
                &format!("{:?}", salt_text(salt)),
                &mut id,
                &names.monsters,
            ) {
                *value = Value::Integer(i64::from(id));
                changed = true;
            }
        }
        FieldKind::Cooldown => {
            let now = plain(value);
            egui::ComboBox::from_id_salt(salt)
                .width(150.0)
                .selected_text(if now.is_empty() {
                    "(pick one)".to_owned()
                } else {
                    now.clone()
                })
                .show_ui(ui, |ui| {
                    for name in cooldowns {
                        if ui.selectable_label(now == *name, name).clicked() {
                            *value = Value::String(name.clone());
                            changed = true;
                        }
                    }
                });
            if cooldowns.is_empty() {
                ui.colored_label(RED, "add a cooldown first");
            }
        }
        FieldKind::Name => {
            let mut text = plain(value);
            if ui
                .add(egui::TextEdit::singleline(&mut text).desired_width(120.0))
                .changed()
            {
                *value = Value::String(text);
                changed = true;
            }
        }
    }
    changed
}

fn phases_form(
    ui: &mut egui::Ui,
    e: &mut Entry,
    selected: &mut Option<usize>,
    graphs: &[u16],
    names: &Names,
) -> bool {
    let mut changed = false;
    let states = e.state_names();
    let cooldowns = e.cooldown_names();
    let own = e.index;
    ui.strong(format!(
        "Boss phases ({} of {MAX_PHASES}) — each starts once, in order",
        e.phases.len()
    ));
    let mut moved: Option<(usize, usize)> = None;
    let mut removed = None;
    let count = e.phases.len();
    for (n, p) in e.phases.iter_mut().enumerate() {
        let open = *selected == Some(n);
        let header = egui::CollapsingHeader::new(format!("★ {}", p.name))
            .id_salt(("phase", n))
            .open(Some(open))
            .show(ui, |ui| {
                changed |= labelled(ui, "Name", |ui| {
                    ui.text_edit_singleline(&mut p.name).changed()
                });
                ui.label("Starts when");
                changed |= conditions_ui(ui, ("ph", n), &mut p.when, &cooldowns, names);
                ui.horizontal(|ui| {
                    ui.label("Then");
                    let mode = if p.state.is_some() {
                        1
                    } else if p.graph.is_some() {
                        2
                    } else {
                        0
                    };
                    let mut now = mode;
                    egui::ComboBox::from_id_salt(("ph-mode", n))
                        .width(200.0)
                        .selected_text(
                            [
                                "only report the event",
                                "go to a state",
                                "switch to another graph",
                            ][mode],
                        )
                        .show_ui(ui, |ui| {
                            for (k, l) in [
                                "only report the event",
                                "go to a state",
                                "switch to another graph",
                            ]
                            .iter()
                            .enumerate()
                            {
                                ui.selectable_value(&mut now, k, *l);
                            }
                        });
                    if now != mode {
                        p.state = (now == 1).then(|| states.first().cloned().unwrap_or_default());
                        p.graph = (now == 2)
                            .then(|| graphs.iter().copied().find(|g| *g != own).unwrap_or(own));
                        changed = true;
                    }
                    if let Some(s) = &mut p.state {
                        egui::ComboBox::from_id_salt(("ph-state", n))
                            .width(160.0)
                            .selected_text(s.clone())
                            .show_ui(ui, |ui| {
                                for name in &states {
                                    if ui.selectable_label(s == name, name).clicked() {
                                        *s = name.clone();
                                        changed = true;
                                    }
                                }
                            });
                    }
                    if let Some(g) = &mut p.graph {
                        egui::ComboBox::from_id_salt(("ph-graph", n))
                            .width(100.0)
                            .selected_text(g.to_string())
                            .show_ui(ui, |ui| {
                                for other in graphs.iter().filter(|x| **x != own) {
                                    if ui
                                        .selectable_label(*g == *other, other.to_string())
                                        .clicked()
                                    {
                                        *g = *other;
                                        changed = true;
                                    }
                                }
                            });
                    }
                });
                ui.horizontal(|ui| {
                    let mut on = p.immune_seconds.is_some();
                    if ui
                        .checkbox(&mut on, "Immune to crowd control for")
                        .on_hover_text("Needs `immune_state` in crowd_control.toml")
                        .changed()
                    {
                        p.immune_seconds = on.then(|| seconds(3.0));
                        changed = true;
                    }
                    if let Some(v) = &mut p.immune_seconds {
                        let mut s = as_f64(v);
                        if ui
                            .add(
                                egui::DragValue::new(&mut s)
                                    .range(0.0..=600.0)
                                    .speed(0.1)
                                    .suffix(" s"),
                            )
                            .changed()
                        {
                            *v = seconds(s);
                            changed = true;
                        }
                    }
                });
                let (to, remove) = ui.horizontal(|ui| move_buttons(ui, n, count)).inner;
                if let Some(to) = to {
                    moved = Some((n, to));
                }
                if remove {
                    removed = Some(n);
                }
            });
        if header.header_response.clicked() {
            *selected = if open { None } else { Some(n) };
        }
    }
    if let Some((a, b)) = moved {
        e.phases.swap(a, b);
        *selected = Some(b);
        changed = true;
    }
    if let Some(n) = removed {
        e.phases.remove(n);
        *selected = None;
        changed = true;
    }
    if ui
        .add_enabled(e.phases.len() < MAX_PHASES, egui::Button::new("Add phase"))
        .clicked()
    {
        let taken: Vec<String> = e.phases.iter().map(|p| p.name.clone()).collect();
        e.phases.push(Phase {
            name: unique_name(&taken, "phase"),
            when: Vec::new(),
            state: None,
            graph: None,
            immune_seconds: None,
        });
        *selected = Some(e.phases.len() - 1);
        changed = true;
    }
    changed
}

fn problems_panel(
    ui: &mut egui::Ui,
    problems: &[Problem],
    entry: &mut usize,
    state: &mut usize,
    phase: &mut Option<usize>,
) {
    ui.separator();
    ui.strong(format!("Problems in {AI_FILE} ({})", problems.len()));
    if problems.is_empty() {
        ui.colored_label(Color32::LIGHT_GREEN, "The engine accepts this file.");
    }
    for p in problems {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(RED, &p.message);
            if let Some(at) = p.at {
                if ui.small_button("Show").clicked() {
                    *entry = at.entry;
                    if let Some(s) = at.state {
                        *state = s;
                    }
                    *phase = at.phase;
                }
            }
        });
    }
}

#[cfg(test)]
mod cooldown_tests {
    use super::*;
    #[test]
    fn adding_a_cooldown_keeps_its_name_and_time_visible() {
        let path =
            std::env::temp_dir().join(format!("studio-cooldown-input-{}", std::process::id()));
        let script = serde_json::from_value(serde_json::json!({"steps":[
            {"do":"click", "text":"Cooldowns (0)"},
            {"do":"click", "text":"Add cooldown"},
            {"do":"wait", "frames":20}
        ]}))
        .unwrap();
        let mut runner = crate::input_script::Runner::new(script, &path).unwrap();
        let ctx = egui::Context::default();
        let mut entry = Entry::new_graph(148, "Warden");
        for frame in 0..150 {
            let mut raw = egui::RawInput {
                time: Some(frame as f64 / 60.),
                ..Default::default()
            };
            runner.input(&mut raw);
            let _ = ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    cooldowns_form(ui, &mut entry);
                });
                runner.observe(ctx).unwrap();
            });
            if runner.finished {
                break;
            }
        }
        assert!(
            runner.finished && runner.error.is_none(),
            "{:?}",
            runner.error
        );
        assert_eq!(entry.cooldowns.len(), 1);
        assert!(
            runner.visible().iter().any(|p| p.text == "cooldown"),
            "new cooldown collapsed before it could be named"
        );
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn a_condition_can_be_found_and_selected_without_scrolling_a_long_palette() {
        let path =
            std::env::temp_dir().join(format!("studio-ai-filter-input-{}", std::process::id()));
        let script = serde_json::from_value(serde_json::json!({"steps":[
            {"do":"click","text":"+ Always — add a condition"},
            {"do":"fill","text":"Find behaviour","value":"cooldown","offset":[0,22]},
            {"do":"wait","frames":30},
            {"do":"click","text":"Cooldown ready"}
        ]}))
        .unwrap();
        let mut runner = crate::input_script::Runner::new(script, &path).unwrap();
        let ctx = egui::Context::default();
        let mut conditions = Vec::new();
        for frame in 0..150 {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360., 240.),
                )),
                time: Some(frame as f64 / 60.),
                ..Default::default()
            };
            runner.input(&mut raw);
            let _ = ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    conditions_ui(
                        ui,
                        "test-conditions",
                        &mut conditions,
                        &["pulse".into()],
                        &Names::default(),
                    );
                });
                runner.observe(ctx).unwrap();
            });
            if runner.finished {
                break;
            }
        }
        assert!(
            runner.finished && runner.error.is_none(),
            "step {}: {:?}",
            runner.step,
            runner.error
        );
        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].kind, "cooldown_ready");
        assert!(
            !ctx.memory(|m| m.any_popup_open()),
            "selection must close the palette so the form can be edited"
        );
        std::fs::remove_dir_all(path).unwrap();
    }
}
