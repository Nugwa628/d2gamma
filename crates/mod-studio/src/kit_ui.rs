//! Forms for the state, skill and trigger editors, and the per-level curve chart.
use crate::kit::*;
use crate::tables::{self, Tables};
use d2_sim::rules::data_mod::blocks::Family;
use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke};
use std::collections::BTreeSet;
use std::path::Path;

pub mod lift;
mod quests;

/// Which editor is on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    States,
    Skills,
    Triggers,
    Quests,
}
impl View {
    pub const TABS: [(&'static str, View); 4] = [
        ("States", View::States),
        ("Skills", View::Skills),
        ("Triggers", View::Triggers),
        ("Quests", View::Quests),
    ];
    pub fn of(tab: &str) -> Option<View> {
        Self::TABS.iter().find(|(t, _)| *t == tab).map(|(_, v)| *v)
    }
}

#[derive(Default)]
pub struct KitEditor {
    pub kit: Option<Kit>,
    pub error: String,
    pub state: usize,
    pub skill: usize,
    pub trigger: usize,
    pub quest: usize,
    chart: BTreeSet<String>,
    chart_for: Option<(u16, usize)>,
    params: [i32; 8],
    template_open: bool,
    template_ids: Vec<u16>,
    create_rows: bool,
    new_name: String,
    message: String,
}

// ------------------------------------------------------------ table access

fn table_index(t: &Tables, name: &str) -> Option<usize> {
    t.edited
        .tables
        .iter()
        .position(|x| x.name().eq_ignore_ascii_case(name))
}
fn names_of(t: &Tables, name: &str) -> Vec<(u32, String)> {
    let Some(i) = table_index(t, name) else {
        return Vec::new();
    };
    let table = &t.edited.tables[i];
    (0..table.rows.len())
        .filter(|r| !table.disabled(*r))
        .map(|r| (r as u32, tables::row_name(table, r)))
        .collect()
}
fn column(t: &Tables, ti: usize, name: &str) -> Option<usize> {
    t.edited.tables[ti]
        .header
        .iter()
        .position(|h| h.eq_ignore_ascii_case(name.as_bytes()))
}
fn cell(t: &Tables, table: &str, row: usize, col: &str) -> Option<String> {
    let ti = table_index(t, table)?;
    let c = column(t, ti, col)?;
    Some(tables::text(&t.edited.tables[ti].rows.get(row)?.cells[c]))
}
fn set_cell(t: &mut Tables, table: &str, row: usize, col: &str, value: &str) -> bool {
    let Some(ti) = table_index(t, table) else {
        return false;
    };
    let Some(c) = column(t, ti, col) else {
        return false;
    };
    let Some(r) = t.edited.tables[ti].rows.get_mut(row) else {
        return false;
    };
    r.cells[c] = value.as_bytes().to_vec();
    t.dirty = true;
    t.validate();
    true
}
fn row_exists(t: &Tables, table: &str, row: u16) -> bool {
    table_index(t, table).is_some_and(|i| usize::from(row) < t.edited.tables[i].rows.len())
}
/// Adds a row with a key not yet used and returns its id.
fn add_row(t: &mut Tables, table: &str, key: &str) -> anyhow::Result<u16> {
    let ti =
        table_index(t, table).ok_or_else(|| anyhow::anyhow!("The {table} table is not loaded"))?;
    let mut candidate = key.trim().to_owned();
    let mut n = 2;
    while t.edited.tables[ti].rows.iter().enumerate().any(|(r, _)| {
        t.edited.tables[ti]
            .key(r)
            .eq_ignore_ascii_case(candidate.as_bytes())
    }) {
        candidate = format!("{} {n}", key.trim());
        n += 1;
    }
    let r = t.add(ti, None, &candidate)?;
    u16::try_from(r).map_err(|_| anyhow::anyhow!("Row {r} is past the id limit"))
}
pub(crate) fn build_names(t: Option<&Tables>) -> Names {
    let Some(t) = t else { return Names::default() };
    Names {
        states: names_of(t, "states"),
        skills: names_of(t, "skills"),
        stats: names_of(t, "itemstatcost"),
        overlays: names_of(t, "overlay"),
        missiles: names_of(t, "missiles"),
        monsters: names_of(t, "monstats"),
    }
}
fn params_of(t: &Tables, skill: u16) -> Option<[i32; 8]> {
    row_exists(t, "skills", skill).then(|| {
        std::array::from_fn(|i| {
            cell(t, "skills", usize::from(skill), &format!("param{}", i + 1))
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0)
        })
    })
}

// ------------------------------------------------------------------ widgets

pub(crate) const RED: Color32 = Color32::LIGHT_RED;

pub(crate) fn drag<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    v: &mut N,
    range: std::ops::RangeInclusive<N>,
) -> bool {
    ui.add(egui::DragValue::new(v).range(range)).changed()
}

/// A row picker over `rows`, or a plain number when no tables are loaded.
pub(crate) fn pick(ui: &mut egui::Ui, salt: &str, value: &mut u16, rows: &[(u32, String)]) -> bool {
    if rows.is_empty() {
        return drag(ui, value, 0..=u16::MAX);
    }
    let current = rows
        .iter()
        .find(|(i, _)| *i == u32::from(*value))
        .map_or_else(
            || format!("#{value} (no such row)"),
            |(i, n)| format!("{n} · {i}"),
        );
    let mut changed = false;
    let id = ui.id().with(salt);
    egui::ComboBox::from_id_salt(id)
        .width(230.0)
        .selected_text(current)
        .show_ui(ui, |ui| {
            let q_id = id.with("query");
            let mut q: String = ui.data_mut(|d| d.get_temp(q_id).unwrap_or_default());
            ui.add(egui::TextEdit::singleline(&mut q).hint_text("Search"));
            ui.data_mut(|d| d.insert_temp(q_id, q.clone()));
            let q = q.to_lowercase();
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for (i, n) in rows
                        .iter()
                        .filter(|(i, n)| n.to_lowercase().contains(&q) || i.to_string() == q)
                    {
                        if ui
                            .selectable_label(*i == u32::from(*value), format!("{n} · {i}"))
                            .clicked()
                        {
                            *value = *i as u16;
                            changed = true;
                        }
                    }
                });
        });
    changed
}

pub(crate) fn labelled(
    ui: &mut egui::Ui,
    label: &str,
    add: impl FnOnce(&mut egui::Ui) -> bool,
) -> bool {
    ui.horizontal(|ui| {
        ui.add_sized([150.0, 20.0], egui::Label::new(label));
        add(ui)
    })
    .inner
}

fn pick_field(ui: &mut egui::Ui, label: &str, v: &mut u16, rows: &[(u32, String)]) -> bool {
    labelled(ui, label, |ui| pick(ui, label, v, rows))
}

/// A formula with the engine compiler's verdict under it.
fn fm(ui: &mut egui::Ui, label: &str, f: &mut Fm) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([150.0, 20.0], egui::Label::new(label));
        changed = ui
            .add(
                egui::TextEdit::singleline(&mut f.0)
                    .desired_width(260.0)
                    .font(egui::TextStyle::Monospace),
            )
            .changed();
        match f.check() {
            Ok(()) => {
                ui.colored_label(Color32::LIGHT_GREEN, "✔");
            }
            Err(e) => {
                ui.colored_label(
                    RED,
                    format!("✘ {}", e.replace(&format!("formula `{}`: ", f.0), "")),
                );
            }
        }
    });
    changed
}

/// An optional formula: empty means the engine's default.
fn opt_fm(ui: &mut egui::Ui, label: &str, f: &mut Option<Fm>, default: &str) -> bool {
    let mut text = f.as_ref().map_or(String::new(), |f| f.0.clone());
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([150.0, 20.0], egui::Label::new(label));
        if ui
            .add(
                egui::TextEdit::singleline(&mut text)
                    .desired_width(260.0)
                    .font(egui::TextStyle::Monospace)
                    .hint_text(format!("default: {default}")),
            )
            .changed()
        {
            *f = (!text.trim().is_empty()).then(|| Fm(text.clone()));
            changed = true;
        }
        if let Some(Err(e)) = f.as_ref().map(Fm::check) {
            ui.colored_label(
                RED,
                format!("✘ {}", e.replace(&format!("formula `{text}`: "), "")),
            );
        } else if f.is_some() {
            ui.colored_label(Color32::LIGHT_GREEN, "✔");
        }
    });
    changed
}

/// A choice where "none" means the engine's default.
fn choice(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut Option<String>,
    options: &[(&str, &str)],
    default: &str,
) -> bool {
    let mut changed = false;
    let shown = |key: &str| {
        options
            .iter()
            .find(|(k, _)| *k == key)
            .map_or(key.to_owned(), |(_, l)| (*l).to_owned())
    };
    labelled(ui, label, |ui| {
        let text = match v {
            Some(k) => shown(k),
            None => format!("{} (default)", shown(default)),
        };
        egui::ComboBox::from_id_salt(label)
            .width(230.0)
            .selected_text(text)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(v.is_none(), format!("{} (default)", shown(default)))
                    .clicked()
                {
                    *v = None;
                    changed = true;
                }
                for (k, l) in options {
                    if ui.selectable_label(v.as_deref() == Some(*k), *l).clicked() {
                        *v = Some((*k).to_owned());
                        changed = true;
                    }
                }
            });
        changed
    });
    changed
}

fn required_choice(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut String,
    options: &[(&str, &str)],
) -> bool {
    let mut changed = false;
    labelled(ui, label, |ui| {
        let text = options
            .iter()
            .find(|(k, _)| k == v)
            .map_or(v.clone(), |(_, l)| (*l).to_owned());
        egui::ComboBox::from_id_salt(label)
            .width(230.0)
            .selected_text(text)
            .show_ui(ui, |ui| {
                for (k, l) in options {
                    if ui.selectable_label(v == k, *l).clicked() {
                        *v = (*k).to_owned();
                        changed = true;
                    }
                }
            });
        changed
    });
    changed
}

pub(crate) fn move_buttons(ui: &mut egui::Ui, i: usize, len: usize) -> (Option<usize>, bool) {
    let mut to = None;
    let mut remove = false;
    if ui
        .add_enabled(i > 0, egui::Button::new("Up").small())
        .on_hover_text("Move up")
        .clicked()
    {
        to = Some(i - 1);
    }
    if ui
        .add_enabled(i + 1 < len, egui::Button::new("Down").small())
        .on_hover_text("Move down")
        .clicked()
    {
        to = Some(i + 1);
    }
    if ui
        .add(egui::Button::new("Remove").small())
        .on_hover_text("Remove")
        .clicked()
    {
        remove = true;
    }
    (to, remove)
}

fn slug(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    s.trim_matches('_').to_owned()
}

// ----------------------------------------------------------------- editor

impl KitEditor {
    pub fn open(&mut self, root: &Path) {
        *self = KitEditor::default();
        match Kit::load(root) {
            Ok(kit) => self.kit = Some(kit),
            Err(e) => self.error = format!("{e:#}"),
        }
    }
    pub fn dirty(&self) -> bool {
        self.kit.as_ref().is_some_and(|k| k.dirty)
    }
    pub fn save(&mut self) -> anyhow::Result<()> {
        match &mut self.kit {
            Some(k) if k.dirty => {
                let problems = k.problems();
                if let Some(p) = problems.first() {
                    anyhow::bail!("Fix the {} problems before saving:\n{}", p.file, p.message);
                }
                k.save()
            }
            _ => Ok(()),
        }
    }

    pub fn draw(&mut self, ui: &mut egui::Ui, view: View, mut tables: Option<&mut Tables>) {
        if !self.error.is_empty() {
            ui.colored_label(RED, &self.error);
            ui.label("Studio will not overwrite a file it cannot read. Fix it in a text editor, then reopen the project.");
            return;
        }
        let names = build_names(tables.as_deref());
        let Some(kit) = &mut self.kit else {
            ui.label("Open a project to edit its states, skills and triggers.");
            return;
        };
        if tables.is_none() {
            ui.weak("No install loaded: ids are plain numbers. Load tables to pick rows by name and add the rows these files need.");
        }
        let mut changed = false;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                let width = if view == View::Triggers { 380.0 } else { 270.0 };
                ui.set_width(width);
                ui.set_max_width(width);
                changed |= match view {
                    View::States => list_states(
                        ui,
                        kit,
                        &names,
                        &mut self.state,
                        &mut self.new_name,
                        &mut tables,
                        &mut self.message,
                    ),
                    View::Skills => list_skills(
                        ui,
                        kit,
                        &names,
                        &mut self.skill,
                        &mut self.new_name,
                        &mut tables,
                        &mut self.message,
                    ),
                    View::Triggers => list_triggers(ui, kit, &names, &mut self.trigger),
                    View::Quests => quests::list(ui, kit, &names, &mut self.quest),
                };
                ui.separator();
                changed |= view != View::Quests
                    && template_panel(
                        ui,
                        kit,
                        &mut tables,
                        &mut self.template_open,
                        &mut self.template_ids,
                        &mut self.create_rows,
                        &mut self.message,
                        &mut [&mut self.state, &mut self.skill, &mut self.trigger],
                    );
            });
            ui.separator();
            ui.vertical(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("kit-form", view as u8))
                    .show(ui, |ui| {
                        match view {
                            View::States => {
                                changed |= state_form(ui, kit, &names, self.state, &mut tables)
                            }
                            View::Skills => {
                                changed |= skill_form(
                                    ui,
                                    kit,
                                    &names,
                                    self.skill,
                                    &mut tables,
                                    &mut self.chart,
                                    &mut self.chart_for,
                                    &mut self.params,
                                )
                            }
                            View::Triggers => {
                                changed |= trigger_form(ui, kit, &names, self.trigger)
                            }
                            View::Quests => changed |= quests::form(ui, kit, &names, self.quest),
                        }
                        problems_panel(ui, kit, view);
                        block_reference_panel(ui, view);
                    });
            });
        });
        if !self.message.is_empty() {
            ui.colored_label(RED, &self.message);
        }
        if changed {
            kit.dirty = true;
        }
    }
}

fn problems_panel(ui: &mut egui::Ui, kit: &Kit, view: View) {
    let file = match view {
        View::States => STATES_FILE,
        View::Skills => SKILLS_FILE,
        View::Triggers => TRIGGERS_FILE,
        View::Quests => QUESTS_FILE,
    };
    let all = kit.problems();
    ui.separator();
    let mine: Vec<_> = all.iter().filter(|p| p.file == file).collect();
    ui.strong(format!("Problems in {file} ({})", mine.len()));
    if mine.is_empty() {
        ui.colored_label(Color32::LIGHT_GREEN, "The engine accepts this file.");
    }
    for p in mine {
        ui.colored_label(RED, &p.message);
    }
    let others = all.len() - all.iter().filter(|p| p.file == file).count();
    if others > 0 {
        ui.weak(format!("{others} more problem(s) in the other kit files."));
    }
}

/// The block vocabulary for the open page: every block the engine knows for this
/// file, with what it does.
fn block_reference_panel(ui: &mut egui::Ui, view: View) {
    let file = match view {
        View::States => return,
        View::Skills => SKILLS_FILE,
        View::Triggers => TRIGGERS_FILE,
        View::Quests => QUESTS_FILE,
    };
    ui.separator();
    egui::CollapsingHeader::new("Block reference")
        .id_salt(("block-reference", view as u8))
        .show(ui, |ui| {
            for (title, rows) in block_reference(file) {
                ui.strong(title);
                for row in rows {
                    ui.label(format!("{} (`{}`): {}", row.label, row.name, row.about));
                }
            }
        });
}

// ------------------------------------------------------------------ lists

fn list_header(ui: &mut egui::Ui, title: &str) {
    ui.heading(title);
}

fn list_states(
    ui: &mut egui::Ui,
    kit: &mut Kit,
    names: &Names,
    sel: &mut usize,
    new_name: &mut String,
    tables: &mut Option<&mut Tables>,
    message: &mut String,
) -> bool {
    list_header(ui, "States");
    let mut changed = false;
    for (i, s) in kit.states.iter().enumerate() {
        let label = format!("{} · {}", kit.state_name(names, s.id), s.id);
        if ui.selectable_label(*sel == i, label).clicked() {
            *sel = i;
        }
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(new_name)
                .desired_width(140.0)
                .hint_text("New state name"),
        );
        if ui
            .add_enabled(!new_name.trim().is_empty(), egui::Button::new("Add"))
            .clicked()
        {
            let id = match tables {
                Some(t) => add_row(t, "states", &slug(new_name)).map_err(|e| e.to_string()),
                None => Ok(kit.states.iter().map(|s| s.id).max().map_or(1, |m| m + 1)),
            };
            match id {
                Ok(id) => {
                    kit.states.push(StateEntry::new(id, new_name.trim()));
                    *sel = kit.states.len() - 1;
                    new_name.clear();
                    message.clear();
                    changed = true;
                }
                Err(e) => *message = e,
            }
        }
    });
    if ui
        .add_enabled(
            *sel < kit.states.len(),
            egui::Button::new("Delete selected"),
        )
        .clicked()
    {
        kit.states.remove(*sel);
        *sel = sel.saturating_sub(1);
        changed = true;
    }
    changed
}

fn list_skills(
    ui: &mut egui::Ui,
    kit: &mut Kit,
    names: &Names,
    sel: &mut usize,
    new_name: &mut String,
    tables: &mut Option<&mut Tables>,
    message: &mut String,
) -> bool {
    list_header(ui, "Skills made of parts");
    let mut changed = false;
    for (i, s) in kit.skills.iter().enumerate() {
        let label = format!("{} · {}", kit.skill_name(names, s.id), s.id);
        if ui.selectable_label(*sel == i, label).clicked() {
            *sel = i;
        }
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(new_name)
                .desired_width(140.0)
                .hint_text("New skill name"),
        );
        if ui
            .add_enabled(!new_name.trim().is_empty(), egui::Button::new("Add"))
            .clicked()
        {
            let id = match tables {
                Some(t) => add_row(t, "skills", new_name.trim())
                    .inspect(|id| {
                        set_cell(
                            t,
                            "skills",
                            usize::from(*id),
                            "srvdofunc",
                            &BLOCK_SKILL_SLOT.to_string(),
                        );
                    })
                    .map_err(|e| e.to_string()),
                None => Ok(kit.skills.iter().map(|s| s.id).max().map_or(1, |m| m + 1)),
            };
            match id {
                Ok(id) => {
                    kit.skills.push(SkillEntry::new(id, new_name.trim()));
                    *sel = kit.skills.len() - 1;
                    new_name.clear();
                    message.clear();
                    changed = true;
                }
                Err(e) => *message = e,
            }
        }
    });
    if ui
        .add_enabled(
            *sel < kit.skills.len(),
            egui::Button::new("Delete selected"),
        )
        .clicked()
    {
        kit.skills.remove(*sel);
        *sel = sel.saturating_sub(1);
        changed = true;
    }
    if let Some(t) = tables.as_deref_mut() {
        changed |= copy_base_panel(ui, kit, t, sel, message);
    }
    changed
}

/// "Copy a base skill": a base skill of the tables becomes parts to edit.
fn copy_base_panel(
    ui: &mut egui::Ui,
    kit: &mut Kit,
    t: &mut Tables,
    sel: &mut usize,
    message: &mut String,
) -> bool {
    let mut changed = false;
    ui.add_space(6.0);
    egui::CollapsingHeader::new("Copy a base skill")
        .id_salt("copy-base")
        .show(ui, |ui| {
            let id = ui.id().with("copy-base-pick");
            let rows = lift::liftable(t);
            let mut pick: Option<u16> = ui.data_mut(|d| d.get_temp(id));
            let label = |r: &lift::Liftable| format!("{} · {} ({})", r.name, r.id, r.family.name());
            let shown = rows
                .iter()
                .find(|r| Some(r.id) == pick)
                .map_or_else(|| "Pick a skill".to_owned(), label);
            egui::ComboBox::from_id_salt("copy-base-combo")
                .width(220.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for r in &rows {
                        ui.selectable_value(&mut pick, Some(r.id), label(r));
                    }
                });
            ui.data_mut(|d| d.insert_temp(id, pick));
            if ui
                .add_enabled(pick.is_some(), egui::Button::new("Copy as parts"))
                .clicked()
            {
                if let Some(base) = pick {
                    match lift::copy_base(kit, t, base) {
                        Ok(lifted) => {
                            *sel = kit.skills.len() - 1;
                            *message = lifted.notes.join(" ");
                            changed = true;
                        }
                        Err(e) => *message = e.to_string(),
                    }
                }
            }
            ui.weak("The available copies use the parts supported by this version of the lifter.");
        });
    changed |= lift::player::panel(ui, kit, t, sel, message);
    changed
}

fn list_triggers(ui: &mut egui::Ui, kit: &mut Kit, names: &Names, sel: &mut usize) -> bool {
    list_header(ui, "Triggers");
    let mut changed = false;
    let sentences: Vec<String> = kit
        .triggers
        .iter()
        .map(|t| kit.trigger_sentence(names, t))
        .collect();
    let len = kit.triggers.len();
    let mut swap = None;
    let mut remove = None;
    for (i, sentence) in sentences.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            ui.label(RichText::new(format!("{}.", i + 1)).weak());
            if ui.selectable_label(*sel == i, sentence).clicked() {
                *sel = i;
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(22.0);
            let (to, rm) = move_buttons(ui, i, len);
            if let Some(to) = to {
                swap = Some((i, to));
            }
            if rm {
                remove = Some(i);
            }
        });
    }
    if let Some((a, b)) = swap {
        kit.triggers.swap(a, b);
        *sel = b;
        changed = true;
    }
    if let Some(i) = remove {
        kit.triggers.remove(i);
        *sel = sel.saturating_sub(1);
        changed = true;
    }
    if ui.button("Add trigger").clicked() {
        kit.triggers.push(TriggerEntry::new("New trigger"));
        *sel = kit.triggers.len() - 1;
        changed = true;
    }
    changed
}

#[allow(clippy::too_many_arguments)]
fn template_panel(
    ui: &mut egui::Ui,
    kit: &mut Kit,
    tables: &mut Option<&mut Tables>,
    open: &mut bool,
    ids: &mut Vec<u16>,
    create_rows: &mut bool,
    message: &mut String,
    selections: &mut [&mut usize; 3],
) -> bool {
    let mut changed = false;
    if ui.button("New from template…").clicked() {
        *open = !*open;
    }
    if !*open {
        return false;
    }
    let template = &TEMPLATES[0];
    ui.strong(template.name);
    ui.label(template.about);
    if ids.len() != template.rows.len() {
        *ids = template.rows.iter().map(|r| r.id).collect();
    }
    let have_tables = tables.is_some();
    ui.add_enabled_ui(have_tables, |ui| {
        ui.checkbox(
            create_rows,
            "Also add the table rows it needs (states, skills, stat)",
        );
    });
    if !have_tables {
        *create_rows = false;
        ui.weak("Choose ids below; add their table rows later with the table editor.");
    }
    if !*create_rows {
        for (row, id) in template.rows.iter().zip(ids.iter_mut()) {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [170.0, 18.0],
                    egui::Label::new(format!("{} {} (was {})", row.table, row.key, row.id)),
                );
                drag(ui, id, 0..=u16::MAX);
            });
        }
    }
    if ui.button("Add to this mod").clicked() {
        let chosen: Vec<u16> = if *create_rows {
            Vec::new()
        } else {
            ids.clone()
        };
        match apply_template(kit, tables.as_deref_mut(), template, &chosen) {
            Ok(()) => {
                *selections[0] = kit.states.len().saturating_sub(2);
                *selections[1] = kit.skills.len().saturating_sub(3);
                *selections[2] = kit.triggers.len().saturating_sub(3);
                message.clear();
                *open = false;
                changed = true;
            }
            Err(e) => *message = format!("{e:#}"),
        }
    }
    changed
}

/// Adds `template` to the kit. With `ids` empty and tables given, the rows it
/// needs are added to the tables (skills get the block slot) and the template's
/// ids become those new rows; otherwise `ids` gives the new id of each template row.
pub fn apply_template(
    kit: &mut Kit,
    mut tables: Option<&mut Tables>,
    template: &Template,
    ids: &[u16],
) -> anyhow::Result<()> {
    let mut map = IdMap::default();
    for (n, row) in template.rows.iter().enumerate() {
        let new = match (ids.get(n), tables.as_deref_mut()) {
            (Some(id), _) => *id,
            (None, Some(t)) => {
                let id = add_row(t, row.table, row.key)?;
                if row.block {
                    set_cell(
                        t,
                        row.table,
                        usize::from(id),
                        "srvdofunc",
                        &BLOCK_SKILL_SLOT.to_string(),
                    );
                }
                id
            }
            (None, None) => row.id,
        };
        let list = match row.kind {
            IdKind::State => &mut map.states,
            IdKind::Skill => &mut map.skills,
            IdKind::Stat => &mut map.stats,
        };
        list.push((row.id, new));
    }
    kit.add_template(template, &map)
}

// ------------------------------------------------------------ state form

fn table_note(ui: &mut egui::Ui, text: &str, fix: Option<&str>) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.colored_label(Color32::from_rgb(230, 190, 90), format!("⚠ {text}"));
        if let Some(fix) = fix {
            clicked = ui.small_button(fix).clicked();
        }
    });
    clicked
}

fn state_form(
    ui: &mut egui::Ui,
    kit: &mut Kit,
    names: &Names,
    i: usize,
    tables: &mut Option<&mut Tables>,
) -> bool {
    let Some(mut s) = kit.states.get(i).cloned() else {
        ui.label("Add a state on the left, or start from a template.");
        return false;
    };
    let before = s.clone();
    ui.heading(if s.name.is_empty() {
        format!("State {}", s.id)
    } else {
        s.name.clone()
    });
    labelled(ui, "Name", |ui| {
        ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(260.0))
            .changed()
    });
    labelled(ui, "States table row", |ui| {
        drag(ui, &mut s.id, 0..=u16::MAX)
    });
    if let Some(t) = tables.as_deref_mut() {
        if !row_exists(t, "states", s.id)
            && table_note(
                ui,
                "No row with this id in the states table.",
                Some("Add the row"),
            )
        {
            if let Ok(id) = add_row(t, "states", &slug(&s.name)) {
                s.id = id;
            }
        }
    }
    labelled(ui, "Duration", |ui| {
        let mut own = s.duration_seconds.is_some();
        let mut changed = ui.checkbox(&mut own, "own time").changed();
        if changed {
            s.duration_seconds = own.then_some(10);
        }
        if let Some(d) = &mut s.duration_seconds {
            changed |= drag(ui, d, 1..=86_400);
            ui.label("seconds");
        } else {
            ui.weak("the applying skill's time");
        }
        changed
    });
    labelled(ui, "Max stacks", |ui| {
        let c = drag(ui, &mut s.max_stacks, 1..=255);
        let max = s.max_stacks as u16;
        for e in &mut s.effects {
            e.min_stacks = e.min_stacks.clamp(1, max.max(1));
        }
        c
    });
    let mut reapply = s.reapply.clone();
    if choice(ui, "When applied again", &mut reapply, REAPPLY, "refresh") {
        s.reapply = reapply;
    }
    let mut decay = s.decay.clone();
    if choice(ui, "When time runs out", &mut decay, DECAY, "lose_all") {
        s.decay = decay;
    }
    labelled(ui, "Icon name", |ui| {
        let mut text = s.icon.clone().unwrap_or_default();
        let c = ui
            .add(
                egui::TextEdit::singleline(&mut text)
                    .desired_width(200.0)
                    .hint_text("none"),
            )
            .changed();
        if c {
            s.icon = (!text.trim().is_empty()).then_some(text.clone());
        }
        if let Some(icon) = &s.icon {
            let file = kit.root.join(format!("files/icons/{icon}.png"));
            if file.exists() {
                ui.colored_label(Color32::LIGHT_GREEN, "✔ files/icons/ has it");
            } else {
                ui.weak(format!(
                    "add files/icons/{icon}.png (HUD shows a letter square until then)"
                ));
            }
        }
        c
    });
    labelled(ui, "Overlay effect", |ui| {
        let mut on = s.overlay.is_some();
        let mut c = ui.checkbox(&mut on, "").changed();
        if c {
            s.overlay = on.then_some(0);
        }
        if let Some(o) = &mut s.overlay {
            c |= pick(ui, "overlay", o, &names.overlays);
        } else {
            ui.weak("none");
        }
        c
    });
    ui.separator();
    ui.strong("What one stack gives");
    let mut remove = None;
    for (n, e) in s.effects.iter_mut().enumerate() {
        ui.push_id(n, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Stat");
                pick(ui, "stat", &mut e.stat, &names.stats);
                ui.label("per stack");
                drag(ui, &mut e.per_stack, i32::MIN..=i32::MAX);
                ui.label("from stack");
                drag(ui, &mut e.min_stacks, 1..=s.max_stacks.min(255) as u16);
                ui.checkbox(&mut e.flat, "flat").on_hover_text(
                    "Give the value once from that stack on, however many stacks there are",
                );
                if ui.small_button("Remove").clicked() {
                    remove = Some(n);
                }
            });
        });
        if let Some(t) = tables.as_deref() {
            if !names.stats.is_empty() && !row_exists(t, "itemstatcost", e.stat) {
                ui.colored_label(RED, "No such itemstatcost row.");
            }
        }
    }
    if let Some(n) = remove {
        s.effects.remove(n);
    }
    if ui.button("Add effect").clicked() {
        s.effects.push(EffectEntry {
            stat: 0,
            per_stack: 1,
            min_stacks: 1,
            flat: false,
        });
    }
    ui.separator();
    ui.strong("Preview");
    ui.label(kit.state_summary(&s));
    for line in kit.state_preview(names, &s) {
        ui.label(RichText::new(line).monospace());
    }
    let changed = s != before;
    if changed {
        kit.states[i] = s;
    }
    changed
}

// ------------------------------------------------------------ skill form

fn damage_form(ui: &mut egui::Ui, d: &mut DamageEntry, names: &Names) -> bool {
    let mut c = false;
    let mut element = d.element.clone();
    let options: Vec<(&str, &str)> = ELEMENTS.iter().map(|e| (*e, *e)).collect();
    if choice(ui, "Element", &mut element, &options, "physical") {
        d.element = element;
        c = true;
    }
    c |= fm(ui, "Damage min", &mut d.min);
    c |= fm(ui, "Damage max", &mut d.max);
    c |= opt_fm(ui, "Duration (poison, cold)", &mut d.seconds, "0");
    c |= labelled(ui, "Bonus from stat", |ui| {
        let mut on = d.bonus_stat.is_some();
        let mut c = ui.checkbox(&mut on, "").changed();
        if c {
            d.bonus_stat = on.then_some(0);
        }
        if let Some(b) = &mut d.bonus_stat {
            c |= pick(ui, "bonus", b, &names.stats);
            ui.weak("read on you as a percent");
        }
        c
    });
    c |= labelled(ui, "Charge-up", |ui| {
        let mut on = d.charge.is_some();
        let c = ui
            .checkbox(&mut on, "grows with the stacks of a state on you")
            .changed();
        if c {
            d.charge = on.then(|| ChargeEntry {
                state: 0,
                percent: Fm::new("10"),
                consume: false,
            });
        }
        c
    });
    if let Some(ch) = &mut d.charge {
        c |= pick_field(ui, "  State", &mut ch.state, &names.states);
        c |= fm(ui, "  Percent per stack", &mut ch.percent);
        c |= ui.checkbox(&mut ch.consume, "Spend the stacks").changed();
    }
    c
}

/// The range, time, filter and stats of an aura or curse part.
fn aura_fields(
    ui: &mut egui::Ui,
    curse: bool,
    range: &mut Fm,
    seconds: &mut Fm,
    affects: &mut Option<String>,
    stats: &mut Vec<AuraStatEntry>,
    names: &Names,
) -> bool {
    let mut c = fm(ui, "Range", range);
    c |= fm(ui, "Seconds", seconds);
    c |= choice(
        ui,
        "Affects",
        affects,
        AFFECTS,
        if curse { "enemies" } else { "allies" },
    );
    let mut remove = None;
    for (n, s) in stats.iter_mut().enumerate() {
        ui.push_id(n, |ui| {
            c |= pick_field(ui, &format!("Stat {}", n + 1), &mut s.stat, &names.stats);
            c |= fm(ui, "  value", &mut s.value);
            if ui.small_button("Remove stat").clicked() {
                remove = Some(n);
            }
        });
    }
    if let Some(n) = remove {
        stats.remove(n);
        c = true;
    }
    if stats.len() < 6 && ui.small_button("Add stat").clicked() {
        stats.push(AuraStatEntry {
            stat: 0,
            value: Fm::new("1"),
        });
        c = true;
    }
    c
}

fn part_form(ui: &mut egui::Ui, p: &mut PartEntry, names: &Names) -> bool {
    let mut c = false;
    let curse = p.kind() == "curse";
    match p {
        PartEntry::CorpseFind {
            chance,
            potion,
            band1,
            band2,
            band3,
            band4,
        } => {
            c |= fm(ui, "Chance percent", chance);
            c |= ui.checkbox(potion, "Find potion").changed();
            for (label, value) in [
                ("Band1", band1),
                ("Band2", band2),
                ("Band3", band3),
                ("Band4", band4),
            ] {
                c |= fm(ui, label, value);
            }
        }
        PartEntry::Raven {
            class,
            pet_type,
            mode,
            pet_max,
            level,
        } => {
            c |= pick_field(ui, "Monster", class, &names.monsters);
            c |= labelled(ui, "Pet type", |ui| drag(ui, pet_type, 0..=127));
            c |= labelled(ui, "Mode", |ui| drag(ui, mode, 0..=15));
            c |= fm(ui, "Pet cap", pet_max);
            c |= fm(ui, "Level", level);
        }
        PartEntry::Golem {
            class,
            pet_type,
            mode,
            pet_max,
            level,
        } => {
            c |= pick_field(ui, "Monster", class, &names.monsters);
            c |= labelled(ui, "Pet type", |ui| drag(ui, pet_type, 0..=127));
            c |= labelled(ui, "Mode", |ui| drag(ui, mode, 0..=15));
            c |= fm(ui, "Pet cap", pet_max);
            c |= fm(ui, "Level (0: owner scaling)", level);
        }
        PartEntry::Passive {
            state,
            layer,
            blocked_by,
            stats,
        } => {
            c |= pick_field(ui, "Passive state", state, &names.states);
            c |= labelled(ui, "Item-type layer", |ui| drag(ui, layer, 0..=32767));
            let mut has = blocked_by.is_some();
            if ui.checkbox(&mut has, "Suppressed by aura state").changed() {
                *blocked_by = has.then_some(*state);
                c = true;
            }
            if let Some(s) = blocked_by {
                c |= pick_field(ui, "Blocking state", s, &names.states);
            }
            let mut remove = None;
            for (i, stat) in stats.iter_mut().enumerate() {
                ui.push_id(("passive-stat", i), |ui| {
                    c |= pick_field(ui, "Stat", &mut stat.stat, &names.stats);
                    c |= fm(ui, "Value", &mut stat.value);
                    if ui.small_button("Remove stat").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                stats.remove(i);
                c = true;
            }
            if stats.len() < 5 && ui.small_button("Add stat").clicked() {
                stats.push(AuraStatEntry {
                    stat: 0,
                    value: Fm::new("1"),
                });
                c = true;
            }
        }
        PartEntry::MartialCharge {
            state,
            counter,
            frames,
            attack_rating,
            source_damage,
            claws,
            bonus,
        } => {
            c |= pick_field(ui, "State", state, &names.states);
            c |= pick_field(ui, "Counter stat", counter, &names.stats);
            c |= fm(ui, "Duration", frames);
            c |= fm(ui, "Attack rating", attack_rating);
            c |= labelled(ui, "Source scale /128", |ui| {
                drag(ui, source_damage, 0..=255)
            });
            c |= ui.checkbox(claws, "Dual-claw frame selection").changed();
            let mut has = bonus.is_some();
            if ui.checkbox(&mut has, "Incremental stat bonus").changed() {
                *bonus = has.then(|| AuraStatEntry {
                    stat: 31,
                    value: Fm::new("1"),
                });
                c = true;
            }
            if let Some(s) = bonus {
                c |= pick_field(ui, "Bonus stat", &mut s.stat, &names.stats);
                c |= fm(ui, "Bonus per charge", &mut s.value);
            }
        }
        PartEntry::Shadow {
            class,
            pet_type,
            mode,
            pet_max,
            life_percent,
            item_level,
            frames,
            state,
            umod,
            stats,
        } => {
            c |= pick_field(ui, "Monster", class, &names.monsters);
            c |= labelled(ui, "Pet type", |ui| drag(ui, pet_type, 0..=127));
            c |= labelled(ui, "Mode", |ui| drag(ui, mode, 0..=15));
            for (key, value) in [
                ("Pet cap", pet_max),
                ("Life percent", life_percent),
                ("Equipment level", item_level),
                ("Lifetime ticks", frames),
            ] {
                c |= fm(ui, key, value);
            }
            let mut has = state.is_some();
            if ui.checkbox(&mut has, "Pet state").changed() {
                *state = has.then_some(1);
                c = true;
            }
            if let Some(s) = state {
                c |= pick_field(ui, "State", s, &names.states);
            }
            let mut has = umod.is_some();
            if ui.checkbox(&mut has, "Monster modifier").changed() {
                *umod = has.then_some(1);
                c = true;
            }
            if let Some(u) = umod {
                c |= labelled(ui, "Modifier", |ui| drag(ui, u, 1..=42));
            }
            let mut remove = None;
            for (i, s) in stats.iter_mut().enumerate() {
                ui.push_id(("shadow-stat", i), |ui| {
                    c |= pick_field(ui, "Stat", &mut s.stat, &names.stats);
                    c |= fm(ui, "Value", &mut s.value);
                    if ui.small_button("Remove stat").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                stats.remove(i);
                c = true;
            }
            if stats.len() < 11 && ui.small_button("Add stat").clicked() {
                stats.push(AuraStatEntry {
                    stat: 0,
                    value: Fm::new("1"),
                });
                c = true;
            }
        }
        PartEntry::AiCurse {
            state,
            range,
            frames,
            filter,
            attract,
            stats,
        } => {
            c |= pick_field(ui, "State", state, &names.states);
            c |= fm(ui, "Range", range);
            c |= fm(ui, "Duration ticks", frames);
            c |= labelled(ui, "Scan filter", |ui| {
                ui.add(egui::DragValue::new(filter)).changed()
            });
            c |= ui.checkbox(attract, "Attract").changed();
            let mut remove = None;
            for (i, s) in stats.iter_mut().enumerate() {
                ui.push_id(("ai-curse-stat", i), |ui| {
                    c |= pick_field(ui, "Stat", &mut s.stat, &names.stats);
                    c |= fm(ui, "Value", &mut s.value);
                    if ui.small_button("Remove stat").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                stats.remove(i);
                c = true;
            }
            if !*attract && stats.len() < 5 && ui.small_button("Add stat").clicked() {
                stats.push(AuraStatEntry {
                    stat: 0,
                    value: Fm::new("1"),
                });
                c = true;
            }
        }
        PartEntry::ProjectileFan {
            empty_ok,
            missile,
            non_bow,
            count,
            central,
            activate,
            jitter,
        } => {
            c |= pick_field(ui, "Missile", missile, &names.missiles);
            let mut has = non_bow.is_some();
            if ui.checkbox(&mut has, "Non-bow missile").changed() {
                *non_bow = has.then_some(0);
                c = true;
            }
            if let Some(m) = non_bow {
                c |= pick_field(ui, "Alternative", m, &names.missiles);
            }
            for (k, v) in [
                ("Count", count),
                ("Central shots", central),
                ("Activation", activate),
            ] {
                c |= fm(ui, k, v);
            }
            c |= ui.checkbox(jitter, "Jitter fan").changed();
            c |= ui.checkbox(empty_ok, "Empty count succeeds").changed();
        }
        PartEntry::GuidedProjectile {
            missile,
            non_bow,
            damage_percent,
        } => {
            c |= pick_field(ui, "Missile", missile, &names.missiles);
            let mut has = non_bow.is_some();
            if ui.checkbox(&mut has, "Non-bow missile").changed() {
                *non_bow = has.then_some(0);
                c = true;
            }
            if let Some(m) = non_bow {
                c |= pick_field(ui, "Alternative", m, &names.missiles);
            }
            c |= fm(ui, "Damage percent", damage_percent);
        }
        PartEntry::Sight {
            state,
            stat,
            value,
            range,
            frames,
            filter,
        } => {
            c |= pick_field(ui, "State", state, &names.states);
            c |= pick_field(ui, "Stat", stat, &names.stats);
            for (k, v) in [
                ("Value", value),
                ("Range", range),
                ("Duration ticks", frames),
            ] {
                c |= fm(ui, k, v);
            }
            c |= labelled(ui, "Filter", |ui| {
                ui.add(egui::DragValue::new(filter)).changed()
            });
        }
        PartEntry::DamageAura {
            state,
            target_state,
            period,
            range,
            cost,
            filter,
            freeze,
            hit_class,
            result_flags,
            hit_flags,
            own_stats,
            stats,
        } => {
            c |= pick_field(ui, "Caster state", state, &names.states);
            let mut has = target_state.is_some();
            if ui.checkbox(&mut has, "Target state").changed() {
                *target_state = if has { Some(*state) } else { None };
                c = true;
            }
            if let Some(s) = target_state {
                c |= pick_field(ui, "Target state", s, &names.states);
            }
            c |= fm(ui, "Period", period);
            c |= fm(ui, "Radius", range);
            c |= fm(ui, "Mana threshold", cost);
            c |= labelled(ui, "Aura filter", |ui| drag(ui, filter, 0..=u32::MAX));
            c |= ui.checkbox(freeze, "Freeze pulse").changed();
            c |= labelled(ui, "Hit class", |ui| drag(ui, hit_class, 0..=u8::MAX));
            c |= labelled(ui, "Result flags", |ui| {
                drag(ui, result_flags, 0..=u16::MAX)
            });
            c |= labelled(ui, "Hit flags", |ui| drag(ui, hit_flags, 0..=u32::MAX));
            for (key, items, max) in [("Caster stats", own_stats, 5), ("Target stats", stats, 6)] {
                ui.push_id(key, |ui| {
                    ui.label(key);
                    let mut remove = None;
                    for (i, stat) in items.iter_mut().enumerate() {
                        ui.push_id(i, |ui| {
                            c |= pick_field(ui, "Stat", &mut stat.stat, &names.stats);
                            c |= fm(ui, "Value", &mut stat.value);
                            if ui.small_button("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        items.remove(i);
                        c = true;
                    }
                    if items.len() < max && ui.small_button("Add stat").clicked() {
                        items.push(AuraStatEntry {
                            stat: 0,
                            value: Fm::new("1"),
                        });
                        c = true;
                    }
                });
            }
        }
        PartEntry::Vine {
            class,
            pet_type,
            state,
            pet_max,
            level,
        } => {
            c |= pick_field(ui, "Monster", class, &names.monsters);
            c |= labelled(ui, "Pet type", |ui| drag(ui, pet_type, 0..=127));
            c |= pick_field(ui, "Movement state", state, &names.states);
            c |= fm(ui, "Pet cap", pet_max);
            c |= fm(ui, "Level", level);
        }
        PartEntry::Form {
            state,
            frames,
            delay,
            stats,
        } => {
            c |= pick_field(ui, "Form state", state, &names.states);
            c |= fm(ui, "Base ticks", frames);
            c |= fm(ui, "Toggle cooldown", delay);
            let mut remove = None;
            for (i, stat) in stats.iter_mut().enumerate() {
                ui.push_id(("form-stat", i), |ui| {
                    c |= pick_field(ui, "Stat", &mut stat.stat, &names.stats);
                    c |= fm(ui, "Value", &mut stat.value);
                    if ui.small_button("Remove stat").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                stats.remove(i);
                c = true;
            }
            if stats.len() < 6 && ui.small_button("Add stat").clicked() {
                stats.push(AuraStatEntry {
                    stat: 0,
                    value: Fm::new("1"),
                });
                c = true;
            }
        }
        PartEntry::Sentry {
            class,
            pet_type,
            mode,
            pet_max,
            in_town,
        } => {
            c |= pick_field(ui, "Monster", class, &names.monsters);
            c |= labelled(ui, "Pet type", |ui| drag(ui, pet_type, 0..=127));
            c |= labelled(ui, "Initial mode", |ui| drag(ui, mode, 0..=15));
            c |= fm(ui, "Pet cap", pet_max);
            c |= ui.checkbox(in_town, "Permit in town").changed();
        }
        PartEntry::Warcry {
            missile,
            state,
            frames,
            stats,
        } => {
            c |= pick_field(ui, "Warcry missile", missile, &names.missiles);
            let mut active = state.is_some();
            if ui.checkbox(&mut active, "Apply timed state").changed() {
                *state = active.then_some(0);
                c = true;
            }
            if let Some(state) = state {
                c |= pick_field(ui, "State", state, &names.states);
            }
            c |= fm(ui, "Base ticks", frames);
            let mut remove = None;
            for (i, stat) in stats.iter_mut().enumerate() {
                ui.push_id(("warcry-stat", i), |ui| {
                    c |= pick_field(ui, "Stat", &mut stat.stat, &names.stats);
                    c |= fm(ui, "Value", &mut stat.value);
                    if ui.small_button("Remove stat").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                stats.remove(i);
                c = true;
            }
            if stats.len() < 6 && ui.small_button("Add stat").clicked() {
                stats.push(AuraStatEntry {
                    stat: 0,
                    value: Fm::new("1"),
                });
                c = true;
            }
        }
        PartEntry::DruidSummon {
            class,
            pet_type,
            mode,
            pet_max,
            level,
        } => {
            c |= pick_field(ui, "Monster class", class, &names.monsters);
            c |= labelled(ui, "Pet type row", |ui| drag(ui, pet_type, 0..=127));
            c |= labelled(ui, "Initial mode", |ui| drag(ui, mode, 0..=15));
            c |= fm(ui, "Most at once", pet_max);
            c |= fm(ui, "Monster level", level);
        }
        PartEntry::TimedBuff {
            state,
            frames,
            stats,
            events,
        } => {
            c |= pick_field(ui, "State", state, &names.states);
            c |= fm(ui, "Base frames", frames);
            let mut remove = None;
            for (i, s) in stats.iter_mut().enumerate() {
                ui.push_id(("buff-stat", i), |ui| {
                    c |= pick_field(ui, "Stat", &mut s.stat, &names.stats);
                    c |= fm(ui, "Value", &mut s.value);
                    if ui.small_button("Remove stat").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                stats.remove(i);
                c = true;
            }
            if stats.len() < 11 && ui.small_button("Add stat").clicked() {
                stats.push(AuraStatEntry {
                    stat: 0,
                    value: Fm::new("1"),
                });
                c = true;
            }
            let mut remove = None;
            for (i, e) in events.iter_mut().enumerate() {
                ui.push_id(("buff-event", i), |ui| {
                    c |= labelled(ui, "Event", |ui| drag(ui, &mut e.event, 0..=255));
                    c |= labelled(ui, "Callback", |ui| drag(ui, &mut e.callback, 0..=49));
                    if ui.small_button("Remove event").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                events.remove(i);
                c = true;
            }
            if events.len() < 3 && ui.small_button("Add event").clicked() {
                events.push(crate::kit::BuffEventEntry {
                    event: 0,
                    callback: 1,
                });
                c = true;
            }
        }
        PartEntry::CallScript {
            module,
            function,
            argument,
        } => {
            c |= labelled(ui, "Module (.luau / .graph.toml)", |ui| {
                ui.text_edit_singleline(module).changed()
            });
            c |= labelled(ui, "Function / graph call name", |ui| {
                ui.text_edit_singleline(function).changed()
            });
            c |= opt_fm(ui, "Argument", argument, "0");
        }
        PartEntry::Missile {
            missile,
            lob,
            at_aim,
            channel,
            ..
        } => {
            c |= pick_field(ui, "Missile row", missile, &names.missiles);
            c |= ui.checkbox(lob, "Lobbed").changed();
            c |= ui
                .checkbox(at_aim, "Drop it on the aim point (as Meteor does)")
                .changed();
            c |= ui
                .checkbox(channel, "Length set the way a channel sets it")
                .changed();
        }
        PartEntry::Resurrect { overlay } | PartEntry::Nest { overlay } => {
            c |= labelled(ui, "Overlay row", |ui| {
                let mut some = overlay.is_some();
                let mut c = ui.checkbox(&mut some, "play one").changed();
                if c {
                    *overlay = some.then_some(0);
                }
                if let Some(o) = overlay {
                    c |= drag(ui, o, 0..=u16::MAX);
                }
                c
            });
        }
        PartEntry::Wall { missile, centre } => {
            c |= pick_field(ui, "Wall missile row", missile, &names.missiles);
            c |= labelled(ui, "Missile at the middle", |ui| {
                let mut some = centre.is_some();
                let mut c = ui.checkbox(&mut some, "one").changed();
                if c {
                    *centre = some.then_some(*missile);
                }
                if let Some(m) = centre {
                    c |= pick(ui, "centre", m, &names.missiles);
                }
                c
            });
        }
        PartEntry::Heal { min, max } => {
            c |= fm(ui, "Least % of life", min);
            c |= fm(ui, "Most % of life", max);
        }
        PartEntry::Aura {
            state,
            target_state,
            range,
            seconds,
            affects,
            stats,
        } => {
            c |= pick_field(ui, "State", state, &names.states);
            c |= labelled(ui, "State for the units around", |ui| {
                let mut other = target_state.is_some();
                let mut c = ui.checkbox(&mut other, "another one").changed();
                if c {
                    *target_state = other.then_some(*state);
                }
                if let Some(t) = target_state {
                    c |= pick(ui, "target_state", t, &names.states);
                }
                c
            });
            c |= aura_fields(ui, curse, range, seconds, affects, stats, names);
        }
        PartEntry::Curse {
            state,
            range,
            seconds,
            affects,
            stats,
        } => {
            c |= pick_field(ui, "State", state, &names.states);
            c |= aura_fields(ui, curse, range, seconds, affects, stats, names);
        }
        PartEntry::Summon {
            class,
            pet_type,
            pet_max,
            count,
            level,
        } => {
            c |= pick_field(ui, "Monster class", class, &names.monsters);
            c |= labelled(ui, "Pet type row", |ui| drag(ui, pet_type, 0..=u16::MAX));
            c |= opt_fm(ui, "Most at once", pet_max, "1");
            c |= opt_fm(ui, "Made per cast", count, "1");
            c |= opt_fm(ui, "Pet level", level, "0 (follows you)");
        }
        PartEntry::State {
            state,
            op,
            target,
            radius,
            affects,
            seconds,
            count,
        } => {
            c |= pick_field(ui, "State", state, &names.states);
            c |= choice(ui, "Does", op, STATE_OPS, "apply");
            c |= choice(ui, "On", target, STATE_TARGETS, "caster");
            if target.as_deref() == Some("area") {
                c |= opt_fm(ui, "Radius", radius, "required for an area");
                c |= choice(ui, "Affects", affects, AFFECTS, "enemies");
            }
            c |= opt_fm(ui, "Seconds", seconds, "the state's duration");
            c |= opt_fm(ui, "Stacks", count, "1");
        }
        PartEntry::MeleeHit {
            weapon_percent,
            attack_rating,
            damage,
            enhance,
            own_element,
            sequence,
            life_leech,
            mana_leech,
            stun_frames,
            monster_mode,
            overlay,
            target_state,
            target_frames,
            target_stats,
            result_flags,
            hit_flags,
            hit_class,
            ..
        } => {
            c |= labelled(ui, "Weapon damage %", |ui| {
                drag(ui, weapon_percent, 0..=400)
            });
            c |= opt_fm(ui, "Attack rating added", attack_rating, "0");
            c |= opt_fm(ui, "Damage % added on a hit", enhance, "0");
            c |= ui
                .checkbox(own_element, "Add the skill row's own elemental damage")
                .changed();
            c |= opt_fm(ui, "Hits in a row", sequence, "0");
            c |= opt_fm(ui, "Life leech on a hit", life_leech, "0");
            c |= opt_fm(ui, "Mana leech on a hit", mana_leech, "0");
            c |= opt_fm(ui, "Stun frames", stun_frames, "0");
            let mut flash = overlay.is_some();
            if ui
                .checkbox(&mut flash, "Overlay on the target on a hit")
                .changed()
            {
                *overlay = flash.then_some(0);
                c = true;
            }
            if let Some(overlay) = overlay {
                c |= labelled(ui, "Overlay row", |ui| drag(ui, overlay, 0..=u16::MAX));
            }
            let mut marks = target_state.is_some();
            if ui
                .checkbox(&mut marks, "State on the target on a hit")
                .changed()
            {
                *target_state = marks.then_some(0);
                c = true;
            }
            if let Some(state) = target_state {
                c |= pick_field(ui, "Target state", state, &names.states);
                c |= opt_fm(ui, "Target state frames", target_frames, "1");
                let mut remove = None;
                for (i, stat) in target_stats.iter_mut().enumerate() {
                    ui.push_id(("melee-target-stat", i), |ui| {
                        c |= pick_field(ui, "Stat", &mut stat.stat, &names.stats);
                        c |= fm(ui, "Value", &mut stat.value);
                        if ui.small_button("Remove stat").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    target_stats.remove(i);
                    c = true;
                }
                if target_stats.len() < 6 && ui.small_button("Add stat").clicked() {
                    target_stats.push(AuraStatEntry {
                        stat: 0,
                        value: Fm::new("1"),
                    });
                    c = true;
                }
            }
            c |= labelled(ui, "Result flags", |ui| {
                drag(ui, result_flags, 0..=u16::MAX)
            });
            c |= labelled(ui, "Hit flags", |ui| drag(ui, hit_flags, 0..=u32::MAX));
            c |= labelled(ui, "Hit class", |ui| drag(ui, hit_class, 0..=u8::MAX));
            c |= choice(
                ui,
                "Monster attack mode",
                monster_mode,
                &[
                    ("a1", "First attack (a1)"),
                    ("a2", "Second attack (a2)"),
                    ("s1", "First skill (s1)"),
                ],
                "a1",
            );
            let mut extra = damage.is_some();
            if ui.checkbox(&mut extra, "Extra skill damage").changed() {
                *damage =
                    extra.then(|| DamageEntry::new("physical", "5 + lvl * 3", "10 + lvl * 4"));
                c = true;
            }
            if let Some(d) = damage {
                c |= damage_form(ui, d, names);
            }
        }
        PartEntry::AreaHit {
            radius,
            around,
            affects,
            damage,
            ..
        } => {
            c |= fm(ui, "Radius", radius);
            c |= choice(ui, "Around", around, AROUND, "target");
            c |= choice(ui, "Affects", affects, AFFECTS, "enemies");
            c |= damage_form(ui, damage, names);
        }
        PartEntry::MissileRing {
            missile,
            count,
            velocity,
            around,
        } => {
            c |= pick_field(ui, "Missile row", missile, &names.missiles);
            c |= opt_fm(ui, "Missiles (1 to 64)", count, "16");
            c |= opt_fm(ui, "Velocity", velocity, "the missile row's");
            c |= choice(
                ui,
                "Around",
                around,
                &[("caster", "You"), ("target", "The aim point")],
                "caster",
            );
        }
        PartEntry::MissileArc {
            missile,
            count,
            span,
            velocity,
        } => {
            c |= pick_field(ui, "Missile row", missile, &names.missiles);
            c |= opt_fm(ui, "Missiles (1 to 64)", count, "5");
            c |= opt_fm(ui, "Width (64ths of a circle)", span, "16");
            c |= opt_fm(ui, "Velocity", velocity, "the missile row's");
        }
        PartEntry::MissileVolley { missile, count } => {
            c |= pick_field(ui, "Missile row", missile, &names.missiles);
            c |= opt_fm(ui, "Missiles (1 to 64)", count, "3");
        }
        PartEntry::Chain {
            radius,
            jumps,
            falloff,
            affects,
            damage,
            ..
        }
        | PartEntry::Bounce {
            radius,
            jumps,
            falloff,
            affects,
            damage,
            ..
        } => {
            c |= fm(ui, "Jump reach", radius);
            c |= fm(ui, "Jumps (0 to 16)", jumps);
            c |= opt_fm(ui, "Damage lost per jump %", falloff, "0");
            c |= choice(ui, "Affects", affects, AFFECTS, "enemies");
            c |= damage_form(ui, damage, names);
        }
        PartEntry::Teleport { max_range } => {
            c |= opt_fm(ui, "Farthest it goes", max_range, "no limit");
        }
        PartEntry::Raise { pet_type, pet_max } => {
            c |= labelled(ui, "Pet type row", |ui| drag(ui, pet_type, 0..=u16::MAX));
            c |= opt_fm(ui, "Most at once", pet_max, "1");
        }
        PartEntry::MinionOrder {
            order,
            radius,
            skill,
            level,
        } => {
            let mut chosen = Some(order.clone());
            if choice(ui, "Order", &mut chosen, MINION_ORDERS, "recall") {
                *order = chosen.unwrap_or_else(|| "recall".into());
                c = true;
            }
            c |= fm(ui, "Minions within", radius);
            if order == "attack" {
                let id = skill.get_or_insert(0);
                c |= pick_field(ui, "Skill they cast", id, &names.skills);
                c |= opt_fm(ui, "Skill level", level, "this skill's level");
            }
        }
        PartEntry::EngineBody { slot, skill, level } => {
            c |= labelled(ui, "Function number (1 to 190)", |ui| {
                drag(ui, slot, 1..=190)
            });
            let mut other = skill.is_some();
            if ui
                .checkbox(&mut other, "Read another skill's numbers")
                .changed()
            {
                *skill = other.then_some(0);
                c = true;
            }
            if let Some(id) = skill {
                c |= pick_field(ui, "Skills row", id, &names.skills);
            }
            c |= opt_fm(ui, "Level", level, "this skill's level");
        }
        PartEntry::Repeat {
            skill,
            every,
            times,
            now,
            while_state,
            stop_on_move,
        } => {
            let mut other = skill.is_some();
            if ui
                .checkbox(&mut other, "Another skill's parts run each time")
                .changed()
            {
                *skill = other.then_some(0);
                c = true;
            }
            match skill {
                Some(id) => {
                    c |= pick_field(ui, "Skill whose parts run each time", id, &names.skills)
                }
                None => {
                    ui.label("The parts after this one run each time, and not at the cast.");
                }
            }
            c |= opt_fm(ui, "Ticks between runs (25 = 1 s)", every, "25");
            c |= opt_fm(ui, "Runs in all (1 to 600)", times, "5");
            c |= ui.checkbox(now, "First run at once").changed();
            c |= ui.checkbox(stop_on_move, "Stops when you move").changed();
            let mut held = while_state.is_some();
            if ui.checkbox(&mut held, "Stops when a state ends").changed() {
                *while_state = held.then_some(0);
                c = true;
            }
            if let Some(state) = while_state {
                c |= pick_field(ui, "State", state, &names.states);
            }
        }
        PartEntry::Rush {
            speed,
            steps,
            stop_in_reach,
            each,
            land,
        } => {
            c |= opt_fm(ui, "Speed (% of running speed)", speed, "100");
            c |= opt_fm(ui, "Most steps (one a tick, to 125)", steps, "25");
            c |= ui
                .checkbox(stop_in_reach, "Stops when the target is in melee reach")
                .changed();
            for (label, slot, none) in [
                ("Another skill's parts run after each step", each, ""),
                (
                    "Another skill's parts run where it stops",
                    land,
                    "The parts after this one run where it stops, and not at the cast.",
                ),
            ] {
                let mut other = slot.is_some();
                if ui.checkbox(&mut other, label).changed() {
                    *slot = other.then_some(0);
                    c = true;
                }
                match slot {
                    Some(id) => c |= pick_field(ui, "Skills row", id, &names.skills),
                    None if !none.is_empty() => {
                        ui.label(none);
                    }
                    None => {}
                }
            }
        }
        PartEntry::Leap {
            frames,
            max_range,
            land,
        } => {
            c |= opt_fm(ui, "Ticks in the air", frames, "10");
            c |= opt_fm(ui, "Farthest it goes", max_range, "no limit");
            let mut lands = land.is_some();
            if ui
                .checkbox(&mut lands, "Another skill's parts run on landing")
                .changed()
            {
                *land = lands.then_some(0);
                c = true;
            }
            if let Some(id) = land {
                c |= pick_field(ui, "Skills row", id, &names.skills);
            }
        }
    }
    if let Some(controls) = p.controls_mut() {
        c |= control_form(ui, controls);
    }
    c
}

fn control_form(ui: &mut egui::Ui, effects: &mut Vec<ControlEntry>) -> bool {
    let mut changed = false;
    ui.strong("Declared crowd control");
    let mut remove = None;
    for (i, effect) in effects.iter_mut().enumerate() {
        ui.push_id(("control", i), |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Effect {}", i + 1));
                egui::ComboBox::from_id_salt("kind")
                    .selected_text(&effect.kind)
                    .show_ui(ui, |ui| {
                        for kind in ["interrupt", "knockback", "stun", "freeze"] {
                            if ui.selectable_label(effect.kind == kind, kind).clicked() {
                                effect.kind = kind.into();
                                effect.seconds = matches!(kind, "stun" | "freeze").then_some(1.0);
                                changed = true;
                            }
                        }
                    });
                if ui.small_button("Remove control").clicked() {
                    remove = Some(i);
                }
            });
            if let Some(seconds) = &mut effect.seconds {
                changed |= labelled(ui, "Control seconds", |ui| {
                    ui.add(egui::DragValue::new(seconds).range(0.04..=10.0).speed(0.04))
                        .changed()
                });
            }
            let mut chance = effect.chance.unwrap_or(100);
            if labelled(ui, "Control chance %", |ui| drag(ui, &mut chance, 1..=100)) {
                effect.chance = (chance != 100).then_some(chance);
                changed = true;
            }
        });
    }
    if let Some(i) = remove {
        effects.remove(i);
        changed = true;
    }
    if ui
        .add_enabled(effects.len() < 8, egui::Button::new("Add crowd control"))
        .clicked()
    {
        effects.push(ControlEntry {
            kind: "interrupt".into(),
            seconds: None,
            chance: None,
        });
        changed = true;
    }
    changed
}

#[allow(clippy::too_many_arguments)]
fn skill_form(
    ui: &mut egui::Ui,
    kit: &mut Kit,
    names: &Names,
    i: usize,
    tables: &mut Option<&mut Tables>,
    chart: &mut BTreeSet<String>,
    chart_for: &mut Option<(u16, usize)>,
    local_params: &mut [i32; 8],
) -> bool {
    let Some(mut s) = kit.skills.get(i).cloned() else {
        ui.label("Add a skill on the left, or start from a template.");
        return false;
    };
    let before = s.clone();
    ui.heading(if s.name.is_empty() {
        format!("Skill {}", s.id)
    } else {
        s.name.clone()
    });
    labelled(ui, "Name", |ui| {
        ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(260.0))
            .changed()
    });
    labelled(ui, "Skills table row", |ui| {
        drag(ui, &mut s.id, 0..=u16::MAX)
    });
    if let Some(t) = tables.as_deref_mut() {
        if !row_exists(t, "skills", s.id) {
            if table_note(
                ui,
                "No row with this id in the skills table.",
                Some("Add the row"),
            ) {
                if let Ok(id) = add_row(t, "skills", s.name.trim()) {
                    s.id = id;
                    set_cell(
                        t,
                        "skills",
                        usize::from(id),
                        "srvdofunc",
                        &BLOCK_SKILL_SLOT.to_string(),
                    );
                }
            }
        } else if cell(t, "skills", usize::from(s.id), "srvdofunc")
            .as_deref()
            .map(str::trim)
            != Some(&BLOCK_SKILL_SLOT.to_string())
            && table_note(
                ui,
                "The row's srvdofunc is not the block slot, so the parts never run.",
                Some("Set srvdofunc to 400"),
            )
        {
            set_cell(
                t,
                "skills",
                usize::from(s.id),
                "srvdofunc",
                &BLOCK_SKILL_SLOT.to_string(),
            );
        }
    }
    if let Some(t) = tables.as_deref() {
        if let Some(lines) = lift::diff_against_base(kit, t, s.id) {
            egui::CollapsingHeader::new(if lift::differs(&lines) {
                "Differences from the base skill"
            } else {
                "Same as the base skill"
            })
            .id_salt("base-diff")
            .show(ui, |ui| {
                for (change, text) in &lines {
                    let (mark, color) = match change {
                        lift::Change::Same => (" ", Color32::GRAY),
                        lift::Change::Removed => ("-", Color32::from_rgb(230, 110, 110)),
                        lift::Change::Added => ("+", Color32::from_rgb(110, 200, 130)),
                    };
                    ui.label(
                        RichText::new(format!("{mark} {text}"))
                            .monospace()
                            .color(color),
                    );
                }
            });
        }
    }
    ui.separator();
    ui.strong(format!("Parts (run in order, {} of {})", s.parts.len(), 16));
    let len = s.parts.len();
    let mut swap = None;
    let mut remove = None;
    for n in 0..len {
        let summary = kit.part_summary(names, &s.parts[n]);
        let title = format!("{}. {} — {summary}", n + 1, s.parts[n].label());
        ui.push_id(("part", n), |ui| {
            egui::CollapsingHeader::new(title)
                .id_salt(("part-header", n))
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (to, rm) = move_buttons(ui, n, len);
                        if let Some(to) = to {
                            swap = Some((n, to));
                        }
                        if rm {
                            remove = Some(n);
                        }
                    });
                    part_form(ui, &mut s.parts[n], names);
                });
        });
    }
    if let Some((a, b)) = swap {
        s.parts.swap(a, b);
    }
    if let Some(n) = remove {
        s.parts.remove(n);
    }
    ui.horizontal(|ui| {
        let id = ui.id().with("new-part");
        let mut kind: String =
            ui.data_mut(|d| d.get_temp(id).unwrap_or_else(|| "missile".to_owned()));
        egui::ComboBox::from_id_salt("add-part")
            .selected_text(
                PART_KINDS
                    .iter()
                    .find(|(k, _)| *k == kind)
                    .map_or("", |(_, l)| l),
            )
            .show_ui(ui, |ui| {
                for (k, l) in PART_KINDS {
                    ui.selectable_value(&mut kind, (*k).to_owned(), *l)
                        .on_hover_text(block_help(Family::SkillPart, k));
                }
            });
        ui.data_mut(|d| d.insert_temp(id, kind.clone()));
        if ui
            .add_enabled(s.parts.len() < 16, egui::Button::new("Add part"))
            .clicked()
        {
            if let Some(p) = PartEntry::new(&kind) {
                s.parts.push(p);
            }
        }
    });
    ui.separator();
    curve_section(ui, &s, tables, chart, chart_for, local_params, names);
    let changed = s != before;
    if changed {
        kit.skills[i] = s;
    }
    changed
}

const SERIES: [Color32; 6] = [
    Color32::from_rgb(0x39, 0x87, 0xe5),
    Color32::from_rgb(0xd9, 0x59, 0x26),
    Color32::from_rgb(0x19, 0x9e, 0x70),
    Color32::from_rgb(0xc9, 0x85, 0x00),
    Color32::from_rgb(0xd5, 0x51, 0x81),
    Color32::from_rgb(0x90, 0x85, 0xe9),
];

fn curve_section(
    ui: &mut egui::Ui,
    s: &SkillEntry,
    tables: &mut Option<&mut Tables>,
    chart: &mut BTreeSet<String>,
    chart_for: &mut Option<(u16, usize)>,
    local_params: &mut [i32; 8],
    _names: &Names,
) {
    ui.strong("Numbers by skill level (1–30)");
    let mut formulas: Vec<(String, Fm)> = Vec::new();
    for (n, p) in s.parts.iter().enumerate() {
        for (label, f) in p.formulas() {
            formulas.push((format!("{}. {} · {label}", n + 1, p.label()), f.clone()));
        }
    }
    // Pick the first damage pair when this skill (or its part count) changes.
    if *chart_for != Some((s.id, s.parts.len())) {
        *chart_for = Some((s.id, s.parts.len()));
        chart.clear();
        let first = formulas
            .iter()
            .find(|(l, _)| l.ends_with("damage min"))
            .map(|(l, _)| l.clone());
        if let Some(min) = first {
            chart.insert(min.replace("damage min", "damage max"));
            chart.insert(min);
        } else if let Some((l, _)) = formulas.first() {
            chart.insert(l.clone());
        }
    }
    let table_params = tables.as_deref().and_then(|t| params_of(t, s.id));
    egui::CollapsingHeader::new(match table_params {
        Some(_) => "Skill row Param1–Param8 (from the skills table)",
        None => "Param1–Param8 (no skills row loaded; used only for this chart)",
    })
    .default_open(false)
    .show(ui, |ui| {
        let mut p = table_params.unwrap_or(*local_params);
        ui.horizontal_wrapped(|ui| {
            for (k, v) in p.iter_mut().enumerate() {
                ui.label(format!("par{}", k + 1));
                if drag(ui, v, i32::MIN..=i32::MAX) {
                    match tables.as_deref_mut().filter(|_| table_params.is_some()) {
                        Some(t) => {
                            set_cell(
                                t,
                                "skills",
                                usize::from(s.id),
                                &format!("param{}", k + 1),
                                &v.to_string(),
                            );
                        }
                        None => local_params[k] = *v,
                    }
                }
            }
        });
    });
    let params = table_params.unwrap_or(*local_params);
    ui.horizontal_wrapped(|ui| {
        for (k, (label, _)) in formulas.iter().enumerate() {
            let mut on = chart.contains(label);
            ui.colored_label(SERIES[k % SERIES.len()], "■");
            if ui.checkbox(&mut on, label).changed() {
                if on {
                    chart.insert(label.clone());
                } else {
                    chart.remove(label);
                }
            }
        }
    });
    let series: Vec<Series> = formulas
        .iter()
        .enumerate()
        .filter(|(_, (l, _))| chart.contains(l))
        .map(|(k, (l, f))| {
            (
                l.clone(),
                SERIES[k % SERIES.len()],
                f.curve(&params, CURVE_LEVELS),
            )
        })
        .collect();
    draw_chart(ui, &series);
    ui.weak("Values come from the engine's own formula evaluation. rand() and stat reads need a game, so the chart shows them as the engine does without a unit.");
}

/// A charted formula: label, colour, (level, value) points.
pub type Series = (String, Color32, Vec<(i32, i32)>);

/// A plain line chart: x is the skill level, y the formula's value.
pub fn draw_chart(ui: &mut egui::Ui, series: &[Series]) {
    let size = egui::vec2(ui.available_width().clamp(360.0, 640.0), 220.0);
    let (response, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = response.rect;
    let plot = egui::Rect::from_min_max(
        rect.min + egui::vec2(46.0, 10.0),
        rect.max - egui::vec2(12.0, 24.0),
    );
    let ink = Color32::from_rgb(0xc3, 0xc2, 0xb7);
    let grid = Color32::from_rgb(0x38, 0x38, 0x35);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(0x1a, 0x1a, 0x19));
    let lo = *CURVE_LEVELS.start();
    let hi = *CURVE_LEVELS.end();
    let values = series
        .iter()
        .flat_map(|(_, _, p)| p.iter().map(|(_, v)| *v));
    let (mut ymin, mut ymax) = values.fold((0, 1), |(a, b), v| (a.min(v), b.max(v)));
    let step = nice_step(f64::from(ymax - ymin) / 4.0);
    ymax = ((f64::from(ymax) / step).ceil() * step) as i32;
    ymin = ((f64::from(ymin) / step).floor() * step) as i32;
    let x = |lvl: i32| plot.left() + plot.width() * (lvl - lo) as f32 / (hi - lo) as f32;
    let y =
        |v: i32| plot.bottom() - plot.height() * (v - ymin) as f32 / (ymax - ymin).max(1) as f32;
    let mut tick = ymin;
    while tick <= ymax {
        painter.line_segment(
            [
                egui::pos2(plot.left(), y(tick)),
                egui::pos2(plot.right(), y(tick)),
            ],
            Stroke::new(1.0_f32, grid),
        );
        painter.text(
            egui::pos2(plot.left() - 6.0, y(tick)),
            Align2::RIGHT_CENTER,
            tick.to_string(),
            FontId::proportional(11.0),
            ink,
        );
        tick += step as i32;
    }
    for lvl in [1, 5, 10, 15, 20, 25, 30] {
        painter.text(
            egui::pos2(x(lvl), plot.bottom() + 4.0),
            Align2::CENTER_TOP,
            lvl.to_string(),
            FontId::proportional(11.0),
            ink,
        );
    }
    painter.text(
        egui::pos2(plot.right(), rect.bottom() - 1.0),
        Align2::RIGHT_BOTTOM,
        "skill level",
        FontId::proportional(10.0),
        ink,
    );
    for (_, color, points) in series {
        let line: Vec<egui::Pos2> = points
            .iter()
            .map(|(l, v)| egui::pos2(x(*l), y(*v)))
            .collect();
        painter.add(egui::Shape::line(line, Stroke::new(2.0_f32, *color)));
    }
    if series.is_empty() {
        painter.text(
            plot.center(),
            Align2::CENTER_CENTER,
            "Tick a formula above to chart it",
            FontId::proportional(13.0),
            ink,
        );
    }
    if let Some(pos) = response.hover_pos().filter(|p| plot.contains(*p)) {
        let level =
            (lo as f32 + (pos.x - plot.left()) / plot.width() * (hi - lo) as f32).round() as i32;
        let level = level.clamp(lo, hi);
        painter.line_segment(
            [
                egui::pos2(x(level), plot.top()),
                egui::pos2(x(level), plot.bottom()),
            ],
            Stroke::new(1.0_f32, ink),
        );
        for (_, color, points) in series {
            if let Some((_, v)) = points.iter().find(|(l, _)| *l == level) {
                painter.circle(
                    egui::pos2(x(level), y(*v)),
                    4.0,
                    *color,
                    Stroke::new(2.0_f32, Color32::from_rgb(0x1a, 0x1a, 0x19)),
                );
            }
        }
        response.on_hover_ui_at_pointer(|ui| {
            ui.strong(format!("Level {level}"));
            for (label, color, points) in series {
                if let Some((_, v)) = points.iter().find(|(l, _)| *l == level) {
                    ui.horizontal(|ui| {
                        ui.colored_label(*color, "■");
                        ui.label(format!("{label}: {v}"));
                    });
                }
            }
        });
    }
}

fn nice_step(raw: f64) -> f64 {
    let raw = raw.max(1.0);
    let mag = 10f64.powf(raw.log10().floor());
    for m in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if raw <= m * mag {
            return (m * mag).max(1.0);
        }
    }
    10.0 * mag
}

// ---------------------------------------------------------- trigger form

fn trigger_form(ui: &mut egui::Ui, kit: &mut Kit, names: &Names, i: usize) -> bool {
    let Some(mut t) = kit.triggers.get(i).cloned() else {
        ui.label("Add a trigger on the left, or start from a template.");
        return false;
    };
    let before = t.clone();
    ui.heading(kit.trigger_sentence(names, &t));
    ui.weak(kit.trigger_source(names, &t));
    labelled(ui, "Name", |ui| {
        ui.add(egui::TextEdit::singleline(&mut t.name).desired_width(260.0))
            .changed()
    });
    ui.separator();
    ui.strong("Where it lives");
    labelled(ui, "While", |ui| {
        let mut c = false;
        let now = if t.while_skill.is_some() {
            0
        } else if t.while_stat.is_some() {
            1
        } else {
            2
        };
        let mut pick_kind = now;
        egui::ComboBox::from_id_salt("source")
            .width(230.0)
            .selected_text(
                [
                    "you have a skill",
                    "you have a stat above 0",
                    "you are in a state",
                ][now],
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut pick_kind, 0, "you have a skill");
                ui.selectable_value(
                    &mut pick_kind,
                    1,
                    "you have a stat above 0 (an item property gives it)",
                );
                ui.selectable_value(&mut pick_kind, 2, "you are in a state");
            });
        if pick_kind != now {
            (t.while_skill, t.while_stat, t.while_state) = match pick_kind {
                0 => (Some(0), None, None),
                1 => (None, Some(0), None),
                _ => (None, None, Some(0)),
            };
            c = true;
        }
        c
    });
    if let Some(v) = &mut t.while_skill {
        pick_field(ui, "Skill", v, &names.skills);
    }
    if let Some(v) = &mut t.while_stat {
        pick_field(ui, "Stat", v, &names.stats);
    }
    if let Some(v) = &mut t.while_state {
        pick_field(ui, "State", v, &names.states);
    }
    ui.strong("When");
    let events: Vec<(&str, &str)> = EVENTS.to_vec();
    let on_before = t.on.clone();
    required_choice(ui, "Event", &mut t.on, &events);
    if t.on != on_before {
        t.skills.clear();
        if !matches!(t.on.as_str(), "attack" | "hit" | "kill") {
            t.by_skill = None;
        }
        t.state = matches!(t.on.as_str(), "state_start" | "state_end").then_some(0);
    }
    if let Some(s) = &mut t.state {
        pick_field(ui, "State", s, &names.states);
    }
    if t.on == "skill_used" {
        ui.label("Only these skills (none = any)");
        let mut remove = None;
        for (n, s) in t.skills.iter_mut().enumerate() {
            ui.push_id(n, |ui| {
                ui.horizontal(|ui| {
                    pick(ui, "skill", s, &names.skills);
                    if ui.small_button("Remove").clicked() {
                        remove = Some(n);
                    }
                });
            });
        }
        if let Some(n) = remove {
            t.skills.remove(n);
        }
        if ui.small_button("Add skill").clicked() {
            t.skills.push(0);
        }
    }
    if matches!(t.on.as_str(), "attack" | "hit" | "kill") {
        let mut only = t.by_skill.is_some();
        if ui
            .checkbox(&mut only, "Only while using one skill")
            .changed()
        {
            t.by_skill = only.then_some(0);
        }
        if let Some(s) = &mut t.by_skill {
            pick_field(ui, "Skill", s, &names.skills);
        }
    }
    opt_fm(ui, "Chance (%)", &mut t.chance, "100");
    ui.separator();
    ui.strong("Only if (all must hold)");
    let mut remove = None;
    for (n, c) in t.conditions.iter_mut().enumerate() {
        ui.push_id(("cond", n), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{}.", n + 1)).weak());
                let who = c.who().cloned();
                let mut who_other = who.as_deref() == Some("other");
                egui::ComboBox::from_id_salt("who")
                    .width(120.0)
                    .selected_text(if who_other { "the other unit" } else { "you" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut who_other, false, "you");
                        ui.selectable_value(&mut who_other, true, "the other unit");
                    });
                *c.who_mut() = who_other.then(|| "other".to_owned());
                ui.label(
                    CONDITION_KINDS
                        .iter()
                        .find(|(k, _)| *k == c.kind())
                        .map_or("", |(_, l)| l),
                );
                match c {
                    ConditionEntry::HasState { state, .. }
                    | ConditionEntry::LacksState { state, .. } => {
                        pick(ui, "state", state, &names.states);
                    }
                    ConditionEntry::StacksAtLeast { state, count, .. } => {
                        pick(ui, "state", state, &names.states);
                        drag(ui, count, 1..=255);
                    }
                    ConditionEntry::LifeBelow { percent, .. }
                    | ConditionEntry::LifeAbove { percent, .. } => {
                        drag(ui, percent, 0..=100);
                    }
                }
                if ui.small_button("Remove").clicked() {
                    remove = Some(n);
                }
            });
        });
    }
    if let Some(n) = remove {
        t.conditions.remove(n);
    }
    ui.horizontal(|ui| {
        let id = ui.id().with("new-cond");
        let mut kind: String =
            ui.data_mut(|d| d.get_temp(id).unwrap_or_else(|| "has_state".to_owned()));
        egui::ComboBox::from_id_salt("add-cond")
            .selected_text(
                CONDITION_KINDS
                    .iter()
                    .find(|(k, _)| *k == kind)
                    .map_or("", |(_, l)| l),
            )
            .show_ui(ui, |ui| {
                for (k, l) in CONDITION_KINDS {
                    ui.selectable_value(&mut kind, (*k).to_owned(), *l)
                        .on_hover_text(block_help(Family::Condition, k));
                }
            });
        ui.data_mut(|d| d.insert_temp(id, kind.clone()));
        if ui
            .add_enabled(t.conditions.len() < 8, egui::Button::new("Add condition"))
            .clicked()
        {
            t.conditions.extend(ConditionEntry::new(&kind));
        }
    });
    ui.separator();
    ui.strong("Then do (in order)");
    let len = t.actions.len();
    let mut swap = None;
    let mut remove = None;
    for (n, a) in t.actions.iter_mut().enumerate() {
        ui.push_id(("act", n), |ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{}. {}",
                            n + 1,
                            ACTION_KINDS
                                .iter()
                                .find(|(k, _)| *k == a.kind())
                                .map_or("", |(_, l)| l)
                        ))
                        .strong(),
                    );
                    let (to, rm) = move_buttons(ui, n, len);
                    if let Some(to) = to {
                        swap = Some((n, to));
                    }
                    if rm {
                        remove = Some(n);
                    }
                });
                action_form(ui, a, names);
            });
        });
    }
    if let Some((a, b)) = swap {
        t.actions.swap(a, b);
    }
    if let Some(n) = remove {
        t.actions.remove(n);
    }
    ui.horizontal(|ui| {
        let id = ui.id().with("new-act");
        let mut kind: String =
            ui.data_mut(|d| d.get_temp(id).unwrap_or_else(|| "apply_state".to_owned()));
        egui::ComboBox::from_id_salt("add-act")
            .selected_text(
                ACTION_KINDS
                    .iter()
                    .find(|(k, _)| *k == kind)
                    .map_or("", |(_, l)| l),
            )
            .show_ui(ui, |ui| {
                for (k, l) in ACTION_KINDS {
                    ui.selectable_value(&mut kind, (*k).to_owned(), *l)
                        .on_hover_text(block_help(Family::Action, k));
                }
            });
        ui.data_mut(|d| d.insert_temp(id, kind.clone()));
        if ui
            .add_enabled(t.actions.len() < 8, egui::Button::new("Add action"))
            .clicked()
        {
            t.actions.extend(ActionEntry::new(&kind));
        }
    });
    let changed = t != before;
    if changed {
        kit.triggers[i] = t;
    }
    changed
}

fn action_form(ui: &mut egui::Ui, a: &mut ActionEntry, names: &Names) {
    match a {
        ActionEntry::CallScript {
            module,
            function,
            argument,
        } => {
            labelled(ui, "Module (.luau / .graph.toml)", |ui| {
                ui.text_edit_singleline(module).changed()
            });
            labelled(ui, "Function / graph call name", |ui| {
                ui.text_edit_singleline(function).changed()
            });
            opt_fm(ui, "Argument", argument, "0");
        }
        ActionEntry::ApplyState {
            state,
            seconds,
            who,
        } => {
            pick_field(ui, "State", state, &names.states);
            opt_fm(ui, "Seconds", seconds, "the state's duration");
            choice(ui, "On", who, WHO, "self");
        }
        ActionEntry::AddStack {
            state,
            count,
            seconds,
            who,
        } => {
            pick_field(ui, "State", state, &names.states);
            opt_fm(ui, "Stacks", count, "1");
            opt_fm(ui, "Seconds", seconds, "the state's duration");
            choice(ui, "On", who, WHO, "self");
        }
        ActionEntry::RemoveStack { state, count, who } => {
            pick_field(ui, "State", state, &names.states);
            opt_fm(ui, "Stacks", count, "1");
            choice(ui, "On", who, WHO, "self");
        }
        ActionEntry::RemoveState { state, who } => {
            pick_field(ui, "State", state, &names.states);
            choice(ui, "On", who, WHO, "self");
        }
        ActionEntry::CastSkill { skill, level, at } => {
            pick_field(ui, "Skill", skill, &names.skills);
            opt_fm(ui, "Level", level, "lvl");
            choice(ui, "At", at, WHO, "other");
        }
    }
}
