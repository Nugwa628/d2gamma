//! `mp-test`: runs multiplayer scenarios against a server process.
//!
//! ```text
//! mp-test [--server synth] [--server-bin PATH] [--game-dir DIR]
//!         [--scenarios DIR|FILE].. [--filter TEXT] [--out DIR] [--work DIR]
//!         [--seed N] [--list] [--verbs]
//! mp-test --setup split|split:N|host [--realm d2rust-realm|synth]
//!         [--realm-bin PATH] [--node d2rust-node|stub] [--node-bin PATH] ...
//! ```
//!
//! With `--setup` (alias `--topology`) the tool starts a v2 setup (plan
//! N1): the main server and each game server as its own process. The
//! default scenarios are then `tools/mp-test/scenarios/v2`; the default
//! programs `d2rust-realm` and `d2rust-node`, the binaries next to this one.
//! Without it the server is the stand-in (`mp-test-synth`, one process);
//! the real servers always run as a setup: the v1 scenarios on `split:1`
//! (`--setup split:1 --scenarios tools/mp-test/scenarios`, plan N5).
//!
//! Exit code 0 when no rule broke (pending scenarios don't count), 1 when
//! one did, 2 for a usage error or a scenario that doesn't parse.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mp_test::report::{Report, ScenarioReport, Verdict};
use mp_test::runner::{self, Probe};
use mp_test::scenario::{self, Scenario};
use mp_test::server::Target;
use mp_test::topology::{NodeProgram, Nodes, Setup};
use mp_test::verbs;

const USAGE: &str = "usage: mp-test [--server synth] [--server-bin PATH] [--game-dir DIR] \
[--scenarios DIR|FILE].. [--filter TEXT] [--out DIR] [--work DIR] [--seed N] [--list] [--verbs] \
[--setup split|split:N|host] [--realm d2rust-realm|synth] [--realm-bin PATH] \
[--node d2rust-node|stub] [--node-bin PATH]";

struct Args {
    server: String,
    server_bin: Option<PathBuf>,
    game_dir: Option<PathBuf>,
    scenarios: Vec<PathBuf>,
    filter: Option<String>,
    out: PathBuf,
    work: Option<PathBuf>,
    seed: u64,
    list: bool,
    verbs: bool,
    setup: Option<Setup>,
    realm: Option<String>,
    realm_bin: Option<PathBuf>,
    node: Option<String>,
    node_bin: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        server: "synth".into(),
        server_bin: None,
        game_dir: std::env::var_os("D2_GAME_DIR").map(PathBuf::from),
        scenarios: Vec::new(),
        filter: None,
        out: PathBuf::from("target/mp-test"),
        work: None,
        seed: 1,
        list: false,
        verbs: false,
        setup: None,
        realm: None,
        realm_bin: None,
        node: None,
        node_bin: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut val = || it.next().ok_or_else(|| format!("{k} takes a value"));
        match k.as_str() {
            "--server" => a.server = val()?,
            "--server-bin" => a.server_bin = Some(PathBuf::from(val()?)),
            "--game-dir" => a.game_dir = Some(PathBuf::from(val()?)),
            "--scenarios" => a.scenarios.push(PathBuf::from(val()?)),
            "--filter" => a.filter = Some(val()?),
            "--out" => a.out = PathBuf::from(val()?),
            // Absolute: the servers run in folders of their own, and a
            // setup's config paths must not depend on where they run.
            "--work" => {
                let dir = PathBuf::from(val()?);
                a.work = Some(
                    std::path::absolute(&dir)
                        .map_err(|e| format!("--work {}: {e}", dir.display()))?,
                );
            }
            "--seed" => {
                a.seed = val()?
                    .parse()
                    .map_err(|_| "--seed takes a number".to_string())?
            }
            "--setup" | "--topology" => a.setup = Some(Setup::parse(&val()?)?),
            "--realm" => a.realm = Some(val()?),
            "--realm-bin" => a.realm_bin = Some(PathBuf::from(val()?)),
            "--node" => a.node = Some(val()?),
            "--node-bin" => a.node_bin = Some(PathBuf::from(val()?)),
            "--list" => a.list = true,
            "--verbs" => a.verbs = true,
            _ => return Err(format!("unknown argument {k}")),
        }
    }
    if a.setup.is_none() && (a.realm.is_some() || a.node.is_some()) {
        return Err("--realm and --node name a v2 setup's programs: give --setup too".into());
    }
    if a.scenarios.is_empty() {
        let dir = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tools/mp-test/scenarios"
        ));
        a.scenarios.push(if a.setup.is_some() {
            dir.join("v2")
        } else {
            dir
        });
    }
    Ok(a)
}

/// The binary next to this one (`target/<profile>/NAME`).
fn sibling(name: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let mut p = exe.with_file_name(name);
    if cfg!(windows) {
        p.set_extension("exe");
    }
    p
}

fn target(a: &Args) -> Result<Target, String> {
    let Some(setup) = a.setup else {
        return v1_target(a);
    };
    let game_dir = || {
        a.game_dir
            .clone()
            .ok_or("d2rust-realm and d2rust-node need --game-dir or D2_GAME_DIR")
    };
    let realm_name = a.realm.as_deref().unwrap_or("d2rust-realm");
    let realm_bin = a.realm_bin.clone();
    let realm = match realm_name {
        "d2rust-realm" => Target::D2rustRealm {
            bin: realm_bin.unwrap_or_else(|| sibling("d2rust-realm")),
            game_dir: game_dir()?,
        },
        "synth" => Target::Synth {
            bin: realm_bin.unwrap_or_else(|| sibling("mp-test-synth")),
        },
        r => return Err(format!("unknown realm {r} (d2rust-realm, synth)")),
    };
    let program = match a.node.as_deref().unwrap_or("d2rust-node") {
        "d2rust-node" => NodeProgram::D2rustNode {
            bin: a.node_bin.clone().unwrap_or_else(|| sibling("d2rust-node")),
            game_dir: game_dir()?,
        },
        "stub" => NodeProgram::Stub {
            bin: a
                .node_bin
                .clone()
                .unwrap_or_else(|| sibling("mp-test-stub-node")),
        },
        n => return Err(format!("unknown node {n} (d2rust-node, stub)")),
    };
    Ok(Target::Topology {
        realm: Box::new(realm),
        nodes: Nodes { program, setup },
    })
}

fn v1_target(a: &Args) -> Result<Target, String> {
    match a.server.as_str() {
        "synth" => Ok(Target::Synth {
            bin: a
                .server_bin
                .clone()
                .unwrap_or_else(|| sibling("mp-test-synth")),
        }),
        "d2rust-server" | "d2rust-realm" => Err(
            "the real servers run as a setup: --setup split:1 (d2rust-realm and d2rust-node; \
             --scenarios tools/mp-test/scenarios for the v1 scenarios)"
                .into(),
        ),
        s => Err(format!(
            "unknown server {s} (synth; the real servers: --setup)"
        )),
    }
}

/// Every `.mps` file under the given paths, sorted.
fn scenario_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            let rd = std::fs::read_dir(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let mut files: Vec<PathBuf> = rd
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|f| f.extension().is_some_and(|x| x == "mps"))
                .collect();
            files.sort();
            out.extend(files);
        } else {
            out.push(p.clone());
        }
    }
    Ok(out)
}

fn main() -> ExitCode {
    let a = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("mp-test: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if a.verbs {
        for v in verbs::VERBS
            .iter()
            .chain(verbs::TOPO_VERBS)
            .chain(verbs::MODIFIERS)
        {
            let state = if v.built { "built" } else { "pending" };
            println!("{:<14} {:<12} {:<8} {}", v.name, v.cap, state, v.about);
        }
        return ExitCode::SUCCESS;
    }
    let files = match scenario_files(&a.scenarios) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("mp-test: {e}");
            return ExitCode::from(2);
        }
    };
    let mut scenarios: Vec<Scenario> = Vec::new();
    let mut bad = false;
    for f in &files {
        match scenario::load(f)
            .map_err(|e| e.to_string())
            .and_then(|s| runner::validate(&s).map(|()| s))
        {
            Ok(s) => {
                if a.filter
                    .as_ref()
                    .is_none_or(|t| s.name.contains(t.as_str()))
                {
                    scenarios.push(s);
                }
            }
            Err(e) => {
                eprintln!("mp-test: {e}");
                bad = true;
            }
        }
    }
    if bad {
        return ExitCode::from(2);
    }
    let t = match target(&a) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("mp-test: {e}");
            return ExitCode::from(2);
        }
    };
    let work = a
        .work
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join(format!("mp-test-{}", std::process::id())));
    let probe = runner::probe(&t, work.join("probe"));
    let mut probes = Probes {
        base: probe.clone(),
        by: BTreeMap::from([(Vec::new(), probe.clone())]),
    };
    if a.list {
        for s in &scenarios {
            let state = runner::pending_on(s, &t, &probes.get(&t, &work, s))
                .map_or("runs".into(), |p| format!("pending: {p}"));
            println!("{:<28} {:>2} bots  {}", s.name, s.bots, state);
        }
        return ExitCode::SUCCESS;
    }
    eprintln!("mp-test: server {}: {probe:?}", t.describe());
    let mut reports = Vec::new();
    for (i, s) in scenarios.iter().enumerate() {
        let probe = probes.get(&t, &work, s);
        let r = match runner::pending_on(s, &t, &probe) {
            Some(p) => ScenarioReport {
                name: s.name.clone(),
                file: s.file.display().to_string(),
                about: s.about.clone(),
                verdict: Verdict::Pending,
                pending: Some(p),
                needs: runner::needs_of(s),
                bots: s.bots,
                steps_run: 0,
                ms: 0,
                failures: Vec::new(),
                datagrams: (0, 0),
                server_tail: None,
            },
            None => runner::run(s, &t, work.join(format!("{i:02}-{}", s.name)), a.seed),
        };
        let mark = match r.verdict {
            Verdict::Pass => "pass",
            Verdict::Fail => "FAIL",
            Verdict::Pending => "pending",
        };
        eprintln!(
            "mp-test: {:<28} {mark} ({:.1} s)",
            r.name,
            r.ms as f64 / 1000.0
        );
        for f in &r.failures {
            eprintln!(
                "    broken {}: {} (line {}: expected {}, got {})",
                f.rule.id, f.rule.text, f.line, f.expected, f.actual
            );
        }
        reports.push(r);
    }
    let count = |v: Verdict| reports.iter().filter(|r| r.verdict == v).count();
    let report = Report {
        tool: format!("mp-test {}", env!("CARGO_PKG_VERSION")),
        server: t.name().into(),
        server_bin: t.bin().display().to_string(),
        server_caps: match &probe {
            Probe::Serving(Some(c)) => Some(c.iter().cloned().collect()),
            _ => None,
        },
        seed: a.seed,
        passed: count(Verdict::Pass),
        failed: count(Verdict::Fail),
        pending: count(Verdict::Pending),
        scenarios: reports,
    };
    if let Err(e) = report.write(&a.out) {
        eprintln!("mp-test: writing the report to {}: {e}", a.out.display());
        return ExitCode::from(2);
    }
    if report.failed == 0 && a.work.is_none() {
        let _ = std::fs::remove_dir_all(&work);
    }
    eprintln!(
        "mp-test: {} passed, {} failed, {} pending; report in {}",
        report.passed,
        report.failed,
        report.pending,
        Path::new(&a.out).join("mp-test.md").display()
    );
    if report.failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// The server probed once per settings: a scenario that starts it with
/// its own settings is checked against the server started that way.
struct Probes {
    /// Started with no settings.
    base: Probe,
    /// By settings.
    by: BTreeMap<Vec<(String, String)>, Probe>,
}

impl Probes {
    fn get(&mut self, t: &Target, work: &Path, s: &Scenario) -> Probe {
        if !matches!(self.base, Probe::Serving(_)) {
            return self.base.clone();
        }
        let n = self.by.len();
        let base = &self.base;
        self.by
            .entry(s.server.clone())
            .or_insert_with(|| {
                // A server that doesn't start with these settings (the
                // stand-in takes few) is judged as started without them;
                // the run itself then reports the start.
                match runner::probe_with(t, work.join(format!("probe-{n}")), s.server.clone()) {
                    p @ Probe::Serving(_) => p,
                    _ => base.clone(),
                }
            })
            .clone()
    }
}
