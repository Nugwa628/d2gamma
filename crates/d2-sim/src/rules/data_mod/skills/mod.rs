//! Building-block skills: a mod's `files/skills.toml`.
//!
//! A skill made of blocks is a row of the `skills` table (added with a table
//! patch, so its mana cost, cast and attack animations, range, level
//! requirements and description are ordinary columns and the skill goes
//! through the normal skill path: client request, server decision, mana,
//! animation) whose `srvdofunc` is [`BLOCK_SLOT`]. This file says what the
//! skill does when it fires, as a list of parts run in order:
//!
//! ```toml
//! version = 1
//!
//! [[skill]]
//! id = 390                       # the skills row
//! name = "Ember Burst"           # for tools; the game ignores it
//!
//! [[skill.part]]
//! kind = "area_hit"              # damage everything around the target
//! radius = 12                    # sub-tiles
//! [skill.part.damage]
//! element = "fire"
//! min = "10 + lvl * 4"           # the engine's skill formula language
//! max = "20 + lvl * 6"
//!
//! [[skill.part]]
//! kind = "state"                 # then burn them: states.toml decides the stacks
//! op = "add_stack"
//! state = 380
//! on = "area"
//! radius = 12
//! seconds = 8
//! ```
//!
//! The parts are `missile`, `aura`, `summon`, `state`, `melee_hit`,
//! `area_hit`, `missile_ring`, `missile_arc`, `missile_volley`, `chain`,
//! `bounce`, `teleport`, `raise`, `minion_order`, `engine_body`, `call_script`,
//! `repeat` and `leap` (see [`Part`]). Every part is bounded: a fixed list, at most
//! [`MAX_PARTS`] of them, an area visits at most [`MAX_AREA_TARGETS`] units
//! in the unit lists' order, a summon at most [`MAX_SUMMONS`] pets. They draw
//! random numbers only from the casting unit's seed, so a replay gives the
//! same game.
//!
//! The host does no file work: it is handed the text once and keeps the
//! definitions ([`SkillRules`]).

use serde::Deserialize;

use super::cc::effects as cc_effects;
use super::StateRules;
use crate::combat::cc::CcEffect;
use crate::rules::{SkillFn, SkillHost, SkillProvider, Tick};
use crate::units::UnitId;

mod sentry;
pub use sentry::SentryPart;
mod form;
pub use form::FormPart;
mod vine;
pub use vine::VinePart;
mod damage_aura;
pub use damage_aura::DamageAuraPart;
mod passive;
pub use passive::PassivePart;
mod golem;
pub use golem::GolemPart;
mod raven;
pub use raven::RavenPart;
mod corpse_find;
pub use corpse_find::CorpseFindPart;
mod martial_charge;
pub use martial_charge::MartialChargePart;
mod shadow;
pub use shadow::ShadowPart;
mod ai_curse;
pub use ai_curse::AiCursePart;
mod projectile_fan;
pub use projectile_fan::ProjectileFanPart;
mod guided_projectile;
pub use guided_projectile::GuidedProjectilePart;
mod sight;
pub use sight::SightPart;
mod buff;
mod warcry;
pub use warcry::WarcryPart;
mod druid_summon;
pub use druid_summon::DruidSummonPart;
mod curve;
pub use buff::{BuffEvent, BuffPart};
mod formula;
mod run;
pub use curve::SkillParams;
pub use formula::Formula;

/// The one version of `skills.toml` this build reads.
pub const VERSION: u32 = 1;
/// The do slot (`srvdofunc`) of the skills table that a block skill's row
/// names. The provider answers it for every skill the file defines, by the
/// skill's row id.
pub const BLOCK_SLOT: u16 = 400;
/// Most parts in one skill.
pub const MAX_PARTS: usize = 16;
/// Most units one area part visits.
pub const MAX_AREA_TARGETS: usize = 64;
/// Most pets one summon part makes.
pub const MAX_SUMMONS: u32 = 8;
/// Most stats one aura part carries (the engine's aura state holds six).
pub const MAX_AURA_STATS: usize = 6;
/// Most missiles one ring, arc or volley part fires (a ring is 64 steps).
pub const MAX_MISSILES: i32 = 64;
/// Most jumps one chain or bounce part makes after the first hit.
pub const MAX_JUMPS: i32 = 16;
/// Most runs one `repeat` part makes (a 24 second channel at one run a
/// second is 24; a minute at ten a second is 600).
pub const MAX_REPEATS: i32 = 600;
/// Base ticks between the steps of a `leap`.
pub const LEAP_STEP_FRAMES: i32 = 2;
/// Most steps one `leap` takes.
pub const MAX_LEAP_STEPS: i32 = 50;
/// The most steps a `rush` part takes (five seconds).
pub const MAX_RUSH_STEPS: i32 = 125;
/// The most hits one `melee_hit` makes in a row.
pub const MAX_SEQUENCE: usize = 8;

/// A `skills.toml` that cannot be used. A setting error names the entry and
/// the key (`skill[1].part[0].radius`).
#[derive(Debug, thiserror::Error)]
pub enum SkillsError {
    #[error("invalid skills.toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid skills.toml setting: {0}")]
    Setting(String),
}

/// Whom an area, aura or state part reaches (the engine's unit filter).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Affects {
    /// Hostile players and monsters that can be attacked, not in town.
    #[default]
    Enemies,
    /// The caster's allies, the caster included.
    Allies,
    /// Every player and monster that can be attacked, not in town.
    Everyone,
    /// The caster's allies that are players, the caster included (a party aura
    /// that leaves pets and mercenaries out).
    AlliedPlayers,
}

impl Affects {
    /// The `aurafilter` bits.
    pub fn filter(self) -> u32 {
        const UNITS: u32 = 0x3 | 0x80 | 0x400 | 0x100;
        match self {
            Affects::Enemies => UNITS | 0x8000,
            Affects::Allies => 0x3 | 0x80 | 0x10000,
            Affects::Everyone => UNITS,
            Affects::AlliedPlayers => 0x1 | 0x80 | 0x10000,
        }
    }
}

/// The element of a damage part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Element {
    #[default]
    Physical,
    Fire,
    Lightning,
    Magic,
    Cold,
    Poison,
}

impl Element {
    /// The `elemtypes` row (0 is physical).
    pub fn id(self) -> i32 {
        match self {
            Element::Physical => 0,
            Element::Fire => 1,
            Element::Lightning => 2,
            Element::Magic => 3,
            Element::Cold => 4,
            Element::Poison => 5,
        }
    }
}

/// Damage a part deals: a roll between two formulas, in whole points.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Damage {
    #[serde(default)]
    pub element: Element,
    pub min: Formula,
    pub max: Formula,
    /// How long a poison, a cold slow lasts, in seconds; ignored by the
    /// other elements.
    #[serde(default = "zero")]
    pub seconds: Formula,
    /// A stat (the `itemstatcost` row) read as a percent on the caster that
    /// raises the damage: `+25` makes 125%. This is how a state's stat
    /// effect ("+Y% damage while charged") reaches a skill.
    #[serde(default)]
    pub bonus_stat: Option<u16>,
    /// Damage that grows with the stacks of a state on the caster.
    #[serde(default)]
    pub charge: Option<Charge>,
}

/// A charge-up: each stack of `state` on the caster adds `percent` to the
/// damage of the part, and the part can spend the stacks when it is done.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Charge {
    pub state: u16,
    /// Percent added per stack.
    pub percent: Formula,
    /// End the state on the caster once the part has hit.
    #[serde(default)]
    pub consume: bool,
}

fn zero() -> Formula {
    Formula::constant(0)
}

fn one() -> Formula {
    Formula::constant(1)
}

/// Fires an existing `missiles` row from the caster at the skill's target
/// point, as the skills table's own `srvmissile` column does. The missile
/// row's own columns say how it flies, what it hits and its damage.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissilePart {
    pub missile: u16,
    #[serde(default)]
    pub lob: bool,
    /// Drops the missile on the aim point, as Meteor does, instead of firing
    /// it from the caster: the point must be within 100 of the caster and
    /// nothing may block a missile of this row's size there.
    #[serde(default)]
    pub at_aim: bool,
    /// Sets the missile's length the way the engine's channels do: from the
    /// skills row's `calc1` at the skill's level, or the missile row's second
    /// parameter plus the level less one when that is not above 0.
    #[serde(default)]
    pub channel: bool,
    /// Crowd control the missile's hits declare (see [`CcEffect`]).
    #[serde(default, deserialize_with = "cc_effects")]
    pub cc: Vec<CcEffect>,
}

/// A wall of missiles across the skill's target, as Fire Wall lays it: two
/// missiles of the row run out from the target along the wall, and an optional
/// second row sits at its middle. The target is the target unit's place, else
/// the aim point; the place must be free of what blocks a missile of the row's
/// size and not in town.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WallPart {
    /// The missiles row of the two wall missiles.
    pub missile: u16,
    /// The missiles row at the wall's middle, if any.
    #[serde(default)]
    pub centre: Option<u16>,
}

/// Raises the dead monster that is the skill's target, as the monsters'
/// Resurrect does: the monster's own revive and its rise animation, with an
/// optional overlay on it. The target must be a monster that is dead or
/// dying, not hidden, with a free place under it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResurrectPart {
    /// The `overlay` row played on the risen monster, if any.
    #[serde(default)]
    pub overlay: Option<u16>,
}

/// Hatches a nest: spawns the monster that the skill's start function (the
/// engine's Nest start, `srvstfunc` 49, which the lifted row keeps) chose from
/// the caster's `monstats` spawn columns, at the place it chose, with an
/// optional overlay on it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NestPart {
    /// The `overlay` row played on the new monster, if any.
    #[serde(default)]
    pub overlay: Option<u16>,
}

/// Heals the caster's target by a share of its maximum life, as the
/// Zakarum heal does: the share is rolled between `min` and `max` percent.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealPart {
    /// The least percent of maximum life (0 to 100).
    pub min: Formula,
    /// The most percent of maximum life (0 to 100).
    pub max: Formula,
}

/// One stat of an aura part's state.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuraStat {
    pub stat: u16,
    pub value: Formula,
}

/// Puts a state with stats on the caster and on every unit around the
/// caster once, as an aura does each period. The aura state keeps its own
/// refresh rules, not the stack rules of `states.toml`; use a `state` part
/// for those.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuraPart {
    /// The state the caster gets (and the units around, unless
    /// `target_state` says another).
    pub state: u16,
    /// The state the units around the caster get, as the skills table's
    /// `auratargetstate` is apart from `aurastate`; leave it out for the
    /// same state as the caster's.
    #[serde(default)]
    pub target_state: Option<u16>,
    /// Like the skills table's `aurarangecalc`.
    pub range: Formula,
    /// How long the state stays, in seconds.
    pub seconds: Formula,
    #[serde(default = "allies")]
    pub affects: Affects,
    #[serde(default, rename = "stat")]
    pub stats: Vec<AuraStat>,
}

fn allies() -> Affects {
    Affects::Allies
}

fn enemies() -> Affects {
    Affects::Enemies
}

/// A curse: puts a state with stats on the units around the aim point, with
/// the curse tests of the engine's curse skills (hostile, alive, awake).
/// It reuses the stats of an aura part. The AI curses that make a monster
/// flee or turn on its own (Dim Vision, Terror, Taunt) stay with the
/// engine's body.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CursePart {
    pub state: u16,
    /// Sub-tiles around the aim point.
    pub range: Formula,
    /// How long the state stays, in seconds.
    pub seconds: Formula,
    #[serde(default = "enemies")]
    pub affects: Affects,
    #[serde(default, rename = "stat")]
    pub stats: Vec<AuraStat>,
}

/// Summons pets of a `monstats` row for the caster.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SummonPart {
    /// The `monstats` row.
    pub class: u16,
    /// The `pettype` row: pets of one type share a maximum.
    pub pet_type: u16,
    /// Most pets of this type at once; the oldest goes past it.
    #[serde(default = "one")]
    pub pet_max: Formula,
    /// How many are made by one cast (1..=[`MAX_SUMMONS`]).
    #[serde(default = "one")]
    pub count: Formula,
    /// The pet's level; 0 (default) follows the caster as the engine's
    /// summons do.
    #[serde(default = "zero")]
    pub level: Formula,
}

/// Fires a missile row in a ring of `count` evenly spaced missiles around
/// the caster, as a nova does.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissileRingPart {
    pub missile: u16,
    /// How many (1..=[`MAX_MISSILES`]); 64 is the engine's own nova.
    #[serde(default = "sixteen")]
    pub count: Formula,
    /// Velocity; leave it out to keep the missile row's own.
    #[serde(default)]
    pub velocity: Option<Formula>,
    /// Around the caster (default) or the aim point.
    #[serde(default = "caster")]
    pub around: Around,
}

/// Fires a missile row in an arc of `count` missiles toward the aim point.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissileArcPart {
    pub missile: u16,
    #[serde(default = "five")]
    pub count: Formula,
    /// How wide the arc is, in 64ths of a full circle (16 is a quarter).
    #[serde(default = "sixteen")]
    pub span: Formula,
    #[serde(default)]
    pub velocity: Option<Formula>,
}

/// Fires `count` missiles at the aim point, each with a small random
/// spread, as the engine's multi-shot skills do.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissileVolleyPart {
    pub missile: u16,
    #[serde(default = "three")]
    pub count: Formula,
}

/// Damage to the skill's target, then to the nearest other unit within
/// `radius` of it, and on: a chain hits each unit once, a bounce may return
/// to a unit it hit before (but not twice in a row).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainPart {
    /// How far a jump reaches, in sub-tiles.
    pub radius: Formula,
    /// Jumps after the first hit (0..=[`MAX_JUMPS`]).
    pub jumps: Formula,
    /// Percent of the damage lost on each jump.
    #[serde(default = "zero")]
    pub falloff: Formula,
    #[serde(default)]
    pub affects: Affects,
    pub damage: Damage,
    #[serde(default, deserialize_with = "cc_effects")]
    pub cc: Vec<CcEffect>,
}

/// Moves the caster to the aim point.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeleportPart {
    /// The farthest the caster goes, in sub-tiles; a farther aim point
    /// takes it that far toward it. Leave it out for no limit.
    #[serde(default)]
    pub max_range: Option<Formula>,
}

/// Raises the corpse the skill is aimed at as a pet of the caster.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RaisePart {
    /// The `pettype` row: pets of one type share a maximum.
    pub pet_type: u16,
    #[serde(default = "one")]
    pub pet_max: Formula,
}

/// What a `minion_order` part tells the caster's minions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// Move to the caster.
    Recall,
    /// Follow the caster again.
    Follow,
    /// Cast `skill` at the skill's target.
    Attack,
}

/// Runs D2Gamma's own body of one `skills` do-function slot, so a data skill
/// can keep the engine's behaviour for a step and add its own parts around
/// it. The body reads the skills row `skill` (this skill's by default) for its
/// numbers; it never runs the mod's slot, so it cannot call itself.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineBodyPart {
    /// The do-function slot (1 up to, not including, the engine's table size).
    pub slot: u16,
    /// The skills row whose numbers the body reads; this skill's by default.
    #[serde(default)]
    pub skill: Option<u16>,
    /// The level the body runs at; this skill's level by default.
    #[serde(default)]
    pub level: Option<Formula>,
}

/// Gives an order to the caster's minions within `radius` of the caster.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinionOrderPart {
    pub order: Order,
    /// Sub-tiles around the caster the minions are looked for.
    pub radius: Formula,
    /// For `attack`: the skills row each minion casts.
    #[serde(default)]
    pub skill: Option<u16>,
    /// For `attack`: its level (default: this skill's level).
    #[serde(default)]
    pub level: Option<Formula>,
}

/// Runs block skill parts over and over, as the casting skill: a channel
/// (every second a pulse of damage while the caster stands still), an aura
/// zone (every second an aura pulse around the caster), a charge that builds
/// (every second one more stack). What runs is another skill of this file
/// (`skill`, a row that is not itself a `repeat` or a `leap`), or, with no
/// `skill`, the parts that follow this one in the same skill (which are then
/// not run at the cast, only on each repeat; they must not repeat or leap
/// either). It runs with this skill's row and level.
///
/// A new cast of the skill ends the one before it. The repeats also end when
/// the caster dies, when `while_state` is given and the caster has lost it, and
/// when `stop_on_move` is set and the caster has left the spot it cast from.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepeatPart {
    /// The skills row whose parts run on each repeat; none for the parts
    /// that follow this one in the same skill.
    #[serde(default)]
    pub skill: Option<u16>,
    /// Base ticks between two runs (25 to a second; at least 1).
    #[serde(default = "twenty_five")]
    pub every: Formula,
    /// How many runs in all, 1 to [`MAX_REPEATS`].
    #[serde(default = "five")]
    pub times: Formula,
    /// Run the first one at once (default), or only after `every`.
    #[serde(default = "yes")]
    pub now: bool,
    /// Stop when the caster does not have this state.
    #[serde(default)]
    pub while_state: Option<u16>,
    /// Stop when the caster has moved.
    #[serde(default)]
    pub stop_on_move: bool,
}

/// Moves the caster to the aim point in steps over `frames`, then runs the
/// `land` skill's parts where it stands. Each step places the caster, so
/// walls do not stop a leap; it ends early where a step finds no free spot.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeapPart {
    /// How long the leap takes, in base ticks (a step every
    /// [`LEAP_STEP_FRAMES`]; at most [`MAX_LEAP_STEPS`] steps).
    #[serde(default = "ten")]
    pub frames: Formula,
    /// The farthest the caster goes, in sub-tiles. Leave it out for no
    /// limit.
    #[serde(default)]
    pub max_range: Option<Formula>,
    /// The skills row whose parts run on landing (an area hit around the
    /// caster, say).
    #[serde(default)]
    pub land: Option<u16>,
}

/// Runs the caster toward its target (else the aim point) one step every base
/// tick, as a charge or a rush does, for at most `steps` steps. It stops where
/// the target is within melee reach, where a step finds no free spot, or when
/// the steps are spent; then it runs the `land` skill's parts where the caster
/// stands (a melee hit on the target, say). With no `land` skill those are the
/// parts that follow this one in the same skill, which then run only where
/// the rush ends and not at the cast. The `each` skill's parts run after every
/// step (an area hit around the caster, for a whirling run).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RushPart {
    /// The speed, in percent of the caster's running speed (a step is a fifth of
    /// the run velocity times this, rounded up, at least one sub-tile).
    #[serde(default = "full_speed")]
    pub speed: Formula,
    /// The most steps, 1 to [`MAX_RUSH_STEPS`].
    #[serde(default = "twenty_five")]
    pub steps: Formula,
    /// Stop when the target is within melee reach (default), or run on to
    /// where the target stood.
    #[serde(default = "yes")]
    pub stop_in_reach: bool,
    /// The skills row whose parts run after each step.
    #[serde(default)]
    pub each: Option<u16>,
    /// The skills row whose parts run where the rush ends; none for the
    /// parts that follow this one in the same skill.
    #[serde(default)]
    pub land: Option<u16>,
}

fn full_speed() -> Formula {
    Formula::constant(100)
}

/// A part that starts ticks: only plain parts can run on them.
pub(super) fn is_timed(p: &Part) -> bool {
    matches!(p, Part::Repeat(_) | Part::Leap(_) | Part::Rush(_))
}

fn twenty_five() -> Formula {
    Formula::constant(25)
}

fn ten() -> Formula {
    Formula::constant(10)
}

fn yes() -> bool {
    true
}

fn sixteen() -> Formula {
    Formula::constant(16)
}

fn five() -> Formula {
    Formula::constant(5)
}

fn three() -> Formula {
    Formula::constant(3)
}

fn caster() -> Around {
    Around::Caster
}

/// What a `state` part does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StateOp {
    /// Put the state on; a second application does what `states.toml`
    /// says (`reapply`).
    #[default]
    Apply,
    /// Add `count` stacks (up to the state's `max_stacks`), whatever its
    /// `reapply` says; puts the state on first when it is off.
    AddStack,
    /// Take `count` stacks off; the state ends with the last one.
    RemoveStack,
    /// End the state.
    Remove,
}

/// Who a `state` part works on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StateTarget {
    /// The caster.
    #[default]
    Caster,
    /// The unit the skill is aimed at.
    Target,
    /// The units around the aim point (see `radius`, `affects`).
    Area,
}

/// Applies, stacks, unstacks or ends a state (a state with its row in the
/// `states` table; the stack rules come from `states.toml`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatePart {
    pub state: u16,
    #[serde(default)]
    pub op: StateOp,
    #[serde(default, rename = "on")]
    pub target: StateTarget,
    /// For `on = "area"`: sub-tiles around the aim point.
    #[serde(default)]
    pub radius: Option<Formula>,
    #[serde(default)]
    pub affects: Affects,
    /// How long it lasts, in seconds. Leave it out when the state's
    /// `states.toml` entry gives `duration_seconds`.
    #[serde(default)]
    pub seconds: Option<Formula>,
    /// Stacks for `add_stack` and `remove_stack`.
    #[serde(default = "one")]
    pub count: Formula,
}

/// A melee attack on the skill's target: the hit test, the weapon's damage
/// scaled by `weapon_percent`, and the skill's own `damage` on top.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeHitPart {
    /// Percent of the weapon's damage (0 for none, default 100).
    #[serde(default = "hundred")]
    pub weapon_percent: u32,
    /// Added to the attack rating of this hit.
    #[serde(default = "zero")]
    pub attack_rating: Formula,
    #[serde(default)]
    pub damage: Option<Damage>,
    /// Percent added to the damage of a hit that lands (the engine's Jab
    /// reads it from `calc1`).
    #[serde(default = "zero")]
    pub enhance: Formula,
    /// Adds the skills table row's own elemental damage at the skill's
    /// level (its `EType` columns, with the mastery and the length), as the
    /// engine's Jab does for a hit that lands.
    #[serde(default)]
    pub own_element: bool,
    /// Stun length of the hit, in frames, set whether or not it lands (the
    /// engine's Smite reads it from `calc2`).
    #[serde(default = "zero")]
    pub stun_frames: Formula,
    /// Hits in a row: the target, then other enemies within reach, each once,
    /// up to this many (the engine's Fend and Zeal swing at the next enemy
    /// after each hit). Zero or one is a single hit; the most is
    /// [`MAX_SEQUENCE`].
    #[serde(default = "zero")]
    pub sequence: Formula,
    /// Life and mana the attacker takes from a hit that lands, added to the
    /// weapon's own leech as the engine's Hunger adds its `calc2` and `calc3`.
    #[serde(default = "zero")]
    pub life_leech: Formula,
    #[serde(default = "zero")]
    pub mana_leech: Formula,
    /// A monster caster first takes the damage and to-hit of this attack
    /// mode of its `monstats` row (with the difficulty and level scaling the
    /// engine's monster skills use); a player caster is not changed.
    #[serde(default)]
    pub monster_mode: Option<AttackMode>,
    /// Crowd control the hit declares (`[[skill.part.cc]]`).
    #[serde(default, deserialize_with = "cc_effects")]
    pub cc: Vec<CcEffect>,
    /// An `overlay` row played on the target by a hit that lands.
    #[serde(default)]
    pub overlay: Option<u16>,
    /// A state a hit that lands puts on the target, for `target_frames` base
    /// frames (at least one), with the `target_stat` values on it.
    #[serde(default)]
    pub target_state: Option<u16>,
    #[serde(default = "zero")]
    pub target_frames: Formula,
    #[serde(default, rename = "target_stat")]
    pub target_stats: Vec<AuraStat>,
    /// Result, hit and hit-class flags (the `skills` table's `ResultFlags`,
    /// `HitFlags` and `HitClass`) set on a hit that lands: they say how the
    /// target reacts (knockback, stun, the hit sound).
    #[serde(default)]
    pub result_flags: u16,
    #[serde(default)]
    pub hit_flags: u32,
    #[serde(default)]
    pub hit_class: u8,
}

/// An attack mode of a `monstats` row: its first and second attack, or its
/// first skill mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttackMode {
    A1,
    A2,
    S1,
}

impl AttackMode {
    /// The mode number the engine's monster damage setup takes.
    pub fn mode(self) -> i32 {
        match self {
            AttackMode::A1 => 1,
            AttackMode::A2 => 5,
            AttackMode::S1 => 8,
        }
    }
}

fn hundred() -> u32 {
    100
}

/// Damage to every unit within a radius of the caster or of the aim point.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaHitPart {
    /// Sub-tiles (five to a tile).
    pub radius: Formula,
    /// Around the caster (`"caster"`) or the aim point (`"target"`, default).
    #[serde(default)]
    pub around: Around,
    #[serde(default)]
    pub affects: Affects,
    pub damage: Damage,
    /// Crowd control each hit declares (`[[skill.part.cc]]`).
    #[serde(default, deserialize_with = "cc_effects")]
    pub cc: Vec<CcEffect>,
}

/// The centre of an area part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Around {
    Caster,
    #[default]
    Target,
}

/// One part of a block skill.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Part {
    CallScript(crate::rules::scripts::CallSpec),
    Missile(MissilePart),
    Aura(AuraPart),
    Curse(CursePart),
    Heal(HealPart),
    Sentry(SentryPart),
    Form(FormPart),
    Vine(VinePart),
    DamageAura(DamageAuraPart),
    Passive(PassivePart),
    Golem(GolemPart),
    Raven(RavenPart),
    CorpseFind(CorpseFindPart),
    MartialCharge(MartialChargePart),
    Shadow(ShadowPart),
    AiCurse(AiCursePart),
    ProjectileFan(ProjectileFanPart),
    GuidedProjectile(GuidedProjectilePart),
    Sight(SightPart),
    TimedBuff(BuffPart),
    Warcry(WarcryPart),
    DruidSummon(DruidSummonPart),
    Wall(WallPart),
    Resurrect(ResurrectPart),
    Nest(NestPart),
    Summon(SummonPart),
    State(StatePart),
    MeleeHit(Box<MeleeHitPart>),
    AreaHit(AreaHitPart),
    MissileRing(MissileRingPart),
    MissileArc(MissileArcPart),
    MissileVolley(MissileVolleyPart),
    Chain(ChainPart),
    Bounce(ChainPart),
    Teleport(TeleportPart),
    Raise(RaisePart),
    MinionOrder(MinionOrderPart),
    EngineBody(EngineBodyPart),
    Repeat(RepeatPart),
    Leap(LeapPart),
    Rush(RushPart),
}

/// A skill made of parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillDef {
    /// The skills row.
    pub id: u16,
    /// A label for tools and messages; the game ignores it.
    pub name: String,
    pub parts: Vec<Part>,
}

/// A `[[skill]]` as the file has it; its parts are read one by one so an
/// error can name the part.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSkill {
    id: u16,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "part")]
    parts: Vec<toml::Value>,
}

/// All the block skills of a mod, by skills row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkillRules {
    defs: Vec<SkillDef>,
}

impl SkillRules {
    /// No block skills.
    pub fn none() -> SkillRules {
        SkillRules::default()
    }

    /// `true` when the mod defines no block skill.
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// The definitions, in file order.
    pub fn defs(&self) -> &[SkillDef] {
        &self.defs
    }

    /// The block skill on the skills row `id`.
    pub fn get(&self, id: i32) -> Option<&SkillDef> {
        let id = u16::try_from(id).ok()?;
        self.defs.iter().find(|d| d.id == id)
    }

    /// The crowd control the missile parts of block skill `skill` declare, for
    /// the hits of the missiles the skill fires; empty when it has none.
    pub fn missile_cc(&self, skill: i32) -> &[CcEffect] {
        self.get(skill)
            .and_then(|d| {
                d.parts.iter().find_map(|p| match p {
                    Part::Missile(m) if !m.cc.is_empty() => Some(m.cc.as_slice()),
                    _ => None,
                })
            })
            .unwrap_or(&[])
    }

    /// Reads a `skills.toml`. A wrong value is an error naming the entry
    /// and the key (`skill[1].part[0].radius`).
    pub fn from_toml(text: &str) -> Result<SkillRules, SkillsError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct File {
            #[serde(default = "file_version")]
            version: u32,
            #[serde(default, rename = "skill")]
            skills: Vec<RawSkill>,
        }
        let file: File = toml::from_str(text).map_err(|e| name_the_key(e, text))?;
        if file.version != VERSION {
            return Err(SkillsError::Setting(format!(
                "`version`: {} is not supported (this build reads version {VERSION})",
                file.version
            )));
        }
        let mut defs: Vec<SkillDef> = Vec::new();
        for (i, raw) in file.skills.into_iter().enumerate() {
            let mut parts = Vec::with_capacity(raw.parts.len());
            for (j, value) in raw.parts.into_iter().enumerate() {
                // The formulas are compiled here, so a bad one is reported
                // with the part it is in.
                let part = Part::deserialize(value).map_err(|e| {
                    SkillsError::Setting(format!("`skill[{i}].part[{j}]`: {}", e.message()))
                })?;
                parts.push(part);
            }
            let d = SkillDef {
                id: raw.id,
                name: raw.name,
                parts,
            };
            check(i, &d)?;
            if defs.iter().any(|o| o.id == d.id) {
                return Err(bad(
                    i,
                    None,
                    "id",
                    format!("skill {} is defined twice", d.id),
                ));
            }
            defs.push(d);
        }
        Ok(SkillRules { defs })
    }

    /// Checks the skills a `repeat` or `leap` part runs: each is a skill of
    /// this file, another than the one that names it, and not one that
    /// repeats or leaps itself (so ticks never start ticks).
    pub fn check_ticks(&self) -> Result<(), SkillsError> {
        for (i, d) in self.defs.iter().enumerate() {
            for (j, p) in d.parts.iter().enumerate() {
                let (key, id) = match p {
                    Part::Repeat(RepeatPart { skill: Some(s), .. }) => ("skill", *s),
                    Part::Repeat(RepeatPart { skill: None, .. }) => {
                        let tail = &d.parts[j + 1..];
                        if tail.is_empty() {
                            return Err(bad(
                                i,
                                Some(j),
                                "skill",
                                "no `skill` and no part after this one to repeat",
                            ));
                        }
                        if tail.iter().any(is_timed) {
                            return Err(bad(
                                i,
                                Some(j),
                                "skill",
                                "the parts after a repeat cannot repeat, leap or rush \
                                 themselves; only plain parts can run over time",
                            ));
                        }
                        continue;
                    }
                    Part::Leap(LeapPart { land: Some(l), .. }) => ("land", *l),
                    Part::Rush(r) => {
                        // Both skills a rush runs are checked the same way.
                        for (key, id) in [("each", r.each), ("land", r.land)] {
                            if let Some(id) = id {
                                self.check_timed_child(i, j, d.id, key, id)?;
                            }
                        }
                        if r.land.is_none() && d.parts[j + 1..].iter().any(is_timed) {
                            return Err(bad(
                                i,
                                Some(j),
                                "land",
                                "the parts after a rush cannot repeat, leap or rush                                  themselves; only plain parts can run over time",
                            ));
                        }
                        continue;
                    }
                    _ => continue,
                };
                let Some(child) = self.defs.iter().find(|o| o.id == id) else {
                    return Err(bad(
                        i,
                        Some(j),
                        key,
                        format!("skill {id} is not defined in this file"),
                    ));
                };
                if child.id == d.id {
                    return Err(bad(i, Some(j), key, "a skill cannot run itself"));
                }
                if child.parts.iter().any(is_timed) {
                    return Err(bad(
                        i,
                        Some(j),
                        key,
                        format!("skill {id} repeats, leaps or rushes itself; only plain skills can run over time"),
                    ));
                }
            }
        }
        Ok(())
    }

    /// The `key` skill (`id`) of the timed part `j` of skill `i` (`own`): a
    /// skill of this file, another than `own`, with no timed part itself.
    fn check_timed_child(
        &self,
        i: usize,
        j: usize,
        own: u16,
        key: &'static str,
        id: u16,
    ) -> Result<(), SkillsError> {
        let Some(child) = self.defs.iter().find(|o| o.id == id) else {
            return Err(bad(
                i,
                Some(j),
                key,
                format!("skill {id} is not defined in this file"),
            ));
        };
        if child.id == own {
            return Err(bad(i, Some(j), key, "a skill cannot run itself"));
        }
        if child.parts.iter().any(is_timed) {
            return Err(bad(
                i,
                Some(j),
                key,
                format!("skill {id} repeats, leaps or rushes itself; only plain skills can run over time"),
            ));
        }
        Ok(())
    }

    /// Checks the skills against the mod's states: a `state` part that gives
    /// no `seconds` needs a state whose `states.toml` entry has a duration,
    /// else the state would never end.
    pub fn check_states(&self, states: &StateRules) -> Result<(), SkillsError> {
        for (i, d) in self.defs.iter().enumerate() {
            for (j, p) in d.parts.iter().enumerate() {
                let Part::State(s) = p else { continue };
                let needs_time = matches!(s.op, StateOp::Apply | StateOp::AddStack);
                let has_time = s.seconds.is_some()
                    || states
                        .get(i32::from(s.state))
                        .is_some_and(|def| def.duration_seconds.is_some());
                if needs_time && !has_time {
                    return Err(bad(
                        i,
                        Some(j),
                        "seconds",
                        format!(
                            "state {} has no `duration_seconds` in states.toml, so this part \
                             needs `seconds` (it would never end)",
                            s.state
                        ),
                    ));
                }
            }
        }
        Ok(())
    }
}

fn file_version() -> u32 {
    VERSION
}

fn bad(i: usize, part: Option<usize>, key: &str, msg: impl std::fmt::Display) -> SkillsError {
    match part {
        Some(j) => SkillsError::Setting(format!("`skill[{i}].part[{j}].{key}`: {msg}")),
        None => SkillsError::Setting(format!("`skill[{i}].{key}`: {msg}")),
    }
}

/// A TOML error with the key its line holds, so the message says which setting.
fn name_the_key(error: toml::de::Error, text: &str) -> SkillsError {
    let message = error.message().to_owned();
    match error.span().and_then(|s| line_key(text, s.start)) {
        Some(key) => SkillsError::Setting(format!("`{key}`: {message}")),
        None => SkillsError::Toml(error),
    }
}

/// The `[header]` above byte `at` of `text` joined to the key on that line.
fn line_key(text: &str, at: usize) -> Option<String> {
    let at = at.min(text.len());
    let mut header = String::new();
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let end = start + line.len();
        let trimmed = line.trim();
        if at >= end {
            if trimmed.starts_with('[') {
                header = trimmed.trim_matches(|c| c == '[' || c == ']').to_owned();
            }
        } else {
            let key = trimmed.split('=').next()?.trim();
            if trimmed.contains('=') && !key.is_empty() && !key.starts_with('[') {
                return Some(if header.is_empty() {
                    key.to_owned()
                } else {
                    format!("{header}.{key}")
                });
            }
            return None;
        }
        start = end;
    }
    None
}

fn check(i: usize, d: &SkillDef) -> Result<(), SkillsError> {
    if d.parts.is_empty() || d.parts.len() > MAX_PARTS {
        return Err(bad(
            i,
            None,
            "part",
            format!("{} parts; a skill has 1 to {MAX_PARTS}", d.parts.len()),
        ));
    }
    for (j, p) in d.parts.iter().enumerate() {
        match p {
            Part::Sight(p) if p.state == 0 || p.state >= 0x8000 || p.stat >= 0x8000 => {
                return Err(bad(
                    i,
                    Some(j),
                    "sight",
                    "positive state and valid stat required",
                ))
            }
            Part::GuidedProjectile(p)
                if p.missile >= 0x8000 || p.non_bow.is_some_and(|m| m >= 0x8000) =>
            {
                return Err(bad(
                    i,
                    Some(j),
                    "guided_projectile",
                    "valid missile rows required",
                ))
            }
            Part::ProjectileFan(p)
                if p.missile >= 0x8000 || p.non_bow.is_some_and(|m| m >= 0x8000) =>
            {
                return Err(bad(
                    i,
                    Some(j),
                    "projectile_fan",
                    "valid missile rows required",
                ))
            }
            Part::AiCurse(p)
                if p.state == 0
                    || p.state >= 0x8000
                    || p.stats.len() > 5
                    || (p.attract && !p.stats.is_empty()) =>
            {
                return Err(bad(
                    i,
                    Some(j),
                    "ai_curse",
                    "positive state and at most five confuse stats required",
                ))
            }
            Part::MartialCharge(p)
                if p.state == 0
                    || p.state >= 0x8000
                    || p.counter >= 0x8000
                    || p.bonus.as_ref().is_some_and(|s| s.stat >= 0x8000) =>
            {
                return Err(bad(
                    i,
                    Some(j),
                    "martial_charge",
                    "invalid charge state or stat",
                ))
            }
            Part::Shadow(p)
                if p.class >= 0x8000
                    || p.pet_type >= 128
                    || p.mode > 15
                    || p.stats.len() > 11
                    || p.state.is_some_and(|s| s == 0 || s >= 0x8000)
                    || p.umod.is_some_and(|u| u == 0 || u > 42) =>
            {
                return Err(bad(
                    i,
                    Some(j),
                    "shadow",
                    "invalid shadow rows or stat count",
                ))
            }
            Part::Raven(p) if p.mode > 15 || p.class >= 0x8000 || p.pet_type >= 128 => {
                return Err(bad(i, Some(j), "raven", "invalid monster class or mode"))
            }
            Part::Golem(p) if p.mode > 15 || p.class >= 0x8000 || p.pet_type >= 128 => {
                return Err(bad(i, Some(j), "golem", "invalid monster class or mode"))
            }
            Part::Passive(p)
                if p.state == 0
                    || p.state >= 0x8000
                    || p.layer >= 0x8000
                    || p.stats.len() > 5
                    || p.blocked_by.is_some_and(|s| s == 0 || s >= 0x8000) =>
            {
                return Err(bad(
                    i,
                    Some(j),
                    "passive",
                    "positive state and at most five stats required",
                ))
            }
            Part::MeleeHit(p) if p.target_stats.len() > 6 => {
                return Err(bad(i, Some(j), "target_stat", "at most six stats"))
            }
            Part::DamageAura(p) if p.own_stats.len() > 5 || p.stats.len() > 6 => {
                return Err(bad(
                    i,
                    Some(j),
                    "damage_aura",
                    "at most five caster stats and six target stats",
                ))
            }
            Part::Form(p) if p.stats.len() > 6 => {
                return Err(bad(i, Some(j), "form", "at most six stats"))
            }
            Part::Vine(p) if p.pet_type >= 128 || p.class >= 0x8000 => {
                return Err(bad(i, Some(j), "vine", "invalid class or signed pet type"))
            }
            Part::Sentry(p) if p.class >= 0x8000 || p.pet_type >= 128 || p.mode > 15 => {
                return Err(bad(
                    i,
                    Some(j),
                    "sentry",
                    "invalid monster class, signed pet type or mode",
                ))
            }
            Part::Warcry(p) if p.stats.len() > 6 => {
                return Err(bad(i, Some(j), "warcry", "at most six stats"))
            }
            Part::DruidSummon(p) if p.mode > 15 || p.pet_type >= 128 || p.class >= 0x8000 => {
                return Err(bad(
                    i,
                    Some(j),
                    "druid_summon",
                    "invalid class, signed pet type or monster mode",
                ))
            }
            Part::TimedBuff(b)
                if b.events
                    .iter()
                    .any(|e| e.event > 255 || !(1..=31).contains(&e.callback)) =>
            {
                return Err(bad(
                    i,
                    Some(j),
                    "event",
                    "event must fit a byte and callback must be a filled slot (1..31)",
                ));
            }
            Part::TimedBuff(b) if b.stats.len() > 11 || b.events.len() > 3 => {
                return Err(bad(
                    i,
                    Some(j),
                    "timed_buff",
                    "at most 11 stats and 3 events",
                ));
            }
            Part::CallScript(call) => call
                .check()
                .map_err(|e| bad(i, Some(j), "call_script", e))?,
            Part::Aura(a) if a.stats.len() > MAX_AURA_STATS => {
                return Err(bad(
                    i,
                    Some(j),
                    "stat",
                    format!(
                        "{} stats; an aura state holds {MAX_AURA_STATS}",
                        a.stats.len()
                    ),
                ));
            }
            Part::Curse(c) if c.stats.len() > MAX_AURA_STATS => {
                return Err(bad(
                    i,
                    Some(j),
                    "stat",
                    format!(
                        "{} stats; a curse state holds {MAX_AURA_STATS}",
                        c.stats.len()
                    ),
                ));
            }
            Part::MeleeHit(m) if m.weapon_percent > 400 => {
                return Err(bad(i, Some(j), "weapon_percent", "more than 400"));
            }
            Part::State(s) if s.target == StateTarget::Area && s.radius.is_none() => {
                return Err(bad(i, Some(j), "radius", "`on = \"area\"` needs a radius"));
            }
            Part::EngineBody(e) if !(1..crate::skills::use_::table::DO_SLOTS).contains(&e.slot) => {
                return Err(bad(
                    i,
                    Some(j),
                    "slot",
                    format!(
                        "slot {}; the engine has slots 1 to {}",
                        e.slot,
                        crate::skills::use_::table::DO_SLOTS - 1
                    ),
                ));
            }
            Part::MinionOrder(m) if m.order == Order::Attack && m.skill.is_none() => {
                return Err(bad(
                    i,
                    Some(j),
                    "skill",
                    "`order = \"attack\"` needs the skill the minions cast",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

impl SkillProvider for SkillRules {
    fn refresh_passive(&self, w: &mut dyn SkillHost, u: UnitId, skill: i32) -> bool {
        let Some(def) = self.get(skill) else {
            return false;
        };
        let Some(p) = def.parts.iter().find_map(|p| {
            if let Part::Passive(p) = p {
                Some(p)
            } else {
                None
            }
        }) else {
            return false;
        };
        passive::refresh(w, u, skill, p);
        true
    }
    fn form_removed(&self, w: &mut dyn SkillHost, u: UnitId, state: u16) {
        for def in &self.defs {
            if def
                .parts
                .iter()
                .any(|p| matches!(p,Part::Form(p) if p.state==state))
            {
                w.clear_form_entry(u, i32::from(def.id));
            }
        }
    }
    fn warcry_hit(
        &self,
        w: &mut dyn SkillHost,
        source: UnitId,
        target: UnitId,
        skill: i32,
        level: i32,
    ) -> Option<bool> {
        let part = self.get(skill)?.parts.iter().find_map(|p| {
            if let Part::Warcry(p) = p {
                Some(p)
            } else {
                None
            }
        })?;
        Some(warcry::hit(w, source, target, skill, level, part))
    }
    fn has(&self, kind: SkillFn, index: u16) -> bool {
        kind == SkillFn::Do && index == BLOCK_SLOT && !self.is_empty()
    }

    fn srvdo(
        &self,
        w: &mut dyn SkillHost,
        index: u16,
        u: UnitId,
        skill: i32,
        lvl: i32,
        _charge: bool,
        _item: bool,
        _aim: bool,
    ) -> Option<i32> {
        if index != BLOCK_SLOT {
            return None;
        }
        // A row that names the block slot but has no entry does nothing.
        Some(
            self.get(skill)
                .map_or(0, |d| run::run(self, d, w, u, skill, lvl)),
        )
    }

    fn tick(&self, w: &mut dyn SkillHost, u: UnitId, tick: &Tick) {
        run::tick(self, w, u, tick);
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_parts;
