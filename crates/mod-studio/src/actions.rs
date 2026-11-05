//! Headless tool jobs and the versioned JSON boundary used by the problems panel.
use crate::ToolCommand;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Problem {
    pub severity: Severity,
    pub category: String,
    pub location: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Report {
    pub format_version: u32,
    pub target: String,
    pub exit_code: u8,
    pub findings: Vec<Problem>,
    pub completed: Vec<String>,
}
impl Report {
    pub fn parse(json: &str) -> Result<Self> {
        let mut report: Self = serde_json::from_str(json).context("Invalid check report JSON")?;
        if report.format_version != 1 {
            bail!("Unsupported check report version {}", report.format_version);
        }
        if report
            .findings
            .iter()
            .any(|p| p.severity == Severity::Error)
        {
            report.exit_code = report.exit_code.max(1);
        }
        // Native loading joins patch diagnostics into one error string.
        // Give each diagnostic its own clickable row while retaining its text.
        report.findings = report
            .findings
            .into_iter()
            .flat_map(|problem| {
                problem
                    .message
                    .split("; error[")
                    .enumerate()
                    .map(|(i, text)| {
                        let mut p = problem.clone();
                        p.message = if i == 0 {
                            text.into()
                        } else {
                            format!("error[{text}")
                        };
                        p
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        report.findings.sort_by_key(|p| p.severity);
        Ok(report)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Jump {
    Graph {
        path: PathBuf,
        node: String,
    },
    Item {
        index: usize,
    },
    Key {
        table: String,
        key: String,
    },
    Table {
        table: String,
        row: usize,
        column: Option<String>,
    },
    File {
        path: PathBuf,
        line: Option<usize>,
    },
}

impl Problem {
    /// Recognizes the checker's row locations and native patch diagnostics.
    /// Unknown scenario identities stay visible without inventing a target.
    pub fn jump(&self, report_target: &Path, project: &Path) -> Option<Jump> {
        let location = self.location.trim();
        if let Some((file, node)) = location.split_once(" · node ") {
            let relative = Path::new(file);
            if file.ends_with(".graph.toml")
                && relative.starts_with("files/scripts")
                && relative
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
                && d2_modkit::graph::identifier(node)
            {
                let path = project.join(relative);
                if path.is_file() {
                    return Some(Jump::Graph {
                        path,
                        node: node.into(),
                    });
                }
            }
        }
        if let Some(rest) = location.strip_prefix("item ") {
            if let Some(index) = rest.split_whitespace().next().and_then(|s| s.parse().ok()) {
                return Some(Jump::Item { index });
            }
        }
        if let Some(rest) = location.strip_prefix("treasure class ") {
            if let Some((_, name)) = rest.split_once(' ') {
                return Some(Jump::Key {
                    table: "treasureclassex".into(),
                    key: name.into(),
                });
            }
        }
        let parts: Vec<_> = location.split('-').collect();
        let number = |n: usize| parts.get(n).and_then(|s| s.parse::<usize>().ok());
        if parts.first() == Some(&"item") {
            if let Some(index) = number(1) {
                return Some(Jump::Item { index });
            }
        }
        let scenario = match parts.first().copied() {
            Some("recipe") => number(1).map(|row| ("cubemain", row)),
            Some("npc" | "vendor" | "monster") => number(1).map(|row| ("monstats", row)),
            Some("area" | "link" | "level") => number(1).map(|row| ("levels", row)),
            Some("waypoint") => number(2).map(|row| ("levels", row)),
            Some("class") if matches!(parts.get(2), Some(&"skill" | &"cast")) => {
                number(3).map(|row| ("skills", row))
            }
            Some("class") => number(1).map(|row| ("charstats", row)),
            _ => None,
        };
        if let Some((table, row)) = scenario {
            return Some(Jump::Table {
                table: table.into(),
                row,
                column: None,
            });
        }
        let schemas = crate::tables::schemas();
        for text in [&self.location, &self.message] {
            let words: Vec<_> = text.split_whitespace().collect();
            for (i, word) in words.iter().enumerate() {
                let name = word
                    .trim_matches(|c: char| !c.is_ascii_alphanumeric())
                    .trim_end_matches(".txt");
                if !schemas.contains_key(name) {
                    continue;
                }
                let tail = &words[i + 1..];
                if tail.first() == Some(&"row") {
                    let row = tail
                        .get(1)?
                        .trim_matches(|c: char| !c.is_ascii_digit())
                        .parse()
                        .ok()?;
                    let column = text
                        .split_once("column `")
                        .and_then(|(_, s)| s.split_once('`'))
                        .map(|(name, _)| name.to_owned())
                        .or_else(|| {
                            tail.iter()
                                .position(|w| *w == "column")
                                .and_then(|n| tail.get(n + 1))
                                .map(|s| s.trim_matches([':', '\'', '"', '`']).to_owned())
                        });
                    return Some(Jump::Table {
                        table: name.into(),
                        row,
                        column,
                    });
                }
                if let Some((r, token)) = tail.iter().enumerate().find(|(_, w)| w.starts_with("(#"))
                {
                    let row = token
                        .trim_start_matches("(#")
                        .trim_end_matches(')')
                        .parse()
                        .ok()?;
                    let column = tail
                        .get(r + 1..)
                        .map(|rest| rest.join(" "))
                        .and_then(|rest| {
                            rest.split_once(':').map(|(name, _)| name.trim().to_owned())
                        })
                        .filter(|s| !s.is_empty());
                    return Some(Jump::Table {
                        table: name.into(),
                        row,
                        column,
                    });
                }
            }
        }
        // A syntax diagnostic may only have a patch file and line, with no row.
        if let Some((_, diagnostic)) = self.message.split_once("] ") {
            let mut fields = diagnostic.splitn(4, ':');
            if let (Some(path), Some(line), Some(_), Some(_)) =
                (fields.next(), fields.next(), fields.next(), fields.next())
            {
                if let Ok(line) = line.parse() {
                    let path = path.split_once("/files/").map_or(path, |(_, p)| p);
                    if Path::new(path)
                        .components()
                        .any(|c| !matches!(c, std::path::Component::Normal(_)))
                    {
                        return None;
                    }
                    let path = if path.starts_with("patches/") {
                        project.join("files").join(path)
                    } else {
                        project.join(path)
                    };
                    if path.is_file() && path.starts_with(project) {
                        return Some(Jump::File {
                            path,
                            line: Some(line),
                        });
                    }
                }
            }
        }
        let location = self.location.replace('\\', "/");
        let location = location.strip_prefix("//?/").unwrap_or(&location);
        let (location, line) = location
            .rsplit_once(':')
            .and_then(|(p, n)| n.parse::<usize>().ok().map(|n| (p, Some(n))))
            .unwrap_or((location, None));
        let source = Path::new(location);
        let target = report_target.to_string_lossy().replace('\\', "/");
        let target = target.strip_prefix("//?/").unwrap_or(&target);
        let relative = source.strip_prefix(Path::new(target)).unwrap_or(source);
        // File locations are project-relative (native assets omit `files/`).
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return None;
        }
        let relative = if relative.starts_with("data") {
            Path::new("files").join(relative)
        } else {
            relative.into()
        };
        let path = project.join(relative);
        (path.is_file()).then_some(Jump::File { path, line })
    }
}

pub enum Event {
    Output(String),
    Finished {
        result: Result<(), String>,
        report: Option<(Report, String)>,
    },
}

pub struct Job {
    pub events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
    pub started: Instant,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl Job {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn start(
        make: impl FnOnce(&Path) -> ToolCommand + Send + 'static,
        packaged: Option<PathBuf>,
    ) -> Self {
        let (tx, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        std::thread::spawn(move || {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "studio-job-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let result = (|| -> Result<()> {
                fs::create_dir(&dir)?;
                let log = fs::File::create(dir.join("tool.log"))?;
                let mut command = make(&dir).command();
                let temporary = dir.join("tmp");
                fs::create_dir(&temporary)?;
                // Staged snapshots also disappear when a cancelled process is killed.
                command
                    .env("TMPDIR", &temporary)
                    .env("TMP", &temporary)
                    .env("TEMP", &temporary);
                command
                    .stdout(Stdio::from(log.try_clone()?))
                    .stderr(Stdio::from(log));
                hide_console(&mut command);
                let mut child = command
                    .spawn()
                    .context("Could not start configured game executable")?;
                let mut previous = String::new();
                loop {
                    if flag.load(Ordering::Relaxed) {
                        let _ = child.kill();
                        let _ = child.wait();
                        bail!("Cancelled");
                    }
                    let output = log_tail(&dir.join("tool.log"));
                    if output != previous {
                        let _ = tx.send(Event::Output(output.clone()));
                        previous = output;
                    }
                    if let Some(status) = child.try_wait()? {
                        let _ = tx.send(Event::Output(log_tail(&dir.join("tool.log"))));
                        if !status.success() {
                            bail!("Tool failed ({status}); see problems and output");
                        }
                        if let Some(path) = &packaged {
                            if !path.is_file() {
                                bail!("Packager succeeded without writing {}", path.display());
                            }
                        }
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            })();
            let mut result = result.map_err(|e| format!("{e:#}"));
            let report = if flag.load(Ordering::Relaxed) {
                None
            } else {
                match fs::read_to_string(dir.join("report.json")) {
                    Ok(json) => match Report::parse(&json) {
                        Ok(report) => {
                            if report.exit_code != 0 {
                                result =
                                    Err("Check failed; publication refused. See Problems.".into());
                            }
                            Some((report, json))
                        }
                        Err(e) => {
                            result = Err(e.to_string());
                            None
                        }
                    },
                    Err(e) => {
                        if result.is_ok() {
                            result = Err(format!("Tool produced no check report: {e}"));
                        }
                        None
                    }
                }
            };
            let _ = tx.send(Event::Finished { result, report });
            let _ = fs::remove_dir_all(&dir);
        });
        Self {
            events,
            cancel,
            started: Instant::now(),
        }
    }
}

fn log_tail(path: &Path) -> String {
    (|| -> std::io::Result<String> {
        let mut file = fs::File::open(path)?;
        let length = file.metadata()?.len();
        file.seek(SeekFrom::Start(length.saturating_sub(65536)))?;
        let mut bytes = Vec::new();
        file.take(65536).read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })()
    .unwrap_or_default()
}

pub fn hide_console(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    #[cfg(not(windows))]
    let _ = command;
}
