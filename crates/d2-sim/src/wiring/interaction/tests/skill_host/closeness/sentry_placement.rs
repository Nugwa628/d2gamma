//! A sentry's data placement permission is checked against real DRLG towns.
use super::*;

#[test]
fn sentry_placement_obeys_towns_and_the_requested_point() {
    use crate::rules::{SentrySetup, SkillPets};
    let case = CASES.iter().find(|c| c.name == "sentry").unwrap();
    let mut t = tables(case, None);
    t.skills.push(t.skills[0].clone());
    let mut fx = fx_with(t);
    use d2_formats::animdata::{self, AnimData, AnimRecord};
    let mut anim = AnimData {
        buckets: vec![Vec::new(); animdata::BUCKETS],
    };
    anim.buckets[animdata::hash(b"GENANIM")].push(AnimRecord {
        name: *b"GENANIM\0",
        frames: 4,
        speed: 256,
        events: [0; animdata::EVENTS],
    });
    fx.sim.sys.hooks.anim_data = Some(Arc::new(anim));
    fx.sim.sys.hooks.x.generated_anim = true;
    fx.sim.sys.hooks.bodies = Some(Arc::new(BodyTables {
        stats: vec![BodyStat::default(); 359],
        state_group: vec![0; super::super::super::N_STATES],
        state_aura: vec![false; super::super::super::N_STATES],
        pettype_count: 3,
        pettype_group: vec![0; 3],
        ..BodyTables::default()
    }));
    let p = fx.spawn(UnitType::Player, 10, 10);
    fx.set(
        p,
        &[
            (LEVEL_STAT, 10),
            (crate::stats::stat::MAXHP, 100 << 8),
            (crate::stats::stat::HITPOINTS, 100 << 8),
        ],
    );
    fx.sim
        .sys
        .hooks
        .drlg
        .dungeon
        .set_towns(crate::drlg::TownLevels::new([2]));
    let mut setup = SentrySetup {
        class: 0,
        pet_type: 2,
        mode: 1,
        cap: 1,
        skill: 1,
        level: 5,
        in_town: false,
        at: (12, 12),
    };
    assert!(fx
        .sim
        .skill_use(&mut fx.game, |w| w.place_sentry(p, &setup))
        .is_none());
    assert_eq!(fx.sim.skill_use(&mut fx.game, |w| w.pet_count(p, 2)), 0);
    setup.in_town = true;
    let pet = fx
        .sim
        .skill_use(&mut fx.game, |w| w.place_sentry(p, &setup))
        .expect("explicit town permission");
    let at = fx.sim.skill_use(&mut fx.game, |w| {
        crate::skills::use_::UseWorld::position(w, pet)
    });
    assert_eq!(at, (12, 12));
    setup.at = (1000, 1000);
    assert!(fx
        .sim
        .skill_use(&mut fx.game, |w| w.place_sentry(p, &setup))
        .is_none());
    fx.assert_clean();
}

#[test]
fn sentry_edited_data_changes_the_owned_pet_cap() {
    let case = CASES.iter().find(|c| c.name == "sentry").unwrap();
    let text="[[skill]]\nid=0\n[[skill.part]]\nkind='sentry'\nclass=0\npet_type=2\nmode=1\npet_max='1'\n";
    let cells = [("srvdofunc", crate::rules::data_mod::BLOCK_SLOT.to_string())];
    let out = run(case, Some(text), tables(case, Some(&cells)));
    assert_eq!(out.results, [1, 1, 1]);
    assert_eq!(out.pets, 1);
    assert_eq!(out.pet_stats, 5);
}
