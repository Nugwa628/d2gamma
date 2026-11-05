//! Refresh the real learned entry, including level changes and removal.
use super::*;
use crate::skills::use_::bodies::BodyWorld;

fn refresh(fx: &mut Fx, u: UnitId, level: i32) -> (i32, Option<(i32, i32)>) {
    let entry = SkillEntry {
        skill: 1,
        base: level,
        owner_guid: -1,
        ..SkillEntry::default()
    };
    fx.sim.hooks().x.skills.insert(u, vec![entry]);
    fx.sim.skill_use(&mut fx.game, |w| {
        BodyWorld::passive_state_apply(w, u, &entry);
        let value = crate::skills::SkillUnits::stat(w, u, 31, 4);
        let marks = w
            .state_list(u, i32::from(STATE))
            .map(|l| (w.list_get(l, 350), w.list_get(l, 351)));
        (value, marks)
    })
}

#[test]
fn passive_refresh_matches_layered_stats_and_has_no_cast_results() {
    let case = CASES.iter().find(|c| c.name == "passive").unwrap();
    let src = Cells(case.cells.iter().copied().collect());
    let lifted = lift(0, &src).unwrap();
    assert!(lifted.cells.is_empty());
    let rust = run(case, None, tables(case, None));
    let data = run(case, Some(&lifted.toml), tables(case, Some(&lifted.cells)));
    assert_eq!(rust.passive_values, [12, 0, 0, 0, 0]);
    assert_eq!(rust.passive_skill, 1);
    assert_eq!(rust.passive_level, 5);
    assert!(rust.results.is_empty());
    assert_eq!(rust, data);
}

#[test]
fn passive_level_changes_remove_and_restore_the_real_state_list() {
    let case = CASES.iter().find(|c| c.name == "passive").unwrap();
    for data in [false, true] {
        let mut t = tables(case, None);
        t.skills.push(t.skills[0].clone());
        // Native formula is the skill level, with the same layer as data.
        t.skills_code = vec![4, 16, 0];
        let mut fx = fx_with(t);
        let p = fx.spawn(UnitType::Player, 10, 10);
        fx.sim
            .with(&mut fx.game, |_, v| v.set_state(p, STATE, true));
        if data {
            install(&mut fx,"[[skill]]\nid=1\n[[skill.part]]\nkind='passive'\nstate=30\nlayer=4\nblocked_by=31\n[[skill.part.stat]]\nstat=31\nvalue='lvl'\n","");
        }
        // The native row suppresses the passive while its aura is on.
        Arc::make_mut(&mut fx.sim.sys.hooks.tables).skills.skills[1].aurastate = OTHER;
        assert_eq!(refresh(&mut fx, p, 5), (5, Some((1, 5))));
        assert_eq!(
            refresh(&mut fx, p, 5),
            (5, Some((1, 5))),
            "repeat is idempotent"
        );
        assert_eq!(refresh(&mut fx, p, 7), (7, Some((1, 7))));
        assert_eq!(refresh(&mut fx, p, 0), (0, None));
        assert_eq!(refresh(&mut fx, p, 3), (3, Some((1, 3))));
        fx.sim
            .with(&mut fx.game, |_, v| v.set_state(p, OTHER, true));
        assert_eq!(refresh(&mut fx, p, 3), (0, None));
        fx.assert_clean();
    }
}
