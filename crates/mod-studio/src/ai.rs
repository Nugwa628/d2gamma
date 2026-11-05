//! `files/ai.toml` as Studio edits it: monster behaviour graphs and presets.
//!
//! The model mirrors the file key for key. Which kinds of condition and action
//! exist, and the keys each takes, come from the engine's own vocabulary
//! ([`d2_sim::rules::data_mod::ai::CONDITIONS`] and `ACTIONS`); whether a file
//! is acceptable is decided by the engine's reader, never here. Studio writes
//! the file back canonically (comments and key order of a hand-written file are
//! not kept).
use anyhow::{bail, Context, Result};
use d2_sim::rules::data_mod::ai::{action_kind, condition_kind, FieldKind, KindInfo};
use d2_sim::rules::data_mod::{AiRules, DataMod, DataModFiles};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use toml::{Table, Value};

pub const AI_FILE: &str = "files/ai.toml";
pub const CONTROL_FILE: &str = "files/crowd_control.toml";

/// A condition, or the kind-specific part of an action: the `kind` word and
/// the other keys it carries.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: String,
    pub keys: BTreeMap<String, Value>,
}

impl Item {
    /// A new item of this kind with the required keys at their defaults.
    pub fn new(info: &KindInfo, cooldowns: &[String]) -> Item {
        let keys = info
            .fields
            .iter()
            .filter(|f| f.required)
            .map(|f| {
                let v = match f.kind {
                    FieldKind::Cooldown => {
                        Value::String(cooldowns.first().cloned().unwrap_or_default())
                    }
                    _ => f.default_value(),
                };
                (f.key.to_owned(), v)
            })
            .collect();
        Item {
            kind: info.name.to_owned(),
            keys,
        }
    }

    /// The vocabulary entry of a condition.
    pub fn condition_info(&self) -> Option<&'static KindInfo> {
        condition_kind(&self.kind)
    }

    /// The vocabulary entry of an action.
    pub fn action_info(&self) -> Option<&'static KindInfo> {
        action_kind(&self.kind)
    }

    /// A short sentence for graph labels, from the vocabulary: `Life below 50`.
    pub fn text(&self, info: Option<&KindInfo>) -> String {
        let Some(info) = info else {
            return self.kind.clone();
        };
        let mut out = info.label.to_owned();
        for f in info.fields {
            if let Some(v) = self.keys.get(f.key) {
                out.push(' ');
                out.push_str(&plain(v));
            }
        }
        out
    }
}

/// A TOML value as bare text.
pub fn plain(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A seconds value kept as written: a whole number stays whole.
pub fn seconds(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 1.0e9 {
        Value::Integer(v as i64)
    } else {
        Value::Float(v)
    }
}

pub fn as_f64(v: &Value) -> f64 {
    match v {
        Value::Integer(n) => *n as f64,
        Value::Float(f) => *f,
        _ => 0.0,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub item: Item,
    pub when: Vec<Item>,
    pub starts_cooldown: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub to: String,
    pub when: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub name: String,
    pub transitions: Vec<Transition>,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cooldown {
    pub name: String,
    pub seconds: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Phase {
    pub name: String,
    pub when: Vec<Item>,
    pub state: Option<String>,
    pub graph: Option<u16>,
    pub immune_seconds: Option<Value>,
}

/// A preset parameter: one number, or one for each difficulty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Param {
    All(i64),
    Each([i64; 3]),
}

/// One entry of the file: a behaviour graph, or a preset with its parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub index: u16,
    pub name: String,
    pub target: Option<String>,
    pub start: Option<String>,
    pub preset: Option<String>,
    pub params: Vec<(String, Param)>,
    pub cooldowns: Vec<Cooldown>,
    pub states: Vec<State>,
    pub phases: Vec<Phase>,
}

impl Entry {
    /// A graph with one state that stands still.
    pub fn new_graph(index: u16, name: &str) -> Entry {
        Entry {
            index,
            name: name.to_owned(),
            target: None,
            start: None,
            preset: None,
            params: Vec::new(),
            cooldowns: Vec::new(),
            states: vec![State {
                name: "idle".into(),
                transitions: Vec::new(),
                actions: Vec::new(),
            }],
            phases: Vec::new(),
        }
    }

    /// A preset entry with no parameters changed.
    pub fn new_preset(index: u16, preset: &str) -> Entry {
        Entry {
            index,
            name: preset.to_owned(),
            target: None,
            start: None,
            preset: Some(preset.to_owned()),
            params: Vec::new(),
            cooldowns: Vec::new(),
            states: Vec::new(),
            phases: Vec::new(),
        }
    }

    pub fn state_names(&self) -> Vec<String> {
        self.states.iter().map(|s| s.name.clone()).collect()
    }

    pub fn cooldown_names(&self) -> Vec<String> {
        self.cooldowns.iter().map(|c| c.name.clone()).collect()
    }

    /// The position of the state a monster starts in.
    pub fn start_position(&self) -> usize {
        self.start
            .as_ref()
            .and_then(|s| self.states.iter().position(|x| &x.name == s))
            .unwrap_or(0)
    }

    /// Renames a state and everything that points at it.
    pub fn rename_state(&mut self, at: usize, new: &str) {
        let old = std::mem::replace(&mut self.states[at].name, new.to_owned());
        if old == new {
            return;
        }
        for s in &mut self.states {
            for t in &mut s.transitions {
                if t.to == old {
                    t.to = new.to_owned();
                }
            }
        }
        if self.start.as_deref() == Some(&old) {
            self.start = Some(new.to_owned());
        }
        for p in &mut self.phases {
            if p.state.as_deref() == Some(&old) {
                p.state = Some(new.to_owned());
            }
        }
    }

    /// Removes a state, the transitions that led to it, and the start or phase
    /// settings that named it.
    pub fn delete_state(&mut self, at: usize) {
        let gone = self.states.remove(at).name;
        for s in &mut self.states {
            s.transitions.retain(|t| t.to != gone);
        }
        if self.start.as_deref() == Some(&gone) {
            self.start = None;
        }
        for p in &mut self.phases {
            if p.state.as_deref() == Some(&gone) {
                p.state = None;
            }
        }
    }

    /// Renames a cooldown and everything that reads or starts it.
    pub fn rename_cooldown(&mut self, at: usize, new: &str) {
        let old = std::mem::replace(&mut self.cooldowns[at].name, new.to_owned());
        if old == new {
            return;
        }
        let fix = |list: &mut Vec<Item>| {
            for c in list {
                if c.kind == "cooldown_ready"
                    && c.keys.get("name") == Some(&Value::String(old.clone()))
                {
                    c.keys.insert("name".into(), Value::String(new.to_owned()));
                }
            }
        };
        for s in &mut self.states {
            for t in &mut s.transitions {
                fix(&mut t.when);
            }
            for a in &mut s.actions {
                fix(&mut a.when);
                if a.starts_cooldown.as_deref() == Some(&old) {
                    a.starts_cooldown = Some(new.to_owned());
                }
            }
        }
        for p in &mut self.phases {
            fix(&mut p.when);
        }
    }

    /// Removes a cooldown, the conditions that read it and the actions' start of it.
    pub fn delete_cooldown(&mut self, at: usize) {
        let gone = self.cooldowns.remove(at).name;
        let drop = |list: &mut Vec<Item>| {
            list.retain(|c| {
                !(c.kind == "cooldown_ready"
                    && c.keys.get("name") == Some(&Value::String(gone.clone())))
            });
        };
        for s in &mut self.states {
            for t in &mut s.transitions {
                drop(&mut t.when);
            }
            for a in &mut s.actions {
                drop(&mut a.when);
                if a.starts_cooldown.as_deref() == Some(&gone) {
                    a.starts_cooldown = None;
                }
            }
        }
        for p in &mut self.phases {
            drop(&mut p.when);
        }
    }
}

/// A name based on `base` that none of `taken` has.
pub fn unique_name(taken: &[String], base: &str) -> String {
    if !taken.iter().any(|t| t == base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base}_{n}"))
        .find(|c| !taken.iter().any(|t| t == c))
        .expect("an unused name")
}

/// A thing the engine refuses, with where in the file it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub message: String,
    pub at: Option<At>,
}

/// The entry and, when the message names one, the state or phase of a problem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct At {
    pub entry: usize,
    pub state: Option<usize>,
    pub phase: Option<usize>,
}

/// Reads `ai[2].state[1]` out of an engine message.
pub fn locate(message: &str) -> Option<At> {
    let index = |key: &str| -> Option<usize> {
        let from = message.find(&format!("{key}["))? + key.len() + 1;
        let rest = &message[from..];
        rest[..rest.find(']')?].parse().ok()
    };
    Some(At {
        entry: index("ai")?,
        state: index("state"),
        phase: index("phase"),
    })
}

/// The entries of `ai.toml` plus the bookkeeping to save them safely.
#[derive(Clone, Debug, Default)]
pub struct AiFile {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
    pub dirty: bool,
    on_disk: Option<String>,
    control: Table,
    control_on_disk: Option<String>,
    control_dirty: bool,
}

fn read(root: &Path, rel: &str) -> Result<Option<String>> {
    match fs::read_to_string(root.join(rel)) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {rel}")),
    }
}

impl AiFile {
    /// Reads the project's `ai.toml`; a missing file is an empty list. A file
    /// the editor cannot read is an error and nothing is overwritten.
    pub fn load(root: &Path) -> Result<AiFile> {
        let text = read(root, AI_FILE)?;
        let entries = match &text {
            Some(t) => {
                parse(t).with_context(|| format!("{AI_FILE} cannot be opened in the editor"))?
            }
            None => Vec::new(),
        };
        let control_on_disk = read(root, CONTROL_FILE)?;
        let control = match &control_on_disk {
            Some(text) => {
                d2_sim::rules::data_mod::CcRules::from_toml(text)?;
                text.parse::<Table>()?
            }
            None => Table::new(),
        };
        Ok(AiFile {
            control,
            control_on_disk,
            control_dirty: false,
            root: root.to_owned(),
            entries,
            dirty: false,
            on_disk: text,
        })
    }

    pub fn immune_state(&self) -> Option<u16> {
        self.control
            .get("immune_state")
            .and_then(Value::as_integer)
            .and_then(|n| u16::try_from(n).ok())
    }

    pub fn set_immune_state(&mut self, state: Option<u16>) {
        if self.immune_state() == state {
            return;
        }
        if let Some(state) = state {
            self.control
                .insert("immune_state".into(), Value::Integer(i64::from(state)));
            self.control.entry("version").or_insert(Value::Integer(1));
        } else {
            self.control.remove("immune_state");
        }
        self.control_dirty = true;
        self.dirty = true;
    }

    /// The canonical text of the file.
    pub fn text(&self) -> String {
        write(&self.entries)
    }

    /// Writes the file when it has entries (or exists already). Refuses when it
    /// was changed outside Studio since it was read.
    pub fn save(&mut self) -> Result<()> {
        if read(&self.root, AI_FILE)? != self.on_disk {
            bail!("{AI_FILE} changed outside Studio. Reopen the project before saving.");
        }
        if self.control_dirty && read(&self.root, CONTROL_FILE)? != self.control_on_disk {
            bail!("{CONTROL_FILE} changed outside Studio. Reopen the project before saving.");
        }
        if self.control_dirty {
            let text = toml::to_string_pretty(&self.control)?;
            d2_sim::rules::data_mod::CcRules::from_toml(&text)?;
            fs::create_dir_all(self.root.join("files"))?;
            fs::write(self.root.join(CONTROL_FILE), &text)?;
            self.control_on_disk = Some(text);
            self.control_dirty = false;
        }
        if self.entries.is_empty() && self.on_disk.is_none() {
            self.dirty = false;
            return Ok(());
        }
        let text = self.text();
        let path = self.root.join(AI_FILE);
        fs::create_dir_all(path.parent().context("files folder")?)?;
        fs::write(&path, &text)?;
        self.on_disk = Some(text);
        self.dirty = false;
        Ok(())
    }

    /// The `index` for a new entry: the first one from 200 no entry has.
    pub fn next_index(&self) -> u16 {
        (200..=u16::MAX)
            .find(|i| !self.entries.iter().any(|e| e.index == *i))
            .unwrap_or(200)
    }

    /// Everything the engine objects to, read the way the game reads the mod:
    /// this file together with the project's other rule files, so the
    /// cross-file checks (the immunity state, for one) run too.
    pub fn problems(&self) -> Vec<Problem> {
        let text = self.text();
        let load = |rel: &str| read(&self.root, rel).ok().flatten();
        let rules = load("files/rules.toml");
        let states = load("files/states.toml");
        let skills = load("files/skills.toml");
        let triggers = load("files/triggers.toml");
        let cc = if self.control_dirty {
            (!self.control.is_empty())
                .then(|| toml::to_string_pretty(&self.control).expect("TOML values"))
        } else {
            load(CONTROL_FILE)
        };
        let stats = load("files/stats.toml");
        let files = DataModFiles {
            ai: Some((AI_FILE, text.as_str())),
            rules: rules.as_deref().map(|t| ("files/rules.toml", t)),
            states: states.as_deref().map(|t| ("files/states.toml", t)),
            skills: skills.as_deref().map(|t| ("files/skills.toml", t)),
            triggers: triggers.as_deref().map(|t| ("files/triggers.toml", t)),
            crowd_control: cc.as_deref().map(|t| ("files/crowd_control.toml", t)),
            stats: stats.as_deref().map(|t| ("files/stats.toml", t)),
            ..Default::default()
        };
        match DataMod::from_files("studio", &files) {
            Ok(_) => Vec::new(),
            Err(e) => {
                let message = e.to_string();
                let at = locate(&message);
                vec![Problem { message, at }]
            }
        }
    }

    /// Whether the engine's reader alone accepts the text (no other file).
    pub fn reads(&self) -> Result<(), String> {
        AiRules::from_toml(&self.text())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

// ------------------------------------------------------------------ layout

/// A way out of a state, for drawing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub label: String,
}

/// The transitions of a graph as edges, with a sentence for each.
pub fn edges(e: &Entry) -> Vec<Edge> {
    let mut out = Vec::new();
    for (from, s) in e.states.iter().enumerate() {
        for t in &s.transitions {
            let Some(to) = e.states.iter().position(|x| x.name == t.to) else {
                continue;
            };
            let label = if t.when.is_empty() {
                "always".to_owned()
            } else {
                t.when
                    .iter()
                    .map(|c| c.text(c.condition_info()))
                    .collect::<Vec<_>>()
                    .join(" and ")
            };
            out.push(Edge { from, to, label });
        }
    }
    out
}

/// The `(column, row)` of each state: the start state in the first column, then
/// every step along a transition one column on; states nothing leads to go in
/// the column after the last.
pub fn layout(e: &Entry) -> Vec<(usize, usize)> {
    let n = e.states.len();
    let mut column: Vec<Option<usize>> = vec![None; n];
    if n == 0 {
        return Vec::new();
    }
    let edges = edges(e);
    let mut queue = std::collections::VecDeque::new();
    let start = e.start_position();
    column[start] = Some(0);
    queue.push_back(start);
    while let Some(s) = queue.pop_front() {
        let here = column[s].unwrap_or(0);
        for edge in edges.iter().filter(|x| x.from == s) {
            if column[edge.to].is_none() {
                column[edge.to] = Some(here + 1);
                queue.push_back(edge.to);
            }
        }
    }
    let last = column.iter().flatten().max().copied().unwrap_or(0);
    let mut rows = vec![0usize; n + 2];
    let mut out = Vec::with_capacity(n);
    for c in column {
        let col = c.unwrap_or(last + 1);
        out.push((col, rows[col]));
        rows[col] += 1;
    }
    out
}

// ----------------------------------------------------------------- parsing

fn keyed<'a>(t: &'a mut Table, what: &str) -> Taker<'a> {
    Taker {
        t,
        what: what.to_owned(),
    }
}

struct Taker<'a> {
    t: &'a mut Table,
    what: String,
}

impl Taker<'_> {
    fn string(&mut self, key: &str) -> Result<Option<String>> {
        match self.t.remove(key) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(_) => bail!("{}.{key} is not text", self.what),
        }
    }
    fn number(&mut self, key: &str) -> Result<Option<Value>> {
        match self.t.remove(key) {
            None => Ok(None),
            Some(v @ (Value::Integer(_) | Value::Float(_))) => Ok(Some(v)),
            Some(_) => bail!("{}.{key} is not a number", self.what),
        }
    }
    fn index(&mut self, key: &str) -> Result<Option<u16>> {
        match self.t.remove(key) {
            None => Ok(None),
            Some(Value::Integer(n)) => {
                Ok(Some(u16::try_from(n).with_context(|| {
                    format!("{}.{key} is out of range", self.what)
                })?))
            }
            Some(_) => bail!("{}.{key} is not a whole number", self.what),
        }
    }
    fn tables(&mut self, key: &str) -> Result<Vec<Table>> {
        match self.t.remove(key) {
            None => Ok(Vec::new()),
            Some(Value::Array(a)) => a
                .into_iter()
                .map(|v| match v {
                    Value::Table(t) => Ok(t),
                    _ => bail!("{}.{key} holds something that is not a table", self.what),
                })
                .collect(),
            Some(_) => bail!("{}.{key} is not a list of tables", self.what),
        }
    }
    fn done(self) -> Result<()> {
        if let Some(key) = self.t.keys().next() {
            bail!(
                "{} has the key `{key}`, which the editor does not know",
                self.what
            );
        }
        Ok(())
    }
}

fn item(mut t: Table, what: &str) -> Result<Item> {
    let kind = keyed(&mut t, what)
        .string("kind")?
        .with_context(|| format!("{what} has no kind"))?;
    Ok(Item {
        kind,
        keys: t.into_iter().collect(),
    })
}

fn items(list: Vec<Table>, what: &str) -> Result<Vec<Item>> {
    list.into_iter()
        .enumerate()
        .map(|(i, t)| item(t, &format!("{what}[{i}]")))
        .collect()
}

fn conditions(t: &mut Taker<'_>, key: &str) -> Result<Vec<Item>> {
    let what = format!("{}.{key}", t.what);
    items(t.tables(key)?, &what)
}

fn param(v: Value, what: &str) -> Result<Param> {
    match v {
        Value::Integer(n) => Ok(Param::All(n)),
        Value::Array(a) if a.len() == 3 => {
            let mut out = [0i64; 3];
            for (slot, v) in out.iter_mut().zip(a) {
                *slot = v
                    .as_integer()
                    .with_context(|| format!("{what} holds a non-integer"))?;
            }
            Ok(Param::Each(out))
        }
        _ => bail!("{what} is not a number or three numbers"),
    }
}

/// Reads the text into entries. Anything the editor would lose on writing it
/// back is an error.
pub fn parse(text: &str) -> Result<Vec<Entry>> {
    let mut file: Table = text.parse().context("not valid TOML")?;
    let mut top = keyed(&mut file, "the file");
    top.number("version")?;
    let entries = top.tables("ai")?;
    top.done()?;
    entries
        .into_iter()
        .enumerate()
        .map(|(i, t)| entry(t, i))
        .collect()
}

fn entry(mut t: Table, i: usize) -> Result<Entry> {
    let what = format!("ai[{i}]");
    let mut k = keyed(&mut t, &what);
    let index = k
        .index("index")?
        .with_context(|| format!("{what} has no index"))?;
    let name = k.string("name")?.unwrap_or_default();
    let target = k.string("target")?;
    let start = k.string("start")?;
    let preset = k.string("preset")?;
    let mut params = Vec::new();
    if let Some(v) = k.t.remove("params") {
        let Value::Table(p) = v else {
            bail!("{what}.params is not a table");
        };
        for (key, v) in p {
            let value = param(v, &format!("{what}.params.{key}"))?;
            params.push((key, value));
        }
    }
    let cooldowns = k
        .tables("cooldown")?
        .into_iter()
        .enumerate()
        .map(|(j, mut c)| {
            let w = format!("{what}.cooldown[{j}]");
            let mut kc = keyed(&mut c, &w);
            let out = Cooldown {
                name: kc.string("name")?.unwrap_or_default(),
                seconds: kc.number("seconds")?.unwrap_or(Value::Integer(1)),
            };
            kc.done()?;
            Ok(out)
        })
        .collect::<Result<Vec<_>>>()?;
    let states = k
        .tables("state")?
        .into_iter()
        .enumerate()
        .map(|(j, s)| state(s, &format!("{what}.state[{j}]")))
        .collect::<Result<Vec<_>>>()?;
    let phases = k
        .tables("phase")?
        .into_iter()
        .enumerate()
        .map(|(j, p)| phase(p, &format!("{what}.phase[{j}]")))
        .collect::<Result<Vec<_>>>()?;
    k.done()?;
    Ok(Entry {
        index,
        name,
        target,
        start,
        preset,
        params,
        cooldowns,
        states,
        phases,
    })
}

fn state(mut t: Table, what: &str) -> Result<State> {
    let mut k = keyed(&mut t, what);
    let name = k.string("name")?.unwrap_or_default();
    let transitions = k
        .tables("transition")?
        .into_iter()
        .enumerate()
        .map(|(j, mut x)| {
            let w = format!("{what}.transition[{j}]");
            let mut kt = keyed(&mut x, &w);
            let out = Transition {
                to: kt.string("to")?.unwrap_or_default(),
                when: conditions(&mut kt, "when")?,
            };
            kt.done()?;
            Ok(out)
        })
        .collect::<Result<Vec<_>>>()?;
    let actions = k
        .tables("action")?
        .into_iter()
        .enumerate()
        .map(|(j, mut x)| {
            let w = format!("{what}.action[{j}]");
            let mut ka = keyed(&mut x, &w);
            let when = conditions(&mut ka, "when")?;
            let starts_cooldown = ka.string("starts_cooldown")?;
            Ok(Action {
                item: item(x, &w)?,
                when,
                starts_cooldown,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    k.done()?;
    Ok(State {
        name,
        transitions,
        actions,
    })
}

fn phase(mut t: Table, what: &str) -> Result<Phase> {
    let mut k = keyed(&mut t, what);
    let out = Phase {
        name: k.string("name")?.unwrap_or_default(),
        when: conditions(&mut k, "when")?,
        state: k.string("state")?,
        graph: k.index("graph")?,
        immune_seconds: k.number("immune_seconds")?,
    };
    k.done()?;
    Ok(out)
}

// ----------------------------------------------------------------- writing

fn quoted(s: &str) -> String {
    Value::String(s.to_owned()).to_string()
}

/// `{ kind = "x", a = 1 }` with the keys in the vocabulary's order.
fn inline(it: &Item, info: Option<&KindInfo>) -> String {
    let mut out = format!("{{ kind = {}", quoted(&it.kind));
    let mut done: Vec<&str> = Vec::new();
    if let Some(info) = info {
        for f in info.fields {
            if let Some(v) = it.keys.get(f.key) {
                out.push_str(&format!(", {} = {v}", f.key));
                done.push(f.key);
            }
        }
    }
    for (k, v) in &it.keys {
        if !done.contains(&k.as_str()) {
            out.push_str(&format!(", {k} = {v}"));
        }
    }
    out.push_str(" }");
    out
}

fn when_line(out: &mut String, list: &[Item]) {
    if list.is_empty() {
        return;
    }
    let parts: Vec<String> = list.iter().map(|c| inline(c, c.condition_info())).collect();
    out.push_str(&format!("when = [{}]\n", parts.join(", ")));
}

/// The canonical text of a list of entries.
pub fn write(entries: &[Entry]) -> String {
    let mut out = String::from("version = 1\n");
    for e in entries {
        out.push_str(&format!("\n[[ai]]\nindex = {}\n", e.index));
        if !e.name.is_empty() {
            out.push_str(&format!("name = {}\n", quoted(&e.name)));
        }
        if let Some(t) = &e.target {
            out.push_str(&format!("target = {}\n", quoted(t)));
        }
        if let Some(s) = &e.start {
            out.push_str(&format!("start = {}\n", quoted(s)));
        }
        if let Some(p) = &e.preset {
            out.push_str(&format!("preset = {}\n", quoted(p)));
        }
        if !e.params.is_empty() {
            out.push_str("\n[ai.params]\n");
            for (k, v) in &e.params {
                match v {
                    Param::All(n) => out.push_str(&format!("{k} = {n}\n")),
                    Param::Each([a, b, c]) => out.push_str(&format!("{k} = [{a}, {b}, {c}]\n")),
                }
            }
        }
        for c in &e.cooldowns {
            out.push_str(&format!(
                "\n[[ai.cooldown]]\nname = {}\nseconds = {}\n",
                quoted(&c.name),
                c.seconds
            ));
        }
        for p in &e.phases {
            out.push_str(&format!("\n[[ai.phase]]\nname = {}\n", quoted(&p.name)));
            when_line(&mut out, &p.when);
            if let Some(s) = &p.state {
                out.push_str(&format!("state = {}\n", quoted(s)));
            }
            if let Some(g) = p.graph {
                out.push_str(&format!("graph = {g}\n"));
            }
            if let Some(s) = &p.immune_seconds {
                out.push_str(&format!("immune_seconds = {s}\n"));
            }
        }
        for s in &e.states {
            out.push_str(&format!("\n[[ai.state]]\nname = {}\n", quoted(&s.name)));
            for t in &s.transitions {
                out.push_str(&format!(
                    "\n[[ai.state.transition]]\nto = {}\n",
                    quoted(&t.to)
                ));
                when_line(&mut out, &t.when);
            }
            for a in &s.actions {
                out.push_str(&format!(
                    "\n[[ai.state.action]]\nkind = {}\n",
                    quoted(&a.item.kind)
                ));
                let info = a.item.action_info();
                let mut done: Vec<&str> = Vec::new();
                if let Some(info) = info {
                    for f in info.fields {
                        if let Some(v) = a.item.keys.get(f.key) {
                            out.push_str(&format!("{} = {v}\n", f.key));
                            done.push(f.key);
                        }
                    }
                }
                for (k, v) in &a.item.keys {
                    if !done.contains(&k.as_str()) {
                        out.push_str(&format!("{k} = {v}\n"));
                    }
                }
                when_line(&mut out, &a.when);
                if let Some(c) = &a.starts_cooldown {
                    out.push_str(&format!("starts_cooldown = {}\n", quoted(c)));
                }
            }
        }
    }
    out
}
