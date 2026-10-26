//! Single player's stash: puts, takes, cells, stack counts, the inventory's
//! "Send stackables to stash", reloads, and the splits and mods kept
//! apart.

use super::*;
use crate::local::file_name;

fn file_text(g: &FakeGame) -> String {
    std::fs::read_to_string(&g.path).unwrap()
}

#[test]
fn an_item_goes_to_the_first_free_cell_and_comes_back() {
    let dir = folder("item-round-trip");
    let mut s = open(&dir, false);
    let mut g = FakeGame::new(&dir, "Hero");
    let sword = g.add("lsd", None, (1, 3), "Grandfather");
    g.save();
    let done = s
        .put(
            &mut g,
            Put::Item {
                unit: sword,
                at: None,
            },
        )
        .unwrap();
    assert_eq!(done.moved, vec![sword]);
    let page = s.page(0).unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].cell, Cell::new(0, 0, 0));
    assert_eq!(page[0].size, (1, 3));
    assert_eq!(page[0].look.as_deref(), Some(&b"look of Grandfather"[..]));
    assert!(g.items.is_empty());
    assert!(
        !file_text(&g).contains("Grandfather"),
        "the file shows the move"
    );
    assert!(!g.held, "saves are released after the move");

    let id = page[0].id;
    assert_eq!(
        s.take(
            &mut g,
            Take::Item {
                id,
                to: TakeTo::Cursor
            }
        ),
        Ok(1)
    );
    assert!(s.page(0).unwrap().is_empty());
    assert_eq!(g.totals().get("Grandfather"), Some(&1));
    assert!(file_text(&g).contains("Grandfather"));
}

#[test]
fn a_drop_on_a_cell_lands_there_or_on_the_next_free_one() {
    let dir = folder("cells");
    let mut s = open(&dir, false);
    let mut g = FakeGame::new(&dir, "Hero");
    let a = g.add("xpl", None, (2, 3), "PlateA");
    let b = g.add("xpl", None, (2, 3), "PlateB");
    let c = g.add("rin", None, (1, 1), "Ring");
    s.put(
        &mut g,
        Put::Item {
            unit: a,
            at: Some(Cell::new(3, 4, 2)),
        },
    )
    .unwrap();
    // Over the first plate: the first free cells of that page.
    s.put(
        &mut g,
        Put::Item {
            unit: b,
            at: Some(Cell::new(3, 5, 3)),
        },
    )
    .unwrap();
    // Past the page's edge: same.
    s.put(
        &mut g,
        Put::Item {
            unit: c,
            at: Some(Cell::new(3, COLS, 0)),
        },
    )
    .unwrap();
    let mut cells: Vec<(String, Cell)> = s
        .page(3)
        .unwrap()
        .into_iter()
        .map(|i| (i.facts.unwrap().name, i.cell))
        .collect();
    cells.sort();
    assert_eq!(
        cells,
        vec![
            ("PlateA".into(), Cell::new(3, 4, 2)),
            ("PlateB".into(), Cell::new(3, 0, 0)),
            ("Ring".into(), Cell::new(3, 2, 0)),
        ]
    );
    let ring = s.search("ring").unwrap()[0].0;
    assert_eq!(
        s.move_item(ring, Cell::new(3, 4, 3)),
        Err(StoreError::Refused(Refusal::StashFull)),
        "onto the plate: refused"
    );
    s.move_item(ring, Cell::new(7, 7, 10)).unwrap();
    assert_eq!(s.page(7).unwrap()[0].cell, Cell::new(7, 7, 10));
    // Packing page 3: the plates first, top-left.
    s.arrange(3).unwrap();
    let mut cells: Vec<Cell> = s.page(3).unwrap().into_iter().map(|i| i.cell).collect();
    cells.sort();
    assert_eq!(cells, vec![Cell::new(3, 0, 0), Cell::new(3, 2, 0)]);
}

#[test]
fn full_pages_refuse_and_the_item_stays_on_the_character() {
    let dir = folder("full");
    let mut s = open(&dir, false);
    let mut g = FakeGame::new(&dir, "Hero");
    let per_page = usize::from(COLS / 2) * usize::from(ROWS / 4);
    for i in 0..per_page * usize::from(PAGES) {
        let u = g.add("xpl", None, (2, 4), &format!("P{i}"));
        s.put(&mut g, Put::Item { unit: u, at: None }).unwrap();
    }
    let last = g.add("xpl", None, (2, 4), "Last");
    assert_eq!(
        s.put(
            &mut g,
            Put::Item {
                unit: last,
                at: None
            }
        ),
        Err(StoreError::Refused(Refusal::StashFull))
    );
    assert_eq!(g.units(), vec![last]);
}

#[test]
fn send_stackables_moves_only_listed_stackables() {
    let dir = folder("send-mixed");
    let mut s = open(&dir, false);
    let mut g = FakeGame::new(&dir, "Hero");
    let runes = [g.stackable("r01"), g.stackable("r01"), g.stackable("r30")];
    let gems = [g.stackable("gpw"), g.stackable("skz")];
    let keys = g.add("key", Some(7), (1, 1), "key");
    let orb = g.stackable("cur1");
    let sword = g.add("lsd", None, (1, 3), "Sword");
    // A rune with something about it that keeps it an item.
    let odd = g.add("r02", None, (1, 1), "OddRune");
    // A charm: not a listed kind.
    let charm = g.add("cm1", Some(1), (1, 1), "Charm");
    g.save();
    let before = world_totals(&g, &mut s);
    let units = g.units();
    let done = s.put(&mut g, Put::Stacks { units }).unwrap();
    let mut moved = done.moved.clone();
    moved.sort();
    let mut want: Vec<u32> = runes.iter().chain(&gems).copied().collect();
    want.extend([keys, orb]);
    want.sort();
    assert_eq!(moved, want);
    assert_eq!(
        done.left,
        vec![
            (sword, Refusal::BadRequest),
            (odd, Refusal::BadRequest),
            (charm, Refusal::BadRequest)
        ]
    );
    assert_eq!(g.units(), vec![sword, odd, charm], "the others never moved");
    let stacks = s.stacks().unwrap();
    assert_eq!(stacks.get("r01"), Some(&2));
    assert_eq!(stacks.get("r30"), Some(&1));
    assert_eq!(stacks.get("key"), Some(&7));
    assert_eq!(stacks.get("cur1"), Some(&1));
    assert_eq!(world_totals(&g, &mut s), before);
    let file = FakeGame::load(&dir, "Hero");
    assert_eq!(file.totals(), g.totals(), "the file holds the result");
}

#[test]
fn a_full_slot_leaves_the_rest_on_the_character() {
    let dir = folder("cap");
    let mut s = open(&dir, false);
    let mut g = FakeGame::new(&dir, "Hero");
    let big = g.add("key", Some(STACK_CAP - 1), (1, 1), "key");
    s.put(&mut g, Put::Stacks { units: vec![big] }).unwrap();
    let one = g.stackable("key");
    let two = g.add("key", Some(2), (1, 1), "key");
    let rune = g.stackable("r10");
    let done = s
        .put(
            &mut g,
            Put::Stacks {
                units: vec![one, two, rune],
            },
        )
        .unwrap();
    assert_eq!(done.moved, vec![one, rune]);
    assert_eq!(done.left, vec![(two, Refusal::StashFull)]);
    assert_eq!(s.stacks().unwrap().get("key"), Some(&STACK_CAP));
    assert_eq!(g.units(), vec![two]);
}

#[test]
fn taking_from_a_slot_gives_single_items_as_many_as_fit() {
    let dir = folder("take-stack");
    let mut s = open(&dir, false);
    let mut g = FakeGame::new(&dir, "Hero");
    let units: Vec<u32> = (0..5).map(|_| g.stackable("r20")).collect();
    s.put(&mut g, Put::Stacks { units }).unwrap();
    assert_eq!(
        s.take(
            &mut g,
            Take::Stack {
                code: "r20".into(),
                count: 3
            }
        ),
        Ok(3)
    );
    assert_eq!(g.items.len(), 3);
    assert!(g.items.iter().all(|i| i.count == Some(1)), "single items");
    assert_eq!(s.stacks().unwrap().get("r20"), Some(&2));
    // Only one more fits.
    for _ in 0..INV_ITEMS - 4 {
        g.add("lsd", None, (1, 3), "Filler");
    }
    assert_eq!(
        s.take(
            &mut g,
            Take::Stack {
                code: "r20".into(),
                count: 9
            }
        ),
        Ok(1)
    );
    assert_eq!(s.stacks().unwrap().get("r20"), Some(&1));
    assert_eq!(
        s.take(
            &mut g,
            Take::Stack {
                code: "r20".into(),
                count: 1
            }
        ),
        Err(StoreError::NoRoom)
    );
    assert_eq!(s.stacks().unwrap().get("r20"), Some(&1), "nothing moved");
    assert_eq!(
        s.take(
            &mut g,
            Take::Stack {
                code: "r21".into(),
                count: 1
            }
        ),
        Err(StoreError::Refused(Refusal::ItemNotFound))
    );
}

#[test]
fn keys_come_back_in_the_game_s_own_stacks() {
    let dir = folder("keys");
    let mut s = open(&dir, false);
    let mut g = FakeGame::new(&dir, "Hero");
    let k = g.add("key", Some(12), (1, 1), "key");
    let k2 = g.add("key", Some(8), (1, 1), "key");
    s.put(&mut g, Put::Stacks { units: vec![k, k2] }).unwrap();
    assert_eq!(s.stacks().unwrap().get("key"), Some(&20));
    assert_eq!(
        s.take(
            &mut g,
            Take::Stack {
                code: "key".into(),
                count: 20
            }
        ),
        Ok(20)
    );
    let counts: Vec<Option<u32>> = g.items.iter().map(|i| i.count).collect();
    assert_eq!(counts, vec![Some(12), Some(8)]);
}

#[test]
fn gold_moves_both_ways_within_the_limits() {
    let dir = folder("gold");
    let mut s = open(&dir, false);
    let mut g = FakeGame::new(&dir, "Hero");
    g.gold = 5000;
    s.put(&mut g, Put::Gold(3000)).unwrap();
    assert_eq!((g.gold, s.gold().unwrap()), (2000, 3000));
    assert!(s.put(&mut g, Put::Gold(9000)).is_err());
    assert_eq!((g.gold, s.gold().unwrap()), (2000, 3000));
    g.gold = GOLD_MAX - 100;
    assert_eq!(s.take(&mut g, Take::Gold(3000)), Ok(100));
    assert_eq!((g.gold, s.gold().unwrap()), (GOLD_MAX, 2900));
}

#[test]
fn search_finds_items_on_every_page() {
    let dir = folder("search");
    let mut s = open(&dir, false);
    let mut g = FakeGame::new(&dir, "Hero");
    for (name, page) in [("Shako", 0), ("Oculus", 4), ("Shaftstop", 9)] {
        let u = g.add("uap", None, (2, 2), name);
        s.put(
            &mut g,
            Put::Item {
                unit: u,
                at: Some(Cell::new(page, 0, 0)),
            },
        )
        .unwrap();
    }
    let hits: Vec<u8> = s
        .search("sha")
        .unwrap()
        .into_iter()
        .map(|(_, c)| c.page)
        .collect();
    assert_eq!(hits, vec![0, 9]);
    assert!(s.search("  ").unwrap().is_empty());
}

#[test]
fn a_reload_keeps_everything_and_a_backup() {
    let dir = folder("reload");
    let mut g = FakeGame::new(&dir, "Hero");
    let ring = g.add("rin", None, (1, 1), "Ring");
    let rune = g.stackable("r33");
    g.gold = 100;
    {
        let mut s = open(&dir, false);
        s.put(
            &mut g,
            Put::Item {
                unit: ring,
                at: Some(Cell::new(2, 3, 4)),
            },
        )
        .unwrap();
        s.put(&mut g, Put::Stacks { units: vec![rune] }).unwrap();
        s.put(&mut g, Put::Gold(60)).unwrap();
    }
    let mut s = open(&dir, false);
    assert_eq!(s.recovered(), &Recovered::default());
    assert_eq!(s.page(2).unwrap()[0].cell, Cell::new(2, 3, 4));
    assert_eq!(s.stacks().unwrap().get("r33"), Some(&1));
    assert_eq!(s.gold().unwrap(), 60);
    let bak = dir.join(format!("{}.bak", file_name(&key(MOD, false))));
    assert!(bak.is_file(), "{} kept", bak.display());
}

#[test]
fn softcore_and_hardcore_are_apart() {
    let dir = folder("splits");
    let mut sc = open(&dir, false);
    let mut hc = open(&dir, true);
    assert_ne!(sc.path(), hc.path());
    let mut g = FakeGame::new(&dir, "Hero");
    let rune = g.stackable("r01");
    sc.put(&mut g, Put::Stacks { units: vec![rune] }).unwrap();
    assert_eq!(sc.stacks().unwrap().get("r01"), Some(&1));
    assert!(hc.stacks().unwrap().is_empty());
    assert_eq!(
        hc.take(
            &mut g,
            Take::Stack {
                code: "r01".into(),
                count: 1
            }
        ),
        Err(StoreError::Refused(Refusal::ItemNotFound))
    );
    let classic = LocalStash::open(
        &dir,
        EconomyKey {
            expansion: false,
            ..key(MOD, false)
        },
        catalog(),
    )
    .unwrap();
    assert!(classic.path().ends_with("shared-stash-sc-classic.sqlite"));
}

#[test]
fn mods_are_apart() {
    let a = folder("mod-a");
    let b = folder("mod-b");
    let mut base = open(&a, false);
    let mut other = LocalStash::open(&b, key("other-mod", false), catalog()).unwrap();
    let mut g = FakeGame::new(&a, "Hero");
    let rune = g.stackable("r05");
    base.put(&mut g, Put::Stacks { units: vec![rune] }).unwrap();
    assert!(other.stacks().unwrap().is_empty());
    drop(base);
    // Another mod's name on the same file is refused.
    assert!(LocalStash::open(&a, key("other-mod", false), catalog()).is_err());
}

#[test]
fn a_game_save_drops_a_pending_record() {
    let dir = folder("saved");
    let mut s = open(&dir, false);
    let mut g = FakeGame::new(&dir, "Hero");
    let rune = g.stackable("r01");
    // The character's file can't be written (its folder is a file): the
    // move stands, its record stays pending.
    g.path = dir.join("blocked").join("Hero.d2s");
    std::fs::write(dir.join("blocked"), b"x").unwrap();
    assert!(matches!(
        s.put(&mut g, Put::Stacks { units: vec![rune] }),
        Err(StoreError::Io(_))
    ));
    assert_eq!(s.stacks().unwrap().get("r01"), Some(&1));
    // The game saved the character itself: nothing left to write.
    s.character_saved("Hero").unwrap();
    std::fs::remove_file(dir.join("blocked")).unwrap();
    drop(s);
    let s = open(&dir, false);
    assert!(s.recovered().rewritten.is_empty());
}
