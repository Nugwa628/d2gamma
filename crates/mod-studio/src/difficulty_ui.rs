//! Difficulty names in their stable save and table order.
use crate::definitions::Document;
use eframe::egui;
use std::path::Path;
use toml::Value;
#[derive(Default)]
pub struct DifficultyEditor {
    pub document: Option<Document>,
}
impl DifficultyEditor {
    pub fn open(&mut self, root: &Path) {
        self.document = Some(Document::open(
            root,
            "files/difficulties.toml",
            "version=1\nnames=['Normal','Nightmare','Hell']",
            |s| {
                d2_sim::rules::difficulties::Difficulties::from_toml(s)
                    .map(|_| ())
                    .map_err(Into::into)
            },
        ));
    }
    pub fn dirty(&self) -> bool {
        self.document.as_ref().is_some_and(Document::dirty)
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        self.document.as_ref().map_or(Ok(()), Document::validate)
    }
    pub fn save(&mut self) -> anyhow::Result<()> {
        self.document.as_mut().map_or(Ok(()), Document::save)
    }
    pub fn draw(&mut self, ui: &mut egui::Ui, screen_only: bool) {
        if screen_only {
            ui.label("Difficulty definitions require a gameplay mod.");
            return;
        }
        let Some(doc) = &mut self.document else {
            return;
        };
        if let Some(error) = &doc.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
            return;
        }
        ui.label("Difficulty names · IDs are table row numbers");
        ui.weak("Append difficulties to preserve released saves. Add matching difficultylevels rows in the Table editor.");
        let names = doc.table.get_mut("names").unwrap().as_array_mut().unwrap();
        let can_remove = names.len() > 1;
        let mut remove = None;
        for (id, name) in names.iter_mut().enumerate() {
            ui.push_id(id, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Difficulty {id}"));
                    let mut text = name.as_str().unwrap().to_owned();
                    if ui.text_edit_singleline(&mut text).changed() {
                        *name = text.into();
                    }
                    if can_remove && ui.button("Remove difficulty").clicked() {
                        remove = Some(id);
                    }
                })
            });
        }
        if let Some(id) = remove {
            names.remove(id);
        }
        if names.len() < 255 && ui.button("Add difficulty").clicked() {
            let name = (names.len() + 1..)
                .map(|n| format!("Difficulty {n}"))
                .find(|n| !names.iter().any(|v| v.as_str() == Some(n)))
                .unwrap();
            names.push(Value::String(name));
        }
        if let Err(error) = doc.validate() {
            ui.colored_label(egui::Color32::LIGHT_RED, error.to_string());
        }
    }
}
