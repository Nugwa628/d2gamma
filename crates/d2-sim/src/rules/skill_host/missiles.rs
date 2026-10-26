//! Missiles for a mod game's skill bodies.

pub use crate::skills::use_::bodies::init_cb as missile_init;
pub use crate::skills::use_::bodies::MissileRequest;
use crate::units::UnitId;

/// What a skill body does with missiles: create them as D2RUST's own
/// bodies do, and test the spot one would land on.
pub trait HostMissiles {
    /// Creates the missile `req` describes (its `missiles` row, owner,
    /// start and target, skill and level, velocity, range, init callback
    /// from [`missile_init`]); `None` when none was made.
    fn spawn_missile(&mut self, req: MissileRequest<UnitId>) -> Option<UnitId> {
        let _ = req;
        None
    }
    /// Sets the two auxiliary data words of missile `m` (the words its
    /// server-do functions read back, such as a re-seed value).
    fn set_missile_data(&mut self, m: UnitId, first: i32, second: i32) {
        let _ = (m, first, second);
    }
    /// Sets the path type of the unit's path (missile path types: 14
    /// follows its target point) and restarts the path.
    fn set_path_type(&mut self, u: UnitId, path_type: u8) {
        let _ = (u, path_type);
    }
    /// A `size` × `size` box at (x, y) collides with `mask` (5: walls and
    /// missile blockers), searched from the unit's room; a point in no
    /// room is blocked.
    fn box_blocked(&self, u: UnitId, x: i32, y: i32, size: i32, mask: u32) -> bool {
        let _ = (u, x, y, size, mask);
        true
    }
    /// Fires missile row `missile` from the unit at its skill target
    /// point the way a skill's `srvmissile` does (lobbed with `lob`; a
    /// monster's carries its attack rating), with `skill` at `lvl`;
    /// `None` without a target point or a missile.
    fn fire_skill_missile(
        &mut self,
        u: UnitId,
        missile: i32,
        skill: i32,
        lvl: i32,
        lob: bool,
    ) -> Option<UnitId> {
        let _ = (u, missile, skill, lvl, lob);
        None
    }
}
