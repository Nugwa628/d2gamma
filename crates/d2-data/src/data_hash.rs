//! The game-data hash: one SHA-256 over the data a game server runs on.
//!
//! Two programs of the same version run a game the same way only on the
//! same data. The hash is what they compare: the main server and a game
//! server at the link's version match, and a journal's `Start` record for
//! the replay audit (`docs/plans/server/multiplayer-contracts.md` 11.3 and
//! 11.11).
//!
//! It covers the data as loaded, not the files it came from, so an install
//! read from its archives and the same install converted to a native folder
//! hash the same:
//!
//! - from the [`BinSet`]: whether the expansion is loaded, the data rules,
//!   every runtime table's name, record size and record bytes in load order,
//!   `hitclass`, the formula code buffers, and a mod table's wider records
//!   when it has any;
//! - `world`: everything else the game reads, passed by the caller in a
//!   fixed order (the game server passes `AnimData.d2`, `ExpField.D2` and
//!   every parsed DS1 and DT1 the level types read).
//!
//! It leaves out what a game never reads: the archive names the files came
//! from, the string tables, the runtime sound `.txt` tables and the code
//! buffer reports. Data the game computes from these inputs (the fix-ups,
//! the table views) is not hashed: the program's version covers it.
//!
//! `world` goes in through [`Hash`] on a [`DataHasher`], which writes every
//! integer little-endian at a fixed width (`usize` as `u64`). The standard
//! library passes a slice of integers as its bytes in memory, so the hash is
//! that of a little-endian host; every target D2RUST builds for is one.

use std::hash::{Hash, Hasher};

use sha2::{Digest, Sha256};

use crate::bin::{BinSet, BinTable};

/// The first bytes hashed: names the hash and its layout. A change to
/// what is hashed or how gets a new tag.
pub const TAG: &[u8] = b"d2rust game data 1";

/// A [`Hasher`] that feeds SHA-256 with a fixed encoding: every integer
/// little-endian at its own width, `usize` and `isize` as 64 bits.
#[derive(Clone, Debug, Default)]
pub struct DataHasher(Sha256);

impl DataHasher {
    /// A hasher that has seen nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// The SHA-256 of everything written.
    pub fn digest(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}

impl Hasher for DataHasher {
    /// The first 8 bytes of the digest so far, little-endian. Use
    /// [`DataHasher::digest`] for the hash itself.
    fn finish(&self) -> u64 {
        let d = self.0.clone().finalize();
        u64::from_le_bytes([d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]])
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    fn write_u8(&mut self, i: u8) {
        self.write(&[i]);
    }
    fn write_u16(&mut self, i: u16) {
        self.write(&i.to_le_bytes());
    }
    fn write_u32(&mut self, i: u32) {
        self.write(&i.to_le_bytes());
    }
    fn write_u64(&mut self, i: u64) {
        self.write(&i.to_le_bytes());
    }
    fn write_u128(&mut self, i: u128) {
        self.write(&i.to_le_bytes());
    }
    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }
    fn write_i8(&mut self, i: i8) {
        self.write_u8(i as u8);
    }
    fn write_i16(&mut self, i: i16) {
        self.write_u16(i as u16);
    }
    fn write_i32(&mut self, i: i32) {
        self.write_u32(i as u32);
    }
    fn write_i64(&mut self, i: i64) {
        self.write_u64(i as u64);
    }
    fn write_i128(&mut self, i: i128) {
        self.write_u128(i as u128);
    }
    fn write_isize(&mut self, i: isize) {
        self.write_u64(i as u64);
    }
}

/// The game-data hash of the loaded `tables` and the rest of the data the
/// game reads, `world` (see the module docs).
pub fn game_data_hash(tables: &BinSet, world: &(impl Hash + ?Sized)) -> [u8; 32] {
    let mut h = DataHasher::new();
    h.write(TAG);
    tables.lod.hash(&mut h);
    tables.rules.hash(&mut h);
    tables.tables.len().hash(&mut h);
    for t in &tables.tables {
        table(&mut h, t);
    }
    table(&mut h, &tables.hitclass);
    tables.code.len().hash(&mut h);
    for (buffer, file) in &tables.code {
        buffer.name().hash(&mut h);
        file.bytes.hash(&mut h);
    }
    // A mod game's wider records (`crate::wide`): hashed only when there
    // are any, so a set without them hashes as it always has.
    if !tables.wide.is_empty() {
        tables.wide.len().hash(&mut h);
        for t in tables.wide.values() {
            table(&mut h, t);
        }
    }
    world.hash(&mut h);
    h.digest()
}

fn table(h: &mut DataHasher, t: &BinTable) {
    t.name.hash(h);
    t.record_size.hash(h);
    t.count.hash(h);
    t.records.hash(h);
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::bin::{CodeFile, DataRules};
    use crate::schema::CalcBuffer;
    use crate::strings::StringTables;
    use crate::txt::TxtTable;

    fn bin(name: &str, record_size: usize, records: &[u8]) -> BinTable {
        BinTable {
            name: name.into(),
            source: "d2data.mpq".into(),
            count: records.len() / record_size,
            record_size,
            records: records.to_vec(),
        }
    }

    fn txt() -> TxtTable {
        TxtTable {
            header: vec![b"Sound".to_vec()],
            records: Vec::new(),
            removed_lines: Vec::new(),
        }
    }

    /// A small set: three tables, `hitclass` and one code buffer.
    fn set() -> BinSet {
        let mut code = BTreeMap::new();
        code.insert(
            CalcBuffer::SkillsCode,
            CodeFile {
                buffer: CalcBuffer::SkillsCode,
                source: "patch_d2.mpq".into(),
                bytes: vec![0x01, 0x02, 0x03],
            },
        );
        BinSet {
            lod: true,
            rules: DataRules::Original,
            strings: StringTables {
                base: None,
                patch: None,
                expansion: None,
                layer: Default::default(),
            },
            wide: Default::default(),
            tables: vec![
                bin("armor", 4, &[1, 2, 3, 4, 5, 6, 7, 8]),
                bin("levels", 2, &[9, 10, 11, 12]),
                bin("pettype", 1, &[13]),
            ],
            code,
            code_reports: BTreeMap::new(),
            hitclass: bin("hitclass", 2, &[14, 15]),
            sounds: txt(),
            soundenviron: txt(),
        }
    }

    /// The rest of the game's data, as a caller passes it.
    type World = (Vec<u32>, BTreeMap<Vec<u8>, Vec<u32>>);

    fn world() -> World {
        let mut files = BTreeMap::new();
        files.insert(b"act1\\town.ds1".to_vec(), vec![1, 2, 3]);
        (vec![7, 8], files)
    }

    fn hex(h: [u8; 32]) -> String {
        h.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn integers_go_in_little_endian_at_a_fixed_width() {
        let d = |f: &dyn Fn(&mut DataHasher)| {
            let mut h = DataHasher::new();
            f(&mut h);
            h.digest()
        };
        assert_eq!(
            d(&|h| h.write_usize(0x0102)),
            d(&|h| h.write(&[2, 1, 0, 0, 0, 0, 0, 0]))
        );
        assert_eq!(d(&|h| h.write_isize(-1)), d(&|h| h.write(&[0xFF; 8])));
        assert_eq!(
            d(&|h| 0x0102_0304u32.hash(h)),
            d(&|h| h.write(&[4, 3, 2, 1]))
        );
        assert_eq!(
            d(&|h| u16::hash_slice(&[0x0102, 0x0304], h)),
            d(&|h| h.write(&[2, 1, 4, 3]))
        );
        assert_eq!(
            d(&|h| vec![0x0102u16].hash(h)),
            d(&|h| h.write(&[1, 0, 0, 0, 0, 0, 0, 0, 2, 1]))
        );
    }

    /// The bytes [`game_data_hash`] hashes for [`set`] and [`world`],
    /// written out by hand.
    fn layout() -> Vec<u8> {
        let mut b = TAG.to_vec();
        let n = |b: &mut Vec<u8>, n: u64| b.extend(n.to_le_bytes());
        let bytes = |b: &mut Vec<u8>, v: &[u8]| {
            n(b, v.len() as u64);
            b.extend(v);
        };
        let name = |b: &mut Vec<u8>, s: &str| {
            b.extend(s.as_bytes());
            b.push(0xFF);
        };
        let table = |b: &mut Vec<u8>, t: &str, size: u64, records: &[u8]| {
            name(b, t);
            n(b, size);
            n(b, records.len() as u64 / size);
            bytes(b, records);
        };
        // lod, then the rules' variant index.
        b.push(1);
        n(&mut b, 0);
        n(&mut b, 3);
        table(&mut b, "armor", 4, &[1, 2, 3, 4, 5, 6, 7, 8]);
        table(&mut b, "levels", 2, &[9, 10, 11, 12]);
        table(&mut b, "pettype", 1, &[13]);
        table(&mut b, "hitclass", 2, &[14, 15]);
        n(&mut b, 1);
        name(&mut b, "skillscode");
        bytes(&mut b, &[1, 2, 3]);
        // The world: the u32 list, then the map of one file.
        n(&mut b, 2);
        b.extend([7, 0, 0, 0, 8, 0, 0, 0]);
        n(&mut b, 1);
        bytes(&mut b, b"act1\\town.ds1");
        n(&mut b, 3);
        b.extend([1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0]);
        b
    }

    /// The same data hashes to this value in every run, on every build
    /// and host: a change here changes every journal's and link's hash.
    #[test]
    fn the_hash_of_a_set_is_pinned() {
        let h = game_data_hash(&set(), &world());
        assert_eq!(h, game_data_hash(&set().clone(), &world()));
        assert_eq!(h, <[u8; 32]>::from(Sha256::digest(layout())));
        assert_eq!(
            hex(h),
            "ca327e4ec5e6c13e0f640bd388a9051ef6ec64a600f048c1dcfd8f619ae076c3"
        );
    }

    #[test]
    fn one_changed_table_byte_changes_it() {
        let base = game_data_hash(&set(), &world());
        let mut seen = vec![base];
        for t in 0..set().tables.len() {
            for i in 0..set().tables[t].records.len() {
                let mut s = set();
                s.tables[t].records[i] ^= 1;
                let h = game_data_hash(&s, &world());
                assert!(!seen.contains(&h), "table {t} byte {i}");
                seen.push(h);
            }
        }
        let mut s = set();
        s.hitclass.records[1] ^= 0x80;
        assert_ne!(game_data_hash(&s, &world()), base);
        let mut s = set();
        s.code.get_mut(&CalcBuffer::SkillsCode).unwrap().bytes[2] = 0;
        assert_ne!(game_data_hash(&s, &world()), base);
    }

    #[test]
    fn the_layout_and_the_switches_change_it() {
        let base = game_data_hash(&set(), &world());
        let changed = |f: &dyn Fn(&mut BinSet)| {
            let mut s = set();
            f(&mut s);
            game_data_hash(&s, &world())
        };
        // A byte moved from one table to the next.
        assert_ne!(
            changed(&|s| {
                let b = s.tables[0].records.pop().unwrap();
                s.tables[0].count = 1;
                s.tables[0].records.truncate(4);
                s.tables[1].records.insert(0, b);
            }),
            base
        );
        // The same bytes cut in other records.
        assert_ne!(
            changed(&|s| {
                s.tables[1].record_size = 4;
                s.tables[1].count = 1;
            }),
            base
        );
        assert_ne!(changed(&|s| s.tables.swap(1, 2)), base);
        assert_ne!(changed(&|s| s.tables[2].name = "states".into()), base);
        assert_ne!(changed(&|s| s.lod = false), base);
        assert_ne!(changed(&|s| s.rules = DataRules::Mod), base);
        assert_ne!(changed(&|s| s.code.clear()), base);
        // A mod table's wider records, and each of their bytes.
        let widen = |s: &mut BinSet| {
            s.wide
                .insert("runes".into(), bin("runes", 4, &[1, 2, 3, 4]));
        };
        let wide = changed(&widen);
        assert_ne!(wide, base);
        assert_ne!(
            changed(&|s| {
                widen(s);
                s.wide.get_mut("runes").unwrap().records[3] = 9;
            }),
            wide
        );
        // An empty map hashes as a set that never had one.
        assert_eq!(changed(&|s| s.wide.clear()), base);
    }

    #[test]
    fn the_world_data_changes_it() {
        let base = game_data_hash(&set(), &world());
        let mut w = world();
        w.1.get_mut(&b"act1\\town.ds1"[..]).unwrap()[2] = 4;
        assert_ne!(game_data_hash(&set(), &w), base);
        let mut w = world();
        let cells = w.1.remove(&b"act1\\town.ds1"[..]).unwrap();
        w.1.insert(b"act1\\town2.ds1".to_vec(), cells);
        assert_ne!(game_data_hash(&set(), &w), base);
        assert_ne!(game_data_hash(&set(), &()), base);
    }

    #[test]
    fn what_the_game_does_not_read_leaves_it() {
        let base = game_data_hash(&set(), &world());
        let mut s = set();
        for t in s.tables.iter_mut().chain([&mut s.hitclass]) {
            t.source = "native".into();
        }
        for f in s.code.values_mut() {
            f.source = "native".into();
        }
        s.sounds.header.push(b"Volume".to_vec());
        s.soundenviron.removed_lines.push(3);
        assert_eq!(game_data_hash(&s, &world()), base);
    }
}
