//! The record of a test arena run: one monster against one test
//! character for a number of ticks, as the text file `ai-trace-1`.
//!
//! The arena (`d2-client ai-arena`) writes it, Mod Studio reads it. It holds
//! only what the run saw, as whole numbers, so two runs with the same seed
//! write the same bytes and a trace of a Rust AI and one of a data graph
//! can be set side by side ([`Trace::summary`]).
//!
//! ```text
//! ai-trace-1
//! seed 7
//! ticks 3
//! class 12
//! ai 200
//! monster Fallen
//! state 0 fight
//! phase 0 enrage
//! windup 0 roar
//! s 1 state=0 mode=1 dist=9 life=40/40 target=80
//! e 2 state 0 1
//! e 2 windup 0 8
//! e 3 phase 0
//! ```
//!
//! `s` is one sample per tick (state index or `-`, the unit's mode, the
//! distance to the character in sub-tiles, the monster's and the
//! character's hit points); `e` an event of the monster's graph. The
//! names are the file's, in the order the events refer to them by number.

pub mod closeness;

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The first line of every trace.
pub const FORMAT: &str = "ai-trace-1";

/// A run's record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trace {
    pub seed: u32,
    /// Server ticks run.
    pub ticks: u32,
    /// The monster's class id.
    pub class: u32,
    /// The graph the monster ran (an `index` of `ai.toml`); 0: none, the
    /// monster ran one of D2Gamma's own AIs.
    pub ai: u16,
    /// What to call the monster in a report.
    pub monster: String,
    /// The graph's state names, by number.
    pub states: Vec<String>,
    /// The graph's phase names, by number.
    pub phases: Vec<String>,
    /// The graph's wind-up event names, by number.
    pub windups: Vec<String>,
    pub samples: Vec<Sample>,
    pub events: Vec<Event>,
}

/// The monster and the character after one tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub tick: u32,
    /// The graph state; `None`: not in a graph.
    pub state: Option<u8>,
    /// The monster's mode (`units::modes`); 0 is death, 1 neutral.
    pub mode: u32,
    /// Sub-tiles between the monster and the character (the larger of the
    /// two axes).
    pub dist: u32,
    /// Hit points (whole points) and their maximum.
    pub life: i32,
    pub max_life: i32,
    /// The character's hit points.
    pub target: i32,
}

/// What the monster's graph reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub tick: u32,
    pub kind: EventKind,
}

/// [`crate::monsters::ai::graph::GraphEventKind`], without the game's types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    State { from: u8, to: u8 },
    WindUp { event: u8, ticks: i32 },
    Phase { phase: u8 },
}

/// How a run went, in numbers that two runs can be compared by.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Ticks spent in each state (`None`: outside a graph), by state.
    pub in_state: Vec<(Option<u8>, u32)>,
    /// Ticks spent in each mode, by mode.
    pub in_mode: Vec<(u32, u32)>,
    pub transitions: u32,
    pub windups: u32,
    pub phases: u32,
    /// The smallest and the largest distance to the character.
    pub closest: u32,
    pub farthest: u32,
    /// Whole hit points the character lost, and the monster lost.
    pub damage_dealt: i32,
    pub damage_taken: i32,
    /// The tick the monster died; `None`: it lived.
    pub died_at: Option<u32>,
}

impl Trace {
    /// The trace as the text of the file.
    pub fn write(&self) -> String {
        let mut o = String::new();
        let _ = writeln!(o, "{FORMAT}");
        let _ = writeln!(o, "seed {}", self.seed);
        let _ = writeln!(o, "ticks {}", self.ticks);
        let _ = writeln!(o, "class {}", self.class);
        let _ = writeln!(o, "ai {}", self.ai);
        let _ = writeln!(o, "monster {}", one_line(&self.monster));
        for (key, names) in [
            ("state", &self.states),
            ("phase", &self.phases),
            ("windup", &self.windups),
        ] {
            for (i, n) in names.iter().enumerate() {
                let _ = writeln!(o, "{key} {i} {}", one_line(n));
            }
        }
        for s in &self.samples {
            let state = s.state.map_or("-".to_owned(), |n| n.to_string());
            let _ = writeln!(
                o,
                "s {} state={state} mode={} dist={} life={}/{} target={}",
                s.tick, s.mode, s.dist, s.life, s.max_life, s.target
            );
        }
        for e in &self.events {
            match e.kind {
                EventKind::State { from, to } => {
                    let _ = writeln!(o, "e {} state {from} {to}", e.tick);
                }
                EventKind::WindUp { event, ticks } => {
                    let _ = writeln!(o, "e {} windup {event} {ticks}", e.tick);
                }
                EventKind::Phase { phase } => {
                    let _ = writeln!(o, "e {} phase {phase}", e.tick);
                }
            }
        }
        o
    }

    /// Reads what [`Trace::write`] wrote; a line that does not fit says
    /// which one.
    pub fn parse(text: &str) -> Result<Trace, String> {
        let mut lines = text.lines().enumerate();
        match lines.next() {
            Some((_, FORMAT)) => {}
            _ => return Err(format!("not an {FORMAT} file")),
        }
        let mut t = Trace::default();
        for (n, line) in lines {
            let at = |m: &str| format!("line {}: {m}", n + 1);
            if line.trim().is_empty() {
                continue;
            }
            let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "seed" => t.seed = num(rest).map_err(|e| at(&e))?,
                "ticks" => t.ticks = num(rest).map_err(|e| at(&e))?,
                "class" => t.class = num(rest).map_err(|e| at(&e))?,
                "ai" => t.ai = num(rest).map_err(|e| at(&e))?,
                "monster" => t.monster = rest.to_owned(),
                "state" | "phase" | "windup" => {
                    let (i, name) = rest.split_once(' ').unwrap_or((rest, ""));
                    let i: usize = num(i).map_err(|e| at(&e))?;
                    let list = match key {
                        "state" => &mut t.states,
                        "phase" => &mut t.phases,
                        _ => &mut t.windups,
                    };
                    if i != list.len() {
                        return Err(at(&format!("{key} {i} out of order")));
                    }
                    list.push(name.to_owned());
                }
                "s" => t.samples.push(sample(rest).map_err(|e| at(&e))?),
                "e" => t.events.push(event(rest).map_err(|e| at(&e))?),
                other => return Err(at(&format!("unknown line `{other}`"))),
            }
        }
        Ok(t)
    }

    /// The name of state `n`, or its number when the trace has no name.
    pub fn state_name(&self, n: u8) -> String {
        name(&self.states, n)
    }

    /// The name of phase `n`.
    pub fn phase_name(&self, n: u8) -> String {
        name(&self.phases, n)
    }

    /// The name of wind-up event `n`.
    pub fn windup_name(&self, n: u8) -> String {
        name(&self.windups, n)
    }

    /// One line for an event, with names.
    pub fn describe(&self, e: &Event) -> String {
        match e.kind {
            EventKind::State { from, to } => format!(
                "tick {}: {} to {}",
                e.tick,
                self.state_name(from),
                self.state_name(to)
            ),
            EventKind::WindUp { event, ticks } => format!(
                "tick {}: wind-up {} ({ticks} ticks)",
                e.tick,
                self.windup_name(event)
            ),
            EventKind::Phase { phase } => {
                format!("tick {}: phase {}", e.tick, self.phase_name(phase))
            }
        }
    }

    /// The numbers of the run.
    pub fn summary(&self) -> Summary {
        let mut in_state: BTreeMap<Option<u8>, u32> = BTreeMap::new();
        let mut in_mode: BTreeMap<u32, u32> = BTreeMap::new();
        let mut s = Summary {
            closest: u32::MAX,
            ..Summary::default()
        };
        for x in &self.samples {
            *in_state.entry(x.state).or_default() += 1;
            *in_mode.entry(x.mode).or_default() += 1;
            s.closest = s.closest.min(x.dist);
            s.farthest = s.farthest.max(x.dist);
            if x.mode == 0 && s.died_at.is_none() {
                s.died_at = Some(x.tick);
            }
        }
        if self.samples.is_empty() {
            s.closest = 0;
        }
        if let (Some(a), Some(b)) = (self.samples.first(), self.samples.last()) {
            s.damage_dealt = (a.target - b.target).max(0);
            s.damage_taken = (a.life - b.life).max(0);
        }
        for e in &self.events {
            match e.kind {
                EventKind::State { .. } => s.transitions += 1,
                EventKind::WindUp { .. } => s.windups += 1,
                EventKind::Phase { .. } => s.phases += 1,
            }
        }
        s.in_state = in_state.into_iter().collect();
        s.in_mode = in_mode.into_iter().collect();
        s
    }

    /// The summary as lines of text, with state names.
    pub fn summary_text(&self) -> String {
        let s = self.summary();
        let mut o = String::new();
        let _ = writeln!(
            o,
            "{} (class {}, ai {}), seed {}, {} ticks",
            self.monster, self.class, self.ai, self.seed, self.ticks
        );
        for (st, ticks) in &s.in_state {
            let label = st.map_or("(no graph)".to_owned(), |n| self.state_name(n));
            let _ = writeln!(o, "  in {label}: {ticks} ticks");
        }
        let _ = writeln!(
            o,
            "  {} transitions, {} wind-ups, {} phases",
            s.transitions, s.windups, s.phases
        );
        let _ = writeln!(
            o,
            "  distance {}..{} sub-tiles; the character lost {}, the monster lost {}",
            s.closest, s.farthest, s.damage_dealt, s.damage_taken
        );
        if let Some(t) = s.died_at {
            let _ = writeln!(o, "  the monster died at tick {t}");
        }
        o
    }
}

fn name(list: &[String], n: u8) -> String {
    list.get(usize::from(n))
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| format!("#{n}"))
}

/// `s` with its line breaks made spaces: a name stays on its line.
fn one_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}

fn num<T: std::str::FromStr>(s: &str) -> Result<T, String> {
    s.trim()
        .parse()
        .map_err(|_| format!("`{}` is not a number", s.trim()))
}

fn field<'a>(word: &'a str, key: &str) -> Result<&'a str, String> {
    word.strip_prefix(key)
        .and_then(|r| r.strip_prefix('='))
        .ok_or_else(|| format!("expected `{key}=`, found `{word}`"))
}

fn sample(rest: &str) -> Result<Sample, String> {
    let w: Vec<&str> = rest.split(' ').collect();
    let [tick, state, mode, dist, life, target] = w[..] else {
        return Err("a sample has a tick and five fields".into());
    };
    let state = field(state, "state")?;
    let (life, max_life) = field(life, "life")?
        .split_once('/')
        .ok_or("life is `now/max`")?;
    Ok(Sample {
        tick: num(tick)?,
        state: if state == "-" {
            None
        } else {
            Some(num(state)?)
        },
        mode: num(field(mode, "mode")?)?,
        dist: num(field(dist, "dist")?)?,
        life: num(life)?,
        max_life: num(max_life)?,
        target: num(field(target, "target")?)?,
    })
}

fn event(rest: &str) -> Result<Event, String> {
    let w: Vec<&str> = rest.split(' ').collect();
    let tick = num(w.first().ok_or("an event has a tick")?)?;
    let kind = match w[1..] {
        ["state", from, to] => EventKind::State {
            from: num(from)?,
            to: num(to)?,
        },
        ["windup", event, ticks] => EventKind::WindUp {
            event: num(event)?,
            ticks: num(ticks)?,
        },
        ["phase", phase] => EventKind::Phase { phase: num(phase)? },
        _ => return Err(format!("unknown event `{rest}`")),
    };
    Ok(Event { tick, kind })
}

#[cfg(test)]
mod tests;
