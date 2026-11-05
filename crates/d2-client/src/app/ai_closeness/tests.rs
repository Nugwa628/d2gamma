use super::*;

fn args(list: &[&str]) -> Result<ClosenessArgs> {
    parse_args(&list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
}

#[test]
fn the_classes_the_seeds_and_the_arena_options_are_read() {
    let a = args(&[
        "--classes",
        "19,20,21",
        "--ai",
        "6",
        "--seeds",
        "3",
        "--seed",
        "10",
        "--ticks",
        "200",
    ])
    .unwrap();
    assert_eq!(a.classes, [19, 20, 21]);
    assert_eq!((a.seeds, a.arena.seed, a.arena.ticks), (3, 10, 200));
    assert_eq!(a.arena.ai, Some(6));
}

#[test]
fn a_single_class_is_a_list_of_one_and_seeds_default() {
    let a = args(&["--class", "19", "--ai", "6"]).unwrap();
    assert_eq!((a.classes.clone(), a.seeds), (vec![19], SEEDS));
}

#[test]
fn what_is_missing_is_named() {
    let e = |l: &[&str]| args(l).unwrap_err().to_string();
    assert!(e(&["--ai", "6"]).contains("--classes"));
    assert!(e(&["--classes", "19"]).contains("--ai N"));
    assert!(e(&["--classes", "19", "--ai", "6", "--seeds", "0"]).contains("at least 1"));
    assert!(e(&["--classes", "x", "--ai", "6"]).contains("--classes"));
    assert!(e(&["--classes", "19", "--ai", "6", "--nope"]).contains("--nope"));
}

#[test]
fn a_report_sums_each_side_from_its_traces() {
    use d2_sim::rules::data_mod::ai::trace::Sample;
    let trace = |mode: u32| Trace {
        seed: 1,
        ticks: 10,
        class: 19,
        ai: 6,
        monster: "x".into(),
        states: Vec::new(),
        phases: Vec::new(),
        windups: Vec::new(),
        samples: (1..=10)
            .map(|tick| Sample {
                tick,
                state: None,
                mode,
                dist: 5,
                life: 10,
                max_life: 10,
                target: 50,
            })
            .collect(),
        events: Vec::new(),
    };
    let r = report("c", 10, &[trace(1)], &[trace(2)]);
    let row = |n: &str| r.rows.iter().find(|x| x.name == n).unwrap();
    assert_eq!(
        (row("ticks standing").rust, row("ticks standing").data),
        (10.0, 0.0)
    );
    assert_eq!(row("ticks walking").data, 10.0);
}
