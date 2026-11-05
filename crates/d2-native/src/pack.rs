//! Data packs: one file holding a whole layer folder, read in place.
//!
//! A mod game ships its converted layer as a single pack next to its
//! executable instead of a folder of tens of thousands of files. The pack
//! holds the folder exactly: every file's bytes, under its `/`-joined path
//! relative to the folder (`mod.toml`, `files/...`). The layer stack reads a
//! pack layer the same way as a folder layer ([`crate::layers`]), and the
//! bytes it returns are the folder's bytes.
//!
//! Layout (all integers little-endian):
//!
//! ```text
//! header   magic "D2GPACK\x1a" (8), format version u32 (= 1), reserved u32 (= 0)
//! data     every stored file, back to back, in index order
//! index    count u32, then per file:
//!            path length u16, path (UTF-8), offset u64, stored length u64,
//!            raw length u64, method u8 (0 stored, 1 zstd), crc32 of the raw bytes u32
//! trailer  index offset u64, index length u64, SHA-256 of the index (32),
//!          magic "D2GPKEND" (8)
//! ```
//!
//! Why this shape:
//! - Each file is compressed on its own (zstd, through the pure-Rust
//!   `ruzstd`, already in the build), so any file is read with one
//!   positioned read and one decode: random access without unpacking the
//!   rest. A file is kept uncompressed when zstd does not make it smaller
//!   (most PNGs), so those reads are a plain copy.
//! - Reads are positioned file reads, not a memory map: the workspace
//!   forbids `unsafe`, which mapping a file needs. Positioned reads share
//!   one open handle between threads without a lock.
//! - The index sits at the end so the pack is written in one pass; the
//!   trailer's SHA-256 covers the index, so a cut-off or damaged pack is
//!   refused when it is opened, and every read checks the file's CRC-32, so
//!   a damaged file is refused when it is read. `verify` reads every file.
//! - Paths are sorted and unique, so a lookup is a binary search and the
//!   files of a folder are one contiguous range.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// A pack file's extension, without the dot.
pub const EXTENSION: &str = "d2pack";
/// The pack format this build writes and reads.
pub const FORMAT_VERSION: u32 = 1;

const MAGIC: &[u8; 8] = b"D2GPACK\x1a";
const END_MAGIC: &[u8; 8] = b"D2GPKEND";
const HEADER_LEN: u64 = 16;
const TRAILER_LEN: u64 = 8 + 8 + 32 + 8;
/// The largest single file a pack holds; a larger length in an index is
/// damage, not data.
const MAX_FILE: u64 = 1 << 32;
/// How many files are compressed at once between writes.
const BATCH: usize = 512;

const STORED: u8 = 0;
const ZSTD: u8 = 1;

/// A pack that cannot be built, opened or read: the pack (or folder) and
/// what is wrong.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{pack}: {detail}")]
pub struct PackError {
    /// The pack or folder path.
    pub pack: String,
    /// What is wrong, naming the file inside the pack when there is one.
    pub detail: String,
}

fn err(pack: &Path, detail: impl Into<String>) -> PackError {
    PackError {
        pack: pack.display().to_string(),
        detail: detail.into(),
    }
}

/// One file of a pack.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    path: String,
    offset: u64,
    stored: u64,
    raw: u64,
    method: u8,
    crc: u32,
}

/// An open pack: its index in memory, its file open for positioned reads.
#[derive(Debug)]
pub struct Pack {
    path: PathBuf,
    file: File,
    entries: Vec<Entry>,
    index_sha256: [u8; 32],
}

/// Whether `p` is a path a pack may hold: relative, `/`-joined, no empty,
/// `.` or `..` parts, no `\`.
fn valid_path(p: &str) -> bool {
    !p.is_empty()
        && !p.contains('\\')
        && !p.contains(':')
        && p.split('/').all(|s| !s.is_empty() && s != "." && s != "..")
}

#[cfg(unix)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(file, buf, offset)
}

#[cfg(windows)]
fn read_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        match file.seek_read(buf, offset) {
            Ok(0) => return Err(std::io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => {
                buf = &mut buf[n..];
                offset += n as u64;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// A little-endian reader over the index bytes.
struct Cursor<'a> {
    bytes: &'a [u8],
}

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.bytes.len() < n {
            return None;
        }
        let (head, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }
    fn u64(&mut self) -> Option<u64> {
        self.take(8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    }
}

impl Pack {
    /// Opens the pack at `path` and checks its structure: the header, the
    /// trailer, the index's SHA-256, and every entry (a valid path, sorted
    /// and unique, its bytes inside the data area, a known method). A
    /// pack that is cut off, damaged or of another format version is
    /// refused here, before any file is read.
    pub fn open(path: &Path) -> Result<Pack, PackError> {
        let file = File::open(path).map_err(|e| err(path, e.to_string()))?;
        let len = file.metadata().map_err(|e| err(path, e.to_string()))?.len();
        if len < HEADER_LEN + TRAILER_LEN {
            return Err(err(
                path,
                format!("{len} bytes: too short to be a data pack (cut off?)"),
            ));
        }
        let mut header = [0u8; HEADER_LEN as usize];
        read_at(&file, &mut header, 0).map_err(|e| err(path, e.to_string()))?;
        if &header[..8] != MAGIC {
            return Err(err(path, "not a data pack (wrong magic)"));
        }
        let version = u32::from_le_bytes(header[8..12].try_into().unwrap());
        if version != FORMAT_VERSION {
            return Err(err(
                path,
                format!(
                    "data pack format {version} is not known (this build reads {FORMAT_VERSION})"
                ),
            ));
        }
        let mut trailer = [0u8; TRAILER_LEN as usize];
        read_at(&file, &mut trailer, len - TRAILER_LEN).map_err(|e| err(path, e.to_string()))?;
        if &trailer[48..] != END_MAGIC {
            return Err(err(
                path,
                "the pack's end marker is missing: the file is cut off or damaged",
            ));
        }
        let index_offset = u64::from_le_bytes(trailer[..8].try_into().unwrap());
        let index_len = u64::from_le_bytes(trailer[8..16].try_into().unwrap());
        let index_end = len - TRAILER_LEN;
        if index_offset < HEADER_LEN || index_offset.checked_add(index_len) != Some(index_end) {
            return Err(err(path, "the index position is damaged"));
        }
        let mut index = vec![0u8; index_len as usize];
        read_at(&file, &mut index, index_offset).map_err(|e| err(path, e.to_string()))?;
        let index_sha256: [u8; 32] = Sha256::digest(&index).into();
        if index_sha256[..] != trailer[16..48] {
            return Err(err(
                path,
                "the index does not match its checksum: the pack is damaged",
            ));
        }
        let entries = parse_index(path, &index, index_offset)?;
        Ok(Pack {
            path: path.to_path_buf(),
            file,
            entries,
            index_sha256,
        })
    }

    /// The pack's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The SHA-256 of the pack's index: changes whenever any file's bytes,
    /// name or placement change, so it identifies the pack's content.
    pub fn index_sha256(&self) -> [u8; 32] {
        self.index_sha256
    }

    /// Every file path in the pack, sorted.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.path.as_str())
    }

    /// How many files the pack holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the pack holds no file.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn entry(&self, path: &str) -> Option<&Entry> {
        self.entries
            .binary_search_by(|e| e.path.as_str().cmp(path))
            .ok()
            .map(|i| &self.entries[i])
    }

    /// Whether the pack holds file `path`.
    pub fn contains(&self, path: &str) -> bool {
        self.entry(path).is_some()
    }

    /// The raw length of file `path`.
    pub fn file_len(&self, path: &str) -> Option<u64> {
        self.entry(path).map(|e| e.raw)
    }

    /// The paths under folder `dir` (no trailing `/`), relative to it,
    /// sorted. Empty `dir` lists every file.
    pub fn files_under(&self, dir: &str) -> Vec<&str> {
        if dir.is_empty() {
            return self.paths().collect();
        }
        let prefix = format!("{dir}/");
        let start = self
            .entries
            .partition_point(|e| e.path.as_str() < prefix.as_str());
        self.entries[start..]
            .iter()
            .take_while(|e| e.path.starts_with(&prefix))
            .map(|e| &e.path[prefix.len()..])
            .collect()
    }

    /// Whether some file lies under folder `dir`.
    pub fn has_dir(&self, dir: &str) -> bool {
        let prefix = format!("{dir}/");
        let start = self
            .entries
            .partition_point(|e| e.path.as_str() < prefix.as_str());
        self.entries
            .get(start)
            .is_some_and(|e| e.path.starts_with(&prefix))
    }

    /// The bytes of file `path`: `None` when the pack does not hold it, an
    /// error naming the file when its bytes do not decode to the length and
    /// CRC-32 the index records.
    pub fn read(&self, path: &str) -> Option<Result<Vec<u8>, PackError>> {
        let e = self.entry(path)?;
        Some(self.read_entry(e))
    }

    fn read_entry(&self, e: &Entry) -> Result<Vec<u8>, PackError> {
        let fail = |d: String| err(&self.path, format!("{}: {d}", e.path));
        let mut stored = vec![0u8; e.stored as usize];
        read_at(&self.file, &mut stored, e.offset).map_err(|x| fail(x.to_string()))?;
        let raw = match e.method {
            STORED => stored,
            _ => {
                let mut out = vec![0u8; e.raw as usize];
                let n = ruzstd::decoding::FrameDecoder::new()
                    .decode_all(&stored, &mut out)
                    .map_err(|x| fail(format!("does not decompress: {x}")))?;
                if n as u64 != e.raw {
                    return Err(fail(format!(
                        "decompresses to {n} bytes, the index says {}",
                        e.raw
                    )));
                }
                out
            }
        };
        if crc32fast::hash(&raw) != e.crc {
            return Err(fail("checksum mismatch: the pack is damaged".into()));
        }
        Ok(raw)
    }
}

fn parse_index(path: &Path, index: &[u8], data_end: u64) -> Result<Vec<Entry>, PackError> {
    let bad = |d: String| err(path, format!("index: {d}"));
    let mut c = Cursor { bytes: index };
    let count = c.u32().ok_or_else(|| bad("cut off".into()))?;
    let mut entries: Vec<Entry> = Vec::with_capacity(count.min(1 << 20) as usize);
    for _ in 0..count {
        let short = || bad("cut off".into());
        let n = c.u16().ok_or_else(short)? as usize;
        let name = c.take(n).ok_or_else(short)?;
        let name = std::str::from_utf8(name)
            .map_err(|_| bad("a path is not UTF-8".into()))?
            .to_owned();
        let e = Entry {
            offset: c.u64().ok_or_else(short)?,
            stored: c.u64().ok_or_else(short)?,
            raw: c.u64().ok_or_else(short)?,
            method: c.u8().ok_or_else(short)?,
            crc: c.u32().ok_or_else(short)?,
            path: name,
        };
        if !valid_path(&e.path) {
            return Err(bad(format!("`{}` is not a relative file path", e.path)));
        }
        if entries.last().is_some_and(|p| p.path >= e.path) {
            return Err(bad(format!("`{}` is out of order or listed twice", e.path)));
        }
        let fits = e.offset >= HEADER_LEN
            && e.offset
                .checked_add(e.stored)
                .is_some_and(|end| end <= data_end);
        if !fits || e.raw > MAX_FILE {
            return Err(bad(format!("`{}` lies outside the data area", e.path)));
        }
        match e.method {
            STORED if e.stored == e.raw => {}
            ZSTD => {}
            m => {
                return Err(bad(format!(
                    "`{}` has an unknown storage method {m}",
                    e.path
                )))
            }
        }
        entries.push(e);
    }
    if !c.bytes.is_empty() {
        return Err(bad("bytes after the last entry".into()));
    }
    Ok(entries)
}

/// Every file under `dir`, as `/`-joined paths relative to it, sorted by
/// byte order. A name that is not UTF-8 is an error.
pub fn walk(dir: &Path) -> Result<Vec<String>, PackError> {
    let mut out = Vec::new();
    let mut todo = vec![(dir.to_path_buf(), String::new())];
    while let Some((d, prefix)) = todo.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| err(&d, e.to_string()))? {
            let e = e.map_err(|e| err(&d, e.to_string()))?;
            let name = e
                .file_name()
                .into_string()
                .map_err(|n| err(&d, format!("{n:?}: the name is not UTF-8")))?;
            let rel = format!("{prefix}{name}");
            if e.file_type()
                .map_err(|x| err(&e.path(), x.to_string()))?
                .is_dir()
            {
                todo.push((e.path(), format!("{rel}/")));
            } else {
                out.push(rel);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// What [`build`] wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuildStats {
    /// Files packed.
    pub files: usize,
    /// Files stored compressed.
    pub compressed: usize,
    /// The files' total size.
    pub raw_bytes: u64,
    /// The pack's size.
    pub pack_bytes: u64,
}

/// One file ready to write: its stored bytes and index fields.
struct Prepared {
    data: Vec<u8>,
    raw: u64,
    method: u8,
    crc: u32,
}

/// Reads and compresses one file. zstd is kept only when it saves at least
/// 1/32 of the size and decodes back to the same bytes.
fn prepare(dir: &Path, rel: &str) -> Result<Prepared, PackError> {
    let p = dir.join(rel);
    let raw = std::fs::read(&p).map_err(|e| err(&p, e.to_string()))?;
    if raw.len() as u64 > MAX_FILE {
        return Err(err(&p, "too large for a data pack"));
    }
    let crc = crc32fast::hash(&raw);
    let packed = ruzstd::encoding::compress_to_vec(
        raw.as_slice(),
        ruzstd::encoding::CompressionLevel::Fastest,
    );
    let worth = packed.len() + raw.len() / 32 < raw.len();
    if worth {
        let mut back = vec![0u8; raw.len()];
        let same = ruzstd::decoding::FrameDecoder::new()
            .decode_all(&packed, &mut back)
            .is_ok_and(|n| n == raw.len() && back == raw);
        if same {
            return Ok(Prepared {
                data: packed,
                raw: raw.len() as u64,
                method: ZSTD,
                crc,
            });
        }
    }
    Ok(Prepared {
        raw: raw.len() as u64,
        data: raw,
        method: STORED,
        crc,
    })
}

/// Packs every file under folder `dir` into `out`, compressing on
/// `threads` threads (0: one per core). The pack is written next to `out`
/// and renamed into place when complete, so `out` is never half written.
pub fn build(dir: &Path, out: &Path, threads: usize) -> Result<BuildStats, PackError> {
    if !dir.is_dir() {
        return Err(err(dir, "not a folder"));
    }
    let files = walk(dir)?;
    for f in &files {
        if f.len() > u16::MAX as usize || !valid_path(f) {
            return Err(err(dir, format!("{f}: not a path a data pack can hold")));
        }
    }
    let threads = match threads {
        0 => std::thread::available_parallelism().map_or(1, |n| n.get()),
        n => n,
    };
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| err(parent, e.to_string()))?;
    }
    let tmp = out.with_extension(format!("{EXTENSION}.part"));
    let result = write_pack(dir, &files, &tmp, threads);
    match result {
        Ok(stats) => {
            std::fs::rename(&tmp, out).map_err(|e| err(out, e.to_string()))?;
            Ok(stats)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn write_pack(
    dir: &Path,
    files: &[String],
    tmp: &Path,
    threads: usize,
) -> Result<BuildStats, PackError> {
    let mut w = std::io::BufWriter::with_capacity(
        1 << 20,
        File::create(tmp).map_err(|e| err(tmp, e.to_string()))?,
    );
    let io = |e: std::io::Error| err(tmp, e.to_string());
    w.write_all(MAGIC).map_err(io)?;
    w.write_all(&FORMAT_VERSION.to_le_bytes()).map_err(io)?;
    w.write_all(&0u32.to_le_bytes()).map_err(io)?;
    let mut offset = HEADER_LEN;
    let mut stats = BuildStats::default();
    let mut entries = Vec::with_capacity(files.len());
    for batch in files.chunks(BATCH) {
        let per = batch.len().div_ceil(threads).max(1);
        let prepared: Vec<Result<Prepared, PackError>> = std::thread::scope(|s| {
            let jobs: Vec<_> = batch
                .chunks(per)
                .map(|part| {
                    s.spawn(move || part.iter().map(|f| prepare(dir, f)).collect::<Vec<_>>())
                })
                .collect();
            jobs.into_iter()
                .flat_map(|j| j.join().expect("pack worker panicked"))
                .collect()
        });
        for (path, p) in batch.iter().zip(prepared) {
            let p = p?;
            w.write_all(&p.data).map_err(io)?;
            stats.files += 1;
            stats.raw_bytes += p.raw;
            if p.method == ZSTD {
                stats.compressed += 1;
            }
            entries.push(Entry {
                path: path.clone(),
                offset,
                stored: p.data.len() as u64,
                raw: p.raw,
                method: p.method,
                crc: p.crc,
            });
            offset += p.data.len() as u64;
        }
    }
    let mut index = Vec::new();
    index.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for e in &entries {
        index.extend_from_slice(&(e.path.len() as u16).to_le_bytes());
        index.extend_from_slice(e.path.as_bytes());
        index.extend_from_slice(&e.offset.to_le_bytes());
        index.extend_from_slice(&e.stored.to_le_bytes());
        index.extend_from_slice(&e.raw.to_le_bytes());
        index.push(e.method);
        index.extend_from_slice(&e.crc.to_le_bytes());
    }
    w.write_all(&index).map_err(io)?;
    w.write_all(&offset.to_le_bytes()).map_err(io)?;
    w.write_all(&(index.len() as u64).to_le_bytes())
        .map_err(io)?;
    w.write_all(&Sha256::digest(&index)).map_err(io)?;
    w.write_all(END_MAGIC).map_err(io)?;
    let file = w.into_inner().map_err(|e| err(tmp, e.to_string()))?;
    file.sync_all().map_err(io)?;
    stats.pack_bytes = offset + index.len() as u64 + TRAILER_LEN;
    Ok(stats)
}

/// Reads every file of the pack at `path` (structure, decoding and CRC-32)
/// and, with `against`, compares the pack with that folder: the same paths
/// and the same bytes. Returns the problems found, empty when the pack is
/// sound; an error when it cannot be opened at all.
pub fn verify(path: &Path, against: Option<&Path>) -> Result<Vec<String>, PackError> {
    let pack = Pack::open(path)?;
    let mut problems = Vec::new();
    let folder = match against {
        Some(dir) => Some((dir, walk(dir)?)),
        None => None,
    };
    for e in &pack.entries {
        let bytes = match pack.read_entry(e) {
            Ok(b) => b,
            Err(x) => {
                problems.push(x.detail);
                continue;
            }
        };
        if let Some((dir, _)) = &folder {
            match std::fs::read(dir.join(&e.path)) {
                Ok(f) if f == bytes => {}
                Ok(_) => problems.push(format!("{}: differs from the folder's file", e.path)),
                Err(x) => problems.push(format!("{}: not in the folder ({x})", e.path)),
            }
        }
    }
    if let Some((_, files)) = &folder {
        for f in files.iter().filter(|f| !pack.contains(f)) {
            problems.push(format!("{f}: in the folder but not in the pack"));
        }
    }
    Ok(problems)
}

/// The mod kit reads a data pack through [`d2_modkit::PackSource`]: the
/// manifest without unpacking the rest, and the content hash, which is the
/// same for a pack and the folder it was built from.
impl d2_modkit::PackSource for Pack {
    fn paths(&self) -> Vec<String> {
        Pack::paths(self).map(str::to_owned).collect()
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, d2_modkit::ModError> {
        match Pack::read(self, path) {
            Some(Ok(bytes)) => Ok(bytes),
            Some(Err(e)) => Err(d2_modkit::ModError::BadPack(e.to_string())),
            None => Err(d2_modkit::ModError::BadPack(format!(
                "{}: no file {path}",
                self.path().display()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("d2pack-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn index_paths_must_be_relative_file_paths() {
        for ok in ["a", "files/data/x.png", "mod.toml"] {
            assert!(valid_path(ok), "{ok}");
        }
        for bad in ["", "/a", "a/", "a//b", "../a", "a/./b", "a\\b", "c:/a"] {
            assert!(!valid_path(bad), "{bad}");
        }
    }

    #[test]
    fn the_mod_kit_reads_a_built_pack_like_the_folder_it_came_from() {
        let root = tmp("kit");
        let dir = root.join("kit-mod");
        std::fs::create_dir_all(dir.join("files/tables")).unwrap();
        std::fs::write(
            dir.join("mod.toml"),
            format!(
                "id = \"kit-mod\"\nname = \"Kit Mod\"\nversion = \"1.0.0\"\nauthor = \"A\"\n\
                 description = \"\"\napi = {}\nkind = \"full\"\n",
                d2_modkit::CURRENT_API
            ),
        )
        .unwrap();
        std::fs::write(dir.join("files/rules.toml"), "# rules\n").unwrap();
        std::fs::write(dir.join("files/tables/extra.txt"), "a\tb\n").unwrap();
        let out = root.join("kit-mod.d2pack");
        build(&dir, &out, 2).unwrap();

        let pack = Pack::open(&out).unwrap();
        let folder = d2_modkit::ModFolder::scan(&dir).unwrap();
        let from_pack = d2_modkit::read_manifest(&pack).unwrap();
        assert_eq!(from_pack, d2_modkit::read_manifest(&folder).unwrap());
        assert_eq!(from_pack.id, "kit-mod");
        let hash = d2_modkit::content_hash(&pack).unwrap();
        assert_eq!(hash, d2_modkit::content_hash(&folder).unwrap());
        d2_modkit::pack::verify_content(&pack, &hash).unwrap();
        assert!(d2_modkit::PackSource::read(&pack, "files/nope").is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn files_under_lists_one_folder_and_has_dir_sees_it() {
        let root = tmp("under");
        let dir = root.join("layer");
        for f in ["a/b.txt", "a/c/d.txt", "ab.txt", "z.txt"] {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, f).unwrap();
        }
        let out = root.join("l.d2pack");
        build(&dir, &out, 2).unwrap();
        let pack = Pack::open(&out).unwrap();
        assert_eq!(pack.files_under("a"), ["b.txt", "c/d.txt"]);
        assert_eq!(pack.files_under("a/c"), ["d.txt"]);
        assert!(pack.files_under("q").is_empty());
        assert!(pack.has_dir("a/c") && !pack.has_dir("ab.txt") && !pack.has_dir("q"));
        assert_eq!(pack.file_len("z.txt"), Some(5));
        assert!(pack.read("nope").is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
