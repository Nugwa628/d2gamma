// D2RUST's own: a market listing's item tip.
//! A listing's item comes as its save-format record (contract
//! `Goods::Item`), not as the item message the game client sees in play.
//! The client reads the record with the save format's item reader
//! (`d2_proto::item_bits::save_entry_len`), which gives the same decoded
//! item the play tips are built from, and builds the game's own tip from
//! it ([`super::super::item_tip::ItemTips::tip_lines_of`]); the record's
//! socketed children are the tip's socket fillers. A record that does not
//! read (or a fake listing without one) shows the listing's own lines:
//! its name in its group's colour, the base, the requirement and the key
//! stats.
//!
//! The search box's names come from the same game data: the base items',
//! the unique and the set items' names ([`names`]).

use std::collections::BTreeMap;

use d2_server::adapters::item_bits::TablesLookup;

use crate::market::source::{Goods, Group, ItemView, ListingRow};
use crate::ui::item_tip::{ItemTips, TipLine};
use crate::ui::item_tip_build::TipCtx;

/// Text colours of the string tables' colour codes.
const WHITE: u16 = 0;
const BLUE: u16 = 3;
const GREY: u16 = 5;

/// The game's tip of a save-format item record; empty when it does not
/// read.
pub fn record_lines(tips: &ItemTips, record: &[u8]) -> Vec<TipLine> {
    if record.is_empty() {
        return Vec::new();
    }
    let lookup = TablesLookup(&tips.lookup);
    let Ok(entry) = d2_proto::item_bits::save_entry_len(record, &lookup) else {
        return Vec::new();
    };
    if entry.item.failed {
        return Vec::new();
    }
    let ctx = TipCtx {
        fillers: entry.children.iter().map(|c| c.item.clone()).collect(),
        ..TipCtx::default()
    };
    tips.tip_lines_of(&entry.item, &ctx)
}

/// The listing's tip: the game's from its record when there are tips and
/// the record reads, else its own lines.
/// The group and the view of an item record from the realm, as the
/// window lists it: the tip's first line is its name, the lines after
/// the base and the requirement its stats. `None` for a record that does
/// not read.
pub fn read_item(tips: &ItemTips, record: &[u8]) -> Option<(Group, ItemView)> {
    use d2_sim::items::flag;
    let lookup = TablesLookup(&tips.lookup);
    let entry = d2_proto::item_bits::save_entry_len(record, &lookup).ok()?;
    let b = &entry.item;
    if b.failed {
        return None;
    }
    let code = String::from_utf8_lossy(&b.code).trim_end().to_owned();
    let base = tips.base_name(&code).unwrap_or_else(|| code.clone());
    let lines: Vec<String> = record_lines(tips, record)
        .iter()
        .map(|l| String::from_utf16_lossy(&l.text))
        .filter(|l| !l.trim().is_empty())
        .collect();
    let name = lines.first().cloned().unwrap_or_else(|| base.clone());
    let mut level_req = 0;
    let mut stats = Vec::new();
    for l in lines.iter().skip(1) {
        if let Some(n) = l.strip_prefix("Required Level: ") {
            level_req = n.trim().parse().unwrap_or(0);
        } else if *l != base {
            stats.push(l.clone());
        }
    }
    let t = &tips.lookup;
    let item_type = t
        .find_code(b.code)
        .and_then(|i| t.itype_of(i))
        .map(|ty| String::from_utf8_lossy(&ty.code).trim_end().to_owned())
        .unwrap_or_default();
    let group = if b.flags & flag::RUNEWORD != 0 {
        Group::Runeword
    } else {
        super::sell::group_of_quality(b.quality)
    };
    let view = ItemView {
        name,
        base,
        code,
        item_type,
        level_req,
        sockets: b.sockets.unwrap_or(0),
        ethereal: b.flags & flag::ETHEREAL != 0,
        identified: b.flags & flag::IDENTIFIED != 0,
        stats,
    };
    Some((group, view))
}

pub fn listing_lines(row: &ListingRow, tips: Option<&ItemTips>) -> Vec<TipLine> {
    let game = match (tips, row.record()) {
        (Some(t), Some(r)) => record_lines(t, r),
        _ => Vec::new(),
    };
    if game.is_empty() {
        own_lines(row)
    } else {
        game
    }
}

/// The listing's own lines (no record, or none that reads).
pub fn own_lines(row: &ListingRow) -> Vec<TipLine> {
    let count = match &row.goods {
        Goods::Stack { count, .. } => *count,
        _ => 1,
    };
    view_lines(row.group, &row.item, count)
}

/// An item's own lines from the client's reading of it (`count` of a
/// stackable): offers' and deliveries' items without a record that reads.
pub fn view_lines(group: Group, v: &ItemView, count: u32) -> Vec<TipLine> {
    let mut out = Vec::new();
    if count > 1 {
        out.push(TipLine::new(format!("{} x{count}", v.name), group.color()));
    } else {
        out.push(TipLine::new(&v.name, group.color()));
    }
    if !v.base.is_empty() && v.base != v.name && group != Group::Stackable {
        out.push(TipLine::new(&v.base, GREY));
    }
    if v.level_req > 1 {
        out.push(TipLine::new(
            format!("Required Level: {}", v.level_req),
            WHITE,
        ));
    }
    for s in &v.stats {
        out.push(TipLine::new(s, BLUE));
    }
    if v.ethereal {
        out.push(TipLine::new("Ethereal", BLUE));
    }
    if v.sockets > 0 {
        out.push(TipLine::new(format!("Socketed ({})", v.sockets), BLUE));
    }
    if !v.identified {
        out.push(TipLine::new("Unidentified", 1));
    }
    out
}

/// The names the search box completes: every base item's, unique item's
/// and set item's name in the game data.
pub fn names(tips: &ItemTips) -> Vec<String> {
    let mut out: Vec<String> = tips
        .codes
        .values()
        .filter_map(|t| tips.strings.get_id(t.name_id))
        .map(String::from_utf16_lossy)
        .collect();
    for key in tips.unique.iter().chain(tips.set.iter()) {
        if let Some(s) = tips.strings.get(key) {
            out.push(String::from_utf16_lossy(s));
        }
    }
    // Colour codes and line breaks are not part of a name.
    out.retain(|n| !n.contains('\u{ff}') && !n.contains('\n'));
    out
}

/// The names of the base items `codes` (the realm's currencies), by code.
pub fn coin_names(tips: &ItemTips, codes: &[String]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for c in codes {
        let mut key = [b' '; 4];
        for (k, b) in key.iter_mut().zip(c.bytes()) {
            *k = b;
        }
        let name = tips
            .codes
            .get(&key)
            .and_then(|t| tips.strings.get_id(t.name_id))
            .map(String::from_utf16_lossy);
        if let Some(n) = name.filter(|n| !n.contains('\u{ff}')) {
            out.insert(c.clone(), n.replace('\n', " "));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::source::{ItemView, ListingMode};
    use crate::ui::item_tip::tests::tips;
    use d2_sim::items::bitstream::{self, Isc, Kind, StatEntry, StreamItem};

    /// The fixture's cap: identified, normal quality, one property (stat
    /// 1 described "+N to Life"), with its defense and durability.
    fn cap(mode: u32) -> StreamItem {
        StreamItem {
            flags: 0x10,
            version: 101,
            mode,
            code: *b"cap ",
            ilvl: 5,
            quality: 2,
            kind: Kind {
                armor: true,
                ..Kind::default()
            },
            main: Some(vec![StatEntry {
                stat: 1,
                param: 0,
                value: 20,
            }]),
            ..StreamItem::default()
        }
    }

    /// Save widths for the stats the cap writes.
    fn isc() -> Vec<Isc> {
        let mut v = vec![Isc::default(); 200];
        let w = |bits| Isc {
            save_bits: bits,
            ..Isc::default()
        };
        v[1] = w(8);
        v[31] = Isc {
            save_bits: 11,
            save_add: 10,
            ..Isc::default()
        };
        v[72] = w(8);
        v[73] = w(8);
        v[194] = w(4);
        v
    }

    #[test]
    fn record_tip_matches_game_tip() {
        let mut t = tips();
        let table = isc();
        std::sync::Arc::make_mut(&mut t.lookup).isc = table.clone();
        let (net, _) = bitstream::write(&cap(0), &table).unwrap();
        let (save, _) = bitstream::write_save(&cap(0), &table).unwrap();
        let game = t.lines(&net);
        assert!(!game.is_empty(), "the play tip reads");
        assert_eq!(record_lines(&t, &save), game);
        // A record that does not read gives nothing; the listing falls
        // back to its own lines.
        assert!(record_lines(&t, &[0xFF, 0x00]).is_empty());
        let row = ListingRow {
            listing: 1,
            group: Group::Unique,
            goods: Goods::Item {
                item_id: 1,
                record: save,
            },
            mode: ListingMode::Price,
            price: None,
            price_hash: [0; 32],
            seller: String::new(),
            seller_sales: 0,
            posted_ms: 0,
            expires_ms: 0,
            note: String::new(),
            offers: 0,
            mine: false,
            item: ItemView {
                name: "Ashen Crown".into(),
                ..ItemView::default()
            },
        };
        assert_eq!(listing_lines(&row, Some(&t)), game);
        let own = listing_lines(&row, None);
        assert_eq!(own[0], TipLine::new("Ashen Crown", 4));
    }

    /// The realm's records read as the window's rows (K1d): the tip's
    /// name, the base, the type, the flags and the stat lines.
    #[test]
    fn a_record_reads_as_its_view() {
        let mut t = tips();
        let table = isc();
        std::sync::Arc::make_mut(&mut t.lookup).isc = table.clone();
        let (save, _) = bitstream::write_save(&cap(0), &table).unwrap();
        let game: Vec<String> = record_lines(&t, &save)
            .iter()
            .map(|l| String::from_utf16_lossy(&l.text))
            .collect();
        let (group, v) = read_item(&t, &save).expect("the cap reads");
        assert_eq!(group, Group::Other);
        assert_eq!(v.code, "cap");
        assert_eq!(v.name, game[0]);
        assert_eq!(Some(v.base.clone()), t.base_name("cap"));
        assert!(v.identified && !v.ethereal);
        assert_eq!(v.sockets, 0);
        assert!(!v.stats.is_empty(), "the property line: {game:?}");
        assert!(!v.stats.contains(&v.name));
        let eth = StreamItem {
            flags: d2_sim::items::flag::IDENTIFIED | d2_sim::items::flag::ETHEREAL,
            ..cap(0)
        };
        let (save, _) = bitstream::write_save(&eth, &table).unwrap();
        assert!(read_item(&t, &save).expect("reads").1.ethereal);
        assert_eq!(read_item(&t, &[0xFF, 0x00]), None);
    }
}
