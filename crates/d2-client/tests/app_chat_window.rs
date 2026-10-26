//! The in-game chat window on the real game over the control channel
//! (`play --offscreen --control stdio`), against the in-process fake realm
//! chat (`D2RUST_CHAT=fake`): the window shows with the Game tab and the
//! realm's tabs; Enter opens its input line and a line typed there goes to
//! the game server as its chat message (C→S 0x15); the window
//! collapses to its bar, M hides and shows it, and it turns light when
//! idle. The window drags by its tab strip and resizes at a corner; it
//! collapses where it was moved, and the next game puts it back there
//! (`layouts.toml`). In single player (no fake) there is no window: the
//! top-left list stays and M opens the message log.
//!
//! Real data (`D2_GAME_DIR`) and a GPU, so `#[ignore]`. The window is
//! hidden and never takes focus; nothing reads or moves the real mouse.
//! Shots show game art: they go to the build's own temporary folder, or
//! to `D2RUST_CHAT_SHOTS` when set.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::Value;

/// A `play` game on the channel.
struct Game {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    errors: PathBuf,
}

impl Game {
    /// `tag` names the run's own save folder; `fake` turns the fake realm
    /// chat on.
    fn start(tag: &str, fake: bool) -> Self {
        Self::start_in(tag, fake, "1280x720")
    }

    /// [`Self::start`] in the frame `res` (`1280x720`, `800x600`).
    fn start_in(tag: &str, fake: bool, res: &str) -> Self {
        let dir = std::env::var_os("D2_GAME_DIR").expect("D2_GAME_DIR");
        std::fs::create_dir_all(tmp().join(tag)).expect("the save folder");
        let errors = tmp().join(format!("{tag}.stderr.log"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_D2Gamma-Game"));
        cmd.args(["play", "--game-dir"]).arg(dir);
        // A later game in the same folder plays the saved character.
        let saved = tmp().join(tag).join("Chatty.d2s");
        if saved.exists() {
            cmd.arg("--save").arg(saved);
        } else {
            cmd.args(["--new", "sorceress", "Chatty"]);
        }
        cmd.args(["--seed", "7", "--save-dir"])
            .arg(tmp().join(tag))
            .arg("--no-sound")
            .args(["--res", res, "--offscreen", "--size", res])
            .args(["--control", "stdio"])
            // Frame n at 40n ms: the same frames every run.
            .args(["--display-clock", "fixed:25"])
            .env_remove("D2RUST_CHAT")
            .env_remove("D2RUST_MARKET")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(std::fs::File::create(&errors).expect("the stderr log"));
        if fake {
            cmd.env("D2RUST_CHAT", "fake");
        }
        let mut child = cmd.spawn().expect("the game starts");
        let input = child.stdin.take().expect("stdin");
        let output = BufReader::new(child.stdout.take().expect("stdout"));
        let mut g = Game {
            child,
            input,
            output,
            errors,
        };
        assert_eq!(g.reply()["k"], "hello");
        g
    }

    /// The next protocol line; the game's own log lines are skipped.
    fn reply(&mut self) -> Value {
        loop {
            let mut line = String::new();
            let n = self.output.read_line(&mut line).expect("read");
            if n == 0 {
                let errors = std::fs::read_to_string(&self.errors).unwrap_or_default();
                let lines: Vec<&str> = errors.lines().collect();
                let tail = lines[lines.len().saturating_sub(30)..].join("\n");
                panic!("the game ended without a reply\nstderr:\n{tail}");
            }
            if line.starts_with('{') {
                return serde_json::from_str(&line).expect("a JSON reply");
            }
        }
    }

    fn send(&mut self, line: &str) -> Value {
        writeln!(self.input, "{line}").expect("write");
        self.input.flush().expect("flush");
        let r = self.reply();
        assert_ne!(r["k"], "error", "`{line}`: {r}");
        r
    }

    /// `line`, then a few passes for it to show.
    fn act(&mut self, line: &str) {
        self.send(line);
        self.send("step 3");
    }

    fn chat(&mut self) -> Value {
        self.send("state")["client"]["chat"].clone()
    }

    fn open_states(&mut self) -> Vec<u64> {
        self.send("state")["client"]["open"]
            .as_array()
            .expect("open")
            .iter()
            .filter_map(Value::as_u64)
            .collect()
    }

    /// The rect of UI element `id` (frame pixels).
    fn element(&mut self, id: &str) -> [i64; 4] {
        let ui = self.send("ui");
        let e = ui["ids"]
            .as_array()
            .expect("ids")
            .iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("{id}: {ui}"))
            .clone();
        rect(&e["rect"])
    }

    /// A left-button drag from `from` to `to` (frame pixels), through a
    /// point on the way.
    fn drag(&mut self, from: (i64, i64), to: (i64, i64)) {
        self.send(&format!("move {} {}", from.0, from.1));
        self.send(&format!("press L {} {}", from.0, from.1));
        let mid = ((from.0 + to.0) / 2, (from.1 + to.1) / 2);
        self.send(&format!("move {} {}", mid.0, mid.1));
        self.send(&format!("move {} {}", to.0, to.1));
        self.send(&format!("release L {} {}", to.0, to.1));
        self.send("step 3");
    }

    fn ids(&mut self) -> Vec<String> {
        self.send("ui")["ids"]
            .as_array()
            .expect("ids")
            .iter()
            .filter_map(|e| e["id"].as_str().map(str::to_owned))
            .collect()
    }

    fn shot(&mut self, name: &str) {
        let dir = std::env::var_os("D2RUST_CHAT_SHOTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| tmp().join("shots"));
        std::fs::create_dir_all(&dir).expect("the shot folder");
        let path = dir.join(format!("{name}.png"));
        let s = self.send(&format!("shot {}", path.display()));
        assert_eq!(s["k"], "shot", "{s}");
    }

    fn quit(mut self) {
        self.send("quit");
        drop(self.input);
        let status = self.child.wait().expect("the game exits");
        assert!(status.success(), "exit {status}");
    }
}

/// `[x, y, w, h]`.
fn rect(v: &Value) -> [i64; 4] {
    let a = v.as_array().unwrap_or_else(|| panic!("a rect: {v}"));
    [0, 1, 2, 3].map(|i| a[i].as_i64().expect("a number"))
}

fn tmp() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("chat-{}", std::process::id()))
}

/// The message log's UI state.
const MESSAGE_LOG: u64 = 0x18;

#[test]
#[ignore = "real data: needs D2_GAME_DIR and a GPU"]
fn the_chat_window_on_the_fake_realm() {
    let mut g = Game::start("online", true);
    g.send("step-ticks 20");
    let c = g.chat();
    assert_eq!(c["shown"], true, "{c}");
    assert_eq!(c["realm"], true, "{c}");
    assert_eq!(c["tab"], "game", "{c}");
    for t in ["game", "global", "mod", "guild", "dm"] {
        assert!(c["tabs"].get(t).is_some(), "tab {t}: {c}");
    }
    let ids = g.ids();
    for id in [
        "chat_window.tab(game)",
        "chat_window.tab(mod)",
        "chat_window.collapse",
        "chat_window.input",
    ] {
        assert!(ids.iter().any(|i| i == id), "{id}: {ids:?}");
    }
    g.shot("chat-window");
    // Enter opens the input line; the typed line goes to the game server,
    // which says it back to every player of the game.
    g.act("key ENTER");
    assert_eq!(g.chat()["typing"], true);
    g.act("text Hello!");
    let c = g.chat();
    assert_eq!(c["input"], "Hello!", "{c}");
    // The backlog came once, with the realm's one hello.
    let grim = c["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .filter(|l| l.as_str().is_some_and(|l| l.contains("Grim Crown")))
        .count();
    assert_eq!(grim, 1, "{c}");
    g.shot("chat-typing");
    g.act("key ENTER");
    let c = g.chat();
    assert_eq!(c["typing"], false, "{c}");
    // The line went out as the game's chat message, and the game says it
    // back once its ticks run (held frames take no time).
    g.send("step-ticks 10");
    let st = g.send("state");
    assert_eq!(st["client"]["sent"]["15"], 1, "{}", st["client"]["sent"]);
    let c = &st["client"]["chat"];
    let said = c["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .filter(|l| l.as_str() == Some("Chatty: Hello!"))
        .count();
    assert_eq!(said, 1, "{c}");
    g.shot("chat-said");
    // The realm's tabs.
    g.act("click ui:chat_window.tab(mod)");
    assert_eq!(g.chat()["tab"], "mod");
    g.shot("chat-mod-tab");
    g.act("click ui:chat_window.tab(game)");
    // Collapsed: the bar with its unread counts.
    g.act("click ui:chat_window.collapse");
    let c = g.chat();
    assert_eq!(c["collapsed"], true, "{c}");
    assert!(g.ids().iter().any(|i| i == "chat_window.expand"));
    g.shot("chat-collapsed");
    g.act("click ui:chat_window.expand");
    assert_eq!(g.chat()["collapsed"], false);
    // M hides it and shows it again; the message log stays shut.
    g.act("key M");
    assert_eq!(g.chat()["shown"], false);
    assert!(!g.open_states().contains(&MESSAGE_LOG));
    assert!(!g.ids().iter().any(|i| i.starts_with("chat_window.")));
    g.shot("chat-hidden");
    g.act("key M");
    assert_eq!(g.chat()["shown"], true);
    // Idle: the backing turns light after 5 s without input. Held frames
    // take no time on the channel, so the game runs on: 320 ticks are
    // more than 5 s at either tick rate.
    g.send("move 700 100");
    g.send("step-ticks 320");
    let c = g.chat();
    assert_eq!(c["idle"], true, "{c}");
    g.shot("chat-idle");
    g.quit();
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR and a GPU"]
fn in_single_player_there_is_no_window_and_m_opens_the_message_log() {
    let mut g = Game::start("single", false);
    g.send("step-ticks 20");
    assert!(g.chat().is_null());
    assert!(!g.ids().iter().any(|i| i.starts_with("chat_window.")));
    g.shot("single-player");
    g.act("key M");
    assert!(g.open_states().contains(&MESSAGE_LOG));
    g.shot("single-player-message-log");
    g.quit();
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR and a GPU"]
fn the_chat_window_moves_resizes_and_comes_back_in_the_next_game() {
    // Its own config folder (the save folder's parent): the layout file.
    let tag = "move/saves";
    let mut g = Game::start(tag, true);
    g.send("step-ticks 20");
    let c = g.chat();
    assert_eq!(c["moved"], false, "{c}");
    let home = rect(&c["window"]);
    g.shot("chat-default");

    // Dragged by the tab strip (left of the menu box) up and right.
    let menu = g.element("chat_window.menu");
    let grip = (menu[0] - 40, menu[1] + 7);
    g.drag(grip, (grip.0 + 300, grip.1 - 200));
    let c = g.chat();
    assert_eq!(c["moved"], true, "{c}");
    let moved = rect(&c["window"]);
    assert_eq!(
        moved,
        [home[0] + 300, home[1] - 200, home[2], home[3]],
        "{c}"
    );
    g.shot("chat-moved");

    // Resized at its bottom-right corner: wider and taller.
    let collapse = g.element("chat_window.collapse");
    let input = g.element("chat_window.input");
    let corner = (collapse[0] + collapse[2] + 2, input[1] + input[3] + 2);
    g.drag(corner, (corner.0 + 160, corner.1 + 60));
    let c = g.chat();
    let resized = rect(&c["window"]);
    assert_eq!(
        resized,
        [moved[0], moved[1], moved[2] + 160, moved[3] + 60],
        "{c}"
    );
    g.shot("chat-resized");

    // Collapsed after the move: the bar at the moved window's bottom.
    g.act("click ui:chat_window.collapse");
    let c = g.chat();
    assert_eq!(c["collapsed"], true, "{c}");
    assert_eq!(rect(&c["window"]), resized, "{c}");
    let bar = rect(&c["bar"]);
    assert_eq!(bar[1] + bar[3], resized[1] + resized[3], "{c}");
    g.shot("chat-collapsed-moved");
    g.act("click ui:chat_window.expand");
    g.quit();

    // Saved per player (the fake realm's account), scene `game`.
    let file = tmp().join("move").join("layouts.toml");
    let text = std::fs::read_to_string(&file).expect("layouts.toml");
    assert!(text.contains("scene = \"game\""), "{text}");
    assert!(text.contains("id = \"chat\""), "{text}");

    // The next game: the window where it was left.
    let mut g = Game::start(tag, true);
    g.send("step-ticks 20");
    let c = g.chat();
    assert_eq!(rect(&c["window"]), resized, "{c}");
    assert_eq!(c["moved"], true, "{c}");
    g.shot("chat-next-game");
    g.act("click ui:chat_window.menu");
    assert!(g.ids().iter().any(|i| i == "chat_window.reset"));
    g.shot("chat-menu");
    g.act("click ui:chat_window.menu");
    g.quit();

    // A smaller frame: the same size, kept by its nearest corner (bottom
    // left: the same gap to the HUD and to the frame's left edge, pulled
    // in where it would run off the right edge). The rects are in the
    // 800-column UI box, 240 columns into 1280 × 720.
    let mut g = Game::start_in(tag, true, "800x600");
    g.send("step-ticks 20");
    let c = g.chat();
    assert_eq!(c["moved"], true, "{c}");
    let small = rect(&c["window"]);
    assert_eq!([small[2], small[3]], [resized[2], resized[3]], "{c}");
    assert_eq!(small[0], (resized[0] + 240).min(800 - small[2]), "{c}");
    let hud_gap = |r: [i64; 4], h: i64| h - 104 - r[1] - r[3];
    assert_eq!(hud_gap(small, 600), hud_gap(resized, 720), "{c}");
    g.shot("chat-moved-800x600");

    // The menu's reset: the default spot again.
    g.act("click ui:chat_window.menu");
    assert!(g.ids().iter().any(|i| i == "chat_window.reset"));
    g.act("click ui:chat_window.reset");
    let c = g.chat();
    assert_eq!(c["moved"], false, "{c}");
    g.shot("chat-default-800x600");
    g.quit();

    // Reset is saved too: the next game has the default spot.
    let mut g = Game::start(tag, true);
    g.send("step-ticks 20");
    let c = g.chat();
    assert_eq!(c["moved"], false, "{c}");
    assert_eq!(rect(&c["window"]), home, "{c}");
    g.quit();
}
