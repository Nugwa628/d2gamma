//! `ai-arena` end to end on the synthetic install (the five acts and a
//! `beast1` monster that runs the `Hunter` AI): the arena spawns it beside a
//! new character, records the trace of a plain run and of a run whose mod
//! replaces `Hunter` with a behaviour graph, and the same arguments give
//! the same bytes.

use std::path::{Path, PathBuf};

use d2_client::app::ai_arena::{self, ArenaArgs};
use d2_client::app::single_player::GameData;
use d2_sim::rules::data_mod::ai::trace::Trace;
use d2_sim::rules::data_mod::{AiRules, DataMod, DataModFiles};
use d2_sim::rules::Ruleset;

/// The `monstats` row of `beast1`.
const CLASS: u32 = 0;
/// The `monai` row of `Hunter`, the AI `beast1` runs.
const HUNTER: u16 = 2;

const GRAPH: &str = r#"
[[ai]]
index = 2
name = "hunter graph"
[[ai.state]]
name = "wait"
[[ai.state.transition]]
to = "hunt"
when = [{ kind = "has_target" }]
[[ai.state.action]]
kind = "idle"
[[ai.state]]
name = "hunt"
[[ai.state.action]]
kind = "attack"
when = [{ kind = "in_melee" }]
[[ai.state.action]]
kind = "move_to"
"#;

fn install() -> PathBuf {
    let dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("arena-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let data = test_fixtures::play::playable();
    test_fixtures::cache::copy_install(Path::new(env!("CARGO_TARGET_TMPDIR")), &dir, &data)
        .expect("the synthetic install builds");
    dir
}

fn args(ticks: u32, ai: Option<u16>) -> ArenaArgs {
    ArenaArgs {
        class: CLASS,
        ai,
        name: None,
        seed: 7,
        ticks,
        distance: 15,
        hurt: Vec::new(),
        // The synthetic install has no field level to warp to.
        level: 0,
        difficulty: 0,
        out: None,
        native: None,
        game_dir: None,
    }
}

fn run(dir: &Path, a: &ArenaArgs, with_graph: bool) -> Trace {
    let data = GameData::select(Some(dir)).expect("the install opens");
    let (rules, graphs) = if with_graph {
        let files = DataModFiles {
            ai: Some(("ai.toml", GRAPH)),
            ..DataModFiles::default()
        };
        let m = DataMod::from_files("arena", &files).expect("the graph is read");
        let graphs = m.ai_rules().clone();
        (m.into_ruleset(), graphs)
    } else {
        (Ruleset::Original, AiRules::none())
    };
    ai_arena::run_on(a, data, rules, &graphs, Some(dir)).unwrap_or_else(|e| panic!("{e:#}"))
}

#[test]
fn the_arena_records_a_plain_monster_and_repeats_to_the_byte() {
    let dir = install();
    let a = args(60, None);
    let first = run(&dir, &a, false);
    assert_eq!(first.ticks, 60);
    assert_eq!(first.samples.len(), 60);
    assert_eq!((first.samples[0].tick, first.samples[59].tick), (1, 60));
    // Not a graph: no state, no events.
    assert!(first.samples.iter().all(|s| s.state.is_none()));
    assert!(first.events.is_empty());
    assert!(first.samples[0].max_life > 0);
    let second = run(&dir, &a, false);
    assert_eq!(first.write(), second.write());
    let other = run(&dir, &ArenaArgs { seed: 8, ..a }, false);
    assert_eq!(other.samples.len(), 60);
}

#[test]
fn the_hurt_schedule_sets_the_monsters_life_from_its_tick() {
    let dir = install();
    let a = ArenaArgs {
        hurt: vec![(20, 50), (40, 25)],
        ..args(60, None)
    };
    let t = run(&dir, &a, false);
    let max = t.samples[0].max_life;
    assert_eq!(t.samples[0].life, max, "unhurt before the schedule");
    // The set takes hold on the next tick.
    assert_eq!(t.samples[25].life, max / 2);
    assert_eq!(t.samples[45].life, max / 4);
    // Unhurt runs are untouched by the option.
    assert_eq!(run(&dir, &args(60, None), false).samples[45].life, max);
}

#[test]
fn a_mods_graph_runs_the_monster_and_names_its_states() {
    let dir = install();
    let a = args(120, Some(HUNTER));
    let t = run(&dir, &a, true);
    assert_eq!((t.ai, t.monster.as_str()), (HUNTER, "hunter graph"));
    assert_eq!(t.states, ["wait", "hunt"]);
    assert!(
        t.samples.iter().all(|s| s.state.is_some()),
        "every tick is in a graph state"
    );
    // The same arguments, the same bytes; and the text reads back.
    assert_eq!(t.write(), run(&dir, &a, true).write());
    assert_eq!(Trace::parse(&t.write()).unwrap(), t);
}

#[test]
fn a_class_that_is_not_there_is_an_error_not_an_empty_trace() {
    let dir = install();
    let data = GameData::select(Some(&dir)).unwrap();
    let a = ArenaArgs {
        class: 9999,
        ..args(10, None)
    };
    let e =
        ai_arena::run_on(&a, data, Ruleset::Original, &AiRules::none(), Some(&dir)).unwrap_err();
    assert!(e.to_string().contains("was in the game for 0"), "{e:#}");
}

#[test]
fn closeness_sets_a_plain_run_beside_the_graph_run() {
    let dir = install();
    let a = args(60, Some(HUNTER));
    let rust: Vec<Trace> = (0..2)
        .map(|i| {
            run(
                &dir,
                &ArenaArgs {
                    seed: 7 + i,
                    ai: None,
                    ..a.clone()
                },
                false,
            )
        })
        .collect();
    let data: Vec<Trace> = (0..2)
        .map(|i| {
            run(
                &dir,
                &ArenaArgs {
                    seed: 7 + i,
                    ..a.clone()
                },
                true,
            )
        })
        .collect();
    let r = d2_client::app::ai_closeness::report("beast1 (AI 2)", 60, &rust, &data);
    assert_eq!(r.seeds, 2);
    let text = r.text();
    assert!(
        text.contains("beast1 (AI 2): Rust body vs data graph, 2 seeds x 60 ticks"),
        "{text}"
    );
    assert!(text.contains("ticks standing"), "{text}");
    assert!(text.contains("hit points the character lost"), "{text}");
}

#[test]
fn the_ai_overlay_reads_the_graph_monsters_place_while_the_arena_runs() {
    use d2_client::app::dev::ai_watch::{text_of, AiSeen, Watcher};
    use d2_client::app::single_player::local_player;
    use std::sync::{Arc, Mutex};

    let dir = install();
    let data = GameData::select(Some(&dir)).expect("the install opens");
    let files = DataModFiles {
        ai: Some(("ai.toml", GRAPH)),
        ..DataModFiles::default()
    };
    let m = DataMod::from_files("arena", &files).expect("the graph is read");
    let graphs = m.ai_rules().clone();
    let looks: Arc<Mutex<Vec<Vec<AiSeen>>>> = Arc::default();
    let (shared, g) = (looks.clone(), graphs.clone());
    let mut watcher = Watcher::default();
    let also: d2_client::app::state_dump::TickObserver = Box::new(move |h| {
        let sim = &mut h.game;
        if let Some((player, _)) = local_player(sim) {
            let seen = watcher.look(sim, &g, player);
            shared.lock().unwrap().push(seen);
        }
    });
    let a = args(120, Some(HUNTER));
    let t = ai_arena::run_with(&a, data, m.into_ruleset(), &graphs, Some(&dir), Some(also))
        .unwrap_or_else(|e| panic!("{e:#}"));
    assert_eq!(t.samples.len(), 120);
    let looks = looks.lock().unwrap();
    let last = looks
        .last()
        .and_then(|l| l.first())
        .expect("the monster is seen");
    assert_eq!(last.class, CLASS);
    assert!(last.line.starts_with("hunter graph: "), "{}", last.line);
    assert!(text_of(&looks.last().unwrap()[..]).contains("class 0"));
    // The first state, the graph's name, and no transition yet (the synthetic
    // install has no field level, so the monster does not leave `wait`).
    assert!(last.line.contains("wait for"), "{}", last.line);
    assert!(last.line.ends_with("no transition yet"), "{}", last.line);
    assert!(last.dist <= 15, "{}", last.dist);
}
