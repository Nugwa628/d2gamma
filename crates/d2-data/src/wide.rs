//! Mod tables stored with wider records than D2Gamma's layout.
//!
//! A mod game's `.bin` may carry more columns than D2Gamma's record for
//! the same table: the D2Gamma columns first, at their usual offsets, then
//! the mod's own (a `runes` record with more property slots after the
//! seven of D2Gamma's). Under [`DataRules::Mod`] such a table loads: every
//! reader of the table sees D2Gamma's columns (the first `record_size`
//! bytes of each record), and the full records stay in
//! [`BinSet::wide`](crate::bin::BinSet::wide), where the mod game's own code
//! reads its extra columns. Under [`DataRules::Original`] a record size
//! that differs from D2Gamma's is an error, as before.

use crate::bin::{BinTable, DataRules, LoadError};

/// [`BinTable::parse`] under the data `rules`: the table at D2Gamma's
/// `record_size`, and under [`DataRules::Mod`], when the file's records
/// are wider, the full records as a second table (`record_size` of the
/// file's width). A file whose length is no whole number of records at
/// least `record_size` wide is the size mismatch [`BinTable::parse`]
/// reports.
pub fn parse_under(
    name: &str,
    source: &str,
    file: &str,
    data: &[u8],
    record_size: usize,
    rules: DataRules,
) -> Result<(BinTable, Option<BinTable>), LoadError> {
    let err = match BinTable::parse(name, source, file, data, record_size) {
        Ok(t) => return Ok((t, None)),
        Err(e) => e,
    };
    let LoadError::SizeMismatch { count, len, .. } = err else {
        return Err(err);
    };
    let (count, body) = (count as usize, len - 4);
    if rules != DataRules::Mod || count == 0 || body % count != 0 || body / count <= record_size {
        return Err(err);
    }
    let full = BinTable::parse(name, source, file, data, body / count)?;
    let records = full
        .iter()
        .flat_map(|r| &r[..record_size])
        .copied()
        .collect();
    let narrow = BinTable {
        name: full.name.clone(),
        source: full.source.clone(),
        count,
        record_size,
        records,
    };
    Ok((narrow, Some(full)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `count` records of `width` bytes, record `i` filled with byte `i`
    /// and its last byte `0xEE`.
    fn file(count: u32, width: usize) -> Vec<u8> {
        let mut d = count.to_le_bytes().to_vec();
        for i in 0..count as u8 {
            let mut r = vec![i; width];
            r[width - 1] = 0xEE;
            d.extend(r);
        }
        d
    }

    #[test]
    fn a_mod_tables_wider_records_keep_d2gammas_columns_and_the_full_rows() {
        let data = file(3, 12);
        let (t, full) = parse_under("runes", "m", "runes.bin", &data, 8, DataRules::Mod).unwrap();
        assert_eq!((t.count, t.record_size), (3, 8));
        assert_eq!(t.record(2), &[2; 8]);
        let full = full.expect("the full records");
        assert_eq!((full.count, full.record_size), (3, 12));
        assert_eq!(&full.record(1)[8..], &[1, 1, 1, 0xEE]);
        assert_eq!(full.name, "runes");
    }

    #[test]
    fn d2gammas_own_rules_refuse_a_wider_record() {
        let data = file(3, 12);
        assert!(matches!(
            parse_under("runes", "m", "runes.bin", &data, 8, DataRules::Original),
            Err(LoadError::SizeMismatch { .. })
        ));
        // The usual width loads the same under both, with no full rows.
        for rules in [DataRules::Original, DataRules::Mod] {
            let (t, full) = parse_under("runes", "m", "r", &file(3, 8), 8, rules).unwrap();
            assert_eq!(
                t,
                BinTable::parse("runes", "m", "r", &file(3, 8), 8).unwrap()
            );
            assert!(full.is_none());
        }
    }

    #[test]
    fn narrower_or_ragged_records_stay_a_size_mismatch() {
        let m = DataRules::Mod;
        // Narrower than D2Gamma's record.
        assert!(parse_under("t", "m", "t", &file(3, 6), 8, m).is_err());
        // Not a whole number of records.
        let mut ragged = file(3, 12);
        ragged.push(0);
        assert!(parse_under("t", "m", "t", &ragged, 8, m).is_err());
        // No records but bytes after the count.
        let mut empty = 0u32.to_le_bytes().to_vec();
        empty.extend([0; 8]);
        assert!(parse_under("t", "m", "t", &empty, 8, m).is_err());
        assert!(parse_under("t", "m", "t", &[1, 0], 8, m).is_err());
    }
}
