//! The audit report: `data/audits/<game>.md` (contracts 11.11), rewritten
//! whenever its audit or rollbacks change. It says what was re-run, what
//! matched, the first differing tick and what differed, the characters
//! affected and what happened to them.

use std::fmt::Write as _;
use std::path::PathBuf;

use crate::db::{
    AuditRow, AuditState, FindingRow, FindingState, GameRow, ReportRow, RollbackRow, SeatRow,
};
use crate::{AuditError, AuditId, Audits, GameKind};

/// Everything a report shows.
pub struct ReportData<'a> {
    pub game: &'a GameRow,
    pub audit: &'a AuditRow,
    pub seats: &'a [SeatRow],
    pub findings: &'a [FindingRow],
    pub rollbacks: &'a [RollbackRow],
    pub reports: &'a [ReportRow],
}

fn kind_name(k: GameKind) -> &'static str {
    match k {
        GameKind::Official => "official",
        GameKind::PlayerHosted => "player-hosted",
    }
}

fn opt(t: Option<u32>) -> String {
    t.map_or("-".into(), |t| t.to_string())
}

/// The report's Markdown.
pub fn render(d: &ReportData<'_>) -> String {
    let (g, a) = (d.game, d.audit);
    let mut s = String::new();
    let official_bug = g.kind == GameKind::Official && a.state == AuditState::Failed;
    let title = if official_bug {
        "determinism bug"
    } else {
        a.state.name()
    };
    let _ = writeln!(s, "# Audit {} of game {}: {title}\n", a.id, g.id);
    let _ = writeln!(
        s,
        "- Game: {} on `{}` ({}), realm `{}`",
        g.id,
        g.node,
        kind_name(g.kind),
        g.realm
    );
    let _ = writeln!(s, "- Journal: `{}`", g.journal.display());
    let _ = writeln!(
        s,
        "- Queued at {} ms, finished at {}",
        a.queued,
        a.finished.map_or("-".into(), |f| format!("{f} ms"))
    );
    let _ = writeln!(s, "- Result: {}", a.state.name());
    if !a.detail.is_empty() {
        let _ = writeln!(s, "- {}", a.detail);
    }
    if official_bug {
        let _ = writeln!(
            s,
            "\nThis game ran on an official game server: a difference is a determinism bug \
             in the game, not a cheat. No character is rolled back. Keep the journal \
             (it is held) to reproduce it."
        );
    }
    let _ = writeln!(s, "\n## Why it was audited\n");
    for (r, lease, detail) in &a.reasons {
        let lease = lease.map_or(String::new(), |l| format!(" (lease {l})"));
        let detail = if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        };
        let _ = writeln!(s, "- {}{lease}{detail}", r.name());
    }
    for r in d.reports {
        let _ = writeln!(
            s,
            "- report by {} (reason {}): {}",
            r.reporter,
            r.reason,
            if r.text.is_empty() { "-" } else { &r.text }
        );
    }
    let _ = writeln!(s, "\n## Characters\n");
    let _ = writeln!(s, "| Lease | Character | Host |");
    let _ = writeln!(s, "|---|---|---|");
    for seat in d.seats {
        let _ = writeln!(
            s,
            "| {} | {} | {} |",
            seat.lease,
            seat.character,
            if seat.host { "yes" } else { "" }
        );
    }
    if !d.findings.is_empty() {
        let _ = writeln!(s, "\n## Findings\n");
        let _ = writeln!(
            s,
            "| # | What | Tick | Last good tick | Lease | Detail | State |"
        );
        let _ = writeln!(s, "|---|---|---|---|---|---|---|");
        for f in d.findings {
            let state = match f.state {
                FindingState::Open => "open",
                FindingState::RolledBack => "rolled back",
                FindingState::Dismissed => "dismissed",
                FindingState::Bug => "bug",
            };
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {} | {} | {state} |",
                f.id,
                f.kind.name(),
                opt(f.tick),
                opt(f.last_good),
                f.lease.map_or("-".into(), |l| l.to_string()),
                f.detail.replace('|', "/"),
            );
        }
    }
    if !d.rollbacks.is_empty() {
        let _ = writeln!(s, "\n## Rollbacks\n");
        let _ = writeln!(
            s,
            "Each character goes back to the record it joined this game with.\n"
        );
        let _ = writeln!(s, "| # | Character | Lease | State |");
        let _ = writeln!(s, "|---|---|---|---|");
        for r in d.rollbacks {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} |",
                r.id,
                r.character,
                r.lease,
                r.state.name()
            );
        }
    }
    s
}

impl Audits {
    /// Writes (or rewrites) the report of `audit` and returns its path.
    pub fn write_report(&self, audit: AuditId) -> Result<PathBuf, AuditError> {
        let a = self.audit(audit)?;
        let game = self.game(a.game)?;
        let seats = self.seats(a.game)?;
        let findings = self.findings_of(audit)?;
        let rollbacks = self.rollbacks_of(audit)?;
        let reports = self.reports_of(a.game)?;
        let text = render(&ReportData {
            game: &game,
            audit: &a,
            seats: &seats,
            findings: &findings,
            rollbacks: &rollbacks,
            reports: &reports,
        });
        let io = |e: std::io::Error| AuditError::Io(format!("{}: {e}", self.reports.display()));
        std::fs::create_dir_all(&self.reports).map_err(io)?;
        let path = self.reports.join(format!("{}.md", a.game));
        let tmp = self.reports.join(format!("{}.md.tmp", a.game));
        std::fs::write(&tmp, text).map_err(io)?;
        std::fs::rename(&tmp, &path).map_err(io)?;
        Ok(path)
    }
}
