//! [`crate::rules::HostUnits`] through a mod body.

use std::sync::Arc;

use crate::combat::RoomKind;
use crate::rules::HostUnits;
use crate::units::{UnitId, UnitType};

use super::{fx, probe};

/// Every unit query of `u`, as text.
fn read<W: HostUnits + ?Sized>(w: &W, u: UnitId) -> String {
    format!(
        "{:?}",
        (
            (w.unit_type(u), w.is_hireling(u), w.has_path(u)),
            (w.position(u), w.target(u), w.target_position(u)),
            (w.room_kind(u), w.level_id(u), w.area_level(u)),
            (w.level_teleport(u), w.is_dead(u), w.owner(u)),
            (w.unit_flags(u), w.difficulty(), w.expansion()),
        )
    )
}

#[test]
fn a_mod_body_reads_the_unit_queries() {
    let mut fx = fx();
    let p = fx.spawn(UnitType::Player, 3, 4);
    let m = fx.spawn(UnitType::Monster, 6, 4);
    fx.sim.sys.hooks.x.targets.insert(p, m);
    let id = fx
        .sim
        .skill_use(&mut fx.game, |w| HostUnits::level_id(w, p))
        .expect("the player stands in a level");
    let row = &mut Arc::make_mut(&mut fx.sim.sys.hooks.tables).levels[id as usize];
    row.teleport = 1;
    for l in [
        &mut row.monlvl1,
        &mut row.monlvl2,
        &mut row.monlvl3,
        &mut row.monlvl1ex,
        &mut row.monlvl2ex,
        &mut row.monlvl3ex,
    ] {
        *l = 7;
    }
    for u in [p, m] {
        let got = probe(&mut fx, u, 0, 1, |w, u, _, _| read(w, u));
        let want = fx.sim.skill_use(&mut fx.game, |w| read(w, u));
        assert_eq!(got, Some(want));
    }
    // The view's answers are the game's.
    fx.sim.skill_use(&mut fx.game, |w| {
        assert_eq!(HostUnits::unit_type(w, p), Some(UnitType::Player));
        assert_eq!(HostUnits::unit_type(w, m), Some(UnitType::Monster));
        assert_eq!(HostUnits::position(w, p), (3, 4));
        assert_eq!(HostUnits::position(w, m), (6, 4));
        assert_eq!(HostUnits::target(w, p), Some(m));
        assert_eq!(HostUnits::room_kind(w, p), RoomKind::Field);
        assert_eq!(HostUnits::area_level(w, p), Some(7));
        assert_eq!(HostUnits::level_teleport(w, p), Some(1));
        assert!(!HostUnits::is_dead(w, m));
        assert!(!HostUnits::is_hireling(w, m));
    });
    fx.assert_clean();
}

#[test]
fn a_mod_body_sets_the_unit_flags() {
    let mut fx = fx();
    let p = fx.spawn(UnitType::Player, 3, 4);
    let before = fx.sim.sys.units.get(p).unwrap().flags;
    let got = probe(&mut fx, p, 0, 1, |w, u, _, _| {
        let f = w.unit_flags(u);
        w.set_unit_flags(u, f | 0x40);
        format!("{f:#x}")
    });
    assert_eq!(got, Some(format!("{before:#x}")));
    assert_eq!(fx.sim.sys.units.get(p).unwrap().flags, before | 0x40);
    fx.assert_clean();
}
