//! Server script seam. The simulation owns events and capabilities; the
//! server supplies the language runtime. No runtime is installed by default.

use super::SkillHost;
use crate::units::UnitId;

pub const API_VERSION: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    AttackStart,
    AttackHit,
    CastStart,
    CastComplete,
    SkillUsed,
    HitTaken,
    StateStart,
    StateEnd,
    Kill,
    Drop,
    BuffEnd,
    Timer,
}

impl EventKind {
    pub const ALL: [Self; 12] = [
        Self::AttackStart,
        Self::AttackHit,
        Self::CastStart,
        Self::CastComplete,
        Self::SkillUsed,
        Self::HitTaken,
        Self::StateStart,
        Self::StateEnd,
        Self::Kill,
        Self::Drop,
        Self::BuffEnd,
        Self::Timer,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::AttackStart => "attack_start",
            Self::AttackHit => "attack_hit",
            Self::CastStart => "cast_start",
            Self::CastComplete => "cast_complete",
            Self::SkillUsed => "skill_used",
            Self::HitTaken => "hit_taken",
            Self::StateStart => "state_start",
            Self::StateEnd => "state_end",
            Self::Kill => "kill",
            Self::Drop => "drop",
            Self::BuffEnd => "buff_end",
            Self::Timer => "timer",
        }
    }
}

/// `id` is the skill, state or timer cookie, or zero. `other` is the
/// combat counterpart when known. Unit IDs are game-local opaque handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub kind: EventKind,
    pub unit: UnitId,
    pub other: Option<UnitId>,
    pub id: i32,
}

/// A reusable call block shared by skills, triggers and future registries.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallSpec {
    pub module: String,
    pub function: String,
    #[serde(default = "zero")]
    pub argument: super::data_mod::Formula,
}
fn zero() -> super::data_mod::Formula {
    super::data_mod::Formula::constant(0)
}
impl CallSpec {
    pub fn check(&self) -> Result<(), String> {
        if self.module.len() > 128
            || !(self.module.ends_with(".luau") || self.module.ends_with(".graph.toml"))
            || self.module.split('/').any(|part| {
                part.is_empty()
                    || part == "."
                    || part == ".."
                    || !part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
        {
            return Err(
                "module must be a relative scripts/ path ending in .luau or .graph.toml".into(),
            );
        }
        if self.function.is_empty()
            || self.function.len() > 64
            || !self
                .function
                .bytes()
                .enumerate()
                .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
        {
            return Err("function must be an ASCII identifier of at most 64 bytes".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallContext {
    pub unit: UnitId,
    pub other: Option<UnitId>,
    pub skill: i32,
    pub level: i32,
    pub argument: i32,
}

pub trait Runtime: Send + Sync {
    fn call(
        &self,
        _host: &mut dyn SkillHost,
        _spec: &CallSpec,
        _context: CallContext,
    ) -> Result<i32, String> {
        Err("script calls are unavailable".into())
    }
    fn event(&self, host: &mut dyn SkillHost, event: Event);
    fn tick(&self, host: &mut dyn SkillHost);
}
