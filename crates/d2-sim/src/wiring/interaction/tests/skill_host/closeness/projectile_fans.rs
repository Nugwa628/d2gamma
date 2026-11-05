//! Real emitted missile counts, bounded edits and deterministic trajectories.
use super::*;
#[test]
fn projectile_fan_report_observes_all_shots_and_deterministic_missiles() {
    let (_, all) = report();
    for name in ["projectile_fan", "jitter_fan", "bolt_fan"] {
        let (_, native, data) = all.iter().find(|(n, _, _)| *n == name).unwrap();
        assert_eq!(native, data);
        assert_eq!(native.results, [1, 1, 1]);
        assert_eq!(native.fired, 15);
        assert!(native.missiles > 0);
    }
}
#[test]
fn projectile_fan_edits_change_counts_and_bound_extreme_values() {
    for (count, want) in [("2", 6), ("1000000", 192)] {
        let case = CASES.iter().find(|c| c.name == "projectile_fan").unwrap();
        let mut t = tables(case, None);
        t.skills[0].srvdofunc = BLOCK_SLOT;
        let text = format!(
            "[[skill]]\nid=0\n[[skill.part]]\nkind='projectile_fan'\nmissile=0\ncount='{count}'\n"
        );
        let out = run(case, Some(&text), t);
        assert_eq!(out.fired, want);
    }
}

#[test]
fn projectile_fan_empty_count_keeps_bolt_and_tornado_return_rules() {
    for name in ["bolt_fan", "jitter_fan"] {
        let c = CASES.iter().find(|c| c.name == name).unwrap();
        let mut n = tables(c, None);
        n.skills_code[1] = 0;
        let native = run(c, None, n.clone());
        n.skills[0].srvdofunc = BLOCK_SLOT;
        let text=format!("[[skill]]\nid=0\n[[skill.part]]\nkind='projectile_fan'\nmissile=0\ncount='0'\njitter=true\nempty_ok={}\n",name=="bolt_fan");
        let data = run(c, Some(&text), n);
        assert_eq!(native, data);
        assert_eq!(native.results, vec![i32::from(name == "bolt_fan"); 3]);
        assert_eq!(native.fired, 0);
    }
}
