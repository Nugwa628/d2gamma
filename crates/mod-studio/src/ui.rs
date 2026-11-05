//! Studio's start page, project forms, file browser and external tool output.
use crate::actions::{Job, Jump, Report};
use crate::actions_ui::Action;
use crate::*;
use eframe::egui::{self, Color32, RichText};
use std::sync::mpsc::Receiver;

/// The tab of the monster AI editor.
pub const AI_TAB: &str = "Monster AI";

pub struct Studio {
    pub settings: Settings,
    pub tables: crate::table_ui::TableEditor,
    pub workbench: crate::workbench_ui::Workbench,
    pub kit: crate::kit_ui::KitEditor,
    pub maps: crate::map_ui::MapEditor,
    pub ai: crate::ai_ui::AiEditor,
    pub net: crate::net_ui::NetEditor,
    pub stats: crate::stats_ui::StatsEditor,
    pub acts: crate::acts_ui::ActsEditor,
    pub difficulties: crate::difficulty_ui::DifficultyEditor,
    pub effects: crate::effects_ui::EffectsEditor,
    pub importer: crate::import_ui::Importer,
    pub layout: crate::layout_ui::LayoutEditor,
    pub scripts: crate::scripts_ui::ScriptEditor,
    pub graphs: crate::graphs_ui::GraphEditor,
    pub(crate) script_check_pending: bool,
    pub project: Option<Project>,
    wizard: bool,
    draft: Manifest,
    destination: String,
    open_path: String,
    template: usize,
    base: usize,
    pub(crate) tab: String,
    pub(crate) output: String,
    pub(crate) worker: Option<Job>,
    pub(crate) game_process: Option<std::process::Child>,
    pub report: Option<Report>,
    pub(crate) report_json: String,
    pub(crate) package_path: Option<PathBuf>,
    pub(crate) file_preview: Option<(PathBuf, String, Option<usize>)>,
    pub(crate) table_worker: Option<Receiver<Result<(crate::tables::Tables, Jump), String>>>,
    pub render: Option<PathBuf>,
    frames: usize,
    map_close: bool,
}
impl Default for Studio {
    fn default() -> Self {
        Self {
            settings: Settings::load(),
            tables: Default::default(),
            workbench: Default::default(),
            kit: Default::default(),
            maps: Default::default(),
            ai: Default::default(),
            scripts: Default::default(),
            graphs: Default::default(),
            layout: Default::default(),
            effects: Default::default(),
            net: Default::default(),
            stats: Default::default(),
            difficulties: Default::default(),
            acts: Default::default(),
            importer: Default::default(),
            script_check_pending: false,
            project: None,
            wizard: false,
            draft: default_manifest(),
            destination: String::new(),
            open_path: String::new(),
            template: 0,
            base: 0,
            tab: "Manifest".into(),
            output: String::new(),
            worker: None,
            game_process: None,
            report: None,
            report_json: String::new(),
            package_path: None,
            file_preview: None,
            table_worker: None,
            render: None,
            frames: 0,
            map_close: false,
        }
    }
}
impl Studio {
    pub fn show_tables(&mut self, data: crate::tables::Tables, dropdown: bool) {
        self.tables.data = Some(data);
        self.tables.render_dropdown = dropdown;
        self.tab = "Tables".into();
    }
    /// Open an engine workbench over the shared table working set.
    pub fn show_workbench(&mut self, data: crate::tables::Tables, view: usize) {
        self.tab = ["Items", "Drops", "Cube"][view.min(2)].into();
        self.workbench.view = view.min(2);
        self.tables.data = Some(data);
    }
    /// Open one of the state, skill and trigger editors.
    pub fn show_kit(&mut self, view: crate::kit_ui::View, data: Option<crate::tables::Tables>) {
        self.tab = crate::kit_ui::View::TABS
            .iter()
            .find(|(_, v)| *v == view)
            .map_or("States", |(t, _)| t)
            .into();
        if data.is_some() {
            self.tables.data = data;
        }
    }
    /// Open the monster AI editor.
    pub fn show_ai(&mut self, data: Option<crate::tables::Tables>) {
        self.tab = AI_TAB.into();
        if data.is_some() {
            self.tables.data = data;
        }
    }
    pub fn show_graphs(&mut self) {
        self.tab = "Graphs".into();
    }
    pub fn show_network(&mut self) {
        self.tab = "Network".into();
    }
    pub fn show_effects(&mut self) {
        self.tab = "Effects".into();
    }
    pub fn show_layout(&mut self, data: Option<crate::tables::Tables>) {
        self.tab = "Layout".into();
        if data.is_some() {
            self.tables.data = data;
        }
    }
    fn result(&mut self, result: Result<()>) {
        self.output = result.map_or_else(|e| format!("{e:#}"), |()| "Saved.".into());
    }
    pub fn open(&mut self, path: &Path) {
        if self.maps.dirty() {
            self.output = "Save map edits before opening another project.".into();
            return;
        }
        match Project::open(path) {
            Ok(project) => {
                self.settings.recent.retain(|p| p != path);
                self.settings.recent.insert(0, path.into());
                self.settings.recent.truncate(12);
                self.project = Some(project);
                self.tables = Default::default();
                self.workbench = Default::default();
                self.kit.open(path);
                self.maps = Default::default();
                self.ai.open(path);
                self.scripts.open(path);
                self.graphs.open(path);
                self.layout.open(path);
                self.effects.open(path);
                self.net.open(path);
                self.stats.open(path);
                self.difficulties.open(path);
                self.acts.open(path);
                self.importer = Default::default();
                self.script_check_pending = false;
                self.report = None;
                self.file_preview = None;
                self.package_path = None;
                let result = self.settings.save();
                self.result(result);
            }
            Err(e) => self.output = format!("{e:#}"),
        }
    }
    fn start(&mut self, ui: &mut egui::Ui) {
        ui.add_space(35.0);
        ui.heading(RichText::new("Mod Studio").size(36.0));
        ui.label("Make a new world. Keep your files readable.");
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            if ui.button("New mod").clicked() {
                self.draft = default_manifest();
                self.base = 0;
                self.template = 0;
                self.wizard = true;
            }
            if ui.button("New add-on").clicked() {
                self.draft = default_manifest();
                self.draft.kind = Kind::Addon;
                self.draft.id = "my-new-addon".into();
                self.draft.name = "My new add-on".into();
                self.base = 0;
                self.template = 0;
                self.wizard = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Project folder");
            ui.text_edit_singleline(&mut self.open_path);
            if ui.button("Open folder").clicked() {
                self.open(&PathBuf::from(&self.open_path));
            }
        });
        ui.add_space(20.0);
        ui.heading("Recent projects");
        for path in self.settings.recent.clone() {
            if ui.button(path.display().to_string()).clicked() {
                self.open(&path);
            }
        }
        if self.settings.recent.is_empty() {
            ui.weak("Your projects will appear here after you open or create one.");
        }
        if self.wizard {
            ui.separator();
            ui.heading(if self.draft.kind == Kind::Addon {
                "New add-on"
            } else {
                "New mod"
            });
            field(ui, "Name", &mut self.draft.name);
            field(ui, "ID", &mut self.draft.id);
            field(ui, "Author", &mut self.draft.author);
            field(ui, "Description", &mut self.draft.description);
            field(ui, "Destination folder", &mut self.destination);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.draft.kind, Kind::Full, "Full mod");
                ui.selectable_value(&mut self.draft.kind, Kind::Addon, "Add-on");
            });
            let bases = if self.draft.kind == Kind::Addon {
                installed_addon_targets(Path::new(&self.settings.installed))
            } else {
                installed_bases(Path::new(&self.settings.installed))
            };
            if self.base > bases.len() {
                self.base = 0;
            }
            egui::ComboBox::from_label("Base / add-on target")
                .selected_text(if self.base == 0 {
                    "D2Gamma"
                } else {
                    &bases[self.base - 1].name
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.base, 0, "D2Gamma");
                    for (i, b) in bases.iter().enumerate() {
                        ui.selectable_value(&mut self.base, i + 1, &b.name);
                    }
                });
            if self.draft.kind == Kind::Addon {
                ui.weak("Choose the target mod. Its maker must list this add-on id in approved_addons before it can run. Plain D2Gamma needs owner approval.");
                if self.base == 0 {
                    self.draft.screen_only = true;
                } else {
                    ui.checkbox(
                        &mut self.draft.screen_only,
                        "Screen only (UI, visuals and strings)",
                    );
                }
                if self.draft.screen_only {
                    self.template = 0;
                }
            } else {
                ui.weak("Installed bases must allow derived works.");
            }
            ui.add_enabled_ui(!self.draft.screen_only, |ui| {
                egui::ComboBox::from_label("Template")
                    .selected_text(TEMPLATES[self.template].0)
                    .show_ui(ui, |ui| {
                        for (i, (name, _)) in TEMPLATES.iter().enumerate() {
                            ui.selectable_value(&mut self.template, i, *name);
                        }
                    });
            });
            let validation =
                select_base(&mut self.draft, self.base.checked_sub(1).map(|i| &bases[i])).and_then(
                    |()| {
                        Manifest::parse(&self.draft.to_toml())
                            .map(|_| ())
                            .map_err(Into::into)
                    },
                );
            if let Err(e) = &validation {
                ui.colored_label(Color32::LIGHT_RED, e.to_string());
            }
            if ui
                .add_enabled(
                    validation.is_ok() && !self.destination.is_empty(),
                    egui::Button::new("Create project"),
                )
                .clicked()
            {
                let path = PathBuf::from(&self.destination);
                match create_project(&path, &self.draft, self.template) {
                    Ok(_) => {
                        self.wizard = false;
                        self.open(&path);
                    }
                    Err(e) => self.output = format!("{e:#}"),
                }
            }
        }
    }
    pub fn draw(&mut self, ctx: &egui::Context) {
        ctx.set_visuals(egui::Visuals::dark());
        self.importer.poll(ctx);
        self.poll_jobs(ctx);
        if self.project.is_some()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S))
        {
            self.save_from_ui();
        }
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("MOD STUDIO");
                ui.separator();
                let busy = self.busy();
                if ui
                    .add_enabled(
                        self.project.is_some() && !busy,
                        egui::Button::new("Start page"),
                    )
                    .clicked()
                {
                    if self.project.as_ref().is_some_and(|p| p.dirty)
                        || self.tables.dirty()
                        || self.kit.dirty()
                        || self.maps.dirty()
                        || self.ai.dirty()
                        || self.scripts.dirty()
                        || self.graphs.dirty()
                        || self.layout.dirty()
                        || self.effects.dirty()
                        || self.net.dirty()
                    {
                        self.output = "Save the project before returning to the start page.".into();
                    } else {
                        self.project = None;
                        self.report = None;
                        self.file_preview = None;
                        self.package_path = None;
                    }
                }
                if ui
                    .add_enabled(self.project.is_some(), egui::Button::new("Save"))
                    .clicked()
                {
                    self.save_from_ui();
                }
                if ui
                    .add_enabled(
                        self.project.is_some()
                            && !busy
                            && self.game_process.is_none()
                            && !self.settings.game.is_empty(),
                        egui::Button::new("Play"),
                    )
                    .clicked()
                {
                    self.run_tool(Action::Play);
                }
                if self.game_process.is_some() && ui.button("Stop game").clicked() {
                    self.stop_game();
                }
                if ui
                    .add_enabled(
                        self.project.is_some()
                            && !busy
                            && !self.settings.game.is_empty()
                            && !self.settings.native.is_empty(),
                        egui::Button::new("Check"),
                    )
                    .clicked()
                {
                    self.run_tool(Action::Check);
                }
                if ui
                    .add_enabled(
                        self.project.is_some()
                            && !busy
                            && !self.settings.game.is_empty()
                            && !self.settings.native.is_empty(),
                        egui::Button::new("Package"),
                    )
                    .clicked()
                {
                    self.run_tool(Action::Package);
                }
                if busy {
                    ui.spinner();
                    if let Some(job) = &self.worker {
                        ui.label(format!(
                            "Running: {:.0}s",
                            job.started.elapsed().as_secs_f32()
                        ));
                    }
                    if ui.button("Cancel").clicked() {
                        if let Some(job) = &self.worker {
                            job.cancel();
                        }
                        self.table_worker = None;
                        self.script_check_pending = false;
                        self.output = "Cancelling...".into();
                    }
                }
            });
        });
        egui::TopBottomPanel::bottom("settings-output").show(ctx, |ui| {
            if self.worker.is_none() {
                if let Some(path) = &self.package_path {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::LIGHT_GREEN, "Package ready");
                        ui.label(path.display().to_string());
                        if ui.small_button("Copy path").clicked() {
                            ui.ctx().copy_text(path.display().to_string());
                        }
                    });
                }
            }
            egui::CollapsingHeader::new("Tools & install paths").show(ui, |ui| {
                field(ui, "D2Gamma-Game executable", &mut self.settings.game);
                field(ui, "Converted native install", &mut self.settings.native);
                field(ui, "Installed mods folder", &mut self.settings.installed);
                if ui.button("Save paths").clicked() {
                    let r = self.settings.save();
                    self.result(r);
                }
            });
            egui::ScrollArea::vertical()
                .max_height(120.0)
                .show(ui, |ui| {
                    ui.label(&self.output);
                });
        });
        self.problems(ctx);
        if let Some(project) = &mut self.project {
            egui::SidePanel::left("files")
                .default_width(250.0)
                .show(ctx, |ui| {
                    ui.heading(&project.folder.manifest.name);
                    ui.weak(project.folder.root.display().to_string());
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("studio-navigation")
                        .show(ui, |ui| {
                            ui.selectable_value(
                                &mut self.tab,
                                "Manifest".into(),
                                "Manifest - mod.toml",
                            );
                            ui.selectable_value(
                                &mut self.tab,
                                "Rules".into(),
                                "Rules - rules.toml",
                            );
                            ui.selectable_value(&mut self.tab, "Acts".into(), "Acts & waypoints");
                            ui.selectable_value(&mut self.tab, "Stats".into(), "Named stats");
                            ui.selectable_value(
                                &mut self.tab,
                                "Difficulties".into(),
                                "Difficulties",
                            );
                            ui.selectable_value(&mut self.tab, "Tables".into(), "Table editor");
                            for tab in ["Items", "Drops", "Cube"] {
                                ui.selectable_value(&mut self.tab, tab.into(), tab);
                            }
                            ui.selectable_value(&mut self.tab, "Maps".into(), "Map editor");
                            for (tab, _) in crate::kit_ui::View::TABS {
                                ui.selectable_value(&mut self.tab, tab.into(), tab);
                            }
                            ui.selectable_value(&mut self.tab, AI_TAB.into(), AI_TAB);
                            ui.selectable_value(&mut self.tab, "Scripts".into(), "Scripts");
                            ui.selectable_value(&mut self.tab, "Graphs".into(), "Visual scripts");
                            ui.selectable_value(&mut self.tab, "Layout".into(), "Panel layout");
                            ui.selectable_value(&mut self.tab, "Effects".into(), "Visual effects");
                            ui.selectable_value(
                                &mut self.tab,
                                "Network".into(),
                                "Network values & actions",
                            );
                            ui.selectable_value(
                                &mut self.tab,
                                "Import".into(),
                                "Import art & sound",
                            );
                            if ui
                                .button("Load tables")
                                .on_hover_text("Save edits before loading again")
                                .clicked()
                            {
                                if self.tables.dirty() || project.dirty {
                                    self.output = "Save table edits before loading again.".into();
                                } else {
                                    match crate::tables::Tables::load(
                                        Path::new(&self.settings.native),
                                        Path::new(&self.settings.installed),
                                        &project.folder.root,
                                    ) {
                                        Ok(data) => {
                                            self.tables = Default::default();
                                            self.workbench = Default::default();
                                            self.tables.data = Some(data);
                                            self.tab = "Tables".into();
                                        }
                                        Err(e) => self.output = format!("{e:#}"),
                                    }
                                }
                            }
                            egui::CollapsingHeader::new("Project files").show(ui, |ui| {
                                for kind in ["Tables", "Strings", "Art", "Sounds", "Other"] {
                                    egui::CollapsingHeader::new(kind).default_open(false).show(
                                        ui,
                                        |ui| {
                                            for f in &project.folder.files {
                                                if f.path != "files/rules.toml"
                                                    && file_kind(&f.path) == kind
                                                {
                                                    ui.label(&f.path);
                                                }
                                            }
                                        },
                                    );
                                }
                            });
                        });
                });
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading(format!(
                    "{}{}",
                    self.tab,
                    if project.dirty { " * unsaved" } else { "" }
                ));
                ui.separator();
                if self.tab == "Acts" {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.acts.draw(ui, project.folder.manifest.screen_only)
                    });
                    return;
                }
                if self.tab == "Difficulties" {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.difficulties
                            .draw(ui, project.folder.manifest.screen_only)
                    });
                    return;
                }
                if self.tab == "Stats" {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.stats.draw(ui, project.folder.manifest.screen_only)
                    });
                    return;
                }
                if self.tab == "Graphs" {
                    egui::ScrollArea::vertical()
                        .id_salt("visual-graph-page")
                        .show(ui, |ui| self.graphs.draw(ui));
                    return;
                }
                if self.tab == "Network" {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.net
                            .draw(ui, self.tables.data.as_ref(), &self.scripts.scripts)
                    });
                    return;
                }
                if self.tab == "Effects" {
                    self.effects
                        .draw(ui, self.tables.data.as_ref(), &self.settings.native);
                    return;
                }
                if self.tab == "Import" {
                    self.importer
                        .draw(ui, &project.folder.root, &self.settings.native);
                    return;
                }
                if self.tab == "Layout" {
                    let definition = self.net.definition().unwrap_or_default();
                    self.layout.synced_choices = definition
                        .values
                        .iter()
                        .map(|v| format!("sync.{}", v.name))
                        .collect();
                    self.layout.network_actions = definition
                        .actions
                        .iter()
                        .map(|a| format!("net.{}", a.rule.name))
                        .collect();
                    self.layout.draw(ui, self.tables.data.as_ref());
                    return;
                }
                if self.tab == "Scripts" {
                    self.scripts.draw(ui);
                    return;
                }
                if self.tab == "Maps" {
                    egui::ScrollArea::vertical()
                        .id_salt("map-editor")
                        .show(ui, |ui| {
                            self.maps.draw(
                                ui,
                                &project.folder.root,
                                Path::new(&self.settings.native),
                                Path::new(&self.settings.installed),
                                project.folder.manifest.screen_only,
                            )
                        });
                    return;
                }
                if let Some(view) = crate::kit_ui::View::of(&self.tab) {
                    self.kit.draw(ui, view, self.tables.data.as_mut());
                    return;
                }
                if self.tab == AI_TAB {
                    let tools = crate::arena_ui::ArenaTools {
                        game: Path::new(&self.settings.game),
                        native: Path::new(&self.settings.native),
                        root: &project.folder.root,
                    };
                    self.ai.draw(ui, self.tables.data.as_ref(), &tools);
                    return;
                }
                if let Some(view) = ["Items", "Drops", "Cube"]
                    .iter()
                    .position(|t| *t == self.tab)
                {
                    self.workbench.view = view;
                    self.workbench.draw(ui, self.tables.data.as_mut());
                    return;
                }
                if self.tab == "File" {
                    if let Some((path, text, line)) = &mut self.file_preview {
                        ui.label(path.display().to_string());
                        ui.weak("File preview: first 256 KiB");
                        let selected = line.take();
                        egui::ScrollArea::both()
                            .id_salt("problem-file")
                            .show(ui, |ui| {
                                for (n, text) in text.lines().enumerate() {
                                    let response = ui.monospace(format!("{:4}  {text}", n + 1));
                                    if selected == Some(n + 1) {
                                        response.scroll_to_me(None);
                                    }
                                }
                            });
                    }
                    return;
                }
                if self.tab == "Tables" {
                    self.tables.draw(ui);
                    return;
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.tab == "Manifest" {
                        project.dirty |= manifest_form(ui, &mut project.manifest);
                        match project.validate() {
                            Ok(m) => {
                                ui.colored_label(Color32::LIGHT_GREEN, "Manifest valid");
                                match d2_modkit::check_api_now(m.api) {
                                    d2_modkit::ApiCheck::Ok => {
                                        ui.label("API version supported");
                                    }
                                    d2_modkit::ApiCheck::NeedsUpdate(message)
                                    | d2_modkit::ApiCheck::TooNew(message) => {
                                        ui.colored_label(Color32::LIGHT_RED, message);
                                    }
                                }
                            }
                            Err(e) => {
                                ui.colored_label(Color32::LIGHT_RED, e.to_string());
                            }
                        }
                    } else {
                        ui.label(
                            "Omitted settings keep D2Gamma's defaults. Hover a key for its effect.",
                        );
                        let fields = rule_fields();
                        if let Ok(mut table) = project.rules.parse::<toml::Table>() {
                            let mut changed = false;
                            for (key, default, help) in fields {
                                let parts: Vec<_> = key.split('.').collect();
                                changed |=
                                    rule_control(ui, &mut table, &parts, &key, &default, &help);
                            }
                            if changed {
                                project.rules = toml::to_string_pretty(&table).unwrap_or_default();
                                project.dirty = true;
                            }
                        }
                        ui.separator();
                        ui.label("Readable rules.toml (TOML syntax checked on save)");
                        project.dirty |= ui
                            .add(
                                egui::TextEdit::multiline(&mut project.rules)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(12),
                            )
                            .changed();
                    }
                });
            });
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.start(ui);
            });
        }
    }
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
    });
}
fn table_form(ui: &mut egui::Ui, table: &mut toml::Table, prefix: &str) -> bool {
    let mut changed = false;
    for (key, value) in table {
        let label = format!("{prefix}{key}");
        match value {
            toml::Value::Table(t) => {
                changed |= table_form(ui, t, &format!("{label}."));
            }
            toml::Value::String(s) => {
                ui.horizontal(|ui| {
                    ui.label(label);
                    changed |= ui.text_edit_singleline(s).changed();
                });
            }
            toml::Value::Boolean(b) => {
                changed |= ui.checkbox(b, label).changed();
            }
            toml::Value::Integer(n) => {
                ui.horizontal(|ui| {
                    ui.label(label);
                    changed |= ui.add(egui::DragValue::new(n)).changed();
                });
            }
            toml::Value::Array(values) => {
                ui.label(&label);
                let mut remove = None;
                for (i, value) in values.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if let toml::Value::String(s) = value {
                            changed |= ui.text_edit_singleline(s).changed();
                        }
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    values.remove(i);
                    changed = true;
                }
                if ui.small_button(format!("Add {label}")).clicked() {
                    values.push(toml::Value::String(String::new()));
                    changed = true;
                }
            }
            other => {
                let mut text = other.to_string();
                ui.horizontal(|ui| {
                    ui.label(label);
                    if ui.text_edit_singleline(&mut text).changed() {
                        if let Ok(t) = format!("v = {text}").parse::<toml::Table>() {
                            *other = t["v"].clone();
                            changed = true;
                        }
                    }
                });
            }
        }
    }
    changed
}
fn rule_control(
    ui: &mut egui::Ui,
    table: &mut toml::Table,
    parts: &[&str],
    label: &str,
    default: &str,
    help: &str,
) -> bool {
    if parts.len() > 1 {
        // Do not add a section until a control in it is changed.
        let mut section = table
            .get(parts[0])
            .and_then(|v| v.as_table())
            .cloned()
            .unwrap_or_default();
        if rule_control(ui, &mut section, &parts[1..], label, default, help) {
            table.insert(parts[0].into(), toml::Value::Table(section));
            return true;
        }
        return false;
    }
    let key = parts[0];
    let mut enabled = table.contains_key(key);
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui
            .checkbox(&mut enabled, label)
            .on_hover_text(help)
            .changed()
        {
            if enabled {
                let initial = if default == "omitted" {
                    if label.contains("penalties") {
                        "[0, 0, 0]"
                    } else if label.contains("threshold_denominator") {
                        "12"
                    } else {
                        "100"
                    }
                } else {
                    default
                };
                if let Ok(t) = format!("v = {initial}").parse::<toml::Table>() {
                    table.insert(key.into(), t["v"].clone());
                }
            } else {
                table.remove(key);
            }
            changed = true;
        }
        if let Some(value) = table.get_mut(key) {
            match value {
                toml::Value::Boolean(b) => {
                    changed |= ui.checkbox(b, "Enabled").changed();
                }
                toml::Value::Integer(n) => {
                    changed |= ui.add(egui::DragValue::new(n)).changed();
                }
                toml::Value::Array(values) => {
                    for value in values {
                        if let toml::Value::Integer(n) = value {
                            changed |= ui.add(egui::DragValue::new(n)).changed();
                        }
                    }
                }
                other => {
                    let mut text = other.to_string();
                    if ui.text_edit_singleline(&mut text).changed() {
                        if let Ok(t) = format!("v = {text}").parse::<toml::Table>() {
                            *other = t["v"].clone();
                            changed = true;
                        }
                    }
                }
            }
        } else {
            ui.weak(format!("Default: {default}"));
        }
    });
    changed
}
fn file_kind(path: &str) -> &str {
    if path.contains("patches/") || path.contains("tables/") {
        "Tables"
    } else if path.contains("strings/") || path.ends_with(".tbl") {
        "Strings"
    } else if path.ends_with(".wav") || path.ends_with(".ogg") {
        "Sounds"
    } else if path.ends_with(".png") || path.ends_with(".dc6") || path.ends_with(".dcc") {
        "Art"
    } else {
        "Other"
    }
}
impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && self.maps.dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.map_close = true;
        }
        self.draw(ctx);
        if self.map_close {
            egui::Window::new("Unsaved map edits")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Save your map edits before closing Studio?");
                    ui.horizontal(|ui| {
                        if ui.button("Save and close").clicked() {
                            match self.save_project() {
                                Ok(_) => {
                                    self.map_close = false;
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                                Err(e) => self.output = format!("Could not save project: {e:#}"),
                            }
                        }
                        if ui.button("Discard and close").clicked() {
                            self.maps = Default::default();
                            self.map_close = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Keep editing").clicked() {
                            self.map_close = false;
                        }
                    });
                });
        }
        self.frames += 1;
        if let Some(path) = &self.render {
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let bytes: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    image::save_buffer(
                        path,
                        &bytes,
                        image.size[0] as u32,
                        image.size[1] as u32,
                        image::ColorType::Rgba8,
                    )
                    .expect("save render");
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            if self.frames == 4 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            ctx.request_repaint();
        }
    }
}
fn manifest_form(ui: &mut egui::Ui, table: &mut toml::Table) -> bool {
    let mut changed = false;
    for (key, label) in [
        ("name", "Name"),
        ("id", "Id"),
        ("version", "Version"),
        ("author", "Author"),
        ("description", "Description"),
    ] {
        let value = table
            .entry(key)
            .or_insert(toml::Value::String(String::new()));
        if let toml::Value::String(s) = value {
            ui.horizontal(|ui| {
                ui.add_sized([130.0, 22.0], egui::Label::new(label));
                changed |= ui
                    .add(egui::TextEdit::singleline(s).desired_width(400.0))
                    .changed();
            });
        }
    }
    let mut kind = table
        .get("kind")
        .and_then(toml::Value::as_str)
        .unwrap_or("full")
        .to_owned();
    let old_kind = kind.clone();
    egui::ComboBox::from_label("Kind")
        .selected_text(if kind == "full" { "Full" } else { "Add-on" })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut kind, "full".into(), "Full");
            ui.selectable_value(&mut kind, "addon".into(), "Add-on");
        });
    if old_kind != kind {
        table.insert("kind".into(), toml::Value::String(kind.clone()));
        if kind == "addon" {
            table.remove("base");
            table.remove("allow_derived");
            table.remove("approved_addons");
            table.remove("market");
            table.remove("ladder");
            table.remove("accepts_d2gamma_characters");
            table.insert("for".into(), toml::Value::String("d2gamma".into()));
            table.insert("screen_only".into(), toml::Value::Boolean(true));
        } else {
            table.remove("for");
            table.remove("screen_only");
        }
        changed = true;
    }
    if kind == "full" {
        let mut has_base = table.contains_key("base");
        if ui.checkbox(&mut has_base, "Base mod").changed() {
            if has_base {
                table.insert(
                    "base".into(),
                    toml::Value::Table(toml::Table::from_iter([
                        ("id".into(), toml::Value::String(String::new())),
                        ("version".into(), toml::Value::String("*".into())),
                    ])),
                );
            } else {
                table.remove("base");
            }
            changed = true;
        }
        if let Some(toml::Value::Table(base)) = table.get_mut("base") {
            for (key, label) in [("id", "Base mod Id"), ("version", "Base version range")] {
                if let Some(toml::Value::String(s)) = base.get_mut(key) {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        changed |= ui.text_edit_singleline(s).changed();
                    });
                }
            }
        }
        let value = table
            .entry("allow_derived")
            .or_insert(toml::Value::Boolean(false));
        if let toml::Value::Boolean(b) = value {
            changed |= ui
                .checkbox(b, "Allow others to build on this mod")
                .changed();
        }
        let value = table
            .entry("accepts_d2gamma_characters")
            .or_insert(toml::Value::Boolean(false));
        if let toml::Value::Boolean(b) = value {
            changed |= ui
                .checkbox(b, "Accept copies of D2Gamma characters")
                .changed();
        }
        for (key, label) in [
            ("market", "Enable online market"),
            ("ladder", "Enable online ladder"),
        ] {
            let mut enabled = table
                .get(key)
                .and_then(toml::Value::as_bool)
                .unwrap_or(true);
            if ui.checkbox(&mut enabled, label).changed() {
                table.insert(key.into(), toml::Value::Boolean(enabled));
                changed = true;
            }
        }
        ui.label("Approved add-ons");
        let value = table
            .entry("approved_addons")
            .or_insert(toml::Value::Array(Vec::new()));
        let mut subset = toml::Table::from_iter([("Approved add-on".into(), value.clone())]);
        if table_form(ui, &mut subset, "") {
            *value = subset.remove("Approved add-on").expect("array");
            changed = true;
        }
    } else {
        if let Some(toml::Value::String(s)) = table.get_mut("for") {
            ui.horizontal(|ui| {
                ui.label("Attach to mod");
                changed |= ui.text_edit_singleline(s).changed();
            });
        }
        if let Some(toml::Value::Boolean(b)) = table.get_mut("screen_only") {
            changed |= ui.checkbox(b, "Screen changes only").changed();
        }
    }
    ui.label(format!(
        "API version: {} (read-only)",
        table
            .get("api")
            .map(ToString::to_string)
            .unwrap_or_default()
    ));
    ui.collapsing("Advanced layer settings", |ui| {
        for (key, label) in [
            ("requires", "Required layers"),
            ("native_format_version", "Native format version"),
        ] {
            if let Some(value) = table.get_mut(key) {
                let mut subset = toml::Table::from_iter([(label.into(), value.clone())]);
                if table_form(ui, &mut subset, "") {
                    *value = subset.remove(label).expect("field");
                    changed = true;
                }
            }
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_save_check_requires_configured_tools() {
        let mut studio = Studio {
            settings: Default::default(),
            ..Default::default()
        };
        studio.request_script_check();
        assert!(!studio.script_check_pending);
        assert!(studio.output.contains("Configure the game executable"));
        studio.settings.game = "game".into();
        studio.settings.native = "native".into();
        studio.request_script_check();
        assert!(studio.script_check_pending);
    }

    #[test]
    fn script_automatic_check_does_not_save_newer_edits() {
        let dir =
            std::env::temp_dir().join(format!("studio-auto-script-check-{}", std::process::id()));
        let project = crate::create_project(&dir, &crate::default_manifest(), 0).unwrap();
        let mut studio = Studio {
            settings: Default::default(),
            project: Some(project),
            ..Default::default()
        };
        studio.scripts.open(&dir);
        let index = studio.scripts.scripts.add("main.luau").unwrap();
        assert!(studio.save_project().unwrap());
        let saved = std::fs::read_to_string(dir.join("files/scripts/main.luau")).unwrap();
        studio.scripts.scripts.files[index]
            .text
            .push_str("-- unsaved later edit\n");
        studio.settings.game = dir.join("missing-game").display().to_string();
        studio.settings.native = dir.display().to_string();
        studio.request_script_check();
        studio.poll_jobs(&egui::Context::default());
        assert!(!studio.script_check_pending);
        assert!(studio.worker.is_some());
        assert!(studio.scripts.dirty());
        assert_eq!(
            std::fs::read_to_string(dir.join("files/scripts/main.luau")).unwrap(),
            saved
        );
        drop(studio);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn project_save_includes_validated_layout_edits() {
        let root = std::env::temp_dir().join(format!("studio-layout-save-{}", std::process::id()));
        let project = crate::create_project(&root, &crate::default_manifest(), 0).unwrap();
        let mut studio = Studio {
            project: Some(project),
            ..Default::default()
        };
        studio.layout.open(&root);
        let i = studio
            .layout
            .layouts
            .as_mut()
            .unwrap()
            .create("hud_extra")
            .unwrap();
        studio.layout.layouts.as_mut().unwrap().documents[i]
            .add("bar", 10, 20)
            .unwrap();
        assert!(studio.layout.dirty());
        studio.save_project().unwrap();
        assert!(!studio.layout.dirty());
        assert!(root.join("files/ui/hud_extra.toml").is_file());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn project_save_includes_effects() {
        let root = std::env::temp_dir().join(format!("studio-effects-save-{}", std::process::id()));
        let project = crate::create_project(&root, &crate::default_manifest(), 0).unwrap();
        let mut studio = Studio {
            project: Some(project),
            ..Default::default()
        };
        studio.effects.open(&root);
        studio.effects.add().unwrap();
        studio.save_project().unwrap();
        assert!(root.join("files/effects/studio.toml").is_file());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn project_save_includes_validated_ai_edits() {
        let root = std::env::temp_dir().join(format!("studio-ai-save-{}", std::process::id()));
        let project = crate::create_project(&root, &crate::default_manifest(), 0).unwrap();
        let mut studio = Studio {
            project: Some(project),
            ..Default::default()
        };
        studio.ai.open(&root);
        let file = studio.ai.file.as_mut().unwrap();
        file.entries
            .push(crate::ai::Entry::new_graph(200, "saved graph"));
        file.dirty = true;
        studio.save_project().unwrap();
        assert!(!studio.ai.dirty());
        let saved = crate::ai::AiFile::load(&root).unwrap();
        assert_eq!(saved.entries[0].name, "saved graph");
        assert!(saved.problems().is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_save_includes_graphs_and_checks_combined_script_limit() {
        let root = std::env::temp_dir().join(format!("studio-graph-save-{}", std::process::id()));
        let project = crate::create_project(&root, &crate::default_manifest(), 0).unwrap();
        let mut studio = Studio {
            project: Some(project),
            ..Default::default()
        };
        studio.scripts.open(&root);
        studio.graphs.open(&root);
        studio
            .graphs
            .graphs
            .as_mut()
            .unwrap()
            .add("echo.graph.toml")
            .unwrap();
        assert!(studio.save_project().unwrap());
        assert!(!studio.graphs.dirty());
        assert!(root.join("files/scripts/echo.graph.toml").is_file());
        for i in 0..64 {
            studio
                .scripts
                .scripts
                .add(&format!("module{i}.luau"))
                .unwrap();
        }
        assert!(studio
            .save_project()
            .unwrap_err()
            .to_string()
            .contains("64 combined"));
        assert!(!root.join("files/scripts/module0.luau").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod network_save_tests {
    use super::*;
    #[test]
    fn network_save_validates_references_before_writing_project_changes() {
        let root = std::env::temp_dir().join(format!("studio-network-save-{}", std::process::id()));
        std::fs::create_dir_all(root.join("files/ui")).unwrap();
        std::fs::write(root.join("files/ui/hud.toml"),"version=1\nname='hud'\n[[elements]]\nid='x'\nkind='text'\n[elements.bind]\nsource='sync.missing'").unwrap();
        let mut s = Studio::default();
        s.layout.open(&root);
        s.net.open(&root);
        s.net.add(false).unwrap();
        assert!(s.save_project().is_err());
        assert!(!root.join("files/net.toml").exists());
        let t = s
            .net
            .table
            .get_mut("values")
            .unwrap()
            .as_array_mut()
            .unwrap()[0]
            .as_table_mut()
            .unwrap();
        t.insert("name".into(), toml::Value::String("missing".into()));
        s.save_project().unwrap();
        assert!(root.join("files/net.toml").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
