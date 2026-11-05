//! The kinds this binary converts.
//!
//! The image and text kinds are adapters over the `d2-native`
//! reader and writer of that kind ([`Native`]); the verbatim kinds,
//! `excel`, `colormap`, `palshift` and `colortxt`, are copies checked by byte
//! equality.

use d2_native::asset::{AssetError, AssetKind};
use d2_native::kind::NativeFile;

use crate::kind::{Failure, Kind, NativeData, Written};

/// All kinds this build converts, in registry order.
pub fn builtin() -> Vec<Box<dyn Kind>> {
    let mut v: Vec<Box<dyn Kind>> = vec![
        Box::new(Excel),
        Box::new(ColorMap),
        Box::new(PalShift),
        Box::new(ColorTxt),
    ];
    v.extend(
        AssetKind::ALL
            .into_iter()
            .map(|k| Box::new(Native(k)) as Box<dyn Kind>),
    );
    v
}

/// The kinds a server reads: the tables, the strings, the maps and their
/// tiles, the animation lengths and the experience bar's field. A game
/// server's probe (one game, a new character of every class joins and is
/// saved) reads files of these kinds only; the art, sounds, palettes and
/// fonts are the client's.
pub const SERVER_KINDS: [&str; 6] = ["excel", "tbl", "ds1", "dt1", "animdata", "expfield"];

/// [`builtin`] limited to [`SERVER_KINDS`], in registry order: what a
/// server's own conversion converts.
pub fn server() -> Vec<Box<dyn Kind>> {
    builtin()
        .into_iter()
        .filter(|k| SERVER_KINDS.contains(&k.name()))
        .collect()
}

/// Extensions no kind claims on purpose, with the reason the report lists
/// their files under. None at the moment: sounds convert (`wav`).
pub const SKIPPED_EXTENSIONS: &[(&str, &str)] = &[];

/// Files a kind claims by name that the game never loads: not converted,
/// counted as skipped and listed in the report with the reason. Canonical
/// paths.
pub const NEVER_LOADED: &[(&str, &str)] = &[
    (
        "data/global/chars/am/cof/amblxbow.cof",
        "not a COF; its name breaks the token + mode + class pattern (cof.md)",
    ),
    (
        "data/global/tiles/act1/barracks/barracks.dt1",
        "version-4 DT1 named by no LvlTypes, LvlPrest or DS1 (dt1.md)",
    ),
    (
        "data/global/tiles/act1/barracks/gargtrap.dt1",
        "version-4 DT1 named by no LvlTypes, LvlPrest or DS1 (dt1.md)",
    ),
    (
        "data/global/tiles/act1/catacomb/catacombs.dt1",
        "version-4 DT1 named by no LvlTypes, LvlPrest or DS1 (dt1.md)",
    ),
    (
        "data/global/tiles/act1/cathedrl/cathedrl.dt1",
        "version-4 DT1 named by no LvlTypes, LvlPrest or DS1 (dt1.md)",
    ),
    (
        "data/global/tiles/act1/court/court.dt1",
        "version-4 DT1 named by no LvlTypes, LvlPrest or DS1 (dt1.md)",
    ),
    (
        "data/global/tiles/act1/outdoors/outdoor1.dt1",
        "version-4 DT1 named by no LvlTypes, LvlPrest or DS1 (dt1.md)",
    ),
    (
        "data/global/sfx/cursor/curindx.wav",
        "an index file, not a RIFF/WAVE sound; sounds.txt names no such file",
    ),
    (
        "data/global/sfx/cursor/wavindx.wav",
        "an index file, not a RIFF/WAVE sound; sounds.txt names no such file",
    ),
    (
        "data/local/font/latin/default.tbl",
        "plain text, not a font table; Game.exe never names it (tbl.md OQ2)",
    ),
    (
        "data/local/font/latin/fonter.tbl",
        "plain text, not a font table; Game.exe never names it (tbl.md OQ2)",
    ),
];

/// The reason `canon` is never loaded, if it is on [`NEVER_LOADED`].
pub fn never_loaded(canon: &str) -> Option<&'static str> {
    NEVER_LOADED
        .iter()
        .find(|(p, _)| *p == canon)
        .map(|(_, why)| *why)
}

/// Adapter over `d2_native::asset::AssetKind` (the image kinds and the
/// text kinds).
pub struct Native(pub AssetKind);

fn fail(e: AssetError) -> Failure {
    Failure::new(e.check, e.detail)
}

impl Kind for Native {
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn native_version(&self) -> u32 {
        self.0.native_version()
    }
    fn phase(&self) -> u8 {
        self.0.phase()
    }
    fn claims(&self, canon: &str) -> bool {
        self.0.claims(canon)
    }
    fn has_references(&self) -> bool {
        self.0.has_references()
    }
    fn references(&self, canon: &str, src: &[u8]) -> Vec<String> {
        self.0.references(canon, src)
    }
    fn write(&self, canon: &str, src: &[u8]) -> Result<Written, Failure> {
        let w = self.0.write(canon, src).map_err(fail)?;
        Ok(Written {
            files: w
                .files
                .into_iter()
                .map(|f| NativeData {
                    name: f.path,
                    bytes: f.bytes,
                })
                .collect(),
            notes: w.notes,
        })
    }
    fn check(&self, canon: &str, src: &[u8], native: &[NativeData]) -> Result<(), Failure> {
        let files: Vec<NativeFile> = native
            .iter()
            .map(|n| NativeFile {
                path: n.name.clone(),
                bytes: n.bytes.clone(),
            })
            .collect();
        self.0.check(canon, src, &files).map_err(fail)
    }
}

/// `data/global/excel/*.txt`: a byte-for-byte copy; the check is
/// byte equality (C-TABLE step 1). The step that compiles the native set
/// against each live `.bin` (C-TABLE step 2, `_bin-overrides.toml`) comes
/// with the native tables (`tables.rs`).
///
/// `sounds.txt` references the sound, speech and music files it names: the
/// sound archive's own listing names almost none of them, so without this
/// they are never converted.
pub struct Excel;

/// The table whose rows name sound files.
pub const SOUNDS_TXT: &str = "data/global/excel/sounds.txt";

impl Kind for Excel {
    fn name(&self) -> &'static str {
        "excel"
    }
    /// 2: the conversion follows the file names in `sounds.txt`, so a
    /// root converted at 1 has no base sounds and must be converted again.
    fn native_version(&self) -> u32 {
        2
    }
    fn phase(&self) -> u8 {
        0
    }
    fn claims(&self, canon: &str) -> bool {
        canon
            .strip_prefix("data/global/excel/")
            .is_some_and(|rest| !rest.contains('/') && rest.ends_with(".txt"))
    }
    fn has_references(&self) -> bool {
        true
    }
    fn references(&self, canon: &str, src: &[u8]) -> Vec<String> {
        if canon != SOUNDS_TXT {
            return Vec::new();
        }
        d2_data::txt::TxtTable::parse("sounds.txt", src)
            .and_then(|t| d2_data::sounds::sound_paths(&t))
            .unwrap_or_default()
    }
    fn write(&self, canon: &str, src: &[u8]) -> Result<Written, Failure> {
        Ok(copy(canon, src))
    }
    fn check(&self, canon: &str, src: &[u8], native: &[NativeData]) -> Result<(), Failure> {
        check_copy("C-TABLE", canon, src, native)
    }
}

/// `data/global/items/palette/*.dat` item colour maps: a byte-for-byte
/// copy; the check is byte equality (C-COPY).
pub struct ColorMap;

impl Kind for ColorMap {
    fn name(&self) -> &'static str {
        "colormap"
    }
    fn native_version(&self) -> u32 {
        1
    }
    fn phase(&self) -> u8 {
        1
    }
    fn claims(&self, canon: &str) -> bool {
        d2_native::source::is_colormap(canon)
    }
    fn write(&self, canon: &str, src: &[u8]) -> Result<Written, Failure> {
        Ok(copy(canon, src))
    }
    fn check(&self, canon: &str, src: &[u8], native: &[NativeData]) -> Result<(), Failure> {
        check_copy("C-COPY", canon, src, native)
    }
}

/// Monster palette shift files (a class's `palshift.dat` and the shared
/// `randtransforms.dat`): a byte-for-byte copy; the check is byte equality
/// (C-COPY).
pub struct PalShift;

impl Kind for PalShift {
    fn name(&self) -> &'static str {
        "palshift"
    }
    fn native_version(&self) -> u32 {
        1
    }
    fn phase(&self) -> u8 {
        1
    }
    fn claims(&self, canon: &str) -> bool {
        d2_native::source::is_palshift(canon)
    }
    fn write(&self, canon: &str, src: &[u8]) -> Result<Written, Failure> {
        Ok(copy(canon, src))
    }
    fn check(&self, canon: &str, src: &[u8], native: &[NativeData]) -> Result<(), Failure> {
        check_copy("C-COPY", canon, src, native)
    }
}

/// `data/local/Color.txt`, the green-blood switch: a byte-for-byte copy;
/// the check is byte equality (C-COPY).
pub struct ColorTxt;

impl Kind for ColorTxt {
    fn name(&self) -> &'static str {
        "colortxt"
    }
    fn native_version(&self) -> u32 {
        1
    }
    fn phase(&self) -> u8 {
        1
    }
    fn claims(&self, canon: &str) -> bool {
        canon == d2_native::source::COLOR_TXT
    }
    fn write(&self, canon: &str, src: &[u8]) -> Result<Written, Failure> {
        Ok(copy(canon, src))
    }
    fn check(&self, canon: &str, src: &[u8], native: &[NativeData]) -> Result<(), Failure> {
        check_copy("C-COPY", canon, src, native)
    }
}

/// The one native file of a verbatim kind: `canon` with the source bytes.
fn copy(canon: &str, src: &[u8]) -> Written {
    Written {
        files: vec![NativeData {
            name: canon.to_owned(),
            bytes: src.to_vec(),
        }],
        notes: Vec::new(),
    }
}

/// A verbatim kind's check: exactly one native file, named `canon`, with
/// the source bytes. Failures carry `check`'s name.
fn check_copy(
    check: &'static str,
    canon: &str,
    src: &[u8],
    native: &[NativeData],
) -> Result<(), Failure> {
    let [one] = native else {
        return Err(Failure::new(
            check,
            format!("{} native files", native.len()),
        ));
    };
    if one.name != canon {
        return Err(Failure::new(check, format!("native name {}", one.name)));
    }
    match src.iter().zip(&one.bytes).position(|(a, b)| a != b) {
        Some(at) => Err(Failure::new(
            check,
            format!("{canon}: first difference at byte {at}"),
        )),
        None if src.len() != one.bytes.len() => Err(Failure::new(
            check,
            format!("{canon}: length {} vs {}", one.bytes.len(), src.len()),
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_loaded_paths_are_canonical_and_claimed() {
        let kinds = builtin();
        assert_eq!(NEVER_LOADED.len(), 11);
        for (p, _) in NEVER_LOADED {
            assert_eq!(crate::names::canonical(p).as_deref(), Some(*p));
            assert!(
                kinds.iter().any(|k| k.claims(p)),
                "{p} is claimed by no kind"
            );
        }
        assert!(never_loaded("data/global/tiles/act1/court/court.dt1").is_some());
        assert!(never_loaded("data/local/font/latin/font8.tbl").is_none());
        assert!(never_loaded("data/global/sfx/cursor/curindx.wav").is_some());
        assert!(never_loaded("data/global/sfx/cursor/wavindx.wav").is_some());
        assert!(never_loaded("data/global/sfx/cursor/button.wav").is_none());
    }

    #[test]
    fn the_server_kinds_are_builtin_kinds() {
        let names: Vec<&str> = server().iter().map(|k| k.name()).collect();
        assert_eq!(names.len(), SERVER_KINDS.len(), "{names:?}");
        for k in SERVER_KINDS {
            assert!(names.contains(&k), "{k} is not a builtin kind");
        }
        assert!(!names.contains(&"dcc") && !names.contains(&"wav"));
    }

    #[test]
    fn sounds_txt_references_the_files_its_rows_name() {
        let kinds = builtin();
        let k = kinds.iter().find(|k| k.claims(SOUNDS_TXT)).unwrap();
        assert_eq!(k.name(), "excel");
        assert!(k.has_references());
        let mut lines: Vec<String> = (0..4660)
            .map(|i| match i {
                0 => "none\t0\t".to_owned(),
                1 => "cursor_pass\t1\tcursor\\pass.wav".to_owned(),
                3000 => "akara_hi\t3000\tnpc\\akara\\hi.wav".to_owned(),
                4657 => "town1\t4657\tact1\\town1.wav".to_owned(),
                _ => format!("r{i}\t{i}\t"),
            })
            .collect();
        lines.insert(0, "Sound\tIndex\tFileName".to_owned());
        let src = lines.join("\r\n") + "\r\n";
        let refs: Vec<String> = k
            .references(SOUNDS_TXT, src.as_bytes())
            .iter()
            .filter_map(|r| crate::names::canonical(r))
            .collect();
        assert_eq!(
            refs,
            [
                "data/global/sfx/cursor/pass.wav",
                "data/local/sfx/npc/akara/hi.wav",
                "data/global/music/act1/town1.wav",
            ]
        );
        // Other tables name no files; an unreadable table names none.
        assert!(k
            .references("data/global/excel/weapons.txt", src.as_bytes())
            .is_empty());
        assert!(k.references(SOUNDS_TXT, b"").is_empty());
    }

    #[test]
    fn colormap_is_a_verbatim_copy() {
        let kinds = builtin();
        let p = "data/global/items/palette/invgrey.dat";
        let claim: Vec<_> = kinds.iter().filter(|k| k.claims(p)).collect();
        assert_eq!(claim.len(), 1);
        let k = claim[0];
        assert_eq!(k.name(), "colormap");
        for other in [
            "data/global/palette/act1/pal.dat",
            "data/global/items/palette/sub/x.dat",
        ] {
            assert!(!k.claims(other), "{other}");
        }
        let src: Vec<u8> = (0..21 * 256).map(|i| (i % 251) as u8).collect();
        let w = k.write(p, &src).unwrap();
        assert_eq!(w.files.len(), 1);
        assert_eq!(w.files[0].name, p);
        k.check(p, &src, &w.files).unwrap();
        let mut bad = w.files.clone();
        bad[0].bytes[300] ^= 1;
        assert_eq!(k.check(p, &src, &bad).unwrap_err().check, "C-COPY");
    }

    #[test]
    fn palshift_is_a_verbatim_copy() {
        let kinds = builtin();
        for p in [
            "data/global/monsters/zm/cof/palshift.dat",
            "data/global/monsters/#b/cof/palshift.dat",
            "data/global/monsters/randtransforms.dat",
        ] {
            let claim: Vec<_> = kinds.iter().filter(|k| k.claims(p)).collect();
            assert_eq!(claim.len(), 1, "{p}");
            let k = claim[0];
            assert_eq!(k.name(), "palshift");
            let src: Vec<u8> = (0..8 * 256).map(|i| (i % 253) as u8).collect();
            let w = k.write(p, &src).unwrap();
            assert_eq!(w.files.len(), 1);
            assert_eq!(w.files[0].name, p);
            k.check(p, &src, &w.files).unwrap();
            let mut bad = w.files.clone();
            bad[0].bytes[300] ^= 1;
            assert_eq!(k.check(p, &src, &bad).unwrap_err().check, "C-COPY");
        }
        for other in [
            "data/global/monsters/palshift.dat",
            "data/global/monsters/zm/palshift.dat",
            "data/global/monsters/zm/cof/sub/palshift.dat",
            "data/global/monsters/zm/cof/randtransforms.dat",
        ] {
            assert!(kinds.iter().all(|k| !k.claims(other)), "{other}");
        }
    }

    #[test]
    fn color_txt_is_a_verbatim_copy() {
        let kinds = builtin();
        let p = "data/local/color.txt";
        let claim: Vec<_> = kinds.iter().filter(|k| k.claims(p)).collect();
        assert_eq!(claim.len(), 1);
        let k = claim[0];
        assert_eq!(k.name(), "colortxt");
        assert!(!k.claims("data/local/other.txt"));
        let w = k.write(p, b"1").unwrap();
        assert_eq!(w.files.len(), 1);
        assert_eq!(w.files[0].name, p);
        k.check(p, b"1", &w.files).unwrap();
        let mut bad = w.files.clone();
        bad[0].bytes[0] = b'0';
        assert_eq!(k.check(p, b"1", &bad).unwrap_err().check, "C-COPY");
    }
}
