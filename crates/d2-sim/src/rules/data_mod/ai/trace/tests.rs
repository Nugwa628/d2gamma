//! The trace file: written, read back, summed up.

use super::*;

fn sample_at(tick: u32, state: Option<u8>, mode: u32, dist: u32, life: i32, target: i32) -> Sample {
    Sample {
        tick,
        state,
        mode,
        dist,
        life,
        max_life: 40,
        target,
    }
}

fn trace() -> Trace {
    Trace {
        seed: 7,
        ticks: 4,
        class: 12,
        ai: 200,
        monster: "Fallen".into(),
        states: vec!["fight".into(), "flee".into()],
        phases: vec!["enrage".into()],
        windups: vec!["roar".into()],
        samples: vec![
            sample_at(1, Some(0), 1, 9, 40, 80),
            sample_at(2, Some(0), 2, 3, 40, 80),
            sample_at(3, Some(1), 4, 1, 30, 70),
            sample_at(4, Some(1), 0, 1, 0, 60),
        ],
        events: vec![
            Event {
                tick: 3,
                kind: EventKind::State { from: 0, to: 1 },
            },
            Event {
                tick: 3,
                kind: EventKind::WindUp { event: 0, ticks: 8 },
            },
            Event {
                tick: 4,
                kind: EventKind::Phase { phase: 0 },
            },
        ],
    }
}

#[test]
fn a_trace_reads_back_as_it_was_written() {
    let t = trace();
    let text = t.write();
    assert_eq!(Trace::parse(&text).unwrap(), t);
    assert_eq!(Trace::parse(&text).unwrap().write(), text);
}

#[test]
fn the_text_is_the_one_documented() {
    let text = trace().write();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("ai-trace-1"));
    assert!(text.contains("\nstate 1 flee\n"));
    assert!(text.contains("\ns 1 state=0 mode=1 dist=9 life=40/40 target=80\n"));
    assert!(text.contains("\ne 3 state 0 1\n"));
    assert!(text.contains("\ne 3 windup 0 8\n"));
    assert!(text.ends_with("e 4 phase 0\n"));
}

#[test]
fn a_sample_outside_a_graph_has_no_state() {
    let mut t = trace();
    t.samples = vec![sample_at(1, None, 1, 5, 40, 80)];
    t.events.clear();
    let text = t.write();
    assert!(text.contains("s 1 state=- mode=1"));
    assert_eq!(Trace::parse(&text).unwrap(), t);
}

#[test]
fn a_name_stays_on_its_line() {
    let mut t = trace();
    t.monster = "two\nlines".into();
    let back = Trace::parse(&t.write()).unwrap();
    assert_eq!(back.monster, "two lines");
}

#[test]
fn what_is_not_a_trace_is_refused_with_its_line() {
    assert!(Trace::parse("").is_err());
    assert!(Trace::parse("ai-trace-2\n").is_err());
    for (bad, line) in [
        ("seed x", "line 2"),
        ("s 1 state=0", "line 2"),
        ("s 1 mode=0 state=0 dist=1 life=1/1 target=1", "line 2"),
        ("s 1 state=0 mode=0 dist=1 life=1 target=1", "line 2"),
        ("e 1 dance 3", "line 2"),
        ("state 2 late", "line 2"),
        ("zzz 1", "line 2"),
    ] {
        let e = Trace::parse(&format!("ai-trace-1\n{bad}\n")).unwrap_err();
        assert!(e.starts_with(line), "{bad}: {e}");
    }
}

#[test]
fn the_summary_counts_what_happened() {
    let s = trace().summary();
    assert_eq!(s.in_state, [(Some(0), 2), (Some(1), 2)]);
    assert_eq!(s.in_mode, [(0, 1), (1, 1), (2, 1), (4, 1)]);
    assert_eq!((s.transitions, s.windups, s.phases), (1, 1, 1));
    assert_eq!((s.closest, s.farthest), (1, 9));
    assert_eq!((s.damage_dealt, s.damage_taken), (20, 40));
    assert_eq!(s.died_at, Some(4));
}

#[test]
fn an_empty_run_sums_to_nothing() {
    let s = Trace::default().summary();
    assert_eq!(s, Summary::default());
}

#[test]
fn events_are_described_with_names() {
    let t = trace();
    let d: Vec<_> = t.events.iter().map(|e| t.describe(e)).collect();
    assert_eq!(
        d,
        [
            "tick 3: fight to flee",
            "tick 3: wind-up roar (8 ticks)",
            "tick 4: phase enrage"
        ]
    );
    // A number the trace has no name for stays a number.
    assert_eq!(t.state_name(9), "#9");
}

#[test]
fn the_summary_text_names_the_states() {
    let text = trace().summary_text();
    assert!(text.contains("Fallen (class 12, ai 200), seed 7, 4 ticks"));
    assert!(text.contains("in fight: 2 ticks"));
    assert!(text.contains("1 transitions, 1 wind-ups, 1 phases"));
    assert!(text.contains("the monster died at tick 4"));
}

mod closeness {
    use super::super::closeness::{compare, Report};
    use super::super::Summary;

    fn run(walking: u32, standing: u32, dealt: i32, died: bool) -> Summary {
        Summary {
            in_mode: vec![(1, standing), (2, walking)],
            closest: 3,
            farthest: 24,
            damage_dealt: dealt,
            damage_taken: 0,
            died_at: died.then_some(40),
            ..Summary::default()
        }
    }

    fn row<'a>(r: &'a Report, name: &str) -> &'a super::super::closeness::Row {
        r.rows.iter().find(|x| x.name == name).expect(name)
    }

    #[test]
    fn the_same_runs_do_not_differ() {
        let a = [run(100, 400, 20, false), run(120, 380, 30, false)];
        let r = compare("x", 500, &a, &a);
        assert!(r.big().is_empty(), "{}", r.text());
        assert_eq!(row(&r, "ticks walking").rust, 110.0);
        assert_eq!(row(&r, "ticks standing").data, 390.0);
    }

    #[test]
    fn a_big_difference_is_marked_and_a_small_one_is_not() {
        let rust = [run(100, 400, 20, false), run(100, 400, 20, false)];
        let data = [run(110, 390, 20, true), run(300, 200, 20, true)];
        let r = compare("x", 500, &rust, &data);
        // 100 vs 205 walking: far more than a quarter and than 25 ticks.
        assert!(row(&r, "ticks walking").big());
        // 400 vs 295 standing is more than 25 ticks and over a quarter of 400.
        assert!(row(&r, "ticks standing").big());
        // The character lost the same.
        assert!(!row(&r, "hit points the character lost").big());
        // It never died against always.
        assert!(row(&r, "runs in which it died (of 1)").big());
        let t = r.text();
        assert!(
            t.contains("x: Rust body vs data graph, 2 seeds x 500 ticks"),
            "{t}"
        );
        assert!(t.contains("ticks walking") && t.contains("<- BIG"), "{t}");
    }

    #[test]
    fn a_few_ticks_is_within_the_slack_even_when_it_is_most_of_a_small_number() {
        let r = compare("x", 500, &[run(2, 498, 0, false)], &[run(8, 492, 0, false)]);
        assert!(r.big().is_empty(), "{}", r.text());
    }

    #[test]
    fn a_mode_only_one_side_reached_is_a_row() {
        let rust = Summary {
            in_mode: vec![(1, 500)],
            ..Summary::default()
        };
        let data = Summary {
            in_mode: vec![(1, 300), (4, 200)],
            ..Summary::default()
        };
        let r = compare("x", 500, &[rust], &[data]);
        assert_eq!(row(&r, "ticks attack 1").rust, 0.0);
        assert_eq!(row(&r, "ticks attack 1").data, 200.0);
        assert!(row(&r, "ticks attack 1").big());
    }

    #[test]
    fn no_runs_is_an_empty_comparison_not_a_crash() {
        let r = compare("x", 500, &[], &[]);
        assert!(r.big().is_empty());
        assert_eq!(r.seeds, 0);
    }
}
