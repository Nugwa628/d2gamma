//! Save seams on the synthetic playable install: a mod game's own sections
//! in a character save (`SaveHooks`), the stats the stat table marks saved
//! (with their params), the item flags a save and a reload leave, and a
//! new character's waypoint records.
//! The install gets one stat past the fixture's own, marked saved and keyed
//! by a param.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use d2_formats::d2s::{self, D2s, ItemEntry, ModSection, StatEntry};
use d2_proto::dev::{DevCommand, Quality, Target};
use d2_server::adapters::character::LoadContext;
use d2_server::adapters::ProtoSizes;
use d2_server::dev::{DevCatalog, DevServer};
use d2_server::game::character_save::{apply_live, base_save, read_live, Live};
use d2_server::game::{create_request_for, new_character, Character, GameBuilder, GameData, Sim};
use d2_server::host::Host;
use d2_server::seams::{ClientId, Clock, MessageSink, SessionHandler};
use d2_sim::rules::{EventHost, ModRules, Ruleset, SaveHooks};
use d2_sim::units::UnitId;
use d2_sim::world::waypoints::WaypointRecord;

/// The install, and the id of its extra saved stat.
fn data() -> &'static (GameData, u16) {
    static DATA: OnceLock<(GameData, u16)> = OnceLock::new();
    DATA.get_or_init(|| {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("mod-saves-{}", std::process::id()));
        let mut synth = test_fixtures::play::playable();
        let id = synth.tables.files["itemstatcost.txt"].rows.len() as u16;
        synth.tables.row(
            "itemstatcost",
            &[
                ("stat", "counters"),
                ("send bits", "32"),
                ("save bits", "32"),
                ("csvbits", "32"),
                ("csvparam", "16"),
                ("saved", "1"),
            ],
        );
        synth.tables.row(
            "difficultylevels",
            &[("MonsterSkillBonus", "27"), ("ResistPenalty", "-150")],
        );
        // A sixth town uses another instance of the generated town preset.
        // Its map bytes are synthetic and shared with the fixture's first town.
        let levels = synth.tables.files.get_mut("levels.txt").unwrap();
        let column = |name: &str| {
            levels
                .columns
                .iter()
                .position(|c| c.eq_ignore_ascii_case(name))
                .unwrap()
        };
        let level_id = column("Id");
        let mut town = levels
            .rows
            .iter()
            .find(|r| r[level_id] == "1")
            .unwrap()
            .clone();
        for (name, value) in [
            ("Id", "137"),
            ("Act", "5"),
            ("Vis0", "0"),
            ("Warp0", "-1"),
            ("Waypoint", "111"),
        ] {
            town[column(name)] = value.into();
        }
        levels
            .rows
            .resize_with(138, || vec![String::new(); levels.columns.len()]);
        levels.rows[137] = town;
        let presets = synth.tables.files.get_mut("lvlprest.txt").unwrap();
        let level = presets
            .columns
            .iter()
            .position(|c| c.eq_ignore_ascii_case("LevelId"))
            .unwrap();
        let def = presets
            .columns
            .iter()
            .position(|c| c.eq_ignore_ascii_case("Def"))
            .unwrap();
        let mut town = presets
            .rows
            .iter()
            .find(|r| r[level] == "1")
            .unwrap()
            .clone();
        town[level] = "137".into();
        town[def] = presets.rows.len().to_string();
        presets.rows.push(town);
        test_fixtures::install::write_text_install(&dir, &synth).unwrap();
        test_fixtures::install::compile_and_pack(&dir, &synth).unwrap();
        let data = GameData::open_with(
            &d2_server::game::GameSource::Archives(dir),
            d2_data::bin::DataRules::Mod,
        )
        .expect("the playable mod install loads");
        (data, id)
    })
}

fn counters() -> u16 {
    data().1
}

#[test]
fn acts_single_registry_runs_and_saves_with_one_act() {
    use d2_server::game::{CharacterCodec, ModCodec};
    use d2_sim::rules::{DataMod, DataModFiles};
    let rules = DataMod::from_files("one", &DataModFiles {
        acts: Some(("acts.toml", "version=1\n[[acts]]\nname='Home'\nstart=1\nend=40\ntown=1\nwaypoints=[{id=111,level=1}]")),
        difficulties: Some(("difficulties.toml", "version=1\nnames=['A','B','C','D']")),
        ..Default::default()
    }).unwrap().into_ruleset();
    let mut g = Game::start(
        new_character("amazon", "Home").unwrap().with_difficulty(3),
        rules.clone(),
    );
    assert_eq!(g.host.game.game.lists.act_count(), 1);
    for _ in 0..20 {
        g.frame();
    }
    let save = g.save();
    let records =
        d2_formats::d2m::progress::read(save.body.as_ref().unwrap(), save.header.towns).unwrap();
    assert_eq!(records.len(), 4);
    assert_eq!(records[3].town, 0x80);
    for r in &records {
        let wp = WaypointRecord::load_mod_copy(&r.waypoints).unwrap();
        assert!(wp.test(111).unwrap());
        assert!(!wp.test(0).unwrap());
    }
    let codec = ModCodec(rules.clone());
    let GameData::Live(data) = &data().0;
    let back = codec
        .read(data, &codec.write(&save, &data.save).unwrap(), 3)
        .unwrap();
    let mut g = Game::start(loaded(&back).with_difficulty(3), rules);
    let again = g.save();
    assert_eq!(
        d2_formats::d2m::progress::read(again.body.as_ref().unwrap(), again.header.towns).unwrap(),
        records
    );
}

struct NoSession;

impl SessionHandler for NoSession {
    fn system_message(&mut self, _: ClientId, _: &[u8], _: usize, _: &mut dyn MessageSink) {}
}

#[derive(Default)]
struct Steps(u32);

impl Clock for Steps {
    fn now_ms(&mut self) -> u32 {
        self.0
    }
}

/// One game with one player (client 0).
struct Game {
    host: Host<Sim, ProtoSizes, NoSession, Steps>,
    character: Character,
    dev: DevServer,
}

impl Game {
    fn start(character: Character, rules: Ruleset) -> Game {
        let sim = GameBuilder::new(data().0.clone())
            .character(character.clone())
            .rules(rules)
            .build()
            .expect("the game builds");
        let mut host = Host::new(sim, ProtoSizes, NoSession, Steps(1000));
        host.connect(0);
        let GameData::Live(d) = &data().0;
        let dev = DevServer::new(DevCatalog::from_live(d, |_| String::new()));
        let mut g = Game {
            host,
            character,
            dev,
        };
        let req = create_request_for(&g.character, 800, 600);
        g.host.send_system(0, &req.encode()).expect("queued");
        let mut asked = false;
        for _ in 0..200 {
            let got = g.frame();
            if !asked && got.iter().any(|m| m.first() == Some(&0x02)) {
                g.host.send_system(0, &[0x6B]).expect("queued");
                asked = true;
            }
            if asked && g.host.game.player_of(0).is_some() {
                for _ in 0..4 {
                    g.frame();
                }
                return g;
            }
        }
        panic!("the player did not join");
    }

    fn frame(&mut self) -> Vec<Vec<u8>> {
        self.host.clock.0 += 40;
        self.host.frame().expect("frame");
        self.host.receive(0)
    }

    fn player(&self) -> UnitId {
        self.host.game.player_of(0).expect("player")
    }

    fn give(&mut self, code: &str) {
        let p = self.host.game.player_of(0);
        let r = self.dev.apply(
            &mut self.host.game,
            p,
            &DevCommand::Item {
                item: Target::Name(code.into()),
                quality: Quality::Normal,
                ilvl: Some(5),
                ground: false,
            },
        );
        assert!(r.ok, "{}", r.text);
    }

    fn set_stat(&mut self, id: u16, layer: u16, value: i32) {
        let p = self.player();
        let sim = &mut self.host.game;
        sim.events.action.with(&mut sim.game, |_, v| {
            v.stats.unit_set(&mut *v.h, p, id, value, layer)
        });
    }

    fn base_stat(&mut self, id: u16, layer: u16) -> i32 {
        let p = self.player();
        let sim = &mut self.host.game;
        sim.events
            .action
            .with(&mut sim.game, |_, v| v.stats.unit_base(p, id, layer))
    }

    fn live(&mut self) -> Live {
        let p = self.player();
        read_live(&mut self.host.game, p)
    }

    fn save(&mut self) -> D2s {
        let live = self.live();
        apply_live(&base_save(&self.character), &live, 1_790_000_000)
    }
}

fn loaded(save: &D2s) -> Character {
    Character::Save(
        Box::new(save.clone()),
        LoadContext {
            difficulty: 0,
            map_seed_applies: false,
        },
    )
}

fn sections(save: &D2s) -> &[ModSection] {
    &save.body.as_ref().expect("a body").mod_sections
}

/// Test save hooks: the mod's own per-player value (behind its lock) is
/// saved as section `CNTR`; a load hands it back, and every load is
/// logged.
#[derive(Debug, Default)]
struct Hooks {
    value: Mutex<u32>,
    loads: Mutex<Vec<Vec<ModSection>>>,
}

impl SaveHooks for Hooks {
    fn save_sections(&self, _: &mut dyn EventHost, _: UnitId) -> Vec<ModSection> {
        let v = *self.value.lock().unwrap();
        vec![ModSection::new(*b"CNTR", v.to_le_bytes().to_vec())]
    }
    fn load_sections(&self, _: &mut dyn EventHost, _: UnitId, sections: &[ModSection]) {
        if let Some(s) = d2s::mod_sections::find(sections, b"CNTR") {
            *self.value.lock().unwrap() = u32::from_le_bytes(s.data[..4].try_into().unwrap());
        }
        self.loads.lock().unwrap().push(sections.to_vec());
    }
}

#[derive(Debug)]
struct TestGame(Arc<Hooks>);

impl ModRules for TestGame {
    fn name(&self) -> &str {
        "test-saves"
    }
    fn saves(&self) -> Option<&dyn SaveHooks> {
        Some(self.0.as_ref())
    }
}

fn modded() -> (Arc<Hooks>, Ruleset) {
    let hooks = Arc::new(Hooks::default());
    (hooks.clone(), Ruleset::Mod(Arc::new(TestGame(hooks))))
}

/// A mod game's sections go into the save it makes, come back to its hooks
/// when the character joins the next game, and replace the loaded ones at
/// the next save. A `.d2s` file has no place for them.
#[test]
fn a_mod_games_sections_are_saved_and_loaded_back() {
    let (hooks, rules) = modded();
    let mut g = Game::start(new_character("amazon", "Keeper").expect("new"), rules);
    // A new character's join hands the hooks no sections.
    assert_eq!(*hooks.loads.lock().unwrap(), [Vec::<ModSection>::new()]);
    *hooks.value.lock().unwrap() = 7;
    let first = g.save();
    let want = [ModSection::new(*b"CNTR", 7u32.to_le_bytes().to_vec())];
    assert_eq!(sections(&first), want);
    let GameData::Live(d) = &data().0;
    assert!(d2s::write(&first, &d.save).is_err());

    // The next game's hooks start empty and get the saved value back.
    let (hooks, rules) = modded();
    let mut g = Game::start(loaded(&first), rules);
    assert_eq!(*hooks.loads.lock().unwrap(), [want.to_vec()]);
    assert_eq!(*hooks.value.lock().unwrap(), 7);
    *hooks.value.lock().unwrap() = 9;
    let second = g.save();
    assert_eq!(
        sections(&second),
        [ModSection::new(*b"CNTR", 9u32.to_le_bytes().to_vec())]
    );
}

/// D2RUST's own rules ask nothing: a new character's save has no sections
/// and is a `.d2s` file, and a loaded save's sections pass through.
#[test]
fn without_save_hooks_a_save_keeps_its_sections() {
    let mut g = Game::start(
        new_character("amazon", "Plain").expect("new"),
        Ruleset::Original,
    );
    assert_eq!(g.live().mod_sections, None);
    let mut first = g.save();
    assert!(sections(&first).is_empty());
    let GameData::Live(d) = &data().0;
    assert!(d2s::write(&first, &d.save).is_ok());
    let kept = vec![ModSection::new(*b"KEEP", vec![1, 2, 3])];
    first.body.as_mut().unwrap().mod_sections = kept.clone();
    let mut g = Game::start(loaded(&first), Ruleset::Original);
    assert_eq!(sections(&g.save()), kept);
}

/// A new character joins with its waypoint records, each knowing only the
/// town's waypoint (bit 0), and saves them before any waypoint is used.
#[test]
fn a_new_character_joins_knowing_the_town_waypoint() {
    let mut g = Game::start(
        new_character("amazon", "Walker").expect("new"),
        Ruleset::Original,
    );
    let town = WaypointRecord::allocate().0;
    assert_eq!(g.live().extra.waypoints, Some([town; 3]));
}

fn entries(save: &D2s, id: u16) -> Vec<StatEntry> {
    save.body
        .as_ref()
        .expect("a body")
        .stats
        .entries()
        .into_iter()
        .filter(|e| e.id == id)
        .collect()
}

/// A stat the table marks saved is saved at every param the player holds,
/// loads back into the next game, and a param the player no longer holds
/// leaves the save.
#[test]
fn every_saved_stat_is_saved_with_its_params() {
    let id = counters();
    let mut g = Game::start(
        new_character("amazon", "Counter").expect("new"),
        Ruleset::Original,
    );
    g.set_stat(id, 0, 5);
    g.set_stat(id, 3, 300);
    g.set_stat(id, 0x2004, -1);
    let first = g.save();
    let e = |layer, value| StatEntry { id, layer, value };
    assert_eq!(entries(&first, id), [e(0, 5), e(3, 300), e(0x2004, -1)]);
    // The 16 base stats are saved as before.
    let level = StatEntry {
        id: 12,
        layer: 0,
        value: 1,
    };
    assert_eq!(entries(&first, 12), [level]);

    let mut g = Game::start(loaded(&first), Ruleset::Original);
    assert_eq!(g.base_stat(id, 3), 300);
    assert_eq!(g.base_stat(id, 0x2004), -1);
    g.set_stat(id, 3, 0);
    g.set_stat(id, 9, 90);
    let second = apply_live(&first, &g.live(), 1);
    assert_eq!(entries(&second, id), [e(0, 5), e(9, 90), e(0x2004, -1)]);
}

#[test]
fn named_stats_survive_the_live_character_mod_codec_and_reload() {
    use d2_server::game::{CharacterCodec, ModCodec};
    use d2_sim::rules::{DataMod, DataModFiles};
    let rules = DataMod::from_files(
        "wide",
        &DataModFiles {
            stats: Some((
                "stats.toml",
                "version = 1\n[[stat]]\nname = 'focus'\nid = 40000\n",
            )),
            ..DataModFiles::default()
        },
    )
    .unwrap()
    .into_ruleset();
    let mut g = Game::start(new_character("amazon", "Wide").unwrap(), rules.clone());
    g.host.set_wire(d2_proto::envelope::Wire::Mod);
    g.set_stat(40000, 0, i32::MAX);
    g.set_stat(40000, 65535, i32::MIN);
    let messages = g.frame();
    assert!(messages
        .iter()
        .flat_map(|b| b.windows(d2_proto::wide_stat::SIZE))
        .filter_map(d2_proto::wide_stat::decode)
        .any(|m| m.stat == 40000 && m.value == i32::MAX));
    g.set_stat(40000, 0, 0);
    assert!(g
        .frame()
        .iter()
        .flat_map(|b| b.windows(d2_proto::wide_stat::SIZE))
        .filter_map(d2_proto::wide_stat::decode)
        .any(|m| m.stat == 40000 && m.value == 0));
    assert!(!g
        .frame()
        .iter()
        .flat_map(|b| b.windows(d2_proto::wide_stat::SIZE))
        .filter_map(d2_proto::wide_stat::decode)
        .any(|m| m.stat == 40000));
    g.set_stat(40000, 0, i32::MAX);
    let first = g.save();
    let codec = ModCodec(rules.clone());
    let GameData::Live(data) = &data().0;
    let bytes = codec.write(&first, &data.save).unwrap();
    assert_eq!(codec.extension(), "d2m");
    assert!(d2s::read(&bytes, &d2s::ReadOptions::default(), &data.save).is_err());
    let back = codec.read(data, &bytes, 0).unwrap();
    assert_eq!(entries(&back, 40000), entries(&first, 40000));
    let mut reloaded = Game::start(loaded(&back), rules);
    assert_eq!(reloaded.base_stat(40000, 0), i32::MAX);
    assert_eq!(reloaded.base_stat(40000, 65535), i32::MIN);
}

/// The JM flags of every record of the player's items.
fn flags(save: &D2s) -> Vec<u32> {
    let items: &[ItemEntry] = &save.body.as_ref().expect("a body").items;
    items
        .iter()
        .map(|i| i.record_flags(0).expect("a JM record"))
        .collect()
}

/// An item made straight into the inventory keeps "in a store" (0x2000)
/// until a load: the first save of a new item has it, and a load clears
/// it on every item ([`d2s::item_flags_on_load`]), so the saves after the
/// reload hold the same records without it and stay equal.
#[test]
fn the_in_store_flag_goes_at_the_first_reload() {
    let mut g = Game::start(
        new_character("amazon", "Flags").expect("new"),
        Ruleset::Original,
    );
    g.give("sb1");
    g.give("sb1");
    let first = g.save();
    assert!(flags(&first).len() >= 2);
    let mut g = Game::start(loaded(&first), Ruleset::Original);
    let second = g.save();
    assert_eq!(
        flags(&second),
        flags(&first)
            .iter()
            .map(|f| f & !d2s::ITEM_FLAG_INSTORE)
            .collect::<Vec<_>>()
    );
    let mut g = Game::start(loaded(&second), Ruleset::Original);
    let third = g.save();
    assert_eq!(flags(&third), flags(&second));
    // Given items had it at their first save.
    assert!(flags(&first)
        .iter()
        .rev()
        .take(2)
        .all(|f| f & d2s::ITEM_FLAG_INSTORE != 0));
}

#[test]
fn fourth_difficulty_progress_survives_mod_codec_and_rejoin_independently() {
    use d2_formats::d2m::progress;
    use d2_server::game::{CharacterCodec, ModCodec};
    use d2_sim::rules::{DataMod, DataModFiles};
    let rules = DataMod::from_files(
        "four",
        &DataModFiles {
            difficulties: Some((
                "difficulties.toml",
                "version=1\nnames=['Dawn','Day','Dusk','Night']",
            )),
            ..DataModFiles::default()
        },
    )
    .unwrap()
    .into_ruleset();
    assert!(GameBuilder::new(data().0.clone())
        .character(
            new_character("amazon", "Invalid")
                .unwrap()
                .with_difficulty(3)
        )
        .build()
        .is_err());
    assert_eq!(
        d2_server::game::parse_difficulty_for("Night", &rules),
        Some(3)
    );
    assert_eq!(d2_server::game::parse_difficulty_for("3", &rules), Some(3));
    assert_eq!(d2_server::game::parse_difficulty_for("4", &rules), None);
    assert_eq!(
        d2_server::game::parse_difficulty_for("3", &Ruleset::Original),
        None
    );
    let mut g = Game::start(
        new_character("amazon", "Fourth")
            .unwrap()
            .with_difficulty(3),
        rules.clone(),
    );
    let p = g.player();
    assert_eq!(g.host.game.events.action.hooks().ai_info.difficulty, 3);
    assert!(g
        .host
        .game
        .events
        .action
        .hooks()
        .drlg
        .dungeon
        .acts
        .iter()
        .flatten()
        .all(|d| d.difficulty == 2));
    let q = g.host.game.world.rest.quests.get_mut(&p).unwrap();
    assert_eq!(q.flags.len(), 4);
    q.flags[3].set(0, 2);
    q.hear(3, 148);
    q.intro[3].insert(148);
    g.host
        .game
        .events
        .action
        .hooks()
        .waypoints
        .get_mut(&p)
        .unwrap()
        .get_mut(3)
        .set(7)
        .unwrap();
    let save = g.save();
    let before = progress::read(save.body.as_ref().unwrap(), save.header.towns).unwrap();
    assert_eq!(before.len(), 4);
    assert_ne!(before[3].quests, before[2].quests);
    assert_ne!(before[3].npc_a, before[2].npc_a);
    assert_ne!(before[3].npc_b, before[2].npc_b);
    assert_ne!(before[3].waypoints, before[2].waypoints);
    assert_ne!(before[3].town & 0x80, 0);
    let GameData::Live(data) = &data().0;
    let codec = ModCodec(rules.clone());
    let bytes = codec.write(&save, &data.save).unwrap();
    let back = codec.read(data, &bytes, 3).unwrap();
    assert!(codec.read(data, &bytes, 4).is_err());
    let mut g = Game::start(loaded(&back).with_difficulty(3), rules);
    let p = g.player();
    let q = &g.host.game.world.rest.quests[&p];
    assert!(q.flags[3].get(0, 2));
    assert!(!q.flags[2].get(0, 2));
    assert!(q.heard(3, 148));
    assert!(q.intro[3].contains(&148));
    assert!(g.host.game.events.action.hooks().waypoints[&p].0[3]
        .test(7)
        .unwrap());
    let again = g.save();
    assert_eq!(
        progress::read(again.body.as_ref().unwrap(), again.header.towns).unwrap(),
        before
    );
}

#[test]
fn difficulty_names_require_matching_rows_and_keep_base_count_fixed() {
    use d2_sim::rules::{DataMod, DataModFiles};
    let rules = DataMod::from_files(
        "too-many",
        &DataModFiles {
            difficulties: Some((
                "difficulties.toml",
                "version=1\nnames=['A','B','C','D','E']",
            )),
            ..Default::default()
        },
    )
    .unwrap()
    .into_ruleset();
    assert!(GameBuilder::new(data().0.clone())
        .rules(rules)
        .build()
        .is_err());
    // The same fixture has four rows; D2Gamma still rejects the fourth id.
    assert!(GameBuilder::new(data().0.clone())
        .character(
            new_character("amazon", "Fourth")
                .unwrap()
                .with_difficulty(3)
        )
        .build()
        .is_err());
}

#[test]
fn sixth_act_town_rejoins_from_mod_progress_and_reaches_its_waypoint() {
    use d2_formats::d2m::progress;
    use d2_server::adapters::handlers::world::{NpcCall, WorldHost};
    use d2_server::game::{CharacterCodec, ModCodec};
    use d2_sim::rules::{DataMod, DataModFiles};
    use d2_sim::world::npc::{NpcControl, NpcVendors, NpcWorld};
    struct Travel(UnitId);
    impl NpcCall for Travel {
        type Out = (bool, bool);
        fn call<W: NpcWorld + NpcVendors>(self, _: &mut NpcControl, w: &mut W) -> Self::Out {
            (
                w.data_act_travel(self.0, 155, 0),
                w.data_act_travel(self.0, 155, 9),
            )
        }
    }
    let mut text = String::from("version=1\n");
    for (i, (start, end)) in [
        (1, 40),
        (40, 75),
        (75, 103),
        (103, 109),
        (109, 137),
        (137, 138),
    ]
    .into_iter()
    .enumerate()
    {
        use std::fmt::Write;
        writeln!(text,"[[acts]]\nname='Act {}'\nstart={start}\nend={end}\ntown={start}\nwaypoints=[{{id={i},level={start}}}]",i+1).unwrap();
        if i == 0 {
            writeln!(text, "travel=[{{npc=155,action=0,to=137,text=5000}}]").unwrap();
        }
    }
    let rules = DataMod::from_files(
        "six",
        &DataModFiles {
            acts: Some(("acts.toml", &text)),
            ..Default::default()
        },
    )
    .unwrap()
    .into_ruleset();
    let mut g = Game::start(new_character("amazon", "Beyond").unwrap(), rules.clone());
    assert_eq!(g.host.game.game.lists.act_count(), 6);
    let p = g.player();
    let s = &mut g.host.game;
    assert_eq!(
        s.world.npc(&mut s.game, &mut s.events, Travel(p)),
        Some((true, false))
    );
    for _ in 0..20 {
        g.frame();
    }
    let room = g.host.game.game.lists.unit(p).unwrap().room().unwrap();
    assert_eq!(g.host.game.game.lists.room(room).unwrap().act, 5);
    let save = g.save();
    let progress = progress::read(save.body.as_ref().unwrap(), save.header.towns).unwrap();
    assert_eq!(progress[0].town, 0x85);
    let GameData::Live(data) = &data().0;
    let codec = ModCodec(rules.clone());
    let bytes = codec.write(&save, &data.save).unwrap();
    let back = codec.read(data, &bytes, 0).unwrap();
    let mut g = Game::start(loaded(&back), rules);
    let p = g.player();
    let room = g.host.game.game.lists.unit(p).unwrap().room().unwrap();
    assert_eq!(g.host.game.game.lists.room(room).unwrap().act, 5);
    let h = g.host.game.events.action.hooks();
    assert_eq!(h.drlg.level_id(&g.host.game.game, room), Some(137));
    assert_eq!(
        h.waypoint_init.as_ref().unwrap().map.index_of_level(137),
        Some(5)
    );
    let again = g.save();
    let progress = progress::read(again.body.as_ref().unwrap(), again.header.towns).unwrap();
    assert_eq!(progress[0].town, 0x85);
}

#[test]
fn character_copy_transcodes_wide_items_in_inventory_corpses_hireling_and_golem() {
    use d2_server::game::{CharacterCodec, ModCodec};
    use d2_sim::items::bitstream::{read::read_save_entry, write_save};
    let mut game = Game::start(
        new_character("amazon", "CopyHero").unwrap(),
        Ruleset::Original,
    );
    game.give("sb1");
    let mut save = game.save();
    let GameData::Live(live) = &data().0;
    let body = save.body.as_mut().unwrap();
    let entry = body.items.last().unwrap().clone();
    let mut parent = read_save_entry(&entry.bytes, &live.save.items)
        .unwrap()
        .item
        .item;
    let mut child = parent.clone();
    child.mode = 6;
    child.page = 0xff;
    child.children.clear();
    child.filled = 0;
    parent.flags |= 0x800;
    parent.base_sockets = 1;
    parent.filled = 1;
    parent.children = vec![child];
    let entry = ItemEntry {
        bytes: write_save(&parent, &live.save.items).unwrap().0,
    };
    body.items = vec![entry.clone()];
    body.corpses = vec![d2s::Corpse {
        items: vec![entry.clone()],
        ..Default::default()
    }];
    body.hireling_items = Some(Some(vec![entry.clone()]));
    body.golem = Some(d2s::Golem {
        flag: 1,
        item: Some(entry.clone()),
    });
    // Make the hireling present so the format's conditional list is read.
    save.header.hireling.seed = 1;
    save.header.hireling.id = live
        .save
        .hirelings
        .rows
        .iter()
        .map(|r| r.id)
        .find(|&id| live.save.hirelings.row_at(true, id, 1).is_some())
        .unwrap() as u16;
    let source = d2s::write(&save, &live.save).unwrap();
    let before = source.clone();
    let converted = d2_server::game::copy_d2gamma_character(&source, &live.save).unwrap();
    assert_eq!(source, before);
    assert!(d2s::read(
        &converted,
        &d2s::ReadOptions {
            expansion: true,
            game: None
        },
        &live.save
    )
    .is_err());
    let rules = d2_sim::rules::DataMod::new("copy").into_ruleset();
    let copied = ModCodec(rules.clone()).read(live, &converted, 0).unwrap();
    assert_eq!(copied.header.name, save.header.name);
    let body = copied.body.unwrap();
    let entries = [
        &body.items[0],
        &body.corpses[0].items[0],
        &body.hireling_items.as_ref().unwrap().as_ref().unwrap()[0],
        body.golem.as_ref().unwrap().item.as_ref().unwrap(),
    ];
    let mut wide = live.save.items.clone();
    wide.rules = rules;
    let expected = read_save_entry(&entry.bytes, &live.save.items).unwrap();
    for item in entries {
        let decoded = read_save_entry(&item.bytes, &wide).unwrap();
        assert_eq!(decoded.item.item, expected.item.item);
        assert_eq!(decoded.children.len(), 1);
        assert_eq!(
            decoded.children[0].item.item,
            expected.children[0].item.item
        );
    }
}
