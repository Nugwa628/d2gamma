//! Mod layering over the converted native files.
//!
//! The layer stack is `base/` first, then each enabled mod's `files/` tree
//! in the order `mods/order.toml` lists them. A native path resolves to the
//! last layer that has it, so a later mod wins over an earlier one and any
//! mod wins over the base. The runtime native source reads every asset
//! through the stack.
//!
//! Opening the stack checks each mod before any of its files is used:
//! - its `mod.toml` is well formed and targets this build's
//!   `native_format_version`;
//! - every mod it `requires` comes earlier in the order;
//! - it ships no copy of an excel `.txt` or a string table: mods change
//!   those with table patches, so several mods can edit the same table;
//! - every two-part file is complete in the mod's own layer: a sprite,
//!   palette transform or expansion field sidecar (`P.toml`) has its
//!   pixels (`P.png`), and a tile set sidecar has its tile folder
//!   (`P.d/`), and the other way round.
//!
//! A mod's layer is a folder `mods/<name>/` or a data pack holding that
//! folder ([`crate::pack`]): `mods/<name>.d2pack`, or a pack anywhere when
//! the layer list names its path (`/games/my-mod/my-mod.d2pack`). Both read
//! the same bytes; a game ships its layer as a pack next to its executable.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::manifest::FORMAT_VERSION;
use crate::pack::{self, Pack};

/// Name of the base layer in messages.
pub const BASE: &str = "base";
/// The file listing the enabled mods in order.
pub const ORDER_FILE: &str = "mods/order.toml";
/// Where a mod keeps its table patches, under its `files/` tree.
pub const PATCH_DIR: &str = "patches";
/// Where a mod game keeps its own side tables, under its `files/` tree
/// (`d2_data::side`).
pub const SIDE_DIR: &str = "tables";

/// A layer-stack error naming the mod and file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LayerError {
    /// A file or folder could not be read: path, message.
    #[error("{0}: {1}")]
    Io(String, String),
    /// A malformed or inconsistent file.
    #[error("{file}: {detail}")]
    Bad { file: String, detail: String },
    /// A mod ships a copy of an excel table or string table.
    #[error("mod `{layer}` ships {file}: excel tables and string tables are changed by patches, not copies (rule 9)")]
    Copy { layer: String, file: String },
    /// One half of a toml / pixel pair is missing from the mod.
    #[error("mod `{layer}`: {file} has no matching {missing} in the same layer")]
    Unpaired {
        layer: String,
        file: String,
        missing: String,
    },
}

fn bad(file: impl Into<String>, detail: impl Into<String>) -> LayerError {
    LayerError::Bad {
        file: file.into(),
        detail: detail.into(),
    }
}

/// A mod's `mod.toml`.
///
/// Two kinds of file read here. A layer-only `mod.toml` (`name`, `version`,
/// `native_format_version`, `requires`, no `api`) is what mods had before the
/// mod kit and loads exactly as it always did. A mod-kit manifest
/// (`d2_modkit::Manifest`, it names an `api`) is checked by the kit's rules
/// and also fills these fields, so the layer stack treats both alike.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModInfo {
    /// The mod's name.
    pub name: String,
    /// The mod's own version string.
    pub version: String,
    /// The native layout version the mod targets; must equal this build's.
    /// A kit manifest may leave it out, which means this build's.
    pub native_format_version: u32,
    /// Mods that must come earlier in the order.
    pub requires: Vec<String>,
    /// The mod-kit manifest, for a mod that has one.
    pub kit: Option<d2_modkit::Manifest>,
}

impl ModInfo {
    /// Parses `mod.toml`. Strict: every key present and of its type, and
    /// no keys the format does not know.
    pub fn from_toml(file: &str, text: &str) -> Result<ModInfo, LayerError> {
        match d2_modkit::manifest::parse_mod_toml(text).map_err(|e| bad(file, e.detail()))? {
            d2_modkit::manifest::ModToml::Legacy(l) => Ok(ModInfo {
                name: l.name,
                version: l.version,
                native_format_version: l.native_format_version,
                requires: l.requires,
                kit: None,
            }),
            d2_modkit::manifest::ModToml::Kit(m) => Ok(ModInfo {
                name: m.name.clone(),
                version: m.version.to_string(),
                native_format_version: m.native_format_version.unwrap_or(FORMAT_VERSION),
                requires: m.requires.clone(),
                kit: Some(*m),
            }),
        }
    }
}

/// Parses `mods/order.toml`: `order = ["modA", "modB"]`.
pub fn parse_order(text: &str) -> Result<Vec<String>, LayerError> {
    let t: toml::Table = text.parse().map_err(|e| bad(ORDER_FILE, format!("{e}")))?;
    let order = t
        .get("order")
        .and_then(|v| v.as_array())
        .and_then(|a| {
            a.iter()
                .map(|x| x.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| bad(ORDER_FILE, "`order` missing or not a list of strings"))?;
    let mut seen = BTreeSet::new();
    for m in &order {
        if !seen.insert(m) {
            return Err(bad(ORDER_FILE, format!("mod `{m}` listed twice")));
        }
    }
    Ok(order)
}

/// Where a layer's files are.
#[derive(Debug, Clone)]
pub enum LayerFiles {
    /// A folder; native paths are relative to it.
    Dir(PathBuf),
    /// A data pack of a mod folder; native paths are relative to the
    /// pack's `files/`.
    Pack(Arc<Pack>),
}

impl PartialEq for LayerFiles {
    fn eq(&self, other: &LayerFiles) -> bool {
        match (self, other) {
            (LayerFiles::Dir(a), LayerFiles::Dir(b)) => a == b,
            (LayerFiles::Pack(a), LayerFiles::Pack(b)) => {
                a.path() == b.path() && a.index_sha256() == b.index_sha256()
            }
            _ => false,
        }
    }
}

impl Eq for LayerFiles {}

/// The folder of a mod's native files inside its pack.
const PACK_FILES: &str = "files";

/// One layer: its name and where its native files are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    /// `base` or the mod's name.
    pub name: String,
    /// The folder or pack native paths are relative to.
    pub files: LayerFiles,
    /// `None` for `base`.
    pub info: Option<ModInfo>,
}

impl Layer {
    /// A layer over folder `dir`.
    pub fn dir(name: impl Into<String>, dir: PathBuf, info: Option<ModInfo>) -> Layer {
        Layer {
            name: name.into(),
            files: LayerFiles::Dir(dir),
            info,
        }
    }

    /// Whether the layer holds native file `path`.
    pub fn has(&self, path: &str) -> bool {
        match &self.files {
            LayerFiles::Dir(d) => d.join(path).is_file(),
            LayerFiles::Pack(p) => p.contains(&format!("{PACK_FILES}/{path}")),
        }
    }

    /// The bytes of native file `path` in this layer only: `None` when the
    /// layer does not hold it.
    pub fn read(&self, path: &str) -> Option<Result<Vec<u8>, LayerError>> {
        match &self.files {
            LayerFiles::Dir(d) => {
                let p = d.join(path);
                match std::fs::read(&p) {
                    Ok(b) => Some(Ok(b)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => Some(Err(io(&p, e))),
                }
            }
            LayerFiles::Pack(p) => p
                .read(&format!("{PACK_FILES}/{path}"))
                .map(|r| r.map_err(|e| LayerError::Io(e.pack, e.detail))),
        }
    }

    /// Every native file under folder `sub` of the layer (`""` for all),
    /// relative to `sub`, sorted; empty when the folder is absent.
    pub fn files_under(&self, sub: &str) -> Result<Vec<String>, LayerError> {
        match &self.files {
            LayerFiles::Dir(d) => {
                let dir = d.join(sub);
                if dir.is_dir() {
                    walk(&dir)
                } else {
                    Ok(Vec::new())
                }
            }
            LayerFiles::Pack(p) => {
                let dir = match sub {
                    "" => PACK_FILES.to_owned(),
                    s => format!("{PACK_FILES}/{s}"),
                };
                Ok(p.files_under(&dir).into_iter().map(str::to_owned).collect())
            }
        }
    }
}

/// Whether layer-list entry `entry` names a data pack (`<name>.d2pack`,
/// a file name under `mods/` or a path) rather than a mod folder.
pub fn is_pack_entry(entry: &str) -> bool {
    pack_stem(entry).is_some()
}

/// The mod name of pack entry `entry`: its file name without `.d2pack`.
fn pack_stem(entry: &str) -> Option<&str> {
    let file = entry.rsplit(['/', '\\']).next()?;
    let cut = file.len().checked_sub(pack::EXTENSION.len() + 1)?;
    let (stem, ext) = file.split_at_checked(cut)?;
    let is_pack = ext.strip_prefix('.')?.eq_ignore_ascii_case(pack::EXTENSION);
    (is_pack && valid_name(stem)).then_some(stem)
}

/// Whether `name` is a mod name: letters, digits, `-` and `_`.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Whether `entry` can stand in a layer list: a mod name (letters, digits,
/// `-`, `_`) or a data pack ([`is_pack_entry`]).
pub fn valid_mod_entry(entry: &str) -> bool {
    valid_name(entry) || is_pack_entry(entry)
}

/// The mod name of layer-list entry `entry`: the entry itself, or a pack's
/// file name without `.d2pack`.
pub fn mod_name(entry: &str) -> &str {
    pack_stem(entry).unwrap_or(entry)
}

/// The layer-list entry of a `--mod` command-line value: a mod name as
/// given, a bare `<name>.d2pack` as given (looked up under `mods/`), and a
/// pack path with a folder part made absolute, so it is read from where the
/// player pointed rather than from under `mods/`.
pub fn cli_entry(arg: &str) -> Result<String, String> {
    if !valid_mod_entry(arg) {
        return Err(format!(
            "--mod {arg}: needs a single mod folder name or a .{} data pack",
            pack::EXTENSION
        ));
    }
    let path = Path::new(arg);
    let has_folder = path.parent().is_some_and(|p| !p.as_os_str().is_empty());
    if is_pack_entry(arg) && has_folder {
        let full = std::path::absolute(path).map_err(|e| format!("--mod {arg}: {e}"))?;
        return Ok(full.to_string_lossy().into_owned());
    }
    Ok(arg.to_owned())
}

/// The ordered layers, `base` first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerStack {
    /// `base`, then the mods in load order.
    pub layers: Vec<Layer>,
}

fn io(path: &Path, e: std::io::Error) -> LayerError {
    LayerError::Io(path.display().to_string(), e.to_string())
}

/// Every file under `dir`, as `/`-joined paths relative to it, sorted.
fn walk(dir: &Path) -> Result<Vec<String>, LayerError> {
    let mut out = Vec::new();
    let mut todo = vec![(dir.to_path_buf(), String::new())];
    while let Some((d, prefix)) = todo.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| io(&d, e))? {
            let e = e.map_err(|e| io(&d, e))?;
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = format!("{prefix}{name}");
            if e.file_type().map_err(|err| io(&e.path(), err))?.is_dir() {
                todo.push((e.path(), format!("{rel}/")));
            } else {
                out.push(rel);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Whether a mod file is a copy of a table that mods may only patch: an
/// excel `.txt`, the `.bin` overrides, or a string table's native files.
fn is_table_copy(f: &str) -> bool {
    (f.starts_with("data/global/excel/") && f.ends_with(".txt"))
        || f.starts_with("data/global/excel/_bin-overrides")
        || (f.starts_with("data/local/lng/")
            && (f.ends_with(".tbl.tsv") || f.ends_with(".tbl.toml")))
}

/// The file-list checks of one mod (`files` sorted, relative to its
/// `files/` tree): no copies of excel or string tables, and every
/// sidecar / pixel pair complete.
pub fn check_mod_files(layer: &str, files: &[String]) -> Result<(), LayerError> {
    let set: BTreeSet<&str> = files.iter().map(String::as_str).collect();
    // A folder is present when some file sorts right after `dir/` and
    // starts with it.
    let has_dir = |d: &str| {
        let p = format!("{d}/");
        set.range(p.as_str()..)
            .next()
            .is_some_and(|f| f.starts_with(&p))
    };
    for f in files {
        if is_table_copy(f) {
            return Err(LayerError::Copy {
                layer: layer.into(),
                file: f.clone(),
            });
        }
        if crate::source::fold(f).starts_with("data/") && crate::source::fold(f) != *f {
            return Err(bad(
                format!("mod `{layer}`: {f}"),
                "native asset paths must be lowercase with / separators",
            ));
        }
        if let Some(kind) = crate::source::AssetKind::of(f) {
            let primary = kind.primary(f);
            if primary != *f {
                return Err(bad(format!("mod `{layer}`: {f}"), format!(
                    "raw asset format is not loaded from native mods; supply {primary} in converted native format"
                )));
            }
        }
        let unpaired = |missing: String| LayerError::Unpaired {
            layer: layer.into(),
            file: f.clone(),
            missing,
        };
        // Sprite sheets, palette transforms and the expansion field:
        // `P.toml` + `P.png`.
        for ext in [".dc6", ".dcc", ".pl2", "expfield.d2"] {
            for (have, want) in [(".toml", ".png"), (".png", ".toml")] {
                if let Some(p) = f.strip_suffix(&format!("{ext}{have}")) {
                    let other = format!("{p}{ext}{want}");
                    if !set.contains(other.as_str()) {
                        return Err(unpaired(other));
                    }
                }
            }
        }
        // Tile sets: `P.toml` + the tile folder `P.d/`.
        if let Some(p) = f.strip_suffix(".dt1.toml") {
            let d = format!("{p}.dt1.d");
            if !has_dir(&d) {
                return Err(unpaired(format!("{d}/")));
            }
        }
        if let Some((p, _)) = f.split_once(".dt1.d/") {
            let t = format!("{p}.dt1.toml");
            if !set.contains(t.as_str()) {
                return Err(unpaired(t));
            }
        }
    }
    Ok(())
}

/// Where a layer-list entry's files are.
enum ModLocation {
    Dir(PathBuf),
    Pack(PathBuf),
}

/// Finds layer-list entry `entry` under `root`: a pack entry is that pack
/// (a relative path is under `mods/`); an absolute path to a folder is that
/// folder, wherever it is (the mod kit's `--mod-dir`, a mod installed outside
/// the native folder); a name is the folder `mods/<name>/` or, when there is
/// no such folder, the pack `mods/<name>.d2pack`. Both present is refused, so
/// the layer that loads is never a guess.
fn locate(root: &Path, entry: &str) -> Result<(String, ModLocation), LayerError> {
    let mods = root.join("mods");
    if let Some(stem) = pack_stem(entry) {
        let p = Path::new(entry);
        let path = if p.is_absolute() {
            p.to_path_buf()
        } else {
            mods.join(p)
        };
        return Ok((stem.to_owned(), ModLocation::Pack(path)));
    }
    if Path::new(entry).is_absolute() {
        let dir = PathBuf::from(entry);
        let name = dir
            .file_name()
            .map_or_else(|| entry.to_owned(), |n| n.to_string_lossy().into_owned());
        return Ok((name, ModLocation::Dir(dir)));
    }
    let dir = mods.join(entry);
    let pack = mods.join(format!("{entry}.{}", pack::EXTENSION));
    match (dir.is_dir(), pack.is_file()) {
        (true, true) => Err(bad(
            format!("mods/{entry}"),
            format!(
                "both the folder mods/{entry}/ and the pack mods/{entry}.{} exist; keep one",
                pack::EXTENSION
            ),
        )),
        (false, true) => Ok((entry.to_owned(), ModLocation::Pack(pack))),
        _ => Ok((entry.to_owned(), ModLocation::Dir(dir))),
    }
}

/// Opens and checks layer-list entry `entry` of `root` (a mod folder or a
/// pack, see [`locate`]), given the layers enabled before it.
///
/// The layer of an absolute path is named by the mod's `id` when its
/// `mod.toml` is a kit manifest, else by the folder's (or pack's) name, so
/// `requires` finds it by that name. A kit mod under `mods/` must be named
/// by its `id`.
fn open_mod(root: &Path, entry: &str, earlier: &[Layer]) -> Result<Layer, LayerError> {
    let (name, at) = locate(root, entry)?;
    let external = Path::new(entry).is_absolute();
    let (file, text, files, layer_files) = match at {
        ModLocation::Dir(dir) => {
            let file = if external {
                format!("{}/mod.toml", dir.display())
            } else {
                format!("mods/{name}/mod.toml")
            };
            let text = std::fs::read_to_string(dir.join("mod.toml"))
                .map_err(|e| LayerError::Io(file.clone(), e.to_string()))?;
            let files_dir = dir.join("files");
            let files = if files_dir.is_dir() {
                walk(&files_dir)?
            } else {
                Vec::new()
            };
            (file, text, files, LayerFiles::Dir(files_dir))
        }
        ModLocation::Pack(path) => {
            let p = Pack::open(&path).map_err(|e| LayerError::Io(e.pack, e.detail))?;
            if p.contains(d2_modkit::publish::RECORD) {
                d2_modkit::publish::verify(&p)
                    .map_err(|e| bad(path.display().to_string(), e.to_string()))?;
            }
            let file = format!("{}: mod.toml", path.display());
            let bytes = p
                .read("mod.toml")
                .ok_or_else(|| LayerError::Io(file.clone(), "missing from the pack".into()))?
                .map_err(|e| LayerError::Io(e.pack, e.detail))?;
            let text = String::from_utf8(bytes).map_err(|_| bad(&file, "not UTF-8"))?;
            if matches!(
                d2_modkit::manifest::parse_mod_toml(&text),
                Ok(d2_modkit::manifest::ModToml::Kit(_))
            ) && !p.contains(d2_modkit::publish::RECORD)
            {
                return Err(bad(
                    &file,
                    "mod pack has no author signature; publish it with package-mod",
                ));
            }
            let files = p
                .files_under(PACK_FILES)
                .into_iter()
                .map(str::to_owned)
                .collect();
            (file, text, files, LayerFiles::Pack(Arc::new(p)))
        }
    };
    let info = ModInfo::from_toml(&file, &text)?;
    if info.native_format_version != FORMAT_VERSION {
        return Err(bad(
            &file,
            format!(
                "native_format_version {} is not known (this build reads {FORMAT_VERSION})",
                info.native_format_version
            ),
        ));
    }
    let layer_name = match (&info.kit, external) {
        (Some(kit), true) => kit.id.clone(),
        (Some(kit), false) if kit.id != name => {
            return Err(bad(
                &file,
                format!(
                    "`id` is \"{}\" but the folder is named \"{name}\"; they must match",
                    kit.id
                ),
            ));
        }
        _ => name,
    };
    if earlier.iter().any(|l| l.name == layer_name) {
        return Err(bad(
            format!("mods/{layer_name}"),
            format!("mod `{layer_name}` is enabled twice"),
        ));
    }
    if let Some(kit) = &info.kit {
        match d2_modkit::check_api_now(kit.api) {
            d2_modkit::ApiCheck::Ok => {}
            d2_modkit::ApiCheck::NeedsUpdate(msg) | d2_modkit::ApiCheck::TooNew(msg) => {
                return Err(bad(&file, msg));
            }
        }
    }
    if let Some(r) = info
        .requires
        .iter()
        .find(|r| !earlier.iter().any(|l| &l.name == *r))
    {
        return Err(bad(
            &file,
            format!("requires `{r}`, which is not enabled before it in {ORDER_FILE}"),
        ));
    }
    check_mod_files(&layer_name, &files)?;
    Ok(Layer {
        name: layer_name,
        files: layer_files,
        info: Some(info),
    })
}

impl LayerStack {
    /// The stack of a native root: `base/`, then the mods of
    /// `mods/order.toml` (none when the file is absent).
    pub fn open(root: &Path) -> Result<LayerStack, LayerError> {
        let order_path = root.join(ORDER_FILE);
        let order = match std::fs::read_to_string(&order_path) {
            Ok(text) => parse_order(&text)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(io(&order_path, e)),
        };
        LayerStack::with_mods(root, &order)
    }

    /// The stack of `root` with the mods `order` (mod folders or data
    /// packs, see the module header), each checked as the module header
    /// describes.
    pub fn with_mods(root: &Path, order: &[String]) -> Result<LayerStack, LayerError> {
        Self::checked_layers(root, order, false)
    }

    /// Authoring validation only: checks capabilities, maker approval and conflicts
    /// without requiring the owner's permission to activate a plain-game add-on.
    /// Runtime callers must use `with_mods`.
    pub fn for_check(root: &Path, order: &[String]) -> Result<LayerStack, LayerError> {
        Self::checked_layers(root, order, true)
    }
    fn checked_layers(
        root: &Path,
        order: &[String],
        authoring: bool,
    ) -> Result<LayerStack, LayerError> {
        let mut layers = vec![Layer::dir(BASE, root.join("base"), None)];
        for entry in order {
            let layer = open_mod(root, entry, &layers)?;
            layers.push(layer);
        }
        for layer in layers.iter().skip(1) {
            crate::addons::check_layer(layer)?;
        }
        check_composition(root, &layers, authoring)?;
        Ok(LayerStack { layers })
    }

    /// The last layer holding the native file `path` (later wins).
    pub fn find(&self, path: &str) -> Option<&Layer> {
        self.layers.iter().rev().find(|l| l.has(path))
    }

    /// Reads `path` from layer `layer` only (a pair's other half comes
    /// from the same layer).
    pub fn read_in(&self, layer: &Layer, path: &str) -> Option<Result<Vec<u8>, LayerError>> {
        layer.read(path)
    }

    /// Reads `path` from the last layer that has it.
    pub fn read(&self, path: &str) -> Option<Result<Vec<u8>, LayerError>> {
        self.find(path).and_then(|l| self.read_in(l, path))
    }

    /// Every mod's table patches, in stack order: layer order, then file
    /// name (byte order) within a layer. `(stack path, bytes)`.
    pub fn patches(&self) -> Result<Vec<(String, Vec<u8>)>, LayerError> {
        self.mod_files(PATCH_DIR)
    }

    /// Every mod's side tables (`files/tables/`), in the same order as
    /// [`LayerStack::patches`], for `d2_data::side::SideTables::from_files`.
    pub fn side_tables(&self) -> Result<Vec<(String, Vec<u8>)>, LayerError> {
        self.mod_files(SIDE_DIR)
    }

    /// Every file under folder `sub` of each mod's `files/` tree: layer
    /// order, then file name (byte order). `(stack path, bytes)`.
    fn mod_files(&self, sub: &str) -> Result<Vec<(String, Vec<u8>)>, LayerError> {
        let mut out = Vec::new();
        for l in self.layers.iter().skip(1) {
            for f in l.files_under(sub)? {
                // Authoring folders keep empty directories with .gitkeep;
                // editor metadata is not table or patch content.
                if f.split('/').any(|part| part.starts_with('.')) {
                    continue;
                }
                let path = format!("{sub}/{f}");
                let stack_path = format!("mods/{}/files/{path}", l.name);
                let bytes = l.read(&path).unwrap_or_else(|| {
                    Err(LayerError::Io(stack_path.clone(), "vanished".into()))
                })?;
                out.push((stack_path, bytes));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_tables_come_from_each_mods_tables_folder() {
        let root = std::env::temp_dir().join(format!("d2native-side-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let put = |rel: &str, text: &str| {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        put("base/tables/ignored.txt", "x");
        put("a/tables/b.txt", "1");
        put("a/tables/a.txt", "2");
        put("a/patches/p.d2patch", "3");
        put("c/tables/a.txt", "4");
        let layer = |name: &str| Layer::dir(name, root.join(name), None);
        let stack = LayerStack {
            layers: vec![layer("base"), layer("a"), layer("c")],
        };
        let got = stack.side_tables().unwrap();
        let names: Vec<&str> = got.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            names,
            [
                "mods/a/files/tables/a.txt",
                "mods/a/files/tables/b.txt",
                "mods/c/files/tables/a.txt"
            ]
        );
        assert_eq!(got[2].1, b"4");
        assert_eq!(stack.patches().unwrap().len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn authoring_placeholders_are_not_patches_or_side_tables() {
        let root =
            std::env::temp_dir().join(format!("d2native-placeholders-{}", std::process::id()));
        for path in [
            "patches/.gitkeep",
            "tables/.gitkeep",
            "patches/.editor/cache",
        ] {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"").unwrap();
        }
        std::fs::write(root.join("patches/edit.d2patch"), b"d2patch 1\n").unwrap();
        let stack = LayerStack {
            layers: vec![
                Layer::dir("base", root.join("base"), None),
                Layer::dir("mod", root.clone(), None),
            ],
        };
        assert!(stack.side_tables().unwrap().is_empty());
        assert_eq!(
            stack.patches().unwrap(),
            vec![(
                "mods/mod/files/patches/edit.d2patch".into(),
                b"d2patch 1\n".to_vec()
            )]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    fn files(list: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = list.iter().map(|s| s.to_string()).collect();
        v.sort();
        v
    }

    #[test]
    fn mod_excel_and_tbl_copies_are_refused() {
        for f in [
            "data/global/excel/weapons.txt",
            "data/local/lng/eng/string.tbl.tsv",
        ] {
            let e = check_mod_files("m", &files(&[f])).unwrap_err();
            assert!(e.to_string().contains(f), "{e}");
        }
        check_mod_files("m", &files(&["patches/a.d2patch"])).unwrap();
    }

    #[test]
    fn half_pairs_are_refused() {
        let e = check_mod_files("m", &files(&["data/x.dc6.toml"])).unwrap_err();
        assert!(e.to_string().contains("data/x.dc6.png"), "{e}");
        let e = check_mod_files("m", &files(&["data/x.dcc.png"])).unwrap_err();
        assert!(e.to_string().contains("data/x.dcc.toml"), "{e}");
        let e = check_mod_files("m", &files(&["data/t.dt1.toml"])).unwrap_err();
        assert!(e.to_string().contains("data/t.dt1.d/"), "{e}");
        let e = check_mod_files("m", &files(&["data/t.dt1.d/0.png"])).unwrap_err();
        assert!(e.to_string().contains("data/t.dt1.toml"), "{e}");
        check_mod_files(
            "m",
            &files(&[
                "data/x.dc6.toml",
                "data/x.dc6.png",
                "data/t.dt1.toml",
                "data/t.dt1.d/0.png",
            ]),
        )
        .unwrap();
    }

    #[test]
    fn raw_assets_and_unfolded_paths_have_actionable_errors() {
        for f in [
            "data/new.dc6",
            "data/new.dcc",
            "data/new.ds1",
            "data/new.dt1",
            "data/new.tbl",
        ] {
            let e = check_mod_files("m", &files(&[f])).unwrap_err().to_string();
            assert!(
                e.contains(f) && e.contains("converted native format"),
                "{e}"
            );
        }
        let e = check_mod_files("m", &files(&["data/New.wav"]))
            .unwrap_err()
            .to_string();
        assert!(e.contains("lowercase"), "{e}");
    }

    #[test]
    fn order_and_mod_toml_are_strict() {
        assert_eq!(
            parse_order("order = [\"a\", \"b\"]").unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(parse_order("order = [\"a\", \"a\"]").is_err());
        assert!(parse_order("order = 3").is_err());
        let ok = "name = \"a\"\nversion = \"1\"\nnative_format_version = 1\n";
        assert!(ModInfo::from_toml("m", ok).unwrap().requires.is_empty());
        assert!(ModInfo::from_toml("m", &format!("{ok}extra = 1\n")).is_err());
        assert!(ModInfo::from_toml("m", "name = \"a\"\n").is_err());
    }

    /// A native root with `base/` and the mod `name` holding `mod_toml`.
    fn root_with_mod(tag: &str, name: &str, mod_toml: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("d2native-kit-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("base")).unwrap();
        let dir = root.join("mods").join(name);
        std::fs::create_dir_all(dir.join("files")).unwrap();
        std::fs::write(dir.join("mod.toml"), mod_toml).unwrap();
        root
    }

    const KIT: &str = "id = \"kit-mod\"\nname = \"Kit Mod\"\nversion = \"1.2.0\"\nauthor = \"A\"\n\
                       description = \"\"\nkind = \"full\"\n";

    #[test]
    fn old_layer_files_load_unchanged() {
        let old = "name = \"old\"\nversion = \"v7\"\nnative_format_version = 1\nrequires = []\n";
        let info = ModInfo::from_toml("m", old).unwrap();
        assert_eq!(info.name, "old");
        assert_eq!(info.version, "v7");
        assert_eq!(info.native_format_version, 1);
        assert!(info.kit.is_none());
        let root = root_with_mod("old", "old", old);
        let stack = LayerStack::with_mods(&root, &["old".to_string()]).unwrap();
        assert_eq!(stack.layers.len(), 2);
    }

    #[test]
    fn a_kit_mod_loads_through_the_stack() {
        let root = root_with_mod(
            "kit",
            "kit-mod",
            &format!("{KIT}api = {}\n", d2_modkit::CURRENT_API),
        );
        let stack = LayerStack::with_mods(&root, &["kit-mod".to_string()]).unwrap();
        let info = stack.layers[1].info.as_ref().unwrap();
        assert_eq!(info.name, "Kit Mod");
        assert_eq!(info.version, "1.2.0");
        assert_eq!(info.native_format_version, FORMAT_VERSION);
        assert_eq!(info.kit.as_ref().unwrap().id, "kit-mod");
    }

    #[test]
    fn a_mod_folder_anywhere_is_a_named_layer() {
        let api = d2_modkit::CURRENT_API;
        let root = root_with_mod("abs", "unused", "x");
        let outside = std::env::temp_dir().join(format!("d2native-outside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&outside);
        let kit = outside.join("any-folder-name");
        std::fs::create_dir_all(kit.join("files")).unwrap();
        std::fs::write(kit.join("mod.toml"), format!("{KIT}api = {api}\n")).unwrap();
        let old = outside.join("old-layer");
        std::fs::create_dir_all(old.join("files")).unwrap();
        std::fs::write(
            old.join("mod.toml"),
            "name = \"o\"\nversion = \"1\"\nnative_format_version = 1\n",
        )
        .unwrap();
        let entries = [old.display().to_string(), kit.display().to_string()];
        let stack = LayerStack::with_mods(&root, &entries).unwrap();
        let names: Vec<&str> = stack.layers.iter().map(|l| l.name.as_str()).collect();
        // A kit mod is named by its id, a layer-only one by its folder.
        assert_eq!(names, ["base", "old-layer", "kit-mod"]);
        assert_eq!(stack.layers[2].files, LayerFiles::Dir(kit.join("files")));
    }

    #[test]
    fn a_kit_mod_under_mods_needs_a_folder_named_by_its_id() {
        let api = d2_modkit::CURRENT_API;
        let root = root_with_mod("idname", "other-name", &format!("{KIT}api = {api}\n"));
        let e = LayerStack::with_mods(&root, &["other-name".to_string()])
            .unwrap_err()
            .to_string();
        assert!(e.contains("must match"), "{e}");
    }

    #[test]
    fn a_kit_mod_from_a_newer_api_is_refused_in_plain_words() {
        let api = d2_modkit::CURRENT_API + 1;
        let root = root_with_mod("new", "kit-mod", &format!("{KIT}api = {api}\n"));
        let e = LayerStack::with_mods(&root, &["kit-mod".to_string()])
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("newer D2Gamma") && e.contains("Update D2Gamma"),
            "{e}"
        );
    }

    #[test]
    fn a_kit_mod_names_the_bad_field() {
        let root = root_with_mod("bad", "kit-mod", &format!("{KIT}api = 0\n"));
        let e = LayerStack::with_mods(&root, &["kit-mod".to_string()])
            .unwrap_err()
            .to_string();
        assert!(e.contains("`api`"), "{e}");
    }
}

/// Check kit permissions even for callers using an explicit native layer list.
fn check_composition(root: &Path, layers: &[Layer], authoring: bool) -> Result<(), LayerError> {
    use d2_modkit::Kind;
    let manifests = layers
        .iter()
        .filter_map(|l| l.info.as_ref()?.kit.as_ref())
        .collect::<Vec<_>>();
    d2_modkit::addons::check_stack(&manifests).map_err(|e| bad(ORDER_FILE, e))?;
    let mut files = std::collections::BTreeMap::new();
    for layer in layers {
        if !layer
            .info
            .as_ref()
            .and_then(|i| i.kit.as_ref())
            .is_some_and(|m| m.kind == Kind::Addon)
        {
            continue;
        }
        if !authoring && !manifests.iter().any(|m| m.kind == Kind::Full) {
            let approved =
                d2_modkit::addons::plain_approved(root).map_err(|e| bad(ORDER_FILE, e))?;
            let manifest = layer
                .info
                .as_ref()
                .and_then(|i| i.kit.as_ref())
                .expect("add-on manifest");
            if !approved.contains(&manifest.id) {
                return Err(bad(
                    ORDER_FILE,
                    format!(
                        "plain D2Gamma add-on {} requires owner approval in {}",
                        manifest.id,
                        d2_modkit::addons::PLAIN_APPROVAL_FILE
                    ),
                ));
            }
        }
        for path in layer.files_under("")? {
            if path == "d2gamma-pack-signatures.toml" || path.split('/').any(|p| p.starts_with('.'))
            {
                continue;
            }
            let key = path.to_ascii_lowercase();
            if let Some(previous) = files.insert(key, &layer.name) {
                return Err(bad(ORDER_FILE, format!("add-on conflict: {previous} and {} both provide files/{path}; disable one or ask the makers to combine them", layer.name)));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod addon_tests {
    use super::*;
    #[test]
    fn native_lists_cannot_bypass_maker_approval_or_conflicts() {
        let root = std::env::temp_dir().join(format!("addon-stack-{}", std::process::id()));
        let put = |id: &str, extra: &str, file: &str| {
            let dir = root.join("mods").join(id);
            std::fs::create_dir_all(dir.join("files/ui")).unwrap();
            std::fs::write(dir.join("mod.toml"),format!("id='{id}'\nname='Test'\nversion='1.0.0'\nauthor='Tester'\ndescription=''\napi=1\n{extra}")).unwrap();
            if !file.is_empty() {
                std::fs::write(dir.join("files/ui").join(file), "version=1").unwrap();
            }
        };
        put(
            "test-mod",
            "kind='full'\napproved_addons=['test-ui','test-extra']",
            "",
        );
        put("test-ui", "kind='addon'\nfor='test-mod'", "hud.toml");
        put("test-extra", "kind='addon'\nfor='test-mod'", "hud.toml");
        let entries = |ids: &[&str]| ids.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(LayerStack::with_mods(&root, &[]).unwrap().layers.len(), 1);
        assert!(LayerStack::with_mods(&root, &entries(&["test-mod", "test-ui"])).is_ok());
        assert!(
            LayerStack::with_mods(&root, &entries(&["test-mod", "test-ui", "test-extra"]))
                .unwrap_err()
                .to_string()
                .contains("both provide files/ui/hud.toml")
        );
        assert!(LayerStack::with_mods(&root, &entries(&["test-ui"])).is_err());
        put("test-mod", "kind='full'", "");
        assert!(
            LayerStack::with_mods(&root, &entries(&["test-mod", "test-ui"]))
                .unwrap_err()
                .to_string()
                .contains("approved")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
