//! `play --control stdio|pipe`: drive the real game (windowed or
//! `--offscreen`) line by line while it runs: pause it, step whole server
//! ticks, send pointer and key input, read its state and take shots of the
//! presented picture. It speaks the `autoplay-1` protocol of
//! `autoplay-host` ([`protocol`]), plus:
//!
//! - `pause` / `resume`: hold the game between two server ticks, or let
//!   it run on its clock again;
//! - `step-ticks N`: run exactly N server ticks, one a frame, then hold;
//! - `shot PATH`: a PNG of the presented picture (a held game's once its
//!   newest tick is drawn), replied once written;
//! - `query NAME, ...`: the named values of `repro`'s query table.
//! - `setting SECTION.KEY VALUE`: one `settings.toml` key changed live,
//!   as the settings menu changes it (look comparisons from one frame).
//!
//! The channel is off unless `--control` is given; it listens on stdin /
//! stdout or a local named pipe ([`transport`]), never on a network port.
//!
//! **Pausing** uses the game's own single-player pause, the one the Esc
//! menu gives: a paused frame pumps no server frame, so no tick runs, while
//! Bevy keeps drawing, the UI keeps taking input and the reads keep
//! answering. The held tick's picture is drawn again after each input or
//! `setting` command, so a panel a key opens shows in the next `shot`. Stepping lifts the hold until the bridge has seen N more
//! ticks; a bridge frame runs at most one tick, so the hold comes back
//! exactly after the Nth. The game starts held: the hold applies from the
//! first frame the local player stands in the game, and the `hello` reply
//! comes then, with that frame's tick.
//!
//! **Input** goes where a player's goes ([`protocol::pointer`],
//! [`protocol::key`]). While a channel is open the window's own pointer is
//! ignored (as under `--input`), so a controlled run never reads, or moves,
//! the real mouse.
//!
//! **Order**: lines run one at a time in arrival order. A command that
//! takes frames (`step`, `step-ticks`, `shot`) answers when it is done,
//! and the next line waits for that answer. Every reply is one JSON line
//! with a `k` field; on stdio the game's own log lines share stdout, so a
//! driver reads only the lines that start with `{`.
//!
//! The channel runs in `First`, before the bridge frame, so the hold, the
//! input and the reads all fall between two bridge frames.

pub mod protocol;
pub mod transport;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use serde_json::{json, Value};

use self::protocol::{Command, MessageLog};
pub use self::transport::Transport;
use self::transport::{Channel, Incoming};
use super::play::ServerClock;
use super::server_thread::ThreadLink;
use super::single_player::Link;
use crate::bridge::link::{LinkError, Pumped, SendQueue, Sent, ServerLink};
use crate::bridge::mirror::DynLink;
use crate::bridge::BridgeResource;
use crate::ui::UiEvent;
use crate::world_view::WorldViewUi;

/// `setting`: hands the changed settings to the game's UI as the settings
/// menu does, so the next frame saves and applies them the same way
/// (`config::apply_settings`). The config takes them at once too, so a
/// second `setting` in the same frame builds on the first.
fn set_setting(world: &mut World, key: &str, value: &str) -> Result<(), String> {
    let current = world
        .get_resource::<crate::app::config::ConfigRes>()
        .ok_or("this run has no settings")?
        .settings;
    let new = protocol::with_setting(&current, key, value)?;
    let mut ui = world.non_send_mut::<WorldViewUi>();
    let original = ui.original.as_mut().ok_or("the game UI is not up yet")?;
    original.edit_settings(new);
    world
        .resource_mut::<crate::app::config::ConfigRes>()
        .settings = new;
    Ok(())
}

/// The play game's server thread, shared between the app's link and the
/// channel's reads.
pub type Server = Arc<Mutex<ThreadLink<Link<ServerClock>>>>;

/// How long `step-ticks` waits for a tick before it gives up: the game is
/// held by something else (a menu, a loading screen) or the PC is stalled.
const TICK_WAIT: Duration = Duration::from_secs(120);

/// Passes a `shot` waits at most for a held game's picture to catch up
/// ([`shot_ready`]) before it takes the picture as it is.
const SHOT_WAIT_PASSES: u32 = 120;

/// Whether a held game's picture shows its state: the newest tick is drawn
/// and no redraw asked for by input is still to come. A running game's
/// picture is taken as it is.
fn shot_ready(world: &World) -> bool {
    let bridge = &world.resource::<BridgeResource>().0;
    if !bridge.is_paused() {
        return true;
    }
    let tick = bridge.world().server_ticks;
    let drawn = world
        .get_resource::<crate::world_view::present::WorldViewState>()
        .and_then(|s| s.last.as_ref())
        .map(|l| l.server_tick);
    let redraw = world
        .get_non_send::<WorldViewUi>()
        .is_some_and(|ui| ui.redraw_pending());
    drawn.is_none_or(|d| d >= tick) && !redraw
}

/// The app's link of a game that is not saved, shared with the reads:
/// every call goes to the server thread unchanged.
pub struct LockedLink(pub Server);

impl LockedLink {
    fn lock(&self) -> std::sync::MutexGuard<'_, ThreadLink<Link<ServerClock>>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl ServerLink for LockedLink {
    fn protocol_version(&self) -> u32 {
        self.lock().protocol_version()
    }
    fn tick_rate(&self) -> d2_sim::tick::TickRate {
        self.lock().tick_rate()
    }
    fn server_frame(&mut self) -> Option<i32> {
        self.lock().server_frame()
    }
    fn send(&mut self, queue: SendQueue, msg: &[u8]) -> Result<Sent, LinkError> {
        self.lock().send(queue, msg)
    }
    fn pump(&mut self) -> Result<Pumped, LinkError> {
        self.lock().pump()
    }
    fn set_display_catch_up(&mut self, enabled: bool) {
        self.lock().set_display_catch_up(enabled)
    }
    fn tick_pending(&mut self) -> bool {
        self.lock().tick_pending()
    }
    fn receive(&mut self) -> Vec<Vec<u8>> {
        self.lock().receive()
    }
    fn server_paths(&mut self) -> Vec<crate::bridge::critter_path::ServerPath> {
        super::save::server_paths_on(&mut self.lock())
    }
    fn set_send_now(&mut self, now: Option<u32>) {
        self.lock().set_send_now(now)
    }
}

/// The app's link with the message counts the `state` reply shows; every
/// call goes to `inner` unchanged.
pub struct CountingLink {
    inner: DynLink,
    sent: MessageLog,
    received: MessageLog,
}

impl ServerLink for CountingLink {
    fn protocol_version(&self) -> u32 {
        self.inner.protocol_version()
    }
    fn tick_rate(&self) -> d2_sim::tick::TickRate {
        self.inner.tick_rate()
    }
    fn server_frame(&mut self) -> Option<i32> {
        self.inner.server_frame()
    }
    fn send(&mut self, queue: SendQueue, msg: &[u8]) -> Result<Sent, LinkError> {
        protocol::count(&self.sent, msg);
        self.inner.send(queue, msg)
    }
    fn pump(&mut self) -> Result<Pumped, LinkError> {
        self.inner.pump()
    }
    fn set_display_catch_up(&mut self, enabled: bool) {
        self.inner.set_display_catch_up(enabled)
    }
    fn tick_pending(&mut self) -> bool {
        self.inner.tick_pending()
    }
    fn receive(&mut self) -> Vec<Vec<u8>> {
        let msgs = self.inner.receive();
        for m in &msgs {
            protocol::count(&self.received, m);
        }
        msgs
    }
    fn server_paths(&mut self) -> Vec<crate::bridge::critter_path::ServerPath> {
        self.inner.server_paths()
    }
    fn step_paths(&mut self) -> Vec<crate::bridge::critter_path::ServerPath> {
        self.inner.step_paths()
    }
    fn set_send_now(&mut self, now: Option<u32>) {
        self.inner.set_send_now(now)
    }
}

/// Where a shot's observer leaves the written size (or the error).
type ShotResult = Arc<Mutex<Option<Result<(u32, u32), String>>>>;

/// A command that is answered after some frames.
enum Pending {
    /// `step N`: passes still to run.
    Frames(u32),
    /// `step-ticks N`: the tick to reach; when the last tick came, and
    /// which one it was.
    Ticks {
        target: u64,
        seen: u64,
        since: Instant,
    },
    /// `shot PATH`: the PNG's result once the capture has written it;
    /// `None` while it waits for the picture ([`shot_ready`]), with the
    /// passes waited.
    Shot {
        path: PathBuf,
        tick: u64,
        done: Option<ShotResult>,
        waited: u32,
    },
}

/// The open channel of a `play --control` game.
#[derive(Resource)]
pub struct Control {
    channel: Channel,
    server: Server,
    sent: MessageLog,
    received: MessageLog,
    /// Lines received and not yet run.
    lines: VecDeque<String>,
    pending: Option<Pending>,
    /// The game is held (`pause`, `step-ticks`, and from the start).
    paused: bool,
    /// A driver is there and has no `hello` yet.
    hello_due: bool,
    /// Passes since the channel opened.
    passes: u64,
    /// Window keys pressed for this pass (`key`, `rawkey`), released at
    /// the start of the next.
    keys: Vec<KeyCode>,
}

/// A play game's link that is not shared with a save yet, shared so the
/// channel can read the game: the app's link and the shared thread.
pub fn shared(thread: ThreadLink<Link<ServerClock>>) -> (DynLink, Server) {
    let server = Arc::new(Mutex::new(thread));
    (Box::new(LockedLink(server.clone())), server)
}

/// Opens the channel of `transport` for the play game whose link is
/// `inner` and whose server thread is `server` (the same game): the app's
/// link, which now counts the messages the `state` reply shows, and the
/// channel to [`install`].
pub fn open(
    inner: DynLink,
    server: Server,
    transport: Transport,
) -> anyhow::Result<(DynLink, Control)> {
    let channel =
        transport::open(transport).map_err(|e| anyhow::anyhow!("--control {transport:?}: {e}"))?;
    eprintln!("control: listening on {}", channel.name);
    let sent = MessageLog::default();
    let received = MessageLog::default();
    let link = CountingLink {
        inner,
        sent: sent.clone(),
        received: received.clone(),
    };
    Ok((
        Box::new(link),
        Control {
            channel,
            server,
            sent,
            received,
            lines: VecDeque::new(),
            pending: None,
            paused: true,
            hello_due: false,
            passes: 0,
            keys: Vec::new(),
        },
    ))
}

/// Installs the open channel on the game app. The window's pointer is
/// handed to the channel: an empty input script takes it, as under
/// `--input`, unless the run has a script already.
pub fn install(world: &mut World, control: Control) {
    if !world.contains_resource::<crate::world_view::input_script::InputScript>() {
        world.insert_resource(crate::world_view::input_script::InputScript::new(Vec::new()));
    }
    world.insert_resource(control);
}

/// Adds the channel's system to a play app; it runs only while a
/// [`Control`] is installed.
pub fn register(app: &mut App) {
    app.add_systems(First, control_frame.run_if(resource_exists::<Control>));
}

/// One pass of the channel, before the bridge frame: release last pass's
/// keys, finish the pending command, run the waiting lines, then set the
/// bridge's hold.
fn control_frame(world: &mut World) {
    world.resource_scope(|world, mut c: Mut<Control>| {
        c.passes += 1;
        if !c.keys.is_empty() {
            let keys = std::mem::take(&mut c.keys);
            if let Some(mut input) = world.get_resource_mut::<ButtonInput<KeyCode>>() {
                for k in keys {
                    input.release(k);
                }
            }
        }
        while let Ok(i) = c
            .channel
            .incoming
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .try_recv()
        {
            match i {
                Incoming::Connected => {
                    c.lines.clear();
                    c.pending = None;
                    c.hello_due = true;
                }
                Incoming::Line(l) => c.lines.push_back(l),
                Incoming::Closed => {
                    c.lines.clear();
                    c.pending = None;
                    c.hello_due = false;
                    if c.channel.exit_on_close {
                        world.write_message(AppExit::Success);
                    }
                }
            }
        }
        let (tick, in_game) = {
            let b = &world.resource::<BridgeResource>().0;
            let w = b.world();
            (
                w.server_ticks,
                w.local().is_some_and(|u| u.position.is_some()),
            )
        };
        // Commands wait until the game can answer them.
        if c.hello_due && in_game {
            c.hello_due = false;
            let hello = json!({
                "k": "hello",
                "format": protocol::FORMAT,
                "control": c.channel.name,
                "pid": std::process::id(),
                "tick": tick,
                "paused": c.paused,
            });
            c.reply(hello);
        }
        if in_game && !c.hello_due {
            c.finish_pending(world, tick);
            while c.pending.is_none() {
                let Some(line) = c.lines.pop_front() else {
                    break;
                };
                let t = line.trim();
                if t.is_empty() || t.starts_with('#') {
                    continue;
                }
                let reply = match protocol::parse_command(t) {
                    Ok(cmd) => c.run(world, cmd, tick),
                    Err(e) => Some(json!({"k": "error", "error": e})),
                };
                if let Some(r) = reply {
                    c.reply(r);
                }
            }
        }
        let stepping = matches!(c.pending, Some(Pending::Ticks { .. }));
        let held = c.paused && !stepping;
        let mut bridge = world.resource_mut::<BridgeResource>();
        bridge.0.set_held(held);
        bridge.0.set_stepping(stepping);
    });
}

fn control_ui_elements(world: &World) -> Vec<crate::ui::ui_ids::UiElement> {
    let bridge = world.resource::<BridgeResource>();
    let ui = world.non_send::<WorldViewUi>();
    crate::world_view::ui_bind::ui_elements(&ui.root, bridge.0.world(), &*ui.strings)
}

impl Control {
    fn reply(&self, v: Value) {
        let _ = self.channel.replies.send(v.to_string());
    }

    /// Answers the pending command once its frames have run.
    fn finish_pending(&mut self, world: &mut World, tick: u64) {
        let reply = match &mut self.pending {
            None => return,
            Some(Pending::Frames(left)) => {
                *left = left.saturating_sub(1);
                if *left > 0 {
                    return;
                }
                json!({"k": "ok", "cmd": "step", "passes": self.passes, "tick": tick})
            }
            Some(Pending::Ticks {
                target,
                seen,
                since,
            }) => {
                if tick >= *target {
                    json!({"k": "ok", "cmd": "step-ticks", "tick": tick})
                } else if tick != *seen {
                    *seen = tick;
                    *since = Instant::now();
                    return;
                } else if world.resource::<BridgeResource>().0.menu_paused() {
                    json!({"k": "error", "error": "step-ticks: a game menu pauses the game", "tick": tick})
                } else if since.elapsed() > TICK_WAIT {
                    json!({"k": "error", "error": "step-ticks: no server tick came", "tick": tick})
                } else {
                    return;
                }
            }
            Some(Pending::Shot {
                path,
                done: done @ None,
                waited,
                ..
            }) => {
                if *waited < SHOT_WAIT_PASSES && !shot_ready(world) {
                    *waited += 1;
                    return;
                }
                *done = Some(capture(world, path.clone()));
                return;
            }
            Some(Pending::Shot {
                path,
                tick,
                done: Some(done),
                ..
            }) => {
                let Some(result) = done.lock().unwrap_or_else(|e| e.into_inner()).take() else {
                    return;
                };
                match result {
                    Ok((w, h)) => json!({
                        "k": "shot", "path": path.display().to_string(),
                        "tick": tick, "size": [w, h],
                    }),
                    Err(e) => {
                        json!({"k": "error", "error": format!("shot {}: {e}", path.display())})
                    }
                }
            }
        };
        self.pending = None;
        self.reply(reply);
    }

    /// Runs one command; its reply now, or `None` when it answers later.
    /// Input and settings redraw a held game's picture, so a `shot` after
    /// them shows what they changed.
    fn run(&mut self, world: &mut World, cmd: Command, tick: u64) -> Option<Value> {
        if matches!(
            cmd,
            Command::Move(_)
                | Command::Press(..)
                | Command::Release(..)
                | Command::Key(_)
                | Command::CtrlKey(_)
                | Command::RawKey(_)
                | Command::ClickUi(_)
                | Command::Text(_)
                | Command::Setting(..)
        ) {
            world.non_send_mut::<WorldViewUi>().redraw();
        }
        let ok = |what: &str| Some(json!({"k": "ok", "cmd": what, "tick": tick}));
        match cmd {
            Command::Step(0) => {
                Some(json!({"k": "ok", "cmd": "step", "passes": self.passes, "tick": tick}))
            }
            Command::Step(n) => {
                self.pending = Some(Pending::Frames(n));
                None
            }
            Command::StepTicks(n) => {
                self.paused = true;
                if n == 0 {
                    return ok("step-ticks");
                }
                if world.resource::<BridgeResource>().0.menu_paused() {
                    return Some(
                        json!({"k": "error", "error": "step-ticks: a game menu pauses the game", "tick": tick}),
                    );
                }
                self.pending = Some(Pending::Ticks {
                    target: tick + u64::from(n),
                    seen: tick,
                    since: Instant::now(),
                });
                None
            }
            Command::Pause => {
                self.paused = true;
                ok("pause")
            }
            Command::Resume => {
                self.paused = false;
                ok("resume")
            }
            Command::Shot(path) => match self.shot(world, path, tick) {
                Ok(()) => None,
                Err(e) => Some(json!({"k": "error", "error": e})),
            },
            Command::Move(p) => {
                protocol::pointer(&mut world.non_send_mut::<WorldViewUi>(), p, None);
                ok("move")
            }
            Command::Press(button, at) => {
                let e = Some(UiEvent::Press { button, at });
                protocol::pointer(&mut world.non_send_mut::<WorldViewUi>(), at, e);
                ok("press")
            }
            Command::Release(button, at) => {
                let e = Some(UiEvent::Release { button, at });
                protocol::pointer(&mut world.non_send_mut::<WorldViewUi>(), at, e);
                ok("release")
            }
            Command::Key(vk) => {
                // The settings screens' keys are theirs, as the window's
                // are (Ctrl held with `rawkey CTRL`).
                let ctrl = world
                    .get_resource::<ButtonInput<KeyCode>>()
                    .is_some_and(|k| k.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]));
                let menu_took = protocol::key_codes(vk).first().is_some_and(|&code| {
                    world
                        .run_system_cached_with(
                            |In((code, ctrl)): In<(KeyCode, bool)>,
                             mut k: crate::ui::settings::overlay::SettingsKeys| {
                                k.key(code, ctrl)
                            },
                            (code, ctrl),
                        )
                        .unwrap_or(false)
                });
                if menu_took {
                    return ok("key");
                }
                let codes = protocol::key(&mut world.non_send_mut::<WorldViewUi>(), vk);
                self.hold_keys(world, codes);
                ok("key")
            }
            Command::CtrlKey(vk) => {
                let codes = protocol::ctrl_key(&mut world.non_send_mut::<WorldViewUi>(), vk);
                self.hold_keys(world, codes);
                ok("key")
            }
            Command::RawKey(vk) => {
                self.hold_keys(world, protocol::key_codes(vk));
                ok("rawkey")
            }
            Command::ClickUnit(_)
            | Command::InteractUnit(_)
            | Command::UseUnit(_)
            | Command::WalkLevel(_) => {
                Some(json!({"k": "error", "error": "unit approach commands require autoplay-host"}))
            }
            Command::ClickUi(id) => {
                let elements = control_ui_elements(world);
                match crate::ui::ui_ids::find(&elements, &id) {
                    Ok(element) => {
                        let at = element.at;
                        let button = crate::ui::PointerButton::Left;
                        let mut ui = world.non_send_mut::<WorldViewUi>();
                        protocol::pointer(&mut ui, at, Some(UiEvent::Press { button, at }));
                        protocol::pointer(&mut ui, at, Some(UiEvent::Release { button, at }));
                        Some(json!({"k": "ok", "cmd": "click", "id": id, "at": [at.x, at.y]}))
                    }
                    Err(error) => Some(json!({"k": "error", "error": error})),
                }
            }
            Command::Ui => Some(super::autoplay_front::ui_reply(
                "game",
                &control_ui_elements(world),
            )),
            // The chat window's input line takes typed text while it has
            // the typing (Enter opens it).
            Command::Text(text) => {
                let mut ui = world.non_send_mut::<WorldViewUi>();
                if !ui.original.as_ref().is_some_and(|o| o.chat_typing()) {
                    return Some(
                        json!({"k": "error", "error": "text: the chat input only (press Enter first), or autoplay-host --front"}),
                    );
                }
                let chars = text.encode_utf16().map(UiEvent::Char);
                ui.queue.0.extend(chars);
                ok("text")
            }
            Command::Setting(key, value) => Some(match set_setting(world, &key, &value) {
                Ok(()) => json!({"k": "ok", "cmd": "setting", "key": key, "tick": tick}),
                Err(e) => json!({"k": "error", "error": format!("setting {key}: {e}")}),
            }),
            Command::State => Some(self.state(world)),
            Command::Query(names) => Some(match self.state_of(world) {
                Ok(s) => protocol::query_reply(&names, &s),
                Err(e) => json!({"k": "error", "error": format!("{e:#}")}),
            }),
            Command::Map => Some(
                protocol::map(&self.server)
                    .unwrap_or_else(|e| json!({"k": "error", "error": format!("{e:#}")})),
            ),
            Command::Quit => {
                world.write_message(AppExit::Success);
                ok("quit")
            }
        }
    }

    fn state_of(&self, world: &World) -> anyhow::Result<Value> {
        let counts = protocol::Counts {
            passes: self.passes,
            sent: &self.sent,
            received: &self.received,
        };
        protocol::state(world, &self.server, &counts)
    }

    fn state(&self, world: &World) -> Value {
        self.state_of(world)
            .unwrap_or_else(|e| json!({"k": "error", "error": format!("{e:#}")}))
    }

    /// Presses `codes` in the window's key state for this pass (the death
    /// and hardcore screens read it); the next pass releases them.
    fn hold_keys(&mut self, world: &mut World, codes: Vec<KeyCode>) {
        if let Some(mut input) = world.get_resource_mut::<ButtonInput<KeyCode>>() {
            for &k in &codes {
                input.press(k);
            }
        }
        self.keys.extend(codes);
    }

    /// Asks for a screenshot of the primary window once a held game's
    /// picture shows its state ([`shot_ready`]); the PNG is written when
    /// Bevy hands the picture back, after that frame is presented.
    fn shot(&mut self, world: &mut World, path: PathBuf, tick: u64) -> Result<(), String> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| format!("shot {}: {e}", dir.display()))?;
        }
        let done = shot_ready(world).then(|| capture(world, path.clone()));
        self.pending = Some(Pending::Shot {
            path,
            tick,
            done,
            waited: 0,
        });
        Ok(())
    }
}

/// Asks for a screenshot of the primary window, written to `to`.
fn capture(world: &mut World, to: PathBuf) -> ShotResult {
    let done = Arc::new(Mutex::new(None));
    let out = done.clone();
    // The window cursor is read when the picture comes back: the frame it
    // shows ran after this command, and no other line runs before the
    // shot is answered.
    world.spawn(Screenshot::primary_window()).observe(
        move |shot: On<ScreenshotCaptured>,
              cursor: Option<Res<crate::world_view::hw_cursor::CursorInShots>>| {
            let r = crate::world_view::shots::write_png(&shot.image, &to, cursor.as_deref());
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
        },
    );
    done
}
