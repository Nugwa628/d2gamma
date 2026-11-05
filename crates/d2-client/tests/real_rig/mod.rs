//! The skill test rig on the user's install (q-fixture-migrate; it
//! replaces the barbarian and skill-gaps rigs, which laid made-up skill,
//! state, monster and AnimData tables over the game): the play app's own
//! game and client on `$D2_GAME_DIR` with a new character of the class,
//! the named skills of the install's `skills` table learned at level 1,
//! and helpers to walk out of town, spawn a monster and send intents.
//! Every table is the install's; the tests read their expected numbers
//! from the same rows.
//!
//! Staging (test inputs, not game rules): the player's skill list is set
//! to the learned skills, and [`Rig::strengthen`] gives the player mana,
//! life, level and to-hit so a single swing lands; a monster is spawned
//! next to the player with [`Rig::spawn_monster`].
#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use d2_client::app::play::{
    add_client_data, add_game, add_preview, add_walk, send_create_game_for,
};
use d2_client::app::server_thread::ThreadLink;
use d2_client::app::single_player::{self, BuildError, Link};
use d2_client::bridge::click::{screen_to_world, ClickView};
use d2_client::bridge::link::{LinkError, Pumped, SendQueue, Sent, ServerLink};
use d2_client::bridge::local::{LocalLink, PendingSession};
use d2_client::bridge::mirror::DynLink;
use d2_client::bridge::predict::WalkTap;
use d2_client::bridge::BridgeResource;
use d2_client::controls::click::{ClickState, Kind};
use d2_client::rules::camera::{moving_to_client, Camera, FrameSize, OpenMode};
use d2_client::world_view::tile_assets::TileAssets;
use d2_client::world_view::walk::PreviewWalk;
use d2_data::tables::Skills;
use d2_server::adapters::ProtoSizes;
use d2_server::host::Host;
use d2_server::seams::Clock;
use d2_sim::items::inventory::UnitKind;
use d2_sim::items::moves::InventoryOps;
use d2_sim::items::ItemRequest;
use d2_sim::missiles::unit_flag as flags;
use d2_sim::skills::list::ListOwner;
use d2_sim::stats::stat;
use d2_sim::units::lifecycle::AllocRequest;
use d2_sim::units::{UnitId, UnitType};
use d2_sim::wiring::economy::ItemSpawn;

/// `monstats` row 5, `zombie1`: a Blood Moor native.
pub const ZOMBIE: u32 = 5;

const TOHIT: u16 = 19;

pub struct StepClock(Arc<AtomicU32>);

impl Clock for StepClock {
    fn now_ms(&mut self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }
}

/// The drained C→S messages of every server pump, as (id, fate).
type Fates = Arc<Mutex<Vec<(u8, String)>>>;

/// The shared server link; what the client receives is also kept in `rx`
/// (see [`Rig::s2c_since`]), and the fate of every C→S message the server
/// drains in `fates`: one rig step runs as many server ticks as its 40 ms
/// hold (two or three at the game's 60 Hz), so the last pump's report
/// alone misses a message drained by an earlier pump of the same step.
/// The last field makes the link act as a realm's ([`Rig::remote`]).
struct Shared<L>(
    Arc<Mutex<L>>,
    Arc<Mutex<Vec<Vec<u8>>>>,
    Arc<Mutex<Vec<Vec<u8>>>>,
    Arc<Mutex<ThreadLink<Link<StepClock>>>>,
    Fates,
    bool,
);

impl<L: ServerLink> ServerLink for Shared<L> {
    fn protocol_version(&self) -> u32 {
        self.0.lock().unwrap().protocol_version()
    }
    fn tick_rate(&self) -> d2_sim::tick::TickRate {
        self.0.lock().unwrap().tick_rate()
    }
    fn server_frame(&mut self) -> Option<i32> {
        self.0.lock().unwrap().server_frame()
    }
    // A realm's link has no server units to copy: the client has only
    // the messages.
    fn server_paths(&mut self) -> Vec<d2_client::bridge::critter_path::ServerPath> {
        if self.5 {
            return Vec::new();
        }
        self.0.lock().unwrap().server_paths()
    }
    fn step_paths(&mut self) -> Vec<d2_client::bridge::critter_path::ServerPath> {
        if self.5 {
            return Vec::new();
        }
        self.0.lock().unwrap().step_paths()
    }
    fn is_remote(&self) -> bool {
        self.5
    }
    fn set_send_now(&mut self, now: Option<u32>) {
        self.0.lock().unwrap().set_send_now(now);
    }
    fn send(&mut self, q: SendQueue, msg: &[u8]) -> Result<Sent, LinkError> {
        self.2.lock().unwrap().push(msg.to_vec());
        self.0.lock().unwrap().send(q, msg)
    }
    fn pump(&mut self) -> Result<Pumped, LinkError> {
        let pumped = self.0.lock().unwrap().pump()?;
        let drained = self.3.lock().unwrap().with(drained_of_last_pump);
        if let Ok(d) = drained {
            self.4.lock().unwrap().extend(d);
        }
        Ok(pumped)
    }
    fn set_display_catch_up(&mut self, enabled: bool) {
        self.0.lock().unwrap().set_display_catch_up(enabled);
    }
    fn tick_pending(&mut self) -> bool {
        self.0.lock().unwrap().tick_pending()
    }
    fn receive(&mut self) -> Vec<Vec<u8>> {
        let got = self.0.lock().unwrap().receive();
        self.1.lock().unwrap().extend(got.iter().cloned());
        got
    }
}

/// The C→S messages the server's last pump drained, as (id, fate).
fn drained_of_last_pump(l: &mut Link<StepClock>) -> Vec<(u8, String)> {
    l.last_frame()
        .messages
        .iter()
        .map(|m| (m.id, format!("{:?}", m.handled)))
        .collect()
}

/// The install's `skills` row `id`.
pub fn skill_row(id: usize) -> Skills {
    let d = crate::app_support::live();
    let rows: Vec<Skills> = d.tables.rows().expect("skills");
    rows.into_iter()
        .find(|r| usize::from(r.skill) == id)
        .unwrap_or_else(|| panic!("no skills row {id}"))
}

/// An excel text table of the install (`skills.txt`, `weapons.txt`, ...):
/// the header and the rows, cells by column name.
pub struct ExcelTable {
    header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl ExcelTable {
    pub fn load(file: &str) -> ExcelTable {
        use d2_data::bin::TableFiles;
        let (_, bytes) = crate::app_support::live()
            .archives
            .read_excel(file)
            .expect("excel read")
            .unwrap_or_else(|| panic!("{file} in the install"));
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let mut lines = text.lines();
        let header = lines
            .next()
            .unwrap_or("")
            .split('\t')
            .map(str::to_owned)
            .collect();
        let rows = lines
            .map(|l| l.split('\t').map(str::to_owned).collect())
            .collect();
        ExcelTable { header, rows }
    }

    /// The row whose first column is `key`.
    pub fn row(&self, key: &str) -> &[String] {
        self.rows
            .iter()
            .find(|r| r.first().map(String::as_str) == Some(key))
            .unwrap_or_else(|| panic!("no row {key:?}"))
    }

    /// The cell of `row` in column `col` ("" when empty or absent).
    pub fn cell<'a>(&self, row: &'a [String], col: &str) -> &'a str {
        let i = self
            .header
            .iter()
            .position(|h| h.eq_ignore_ascii_case(col))
            .unwrap_or_else(|| panic!("no column {col:?}"));
        row.get(i).map_or("", String::as_str)
    }
}

/// The `skills` id (`Id` column) of the install's skill named `name`.
pub fn skill_named(name: &str) -> usize {
    let t = ExcelTable::load("skills.txt");
    t.cell(t.row(name), "Id")
        .parse()
        .unwrap_or_else(|_| panic!("skill {name:?} has an Id"))
}

pub struct Rig {
    pub app: App,
    pub ms: Arc<AtomicU32>,
    pub link: Arc<Mutex<ThreadLink<Link<StepClock>>>>,
    /// The walks the rig sends, as the play app's `PredictLink` records
    /// its own: hand it to `add_walk` so the client walks them too.
    pub tap: WalkTap,
    /// Client-generated messages (raw staging sends bypass this trace).
    pub tx: Arc<Mutex<Vec<Vec<u8>>>>,
    /// Every S→C message the client received so far.
    rx: Arc<Mutex<Vec<Vec<u8>>>>,
    /// The fate of every C→S message the server drained so far.
    fates: Fates,
}

impl Rig {
    /// The joined game of a new `class` character (a
    /// `single_player::new_character` class name) with `learned` skills
    /// (install `skills` ids) at level 1.
    pub fn new(class: &str, learned: &[usize]) -> Rig {
        Rig::with_seed(class, learned, single_player::DEFAULT_SEED)
    }

    /// [`Rig::new`] on the game seed `seed`.
    pub fn with_seed(class: &str, learned: &[usize], seed: u32) -> Rig {
        Rig::build(class, learned, seed, false)
    }

    /// [`Rig::new`] with the client on a link that acts as a realm's: the
    /// same server and messages, but no single-player server copy (the
    /// client reads no server unit) and `is_remote`.
    pub fn remote(class: &str, learned: &[usize]) -> Rig {
        Rig::build(class, learned, single_player::DEFAULT_SEED, true)
    }

    fn build(class: &str, learned: &[usize], seed: u32, remote: bool) -> Rig {
        let character = single_player::new_character(class, "Test").unwrap();
        let class_id = i32::from(single_player::parse_class(class).unwrap());
        let ms = Arc::new(AtomicU32::new(1000));
        let clock = StepClock(ms.clone());
        let spawn_character = character.clone();
        let link = ThreadLink::spawn(move || {
            let g =
                single_player::build_with(&crate::app_support::game_data(), seed, spawn_character)?;
            Ok::<_, BuildError>(LocalLink::new(Host::new(
                g.sim,
                ProtoSizes,
                PendingSession::default(),
                clock,
            )))
        })
        .unwrap();
        // Use the play app's shared link, including its authoritative
        // single-player path copies, behind the packet trace.
        let shared = d2_client::app::save::SharedLink::new(link);
        let link = shared.server();
        let rx: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
        let tx: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
        let fates: Fates = Arc::default();
        let dyn_link: DynLink = Box::new(Shared(
            Arc::new(Mutex::new(shared)),
            rx.clone(),
            tx.clone(),
            link.clone(),
            fates.clone(),
            remote,
        ));
        let (dyn_link, tap) = d2_client::app::play::predict_link(dyn_link);
        let mut app = App::new();
        app.insert_resource(d2_client::bridge::mirror::ScriptedClock(ms.clone()));
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<ButtonInput<KeyCode>>();
        add_game(&mut app, dyn_link, false).unwrap();
        // The client tables before the join's messages (the 0x23 hand
        // selects need the skill rows).
        crate::app_support::live_tables(&mut app);
        app.world_mut()
            .resource_mut::<BridgeResource>()
            .0
            .set_player_anims(Arc::new(
                single_player::client_player_anims(&crate::app_support::game_data()).unwrap(),
            ));
        send_create_game_for(&mut app, &character).unwrap();
        let data = crate::app_support::game_data();
        let levels = single_player::client_level_rows(&data);
        add_client_data(
            &mut app,
            single_player::client_drlg_source(&data),
            levels.clone(),
        );
        add_preview(&mut app, levels, TileAssets::default());
        // The client walks what the rig sends (the play app's own-walk
        // prediction), so its C→S 0x5F reports the position the player
        // really has; without it the client stays at the spawn point and
        // the server walks the player back to it
        // (q-fix-real-gate-snapback).
        let speeds = single_player::walk_speeds(&data, &character).unwrap();
        add_walk(&mut app, tap.clone(), speeds);
        let mut r = Rig {
            app,
            ms,
            link,
            tap,
            tx,
            rx,
            fates,
        };
        while r.joined().is_none() {
            r.step(1);
        }
        r.step(10);
        let learned: Vec<u16> = learned.iter().map(|&s| s as u16).collect();
        r.with(move |sim, p| {
            let h = &mut sim.events.action.sys.hooks;
            let rows = h.tables.skills.skills.clone();
            let list = h.skill_lists.entry(p).or_default();
            *list = Default::default();
            let mut ids = [0xFFFFu16; 10];
            for (slot, s) in ids.iter_mut().zip(&learned) {
                *slot = *s;
            }
            list.init_player(&rows, ListOwner::player(class_id), Some(&ids))
                .unwrap();
        });
        r
    }

    fn joined(&self) -> Option<UnitId> {
        self.link
            .lock()
            .unwrap()
            .with(|l| single_player::local_player(&l.host().game).map(|(p, _)| p))
            .unwrap()
    }

    /// How many S→C messages the client has received so far (a mark for
    /// [`Rig::s2c_since`]).
    pub fn s2c_mark(&self) -> usize {
        self.rx.lock().unwrap().len()
    }

    /// Whether the client received a message with id `id` after `mark`.
    pub fn s2c_since(&self, mark: usize, id: u8) -> bool {
        self.rx.lock().unwrap()[mark..]
            .iter()
            .any(|m| m.first() == Some(&id))
    }

    /// How many S→C messages with id `id` the client has received so far.
    pub fn s2c_count(&self, id: u8) -> usize {
        self.rx
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.first() == Some(&id))
            .count()
    }

    pub fn step(&mut self, n: usize) {
        for _ in 0..n {
            self.app.update();
            self.ms.fetch_add(40, Ordering::SeqCst);
        }
    }

    /// The server ticks the client has seen.
    fn server_ticks(&self) -> u64 {
        self.app
            .world()
            .resource::<BridgeResource>()
            .0
            .world()
            .server_ticks
    }

    /// Runs updates at the current time until one sees no new server tick:
    /// the ticks a [`Self::step`] left due have all run, and the next
    /// [`Self::tick`] sees exactly one.
    pub fn settle_ticks(&mut self) {
        loop {
            let seen = self.server_ticks();
            self.app.update();
            if self.server_ticks() == seen {
                return;
            }
        }
    }

    /// Runs updates 1 ms apart until the client has seen one more server
    /// tick: the update that sees it runs at the tick's scheduled time, so
    /// the next tick is still a whole tick length ahead. For checks of what
    /// the game advances once per tick (an animation frame, a motion
    /// record), where one 40 ms [`Self::step`] runs two or three ticks at
    /// the game's 60 Hz. Call [`Self::settle_ticks`] after a step first.
    pub fn tick(&mut self) {
        let start = self.server_ticks();
        while self.server_ticks() == start {
            self.ms.fetch_add(1, Ordering::SeqCst);
            self.app.update();
        }
        assert_eq!(self.server_ticks(), start + 1, "one server tick");
    }

    /// Runs `f` on the server's sim with the local player.
    pub fn with<R: Send + 'static>(
        &mut self,
        f: impl FnOnce(&mut single_player::Sim, UnitId) -> R + Send + 'static,
    ) -> R {
        self.link
            .lock()
            .unwrap()
            .with(|l| {
                let sim = &mut l.host_mut().game;
                let (p, _) = single_player::local_player(sim).expect("joined");
                f(sim, p)
            })
            .unwrap()
    }

    pub fn send(&mut self, msg: &[u8]) {
        self.tap.record(msg);
        self.link
            .lock()
            .unwrap()
            .send(SendQueue::Game, msg)
            .unwrap();
    }

    pub fn pos(&mut self) -> (i32, i32) {
        self.with(|sim, p| sim.events.action.sys.hooks.path_position(p))
    }

    pub fn mode(&mut self) -> u32 {
        self.with(|sim, p| sim.events.action.sys.units.get(p).map_or(0, |u| u.mode))
    }

    pub fn level(&mut self) -> Option<u32> {
        self.with(|sim, p| {
            let room = sim.game.lists.unit(p)?.room()?;
            sim.events.action.hooks().drlg.level_id(&sim.game, room)
        })
    }

    /// A run leg (C→S 0x03), then frames until the player stops.
    pub fn leg(&mut self, (x, y): (i32, i32)) {
        let mut m = vec![0x03];
        m.extend_from_slice(&(x as u16).to_le_bytes());
        m.extend_from_slice(&(y as u16).to_le_bytes());
        self.send(&m);
        self.step(2);
        for _ in 0..400 {
            if !test_fixtures::host::MOVING.contains(&self.mode()) {
                break;
            }
            self.step(1);
        }
    }

    /// Out of the Rogue Encampment into the Blood Moor on foot (run legs
    /// along the collision route, `test_fixtures::host::route`), then
    /// twenty sub-tiles on into the level (a monster in a town room is no
    /// target). The town's NPCs walk their map-AI paths,
    /// so a leg can be cut short near the gate:
    /// the route is retried from where the player stands, up to 12 times.
    pub fn leave_town(&mut self) {
        let (town, moor) = (single_player::ACT1_TOWN, single_player::BLOOD_MOOR);
        for _ in 0..12 {
            if self.level() == Some(moor) {
                break;
            }
            let start = self.pos();
            let legs = self.with(move |sim, _| {
                let h = sim.events.action.hooks();
                let d = h.drlg.dungeon.acts[0].as_ref().expect("Act I");
                let rect = |id| {
                    d.find_level(id)
                        .map(|l| d.level(l).rect)
                        .expect("level allocated")
                };
                test_fixtures::host::route(d, start, rect(town), rect(moor), 12)
            });
            for g in legs {
                self.leg(g);
                if self.level() == Some(moor) {
                    break;
                }
            }
        }
        assert_eq!(self.level(), Some(moor), "walked into the Blood Moor");
        // On past the border: a free spot with room around it.
        let p = self.pos();
        let moor_rect = self.with(move |sim, _| {
            let d = sim.events.action.hooks().drlg.dungeon.acts[0]
                .as_ref()
                .unwrap();
            let l = d.find_level(moor).unwrap();
            d.level(l).rect
        });
        let centre = (
            (moor_rect.x * 2 + moor_rect.w) * 5 / 2,
            (moor_rect.y * 2 + moor_rect.h) * 5 / 2,
        );
        self.leg((
            p.0 + (centre.0 - p.0).clamp(-20, 20),
            p.1 + (centre.1 - p.1).clamp(-20, 20),
        ));
        self.step(4);
    }

    /// Staging: mana and life 100, character level 30 (the highest
    /// `reqlevel` of the skills these tests learn), to-hit 1000, so a
    /// swing lands and a cast is paid.
    pub fn strengthen(&mut self) {
        self.with(move |sim, p| {
            sim.events.action.with(&mut sim.game, |_, v| {
                for st in [stat::MANA, stat::MAXMANA, stat::HITPOINTS, stat::MAXHP] {
                    v.set_base(p, st, 100 << 8);
                }
                v.set_base(p, stat::LEVEL, 30);
                v.set_base(p, TOHIT, 1000);
            });
        });
    }

    /// A zombie ([`ZOMBIE`]) `dx` sub-tiles east of the player, created
    /// as monster creation leaves it (no AI started), with 100 life.
    pub fn spawn_monster(&mut self, dx: i32) -> UnitId {
        self.spawn_monster_at(dx, 0)
    }

    /// [`Rig::spawn_monster`] `dx`, `dy` sub-tiles from the player.
    pub fn spawn_monster_at(&mut self, dx: i32, dy: i32) -> UnitId {
        self.with(move |sim, p| {
            let a = &mut sim.events.action;
            let (px, py) = a.sys.hooks.path_position(p);
            let room = sim.game.lists.unit(p).and_then(|e| e.room()).expect("room");
            let req = AllocRequest {
                ty: UnitType::Monster,
                class: ZOMBIE,
                room: Some(room),
                add: true,
                fixed_guid: None,
                mode: 1,
                allied: false,
            };
            let m = a
                .with(&mut sim.game, |g, v| v.allocate(g, &req, px + dx, py + dy))
                .expect("monster");
            a.with(&mut sim.game, |_, v| {
                v.set_base(m, stat::LEVEL, 1);
                v.set_base(m, stat::MAXHP, 100 << 8);
                v.set_base(m, stat::HITPOINTS, 100 << 8);
            });
            a.sys.units.get_mut(m).unwrap().flags |=
                flags::BIT1 | flags::IS_VALID_TARGET | flags::CAN_BE_ATTACKED;
            m
        })
    }

    /// Makes `skill` the player's right skill.
    pub fn select_right(&mut self, skill: usize) {
        self.with(move |sim, p| {
            let h = &mut sim.events.action.sys.hooks;
            let list = h.skill_lists.get_mut(&p).expect("list");
            let i = list
                .view()
                .iter()
                .position(|e| e.skill == skill as i32)
                .expect("learned");
            list.right = Some(i);
        });
    }

    /// Makes `skill` the client's right skill as the skill menu does: the
    /// client learns it (S→C 0x94; [`Rig::new`] stages the list on the
    /// server only) and the picker's C→S 0x3C selects it, the server
    /// answering 0x23.
    pub fn select_right_as_client(&mut self, skill: usize) {
        let guid = self.with(|sim, p| sim.game.lists.unit(p).expect("player").guid);
        let mut learned = vec![0x94, 1];
        learned.extend(guid.to_le_bytes());
        learned.extend((skill as u16).to_le_bytes());
        learned.push(1);
        self.app
            .world_mut()
            .resource_mut::<BridgeResource>()
            .0
            .receive_chunk(&learned)
            .unwrap();
        self.send(&d2_client::ui::original::hud_hotkeys::select(skill as i32, false, u32::MAX).0);
        self.step(4);
        let bridge = &self.app.world().resource::<BridgeResource>().0;
        let list = bridge.world().local().unwrap().skills.as_ref().unwrap();
        let right = list.right.expect("a right skill");
        assert_eq!(
            usize::from(list.entries[right].skill),
            skill,
            "the client's right skill"
        );
    }

    /// The right skill at the point `dx`, `dy` sub-tiles from the
    /// player's predicted cell, clicked as a player clicks it: a right
    /// press and its release before the next update, through the client's
    /// world-click dispatcher with no unit hovered. The press starts the
    /// client's own cast (the local player's mode request) and sends
    /// C→S 0x0C. The server sends the caster's own client no skill
    /// message, so the client draws only the casts it started itself.
    pub fn right_click_point_as_client(&mut self, dx: i32, dy: i32) {
        let size = FrameSize::D2RUST;
        let (at, mode, speeds) = {
            let walk = self.app.world().resource::<PreviewWalk>();
            (walk.pump_position(), walk.predict.mode(), walk.speeds)
        };
        let (x16, y16) = at.expect("the predicted player");
        let camera = Camera::new(size, OpenMode::NONE, moving_to_client(x16, y16), (0, 0));
        let goal = ((x16 >> 16) as i32 + dx, (y16 >> 16) as i32 + dy);
        let mouse = (
            16 * (goal.0 - goal.1) - camera.unit.x + camera.view.shift_x,
            8 * (goal.0 + goal.1) + 8 - camera.unit.y,
        );
        assert_eq!(screen_to_world(&camera, mouse.0, mouse.1), goal);
        let mut state = ClickState::default();
        for kind in [Kind::RightDown, Kind::RightUp] {
            let view = ClickView {
                size,
                ui: d2_client::ui::ui_space::UiSpace::identity(size),
                open_mode: 0,
                right_panel_bottom: size.play_height(),
                skill_y_limit: size.play_height(),
                mouse,
                game_menu_open: false,
                pick: false,
                shake: (0, 0),
                prev_hover: Some(None),
                local_mode: mode,
                speeds,
            };
            self.app
                .world_mut()
                .resource_mut::<BridgeResource>()
                .0
                .world_click_at(&mut state, view, kind, Some(mouse), 0, at)
                .unwrap();
            state.end_pass();
        }
    }

    /// C→S 0x0D: the right skill on a monster.
    pub fn right_click_unit(&mut self, m: UnitId) {
        let guid = self.with(move |sim, _| sim.game.lists.unit(m).expect("unit").guid);
        let mut msg = vec![0x0D];
        msg.extend(1u32.to_le_bytes());
        msg.extend(guid.to_le_bytes());
        self.send_traced(&msg);
    }

    /// C→S 0x10: the right skill still held on a monster (the repeat of
    /// [`Self::right_click_unit`] while the button stays down).
    pub fn right_click_unit_held(&mut self, m: UnitId) {
        let guid = self.with(move |sim, _| sim.game.lists.unit(m).expect("unit").guid);
        let mut msg = vec![0x10];
        msg.extend(1u32.to_le_bytes());
        msg.extend(guid.to_le_bytes());
        self.send_traced(&msg);
    }

    /// C→S 0x0C: the right skill at a point `dx`, `dy` sub-tiles from the
    /// player.
    pub fn right_click_point(&mut self, dx: i32, dy: i32) {
        let (px, py) = self.pos();
        let mut msg = vec![0x0C];
        msg.extend(((px + dx) as u16).to_le_bytes());
        msg.extend(((py + dy) as u16).to_le_bytes());
        self.send_traced(&msg);
    }

    pub fn stat_of(&mut self, u: UnitId, st: u16) -> i32 {
        self.with(move |sim, _| sim.events.action.with(&mut sim.game, |_, v| v.stat(u, st)))
    }

    pub fn mana(&mut self) -> i32 {
        self.with(move |sim, p| {
            sim.events
                .action
                .with(&mut sim.game, |_, v| v.stat(p, stat::MANA))
        })
    }

    pub fn life(&mut self, u: UnitId) -> i32 {
        self.stat_of(u, stat::HITPOINTS)
    }

    /// Steps `frames` frames and returns the lowest life `u` had (a hit
    /// shows there: the monster's regeneration heals it back within the
    /// frames).
    pub fn lowest_life(&mut self, u: UnitId, frames: usize) -> i32 {
        let mut low = self.life(u);
        for _ in 0..frames {
            self.step(1);
            low = low.min(self.life(u));
        }
        low
    }

    /// The unit has a state list for `state`.
    pub fn has_state(&mut self, u: UnitId, state: u16) -> bool {
        self.with(move |sim, _| {
            sim.events
                .action
                .with(&mut sim.game, |_, v| v.state_list(u, state).is_some())
        })
    }

    /// Gives the player `n` unspent skill points (stat 5).
    pub fn give_skill_points(&mut self, n: i32) {
        self.with(move |sim, p| {
            sim.events
                .action
                .with(&mut sim.game, |_, v| v.set_base(p, 5, n));
        });
    }

    /// C→S 0x3B: spend a skill point on `skill`.
    pub fn add_skill_point(&mut self, skill: usize) {
        self.send(&[0x3B, skill as u8, (skill >> 8) as u8]);
    }

    /// `stat` of the player's list of `state`.
    pub fn state_stat(&mut self, state: u16, st: u16) -> Option<i32> {
        self.with(move |sim, p| {
            sim.events
                .action
                .with(&mut sim.game, |_, v| v.state_stat(p, state, st))
        })
    }

    pub fn player(&mut self) -> UnitId {
        self.with(|_, p| p)
    }

    /// [`Rig::send`]; with `RIG_DEBUG` set, the message's fate and the
    /// player's mode over the next frames are printed (diagnostics).
    fn send_traced(&mut self, msg: &[u8]) {
        if std::env::var_os("RIG_DEBUG").is_none() {
            self.send(msg);
            return;
        }
        let fate = self.send_for_fate(msg);
        let mut modes = Vec::new();
        for _ in 0..30 {
            let m = self.mode();
            if modes.last() != Some(&m) {
                modes.push(m);
            }
            self.step(1);
        }
        let (mana, used) = self.with(|sim, p| {
            let h = &sim.events.action.sys.hooks;
            (
                sim.events.action.sys.stats.unit_base(p, stat::MANA, 0) >> 8,
                format!("{:?}", h.used_skill_of(p)),
            )
        });
        eprintln!(
            "RIG {:02X?}: {fate}; modes {modes:?}; mana {mana}; used {used}",
            &msg[..1]
        );
    }

    /// The C→S messages the server drained since the last call, every
    /// pump of every step, as (id, fate).
    pub fn drained(&mut self) -> Vec<(u8, String)> {
        std::mem::take(&mut *self.fates.lock().unwrap())
    }

    /// Sends `msg` (C→S) and steps one frame at a time until the server
    /// drains it; returns its fate.
    pub fn send_for_fate(&mut self, msg: &[u8]) -> String {
        let id = msg[0];
        self.drained();
        self.send(msg);
        for _ in 0..5 {
            self.step(1);
            if let Some((_, f)) = self.drained().into_iter().find(|(i, _)| *i == id) {
                return f;
            }
        }
        "not drained".into()
    }

    /// The items code worn at body location `loc` (client model; 4 right
    /// arm, 5 left arm).
    pub fn worn(&self, loc: u8) -> Option<[u8; 4]> {
        let w = self.app.world().resource::<BridgeResource>().0.world();
        d2_client::bridge::items::local_items(w)
            .into_iter()
            .find(|i| i.mode == 1 && i.body == loc)
            .and_then(|i| i.code)
    }

    /// Takes the item at body location `loc` off to the cursor (C→S 0x1C)
    /// and drops it (0x17), so the hand and the cursor are empty.
    pub fn take_off(&mut self, loc: u8) {
        self.send(&[0x1C, loc, 0]);
        self.step(6);
        let cursor = {
            let w = self.app.world().resource::<BridgeResource>().0.world();
            d2_client::bridge::items::cursor_item(w).map(|i| i.key.guid)
        };
        let guid = cursor.expect("the item on the cursor");
        let mut m = vec![0x17];
        m.extend_from_slice(&guid.to_le_bytes());
        self.send(&m);
        self.step(6);
        assert_eq!(self.worn(loc), None, "body location {loc} is empty");
    }

    /// Staging: makes the install's item `code` (normal quality, item
    /// level 1) and wears it at body location `loc` from the cursor (the
    /// item moves' equip).
    pub fn wear(&mut self, code: [u8; 4], loc: u8) {
        let worn = self.with(move |s, owner| {
            s.world
                .with_economy(&mut s.game, &mut s.events, |econ, parts| {
                    let record = econ
                        .tables
                        .items
                        .iter()
                        .position(|r| r.code == code)
                        .expect("an items row of the code");
                    let mut rq = ItemRequest {
                        item: record as i32,
                        ilvl: 1,
                        quality: 2,
                        format: 1,
                        ..ItemRequest::default()
                    };
                    let held = ItemSpawn {
                        room: None,
                        mode: 4,
                        init_flags: 1,
                    };
                    let item = econ.create_item(&mut rq, false, held).expect("item");
                    let inv = parts.inventory.as_deref_mut().expect("inventory model");
                    let (class, guid) = {
                        let u = econ.units.get(owner).unwrap();
                        (u.class, u.guid)
                    };
                    if !inv.state.inventories.contains_key(&owner) {
                        let kind = UnitKind::Player { class: class as u8 };
                        inv.state.add_inventory(owner, kind, guid);
                    }
                    let mut d = inv.desk(econ);
                    d.sync_in();
                    if let Some(i) = d.state.items.get_mut(&item) {
                        i.mode = d2_sim::items::inventory::mode::CURSOR;
                    }
                    d.sync_out();
                    match (d.owner_of(owner), d.guid_of(item)) {
                        (Some(o), g) => d.equip_from_cursor(o, g, loc, true).0,
                        _ => false,
                    }
                })
        });
        assert!(
            worn,
            "{} worn at body location {loc}",
            String::from_utf8_lossy(&code)
        );
        self.step(4);
    }

    /// The skill id of the player's used skill entry.
    pub fn used_skill(&mut self) -> Option<i32> {
        self.with(|sim, p| {
            sim.events
                .action
                .sys
                .hooks
                .used_skill_of(p)
                .map(|e| e.skill)
        })
    }

    /// The pets of the local player (client model).
    pub fn pets(&self) -> usize {
        self.app
            .world()
            .resource::<BridgeResource>()
            .0
            .world()
            .pets
            .len()
    }

    /// The unit has `state` (its state bits).
    pub fn state_on(&mut self, u: UnitId, state: u16) -> bool {
        self.with(move |sim, _| sim.events.action.sys.stats.has_state(u, u32::from(state)))
    }

    pub fn errors(&mut self) -> String {
        self.with(move |sim, _| format!("{:?}", sim.events.action.sys.hooks.errors))
    }
}
