// D2RUST's own: mod-kit tooling.
//! `d2-client ai-closeness`: how close a mod's data copy of an AI comes to
//! the Rust body it was lifted from. For each class and each seed the test
//! arena ([`super::ai_arena`]) runs the monster twice, once on D2Gamma's own
//! AI and once under the mod's graph, and
//! [`d2_sim::rules::data_mod::ai::trace::closeness`] sets the means side by
//! side and marks the numbers that differ a lot. It is a report: the exit
//! status says only whether it could be made.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use d2_sim::rules::data_mod::ai::trace::closeness::{compare, Report};
use d2_sim::rules::data_mod::ai::trace::{Summary, Trace};
use d2_sim::rules::data_mod::AiRules;
use d2_sim::rules::Ruleset;

use super::ai_arena::{self, ArenaArgs};

/// Seeds per class when `--seeds` is not given.
pub const SEEDS: u32 = 8;

/// The options of `ai-closeness`.
#[derive(Clone, Debug)]
pub struct ClosenessArgs {
    /// The arena options; `class` and `seed` are set per run, `ai` is the
    /// graph, and `out` is where the report goes.
    pub arena: ArenaArgs,
    /// The classes to compare (`--classes 19,20,21` or `--class N`).
    pub classes: Vec<u32>,
    /// How many seeds, counting up from `--seed`.
    pub seeds: u32,
}

/// Parses the options after `ai-closeness`: the arena's, plus `--classes`
/// and `--seeds`. `--ai N` (the AI row the graph replaces) is required.
pub fn parse_args(args: &[String]) -> Result<ClosenessArgs> {
    let mut rest = Vec::new();
    let mut classes: Vec<u32> = Vec::new();
    let mut seeds = SEEDS;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--classes" => {
                let v = it.next().context("--classes needs a list like 19,20,21")?;
                for c in v.split(',').filter(|c| !c.is_empty()) {
                    classes.push(c.trim().parse().with_context(|| format!("--classes {v}"))?);
                }
            }
            "--seeds" => {
                seeds = it
                    .next()
                    .context("--seeds needs a value")?
                    .parse()
                    .context("--seeds")?;
            }
            _ => rest.push(a.clone()),
        }
    }
    if seeds == 0 {
        bail!("--seeds must be at least 1");
    }
    // The arena parser wants a class; the list is ours.
    if !rest.iter().any(|a| a == "--class") {
        let first = classes
            .first()
            .context("ai-closeness needs --classes 19,20 (or --class N)")?;
        rest.push("--class".into());
        rest.push(first.to_string());
    }
    let arena = ai_arena::parse_args(&rest)?;
    if arena.ai.is_none() {
        bail!("ai-closeness needs --ai N: the AI row the mod's graph replaces");
    }
    if classes.is_empty() {
        classes.push(arena.class);
    }
    Ok(ClosenessArgs {
        arena,
        classes,
        seeds,
    })
}

/// The text `ai-closeness` prints for `--help`.
pub const USAGE: &str = "\
d2-client ai-closeness --classes N[,N...] --ai ROW [--seeds N] [--seed N] [--ticks N]
                       [--distance SUBTILES] [--hurt TICK:PERCENT,...] [--level N]
                       [--difficulty D] [--out FILE]
                       [--native DIR] [--game-dir DIR] (--mod ID | --mod-dir DIR)

For each monstats row in --classes and each of --seeds seeds (default 8, counting
up from --seed) runs the test arena twice: on D2Gamma's own AI, and under the
mod, whose ai.toml has a graph with index ROW (the AI table row it was lifted
from). Prints, per class, the means side by side and marks the numbers that
differ a lot. A report to read, not a check: it fails only when it cannot be made.";

/// The report of one class from its runs: `rust[i]` and `data[i]` are the
/// same seed.
pub fn report(label: &str, ticks: u32, rust: &[Trace], data: &[Trace]) -> Report {
    let sums = |t: &[Trace]| t.iter().map(Trace::summary).collect::<Vec<Summary>>();
    compare(label, ticks, &sums(rust), &sums(data))
}

/// Runs the comparison and returns one report per class.
pub fn run(args: &ClosenessArgs) -> Result<Vec<Report>> {
    let dir = args
        .arena
        .game_dir
        .clone()
        .or_else(|| std::env::var_os("D2_GAME_DIR").map(PathBuf::from));
    let graphs = match crate::kit_mod::active() {
        Some(stack) => crate::kit_mod::host(stack)?.ai_rules().clone(),
        None => bail!("ai-closeness needs the mod with the data copy: --mod ID or --mod-dir DIR"),
    };
    let ai = args.arena.ai.context("--ai")?;
    if !graphs.defs().iter().any(|d| d.index == ai) {
        bail!("the mod's ai.toml has no graph with index {ai}");
    }
    let mut reports = Vec::new();
    for &class in &args.classes {
        let (mut rust, mut data) = (Vec::new(), Vec::new());
        for i in 0..args.seeds {
            let a = ArenaArgs {
                class,
                seed: args.arena.seed.wrapping_add(i),
                out: None,
                ..args.arena.clone()
            };
            let base = ArenaArgs {
                ai: None,
                ..a.clone()
            };
            rust.push(ai_arena::run_on(
                &base,
                ai_arena::open_data(&base, dir.as_deref())?,
                Ruleset::Original,
                &AiRules::none(),
                dir.as_deref(),
            )?);
            let t = ai_arena::run_on(
                &a,
                ai_arena::open_data(&a, dir.as_deref())?,
                crate::kit_mod::ruleset(),
                &graphs,
                dir.as_deref(),
            )?;
            if t.samples.iter().all(|s| s.state.is_none()) {
                bail!(
                    "class {class} did not run the graph {ai} (its monstats AI is not row {ai}, \
                     or the mod does not replace it)"
                );
            }
            data.push(t);
        }
        let label = format!("class {class} (AI {ai})");
        reports.push(report(&label, args.arena.ticks, &rust, &data));
    }
    Ok(reports)
}

#[cfg(test)]
mod tests;
