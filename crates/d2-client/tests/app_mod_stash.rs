//! A mod game's own stash on the real game (the stash seams,
//! `d2_sim::rules::stash`): the engine's item host lists, reads, destroys,
//! places and makes a player's real items, and the mod's stash requests
//! arrive as mod messages with the items at hand.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use d2_client::app::server_thread::ThreadLink;
use d2_client::app::single_player::{self, GameBuilder, DEFAULT_SEED};
use d2_client::bridge::link::{SendQueue, ServerLink};
use d2_proto::envelope::{self, Direction};
use d2_server::{mod_items, stash_items};
use d2_sim::rules::stash::{
    self, HeldItem, ItemHost, ItemPlace, PlaceAt, StashDest, StashDrop, StashHooks, StashRefusal,
};
use d2_sim::rules::{ModOutbox, ModRules, Ruleset};
use d2_sim::units::UnitId;

use crate::app_support;

struct StepClock(Arc<AtomicU32>);

impl d2_server::seams::Clock for StepClock {
    fn now_ms(&mut self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Mod message: the send-to-stash button.
const SEND: u8 = 1;
/// Mod message: the last stored item back to the cursor.
const WITHDRAW: u8 = 2;

/// A tiny mod stash: runes to a counter, anything else kept whole as its
/// record.
#[derive(Debug, Default)]
struct Book {
    runes: Mutex<u32>,
    pages: Mutex<Vec<Vec<u8>>>,
    /// Mod messages the stash did not take (for `ModRules::message`).
    other: Mutex<Vec<Vec<u8>>>,
}

impl StashHooks for Book {
    fn message(
        &self,
        items: &mut dyn ItemHost,
        player: UnitId,
        payload: &[u8],
        _: &mut dyn ModOutbox,
    ) -> bool {
        match payload.first() {
            Some(&SEND) => {
                stash::send(items, self, player).unwrap();
                true
            }
            Some(&WITHDRAW) => {
                let rec = self.pages.lock().unwrap().pop().unwrap();
                items.place(player, &rec, PlaceAt::Cursor).unwrap();
                true
            }
            _ => false,
        }
    }
    fn put(
        &self,
        _: UnitId,
        item: &HeldItem,
        record: &[u8],
        _: StashDrop,
    ) -> Result<StashDest, StashRefusal> {
        if &item.code == b"r01 " {
            *self.runes.lock().unwrap() += 1;
            return Ok(StashDest::Deposit);
        }
        let mut pages = self.pages.lock().unwrap();
        pages.push(record.to_vec());
        Ok(StashDest::Page {
            page: 0,
            x: pages.len() as u8 - 1,
            y: 0,
        })
    }
}

#[derive(Debug, Default)]
struct BookMod(Book);

impl ModRules for BookMod {
    fn name(&self) -> &str {
        "book"
    }
    fn stash(&self) -> Option<&dyn StashHooks> {
        Some(&self.0)
    }
    fn message(
        &self,
        _: &mut d2_sim::game::Game,
        _: UnitId,
        payload: &[u8],
        _: &mut dyn ModOutbox,
    ) {
        self.0.other.lock().unwrap().push(payload.to_vec());
    }
}

type Link = ThreadLink<single_player::Link<StepClock>>;

/// A new barbarian joined in a game under `rules` (in the Rogue camp).
fn joined(rules: Ruleset) -> (Link, Arc<AtomicU32>) {
    let character = single_player::new_character("barbarian", "Booker").unwrap();
    let ms = Arc::new(AtomicU32::new(1000));
    let (mut link, _) = GameBuilder::new(app_support::game_data())
        .seed(DEFAULT_SEED)
        .character(character.clone())
        .rules(rules)
        .start(StepClock(ms.clone()))
        .unwrap();
    link.send(
        SendQueue::System,
        &single_player::create_request_for(&character).encode(),
    )
    .unwrap();
    link.pump().unwrap();
    ms.fetch_add(40, Ordering::SeqCst);
    link.pump().unwrap();
    link.receive();
    link.send(SendQueue::System, &[0x6B]).unwrap();
    ms.fetch_add(40, Ordering::SeqCst);
    assert!(link.pump().unwrap().ticked);
    link.receive();
    (link, ms)
}

fn on_game<R: Send + 'static>(
    link: &mut Link,
    f: impl FnOnce(&mut single_player::Sim, UnitId) -> R + Send + 'static,
) -> R {
    link.with(move |l| {
        let sim = &mut l.host_mut().game;
        let (p, _) = single_player::local_player(sim).unwrap();
        f(sim, p)
    })
    .unwrap()
}

/// One mod message from the client, drained in the next frame.
fn send(link: &mut Link, ms: &AtomicU32, payload: &[u8]) {
    let msg = envelope::encode(Direction::ToServer, payload).unwrap();
    link.send(SendQueue::Game, &msg).unwrap();
    ms.fetch_add(40, Ordering::SeqCst);
    link.pump().unwrap();
    link.receive();
}

fn items_of(link: &mut Link) -> Vec<HeldItem> {
    on_game(link, |s, p| {
        mod_items::with_items(s, |items| items.items(p)).unwrap()
    })
}

fn codes(items: &[HeldItem], place: ItemPlace) -> Vec<[u8; 4]> {
    let mut c: Vec<_> = items
        .iter()
        .filter(|i| i.place == place)
        .map(|i| i.code)
        .collect();
    c.sort();
    c
}

/// The item host on D2Gamma's own game: list, read, make, place, destroy.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn the_item_host_moves_real_items() {
    let (mut link, _) = joined(Ruleset::Original);
    on_game(&mut link, |s, p| {
        stash_items::give_plain(s, p, "r01", 2).unwrap();
    });
    let before = items_of(&mut link);
    assert_eq!(
        codes(&before, ItemPlace::Inventory)
            .iter()
            .filter(|c| *c == b"r01 ")
            .count(),
        2
    );
    let rune = before.iter().find(|i| &i.code == b"r01 ").copied().unwrap();
    assert_eq!(rune.size, (1, 1));

    let (facts, town, free, room, made, stat) = on_game(&mut link, move |s, p| {
        mod_items::with_items(s, |items| {
            let rec = items.record(p, rune.guid).unwrap();
            let facts = items.facts(&rec).unwrap();
            let town = items.in_town(p);
            let free = items.cursor_free(p);
            let room = items.room(p, *b"r01 ");
            // A fresh item by code with a stat set (stat 70: quantity).
            let made = items
                .create(p, *b"key ", &[(70, 3)], PlaceAt::Inventory(None))
                .unwrap();
            let stat = items.item_stat(p, made, 70);
            (facts, town, free, room, made, stat)
        })
        .unwrap()
    });
    assert_eq!(facts.code, *b"r01 ");
    assert_eq!(facts.size, Some((1, 1)));
    assert_eq!(
        (facts.x, facts.y, facts.page),
        (rune.cell.0, rune.cell.1, Some(0))
    );
    assert!(town, "a new character starts in the Rogue camp");
    assert!(free);
    assert!(room > 0);
    assert_eq!(stat, Some(3));
    let after = items_of(&mut link);
    assert!(after
        .iter()
        .any(|i| i.guid == made && &i.code == b"key " && i.place == ItemPlace::Inventory));

    // Destroy the rune, put its record back on the cursor: the same kind.
    let placed = on_game(&mut link, move |s, p| {
        mod_items::with_items(s, |items| {
            let rec = items.record(p, rune.guid).unwrap();
            assert!(items.destroy(p, rune.guid));
            assert!(!items.destroy(p, rune.guid), "gone");
            let g = items.place(p, &rec, PlaceAt::Cursor).unwrap();
            assert!(!items.cursor_free(p));
            assert_eq!(
                items.place(p, &rec, PlaceAt::Cursor),
                Err(d2_sim::rules::stash::ItemError::CursorBusy)
            );
            g
        })
        .unwrap()
    });
    let now = items_of(&mut link);
    let cursor: Vec<_> = now
        .iter()
        .filter(|i| i.place == ItemPlace::Cursor)
        .collect();
    assert_eq!(cursor.len(), 1);
    assert_eq!((cursor[0].guid, cursor[0].code), (placed, *b"r01 "));
    assert!(!now.iter().any(|i| i.guid == rune.guid));
    // Worn items are never destroyed.
    let worn = now
        .iter()
        .find(|i| i.place == ItemPlace::Equipped)
        .copied()
        .unwrap();
    let kept = on_game(&mut link, move |s, p| {
        mod_items::with_items(s, |items| items.destroy(p, worn.guid)).unwrap()
    });
    assert!(!kept);
}

/// A mod's stash requests reach its stash hooks with the items: the
/// send-to-stash pass takes the inventory (runes to the counter, the rest
/// whole), leaves the worn items, and a withdraw puts a kept record back
/// on the cursor; any other mod message still reaches `ModRules::message`.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn mod_stash_requests_arrive_with_the_items() {
    let rules = Arc::new(BookMod::default());
    let (mut link, ms) = joined(Ruleset::Mod(rules.clone()));
    on_game(&mut link, |s, p| {
        stash_items::give_plain(s, p, "r01", 2).unwrap();
        stash_items::give_plain(s, p, "isc", 1).unwrap();
    });
    let before = items_of(&mut link);
    let inventory = codes(&before, ItemPlace::Inventory).len();
    let worn = codes(&before, ItemPlace::Equipped);
    assert!(inventory >= 3);

    send(&mut link, &ms, &[SEND]);
    send(&mut link, &ms, &[9, 9]);
    let book = &rules.0;
    assert_eq!(*book.runes.lock().unwrap(), 2);
    assert_eq!(book.pages.lock().unwrap().len(), inventory - 2);
    assert_eq!(*book.other.lock().unwrap(), vec![vec![9, 9]]);
    let after = items_of(&mut link);
    assert!(codes(&after, ItemPlace::Inventory).is_empty());
    assert_eq!(codes(&after, ItemPlace::Equipped), worn);

    send(&mut link, &ms, &[WITHDRAW]);
    let back = items_of(&mut link);
    assert_eq!(codes(&back, ItemPlace::Cursor).len(), 1);
    assert_eq!(rules.0.pages.lock().unwrap().len(), inventory - 3);
}
