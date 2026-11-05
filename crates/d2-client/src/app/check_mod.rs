//! Headless mod checks over the same resolved layers as `--mod-dir`.
//! Errors reject content; scenario findings remain Unverified warnings. The
//! versioned JSON report is the subprocess boundary for Mod Studio.

mod content;
mod kit;
mod maps;
mod scripts;
mod skills;
mod targets;

use super::single_player::GameData;
use super::sweep;
use anyhow::{bail, Context, Result};
use d2_data::bin::DataRules;
use d2_modkit::{ModSpec, ModStack};
use d2_native::source::{AssetKind, NativeSource};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Severity is stable JSON vocabulary, ordered for the text report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub category: String,
    /// File, table/row, or reproducible scenario identity.
    pub location: String,
    pub message: String,
}

/// Persisted report schema, versioned independently of the modding API.
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub format_version: u32,
    pub target: String,
    pub exit_code: u8,
    pub findings: Vec<Finding>,
    pub completed: Vec<String>,
    /// Complete sweep reports, including generated skips and Unverified status.
    pub sweeps: BTreeMap<String, Value>,
    pub gameplay_scope: String,
    /// Effective text row indices changed or added relative to the base.
    pub changed_rows: BTreeMap<String, BTreeSet<usize>>,
}

impl Report {
    /// Represent publication/setup errors through the same report boundary.
    pub fn failure(target: String, message: impl ToString) -> Self {
        let mut report = Self::new(target);
        report.finding(Severity::Error, "publication", "mod.toml", message);
        report
    }
    fn new(target: String) -> Self {
        Self {
            format_version: 1,
            target,
            exit_code: 0,
            findings: Vec::new(),
            completed: Vec::new(),
            sweeps: BTreeMap::new(),
            gameplay_scope: "changed rows".into(),
            changed_rows: BTreeMap::new(),
        }
    }

    fn finding(
        &mut self,
        severity: Severity,
        category: &str,
        location: &str,
        message: impl ToString,
    ) {
        if severity == Severity::Error {
            self.exit_code = 1;
        }
        self.findings.push(Finding {
            severity,
            category: category.into(),
            location: location.into(),
            message: message.to_string(),
        });
    }

    /// Plain English grouped by severity and content category.
    pub fn text(&self) -> String {
        use std::fmt::Write;
        let mut out = format!("Mod check: {}\n", self.target);
        for severity in [Severity::Error, Severity::Warning, Severity::Info] {
            let mut groups = BTreeMap::<&str, Vec<&Finding>>::new();
            for finding in self.findings.iter().filter(|f| f.severity == severity) {
                groups.entry(&finding.category).or_default().push(finding);
            }
            if groups.is_empty() {
                continue;
            }
            let _ = writeln!(out, "{severity:?}:");
            for (category, findings) in groups {
                let _ = writeln!(out, "  {} {category}:", findings.len());
                for f in findings {
                    let _ = writeln!(out, "    {}: {}", f.location, f.message);
                }
            }
        }
        let _ = writeln!(out, "Completed: {}", self.completed.join(", "));
        let _ = writeln!(
            out,
            "{} (exit {})",
            if self.exit_code == 0 {
                "Passed"
            } else {
                "Failed"
            },
            self.exit_code
        );
        out
    }
}

/// Paths are explicit so Studio never has to activate a process-wide identity.
#[derive(Clone, Debug)]
pub struct Options {
    /// None checks D2Gamma's base alone.
    pub target: Option<PathBuf>,
    pub native: PathBuf,
    pub mods_dir: PathBuf,
    pub out: PathBuf,
    pub jobs: usize,
    /// Run every gameplay scenario instead of only changed-row scenarios.
    pub full: bool,
}

fn resolve(o: &Options) -> Result<Option<ModStack>> {
    let Some(target) = &o.target else {
        return Ok(None);
    };
    let loaded = d2_native::package::open_mod(target, d2_modkit::api::Date::today())?;
    if loaded.manifest.kind == d2_modkit::Kind::Full {
        return Ok(Some(d2_modkit::resolve::resolve_with(
            &ModSpec::Dir(loaded.dir),
            &o.mods_dir,
            d2_modkit::api::Date::today(),
            d2_native::package::open_mod,
        )?));
    }
    let mut stack = match loaded.manifest.attaches_to.as_deref() {
        Some("d2gamma") => ModStack { mods: Vec::new() },
        Some(id) => d2_modkit::resolve::resolve_with(
            &ModSpec::Id(id.into()),
            &o.mods_dir,
            d2_modkit::api::Date::today(),
            d2_native::package::open_mod,
        )?,
        None => bail!("add-on has no target"),
    };
    stack.mods.retain(|m| m.manifest.id != loaded.manifest.id);
    stack.mods.push(loaded);
    d2_modkit::addons::check_stack(&stack.mods.iter().map(|m| &m.manifest).collect::<Vec<_>>())
        .map_err(anyhow::Error::msg)?;
    Ok(Some(stack))
}

fn read_rules(stack: &ModStack, report: &mut Report) {
    for m in &stack.mods {
        let result = (|| -> Result<Option<Vec<u8>>> {
            if m.dir.is_dir() {
                match std::fs::read(m.dir.join("files/rules.toml")) {
                    Ok(bytes) => Ok(Some(bytes)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(e) => Err(e.into()),
                }
            } else {
                let pack = d2_native::pack::Pack::open(&m.dir)?;
                Ok(pack.read("files/rules.toml").transpose()?)
            }
        })()
        .and_then(|bytes| {
            if let Some(bytes) = bytes {
                d2_sim::rules::global::GlobalRules::from_toml(std::str::from_utf8(&bytes)?)?;
            }
            Ok(())
        });
        if let Err(e) = result {
            report.finding(
                Severity::Error,
                "invalid global rules files",
                &format!("{}/files/rules.toml", m.dir.display()),
                format!("{e:#}"),
            );
        }
    }
    report.completed.push("global rules parse".into());
}

fn assets(src: &NativeSource, report: &mut Report) -> Result<()> {
    let mut paths = BTreeSet::new();
    let mut files = 0usize;
    let mut overrides = 0usize;
    for layer in src.layers.layers.iter().skip(1) {
        for path in layer.files_under("")? {
            files += 1;
            overrides += usize::from(src.layers.layers[0].has(&path));
            let canon = path
                .strip_suffix(".toml")
                .or_else(|| path.strip_suffix(".tsv"))
                .or_else(|| path.strip_suffix(".pal"))
                .unwrap_or(&path);
            if AssetKind::of(canon).is_some_and(|kind| kind.primary(canon) == path) {
                paths.insert(canon.to_owned());
            }
        }
    }
    for path in paths {
        if let Some(Err(e)) = src.read_native(&path) {
            report.finding(Severity::Error, "broken assets", &path, e);
        }
    }
    report.finding(Severity::Info, "content summaries", "mod layers", format!("{files} native files, {overrides} override base paths; content-threshold policy is checked by the packager"));
    report.completed.push("asset decoding".into());
    Ok(())
}

// Content formulas and generators may expose a runtime invariant. Keep the
// partial report and other independent checks when an in-process sweep panics.
fn stage<T>(run: impl FnOnce() -> Result<T>) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("unknown panic");
        Err(anyhow::anyhow!("content check panicked: {message}"))
    })
}

/// Check without opening a window, audio device, or modifying saves/settings.
/// Validation errors live in the report. I/O preventing a report is Err (exit 2).
pub fn check(o: &Options) -> Result<Report> {
    if o.jobs == 0 {
        bail!("--jobs must be positive");
    }
    let mut report = Report::new(
        o.target
            .as_ref()
            .map_or_else(|| "D2Gamma".into(), |p| p.display().to_string()),
    );
    if o.full {
        report.gameplay_scope = "full".into();
    }
    let stack = match resolve(o) {
        Ok(s) => s,
        Err(e) => {
            report.finding(
                Severity::Error,
                "invalid manifests or API versions",
                "mod.toml",
                format!("{e:#}"),
            );
            return Ok(report);
        }
    };
    report
        .completed
        .push("manifest, API and dependency checks".into());
    let mut gameplay_rules = d2_sim::rules::Ruleset::Original;
    if let Some(stack) = &stack {
        read_rules(stack, &mut report);
        if let Some(rules) = kit::check(stack, &mut report) {
            gameplay_rules = rules;
        }
    }
    let entries = stack
        .as_ref()
        .map(ModStack::layer_entries)
        .unwrap_or_default();
    let source = match (|| -> Result<NativeSource> {
        let mut source = NativeSource::open_with(&o.native, Some(&[]))?;
        source.layers = d2_native::layers::LayerStack::for_check(&o.native, &entries)?;
        Ok(source)
    })() {
        Ok(s) => s,
        Err(e) => {
            report.finding(
                Severity::Error,
                "invalid native layers",
                &o.native.display().to_string(),
                e,
            );
            return Ok(report);
        }
    };
    if stack.as_ref().is_some_and(|s| {
        s.mods
            .iter()
            .all(|m| m.manifest.kind == d2_modkit::Kind::Addon)
    }) {
        report.finding(Severity::Warning, "activation approval", "mod.toml", "Authoring check only: plain D2Gamma activation still requires owner approval in plain-addons.toml; packaging grants no permission.");
    }
    report.completed.push("layer file checks".into());
    let mut scripts = None;
    if let Some(stack) = &stack {
        match scripts::check(&source.layers, &stack.picked().manifest.name) {
            Ok(host) => scripts = host,
            Err(error) => {
                let location = error
                    .split_once(": node ")
                    .map(|(file, rest)| {
                        format!(
                            "{file} · node {}",
                            rest.split(':').next().unwrap_or("document")
                        )
                    })
                    .unwrap_or_else(|| "files/scripts".into());
                report.finding(Severity::Error, "invalid scripts", &location, error)
            }
        }
        report.completed.push("sandboxed script load".into());
    }
    assets(&source, &mut report)?;
    let rules = if stack.as_ref().is_some_and(|s| {
        s.mods
            .iter()
            .any(|m| m.manifest.kind == d2_modkit::Kind::Full)
    }) {
        DataRules::Mod
    } else {
        DataRules::Original
    };
    // This is the runtime patch/compiler path, including K4 reference and
    // explicit asset/string checks. Preserve its table/row/column messages.
    match source.tables_with("eng", rules) {
        Ok(_) => {
            report
                .completed
                .push("table patches, schemas, references and asset references".into());
        }
        Err(e) => {
            report.finding(
                Severity::Error,
                "broken tables or references",
                "effective tables",
                e,
            );
            return Ok(report);
        }
    };
    if stack.is_some() {
        report.changed_rows = targets::changed_rows(&source)?;
    }
    let data = match (|| -> Result<GameData> {
        let files = d2_server::game_files::GameFiles::from_native_under(
            std::sync::Arc::new(source.clone()),
            rules,
        )
        .map_err(anyhow::Error::msg)?;
        Ok(GameData::Live(std::sync::Arc::new(
            super::single_player::LiveData::load(std::sync::Arc::new(files))?,
        )))
    })() {
        Ok(d) => d,
        Err(e) => {
            report.finding(
                Severity::Error,
                "game data load failures",
                "effective data",
                e,
            );
            return Ok(report);
        }
    };
    let GameData::Live(live) = &data;
    if let Err(e) = stage(|| maps::check(live, &source, &mut report)) {
        report.finding(
            Severity::Error,
            "map preflight failure",
            "files/levels.toml",
            format!("{e:#}"),
        );
    }
    if let Err(error) = crate::ui::data_panels::load(live.archives.source().as_ref()) {
        report.finding(Severity::Error, "invalid UI", "files/ui", error);
    }
    if let Err(error) = d2_server::ui_actions::UiActions::load(live.archives.source().as_ref())
        .and_then(|a| a.validate_scripts(scripts.as_ref()))
    {
        report.finding(Severity::Error, "invalid UI actions", "files/ui", error);
    }
    if let Err(error) =
        crate::world_view::mod_effects::ModEffects::load(live.archives.source().as_ref())
    {
        report.finding(Severity::Error, "invalid effects", "files/effects", error);
    }
    if let Err(error) = d2_server::mod_network::Network::load(live.archives.source().as_ref())
        .and_then(|n| n.validate_scripts(scripts.as_ref()))
    {
        report.finding(
            Severity::Error,
            "invalid network contract",
            "files/net.toml",
            error,
        );
    }
    report.completed.push("mod network contract".into());
    report.completed.push("mod UI documents".into());
    report.completed.push("mod effect recipes".into());
    if let Err(e) = stage(|| content::check(live, &mut report)) {
        report.finding(
            Severity::Error,
            "content sweep failures",
            "effective data",
            format!("{e:#}"),
        );
    }
    if let Err(e) = stage(|| skills::check(live, &mut report)) {
        report.finding(
            Severity::Error,
            "content sweep failures",
            "skills",
            format!("{e:#}"),
        );
    }
    if report.exit_code != 0 {
        report.finding(
            Severity::Info,
            "coverage limits",
            "scenario sweeps",
            "not run because effective data has errors; repair those errors and check again",
        );
        return Ok(report);
    }
    for family in [
        "items", "skills", "npcs", "areas", "panels", "anims", "doors",
    ] {
        let selected = if o.full {
            None
        } else {
            let names = match stage(|| {
                targets::select(live, family, &report.changed_rows, &gameplay_rules)
            }) {
                Ok(names) => names,
                Err(e) => {
                    report.finding(
                        Severity::Error,
                        "content sweep failures",
                        family,
                        format!("{e:#}"),
                    );
                    continue;
                }
            };
            if names.is_empty() {
                continue;
            }
            Some(names)
        };
        let result = stage(|| {
            sweep::run_selected(
                data.clone(),
                &sweep::Options {
                    family: family.into(),
                    jobs: o.jobs,
                    out: o.out.join(family),
                    ..Default::default()
                },
                selected.as_ref(),
                Some(gameplay_rules.clone()),
            )
        });
        match result {
            Ok(sweep) => {
                for case in sweep["cases"].as_array().into_iter().flatten() {
                    if case["skipped"].is_string() || case["exit"] == 0 {
                        continue;
                    }
                    let severity = if case["exit"] == 1 {
                        Severity::Warning
                    } else {
                        Severity::Error
                    };
                    report.finding(
                        severity,
                        &format!("{family} sweep findings (Unverified)"),
                        case["case"].as_str().unwrap_or("case"),
                        format!("{}; see {family}/sweep.json", case["failures"]),
                    );
                }
                report.completed.push(format!("{family} content sweep"));
                report.sweeps.insert(family.into(), sweep);
            }
            Err(e) => report.finding(
                Severity::Error,
                "content sweep failures",
                family,
                format!("{e:#}"),
            ),
        }
    }
    report.finding(Severity::Info, "gameplay coverage", "scenario selection",
        format!("{}; {} families selected; all data sweeps ran; rows without a compatible generated scenario receive data checks only; use --full for every gameplay scenario", report.gameplay_scope, report.sweeps.len()));
    Ok(report)
}

fn parse(args: &[String]) -> Result<Options> {
    let mut o = Options {
        target: None,
        native: PathBuf::new(),
        mods_dir: crate::user_dir::mods_dir(),
        out: "check-mod-out".into(),
        full: false,
        jobs: std::thread::available_parallelism().map_or(1, |n| n.get().min(4)),
    };
    let mut it = args.iter();
    let target = it.next().context("check-mod <mod folder | pack | --base> --native DIR [--mods-dir DIR] [--out DIR] [--jobs N] [--full]")?;
    if target != "--base" {
        o.target = Some(target.into());
    }
    while let Some(flag) = it.next() {
        if flag == "--full" {
            o.full = true;
            continue;
        }
        let value = it.next().with_context(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--native" => o.native = value.into(),
            "--mods-dir" => o.mods_dir = value.into(),
            "--out" => o.out = value.into(),
            "--jobs" => o.jobs = value.parse().context("--jobs must be positive")?,
            _ => bail!("unknown check-mod option {flag}"),
        }
    }
    let native_arg = (!o.native.as_os_str().is_empty()).then_some(o.native.as_path());
    let crate::assets::SourceChoice::Native(native) = crate::assets::choose_source(
        native_arg,
        Some("native"),
        |key| std::env::var(key).ok(),
        crate::assets::default_native_dir(),
    )
    .map_err(anyhow::Error::msg)?
    else {
        bail!("check-mod requires a converted native install");
    };
    o.native = native;
    if o.jobs == 0 {
        bail!("--jobs must be positive");
    }
    Ok(o)
}

/// Write both views of the same check result.
pub fn write_report(report: &Report, out: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join("report.json"), serde_json::to_vec_pretty(report)?)?;
    std::fs::write(out.join("report.txt"), report.text())?;
    Ok(())
}

/// CLI exit codes: 0 passes, 1 content errors, 2 invocation/report I/O errors.
pub fn main(args: &[String]) -> u8 {
    let result = (|| -> Result<u8> {
        let o = parse(args)?;
        let report = check(&o)?;
        write_report(&report, &o.out)?;
        println!("{}", report.text());
        Ok(report.exit_code)
    })();
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("check-mod: {e:#}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_is_an_optional_valueless_flag() {
        let args = [
            "--base",
            "--full",
            "--native",
            "unused-native",
            "--jobs",
            "2",
        ]
        .map(str::to_owned);
        let options = parse(&args).unwrap();
        assert!(options.full);
        assert_eq!(options.jobs, 2);
        assert!(
            !parse(&["--base".into(), "--native".into(), "unused-native".into()])
                .unwrap()
                .full
        );
    }

    #[test]
    fn panicking_content_retains_a_serializable_error_report() {
        let mut report = Report::new("synthetic".into());
        report.completed.push("table references".into());
        let error = stage::<()>(|| panic!("invalid content invariant")).unwrap_err();
        report.finding(Severity::Error, "content sweep failures", "skills", error);
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["exit_code"], 1);
        assert_eq!(json["completed"][0], "table references");
        assert!(report.text().contains("invalid content invariant"));
    }
}
