//! Generated companion equipment and actual host observations, without assets.
use super::*;
use crate::items::tables::ItemRec;
use crate::items::ty;
use crate::rules::{SkillPets, StateStat};
use crate::skills::fake::blank;
use crate::skills::use_::bodies::{helpers2, BodyWorld};
use crate::wiring::economy::{DeathDrops, GameFields};

pub(super) fn equipment(fx: &mut Fx) {
    let mut tables = super::corpse_drops::drop_tables();
    tables.items.itemtypes[usize::from(ty::MISC)].normal = 1;
    tables.items.items.push(ItemRec {
        code: *b"hatx",
        type_: ty::MISC as i16,
        level: 1,
        ..ItemRec::default()
    });
    fx.sim.sys.hooks.object_drops = Some(Box::new(DeathDrops::new(
        Arc::new(tables),
        GameFields::new(crate::rng::Seed::init(), false),
    )));
    let mut row: d2_data::tables::Monequip = blank();
    row.item1 = *b"hatx";
    row.loc1 = 1;
    row.mod1 = 2;
    Arc::make_mut(&mut fx.sim.sys.hooks.tables).monequip = vec![row];
}

pub(super) fn observe(fx: &mut Fx, out: &mut Outcome, owner: UnitId, pets: &[UnitId]) {
    let mut count = 0;
    let mut levels = 0;
    let mut timers = 0;
    let mut links = 0;
    for &pet in pets {
        let h = &fx.sim.sys.hooks;
        let gear = h
            .monster_equip
            .get(&pet)
            .expect("generated equipment on each shadow");
        for &item in gear.values() {
            count += 1;
            levels += i64::from(h.items.get(item).unwrap().ilvl);
            assert_eq!(fx.sim.sys.units.get(item).unwrap().mode, 1);
        }
        for id in fx.game.timers.unit_timers(pet) {
            if fx
                .game
                .timers
                .event(id)
                .is_some_and(|(e, _, _)| e == 2 || e == 7)
            {
                timers += 1;
            }
        }
        fx.sim.skill_use(&mut fx.game, |w| {
            let guid = helpers2::guid(w, owner);
            if crate::skills::SkillUnits::stat(w, pet, crate::stats::ids::SOURCE_UNIT_ID, 0)
                == guid as i32
                && BodyWorld::unit_c8(w, pet) & 0x400 != 0
            {
                links += 1;
            }
        });
    }
    let stats = fx.sim.with(&mut fx.game, |_, v| {
        pets.iter().map(|&pet| i64::from(v.stat(pet, 32))).sum()
    });
    out.shadow_equipment = out.shadow_equipment.max(count);
    out.shadow_item_levels = out.shadow_item_levels.max(levels);
    out.shadow_timers = out.shadow_timers.max(timers);
    out.shadow_links = out.shadow_links.max(links);
    out.shadow_stats = out.shadow_stats.max(stats);
    let drops = fx.sim.sys.hooks.object_drops.as_ref().unwrap();
    assert!(drops.failures.is_empty(), "{:?}", drops.failures);
}

#[test]
fn shadow_report_observes_equipment_levels_source_links_stats_and_timers() {
    let (_, all) = report();
    let (_, native, data) = all.iter().find(|(name, _, _)| *name == "shadow").unwrap();
    assert_eq!(native, data);
    assert_eq!(native.results, [1, 1, 1]);
    assert_eq!(native.pets, 2);
    assert_eq!(native.pet_levels, 60);
    assert_eq!(native.pet_stats, 10);
    assert_eq!(native.shadow_stats, 4);
    assert_eq!(native.shadow_equipment, 2);
    assert_eq!(native.shadow_item_levels, 22);
    assert_eq!(native.shadow_timers, 4);
    assert_eq!(native.shadow_links, 2);
}

#[test]
fn shadow_data_edits_control_cap_equipment_level_and_lifetime() {
    let case = CASES.iter().find(|c| c.name == "shadow").unwrap();
    let mut t = tables(case, None);
    t.skills[0].srvdofunc = BLOCK_SLOT;
    let text="[[skill]]\nid=0\n[[skill.part]]\nkind='shadow'\nclass=0\npet_type=2\nmode=1\npet_max='1'\nlife_percent='20'\nitem_level='7'\nframes='0'\n";
    let out = run(case, Some(text), t);
    assert_eq!(out.pets, 1);
    assert_eq!(out.shadow_equipment, 1);
    assert_eq!(out.shadow_item_levels, 7);
    assert_eq!(out.shadow_timers, 1);
    assert_eq!(out.shadow_links, 1);
}

#[test]
fn shadow_stat_adapter_matches_nonzero_health_and_level_one_skip() {
    for level in [1, 5] {
        let mut values = Vec::new();
        for data in [false, true] {
            let case = CASES.iter().find(|c| c.name == "shadow").unwrap();
            let t = tables(case, None);
            let mut fx = fx_with(t.clone());
            let pet = fx.spawn(UnitType::Monster, 10, 10);
            fx.set(pet, &[(sst::MAXHP, 100 << 8), (sst::HITPOINTS, 100 << 8)]);
            fx.sim.skill_use(&mut fx.game, |w| {
                if data {
                    SkillPets::shadow_stats(
                        w,
                        pet,
                        level,
                        (level - 1) * 5,
                        &[
                            StateStat {
                                stat: 32,
                                layer: 0,
                                value: 2,
                            },
                            StateStat {
                                stat: 31,
                                layer: 0,
                                value: 5,
                            },
                        ],
                        None,
                    );
                } else {
                    helpers2::shadow_stats(w, &t, pet, 0, level);
                }
            });
            let value = fx.sim.with(&mut fx.game, |_, v| {
                (
                    v.stat(pet, sst::HITPOINTS),
                    v.stat(pet, sst::MAXHP),
                    v.stat(pet, 31),
                    v.stat(pet, 32),
                )
            });
            assert_eq!(
                value,
                if level == 1 {
                    (100 << 8, 100 << 8, 0, 0)
                } else {
                    (120 << 8, 120 << 8, 5, 2)
                }
            );
            values.push(value);
            fx.assert_clean();
        }
        assert_eq!(values[0], values[1]);
    }
}

#[test]
fn shadow_pet_timer_scales_base_ticks_at_sixty_hz() {
    let case = CASES.iter().find(|c| c.name == "shadow").unwrap();
    let mut fx = fx_with(tables(case, None));
    fx.game.tick_rate = crate::tick::TickRate::new(60).unwrap();
    let pet = fx.spawn(UnitType::Monster, 10, 10);
    let frame = fx.game.frame;
    fx.sim
        .skill_use(&mut fx.game, |w| SkillPets::pet_timer(w, pet, 7, 25));
    let timers = fx.game.timers.unit_timers(pet);
    assert_eq!(timers.len(), 1);
    assert_eq!(fx.game.timers.event(timers[0]).unwrap().0, 7);
    assert_eq!(fx.game.timers.expire(timers[0]), Some(frame + 60));
    fx.assert_clean();
}
