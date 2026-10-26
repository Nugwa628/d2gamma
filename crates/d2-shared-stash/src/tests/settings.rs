//! A game's own stash settings: the item pages' shape, the slot cap, the
//! category pages and the page names, each used when a mod game sets it,
//! and D2Gamma's defaults unchanged.

use super::*;
use crate::local::file_name;

/// A mod game's shape: two pages of 14 x 14 the player may name.
const WIDE: StashShape = StashShape {
    pages: 2,
    cols: 14,
    rows: 14,
    renamable: true,
};

fn open_shaped(dir: &Path, catalog: Catalog, shape: StashShape) -> LocalStash {
    LocalStash::open_with(dir, key(MOD, false), catalog, shape).unwrap()
}

fn put_at(
    s: &mut LocalStash,
    g: &mut FakeGame,
    unit: u32,
    at: Cell,
) -> Result<PutDone, StoreError> {
    s.put(g, Put::Item { unit, at: Some(at) })
}

#[test]
fn d2gammas_stash_keeps_its_shape_cap_and_pages() {
    assert_eq!(
        StashShape::DEFAULT,
        StashShape {
            pages: 10,
            cols: 8,
            rows: 11,
            renamable: false
        }
    );
    assert_eq!(StashShape::default(), StashShape::DEFAULT);
    let cat = catalog();
    assert_eq!(cat.cap(), STACK_CAP);
    assert_eq!(STACK_CAP, 9_999);
    assert!(cat.categories().is_empty());
    assert_eq!(cat.pages(), StackPage::ALL.to_vec());
    assert_eq!(cat.cols(StackPage::Gems), 7);
    assert_eq!(cat.page_name(StackPage::Misc), "Misc");

    let dir = folder("settings-default");
    let mut s = open(&dir, false);
    assert_eq!(s.shape(), StashShape::DEFAULT);
    let names = s.page_names().unwrap();
    assert_eq!(names.first().map(String::as_str), Some("I"));
    assert_eq!(names.last().map(String::as_str), Some("X"));
    assert_eq!(names.len(), 10);
    assert_eq!(
        s.rename_page(0, "Mine"),
        Err(StoreError::Refused(Refusal::BadRequest))
    );
    // A drop past the 8 columns is not a cell of D2Gamma's pages.
    let mut g = FakeGame::new(&dir, "Hero");
    let ring = g.add("rin", None, (1, 1), "Ring");
    put_at(&mut s, &mut g, ring, Cell::new(0, 12, 0)).unwrap();
    assert_eq!(s.page(0).unwrap()[0].cell, Cell::new(0, 0, 0));
    assert_eq!(
        LocalStash::file_mod(&dir.join(file_name(&key(MOD, false)))).unwrap(),
        Some(MOD.to_owned())
    );
}

#[test]
fn a_mod_shape_places_items_on_its_own_grid() {
    let dir = folder("settings-shape");
    let mut s = open_shaped(&dir, catalog(), WIDE);
    assert_eq!(s.shape(), WIDE);
    let mut g = FakeGame::new(&dir, "Hero");
    let plate = g.add("xpl", None, (2, 4), "Plate");
    put_at(&mut s, &mut g, plate, Cell::new(1, 12, 10)).unwrap();
    assert_eq!(s.page(1).unwrap()[0].cell, Cell::new(1, 12, 10));
    // Past the 14 rows: the first free cell of that page.
    let ring = g.add("rin", None, (1, 1), "Ring");
    put_at(&mut s, &mut g, ring, Cell::new(1, 0, 14)).unwrap();
    assert!(s
        .page(1)
        .unwrap()
        .iter()
        .any(|i| i.cell == Cell::new(1, 0, 0)));
    // Page 2 is not one of its pages: the pages it has.
    let amulet = g.add("amu", None, (1, 1), "Amulet");
    put_at(&mut s, &mut g, amulet, Cell::new(2, 0, 0)).unwrap();
    assert!(s.page(2).unwrap().is_empty());
    let store = s.with_game(&mut g);
    assert_eq!(StashStore::shape(&store), WIDE);
}

#[test]
fn a_shape_that_shrank_moves_items_back_on_the_pages() {
    let dir = folder("settings-shrink");
    {
        let mut s = open_shaped(&dir, catalog(), WIDE);
        let mut g = FakeGame::new(&dir, "Hero");
        let ring = g.add("rin", None, (1, 1), "Ring");
        put_at(&mut s, &mut g, ring, Cell::new(0, 13, 13)).unwrap();
    }
    let mut s = open(&dir, false);
    let page = s.page(0).unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].cell, Cell::new(0, 0, 0));
}

#[test]
fn the_slot_cap_is_the_catalogs() {
    let dir = folder("settings-cap");
    let mut s = open_shaped(&dir, catalog().with_cap(20), StashShape::DEFAULT);
    assert_eq!(s.catalog().cap(), 20);
    let mut g = FakeGame::new(&dir, "Hero");
    let keys = g.add("key", Some(15), (1, 1), "key");
    s.put(&mut g, Put::Stacks { units: vec![keys] }).unwrap();
    let more = g.add("key", Some(6), (1, 1), "key");
    let done = s.put(&mut g, Put::Stacks { units: vec![more] }).unwrap();
    assert_eq!(done.left, vec![(more, Refusal::StashFull)]);
    assert_eq!(
        s.put(
            &mut g,
            Put::Item {
                unit: more,
                at: None
            }
        ),
        Err(StoreError::Refused(Refusal::StashFull))
    );
    assert_eq!(s.stacks().unwrap().get("key"), Some(&15));

    // A cap above D2Gamma's lets a slot past 9,999.
    let dir = folder("settings-cap-high");
    let mut s = open_shaped(&dir, catalog().with_cap(9_999_999), StashShape::DEFAULT);
    let mut g = FakeGame::new(&dir, "Hero");
    let a = g.add("key", Some(9_000), (1, 1), "key");
    let b = g.add("key", Some(9_000), (1, 1), "key");
    let done = s.put(&mut g, Put::Stacks { units: vec![a, b] }).unwrap();
    assert_eq!(done.moved, vec![a, b]);
    assert_eq!(s.stacks().unwrap().get("key"), Some(&18_000));
}

fn orbs() -> CategoryPage {
    CategoryPage {
        name: "Orbs".into(),
        cols: 4,
        codes: vec![
            // Already a slot (the Misc page's): keeps it.
            Stackable::new("cur1", "Mod Orb"),
            Stackable::new("orb1", "Orb One"),
            Stackable::new("orb2", "Orb Two"),
        ],
        item_slots: 3,
    }
}

#[test]
fn category_pages_add_named_slots() {
    let cat = catalog().with_categories(vec![orbs()]);
    assert_eq!(cat.pages().len(), 4);
    assert_eq!(cat.pages()[3], StackPage::Category(0));
    assert_eq!(cat.page_name(StackPage::Category(0)), "Orbs");
    assert_eq!(cat.cols(StackPage::Category(0)), 4);
    assert_eq!(cat.slot("cur1").unwrap().page, StackPage::Misc);
    let one = cat.slot("orb1").unwrap();
    assert_eq!((one.page, one.at), (StackPage::Category(0), (0, 0)));
    assert_eq!(cat.slot("orb2").unwrap().at, (1, 0));
    assert_eq!(
        cat.at(StackPage::Category(0), (1, 0))
            .map(|s| s.code.as_str()),
        Some("orb2")
    );
    assert_eq!(cat.categories()[0].item_slots, 3);
    // D2Gamma's pages are as they were.
    assert_eq!(
        cat.page(StackPage::Runes).count(),
        catalog().page(StackPage::Runes).count()
    );

    // The counters work as any slot's.
    let dir = folder("settings-category-stacks");
    let mut s = open_shaped(&dir, cat, StashShape::DEFAULT);
    let mut g = FakeGame::new(&dir, "Hero");
    let orb = g.add("orb1", Some(1), (1, 1), "orb1");
    s.put(&mut g, Put::Stacks { units: vec![orb] }).unwrap();
    assert_eq!(s.stacks().unwrap().get("orb1"), Some(&1));
}

#[test]
fn whole_items_fill_a_categorys_slots() {
    let dir = folder("settings-category-items");
    let cat = catalog().with_categories(vec![orbs()]);
    let mut s = open_shaped(&dir, cat, StashShape::DEFAULT);
    let mut g = FakeGame::new(&dir, "Hero");
    let page = CATEGORY_PAGE;
    let units: Vec<u32> = (0..4)
        .map(|i| g.add("jew", None, (1, 2), &format!("Jewel{i}")))
        .collect();
    put_at(&mut s, &mut g, units[0], Cell::new(page, 0, 0)).unwrap();
    // The same slot is taken: the category's first free slot.
    put_at(&mut s, &mut g, units[1], Cell::new(page, 0, 0)).unwrap();
    put_at(&mut s, &mut g, units[2], Cell::new(page, 3, 0)).unwrap();
    let cells: Vec<Cell> = s.page(page).unwrap().iter().map(|i| i.cell).collect();
    assert_eq!(
        cells,
        vec![
            Cell::new(page, 0, 0),
            Cell::new(page, 1, 0),
            Cell::new(page, 2, 0)
        ]
    );
    // Three slots: the fourth is refused and stays on the character.
    assert_eq!(
        put_at(&mut s, &mut g, units[3], Cell::new(page, 0, 0)),
        Err(StoreError::Refused(Refusal::StashFull))
    );
    assert_eq!(g.units(), vec![units[3]]);
    // No item page shows them; a take frees the slot.
    assert!(s.page(0).unwrap().is_empty());
    let first = s.page(page).unwrap()[0].id;
    s.take(
        &mut g,
        Take::Item {
            id: first,
            to: TakeTo::Inventory,
        },
    )
    .unwrap();
    put_at(&mut s, &mut g, units[3], Cell::new(page, 0, 0)).unwrap();
    assert_eq!(s.page(page).unwrap().len(), 3);
    // A reopen keeps every slot; a slot past the category is no cell.
    drop(s);
    let mut s = open_shaped(
        &dir,
        catalog().with_categories(vec![orbs()]),
        StashShape::DEFAULT,
    );
    assert_eq!(s.page(page).unwrap().len(), 3);
    let id = s.page(page).unwrap()[0].id;
    assert_eq!(
        s.move_item(id, Cell::new(page, 3, 0)),
        Err(StoreError::Refused(Refusal::StashFull))
    );
}

#[test]
fn renamable_pages_keep_their_names() {
    let dir = folder("settings-names");
    let mut s = open_shaped(&dir, catalog(), WIDE);
    assert_eq!(s.page_names().unwrap(), vec!["I", "II"]);
    s.rename_page(1, "  Charms ").unwrap();
    assert_eq!(s.page_names().unwrap(), vec!["I", "Charms"]);
    let bad = Err(StoreError::Refused(Refusal::BadRequest));
    assert_eq!(s.rename_page(1, "Thirteen char"), bad);
    assert_eq!(s.rename_page(2, "Past"), bad);
    assert_eq!(s.rename_page(0, "a\nb"), bad);
    s.rename_page(0, "Twelve chars").unwrap();
    drop(s);
    let mut s = open_shaped(&dir, catalog(), WIDE);
    assert_eq!(s.page_names().unwrap(), vec!["Twelve chars", "Charms"]);
    let mut g = FakeGame::new(&dir, "Hero");
    let mut store = s.with_game(&mut g);
    store.rename_page(1, "").unwrap();
    assert_eq!(store.page_names().unwrap(), vec!["Twelve chars", "II"]);
}

#[test]
fn recovery_alone_leaves_a_mod_games_cells() {
    let dir = folder("settings-recover-only");
    {
        let mut s = open_shaped(&dir, catalog(), WIDE);
        let mut g = FakeGame::new(&dir, "Hero");
        let ring = g.add("rin", None, (1, 1), "Ring");
        put_at(&mut s, &mut g, ring, Cell::new(1, 13, 13)).unwrap();
    }
    // A start that doesn't know the game's shape: no cell moves.
    let done = LocalStash::recover_only(&dir, key(MOD, false), catalog()).unwrap();
    assert!(done.homeless.is_empty());
    let mut s = open_shaped(&dir, catalog(), WIDE);
    assert_eq!(s.page(1).unwrap()[0].cell, Cell::new(1, 13, 13));
}
