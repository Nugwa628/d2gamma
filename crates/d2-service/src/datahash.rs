//! The game-data hash: one SHA-256 over every file in a game-data folder
//! (contracts 11.2, `NodeHello.data_hash`). A game server and its main
//! server must run on the same game data, so each hashes its own folder
//! and the main server compares them.
//!
//! The hash covers each file's path below the folder (with `/`), its
//! size and its bytes, in path order, so a renamed, added, removed or
//! changed file changes it. A game folder is large (about 2 GB), so the
//! result is kept in a cache file next to a stamp of the folder (every
//! file's path, size and modification time): a start with an unchanged
//! folder reads the cache instead of every byte.

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};

/// The cache file's name, in the folder the program passes.
pub const CACHE_FILE: &str = "game-data.sha256";

/// One file of the folder.
#[derive(Debug)]
struct FileEntry {
    /// The path below the folder, with `/`.
    rel: String,
    path: PathBuf,
    size: u64,
    /// The modification time in nanoseconds since 1970 (0 if unknown).
    mtime_ns: u128,
}

/// Every file below `dir`, sorted by path.
fn files(dir: &Path) -> io::Result<Vec<FileEntry>> {
    let mut out = Vec::new();
    let mut todo = vec![(dir.to_path_buf(), String::new())];
    while let Some((folder, prefix)) = todo.pop() {
        for item in std::fs::read_dir(&folder)? {
            let item = item?;
            let name = item.file_name().to_string_lossy().into_owned();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let meta = std::fs::metadata(item.path())?;
            if meta.is_dir() {
                todo.push((item.path(), rel));
            } else if meta.is_file() {
                let mtime_ns = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                out.push(FileEntry {
                    rel,
                    path: item.path(),
                    size: meta.len(),
                    mtime_ns,
                });
            }
        }
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

/// The folder's stamp: its files' paths, sizes and times, hashed.
fn stamp(files: &[FileEntry]) -> [u8; 32] {
    let mut h = Sha256::new();
    for f in files {
        h.update((f.rel.len() as u64).to_le_bytes());
        h.update(f.rel.as_bytes());
        h.update(f.size.to_le_bytes());
        h.update(f.mtime_ns.to_le_bytes());
    }
    h.finalize().into()
}

/// The hash of the files' paths, sizes and bytes.
fn content(files: &[FileEntry]) -> io::Result<[u8; 32]> {
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    for f in files {
        h.update((f.rel.len() as u64).to_le_bytes());
        h.update(f.rel.as_bytes());
        h.update(f.size.to_le_bytes());
        let mut file = File::open(&f.path)?;
        let mut left = f.size;
        while left > 0 {
            let n = file.read(&mut buf)?;
            if n == 0 {
                return Err(io::Error::other(format!(
                    "{} changed while it was hashed",
                    f.rel
                )));
            }
            h.update(&buf[..n]);
            left = left.saturating_sub(n as u64);
        }
    }
    Ok(h.finalize().into())
}

fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<[u8; 32]> {
    d2_quic::parse_fingerprint(s)
}

/// The cached hash in `cache` when its stamp is `want`.
fn cached(cache: &Path, want: &[u8; 32]) -> Option<[u8; 32]> {
    let text = std::fs::read_to_string(cache).ok()?;
    let (s, h) = text.trim().split_once(' ')?;
    (unhex(s)? == *want).then_some(())?;
    unhex(h)
}

/// The hash of the game-data folder `dir`. With `cache` (a folder), the
/// result is read from and written to [`CACHE_FILE`] there; a cache that
/// can't be written is skipped.
pub fn game_data_hash(dir: &Path, cache: Option<&Path>) -> io::Result<[u8; 32]> {
    let files = files(dir)?;
    let stamp = stamp(&files);
    let cache_file = cache.map(|c| c.join(CACHE_FILE));
    if let Some(hash) = cache_file.as_deref().and_then(|c| cached(c, &stamp)) {
        return Ok(hash);
    }
    let hash = content(&files)?;
    if let (Some(folder), Some(file)) = (cache, cache_file) {
        let tmp = folder.join(format!("{CACHE_FILE}.tmp"));
        let text = format!("{} {}\n", hex(&stamp), hex(&hash));
        if std::fs::create_dir_all(folder).is_ok() && std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &file);
        }
    }
    Ok(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("d2-service-hash-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("data/global")).unwrap();
        std::fs::write(d.join("data/global/a.txt"), b"one").unwrap();
        std::fs::write(d.join("b.mpq"), b"two two").unwrap();
        d
    }

    #[test]
    fn the_hash_follows_paths_sizes_and_bytes() {
        let d = dir("follows");
        let first = game_data_hash(&d, None).unwrap();
        assert_eq!(game_data_hash(&d, None).unwrap(), first, "stable");
        // The same bytes in another folder hash the same.
        let other = dir("follows-copy");
        assert_eq!(game_data_hash(&other, None).unwrap(), first);
        // One byte changed, same size.
        std::fs::write(other.join("b.mpq"), b"two twX").unwrap();
        let changed = game_data_hash(&other, None).unwrap();
        assert_ne!(changed, first);
        // A file renamed.
        std::fs::write(other.join("b.mpq"), b"two two").unwrap();
        std::fs::rename(other.join("b.mpq"), other.join("c.mpq")).unwrap();
        assert_ne!(game_data_hash(&other, None).unwrap(), first);
        // A file added.
        std::fs::rename(other.join("c.mpq"), other.join("b.mpq")).unwrap();
        assert_eq!(game_data_hash(&other, None).unwrap(), first);
        std::fs::write(other.join("extra"), b"").unwrap();
        assert_ne!(game_data_hash(&other, None).unwrap(), first);
        assert!(game_data_hash(&d.join("missing"), None).is_err());
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&other);
    }

    /// An unchanged folder's hash comes from the cache; a changed file
    /// (its size or time) is hashed again.
    #[test]
    fn the_cache_is_used_until_the_folder_changes() {
        let d = dir("cache");
        let cache = d.with_extension("cache");
        let _ = std::fs::remove_dir_all(&cache);
        let real = game_data_hash(&d, Some(&cache)).unwrap();
        assert_eq!(real, game_data_hash(&d, None).unwrap());
        let file = cache.join(CACHE_FILE);
        let text = std::fs::read_to_string(&file).unwrap();
        let (stamp, _) = text.trim().split_once(' ').unwrap();
        // Proof the cache is read: a planted hash for the same stamp.
        let planted = format!("{stamp} {}\n", "ab".repeat(32));
        std::fs::write(&file, planted).unwrap();
        assert_eq!(game_data_hash(&d, Some(&cache)).unwrap(), [0xab; 32]);
        // A file of another size: hashed again, and the cache rewritten.
        std::fs::write(d.join("b.mpq"), b"three").unwrap();
        let again = game_data_hash(&d, Some(&cache)).unwrap();
        assert_ne!(again, [0xab; 32]);
        assert_eq!(again, game_data_hash(&d, None).unwrap());
        assert!(std::fs::read_to_string(&file)
            .unwrap()
            .ends_with(&format!("{}\n", hex(&again))));
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&cache);
    }
}
