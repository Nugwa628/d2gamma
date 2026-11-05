//! The test arena from Studio: the command, the trace read back, and what
//! the screen makes of it.
use d2_sim::rules::data_mod::ai::trace::{Event, EventKind, Sample, Trace};
use eframe::egui;
use mod_studio::arena::*;
use mod_studio::arena_ui::{state_color, strip_spans, ArenaPanel, ArenaTools};
use mod_studio::ToolCommand;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn sample(tick: u32, state: Option<u8>) -> Sample {
    Sample {
        tick,
        state,
        mode: 1,
        dist: 24,
        life: 100,
        max_life: 100,
        target: -1,
    }
}

fn trace(states: &[Option<u8>]) -> Trace {
    Trace {
        seed: 7,
        ticks: states.len() as u32,
        class: 3,
        ai: 20,
        monster: "zombie".into(),
        states: names(),
        phases: vec!["enrage".into()],
        windups: vec!["slam".into()],
        samples: states
            .iter()
            .enumerate()
            .map(|(i, s)| sample(i as u32 + 1, *s))
            .collect(),
        events: vec![
            Event {
                tick: 3,
                kind: EventKind::State { from: 0, to: 1 },
            },
            Event {
                tick: 4,
                kind: EventKind::WindUp { event: 0, ticks: 5 },
            },
            Event {
                tick: 5,
                kind: EventKind::Phase { phase: 0 },
            },
        ],
    }
}

fn names() -> Vec<String> {
    vec!["idle".into(), "chase".into()]
}

#[test]
fn the_command_asks_the_game_for_the_arena() {
    let options = ArenaOptions {
        class: 5,
        ticks: 300,
        seed: 9,
        distance: 30,
        level: 1,
    };
    let c = ToolCommand::arena(
        Path::new("game.exe"),
        Path::new("mods/mine"),
        Path::new("native"),
        &options,
        20,
        Path::new("out.txt"),
    );
    assert_eq!(c.program, PathBuf::from("game.exe"));
    let args: Vec<String> = c.args.iter().map(|a| a.to_string_lossy().into()).collect();
    assert_eq!(
        args,
        [
            "ai-arena",
            "--class",
            "5",
            "--ai",
            "20",
            "--seed",
            "9",
            "--ticks",
            "300",
            "--distance",
            "30",
            "--level",
            "1",
            "--mod-dir",
            "mods/mine",
            "--native",
            "native",
            "--out",
            "out.txt"
        ]
    );
}

#[test]
fn the_defaults_are_the_games() {
    let o = ArenaOptions::default();
    assert_eq!((o.ticks, o.distance, o.level), (500, 24, 2));
}

#[test]
fn stretches_are_runs_of_the_same_state() {
    let t = trace(&[Some(0), Some(0), Some(1), None, None, Some(0)]);
    let at = |from, to, state| Stretch { from, to, state };
    assert_eq!(
        stretches(&t),
        [
            at(1, 2, Some(0)),
            at(3, 3, Some(1)),
            at(4, 5, None),
            at(6, 6, Some(0))
        ]
    );
}

#[test]
fn occupancy_counts_ticks_per_state_and_notices_a_changed_graph() {
    let t = trace(&[Some(0), Some(0), Some(1), None]);
    assert_eq!(occupancy(&t, 20, &names()), Some(vec![2, 1]));
    assert_eq!(occupancy(&t, 21, &names()), None);
    assert_eq!(occupancy(&t, 20, &["idle".into()]), None);
    assert_eq!(occupancy(&t, 20, &["idle".into(), "walk".into()]), None);
}

#[test]
fn marks_are_the_windups_and_phases() {
    assert_eq!(marks(&trace(&[Some(0); 6])), [(4, false), (5, true)]);
}

#[test]
fn the_strip_covers_its_width_without_gaps() {
    let t = trace(&[Some(0), Some(0), Some(1), None, None, Some(0)]);
    let spans = strip_spans(&stretches(&t), t.ticks, 120.0);
    assert_eq!(spans.last().map(|s| s.1), Some(120.0));
    assert_eq!(spans.first().map(|s| s.0), Some(0.0));
    for w in spans.windows(2) {
        assert_eq!(w[0].1, w[1].0);
    }
}

#[test]
fn states_get_distinct_colours() {
    let colours: Vec<_> = (0..8).map(|n| state_color(Some(n))).collect();
    for (i, a) in colours.iter().enumerate() {
        for b in &colours[i + 1..] {
            assert_ne!(a, b);
        }
    }
    assert_ne!(state_color(None), colours[0]);
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mod-studio-arena-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_finished_run_is_its_trace_or_the_end_of_the_log() {
    let dir = temp("result");
    let out = dir.join("trace.txt");
    let t = trace(&[Some(0), Some(1)]);
    fs::write(&out, t.write()).unwrap();
    assert_eq!(read_result(true, "", &out).unwrap(), t);

    let log = (1..=30).map(|i| format!("line {i}\n")).collect::<String>();
    let e = read_result(false, &log, &out).unwrap_err().to_string();
    assert!(e.contains("line 30") && !e.contains("line 5\n"), "{e}");
    let e = read_result(true, "no trace here", &dir.join("missing.txt"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("no trace here"), "{e}");
    fs::write(&out, "not a trace").unwrap();
    assert!(read_result(true, "", &out).is_err());
}

fn wait(run: &ArenaRun) -> Result<Trace, String> {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(r) = run.poll() {
            return r;
        }
        assert!(Instant::now() < until, "the run never finished");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_game_that_cannot_start_is_reported() {
    let run = ArenaRun::start(
        PathBuf::from("definitely-not-a-game-executable"),
        PathBuf::from("mod"),
        PathBuf::from("native"),
        ArenaOptions::default(),
        20,
    );
    let e = wait(&run).unwrap_err();
    assert!(e.contains("Could not start"), "{e}");
}

#[cfg(unix)]
fn script(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let game = dir.join("game.sh");
    fs::write(&game, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&game, fs::Permissions::from_mode(0o755)).unwrap();
    game
}

#[cfg(unix)]
#[test]
fn a_game_that_writes_a_trace_gives_it_back() {
    let dir = temp("fake");
    let t = trace(&[Some(0), Some(1), Some(1)]);
    fs::write(dir.join("trace.txt"), t.write()).unwrap();
    // Copies the canned trace to the --out it is given (the last argument).
    let game = script(
        &dir,
        "for a; do out=\"$a\"; done\ncp \"$(dirname \"$0\")/trace.txt\" \"$out\"",
    );
    let run = ArenaRun::start(game, dir.clone(), dir, ArenaOptions::default(), 20);
    assert_eq!(wait(&run).unwrap(), t);
}

#[cfg(unix)]
#[test]
fn a_run_can_be_cancelled() {
    let dir = temp("slow");
    let game = script(&dir, "sleep 30");
    let run = ArenaRun::start(game, dir.clone(), dir, ArenaOptions::default(), 20);
    std::thread::sleep(Duration::from_millis(300));
    run.cancel();
    assert_eq!(wait(&run).unwrap_err(), "Cancelled");
}

fn frame(panel: &mut ArenaPanel, tools: &ArenaTools, e: &mod_studio::ai::Entry) -> bool {
    let ctx = egui::Context::default();
    let mut run = false;
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                run |= panel.show(ui, &Default::default(), tools, e, None);
            });
        });
    }
    run
}

#[test]
fn the_panel_draws_without_and_with_a_result() {
    let none = ArenaTools {
        game: Path::new(""),
        native: Path::new(""),
        root: Path::new(""),
    };
    assert!(none.missing().unwrap().contains("executable"));
    let tools = ArenaTools {
        game: Path::new("g"),
        native: Path::new("n"),
        root: Path::new("r"),
    };
    assert!(tools.missing().is_none());
    let entries = mod_studio::ai::parse(include_str!(
        "../../../templates/monster-ai-graph/files/ai.toml"
    ))
    .unwrap();
    let mut e = entries[0].clone();
    let mut panel = ArenaPanel::default();
    assert!(!frame(&mut panel, &none, &e));
    assert!(panel.ticks_for(&e).is_none());

    let mut t = trace(&[Some(0), Some(1), Some(1), None]);
    t.ai = e.index;
    t.states = e.state_names();
    panel.result = Some(t);
    assert_eq!(panel.ticks_for(&e).unwrap().iter().sum::<u32>(), 3);
    assert!(!frame(&mut panel, &tools, &e));
    // Edited since: the trace no longer lines up.
    e.states[0].name = "renamed".into();
    assert!(panel.ticks_for(&e).is_none());
    assert!(!frame(&mut panel, &tools, &e));
}
