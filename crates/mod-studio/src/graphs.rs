//! Graph buffers preserve invalid drafts and refuse external edits and links.
use anyhow::{bail, Context, Result};
use d2_modkit::graph::{Graph, Input, Node, Type};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
#[derive(Clone)]
pub struct Document {
    pub name: String,
    pub text: String,
    saved: Option<String>,
}
impl Document {
    pub fn graph(&self) -> Result<Graph> {
        Graph::parse(&self.text).map_err(Into::into)
    }
    pub fn replace(&mut self, g: &Graph) -> Result<()> {
        self.text = toml::to_string_pretty(g)?;
        Ok(())
    }
    pub fn dirty(&self) -> bool {
        self.saved.as_ref() != Some(&self.text)
    }
}
#[derive(Default)]
pub struct Graphs {
    root: Option<PathBuf>,
    pub documents: Vec<Document>,
}
impl Graphs {
    pub fn open(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let mut documents = Vec::new();
        let dir = root.join("files/scripts");
        if dir.exists() {
            collect(&root, &dir, &mut documents)?;
        }
        documents.sort_by(|a, b| a.name.cmp(&b.name));
        if documents.iter().map(|d| d.text.len()).sum::<usize>() > 1024 * 1024 {
            bail!("Graphs exceed 1 MiB total");
        }
        Ok(Self {
            root: Some(root),
            documents,
        })
    }
    pub fn dirty(&self) -> bool {
        self.documents.iter().any(Document::dirty)
    }
    pub fn add(&mut self, name: &str) -> Result<usize> {
        let root = self.root.as_ref().context("Open a project")?;
        let path = checked(root, name)?;
        if self.documents.len() >= 64 {
            bail!("At most 64 graph modules");
        }
        if path.exists()
            || self
                .documents
                .iter()
                .any(|d| d.name.eq_ignore_ascii_case(name))
        {
            bail!("Graph already exists");
        }
        let g = Graph {
            version: 1,
            nodes: vec![Node {
                id: "entry".into(),
                kind: "event".into(),
                event: "kill".into(),
                x: 30,
                y: 30,
                ..Default::default()
            }],
        };
        let mut doc = Document {
            name: name.into(),
            text: String::new(),
            saved: None,
        };
        doc.replace(&g)?;
        self.documents.push(doc);
        Ok(self.documents.len() - 1)
    }
    pub fn reload(&mut self, index: usize) -> Result<()> {
        let root = self.root.as_ref().context("Open a project")?;
        let d = self.documents.get_mut(index).context("Choose graph")?;
        let text = read(&checked(root, &d.name)?)?.context("Graph no longer exists")?;
        d.text = text.clone();
        d.saved = Some(text);
        Ok(())
    }
    pub fn save(&mut self) -> Result<bool> {
        let Some(root) = &self.root else {
            return Ok(false);
        };
        let mut pending = Vec::new();
        if self.documents.iter().map(|d| d.text.len()).sum::<usize>() > 1024 * 1024 {
            bail!("Graphs exceed 1 MiB total");
        }
        for (i, d) in self.documents.iter().enumerate().filter(|(_, d)| d.dirty()) {
            if d.text.len() > d2_modkit::graph::MAX_BYTES {
                bail!("{} exceeds 256 KiB", d.name);
            }
            let path = checked(root, &d.name)?;
            if read(&path)? != d.saved {
                bail!("{} changed outside Studio; copy edits then reload", d.name);
            }
            pending.push((i, path));
        }
        let changed = !pending.is_empty();
        for (i, path) in pending {
            fs::create_dir_all(path.parent().unwrap())?;
            let d = &mut self.documents[i];
            fs::write(path, &d.text)?;
            d.saved = Some(d.text.clone());
        }
        Ok(changed)
    }
}
fn checked(root: &Path, name: &str) -> Result<PathBuf> {
    if name.len() > 128
        || !name.ends_with(".graph.toml")
        || name.split('/').any(|s| {
            s.is_empty()
                || s == "."
                || s == ".."
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
    {
        bail!("Use a portable scripts/ path ending .graph.toml");
    }
    let mut path = root.to_path_buf();
    for segment in ["files", "scripts"].into_iter().chain(name.split('/')) {
        path.push(segment);
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if meta.file_type().is_symlink() || !path.canonicalize()?.starts_with(root) {
                bail!("Graph path leaves project or contains a link");
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    bail!("Graph path contains a reparse point");
                }
            }
        }
    }
    Ok(path)
}
fn read(path: &Path) -> Result<Option<String>> {
    match fs::File::open(path) {
        Ok(file) => {
            let mut text = String::new();
            file.take((d2_modkit::graph::MAX_BYTES + 1) as u64)
                .read_to_string(&mut text)?;
            if text.len() > d2_modkit::graph::MAX_BYTES {
                bail!("Graph exceeds 256 KiB");
            }
            Ok(Some(text))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn collect(root: &Path, dir: &Path, out: &mut Vec<Document>) -> Result<()> {
    let relative = dir
        .strip_prefix(root.join("files/scripts"))?
        .join("probe.graph.toml")
        .to_string_lossy()
        .replace('\\', "/");
    checked(root, &relative)?;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            collect(root, &entry.path(), out)?;
        } else if ty.is_symlink() {
            bail!("Graph folders cannot contain symbolic links");
        } else if entry.file_name().to_string_lossy().ends_with(".graph.toml") {
            let name = entry
                .path()
                .strip_prefix(root.join("files/scripts"))?
                .to_string_lossy()
                .replace('\\', "/");
            let text = read(&checked(root, &name)?)?.context("Graph disappeared")?;
            out.push(Document {
                name,
                saved: Some(text.clone()),
                text,
            });
            if out.len() > 64 {
                bail!("At most 64 graph modules");
            }
        }
    }
    Ok(())
}
/// Shared by canvas wiring and tests. Incomplete graphs remain editable.
pub fn connect(g: &mut Graph, source: &str, output: &str, target: &str, input: &str) -> Result<()> {
    let from = g.node(source).context("Missing source")?;
    let ty = from.output(output).context("Missing output pin")?;
    let to = g.node(target).context("Missing target")?;
    let expected = to
        .pins()?
        .iter()
        .find(|(name, _)| *name == input)
        .map(|(_, ty)| *ty)
        .context("Missing input pin")?;
    if ty != expected {
        bail!("Incompatible pins: {ty:?} to {expected:?}");
    }
    let to = g.nodes.iter_mut().find(|n| n.id == target).unwrap();
    to.inputs.insert(
        input.into(),
        Input::Link {
            node: source.into(),
            pin: output.into(),
        },
    );
    Ok(())
}
/// Rename a node without breaking data wires or control edges, including draft edges.
pub fn rename_node(g: &mut Graph, old: &str, new: &str) -> Result<()> {
    anyhow::ensure!(d2_modkit::graph::identifier(new), "Use a node ID of at most 64 letters, digits or underscores, starting with a letter or underscore");
    anyhow::ensure!(g.node(old).is_some(), "Missing node");
    anyhow::ensure!(
        old == new || g.node(new).is_none(),
        "Node ID already exists"
    );
    for n in &mut g.nodes {
        if n.id == old {
            n.id = new.into();
        }
        for target in [&mut n.next, &mut n.yes, &mut n.no, &mut n.body] {
            if target == old {
                *target = new.into();
            }
        }
        for value in n.inputs.values_mut() {
            if let Input::Link { node, .. } = value {
                if node == old {
                    *node = new.into();
                }
            }
        }
    }
    Ok(())
}
pub fn default_input(ty: Type, entry: Option<&str>) -> Input {
    match ty {
        Type::Integer => Input::Integer(1),
        Type::Boolean => Input::Boolean(true),
        Type::Text => Input::Text("A friendly echo".into()),
        Type::Unit => Input::Link {
            node: entry.unwrap_or("entry").into(),
            pin: "unit".into(),
        },
    }
}
pub fn add_node(g: &mut Graph, kind: &str, name: &str) -> Result<String> {
    if g.nodes.len() >= d2_modkit::graph::MAX_NODES {
        bail!("At most 256 nodes");
    }
    let id = (1..)
        .map(|i| format!("node_{i}"))
        .find(|id| {
            g.node(id).is_none()
                && !g.nodes.iter().any(|n| {
                    n.flows().iter().any(|(_, dest)| *dest == id)
                        || n.inputs
                            .values()
                            .any(|v| matches!(v,Input::Link{node,..} if node==id))
                })
        })
        .unwrap();
    let mut n = Node {
        id: id.clone(),
        kind: kind.into(),
        name: name.into(),
        x: 30 + (g.nodes.len() % 4) as i32 * 260,
        y: 30 + (g.nodes.len() / 4) as i32 * 300,
        ..Default::default()
    };
    match kind {
        "event" => {
            n.event = name.into();
            n.name.clear();
        }
        "math" => n.op = "add".into(),
        "condition" => n.op = "less".into(),
        "variable" => {
            n.op = "get".into();
            n.name = "counter".into();
        }
        "loop" => n.count = 3,
        "constant" => {
            n.inputs = BTreeMap::from([("value".into(), Input::Integer(1))]);
        }
        "call" => n.name = "invoke".into(),
        _ => {}
    }
    if !matches!(kind, "api" | "call" | "variable") {
        n.name.clear();
    }
    let entry = g
        .nodes
        .iter()
        .find(|n| matches!(n.kind.as_str(), "event" | "call"))
        .map(|n| n.id.as_str());
    if let Ok(pins) = n.pins() {
        for (name, ty) in pins
            .into_iter()
            .map(|(n, t)| (n.to_owned(), t))
            .collect::<Vec<_>>()
        {
            n.inputs
                .entry(name)
                .or_insert_with(|| default_input(ty, entry));
        }
    }
    g.nodes.push(n);
    Ok(id)
}
#[cfg(test)]
mod tests;
