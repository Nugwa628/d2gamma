//! The games on the user's game data: players join over in-memory game
//! streams, play, leave and come back; the game's rules about seats,
//! saves and closing.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use d2_net::msg::ErrorCode;
use d2_net::wire::Welcome;
use d2_net::{pair, Lane, MemoryEnd, NetError, Packet, Transport};
use d2_server::game::{create_request_for, new_character, GameData};
use d2_server::seams::ClientId;
use d2_sim::units::lists::client_state;
use tokio::sync::mpsc;

use super::*;

/// The user's game data, loaded once for every test.
fn data() -> GameData {
    static DATA: OnceLock<GameData> = OnceLock::new();
    DATA.get_or_init(|| {
        let dir = std::env::var_os("D2_GAME_DIR").expect("D2_GAME_DIR");
        GameData::select(Some(Path::new(&dir))).expect("the game data loads")
    })
    .clone()
}

/// A temporary server folder, removed at the end.
struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "d2rust-games-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Folder(dir)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        // A failed test shows the server's log.
        if std::thread::panicking() {
            let log = std::fs::read_to_string(self.0.join("logs").join(d2_service::log::FILE_NAME));
            eprintln!("server log:\n{}", log.unwrap_or_default());
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn config() -> GamesConfig {
    GamesConfig {
        max_games: 4,
        max_players: 8,
        grace: Duration::from_secs(2),
        frame_pause: Duration::from_millis(1),
        save_interval: Duration::from_secs(3600),
        unused_close: Duration::from_millis(200),
        dev_test: false,
        invent_stash_items: false,
    }
}

struct Server {
    games: Games,
    events: mpsc::Receiver<GameEvent>,
    tx: mpsc::Sender<GameEvent>,
    dir: Folder,
    /// The stored characters: the seats' records come from here.
    sink: MemorySink,
}

impl Server {
    fn new(name: &str) -> Self {
        Self::with(name, config())
    }

    fn with(name: &str, cfg: GamesConfig) -> Self {
        let sink = MemorySink::default();
        Self::with_sink(name, cfg, sink.clone(), Arc::new(sink))
    }

    /// A server whose games save to `saves`; the seats' records come from
    /// `sink`.
    fn with_sink(name: &str, cfg: GamesConfig, sink: MemorySink, saves: Arc<dyn SaveSink>) -> Self {
        let dir = Folder::new(name);
        let log = Arc::new(ServerLog::open(&dir.0.join("logs")).unwrap());
        let (tx, events) = mpsc::channel(1024);
        Server {
            games: Games::new(cfg, data(), log, saves, Arc::new(ClockSeed)),
            events,
            tx,
            dir,
            sink,
        }
    }

    /// A server whose games play `rules`.
    fn with_rules(name: &str, rules: GameRules) -> Self {
        let dir = Folder::new(name);
        let log = Arc::new(ServerLog::open(&dir.0.join("logs")).unwrap());
        let (tx, events) = mpsc::channel(1024);
        let sink = MemorySink::default();
        Server {
            games: Games::with_rules(
                config(),
                data(),
                rules,
                log,
                Arc::new(sink.clone()),
                Arc::new(ClockSeed),
            ),
            events,
            tx,
            dir,
            sink,
        }
    }

    /// A server whose games take their seeds and first item-id blocks
    /// from `seeds`.
    fn with_seeds(name: &str, seeds: Arc<dyn SeedSource>) -> Self {
        let dir = Folder::new(name);
        let log = Arc::new(ServerLog::open(&dir.0.join("logs")).unwrap());
        let (tx, events) = mpsc::channel(1024);
        let sink = MemorySink::default();
        Server {
            games: Games::new(config(), data(), log, Arc::new(sink.clone()), seeds),
            events,
            tx,
            dir,
            sink,
        }
    }

    fn start(&self, id: u32, difficulty: u8) -> GameHandle {
        self.games
            .start(
                GameSpec {
                    id,
                    name: format!("game{id}"),
                    difficulty,
                    hardcore: false,
                    expansion: true,
                    max_players: 0,
                },
                self.tx.clone(),
            )
            .unwrap()
    }

    fn seat(&self, account: i64, name: &str, new_class: Option<u8>) -> Seat {
        self.seat_with(account, name, new_class, 0)
    }

    fn seat_with(&self, account: i64, name: &str, new_class: Option<u8>, progression: u8) -> Seat {
        Seat {
            account: AccountId(account),
            character: name.into(),
            // One lease per account is enough here.
            lease: account as u64,
            record: self.sink.record(AccountId(account), name),
            new: new_class.map(|class| NewChar {
                name: name.into(),
                class,
                hardcore: false,
                expansion: true,
                progression,
            }),
        }
    }

    /// Every event so far.
    fn drain(&mut self) -> Vec<GameEvent> {
        let mut out = Vec::new();
        while let Ok(e) = self.events.try_recv() {
            out.push(e);
        }
        out
    }

    /// Waits for an event `want` picks (others are kept in `seen`).
    fn wait_event(
        &mut self,
        seen: &mut Vec<GameEvent>,
        want: impl Fn(&GameEvent) -> bool,
    ) -> GameEvent {
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(i) = seen.iter().position(&want) {
                return seen.remove(i);
            }
            seen.extend(self.drain());
            assert!(Instant::now() < until, "no such event; saw {seen:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

/// The server's end of a test player's game stream: an in-memory pair end
/// that hands the game's welcome to the player's side.
struct TestStream {
    end: MemoryEnd,
    welcome: Arc<Mutex<Option<Vec<u8>>>>,
}

impl Transport for TestStream {
    fn send(&mut self, p: Packet) -> Result<(), NetError> {
        self.end.send(p)
    }
    fn recv(&mut self) -> Result<Option<Packet>, NetError> {
        self.end.recv()
    }
}

impl AttachedStream for TestStream {
    fn send_body(&mut self, body: Vec<u8>) -> Result<(), NetError> {
        *self.welcome.lock().unwrap() = Some(body);
        Ok(())
    }
}

/// A test player: its end of the game stream, acting as the game client
/// does at the start of a game (0x67, then 0x6B after the server's 0x02).
struct Player {
    end: MemoryEnd,
    welcome: Welcome,
    /// The last raw frame body the game sent outside the packets (its
    /// welcome, then a refusal).
    answer: Arc<Mutex<Option<Vec<u8>>>>,
    joined: bool,
    got: Vec<Vec<u8>>,
}

impl Player {
    /// Admits `seat` into `game` and sends the 0x67 join of `name`
    /// (`class` for the request; the server takes the seat's).
    fn join(
        game: &GameHandle,
        seat: Seat,
        name: &str,
        difficulty: u8,
    ) -> Result<Player, ErrorCode> {
        let (end, server_end) = pair();
        let answer = Arc::new(Mutex::new(None));
        game.admit(
            seat,
            TestStream {
                end: server_end,
                welcome: answer.clone(),
            },
        )?;
        let body = answer.lock().unwrap().take().expect("a welcome");
        let mut p = Player {
            end,
            welcome: d2_net::wire::parse_answer(&body).expect("a game welcome"),
            answer: answer.clone(),
            joined: false,
            got: Vec::new(),
        };
        let mut req = create_request_for(&new_character("barbarian", name).unwrap(), 800, 600);
        req.difficulty = difficulty;
        p.send(Lane::System, req.encode().to_vec());
        Ok(p)
    }

    fn send(&mut self, lane: Lane, msg: Vec<u8>) {
        let _ = self.end.send(Packet::Send { lane, msg });
    }

    /// Reads what arrived; answers the 0x02 with the 0x6B.
    fn pump(&mut self) -> Result<(), NetError> {
        while let Some(p) = self.end.recv()? {
            if let Packet::Deliver(m) = p {
                if m.first() == Some(&0x02) && !self.joined {
                    self.joined = true;
                    self.send(Lane::System, vec![0x6B]);
                }
                self.got.push(m);
            }
        }
        Ok(())
    }

    fn client(&self) -> ClientId {
        self.welcome.client
    }
}

/// A client's state in the game's client list, if it has a record.
fn state(game: &GameHandle, client: ClientId) -> Option<u32> {
    game.with_thread(move |i| {
        let s = &i.hub.host.game;
        let id = s.sim_client(client)?;
        s.game.lists.client(id).map(|e| e.state)
    })
    .unwrap()
}

/// Pumps the players until every one is in the game.
fn until_in_game(game: &GameHandle, players: &mut [&mut Player]) {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        for p in players.iter_mut() {
            p.pump().unwrap();
        }
        if players
            .iter()
            .all(|p| state(game, p.client()) == Some(client_state::IN_GAME))
        {
            return;
        }
        assert!(Instant::now() < until, "not every player got into the game");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The join's character-state messages (the same set the games' re-entry
/// keeps): stats, items, inventory relator, quests/waypoints, mercenary,
/// party, skills and the vitals sync.
fn static_state(m: &[u8]) -> bool {
    matches!(
        m.first(),
        Some(
            0x1D | 0x1E
                | 0x1F
                | 0x21
                | 0x22
                | 0x23
                | 0x3E
                | 0x48
                | 0x28
                | 0x29
                | 0x5E
                | 0x89
                | 0x5F
                | 0x7A
                | 0x81
                | 0x65
                | 0x8D
                | 0x7B
                | 0x94
                | 0x95
                | 0x9C
                | 0x9D
        )
    )
}

const GOLD: u16 = 14;

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn two_players_play_one_game_with_their_own_characters() {
    let s = Server::new("two");
    let g = s.start(1, 0);
    let mut a = Player::join(&g, s.seat(1, "Alpha", Some(4)), "Alpha", 0).unwrap();
    let mut b = Player::join(&g, s.seat(2, "Beta", Some(1)), "Beta", 0).unwrap();
    assert_ne!(a.client(), b.client());
    until_in_game(&g, &mut [&mut a, &mut b]);
    let (pa, pb, ca, cb) = g
        .with_thread({
            let (ca, cb) = (a.client(), b.client());
            move |i| {
                let s = &i.hub.host.game;
                let (pa, pb) = (s.player_of(ca).unwrap(), s.player_of(cb).unwrap());
                let class = |p| s.events.action.sys.units.get(p).map(|u| u.class);
                (pa, pb, class(pa), class(pb))
            }
        })
        .unwrap();
    assert_ne!(pa, pb, "one player unit each");
    // The seat's class, not the request's (both asked for a barbarian).
    assert_eq!((ca, cb), (Some(4), Some(1)));
    assert_eq!(g.players(), 2);
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn leaving_saves_the_character_and_a_new_game_loads_it() {
    let mut s = Server::new("persist");
    let g = s.start(1, 0);
    let seat = s.seat(7, "Keeper", Some(0));
    let mut p = Player::join(&g, seat, "Keeper", 0).unwrap();
    until_in_game(&g, &mut [&mut p]);
    let client = p.client();
    g.with_thread(move |i| {
        let sim = &mut i.hub.host.game;
        let player = sim.player_of(client).unwrap();
        sim.events
            .action
            .with(&mut sim.game, |_, v| v.set_base(player, GOLD, 1234));
    })
    .unwrap();
    // Save and Exit: C→S 0x69, then the stream ends.
    p.send(Lane::System, vec![0x69]);
    let mut seen = Vec::new();
    let saved = s.wait_event(&mut seen, |e| matches!(e, GameEvent::Saved { .. }));
    let GameEvent::Saved {
        account, summary, ..
    } = saved
    else {
        unreachable!()
    };
    assert_eq!(account, AccountId(7));
    assert_eq!(summary.name, "Keeper");
    assert_eq!(summary.class, 0);
    assert_eq!(summary.level, 1);
    assert_eq!(summary.difficulties, 1, "a new character has Normal only");
    assert_eq!(summary.look.len(), 32);
    drop(p);
    s.wait_event(
        &mut seen,
        |e| matches!(e, GameEvent::Left { character, .. } if character == "Keeper"),
    );
    s.wait_event(&mut seen, |e| {
        matches!(e, GameEvent::Closed { game: 1, .. })
    });
    assert!(g.is_closed());
    assert!(!s.sink.record(AccountId(7), "Keeper").unwrap().is_empty());

    // A new game (as after a server restart) loads the record: the gold is
    // back.
    let g2 = s.start(2, 0);
    let mut p2 = Player::join(&g2, s.seat(7, "Keeper", None), "Keeper", 0).unwrap();
    until_in_game(&g2, &mut [&mut p2]);
    let client = p2.client();
    let gold = g2
        .with_thread(move |i| {
            let sim = &mut i.hub.host.game;
            let player = sim.player_of(client).unwrap();
            let class = sim.events.action.sys.units.get(player).map(|u| u.class);
            let gold = sim
                .events
                .action
                .with(&mut sim.game, |_, v| v.stats.unit_base(player, GOLD, 0));
            (class, gold)
        })
        .unwrap();
    assert_eq!(gold, (Some(0), 1234));
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_lost_player_re_enters_with_its_position_and_sees_the_other_player() {
    let mut s = Server::new("resume");
    let g = s.start(1, 0);
    let mut a = Player::join(&g, s.seat(1, "Alpha", Some(4)), "Alpha", 0).unwrap();
    let mut b = Player::join(&g, s.seat(2, "Beta", Some(1)), "Beta", 0).unwrap();
    until_in_game(&g, &mut [&mut a, &mut b]);
    // The load completes on a later frame; the join's character state is
    // in that same flush.
    let until = Instant::now() + Duration::from_secs(20);
    while !a.got.iter().any(|m| m.first() == Some(&0x04)) {
        a.pump().unwrap();
        b.pump().unwrap();
        assert!(
            Instant::now() < until,
            "no LoadComplete for the joiner: {:02X?}",
            a.got
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let (ca, cb) = (a.client(), b.client());
    let old_token = a.welcome.resume;
    let join_messages: Vec<Vec<u8>> = a.got.iter().filter(|m| static_state(m)).cloned().collect();
    assert!(
        !join_messages.is_empty(),
        "the join sent character state: {:02X?}",
        a.got
    );
    let guid_of = |client| {
        g.with_thread(move |i| {
            let sim = &i.hub.host.game;
            let p = sim.player_of(client).unwrap();
            sim.events.action.sys.units.get(p).map(|u| u.guid)
        })
        .unwrap()
        .expect("a player unit")
    };
    let (ga, gb) = (guid_of(ca), guid_of(cb));
    let position = || {
        g.with_thread(move |i| {
            let sim = &mut i.hub.host.game;
            let p = sim.player_of(ca).unwrap();
            sim.events.action.hooks().path_position(p)
        })
        .unwrap()
    };
    let (x, y) = position();

    // The connection is lost: the game parks the slot (grace 2 s).
    drop(a);
    for _ in 0..40 {
        b.pump().unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }

    // The resume is a re-entry: the same client, a new token, and the game
    // state again (rooms, the player at its position, the other player).
    let (end, server_end) = pair();
    let body = Arc::new(Mutex::new(None));
    g.resume(
        old_token,
        TestStream {
            end: server_end,
            welcome: body.clone(),
        },
    )
    .unwrap();
    let body = body.lock().unwrap().take().expect("a resume welcome");
    let welcome = d2_net::wire::parse_answer(&body).expect("a game welcome");
    assert_eq!(welcome.client, ca, "the same client id");
    assert_ne!(welcome.resume, old_token, "a new resume token");
    let mut r = Player {
        end,
        welcome,
        answer: Default::default(),
        joined: true,
        got: Vec::new(),
    };
    let guid_at = |m: &[u8]| u32::from_le_bytes([m[1], m[2], m[3], m[4]]);
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        r.pump().unwrap();
        let own = r
            .got
            .iter()
            .find(|m| m.first() == Some(&0x59) && guid_at(m) == ga);
        let other = r
            .got
            .iter()
            .any(|m| m.first() == Some(&0x59) && guid_at(m) == gb);
        if own.is_some() && other && r.got.iter().any(|m| m.first() == Some(&0x07)) {
            break;
        }
        assert!(
            Instant::now() < until,
            "the re-entry state did not arrive: {:02X?}",
            r.got
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    // The join's character state is sent again (contracts v12 3.7): every
    // such message the join sent arrives on the resumed stream too.
    let resumed: Vec<&Vec<u8>> = r.got.iter().filter(|m| static_state(m)).collect();
    for m in &join_messages {
        assert!(
            resumed.contains(&m),
            "the re-entry did not resend {m:02X?}; it sent {resumed:02X?}"
        );
    }
    // Nothing was reloaded or saved: the game still holds both players at
    // their positions, and the old token is dead.
    assert_eq!(g.players(), 2);
    assert_eq!(position(), (x, y), "the player was not moved");
    assert!(
        !s.drain()
            .iter()
            .any(|e| matches!(e, GameEvent::Saved { .. })),
        "a resume saves nothing"
    );
    let (_late, server_end) = pair();
    let refused = Arc::new(Mutex::new(None));
    assert_eq!(
        g.resume(
            old_token,
            TestStream {
                end: server_end,
                welcome: refused.clone(),
            }
        )
        .err(),
        Some(ErrorCode::UNKNOWN_RESUME),
        "the old token stops working"
    );
}

/// The state messages of a client's character, as the client applies them:
/// the last value of each replaceable record. GUIDs differ between games,
/// so they are zeroed before two views are compared.
#[derive(Debug, Default, PartialEq, Eq)]
struct CharacterView {
    stats: std::collections::BTreeMap<u8, u32>,
    /// Items in a grid, the body, the belt or the cursor (never the
    /// ground), one normalized last record each, in a GUID-independent
    /// order.
    items: Vec<Vec<u8>>,
    /// 0x21 / 0x22 item-skill links (join only).
    item_skills: Vec<Vec<u8>>,
    base_skills: Option<Vec<u8>>,
    hands: std::collections::BTreeMap<u8, Vec<u8>>,
    hotkeys: std::collections::BTreeMap<u8, Vec<u8>>,
    portal: Option<u32>,
    quest_availability: Option<Vec<u8>>,
    quest_info: Vec<Vec<u8>>,
    game_quest: Option<Vec<u8>>,
    quest_status: std::collections::BTreeMap<u8, Vec<u8>>,
    quest_log: Option<Vec<u8>>,
    mercs: Vec<Vec<u8>>,
}

fn u16_at(m: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([m[at], m[at + 1]])
}

fn u32_at(m: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([m[at], m[at + 1], m[at + 2], m[at + 3]])
}

/// `m` with the 4-byte GUIDs at `ranges` zeroed.
fn without_guids(m: &[u8], ranges: &[(usize, usize)]) -> Vec<u8> {
    let mut m = m.to_vec();
    for &(at, end) in ranges {
        m[at..end].fill(0);
    }
    m
}

/// The item flags the move queue keeps on a live item and a save's load
/// normalizes away (the update list's bits and `picked to belt`). They are
/// not part of the item's view.
const QUEUED_ITEM_FLAGS: u32 = 0x800 | 0x1000 | 0x2000 | 0x10000 | 0x200000;

/// `m` with the GUIDs zeroed and the item header's queued flags masked;
/// `head` is the item bit stream's offset in the message.
fn item_record(m: &[u8], head: usize, owned: bool) -> Vec<u8> {
    let mut m = without_guids(m, if owned { &[(4, 8), (9, 13)] } else { &[(4, 8)] });
    let flags = u32_at(&m, head) & !QUEUED_ITEM_FLAGS;
    m[head..head + 4].copy_from_slice(&flags.to_le_bytes());
    m
}

/// The first `n` bits of an LSB-first item bit stream.
struct Bits<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Bits<'_> {
    fn read(&mut self, n: usize) -> Option<u32> {
        if self.at + n > self.bytes.len() * 8 {
            return None;
        }
        let mut v = 0;
        for i in 0..n {
            let at = self.at + i;
            v |= u32::from((self.bytes[at / 8] >> (at % 8)) & 1) << i;
        }
        self.at += n;
        Some(v)
    }
}

/// The item header's mode (0 grid, 1 body, 2 belt, 3 ground, 4 cursor,
/// 5 dropping).
fn item_mode(stream: &[u8]) -> Option<u8> {
    let mut b = Bits {
        bytes: stream,
        at: 0,
    };
    b.read(32)?;
    b.read(10)?;
    Some(b.read(3)? as u8)
}

/// The name of a 0x59 add message, or empty.
fn add_name(m: &[u8]) -> &[u8] {
    let n = m[6..22].iter().position(|&b| b == 0).unwrap_or(16);
    &m[6..6 + n]
}

/// The character state a client is told, from its S→C messages.
fn character_view(messages: &[Vec<u8>], name: &str) -> CharacterView {
    let me = messages
        .iter()
        .find(|m| m.first() == Some(&0x59) && add_name(m) == name.as_bytes())
        .map(|m| u32_at(m, 1))
        .expect("the client's own player add");
    let mut v = CharacterView::default();
    let mut items: std::collections::BTreeMap<u32, Vec<u8>> = Default::default();
    let mut info: std::collections::BTreeMap<(u8, u32), Vec<u8>> = Default::default();
    let mut mercs: std::collections::BTreeMap<u32, Vec<u8>> = Default::default();
    let mut item_skills = Vec::new();
    for m in messages {
        let Some(&id) = m.first() else { continue };
        match id {
            0x1D if m.len() == 3 => {
                v.stats.insert(m[1], u32::from(m[2]));
            }
            0x1E if m.len() == 4 => {
                v.stats.insert(m[1], u32::from(u16_at(m, 2)));
            }
            0x1F if m.len() == 6 => {
                v.stats.insert(m[1], u32_at(m, 2));
            }
            0x9C if m.len() >= 8 && usize::from(m[2]) == m.len() => {
                let mode = item_mode(&m[8..]);
                if mode.is_some_and(|mode| !matches!(mode, 3 | 5)) {
                    items.insert(u32_at(m, 4), item_record(m, 8, false));
                }
            }
            0x9D if m.len() >= 13 && usize::from(m[2]) == m.len() => {
                let owner = (m[8], u32_at(m, 9));
                let mode = item_mode(&m[13..]);
                if owner == (0, me) && mode.is_some_and(|mode| !matches!(mode, 3 | 5)) {
                    items.insert(u32_at(m, 4), item_record(m, 13, true));
                }
            }
            0x21 if m.len() == 12 => item_skills.push(without_guids(m, &[(3, 7)])),
            0x22 if m.len() == 12 => item_skills.push(without_guids(m, &[(3, 7)])),
            0x28 if m.len() == 103 => {
                info.insert((m[1], u32_at(m, 2)), without_guids(m, &[(2, 6)]));
            }
            0x29 if m.len() == 97 => v.game_quest = Some(m.clone()),
            0x5D if m.len() == 6 => {
                v.quest_status.insert(m[1], m.clone());
            }
            0x5E if m.len() == 38 => v.quest_availability = Some(m.clone()),
            0x5F if m.len() == 5 => v.portal = Some(u32_at(m, 1)),
            0x52 if m.len() == 42 => v.quest_log = Some(m.clone()),
            0x23 if m.len() == 13 => {
                v.hands.insert(m[6], without_guids(m, &[(2, 6), (9, 13)]));
            }
            0x7B if m.len() == 8 => {
                v.hotkeys.insert(m[1], without_guids(m, &[(4, 8)]));
            }
            0x94 if m.len() >= 6 => {
                v.base_skills = Some(without_guids(m, &[(2, 6)]));
            }
            // The mercenary add (0x81) and its pet action (0x7A).
            0x81 if m.len() == 20 => {
                mercs.insert(u32_at(m, 8), without_guids(m, &[(4, 8), (8, 12)]));
            }
            0x7A if m.len() == 13 => {
                let pet = u32_at(m, 9);
                if m[1] == 0 {
                    mercs.remove(&pet);
                } else {
                    mercs.insert(pet, without_guids(m, &[(5, 9), (9, 13)]));
                }
            }
            _ => {}
        }
    }
    v.items = items.into_values().collect();
    v.items.sort();
    v.quest_info = info.into_values().collect();
    v.quest_info.sort();
    v.item_skills = item_skills;
    v.item_skills.sort();
    v.mercs = mercs.into_values().collect();
    v.mercs.sort();
    v
}

/// A resume after the state changed: the joined client gains a level (the
/// game's own level-up: stat messages, party info, the item relink), then
/// drops and resumes. The client's view must match a fresh join of the
/// same character from the saved file, so a replay of the join-time state
/// (contracts v12 3.7) is caught.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_resume_after_the_state_changed_matches_a_fresh_join() {
    let mut s = Server::new("resume-state");
    let g = s.start(1, 0);
    let seat = s.seat(1, "Climber", Some(4));
    let mut p = Player::join(&g, seat, "Climber", 0).unwrap();
    until_in_game(&g, &mut [&mut p]);
    let client = p.client();
    // Level 2 through the game's own experience path: the tick sends the
    // changed stats and the level-up's messages.
    g.with_thread(move |i| {
        let sim = &mut i.hub.host.game;
        let player = sim.player_of(client).unwrap();
        let t = sim
            .events
            .action
            .sys
            .hooks
            .vitals
            .clone()
            .expect("the vitals tables");
        let class = sim
            .events
            .action
            .sys
            .units
            .get(player)
            .map_or(0, |r| r.class as i32);
        let gain = t.threshold_in(&d2_sim::rules::ORIGINAL, class, 2);
        let s = &mut sim.events.action.sys;
        let v = d2_sim::wiring::action::View::of(&mut s.units, &mut s.stats, &s.data, &mut s.hooks);
        let mut cv = d2_sim::wiring::action::combat::CombatView {
            game: &mut sim.game,
            v,
        };
        d2_sim::combat::vitals::add_experience(&mut cv, &t, player, gain);
    })
    .unwrap();
    // The level's stat message reaches the client.
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        p.pump().unwrap();
        let leveled = p.got.iter().any(|m| {
            m.get(1) == Some(&12)
                && m.first().is_some_and(|&id| matches!(id, 0x1D..=0x1F))
                && u32::from(m.last().copied().unwrap_or(0)) >= 2
        });
        if leveled {
            break;
        }
        assert!(
            Instant::now() < until,
            "no level-up stat message: {:02X?}",
            p.got
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let level = g
        .with_thread(move |i| {
            let sim = &mut i.hub.host.game;
            let player = sim.player_of(client).unwrap();
            sim.events.action.sys.stats.unit_base(player, 12, 0)
        })
        .unwrap();
    assert!(level > 1, "the character leveled: {level}");
    // The saved character has the new level: the fresh join loads it.
    let results = g
        .with_thread(move |i| i.hub.host.game.save_characters())
        .unwrap();
    assert!(results.iter().all(|(_, r)| r.is_ok()), "{results:?}");
    let mut seen = Vec::new();
    s.wait_event(&mut seen, |e| matches!(e, GameEvent::Saved { .. }));

    // The connection is lost, then the client comes back.
    let old_token = p.welcome.resume;
    drop(p);
    std::thread::sleep(Duration::from_millis(100));
    let (end, server_end) = pair();
    let body = Arc::new(Mutex::new(None));
    g.resume(
        old_token,
        TestStream {
            end: server_end,
            welcome: body.clone(),
        },
    )
    .unwrap();
    let body = body.lock().unwrap().take().expect("a resume welcome");
    let welcome = d2_net::wire::parse_answer(&body).expect("a game welcome");
    let mut resumed = Player {
        end,
        welcome,
        answer: Default::default(),
        joined: true,
        got: Vec::new(),
    };
    for _ in 0..80 {
        resumed.pump().unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }

    // A fresh join of the same character from the file just saved.
    let g2 = s.start(2, 0);
    let mut fresh = Player::join(&g2, s.seat(1, "Climber", None), "Climber", 0).unwrap();
    until_in_game(&g2, &mut [&mut fresh]);
    for _ in 0..80 {
        fresh.pump().unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        fresh.got.iter().any(|m| m.first() == Some(&0x04)),
        "the fresh join did not load: {:02X?}",
        fresh.got
    );
    let left = character_view(&resumed.got, "Climber");
    let right = character_view(&fresh.got, "Climber");
    let want = Some(&(level as u32));
    assert_eq!(
        left.stats.get(&12),
        want,
        "the resumed view has the level-up"
    );
    assert_eq!(right.stats.get(&12), want, "the fresh view has it too");
    assert_eq!(left, right, "the resumed view equals a fresh join's");
}

/// A resume closes an open vendor trade on the game side, removes the
/// player from the NPC's list and sends the client its close.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_resume_closes_an_open_vendor_trade() {
    let s = Server::new("resume-vendor");
    let g = s.start(1, 0);
    let mut p = Player::join(&g, s.seat(1, "Shopper", Some(0)), "Shopper", 0).unwrap();
    until_in_game(&g, &mut [&mut p]);
    let client = p.client();
    let (npc_guid, player) = g
        .with_thread(move |i| {
            let sim = &mut i.hub.host.game;
            let player = sim.player_of(client).unwrap();
            let npc = sim
                .game
                .lists
                .units_of_type(d2_sim::units::UnitType::Monster)
                .into_iter()
                .find(|&u| sim.world.state.lists.contains_key(&u))
                .expect("an NPC with an interaction list");
            let guid = sim
                .events
                .action
                .sys
                .units
                .get(npc)
                .map(|r| r.guid)
                .unwrap();
            sim.world
                .state
                .lists
                .get_mut(&npc)
                .unwrap()
                .nodes
                .insert(0, (player, d2_sim::world::npc::talk::TRADING));
            sim.events
                .action
                .sys
                .units
                .get_mut(player)
                .unwrap()
                .interact
                .set(1, guid);
            (guid, player)
        })
        .unwrap();
    let old_token = p.welcome.resume;
    drop(p);
    std::thread::sleep(Duration::from_millis(100));
    let (end, server_end) = pair();
    let body = Arc::new(Mutex::new(None));
    g.resume(
        old_token,
        TestStream {
            end: server_end,
            welcome: body.clone(),
        },
    )
    .unwrap();
    let body = body.lock().unwrap().take().expect("a resume welcome");
    let welcome = d2_net::wire::parse_answer(&body).expect("a game welcome");
    let mut r = Player {
        end,
        welcome,
        answer: Default::default(),
        joined: true,
        got: Vec::new(),
    };
    for _ in 0..40 {
        r.pump().unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        r.got.iter().any(|m| m == &vec![0x77, 0x0C]),
        "the resumed client was told to close: {:02X?}",
        r.got
    );
    let state = g
        .with_thread(move |i| {
            let sim = &mut i.hub.host.game;
            let npc = sim
                .game
                .lists
                .find_unit(d2_sim::units::UnitType::Monster, npc_guid)
                .unwrap();
            (
                sim.events
                    .action
                    .sys
                    .units
                    .get(player)
                    .unwrap()
                    .interact
                    .get(),
                sim.world.state.lists.get(&npc).map(|l| l.state(player)),
            )
        })
        .unwrap();
    assert_eq!(state.0, None, "the player's interaction is closed");
    assert_eq!(
        state.1.flatten(),
        None,
        "the NPC's list no longer holds the player"
    );
}

/// A resume closes the open stash view: the interaction is reset and the
/// client is sent the stash close.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_resume_closes_an_open_stash() {
    let s = Server::new("resume-stash");
    let g = s.start(1, 0);
    let mut p = Player::join(&g, s.seat(1, "Keeper", Some(1)), "Keeper", 0).unwrap();
    until_in_game(&g, &mut [&mut p]);
    let client = p.client();
    let (_, player) = g
        .with_thread(move |i| {
            let sim = &mut i.hub.host.game;
            let player = sim.player_of(client).unwrap();
            let stash = sim
                .game
                .lists
                .units_of_type(d2_sim::units::UnitType::Object)
                .into_iter()
                .find(|&u| {
                    sim.events
                        .action
                        .sys
                        .units
                        .get(u)
                        .is_some_and(|r| r.class == d2_sim::world::stash::STASH_CLASS)
                })
                .expect("the town's stash");
            let guid = sim
                .events
                .action
                .sys
                .units
                .get(stash)
                .map(|r| r.guid)
                .unwrap();
            sim.events
                .action
                .sys
                .units
                .get_mut(player)
                .unwrap()
                .interact
                .set(2, guid);
            (guid, player)
        })
        .unwrap();
    let old_token = p.welcome.resume;
    drop(p);
    std::thread::sleep(Duration::from_millis(100));
    let (end, server_end) = pair();
    let body = Arc::new(Mutex::new(None));
    g.resume(
        old_token,
        TestStream {
            end: server_end,
            welcome: body.clone(),
        },
    )
    .unwrap();
    let body = body.lock().unwrap().take().expect("a resume welcome");
    let welcome = d2_net::wire::parse_answer(&body).expect("a game welcome");
    let mut r = Player {
        end,
        welcome,
        answer: Default::default(),
        joined: true,
        got: Vec::new(),
    };
    for _ in 0..40 {
        r.pump().unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        r.got.iter().any(|m| m == &vec![0x77, 0x11]),
        "the resumed client was told to close the stash: {:02X?}",
        r.got
    );
    let closed = g
        .with_thread(move |i| {
            let sim = &mut i.hub.host.game;
            sim.events
                .action
                .sys
                .units
                .get(player)
                .unwrap()
                .interact
                .get()
        })
        .unwrap();
    assert_eq!(closed, None, "the stash interaction is closed");
}

/// A resume closes a trade with another player on both sides: the partner
/// is told and its interaction is reset.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_resume_closes_a_player_trade_on_both_sides() {
    let s = Server::new("resume-ptrade");
    let g = s.start(1, 0);
    let mut a = Player::join(&g, s.seat(1, "Seller", Some(0)), "Seller", 0).unwrap();
    let mut b = Player::join(&g, s.seat(2, "Buyer", Some(1)), "Buyer", 0).unwrap();
    until_in_game(&g, &mut [&mut a, &mut b]);
    let (ca, cb) = (a.client(), b.client());
    let (sa, sb) = g
        .with_thread(move |i| {
            let sim = &mut i.hub.host.game;
            let (pa, pb) = (sim.player_of(ca).unwrap(), sim.player_of(cb).unwrap());
            let guid = |p| sim.events.action.sys.units.get(p).map(|r| r.guid).unwrap();
            let (ga, gb) = (guid(pa), guid(pb));
            sim.events
                .action
                .sys
                .units
                .get_mut(pa)
                .unwrap()
                .interact
                .set(0, gb);
            sim.events
                .action
                .sys
                .units
                .get_mut(pb)
                .unwrap()
                .interact
                .set(0, ga);
            (pa, pb)
        })
        .unwrap();
    let old_token = a.welcome.resume;
    drop(a);
    std::thread::sleep(Duration::from_millis(100));
    let (end, server_end) = pair();
    let body = Arc::new(Mutex::new(None));
    g.resume(
        old_token,
        TestStream {
            end: server_end,
            welcome: body.clone(),
        },
    )
    .unwrap();
    let body = body.lock().unwrap().take().expect("a resume welcome");
    let welcome = d2_net::wire::parse_answer(&body).expect("a game welcome");
    let mut r = Player {
        end,
        welcome,
        answer: Default::default(),
        joined: true,
        got: Vec::new(),
    };
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        r.pump().unwrap();
        b.pump().unwrap();
        if r.got.iter().any(|m| m == &vec![0x77, 0x0C])
            && b.got.iter().any(|m| m == &vec![0x77, 0x0C])
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "no close: resumed {:02X?}, partner {:02X?}",
            r.got,
            b.got
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let state = g
        .with_thread(move |i| {
            let sim = &mut i.hub.host.game;
            let get = |p| sim.events.action.sys.units.get(p).unwrap().interact.get();
            (get(sa), get(sb))
        })
        .unwrap();
    assert_eq!(state, (None, None), "both sides are closed");
}

/// A panic inside a resume drops only that session: the game and the other
/// player keep running, and the dropped character is saved.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_panicking_resume_drops_only_that_session() {
    let mut s = Server::new("resume-panic");
    let g = s.start(1, 0);
    let mut a = Player::join(&g, s.seat(1, "Unlucky", Some(0)), "Unlucky", 0).unwrap();
    let mut b = Player::join(&g, s.seat(2, "Bystander", Some(1)), "Bystander", 0).unwrap();
    until_in_game(&g, &mut [&mut a, &mut b]);
    let old_token = a.welcome.resume;
    drop(a);
    for _ in 0..40 {
        b.pump().unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    g.with_thread(|i| i.panic_next_resume()).unwrap();
    let (_end, server_end) = pair();
    let body = Arc::new(Mutex::new(None));
    assert_eq!(
        g.resume(
            old_token,
            TestStream {
                end: server_end,
                welcome: body.clone(),
            }
        )
        .err(),
        Some(ErrorCode::OTHER),
        "the resume failed"
    );
    let mut seen = Vec::new();
    s.wait_event(
        &mut seen,
        |e| matches!(e, GameEvent::Left { character, .. } if character == "Unlucky"),
    );
    s.wait_event(&mut seen, |e| matches!(e, GameEvent::Saved { .. }));
    assert!(!g.is_closed(), "the game keeps running");
    assert_eq!(g.players(), 1);
    b.pump().unwrap();
    assert_eq!(
        state(&g, b.client()),
        Some(client_state::IN_GAME),
        "the other player plays on"
    );
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_new_character_keeps_the_progression_set_before_its_first_game() {
    let mut s = Server::new("new-progression");
    let g = s.start(1, 0);
    let seat = s.seat_with(8, "Unlocked", Some(0), 5);
    let mut p = Player::join(&g, seat, "Unlocked", 0).unwrap();
    until_in_game(&g, &mut [&mut p]);
    // Save and Exit: the first save of a character that was never played.
    p.send(Lane::System, vec![0x69]);
    let mut seen = Vec::new();
    let saved = s.wait_event(&mut seen, |e| matches!(e, GameEvent::Saved { .. }));
    let GameEvent::Saved { summary, .. } = saved else {
        unreachable!()
    };
    assert_eq!(d2_formats::d2s::status::progression(summary.status), 5);
    assert_eq!(summary.difficulties, 0b011, "Nightmare is open at 5");
    let stored = s.sink.record(AccountId(8), "Unlocked").unwrap();
    assert_eq!(
        d2_formats::d2s::header_status(&stored),
        Some(summary.status),
        "the first save kept the seat's progression"
    );
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_join_that_names_another_character_is_refused_with_45() {
    let mut s = Server::new("mismatch");
    let g = s.start(1, 0);
    let mut honest = Player::join(&g, s.seat(1, "Honest", Some(0)), "Honest", 0).unwrap();
    until_in_game(&g, &mut [&mut honest]);
    let mut liar = Player::join(&g, s.seat(2, "Liar", Some(0)), "Honest", 0).unwrap();
    let mut seen = Vec::new();
    s.wait_event(&mut seen, |e| {
        matches!(e, GameEvent::SeatRefused { character, code, .. }
            if character == "Liar" && *code == ErrorCode::CHAR_MISMATCH)
    });
    let refused = liar.answer.lock().unwrap().take().expect("a refusal");
    assert_eq!(
        d2_net::wire::Refused::parse(&refused).unwrap().code,
        ErrorCode::CHAR_MISMATCH
    );
    assert!(liar.pump().is_err() || state(&g, liar.client()).is_none());
    assert_eq!(g.players(), 1);
    honest.pump().unwrap();
    // The refused seat saved nothing and never left.
    assert!(!seen.iter().any(|e| matches!(e,
        GameEvent::Left { character, .. } | GameEvent::Saved { summary: CharSummary { name: character, .. }, .. }
            if character == "Liar")));
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn the_game_decides_the_difficulty_not_the_join() {
    let s = Server::new("difficulty");
    let g = s.start(1, 0);
    // The client asks for Hell in a Normal game.
    let mut p = Player::join(&g, s.seat(1, "Climber", Some(3)), "Climber", 2).unwrap();
    until_in_game(&g, &mut [&mut p]);
    let flags = p
        .got
        .iter()
        .find(|m| m.first() == Some(&0x01))
        .expect("S→C 0x01");
    assert_eq!(flags[1], 0, "S→C 0x01 says Normal: {flags:02X?}");
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn seats_are_refused_when_the_character_is_in_or_the_game_is_full() {
    let s = Server::with(
        "full",
        GamesConfig {
            max_players: 1,
            ..config()
        },
    );
    let g = s.start(1, 0);
    let _p = Player::join(&g, s.seat(1, "Solo", Some(0)), "Solo", 0).unwrap();
    assert_eq!(
        Player::join(&g, s.seat(1, "Solo", None), "Solo", 0).err(),
        Some(ErrorCode::CHAR_IN_GAME)
    );
    assert_eq!(
        Player::join(&g, s.seat(2, "Other", Some(0)), "Other", 0).err(),
        Some(ErrorCode::GAME_FULL)
    );
}

/// A server line is the chat window's plain line: type 4, gold, no name.
#[test]
fn a_server_line_is_a_plain_chat_line() {
    let m = instance::system_line("Restarting for an update in 5 minutes");
    assert_eq!(m[..11], [0x26, 4, 0, 2, 0, 0, 0, 0, 4, 0, 0]);
    assert_eq!(
        &m[11..m.len() - 1],
        b"Restarting for an update in 5 minutes"
    );
    assert_eq!(m.last(), Some(&0));
    let long = instance::system_line(&"x ".repeat(400));
    assert_eq!(long.len(), 11 + 255 + 1);
    assert!(!long[11..long.len() - 1].contains(&0));
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn announce_reaches_every_player() {
    let s = Server::new("announce");
    let (g1, g2) = (s.start(1, 0), s.start(2, 0));
    let mut a = Player::join(&g1, s.seat(1, "Alpha", Some(4)), "Alpha", 0).unwrap();
    let mut b = Player::join(&g1, s.seat(2, "Beta", Some(1)), "Beta", 0).unwrap();
    let mut c = Player::join(&g2, s.seat(3, "Gamma", Some(2)), "Gamma", 0).unwrap();
    until_in_game(&g1, &mut [&mut a, &mut b]);
    until_in_game(&g2, &mut [&mut c]);
    let text = "Restarting for an update in 10 minutes";
    assert_eq!(s.games.announce(text), 3);
    let want = instance::system_line(text);
    let until = Instant::now() + Duration::from_secs(10);
    for p in [&mut a, &mut b, &mut c] {
        while !p.got.contains(&want) {
            p.pump().unwrap();
            assert!(Instant::now() < until, "the line never came");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn shutdown_saves_everyone_and_closes() {
    let mut s = Server::new("shutdown");
    let g = s.start(1, 0);
    let seat = s.seat(3, "Stayer", Some(2));
    let mut p = Player::join(&g, seat, "Stayer", 0).unwrap();
    until_in_game(&g, &mut [&mut p]);
    s.games.shutdown_all(Duration::from_secs(20));
    let mut seen = s.drain();
    s.wait_event(&mut seen, |e| matches!(e, GameEvent::Saved { .. }));
    s.wait_event(&mut seen, |e| {
        matches!(e, GameEvent::Closed { game: 1, .. })
    });
    assert!(
        s.sink.record(AccountId(3), "Stayer").is_some(),
        "the character was saved at shutdown"
    );
}

/// The games hold no character folder: a seat brings its record, saves go
/// to the sink. A whole round (a new character plays, is saved, leaves and
/// comes back in a new game) leaves nothing in the server's folder but its
/// log.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn games_never_open_the_character_folder() {
    let mut s = Server::new("no-folder");
    let g = s.start(1, 0);
    let mut p = Player::join(&g, s.seat(5, "Roamer", Some(3)), "Roamer", 0).unwrap();
    until_in_game(&g, &mut [&mut p]);
    p.send(Lane::System, vec![0x69]);
    let mut seen = Vec::new();
    s.wait_event(&mut seen, |e| matches!(e, GameEvent::Saved { .. }));
    drop(p);
    s.wait_event(&mut seen, |e| {
        matches!(e, GameEvent::Closed { game: 1, .. })
    });
    let seat = s.seat(5, "Roamer", None);
    assert!(seat.record.is_some(), "the sink has the record");
    let g2 = s.start(2, 0);
    let mut back = Player::join(&g2, seat, "Roamer", 0).unwrap();
    until_in_game(&g2, &mut [&mut back]);
    s.games.shutdown_all(Duration::from_secs(20));

    let mut files = Vec::new();
    let mut dirs = vec![s.dir.0.clone()];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let path = e.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.push(path.strip_prefix(&s.dir.0).unwrap().to_path_buf());
            }
        }
    }
    assert_eq!(
        files,
        [Path::new("logs").join(d2_service::log::FILE_NAME)],
        "the games wrote only the log"
    );
}

/// The game thread hands a save off and goes on: a store slower than many
/// frames doesn't stall the tick, and the lobby hears of the save once it
/// is stored.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_slow_store_does_not_stall_the_game() {
    const STORE: Duration = Duration::from_millis(500);
    let slow = QueuedSink::new("saves-slow", |_| {
        std::thread::sleep(STORE);
        Ok(())
    });
    let mut s = Server::with_sink(
        "slow-store",
        config(),
        MemorySink::default(),
        Arc::new(slow),
    );
    let g = s.start(1, 0);
    let mut p = Player::join(&g, s.seat(6, "Patient", Some(1)), "Patient", 0).unwrap();
    until_in_game(&g, &mut [&mut p]);
    let started = Instant::now();
    let (took, results) = g
        .with_thread(|i| {
            let t = Instant::now();
            let results = i.hub.host.game.save_characters();
            (t.elapsed(), results)
        })
        .unwrap();
    assert!(results.iter().all(|(_, r)| r.is_ok()), "{results:?}");
    assert!(
        took < STORE / 5,
        "the save held the game thread for {took:?}"
    );
    // The game plays on while the store runs.
    let frame = Instant::now();
    g.with_thread(|_| ()).unwrap();
    assert!(
        frame.elapsed() < STORE / 5,
        "the game thread was busy for {:?}",
        frame.elapsed()
    );
    let mut seen = Vec::new();
    s.wait_event(&mut seen, |e| matches!(e, GameEvent::Saved { .. }));
    assert!(
        started.elapsed() >= STORE,
        "Saved came before the store ended"
    );
}

/// Each seat numbers its saves 1, 2, 3, ... (contracts 11.6: a main
/// server stores a save only when its number rises).
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn each_seat_numbers_its_saves() {
    let seqs = Arc::new(Mutex::new(Vec::new()));
    let kept = seqs.clone();
    let numbered = QueuedSink::new("saves-numbered", move |s| {
        kept.lock().unwrap().push((s.character.clone(), s.seq));
        Ok(())
    });
    let s = Server::with_sink("seq", config(), MemorySink::default(), Arc::new(numbered));
    let g = s.start(1, 0);
    let mut a = Player::join(&g, s.seat(1, "Akara", Some(1)), "Akara", 0).unwrap();
    let mut b = Player::join(&g, s.seat(2, "Charsi", Some(4)), "Charsi", 0).unwrap();
    until_in_game(&g, &mut [&mut a, &mut b]);
    for _ in 0..3 {
        let results = g
            .with_thread(|i| i.hub.host.game.save_characters())
            .unwrap();
        assert!(results.iter().all(|(_, r)| r.is_ok()), "{results:?}");
    }
    let until = Instant::now() + Duration::from_secs(10);
    while seqs.lock().unwrap().len() < 6 {
        assert!(Instant::now() < until, "saves: {:?}", seqs.lock().unwrap());
        std::thread::sleep(Duration::from_millis(5));
    }
    for name in ["Akara", "Charsi"] {
        let mine: Vec<u32> = seqs
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, q)| *q)
            .collect();
        assert_eq!(mine, [1, 2, 3], "{name}");
    }
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_game_nobody_enters_closes_and_a_panicking_game_closes_alone() {
    let mut s = Server::new("close");
    let unused = s.start(1, 0);
    std::thread::sleep(Duration::from_millis(300));
    s.games.reap();
    let mut seen = Vec::new();
    s.wait_event(&mut seen, |e| {
        *e == GameEvent::Closed {
            game: 1,
            reason: ErrorCode::GAME_CLOSED,
        }
    });
    assert!(unused.is_closed());
    assert!(s.games.get(1).is_none());

    let broken = s.start(2, 0);
    let ok = s.start(3, 0);
    let mut a = Player::join(&broken, s.seat(1, "Unlucky", Some(0)), "Unlucky", 0).unwrap();
    let mut b = Player::join(&ok, s.seat(2, "Lucky", Some(0)), "Lucky", 0).unwrap();
    until_in_game(&broken, &mut [&mut a]);
    until_in_game(&ok, &mut [&mut b]);
    let _ = broken.with_thread(|_: &mut Instance| -> () { panic!("a game fault") });
    s.wait_event(&mut seen, |e| {
        matches!(e, GameEvent::Closed { game: 2, .. })
    });
    // The other game plays on.
    b.pump().unwrap();
    assert_eq!(
        state(&ok, b.client()),
        Some(client_state::IN_GAME),
        "the other game is unaffected"
    );
    assert!(!ok.is_closed());
}

/// A realm game's start: a fixed seed and a first block of one ID, so the
/// game asks for more at once.
struct OneId;

impl SeedSource for OneId {
    fn seed(&self, _spec: &GameSpec) -> u32 {
        7
    }

    fn item_ids(&self, _spec: &GameSpec) -> Option<IdBlock> {
        Some(IdBlock {
            first: 5000,
            count: 1,
        })
    }
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_realm_game_asks_for_item_ids_and_numbers_its_items_from_the_blocks() {
    let mut s = Server::with_seeds("realm-ids", Arc::new(OneId));
    let g = s.start(1, 0);
    let mut p = Player::join(&g, s.seat(3, "Counted", Some(4)), "Counted", 0).unwrap();
    until_in_game(&g, &mut [&mut p]);
    // One ID is below the low-water mark: the game asked at once.
    let mut seen = Vec::new();
    s.wait_event(&mut seen, |e| *e == GameEvent::ItemIdsWanted { game: 1 });
    let more = IdBlock {
        first: 9000,
        count: 4096,
    };
    g.grant_item_ids(more).unwrap();
    let overlap = IdBlock {
        first: 9001,
        count: 1,
    };
    assert!(g.grant_item_ids(overlap).is_err(), "an overlapping block");
    // A save draws the starting items' IDs: the first block's one, then
    // the granted block's.
    let (granted, drawn, remaining, starved) = g
        .with_thread(|i| {
            let sim = &mut i.hub.host.game;
            assert!(sim.save_characters().iter().all(|(_, r)| r.is_ok()));
            let b = sim.item_ids().blocks().expect("realm ids");
            (b.granted().to_vec(), b.drawn(), b.remaining(), b.starved())
        })
        .unwrap();
    assert_eq!(
        granted,
        [
            IdBlock {
                first: 5000,
                count: 1
            },
            more
        ]
    );
    assert!(drawn >= 2, "the starting items took {drawn} IDs");
    assert_eq!(starved, 0, "every item got an ID");
    assert_eq!(remaining, 1 + 4096 - drawn);
}

/// A test sink: every chunk of every game, as cut.
#[derive(Clone, Default)]
struct Chunks(Arc<Mutex<Vec<journal::JournalChunk>>>);

impl journal::JournalSink for Chunks {
    fn chunk(&self, c: journal::JournalChunk) {
        self.0.lock().unwrap().push(c);
    }
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_game_records_its_journal_from_start_to_end() {
    use d2_journal::format::leave;
    use d2_journal::{read_journal, Record};

    let mut s = Server::new("journal");
    let chunks = Chunks::default();
    let to = chunks.clone();
    s.games.set_journals(Some(JournalSetup {
        version: "test".into(),
        data_hash: [7; 32],
        cfg: journal::RecorderConfig {
            hash_every: 60,
            chunk_ticks: 60,
        },
        sinks: Arc::new(move |_| Box::new(to.clone()) as Box<dyn journal::JournalSink>),
    }));
    let g = s.start(1, 0);
    let seat = s.seat(11, "Scribe", Some(3));
    let joined = journal::join_digest(&seat);
    let mut p = Player::join(&g, seat, "Scribe", 0).unwrap();
    until_in_game(&g, &mut [&mut p]);
    p.send(Lane::System, vec![0x69]);
    let mut seen = Vec::new();
    s.wait_event(&mut seen, |e| matches!(e, GameEvent::Saved { .. }));
    drop(p);
    s.wait_event(&mut seen, |e| {
        matches!(e, GameEvent::Closed { game: 1, .. })
    });

    let until = Instant::now() + Duration::from_secs(5);
    while !chunks.0.lock().unwrap().iter().any(|c| c.last) {
        assert!(Instant::now() < until, "no last chunk");
        std::thread::sleep(Duration::from_millis(5));
    }
    let cut = chunks.0.lock().unwrap().clone();
    assert!(cut
        .iter()
        .enumerate()
        .all(|(i, c)| c.game == 1 && c.seq == i as u32));
    let file: Vec<u8> = cut.iter().flat_map(|c| c.gz.iter().copied()).collect();
    let j = read_journal(&file).unwrap();
    assert!(j.ended);
    assert_eq!(j.chunks as usize, cut.len());
    let start = j.start();
    assert_eq!((start.version.as_str(), start.data_hash), ("test", [7; 32]));
    assert_eq!((start.spec.id, start.player_hosted), (1, false));
    let records: Vec<&Record> = j.entries.iter().map(|e| &e.record).collect();
    let client = records
        .iter()
        .find_map(|r| match r {
            Record::Join {
                client,
                lease,
                record,
            } => {
                assert_eq!(
                    (*lease, *record),
                    (11, joined),
                    "the seat's lease and record"
                );
                Some(*client)
            }
            _ => None,
        })
        .expect("a Join record");
    assert!(records.iter().any(
        |r| matches!(r, Record::Intent { client: c, system: true, msg } if *c == client && msg[..] == [0x69])
    ));
    assert!(records.iter().any(|r| matches!(r, Record::StateHash(_))));
    assert!(records
        .iter()
        .any(|r| matches!(r, Record::Save { lease: 11, .. })));
    assert!(records.iter().any(
        |r| matches!(r, Record::Leave { client: c, how } if *c == client && *how == leave::LEFT)
    ));
    assert!(matches!(records.last(), Some(Record::End { .. })));
}

/// A tiny mod game: its rules count the players that joined its games.
#[derive(Debug, Default)]
struct TinyMod {
    joined: std::sync::atomic::AtomicU32,
}

impl d2_sim::rules::ModRules for TinyMod {
    fn name(&self) -> &str {
        "tiny"
    }
    fn events(&self) -> Option<&dyn d2_sim::rules::EventHooks> {
        Some(self)
    }
}

impl d2_sim::rules::EventHooks for TinyMod {
    fn player_joined(&self, _: &mut dyn d2_sim::rules::EventHost, _: d2_sim::units::UnitId) {
        self.joined.fetch_add(1, Ordering::SeqCst);
    }
}

/// The tiny mod's save format: D2RUST's character behind a tag.
#[derive(Debug)]
struct TaggedCodec;

const TAG: &[u8] = b"TINY";

impl CharacterCodec for TaggedCodec {
    fn extension(&self) -> &str {
        "tiny"
    }
    fn read(
        &self,
        data: &d2_server::game::LiveData,
        bytes: &[u8],
        difficulty: u8,
    ) -> Result<d2_formats::d2s::D2s, String> {
        let save = bytes.strip_prefix(TAG).ok_or("not a tiny save")?;
        D2sCodec.read(data, save, difficulty)
    }
    fn write(
        &self,
        save: &d2_formats::d2s::D2s,
        tables: &dyn d2_formats::d2s::SaveTables,
    ) -> Result<Vec<u8>, String> {
        Ok([TAG, &D2sCodec.write(save, tables)?].concat())
    }
}

/// A mod game's server ([`Games::with_rules`]): its games are built with
/// the mod's rules (its hooks run, the game reports its ruleset), its
/// characters are saved in the mod's format, and a new game loads them
/// back through it.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_mod_games_server_plays_its_rules_and_its_save_format() {
    let tiny = Arc::new(TinyMod::default());
    let rules = GameRules {
        rules: Ruleset::Mod(tiny.clone()),
        codec: Arc::new(TaggedCodec),
    };
    let mut s = Server::with_rules("mod", rules);
    assert_eq!(s.games.rules().codec.extension(), "tiny");
    let g = s.start(1, 0);
    let mut p = Player::join(&g, s.seat(3, "Tinker", Some(2)), "Tinker", 0).unwrap();
    until_in_game(&g, &mut [&mut p]);
    let name = g
        .with_thread(|i| {
            let rules = &i.hub.host.game.rules;
            rules.mod_rules().map(|r| r.name().to_owned())
        })
        .unwrap();
    assert_eq!(name.as_deref(), Some("tiny"));
    assert_eq!(tiny.joined.load(Ordering::SeqCst), 1, "the mod's hook ran");

    p.send(Lane::System, vec![0x69]);
    let mut seen = Vec::new();
    s.wait_event(&mut seen, |e| matches!(e, GameEvent::Saved { .. }));
    drop(p);
    s.wait_event(&mut seen, |e| {
        matches!(e, GameEvent::Closed { game: 1, .. })
    });
    let record = s.sink.record(AccountId(3), "Tinker").unwrap();
    assert!(record.starts_with(TAG), "saved in the mod's format");

    // The tagged record is no `.d2s`: only the mod's codec loads it.
    let g2 = s.start(2, 0);
    let mut p2 = Player::join(&g2, s.seat(3, "Tinker", None), "Tinker", 0).unwrap();
    until_in_game(&g2, &mut [&mut p2]);
    assert_eq!(tiny.joined.load(Ordering::SeqCst), 2);
}

/// D2RUST's own games: [`Ruleset::Original`] and `.d2s` records.
#[test]
fn a_servers_games_play_d2rusts_rules_unless_given_others() {
    let own = GameRules::default();
    assert!(own.rules.is_original());
    assert_eq!(own.codec.extension(), "d2s");
}

/// A tiny mod whose stash folds a `r01` rune item into two credits.
#[derive(Debug, Default)]
struct FoldMod;

impl d2_sim::rules::ModRules for FoldMod {
    fn name(&self) -> &str {
        "fold"
    }
    fn stash(&self) -> Option<&dyn d2_sim::rules::StashHooks> {
        Some(self)
    }
}

impl d2_sim::rules::StashHooks for FoldMod {
    fn credits(
        &self,
        items: &mut dyn d2_sim::rules::ItemHost,
        player: d2_sim::units::UnitId,
        guid: u32,
    ) -> Vec<d2_sim::rules::Credit> {
        let folded = items
            .items(player)
            .into_iter()
            .any(|i| i.guid == guid && &i.code == b"r01 ");
        if folded {
            vec![("r03".into(), 3), ("r04".into(), 2)]
        } else {
            Vec::new()
        }
    }
}

/// What `transfer::out` made of putting a fresh `code` item into the stash
/// as a stackable: the transfer's tag and what the journal wrote.
fn fold_put(
    s: &mut Server,
    rules: GameRules,
    code: [u8; 4],
) -> (transfer::TransferGoods, Vec<d2_journal::Record>) {
    use d2_journal::{read_journal, Record};
    use d2_net::msg::stash::goods;
    use d2_sim::rules::stash::PlaceAt;

    let chunks = Chunks::default();
    let to = chunks.clone();
    s.games = Games::with_rules(
        config(),
        data(),
        rules,
        Arc::new(ServerLog::open(&s.dir.0.join("logs")).unwrap()),
        Arc::new(s.sink.clone()),
        Arc::new(ClockSeed),
    );
    s.games.set_journals(Some(JournalSetup {
        version: "test".into(),
        data_hash: [7; 32],
        cfg: journal::RecorderConfig::default(),
        sinks: Arc::new(move |_| Box::new(to.clone()) as Box<dyn journal::JournalSink>),
    }));
    let g = s.start(1, 0);
    let mut p = Player::join(&g, s.seat(3, "Folder", Some(2)), "Folder", 0).unwrap();
    until_in_game(&g, &mut [&mut p]);
    let client = p.client();
    let out = g
        .with_thread(move |i| {
            let tag = transfer::Tag::default();
            let sim = &mut i.hub.host.game;
            let player = sim.player_of(client).expect("the player");
            let guid = d2_server::mod_items::with_items(sim, |items| {
                items.create(player, code, &[], PlaceAt::Inventory(None))
            })
            .and_then(Result::ok)
            .expect("an item is made");
            let pending = transfer::out(
                sim,
                client,
                i.journal.as_ref(),
                &tag,
                transfer::OutAsk {
                    req: 5,
                    unit: guid,
                    to: goods::STACK,
                    gold: 0,
                    invent: false,
                },
            )
            .expect("the put goes through");
            let (_, t) = tag.borrow_mut().take().expect("the save is tagged");
            let gone = d2_server::stash_items::held_item(sim, player, guid).is_none();
            // A refused put brings the item back whole.
            assert_eq!(
                transfer::undo(sim, client, i.journal.as_ref(), &pending),
                None
            );
            let back = d2_server::mod_items::with_items(sim, |items| {
                items.items(player).iter().any(|h| h.code == code)
            })
            .unwrap_or(false);
            i.journal.as_ref().unwrap().cut();
            (t.goods, gone, back)
        })
        .unwrap();
    assert!(out.1, "the item left the character");
    assert!(out.2, "an undone put brings it back");
    drop(p);
    let file: Vec<u8> = chunks
        .0
        .lock()
        .unwrap()
        .iter()
        .flat_map(|c| c.gz.iter().copied())
        .collect();
    let j = read_journal(&file).unwrap();
    let records = j
        .entries
        .iter()
        .map(|e| e.record.clone())
        .filter(|r| {
            matches!(
                r,
                Record::ItemOut { req: 5, .. } | Record::ItemCredits { .. }
            )
        })
        .collect();
    (out.0, records)
}

/// A mod game's server folds an item into credits when its stash says so:
/// the put goes up with the credits (no stack count of its own), and the
/// journal says them right after the item left.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_folded_item_goes_up_with_its_credits_and_the_journal_says_them() {
    use d2_journal::Record;

    let mut s = Server::new("fold");
    let rules = GameRules {
        rules: Ruleset::Mod(Arc::new(FoldMod)),
        codec: Arc::new(D2sCodec),
    };
    let (goods, records) = fold_put(&mut s, rules, *b"r01 ");
    let transfer::TransferGoods::Out {
        credits,
        stack,
        item,
        to,
        ..
    } = goods
    else {
        panic!("not an out");
    };
    assert_eq!(to, d2_net::msg::stash::goods::STACK);
    assert_eq!(credits, vec![("r03".to_owned(), 3), ("r04".to_owned(), 2)]);
    assert_eq!(stack, None, "a folded item has no count of its own");
    assert!(!item.is_empty(), "its record goes up with the credits");
    assert!(matches!(
        &records[..],
        [
            Record::ItemOut { stack: None, .. },
            Record::ItemCredits { req: 5, credits: c, .. }
        ] if c == &credits
    ));
}

/// Items the mod's stash folds nothing for, and every item of D2Gamma's
/// own rules, go up as before: a plain item as a count, no credits, no
/// credits record.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn an_unfolded_item_and_d2gamma_games_put_as_before() {
    use d2_journal::Record;

    let mut s = Server::new("unfolded");
    let rules = || GameRules {
        rules: Ruleset::Mod(Arc::new(FoldMod)),
        codec: Arc::new(D2sCodec),
    };
    for (rules, what) in [
        (rules(), "a mod item its stash does not fold"),
        (GameRules::default(), "D2Gamma's rules"),
    ] {
        let (goods, records) = fold_put(&mut s, rules, *b"r02 ");
        let transfer::TransferGoods::Out { credits, stack, .. } = goods else {
            panic!("not an out");
        };
        assert!(credits.is_empty(), "{what}");
        assert_eq!(stack, Some(("r02".to_owned(), 1)), "{what}");
        assert!(
            matches!(&records[..], [Record::ItemOut { stack: Some(_), .. }]),
            "{what}: {records:?}"
        );
    }
}
