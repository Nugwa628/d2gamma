//! Item legality, affix and property ranges, duplicate item IDs (with the
//! registry hook), kept item IDs.

use d2_sim::items::bitstream::Kind;

use super::*;
use crate::ItemRegistry;

fn accurate_ring(id: u32, tohit: i32) -> StreamItem {
    ring(id, 0, SUFFIX_ACCURACY, &[(TOHIT, tohit)])
}

#[test]
fn legal_items_pass() {
    let after = with_items(
        character(),
        &[
            accurate_ring(1, 7),
            ring(2, PREFIX_FINE, SUFFIX_ACCURACY, &[(TOHIT, 30)]),
        ],
    );
    assert_clean(&run(&character(), &after));
}

#[test]
fn item_of_unknown_code_fails() {
    let mut item = accurate_ring(1, 7);
    item.code = *b"zzz ";
    let after = with_items(character(), &[item]);
    let v = run(&character(), &after);
    assert!(!v.accepted());
    assert!(failed(&v, "item-legality"));
}

#[test]
fn affix_out_of_range_fails() {
    // "Of accuracy" gives to-hit 5–10.
    for ok in [5, 10] {
        let after = with_items(character(), &[accurate_ring(1, ok)]);
        assert!(!failed(&run(&character(), &after), "item-ranges"), "{ok}");
    }
    for bad in [4, 11, 500] {
        let after = with_items(character(), &[accurate_ring(1, bad)]);
        let v = run(&character(), &after);
        assert!(failed(&v, "item-ranges"), "{bad}");
        assert!(!failed(&v, "item-legality"), "{bad}");
    }
    // Two affixes on one stat: 25–40.
    let both = |v| ring(1, PREFIX_FINE, SUFFIX_ACCURACY, &[(TOHIT, v)]);
    let after = with_items(character(), &[both(40)]);
    assert!(!failed(&run(&character(), &after), "item-ranges"));
    let after = with_items(character(), &[both(41)]);
    assert!(failed(&run(&character(), &after), "item-ranges"));
}

/// A magic axe "sharp of maiming" with `main` as its stats.
fn sharp_axe(main: &[(u16, i32)]) -> StreamItem {
    StreamItem {
        code: *b"axe ",
        kind: Kind {
            weapon: true,
            ..Kind::default()
        },
        ..ring(1, PREFIX_SHARP, SUFFIX_MAIMING, main)
    }
}

#[test]
fn small_enhanced_damage_mixed_with_max_damage_passes() {
    // On the axe's maximum damage of 8, enhanced damage below 13% adds less
    // than 1, and the function writes 1 maximum damage instead: the low end
    // of its roll gives more maximum damage than the high end. With "of
    // maiming" (3–4) the axe has 3–5 maximum damage, though every roll at
    // its low end gives 4 (3 + 1) and every roll at its high end 4 too.
    let ok: [&[(u16, i32)]; 4] = [
        // Enhanced damage 10–12% with maiming 3 and 4.
        &[(MAXDAMAGE, 4)],
        &[(MAXDAMAGE, 5)],
        // Enhanced damage 13–20% with maiming 3 and 4.
        &[
            (MAXDAMAGE_PERCENT, 13),
            (MINDAMAGE_PERCENT, 13),
            (MAXDAMAGE, 3),
        ],
        &[
            (MAXDAMAGE_PERCENT, 20),
            (MINDAMAGE_PERCENT, 20),
            (MAXDAMAGE, 4),
        ],
    ];
    for main in ok {
        let after = with_items(character(), &[sharp_axe(main)]);
        assert_clean(&run(&character(), &after));
    }
    for bad in [
        [(MAXDAMAGE, 2)],
        [(MAXDAMAGE, 6)],
        [(MAXDAMAGE_PERCENT, 21)],
    ] {
        let after = with_items(character(), &[sharp_axe(&bad)]);
        assert!(failed(&run(&character(), &after), "item-ranges"), "{bad:?}");
    }
}

#[test]
fn stat_no_source_gives_fails() {
    let after = with_items(
        character(),
        &[ring(1, 0, SUFFIX_ACCURACY, &[(TOHIT, 7), (31, 3)])],
    );
    assert!(failed(&run(&character(), &after), "item-ranges"));
}

#[test]
fn affix_above_item_level_fails() {
    // "Masterly" needs affix level 50; the ring is item level 30.
    let item = ring(1, PREFIX_MASTERLY, 0, &[(TOHIT, 45)]);
    let after = with_items(character(), std::slice::from_ref(&item));
    let v = run(&character(), &after);
    assert!(failed(&v, "item-legality"));
    let high = StreamItem { ilvl: 60, ..item };
    let after = with_items(character(), &[high]);
    assert!(!failed(&run(&character(), &after), "item-legality"));
}

#[test]
fn affix_of_wrong_part_fails() {
    // A prefix id in the suffix slot.
    let after = with_items(character(), &[ring(1, 0, PREFIX_FINE, &[(TOHIT, 25)])]);
    assert!(failed(&run(&character(), &after), "item-legality"));
}

#[test]
fn magic_item_without_affix_fails() {
    let after = with_items(character(), &[ring(1, 0, 0, &[])]);
    assert!(failed(&run(&character(), &after), "item-legality"));
}

#[test]
fn unique_rules() {
    let unique = |ilvl, file_index, v| StreamItem {
        quality: q::UNIQUE,
        file_index,
        ilvl,
        prefix: [0; 3],
        suffix: [0; 3],
        ..ring(1, 0, 0, &[(TOHIT, v)])
    };
    let after = with_items(character(), &[unique(30, UNIQUE_RING, 55)]);
    assert_clean(&run(&character(), &after));
    // Out of the unique's range.
    let after = with_items(character(), &[unique(30, UNIQUE_RING, 61)]);
    assert!(failed(&run(&character(), &after), "item-ranges"));
    // Below the unique's item level; a row that does not exist.
    let after = with_items(character(), &[unique(5, UNIQUE_RING, 55)]);
    assert!(failed(&run(&character(), &after), "item-legality"));
    let after = with_items(character(), &[unique(30, 7, 55)]);
    assert!(failed(&run(&character(), &after), "item-legality"));
}

/// Bit offset of a full record's 7-bit item level: marker 16, flags 32,
/// version 10, mode 3, body 4, x 4, y 4, page 3, code 32, filled 3,
/// seed 32.
const ILVL_BIT: usize = 143;

#[test]
fn item_level_out_of_bounds_fails() {
    // The writer clamps to 1–99; a record written by other code may not.
    let mut after = with_items(character(), &[accurate_ring(1, 7)]);
    let bytes = &mut after.body.as_mut().expect("a body").items[0].bytes;
    for (k, bit) in (0..7).map(|k| (k, 120u8 >> k & 1)) {
        let at = ILVL_BIT + k;
        bytes[at / 8] = bytes[at / 8] & !(1 << (at % 8)) | bit << (at % 8);
    }
    let v = run(&character(), &after);
    assert!(failed(&v, "item-legality"), "{v:?}");
    assert!(!failed(
        &run(
            &character(),
            &with_items(character(), &[accurate_ring(1, 7)])
        ),
        "item-legality"
    ));
}

#[test]
fn duplicate_item_id_fails() {
    let after = with_items(character(), &[accurate_ring(9, 7), accurate_ring(9, 8)]);
    let v = run(&character(), &after);
    assert!(!v.accepted());
    assert!(failed(&v, "item-duplicates"));
    let after = with_items(character(), &[accurate_ring(9, 7), accurate_ring(10, 8)]);
    assert!(!failed(&run(&character(), &after), "item-duplicates"));
}

struct Registry;

impl ItemRegistry for Registry {
    fn other_holder(&self, character: &[u8], id: u32) -> Option<String> {
        (id == 77 && character != b"Holder").then(|| "character Holder".to_owned())
    }
}

#[test]
fn item_id_held_elsewhere_fails() {
    let info = SaveInfo {
        registry: Some(&Registry),
        ..SaveInfo::default()
    };
    let after = with_items(character(), &[accurate_ring(77, 7)]);
    assert!(failed(
        &list().run(&character(), &after, info),
        "item-duplicates"
    ));
    let after = with_items(character(), &[accurate_ring(78, 7)]);
    assert!(!failed(
        &list().run(&character(), &after, info),
        "item-duplicates"
    ));
    // Without a registry the ID is not looked up.
    let after = with_items(character(), &[accurate_ring(77, 7)]);
    assert!(!failed(&run(&character(), &after), "item-duplicates"));
}

#[test]
fn kept_item_id_changed_into_another_item_fails() {
    let before = with_items(character(), &[accurate_ring(5, 7)]);
    // The same item (a new roll of nothing: the same fields) passes.
    assert!(!failed(&run(&before, &before), "item-ids-kept"));
    let after = with_items(
        character(),
        &[ring(5, PREFIX_FINE, SUFFIX_ACCURACY, &[(TOHIT, 30)])],
    );
    assert!(failed(&run(&before, &after), "item-ids-kept"));
    // Dropped items and new IDs are not this rule's business.
    let after = with_items(character(), &[accurate_ring(6, 7)]);
    assert!(!failed(&run(&before, &after), "item-ids-kept"));
}
