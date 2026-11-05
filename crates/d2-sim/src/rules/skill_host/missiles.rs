//! Missiles for a mod game's skill bodies.

pub use crate::skills::use_::bodies::init_cb as missile_init;
pub use crate::skills::use_::bodies::MissileRequest;
use crate::units::UnitId;

/// What a skill body does with missiles: create them as D2RUST's own
/// bodies do, and test the spot one would land on.
pub trait HostMissiles {
    /// The explicit missile row is loaded.
    fn missile_row_exists(&self, row: u16) -> bool {
        let _ = row;
        false
    }
    /// The active hand is a bow hand (the alternative fan row is for other hands).
    fn bow_hand(&self, u: UnitId) -> bool {
        let _ = u;
        false
    }
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
    /// Sets the length of a channel's missile `missile` (its steps, animation
    /// speed and life) as the engine's channels do: from the skills row's
    /// `calc1` at the level, or the missile row's second parameter plus the
    /// level less one when that is not above 0.
    fn channel_length(&mut self, u: UnitId, missile: UnitId, skill: i32, lvl: i32) {
        let _ = (u, missile, skill, lvl);
    }
    /// Drops missile row `missile` on the unit's skill target point as
    /// Meteor does: the point must be within 100 of the unit and free of
    /// what the missile row's size collides with (walls and missile
    /// blockers); `None` when it is not, or without a point or a missile.
    fn drop_skill_missile(
        &mut self,
        u: UnitId,
        missile: i32,
        skill: i32,
        lvl: i32,
    ) -> Option<UnitId> {
        let _ = (u, missile, skill, lvl);
        None
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
