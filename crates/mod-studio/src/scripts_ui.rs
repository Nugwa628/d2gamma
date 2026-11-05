//! Script file picker, Luau colors and server-event stubs.
use crate::scripts::{self, Kind, Scripts};
use d2_sim::rules::scripts::EventKind;
use eframe::egui::{self, Color32, FontId, TextFormat};
use std::path::Path;

pub struct ScriptEditor {
    pub scripts: Scripts,
    pub error: String,
    selected: usize,
    new_name: String,
    event: EventKind,
}
impl Default for ScriptEditor {
    fn default() -> Self {
        Self {
            scripts: Scripts::default(),
            error: String::new(),
            selected: 0,
            new_name: "main.luau".into(),
            event: EventKind::SkillUsed,
        }
    }
}
impl ScriptEditor {
    pub fn open(&mut self, root: &Path) {
        *self = Self::default();
        match Scripts::open(root) {
            Ok(scripts) => self.scripts = scripts,
            Err(e) => self.error = format!("Could not open scripts: {e:#}"),
        }
    }
    pub fn dirty(&self) -> bool {
        self.scripts.dirty()
    }
    pub fn save(&mut self) -> anyhow::Result<bool> {
        self.scripts.save()
    }
    pub fn select_path(&mut self, project: &Path, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(project.join("files/scripts")) else {
            return false;
        };
        let name = relative.to_string_lossy().replace('\\', "/");
        if let Some(i) = self.scripts.files.iter().position(|b| b.name == name) {
            self.selected = i;
            true
        } else {
            false
        }
    }
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        ui.label("Server-side Luau · files/scripts · API v1");
        ui.weak("Save runs the game's Check command. Configure the game executable and native install below.");
        ui.horizontal(|ui| {
            ui.label("New module");
            ui.add(egui::TextEdit::singleline(&mut self.new_name).desired_width(220.0));
            if ui.button("Add module").clicked() {
                match self.scripts.add(self.new_name.trim()) {
                    Ok(i) => {
                        self.selected = i;
                        self.error.clear();
                    }
                    Err(e) => self.error = format!("{e:#}"),
                }
            }
        });
        if !self.error.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
        }
        ui.separator();
        if ui
            .button("Reload selected from disk (discard its edits)")
            .clicked()
        {
            self.error = self
                .scripts
                .reload(self.selected)
                .err()
                .map_or_else(String::new, |e| e.to_string());
        }
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(egui::vec2(220.0, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
            egui::ScrollArea::vertical().id_salt("script-files").show(ui, |ui| {
                for (i, file) in self.scripts.files.iter().enumerate() {
                    if ui.selectable_label(self.selected == i, format!("{}{}", file.name, if file.dirty() { " •" } else { "" })).clicked() {
                        self.selected = i;
                    }
                }
            });
            });
            ui.separator();
            ui.vertical(|ui| {
            let Some(buffer) = self.scripts.files.get_mut(self.selected) else {
                ui.label("Add a module to start, or open a project containing .luau files.");
                return;
            };
            ui.strong(&buffer.name);
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("script-event").selected_text(self.event.name()).show_ui(ui, |ui| {
                    for event in EventKind::ALL { ui.selectable_value(&mut self.event, event, event.name()); }
                });
                if ui.button("Insert event handler").clicked() {
                    self.error = scripts::insert_event(&mut buffer.text, self.event).err().map_or_else(String::new, |e| e.to_string());
                }
            });
            ui.weak("Handlers receive (e, api). Use a final return M or return { ... } for stub insertion.");
            let mut layouter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
                let mut job = highlight(text);
                job.wrap.max_width = wrap_width;
                ui.fonts(|fonts| fonts.layout_job(job))
            };
            egui::ScrollArea::both().id_salt(("script-source", &buffer.name)).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut buffer.text).id_salt(("script-buffer", &buffer.name)).code_editor().desired_width(f32::INFINITY).desired_rows(28).layouter(&mut layouter));
            });
            });
        });
    }
}

pub fn highlight(text: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for token in scripts::tokens(text) {
        let color = match token.kind {
            Kind::Plain => Color32::from_rgb(220, 220, 220),
            Kind::Keyword => Color32::from_rgb(170, 150, 255),
            Kind::String => Color32::from_rgb(170, 215, 150),
            Kind::Comment => Color32::from_rgb(130, 160, 145),
            Kind::Number => Color32::from_rgb(240, 195, 130),
        };
        job.append(
            &text[token.start..token.end],
            0.0,
            TextFormat {
                font_id: FontId::monospace(14.0),
                color,
                ..Default::default()
            },
        );
    }
    job
}
