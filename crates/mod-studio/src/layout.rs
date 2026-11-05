//! Editable panel documents; preserve all shared action, quest and binding fields.
use anyhow::{bail, Context, Result};
use d2_modkit::ui::Panel;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

pub struct Document {
    pub path: PathBuf,
    pub table: Table,
    pub dirty: bool,
}
impl Document {
    pub fn parse(path: PathBuf, text: &str) -> Result<Self> {
        Panel::parse(text).map_err(anyhow::Error::msg)?;
        Ok(Self {
            path,
            table: text.parse()?,
            dirty: false,
        })
    }
    pub fn definition(&self) -> Result<Panel> {
        Panel::parse(&toml::to_string(&self.table)?).map_err(anyhow::Error::msg)
    }
    pub fn elements(&self) -> &[Value] {
        self.table
            .get("elements")
            .and_then(Value::as_array)
            .map_or(&[], Vec::as_slice)
    }
    pub fn element_mut(&mut self, index: usize) -> Option<&mut Table> {
        self.table
            .get_mut("elements")?
            .as_array_mut()?
            .get_mut(index)?
            .as_table_mut()
    }
    pub fn add(&mut self, kind: &str, x: i32, y: i32) -> Result<usize> {
        if !["frame", "text", "bar", "button", "image"].contains(&kind) {
            bail!("Unknown element kind");
        }
        if self.elements().len() >= 256 {
            bail!("At most 256 elements");
        }
        let mut n = 1;
        let id = loop {
            let id = format!("{kind}_{n}");
            if !self
                .elements()
                .iter()
                .any(|v| v.get("id").and_then(Value::as_str) == Some(&id))
            {
                break id;
            }
            n += 1;
        };
        let text = format!(
            "id='{id}'\nkind='{kind}'\nx={}\ny={}\nwidth=140\nheight=30\ntext='{kind}'\n{}",
            x.clamp(0, 1140),
            y.clamp(0, 690),
            if kind == "image" {
                "image='my-panel'\n"
            } else {
                ""
            }
        );
        let element: Table = text.parse()?;
        let a = self
            .table
            .entry("elements")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .context("elements must be an array")?;
        a.push(Value::Table(element));
        self.dirty = true;
        Ok(a.len() - 1)
    }
    pub fn move_element(&mut self, index: usize, x: i32, y: i32) {
        let Some(e) = self.element_mut(index) else {
            return;
        };
        let w = integer(e, "width", 100).clamp(1, 1280) as i32;
        let h = integer(e, "height", 20).clamp(1, 720) as i32;
        e.insert("x".into(), Value::Integer(i64::from(x.clamp(0, 1280 - w))));
        e.insert("y".into(), Value::Integer(i64::from(y.clamp(0, 720 - h))));
        self.dirty = true;
    }
    pub fn remove(&mut self, index: usize) {
        if let Some(a) = self.table.get_mut("elements").and_then(Value::as_array_mut) {
            if index < a.len() {
                a.remove(index);
                self.dirty = true;
            }
        }
    }
}
pub fn integer(t: &Table, key: &str, default: i64) -> i64 {
    t.get(key).and_then(Value::as_integer).unwrap_or(default)
}
pub fn text<'a>(t: &'a Table, key: &str) -> &'a str {
    t.get(key).and_then(Value::as_str).unwrap_or("")
}

#[derive(Default)]
pub struct Layouts {
    pub root: PathBuf,
    pub documents: Vec<Document>,
}
impl Layouts {
    pub fn open(root: &Path) -> Result<Self> {
        fn scan(dir: &Path, out: &mut Vec<Document>) -> Result<()> {
            if !dir.exists() {
                return Ok(());
            }
            let mut entries = std::fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let kind = e.file_type()?;
                if kind.is_dir() {
                    scan(&e.path(), out)?;
                } else if kind.is_file() && e.path().extension().is_some_and(|x| x == "toml") {
                    if out.len() >= 64 {
                        bail!("At most 64 panels");
                    }
                    let path = e.path();
                    if e.metadata()?.len() > 256 * 1024 {
                        bail!("Panel exceeds 256 KiB");
                    }
                    let text = std::fs::read_to_string(&path)?;
                    out.push(
                        Document::parse(path.clone(), &text)
                            .with_context(|| path.display().to_string())?,
                    );
                }
            }
            Ok(())
        }
        let mut documents = Vec::new();
        scan(&root.join("files/ui"), &mut documents)?;
        Ok(Self {
            root: root.into(),
            documents,
        })
    }
    pub fn dirty(&self) -> bool {
        self.documents.iter().any(|d| d.dirty)
    }
    pub fn create(&mut self, name: &str) -> Result<usize> {
        let body = format!("version=1\nname={name:?}\n");
        let p = Panel::parse(&body).map_err(anyhow::Error::msg)?;
        if self.documents.len() >= 64
            || self
                .documents
                .iter()
                .any(|d| text(&d.table, "name") == p.name)
        {
            bail!("Panel name already exists, or 64 panels reached");
        }
        let path = self.root.join("files/ui").join(format!("{name}.toml"));
        if path.exists()
            || self.documents.iter().any(|d| {
                d.path
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&path.to_string_lossy())
            })
        {
            bail!("File already exists");
        }
        let mut d = Document::parse(path, &body)?;
        d.dirty = true;
        self.documents.push(d);
        Ok(self.documents.len() - 1)
    }
    pub fn save(&mut self) -> Result<()> {
        // Validate every document before any write, including duplicate names/replacements.
        let mut names = std::collections::BTreeSet::new();
        let mut replaces = std::collections::BTreeSet::new();
        let mut writes = Vec::new();
        for (i, d) in self.documents.iter().enumerate() {
            let p = d
                .definition()
                .with_context(|| d.path.display().to_string())?;
            if !names.insert(p.name) || p.replace.is_some_and(|r| !replaces.insert(r)) {
                bail!("Duplicate panel name or replacement");
            }
            if d.dirty {
                writes.push((i, toml::to_string_pretty(&d.table)?));
            }
        }
        for (i, text) in writes {
            let d = &mut self.documents[i];
            std::fs::create_dir_all(d.path.parent().context("Panel has no parent")?)?;
            std::fs::write(&d.path, text)?;
            d.dirty = false;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("studio-layout-{tag}-{}", std::process::id()))
    }
    #[test]
    fn untouched_empty_project_has_no_panel_files() {
        let root = root("empty");
        let mut m = Layouts::open(&root).unwrap();
        assert!(m.documents.is_empty());
        assert!(!m.dirty());
        m.save().unwrap();
        assert!(!root.exists());
    }
    #[test]
    fn palette_geometry_clamps_and_round_trip_preserves_all_services() {
        let root = root("round-trip");
        let mut m = Layouts::open(&root).unwrap();
        let i = m.create("custom_hud").unwrap();
        let doc = &mut m.documents[i];
        let text="version=1\nname='custom_hud'\n[[quests]]\nname='done'\nslot=1\nbit=2\n[[actions]]\nname='allocate'\nstat='strength'\ndelta=1\n[[elements]]\nid='button'\nkind='button'\nx=20\ny=30\nwidth=100\nheight=25\naction='allocate'\n[elements.bind]\nsource='quest.done'\nformat='percent'\n[[elements.bind.colors]]\nminimum=1\nmaximum=10\ncolor=3";
        doc.table = text.parse().unwrap();
        let e = doc.add("image", 5000, -200).unwrap();
        doc.move_element(e, -10, 5000);
        let definition = doc.definition().unwrap();
        assert_eq!(
            (definition.elements[e].x, definition.elements[e].y),
            (0, 690)
        );
        m.save().unwrap();
        assert!(!m.dirty());
        let m = Layouts::open(&root).unwrap();
        assert_eq!(m.documents[0].definition().unwrap(), definition);
        std::fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn invalid_or_duplicate_documents_are_rejected_before_writing() {
        let root = root("invalid");
        let mut m = Layouts::open(&root).unwrap();
        assert!(m.create("../escape").is_err());
        let a = m.create("one").unwrap();
        assert!(m.create("one").is_err());
        let b = m.create("two").unwrap();
        m.documents[b]
            .table
            .insert("name".into(), Value::String("one".into()));
        assert!(m.save().is_err());
        assert!(!root.exists());
        m.documents[b]
            .table
            .insert("name".into(), Value::String("two".into()));
        let i = m.documents[a].add("frame", 0, 0).unwrap();
        m.documents[a]
            .element_mut(i)
            .unwrap()
            .insert("x".into(), Value::Integer(1280));
        assert!(m.save().is_err());
        assert!(!root.exists());
        m.documents[a].move_element(i, 0, 0);
        m.save().unwrap();
        m.documents[a].remove(i);
        assert!(m.documents[a].elements().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
