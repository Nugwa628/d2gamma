//! `mpq-tool diff-installs A B`: which game files differ between two
//! Diablo II installs, e.g. a player's 1.13c folder and a 1.14d one.
//!
//! The game reads every file from the highest-priority archive that has it
//! ([`ArchiveSet`]), so the copies compared are the ones the game would
//! read, wherever each install keeps them. The two installs are compared
//! with each other only, never with a fixed list of hashes, and nothing is
//! written: the output is names, archives and sizes.
//!
//! Names come from the archives' `(listfile)`s ([`known_names`]), the
//! table schema (`patch_d2.mpq` has no listfile, so its excel `.bin` names
//! come from `tables/data/tables.tsv` and the formula buffers) and the
//! string tables of each language. For an archive that differs between the
//! installs, every entry of its hash table is compared as well, by its name
//! hashes, locale and platform ([`EntryCounts`]), so files no name reaches
//! are still covered.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use d2_data::bin::excel_path;
use d2_data::schema::{schema, CalcBuffer};
use d2_formats::mpq::names::{known_names, name_key};
use d2_formats::mpq::{archive_file_name, flags, Archive, ArchiveSet, PRIORITY};

/// Languages whose string tables are looked for.
const LANGUAGES: [&str; 10] = [
    "eng", "deu", "fra", "esp", "ita", "kor", "chi", "jpn", "pol", "por",
];

/// The string tables of each language.
const STRING_TABLES: [&str; 3] = ["string", "expansionstring", "patchstring"];

/// What part of the game a file feeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// `data\global\excel\`: the rule tables.
    Rules,
    /// `.tbl` string tables.
    Strings,
    /// `.ds1` / `.dt1` level layouts and tiles.
    Maps,
    /// Sprites, animations, palettes and fonts.
    Art,
    /// `.wav` sound, speech and music.
    Sound,
    /// Everything else.
    Other,
}

impl Kind {
    /// The kind of the archive path `name`.
    pub fn of(name: &str) -> Kind {
        let n = name.to_ascii_lowercase().replace('/', "\\");
        let ext = n.rsplit_once('.').map_or("", |(_, e)| e);
        if n.starts_with(r"data\global\excel\") {
            Kind::Rules
        } else {
            match ext {
                "tbl" => Kind::Strings,
                "ds1" | "dt1" => Kind::Maps,
                "dc6" | "dcc" | "cof" | "dat" | "pl2" | "pcx" => Kind::Art,
                "wav" => Kind::Sound,
                _ => Kind::Other,
            }
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Rules => "rules",
            Kind::Strings => "strings",
            Kind::Maps => "maps",
            Kind::Art => "art",
            Kind::Sound => "sound",
            Kind::Other => "other",
        }
    }
}

/// How one thing compares between install A and install B.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Same,
    Differs,
    OnlyA,
    OnlyB,
    /// One side could not be read (the reason).
    Unreadable(String),
}

impl State {
    fn label(&self) -> &str {
        match self {
            State::Same => "same",
            State::Differs => "differs",
            State::OnlyA => "only-in-a",
            State::OnlyB => "only-in-b",
            State::Unreadable(_) => "unreadable",
        }
    }
}

/// One archive file of the two installs.
#[derive(Debug, Clone)]
pub struct ArchiveRow {
    /// Lowercase archive name, e.g. `patch_d2.mpq`.
    pub name: &'static str,
    pub size_a: Option<u64>,
    pub size_b: Option<u64>,
    pub state: State,
    /// For an archive in both installs that is not byte-identical: how its
    /// entries compare.
    pub entries: Option<EntryCounts>,
}

/// How the entries of one archive compare between the two installs. An
/// entry is a hash table slot in use, keyed by its two name hashes, locale
/// and platform; contents are compared unpacked, so a file stored with
/// other compression or at another place in the archive is the same.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryCounts {
    pub same: usize,
    pub differs: usize,
    pub only_a: usize,
    pub only_b: usize,
    /// Entries one side could not unpack (an encrypted file whose key can't
    /// be recovered without its name).
    pub unreadable: usize,
}

/// The key of one hash table entry: name hashes, locale, platform.
type EntryKey = (u32, u32, u16, u16);

/// The live entries of `ar`: key to block index.
fn entries(ar: &Archive) -> BTreeMap<EntryKey, usize> {
    let blocks = ar.block_table();
    ar.hash_table()
        .iter()
        .filter_map(|h| {
            let i = usize::try_from(h.block_index).ok()?;
            let live = blocks.get(i)?.has(flags::EXISTS);
            live.then_some(((h.name_a, h.name_b, h.locale, h.platform), i))
        })
        .collect()
}

/// The unpacked bytes of block `index`, recovering the key of an encrypted
/// block; `None` when it can't be read.
fn unpack(ar: &Archive, index: usize) -> Option<Vec<u8>> {
    let key = if ar.block_table()[index].has(flags::ENCRYPTED) {
        Some(ar.recover_key(index).ok()??)
    } else {
        None
    };
    ar.read_block(index, key).ok()
}

/// Compares every entry of the two copies of one archive.
fn compare_entries(x: &Archive, y: &Archive) -> EntryCounts {
    let (ex, ey) = (entries(x), entries(y));
    let mut c = EntryCounts {
        only_b: ey.keys().filter(|k| !ex.contains_key(k)).count(),
        ..EntryCounts::default()
    };
    for (k, &i) in &ex {
        let Some(&j) = ey.get(k) else {
            c.only_a += 1;
            continue;
        };
        match (unpack(x, i), unpack(y, j)) {
            (Some(p), Some(q)) if p == q => c.same += 1,
            (Some(_), Some(_)) => c.differs += 1,
            _ => c.unreadable += 1,
        }
    }
    c
}

/// One game file: the copy each install's game would read.
#[derive(Debug, Clone)]
pub struct FileRow {
    pub name: String,
    pub kind: Kind,
    /// The archive and size of A's copy.
    pub a: Option<(String, u64)>,
    pub b: Option<(String, u64)>,
    pub state: State,
}

/// The comparison of two installs.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub archives: Vec<ArchiveRow>,
    pub files: Vec<FileRow>,
}

impl Report {
    /// Counts of each state per kind.
    pub fn counts(&self) -> BTreeMap<(Kind, String), usize> {
        let mut out = BTreeMap::new();
        for f in &self.files {
            *out.entry((f.kind, f.state.label().to_owned())).or_default() += 1;
        }
        out
    }

    /// The files that are not the same, in name order.
    pub fn changed(&self) -> impl Iterator<Item = &FileRow> {
        self.files.iter().filter(|f| f.state != State::Same)
    }
}

/// The names worth comparing in either install, keyed by [`name_key`].
fn names(a: &ArchiveSet, b: &ArchiveSet) -> Result<BTreeMap<String, String>> {
    let mut out = known_names(a.archives())?;
    for (k, n) in known_names(b.archives())? {
        out.entry(k).or_insert(n);
    }
    let mut extra = Vec::new();
    for t in &schema().tables {
        extra.push(excel_path(&t.bin_name));
        extra.push(excel_path(&t.txt_name));
    }
    for buffer in CalcBuffer::ALL {
        extra.push(excel_path(&format!("{}.bin", buffer.name())));
    }
    for lng in LANGUAGES {
        for t in STRING_TABLES {
            extra.push(format!(r"data\local\lng\{lng}\{t}.tbl"));
        }
    }
    for n in extra {
        out.entry(name_key(&n)).or_insert(n);
    }
    Ok(out)
}

/// Whether two files hold the same bytes.
fn same_file(a: &Path, b: &Path) -> std::io::Result<bool> {
    if std::fs::metadata(a)?.len() != std::fs::metadata(b)?.len() {
        return Ok(false);
    }
    let (mut fa, mut fb) = (File::open(a)?, File::open(b)?);
    let (mut ba, mut bb) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    loop {
        let n = read_full(&mut fa, &mut ba)?;
        let m = read_full(&mut fb, &mut bb)?;
        if n != m || ba[..n] != bb[..m] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

/// Fills `buf` as far as the file allows; the count read.
fn read_full(f: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut done = 0;
    while done < buf.len() {
        match f.read(&mut buf[done..])? {
            0 => break,
            n => done += n,
        }
    }
    Ok(done)
}

fn find_archive<'a>(set: &'a ArchiveSet, name: &str) -> Option<&'a Archive> {
    set.archives()
        .iter()
        .find(|a| archive_file_name(a).eq_ignore_ascii_case(name))
}

/// Compares the installs in folders `dir_a` and `dir_b`.
pub fn diff_installs(dir_a: &Path, dir_b: &Path) -> Result<Report> {
    let a = ArchiveSet::open_dir(dir_a).with_context(|| format!("opening {}", dir_a.display()))?;
    let b = ArchiveSet::open_dir(dir_b).with_context(|| format!("opening {}", dir_b.display()))?;
    let mut report = Report::default();

    // Archives that are byte-identical in both installs.
    let mut identical: BTreeSet<&str> = BTreeSet::new();
    for name in PRIORITY {
        let (x, y) = (find_archive(&a, name), find_archive(&b, name));
        let size = |ar: Option<&Archive>| {
            ar.and_then(|ar| std::fs::metadata(ar.path()).ok())
                .map(|m| m.len())
        };
        let state = match (x, y) {
            (None, None) => continue,
            (Some(_), None) => State::OnlyA,
            (None, Some(_)) => State::OnlyB,
            (Some(x), Some(y)) => match same_file(x.path(), y.path()) {
                Ok(true) => {
                    identical.insert(name);
                    State::Same
                }
                Ok(false) => State::Differs,
                Err(e) => State::Unreadable(e.to_string()),
            },
        };
        let entries = match (x, y, &state) {
            (Some(x), Some(y), State::Differs) => Some(compare_entries(x, y)),
            _ => None,
        };
        report.archives.push(ArchiveRow {
            name,
            size_a: size(x),
            size_b: size(y),
            state,
            entries,
        });
    }

    for name in names(&a, &b)?.into_values() {
        let (x, y) = (a.find(&name), b.find(&name));
        if x.is_none() && y.is_none() {
            continue;
        }
        let source = |ar: &Archive| archive_file_name(ar).to_ascii_lowercase();
        let size = |ar: &Archive| {
            ar.find(&name)
                .map_or(0, |i| u64::from(ar.block_table()[i].file_size))
        };
        let state = match (x, y) {
            (Some(_), None) => State::OnlyA,
            (None, Some(_)) => State::OnlyB,
            (Some(x), Some(y)) => {
                let (sx, sy) = (source(x), source(y));
                if sx == sy && identical.contains(sx.as_str()) {
                    State::Same
                } else {
                    match (x.read(&name), y.read(&name)) {
                        (Ok(p), Ok(q)) if p == q => State::Same,
                        (Ok(_), Ok(_)) => State::Differs,
                        (Err(e), _) | (_, Err(e)) => State::Unreadable(e.to_string()),
                    }
                }
            }
            (None, None) => unreachable!("skipped above"),
        };
        report.files.push(FileRow {
            kind: Kind::of(&name),
            a: x.map(|ar| (source(ar), size(ar))),
            b: y.map(|ar| (source(ar), size(ar))),
            name,
            state,
        });
    }
    Ok(report)
}

/// `mpq-tool diff-installs A B`.
pub fn run(dir_a: &Path, dir_b: &Path) -> Result<()> {
    let report = diff_installs(dir_a, dir_b)?;
    let size = |s: Option<u64>| s.map_or("-".to_owned(), |s| s.to_string());
    println!("A = {}", dir_a.display());
    println!("B = {}", dir_b.display());
    println!("archives:");
    for r in &report.archives {
        println!(
            "  {:<10} {:<14} A {:>10}  B {:>10}",
            r.state.label(),
            r.name,
            size(r.size_a),
            size(r.size_b)
        );
        if let Some(e) = &r.entries {
            println!(
                "             entries: {} same, {} differ, {} only in A, {} only in B, {} unreadable",
                e.same, e.differs, e.only_a, e.only_b, e.unreadable
            );
        }
    }
    println!("files that are not the same (the copy the game reads):");
    let side = |s: &Option<(String, u64)>| match s {
        Some((ar, n)) => format!("{ar} {n}"),
        None => "-".to_owned(),
    };
    for f in report.changed() {
        let why = match &f.state {
            State::Unreadable(e) => format!("  ({e})"),
            _ => String::new(),
        };
        println!(
            "  {:<10} {:<7} {}  A {}  B {}{why}",
            f.state.label(),
            f.kind.label(),
            f.name,
            side(&f.a),
            side(&f.b)
        );
    }
    println!("summary ({} files compared):", report.files.len());
    for ((kind, state), n) in report.counts() {
        println!("  {:<7} {state:<10} {n}", kind.label());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2_formats::mpq::writer::MpqWriter;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("mpq-tool-diff-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// A small install: `d2data` (listfile) with art and a table that
    /// `patch_d2` (no listfile, file name `file`) overrides with `skills`.
    fn install(dir: &Path, file: &str, skills: &[u8], extra: Option<(&str, &[u8])>) {
        let mut d = MpqWriter::new().with_listfile();
        d.add_file(r"data\global\ui\panel\x.dc6", b"art".to_vec());
        d.add_file(r"data\global\excel\skills.bin", b"base".to_vec());
        if let Some((n, bytes)) = extra {
            d.add_file(n, bytes.to_vec());
        }
        d.write(dir.join("d2data.mpq")).unwrap();
        let mut x = MpqWriter::new().with_listfile();
        x.add_file(r"data\global\tiles\act1\town\a.ds1", b"map".to_vec());
        x.write(dir.join("d2exp.mpq")).unwrap();
        let mut p = MpqWriter::new();
        p.add_file(r"data\global\excel\skills.bin", skills.to_vec());
        p.add_file(r"data\local\lng\eng\patchstring.tbl", b"tbl".to_vec());
        p.write(dir.join(file)).unwrap();
    }

    fn row<'a>(r: &'a Report, name: &str) -> &'a FileRow {
        r.files
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("{name} not compared"))
    }

    #[test]
    fn identical_installs_differ_nowhere_whatever_the_archive_case() {
        let (a, b) = (scratch("same-a"), scratch("same-b"));
        install(&a, "patch_d2.mpq", b"new", None);
        // A 1.13-era folder: mixed-case patch name, the old exe and DLLs.
        install(&b, "Patch_D2.mpq", b"new", None);
        std::fs::write(b.join("Game.exe"), b"exe").unwrap();
        std::fs::write(b.join("D2Common.dll"), b"dll").unwrap();
        let r = diff_installs(&a, &b).unwrap();
        assert!(r.archives.iter().all(|x| x.state == State::Same), "{r:?}");
        assert!(r.archives.iter().all(|x| x.entries.is_none()), "{r:?}");
        assert_eq!(r.changed().count(), 0, "{r:?}");
        // The patch's table (no listfile) is found through the schema.
        let s = row(&r, r"data\global\excel\skills.bin");
        assert_eq!(s.a, Some(("patch_d2.mpq".into(), 3)));
        assert_eq!(s.kind, Kind::Rules);
        assert_eq!(
            row(&r, r"data\local\lng\eng\patchstring.tbl").state,
            State::Same
        );
    }

    #[test]
    fn a_changed_patch_table_is_named_with_the_archive_it_comes_from() {
        let (a, b) = (scratch("diff-a"), scratch("diff-b"));
        install(&a, "patch_d2.mpq", b"1.14", None);
        install(
            &b,
            "patch_d2.mpq",
            b"1.12x",
            Some((r"data\global\ui\panel\y.dc6", b"more")),
        );
        let r = diff_installs(&a, &b).unwrap();
        let state = |n: &str| {
            r.archives
                .iter()
                .find(|x| x.name == n)
                .unwrap()
                .state
                .clone()
        };
        assert_eq!(state("patch_d2.mpq"), State::Differs);
        assert_eq!(state("d2exp.mpq"), State::Same);
        let entries = |n: &str| {
            r.archives
                .iter()
                .find(|x| x.name == n)
                .unwrap()
                .entries
                .clone()
        };
        // The patch's table changed, its string table did not.
        assert_eq!(
            entries("patch_d2.mpq"),
            Some(EntryCounts {
                same: 1,
                differs: 1,
                ..EntryCounts::default()
            })
        );
        // d2data gained a file (and its listfile changed with it).
        let d = entries("d2data.mpq").unwrap();
        assert_eq!((d.only_a, d.only_b, d.differs), (0, 1, 1), "{d:?}");
        assert_eq!(entries("d2exp.mpq"), None);
        let s = row(&r, r"data\global\excel\skills.bin");
        assert_eq!(s.state, State::Differs);
        assert_eq!(s.b, Some(("patch_d2.mpq".into(), 5)));
        assert_eq!(row(&r, r"data\global\ui\panel\y.dc6").state, State::OnlyB);
        // d2data differs as a file, but its common art is the same.
        assert_eq!(state("d2data.mpq"), State::Differs);
        assert_eq!(row(&r, r"data\global\ui\panel\x.dc6").state, State::Same);
        let changed: Vec<&str> = r.changed().map(|f| f.name.as_str()).collect();
        assert_eq!(changed.len(), 2, "{changed:?}");
        assert_eq!(r.counts()[&(Kind::Rules, "differs".to_owned())], 1);
    }

    #[test]
    fn a_missing_archive_is_reported_and_its_files_are_only_on_one_side() {
        let (a, b) = (scratch("miss-a"), scratch("miss-b"));
        install(&a, "patch_d2.mpq", b"t", None);
        install(&b, "patch_d2.mpq", b"t", None);
        let mut m = MpqWriter::new().with_listfile();
        m.add_file(r"data\global\music\act1\town1.wav", b"music".to_vec());
        m.write(a.join("d2music.mpq")).unwrap();
        let r = diff_installs(&a, &b).unwrap();
        let music = r.archives.iter().find(|x| x.name == "d2music.mpq").unwrap();
        assert_eq!((music.state.clone(), music.size_b), (State::OnlyA, None));
        let w = row(&r, r"data\global\music\act1\town1.wav");
        assert_eq!((w.state.clone(), w.kind), (State::OnlyA, Kind::Sound));
    }

    #[test]
    fn entries_no_name_reaches_are_compared_by_their_hashes() {
        // B's patch has one more file that no listfile or table names: the
        // files the names reach are all the same, the entry rows still see it.
        let (a, b) = (scratch("unnamed-a"), scratch("unnamed-b"));
        let mut x = MpqWriter::new();
        x.add_file(r"data\global\excel\skills.bin", b"t".to_vec());
        x.write(a.join("patch_d2.mpq")).unwrap();
        let mut y = MpqWriter::new();
        y.add_file(r"data\global\excel\skills.bin", b"t".to_vec());
        y.add_file(r"data\global\ui\hidden\new.dc6", b"new".to_vec());
        y.write(b.join("patch_d2.mpq")).unwrap();
        let r = diff_installs(&a, &b).unwrap();
        assert_eq!(r.changed().count(), 0, "{r:?}");
        assert_eq!(
            r.archives[0].entries,
            Some(EntryCounts {
                same: 1,
                only_b: 1,
                ..EntryCounts::default()
            })
        );
    }

    #[test]
    fn kinds_follow_the_folder_and_extension() {
        assert_eq!(Kind::of(r"DATA\GLOBAL\EXCEL\Misc.txt"), Kind::Rules);
        assert_eq!(Kind::of("data/local/lng/eng/string.tbl"), Kind::Strings);
        assert_eq!(Kind::of(r"data\global\tiles\x.DT1"), Kind::Maps);
        assert_eq!(Kind::of(r"data\global\palette\act1\pal.dat"), Kind::Art);
        assert_eq!(Kind::of(r"data\global\sfx\a.wav"), Kind::Sound);
        assert_eq!(Kind::of(r"data\global\animdata.d2"), Kind::Other);
    }
}
