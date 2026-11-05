//! Readable Luau buffers. Saving never evaluates a module in the editor process.
use anyhow::{bail, Context, Result};
use d2_sim::rules::scripts::{CallSpec, EventKind};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub const STARTER: &str = "local M = {}\n\n-- Add handlers with the event picker.\n\nreturn M\n";
const LIMIT: usize = 1024 * 1024;
const MAX_FILES: usize = 64;

pub struct Buffer {
    pub name: String,
    pub text: String,
    saved: Option<String>,
}
impl Buffer {
    pub fn dirty(&self) -> bool {
        self.saved.as_ref() != Some(&self.text)
    }
}

#[derive(Default)]
pub struct Scripts {
    root: Option<PathBuf>,
    pub files: Vec<Buffer>,
}
impl Scripts {
    pub fn open(project: &Path) -> Result<Self> {
        let root = project.canonicalize()?;
        let mut names = Vec::new();
        let dir = root.join("files/scripts");
        if dir.exists() {
            collect(&root, &dir, &mut names)?;
        }
        names.sort();
        let mut files = Vec::new();
        let mut total = 0;
        for name in names {
            let path = checked_path(&root, &name)?;
            let text = read(&path)?.context("script disappeared while opening")?;
            total += text.len();
            if total > LIMIT {
                bail!("Scripts exceed the 1 MiB total source limit");
            }
            files.push(Buffer {
                name,
                saved: Some(text.clone()),
                text,
            });
        }
        Ok(Self {
            root: Some(root),
            files,
        })
    }
    pub fn dirty(&self) -> bool {
        self.files.iter().any(Buffer::dirty)
    }
    pub fn reload(&mut self, index: usize) -> Result<()> {
        let root = self.root.as_ref().context("Open a project first")?;
        let buffer = self.files.get_mut(index).context("Select a module first")?;
        let text = read(&checked_path(root, &buffer.name)?)?
            .context("This module has no saved file to reload")?;
        buffer.text = text.clone();
        buffer.saved = Some(text);
        Ok(())
    }
    pub fn add(&mut self, name: &str) -> Result<usize> {
        validate_name(name)?;
        let root = self.root.as_ref().context("Open a project first")?;
        if self.files.len() >= MAX_FILES {
            bail!("At most {MAX_FILES} script modules are supported");
        }
        if self.files.iter().any(|b| b.name.eq_ignore_ascii_case(name))
            || checked_path(root, name)?.exists()
        {
            bail!("That module already exists; open it from the file list");
        }
        self.files.push(Buffer {
            name: name.into(),
            text: STARTER.into(),
            saved: None,
        });
        Ok(self.files.len() - 1)
    }
    /// Check every buffer before writing any, including external editor conflicts.
    pub fn save(&mut self) -> Result<bool> {
        let Some(root) = &self.root else {
            return Ok(false);
        };
        if self.files.iter().map(|b| b.text.len()).sum::<usize>() > LIMIT {
            bail!("Scripts exceed the 1 MiB total source limit");
        }
        let mut pending = Vec::new();
        for (i, buffer) in self.files.iter().enumerate().filter(|(_, b)| b.dirty()) {
            if buffer.text.len() > LIMIT {
                bail!("{} exceeds the 1 MiB module limit", buffer.name);
            }
            let path = checked_path(root, &buffer.name)?;
            if read(&path)? != buffer.saved {
                bail!(
                    "{} changed outside Studio. Copy your edits, then reload this module from disk",
                    buffer.name
                );
            }
            pending.push((i, path));
        }
        let changed = !pending.is_empty();
        for (i, path) in pending {
            fs::create_dir_all(path.parent().context("script directory")?)?;
            let buffer = &mut self.files[i];
            fs::write(&path, &buffer.text).with_context(|| format!("Save {}", buffer.name))?;
            buffer.saved = Some(buffer.text.clone());
        }
        Ok(changed)
    }
}

fn validate_name(name: &str) -> Result<()> {
    if !name.ends_with(".luau") {
        bail!("Luau modules must end in .luau; create visual graphs in the Graphs pane");
    }
    CallSpec {
        module: name.into(),
        function: "handler".into(),
        argument: d2_sim::rules::data_mod::Formula::constant(0),
    }
    .check()
    .map_err(anyhow::Error::msg)
}

/// Reject links in every existing component, including Windows junctions.
fn checked_path(root: &Path, name: &str) -> Result<PathBuf> {
    validate_name(name)?;
    let mut path = root.to_path_buf();
    for segment in ["files", "scripts"].into_iter().chain(name.split('/')) {
        path.push(segment);
        match fs::symlink_metadata(&path) {
            Ok(meta) => {
                if meta.file_type().is_symlink() || !path.canonicalize()?.starts_with(root) {
                    bail!("Script path must stay inside the project without symbolic links");
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        bail!("Script paths cannot contain reparse points");
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}
fn read(path: &Path) -> Result<Option<String>> {
    match fs::File::open(path) {
        Ok(file) => {
            let mut text = String::new();
            file.take((LIMIT + 1) as u64).read_to_string(&mut text)?;
            if text.len() > LIMIT {
                bail!("{} exceeds the 1 MiB module limit", path.display());
            }
            Ok(Some(text))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn collect(root: &Path, dir: &Path, names: &mut Vec<String>) -> Result<()> {
    // Validate directories even when they contain no Luau files.
    let relative = dir.strip_prefix(root.join("files/scripts"))?;
    let probe = relative
        .join("probe.luau")
        .to_string_lossy()
        .replace('\\', "/");
    checked_path(root, &probe)?;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            collect(root, &entry.path(), names)?;
        } else if entry.path().extension().is_some_and(|s| s == "luau") {
            let name = entry
                .path()
                .strip_prefix(root.join("files/scripts"))?
                .to_string_lossy()
                .replace('\\', "/");
            checked_path(root, &name)?;
            names.push(name);
            if names.len() > MAX_FILES {
                bail!("At most {MAX_FILES} script modules are supported");
            }
        } else if ty.is_symlink() {
            bail!("Script folders cannot contain symbolic links");
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Keyword,
    String,
    Comment,
    Number,
}
#[derive(Debug)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

/// Byte spans always end at UTF-8 boundaries. Long strings/comments honor `=` delimiters.
pub fn tokens(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let kind;
        if source[at..].starts_with("--") {
            kind = Kind::Comment;
            at += 2;
            at = long_end(source, at)
                .unwrap_or_else(|| source[at..].find('\n').map_or(bytes.len(), |n| at + n));
        } else if let Some(end) = long_end(source, at) {
            kind = Kind::String;
            at = end;
        } else if matches!(bytes[at], b'\'' | b'"' | b'`') {
            kind = Kind::String;
            let quote = bytes[at];
            at += 1;
            while at < bytes.len() {
                if bytes[at] == b'\\' {
                    at += 1;
                    if at < bytes.len() {
                        at += source[at..].chars().next().unwrap().len_utf8();
                    }
                } else if bytes[at] == quote {
                    at += 1;
                    break;
                } else {
                    at += source[at..].chars().next().unwrap().len_utf8();
                }
            }
        } else if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            at += 1;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
            kind = if matches!(
                &source[start..at],
                "and"
                    | "break"
                    | "continue"
                    | "do"
                    | "else"
                    | "elseif"
                    | "end"
                    | "export"
                    | "false"
                    | "for"
                    | "function"
                    | "if"
                    | "in"
                    | "local"
                    | "nil"
                    | "not"
                    | "or"
                    | "repeat"
                    | "return"
                    | "then"
                    | "true"
                    | "type"
                    | "until"
                    | "while"
            ) {
                Kind::Keyword
            } else {
                Kind::Plain
            };
        } else if bytes[at].is_ascii_digit() {
            kind = Kind::Number;
            at += 1;
            while at < bytes.len()
                && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'.' | b'_'))
            {
                at += 1;
            }
        } else {
            kind = Kind::Plain;
            at += source[at..].chars().next().unwrap().len_utf8();
        }
        out.push(Token {
            start,
            end: at,
            kind,
        });
    }
    out
}
fn long_end(source: &str, at: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(at) != Some(&b'[') {
        return None;
    }
    let mut n = at + 1;
    while bytes.get(n) == Some(&b'=') {
        n += 1;
    }
    if bytes.get(n) != Some(&b'[') {
        return None;
    }
    let close = format!("]{}]", "=".repeat(n - at - 1));
    Some(
        source[n + 1..]
            .find(&close)
            .map_or(source.len(), |i| n + 1 + i + close.len()),
    )
}

/// Use the stable `local M = {}; return M` module shape created by Studio.
/// Table-return modules get a field before the final closing brace.
pub fn insert_event(source: &mut String, event: EventKind) -> Result<()> {
    let name = format!("on_{}", event.name());
    let spans = tokens(source);
    let code: Vec<_> = spans
        .iter()
        .filter(|t| t.kind != Kind::Comment && !source[t.start..t.end].trim().is_empty())
        .collect();
    if code.iter().any(|t| source[t.start..t.end] == name) {
        bail!("{name} already appears in this module");
    }
    let tail = code.len();
    let tail = if tail > 0 && &source[code[tail - 1].start..code[tail - 1].end] == ";" {
        tail - 1
    } else {
        tail
    };
    if tail >= 2 && &source[code[tail - 2].start..code[tail - 2].end] == "return" {
        let table = &source[code[tail - 1].start..code[tail - 1].end];
        if code[tail - 1].kind == Kind::Plain
            && table
                .bytes()
                .enumerate()
                .all(|(i, c)| c.is_ascii_alphabetic() || c == b'_' || (i > 0 && c.is_ascii_digit()))
            && !table.is_empty()
        {
            let stub = format!("function {table}.{name}(e, api)\n    -- e.unit is the event unit; api exposes server capabilities.\nend\n\n");
            source.insert_str(code[tail - 2].start, &stub);
            return Ok(());
        }
    }
    if tail >= 3 && &source[code[tail - 1].start..code[tail - 1].end] == "}" {
        let mut depth = 0;
        for i in (0..tail).rev() {
            let t = code[i];
            if matches!(t.kind, Kind::String) {
                continue;
            }
            match &source[t.start..t.end] {
                "}" => depth += 1,
                "{" => {
                    depth -= 1;
                    if depth == 0 {
                        if i > 0 && &source[code[i - 1].start..code[i - 1].end] == "return" {
                            let last = &source[code[tail - 2].start..code[tail - 2].end];
                            let comma = if i == tail - 2 || matches!(last, "," | ";") {
                                ""
                            } else {
                                ","
                            };
                            source.insert_str(code[tail-1].start, &format!("{comma}\n    {name} = function(e, api)\n        -- Handle this server event.\n    end,\n"));
                            return Ok(());
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    bail!("Event insertion needs a final `return M` or `return {{ ... }}`. Add this handler manually: {name}(e, api)")
}
