//! [`HostMissiles`] on the skill use view.

use crate::rules::{HostMissiles, MissileRequest};
use crate::skills::use_::bodies::{self, BodyEffect, BodyWorld, PathOp};
use crate::units::UnitId;
use crate::wiring::action::Pending;
use crate::wiring::interaction::{UseRest, UseView};

impl<X: Pending + UseRest> HostMissiles for UseView<'_, X> {
    fn missile_row_exists(&self, row: u16) -> bool {
        usize::from(row) < self.cv.v.h.tables.skills.missiles.len()
    }
    fn bow_hand(&self, u: UnitId) -> bool {
        BodyWorld::hand_class(self, u) == 1
    }
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
    fn channel_length(&mut self, u: UnitId, missile: UnitId, skill: i32, lvl: i32) {
        let t = self.cv.v.h.tables.clone();
        bodies::b4_helpers::channel_frames(self, &t.skills, u, Some(missile), skill, lvl);
    }
    fn drop_skill_missile(
        &mut self,
        u: UnitId,
        missile: i32,
        skill: i32,
        lvl: i32,
    ) -> Option<UnitId> {
        let t = self.cv.v.h.tables.clone();
        bodies::flag_40(self, u);
        if !bodies::point_free(self, &t.skills, u, missile) {
            return None;
        }
        bodies::missile_at(self, u, skill, lvl, missile, (0, 0))
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
