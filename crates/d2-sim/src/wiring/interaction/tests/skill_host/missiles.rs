//! [`crate::rules::HostMissiles`] through a mod body.

use crate::rules::MissileRequest;
use crate::skills::use_::bodies::BodyWorld;
use crate::units::{UnitId, UnitType};

use super::{fx, probe};

/// The fixture's missile row (an arrow).
const ARROW: i32 = 0;

fn missile_of(s: Option<String>) -> UnitId {
    UnitId(s.expect("ran").parse().expect("a missile"))
}

#[test]
fn a_mod_body_spawns_a_missile_and_sets_its_data() {
    let mut fx = fx();
    let p = fx.spawn(UnitType::Player, 10, 10);
    let m = missile_of(probe(&mut fx, p, 0, 4, |w, u, skill, lvl| {
        let m = w.spawn_missile(MissileRequest {
            flags: 1,
            x: 12,
            y: 10,
            target_x: 15,
            target_y: 10,
            skill,
            level: lvl,
            ..MissileRequest::new(u, ARROW)
        });
        let Some(m) = m else {
            return "none".into();
        };
        w.set_missile_data(m, 174, -3);
        w.set_path_type(m, 14);
        m.0.to_string()
    }));
    let store = fx.sim.hooks().missile_store();
    let data = store.get(m).expect("in the missile store");
    assert_eq!(data.target, (174, -3));
    let rec = fx.sim.sys.units.get(m).unwrap();
    assert_eq!((rec.ty, rec.class), (UnitType::Missile, ARROW as u32));
    fx.assert_clean();
}

#[test]
fn a_mod_body_fires_the_skill_missile_at_its_target_point() {
    let mut fx = fx();
    let p = fx.spawn(UnitType::Player, 10, 10);
    fx.sim.sys.hooks.x.aim_at = (16, 10);
    let m = missile_of(probe(&mut fx, p, 0, 2, |w, u, skill, lvl| {
        w.fire_skill_missile(u, ARROW, skill, lvl, false)
            .map_or("none".into(), |m| m.0.to_string())
    }));
    let rec = fx.sim.sys.units.get(m).unwrap();
    assert_eq!((rec.ty, rec.class), (UnitType::Missile, ARROW as u32));
    // No target point: none.
    fx.sim.sys.hooks.x.aim_at = (0, 0);
    let got = probe(&mut fx, p, 0, 2, |w, u, skill, lvl| {
        format!("{:?}", w.fire_skill_missile(u, ARROW, skill, lvl, false))
    });
    assert_eq!(got.as_deref(), Some("None"));
    fx.assert_clean();
}

#[test]
fn a_mod_body_tests_a_box_on_the_rooms() {
    let mut fx = fx();
    let p = fx.spawn(UnitType::Player, 10, 10);
    let got = probe(&mut fx, p, 0, 1, |w, u, _, _| {
        format!(
            "{:?}",
            (
                w.box_blocked(u, 11, 10, 1, 5),
                w.box_blocked(u, -500, -500, 1, 5)
            )
        )
    });
    let open = fx.sim.skill_use(&mut fx.game, |w| {
        let r = BodyWorld::unit_room(w, p).unwrap();
        let r = BodyWorld::room_at(w, r, 11, 10).unwrap();
        BodyWorld::box_collides(w, r, (11, 10), 1, 5)
    });
    assert_eq!(got, Some(format!("{:?}", (open, true))));
    fx.assert_clean();
}
