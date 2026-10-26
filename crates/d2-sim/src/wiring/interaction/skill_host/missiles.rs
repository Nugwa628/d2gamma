//! [`HostMissiles`] on the skill use view.

use crate::rules::{HostMissiles, MissileRequest};
use crate::skills::use_::bodies::{self, BodyEffect, BodyWorld, PathOp};
use crate::units::UnitId;
use crate::wiring::action::Pending;
use crate::wiring::interaction::{UseRest, UseView};

impl<X: Pending + UseRest> HostMissiles for UseView<'_, X> {
    fn spawn_missile(&mut self, req: MissileRequest<UnitId>) -> Option<UnitId> {
        BodyWorld::spawn_missile(self, req)
    }
    fn set_missile_data(&mut self, m: UnitId, first: i32, second: i32) {
        BodyWorld::effect(
            self,
            BodyEffect::MissileData28 {
                missile: m,
                v: first,
            },
        );
        BodyWorld::effect(
            self,
            BodyEffect::MissileData2C {
                missile: m,
                v: second,
            },
        );
    }
    fn set_path_type(&mut self, u: UnitId, path_type: u8) {
        BodyWorld::path_op(self, u, PathOp::Type(path_type));
        BodyWorld::path_op(self, u, PathOp::Reset);
    }
    /// The room containing the point, searched from the unit's room.
    fn box_blocked(&self, u: UnitId, x: i32, y: i32, size: i32, mask: u32) -> bool {
        let room = BodyWorld::unit_room(self, u).and_then(|r| BodyWorld::room_at(self, r, x, y));
        room.is_none_or(|r| BodyWorld::box_collides(self, r, (x, y), size, mask))
    }
    fn fire_skill_missile(
        &mut self,
        u: UnitId,
        missile: i32,
        skill: i32,
        lvl: i32,
        lob: bool,
    ) -> Option<UnitId> {
        bodies::skill_missile_unit(self, missile, u, skill, lvl, (0, 0), (0, 0), false, lob)
    }
}
