//! Spreadsheet controls over the patch working set, with immediate diagnostics.
use crate::tables::{self, Tables};
use eframe::egui::{self, Color32, RichText};

type ColumnChoices = (Option<tables::Column>, Vec<(String, String)>);

#[derive(Default)]
pub struct TableEditor {
    pub data: Option<Tables>,
    pub selected: usize,
    row: usize,
    table_filter: String,
    column_filter: String,
    query: String,
    reference_query: String,
    new_key: String,
    string_key: String,
    string_text: String,
    string_replace: bool,
    message: String,
    scroll_to_row: Option<usize>,
    pub render_dropdown: bool,
    target_column: Option<usize>,
    cached_table: Option<usize>,
    metadata: Vec<ColumnChoices>,
}
impl TableEditor {
    pub fn jump_target(&mut self, target: &crate::actions::Jump) -> anyhow::Result<()> {
        use crate::actions::Jump;
        let data = self
            .data
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Load tables to jump to this problem"))?;
        let (table, row, column) = match target {
            Jump::Table { table, row, column } => (table.clone(), *row, column.clone()),
            Jump::Item { index } => {
                let mut remaining = *index;
                let mut found = None;
                for name in ["weapons", "armor", "misc"] {
                    if let Some(table) = data.edited.table(name) {
                        if remaining < table.rows.len() {
                            found = Some((name.to_owned(), remaining, None));
                            break;
                        }
                        remaining -= table.rows.len();
                    }
                }
                found.ok_or_else(|| anyhow::anyhow!("Item {index} is not available"))?
            }
            Jump::Key { table, key } => {
                let source = data
                    .edited
                    .table(table)
                    .ok_or_else(|| anyhow::anyhow!("Table {table} is not available"))?;
                let row = (0..source.rows.len())
                    .find(|r| source.key(*r).eq_ignore_ascii_case(key.as_bytes()))
                    .ok_or_else(|| anyhow::anyhow!("Row {key} is not available"))?;
                (table.clone(), row, None)
            }
            Jump::File { .. } | Jump::Graph { .. } => anyhow::bail!("Target is a file"),
        };
        self.jump(&table, row, column.as_deref())
    }
    pub fn jump(&mut self, table: &str, row: usize, column: Option<&str>) -> anyhow::Result<()> {
        let data = self
            .data
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Load tables to jump to this problem"))?;
        let t = data
            .edited
            .tables
            .iter()
            .position(|t| t.name().eq_ignore_ascii_case(table))
            .ok_or_else(|| anyhow::anyhow!("Table {table} is not available"))?;
        if row >= data.edited.tables[t].rows.len() {
            anyhow::bail!("Row {row} is not available in {table}");
        }
        self.target_column = column.and_then(|c| {
            data.edited.tables[t]
                .header
                .iter()
                .position(|h| h.eq_ignore_ascii_case(c.as_bytes()))
        });
        self.selected = t;
        self.row = row;
        self.query.clear();
        self.scroll_to_row = Some(row);
        self.cached_table = None;
        Ok(())
    }
    pub fn dirty(&self) -> bool {
        self.data.as_ref().is_some_and(|d| d.dirty)
    }
    pub fn save(&mut self) -> anyhow::Result<()> {
        if let Some(data) = &mut self.data {
            if data.dirty {
                data.save()?;
            }
        }
        Ok(())
    }
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let Some(data) = &mut self.data else {
            ui.label("Configure your converted native install below, then Load tables.");
            return;
        };
        ui.horizontal(|ui| {
            ui.label("Table");
            egui::ComboBox::from_id_salt("table-picker")
                .selected_text(data.edited.tables[self.selected].name())
                .show_ui(ui, |ui| {
                    for (t, table) in data
                        .edited
                        .tables
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| t.name().contains(&self.table_filter))
                    {
                        if ui
                            .selectable_value(&mut self.selected, t, table.name())
                            .changed()
                        {
                            self.row = 0;
                        }
                    }
                });
            ui.label("Find table");
            ui.add(egui::TextEdit::singleline(&mut self.table_filter).desired_width(110.));
        });
        ui.horizontal(|ui| {
            ui.label("Search all tables");
            ui.text_edit_singleline(&mut self.query);
            if ui.small_button("Clear").clicked() {
                self.query.clear();
            }
        });
        if !self.query.is_empty() {
            let results = data.search(&self.query);
            ui.label(format!("{} matching rows", results.len()));
            egui::ScrollArea::vertical()
                .id_salt("search-results")
                .max_height(100.0)
                .show(ui, |ui| {
                    for (t, r, label) in results.iter().take(200) {
                        if ui
                            .selectable_label(self.selected == *t && self.row == *r, label)
                            .clicked()
                        {
                            self.selected = *t;
                            self.row = *r;
                            self.scroll_to_row = Some(*r);
                        }
                    }
                });
        }
        egui::CollapsingHeader::new("Strings and UI labels").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("String key");
                ui.text_edit_singleline(&mut self.string_key);
            });
            ui.horizontal(|ui| {
                ui.label("String text");
                ui.text_edit_singleline(&mut self.string_text);
            });
            ui.checkbox(&mut self.string_replace, "Replace an existing string");
            if ui.button("Add / update string").clicked() {
                self.message = match data.edit_string(
                    &self.string_key,
                    &self.string_text,
                    self.string_replace,
                ) {
                    Ok(()) => format!("String {} ready to save", self.string_key),
                    Err(e) => e.to_string(),
                };
            }
        });
        let t = self.selected;

        ui.horizontal(|ui| {
            ui.label("New row key");
            ui.add(egui::TextEdit::singleline(&mut self.new_key).desired_width(130.0));
            let fixed = data.edited.tables[t].rules.fixed;
            for (label,duplicate) in [("Add row",None),("Duplicate row",Some(self.row))] {
                if ui.add_enabled(!fixed && (duplicate.is_none() || self.row<data.edited.tables[t].rows.len()),egui::Button::new(label)).clicked() {
                    match data.add(t,duplicate,&self.new_key) { Ok(r)=> { self.row=r; self.scroll_to_row=Some(r); self.new_key.clear(); self.message.clear(); self.cached_table=None; }, Err(e)=>self.message=e.to_string() }
                }
            }
            let can_disable = self.row < data.edited.tables[t].rows.len() && !data.edited.tables[t].disabled(self.row);
            if ui.add_enabled(can_disable,egui::Button::new("Disable row")).on_hover_text("Keeps its id reserved. Unsaved edits on this row are discarded. References appear below and block saving.").clicked() {
                self.cached_table=None;
                self.message = data.disable(t,self.row).err().map(|e|e.to_string()).unwrap_or_default();
            }
            if ui.add_enabled(self.row<data.baseline.tables[t].rows.len(),egui::Button::new("Restore row")).on_hover_text("Undo this row's Studio edits, including disabling it").clicked() {
                data.restore(t,self.row);self.cached_table=None;
            }
        });
        ui.weak(
            "Gold cells are patched · select a row by its number · disabled rows retain their id",
        );
        if !self.message.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.message);
        }
        let rows = data.edited.tables[t].rows.len();
        let columns: Vec<_> = data.edited.tables[t]
            .header
            .iter()
            .enumerate()
            .filter(|(_, h)| h.as_slice() != d2_data::patch::DISABLED_COLUMN)
            .map(|(c, h)| (c, tables::text(h)))
            .collect();
        if self.cached_table != Some(t) {
            self.metadata = columns
                .iter()
                .map(|(c, _)| {
                    let column = data.column(t, *c).cloned();
                    let choices = column.as_ref().map(|c| data.choices(c)).unwrap_or_default();
                    (column, choices)
                })
                .collect();
            self.cached_table = Some(t);
        }
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Selected row");
            if ui
                .add(egui::DragValue::new(&mut self.row).range(0..=rows.saturating_sub(1)))
                .changed()
            {
                self.scroll_to_row = Some(self.row);
            }
            ui.label("Edit column");
            ui.add(egui::TextEdit::singleline(&mut self.column_filter).desired_width(160.));
        });
        if !self.column_filter.is_empty() && self.row < rows {
            let exact = columns
                .iter()
                .any(|(_, name)| name.eq_ignore_ascii_case(&self.column_filter));
            egui::ScrollArea::vertical()
                .id_salt("cell-inspector")
                .max_height(130.)
                .show(ui, |ui| {
                    for (c, name) in columns
                        .iter()
                        .filter(|(_, name)| {
                            if exact {
                                name.eq_ignore_ascii_case(&self.column_filter)
                            } else {
                                name.to_lowercase()
                                    .contains(&self.column_filter.to_lowercase())
                            }
                        })
                        .take(8)
                    {
                        ui.push_id((t, self.row, c, "inspector"), |ui| {
                            ui.horizontal(|ui| {
                                ui.label(name);
                                ui.label("Cell value");
                                let mut value =
                                    tables::text(&data.edited.tables[t].rows[self.row].cells[*c]);
                                let response = ui.add_enabled(
                                    !data.edited.tables[t].disabled(self.row),
                                    egui::TextEdit::singleline(&mut value).desired_width(190.),
                                );
                                if response.changed() {
                                    data.edited.tables[t].rows[self.row].cells[*c] =
                                        value.into_bytes();
                                    changed = true;
                                }
                                if let Err(e) = tables::validate_cell(
                                    data.column(t, *c),
                                    &data.edited.tables[t].rows[self.row].cells[*c],
                                ) {
                                    ui.colored_label(Color32::LIGHT_RED, e.to_string());
                                }
                            });
                        });
                    }
                });
        }
        let metadata = &self.metadata;
        let mut sheet = egui::ScrollArea::both()
            .id_salt(("sheet", t))
            .max_height((ui.available_height() - 165.0).max(150.0));
        if self.scroll_to_row.is_some() {
            if let Some(c) = self.target_column {
                sheet = sheet.horizontal_scroll_offset((c as f32 * 158.0 - 100.0).max(0.0));
            }
        }
        sheet.show(ui,|ui| {
            ui.horizontal(|ui| {
                ui.add_sized([50.0,24.0],egui::Label::new("Row"));
                for (_,name) in &columns { ui.add_sized([150.0,24.0],egui::Label::new(RichText::new(crate::labels::label(data.edited.tables[t].name(),name)).strong())).on_hover_text(name); }
            });
            let mut scroll=egui::ScrollArea::vertical().id_salt(("sheet-rows",t));
            if let Some(r)=self.scroll_to_row.take() { scroll=scroll.vertical_scroll_offset(r as f32*30.0); }
            scroll.show_rows(ui,30.0,rows,|ui,range| {
                for r in range {
                    let disabled=data.edited.tables[t].disabled(r);
                    ui.push_id((t,r),|ui|ui.horizontal(|ui| {
                        let label=if disabled { RichText::new(r.to_string()).strikethrough() } else { RichText::new(r.to_string()) };
                        if ui.add_sized([50.0,28.0],egui::Button::new(label).selected(self.row==r)).clicked() { self.row=r; }
                        for ((c,_),(column,choices)) in columns.iter().zip(metadata) {
                            let patched=data.changed(t,r,*c);
                            let value=&mut data.edited.tables[t].rows[r].cells[*c];
                            ui.push_id(*c,|ui| {
                                egui::Frame::new().stroke(if self.row==r && self.target_column==Some(*c) { egui::Stroke::new(2.0_f32,Color32::LIGHT_BLUE) } else { egui::Stroke::NONE }).fill(if self.row==r && self.target_column==Some(*c) { Color32::from_rgb(35,75,100) } else if patched { Color32::from_rgb(72,57,25) } else { Color32::TRANSPARENT }).show(ui,|ui| {
                                    ui.set_width(142.0);
                                    if disabled {
                                        ui.label(RichText::new(tables::text(value)).strikethrough().weak());
                                    } else if column.as_ref().is_some_and(|c| c.kind=="bit") {
                                        let mut flag = value==b"1";
                                        if ui.checkbox(&mut flag,"").changed() { *value=if flag { b"1".to_vec() } else { b"0".to_vec() }; changed=true; }
                                    } else if column.as_ref().is_some_and(|c| !c.references.is_empty()) {
                                        let current=tables::text(value);
                                        let selected=choices.iter().find(|(v,_)|v==&current).map(|(_,n)|n.as_str()).unwrap_or(&current);
                                        let combo=egui::ComboBox::from_id_salt("reference").width(140.0).selected_text(selected).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
                                        if self.render_dropdown && r==0 && choices.len()>1 {
                                            ui.memory_mut(|mem| mem.open_popup(ui.make_persistent_id(egui::Id::new("reference")).with("popup")));
                                        }
                                        combo.show_ui(ui,|ui| {
                                            ui.label("Choose a row");
                                            ui.text_edit_singleline(&mut self.reference_query);
                                            let q=self.reference_query.to_lowercase();
                                            egui::ScrollArea::vertical().max_height(240.0).show(ui,|ui| {
                                                if ui.selectable_label(current.is_empty(),"None / empty").clicked() { value.clear();changed=true;ui.close_menu(); }
                                                for (v,name) in choices.iter().filter(|(_,n)| n.to_lowercase().contains(&q)) {
                                                    if ui.selectable_label(v==&current,name).clicked() { *value=v.as_bytes().to_vec();changed=true;ui.close_menu(); }
                                                }
                                            });
                                        });
                                        if column.as_ref().is_some_and(|c|c.reference_encoding=="expression") {
                                            let mut s=tables::text(value);
                                            if ui.text_edit_singleline(&mut s).on_hover_text("Reference expression; retain any recipe modifiers here").changed() { *value=s.into_bytes();changed=true; }
                                        }
                                    } else {
                                        let mut s=tables::text(value);
                                        let response=ui.add(egui::TextEdit::singleline(&mut s).desired_width(140.0));
                                        if response.changed() { *value=s.into_bytes();changed=true; }
                                        if let Err(e)=tables::validate_cell(column.as_ref(),value) { response.on_hover_text(e.to_string()); ui.colored_label(Color32::LIGHT_RED,"Invalid value"); }
                                    }
                                });
                            });
                        }
                    }));
                }
            });
        });
        if changed {
            self.cached_table = None;
            data.dirty = true;
            data.validate();
        }
        ui.separator();
        ui.strong(format!(
            "Problems ({}){}",
            data.problems.len(),
            if data.dirty { " · unsaved" } else { "" }
        ));
        egui::ScrollArea::vertical()
            .id_salt("problems")
            .max_height(120.0)
            .show(ui, |ui| {
                if data.problems.is_empty() {
                    ui.colored_label(Color32::LIGHT_GREEN, "No table problems");
                }
                for problem in &data.problems {
                    ui.colored_label(Color32::LIGHT_RED, problem);
                }
            });
    }
}
