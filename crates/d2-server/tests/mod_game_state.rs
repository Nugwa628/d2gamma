//! Per-game mod rules on the synthetic playable install: the game builder
//! asks a mod game's rules for the game's own rules object
//! (`ModRules::for_game`) once per game, every part of the game runs that
//! object, and the state a game's hooks keep stays in that game. Also the
//! session's mod hooks: the join gate and the leaving player.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use d2_server::adapters::ProtoSizes;
use d2_server::game::{create_request_for, Character, GameBuilder, GameData, Sim};
use d2_server::host::Host;
use d2_server::seams::{ClientId, Clock, MessageSink, SessionHandler};
use d2_sim::rules::{EventHooks, EventHost, ModRules, NewGame, Ruleset};
use d2_sim::units::{UnitId, UnitType};

fn data() -> GameData {
    static DATA: OnceLock<GameData> = OnceLock::new();
    DATA.get_or_init(|| {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("mod-game-state-{}", std::process::id()));
        test_fixtures::install::build(&dir, &test_fixtures::play::playable())
            .unwrap_or_else(|e| panic!("{e}"));
        GameData::select(Some(&dir)).expect("the playable install loads")
    })
    .clone()
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

/// A mod game counting the players that joined each game: its shared
/// object makes one object per game and keeps them for the test.
#[derive(Debug, Default)]
struct Counting {
    joined: AtomicU32,
    made: Mutex<Vec<(NewGame, Arc<Counting>)>>,
}

impl ModRules for Counting {
    fn name(&self) -> &str {
        "counting"
    }
    fn events(&self) -> Option<&dyn EventHooks> {
        Some(self)
    }
    fn for_game(&self, game: &NewGame) -> Option<Arc<dyn ModRules>> {
        let own = Arc::new(Counting::default());
        self.made.lock().unwrap().push((*game, own.clone()));
        Some(own)
    }
}

impl EventHooks for Counting {
    fn player_joined(&self, _: &mut dyn EventHost, _: UnitId) {
        self.joined.fetch_add(1, Ordering::SeqCst);
    }
}

/// A game of `seed` under `rules` with one joined player.
fn joined(rules: Ruleset, seed: u32) -> Host<Sim, ProtoSizes, NoSession, Steps> {
    let mut host = game(rules, seed);
    ask_to_join(&mut host);
    assert!(host.game.player_of(0).is_some(), "the player did not join");
    host
}

/// A game of `seed` under `rules`, client 0 connected.
fn game(rules: Ruleset, seed: u32) -> Host<Sim, ProtoSizes, NoSession, Steps> {
    let sim = GameBuilder::new(data())
        .seed(seed)
        .character(Character::New)
        .rules(rules)
        .build()
        .expect("the game builds");
    let mut host = Host::new(sim, ProtoSizes, NoSession, Steps(1000));
    host.connect(0);
    host
}

/// Client 0 creates its character and asks to join (C→S 0x6B), until it
/// is in or 200 frames passed: every message it got.
fn ask_to_join(host: &mut Host<Sim, ProtoSizes, NoSession, Steps>) -> Vec<Vec<u8>> {
    let req = create_request_for(&Character::New, 800, 600);
    host.send_system(0, &req.encode()).expect("queued");
    let (mut asked, mut all) = (false, Vec::new());
    for _ in 0..200 {
        host.clock.0 += 40;
        host.frame().expect("frame");
        let got = host.receive(0);
        if !asked && got.iter().any(|m| m.first() == Some(&0x02)) {
            host.send_system(0, &[0x6B]).expect("queued");
            asked = true;
        }
        all.extend(got);
        if asked && host.game.player_of(0).is_some() {
            break;
        }
    }
    all
}

/// The ruleset's mod rules are `want`.
fn runs(r: &Ruleset, want: &Arc<Counting>) -> bool {
    r.mod_rules().is_some_and(|m| {
        std::ptr::eq(
            m as *const dyn ModRules as *const (),
            Arc::as_ptr(want) as *const (),
        )
    })
}

#[test]
fn each_game_runs_its_own_rules_object() {
    let shared = Arc::new(Counting::default());
    let rules = Ruleset::Mod(shared.clone());
    let mut a = joined(rules.clone(), 11);
    let b = joined(rules, 12);
    let made = shared.made.lock().unwrap().clone();
    // Asked once per game, with the game's seed and difficulty.
    let facts: Vec<_> = made.iter().map(|m| (m.0.seed, m.0.difficulty)).collect();
    let difficulty = Character::New.difficulty();
    assert_eq!(facts, [(11, difficulty), (12, difficulty)]);
    let (own_a, own_b) = (&made[0].1, &made[1].1);
    // Every part of a game runs its own object: the host's rules and the
    // simulation's.
    assert!(runs(&a.game.rules, own_a) && runs(&b.game.rules, own_b));
    assert!(runs(&a.game.events.action.hooks().rules, own_a));
    // Each game's join counted in that game only.
    assert_eq!(own_a.joined.load(Ordering::SeqCst), 1);
    assert_eq!(own_b.joined.load(Ordering::SeqCst), 1);
    assert_eq!(shared.joined.load(Ordering::SeqCst), 0);
    // Frames of one game don't touch the other's state.
    for _ in 0..4 {
        a.clock.0 += 40;
        a.frame().expect("frame");
    }
    assert_eq!(own_b.joined.load(Ordering::SeqCst), 1);
}

#[test]
fn d2rusts_own_games_stay_original() {
    let mut host = joined(Ruleset::Original, 11);
    assert!(host.game.rules.is_original());
    assert!(host.game.events.action.hooks().rules.is_original());
}

/// A mod game with a join gate (refusing every join with 0x19 when
/// `refuse`) and a log of the names it was asked about, the joins and
/// the leavers.
#[derive(Debug, Default)]
struct Door {
    refuse: bool,
    names: Mutex<Vec<Vec<u8>>>,
    joined: AtomicU32,
    left: Mutex<Vec<UnitId>>,
}

impl ModRules for Door {
    fn name(&self) -> &str {
        "door"
    }
    fn events(&self) -> Option<&dyn EventHooks> {
        Some(self)
    }
}

impl EventHooks for Door {
    fn may_join(&self, _: &mut dyn EventHost, _: UnitId, name: &[u8]) -> Option<u32> {
        self.names.lock().unwrap().push(name.to_vec());
        self.refuse.then_some(0x19)
    }
    fn player_joined(&self, _: &mut dyn EventHost, _: UnitId) {
        self.joined.fetch_add(1, Ordering::SeqCst);
    }
    fn player_left(&self, _: &mut dyn EventHost, player: UnitId) {
        self.left.lock().unwrap().push(player);
    }
}

#[test]
fn a_refused_join_tells_the_client_and_leaves_no_player() {
    let door = Arc::new(Door {
        refuse: true,
        ..Door::default()
    });
    let mut host = game(Ruleset::Mod(door.clone()), 11);
    let got = ask_to_join(&mut host);
    // The load result 0x19 reaches the client; its player is gone and the
    // join hook never ran.
    assert!(got.iter().any(|m| m[..] == [0xB4, 0x19, 0, 0, 0]));
    assert!(host.game.player_of(0).is_none());
    assert!(host
        .game
        .game
        .lists
        .units_of_type(UnitType::Player)
        .is_empty());
    assert_eq!(door.joined.load(Ordering::SeqCst), 0);
    // The gate saw the character's name, without the padding.
    let names = door.names.lock().unwrap().clone();
    assert_eq!(names.len(), 1);
    assert!(!names[0].is_empty() && !names[0].contains(&0));
}

#[test]
fn a_let_in_join_and_the_players_leave() {
    let door = Arc::new(Door::default());
    let mut host = joined(Ruleset::Mod(door.clone()), 11);
    assert_eq!(door.joined.load(Ordering::SeqCst), 1);
    let p = host.game.player_of(0).expect("joined");
    assert!(door.left.lock().unwrap().is_empty());
    host.disconnect(0);
    assert_eq!(*door.left.lock().unwrap(), [p]);
}
