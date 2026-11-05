//! Mod packs, seen through one small trait.
//!
//! A published mod is one file holding the mod folder exactly: `mod.toml`
//! and the `files/` tree. The container format belongs to the data pack
//! (`d2_native::pack`, which the layer stack reads in place); the mod kit
//! does not write containers. What the kit needs from a pack is only what
//! [`PackSource`] asks for: the list of paths and the bytes of each. A mod
//! folder is a source ([`ModFolder`]), so are the data pack and the
//! in-memory [`MemoryPack`] used by tests and tools.
//!
//! On top of that the kit defines what does not depend on the container:
//!
//! - [`read_manifest`]: the manifest of a pack, read without touching its
//!   other files;
//! - [`check_pack_paths`]: the pack holds `mod.toml` and `files/...` only,
//!   every path safe;
//! - [`content_hash`]: one SHA-256 over every path and every file's bytes,
//!   in path order. The same mod gives the same hash as a folder and as a
//!   pack of that folder, whatever the container compresses or how it
//!   orders things inside, so a packager can record it and a loader can
//!   check it ([`verify_content`]);
//! - signatures over that hash: see [`crate::sign`].

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use sha2::{Digest, Sha256};

use crate::error::ModError;
use crate::layout::{check_rel_path, ModFolder, FILES_DIR};
use crate::manifest::{Manifest, MANIFEST_FILE};

/// A SHA-256 hash.
pub type Hash = [u8; 32];

/// The hash as lowercase hex.
pub fn hex(h: &[u8]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// Something that holds a mod's files: a folder, a pack file, memory.
pub trait PackSource {
    /// Every file's path, `/`-separated, `mod.toml` included. Any order.
    fn paths(&self) -> Vec<String>;
    /// The bytes of the file at `path`.
    fn read(&self, path: &str) -> Result<Vec<u8>, ModError>;
}

impl PackSource for ModFolder {
    fn paths(&self) -> Vec<String> {
        std::iter::once(MANIFEST_FILE.to_owned())
            .chain(self.files.iter().map(|f| f.path.clone()))
            .collect()
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, ModError> {
        if path == MANIFEST_FILE {
            return Ok(self.manifest_text.clone().into_bytes());
        }
        let file = self
            .files
            .iter()
            .find(|f| f.path == path)
            .ok_or_else(|| ModError::BadPack(format!("the mod has no file `{path}`")))?;
        fs::read(&file.disk).map_err(|e| ModError::io(file.disk.display(), e))
    }
}

/// A pack held in memory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryPack {
    pub files: BTreeMap<String, Vec<u8>>,
}

impl MemoryPack {
    /// The pack of a folder: its manifest and everything under `files/`.
    pub fn from_folder(folder: &ModFolder) -> Result<MemoryPack, ModError> {
        let mut files = BTreeMap::new();
        for p in folder.paths() {
            let bytes = folder.read(&p)?;
            files.insert(p, bytes);
        }
        Ok(MemoryPack { files })
    }
}

impl PackSource for MemoryPack {
    fn paths(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, ModError> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| ModError::BadPack(format!("the pack has no file `{path}`")))
    }
}

/// Checks the list of paths of a pack: `mod.toml` is there, everything else
/// is under `files/`, every path is safe, none repeats (ignoring case).
pub fn check_pack_paths(paths: &[String]) -> Result<(), ModError> {
    let bad = |m: String| Err(ModError::BadPack(m));
    if !paths.iter().any(|p| p == MANIFEST_FILE) {
        return bad(format!("it has no {MANIFEST_FILE}"));
    }
    let mut seen = BTreeSet::new();
    for p in paths {
        if let Err(m) = check_rel_path(p) {
            return bad(format!("file path `{p}` {m}"));
        }
        let top = p.split('/').next().unwrap_or("");
        if p != MANIFEST_FILE && top != FILES_DIR {
            return bad(format!(
                "`{p}` is outside {FILES_DIR}/ (a pack holds {MANIFEST_FILE} and {FILES_DIR}/ only)"
            ));
        }
        if !seen.insert(p.to_lowercase()) {
            return bad(format!("`{p}` is listed twice"));
        }
    }
    Ok(())
}

/// The manifest of a pack. Reads `mod.toml` and nothing else.
pub fn read_manifest(src: &dyn PackSource) -> Result<Manifest, ModError> {
    let bytes = src.read(MANIFEST_FILE)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| ModError::BadPack("the manifest is not valid text".into()))?;
    Ok(Manifest::parse(&text)?)
}

/// The content hash of a mod: SHA-256 over, for each file in path order,
/// the path's length (u32 little endian) and bytes, the file's length (u64
/// little endian) and the file's own SHA-256. The paths are checked first
/// ([`check_pack_paths`]).
pub fn content_hash(src: &dyn PackSource) -> Result<Hash, ModError> {
    let mut paths = src.paths();
    check_pack_paths(&paths)?;
    paths.sort();
    let mut h = Sha256::new();
    for p in &paths {
        let bytes = src.read(p)?;
        h.update((p.len() as u32).to_le_bytes());
        h.update(p.as_bytes());
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(Sha256::digest(&bytes));
    }
    Ok(h.finalize().into())
}

/// Checks that the pack's content hash is `expected` (the hash recorded
/// when the pack was made).
pub fn verify_content(src: &dyn PackSource, expected: &Hash) -> Result<(), ModError> {
    let actual = content_hash(src)?;
    if &actual == expected {
        Ok(())
    } else {
        Err(ModError::HashMismatch {
            expected: hex(expected),
            actual: hex(&actual),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{sample, MANIFEST};

    fn folder(name: &str) -> ModFolder {
        ModFolder::scan(&sample(name)).unwrap()
    }

    #[test]
    fn a_folder_and_its_pack_have_one_hash() {
        let f = folder("same-hash");
        let pack = MemoryPack::from_folder(&f).unwrap();
        assert_eq!(content_hash(&f).unwrap(), content_hash(&pack).unwrap());
        assert_eq!(pack.paths().len(), 5); // mod.toml and four files
                                           // The order a source lists its paths in does not matter.
        struct Reversed(MemoryPack);
        impl PackSource for Reversed {
            fn paths(&self) -> Vec<String> {
                let mut p = self.0.paths();
                p.reverse();
                p
            }
            fn read(&self, path: &str) -> Result<Vec<u8>, ModError> {
                self.0.read(path)
            }
        }
        assert_eq!(
            content_hash(&Reversed(pack.clone())).unwrap(),
            content_hash(&pack).unwrap()
        );
    }

    #[test]
    fn the_manifest_reads_back_from_a_pack() {
        let f = folder("manifest-back");
        let pack = MemoryPack::from_folder(&f).unwrap();
        assert_eq!(read_manifest(&pack).unwrap(), f.manifest);
        assert_eq!(pack.read(MANIFEST_FILE).unwrap(), MANIFEST.as_bytes());
        // Only the manifest file is asked for.
        struct OnlyManifest(MemoryPack);
        impl PackSource for OnlyManifest {
            fn paths(&self) -> Vec<String> {
                self.0.paths()
            }
            fn read(&self, path: &str) -> Result<Vec<u8>, ModError> {
                assert_eq!(path, MANIFEST_FILE, "read_manifest touched {path}");
                self.0.read(path)
            }
        }
        assert_eq!(read_manifest(&OnlyManifest(pack)).unwrap().id, "test-mod");
    }

    #[test]
    fn a_bad_manifest_in_a_pack_is_named_by_field() {
        let mut pack = MemoryPack::from_folder(&folder("bad-manifest")).unwrap();
        pack.files.insert(
            MANIFEST_FILE.into(),
            MANIFEST.replace("api = 1", "api = 0").into_bytes(),
        );
        let e = read_manifest(&pack).unwrap_err().to_string();
        assert!(e.contains("`api`"), "{e}");
        pack.files.insert(MANIFEST_FILE.into(), vec![0xff, 0xfe]);
        assert!(read_manifest(&pack).is_err());
    }

    #[test]
    fn any_change_is_a_hash_mismatch() {
        let f = folder("mismatch");
        let pack = MemoryPack::from_folder(&f).unwrap();
        let hash = content_hash(&pack).unwrap();
        assert!(verify_content(&pack, &hash).is_ok());

        let mut changed = pack.clone();
        changed.files.get_mut("files/art/pic.bin").unwrap()[0] ^= 1;
        let e = verify_content(&changed, &hash).unwrap_err();
        assert!(matches!(e, ModError::HashMismatch { .. }), "{e}");
        assert!(e.to_string().contains("changed or damaged"));

        let mut extra = pack.clone();
        extra.files.insert("files/new.txt".into(), vec![]);
        assert!(verify_content(&extra, &hash).is_err());

        let mut missing = pack.clone();
        missing.files.remove("files/rules.toml");
        assert!(verify_content(&missing, &hash).is_err());

        // Moving bytes between two files changes the hash too.
        let mut moved = pack.clone();
        let a = moved.files.get("files/art/pic.bin").unwrap().clone();
        let t = moved.files.get_mut("files/tables/extra.txt").unwrap();
        t.extend_from_slice(&a);
        moved.files.get_mut("files/art/pic.bin").unwrap().clear();
        assert!(verify_content(&moved, &hash).is_err());

        // A changed manifest is a change as well.
        let mut mf = pack.clone();
        mf.files.insert(
            MANIFEST_FILE.into(),
            MANIFEST.replace("0.1.0", "0.1.1").into_bytes(),
        );
        assert!(verify_content(&mf, &hash).is_err());
    }

    #[test]
    fn unsafe_pack_paths_are_refused() {
        let base = MemoryPack::from_folder(&folder("paths")).unwrap();
        for bad in [
            "../evil.txt",
            "files/../../evil.txt",
            "/abs.txt",
            "other/x.txt",
            "files\\x.txt",
            "files/COM1.txt",
        ] {
            let mut p = base.clone();
            p.files.insert(bad.into(), vec![1]);
            let e = content_hash(&p).unwrap_err();
            assert!(matches!(e, ModError::BadPack(_)), "{bad}: {e}");
        }
        let mut twin = base.clone();
        twin.files.insert("files/ART/pic.bin".into(), vec![1]);
        assert!(content_hash(&twin).is_err());
        let mut none = base;
        none.files.remove(MANIFEST_FILE);
        assert!(check_pack_paths(&none.paths()).is_err());
    }
}
