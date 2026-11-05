//! Independently implemented server and client item codecs agree on mod ids.
use d2_proto::item_bits::{self, CodeFacts, IscSave, ItemLookup};
use d2_sim::items::bitstream::{self, Isc, IscTable, StatEntry, StreamItem};

struct Tables;
impl IscTable for Tables {
    fn wide_stats(&self) -> bool {
        true
    }
    fn isc(&self, _: u16) -> Isc {
        Isc {
            save_bits: 32,
            save_param_bits: 16,
            ..Isc::default()
        }
    }
}
impl ItemLookup for Tables {
    fn wide_stats(&self) -> bool {
        true
    }
    fn code(&self, code: [u8; 4]) -> Option<CodeFacts> {
        (code == *b"tst ").then_some(CodeFacts::default())
    }
    fn isc(&self, _: u16) -> Option<IscSave> {
        Some(IscSave {
            save_bits: 32,
            save_param_bits: 16,
            save_add: 0,
        })
    }
}

#[test]
fn wide_ids_layers_and_signed_values_survive_network_and_save_records() {
    let stats = vec![
        StatEntry {
            stat: 511,
            param: u16::MAX,
            value: i32::MIN,
        },
        StatEntry {
            stat: 32768,
            param: 0,
            value: -1,
        },
        StatEntry {
            stat: 65534,
            param: 7,
            value: i32::MAX,
        },
    ];
    let item = StreamItem {
        flags: 0x10,
        version: 101,
        code: *b"tst ",
        quality: 2,
        ilvl: 1,
        runeword: u16::MAX,
        main: Some(stats.clone()),
        ..StreamItem::default()
    };
    let network = bitstream::write(&item, &Tables).unwrap().0;
    let save = bitstream::write_save(&item, &Tables).unwrap().0;
    let client_network = item_bits::decode(&network, &Tables).unwrap();
    let client_save = item_bits::save_entry_len(&save, &Tables).unwrap();
    assert_eq!(client_save.len, save.len());
    for decoded in [client_network, client_save.item] {
        let actual = decoded.lists[0].as_ref().unwrap();
        assert_eq!(actual.len(), stats.len());
        for (got, expected) in actual.iter().zip(&stats) {
            assert_eq!(
                (got.stat, got.param, got.value() as i32),
                (expected.stat, u32::from(expected.param), expected.value)
            );
        }
    }
}
