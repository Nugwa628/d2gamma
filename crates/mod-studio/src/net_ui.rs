//! Editor for the shared, versioned mod network contract.
use anyhow::{bail, Context, Result};
use d2_modkit::net::Definition;
use eframe::egui::{self, Color32};
use std::{
    fs,
    path::{Path, PathBuf},
};
use toml::{Table, Value};
#[derive(Default)]
pub struct NetEditor {
    root: Option<PathBuf>,
    pub table: Table,
    saved: Option<String>,
    enabled: bool,
    pub error: String,
    load_error: bool,
    selected: usize,
    actions: bool,
    raw: String,
}
impl NetEditor {
    pub fn open(&mut self, root: &Path) {
        *self = Self::default();
        self.root = Some(root.into());
        self.table = "version=1".parse().unwrap();
        let path = root.join("files/net.toml");
        if !path.exists() {
            return;
        }
        let result = (|| -> Result<String> {
            if fs::metadata(&path)?.len() > d2_modkit::net::MAX_DOCUMENT as u64 {
                bail!("files/net.toml exceeds 256 KiB");
            }
            let text = fs::read_to_string(&path)?;
            Definition::parse(&text).map_err(anyhow::Error::msg)?;
            Ok(text)
        })();
        match result {
            Ok(text) => {
                self.table = text.parse().unwrap();
                self.saved = Some(text);
                self.enabled = true;
            }
            Err(e) => {
                self.error = format!("{e:#}");
                self.load_error = true;
            }
        }
    }
    pub fn configured(&self) -> bool {
        self.enabled
    }
    pub fn definition(&self) -> Result<Definition> {
        if self.load_error {
            bail!("{}", self.error);
        }
        if !self.enabled {
            return Ok(Definition::default());
        }
        Definition::parse(&toml::to_string(&self.table)?).map_err(anyhow::Error::msg)
    }
    pub fn dirty(&self) -> bool {
        self.enabled
            && self
                .saved
                .as_ref()
                .is_none_or(|saved| saved.parse::<Table>().ok().as_ref() != Some(&self.table))
    }
    pub fn validate(&self) -> Result<()> {
        self.definition()?;
        if !self.enabled {
            return Ok(());
        }
        let root = self.root.as_ref().context("Open a project")?;
        for path in [root.join("files"), root.join("files/net.toml")] {
            if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                bail!("Network file path must be inside this project");
            }
        }
        let disk = match fs::read_to_string(root.join("files/net.toml")) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        if disk != self.saved {
            bail!("files/net.toml changed outside Studio; reopen the project before saving");
        }
        Ok(())
    }
    pub fn save(&mut self) -> Result<()> {
        self.validate()?;
        if !self.dirty() {
            return Ok(());
        }
        let root = self.root.as_ref().context("Open a project")?;
        let text = toml::to_string_pretty(&self.table)?;
        fs::create_dir_all(root.join("files"))?;
        fs::write(root.join("files/net.toml"), &text)?;
        self.saved = Some(text);
        Ok(())
    }
    pub fn add(&mut self, actions: bool) -> Result<()> {
        self.root.as_ref().context("Open a project")?;
        if self.load_error {
            bail!("{}", self.error);
        }
        let key = if actions { "actions" } else { "values" };
        let entries = self
            .table
            .entry(key)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .context("Expected declaration list")?;
        if entries.len() >= 256 {
            bail!("At most 256 {key}");
        }
        let id = (1..65535)
            .find(|id| {
                !entries
                    .iter()
                    .any(|e| e.get("id").and_then(Value::as_integer) == Some(*id))
            })
            .context("No free ID")?;
        let name = if actions { "action" } else { "value" };
        let body = if actions {
            "stat='strength'\ndelta=1\nmaximum=200\ncooldown=25"
        } else {
            "source='stat.strength'"
        };
        let mut suffix = id;
        let name = loop {
            let name = format!("{name}_{suffix}");
            if !entries
                .iter()
                .any(|e| e.get("name").and_then(Value::as_str) == Some(&name))
            {
                break name;
            }
            suffix += 1;
        };
        let declaration: Table = format!("id={id}\nname='{name}'\n{body}").parse()?;
        entries.push(Value::Table(declaration));
        self.enabled = true;
        self.selected = entries.len() - 1;
        self.actions = actions;
        Ok(())
    }
    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        tables: Option<&crate::tables::Tables>,
        scripts: &crate::scripts::Scripts,
    ) {
        ui.label("Values flow from the server to your panels. Buttons request actions checked by the server.");
        if self.load_error {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
            return;
        }
        ui.horizontal(|ui| {
            if ui
                .selectable_value(&mut self.actions, false, "Synced values")
                .clicked()
            {
                self.selected = 0;
            }
            if ui
                .selectable_value(&mut self.actions, true, "Server actions")
                .clicked()
            {
                self.selected = 0;
            }
            if ui.button("Add declaration").clicked() {
                if let Err(e) = self.add(self.actions) {
                    self.error = e.to_string();
                }
            }
        });
        let key = if self.actions { "actions" } else { "values" };
        let mut remove = false;
        if let Some(entries) = self.table.get_mut(key).and_then(Value::as_array_mut) {
            egui::ComboBox::from_id_salt("network-declaration")
                .selected_text(
                    entries
                        .get(self.selected)
                        .and_then(|v| v.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("Choose declaration"),
                )
                .show_ui(ui, |ui| {
                    for (i, v) in entries.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.selected,
                            i,
                            v.get("name").and_then(Value::as_str).unwrap_or("Unnamed"),
                        );
                    }
                });
            if let Some(t) = entries.get_mut(self.selected).and_then(Value::as_table_mut) {
                ui.horizontal(|ui| {
                    number(ui, t, "id", 1, 65534);
                    text(ui, t, "name");
                    remove = ui.button("Delete declaration").clicked();
                });
                if self.actions {
                    ui.label("Data validation: apply a bounded stat change after all checks pass.");
                    text(ui, t, "stat");
                    choices(ui, t, "stat", &table_names(tables, "itemstatcost", "stat"));
                    for (key, min, max) in [
                        ("delta", i32::MIN as i64, i32::MAX as i64),
                        ("minimum", i32::MIN as i64, i32::MAX as i64),
                        ("maximum", i32::MIN as i64, i32::MAX as i64),
                        ("cooldown", 1, 3600),
                    ] {
                        number(ui, t, key, min, max);
                    }
                    let mut town = t.get("town").and_then(Value::as_bool).unwrap_or(false);
                    if ui.checkbox(&mut town, "Only in town").changed() {
                        t.insert("town".into(), Value::Boolean(town));
                    }
                    let mut cost = t.contains_key("cost_stat");
                    if ui.checkbox(&mut cost, "Spend another stat").changed() {
                        if cost {
                            t.insert("cost_stat".into(), Value::String("statpts".into()));
                            t.insert("cost".into(), Value::Integer(1));
                        } else {
                            t.remove("cost_stat");
                            t.remove("cost");
                        }
                    }
                    if cost {
                        text(ui, t, "cost_stat");
                        choices(
                            ui,
                            t,
                            "cost_stat",
                            &table_names(tables, "itemstatcost", "stat"),
                        );
                        number(ui, t, "cost", 1, i32::MAX as i64);
                    }
                    let mut validator = t.contains_key("validator");
                    if ui
                        .checkbox(&mut validator, "Also validate with a Luau predicate")
                        .changed()
                    {
                        if validator {
                            let module = scripts
                                .files
                                .first()
                                .map_or("rules.luau", |s| s.name.as_str());
                            let mut rule = Table::new();
                            rule.insert("module".into(), Value::String(module.into()));
                            rule.insert("function".into(), Value::String("approve".into()));
                            t.insert("validator".into(), Value::Table(rule));
                        } else {
                            t.remove("validator");
                        }
                    }
                    if let Some(v) = t.get_mut("validator").and_then(Value::as_table_mut) {
                        text(ui, v, "module");
                        choices(
                            ui,
                            v,
                            "module",
                            &scripts
                                .files
                                .iter()
                                .map(|f| f.name.clone())
                                .collect::<Vec<_>>(),
                        );
                        text(ui, v, "function");
                        ui.weak("Predicate receives the current stat value; return a positive integer to approve. Check validates the export.");
                    }
                } else {
                    text(ui, t, "source");
                    let mut sources = Vec::new();
                    for (table, col, kind) in [
                        ("itemstatcost", "stat", "stat"),
                        ("states", "state", "state"),
                    ] {
                        sources.extend(
                            table_names(tables, table, col)
                                .into_iter()
                                .map(|n| format!("{kind}.{n}")),
                        );
                    }
                    choices(ui, t, "source", &sources);
                    ui.weak("Use stat.name, state.name, or var.module/name for an M2 player variable. Bind a panel to sync.<value name>.");
                }
            }
            if remove && self.selected < entries.len() {
                entries.remove(self.selected);
                self.selected = self.selected.saturating_sub(1);
            }
        }
        match self.definition() {
            Ok(d) => {
                ui.weak(format!(
                    "{} values, {} actions{}",
                    d.values.len(),
                    d.actions.len(),
                    if self.dirty() { " • unsaved" } else { "" }
                ));
            }
            Err(e) => {
                ui.colored_label(Color32::LIGHT_RED, e.to_string());
            }
        }
        egui::CollapsingHeader::new("Advanced TOML").show(ui, |ui| {
            if ui.button("Read current declarations").clicked() {
                self.raw = toml::to_string_pretty(&self.table).unwrap_or_default();
            }
            ui.add(
                egui::TextEdit::multiline(&mut self.raw)
                    .code_editor()
                    .desired_rows(12),
            );
            if ui.button("Apply TOML").clicked() {
                match Definition::parse(&self.raw) {
                    Ok(_) => {
                        self.table = self.raw.parse().unwrap();
                        self.enabled = true;
                        self.error.clear();
                    }
                    Err(e) => self.error = e,
                }
            }
        });
        if !self.error.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
        }
    }
}
fn text(ui: &mut egui::Ui, t: &mut Table, key: &str) {
    let mut value = t.get(key).and_then(Value::as_str).unwrap_or("").to_owned();
    ui.horizontal(|ui| {
        ui.label(key);
        if ui.text_edit_singleline(&mut value).changed() {
            t.insert(key.into(), Value::String(value));
        }
    });
}
fn number(ui: &mut egui::Ui, t: &mut Table, key: &str, min: i64, max: i64) {
    let mut value = t
        .get(key)
        .and_then(Value::as_integer)
        .unwrap_or(if key == "maximum" {
            i32::MAX as i64
        } else if key == "cooldown" {
            25
        } else {
            min.max(0)
        });
    ui.horizontal(|ui| {
        ui.label(key);
        if ui
            .add(egui::DragValue::new(&mut value).range(min..=max))
            .changed()
        {
            t.insert(key.into(), Value::Integer(value));
        }
    });
}
fn choices(ui: &mut egui::Ui, t: &mut Table, key: &str, choices: &[String]) {
    if choices.is_empty() {
        return;
    }
    egui::ComboBox::from_id_salt(("net-choice", key))
        .selected_text(format!("Choose {key}"))
        .show_ui(ui, |ui| {
            for choice in choices {
                if ui.selectable_label(false, choice).clicked() {
                    t.insert(key.into(), Value::String(choice.clone()));
                }
            }
        });
}
fn table_names(tables: Option<&crate::tables::Tables>, name: &str, col: &str) -> Vec<String> {
    let Some(t) = tables.and_then(|t| {
        t.edited
            .tables
            .iter()
            .find(|t| t.name().eq_ignore_ascii_case(name))
    }) else {
        return Vec::new();
    };
    let Some(c) = t
        .header
        .iter()
        .position(|h| h.eq_ignore_ascii_case(col.as_bytes()))
    else {
        return Vec::new();
    };
    t.rows
        .iter()
        .enumerate()
        .filter(|(r, _)| !t.disabled(*r))
        .filter_map(|(_, r)| {
            let s = crate::tables::text(&r.cells[c]);
            (!s.is_empty()).then_some(s)
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("studio-net-{tag}-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    fn network_editor_absence_writes_nothing_and_forms_round_trip() {
        let root = root("forms");
        let mut n = NetEditor::default();
        n.open(&root);
        assert!(!n.dirty());
        n.save().unwrap();
        assert!(!root.join("files/net.toml").exists());
        n.add(false).unwrap();
        n.add(true).unwrap();
        assert!(n.dirty());
        assert_eq!(n.definition().unwrap().values.len(), 1);
        n.save().unwrap();
        assert!(!n.dirty());
        let expected = n.definition().unwrap();
        let mut reopened = NetEditor::default();
        reopened.open(&root);
        assert_eq!(reopened.definition().unwrap(), expected);
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                reopened.draw(ui, None, &crate::scripts::Scripts::default());
            });
        });
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn network_editor_rejects_invalid_forms_external_conflicts_and_bad_loads() {
        let root = root("guard");
        let mut n = NetEditor::default();
        n.open(&root);
        n.add(false).unwrap();
        n.save().unwrap();
        let saved = fs::read_to_string(root.join("files/net.toml")).unwrap();
        n.table.get_mut("values").unwrap().as_array_mut().unwrap()[0]
            .as_table_mut()
            .unwrap()
            .insert("id".into(), Value::Integer(0));
        assert!(n.save().is_err());
        assert_eq!(
            fs::read_to_string(root.join("files/net.toml")).unwrap(),
            saved
        );
        n.open(&root);
        n.add(true).unwrap();
        fs::write(root.join("files/net.toml"), "version=1\n# external change").unwrap();
        assert!(n.save().unwrap_err().to_string().contains("outside Studio"));
        fs::write(root.join("files/net.toml"), "version=99").unwrap();
        n.open(&root);
        assert!(n.add(false).is_err());
        assert!(n.save().is_err());
        assert_eq!(
            fs::read_to_string(root.join("files/net.toml")).unwrap(),
            "version=99"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn network_editor_allocates_separate_stable_ids_and_caps_declarations() {
        let root = root("ids");
        let mut n = NetEditor::default();
        n.open(&root);
        for _ in 0..256 {
            n.add(false).unwrap();
        }
        assert!(n.add(false).is_err());
        n.add(true).unwrap();
        let d = n.definition().unwrap();
        assert_eq!(d.actions[0].id, 1);
        assert_eq!(d.values[255].id, 256);
        fs::remove_dir_all(root).unwrap();
    }
}
