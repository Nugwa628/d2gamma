//! Name and class, status, level and experience, experience rate, stat
//! and skill points, gold.

use d2_formats::d2s::status;

use super::*;

#[test]
fn name_or_class_change_fails() {
    let before = character();
    let mut after = character();
    after.header.class = 1;
    assert!(failed(&run(&before, &after), "name-class"));
    let mut after = character();
    after.header.set_name(b"Other").expect("fits");
    assert!(failed(&run(&before, &after), "name-class"));
    assert!(!failed(&run(&before, &character()), "name-class"));
}

#[test]
fn hardcore_revival_fails() {
    let mut before = character();
    before.header.status |= status::HARDCORE | status::DEAD;
    let mut after = before.clone();
    after.header.status &= !status::DEAD;
    let v = run(&before, &after);
    assert!(!v.accepted());
    assert!(failed(&v, "status"));
    // Dead stays dead: the same dead record passes.
    assert!(!failed(&run(&before, &before), "status"));
    // A softcore death is no status change.
    let mut sc = character();
    sc.header.status |= status::DEAD;
    assert!(!failed(&run(&sc, &character()), "status"));
}

#[test]
fn hardcore_or_ladder_change_fails() {
    let before = character();
    let mut after = character();
    after.header.status |= status::HARDCORE;
    assert!(failed(&run(&before, &after), "status"));
    let mut after = character();
    after.header.status |= status::LADDER;
    assert!(failed(&run(&before, &after), "status"));
    let mut after = character();
    after.header.status &= !status::EXPANSION;
    assert!(failed(&run(&before, &after), "status"));
}

#[test]
fn level_without_experience_fails() {
    let before = character();
    let mut after = character();
    after.header.level = 10;
    set_stat(&mut after, stat::LEVEL, 10);
    let v = run(&before, &after);
    assert!(!v.accepted());
    assert!(failed(&v, "level-experience"));
    // Level 10 with level 10's experience passes.
    assert!(!failed(&run(&before, &at_level(10)), "level-experience"));
}

#[test]
fn experience_without_level_fails() {
    let mut after = character();
    set_stat(&mut after, stat::EXPERIENCE, 4000);
    assert!(failed(&run(&character(), &after), "level-experience"));
}

#[test]
fn stats_level_unlike_header_fails() {
    let mut after = at_level(3);
    set_stat(&mut after, stat::LEVEL, 4);
    assert!(failed(&run(&character(), &after), "level-experience"));
}

#[test]
fn experience_rate_over_cap_fails() {
    // The fixture's level 99 is 9,604,000 experience: a realm cap of
    // 1,000,000 a minute.
    let mut checks = list();
    checks
        .replace(
            "experience-rate",
            Box::new(crate::character::ExperienceRate {
                per_minute: 1_000_000,
            }),
        )
        .expect("a standard check");
    let before = character();
    let after = at_level(99);
    let played = |ms| SaveInfo {
        played_ms: Some(ms),
        ..SaveInfo::default()
    };
    assert!(failed(
        &checks.run(&before, &after, played(30_000)),
        "experience-rate"
    ));
    // Ten started minutes: within the cap; unknown play time: not checked.
    assert!(!failed(
        &checks.run(&before, &after, played(9 * 60_000 + 1)),
        "experience-rate"
    ));
    assert!(!failed(
        &checks.run(&before, &after, SaveInfo::default()),
        "experience-rate"
    ));
    // The default cap is far above a fixture level 99 in a minute.
    assert!(!failed(
        &list().run(&before, &after, played(30_000)),
        "experience-rate"
    ));
}

#[test]
fn stat_points_over_level_fail() {
    // Level 1, Normal open: the five Normal quest points only.
    let mut after = character();
    set_stat(&mut after, stat::STRENGTH, 25);
    assert!(!failed(&run(&character(), &after), "stat-points"));
    set_stat(&mut after, stat::STATPTS, 1);
    assert!(failed(&run(&character(), &after), "stat-points"));
    // Level 3: 2 × 5 more.
    let mut after = at_level(3);
    set_stat(&mut after, stat::VITALITY, 30);
    set_stat(&mut after, stat::STATPTS, 5);
    assert!(!failed(&run(&character(), &after), "stat-points"));
    set_stat(&mut after, stat::STATPTS, 6);
    assert!(failed(&run(&character(), &after), "stat-points"));
}

#[test]
fn attribute_below_class_start_fails() {
    let mut after = character();
    set_stat(&mut after, stat::DEXTERITY, 24);
    assert!(failed(&run(&character(), &after), "stat-points"));
}

#[test]
fn skill_points_over_level_fail() {
    // Level 1, Normal open: the four Normal quest points only.
    let mut after = character();
    set_stat(&mut after, stat::NEWSKILLS, 4);
    assert!(!failed(&run(&character(), &after), "skill-points"));
    set_stat(&mut after, stat::NEWSKILLS, 5);
    assert!(failed(&run(&character(), &after), "skill-points"));
    // Level 5: four more, spent in the skill bytes.
    let mut after = at_level(5);
    after.body.as_mut().expect("a body").skills[0] = 8;
    assert!(!failed(&run(&character(), &after), "skill-points"));
    after.body.as_mut().expect("a body").skills[1] = 1;
    assert!(failed(&run(&character(), &after), "skill-points"));
}

#[test]
fn gold_over_cap_fails() {
    let before = character();
    let mut after = character();
    set_stat(&mut after, stat::GOLD, 10_000);
    assert!(!failed(&run(&before, &after), "gold"));
    set_stat(&mut after, stat::GOLD, 10_001);
    let v = run(&before, &after);
    assert!(!v.accepted());
    assert!(failed(&v, "gold"));
    // Level 30 carries 300,000.
    let mut after = at_level(30);
    set_stat(&mut after, stat::GOLD, 300_000);
    assert!(!failed(&run(&before, &after), "gold"));
}

#[test]
fn stash_gold_over_cap_fails() {
    let mut after = character();
    set_stat(&mut after, stat::GOLDBANK, 2_500_000);
    assert!(!failed(&run(&character(), &after), "gold"));
    set_stat(&mut after, stat::GOLDBANK, 2_500_001);
    assert!(failed(&run(&character(), &after), "gold"));
}
