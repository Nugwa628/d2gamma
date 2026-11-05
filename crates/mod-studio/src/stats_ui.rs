//! Named stats and transformations, without requiring a TOML editor.
use crate::definitions::Document;
use eframe::egui;
use std::path::Path;
use toml::{Table, Value};

#[derive(Default)]
pub struct StatsEditor {
    pub document: Option<Document>,
}
impl StatsEditor {
    pub fn open(&mut self, root: &Path) {
        self.document = Some(Document::open(
            root,
            "files/stats.toml",
            "version=1",
            |text| {
                d2_sim::stats::named::StatRules::from_toml(text)
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
            ui.label("Named stats require a gameplay mod.");
            return;
        }
        let Some(doc) = &mut self.document else {
            return;
        };
        if let Some(error) = &doc.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
            return;
        }
        ui.label("Custom whole-number stats · stable save IDs 32768–65534");
        ui.weak("Keep released IDs stable. State, skill, script and network declarations use these names.");
        let had_stats = doc.table.contains_key("stat");
        let stats = doc
            .table
            .entry("stat")
            .or_insert_with(|| Value::Array(vec![]))
            .as_array_mut()
            .unwrap();
        // An empty opened editor must not create a file.
        let mut remove = None;
        for (index, stat) in stats.iter_mut().enumerate() {
            ui.push_id(("stat", index), |ui| {
                let row = stat.as_table_mut().unwrap();
                ui.horizontal(|ui| {
                    string(ui, row, "name", "Stat name");
                    integer(ui, row, "id", "Save ID", 32768, 65534);
                    if ui.small_button("Remove stat").clicked() {
                        remove = Some(index);
                    }
                });
            });
        }
        if let Some(index) = remove {
            stats.remove(index);
        }
        if stats.len() < 256 && ui.button("Add named stat").clicked() {
            let id = (40000..65535)
                .chain(32768..40000)
                .find(|id| {
                    !stats
                        .iter()
                        .any(|r| r.get("id").and_then(Value::as_integer) == Some(*id))
                })
                .unwrap();
            let mut row = Table::new();
            let name = (1..)
                .map(|n| format!("stat_{n}"))
                .find(|name| {
                    !stats
                        .iter()
                        .any(|r| r.get("name").and_then(Value::as_str) == Some(name))
                })
                .unwrap();
            row.insert("name".into(), name.into());
            row.insert("id".into(), id.into());
            stats.push(row.into());
        }
        if stats.is_empty() && !had_stats {
            doc.table.remove("stat");
        }
        ui.separator();
        ui.heading("Stat transformations");
        let had_behaviors = doc.table.contains_key("behavior");
        let rows = doc
            .table
            .entry("behavior")
            .or_insert_with(|| Value::Array(vec![]))
            .as_array_mut()
            .unwrap();
        let mut remove = None;
        for (index, row) in rows.iter_mut().enumerate() {
            ui.push_id(("behavior", index), |ui| {
                let row = row.as_table_mut().unwrap();
                ui.group(|ui| {
                    let mut kind = row
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("add")
                        .to_owned();
                    egui::ComboBox::from_label("Transformation")
                        .selected_text(&kind)
                        .show_ui(ui, |ui| {
                            for k in ["add", "convert", "percent_of"] {
                                ui.selectable_value(&mut kind, k.into(), k);
                            }
                        });
                    row.insert("kind".into(), kind.clone().into());
                    string(ui, row, "source", "Source stat");
                    string(ui, row, "target", "Target stat");
                    if kind == "percent_of" {
                        string(ui, row, "of", "Percent of stat");
                    } else {
                        row.remove("of");
                    }
                    integer(ui, row, "percent", "Percent", 0, 10000);
                    if ui.button("Remove transformation").clicked() {
                        remove = Some(index);
                    }
                });
            });
        }
        if let Some(index) = remove {
            rows.remove(index);
        }
        if rows.len() < 512 && ui.button("Add transformation").clicked() {
            rows.push(
                "kind='add'\nsource='strength'\ntarget='armorclass'\npercent=100"
                    .parse::<Table>()
                    .unwrap()
                    .into(),
            );
        }
        if rows.is_empty() && !had_behaviors {
            doc.table.remove("behavior");
        }
        if let Err(error) = doc.validate() {
            ui.colored_label(egui::Color32::LIGHT_RED, error.to_string());
        }
    }
}
pub(crate) fn string(ui: &mut egui::Ui, row: &mut Table, key: &str, label: &str) {
    let mut value = row
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.text_edit_singleline(&mut value).changed() {
            row.insert(key.into(), value.into());
        }
    });
}
pub(crate) fn integer(
    ui: &mut egui::Ui,
    row: &mut Table,
    key: &str,
    label: &str,
    min: i64,
    max: i64,
) {
    let mut value = row.get(key).and_then(Value::as_integer).unwrap_or(min);
    ui.horizontal(|ui| {
        ui.label(label);
        if ui
            .add(egui::DragValue::new(&mut value).range(min..=max))
            .changed()
        {
            row.insert(key.into(), value.into());
        }
    });
}
