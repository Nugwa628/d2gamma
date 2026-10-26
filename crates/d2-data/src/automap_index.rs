//! `automap` tables whose `LevelName` cells hold level type numbers.
//!
//! D2Gamma's `automap.bin` keeps `LevelName` as text: one of
//! [`AUTOMAP_LEVEL_NAMES`](crate::bin::AUTOMAP_LEVEL_NAMES), or under
//! [`DataRules::Mod`] a decimal level type number
//! ([`automap_level_number`](crate::bin::automap_level_number)). A mod
//! game's own compiled table may instead hold the level type itself in
//! the cell, as a little-endian integer (the form a table compiler that
//! resolves the name at compile time writes): `01 00 00 00 …` for level
//! type 1.
//!
//! The two forms can't be told apart cell by cell (level type 51 is the
//! byte of the text `"3"`), so the form belongs to the whole table: under
//! [`DataRules::Mod`] a table is in the numbered form when any of its
//! `LevelName` cells starts with a control byte (level types 1 to 31),
//! which no text name does. Every cell of such a table must then be a
//! number from 0 to [`AUTOMAP_LEVEL_MAX`] with nothing after it.

use crate::bin::{u32_at, BinTable, DataRules, AUTOMAP_LEVEL_MAX};

/// Width of the `LevelName` cell at the start of an `automap` record.
pub const LEVEL_CELL: usize = 0x10;

/// True when `t`'s `LevelName` cells hold level type numbers (see the
/// module docs); always false under [`DataRules::Original`].
pub fn numbered(t: &BinTable, rules: DataRules) -> bool {
    rules == DataRules::Mod && t.iter().any(|r| matches!(r[0], 0x01..=0x1F))
}

/// The level type a numbered `LevelName` cell (the first
/// [`LEVEL_CELL`] bytes of `record`) holds: 0 to [`AUTOMAP_LEVEL_MAX`]
/// in its first four bytes, the rest zero; `None` otherwise.
pub fn level(record: &[u8]) -> Option<usize> {
    let cell = record.get(..LEVEL_CELL)?;
    let n = u32_at(cell, 0) as usize;
    (n <= AUTOMAP_LEVEL_MAX && cell[4..].iter().all(|&b| b == 0)).then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(levels: &[&[u8]]) -> BinTable {
        let mut records = Vec::new();
        for l in levels {
            let mut r = vec![0u8; 0x2C];
            r[..l.len()].copy_from_slice(l);
            r[0x10..0x12].copy_from_slice(b"fl");
            records.extend(r);
        }
        BinTable {
            name: "automap".into(),
            source: "m".into(),
            count: levels.len(),
            record_size: 0x2C,
            records,
        }
    }

    #[test]
    fn a_table_with_a_control_byte_cell_is_numbered_under_mod_rules() {
        let t = table(&[&[1], &[0x33], &[0x2C], &[]]);
        assert!(numbered(&t, DataRules::Mod));
        assert!(!numbered(&t, DataRules::Original));
        let got: Vec<_> = t.iter().map(level).collect();
        // 0x33 is level type 51 here, not the text "3".
        assert_eq!(got, [Some(1), Some(51), Some(44), Some(0)]);
    }

    #[test]
    fn text_tables_are_never_numbered() {
        let t = table(&[b"1 Town", b"3", b"5 Lava", b"None"]);
        assert!(!numbered(&t, DataRules::Mod));
    }

    #[test]
    fn a_numbered_cell_holds_nothing_after_its_number() {
        assert_eq!(
            level(&[0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            Some(255)
        );
        assert_eq!(
            level(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            None
        );
        let mut trailing = [0u8; LEVEL_CELL];
        trailing[0] = 2;
        trailing[5] = b'x';
        assert_eq!(level(&trailing), None);
        assert_eq!(level(&[1, 0]), None);
    }
}
