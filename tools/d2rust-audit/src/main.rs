//! `d2rust-audit`: the owner's audit tool on the main server's data folder
//! (plan `multiplayer-v2` N20). It lists audits, shows their reports and
//! findings, queues audits, applies, declines and undoes rollbacks, holds
//! and prunes journals, and prints the determinism watch.
//!
//! ```text
//! d2rust-audit [--data DIR] [--config FILE] COMMAND
//! ```
//!
//! `DIR` is the main server's data folder (default `data`): the audit
//! database `DIR/audits.sqlite`, the reports `DIR/audits/<game>.md` and the
//! character folder `DIR/characters/<account>/`. `FILE` is the main server's
//! config, for its audit keys. Rollbacks write character files: run them
//! while the main server is stopped, or from the main server itself.

mod chars;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use d2_audit::{AuditConfig, AuditState, Audits, FindingState, GameKind};

const USAGE: &str = "\
usage: d2rust-audit [--data DIR] [--config FILE] COMMAND

  list [STATE]          audits, newest first (STATE: queued, running, passed, failed, error)
  show AUDIT            an audit's report
  game GAME             the latest audit of a game, with its seats
  queue GAME            queue an audit of a game (on demand)
  run [AUDIT]           run an audit (the next one when none is named)
  findings              all findings
  rollbacks             all rollbacks
  rollback ID           apply a proposed rollback
  confirm AUDIT         apply every proposed rollback of an audit
  decline ID            decline a proposed rollback
  undo ID [--force]     undo an applied rollback (--force: even if the character changed since)
  reports               player reports
  watch                 the determinism watch (official games re-run and matched)
  hold GAME             keep a game's journal past its window
  release GAME          release the hold
  prune                 delete journals past their window that nothing holds
";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn num<T: std::str::FromStr>(what: &str, v: Option<&String>) -> Result<T, String> {
    let v = v.ok_or_else(|| format!("{what} missing\n\n{USAGE}"))?;
    v.parse()
        .map_err(|_| format!("{what}: not a number: `{v}`"))
}

fn kind(k: GameKind) -> &'static str {
    match k {
        GameKind::Official => "official",
        GameKind::PlayerHosted => "hosted",
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let mut data = PathBuf::from("data");
    let mut config: Option<PathBuf> = None;
    let mut rest = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--data" => data = it.next().ok_or("--data needs a folder")?.into(),
            "--config" => config = Some(it.next().ok_or("--config needs a file")?.into()),
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            _ => rest.push(a.clone()),
        }
    }
    let cfg = match &config {
        Some(p) => AuditConfig::parse(
            &std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        )
        .map_err(|e| format!("{}: {e}", p.display()))?,
        None => AuditConfig::default(),
    };
    let Some(cmd) = rest.first() else {
        return Err(USAGE.into());
    };
    let arg = rest.get(1);
    std::fs::create_dir_all(&data).map_err(|e| format!("{}: {e}", data.display()))?;
    let audits = Audits::open(&data.join("audits.sqlite"), &data.join("audits"), cfg)
        .map_err(|e| e.to_string())?;
    let store = chars::Folder::new(data.join("characters"));
    let now = now_ms();
    let e = |e: d2_audit::AuditError| e.to_string();
    match cmd.as_str() {
        "list" => {
            let state = match arg {
                Some(s) => Some(AuditState::parse(s).ok_or(format!("unknown state `{s}`"))?),
                None => None,
            };
            println!("audit  game        where     state    reasons");
            for a in audits.audits(state).map_err(e)? {
                let reasons: Vec<&str> = a.reasons.iter().map(|r| r.0.name()).collect();
                println!(
                    "{:<6} {:<11} {:<9} {:<8} {}",
                    a.id,
                    a.game,
                    kind(a.kind),
                    a.state.name(),
                    reasons.join(", ")
                );
            }
        }
        "show" => {
            let id = num("AUDIT", arg)?;
            let path = audits.write_report(id).map_err(e)?;
            let text =
                std::fs::read_to_string(&path).map_err(|x| format!("{}: {x}", path.display()))?;
            print!("{text}");
        }
        "game" => {
            let g = num("GAME", arg)?;
            let game = audits.game(g).map_err(e)?;
            println!(
                "game {} on {} ({}), realm {}, journal {}{}",
                game.id,
                game.node,
                kind(game.kind),
                game.realm,
                game.journal.display(),
                if game.held { ", held" } else { "" }
            );
            for s in audits.seats(g).map_err(e)? {
                println!(
                    "  lease {:<8} {}{}",
                    s.lease,
                    s.character,
                    if s.host { " (host)" } else { "" }
                );
            }
            match audits.audit_of_game(g) {
                Ok(a) => println!("latest audit {}: {}", a.id, a.state.name()),
                Err(_) => println!("no audit"),
            }
        }
        "queue" => {
            let id = audits.request(num("GAME", arg)?, "owner", now).map_err(e)?;
            println!("audit {id} queued");
        }
        "run" => {
            let engine = chars::engine().ok_or(
                "this build has no replay engine yet (the journal replay, N17); \
                 queued audits wait for it",
            )?;
            let r = match arg {
                Some(_) => Some(
                    audits
                        .run(num("AUDIT", arg)?, &*engine, &store, now)
                        .map_err(e)?,
                ),
                None => audits.run_next(&*engine, &store, now).map_err(e)?,
            };
            match r {
                Some(r) => println!(
                    "audit {} of game {}: {}, {} findings, {} rollbacks, report {}",
                    r.audit,
                    r.game,
                    r.state.name(),
                    r.findings.len(),
                    r.rollbacks.len(),
                    r.report.display()
                ),
                None => println!("nothing to run"),
            }
        }
        "findings" => {
            println!("finding audit  game        what                         tick     state");
            for f in audits.findings().map_err(e)? {
                let state = match f.state {
                    FindingState::Open => "open",
                    FindingState::RolledBack => "rolled back",
                    FindingState::Dismissed => "dismissed",
                    FindingState::Bug => "bug",
                };
                println!(
                    "{:<7} {:<6} {:<11} {:<28} {:<8} {state}",
                    f.id,
                    f.audit,
                    f.game,
                    f.kind.name(),
                    f.tick.map_or("-".into(), |t| t.to_string()),
                );
            }
        }
        "rollbacks" => {
            println!("rollback audit  game        character        state");
            for r in audits.rollbacks().map_err(e)? {
                println!(
                    "{:<8} {:<6} {:<11} {:<16} {}",
                    r.id,
                    r.audit,
                    r.game,
                    r.character,
                    r.state.name()
                );
            }
        }
        "rollback" => {
            let r = audits
                .apply_rollback(num("ID", arg)?, &store, now)
                .map_err(e)?;
            println!(
                "rollback {}: {} is back to its record of game {}",
                r.id, r.character, r.game
            );
        }
        "confirm" => {
            for r in audits
                .confirm_audit(num("AUDIT", arg)?, &store, now)
                .map_err(e)?
            {
                println!(
                    "rollback {}: {} is back to its record of game {}",
                    r.id, r.character, r.game
                );
            }
        }
        "decline" => {
            let r = audits.decline_rollback(num("ID", arg)?, now).map_err(e)?;
            println!(
                "rollback {} declined: {} keeps its record",
                r.id, r.character
            );
        }
        "undo" => {
            let force = rest.iter().any(|a| a == "--force");
            let r = audits
                .undo_rollback(num("ID", arg)?, &store, force, now)
                .map_err(e)?;
            println!(
                "rollback {} undone: {} has its record back",
                r.id, r.character
            );
        }
        "reports" => {
            println!("report game        reporter         reason text");
            for r in audits.reports().map_err(e)? {
                println!(
                    "{:<6} {:<11} {:<16} {:<6} {}",
                    r.id, r.game, r.reporter, r.reason, r.text
                );
            }
        }
        "watch" => {
            let w = audits.watch().map_err(e)?;
            println!(
                "official games re-run: {}, matched: {} ({:.1}%)",
                w.official_audited,
                w.official_matched,
                w.matched_pct()
            );
            println!(
                "of those spot checks: {}, matched: {}",
                w.spot_audited, w.spot_matched
            );
            if w.determinism_bugs.is_empty() {
                println!("determinism bugs: none");
            } else {
                let games: Vec<String> = w.determinism_bugs.iter().map(u32::to_string).collect();
                println!("determinism bugs in games: {}", games.join(", "));
            }
        }
        "hold" | "release" => {
            let g = num("GAME", arg)?;
            audits.hold(g, cmd == "hold").map_err(e)?;
            println!(
                "game {g}: journal {}",
                if cmd == "hold" { "held" } else { "released" }
            );
        }
        "prune" => {
            let done = audits.prune(now).map_err(e)?;
            println!("{} journals deleted", done.len());
        }
        other => return Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("d2rust-audit: {msg}");
            ExitCode::FAILURE
        }
    }
}
