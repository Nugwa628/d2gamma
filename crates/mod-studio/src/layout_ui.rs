//! Drag/drop editor for the same validated panel files the game loads.
use crate::layout::{self, Document, Layouts};
use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke};
use std::path::Path;
use toml::{Table, Value};

#[derive(Default)]
pub struct LayoutEditor {
    pub synced_choices: Vec<String>,
    pub network_actions: Vec<String>,
    pub layouts: Option<Layouts>,
    pub error: String,
    pub selected: usize,
    pub element: Option<usize>,
    pub four_k: bool,
    new_name: String,
    palette_drag: Option<String>,
    drag_origin: Option<(i32, i32)>,
    raw: String,
    sample: i32,
    zoom: usize,
    binding_search: String,
}
impl LayoutEditor {
    pub fn open(&mut self, root: &Path) {
        *self = Self::default();
        match Layouts::open(root) {
            Ok(m) => self.layouts = Some(m),
            Err(e) => self.error = format!("{e:#}"),
        }
    }
    pub fn dirty(&self) -> bool {
        self.layouts.as_ref().is_some_and(Layouts::dirty)
    }
    pub fn save(&mut self) -> anyhow::Result<()> {
        match &mut self.layouts {
            Some(m) => m.save(),
            None if self.error.is_empty() => Ok(()),
            None => Err(anyhow::anyhow!(self.error.clone())),
        }
    }
    pub fn draw(&mut self, ui: &mut egui::Ui, tables: Option<&crate::tables::Tables>) {
        let Some(mut model) = self.layouts.take() else {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
            return;
        };
        self.draw_model(ui, &mut model, tables);
        self.layouts = Some(model);
    }
    fn draw_model(
        &mut self,
        ui: &mut egui::Ui,
        model: &mut Layouts,
        tables: Option<&crate::tables::Tables>,
    ) {
        let names: Vec<_> = model
            .documents
            .iter()
            .map(|d| {
                format!(
                    "{}{}",
                    layout::text(&d.table, "name"),
                    if d.dirty { " •" } else { "" }
                )
            })
            .collect();
        let previous = self.selected;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("panel-document")
                .selected_text(
                    names
                        .get(self.selected)
                        .map_or("Choose panel", String::as_str),
                )
                .show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.selected, i, n);
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.new_name)
                    .hint_text("New panel name")
                    .desired_width(150.),
            );
            if ui.button("New panel").clicked() {
                match model.create(&self.new_name) {
                    Ok(i) => {
                        self.selected = i;
                        self.element = None;
                        self.error.clear();
                    }
                    Err(e) => self.error = format!("{e:#}"),
                }
            }
            if ui.button("Save panels").clicked() {
                match model.save() {
                    Ok(()) => self.error.clear(),
                    Err(e) => self.error = format!("{e:#}"),
                }
            }
        });
        if previous != self.selected {
            self.element = None;
            self.raw.clear();
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.four_k, false, "1280 × 720");
            ui.selectable_value(&mut self.four_k, true, "3840 × 2160");
            egui::ComboBox::from_id_salt("layout-zoom")
                .selected_text(["Fit", "25%", "50%", "100%"][self.zoom])
                .show_ui(ui, |ui| {
                    for (i, n) in ["Fit", "25%", "50%", "100%"].iter().enumerate() {
                        ui.selectable_value(&mut self.zoom, i, *n);
                    }
                });
        });
        ui.weak("Drag an element onto the canvas, then drag to position it. Coordinates remain 1280 × 720. Images show their bounds; text uses the Studio font.");
        if !self.error.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
        }
        let Some(doc) = model.documents.get_mut(self.selected) else {
            return;
        };
        ui.horizontal(|ui| {
            for kind in ["frame", "text", "bar", "button", "image"] {
                let r = ui.add(egui::Button::new(kind).sense(Sense::click_and_drag()));
                if r.drag_started() {
                    self.palette_drag = Some(kind.into());
                }
                if r.clicked() {
                    match doc.add(kind, 570, 345) {
                        Ok(i) => self.element = Some(i),
                        Err(e) => self.error = format!("{e:#}"),
                    }
                }
            }
        });
        let canvas_width = (ui.available_width() - 310.).max(200.);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(canvas_width, ui.available_height().max(300.)),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::both()
                        .id_salt("layout-canvas-scroll")
                        .max_width(canvas_width)
                        .show(ui, |ui| self.canvas(ui, doc, canvas_width));
                },
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(285., ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("layout-properties")
                        .show(ui, |ui| self.properties(ui, doc, tables));
                },
            );
        });
    }
    fn canvas(&mut self, ui: &mut egui::Ui, doc: &mut Document, available: f32) {
        let output = if self.four_k {
            (3840., 2160.)
        } else {
            (1280., 720.)
        };
        let scale = [available / output.0, 0.25, 0.5, 1.][self.zoom];
        let size = egui::vec2(output.0 * scale, output.1 * scale);
        let (canvas, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let painter = ui.painter_at(canvas);
        painter.rect_filled(canvas, 0., Color32::from_rgb(19, 23, 30));
        for x in (0..=1280).step_by(80) {
            let x = canvas.left() + x as f32 / 1280. * size.x;
            painter.line_segment(
                [egui::pos2(x, canvas.top()), egui::pos2(x, canvas.bottom())],
                Stroke::new(1_f32, Color32::from_gray(38)),
            );
        }
        for y in (0..=720).step_by(80) {
            let y = canvas.top() + y as f32 / 720. * size.y;
            painter.line_segment(
                [egui::pos2(canvas.left(), y), egui::pos2(canvas.right(), y)],
                Stroke::new(1_f32, Color32::from_gray(38)),
            );
        }
        let rect_of = |e: &d2_modkit::ui::Element| {
            egui::Rect::from_min_size(
                canvas.min + egui::vec2(e.x as f32 / 1280. * size.x, e.y as f32 / 720. * size.y),
                egui::vec2(
                    e.width as f32 / 1280. * size.x,
                    e.height as f32 / 720. * size.y,
                ),
            )
        };
        if let Ok(def) = doc.definition() {
            for (i, e) in def.elements.iter().enumerate() {
                let r = rect_of(e);
                let (label, fill, color) = e
                    .binding
                    .as_ref()
                    .map_or((e.text.clone(), e.value, e.color), |b| {
                        b.present(self.sample, None, e.color)
                    });
                let c = Color32::from_gray(color.max(70));
                match e.kind {
                    d2_modkit::ui::Kind::Frame => {
                        painter.rect_stroke(r, 0., Stroke::new(1_f32, c), egui::StrokeKind::Inside);
                    }
                    d2_modkit::ui::Kind::Bar => {
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                r.min,
                                egui::vec2(r.width() * fill as f32 / 1000., r.height()),
                            ),
                            0.,
                            c,
                        );
                    }
                    d2_modkit::ui::Kind::Button => {
                        painter.rect_filled(r, 2., Color32::from_gray(70));
                    }
                    d2_modkit::ui::Kind::Image => {
                        painter.rect_stroke(
                            r,
                            0.,
                            Stroke::new(1_f32, Color32::LIGHT_BLUE),
                            egui::StrokeKind::Inside,
                        );
                        painter.text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            &e.image,
                            FontId::proportional(10.),
                            Color32::LIGHT_BLUE,
                        );
                    }
                    _ => {}
                }
                if matches!(
                    e.kind,
                    d2_modkit::ui::Kind::Text | d2_modkit::ui::Kind::Button
                ) {
                    painter.text(
                        r.left_bottom(),
                        Align2::LEFT_BOTTOM,
                        label,
                        FontId::proportional((16. * size.x / 1280.).max(8.)),
                        Color32::WHITE,
                    );
                }
                if self.element == Some(i) {
                    painter.rect_stroke(
                        r.expand(2.),
                        0.,
                        Stroke::new(2_f32, Color32::YELLOW),
                        egui::StrokeKind::Outside,
                    );
                }
            }
            if response.clicked() || response.drag_started() {
                if let Some(p) = response.interact_pointer_pos() {
                    self.element = def.elements.iter().rposition(|e| rect_of(e).contains(p));
                    self.drag_origin = self
                        .element
                        .map(|i| (i32::from(def.elements[i].x), i32::from(def.elements[i].y)));
                }
            }
        }
        if response.dragged() {
            if let (Some(i), Some((x, y))) = (self.element, self.drag_origin) {
                let delta = response.drag_delta();
                doc.move_element(
                    i,
                    x + (delta.x * 1280. / size.x).round() as i32,
                    y + (delta.y * 720. / size.y).round() as i32,
                );
            }
        }
        if ui.input(|i| i.pointer.any_released()) {
            if let Some(kind) = self.palette_drag.take() {
                if let Some(p) = ui
                    .input(|i| i.pointer.latest_pos())
                    .filter(|p| canvas.contains(*p))
                {
                    let (x, y) =
                        standard_point(p.x - canvas.left(), p.y - canvas.top(), size.x, size.y);
                    match doc.add(&kind, x, y) {
                        Ok(i) => self.element = Some(i),
                        Err(e) => self.error = format!("{e:#}"),
                    }
                }
            }
            self.drag_origin = None;
        }
    }
    fn properties(
        &mut self,
        ui: &mut egui::Ui,
        doc: &mut Document,
        tables: Option<&crate::tables::Tables>,
    ) {
        ui.heading("Properties");
        doc.dirty |= string_field(ui, &mut doc.table, "name", "Panel name");
        let mut replacement = layout::text(&doc.table, "replace").to_owned();
        let old = replacement.clone();
        egui::ComboBox::from_id_salt("replace-panel")
            .selected_text(if replacement.is_empty() {
                "Added panel"
            } else {
                &replacement
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut replacement, String::new(), "Added panel");
                for name in d2_modkit::ui::BUILTIN_PANELS {
                    ui.selectable_value(&mut replacement, (*name).to_owned(), *name);
                }
            });
        if old != replacement {
            if replacement.is_empty() {
                doc.table.remove("replace");
            } else {
                doc.table
                    .insert("replace".into(), Value::String(replacement));
            }
            doc.dirty = true;
        }
        let labels: Vec<_> = doc
            .elements()
            .iter()
            .map(|v| {
                v.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_owned()
            })
            .collect();
        egui::ComboBox::from_id_salt("selected-element")
            .selected_text(
                self.element
                    .and_then(|i| labels.get(i))
                    .map_or("Choose element", String::as_str),
            )
            .show_ui(ui, |ui| {
                for (i, n) in labels.iter().enumerate() {
                    ui.selectable_value(&mut self.element, Some(i), n);
                }
            });
        let mut actions: Vec<String> = doc
            .table
            .get("actions")
            .and_then(Value::as_array)
            .map_or_else(Vec::new, |a| {
                a.iter()
                    .filter_map(|v| v.get("name").and_then(Value::as_str).map(str::to_owned))
                    .collect()
            });
        actions.extend(self.network_actions.iter().cloned());
        let mut choices = binding_choices(tables, &doc.table);
        choices.extend(self.synced_choices.iter().cloned());
        choices.sort();
        choices.dedup();
        if let Some(i) = self.element {
            let mut changed = false;
            if let Some(e) = doc.element_mut(i) {
                changed |= string_field(ui, e, "id", "ID");
                for (key, label, default, max) in [
                    ("x", "X", 0, 1279),
                    ("y", "Y", 0, 719),
                    ("width", "Width", 100, 1280),
                    ("height", "Height", 20, 720),
                    ("color", "Color", 255, 255),
                    ("value", "Fill / 1000", 1000, 1000),
                    ("frame", "Image frame", 0, 65535),
                ] {
                    changed |= number_field(ui, e, key, label, default, max);
                }
                changed |= string_field(ui, e, "text", "Text");
                changed |= string_field(ui, e, "image", "Image");
                if layout::text(e, "kind") == "button" {
                    let mut a = layout::text(e, "action").to_owned();
                    let old = a.clone();
                    egui::ComboBox::from_id_salt("button-action")
                        .selected_text(if a.is_empty() { "No action" } else { &a })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut a, String::new(), "No action");
                            for name in &actions {
                                ui.selectable_value(&mut a, name.clone(), name);
                            }
                        });
                    if a != old {
                        if a.is_empty() {
                            e.remove("action");
                        } else {
                            e.insert("action".into(), Value::String(a));
                        }
                        changed = true;
                    }
                }
                let mut enabled = e.contains_key("bind");
                if ui.checkbox(&mut enabled, "Live binding").changed() {
                    if enabled {
                        e.insert(
                            "bind".into(),
                            Value::Table(Table::from_iter([(
                                "source".into(),
                                Value::String("var.combat/value".into()),
                            )])),
                        );
                    } else {
                        e.remove("bind");
                    }
                    changed = true;
                }
                if let Some(b) = e.get_mut("bind").and_then(Value::as_table_mut) {
                    changed |= string_field(ui, b, "source", "Source");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.binding_search)
                            .hint_text("Find binding"),
                    );
                    egui::ComboBox::from_id_salt("binding-picker")
                        .height(250.)
                        .selected_text("Pick binding…")
                        .show_ui(ui, |ui| {
                            for source in choices.iter().filter(|s| {
                                s.to_ascii_lowercase()
                                    .contains(&self.binding_search.to_ascii_lowercase())
                            }) {
                                if ui.selectable_label(false, source).clicked() {
                                    b.insert("source".into(), Value::String(source.clone()));
                                    changed = true;
                                }
                            }
                        });
                    changed |= string_field(ui, b, "maximum_source", "Maximum source");
                    if layout::text(b, "maximum_source").is_empty() {
                        b.remove("maximum_source");
                    }
                    let mut fmt = if layout::text(b, "format").is_empty() {
                        "integer".into()
                    } else {
                        layout::text(b, "format").to_owned()
                    };
                    let old = fmt.clone();
                    egui::ComboBox::from_id_salt("binding-format")
                        .selected_text(&fmt)
                        .show_ui(ui, |ui| {
                            for name in ["integer", "percent", "range"] {
                                ui.selectable_value(&mut fmt, name.into(), name);
                            }
                        });
                    if fmt != old {
                        b.insert("format".into(), Value::String(fmt));
                        changed = true;
                    }
                    for (key, label, default, max) in [
                        ("minimum", "Minimum", 0, i32::MAX as i64),
                        ("maximum", "Maximum", 1000, i32::MAX as i64),
                        ("divisor", "Divisor", 1, i32::MAX as i64),
                    ] {
                        changed |= number_field(ui, b, key, label, default, max);
                    }
                    ui.horizontal(|ui| {
                        ui.label("Preview value");
                        ui.add(egui::DragValue::new(&mut self.sample));
                    });
                }
            }
            if changed {
                if let Some(e) = doc.elements().get(i).and_then(Value::as_table) {
                    let x = layout::integer(e, "x", 0) as i32;
                    let y = layout::integer(e, "y", 0) as i32;
                    doc.move_element(i, x, y);
                }
            }
            doc.dirty |= changed;
            if ui.button("Delete element").clicked() {
                doc.remove(i);
                self.element = None;
            }
        }
        if let Err(e) = doc.definition() {
            ui.colored_label(Color32::LIGHT_RED, e.to_string());
        }
        egui::CollapsingHeader::new("Actions, quest names and advanced TOML").show(ui, |ui| {
            if ui.button("Read current document").clicked() || self.raw.is_empty() {
                self.raw = toml::to_string_pretty(&doc.table).unwrap_or_default();
            }
            ui.add(
                egui::TextEdit::multiline(&mut self.raw)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(12)
                    .desired_width(260.),
            );
            if ui.button("Apply document").clicked() {
                match Document::parse(doc.path.clone(), &self.raw) {
                    Ok(d) => {
                        doc.table = d.table;
                        doc.dirty = true;
                        self.element = None;
                        self.error.clear();
                    }
                    Err(e) => self.error = e.to_string(),
                }
            }
        });
    }
}

pub fn standard_point(x: f32, y: f32, w: f32, h: f32) -> (i32, i32) {
    (
        (x * 1280. / w).round() as i32,
        (y * 720. / h).round() as i32,
    )
}
fn string_field(ui: &mut egui::Ui, t: &mut Table, key: &str, label: &str) -> bool {
    let mut value = layout::text(t, key).to_owned();
    let changed = ui
        .horizontal(|ui| {
            ui.label(label);
            ui.add(egui::TextEdit::singleline(&mut value).desired_width(160.))
                .changed()
        })
        .inner;
    if changed {
        if value.is_empty() {
            t.remove(key);
        } else {
            t.insert(key.into(), Value::String(value));
        }
    }
    changed
}
fn number_field(
    ui: &mut egui::Ui,
    t: &mut Table,
    key: &str,
    label: &str,
    default: i64,
    max: i64,
) -> bool {
    let mut value = layout::integer(t, key, default);
    let min = if matches!(key, "minimum" | "maximum") {
        i64::from(i32::MIN)
    } else if matches!(key, "width" | "height" | "divisor") {
        1
    } else {
        0
    };
    let changed = ui
        .horizontal(|ui| {
            ui.label(label);
            ui.add(egui::DragValue::new(&mut value).range(min..=max))
                .changed()
        })
        .inner;
    if changed {
        t.insert(key.into(), Value::Integer(value));
    }
    changed
}
fn binding_choices(tables: Option<&crate::tables::Tables>, panel: &Table) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(t) = tables {
        for (table, column, kinds) in [
            ("itemstatcost", "stat", &["stat"][..]),
            ("states", "state", &["state", "stacks"][..]),
        ] {
            if let Some(t) = t
                .edited
                .tables
                .iter()
                .find(|t| t.name().eq_ignore_ascii_case(table))
            {
                if let Some(c) = t
                    .header
                    .iter()
                    .position(|h| h.eq_ignore_ascii_case(column.as_bytes()))
                {
                    for (r, row) in t.rows.iter().enumerate() {
                        if t.disabled(r) {
                            continue;
                        }
                        let n = crate::tables::text(&row.cells[c]);
                        if !n.is_empty() {
                            for kind in kinds {
                                out.push(format!("{kind}.{n}"));
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(a) = panel.get("quests").and_then(Value::as_array) {
        for q in a {
            if let Some(n) = q.get("name").and_then(Value::as_str) {
                out.push(format!("quest.{n}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drag_positions_and_deltas_are_equal_in_720p_and_4k() {
        assert_eq!(standard_point(600., 300., 1280., 720.), (600, 300));
        assert_eq!(standard_point(1800., 900., 3840., 2160.), (600, 300));
        assert_eq!(standard_point(150., 75., 640., 360.), (300, 150));
    }
    #[test]
    fn editor_frame_paints_valid_documents_without_game_assets() {
        let mut editor = LayoutEditor::default();
        let mut m = Layouts::default();
        let i = m.create("preview").unwrap();
        m.documents[i].add("bar", 20, 30).unwrap();
        editor.layouts = Some(m);
        editor.element = Some(0);
        let ctx = egui::Context::default();
        for four_k in [false, true] {
            editor.four_k = four_k;
            let out = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1500., 1000.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| editor.draw(ui, None));
                },
            );
            assert!(!out.shapes.is_empty());
            assert!(editor.error.is_empty());
        }
    }
}
