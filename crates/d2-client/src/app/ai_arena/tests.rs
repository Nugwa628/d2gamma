//! The arena's parts that need no game: the options, the recorder and the
//! trace header. The whole run is `tests/ai_arena.rs`.

use super::*;
use d2_sim::debug::state::UnitState;
use d2_sim::monsters::ai::graph::GraphEvent;
use d2_sim::units::UnitId;

fn argv(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

#[test]
fn the_options_have_defaults_and_a_class_is_required() {
    let a = parse_args(&argv("--class 12")).unwrap();
    assert_eq!(a.class, 12);
    assert_eq!((a.ticks, a.distance, a.difficulty), (TICKS, DISTANCE, 0));
    assert_eq!(a.level, LEVEL);
    assert_eq!(a.seed, single_player::DEFAULT_SEED);
    assert!(a.out.is_none() && a.ai.is_none());
    let e = parse_args(&argv("--ticks 10")).unwrap_err().to_string();
    assert!(e.contains("--class"), "{e}");
}

#[test]
fn every_option_is_read() {
    let a = parse_args(&argv(
        "--class 3 --ai 200 --name Fallen --seed 9 --ticks 50 --distance 8 \
         --level 0 --difficulty hell --out t.txt --native n --game-dir g",
    ))
    .unwrap();
    assert_eq!(
        (a.class, a.ai, a.seed, a.ticks, a.distance),
        (3, Some(200), 9, 50, 8)
    );
    assert_eq!(a.name.as_deref(), Some("Fallen"));
    assert_eq!(a.difficulty, 2);
    assert_eq!(a.out, Some(PathBuf::from("t.txt")));
    assert_eq!(a.native, Some(PathBuf::from("n")));
    assert_eq!(a.game_dir, Some(PathBuf::from("g")));
}

#[test]
fn the_hurt_schedule_is_read_in_tick_order() {
    let a = parse_args(&argv("--class 3 --hurt 40:25,10:50")).unwrap();
    assert_eq!(a.hurt, [(10, 50), (40, 25)]);
    assert!(parse_args(&argv("--class 3")).unwrap().hurt.is_empty());
    for bad in [
        "--hurt 10",
        "--hurt 0:50",
        "--hurt 10:0",
        "--hurt 10:101",
        "--hurt x:50",
        "--hurt 10:50,",
    ] {
        assert!(
            parse_args(&argv(&format!("--class 3 {bad}"))).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn bad_options_are_refused() {
    for bad in [
        "--class",
        "--class x",
        "--class 1 --ticks 0",
        "--class 1 --distance 0",
        "--class 1 --distance 5000",
        "--class 1 --difficulty easy",
        "--class 1 --zzz",
    ] {
        assert!(parse_args(&argv(bad)).is_err(), "{bad}");
    }
}

fn unit(ut: UnitType, g: u32, cl: u32, m: u32, at: (u32, u32), life: (i32, i32)) -> UnitState {
    UnitState {
        ut: ut as u8,
        g,
        cl: Some(cl),
        m: Some(m),
        x: Some(at.0),
        y: Some(at.1),
        hp: Some(life.0 << 8),
        hpx: Some(life.1 << 8),
        ..UnitState::default()
    }
}

fn snap(units: Vec<UnitState>) -> StateSnapshot {
    StateSnapshot {
        frame: 0,
        seed: [0, 0],
        units,
    }
}

fn arena(monster_x: u32) -> StateSnapshot {
    snap(vec![
        unit(UnitType::Player, 1, 1, 1, (100, 100), (80, 80)),
        unit(UnitType::Monster, 7, 12, 2, (monster_x, 104), (30, 40)),
        // Another class: not the subject.
        unit(UnitType::Monster, 8, 99, 1, (0, 0), (5, 5)),
    ])
}

fn recorder() -> Recorder {
    Recorder::new(Trace::default(), 12, 3)
}

#[test]
fn nothing_is_recorded_before_the_monster_is_there() {
    let mut r = recorder();
    let s = snap(vec![unit(UnitType::Player, 1, 1, 1, (100, 100), (80, 80))]);
    r.record(3, &s, None, &[]);
    assert!(r.trace().samples.is_empty());
    assert_eq!(r.subject(3, &s), None);
}

#[test]
fn a_tick_is_one_sample_of_the_monster_and_the_character() {
    let mut r = recorder();
    r.record(4, &arena(130), Some(1), &[]);
    let t = r.trace();
    assert_eq!(t.ticks, 1);
    assert_eq!(
        t.samples,
        [Sample {
            tick: 1,
            state: Some(1),
            mode: 2,
            dist: 30,
            life: 30,
            max_life: 40,
            target: 80
        }]
    );
}

#[test]
fn the_subject_is_the_first_of_the_class_and_stays() {
    let mut r = recorder();
    let mut s = arena(130);
    assert_eq!(r.subject(4, &s), Some(7));
    // A second monster of the class that appears later is not followed.
    s.units
        .push(unit(UnitType::Monster, 6, 12, 1, (0, 0), (1, 1)));
    assert_eq!(r.subject(4, &s), Some(7));
    r.record(5, &s, None, &[]);
    assert_eq!(r.trace().samples[0].dist, 30);
}

#[test]
fn the_levels_own_monsters_of_the_class_are_not_the_subject() {
    let mut r = recorder();
    // Before the spawn: a monster of the class is already there.
    let before = snap(vec![
        unit(UnitType::Player, 1, 1, 1, (100, 100), (80, 80)),
        unit(UnitType::Monster, 6, 12, 1, (0, 0), (10, 10)),
    ]);
    r.record(3, &before, None, &[]);
    assert!(r.trace().samples.is_empty());
    let mut after = arena(130);
    after
        .units
        .push(unit(UnitType::Monster, 6, 12, 1, (0, 0), (10, 10)));
    assert_eq!(r.subject(4, &after), Some(7));
}

#[test]
fn the_spawn_is_later_after_a_warp() {
    assert!(spawn_tick(LEVEL) > WARP_TICK);
    assert_eq!(spawn_tick(0), SPAWN_TICK);
}

#[test]
fn events_become_trace_events_at_their_tick() {
    let mut r = recorder();
    let e = |frame, kind| GraphEvent {
        frame,
        unit: UnitId(0),
        ai: 200,
        kind,
    };
    r.record(
        6,
        &arena(120),
        Some(1),
        &[
            e(5, GraphEventKind::StateChanged { from: 0, to: 1 }),
            e(6, GraphEventKind::WindUp { event: 0, ticks: 8 }),
            e(6, GraphEventKind::PhaseChanged { phase: 2 }),
        ],
    );
    let kinds: Vec<_> = r.trace().events.iter().map(|e| (e.tick, e.kind)).collect();
    assert_eq!(
        kinds,
        [
            (2, EventKind::State { from: 0, to: 1 }),
            (3, EventKind::WindUp { event: 0, ticks: 8 }),
            (3, EventKind::Phase { phase: 2 }),
        ]
    );
}

#[test]
fn the_distance_is_the_larger_axis() {
    let a = unit(UnitType::Monster, 1, 1, 1, (10, 50), (1, 1));
    let b = unit(UnitType::Player, 2, 1, 1, (30, 45), (1, 1));
    assert_eq!(distance(&a, &b), 20);
    assert_eq!(distance(&b, &a), 20);
}

#[test]
fn without_a_mod_the_header_has_no_graph_names() {
    let a = parse_args(&argv("--class 12 --ai 200")).unwrap();
    let t = header(&a, 5, &AiRules::none());
    assert_eq!((t.seed, t.class, t.ai), (5, 12, 0));
    assert_eq!(t.monster, "class 12");
    assert!(t.states.is_empty() && t.phases.is_empty() && t.windups.is_empty());
}

#[test]
fn a_mods_graph_names_its_states_phases_and_wind_ups() {
    use d2_sim::rules::data_mod::{DataMod, DataModFiles};
    let toml = r#"
[[ai]]
index = 200
name = "Fallen graph"
[[ai.cooldown]]
name = "roar_cd"
seconds = 5
[[ai.state]]
name = "fight"
[[ai.state.action]]
kind = "attack"
[[ai.state]]
name = "flee"
"#;
    let files = DataModFiles {
        ai: Some(("ai.toml", toml)),
        ..DataModFiles::default()
    };
    let rules = DataMod::from_files("t", &files).unwrap();
    let rules = rules.ai_rules();
    let a = parse_args(&argv("--class 12 --ai 200")).unwrap();
    let t = header(&a, 5, rules);
    assert_eq!((t.ai, t.monster.as_str()), (200, "Fallen graph"));
    assert_eq!(t.states, ["fight", "flee"]);
    // The class's own name wins; an unknown graph is none.
    let named = parse_args(&argv("--class 12 --ai 200 --name Fallen")).unwrap();
    assert_eq!(header(&named, 5, rules).monster, "Fallen");
    let other = parse_args(&argv("--class 12 --ai 201")).unwrap();
    assert_eq!(header(&other, 5, rules).ai, 0);
}
