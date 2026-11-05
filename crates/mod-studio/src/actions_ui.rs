//! Tool actions, progress and severity-grouped diagnostics in the Studio shell.
use crate::{
    actions::{Event, Job, Jump, Report, Severity},
    ui::Studio,
    ToolCommand,
};
use anyhow::Result;
use eframe::egui::{self, Color32, RichText};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
};

#[derive(Clone, Copy)]
pub(crate) enum Action {
    Play,
    Check,
    Package,
}

impl Studio {
    pub fn show_report(&mut self, json: &str) -> Result<()> {
        self.report = Some(Report::parse(json)?);
        self.report_json = json.into();
        Ok(())
    }
    pub(crate) fn busy(&self) -> bool {
        self.worker.is_some() || self.table_worker.is_some() || self.importer.busy()
    }
    pub(crate) fn run_tool(&mut self, action: Action) {
        if self.project.is_none() {
            return;
        }
        let scripts_changed = match self.save_project() {
            Ok(changed) => changed,
            Err(e) => {
                self.output = format!("Save failed: {e:#}");
                return;
            }
        };
        self.start_tool(action);
        if scripts_changed && matches!(action, Action::Play) {
            self.request_script_check();
        }
    }
    pub(crate) fn save_project(&mut self) -> Result<bool> {
        anyhow::ensure!(
            !self.importer.busy(),
            "Wait for the import to finish before saving"
        );
        let graphs = self
            .graphs
            .graphs
            .as_ref()
            .map(|g| g.documents.as_slice())
            .unwrap_or_default();
        anyhow::ensure!(
            self.scripts.scripts.files.len() + graphs.len() <= 64,
            "At most 64 combined Luau and graph modules"
        );
        let source_bytes: usize = self
            .scripts
            .scripts
            .files
            .iter()
            .map(|s| s.text.len())
            .sum::<usize>()
            + graphs
                .iter()
                .map(|g| {
                    g.graph()
                        .ok()
                        .and_then(|g| g.compile().ok())
                        .map_or(g.text.len(), |s| s.len())
                })
                .sum::<usize>();
        anyhow::ensure!(
            source_bytes <= 1024 * 1024,
            "Combined generated graph and Luau source exceeds 1 MiB"
        );
        self.acts.validate()?;
        anyhow::ensure!(
            !self.acts.dirty()
                || !self
                    .project
                    .as_ref()
                    .is_some_and(|p| p.folder.manifest.screen_only),
            "Screen-only add-ons cannot define acts"
        );
        self.difficulties.validate()?;
        if self.difficulties.dirty() {
            anyhow::ensure!(
                !self
                    .project
                    .as_ref()
                    .is_some_and(|p| p.folder.manifest.screen_only),
                "Screen-only add-ons cannot define difficulties"
            );
        }
        self.stats.validate()?;
        if self.stats.dirty() {
            anyhow::ensure!(
                !self
                    .project
                    .as_ref()
                    .is_some_and(|p| p.folder.manifest.screen_only),
                "Screen-only add-ons cannot define stats"
            );
        }
        self.net.validate()?;
        let network = self.net.definition()?;
        if self.net.configured() {
            if let Some(layouts) = &self.layout.layouts {
                for doc in &layouts.documents {
                    network
                        .resolve_panel(&mut doc.definition()?)
                        .map_err(anyhow::Error::msg)?;
                }
            }
        }
        self.acts.save()?;
        self.difficulties.save()?;
        self.stats.save()?;
        self.net.save()?;
        self.tables.save()?;
        self.kit.save()?;
        self.maps.save()?;
        self.ai.save()?;
        self.layout.save()?;
        self.effects.save()?;
        let changed = self.scripts.save()?;
        let graphs_changed = self.graphs.save()?;
        let changed = changed || graphs_changed;
        if let Some(project) = &mut self.project {
            project.save()?;
        }
        Ok(changed)
    }
    pub(crate) fn save_from_ui(&mut self) {
        match self.save_project() {
            Ok(changed) => {
                self.output = "Saved.".into();
                if changed {
                    self.request_script_check();
                }
            }
            Err(e) => self.output = format!("Save failed: {e:#}"),
        }
    }
    pub(crate) fn request_script_check(&mut self) {
        if self.settings.game.is_empty() || self.settings.native.is_empty() {
            self.output.push_str("\nScripts saved. Configure the game executable and native install, then run Check to validate Luau.");
        } else {
            self.script_check_pending = true;
            self.output.push_str("\nScript check queued.");
        }
    }
    /// Automatic checks use saved files; never silently save newer editor buffers.
    fn start_tool(&mut self, action: Action) {
        let Some(project) = &self.project else {
            return;
        };
        if !matches!(action, Action::Play) {
            self.script_check_pending = false;
        }
        let root = project.folder.root.clone();
        let game = PathBuf::from(&self.settings.game);
        let native = PathBuf::from(&self.settings.native);
        let installed = PathBuf::from(&self.settings.installed);
        if matches!(action, Action::Play) {
            let mut command = ToolCommand::play(&game, &root).command();
            if !native.as_os_str().is_empty() {
                command.arg("--native").arg(native);
            }
            crate::actions::hide_console(&mut command);
            self.output = match command.spawn() {
                Ok(child) => {
                    self.game_process = Some(child);
                    "Game launched. Save changes for dev reload; Stop game ends this play session."
                        .into()
                }
                Err(e) => format!("Could not launch game: {e}"),
            };
            return;
        }
        let m = &project.folder.manifest;
        let output = root
            .parent()
            .unwrap_or(&root)
            .join(format!("{}-{}.d2pack", m.id, m.version));
        let package = matches!(action, Action::Package);
        self.package_path = package.then_some(output.clone());
        self.output = if package {
            "Checking snapshot, then signing and packaging…"
        } else {
            "Checking project…"
        }
        .into();
        self.report = None;
        self.report_json.clear();
        self.worker = Some(Job::start(
            move |report| {
                if package {
                    ToolCommand::package_checked(&game, &root, &native, &output, report, &installed)
                } else {
                    ToolCommand::check(&game, &root, &native, report, &installed)
                }
            },
            self.package_path.clone(),
        ));
    }
    pub(crate) fn stop_game(&mut self) {
        let Some(child) = &mut self.game_process else {
            return;
        };
        match child.try_wait() {
            Ok(Some(status)) => self.output = format!("Game exited: {status}"),
            Ok(None) => match child.kill().and_then(|()| child.wait()) {
                Ok(_) => self.output = "Game stopped.".into(),
                Err(e) => {
                    self.output = format!("Could not stop game: {e}");
                    return;
                }
            },
            Err(e) => {
                self.output = format!("Could not inspect game: {e}");
                return;
            }
        }
        self.game_process = None;
    }
    pub(crate) fn poll_jobs(&mut self, ctx: &egui::Context) {
        if let Some(child) = &mut self.game_process {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.output = format!("Game exited: {status}");
                    self.game_process = None;
                }
                Ok(None) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
                Err(e) => {
                    self.output = format!("Could not inspect game: {e}");
                    self.game_process = None;
                }
            }
        }
        let events: Vec<_> = self
            .worker
            .as_ref()
            .map(|j| j.events.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Output(text) => self.output = text,
                Event::Finished { result, report } => {
                    if let Some((report, json)) = report {
                        self.report = Some(report);
                        self.report_json = json;
                    }
                    match result {
                        Ok(()) => self
                            .output
                            .push_str(&self.package_path.as_ref().map_or_else(
                                || "\nCheck complete.".into(),
                                |p| format!("\nPackage ready: {}", p.display()),
                            )),
                        Err(e) => {
                            self.package_path = None;
                            self.output.push_str(&format!("\n{e}"));
                            if self.report.as_ref().is_none_or(|r| r.exit_code == 0)
                                && e != "Cancelled"
                            {
                                self.report = Some(Report {
                                    format_version: 1,
                                    target: String::new(),
                                    exit_code: 2,
                                    completed: vec![],
                                    findings: vec![crate::actions::Problem {
                                        severity: Severity::Error,
                                        category: "Tool failure".into(),
                                        location: "Project".into(),
                                        message: e,
                                    }],
                                });
                            }
                        }
                    }
                    self.worker = None;
                }
            }
        }
        if let Some(rx) = &self.table_worker {
            if let Ok(result) = rx.try_recv() {
                self.table_worker = None;
                match result {
                    Ok((data, jump)) => {
                        self.tables.data = Some(data);
                        self.jump(jump);
                    }
                    Err(e) => self.output = e,
                }
            }
        }
        if self.script_check_pending && !self.busy() {
            self.script_check_pending = false;
            self.start_tool(Action::Check);
        }
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    fn jump(&mut self, jump: Jump) {
        match jump {
            target @ (Jump::Table { .. } | Jump::Item { .. } | Jump::Key { .. }) => {
                if self.tables.data.is_none() {
                    let Some(project) = &self.project else {
                        return;
                    };
                    let root = project.folder.root.clone();
                    let native = PathBuf::from(&self.settings.native);
                    let installed = PathBuf::from(&self.settings.installed);
                    let (tx, rx) = mpsc::channel();
                    self.table_worker = Some(rx);
                    std::thread::spawn(move || {
                        let result = crate::tables::Tables::load(&native, &installed, &root)
                            .map(|data| (data, target))
                            .map_err(|e| format!("Could not load jump target: {e:#}"));
                        let _ = tx.send(result);
                    });
                    return;
                }
                match self.tables.jump_target(&target) {
                    Ok(()) => self.tab = "Tables".into(),
                    Err(e) => self.output = e.to_string(),
                }
            }
            Jump::Graph { path, node } => {
                if let Some(project) = &self.project {
                    if self.graphs.select_path(&project.folder.root, &path) {
                        self.graphs.selected_node = Some(node);
                        self.tab = "Graphs".into();
                    }
                }
            }
            Jump::File { path, line } => {
                if self
                    .project
                    .as_ref()
                    .is_some_and(|p| self.scripts.select_path(&p.folder.root, &path))
                {
                    self.tab = "Scripts".into();
                    return;
                }
                use std::io::Read;
                let result = (|| -> Result<String> {
                    let mut bytes = Vec::new();
                    std::fs::File::open(&path)?
                        .take(262144)
                        .read_to_end(&mut bytes)?;
                    Ok(String::from_utf8_lossy(&bytes).into_owned())
                })();
                match result {
                    Ok(text) => {
                        self.file_preview = Some((path, text, line));
                        self.tab = "File".into();
                    }
                    Err(e) => self.output = format!("Could not open problem file: {e}"),
                }
            }
        }
    }
    pub(crate) fn problems(&mut self, ctx: &egui::Context) {
        let Some(report) = &self.report else {
            return;
        };
        let mut chosen = None;
        egui::TopBottomPanel::bottom("check-problems").resizable(true).default_height(300.0).show(ctx,|ui| {
            ui.horizontal(|ui| {
                ui.strong(if report.exit_code==0 { "Problems · Check passed" } else { "Problems · Check failed" });
                if ui.small_button("Copy JSON").clicked() { ui.ctx().copy_text(self.report_json.clone()); }
                if ui.small_button("Dismiss").clicked() { chosen=Some(None); }
            });
            egui::ScrollArea::vertical().id_salt("check-report").show(ui,|ui| {
                for (severity,label,color) in [(Severity::Error,"Errors",Color32::LIGHT_RED),(Severity::Warning,"Warnings",Color32::YELLOW),(Severity::Info,"Information",Color32::LIGHT_BLUE)] {
                    let findings:Vec<_>=report.findings.iter().filter(|p|p.severity==severity).collect();
                    if findings.is_empty() { continue; }
                    egui::CollapsingHeader::new(RichText::new(format!("{label} ({})",findings.len())).color(color)).default_open(true).show(ui,|ui| {
                        for (i,p) in findings.iter().enumerate() {
                            ui.push_id((severity as u8,i),|ui| {
                                let response=ui.link(format!("{} · {}",p.location,p.category));
                                if response.clicked() && !self.busy() {
                                    if let Some(project)=&self.project {
                                        if let Some(jump)=p.jump(Path::new(&report.target),&project.folder.root) { chosen=Some(Some(jump)); }
                                        else { ui.ctx().copy_text(format!("{}: {}",p.location,p.message)); }
                                    }
                                }
                                response.on_hover_text("Open the table/row/cell or project file; scenario summaries copy their details");
                                ui.label(&p.message);
                            });
                        }
                    });
                }
                if report.findings.is_empty() { ui.colored_label(Color32::LIGHT_GREEN,"No problems found"); }
                ui.weak(format!("Completed: {}",report.completed.join(", ")));
            });
        });
        if let Some(jump) = chosen {
            if let Some(jump) = jump {
                self.jump(jump);
            } else {
                self.report = None;
            }
        }
    }
}

#[cfg(test)]
mod play_tests {
    use super::*;
    #[test]
    #[ignore = "child process fixture; started explicitly by stop_game_reaps_its_play_process"]
    fn play_process_fixture() {
        std::thread::sleep(std::time::Duration::from_secs(10));
    }

    #[test]
    fn stop_game_reaps_its_play_process() {
        let mut studio = Studio::default();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "actions_ui::play_tests::play_process_fixture",
                "--ignored",
            ])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        studio.game_process = Some(child);
        studio.stop_game();
        assert!(studio.game_process.is_none());
        assert_eq!(studio.output, "Game stopped.");
        // Another stop never affects a process Studio did not start.
        studio.stop_game();
        assert_eq!(studio.output, "Game stopped.");
    }
}
