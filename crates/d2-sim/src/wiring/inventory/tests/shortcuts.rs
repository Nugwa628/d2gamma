//! The message pairs of the client's item-moving shortcuts: a stored item
//! lifted with 0x19, then placed with the drag's own message (0x18 on the
//! stash, cube or inventory page, 0x17 on the ground). The server takes
//! them as it takes a drag.

use super::*;

/// A `record` item stored at (`x`, `y`) of `page` through the cursor.
fn stored(w: &mut World, record: usize, x: u32, y: u32, page: u32) -> Guid {
    let g = w.cursor_item(record);
    assert_eq!(w.handle(&insert(g, x, y, page)), Ok(0));
    w.drain();
    g
}

fn at(w: &World, g: Guid) -> (u8, i32, i32) {
    let d = w.data(g);
    (d.page, d.x, d.y)
}

/// Inventory → stash in town: the lift, then the insert at the stash spot.
#[test]
fn an_inventory_item_goes_to_the_stash_by_a_lift_and_an_insert() {
    let mut w = World::new();
    w.rest.in_town = true;
    let k = stored(&mut w, KEY, 4, 1, 0);
    assert_eq!(w.handle(&lift(k)), Ok(0));
    assert_eq!(w.handle(&insert(k, 0, 0, 4)), Ok(0));
    assert_eq!(at(&w, k), (4, 0, 0));
    assert_eq!(w.mode(k), 0);
    assert_eq!(w.inventory().cursor(), None);
}

/// Stash → inventory: an idle player is held to page 0, so the lift needs
/// the open stash (the stash interaction), which the shortcut needs too.
#[test]
fn a_stash_item_comes_back_with_the_stash_open() {
    let mut w = World::new();
    w.rest.in_town = true;
    let k = stored(&mut w, KEY, 5, 7, 4);
    assert_eq!(w.handle(&lift(k)), Ok(3), "stash closed");
    assert_eq!(w.mode(k), 0);
    // The stash object the player is using.
    let stash = w.alloc(UnitType::Object, 267);
    let guid = w.units.get(stash).unwrap().guid;
    let p = w.player;
    w.units.get_mut(p).unwrap().interact.set(2, guid);
    assert_eq!(w.handle(&lift(k)), Ok(0));
    assert_eq!(w.handle(&insert(k, 0, 0, 0)), Ok(0));
    assert_eq!(at(&w, k), (0, 0, 0));
}

/// Inventory ↔ open cube: in by the lift and an insert on page 3, out by
/// the lift (the cube interaction lets it leave page 3) and an insert on
/// page 0.
#[test]
fn items_move_in_and_out_of_the_open_cube() {
    let mut w = World::new();
    let cube = stored(&mut w, BOX, 8, 0, 0);
    let k = stored(&mut w, CAP, 0, 0, 0);
    assert_eq!(w.handle(&msg(0x20, &[cube, 0, 0])), Ok(0));
    w.drain();
    assert_eq!(w.handle(&lift(k)), Ok(0));
    assert_eq!(w.handle(&insert(k, 1, 2, 3)), Ok(0));
    assert_eq!(at(&w, k), (3, 1, 2));
    assert_eq!(w.handle(&lift(k)), Ok(0));
    assert_eq!(w.handle(&insert(k, 2, 0, 0)), Ok(0));
    assert_eq!(at(&w, k), (0, 2, 0));
}

/// A 2 × 4 two-hander takes its whole footprint: placed where it fits, a
/// cell where it overhangs the 6-wide stash is refused and the item stays
/// on the cursor. (The client checks the fit first and sends nothing then.)
#[test]
fn a_two_hander_is_placed_only_where_it_fits() {
    let mut w = World::new();
    w.rest.in_town = true;
    let s = stored(&mut w, TWO_HANDER, 0, 0, 0);
    assert_eq!(w.handle(&lift(s)), Ok(0));
    assert_eq!(w.handle(&insert(s, 5, 0, 4)), Ok(3), "overhangs");
    assert_eq!(w.mode(s), 4);
    assert_eq!(w.handle(&insert(s, 4, 4, 4)), Ok(0));
    assert_eq!(at(&w, s), (4, 4, 4));
}

/// Drop at the feet: the lift, then 0x17 on the cursor item.
#[test]
fn an_inventory_item_drops_by_a_lift_and_a_drop() {
    let mut w = World::new();
    let k = stored(&mut w, KEY, 3, 2, 0);
    w.rest.room_at = true;
    w.rest.spot = Some(Spot {
        room: w.room,
        x: 13,
        y: 12,
    });
    assert_eq!(w.handle(&lift(k)), Ok(0));
    assert_eq!(w.handle(&drop_msg(k)), Ok(0));
    assert_eq!(w.mode(k), 3);
    assert!(w.in_room(k));
    assert_eq!(w.inventory().cursor(), None);
}
