//! Publication runs the complete mod checker on the packager's staged bytes.
use super::check_mod::{self, Options};
use anyhow::{bail, Context, Result};
use std::path::PathBuf;

pub fn main(args: &[String]) -> u8 {
    match run(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("package-mod: {e:#}");
            1
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    let mut args = args.iter();
    let folder = PathBuf::from(args.next().context(
        "package-mod FOLDER --base NATIVE [--out PACK] [--report DIR] [--mods-dir DIR]",
    )?);
    let mut native = None;
    let mut output = None;
    let mut report_dir = PathBuf::from("package-mod-report");
    let mut mods_dir = crate::user_dir::mods_dir();
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .with_context(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--base" => native = Some(PathBuf::from(value)),
            "--out" => output = Some(PathBuf::from(value)),
            "--report" => report_dir = value.into(),
            "--mods-dir" => mods_dir = value.into(),
            _ => bail!("unknown package-mod option {flag}"),
        }
    }
    let native = native.context("package-mod needs --base NATIVE")?;
    let output = match output {
        Some(output) => output,
        None => match d2_modkit::ModFolder::scan(&folder) {
            Ok(project) => format!(
                "{}-{}.d2pack",
                project.manifest.id, project.manifest.version
            )
            .into(),
            Err(error) => {
                let report = check_mod::Report::failure(folder.display().to_string(), &error);
                check_mod::write_report(&report, &report_dir)?;
                println!("{}", report.text());
                return Err(error.into());
            }
        },
    };
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .context("cannot find your user folder for the author signing key")?;
    let key = PathBuf::from(home).join(".d2gamma/author-key.pk8");
    package_checked(&folder, &native, &output, &key, mods_dir, report_dir)?;
    println!("Signed pack: {}", output.display());
    Ok(())
}

/// Check and publish the exact staged snapshot, with explicit paths for callers.
pub fn package_checked(
    folder: &std::path::Path,
    native: &std::path::Path,
    output: &std::path::Path,
    key: &std::path::Path,
    mods_dir: PathBuf,
    report_dir: PathBuf,
) -> Result<()> {
    println!("Checking snapshot before packaging…");
    // Reused CLI report destinations must never expose a previous success.
    std::fs::create_dir_all(&report_dir)?;
    let report_path = report_dir.join("report.json");
    if report_path.exists() {
        std::fs::remove_file(&report_path)?;
    }
    let result =
        d2_native::package::package_with_checks(folder, native, output, key, |snapshot, base| {
            let result = (|| -> Result<()> {
                let report = check_mod::check(&Options {
                    target: Some(snapshot.into()),
                    native: base.into(),
                    mods_dir,
                    out: report_dir.clone(),
                    jobs: 4,
                    full: false,
                })?;
                check_mod::write_report(&report, &report_dir)?;
                println!("{}", report.text());
                ensure_passed(&report)
            })();
            result.map_err(|e| format!("{e:#}"))
        });
    if let Err(error) = result {
        let previous = std::fs::read(&report_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<check_mod::Report>(&bytes).ok());
        if previous.as_ref().is_none_or(|report| report.exit_code == 0) {
            let mut report = previous.unwrap_or_else(|| {
                check_mod::Report::failure(folder.display().to_string(), &error)
            });
            if report.exit_code == 0 {
                report.exit_code = 1;
                report.findings.push(check_mod::Finding {
                    severity: check_mod::Severity::Error,
                    category: "publication".into(),
                    location: "mod.toml".into(),
                    message: error.clone(),
                });
            }
            check_mod::write_report(&report, &report_dir)?;
            println!("{}", report.text());
        }
        bail!("{error}");
    }
    Ok(())
}

fn ensure_passed(report: &check_mod::Report) -> Result<()> {
    if report.exit_code != 0
        || report
            .findings
            .iter()
            .any(|f| f.severity == check_mod::Severity::Error)
    {
        bail!("check-mod refused publication; see the check report");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn errors_refuse_publication_and_warnings_pass() {
        let mut report: check_mod::Report = serde_json::from_value(serde_json::json!({
            "format_version":1,"target":"synthetic","exit_code":0,"findings":[],
            "completed":[],"sweeps":{},"gameplay_scope":"changed rows","changed_rows":{}
        }))
        .unwrap();
        assert!(ensure_passed(&report).is_ok());
        report.findings.push(check_mod::Finding {
            severity: check_mod::Severity::Warning,
            category: "synthetic".into(),
            location: "items row 0".into(),
            message: "review".into(),
        });
        assert!(ensure_passed(&report).is_ok());
        report.findings[0].severity = check_mod::Severity::Error;
        assert!(ensure_passed(&report).is_err());
        report.findings.clear();
        report.exit_code = 2;
        assert!(ensure_passed(&report).is_err());
    }
}
