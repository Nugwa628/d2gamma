//! Actual target list values, removal and no-aura eligibility.
use super::*;
#[test]
fn sight_report_observes_actual_timed_stat_and_expiry() {
    let (_, all) = report();
    let (_, native, data) = all.iter().find(|(n, _, _)| *n == "sight").unwrap();
    assert_eq!(native, data);
    assert_eq!(native.results, [1]);
    assert_eq!(native.sight_values, 12);
    assert_eq!(native.affected, 1);
    assert_eq!(native.sight_remaining, 0);
}
#[test]
fn sight_respects_no_aura_monsters_in_both_paths() {
    let original = CASES.iter().find(|c| c.name == "sight").unwrap();
    let case = Case {
        name: "sight_immune",
        ..*original
    };
    let l = lift(0, &Cells(case.cells.iter().copied().collect())).unwrap();
    let n = run(&case, None, tables(&case, None));
    let d = run(&case, Some(&l.toml), tables(&case, Some(&l.cells)));
    assert_eq!(n, d);
    assert_eq!(n.affected, 0);
    assert_eq!(n.sight_values, 0);
}
