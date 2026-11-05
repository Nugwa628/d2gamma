use super::*;
use d2_sim::monsters::ai::graph::WindUp;

const GRAPH: &str = r#"
[[ai]]
index = 6
name = "fallen"
[[ai.state]]
name = "wait"
[[ai.state.transition]]
to = "hunt"
when = [{ kind = "has_target" }]
[[ai.state]]
name = "hunt"
[[ai.state.action]]
kind = "wind_up"
seconds = 0.5
event = "swing"
[[ai.phase]]
name = "enrage"
state = "hunt"
when = [{ kind = "life_below", percent = 50 }]
"#;

fn graphs() -> AiRules {
    AiRules::from_toml(GRAPH).expect("the graph reads")
}

fn event(unit: u32, frame: i32, kind: GraphEventKind) -> GraphEvent {
    GraphEvent {
        frame,
        unit: UnitId(unit),
        ai: 6,
        kind,
    }
}

#[test]
fn a_monster_is_described_by_its_state_transition_and_phase() {
    let g = graphs();
    let def = g.defs().first();
    let mut w = Watcher::default();
    let mut run = GraphRun::new(6, 0, 100);
    let at = |w: &Watcher, run: &GraphRun, frame| w.describe(UnitId(1), run, def, frame);
    assert_eq!(
        at(&w, &run, 110),
        "fallen: wait for 10 ticks; no transition yet"
    );

    w.note(&[event(
        1,
        120,
        GraphEventKind::StateChanged { from: 0, to: 1 },
    )]);
    run.state = 1;
    run.entered = 120;
    assert_eq!(
        at(&w, &run, 130),
        "fallen: hunt for 10 ticks; last wait > hunt 10 ticks ago"
    );

    run.phase = 1;
    run.windup = Some(WindUp {
        action: 0,
        until: 140,
    });
    w.note(&[event(
        1,
        128,
        GraphEventKind::WindUp {
            event: 0,
            ticks: 12,
        },
    )]);
    assert_eq!(
        at(&w, &run, 130),
        "fallen: hunt for 10 ticks; last wait > hunt 10 ticks ago; phase enrage; \
         winding up swing, 10 ticks left"
    );
}

#[test]
fn another_monsters_events_are_not_this_ones() {
    let g = graphs();
    let mut w = Watcher::default();
    w.note(&[event(
        2,
        120,
        GraphEventKind::StateChanged { from: 0, to: 1 },
    )]);
    let run = GraphRun::new(6, 0, 100);
    assert!(w
        .describe(UnitId(1), &run, g.defs().first(), 130)
        .ends_with("no transition yet"));
}

#[test]
fn a_graph_the_mod_no_longer_has_is_still_shown() {
    let w = Watcher::default();
    let run = GraphRun::new(9, 3, 0);
    assert_eq!(
        w.describe(UnitId(1), &run, None, 5),
        "graph 9: state 3 for 5 ticks; no transition yet"
    );
}

#[test]
fn the_text_lists_the_monsters_or_says_there_are_none() {
    assert_eq!(text_of(&[]), "no monster nearby runs a behaviour graph");
    let seen = [AiSeen {
        guid: 12,
        class: 19,
        dist: 30,
        line: "fallen: wait for 3 ticks".into(),
    }];
    assert_eq!(
        text_of(&seen),
        "#12 class 19 (30 away) fallen: wait for 3 ticks"
    );
}
