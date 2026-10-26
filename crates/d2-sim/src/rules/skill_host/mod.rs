//! The parts of [`super::SkillHost`] past its base reads: what D2RUST's own
//! skill bodies use, exposed to a mod game's skill bodies (MS1).
//!
//! Each part is its own trait, a supertrait of [`super::SkillHost`], so a
//! mod body calls every method on its `&mut dyn SkillHost`. Every method
//! has a default that reads as absent and changes nothing; the skill use
//! view (`crate::wiring::interaction::UseView`), the world a provider is
//! handed, answers them all on the game's real units, stat lists, rooms,
//! tables and combat code.

mod damage;
mod missiles;
mod skills;
mod states;
mod timers;
mod units;

pub use damage::{ElementAdded, HostDamage};
pub use missiles::{missile_init, HostMissiles, MissileRequest};
pub use skills::HostSkills;
pub use states::{HostStates, StateRemove, StateSpec, StateStat};
pub use timers::HostTimers;
pub use units::HostUnits;
