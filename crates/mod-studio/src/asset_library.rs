//! Import an artist's native asset folder without importing mod definitions.
use anyhow::{bail, Context, Result};
use d2_native::kind::{FileStore, MemStore, NativeFile, NativeKind};
use std::{cell::RefCell, collections::BTreeSet, path::Path};
struct Tracked {
    store: MemStore,
    used: RefCell<BTreeSet<String>>,
}
impl FileStore for Tracked {
    fn read(&self, p: &str) -> Option<Vec<u8>> {
        self.used.borrow_mut().insert(p.into());
        self.store.read(p)
    }
}
pub fn library(source: &Path) -> Result<Vec<NativeFile>> {
    let mut files = Vec::new();
    let mut bytes = 0u64;
    collect(source, source, &mut files, &mut bytes)?;
    anyhow::ensure!(!files.is_empty(), "Asset folder is empty");
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let store = Tracked {
        store: MemStore::from_files(&files),
        used: RefCell::default(),
    };
    for f in &files {
        if let Some(p) = f.path.strip_suffix(".toml") {
            let text = std::str::from_utf8(&f.bytes)?;
            store.used.borrow_mut().insert(f.path.clone());
            if p.ends_with(".dc6") {
                d2_formats::dc6::Dc6::read(p, &store)?;
            } else if p.ends_with(".dcc") {
                d2_formats::dcc::Dcc::read(p, &store)?;
            } else if p.ends_with(".dt1") {
                d2_formats::dt1::Dt1::read(p, &store)?;
            } else if p.ends_with(".cof") {
                d2_native::toml_kinds::read_cof(p, text)?;
            } else if p.ends_with(".ds1") {
                d2_native::toml_kinds::read_ds1(p, text)?;
            } else {
                bail!(
                    "{}: unsupported asset sidecar; import art and maps only",
                    f.path
                );
            }
        } else if f.path.ends_with(".wav") {
            let w = d2_native::wav::read_wav(&f.path, &f.bytes)?;
            d2_native::wav::check_playable(&f.path, &w)?;
            store.used.borrow_mut().insert(f.path.clone());
        }
    }
    for f in &files {
        anyhow::ensure!(
            store.used.borrow().contains(&f.path),
            "{}: not referenced by an asset sidecar",
            f.path
        );
    }
    Ok(files)
}
fn collect(root: &Path, dir: &Path, files: &mut Vec<NativeFile>, bytes: &mut u64) -> Result<()> {
    anyhow::ensure!(
        dir.strip_prefix(root)?.components().count() < 32,
        "Asset folder nesting exceeds 32"
    );
    let meta = std::fs::symlink_metadata(dir)?;
    anyhow::ensure!(
        !meta.file_type().is_symlink(),
        "Asset folder contains a symlink"
    );
    for entry in std::fs::read_dir(dir)? {
        let e = entry?;
        let path = e.path();
        let meta = std::fs::symlink_metadata(&path)?;
        anyhow::ensure!(
            !meta.file_type().is_symlink(),
            "Asset folder contains a symlink: {}",
            path.display()
        );
        if meta.is_dir() {
            collect(root, &path, files, bytes)?;
            continue;
        }
        anyhow::ensure!(meta.is_file(), "Expected asset file");
        anyhow::ensure!(meta.len() <= 64 * 1024 * 1024, "Asset exceeds 64 MiB");
        *bytes = bytes
            .checked_add(meta.len())
            .context("Asset size overflow")?;
        anyhow::ensure!(
            *bytes <= 128 * 1024 * 1024 && files.len() < 4096,
            "Asset library exceeds 128 MiB or 4096 files"
        );
        let relative = path
            .strip_prefix(root)?
            .to_str()
            .context("Asset path is not UTF-8")?
            .replace('\\', "/");
        let p = format!("data/{relative}");
        crate::import::destination_path(&p, p.rsplit('.').next().unwrap_or_default())?;
        anyhow::ensure!(
            p.ends_with(".toml") || p.ends_with(".png") || p.ends_with(".wav"),
            "{p}: expected native sidecar, PNG or WAV"
        );
        files.push(NativeFile {
            path: p,
            bytes: std::fs::read(path)?,
        });
    }
    Ok(())
}
pub fn import(root: &Path, source: &Path, replace: bool) -> Result<usize> {
    let manifest = d2_modkit::ModFolder::scan(root)?.manifest;
    anyhow::ensure!(
        !manifest.screen_only,
        "Screen-only add-ons cannot import a gameplay asset library; use the panel sprite importer"
    );
    let files = library(source)?;
    crate::import::write(root, &files, replace)?;
    Ok(files.len())
}

/// Preserve an artist's PNG for state icons or the Mods screen picture.
pub fn icon(root: &Path, source: &Path, destination: &str, replace: bool) -> Result<()> {
    d2_modkit::layout::check_rel_path(destination).map_err(anyhow::Error::msg)?;
    anyhow::ensure!(
        (destination.starts_with("icons/") && destination.ends_with(".png"))
            || destination == "mod/picture.png",
        "Use icons/name.png or mod/picture.png"
    );
    let manifest = d2_modkit::ModFolder::scan(root)?.manifest;
    anyhow::ensure!(
        std::fs::metadata(source)?.len() <= 16 * 1024 * 1024,
        "PNG exceeds 16 MiB"
    );
    let bytes = std::fs::read(source)?;
    if manifest.screen_only {
        d2_native::addons::check_file(&format!("files/{destination}"), &bytes)
            .map_err(anyhow::Error::msg)?;
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    reader.decode()?;
    let mut path = root.to_owned();
    for part in std::iter::once("files").chain(destination.split('/')) {
        path.push(part);
        anyhow::ensure!(
            !std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()),
            "PNG destination contains a symlink"
        );
    }
    anyhow::ensure!(replace || !path.exists(), "Already exists: {destination}");
    std::fs::create_dir_all(path.parent().context("PNG parent")?)?;
    std::fs::write(path, bytes)?;
    Ok(())
}
