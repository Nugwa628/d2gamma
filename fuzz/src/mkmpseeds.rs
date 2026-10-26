//! Synthetic seeds for the multiplayer targets, written by the production
//! item writer with the unit tests' item tables, never from game files.
//! Run from `fuzz/`: `cargo run --release --bin mkmpseeds`.
use d2_sim::bench_fixtures::{item_rec, item_tables, push_item};
use d2_sim::items::bitstream::{hflag, write_save, Isc, StreamItem};
use d2_sim::items::{flag, q};

fn main() {
    let mut t = item_tables();
    let mut isc = vec![Isc::default(); 360];
    isc[194].save_bits = 4;
    t.isc = isc;
    push_item(&mut t, item_rec(10, b"rin "));
    push_item(&mut t, item_rec(10, b"r01 "));
    let ring = |id: u32| StreamItem {
        flags: flag::IDENTIFIED,
        version: 101,
        code: *b"rin ",
        unit28: id,
        ilvl: 5,
        quality: q::NORMAL,
        file_index: -1,
        runeword: 0xFFFF,
        main: Some(Vec::new()),
        ..StreamItem::default()
    };
    let compact = StreamItem {
        flags: flag::IDENTIFIED | hflag::COMPACT,
        compact: true,
        version: 101,
        code: *b"r01 ",
        ..StreamItem::default()
    };
    let bytes = |it: &StreamItem| write_save(it, &t.isc).expect("item fits").0;
    let root = std::path::PathBuf::from("seeds/item_ids");
    std::fs::create_dir_all(&root).unwrap();
    for (name, items) in [
        ("ring", vec![ring(0x1111)]),
        ("no_id", vec![ring(0)]),
        ("compact", vec![compact.clone()]),
        (
            "mixed",
            vec![
                ring(0x1111),
                compact.clone(),
                ring(0),
                ring(0x2222),
                ring(0x3333),
            ],
        ),
    ] {
        let data: Vec<u8> = items.iter().flat_map(&bytes).collect();
        std::fs::write(root.join(name), data).unwrap();
    }
}
