//! Node canvas, typed wiring and generated-source/trace views for visual scripts.
use crate::graphs::{self, Graphs};
use d2_modkit::graph::{self, Graph, Input, Node, Type};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};
use std::{collections::BTreeMap, path::Path};
#[derive(Default)]
pub struct GraphEditor {
    pub graphs: Option<Graphs>,
    pub error: String,
    pub selected: usize,
    pub selected_node: Option<String>,
    name: String,
    rename: String,
    rename_source: String,
    node_filter: String,
    source_filter: String,
    target_filter: String,
    search: String,
    pending: Option<(String, String, bool)>,
    pub event: usize,
    event_id: i32,
    pub seed: u32,
    pub trace: Option<crate::graph_testfire::Trace>,
    cursor: usize,
    raw: String,
    pub generated: bool,
}
fn color(ty: Option<Type>) -> Color32 {
    match ty {
        Some(Type::Integer) => Color32::from_rgb(240, 180, 90),
        Some(Type::Boolean) => Color32::from_rgb(210, 130, 230),
        Some(Type::Text) => Color32::from_rgb(120, 210, 145),
        Some(Type::Unit) => Color32::from_rgb(100, 185, 245),
        None => Color32::LIGHT_GRAY,
    }
}
impl GraphEditor {
    pub fn open(&mut self, root: &Path) {
        *self = Self::default();
        self.name = "main.graph.toml".into();
        self.seed = 1;
        match Graphs::open(root) {
            Ok(g) => self.graphs = Some(g),
            Err(e) => self.error = e.to_string(),
        }
    }
    pub fn dirty(&self) -> bool {
        self.graphs.as_ref().is_some_and(Graphs::dirty)
    }
    pub fn save(&mut self) -> anyhow::Result<bool> {
        match &mut self.graphs {
            Some(g) => g.save(),
            None if self.error.is_empty() => Ok(false),
            None => Err(anyhow::anyhow!(self.error.clone())),
        }
    }
    pub fn select_path(&mut self, project: &Path, path: &Path) -> bool {
        let Some(graphs) = &self.graphs else {
            return false;
        };
        let Ok(relative) = path.strip_prefix(project.join("files/scripts")) else {
            return false;
        };
        if let Some(i) = graphs
            .documents
            .iter()
            .position(|d| d.name == relative.to_string_lossy().replace('\\', "/"))
        {
            self.selected = i;
            self.selected_node = None;
            self.pending = None;
            self.trace = None;
            self.error.clear();
            true
        } else {
            false
        }
    }
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let Some(mut files) = self.graphs.take() else {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
            return;
        };
        self.draw_files(ui, &mut files);
        self.graphs = Some(files);
    }
    fn draw_files(&mut self, ui: &mut egui::Ui, files: &mut Graphs) {
        ui.label("Visual scripts · server events · API v1");
        let previous = self.selected;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("graph-file")
                .selected_text(
                    files
                        .documents
                        .get(self.selected)
                        .map_or("Choose graph", |d| d.name.as_str()),
                )
                .show_ui(ui, |ui| {
                    for (i, d) in files.documents.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.selected,
                            i,
                            format!("{}{}", d.name, if d.dirty() { " •" } else { "" }),
                        );
                    }
                });
            ui.label("Graph file name");
            ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(150.));
            if ui.button("New graph").clicked() {
                match files.add(self.name.trim()) {
                    Ok(i) => {
                        self.selected = i;
                        self.error.clear();
                    }
                    Err(e) => self.error = e.to_string(),
                }
            }
            if ui.button("Reload graph (discard edits)").clicked() {
                match files.reload(self.selected) {
                    Err(e) => self.error = e.to_string(),
                    Ok(()) => {
                        self.error.clear();
                        self.trace = None;
                        self.pending = None;
                        self.selected_node = None;
                        self.raw.clear();
                    }
                }
            }
        });
        if previous != self.selected {
            self.pending = None;
            self.selected_node = None;
            self.trace = None;
            self.raw.clear();
        }
        if !self.error.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
        }
        let Some(doc) = files.documents.get_mut(self.selected) else {
            return;
        };
        let mut g = match doc.graph() {
            Ok(g) => g,
            Err(e) => {
                ui.colored_label(Color32::LIGHT_RED, e.to_string());
                ui.add(
                    egui::TextEdit::multiline(&mut doc.text)
                        .code_editor()
                        .desired_rows(18),
                );
                return;
            }
        };
        ui.horizontal(|ui| {
            ui.label("Selected node");
            egui::ComboBox::from_id_salt("selected-graph-node")
                .selected_text(self.selected_node.as_deref().unwrap_or("Choose node"))
                .show_ui(ui, |ui| {
                    for n in g.nodes.iter().filter(|n| n.id.contains(&self.node_filter)) {
                        ui.selectable_value(&mut self.selected_node, Some(n.id.clone()), &n.id);
                    }
                });
            ui.label("Find node");
            ui.add(egui::TextEdit::singleline(&mut self.node_filter).desired_width(140.));
        });
        let before = g.clone();
        let error = g.compile().err();
        if let Some(e) = &error {
            ui.colored_label(Color32::LIGHT_RED, e.to_string());
        } else {
            ui.weak("Graph compiles. Orange integer · purple Boolean · green text · blue unit · white control");
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("fake-event")
                .selected_text(graph::EVENTS[self.event])
                .show_ui(ui, |ui| {
                    for (i, event) in graph::EVENTS.iter().enumerate() {
                        ui.selectable_value(&mut self.event, i, *event);
                    }
                });
            ui.label("ID");
            ui.add(egui::DragValue::new(&mut self.event_id));
            ui.label("Seed");
            ui.add(egui::DragValue::new(&mut self.seed));
            if ui
                .add_enabled(error.is_none(), egui::Button::new("Test-fire fake unit"))
                .clicked()
            {
                match crate::graph_testfire::fire(
                    &g,
                    graph::EVENTS[self.event],
                    self.event_id,
                    self.seed,
                ) {
                    Ok(t) => {
                        self.error = t.error.clone().unwrap_or_default();
                        self.cursor = if t.error.is_some() {
                            t.nodes.len().saturating_sub(1)
                        } else {
                            0
                        };
                        self.trace = Some(t);
                    }
                    Err(e) => {
                        self.error = e;
                        self.trace = None;
                    }
                }
            }
            if ui.button("Next trace step").clicked() {
                if let Some(t) = &self.trace {
                    self.cursor = (self.cursor + 1).min(t.nodes.len().saturating_sub(1));
                }
            }
            ui.checkbox(&mut self.generated, "View generated Luau");
        });
        ui.weak("Fake world starts fresh with units 1 and 2. Spawn/damage/timer calls are recorded; this does not change a game.");
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(175., 500.),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.label("Node palette search");
                    ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search nodes"));
                    egui::ScrollArea::vertical()
                        .id_salt("graph-palette")
                        .max_height(650.)
                        .show(ui, |ui| {
                            let palette = graph::EVENTS
                                .iter()
                                .map(|e| ("event", *e))
                                .chain([
                                    ("constant", "constant"),
                                    ("math", "math"),
                                    ("condition", "condition"),
                                    ("branch", "branch"),
                                    ("loop", "loop"),
                                    ("variable", "variable"),
                                    ("return", "return"),
                                    ("call", "call"),
                                ])
                                .chain(graph::APIS.iter().map(|a| ("api", a.name)));
                            for (kind, name) in palette {
                                let label = format!("{kind}: {name}");
                                if label.contains(&self.search.to_ascii_lowercase())
                                    && ui.button(label).clicked()
                                {
                                    match graphs::add_node(&mut g, kind, name) {
                                        Ok(id) => self.selected_node = Some(id),
                                        Err(e) => self.error = e.to_string(),
                                    }
                                }
                            }
                        });
                },
            );
            ui.add_space(8.0);
            let width = (ui.available_width() - 260.).max(300.);
            ui.allocate_ui_with_layout(
                egui::vec2(width, 500.),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::both()
                        .id_salt("graph-canvas")
                        .max_height(500.)
                        .max_width(width)
                        .show(ui, |ui| {
                            self.canvas(ui, &mut g, error.as_ref().map(|e| e.node.as_str()))
                        });
                },
            );
            ui.add_space(8.0);
            ui.allocate_ui_with_layout(
                egui::vec2(240., 500.),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("graph-properties", self.selected_node.clone()))
                        .max_height(500.)
                        .show(ui, |ui| self.properties(ui, &mut g));
                },
            );
        });
        if g != before {
            if let Err(e) = doc.replace(&g) {
                self.error = e.to_string();
            }
            self.trace = None;
            self.error.clear();
        }
        if self.generated {
            egui::CollapsingHeader::new("Generated Luau (read only)")
                .default_open(true)
                .show(ui, |ui| match g.compile() {
                    Ok(source) => {
                        egui::ScrollArea::both()
                            .id_salt("graph-luau")
                            .max_height(250.)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(egui::RichText::new(source).monospace())
                                        .selectable(true),
                                );
                            });
                    }
                    Err(e) => {
                        ui.label(e.to_string());
                    }
                });
        }
        if let Some(t) = &self.trace {
            egui::CollapsingHeader::new(format!(
                "Step trace: {} nodes, {} API calls",
                t.nodes.len(),
                t.calls.len()
            ))
            .default_open(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("graph-trace")
                    .max_height(160.)
                    .show(ui, |ui| {
                        for (i, node) in t.nodes.iter().enumerate() {
                            if ui
                                .selectable_label(
                                    i == self.cursor,
                                    format!("{} · node {node}", i + 1),
                                )
                                .clicked()
                            {
                                self.cursor = i;
                                self.selected_node = Some(node.clone());
                            }
                        }
                        for call in &t.calls {
                            ui.monospace(call);
                        }
                    });
            });
        }
        egui::CollapsingHeader::new("Advanced graph TOML").show(ui, |ui| {
            if ui.button("Read graph").clicked() {
                self.raw = doc.text.clone();
            }
            ui.add(
                egui::TextEdit::multiline(&mut self.raw)
                    .code_editor()
                    .desired_rows(8),
            );
            if ui.button("Apply TOML").clicked() {
                match Graph::parse(&self.raw) {
                    Ok(_) => {
                        doc.text = self.raw.clone();
                        self.pending = None;
                        self.trace = None;
                        self.selected_node = None;
                    }
                    Err(e) => self.error = e.to_string(),
                }
            }
        });
    }
    fn canvas(&mut self, ui: &mut egui::Ui, g: &mut Graph, error: Option<&str>) {
        let max_x = g.nodes.iter().map(|n| n.x.max(0)).max().unwrap_or(0) as f32 + 260.;
        let max_y = g.nodes.iter().map(|n| n.y.max(0)).max().unwrap_or(0) as f32 + 350.;
        let (canvas, _) =
            ui.allocate_exact_size(egui::vec2(max_x.max(600.), max_y.max(450.)), Sense::hover());
        ui.painter()
            .rect_filled(canvas, 0., Color32::from_rgb(23, 27, 32));
        let mut positions = BTreeMap::new();
        let mut inputs = BTreeMap::new();
        let mut rects = BTreeMap::new();
        for n in &g.nodes {
            let pins = n.pins().unwrap_or_default();
            let outputs = outputs(n);
            let flows = n.flows();
            let rows = pins.len().max(outputs.len() + flows.len());
            let rect = Rect::from_min_size(
                canvas.min + egui::vec2(n.x.max(0) as f32, n.y.max(0) as f32),
                egui::vec2(220., 55. + rows as f32 * 22.),
            );
            rects.insert(n.id.clone(), rect);
            for (i, (pin, _)) in pins.iter().enumerate() {
                inputs.insert(
                    (n.id.clone(), pin.to_string()),
                    Pos2::new(rect.left() + 9., rect.top() + 51. + i as f32 * 22.),
                );
            }
            for (i, (pin, _)) in outputs.iter().enumerate() {
                positions.insert(
                    (n.id.clone(), pin.clone(), false),
                    Pos2::new(rect.right() - 9., rect.top() + 51. + i as f32 * 22.),
                );
            }
            for (i, (pin, _)) in flows.iter().enumerate() {
                positions.insert(
                    (n.id.clone(), pin.to_string(), true),
                    Pos2::new(
                        rect.right() - 9.,
                        rect.top() + 51. + (outputs.len() + i) as f32 * 22.,
                    ),
                );
            }
        }
        for n in &g.nodes {
            for (pin, v) in &n.inputs {
                if let Input::Link { node, pin: source } = v {
                    if let (Some(a), Some(b)) = (
                        positions.get(&(node.clone(), source.clone(), false)),
                        inputs.get(&(n.id.clone(), pin.clone())),
                    ) {
                        ui.painter().line_segment(
                            [*a, *b],
                            Stroke::new(2_f32, color(g.node(node).and_then(|n| n.output(source)))),
                        );
                    }
                }
            }
            for (pin, dest) in n.flows() {
                if let (Some(a), Some(b)) = (
                    positions.get(&(n.id.clone(), pin.into(), true)),
                    rects.get(dest),
                ) {
                    ui.painter().line_segment(
                        [*a, Pos2::new(b.left() + 9., b.top() + 16.)],
                        Stroke::new(2_f32, color(None)),
                    );
                }
            }
        }
        let mut wire = None;
        let mut flow = None;
        for n in &mut g.nodes {
            let rect = rects[&n.id];
            let traced = self
                .trace
                .as_ref()
                .and_then(|t| t.nodes.get(self.cursor))
                .is_some_and(|id| id == &n.id);
            let border = if error == Some(n.id.as_str()) {
                Color32::LIGHT_RED
            } else if traced {
                Color32::YELLOW
            } else if self.selected_node.as_ref() == Some(&n.id) {
                Color32::LIGHT_BLUE
            } else {
                Color32::GRAY
            };
            ui.painter().rect(
                rect,
                5.,
                Color32::from_rgb(42, 47, 55),
                Stroke::new(1.5_f32, border),
                egui::StrokeKind::Inside,
            );
            let title = Rect::from_min_size(rect.min, egui::vec2(rect.width(), 32.));
            let response = ui.interact(
                title,
                ui.id().with((&n.id, "title")),
                Sense::click_and_drag(),
            );
            if response.dragged() {
                let d = response.drag_delta();
                n.x = (n.x + d.x as i32).clamp(0, 10000);
                n.y = (n.y + d.y as i32).clamp(0, 10000);
            }
            if response.clicked() {
                self.selected_node = Some(n.id.clone());
                if let Some((source, pin, true)) = &self.pending {
                    flow = Some((source.clone(), pin.clone(), n.id.clone()));
                }
            }
            ui.painter().text(
                rect.min + egui::vec2(16., 8.),
                Align2::LEFT_TOP,
                format!(
                    "{} · {}",
                    n.id,
                    if n.kind == "api" {
                        n.name.as_str()
                    } else if n.kind == "event" {
                        n.event.as_str()
                    } else {
                        n.kind.as_str()
                    }
                ),
                FontId::proportional(13.),
                Color32::WHITE,
            );
            if n.executable() && !matches!(n.kind.as_str(), "event" | "call") {
                let p = rect.min + egui::vec2(9., 16.);
                ui.painter().circle_filled(p, 5., color(None));
            }
            for (pin, ty) in n.pins().unwrap_or_default() {
                let p = inputs[&(n.id.clone(), pin.into())];
                ui.painter().circle_filled(p, 5., color(Some(ty)));
                ui.painter().text(
                    p + egui::vec2(9., 0.),
                    Align2::LEFT_CENTER,
                    pin,
                    FontId::proportional(12.),
                    color(Some(ty)),
                );
                if ui
                    .interact(
                        Rect::from_center_size(p, egui::vec2(18., 18.)),
                        ui.id().with((&n.id, pin, "in")),
                        Sense::click(),
                    )
                    .clicked()
                {
                    if let Some((from, out, false)) = &self.pending {
                        wire = Some((from.clone(), out.clone(), n.id.clone(), pin.to_owned()));
                    }
                }
            }
            for (pin, ty, control) in outputs(n)
                .into_iter()
                .map(|(p, t)| (p, Some(t), false))
                .chain(n.flows().iter().map(|(p, _)| (p.to_string(), None, true)))
            {
                let p = positions[&(n.id.clone(), pin.clone(), control)];
                ui.painter().circle_filled(p, 5., color(ty));
                ui.painter().text(
                    p - egui::vec2(9., 0.),
                    Align2::RIGHT_CENTER,
                    &pin,
                    FontId::proportional(12.),
                    color(ty),
                );
                if ui
                    .interact(
                        Rect::from_center_size(p, egui::vec2(18., 18.)),
                        ui.id().with((&n.id, &pin, "out")),
                        Sense::click(),
                    )
                    .clicked()
                {
                    self.pending = Some((n.id.clone(), pin, control));
                }
            }
        }
        if let Some((from, pin, to, input)) = wire {
            self.error = graphs::connect(g, &from, &pin, &to, &input)
                .err()
                .map_or_else(String::new, |e| e.to_string());
            self.pending = None;
        }
        if let Some((from, pin, to)) = flow {
            if !matches!(g.node(&to).map(|n| n.kind.as_str()), Some("event" | "call")) {
                if let Some(n) = g.nodes.iter_mut().find(|n| n.id == from) {
                    match pin.as_str() {
                        "next" => n.next = to,
                        "yes" => n.yes = to,
                        "no" => n.no = to,
                        "body" => n.body = to,
                        _ => {}
                    }
                }
            }
            self.pending = None;
        }
        if let Some((node, pin, control)) = &self.pending {
            ui.label(format!(
                "Wiring {node}.{pin}: click {}",
                if *control {
                    "a node title"
                } else {
                    "a matching input pin"
                }
            ));
            if ui.button("Cancel wire").clicked() {
                self.pending = None;
            }
        }
    }
    fn properties(&mut self, ui: &mut egui::Ui, g: &mut Graph) {
        let Some(id) = self.selected_node.clone() else {
            ui.label(
                "Select a node title to edit. Click an output pin, then an input pin to wire.",
            );
            return;
        };
        if self.rename_source != id {
            self.rename = id.clone();
            self.rename_source = id.clone();
        }
        ui.horizontal(|ui| {
            ui.label("Node ID");
            ui.add(egui::TextEdit::singleline(&mut self.rename).desired_width(120.));
        });
        if ui.button("Rename node").clicked() {
            match graphs::rename_node(g, &id, self.rename.trim()) {
                Ok(()) => {
                    self.selected_node = Some(self.rename.trim().into());
                    self.pending = None;
                    self.error.clear();
                }
                Err(e) => self.error = e.to_string(),
            }
            return;
        }
        let sources = g
            .nodes
            .iter()
            .flat_map(|n| {
                outputs(n)
                    .into_iter()
                    .map(|(pin, ty)| (n.id.clone(), pin, ty))
            })
            .collect::<Vec<_>>();
        let destinations = g
            .nodes
            .iter()
            .filter(|n| n.executable() && !matches!(n.kind.as_str(), "event" | "call"))
            .map(|n| n.id.clone())
            .collect::<Vec<_>>();
        let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) else {
            return;
        };
        ui.horizontal(|ui| {
            ui.label("Position X");
            ui.add(egui::DragValue::new(&mut n.x).range(0..=10000));
        });
        ui.horizontal(|ui| {
            ui.label("Position Y");
            ui.add(egui::DragValue::new(&mut n.y).range(0..=10000));
        });
        ui.label(&n.kind);
        let mut remove = false;
        if ui.button("Delete node").clicked() {
            remove = true;
        }
        if n.kind == "event" {
            egui::ComboBox::from_id_salt("graph-event-kind")
                .selected_text(&n.event)
                .show_ui(ui, |ui| {
                    for event in graph::EVENTS {
                        ui.selectable_value(&mut n.event, event.into(), event);
                    }
                });
        }
        if matches!(n.kind.as_str(), "variable" | "call") {
            ui.label("Name");
            ui.text_edit_singleline(&mut n.name);
        }
        if matches!(n.kind.as_str(), "math" | "condition" | "variable") {
            let choices: &[&str] = match n.kind.as_str() {
                "math" => &[
                    "add", "subtract", "multiply", "divide", "modulo", "min", "max",
                ],
                "condition" => &[
                    "equal",
                    "less",
                    "less_equal",
                    "greater",
                    "greater_equal",
                    "and",
                    "or",
                    "not",
                    "exists",
                ],
                _ => &["get", "set"],
            };
            let old = n.op.clone();
            ui.label("Operation");
            egui::ComboBox::from_id_salt("graph-operation")
                .selected_text(&n.op)
                .show_ui(ui, |ui| {
                    for op in choices {
                        ui.selectable_value(&mut n.op, op.to_string(), *op);
                    }
                });
            if old != n.op {
                n.inputs.clear();
                if let Ok(pins) = n.pins() {
                    for (p, ty) in pins
                        .into_iter()
                        .map(|(p, t)| (p.to_owned(), t))
                        .collect::<Vec<_>>()
                    {
                        n.inputs.insert(p, graphs::default_input(ty, None));
                    }
                }
            }
        }
        if n.kind == "loop" {
            ui.label("Iterations");
            ui.add(egui::DragValue::new(&mut n.count).range(1..=64));
        }
        if n.kind == "api" {
            ui.label(&n.name);
        }
        if n.kind == "constant" {
            let mut kind = match n.inputs.get("value") {
                Some(Input::Boolean(_)) => 1,
                Some(Input::Text(_)) => 2,
                _ => 0,
            };
            let old = kind;
            egui::ComboBox::from_id_salt("constant-type")
                .selected_text(["Integer", "Boolean", "Text"][kind])
                .show_ui(ui, |ui| {
                    for (i, label) in ["Integer", "Boolean", "Text"].iter().enumerate() {
                        ui.selectable_value(&mut kind, i, *label);
                    }
                });
            if old != kind {
                n.inputs.insert(
                    "value".into(),
                    graphs::default_input([Type::Integer, Type::Boolean, Type::Text][kind], None),
                );
            }
        }
        let pin_types = n
            .pins()
            .unwrap_or_default()
            .into_iter()
            .map(|(p, t)| (p.to_owned(), t))
            .collect::<BTreeMap<_, _>>();
        for (name, value) in &mut n.inputs {
            let ty = pin_types.get(name).copied().unwrap_or(Type::Integer);
            let selected = match value {
                Input::Link { node, pin } => format!("{node}.{pin}"),
                _ => "Literal".into(),
            };
            ui.label(format!("Input {name}"));
            egui::ComboBox::from_id_salt((&id, name, "source"))
                .selected_text(selected)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show_ui(ui, |ui| {
                    ui.label("Find source");
                    ui.text_edit_singleline(&mut self.source_filter);
                    if ty != Type::Unit
                        && ui
                            .selectable_label(!matches!(value, Input::Link { .. }), "Literal")
                            .clicked()
                    {
                        *value = graphs::default_input(ty, None);
                        ui.memory_mut(|m| m.close_popup());
                    }
                    for (node, pin, source_ty) in &sources {
                        if *source_ty == ty
                            && node != &id
                            && format!("{node}.{pin}").contains(&self.source_filter)
                            && ui
                                .selectable_label(
                                    matches!(value,Input::Link{node:n,pin:p} if n==node && p==pin),
                                    format!("{node}.{pin}"),
                                )
                                .clicked()
                        {
                            *value = Input::Link {
                                node: node.clone(),
                                pin: pin.clone(),
                            };
                            ui.memory_mut(|m| m.close_popup());
                        }
                    }
                });
            match value {
                Input::Integer(v) => {
                    ui.horizontal(|ui| {
                        ui.label(format!("Value {name}"));
                        ui.add(egui::DragValue::new(v));
                    });
                }
                Input::Boolean(v) => {
                    ui.checkbox(v, format!("Value {name}"));
                }
                Input::Text(v) => {
                    ui.horizontal(|ui| {
                        ui.label(format!("Value {name}"));
                        ui.text_edit_singleline(v);
                    });
                }
                Input::Link { .. } => {}
            }
        }
        let flows = n
            .flows()
            .into_iter()
            .map(|(p, _)| p.to_owned())
            .collect::<Vec<_>>();
        for name in flows {
            let target = match name.as_str() {
                "next" => &mut n.next,
                "yes" => &mut n.yes,
                "no" => &mut n.no,
                _ => &mut n.body,
            };
            ui.label(format!("Control {name}"));
            egui::ComboBox::from_id_salt((&id, &name, "flow"))
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .selected_text(if target.is_empty() {
                    "End"
                } else {
                    target.as_str()
                })
                .show_ui(ui, |ui| {
                    ui.label("Find target");
                    ui.text_edit_singleline(&mut self.target_filter);
                    if ui.selectable_value(target, String::new(), "End").clicked() {
                        ui.memory_mut(|m| m.close_popup());
                    }
                    for to in destinations
                        .iter()
                        .filter(|to| to.contains(&self.target_filter))
                    {
                        if ui.selectable_value(target, to.clone(), to).clicked() {
                            ui.memory_mut(|m| m.close_popup());
                        }
                    }
                });
        }
        if remove {
            g.nodes.retain(|n| n.id != id);
            self.selected_node = None;
            self.pending = None;
        }
    }
}
fn outputs(n: &Node) -> Vec<(String, Type)> {
    let names: &[&str] = if matches!(n.kind.as_str(), "event" | "call") {
        &[
            "unit",
            "other",
            "id",
            "frame",
            "unit_type",
            "argument",
            "skill",
            "level",
        ]
    } else {
        &["value"]
    };
    names
        .iter()
        .filter_map(|pin| n.output(pin).map(|ty| (pin.to_string(), ty)))
        .collect()
}
