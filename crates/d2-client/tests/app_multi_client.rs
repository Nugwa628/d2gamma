//! Several clients in one game, in-process: the app's wired game behind
//! one `d2_server::hub::Hub`, each client on its own in-memory transport.
//!
//! The clients act as the bridge does at the start of a game: C→S 0x67
//! (each with its own class and name), then C→S 0x6B once the server's
//! 0x02 arrived. The tests check that every client gets into the game
//! with its own player, that a client can leave while the others play on,
//! and that the same script on the same seed gives the same game, byte
//! for byte, for every client (the hub reads clients in id order, then
//! each client's packets in arrival order).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use d2_client::app::single_player::{self, Sim, DEFAULT_SEED};
use d2_client::bridge::link::{LinkError, Pumped, SendQueue, Sent, ServerLink};
use d2_client::bridge::local::PendingSession;
use d2_client::bridge::motion::MonsterMotion;
use d2_client::bridge::{Bridge, UnitKey};
use d2_net::{pair, Lane, MemoryEnd, Packet, Transport};
use d2_server::adapters::ProtoSizes;
use d2_server::host::Host;
use d2_server::hub::Hub;
use d2_server::seams::{ClientId, Clock};
use d2_sim::units::lists::client_state;

use crate::app_support;

struct StepClock(Arc<AtomicU32>);

impl Clock for StepClock {
    fn now_ms(&mut self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }
}

/// The classes the clients play, in client order (class ids 0–6).
const CLASSES: [&str; 7] = [
    "amazon",
    "sorceress",
    "necromancer",
    "paladin",
    "barbarian",
    "druid",
    "assassin",
];

/// One game and its clients.
struct Game {
    hub: Hub<Sim, ProtoSizes, PendingSession, StepClock, MemoryEnd>,
    ends: Vec<MemoryEnd>,
    /// Everything each client received, in order (frame marks as `[]`
    /// with the tick flag).
    got: Vec<Vec<Vec<u8>>>,
    /// Whether each client answered 0x02 with its 0x6B yet.
    joined: Vec<bool>,
    ms: Arc<AtomicU32>,
    /// What each client received since a [`ClientLink`] last took it
    /// (no frame marks).
    fresh: Vec<Vec<Vec<u8>>>,
    /// Whether the last [`Game::step`] ran a server tick.
    ticked: bool,
}

impl Game {
    /// The game on `seed` with `n` connected clients, each having sent its
    /// 0x67.
    fn new(seed: u32, n: usize) -> Self {
        let classes: Vec<&str> = (0..n).map(|c| CLASSES[c % CLASSES.len()]).collect();
        Self::with_classes(seed, &classes)
    }

    /// [`Game::new`] with one client per entry of `classes`, each playing
    /// that class.
    fn with_classes(seed: u32, classes: &[&str]) -> Self {
        let n = classes.len();
        let data = app_support::game_data();
        let built = single_player::build(&data, seed).expect("the game builds");
        let ms = Arc::new(AtomicU32::new(1000));
        let host = Host::new(
            built.sim,
            ProtoSizes,
            PendingSession::default(),
            StepClock(ms.clone()),
        );
        let mut hub = Hub::new(host);
        let mut ends = Vec::new();
        for c in 0..n {
            let (end, server_end) = pair();
            hub.connect(c as ClientId, server_end);
            ends.push(end);
        }
        let mut g = Self {
            hub,
            ends,
            got: vec![Vec::new(); n],
            joined: vec![false; n],
            ms,
            fresh: vec![Vec::new(); n],
            ticked: false,
        };
        for (c, class) in classes.iter().enumerate() {
            let name = format!("Player{c}");
            let character = single_player::new_character(class, &name).unwrap();
            let req = single_player::create_request_for(&character);
            g.send(c, Lane::System, req.encode().to_vec());
        }
        g
    }

    fn send(&mut self, c: usize, lane: Lane, msg: Vec<u8>) {
        self.ends[c].send(Packet::Send { lane, msg }).unwrap();
    }

    /// One frame for everyone: the hub reads the clients, the clock moves
    /// one tick, the host runs its frame and delivers. A client answers
    /// the server's 0x02 with 0x6B, as the bridge does.
    fn step(&mut self) {
        self.hub.poll().unwrap();
        self.ms.fetch_add(40, Ordering::SeqCst);
        self.hub.frame().unwrap();
        self.hub.deliver_all();
        for c in 0..self.ends.len() {
            while let Ok(Some(p)) = self.ends[c].recv() {
                match p {
                    Packet::Deliver(m) => {
                        if m.first() == Some(&0x02) && !self.joined[c] {
                            self.joined[c] = true;
                            self.send(c, Lane::System, vec![0x6B]);
                        }
                        // A client a test connects later has no list yet.
                        if self.fresh.len() <= c {
                            self.fresh.resize(c + 1, Vec::new());
                        }
                        self.fresh[c].push(m.clone());
                        self.got[c].push(m);
                    }
                    Packet::Frame { ticked } => {
                        self.ticked = ticked;
                        self.got[c].push(vec![u8::from(ticked)]);
                    }
                    Packet::Send { .. } => panic!("a server end never sends C→S"),
                    Packet::Ext { .. } => panic!("D2RUST's own game registers no extension"),
                }
            }
        }
    }

    fn sim(&self) -> &Sim {
        &self.hub.host.game
    }

    /// The client's record state (`client_state`), if it has a record.
    fn state(&self, c: usize) -> Option<u32> {
        let s = self.sim();
        let id = s.sim_client(c as ClientId)?;
        s.game.lists.client(id).map(|e| e.state)
    }

    /// Steps until every client is in the game (at most `limit` frames).
    fn until_all_in_game(&mut self, limit: usize) {
        for _ in 0..limit {
            if (0..self.ends.len()).all(|c| self.state(c) == Some(client_state::IN_GAME)) {
                return;
            }
            self.step();
        }
        let states: Vec<_> = (0..self.ends.len()).map(|c| self.state(c)).collect();
        panic!("not every client got into the game: {states:?}");
    }
}

fn every_client_gets_its_own_player(n: usize) {
    let mut g = Game::new(DEFAULT_SEED, n);
    g.until_all_in_game(400);
    let players: Vec<_> = (0..n)
        .map(|c| g.sim().player_of(c as ClientId).expect("a player"))
        .collect();
    let mut unique = players.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), n, "one player unit per client: {players:?}");
    // Each player is the class its client asked for.
    for (c, &p) in players.iter().enumerate() {
        let class = g.sim().events.action.sys.units.get(p).map(|u| u.class);
        assert_eq!(class, Some((c % CLASSES.len()) as u32), "client {c}");
    }
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn two_clients_play_one_game() {
    every_client_gets_its_own_player(2);
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn eight_clients_play_one_game() {
    every_client_gets_its_own_player(8);
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_client_that_joins_later_gets_the_games_settings() {
    let mut g = Game::new(DEFAULT_SEED, 1);
    g.until_all_in_game(400);
    // A second client connects mid-game and asks for Hell: the game stays
    // as it was created, and S→C 0x01 tells it so.
    let (end, server_end) = pair();
    g.hub.connect(1, server_end);
    g.ends.push(end);
    g.got.push(Vec::new());
    g.joined.push(false);
    let character = single_player::new_character("barbarian", "Later").unwrap();
    let mut req = single_player::create_request_for(&character);
    req.difficulty = 2;
    g.send(1, Lane::System, req.encode().to_vec());
    g.until_all_in_game(400);
    let first = |c: usize| {
        g.got[c]
            .iter()
            .find(|m| m.first() == Some(&0x01))
            .cloned()
            .expect("S→C 0x01")
    };
    assert_eq!(first(1), first(0), "both clients see the same game flags");
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_client_leaves_and_the_others_play_on() {
    let mut g = Game::new(DEFAULT_SEED, 3);
    g.until_all_in_game(400);
    let leaver = g.sim().player_of(1).expect("a player");
    let guid = g.sim().game.lists.unit(leaver).map(|e| e.guid).unwrap();
    let before = g.got[0].len();
    g.send(1, Lane::System, vec![0x69]);
    for _ in 0..5 {
        g.step();
    }
    assert_eq!(g.state(1), None, "the leaver's record is gone");
    assert!(
        g.sim().game.lists.unit(leaver).is_none(),
        "its player is gone"
    );
    // The others are told (S→C 0x5C with the leaver's GUID) and stay in.
    let mut left = vec![0x5C];
    left.extend_from_slice(&guid.to_le_bytes());
    for c in [0, 2] {
        assert!(
            g.got[c][before..].iter().any(|m| m.starts_with(&left)),
            "client {c} hears that the player left"
        );
        assert_eq!(g.state(c), Some(client_state::IN_GAME));
    }
    for _ in 0..50 {
        g.step();
    }
    assert_eq!(g.state(0), Some(client_state::IN_GAME));
    assert_eq!(g.state(2), Some(client_state::IN_GAME));
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_lost_connection_takes_the_player_out() {
    let mut g = Game::new(DEFAULT_SEED, 2);
    g.until_all_in_game(400);
    let lost = g.sim().player_of(1).expect("a player");
    let guid = g.sim().game.lists.unit(lost).map(|e| e.guid).unwrap();
    let before = g.got[0].len();
    // Client 1's end goes away (no leave message): the hub drops it.
    let (dead, _) = pair();
    drop(std::mem::replace(&mut g.ends[1], dead));
    for _ in 0..3 {
        g.step();
    }
    assert_eq!(g.hub.take_dropped().len(), 1);
    assert_eq!(g.state(1), None);
    assert!(g.sim().game.lists.unit(lost).is_none());
    let mut left = vec![0x5C];
    left.extend_from_slice(&guid.to_le_bytes());
    assert!(g.got[0][before..].iter().any(|m| m.starts_with(&left)));
    assert_eq!(g.state(0), Some(client_state::IN_GAME));
}

/// Runs the same script on the same seed: four clients join, walk and one
/// leaves. What each client receives is returned.
fn scripted_game() -> Vec<Vec<Vec<u8>>> {
    let mut g = Game::new(DEFAULT_SEED, 4);
    g.until_all_in_game(400);
    for i in 0..60u16 {
        // Every client asks to walk somewhere near its player (C→S 0x01,
        // a walk to a point), clients 0 and 2 every frame, 1 and 3 every
        // other frame.
        for c in 0..4usize {
            if c % 2 == 1 && i % 2 == 1 {
                continue;
            }
            let Some(pos) = g.sim().player_pos(c as ClientId) else {
                continue;
            };
            let dx = (i % 7) as i32 - 3 + c as i32;
            let dy = (i % 5) as i32 - 2;
            let x = (pos.x + dx) as u16;
            let y = (pos.y + dy) as u16;
            let mut walk = vec![0x01];
            walk.extend_from_slice(&x.to_le_bytes());
            walk.extend_from_slice(&y.to_le_bytes());
            g.send(c, Lane::Game, walk);
        }
        if i == 30 {
            g.send(3, Lane::System, vec![0x69]);
        }
        g.step();
    }
    g.got
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn the_same_script_gives_the_same_game_for_every_client() {
    let a = scripted_game();
    let b = scripted_game();
    assert_eq!(a.len(), b.len());
    for (c, (x, y)) in a.iter().zip(&b).enumerate() {
        assert_eq!(x.len(), y.len(), "client {c}: message count");
        assert!(x == y, "client {c}: the received bytes differ");
    }
}

/// One client of a [`Game`] as the bridge's link, as a realm's game
/// server is to its players: the client gets the server's messages and
/// nothing else (no single-player server copy). Each pump is one
/// [`Game::step`] for everyone; the other clients are driven by the test.
struct ClientLink {
    game: Game,
    client: usize,
}

impl ServerLink for ClientLink {
    fn protocol_version(&self) -> u32 {
        d2_proto::PROTOCOL_VERSION
    }
    fn tick_rate(&self) -> d2_sim::tick::TickRate {
        self.game.hub.host.driver.rate()
    }
    fn send(&mut self, queue: SendQueue, msg: &[u8]) -> Result<Sent, LinkError> {
        let lane = match queue {
            SendQueue::Game => Lane::Game,
            SendQueue::System => Lane::System,
        };
        self.game.send(self.client, lane, msg.to_vec());
        Ok(Sent::Queued)
    }
    fn pump(&mut self) -> Result<Pumped, LinkError> {
        self.game.step();
        Ok(Pumped {
            ticked: self.game.ticked,
        })
    }
    fn receive(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.game.fresh[self.client])
    }
    fn is_remote(&self) -> bool {
        true
    }
}

/// Another player's walk in a realm client's model: client 0 walks a few
/// sub-tiles in town and stops; client 1 (the bridge, with the play app's
/// unit tracks after each frame) moves player 0 along with the server and
/// shows it standing where the server put it within a few frames of the
/// server's stand.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn another_player_stands_in_a_realm_client_when_its_walk_ends() {
    let mut game = Game::new(DEFAULT_SEED, 2);
    // The bridge answers the server's 0x02 itself.
    game.joined[1] = true;
    let mut b = Bridge::new(ClientLink { game, client: 1 }).unwrap();
    app_support::live_bridge_tables(&mut b);
    let data = app_support::game_data();
    b.set_drlg_source(Some(single_player::client_drlg_source(&data)));
    b.set_levels(single_player::client_level_rows(&data));
    b.set_player_anims(Arc::new(single_player::client_player_anims(&data).unwrap()));
    let live = app_support::live();
    b.set_class_velocities(single_player::client_class_velocities(live.archives.as_ref()).unwrap());
    let in_game = |b: &Bridge<ClientLink>| {
        (0..2).all(|c| b.link().game.state(c) == Some(client_state::IN_GAME))
    };
    for _ in 0..400 {
        if in_game(&b) {
            break;
        }
        b.frame().unwrap();
    }
    assert!(in_game(&b), "both clients in the game");
    for _ in 0..20 {
        b.frame().unwrap();
    }
    let g = &b.link().game;
    let p0 = g.sim().player_of(0).unwrap();
    let guid = g.sim().events.action.sys.units.get(p0).unwrap().guid;
    let key = UnitKey::new(0, guid);
    let server = |b: &Bridge<ClientLink>| {
        let s = b.link().game.sim();
        let mode = s.events.action.sys.units.get(p0).unwrap().mode;
        let (x, y) = s.events.action.sys.hooks.path_position(p0);
        (mode, (x as u16, y as u16))
    };
    let (_, (x, y)) = server(&b);
    let mut walk = vec![0x01];
    walk.extend_from_slice(&(x + 6).to_le_bytes());
    walk.extend_from_slice(&y.to_le_bytes());
    b.link_mut().game.send(0, Lane::Game, walk);
    let mut walked = false;
    let mut since_stand = None;
    let mut trace = Vec::new();
    let mut motion = MonsterMotion::default();
    for _ in 0..200 {
        b.frame().unwrap();
        b.preview_motion(&mut motion);
        let (mode, at) = server(&b);
        let u = b
            .world()
            .units
            .get(&key)
            .expect("player 0 in client 1's model");
        trace.push((mode, at, u.mode, u.position));
        walked |= matches!(mode, 2 | 3 | 6);
        if walked && matches!(mode, 1 | 5) {
            let n = since_stand.get_or_insert(0);
            *n += 1;
            if *n > 4 {
                break;
            }
        }
    }
    assert!(walked, "player 0 never walked");
    // While it walks, the client keeps it within two sub-tiles of the
    // server's cell (it moves; it does not walk in place).
    let behind = trace
        .iter()
        .filter_map(|&(_, (sx, _), _, c)| Some((i32::from(sx) - i32::from(c?.0)).abs()))
        .max()
        .unwrap_or(0);
    assert!(behind <= 2, "client 1's player 0 lags: {trace:?}");
    let (mode, at, client_mode, client_at) = *trace.last().unwrap();
    assert!(
        matches!(mode, 1 | 5) && client_mode == mode && client_at == Some(at),
        "client 1's player 0 (server mode, cell, client mode, cell): {trace:?}"
    );
}

/// Both clients of `b` out of the Rogue Encampment into the Blood Moor on
/// foot, as [`real_rig::Rig::leave_town`] does for one: run legs (C→S
/// 0x03) along the collision route, each client its own, frames until
/// nobody moves, twenty sub-tiles on into the level.
fn leave_town(b: &mut Bridge<ClientLink>) {
    let (town, moor) = (single_player::ACT1_TOWN, single_player::BLOOD_MOOR);
    let level = |b: &mut Bridge<ClientLink>, c: usize| {
        let s = &mut b.link_mut().game.hub.host.game;
        let p = s.player_of(c as ClientId)?;
        let room = s.game.lists.unit(p)?.room()?;
        s.events.action.hooks().drlg.level_id(&s.game, room)
    };
    let moving = |b: &Bridge<ClientLink>| {
        let s = b.link().game.sim();
        (0..2).any(|c| {
            let p = s.player_of(c as ClientId).unwrap();
            test_fixtures::host::MOVING.contains(&s.events.action.sys.units.get(p).unwrap().mode)
        })
    };
    let leg = |b: &mut Bridge<ClientLink>, c: usize, (x, y): (i32, i32)| {
        let mut m = vec![0x03];
        m.extend_from_slice(&(x as u16).to_le_bytes());
        m.extend_from_slice(&(y as u16).to_le_bytes());
        b.link_mut().game.send(c, Lane::Game, m);
    };
    for _ in 0..12 {
        if (0..2).all(|c| level(b, c) == Some(moor)) {
            break;
        }
        let routes: Vec<Vec<(i32, i32)>> = (0..2)
            .map(|c| {
                let s = &mut b.link_mut().game.hub.host.game;
                let p = s.player_of(c as ClientId).unwrap();
                let start = s.events.action.sys.hooks.path_position(p);
                let d = s.events.action.hooks().drlg.dungeon.acts[0]
                    .as_ref()
                    .expect("Act I");
                let rect = |id| {
                    d.find_level(id)
                        .map(|l| d.level(l).rect)
                        .expect("level allocated")
                };
                test_fixtures::host::route(d, start, rect(town), rect(moor), 12)
            })
            .collect();
        for i in 0..routes.iter().map(Vec::len).max().unwrap_or(0) {
            for (c, route) in routes.iter().enumerate() {
                if let Some(&g) = route.get(i) {
                    leg(b, c, g);
                }
            }
            for _ in 0..2 {
                b.frame().unwrap();
            }
            for _ in 0..400 {
                if !moving(b) {
                    break;
                }
                b.frame().unwrap();
            }
        }
    }
    assert!(
        (0..2).all(|c| level(b, c) == Some(moor)),
        "both clients walked into the Blood Moor"
    );
    // On past the border: a free spot with room around it.
    let moor_rect = {
        let s = &mut b.link_mut().game.hub.host.game;
        let d = s.events.action.hooks().drlg.dungeon.acts[0]
            .as_ref()
            .unwrap();
        d.level(d.find_level(moor).unwrap()).rect
    };
    let centre = (
        (moor_rect.x * 2 + moor_rect.w) * 5 / 2,
        (moor_rect.y * 2 + moor_rect.h) * 5 / 2,
    );
    for c in 0..2 {
        let s = b.link().game.sim();
        let p = s.player_of(c as ClientId).unwrap();
        let at = s.events.action.sys.hooks.path_position(p);
        leg(
            b,
            c,
            (
                at.0 + (centre.0 - at.0).clamp(-20, 20),
                at.1 + (centre.1 - at.1).clamp(-20, 20),
            ),
        );
    }
    for _ in 0..2 {
        b.frame().unwrap();
    }
    for _ in 0..400 {
        if !moving(b) {
            break;
        }
        b.frame().unwrap();
    }
    for _ in 0..4 {
        b.frame().unwrap();
    }
}

/// Another player's Charge in a realm client: client 0 (a paladin) charges
/// a monster east of it, client 1 (the bridge, no server copy) sees it
/// through the messages. The client has no weapon item of its own for
/// player 0 beyond the 0x9D records of its equipment, and its reach test
/// for the charge reads that player's weapon like any other player's: the
/// charge runs without the client rejecting an update.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn another_players_charge_runs_in_a_realm_client_without_errors() {
    use d2_data::tables::Skills;
    use d2_sim::skills::list::ListOwner;
    use d2_sim::stats::stat;
    use d2_sim::units::lifecycle::AllocRequest;
    use d2_sim::units::UnitType;
    let class_id = i32::from(single_player::parse_class("paladin").unwrap());
    let rows: Vec<Skills> = app_support::live().tables.rows().expect("skills");
    let charge = rows
        .iter()
        .find(|r| i32::from(r.charclass) == class_id && r.srvdofunc == 67)
        .expect("the paladin's Charge")
        .skill;
    let mut game = Game::with_classes(DEFAULT_SEED, &["paladin", "barbarian"]);
    // The bridge answers the server's 0x02 itself.
    game.joined[1] = true;
    let mut b = Bridge::new(ClientLink { game, client: 1 }).unwrap();
    app_support::live_bridge_tables(&mut b);
    let data = app_support::game_data();
    b.set_drlg_source(Some(single_player::client_drlg_source(&data)));
    b.set_levels(single_player::client_level_rows(&data));
    b.set_player_anims(Arc::new(single_player::client_player_anims(&data).unwrap()));
    let live = app_support::live();
    b.set_class_velocities(single_player::client_class_velocities(live.archives.as_ref()).unwrap());
    let in_game = |b: &Bridge<ClientLink>| {
        (0..2).all(|c| b.link().game.state(c) == Some(client_state::IN_GAME))
    };
    for _ in 0..400 {
        if in_game(&b) {
            break;
        }
        b.frame().unwrap();
    }
    assert!(in_game(&b), "both clients in the game");
    for _ in 0..20 {
        b.frame().unwrap();
    }
    // Charge is not for town: both walk out into the Blood Moor.
    leave_town(&mut b);
    // Staging: player 0 knows Charge as its right skill, with mana and
    // level; a monster stands 10 sub-tiles east of it.
    let (p0, guid, target_guid, from) = {
        let sim = &mut b.link_mut().game.hub.host.game;
        let p0 = sim.player_of(0).unwrap();
        let h = &mut sim.events.action.sys.hooks;
        let table = h.tables.skills.skills.clone();
        let list = h.skill_lists.entry(p0).or_default();
        *list = Default::default();
        let mut ids = [0xFFFFu16; 10];
        ids[0] = charge;
        list.init_player(&table, ListOwner::player(class_id), Some(&ids))
            .unwrap();
        list.right = Some(
            list.view()
                .iter()
                .position(|e| e.skill == i32::from(charge))
                .expect("learned"),
        );
        sim.events.action.with(&mut sim.game, |_, v| {
            for st in [stat::MANA, stat::MAXMANA, stat::HITPOINTS, stat::MAXHP] {
                v.set_base(p0, st, 100 << 8);
            }
            v.set_base(p0, stat::LEVEL, 30);
            v.set_base(p0, 19, 1000);
        });
        let a = &mut sim.events.action;
        let (px, py) = a.sys.hooks.path_position(p0);
        let room = sim
            .game
            .lists
            .unit(p0)
            .and_then(|e| e.room())
            .expect("room");
        let req = AllocRequest {
            ty: UnitType::Monster,
            class: 5,
            room: Some(room),
            add: true,
            fixed_guid: None,
            mode: 1,
            allied: false,
        };
        let m = a
            .with(&mut sim.game, |g, v| v.allocate(g, &req, px + 10, py))
            .expect("monster");
        a.with(&mut sim.game, |_, v| {
            v.set_base(m, stat::LEVEL, 1);
            v.set_base(m, stat::MAXHP, 100 << 8);
            v.set_base(m, stat::HITPOINTS, 100 << 8);
        });
        a.sys.units.get_mut(m).unwrap().flags |= d2_sim::missiles::unit_flag::BIT1
            | d2_sim::missiles::unit_flag::IS_VALID_TARGET
            | d2_sim::missiles::unit_flag::CAN_BE_ATTACKED;
        let guid = sim.game.lists.unit(p0).unwrap().guid;
        let target_guid = sim.game.lists.unit(m).unwrap().guid;
        (p0, guid, target_guid, (px, py))
    };
    let rejected_before = b.log().rejected.len();
    let mut msg = vec![0x0D];
    msg.extend(1u32.to_le_bytes());
    msg.extend(target_guid.to_le_bytes());
    b.link_mut().game.send(0, Lane::Game, msg);
    let key = UnitKey::new(0, guid);
    let mut modes = Vec::new();
    let mut charged = false;
    for _ in 0..120 {
        b.frame().unwrap();
        let s = b.link().game.sim();
        let (x, y) = s.events.action.sys.hooks.path_position(p0);
        let server_mode = s.events.action.sys.units.get(p0).unwrap().mode;
        let client = b.world().units.get(&key).map(|u| (u.mode, u.position));
        modes.push((server_mode, (x, y), client));
        charged |= x > from.0 + 1;
    }
    assert!(
        charged,
        "player 0 never charged from {from:?}: {:?}",
        &modes[..modes.len().min(30)]
    );
    let rejected = &b.log().rejected[rejected_before..];
    assert!(
        rejected.is_empty(),
        "the client rejected updates: {rejected:?}"
    );
    assert!(
        modes
            .iter()
            .any(|&(_, _, c)| c.is_some_and(|(m, _)| m == 18)),
        "client 1 never showed player 0 in its charge mode: {modes:?}"
    );
}
