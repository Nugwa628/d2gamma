//! Act boundaries, towns, waypoints and travel routes in one editor.
use crate::{
    definitions::Document,
    stats_ui::{integer, string},
};
use eframe::egui;
use std::path::Path;
use toml::{Table, Value};
#[derive(Default)]
pub struct ActsEditor {
    pub document: Option<Document>,
}
impl ActsEditor {
    pub fn open(&mut self, root: &Path) {
        self.document = Some(Document::open(
            root,
            "files/acts.toml",
            "version=1\nacts=[]",
            |s| {
                d2_sim::rules::acts::Acts::from_toml(s)
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
            ui.label("Act definitions require a gameplay mod.");
            return;
        }
        let Some(doc) = &mut self.document else {
            return;
        };
        if let Some(error) = &doc.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
            return;
        }
        ui.label("Ordered acts · towns · waypoints · travel");
        ui.weak("Ranges exclude the End level. Create matching levels and map files in the Map and Table editors. Waypoint IDs are permanent save bits.");
        let acts = doc.table.get_mut("acts").unwrap().as_array_mut().unwrap();
        let used: std::collections::BTreeSet<i64> = acts
            .iter()
            .filter_map(|a| a.get("waypoints").and_then(Value::as_array))
            .flatten()
            .filter_map(|w| w.get("id").and_then(Value::as_integer))
            .collect();
        let next_waypoint = (0..112).find(|id| !used.contains(id)).unwrap_or(111);
        let mut remove = None;
        for (id, act) in acts.iter_mut().enumerate() {
            ui.push_id(("act", id), |ui| {
                ui.group(|ui| {
                    ui.strong(format!("Act {id}"));
                    let row = act.as_table_mut().unwrap();
                    string(ui, row, "name", "Act name");
                    for (key, label, min, max) in [
                        ("start", "Start level", 1, 255),
                        ("end", "End level (exclusive)", 2, 256),
                        ("town", "Town level", 1, 255),
                    ] {
                        integer(ui, row, key, label, min, max);
                    }
                    let town = row.get("town").and_then(Value::as_integer).unwrap_or(1);
                    children(
                        ui,
                        row,
                        "waypoints",
                        "Waypoint",
                        9,
                        &[
                            ("id", "Waypoint ID", 0, 111),
                            ("level", "Waypoint level", 1, 255),
                        ],
                        &format!("id={next_waypoint}\nlevel={town}"),
                    );
                    children(
                        ui,
                        row,
                        "travel",
                        "Travel route",
                        256,
                        &[
                            ("npc", "NPC class", 0, 65535),
                            ("action", "NPC action", 0, 4294967295),
                            ("to", "Destination level", 1, 255),
                            ("tile", "Arrival tile", 0, 4294967295),
                            ("text", "String ID", 0, 65535),
                        ],
                        "npc=0\naction=0\nto=1\ntile=0\ntext=0",
                    );
                    if ui.button("Remove act").clicked() {
                        remove = Some(id);
                    }
                })
            });
        }
        if let Some(id) = remove {
            acts.remove(id);
        }
        if acts.len() < 128 && ui.button("Add act").clicked() {
            let start = acts
                .last()
                .and_then(|a| a.get("end"))
                .and_then(Value::as_integer)
                .unwrap_or(1)
                .min(255);
            let mut row: Table = "name='New act'\nstart=1\nend=40\ntown=1".parse().unwrap();
            row.insert("name".into(), format!("Act {}", acts.len() + 1).into());
            row.insert("start".into(), start.into());
            row.insert("end".into(), (start + 1).into());
            row.insert("town".into(), start.into());
            acts.push(row.into());
        }
        if let Err(error) = doc.validate() {
            ui.colored_label(egui::Color32::LIGHT_RED, error.to_string());
        }
    }
}
fn children(
    ui: &mut egui::Ui,
    row: &mut Table,
    key: &str,
    label: &str,
    limit: usize,
    fields: &[(&str, &str, i64, i64)],
    initial: &str,
) {
    let had = row.contains_key(key);
    let rows = row
        .entry(key)
        .or_insert_with(|| Value::Array(vec![]))
        .as_array_mut()
        .unwrap();
    let mut remove = None;
    for (id, child) in rows.iter_mut().enumerate() {
        ui.push_id((key, id), |ui| {
            ui.group(|ui| {
                ui.strong(format!("{label} {id}"));
                for &(key, label, min, max) in fields {
                    integer(ui, child.as_table_mut().unwrap(), key, label, min, max);
                }
                if ui.button(format!("Remove {label}")).clicked() {
                    remove = Some(id);
                }
            })
        });
    }
    if let Some(id) = remove {
        rows.remove(id);
    }
    if rows.len() < limit && ui.button(format!("Add {label}")).clicked() {
        rows.push(initial.parse::<Table>().unwrap().into());
    }
    if rows.is_empty() && !had {
        row.remove(key);
    }
}
