//! An item's save record read and patched without making the item: its
//! code, where it lies and (from the game's item table) its size, and a
//! new grid position written into the record in place.
//!
//! A mod game that keeps item records itself (a stash of its own, saved
//! in its own files) reads them with [`facts`] and keeps each record's
//! position in step with its own layout with [`set_position`]. Only the
//! header is touched, so every other bit of the record stays as it was.
//!
//! Header bits, least significant first: the marker (16), the flags (32),
//! the version (10), the mode (3); then for a record on the ground or in
//! a dropping item (modes 3 and 5) x and y in 16 bits each, else the body
//! slot (4), x (4), y (4) and the page plus one (3); then the code (32)
//! unless the record is an ear.

use super::bitstream::{hflag, SAVE_MARKER};

/// Where the mode field starts.
const MODE_AT: usize = 16 + 32 + 10;
/// Where a stored record's x field starts (after the mode and body slot).
const X_AT: usize = MODE_AT + 3 + 4;
/// Where a stored record's y field starts.
const Y_AT: usize = X_AT + 4;
/// Where a stored record's page field starts.
const PAGE_AT: usize = Y_AT + 4;
/// The largest x or y a stored record holds (4 bits).
pub const MAX_CELL: u8 = 15;
/// The largest page a stored record holds (3 bits hold the page plus one).
pub const MAX_PAGE: u8 = 6;

/// What a save record says about its item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordFacts {
    /// The item code, space padded (`*b"r01 "`); `*b"ear "` for an ear.
    pub code: [u8; 4],
    /// Width and height in cells, from the game's item table; `None` when
    /// the table has no such code.
    pub size: Option<(u8, u8)>,
    /// The item mode (0 stored in a grid, 1 equipped, 2 in the belt, 3 on
    /// the ground, 4 on the cursor, 5 dropping, 6 socketed).
    pub mode: u8,
    /// The grid position (stored items), the belt slot or the ground
    /// position.
    pub x: i32,
    pub y: i32,
    /// The inventory page (0 inventory, 3 cube, 4 stash); `None` outside a
    /// grid page.
    pub page: Option<u8>,
}

/// Why [`set_position`] left a record as it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RecordError {
    #[error("not an item save record")]
    NotARecord,
    #[error("the record is not stored in a grid (mode {0})")]
    NotStored(u8),
    #[error("cell ({0}, {1}) or page {2} past what a record holds")]
    OutOfRange(u8, u8, u8),
}

fn bits(buf: &[u8], at: usize, n: usize) -> Option<u32> {
    if at + n > buf.len() * 8 {
        return None;
    }
    Some((0..n).fold(0u32, |v, i| {
        let b = at + i;
        v | u32::from((buf[b / 8] >> (b % 8)) & 1) << i
    }))
}

fn put_bits(buf: &mut [u8], at: usize, n: usize, value: u32) {
    for i in 0..n {
        let b = at + i;
        let mask = 1u8 << (b % 8);
        if value >> i & 1 != 0 {
            buf[b / 8] |= mask;
        } else {
            buf[b / 8] &= !mask;
        }
    }
}

/// The facts of save record `record` (the start of an item entry), the
/// size looked up by `size_of(code)`. `None` when it is not a record.
pub fn facts(record: &[u8], size_of: impl Fn([u8; 4]) -> Option<(u8, u8)>) -> Option<RecordFacts> {
    if bits(record, 0, 16)? != SAVE_MARKER {
        return None;
    }
    let flags = bits(record, 16, 32)?;
    let mode = bits(record, MODE_AT, 3)? as u8;
    let (x, y, page, code_at) = if mode == 3 || mode == 5 {
        let x = bits(record, MODE_AT + 3, 16)? as i32;
        let y = bits(record, MODE_AT + 19, 16)? as i32;
        (x, y, None, MODE_AT + 35)
    } else {
        let x = bits(record, X_AT, 4)? as i32;
        let y = bits(record, Y_AT, 4)? as i32;
        let page = bits(record, PAGE_AT, 3)?.checked_sub(1).map(|p| p as u8);
        (x, y, page, PAGE_AT + 3)
    };
    let code = if flags & hflag::COMPACT != 0 && flags & hflag::EAR != 0 {
        *b"ear "
    } else {
        bits(record, code_at, 32)?.to_le_bytes()
    };
    Some(RecordFacts {
        code,
        size: size_of(code),
        mode,
        x,
        y,
        page: if mode == 0 { page } else { None },
    })
}

/// Writes grid position (`x`, `y`) on `page` into stored record `record`
/// in place; nothing else in it changes.
pub fn set_position(record: &mut [u8], x: u8, y: u8, page: u8) -> Result<(), RecordError> {
    let f = facts(record, |_| None).ok_or(RecordError::NotARecord)?;
    if f.mode != 0 {
        return Err(RecordError::NotStored(f.mode));
    }
    if x > MAX_CELL || y > MAX_CELL || page > MAX_PAGE {
        return Err(RecordError::OutOfRange(x, y, page));
    }
    put_bits(record, X_AT, 4, u32::from(x));
    put_bits(record, Y_AT, 4, u32::from(y));
    put_bits(record, PAGE_AT, 3, u32::from(page) + 1);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::bitstream::tests::isc_114d;
    use crate::items::bitstream::{write_save, StreamItem};

    fn stored(code: &[u8; 4], compact: bool, x: i32, y: i32, page: u8) -> Vec<u8> {
        let it = StreamItem {
            flags: 0x10,
            compact,
            version: 101,
            code: *code,
            x,
            y,
            page,
            unit28: 0xDEAD_BEEF,
            ilvl: 5,
            quality: 2,
            ..StreamItem::default()
        };
        write_save(&it, &isc_114d()).unwrap().0
    }

    fn size(code: [u8; 4]) -> Option<(u8, u8)> {
        match &code {
            b"r01 " => Some((1, 1)),
            b"cap " => Some((2, 2)),
            _ => None,
        }
    }

    #[test]
    fn reads_code_size_and_cell_of_a_compact_record() {
        let r = stored(b"r01 ", true, 7, 3, 4);
        let f = facts(&r, size).unwrap();
        assert_eq!(f.code, *b"r01 ");
        assert_eq!(f.size, Some((1, 1)));
        assert_eq!((f.mode, f.x, f.y, f.page), (0, 7, 3, Some(4)));
    }

    #[test]
    fn reads_a_full_record_and_an_unknown_code() {
        let r = stored(b"cap ", false, 2, 9, 0);
        let f = facts(&r, size).unwrap();
        assert_eq!((f.code, f.size), (*b"cap ", Some((2, 2))));
        assert_eq!((f.x, f.y, f.page), (2, 9, Some(0)));
        let r = stored(b"zzz ", true, 0, 0, 0);
        assert_eq!(facts(&r, size).unwrap().size, None);
    }

    #[test]
    fn refuses_what_is_not_a_record() {
        assert_eq!(facts(&[0u8; 20], size), None);
        assert_eq!(facts(&[0x4a, 0x4d], size), None);
        let mut short = [0x4a, 0x4d, 0, 0];
        assert_eq!(
            set_position(&mut short, 1, 1, 0),
            Err(RecordError::NotARecord)
        );
    }

    #[test]
    fn position_write_back_changes_only_the_cell_and_page() {
        let before = stored(b"cap ", false, 2, 9, 0);
        let mut r = before.clone();
        set_position(&mut r, 13, 13, 4).unwrap();
        let f = facts(&r, size).unwrap();
        assert_eq!((f.x, f.y, f.page), (13, 13, Some(4)));
        // The same record written at that cell by the writer.
        assert_eq!(r, stored(b"cap ", false, 13, 13, 4));
        // Back again: the original bytes.
        set_position(&mut r, 2, 9, 0).unwrap();
        assert_eq!(r, before);
    }

    #[test]
    fn position_write_back_refuses_other_modes_and_wide_cells() {
        let mut r = stored(b"r01 ", true, 0, 0, 0);
        assert_eq!(
            set_position(&mut r, 16, 0, 0),
            Err(RecordError::OutOfRange(16, 0, 0))
        );
        assert_eq!(
            set_position(&mut r, 0, 0, 7),
            Err(RecordError::OutOfRange(0, 0, 7))
        );
        let ground = StreamItem {
            compact: true,
            version: 101,
            code: *b"r01 ",
            mode: 3,
            x: 500,
            y: 600,
            ..StreamItem::default()
        };
        let mut g = write_save(&ground, &isc_114d()).unwrap().0;
        let f = facts(&g, size).unwrap();
        assert_eq!((f.mode, f.x, f.y, f.page), (3, 500, 600, None));
        assert_eq!(f.code, *b"r01 ");
        assert_eq!(
            set_position(&mut g, 1, 1, 0),
            Err(RecordError::NotStored(3))
        );
    }
}
