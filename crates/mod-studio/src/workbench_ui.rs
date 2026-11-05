//! Item forms and background engine jobs over the table editor's working set.
use crate::{
    tables::{self, Tables},
    workbench::{self, Snapshot},
};
use eframe::egui::{self, Color32};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
const ITEM_TABLES: &[&str] = &[
    "weapons",
    "armor",
    "misc",
    "uniqueitems",
    "setitems",
    "runes",
    "magicprefix",
    "magicsuffix",
    "automagic",
];
type Job = mpsc::Receiver<(String, Result<String, String>)>;
pub struct Workbench {
    pub view: usize,
    table: usize,
    row: usize,
    base: usize,
    query: String,
    field_query: String,
    preview: String,
    last_key: String,
    changed_at: Instant,
    job: Option<Job>,
    pub result: String,
    tc: String,
    monster: bool,
    pub difficulty: u8,
    trials: u32,
    drop_options: workbench::DropOptions,
    seed: u32,
    level: i32,
    inputs: String,
    input_code: String,
}
impl Default for Workbench {
    fn default() -> Self {
        Self {
            view: 0,
            table: 0,
            row: 0,
            base: 0,
            query: String::new(),
            field_query: String::new(),
            preview: String::new(),
            last_key: String::new(),
            changed_at: Instant::now(),
            job: None,
            result: String::new(),
            tc: String::new(),
            monster: false,
            difficulty: 0,
            trials: 10_000,
            drop_options: Default::default(),
            seed: 1,
            level: 85,
            inputs: String::new(),
            input_code: String::new(),
        }
    }
}
impl Workbench {
    /// Seed the screenshot example with text computed by the engine before opening a window.
    #[doc(hidden)]
    pub fn prepare_render(&mut self, data: &Tables, preview: String, result: String) {
        self.table = data
            .edited
            .tables
            .iter()
            .position(|t| t.name() == "weapons")
            .unwrap_or(0);
        self.last_key = format!("preview:{}:0:0:{}", self.table, data.revision);
        self.preview = preview;
        self.result = result;
        self.tc = "Test pool".into();
        self.inputs = "qa1,2,10,1,0,0".into();
        self.field_query.clear();
    }
    fn launch(
        &mut self,
        snapshot: Snapshot,
        key: String,
        task: impl FnOnce(workbench::Engine) -> anyhow::Result<String> + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        std::thread::spawn(move || {
            let result = snapshot
                .compile()
                .and_then(task)
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send((key, result));
        });
    }
    pub fn draw(&mut self, ui: &mut egui::Ui, data: Option<&mut Tables>) {
        if let Some(job) = &self.job {
            match job.try_recv() {
                Ok((key, result)) => {
                    let text = result.unwrap_or_else(|e| format!("Engine could not run: {e}"));
                    if key.starts_with("preview:") {
                        if key == self.last_key {
                            self.preview = text;
                        }
                    } else {
                        self.result = text;
                    }
                    self.job = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.result = "Engine worker stopped.".into();
                    self.job = None;
                }
                Err(_) => {}
            }
        }
        let Some(data) = data else {
            ui.label("Load tables from your converted install first.");
            return;
        };
        if self.view == 0 {
            self.item(ui, data);
        } else {
            self.sim(ui, data);
        }
        if self.job.is_some() {
            ui.spinner();
            ui.label("Running engine on a snapshot of your current edits…");
        }
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }
    fn item(&mut self, ui: &mut egui::Ui, data: &mut Tables) {
        let available: Vec<_> = data
            .edited
            .tables
            .iter()
            .enumerate()
            .filter(|(_, t)| ITEM_TABLES.contains(&t.name()))
            .map(|(t, _)| t)
            .collect();
        if !available.contains(&self.table) {
            self.table = available
                .iter()
                .copied()
                .find(|&t| data.edited.tables[t].name() == "weapons")
                .or_else(|| available.first().copied())
                .unwrap_or(0);
            self.row = 0;
        }
        if available.is_empty() {
            ui.label("No item tables loaded.");
            return;
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("item-table")
                .selected_text(data.edited.tables[self.table].name())
                .show_ui(ui, |ui| {
                    for t in &available {
                        if ui
                            .selectable_value(&mut self.table, *t, data.edited.tables[*t].name())
                            .changed()
                        {
                            self.row = 0;
                        }
                    }
                });
            ui.label("Find item");
            ui.text_edit_singleline(&mut self.query);
        });
        let t = self.table;
        let table = &data.edited.tables[t];
        if table.rows.is_empty() {
            ui.label("This table has no rows. Add one in the table editor.");
            return;
        }
        self.row = self.row.min(table.rows.len() - 1);
        egui::ComboBox::from_id_salt("item-row")
            .width(350.0)
            .selected_text(tables::row_name(table, self.row))
            .show_ui(ui, |ui| {
                for r in 0..table.rows.len() {
                    let name = tables::row_name(table, r);
                    if name.to_lowercase().contains(&self.query.to_lowercase())
                        && !table.disabled(r)
                    {
                        ui.selectable_value(&mut self.row, r, name);
                    }
                }
            });
        let name = table.name().to_owned();
        let r = self.row;
        // Base items resolve by their table code; uniques/sets by their base code.
        let base_choices: Vec<_> = ["weapons", "armor", "misc"]
            .into_iter()
            .filter_map(|name| data.edited.table(name))
            .flat_map(|t| {
                (0..t.rows.len()).map(|r| (tables::text(t.key(r)), tables::row_name(t, r)))
            })
            .collect();
        let fixed_base = if ["weapons", "armor", "misc"].contains(&name.as_str()) {
            Some(tables::text(table.key(r)))
        } else if ["uniqueitems", "setitems"].contains(&name.as_str()) {
            table
                .header
                .iter()
                .position(|h| {
                    h.eq_ignore_ascii_case(if name == "setitems" { b"item" } else { b"code" })
                })
                .map(|c| tables::text(&table.rows[r].cells[c]))
        } else {
            None
        };
        if let Some(code) = fixed_base {
            self.base = base_choices
                .iter()
                .position(|(c, _)| c == &code)
                .unwrap_or(usize::MAX);
        } else {
            self.base = self.base.min(base_choices.len().saturating_sub(1));
            egui::ComboBox::from_id_salt("preview-base")
                .selected_text(
                    base_choices
                        .get(self.base)
                        .map(|(_, n)| n.as_str())
                        .unwrap_or("Choose preview base"),
                )
                .show_ui(ui, |ui| {
                    for (i, (_, name)) in base_choices.iter().enumerate() {
                        ui.selectable_value(&mut self.base, i, name);
                    }
                });
        }
        ui.horizontal(|ui| {
            ui.label("Find field");
            ui.text_edit_singleline(&mut self.field_query);
        });
        let mut changed = false;
        ui.columns(2, |ui| {
            ui[0].strong("Item fields");
            egui::ScrollArea::vertical()
                .id_salt("item-form")
                .max_height(560.0)
                .show(&mut ui[0], |ui| {
                    let columns: Vec<_> = data.edited.tables[t]
                        .header
                        .iter()
                        .enumerate()
                        .filter(|(_, h)| h.as_slice() != d2_data::patch::DISABLED_COLUMN)
                        .map(|(c, h)| (c, tables::text(h)))
                        .collect();
                    for group in ["Basics", "Damage", "Requirements", "Sockets", "Other"] {
                        let visible: Vec<_> = columns
                            .iter()
                            .filter(|(_, key)| {
                                crate::labels::group(key) == group
                                    && format!("{} {key}", crate::labels::label(&name, key))
                                        .to_lowercase()
                                        .contains(&self.field_query.to_lowercase())
                            })
                            .collect();
                        if visible.is_empty() {
                            continue;
                        }
                        ui.separator();
                        ui.strong(group);
                        for (c, key) in visible {
                            let c = *c;
                            let label = crate::labels::label(&name, key);
                            let column = data.column(t, c).cloned();
                            let choices = column
                                .as_ref()
                                .map(|col| data.choices(col))
                                .unwrap_or_default();
                            let patched = data.changed(t, r, c);
                            let disabled = data.edited.tables[t].disabled(r);
                            ui.push_id(c, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add_sized(
                                        [160.0, 20.0],
                                        egui::Label::new(egui::RichText::new(label).color(
                                            if patched {
                                                Color32::GOLD
                                            } else {
                                                Color32::WHITE
                                            },
                                        )),
                                    )
                                    .on_hover_text(key);
                                    ui.add_enabled_ui(!disabled, |ui| {
                                        let value = &mut data.edited.tables[t].rows[r].cells[c];
                                        if column.as_ref().is_some_and(|c| c.kind == "bit") {
                                            let mut b = value == b"1";
                                            if ui.checkbox(&mut b, "").changed() {
                                                *value =
                                                    if b { b"1".to_vec() } else { b"0".to_vec() };
                                                changed = true;
                                            }
                                        } else {
                                            let mut text = tables::text(value);
                                            if !choices.is_empty() {
                                                egui::ComboBox::from_id_salt("ref")
                                                    .selected_text(&text)
                                                    .show_ui(ui, |ui| {
                                                        for (v, n) in &choices {
                                                            if ui
                                                                .selectable_value(
                                                                    &mut text,
                                                                    v.clone(),
                                                                    n,
                                                                )
                                                                .changed()
                                                            {
                                                                changed = true;
                                                            }
                                                        }
                                                    });
                                            }
                                            if ui.text_edit_singleline(&mut text).changed() {
                                                changed = true;
                                            }
                                            if text.as_bytes() != value.as_slice() {
                                                *value = text.into_bytes();
                                            }
                                            if let Err(e) =
                                                tables::validate_cell(column.as_ref(), value)
                                            {
                                                ui.colored_label(Color32::LIGHT_RED, e.to_string());
                                            }
                                        }
                                    });
                                });
                            });
                        }
                    }
                });
            ui[1].strong("Live in-game tooltip · seed 1");
            ui[1].weak("Text uses the engine builder. Neutral class 0 viewer; no player bonuses.");
            egui::ScrollArea::vertical()
                .id_salt("tooltip")
                .max_height(560.0)
                .show(&mut ui[1], |ui| {
                    ui.monospace(&self.preview);
                });
        });
        if changed {
            data.dirty = true;
            data.validate();
        }
        let signature = format!("preview:{t}:{r}:{}:{}", self.base, data.revision);
        if signature != self.last_key {
            self.last_key = signature;
            self.changed_at = Instant::now();
            self.preview = "Updating tooltip…".into();
        }
        if self.job.is_none()
            && self.preview == "Updating tooltip…"
            && self.changed_at.elapsed() >= Duration::from_millis(350)
        {
            let base = self.base;
            self.launch(
                Snapshot::from_tables(data),
                self.last_key.clone(),
                move |engine| {
                    Ok(engine
                        .preview(&name, r, base, 1)?
                        .iter()
                        .map(|l| String::from_utf16_lossy(&l.text))
                        .collect::<Vec<_>>()
                        .join("\n"))
                },
            );
        }
        if !data.problems.is_empty() {
            ui.colored_label(
                Color32::LIGHT_RED,
                format!(
                    "{} table problems — inspect the table editor before saving",
                    data.problems.len()
                ),
            );
        }
    }
    fn sim(&mut self, ui: &mut egui::Ui, data: &Tables) {
        ui.horizontal(|ui| {
            ui.label("Difficulty");
            egui::ComboBox::from_id_salt("difficulty")
                .selected_text(["Normal", "Nightmare", "Hell"][self.difficulty as usize])
                .show_ui(ui, |ui| {
                    for (i, name) in ["Normal", "Nightmare", "Hell"].iter().enumerate() {
                        ui.selectable_value(&mut self.difficulty, i as u8, *name);
                    }
                });
            ui.label("Seed");
            ui.add(egui::DragValue::new(&mut self.seed));
            ui.label("Level");
            ui.add(egui::DragValue::new(&mut self.level).range(1..=99));
        });
        if self.view == 1 {
            ui.checkbox(
                &mut self.monster,
                "Pick a monster instead of a treasure class",
            );
            let name = if self.monster {
                "monstats"
            } else {
                "treasureclassex"
            };
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.text_edit_singleline(&mut self.query);
            });
            egui::ComboBox::from_id_salt("drop-source")
                .selected_text(if self.tc.is_empty() {
                    "Choose source"
                } else {
                    &self.tc
                })
                .show_ui(ui, |ui| {
                    if let Some(table) = data.edited.table(name) {
                        for r in 0..table.rows.len() {
                            let label = tables::row_name(table, r);
                            if !table.disabled(r)
                                && label.to_lowercase().contains(&self.query.to_lowercase())
                            {
                                ui.selectable_value(
                                    &mut self.tc,
                                    tables::text(table.key(r)),
                                    label,
                                );
                            }
                        }
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Trials");
                ui.add(egui::DragValue::new(&mut self.trials).range(1..=1_000_000));
            });
            ui.horizontal(|ui| {
                ui.label("Players");
                ui.add(egui::DragValue::new(&mut self.drop_options.players).range(1..=8));
                ui.label("Magic find %");
                ui.add(egui::DragValue::new(&mut self.drop_options.magic_find).range(0..=100000));
            });
            ui.weak("Expansion · /players setting · six-drop limit · mod drop rules applied");
            if ui
                .add_enabled(
                    self.job.is_none() && !self.tc.is_empty(),
                    egui::Button::new("Run drops"),
                )
                .clicked()
            {
                let (source, monster, difficulty, n, seed, level) = (
                    self.tc.clone(),
                    self.monster,
                    self.difficulty,
                    self.trials,
                    self.seed,
                    self.level,
                );
                let options = self.drop_options.clone();
                self.launch(Snapshot::from_tables(data), "drops".into(), move |engine| {
                    let tc = if monster {
                        let table = engine.set.table("monstats").context("monstats")?;
                        let rows = d2_data::tables::decode_all::<d2_data::tables::Monstats>(table)?;
                        let raw = engine_source_row(&engine, &source)?;
                        d2_sim::treasure::monster_tc(
                            &rows[raw],
                            d2_sim::treasure::MonsterRank::Normal,
                            difficulty,
                            true,
                            |_| false,
                        )?
                    } else {
                        engine
                            .tcs
                            .tcs
                            .iter()
                            .position(|t| t.name.eq_ignore_ascii_case(source.as_bytes()))
                            .context("Treasure class not found")? as u16
                    };
                    let counts =
                        engine.drops_with_options(tc, difficulty, n, seed, level, &options)?;
                    Ok(format!(
                        "Seed {seed} · players {} · magic find {}%\n{}",
                        options.players,
                        options.magic_find,
                        engine.format_drops(&counts, n)
                    ))
                });
            }
        } else {
            ui.horizontal(|ui| {
                ui.label("Find input");
                ui.text_edit_singleline(&mut self.query);
                egui::ComboBox::from_id_salt("cube-catalog")
                    .selected_text(if self.input_code.is_empty() {
                        "Choose item"
                    } else {
                        &self.input_code
                    })
                    .show_ui(ui, |ui| {
                        for name in ["weapons", "armor", "misc"] {
                            if let Some(table) = data.edited.table(name) {
                                for row in 0..table.rows.len() {
                                    let label = tables::row_name(table, row);
                                    if !table.disabled(row)
                                        && label.to_lowercase().contains(&self.query.to_lowercase())
                                    {
                                        ui.selectable_value(
                                            &mut self.input_code,
                                            tables::text(table.key(row)),
                                            label,
                                        );
                                    }
                                }
                            }
                        }
                    });
                if ui
                    .add_enabled(!self.input_code.is_empty(), egui::Button::new("Add input"))
                    .clicked()
                {
                    if !self.inputs.is_empty() {
                        self.inputs.push('\n');
                    }
                    self.inputs.push_str(&self.input_code);
                }
            });
            ui.label("Cube inputs · one item per line: code, quality, level, quantity, sockets, ethereal (0/1)");
            ui.weak("Quality: 1 low · 2 normal · 3 superior · 4 magic · 5 set · 6 rare · 7 unique · 8 crafted · 9 tempered");
            ui.add(egui::TextEdit::multiline(&mut self.inputs).desired_rows(8));
            ui.weak("Player class 0 · day 1 · weekday 1 · ladder · expansion. Inventory placement succeeds; quest portals are reported as events.");
            if ui
                .add_enabled(self.job.is_none(), egui::Button::new("Match and transmute"))
                .clicked()
            {
                let text = self.inputs.clone();
                let options = workbench::cube::CubeOptions {
                    seed: self.seed,
                    difficulty: self.difficulty,
                    level: self.level,
                    ..Default::default()
                };
                self.launch(Snapshot::from_tables(data), "cube".into(), move |engine| {
                    let inputs = parse_inputs(&text)?;
                    engine.cube(&inputs, &options)
                });
            }
        }
        egui::ScrollArea::vertical()
            .id_salt("sim-result")
            .show(ui, |ui| {
                ui.monospace(&self.result);
            });
    }
}
use anyhow::Context;
fn engine_source_row(engine: &workbench::Engine, source: &str) -> anyhow::Result<usize> {
    engine
        .row_keys
        .get("monstats")
        .context("monstats")?
        .iter()
        .position(|key| key.eq_ignore_ascii_case(source))
        .context("Monster not found")
}
pub fn parse_inputs(text: &str) -> anyhow::Result<Vec<workbench::cube::CubeInput>> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let c: Vec<_> = line.split(',').map(str::trim).collect();
            let n = |i: usize, default: i32| -> anyhow::Result<i32> {
                c.get(i).map_or(Ok(default), |s| Ok(s.parse()?))
            };
            Ok(workbench::cube::CubeInput {
                code: c[0].into(),
                quality: u8::try_from(n(1, 2)?)?,
                level: n(2, 1)?,
                quantity: n(3, 1)?,
                sockets: n(4, 0)?,
                ethereal: n(5, 0)? != 0,
            })
        })
        .collect()
}
