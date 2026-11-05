//! Checked publication of a mod folder as one signed data pack.
//! Content limits count logical base assets, so a sprite's pixels and
//! sidecar count once. New assets do not consume the override allowance.

use d2_modkit::{MemoryPack, ModFolder, PackSource};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    manifest::{FileRow, Status},
    pack,
    source::NativeSource,
};

/// LC publication defaults, pending the owner's content policy decision.
pub const REFUSE_UNMODIFIED_BASE_FILES: bool = true;
pub const MAX_BASE_OVERRIDE_PERCENT: usize = 10;

/// Container-aware manifest reader for the game's shared mod resolver.
/// Pack payloads are verified before any identity or layer is activated.
pub fn open_mod(
    path: &Path,
    today: d2_modkit::api::Date,
) -> Result<d2_modkit::resolve::LoadedMod, d2_modkit::ModError> {
    use d2_modkit::{ApiCheck, ModError};
    if path.is_dir() {
        let mut loaded = d2_modkit::resolve::open_folder(path, today)?;
        if loaded.manifest.screen_only {
            crate::addons::check_source(&ModFolder::scan(path)?)?;
        }
        // Layer entries are paths, never names relative to the native mods folder.
        loaded.dir = fs::canonicalize(path).map_err(|e| ModError::Load(e.to_string()))?;
        return Ok(loaded);
    }
    let pack = pack::Pack::open(path).map_err(|e| ModError::Load(e.to_string()))?;
    if pack.read(d2_modkit::publish::RECORD).is_none() {
        return Err(ModError::BadPack(
            "no author signature: publish the mod as a signed pack".into(),
        ));
    }
    d2_modkit::publish::verify(&pack)?;
    let manifest = d2_modkit::read_manifest(&pack)?;
    crate::addons::check_source(&pack)?;
    match d2_modkit::api::check_api(manifest.api, today) {
        ApiCheck::Ok => {}
        ApiCheck::NeedsUpdate(m) | ApiCheck::TooNew(m) => return Err(ModError::Load(m)),
    }
    Ok(d2_modkit::resolve::LoadedMod {
        manifest,
        dir: fs::canonicalize(path).map_err(|e| ModError::Load(e.to_string()))?,
    })
}

fn category(row: &FileRow) -> &'static str {
    match row.kind.as_str() {
        "excel" | "tbl" => "tables",
        "wav" => "sounds",
        "ds1" | "dt1" => "maps",
        _ => "art",
    }
}

/// Enforces both publication limits against the converted install's index.
/// Identical bytes are refused even if renamed in the mod.
pub fn check_content(src: &dyn PackSource, rows: &[FileRow]) -> Result<(), String> {
    let rows: Vec<_> = rows.iter().filter(|r| r.status == Status::Ok).collect();
    let hashes: BTreeSet<_> = rows
        .iter()
        .flat_map(|r| r.native.iter().map(|f| f.sha256.as_str()))
        .collect();
    let mut paths = BTreeSet::new();
    let mut tables = BTreeSet::new();
    for path in src.paths().into_iter().filter(|p| p.starts_with("files/")) {
        let bytes = src.read(&path).map_err(|e| e.to_string())?;
        if REFUSE_UNMODIFIED_BASE_FILES
            && hashes.contains(crate::source::sha256_hex(&bytes).as_str())
        {
            return Err(format!(
                "package refused: `{path}` is an unmodified base file; ship only your changes"
            ));
        }
        if path.starts_with("files/patches/") && path.ends_with(".d2patch") {
            let (layer, findings) = d2_data::patch::parse_layer(&path, &bytes, 0);
            if d2_data::patch::has_errors(&findings) {
                return Err(format!("package refused: invalid patch `{path}`"));
            }
            for line in layer.statements {
                if let d2_data::patch::Stmt::Table { name, .. } = line.stmt {
                    tables.insert(String::from_utf8_lossy(&name.text).to_ascii_lowercase());
                }
            }
        }
        paths.insert(path.trim_start_matches("files/").to_owned());
    }
    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for row in rows {
        let count = counts.entry(category(row)).or_default();
        count.1 += 1;
        let table = row
            .path
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".txt");
        if row.native.iter().any(|f| paths.contains(&f.name))
            || (row.kind == "excel" && tables.contains(table))
        {
            count.0 += 1;
        }
    }
    for (kind, (overrides, total)) in counts {
        if overrides * 100 > total * MAX_BASE_OVERRIDE_PERCENT {
            return Err(format!("package refused: overrides {overrides} of {total} base {kind} files; the limit is {MAX_BASE_OVERRIDE_PERCENT}% per kind"));
        }
    }
    Ok(())
}

struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
impl Stage {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..100 {
            let path = std::env::temp_dir().join(format!(
                "d2gamma-package-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Err("could not allocate a packaging folder".into())
    }
}

/// Manifest, reference, table compilation and asset checks from the native
/// loader. K9 can supply its complete check runner to `package_with_checks`.
pub fn check_mod(folder: &Path, native: &Path) -> Result<(), String> {
    let order = [folder.display().to_string()];
    let mut src = NativeSource::open_with(native, Some(&[])).map_err(|e| e.to_string())?;
    src.layers = crate::layers::LayerStack::for_check(native, &order).map_err(|e| e.to_string())?;
    let layer = src.layers.layers.last().expect("a mod layer was opened");
    for path in layer.files_under("").map_err(|e| e.to_string())? {
        let canonical = [".toml", ".tsv", ".png", ".pal"]
            .iter()
            .find_map(|suffix| path.strip_suffix(suffix))
            .unwrap_or(&path);
        if let Some(result) = src.read_native(canonical) {
            result.map_err(|e| format!("{path}: {e}"))?;
        }
    }
    let tables = src
        .tables_with(
            &src.manifest.language.to_ascii_lowercase(),
            d2_data::bin::DataRules::Mod,
        )
        .map_err(|e| e.to_string())?;
    d2_data::bin::load_from(&tables, &src.manifest.language.to_ascii_lowercase())
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Packages a snapshot, checking the exact bytes that will be signed. The
/// callback is the shared K9/Studio hook and must fail for any failed check.
/// `key_path` must be in the author's user folder, outside the mod.
pub fn package_with_checks(
    folder: &Path,
    native: &Path,
    out: &Path,
    key_path: &Path,
    checks: impl FnOnce(&Path, &Path) -> Result<(), String>,
) -> Result<pack::BuildStats, String> {
    let folder = ModFolder::scan(folder).map_err(|e| e.to_string())?;
    let mut snapshot = MemoryPack::from_folder(&folder).map_err(|e| e.to_string())?;
    let rows = crate::manifest::parse_files_tsv(
        &fs::read_to_string(native.join("files.tsv")).map_err(|e| e.to_string())?,
        false,
    )
    .map_err(|e| e.to_string())?;
    crate::addons::check_source(&snapshot).map_err(|e| e.to_string())?;
    check_content(&snapshot, &rows)?;
    let stage = Stage::new()?;
    for (path, bytes) in &snapshot.files {
        let dest = stage.0.join(path);
        fs::create_dir_all(dest.parent().expect("snapshot path has parent"))
            .map_err(|e| e.to_string())?;
        fs::write(dest, bytes).map_err(|e| e.to_string())?;
    }
    checks(&stage.0, native)?;
    let key = d2_modkit::publish::author_key(key_path).map_err(|e| e.to_string())?;
    d2_modkit::publish::seal(&mut snapshot, &key).map_err(|e| e.to_string())?;
    fs::write(
        stage.0.join(d2_modkit::publish::RECORD),
        &snapshot.files[d2_modkit::publish::RECORD],
    )
    .map_err(|e| e.to_string())?;
    pack::build(&stage.0, out, 0).map_err(|e| e.to_string())
}

pub fn package(
    folder: &Path,
    native: &Path,
    out: &Path,
    key_path: &Path,
) -> Result<pack::BuildStats, String> {
    package_with_checks(folder, native, out, key_path, check_mod)
}
