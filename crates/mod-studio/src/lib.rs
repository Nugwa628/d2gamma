//! Readable mod projects and external tool commands. No game renderer dependency.
pub mod actions;
mod actions_ui;
pub mod acts_ui;
pub mod ai;
pub mod ai_ui;
pub mod arena;
pub mod arena_ui;
pub mod kit;
pub mod kit_ui;
pub mod labels;
pub mod layout;
pub mod layout_ui;
pub mod map_ui;
pub mod maps;
pub mod net_ui;
pub mod quest_panel;
pub mod scripts;
pub mod scripts_ui;
pub mod stats_ui;
pub mod table_ui;
pub mod tables;
pub mod ui;
pub mod workbench;
pub mod workbench_ui;

use anyhow::{bail, Context, Result};
use d2_modkit::{manifest::BaseRef, Kind, Manifest, ModFolder};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub const TEMPLATES: &[(&str, &str)] = &[
    (
        "Empty mod",
        include_str!("../../../templates/empty-mod/files/rules.toml"),
    ),
    (
        "Balance tweak",
        include_str!("../../../templates/balance-tweak/files/rules.toml"),
    ),
];

pub fn default_manifest() -> Manifest {
    Manifest::parse("id = 'my-new-mod'\nname = 'My new mod'\nversion = '0.1.0'\nauthor = 'Your name'\ndescription = 'A new D2Gamma mod'\napi = 1\nkind = 'full'\n").expect("built-in manifest")
}

/// Only installed full mods whose authors allow derivatives appear as bases.
pub fn installed_bases(root: &Path) -> Vec<Manifest> {
    let mut bases: Vec<_> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| ModFolder::scan(&e.path()).ok().map(|f| f.manifest))
        .filter(|m| m.kind == Kind::Full && m.allow_derived)
        .collect();
    bases.sort_by(|a, b| a.id.cmp(&b.id));
    bases
}

/// Add-on targets do not need to permit derived full mods. Makers separately
/// approve each add-on id before it can run.
pub fn installed_addon_targets(root: &Path) -> Vec<Manifest> {
    let mut targets = d2_modkit::resolve::list_installed(root)
        .into_iter()
        .filter_map(|i| i.manifest.ok())
        .filter(|m| m.kind == Kind::Full)
        .collect::<Vec<_>>();
    targets.sort_by(|a, b| a.id.cmp(&b.id));
    targets
}

pub fn select_base(manifest: &mut Manifest, base: Option<&Manifest>) -> Result<()> {
    manifest.base = None;
    manifest.attaches_to = None;
    if manifest.kind == Kind::Full {
        manifest.screen_only = false;
        if let Some(base) = base {
            manifest.base = Some(BaseRef {
                id: base.id.clone(),
                version: d2_modkit::VersionReq::parse(&format!("={}", base.version))
                    .map_err(anyhow::Error::msg)?,
            });
            manifest.check_builds_on(base).map_err(anyhow::Error::msg)?;
        }
    } else {
        manifest.allow_derived = false;
        manifest.approved_addons.clear();
        manifest.attaches_to = Some(base.map_or("d2gamma", |b| b.id.as_str()).to_owned());
        manifest.screen_only = base.is_none() || manifest.screen_only;
    }
    Ok(())
}

pub fn create_project(root: &Path, manifest: &Manifest, template: usize) -> Result<Project> {
    let (_, rules) = TEMPLATES.get(template).context("unknown template")?;
    Manifest::parse(&manifest.to_toml())?;
    if manifest.screen_only && template != 0 {
        bail!("Screen-only add-ons cannot change rules; choose Empty mod");
    }
    d2_modkit::layout::init_folder(root, manifest)?;
    if manifest.screen_only {
        fs::remove_file(root.join("files/rules.toml"))?;
    } else {
        fs::write(root.join("files/rules.toml"), rules)?;
    }
    let project = Project::open(root)?;
    d2_native::addons::check_source(&project.folder)?;
    Ok(project)
}

pub struct Project {
    pub folder: ModFolder,
    pub manifest: toml::Table,
    pub rules: String,
    pub dirty: bool,
}

impl Project {
    pub fn open(root: &Path) -> Result<Self> {
        let folder = ModFolder::scan(root)?;
        let manifest = folder.manifest_text.parse()?;
        let rules = match fs::read_to_string(root.join("files/rules.toml")) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            folder,
            manifest,
            rules,
            dirty: false,
        })
    }
    pub fn validate(&self) -> Result<Manifest> {
        Ok(Manifest::parse(&toml::to_string_pretty(&self.manifest)?)?)
    }
    pub fn save(&mut self) -> Result<()> {
        let manifest = self.validate()?;
        let _: toml::Table = self.rules.parse().context("rules.toml is not valid TOML")?;
        if manifest.screen_only && !self.rules.trim().is_empty() {
            bail!("Screen-only add-ons cannot contain rules");
        }
        fs::write(self.folder.root.join("mod.toml"), manifest.to_toml())?;
        if !manifest.screen_only {
            fs::write(self.folder.root.join("files/rules.toml"), &self.rules)?;
        }
        self.folder = ModFolder::scan(&self.folder.root)?;
        d2_native::addons::check_source(&self.folder)?;
        self.dirty = false;
        Ok(())
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub game: String,
    pub converter: String,
    pub native: String,
    pub installed: String,
    pub recent: Vec<PathBuf>,
}
impl Settings {
    pub fn path() -> PathBuf {
        std::env::var_os("APPDATA")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("D2Gamma-Mod-Studio/settings.toml")
    }
    pub fn load() -> Self {
        fs::read_to_string(Self::path())
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }
    pub fn save(&mut self) -> Result<()> {
        self.version = 1;
        let path = Self::path();
        fs::create_dir_all(path.parent().context("settings directory")?)?;
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ToolCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}
impl ToolCommand {
    pub fn check(game: &Path, root: &Path, native: &Path, report: &Path, installed: &Path) -> Self {
        let mut command = Self {
            program: game.into(),
            args: vec![
                "check-mod".into(),
                root.into(),
                "--native".into(),
                native.into(),
                "--out".into(),
                report.into(),
            ],
        };
        command.installed(installed);
        command
    }
    pub fn installed(&mut self, installed: &Path) {
        if !installed.as_os_str().is_empty() {
            self.args.extend(["--mods-dir".into(), installed.into()]);
        }
    }
    pub fn package_checked(
        game: &Path,
        root: &Path,
        native: &Path,
        output: &Path,
        report: &Path,
        installed: &Path,
    ) -> Self {
        let mut command = Self::package(game, root, native, output);
        command.args.extend(["--report".into(), report.into()]);
        command.installed(installed);
        command
    }
    /// Select the game command explicitly: install flags follow the mod selector.
    pub fn play(game: &Path, root: &Path) -> Self {
        Self {
            program: game.into(),
            args: vec!["play".into(), "--mod-dir".into(), root.as_os_str().into()],
        }
    }
    /// The game's `ai-arena`: the monstats row `options.class` against a test
    /// character, with the mod in `root`, its trace written to `out`.
    pub fn arena(
        game: &Path,
        root: &Path,
        native: &Path,
        options: &arena::ArenaOptions,
        ai: u16,
        out: &Path,
    ) -> Self {
        let mut args: Vec<OsString> = vec!["ai-arena".into()];
        for (flag, value) in [
            ("--class", options.class.to_string()),
            ("--ai", ai.to_string()),
            ("--seed", options.seed.to_string()),
            ("--ticks", options.ticks.to_string()),
            ("--distance", options.distance.to_string()),
            ("--level", options.level.to_string()),
        ] {
            args.extend([flag.into(), value.into()]);
        }
        args.extend([
            "--mod-dir".into(),
            root.into(),
            "--native".into(),
            native.into(),
            "--out".into(),
            out.into(),
        ]);
        Self {
            program: game.into(),
            args,
        }
    }
    pub fn package(converter: &Path, root: &Path, native: &Path, output: &Path) -> Self {
        Self {
            program: converter.into(),
            args: vec![
                "package-mod".into(),
                root.as_os_str().into(),
                "--base".into(),
                native.as_os_str().into(),
                "--out".into(),
                output.as_os_str().into(),
            ],
        }
    }
    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.program);
        c.args(&self.args);
        c
    }
}

/// Settings table from the author documentation, expanded for both unit classes.
pub fn rule_fields() -> Vec<(String, String, String)> {
    include_str!("../../../docs/modkit/rules.md")
        .lines()
        .filter_map(|line| {
            let parts: Vec<_> = line.split('|').map(str::trim).collect();
            if parts.len() != 5 || !parts[1].starts_with('`') {
                return None;
            }
            Some((
                parts[1].trim_matches('`').to_owned(),
                parts[2].trim_matches('`').to_owned(),
                parts[3].to_owned(),
            ))
        })
        .flat_map(|(key, default, help)| {
            if key.starts_with("target.") {
                vec![
                    (
                        key.replacen("target", "players", 1),
                        default.clone(),
                        help.clone(),
                    ),
                    (key.replacen("target", "monsters", 1), default, help),
                ]
            } else {
                vec![(key, default, help)]
            }
        })
        .collect()
}

pub mod asset_library;
pub mod definitions;
pub mod difficulty_ui;
pub mod effects_ui;
pub mod import;
pub mod import_ui;
pub mod input_script;
pub mod sprite_preview;

pub mod graph_testfire;
pub mod graphs;
pub mod graphs_ui;
