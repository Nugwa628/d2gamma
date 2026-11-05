//! A mod game's town levels on waypoint travel
//! ([`WaypointWorld::town_level`]): a travel to one of its towns arrives on
//! the town tile, a travel to an act town that is not one of them does not.

use super::tests::{data, fake, msg49, P, ROOM_A};
use super::*;
use crate::drlg::TownLevels;

/// The log of a travel to `level` (from a waypoint in level 1, or in level
/// 3 for a travel to level 1) in a game whose towns are `towns`.
fn travel(towns: Option<&[u32]>, level: u16) -> Vec<String> {
    let d = data();
    let mut f = fake();
    f.towns = towns.map(|t| TownLevels::new(t.iter().copied()));
    f.land = Some((Some(ROOM_A), 0x125A, 0x11B0, u32::from(level)));
    f.spawn = Some(ROOM_A);
    let from = if level == 1 { 3 } else { 1 };
    f.objects[0].1.level = Some(from);
    f.player.as_mut().unwrap().level = Some(from);
    for n in [0, 1] {
        f.records.get_mut(&P).unwrap().0[0].set(n).unwrap();
    }
    let mut arr = ArrivalList::default();
    d.take_or_close(&mut f, &mut arr, P, &msg49(0x0b, level))
        .unwrap();
    f.log
}

#[test]
fn a_travel_to_a_mod_town_arrives_on_the_town_tile() {
    let mod_towns: &[u32] = &[3];
    assert_eq!(travel(Some(mod_towns), 3)[2], "warp 3 13");
    assert_eq!(travel(Some(mod_towns), 1)[2], "warp 1 0");
}

/// D2RUST's towns: level 1 is a town, level 3 is not.
#[test]
fn original_towns_are_unchanged() {
    assert_eq!(travel(None, 3)[2], "warp 3 0");
    assert_eq!(travel(None, 1)[2], "warp 1 13");
    assert_eq!(travel_tile(3, false), tile_code(3));
    assert_eq!(travel_tile(1, true), tile_code(1));
}

#[test]
fn configured_town_waypoint_initializes_as_an_open_town_object() {
    let d = data();
    let mut f = fake();
    f.towns = Some(TownLevels::new([3]));
    let (object, mut facts) = f.objects[0];
    facts.level = Some(3);
    facts.mode = 0;
    d.init_object(&mut f, &mut ArrivalList::default(), object, &facts);
    assert!(
        f.log
            .iter()
            .any(|l| l.contains("mode") && l.ends_with(" 2")),
        "{:?}",
        f.log
    );
}

#[test]
fn configured_waypoint_load_preserves_bits_without_granting_bit_zero() {
    let mut bytes = [0; 16];
    bytes[..2].copy_from_slice(&MAGIC.to_le_bytes());
    bytes[15] = 0x80;
    let r = WaypointRecord::load_mod_copy(&bytes).unwrap();
    assert!(r.test(111).unwrap());
    assert!(!r.test(0).unwrap());
    assert!(WaypointRecord::load_copy(&bytes).unwrap().test(0).unwrap());
    bytes[..2].copy_from_slice(&0x101u16.to_le_bytes());
    assert_eq!(WaypointRecord::load_mod_copy(&bytes).unwrap().0, [0; 16]);
    bytes[0] = 3;
    assert!(WaypointRecord::load_mod_copy(&bytes).is_err());
}
