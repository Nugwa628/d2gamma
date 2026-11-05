//! Versioned visual programs compile to ordinary M2 Luau, without a second
//! runtime. Control edges are acyclic; only bounded loop nodes repeat work.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_BYTES: usize = 256 * 1024;
pub const MAX_NODES: usize = 256;
pub const MAX_STEPS: usize = 4096;
pub const EVENTS: [&str; 12] = [
    "attack_start",
    "attack_hit",
    "cast_start",
    "cast_complete",
    "skill_used",
    "hit_taken",
    "state_start",
    "state_end",
    "kill",
    "drop",
    "buff_end",
    "timer",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Integer,
    Boolean,
    Text,
    Unit,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Input {
    Integer(i32),
    Boolean(bool),
    Text(String),
    Link { node: String, pin: String },
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub op: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub event: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub next: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub yes: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub no: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    #[serde(default)]
    pub count: u16,
    #[serde(default)]
    pub inputs: BTreeMap<String, Input>,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub version: u16,
    pub nodes: Vec<Node>,
}
impl Default for Graph {
    fn default() -> Self {
        Self {
            version: 1,
            nodes: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub node: String,
    pub message: String,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "node {}: {}", self.node, self.message)
    }
}
impl std::error::Error for Error {}
fn error(node: &str, message: impl Into<String>) -> Error {
    Error {
        node: node.into(),
        message: message.into(),
    }
}
pub fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || i > 0 && b.is_ascii_digit())
}
/// Numeric escapes prevent names/text from injecting source, including newlines.
fn quote(s: &str) -> String {
    format!(
        "\"{}\"",
        s.bytes().map(|b| format!("\\{b:03}")).collect::<String>()
    )
}
pub struct Api {
    pub name: &'static str,
    pub inputs: &'static [(&'static str, Type)],
    pub output: Option<Type>,
}
use Type::{Boolean as B, Integer as I, Text as S, Unit as U};
pub const APIS: &[Api] = &[
    Api {
        name: "stat",
        inputs: &[("unit", U), ("stat", I), ("layer", I)],
        output: Some(I),
    },
    Api {
        name: "base_stat",
        inputs: &[("unit", U), ("stat", I), ("layer", I)],
        output: Some(I),
    },
    Api {
        name: "set_stat",
        inputs: &[("unit", U), ("stat", I), ("layer", I), ("value", I)],
        output: None,
    },
    Api {
        name: "has_state",
        inputs: &[("unit", U), ("state", I)],
        output: Some(B),
    },
    Api {
        name: "state_stacks",
        inputs: &[("unit", U), ("state", I)],
        output: Some(I),
    },
    Api {
        name: "apply_state",
        inputs: &[
            ("unit", U),
            ("target", U),
            ("state", I),
            ("ticks", I),
            ("skill", I),
        ],
        output: None,
    },
    Api {
        name: "add_stacks",
        inputs: &[
            ("unit", U),
            ("target", U),
            ("state", I),
            ("ticks", I),
            ("skill", I),
            ("count", I),
        ],
        output: None,
    },
    Api {
        name: "remove_stacks",
        inputs: &[("unit", U), ("state", I), ("count", I)],
        output: None,
    },
    Api {
        name: "clear_state",
        inputs: &[("unit", U), ("state", I)],
        output: None,
    },
    Api {
        name: "damage",
        inputs: &[("unit", U), ("target", U), ("amount", I), ("element", I)],
        output: None,
    },
    Api {
        name: "spawn_missile",
        inputs: &[
            ("unit", U),
            ("missile", I),
            ("skill", I),
            ("level", I),
            ("lob", I),
        ],
        output: Some(U),
    },
    Api {
        name: "spawn_monster",
        inputs: &[
            ("unit", U),
            ("class", I),
            ("x", I),
            ("y", I),
            ("pet_type", I),
            ("maximum", I),
        ],
        output: Some(U),
    },
    Api {
        name: "message",
        inputs: &[("unit", U), ("text", S)],
        output: None,
    },
    Api {
        name: "ui_variable",
        inputs: &[("unit", U), ("name", S), ("value", I)],
        output: None,
    },
    Api {
        name: "random",
        inputs: &[("unit", U), ("upper", I)],
        output: Some(I),
    },
    Api {
        name: "timer",
        inputs: &[("unit", U), ("frames", I), ("cookie", I)],
        output: None,
    },
];
pub fn api(name: &str) -> Option<&'static Api> {
    APIS.iter().find(|a| a.name == name)
}
impl Node {
    pub fn executable(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "event" | "call" | "api" | "branch" | "loop" | "return"
        ) || self.kind == "variable" && self.op == "set"
    }
    pub fn flows(&self) -> Vec<(&str, &str)> {
        match self.kind.as_str() {
            "branch" => vec![("yes", &self.yes), ("no", &self.no)],
            "loop" => vec![("body", &self.body), ("next", &self.next)],
            "return" => Vec::new(),
            _ if self.executable() => vec![("next", &self.next)],
            _ => Vec::new(),
        }
    }
    pub fn output(&self, pin: &str) -> Option<Type> {
        if self.kind == "event" || self.kind == "call" {
            return match pin {
                "unit" | "other" => Some(U),
                "id" | "frame" | "unit_type" | "argument" | "skill" | "level" => Some(I),
                _ => None,
            };
        }
        if pin != "value" {
            return None;
        }
        match self.kind.as_str() {
            "constant" => self.inputs.get("value").and_then(|v| match v {
                Input::Integer(_) => Some(I),
                Input::Boolean(_) => Some(B),
                Input::Text(_) => Some(S),
                _ => None,
            }),
            "math" | "variable" => Some(I),
            "condition" => Some(B),
            "api" => api(&self.name).and_then(|a| a.output),
            _ => None,
        }
    }
    pub fn pins(&self) -> Result<Vec<(&str, Type)>, Error> {
        Ok(match self.kind.as_str() {
            "event" if EVENTS.contains(&self.event.as_str()) => vec![],
            "call"
                if identifier(&self.name)
                    && !self.name.starts_with("on_")
                    && !self.name.starts_with("__") =>
            {
                vec![]
            }
            "constant" if self.output("value").is_some() => {
                vec![("value", self.output("value").unwrap())]
            }
            "math"
                if [
                    "add", "subtract", "multiply", "divide", "modulo", "min", "max",
                ]
                .contains(&self.op.as_str()) =>
            {
                vec![("a", I), ("b", I)]
            }
            "condition"
                if ["equal", "less", "less_equal", "greater", "greater_equal"]
                    .contains(&self.op.as_str()) =>
            {
                vec![("a", I), ("b", I)]
            }
            "condition" if ["and", "or"].contains(&self.op.as_str()) => vec![("a", B), ("b", B)],
            "condition" if self.op == "not" => vec![("a", B)],
            "condition" if self.op == "exists" => vec![("a", U)],
            "branch" => vec![("condition", B)],
            "loop" if (1..=64).contains(&self.count) => vec![],
            "variable" if identifier(&self.name) && self.op == "get" => vec![],
            "variable" if identifier(&self.name) && self.op == "set" => vec![("value", I)],
            "api" => api(&self.name)
                .ok_or_else(|| error(&self.id, "unknown M2 API"))?
                .inputs
                .to_vec(),
            "return" => vec![("value", I)],
            _ => {
                return Err(error(
                    &self.id,
                    "invalid kind, event, operation, name or loop count (1..64)",
                ))
            }
        })
    }
}
impl Graph {
    pub fn parse(text: &str) -> Result<Self, Error> {
        if text.len() > MAX_BYTES {
            return Err(error("document", "graph exceeds 256 KiB"));
        }
        toml::from_str(text).map_err(|e| error("document", e.to_string()))
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.version != 1 || self.nodes.is_empty() || self.nodes.len() > MAX_NODES {
            return Err(error("document", "expected version 1 and 1..256 nodes"));
        }
        let mut ids = BTreeSet::new();
        let mut entries = BTreeSet::new();
        for n in &self.nodes {
            if !identifier(&n.id) || !ids.insert(&n.id) {
                return Err(error(&n.id, "invalid or duplicate ID"));
            }
            if n.kind == "event" || n.kind == "call" {
                let name = if n.kind == "event" {
                    format!("on_{}", n.event)
                } else {
                    n.name.clone()
                };
                if !entries.insert(name) {
                    return Err(error(&n.id, "duplicate entry function"));
                }
            }
        }
        if entries.is_empty() {
            return Err(error("document", "graph needs an event or call entry"));
        }
        for n in &self.nodes {
            let pins = n.pins()?;
            if n.inputs.len() != pins.len() {
                return Err(error(&n.id, "missing or unexpected input pin"));
            }
            for (pin, expected) in pins {
                let input = n
                    .inputs
                    .get(pin)
                    .ok_or_else(|| error(&n.id, format!("missing input {pin}")))?;
                if self.input_type(input).ok() != Some(expected) {
                    return Err(error(
                        &n.id,
                        format!(
                            "input {pin} needs {expected:?}; invalid source or incompatible pin"
                        ),
                    ));
                }
                if let Input::Text(s) = input {
                    if s.len() > 200 {
                        return Err(error(&n.id, "text input exceeds 200 bytes"));
                    }
                }
            }
            for (_, dest) in n.flows().into_iter().filter(|(_, d)| !d.is_empty()) {
                let target = self
                    .node(dest)
                    .ok_or_else(|| error(&n.id, format!("missing control target {dest}")))?;
                if !target.executable() || matches!(target.kind.as_str(), "event" | "call") {
                    return Err(error(
                        &n.id,
                        "control target must be an executable non-entry node",
                    ));
                }
            }
            // Bound recursive expression expansion, including disconnected nodes.
            for input in n.inputs.values() {
                self.expression(input, &mut BTreeSet::new(), 0, &mut 0)?;
            }
            self.check_flow(&n.id, &mut BTreeSet::new(), &mut BTreeSet::new())?;
        }
        self.check_assignments()?;
        Ok(())
    }
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
    fn check_assignments(&self) -> Result<(), Error> {
        // A producer must run on every path to its consumer. Pure expression
        // nodes are expanded at the consumer, rather than retaining stale data.
        let executable: BTreeSet<String> = self
            .nodes
            .iter()
            .filter(|n| n.executable())
            .map(|n| n.id.clone())
            .collect();
        let mut reachable = BTreeSet::new();
        let mut pending = self
            .nodes
            .iter()
            .filter(|n| matches!(n.kind.as_str(), "event" | "call"))
            .map(|n| n.id.clone())
            .collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if !reachable.insert(id.clone()) {
                continue;
            }
            for (_, target) in self.node(&id).unwrap().flows() {
                if !target.is_empty() {
                    pending.push(target.into());
                }
            }
        }
        let mut dominators: BTreeMap<String, BTreeSet<String>> = executable
            .iter()
            .map(|id| (id.clone(), executable.clone()))
            .collect();
        for _ in 0..=self.nodes.len() {
            let mut changed = false;
            for n in self.nodes.iter().filter(|n| n.executable()) {
                let parents = self
                    .nodes
                    .iter()
                    .filter(|p| {
                        reachable.contains(&p.id) && p.flows().iter().any(|(_, d)| *d == n.id)
                    })
                    .collect::<Vec<_>>();
                let mut set = if !reachable.contains(&n.id)
                    || matches!(n.kind.as_str(), "event" | "call")
                    || parents.is_empty()
                {
                    BTreeSet::new()
                } else {
                    let mut set = dominators[&parents[0].id].clone();
                    for p in &parents[1..] {
                        set.retain(|id| dominators[&p.id].contains(id));
                    }
                    set
                };
                set.insert(n.id.clone());
                if dominators[&n.id] != set {
                    dominators.insert(n.id.clone(), set);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        fn dependencies(g: &Graph, input: &Input, out: &mut BTreeSet<String>) {
            if let Input::Link { node, .. } = input {
                let n = g.node(node).unwrap();
                if n.kind == "api" {
                    out.insert(node.clone());
                } else {
                    for v in n.inputs.values() {
                        dependencies(g, v, out);
                    }
                }
            }
        }
        for n in self.nodes.iter().filter(|n| n.executable()) {
            let mut deps = BTreeSet::new();
            for input in n.inputs.values() {
                dependencies(self, input, &mut deps);
            }
            for producer in deps {
                if producer == n.id || !dominators[&n.id].contains(&producer) {
                    return Err(error(
                        &n.id,
                        format!("API output {producer} must run on every incoming control path"),
                    ));
                }
            }
        }
        Ok(())
    }
    fn input_type(&self, v: &Input) -> Result<Type, Error> {
        match v {
            Input::Integer(_) => Ok(I),
            Input::Boolean(_) => Ok(B),
            Input::Text(_) => Ok(S),
            Input::Link { node, pin } => self
                .node(node)
                .and_then(|n| n.output(pin))
                .ok_or_else(|| error(node, "unknown output pin")),
        }
    }
    fn check_flow(
        &self,
        id: &str,
        path: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> Result<(), Error> {
        if done.contains(id) {
            return Ok(());
        }
        if !path.insert(id.into()) {
            return Err(error(id, "control cycle; use a bounded loop node"));
        }
        let n = self.node(id).ok_or_else(|| error(id, "missing node"))?;
        for (_, dest) in n.flows().into_iter().filter(|(_, d)| !d.is_empty()) {
            self.check_flow(dest, path, done)?;
        }
        path.remove(id);
        done.insert(id.into());
        Ok(())
    }
    fn expression(
        &self,
        v: &Input,
        path: &mut BTreeSet<String>,
        depth: usize,
        work: &mut usize,
    ) -> Result<String, Error> {
        *work += 1;
        if depth > 32 || *work > 1024 {
            return Err(error("expression", "expression exceeds depth/work limit"));
        }
        Ok(match v {
            Input::Integer(v) => v.to_string(),
            Input::Boolean(v) => v.to_string(),
            Input::Text(v) => quote(v),
            Input::Link { node, pin } => {
                if !path.insert(node.clone()) {
                    return Err(error(node, "data cycle"));
                }
                let n = self
                    .node(node)
                    .ok_or_else(|| error(node, "missing source node"))?;
                let mut arg = |key: &str| {
                    self.expression(
                        n.inputs
                            .get(key)
                            .ok_or_else(|| error(node, format!("missing input {key}")))?,
                        path,
                        depth + 1,
                        work,
                    )
                };
                let expr = match n.kind.as_str() {
                    "event" | "call" => match pin.as_str() {
                        "unit" | "other" => format!("e.{pin}"),
                        _ => format!("(e.{pin} or 0)"),
                    },
                    "constant" => arg("value")?,
                    "variable" if n.op == "get" => format!("(vars[{}] or 0)", quote(&n.name)),
                    "math" => {
                        let a = arg("a")?;
                        let b = arg("b")?;
                        match n.op.as_str() {
                            "add" => format!("int({a} + {b})"),
                            "subtract" => format!("int({a} - {b})"),
                            "multiply" => format!("int({a} * {b})"),
                            "divide" => format!("div({a}, {b})"),
                            "modulo" => format!("mod({a}, {b})"),
                            "min" | "max" => format!("math.{}({a}, {b})", n.op),
                            _ => return Err(error(node, "unknown math operation")),
                        }
                    }
                    "condition" => {
                        let a = arg("a")?;
                        if n.op == "not" {
                            format!("(not {a})")
                        } else if n.op == "exists" {
                            format!("({a} ~= nil)")
                        } else {
                            let b = arg("b")?;
                            let op = match n.op.as_str() {
                                "equal" => "==",
                                "less" => "<",
                                "less_equal" => "<=",
                                "greater" => ">",
                                "greater_equal" => ">=",
                                "and" => "and",
                                "or" => "or",
                                _ => return Err(error(node, "unknown condition")),
                            };
                            format!("({a} {op} {b})")
                        }
                    }
                    "api" => format!(
                        "read({}, {})",
                        quote(node),
                        quote(&format!("node {node}: value used before its API node ran"))
                    ),
                    _ => return Err(error(node, "node has no value")),
                };
                path.remove(node);
                expr
            }
        })
    }
    /// Node order and canvas positions never affect generated execution order.
    /// `trace` is supplied only by the Studio fake-event runner.
    pub fn compile(&self) -> Result<String, Error> {
        self.validate()?;
        let mut nodes = self.nodes.iter().collect::<Vec<_>>();
        nodes.sort_by_key(|n| &n.id);
        let mut out=String::from("-- Visual graph API 1; generated, edit the .graph.toml source.\nlocal vars = {}\nlocal function int(v) assert(v == math.floor(v) and v >= -2147483648 and v <= 2147483647, 'graph integer overflow'); return v end\nlocal function div(a,b) assert(b ~= 0, 'graph division by zero'); local v=a/b; return int(v < 0 and math.ceil(v) or math.floor(v)) end\nlocal function mod(a,b) assert(b ~= 0, 'graph modulo by zero'); return int(a % b) end\nlocal function run(start,e,api,trace)\n local pc=start; local values={}; local assigned={}; local loops={}; local steps=0\n local function read(id,message) assert(assigned[id],message); return values[id] end\n while pc ~= '' or #loops > 0 do\n  if pc == '' then\n   local l=loops[#loops]; l.left-=1\n   if l.left > 0 then pc=l.body else pc=l.next; loops[#loops]=nil end\n  else\n   steps+=1; assert(steps <= 4096, 'graph step budget exceeded at node '..pc)\n   if trace then trace(pc) end\n");
        for (i, n) in nodes.iter().filter(|n| n.executable()).enumerate() {
            // Source IDs appear in assertions and the step trace; source-map users
            // can also find this marker when a VM reports a line number.
            out.push_str(&format!(
                "   {} pc == {} then -- node {}\n",
                if i == 0 { "if" } else { "elseif" },
                quote(&n.id),
                n.id
            ));
            let arg = |key: &str| {
                self.expression(
                    n.inputs
                        .get(key)
                        .ok_or_else(|| error(&n.id, "missing input"))?,
                    &mut BTreeSet::new(),
                    0,
                    &mut 0,
                )
            };
            match n.kind.as_str() {
                "event" | "call" => out.push_str(&format!("    pc={}\n", quote(&n.next))),
                "branch" => out.push_str(&format!(
                    "    if {} then pc={} else pc={} end\n",
                    arg("condition")?,
                    quote(&n.yes),
                    quote(&n.no)
                )),
                "loop" => out.push_str(&format!(
                    "    loops[#loops+1]={{left={},body={},next={}}}; pc={}\n",
                    n.count,
                    quote(&n.body),
                    quote(&n.next),
                    quote(&n.body)
                )),
                "variable" => out.push_str(&format!(
                    "    vars[{}]={}; pc={}\n",
                    quote(&n.name),
                    arg("value")?,
                    quote(&n.next)
                )),
                "api" => {
                    let args = api(&n.name)
                        .unwrap()
                        .inputs
                        .iter()
                        .map(|(p, _)| arg(p))
                        .collect::<Result<Vec<_>, _>>()?
                        .join(", ");
                    out.push_str(&format!(
                        "    values[{}]=api.{}({args}); assigned[{}]=true; pc={}\n",
                        quote(&n.id),
                        n.name,
                        quote(&n.id),
                        quote(&n.next)
                    ));
                }
                "return" => out.push_str(&format!("    return {}\n", arg("value")?)),
                _ => unreachable!(),
            }
        }
        out.push_str("   else error('unknown graph node '..pc) end\n  end\n end\n return 0\nend\nlocal M={}\n");
        for n in nodes
            .iter()
            .filter(|n| matches!(n.kind.as_str(), "event" | "call"))
        {
            let name = if n.kind == "event" {
                format!("on_{}", n.event)
            } else {
                n.name.clone()
            };
            out.push_str(&format!(
                "M[{}]=function(e,api,trace) return run({},e,api,trace) end\n",
                quote(&name),
                quote(&n.id)
            ));
        }
        out.push_str("return M\n");
        if out.len() > MAX_BYTES {
            return Err(error("document", "generated source exceeds 256 KiB"));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests;
