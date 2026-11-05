use std::collections::BTreeMap;

use super::runtime::{Happened, SECTION};
use super::*;
use crate::game::Game;
use crate::rng::Seed;
use crate::rules::data_mod::{DataMod, DataModFiles};
use crate::rules::{EventHooks, EventHost, ModRules, NewGame, Ruleset};
use crate::units::{UnitId, UnitType};
use d2_formats::d2s::ModSection;

const GOLD: u16 = 14;
const P: UnitId = UnitId(1);

/// An event host that knows a few units and records what the player is told.
#[derive(Default)]
struct Host {
    game: Game,
    types: BTreeMap<UnitId, UnitType>,
    classes: BTreeMap<UnitId, i32>,
    owners: BTreeMap<UnitId, UnitId>,
    stats: BTreeMap<(UnitId, u16), i32>,
    said: Vec<String>,
    vars: Vec<(String, i32)>,
}

impl Host {
    fn new() -> Host {
        let mut h = Host::default();
        h.types.insert(P, UnitType::Player);
        h
    }

    fn monster(&mut self, id: u32, class: i32) -> UnitId {
        let u = UnitId(id);
        self.types.insert(u, UnitType::Monster);
        self.classes.insert(u, class);
        u
    }
}

impl EventHost for Host {
    fn game(&mut self) -> &mut Game {
        &mut self.game
    }
    fn unit_type(&self, u: UnitId) -> Option<UnitType> {
        self.types.get(&u).copied()
    }
    fn class_id(&self, u: UnitId) -> i32 {
        self.classes.get(&u).copied().unwrap_or(0)
    }
    fn stat(&self, u: UnitId, stat: u16, _: u16) -> i32 {
        self.stats.get(&(u, stat)).copied().unwrap_or(0)
    }
    fn set_stat(&mut self, u: UnitId, stat: u16, value: i32) {
        self.stats.insert((u, stat), value);
    }
    fn has_state(&self, _: UnitId, _: u16) -> bool {
        false
    }
    fn seed(&mut self, _: UnitId) -> Option<&mut Seed> {
        None
    }
    fn level(&self, _: UnitId) -> Option<u32> {
        None
    }
    fn send(&mut self, _: UnitId, _: &[u8]) {}
    fn say(&mut self, _: UnitId, text: &str) {
        self.said.push(text.to_owned());
    }
    fn ui_variable(&mut self, _: UnitId, name: &str, value: i32) {
        self.vars.push((name.to_owned(), value));
    }
    fn owner(&self, u: UnitId) -> Option<UnitId> {
        self.owners.get(&u).copied()
    }
}

const RATS: &str = "[[quest]]
id = 1
name = \"Pest Control\"
text = \"Kill three rats.\"
done_text = \"The cellar is quiet.\"
giver = 7
[[quest.objective]]
kind = \"kill\"
monster = 12
count = 3
[[quest.reward]]
kind = \"stat\"
stat = 14
value = 500
[[quest.reward]]
kind = \"message\"
text = \"Thank you.\"

[[quest]]
id = 2
name = \"The Way Down\"
text = \"Go below.\"
requires = [1]
[[quest.objective]]
kind = \"enter\"
level = 5
[[quest.objective]]
kind = \"talk\"
npc = 8
";

fn rules(text: &str) -> QuestRules {
    QuestRules::from_toml(text).unwrap()
}

fn joined(r: &QuestRules, h: &mut Host) {
    r.load(P, &[]);
    r.happened(h, P, Happened::Look);
}

fn err(text: &str) -> String {
    QuestRules::from_toml(text).unwrap_err().to_string()
}

#[test]
fn a_quests_file_is_read() {
    let r = rules(RATS);
    assert_eq!(r.defs().len(), 2);
    let q = r.get(1).unwrap();
    assert_eq!((q.name.as_str(), q.giver), ("Pest Control", Some(7)));
    assert_eq!(
        q.objectives,
        [Objective::Kill {
            monster: 12,
            count: 3
        }]
    );
    assert_eq!(q.rewards.len(), 2);
    assert_eq!(r.get(2).unwrap().requires, [1]);
    assert!(QuestRules::from_toml("").unwrap().is_empty());
    assert!(QuestRules::none().is_empty());
}

#[test]
fn a_bad_quests_file_is_refused_with_the_quest_and_the_key() {
    let quest = |body: &str| format!("[[quest]]\nid = 1\nname = \"Q\"\n{body}");
    let one = "[[quest.objective]]\nkind = \"enter\"\nlevel = 1\n";
    assert!(err(&quest("")).contains("quest[0].objective"));
    assert!(err("version = 2").contains("`version`"));
    assert!(err("[[quest]]\nid = 0\nname = \"Q\"\n").contains("quest[0].id"));
    assert!(err(&format!(
        "{}{one}\n[[quest]]\nid = 1\nname = \"R\"\n{one}",
        quest("")
    ))
    .contains("defined twice"));
    assert!(err(&quest(&format!("requires = [9]\n{one}"))).contains("quest 9 is not defined"));
    assert!(err(&quest(&format!("requires = [1]\n{one}"))).contains("cannot require itself"));
    let loops = format!(
        "{}\n[[quest]]\nid = 2\nname = \"R\"\nrequires = [1]\n{one}",
        quest(&format!("requires = [2]\n{one}"))
    );
    assert!(err(&loops).contains("loop"));
    assert!(err(&quest(
        "[[quest.objective]]\nkind = \"kill\"\nmonster = 1\ncount = 0\n"
    ))
    .contains("quest[0].objective[0].count"));
    assert!(err(&quest(
        "[[quest.objective]]\nkind = \"reach_stat\"\nstat = 14\nvalue = 0\n"
    ))
    .contains("value"));
    assert!(err(&quest("[[quest.objective]]\nkind = \"dance\"\n")).contains("dance"));
    assert!(err(&quest(&format!(
        "{one}[[quest.reward]]\nkind = \"stat\"\nstat = 14\nvalue = 0\n"
    )))
    .contains("quest[0].reward[0].value"));
    let many = one.repeat(MAX_PARTS + 1);
    assert!(err(&quest(&many)).contains("objectives"));
    assert!(err(&format!(
        "[[quest]]\nid = 1\nname = \"{}\"\n{one}",
        "n".repeat(MAX_NAME + 1)
    ))
    .contains("longer than"));
    assert!(err(&quest(&format!("text = \"a\\nb\"\n{one}"))).contains("control"));
    assert!(err(&quest(&format!("{one}colour = 1\n"))).contains("colour"));
}

#[test]
fn a_quest_with_a_giver_starts_when_its_npc_is_talked_to_and_a_kill_counts() {
    let r = rules(RATS);
    let mut h = Host::new();
    joined(&r, &mut h);
    // Quest 1 needs its giver: nothing is said, the kill counts for nothing.
    assert!(h.said.is_empty());
    let rat = h.monster(2, 12);
    r.happened(&mut h, P, Happened::Killed(12));
    assert!(r.log_lines(P).is_empty());
    r.queue(P, Happened::Talked(7));
    r.periodic(&mut h);
    assert_eq!(h.said, ["Quest started: Pest Control", "Kill three rats."]);
    for _ in 0..2 {
        r.happened(&mut h, P, Happened::Killed(12));
    }
    assert_eq!(h.said[2..], ["Pest Control: 1/3", "Pest Control: 2/3"]);
    assert_eq!(h.stat(P, GOLD, 0), 0);
    r.happened(&mut h, P, Happened::Killed(12));
    // Done: told, rewarded once, and the next quest (no giver) starts.
    assert_eq!(h.stat(P, GOLD, 0), 500);
    assert_eq!(
        h.said[4..],
        [
            "Quest complete: Pest Control",
            "Thank you.",
            "Quest started: The Way Down",
            "Go below."
        ]
    );
    r.happened(&mut h, P, Happened::Killed(12));
    assert_eq!(h.stat(P, GOLD, 0), 500);
    let _ = rat;
}

#[test]
fn a_kill_counts_for_the_killer_and_the_owner_of_a_summon_only() {
    let r = rules(&RATS.replace("giver = 7\n", ""));
    let rules = DataMod::from_files(
        "m",
        &DataModFiles {
            quests: Some(("quests.toml", &RATS.replace("giver = 7\n", ""))),
            ..DataModFiles::default()
        },
    )
    .unwrap();
    let mut h = Host::new();
    rules.quests.load(P, &[]);
    rules.quests.happened(&mut h, P, Happened::Look);
    let rat = h.monster(2, 12);
    let pet = h.monster(3, 1);
    h.owners.insert(pet, P);
    let other = UnitId(9);
    h.types.insert(other, UnitType::Player);
    let hooks: &dyn EventHooks = &rules;
    hooks.unit_killed(&mut h, Some(other), rat);
    hooks.unit_killed(&mut h, None, rat);
    // A player dying is not a monster kill.
    hooks.unit_killed(&mut h, Some(P), other);
    assert!(rules.quests.log_lines(P)[1].ends_with("0/3"));
    hooks.unit_killed(&mut h, Some(P), rat);
    hooks.unit_killed(&mut h, Some(pet), rat);
    assert!(rules.quests.log_lines(P)[1].ends_with("2/3"));
    let _ = r;
}

#[test]
fn quests_chain_and_the_other_objectives_count() {
    let r = rules(&RATS.replace("giver = 7\n", ""));
    let mut h = Host::new();
    joined(&r, &mut h);
    // Quest 2 waits for quest 1.
    assert!(!r.log_lines(P).iter().any(|l| l.starts_with("The Way Down")));
    for _ in 0..3 {
        r.happened(&mut h, P, Happened::Killed(12));
    }
    r.happened(&mut h, P, Happened::Entered(4));
    r.happened(&mut h, P, Happened::Entered(5));
    r.happened(&mut h, P, Happened::Talked(9));
    assert!(!r
        .log_lines(P)
        .iter()
        .any(|l| l.starts_with("The Way Down (done)")));
    r.queue(P, Happened::Talked(8));
    r.periodic(&mut h);
    let log = r.log_lines(P);
    assert!(
        log.contains(&"The Way Down (done): The Way Down".to_owned()),
        "{log:?}"
    );
    assert_eq!(h.said.last().unwrap(), "Quest complete: The Way Down");
}

#[test]
fn a_skill_cast_and_a_stat_reached_count() {
    let toml = "[[quest]]\nid = 3\nname = \"Practice\"\n\
        [[quest.objective]]\nkind = \"use_skill\"\nskill = 36\ncount = 2\n\
        [[quest.objective]]\nkind = \"reach_stat\"\nstat = 14\nvalue = 100\n";
    let r = rules(toml);
    assert!(r.wants_skill_events() && r.wants_stat_checks());
    let mut h = Host::new();
    joined(&r, &mut h);
    r.queue(P, Happened::Cast(35));
    r.queue(P, Happened::Cast(36));
    r.queue(P, Happened::Cast(36));
    h.stats.insert((P, GOLD), 99);
    r.periodic(&mut h);
    assert!(!h.said.iter().any(|l| l.contains("complete")));
    h.stats.insert((P, GOLD), 120);
    r.periodic(&mut h);
    assert_eq!(h.said.last().unwrap(), "Quest complete: Practice");
    // A stat that falls back again does not undo it.
    h.stats.insert((P, GOLD), 0);
    r.periodic(&mut h);
    assert!(r.log_lines(P)[0].contains("(done)"));
}

#[test]
fn progress_is_saved_and_comes_back() {
    let r = rules(&RATS.replace("giver = 7\n", ""));
    let mut h = Host::new();
    joined(&r, &mut h);
    r.happened(&mut h, P, Happened::Killed(12));
    let saved = r.save(P);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].tag, SECTION);
    // A fresh game's rules, the same character joining.
    let again = rules(&RATS.replace("giver = 7\n", ""));
    again.load(P, &saved);
    assert_eq!(again.log_lines(P), r.log_lines(P));
    // A player with nothing has no section; a joined player is dropped on leaving.
    let q = UnitId(5);
    again.load(q, &[]);
    assert!(again.save(q).is_empty());
    again.leave(P);
    assert!(again.log_lines(P).is_empty());
}

#[test]
fn a_save_that_does_not_fit_the_file_is_read_as_far_as_it_goes() {
    let r = rules(&RATS.replace("giver = 7\n", ""));
    // Quest 1 with a count past its target and an extra count, quest 99 that
    // is not in the file, then the data cut short.
    let mut data = vec![1u8, 3, 0];
    data.extend_from_slice(&[1, 0, 0, 2]);
    data.extend_from_slice(&50u32.to_le_bytes());
    data.extend_from_slice(&7u32.to_le_bytes());
    data.extend_from_slice(&[99, 0, 1, 1]);
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&[2, 0, 0, 9]);
    r.load(P, &[ModSection::new(SECTION, data)]);
    let log = r.log_lines(P);
    assert!(log[1].ends_with("3/3"), "{log:?}");
    assert_eq!(log.len(), 2);
    // Garbage, an unknown version and an empty section are a new start.
    for bad in [vec![], vec![9, 0, 0], vec![1, 255, 255], vec![1]] {
        r.load(P, &[ModSection::new(SECTION, bad)]);
        assert!(r.log_lines(P).is_empty());
    }
}

#[test]
fn the_hooks_are_the_mods_only_when_it_has_quests() {
    let with = DataMod::from_files(
        "m",
        &DataModFiles {
            quests: Some(("quests.toml", RATS)),
            ..DataModFiles::default()
        },
    )
    .unwrap();
    assert!(with.events().is_some() && with.saves().is_some() && with.npcs().is_some());
    // Each game gets a rules object of its own, with no progress in it.
    with.quests.load(P, &[]);
    let game = NewGame::default();
    let own = with.for_game(&game).expect("a copy for the game");
    assert!(own.saves().is_some());
    assert!(with.quests.save(P).is_empty());

    // D2Gamma's rules, and a mod without quests, ask for none of it.
    let none = DataMod::from_files("m", &DataModFiles::default()).unwrap();
    assert!(none.events().is_none() && none.saves().is_none() && none.npcs().is_none());
    assert!(none.for_game(&game).is_none());
    assert!(none.skills().is_none());
    let orig = Ruleset::Original;
    assert!(orig.events().is_none() && orig.saves().is_none() && orig.npcs().is_none());
}

#[test]
fn the_quests_command_lists_the_log() {
    let with = DataMod::from_files(
        "m",
        &DataModFiles {
            quests: Some(("quests.toml", &RATS.replace("giver = 7\n", ""))),
            ..DataModFiles::default()
        },
    )
    .unwrap();
    let mut h = Host::new();
    let hooks: &dyn EventHooks = &with;
    assert!(!hooks.chat_command(&mut h, P, b"hello"));
    assert!(hooks.chat_command(&mut h, P, b"/quests"));
    assert_eq!(h.said, ["No quests yet."]);
    with.quests.load(P, &[]);
    hooks.player_joined(&mut h, P);
    h.said.clear();
    assert!(hooks.chat_command(&mut h, P, b" /quests "));
    assert_eq!(
        h.said,
        ["Pest Control: Kill three rats.", "  kill monster 12 0/3"]
    );
    hooks.player_left(&mut h, P);
    assert!(with.quests.log_lines(P).is_empty());
}

#[test]
fn a_data_mod_quest_is_asked_about_skill_events_only_when_one_needs_them() {
    let skill_quest = "[[quest]]\nid = 1\nname = \"Q\"\n\
        [[quest.objective]]\nkind = \"use_skill\"\nskill = 36\n";
    let d = |t: &str| {
        DataMod::from_files(
            "m",
            &DataModFiles {
                quests: Some(("quests.toml", t)),
                ..DataModFiles::default()
            },
        )
        .unwrap()
    };
    assert!(d(skill_quest).skills().is_some());
    assert!(d(RATS).skills().is_none());
}

/// The player's items: a bag of (code, quantity) with room for `room` more.
#[derive(Default)]
struct Bag {
    held: Vec<crate::rules::stash::HeldItem>,
    quantity: BTreeMap<u32, i32>,
    room: u32,
    made: Vec<[u8; 4]>,
    unknown: bool,
    gone: Vec<u32>,
}

impl Bag {
    fn with(codes: &[(&[u8; 4], i32)], room: u32) -> Bag {
        let mut b = Bag {
            room,
            ..Bag::default()
        };
        for (n, (code, q)) in codes.iter().enumerate() {
            b.add(
                n as u32 + 1,
                **code,
                *q,
                crate::rules::stash::ItemPlace::Inventory,
            );
        }
        b
    }

    fn add(&mut self, guid: u32, code: [u8; 4], q: i32, place: crate::rules::stash::ItemPlace) {
        self.held.push(crate::rules::stash::HeldItem {
            guid,
            code,
            size: (1, 1),
            place,
            cell: (0, 0),
        });
        if q > 1 {
            self.quantity.insert(guid, q);
        }
    }
}

impl crate::rules::ItemHost for Bag {
    fn items(&mut self, _: UnitId) -> Vec<crate::rules::stash::HeldItem> {
        self.held.clone()
    }
    fn item_stat(&mut self, _: UnitId, guid: u32, stat: u16) -> Option<i32> {
        (stat == crate::stats::ids::QUANTITY)
            .then(|| self.quantity.get(&guid).copied())
            .flatten()
    }
    fn in_town(&mut self, _: UnitId) -> bool {
        true
    }
    fn cursor_free(&mut self, _: UnitId) -> bool {
        true
    }
    fn size_of(&self, _: [u8; 4]) -> Option<(u8, u8)> {
        Some((1, 1))
    }
    fn room(&mut self, _: UnitId, _: [u8; 4]) -> u32 {
        self.room
    }
    fn record(&mut self, _: UnitId, _: u32) -> Option<Vec<u8>> {
        None
    }
    fn destroy(&mut self, _: UnitId, guid: u32) -> bool {
        let n = self.held.len();
        self.held.retain(|i| i.guid != guid);
        self.quantity.remove(&guid);
        let found = self.held.len() < n;
        if found {
            self.gone.push(guid);
        }
        found
    }
    fn place(
        &mut self,
        _: UnitId,
        _: &[u8],
        _: crate::rules::stash::PlaceAt,
    ) -> Result<u32, crate::rules::stash::ItemError> {
        Err(crate::rules::stash::ItemError::NoRoom)
    }
    fn create(
        &mut self,
        _: UnitId,
        code: [u8; 4],
        stats: &[(u16, i32)],
        _: crate::rules::stash::PlaceAt,
    ) -> Result<u32, crate::rules::stash::ItemError> {
        use crate::rules::stash::ItemError;
        if self.unknown {
            return Err(ItemError::UnknownCode(code));
        }
        if self.room == 0 {
            return Err(ItemError::NoRoom);
        }
        self.room -= 1;
        self.made.push(code);
        let guid = 100 + self.made.len() as u32;
        let q = stats
            .iter()
            .find(|(s, _)| *s == crate::stats::ids::QUANTITY)
            .map_or(1, |(_, v)| *v);
        self.add(guid, code, q, crate::rules::stash::ItemPlace::Inventory);
        Ok(guid)
    }
}

const FETCH: &str = "[[quest]]
id = 1
name = \"Fetch\"
[[quest.objective]]
kind = \"collect\"
item = \"hp1\"
count = 3
[[quest.reward]]
kind = \"item\"
item = \"mp1\"
count = 2
";

#[test]
fn a_collect_objective_counts_what_the_player_carries() {
    let r = rules(FETCH);
    assert!(r.wants_items());
    let mut h = Host::new();
    joined(&r, &mut h);
    // Two potions in the bag and one stacked item of the same code.
    let mut bag = Bag::with(&[(b"hp1 ", 1), (b"hp1 ", 1), (b"axe ", 1)], 5);
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    assert_eq!(h.said.last().unwrap(), "Fetch: 2/3");
    assert!(r.log_lines(P)[1].contains("collect hp1 2/3"));
    // The third arrives (a stack of two counts as two).
    bag.add(9, *b"hp1 ", 2, crate::rules::stash::ItemPlace::Belt);
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    assert!(h.said.iter().any(|l| l == "Quest complete: Fetch"));
    // The reward is handed out at the next pass.
    assert!(bag.made.is_empty());
    r.item_pass(&mut bag);
    assert_eq!(bag.made, [*b"mp1 ", *b"mp1 "]);
}

const HAND_IN: &str = "[[quest]]
id = 1
name = \"Hand in\"
[[quest.objective]]
kind = \"collect\"
item = \"hp1\"
count = 3
consume = true
";

#[test]
fn a_consume_objective_takes_the_items_when_the_quest_is_completed() {
    let r = rules(HAND_IN);
    let mut h = Host::new();
    joined(&r, &mut h);
    // Two single items and a stack of four: the singles and one piece of the
    // stack are taken, and the stack is replaced by one of the three left. The stash copy stays, and so does the axe.
    let mut bag = Bag::with(&[(b"hp1 ", 1), (b"hp1 ", 1), (b"axe ", 1)], 5);
    bag.add(9, *b"hp1 ", 4, crate::rules::stash::ItemPlace::Belt);
    bag.add(10, *b"hp1 ", 1, crate::rules::stash::ItemPlace::Stash);
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    assert!(h.said.iter().any(|l| l == "Quest complete: Hand in"));
    // Nothing is taken before the next pass.
    assert!(bag.gone.is_empty());
    r.item_pass(&mut bag);
    assert_eq!(bag.gone, [1, 2, 9]);
    let left: Vec<_> = bag
        .held
        .iter()
        .filter(|i| &i.code == b"hp1 " && i.place != crate::rules::stash::ItemPlace::Stash)
        .map(|i| bag.quantity.get(&i.guid).copied().unwrap_or(1))
        .collect();
    assert_eq!(left, [3]);
    assert!(bag.held.iter().any(|i| &i.code == b"axe "));
    // It takes once, and the quest stays done.
    bag.add(11, *b"hp1 ", 1, crate::rules::stash::ItemPlace::Inventory);
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    assert_eq!(bag.gone, [1, 2, 9]);
    assert!(r.log_lines(P)[0].contains("(done)"));
}

#[test]
fn a_collect_objective_without_consume_takes_nothing() {
    let r = rules(FETCH);
    let mut h = Host::new();
    joined(&r, &mut h);
    let mut bag = Bag::with(&[(b"hp1 ", 1), (b"hp1 ", 1), (b"hp1 ", 1)], 5);
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    r.item_pass(&mut bag);
    assert!(bag.gone.is_empty());
    assert_eq!(bag.held.len(), 3 + 2);
}

#[test]
fn dropped_items_take_the_collect_progress_back_until_it_is_met() {
    let r = rules(FETCH);
    let mut h = Host::new();
    joined(&r, &mut h);
    let mut bag = Bag::with(&[(b"hp1 ", 1), (b"hp1 ", 1)], 5);
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    assert!(r.log_lines(P)[1].contains("2/3"));
    bag.held.pop();
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    assert!(r.log_lines(P)[1].contains("1/3"));
    // The stash does not count.
    bag.add(7, *b"hp1 ", 1, crate::rules::stash::ItemPlace::Stash);
    bag.add(8, *b"hp1 ", 1, crate::rules::stash::ItemPlace::Stash);
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    assert!(r.log_lines(P)[1].contains("1/3"));
}

#[test]
fn an_item_reward_waits_for_room_and_the_player_is_told_once() {
    let r = rules(FETCH);
    let mut h = Host::new();
    joined(&r, &mut h);
    let mut bag = Bag::with(&[(b"hp1 ", 3)], 0);
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    assert!(h.said.iter().any(|l| l == "Quest complete: Fetch"));
    for _ in 0..3 {
        r.item_pass(&mut bag);
        r.periodic(&mut h);
    }
    let told = |h: &Host| h.said.iter().filter(|l| l.contains("Make room")).count();
    assert_eq!((bag.made.len(), told(&h)), (0, 1));
    bag.room = 1;
    r.item_pass(&mut bag);
    assert_eq!(bag.made.len(), 1);
    bag.room = 4;
    r.item_pass(&mut bag);
    assert_eq!(bag.made, [*b"mp1 ", *b"mp1 "]);
    // Nothing is left to give.
    r.item_pass(&mut bag);
    assert_eq!(bag.made.len(), 2);
}

#[test]
fn an_item_the_game_does_not_know_is_reported_and_dropped() {
    let r = rules(FETCH);
    let mut h = Host::new();
    joined(&r, &mut h);
    let mut bag = Bag::with(&[(b"hp1 ", 3)], 4);
    bag.unknown = true;
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    let said = h
        .said
        .iter()
        .filter(|l| l.contains("`mp1` is not in this game"))
        .count();
    assert_eq!(said, 1);
}

#[test]
fn a_player_who_left_gets_no_gift() {
    let r = rules(FETCH);
    let mut h = Host::new();
    joined(&r, &mut h);
    let mut bag = Bag::with(&[(b"hp1 ", 3)], 4);
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    r.leave(P);
    r.item_pass(&mut bag);
    assert!(bag.made.is_empty());
}

#[test]
fn rules_without_item_blocks_ask_nothing_of_the_items() {
    let r = rules(RATS);
    assert!(!r.wants_items());
    let mut bag = Bag::with(&[(b"hp1 ", 3)], 4);
    r.item_pass(&mut bag);
    assert!(bag.made.is_empty());
}

#[test]
fn the_item_blocks_are_checked() {
    let item = |objective: &str, reward: &str| {
        format!(
            "[[quest]]\nid = 1\nname = \"Q\"\n[[quest.objective]]\nkind = \"enter\"\nlevel = 1\n\
             {objective}{reward}"
        )
    };
    let collect = |body: &str| {
        item(
            &format!("[[quest.objective]]\nkind = \"collect\"\n{body}\n"),
            "",
        )
    };
    assert!(QuestRules::from_toml(&collect("item = \"hp1\"\ncount = 2")).is_ok());
    assert!(err(&collect("item = \"\"")).contains("item"));
    assert!(err(&collect("item = \"toolong\"")).contains("item"));
    assert!(err(&collect("item = \"hp1\"\ncount = 0")).contains("count"));
    let give = |body: &str| item("", &format!("[[quest.reward]]\nkind = \"item\"\n{body}\n"));
    assert!(QuestRules::from_toml(&give("item = \"mp1\"")).is_ok());
    assert!(err(&give("item = \"mp1\"\ncount = 21")).contains("count"));
    assert!(err(&give("item = \"mp 1\"")).contains("item"));
}

fn var(h: &Host, name: &str) -> Option<i32> {
    h.vars
        .iter()
        .rev()
        .find(|(n, _)| n == name)
        .map(|(_, v)| *v)
}

#[test]
fn the_log_is_published_to_the_panels_when_it_changes() {
    let r = rules(&RATS.replace(
        "giver = 7
",
        "",
    ));
    let mut h = Host::new();
    // Nothing is published for a quest not begun.
    assert_eq!(variable_name(1, 0), "quests/q1");
    assert_eq!(variable_name(1, 1), "quests/q1o0");
    joined(&r, &mut h);
    assert_eq!(var(&h, "quests/q1"), Some(1));
    assert_eq!(var(&h, "quests/q1o0"), Some(0));
    assert_eq!(var(&h, "quests/q2"), None);
    let sent = h.vars.len();
    // Nothing changed: nothing is sent again.
    r.periodic(&mut h);
    assert_eq!(h.vars.len(), sent);
    let rat = h.monster(2, 12);
    for _ in 0..2 {
        r.happened(&mut h, P, Happened::Killed(12));
    }
    assert_eq!(var(&h, "quests/q1o0"), Some(2));
    assert_eq!(var(&h, "quests/q1"), Some(1));
    let _ = rat;
    r.happened(&mut h, P, Happened::Killed(12));
    // The first quest is done and opens the second.
    assert_eq!(var(&h, "quests/q1"), Some(2));
    assert_eq!(var(&h, "quests/q1o0"), Some(3));
    assert_eq!(var(&h, "quests/q2"), Some(1));
    assert_eq!(var(&h, "quests/q2o0"), Some(0));
}

#[test]
fn the_log_is_published_again_when_a_character_loads() {
    let r = rules(&RATS.replace(
        "giver = 7
",
        "",
    ));
    let mut h = Host::new();
    joined(&r, &mut h);
    r.happened(&mut h, P, Happened::Killed(12));
    let saved = r.save(P);
    r.leave(P);
    let mut again = Host::new();
    r.load(P, &saved);
    r.happened(&mut again, P, Happened::Look);
    assert_eq!(var(&again, "quests/q1o0"), Some(1));
    assert_eq!(var(&again, "quests/q1"), Some(1));
}

#[test]
fn no_more_than_the_budget_of_variables_is_published() {
    let mut text = String::new();
    for id in 1..=60u16 {
        text.push_str(&format!(
            "[[quest]]
id = {id}
name = \"Q{id}\"
             [[quest.objective]]
kind = \"enter\"
level = 1
             [[quest.objective]]
kind = \"enter\"
level = 2
             [[quest.objective]]
kind = \"enter\"
level = 3
"
        ));
    }
    let r = rules(&text);
    let mut h = Host::new();
    joined(&r, &mut h);
    let names: std::collections::BTreeSet<_> = h.vars.iter().map(|(n, _)| n.clone()).collect();
    assert_eq!(names.len(), MAX_VARIABLES);
}

#[test]
fn saved_collect_progress_survives_join_until_the_inventory_has_been_scanned() {
    let r = rules(FETCH);
    let mut h = Host::new();
    joined(&r, &mut h);
    let mut bag = Bag::with(&[(b"hp1 ", 2)], 5);
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    let saved = r.save(P);
    let again = rules(FETCH);
    again.load(P, &saved);
    again.happened(&mut h, P, Happened::Look);
    again.periodic(&mut h);
    assert_eq!(again.save(P), saved);
    assert!(again.log_lines(P)[1].contains("2/3"));
    // Known empty inventory really does remove unfinished collect progress.
    again.item_pass(&mut Bag::default());
    again.periodic(&mut h);
    assert!(again.log_lines(P)[1].contains("0/3"));
    // Reloading in the same runtime also invalidates its old inventory cache.
    again.load(P, &saved);
    again.happened(&mut h, P, Happened::Look);
    assert_eq!(again.save(P), saved);
}

#[test]
fn quest_item_audit_records_only_successful_rewards_and_consumption() {
    let r = rules(&FETCH.replace("count = 3", "count = 3\nconsume = true"));
    let mut h = Host::new();
    joined(&r, &mut h);
    let mut bag = Bag::with(&[(b"hp1 ", 5)], 1);
    r.item_pass(&mut bag);
    r.periodic(&mut h);
    assert!(r.item_balance().is_empty());
    // One slot restores the remainder of the stack. Rewards wait for room.
    r.item_pass(&mut bag);
    assert_eq!(r.item_balance().get(b"hp1 "), Some(&-3));
    assert_eq!(r.item_balance().get(b"mp1 "), None);
    bag.room = 1;
    r.item_pass(&mut bag);
    assert_eq!(r.item_balance().get(b"mp1 "), Some(&1));
    bag.unknown = true;
    r.item_pass(&mut bag);
    assert_eq!(r.item_balance().get(b"mp1 "), Some(&1));
    let copied = r.clone();
    assert!(
        copied.item_balance().is_empty(),
        "another game starts a fresh ledger"
    );
}
