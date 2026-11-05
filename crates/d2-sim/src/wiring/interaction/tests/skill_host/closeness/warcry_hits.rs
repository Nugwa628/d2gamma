//! Data warcry missiles apply their editable effect through real hit routing.
use super::*;

#[test]
fn warcry_missile_hits_use_data_for_party_and_enemy_targets() {
    for target_type in [UnitType::Player, UnitType::Monster] {
        let case = CASES.iter().find(|case| case.name == "warcry").unwrap();
        let mut fx = fx_with(tables(case, None));
        let owner = fx.spawn(UnitType::Player, 10, 10);
        let target = fx.spawn(target_type, 12, 10);
        fx.sim.sys.hooks.bodies = Some(Arc::new(BodyTables {
            stats: vec![BodyStat::default(); 359],
            state_group: vec![0; super::super::super::N_STATES],
            state_aura: vec![false; super::super::super::N_STATES],
            ..BodyTables::default()
        }));
        let hit = |fx: &mut Fx| {
            fx.sim.with(&mut fx.game, |game, v| {
                let mut sim = crate::units::hooks::Sim {
                    game,
                    units: v.units,
                    stats: v.stats,
                    data: v.data,
                };
                crate::wiring::interaction::skill_events::missile_shout_state(
                    v.h, &mut sim, target, owner, 0, 5,
                );
            })
        };
        // With no data provider, the missile still uses the table's 12.
        hit(&mut fx);
        assert_eq!(fx.sim.sys.stats.unit_total(target, 31, 0), 12);
        install(&mut fx,"[[skill]]\nid=0\n[[skill.part]]\nkind='warcry'\nmissile=0\nstate=30\nframes='13'\n[[skill.part.stat]]\nstat=31\nvalue='29'\n","");
        hit(&mut fx);
        assert_eq!(fx.sim.sys.stats.unit_total(target, 31, 0), 29);
        let expire = fx.sim.skill_use(&mut fx.game, |w| {
            use crate::skills::use_::bodies::BodyWorld;
            w.list_expire(w.state_list(target, 30).unwrap())
        });
        for _ in 0..=expire {
            fx.frame();
        }
        assert!(!fx.sim.sys.stats.has_state(target, 30));
        assert_eq!(fx.sim.sys.stats.unit_total(target, 31, 0), 0);
    }
}
