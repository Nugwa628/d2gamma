//! The scenario format (`.mps`): one multiplayer situation, written as
//! lines a person can read.
//!
//! ```text
//! # Two players see each other and one leaves.
//! name   join-leave
//! about  Two bots enter one game; each sees the other; a leave is seen.
//! rule   SEE   every player in a game sees every other player in it
//! rule   LEAVE a player that leaves is gone for the others
//! bots   2
//! server players=8 grace=0
//! net    latency=0
//!
//! all  connect                     expect ok
//! all  enter game1                 expect ok
//! b0   see players=2               within 3s  rule SEE
//! b1   leave
//! b0   see players=1               within 3s  rule LEAVE
//! ```
//!
//! **Header lines** (before or between steps; each at most once except
//! `rule`, `needs`):
//! - `name ID`, `about TEXT`: the scenario's id and what it shows.
//! - `rule ID TEXT`: a stated rule. Every failure names the rule it
//!   breaks: the step's `rule ID`, or the scenario's only rule. A step
//!   that expects more than `ok` names its rule when there are several;
//!   a setup step that fails breaks `core.step`.
//! - `bots N` (1..=64): how many bot clients.
//! - `server KEY=VALUE..`: the fresh server's settings (`players`,
//!   `grace` in ms, `build`).
//! - `net KEY=VALUE..`: the starting network conditions for every bot.
//! - `addresses per-bot|shared`: whether each bot's relay has its own
//!   loopback address (the default; contracts 10.1) or all bots share
//!   127.0.0.1 (for a per-IP limit).
//! - `needs CAP..`: capabilities beyond those its steps imply.
//! - `pending TEXT`: written ahead of the feature; reported, never run.
//! - `seed N`: the seed of the injected loss and jitter.
//! - `setup split|split:N|host`: runs on a v2 setup (plan N1) of at least
//!   that size (`mp-test --setup`); pending on others.
//! - `nat NAME=MODE..`: a game server's NAT mode (`open`, `cone`,
//!   `symmetric`, `blocked`; [`crate::net::nat`]).
//! - `hostile MODE`: the game servers are a hostile test build
//!   (`drop-change`, `invent-intent`, `alter-save`, `copy-item`,
//!   `impossible-item`; contracts 11.10).
//!
//! **Steps:** `WHO VERB ARGS.. [expect PRED..] [within DURATION] [rule ID]`.
//! `WHO` is `bN`, `bA-B` (a range), `all`, `server`, `net` or `wait`;
//! a v2 setup's steps start with their verb instead (below).
//! - `server start|stop|kill|restart`: `stop` is a clean stop (SIGTERM
//!   where there is one), `kill` a crash.
//! - `server admin ARGS..`: runs the server program once with its config
//!   and these arguments (an offline admin command), expects exit 0.
//! - `server dev CHARACTER COMMAND..`: a dev console command for that
//!   character's game, on a test server (one stdin line).
//! - `server econ-seed ACCOUNT gold:N|CODE:N [SPLIT]` and
//!   `server econ-clock +N[s|m|h|d]`: the market's test lines on the
//!   main server's stdin (gold or stackables into a stash; the market's
//!   clock forward, then its expiry sweep).
//! - `server season-start NAME.. [test]`, `server season-end` and
//!   `server ladder-seed CHARACTER LEVEL EXPERIENCE [dead]`: the ladder's
//!   test lines on the main server's stdin (a season opened or ended; a
//!   character's ladder row and summary written as a save would).
//! - `server notice TEXT..`, `server mute ACCOUNT MINUTES [REASON..]`,
//!   `server unmute ACCOUNT` and `server social-clock +N[s|m|h|d]`: the
//!   chat's lines on the main server's stdin (a realm notice in Global,
//!   an admin mute; the DM and invite clock forward, then the sweeps).
//! - `server upgrade VERSION MINUTES MINUTE_MS` and `server upgrade-cancel`:
//!   the upgrade's countdown on a test server's stdin (a minute of
//!   MINUTE_MS ms), and its cancel.
//! - `net [bN|bA-B|all] KEY=VALUE..` (`latency`, `jitter` in ms, `loss`
//!   in percent, `cut=on|off`)
//! - `wait DURATION`
//! - v2 setups: `kill-node ID`, `stop-node ID`,
//!   `start-node ID [version=V] [update=off]`, `pause-node ID`,
//!   `resume-node ID`, `cut-link ID [on|off]`, `restart-realm [version=V]`
//!   (with a version: once the upgrade is at phase swap, as server-ctl),
//!   `host-sleep NAME`, `host-quit NAME`, `host-client-crash NAME`, and the
//!   query `node ID expect ..` (facts `running`, `linked`, `links`,
//!   `starts`, `node`, `closed`, `public`, `nat`, `cut`, `host`, `swaps`,
//!   `version`, `refusal`). A bot step may add `on-node ID` (create or join
//!   on that game server).
//!
//! Predicates: `ok`, `refused`, `refused=CODE`, `error`, `closed`,
//! `KEY=VALUE`, `KEY!=VALUE`, `KEY>=N`, `KEY<=N`, and `node ID` (the same
//! as `node=ID`).
//! Durations: `500ms`, `3s`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::net::nat::NatMode;
use crate::topology::Setup;

/// The most bots one scenario may drive.
pub const MAX_BOTS: usize = 64;

/// A parse error, with the line it is on.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("{file}:{line}: {msg}")]
pub struct ParseError {
    /// The scenario file.
    pub file: String,
    /// The line number, from 1 (0: the whole file).
    pub line: usize,
    /// What is wrong.
    pub msg: String,
}

/// A stated rule a scenario checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Its id, unique in the scenario.
    pub id: String,
    /// The rule in words.
    pub text: String,
}

/// Who a step is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Who {
    /// These bots (indices, ascending).
    Bots(Vec<usize>),
    /// The server process.
    Server,
    /// The network between bots and server.
    Net(Vec<usize>),
    /// Nobody: the clock.
    Wait,
    /// A v2 setup's programs (the verb says which, and how).
    Topo,
}

/// The v2 setup verbs (they start their line).
pub const TOPO_VERBS: &[&str] = &[
    "kill-node",
    "stop-node",
    "start-node",
    "pause-node",
    "resume-node",
    "cut-link",
    "restart-realm",
    "node",
    "host-sleep",
    "host-quit",
    "host-client-crash",
];

/// The misbehaviours of a hostile test build (contracts 11.10).
pub const HOSTILE: &[&str] = &[
    "drop-change",
    "invent-intent",
    "alter-save",
    "copy-item",
    "impossible-item",
    "invent-stash-item",
];

/// One expectation on a step's outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pred {
    /// The action succeeded.
    Ok,
    /// It was refused (with this code, when given).
    Refused(Option<String>),
    /// It failed with an error that isn't a refusal.
    Error,
    /// The connection is closed.
    Closed,
    /// A fact compares to a value.
    Fact {
        /// The fact's key.
        key: String,
        /// How it compares.
        op: Op,
        /// The value it compares to.
        value: String,
    },
}

/// A fact comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `=`
    Eq,
    /// `!=`
    Ne,
    /// `>=` (numbers)
    Ge,
    /// `<=` (numbers)
    Le,
}

impl std::fmt::Display for Pred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Pred::Ok => write!(f, "ok"),
            Pred::Refused(None) => write!(f, "refused"),
            Pred::Refused(Some(c)) => write!(f, "refused={c}"),
            Pred::Error => write!(f, "error"),
            Pred::Closed => write!(f, "closed"),
            Pred::Fact { key, op, value } => {
                let op = match op {
                    Op::Eq => "=",
                    Op::Ne => "!=",
                    Op::Ge => ">=",
                    Op::Le => "<=",
                };
                write!(f, "{key}{op}{value}")
            }
        }
    }
}

/// One step of a scenario.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// Its line in the file.
    pub line: usize,
    /// The line as written (trimmed).
    pub text: String,
    /// Who does it.
    pub who: Who,
    /// The verb (`connect`, `see`, `start`, ...).
    pub verb: String,
    /// The verb's arguments.
    pub args: Vec<String>,
    /// What must hold afterwards (empty: it must simply succeed).
    pub expect: Vec<Pred>,
    /// How long it may take (a query is retried until it holds).
    pub within: Option<Duration>,
    /// The rule a failure breaks.
    pub rule: Option<String>,
    /// The game server a bot step names (`on-node ID`).
    pub node: Option<String>,
}

/// A parsed scenario.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    /// The file it came from.
    pub file: PathBuf,
    /// Its id.
    pub name: String,
    /// What it shows.
    pub about: String,
    /// Its stated rules, in order.
    pub rules: Vec<Rule>,
    /// How many bots it drives.
    pub bots: usize,
    /// The fresh server's settings.
    pub server: Vec<(String, String)>,
    /// The starting network conditions.
    pub net: Vec<(String, String)>,
    /// Every bot shares 127.0.0.1 instead of getting its own address.
    pub shared_address: bool,
    /// Capabilities named with `needs`.
    pub needs: BTreeSet<String>,
    /// Why it is written ahead of its feature, if it is.
    pub pending: Option<String>,
    /// The seed of injected loss and jitter.
    pub seed: u64,
    /// The v2 setup it needs, if any.
    pub setup: Option<Setup>,
    /// Game servers' NAT modes.
    pub nat: Vec<(String, NatMode)>,
    /// A hostile test build's misbehaviour.
    pub hostile: Option<String>,
    /// Its steps, in order.
    pub steps: Vec<Step>,
}

impl Scenario {
    /// The rule a step's failure breaks: its own, or the scenario's only
    /// rule.
    pub fn rule_of(&self, step: &Step) -> Option<&Rule> {
        match &step.rule {
            Some(id) => self.rules.iter().find(|r| &r.id == id),
            // A plain `expect ok` with several rules is setup: no rule of
            // its own.
            None if self.rules.len() == 1 && !step.expect.is_empty() => self.rules.first(),
            None => None,
        }
    }

    /// A server setting, if the scenario gives one.
    pub fn server_opt(&self, key: &str) -> Option<&str> {
        self.server
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Parses a duration: `250ms`, `3s`.
pub fn parse_duration(s: &str) -> Option<Duration> {
    if let Some(ms) = s.strip_suffix("ms") {
        return ms.parse().ok().map(Duration::from_millis);
    }
    s.strip_suffix('s')?
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(Duration::from_secs_f64)
}

/// Parses `KEY=VALUE` arguments.
fn key_values(args: &[&str]) -> Result<Vec<(String, String)>, String> {
    args.iter()
        .map(|a| {
            a.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .ok_or_else(|| format!("`{a}` is not KEY=VALUE"))
        })
        .collect()
}

/// Parses `bN`, `bA-B` or `all` against `bots` bots.
fn parse_bots(s: &str, bots: usize) -> Result<Option<Vec<usize>>, String> {
    if s == "all" {
        return Ok(Some((0..bots).collect()));
    }
    let Some(rest) = s.strip_prefix('b') else {
        return Ok(None);
    };
    let (a, b) = match rest.split_once('-') {
        Some((a, b)) => (a, b),
        None => (rest, rest),
    };
    let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>()) else {
        return Ok(None);
    };
    if a > b || b >= bots {
        return Err(format!(
            "`{s}`: the scenario has bots b0-b{}",
            bots.saturating_sub(1)
        ));
    }
    Ok(Some((a..=b).collect()))
}

/// Parses one predicate.
fn parse_pred(s: &str) -> Result<Pred, String> {
    Ok(match s {
        "ok" => Pred::Ok,
        "refused" => Pred::Refused(None),
        "error" => Pred::Error,
        "closed" => Pred::Closed,
        _ => {
            let (key, op, value) = if let Some((k, v)) = s.split_once("!=") {
                (k, Op::Ne, v)
            } else if let Some((k, v)) = s.split_once(">=") {
                (k, Op::Ge, v)
            } else if let Some((k, v)) = s.split_once("<=") {
                (k, Op::Le, v)
            } else if let Some((k, v)) = s.split_once('=') {
                (k, Op::Eq, v)
            } else {
                return Err(format!("`{s}` is not a predicate"));
            };
            if key.is_empty() {
                return Err(format!("`{s}` has no key"));
            }
            if key == "refused" && op == Op::Eq {
                return Ok(Pred::Refused(Some(value.to_string())));
            }
            if matches!(op, Op::Ge | Op::Le) && value.parse::<i64>().is_err() {
                return Err(format!("`{s}`: {value} is not a number"));
            }
            Pred::Fact {
                key: key.to_string(),
                op,
                value: value.to_string(),
            }
        }
    })
}

/// Reads and parses a scenario file.
pub fn load(path: &Path) -> Result<Scenario, ParseError> {
    let text = std::fs::read_to_string(path).map_err(|e| ParseError {
        file: path.display().to_string(),
        line: 0,
        msg: format!("reading: {e}"),
    })?;
    parse(path, &text)
}

/// Parses a scenario from its text.
pub fn parse(path: &Path, text: &str) -> Result<Scenario, ParseError> {
    let file = path.display().to_string();
    let err = |line: usize, msg: String| ParseError {
        file: file.clone(),
        line,
        msg,
    };
    let mut nat_line = 0;
    let mut s = Scenario {
        file: path.to_path_buf(),
        name: String::new(),
        about: String::new(),
        rules: Vec::new(),
        bots: 1,
        server: Vec::new(),
        net: Vec::new(),
        shared_address: false,
        needs: BTreeSet::new(),
        pending: None,
        seed: 1,
        setup: None,
        nat: Vec::new(),
        hostile: None,
        steps: Vec::new(),
    };
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let body = raw.split_once('#').map_or(raw, |(b, _)| b).trim();
        if body.is_empty() {
            continue;
        }
        let words: Vec<&str> = body.split_whitespace().collect();
        let rest = || body[words[0].len()..].trim().to_string();
        match words[0] {
            "name" => s.name = rest(),
            "about" => s.about = rest(),
            "rule" => {
                let id = words
                    .get(1)
                    .ok_or_else(|| err(line, "rule needs an id and its text".into()))?;
                let text = body["rule".len()..].trim_start()[id.len()..].trim();
                if text.is_empty() {
                    return Err(err(line, format!("rule {id} has no text")));
                }
                if s.rules.iter().any(|r| r.id == *id) {
                    return Err(err(line, format!("rule {id} is stated twice")));
                }
                s.rules.push(Rule {
                    id: id.to_string(),
                    text: text.to_string(),
                });
            }
            "bots" => {
                if !s.steps.is_empty() {
                    return Err(err(line, "bots comes before the first step".into()));
                }
                let n: usize = words
                    .get(1)
                    .and_then(|w| w.parse().ok())
                    .filter(|n| (1..=MAX_BOTS).contains(n))
                    .ok_or_else(|| err(line, format!("bots takes 1..={MAX_BOTS}")))?;
                s.bots = n;
            }
            "server" if words.get(1).is_some_and(|w| w.contains('=')) => {
                s.server = key_values(&words[1..]).map_err(|m| err(line, m))?;
            }
            "net" if s.steps.is_empty() && words.get(1).is_some_and(|w| w.contains('=')) => {
                s.net = key_values(&words[1..]).map_err(|m| err(line, m))?;
            }
            "addresses" => {
                s.shared_address = match words.get(1).copied() {
                    Some("per-bot") => false,
                    Some("shared") => true,
                    _ => return Err(err(line, "addresses takes per-bot or shared".into())),
                }
            }
            "setup" => {
                s.setup = Some(
                    Setup::parse(words.get(1).copied().unwrap_or_default())
                        .map_err(|m| err(line, m))?,
                );
            }
            "nat" => {
                nat_line = line;
                for (k, v) in key_values(&words[1..]).map_err(|m| err(line, m))? {
                    let mode = v.parse::<NatMode>().map_err(|m| err(line, m))?;
                    s.nat.push((k, mode));
                }
            }
            "hostile" => match words.get(1) {
                Some(h) if HOSTILE.contains(h) => s.hostile = Some(h.to_string()),
                _ => {
                    return Err(err(
                        line,
                        format!("hostile takes one of {}", HOSTILE.join(", ")),
                    ))
                }
            },
            "needs" => s.needs.extend(words[1..].iter().map(|w| w.to_string())),
            "pending" => s.pending = Some(rest()),
            "seed" => {
                s.seed = words
                    .get(1)
                    .and_then(|w| w.parse().ok())
                    .ok_or_else(|| err(line, "seed takes a number".into()))?;
            }
            _ => {
                let step = parse_step(line, body, &words, s.bots).map_err(|m| err(line, m))?;
                s.steps.push(step);
            }
        }
    }
    if s.name.is_empty() {
        s.name = path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    if s.rules.is_empty() {
        return Err(err(0, "a scenario states at least one rule".into()));
    }
    for step in &s.steps {
        if let Some(id) = &step.rule {
            if !s.rules.iter().any(|r| &r.id == id) {
                return Err(err(step.line, format!("rule {id} is not stated")));
            }
        } else if step.expect.iter().any(|p| *p != Pred::Ok) && s.rules.len() > 1 {
            return Err(err(
                step.line,
                "a step with expect names its rule (the scenario states several)".into(),
            ));
        }
    }
    if s.steps.is_empty() && s.pending.is_none() {
        return Err(err(0, "a scenario has steps (or is pending)".into()));
    }
    // A NAT is a game server's (`nN`, within the setup) or a bot's.
    for (k, _) in &s.nat {
        let index = |p: char| k.strip_prefix(p).and_then(|d| d.parse::<usize>().ok());
        let known = match (index('n'), index('b'), s.setup) {
            (Some(i), _, Some(Setup::Split(n))) => (1..=n).contains(&i),
            (Some(i), _, None) => i >= 1,
            (_, Some(b), _) => b < s.bots,
            _ => false,
        };
        if !known {
            return Err(err(
                nat_line,
                format!("nat {k}: no such game server or bot in this scenario"),
            ));
        }
    }
    Ok(s)
}

/// Parses one step line.
fn parse_step(line: usize, body: &str, words: &[&str], bots: usize) -> Result<Step, String> {
    let (who, mut i) = match words[0] {
        "server" => (Who::Server, 1),
        "wait" => (Who::Wait, 1),
        w if TOPO_VERBS.contains(&w) => (Who::Topo, 0),
        "net" => match words.get(1).map(|w| parse_bots(w, bots)).transpose()? {
            Some(Some(b)) => (Who::Net(b), 2),
            _ => (Who::Net((0..bots).collect()), 1),
        },
        w => match parse_bots(w, bots)? {
            Some(b) => (Who::Bots(b), 1),
            None => return Err(format!("`{w}` is not a header, bot, server, net or wait")),
        },
    };
    let verb = match who {
        Who::Net(_) => "set".to_string(),
        _ => {
            let v = words.get(i).ok_or("the step has no verb")?.to_string();
            i += 1;
            v
        }
    };
    let mut args = Vec::new();
    let mut expect = Vec::new();
    let mut within = None;
    let mut rule = None;
    let mut node = None;
    let mut mode = 0u8; // 0 args, 1 expect
    while i < words.len() {
        match words[i] {
            "expect" => mode = 1,
            "within" => {
                i += 1;
                let d = words.get(i).ok_or("within takes a duration")?;
                within = Some(parse_duration(d).ok_or(format!("`{d}` is not a duration"))?);
            }
            "rule" => {
                i += 1;
                rule = Some(words.get(i).ok_or("rule takes an id")?.to_string());
            }
            "on-node" if mode == 0 && matches!(who, Who::Bots(_)) => {
                i += 1;
                node = Some(
                    words
                        .get(i)
                        .ok_or("on-node takes a game server")?
                        .to_string(),
                );
            }
            // `expect node ID`: the game runs on that game server.
            "node" if mode == 1 => {
                i += 1;
                expect.push(Pred::Fact {
                    key: "node".into(),
                    op: Op::Eq,
                    value: words
                        .get(i)
                        .ok_or("expect node takes a game server")?
                        .to_string(),
                });
            }
            w if mode == 1 => expect.push(parse_pred(w)?),
            w => args.push(w.to_string()),
        }
        i += 1;
    }
    match (&who, verb.as_str()) {
        (Who::Server, "start" | "stop" | "kill" | "restart") => {}
        (Who::Server, "season-end" | "upgrade-cancel") if args.is_empty() => {}
        (
            Who::Server,
            "admin" | "dev" | "progression" | "econ-seed" | "econ-clock" | "season-start"
            | "ladder-seed" | "notice" | "mute" | "unmute" | "social-clock" | "upgrade",
        ) if !args.is_empty() => {}
        (Who::Server, v) => {
            return Err(format!(
                "server takes start|stop|kill|restart|admin ARGS|dev CHARACTER COMMAND|progression CHARACTER N|econ-seed ..|econ-clock ..|season-start NAME..|season-end|ladder-seed ..|notice ..|mute ..|unmute ..|social-clock ..|upgrade VERSION MINUTES MINUTE_MS|upgrade-cancel, not {v}"
            ))
        }
        (Who::Wait, _) => {
            if parse_duration(&verb).is_none() {
                return Err(format!("wait takes a duration, not {verb}"));
            }
        }
        (Who::Net(_), _) => {
            key_values(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
        }
        (Who::Topo, "restart-realm")
            if args.len() > 1 || args.first().is_some_and(|a| !a.starts_with("version=")) =>
        {
            return Err("restart-realm takes nothing, or version=V".into());
        }
        (Who::Topo, "start-node")
            if args.is_empty()
                || args[1..]
                    .iter()
                    .any(|a| !a.starts_with("version=") && a != "update=off") =>
        {
            return Err("start-node takes a game server, then version=V or update=off".into());
        }
        (Who::Topo, "cut-link")
            if !(1..=2).contains(&args.len())
                || args.get(1).is_some_and(|a| a != "on" && a != "off") =>
        {
            return Err("cut-link takes a game server and on|off".into());
        }
        (Who::Topo, "restart-realm" | "cut-link" | "start-node") => {}
        (Who::Topo, v) if args.len() != 1 => {
            return Err(format!("{v} takes one game server"));
        }
        _ => {}
    }
    Ok(Step {
        line,
        text: body.to_string(),
        who,
        verb,
        args,
        expect,
        within,
        rule,
        node,
    })
}
