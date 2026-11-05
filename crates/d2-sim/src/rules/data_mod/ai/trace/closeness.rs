//! How close a data copy of an AI comes to the Rust body it was lifted
//! from: the summaries of the same seeds run both ways, side by side, with
//! the numbers that differ a lot marked. A report for a person to read, not a
//! test the copy must pass.
//!
//! The two sides never act tick for tick the same (a graph chooses by
//! state and time where the body counts thinks), so only what a player would
//! see is compared: how long the monster spent walking, running, swinging,
//! casting and standing, how near it came, what it did to the character and
//! whether it died. Each number is the mean over the seeds.

// The means are for a person to read: nothing here feeds the simulation, and
// no tick of the game runs on these floats.
#![allow(clippy::float_arithmetic)]

use super::Summary;
use std::fmt::Write as _;

/// The monster modes, by number.
pub const MODES: [&str; 16] = [
    "dying",
    "standing",
    "walking",
    "hit",
    "attack 1",
    "attack 2",
    "blocking",
    "casting",
    "skill 1",
    "skill 2",
    "skill 3",
    "skill 4",
    "dead",
    "knocked back",
    "sequence",
    "running",
];

/// One compared number.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub name: String,
    /// The mean over the seeds of the Rust body's runs, and of the graph's.
    pub rust: f64,
    pub data: f64,
    /// The difference that is too small to matter, in the row's own unit.
    pub slack: f64,
}

impl Row {
    /// The graph minus the body.
    pub fn diff(&self) -> f64 {
        self.data - self.rust
    }

    /// Whether the two differ by more than a quarter of the larger and by
    /// more than the slack.
    pub fn big(&self) -> bool {
        let d = self.diff().abs();
        d > self.slack && d > 0.25 * self.rust.max(self.data)
    }
}

/// The comparison of one class over several seeds.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub label: String,
    pub seeds: usize,
    pub ticks: u32,
    pub rows: Vec<Row>,
}

fn mean(runs: &[Summary], f: impl Fn(&Summary) -> f64) -> f64 {
    if runs.is_empty() {
        return 0.0;
    }
    runs.iter().map(f).sum::<f64>() / runs.len() as f64
}

fn in_mode(s: &Summary, mode: u32) -> f64 {
    s.in_mode
        .iter()
        .find(|(m, _)| *m == mode)
        .map_or(0.0, |(_, t)| f64::from(*t))
}

/// Compares the Rust body's runs with the graph's (the same seeds, the same
/// number of ticks each).
pub fn compare(label: &str, ticks: u32, rust: &[Summary], data: &[Summary]) -> Report {
    let tick_slack = f64::from(ticks) / 20.0;
    let mut rows = Vec::new();
    let mut modes: Vec<u32> = rust
        .iter()
        .chain(data)
        .flat_map(|s| s.in_mode.iter().map(|(m, _)| *m))
        .collect();
    modes.sort_unstable();
    modes.dedup();
    for m in modes {
        rows.push(Row {
            name: format!(
                "ticks {}",
                MODES
                    .get(m as usize)
                    .map_or_else(|| format!("in mode {m}"), |n| (*n).to_owned())
            ),
            rust: mean(rust, |s| in_mode(s, m)),
            data: mean(data, |s| in_mode(s, m)),
            slack: tick_slack,
        });
    }
    let mut add = |name: &str, slack: f64, f: &dyn Fn(&Summary) -> f64| {
        rows.push(Row {
            name: name.into(),
            rust: mean(rust, f),
            data: mean(data, f),
            slack,
        });
    };
    add("closest (sub-tiles)", 5.0, &|s| f64::from(s.closest));
    add("farthest (sub-tiles)", 5.0, &|s| f64::from(s.farthest));
    add("hit points the character lost", 5.0, &|s| {
        f64::from(s.damage_dealt)
    });
    add("hit points the monster lost", 5.0, &|s| {
        f64::from(s.damage_taken)
    });
    add("runs in which it died (of 1)", 0.0, &|s| {
        f64::from(u8::from(s.died_at.is_some()))
    });
    Report {
        label: label.into(),
        seeds: rust.len().max(data.len()),
        ticks,
        rows,
    }
}

impl Report {
    /// The rows that differ a lot.
    pub fn big(&self) -> Vec<&Row> {
        self.rows.iter().filter(|r| r.big()).collect()
    }

    /// The report as text.
    pub fn text(&self) -> String {
        let mut o = String::new();
        let _ = writeln!(
            o,
            "{}: Rust body vs data graph, {} seeds x {} ticks (means)",
            self.label, self.seeds, self.ticks
        );
        let _ = writeln!(o, "  {:<34}{:>9}{:>9}{:>9}", "", "Rust", "data", "diff");
        for r in &self.rows {
            let _ = writeln!(
                o,
                "  {:<34}{:>9.1}{:>9.1}{:>+9.1}{}",
                r.name,
                r.rust,
                r.data,
                r.diff(),
                if r.big() { "  <- BIG" } else { "" }
            );
        }
        let n = self.big().len();
        let _ = writeln!(
            o,
            "  {n} of {} numbers differ a lot (more than a quarter and more than their slack).",
            self.rows.len()
        );
        o
    }
}
