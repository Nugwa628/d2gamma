//! The mod kit's rule files as Studio edits them: `states.toml`,
//! `skills.toml`, `triggers.toml` and `quests.toml`.
//!
//! The structs here mirror the files key for key. Studio writes them back
//! canonically (comments and key order of a hand-written file are not kept) and
//! never decides what is valid itself: every check goes through the engine's own
//! readers ([`StateRules`], [`SkillRules`], [`TriggerRules`]) and its formula
//! compiler ([`Formula`]), so what Studio accepts is what the game loads.
use anyhow::{bail, Context, Result};
use d2_sim::rules::data_mod::blocks::{self, Family};
use d2_sim::rules::data_mod::{
    Formula, Objective, QuestRules, Reward, SkillParams, SkillRules, StateRules, TriggerRules,
    BLOCK_SLOT,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

pub const STATES_FILE: &str = "files/states.toml";
pub const SKILLS_FILE: &str = "files/skills.toml";
pub const TRIGGERS_FILE: &str = "files/triggers.toml";
pub const QUESTS_FILE: &str = "files/quests.toml";
/// The `srvdofunc` a skills row names to run its parts from `skills.toml`.
pub const BLOCK_SKILL_SLOT: u16 = BLOCK_SLOT;
/// Skill levels the curve chart covers.
pub const CURVE_LEVELS: std::ops::RangeInclusive<i32> = 1..=30;

/// What the block vocabulary says about a block, for a tooltip; empty for a name it
/// does not know. Studio lists blocks from its own tables and only borrows the
/// descriptions, so a block the engine adds shows up here without a Studio change.
pub fn block_help(family: Family, name: &str) -> &'static str {
    blocks::find(family, name).map_or("", |b| b.about)
}

/// One block of the reference Studio shows.
pub struct BlockRow {
    pub name: &'static str,
    pub label: &'static str,
    pub about: &'static str,
}

/// The blocks Studio shows for `file` (one of the three kit files), grouped by
/// family: the family title and its blocks.
pub fn block_reference(file: &str) -> Vec<(&'static str, Vec<BlockRow>)> {
    let families: &[Family] = match file {
        SKILLS_FILE => &[Family::SkillPart, Family::Target, Family::Value],
        QUESTS_FILE => &[Family::Objective, Family::Reward],
        TRIGGERS_FILE => &[
            Family::Event,
            Family::Condition,
            Family::Action,
            Family::Target,
        ],
        _ => &[],
    };
    families
        .iter()
        .map(|f| {
            let rows = blocks::family(*f)
                .iter()
                .map(|b| BlockRow {
                    name: b.name,
                    label: b.label,
                    about: b.about,
                })
                .collect();
            (f.title(), rows)
        })
        .collect()
}

/// A formula as written: a number, or text in the engine's skill formula language.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fm(pub String);
impl Fm {
    pub fn new(text: &str) -> Self {
        Fm(text.to_owned())
    }
    /// The engine compiler's verdict on the text.
    pub fn check(&self) -> Result<(), String> {
        Formula::compile(self.0.trim()).map(|_| ())
    }
    /// The engine's value at each level, for the skill row's `params`.
    pub fn curve(
        &self,
        params: &SkillParams,
        levels: std::ops::RangeInclusive<i32>,
    ) -> Vec<(i32, i32)> {
        Formula::compile(self.0.trim())
            .map(|f| f.curve(params, levels))
            .unwrap_or_default()
    }
}
impl Serialize for Fm {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.0.trim().parse::<i32>() {
            Ok(n) => s.serialize_i32(n),
            Err(_) => s.serialize_str(self.0.trim()),
        }
    }
}
impl<'de> Deserialize<'de> for Fm {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Number(i64),
            Text(String),
        }
        Ok(Fm(match Raw::deserialize(d)? {
            Raw::Number(n) => n.to_string(),
            Raw::Text(t) => t,
        }))
    }
}

fn file_version() -> u32 {
    1
}
fn one() -> u32 {
    1
}
fn one16() -> u16 {
    1
}
fn hundred() -> u32 {
    100
}
fn is_one(n: &u32) -> bool {
    *n == 1
}
fn is_one16(n: &u16) -> bool {
    *n == 1
}
fn is_hundred(n: &u32) -> bool {
    *n == 100
}
fn is_false(b: &bool) -> bool {
    !*b
}
fn is_zero<T: Default + PartialEq>(n: &T) -> bool {
    *n == T::default()
}
fn yes() -> bool {
    true
}
fn is_true(b: &bool) -> bool {
    *b
}

// ----------------------------------------------------------------- states

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectEntry {
    pub stat: u16,
    pub per_stack: i32,
    #[serde(default = "one16", skip_serializing_if = "is_one16")]
    pub min_stacks: u16,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flat: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateEntry {
    pub id: u16,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u32>,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub max_stacks: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reapply: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decay: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<u16>,
    #[serde(default, rename = "effect", skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectEntry>,
}
impl StateEntry {
    pub fn new(id: u16, name: &str) -> Self {
        Self {
            id,
            name: name.into(),
            duration_seconds: Some(10),
            max_stacks: 1,
            reapply: None,
            decay: None,
            icon: None,
            overlay: None,
            effects: Vec::new(),
        }
    }
}

pub const REAPPLY: &[(&str, &str)] = &[
    ("refresh", "Start the timer over"),
    ("add_stack", "Add a stack (and start the timer over)"),
    ("ignore", "Do nothing (keep the running timer)"),
];
pub const DECAY: &[(&str, &str)] = &[
    ("lose_all", "The whole state ends"),
    ("lose_one", "Lose one stack, then time again"),
];

// ----------------------------------------------------------------- skills

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
    pub min: Fm,
    pub max: Fm,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<Fm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bonus_stat: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charge: Option<ChargeEntry>,
}

/// Damage that grows with the stacks of a state on the caster.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChargeEntry {
    pub state: u16,
    pub percent: Fm,
    #[serde(default, skip_serializing_if = "is_false")]
    pub consume: bool,
}
impl DamageEntry {
    pub fn new(element: &str, min: &str, max: &str) -> Self {
        Self {
            element: Some(element.into()),
            min: Fm::new(min),
            max: Fm::new(max),
            seconds: None,
            bonus_stat: None,
            charge: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuraStatEntry {
    pub stat: u16,
    pub value: Fm,
}

/// One crowd-control effect declared by a hit-producing skill part.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlEntry {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chance: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuffEventEntry {
    pub event: u16,
    pub callback: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PartEntry {
    Sight {
        state: u16,
        stat: u16,
        value: Fm,
        range: Fm,
        frames: Fm,
        #[serde(default)]
        filter: u32,
    },
    GuidedProjectile {
        missile: u16,
        #[serde(default)]
        non_bow: Option<u16>,
        damage_percent: Fm,
    },
    ProjectileFan {
        #[serde(default)]
        empty_ok: bool,
        missile: u16,
        #[serde(default)]
        non_bow: Option<u16>,
        count: Fm,
        #[serde(default = "projectile_fan_zero")]
        central: Fm,
        #[serde(default = "projectile_fan_zero")]
        activate: Fm,
        #[serde(default)]
        jitter: bool,
    },
    AiCurse {
        state: u16,
        range: Fm,
        frames: Fm,
        #[serde(default)]
        filter: u32,
        #[serde(default)]
        attract: bool,
        #[serde(default, rename = "stat")]
        stats: Vec<AuraStatEntry>,
    },
    MartialCharge {
        state: u16,
        counter: u16,
        frames: Fm,
        attack_rating: Fm,
        source_damage: u8,
        #[serde(default)]
        claws: bool,
        #[serde(default)]
        bonus: Option<AuraStatEntry>,
    },
    Shadow {
        class: u16,
        pet_type: u16,
        mode: u8,
        pet_max: Fm,
        life_percent: Fm,
        item_level: Fm,
        frames: Fm,
        #[serde(default)]
        state: Option<u16>,
        #[serde(default)]
        umod: Option<u8>,
        #[serde(default, rename = "stat")]
        stats: Vec<AuraStatEntry>,
    },
    CorpseFind {
        chance: Fm,
        #[serde(default)]
        potion: bool,
        band1: Fm,
        band2: Fm,
        band3: Fm,
        band4: Fm,
    },
    Raven {
        class: u16,
        pet_type: u16,
        mode: u8,
        pet_max: Fm,
        level: Fm,
    },
    Golem {
        class: u16,
        pet_type: u16,
        mode: u8,
        pet_max: Fm,
        level: Fm,
    },
    Passive {
        state: u16,
        #[serde(default)]
        layer: u16,
        #[serde(default)]
        blocked_by: Option<u16>,
        #[serde(default, rename = "stat")]
        stats: Vec<AuraStatEntry>,
    },
    DamageAura {
        state: u16,
        #[serde(default)]
        target_state: Option<u16>,
        period: Fm,
        range: Fm,
        cost: Fm,
        filter: u32,
        #[serde(default)]
        freeze: bool,
        #[serde(default)]
        hit_class: u8,
        #[serde(default)]
        result_flags: u16,
        #[serde(default)]
        hit_flags: u32,
        #[serde(default, rename = "own_stat")]
        own_stats: Vec<AuraStatEntry>,
        #[serde(default, rename = "stat")]
        stats: Vec<AuraStatEntry>,
    },
    Form {
        state: u16,
        frames: Fm,
        delay: Fm,
        #[serde(default, rename = "stat")]
        stats: Vec<AuraStatEntry>,
    },
    Vine {
        class: u16,
        pet_type: u16,
        state: u16,
        pet_max: Fm,
        level: Fm,
    },
    Sentry {
        class: u16,
        pet_type: u16,
        mode: u8,
        pet_max: Fm,
        #[serde(default)]
        in_town: bool,
    },
    Warcry {
        missile: u16,
        state: Option<u16>,
        frames: Fm,
        #[serde(default, rename = "stat")]
        stats: Vec<AuraStatEntry>,
    },
    DruidSummon {
        class: u16,
        pet_type: u16,
        mode: u8,
        pet_max: Fm,
        level: Fm,
    },
    TimedBuff {
        state: u16,
        frames: Fm,
        #[serde(default, rename = "stat", skip_serializing_if = "Vec::is_empty")]
        stats: Vec<AuraStatEntry>,
        #[serde(default, rename = "event", skip_serializing_if = "Vec::is_empty")]
        events: Vec<BuffEventEntry>,
    },
    CallScript {
        module: String,
        function: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        argument: Option<Fm>,
    },
    Missile {
        missile: u16,
        #[serde(default, skip_serializing_if = "is_false")]
        lob: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        at_aim: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        cc: Vec<ControlEntry>,
        #[serde(default, skip_serializing_if = "is_false")]
        channel: bool,
    },
    Heal {
        min: Fm,
        max: Fm,
    },
    Resurrect {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        overlay: Option<u16>,
    },
    Nest {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        overlay: Option<u16>,
    },
    Wall {
        missile: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        centre: Option<u16>,
    },
    Aura {
        state: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_state: Option<u16>,
        range: Fm,
        seconds: Fm,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        affects: Option<String>,
        #[serde(default, rename = "stat", skip_serializing_if = "Vec::is_empty")]
        stats: Vec<AuraStatEntry>,
    },
    Curse {
        state: u16,
        range: Fm,
        seconds: Fm,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        affects: Option<String>,
        #[serde(default, rename = "stat", skip_serializing_if = "Vec::is_empty")]
        stats: Vec<AuraStatEntry>,
    },
    Summon {
        class: u16,
        pet_type: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pet_max: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level: Option<Fm>,
    },
    State {
        state: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        op: Option<String>,
        #[serde(default, rename = "on", skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        affects: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seconds: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<Fm>,
    },
    MeleeHit {
        #[serde(default = "hundred", skip_serializing_if = "is_hundred")]
        weapon_percent: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attack_rating: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        damage: Option<DamageEntry>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        cc: Vec<ControlEntry>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enhance: Option<Fm>,
        #[serde(default, skip_serializing_if = "is_false")]
        own_element: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sequence: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        life_leech: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mana_leech: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stun_frames: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        monster_mode: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        overlay: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_state: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_frames: Option<Fm>,
        #[serde(default, rename = "target_stat", skip_serializing_if = "Vec::is_empty")]
        target_stats: Vec<AuraStatEntry>,
        #[serde(default, skip_serializing_if = "is_zero")]
        result_flags: u16,
        #[serde(default, skip_serializing_if = "is_zero")]
        hit_flags: u32,
        #[serde(default, skip_serializing_if = "is_zero")]
        hit_class: u8,
    },
    AreaHit {
        radius: Fm,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        around: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        affects: Option<String>,
        damage: DamageEntry,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        cc: Vec<ControlEntry>,
    },
    MissileRing {
        missile: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        velocity: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        around: Option<String>,
    },
    MissileArc {
        missile: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        velocity: Option<Fm>,
    },
    MissileVolley {
        missile: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<Fm>,
    },
    Chain {
        radius: Fm,
        jumps: Fm,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        falloff: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        affects: Option<String>,
        damage: DamageEntry,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        cc: Vec<ControlEntry>,
    },
    Bounce {
        radius: Fm,
        jumps: Fm,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        falloff: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        affects: Option<String>,
        damage: DamageEntry,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        cc: Vec<ControlEntry>,
    },
    Teleport {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_range: Option<Fm>,
    },
    Raise {
        pet_type: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pet_max: Option<Fm>,
    },
    MinionOrder {
        order: String,
        radius: Fm,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skill: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level: Option<Fm>,
    },
    EngineBody {
        slot: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skill: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level: Option<Fm>,
    },
    Repeat {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skill: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        every: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        times: Option<Fm>,
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        now: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        while_state: Option<u16>,
        #[serde(default, skip_serializing_if = "is_false")]
        stop_on_move: bool,
    },
    Leap {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        frames: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_range: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        land: Option<u16>,
    },
    Rush {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        speed: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        steps: Option<Fm>,
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        stop_in_reach: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        each: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        land: Option<u16>,
    },
}

/// The building blocks the editor offers, in the order of `docs/modkit/skills.md`.
pub const PART_KINDS: &[(&str, &str)] = &[
    ("sight", "Sight pulse"),
    ("guided_projectile", "Guided projectile"),
    ("projectile_fan", "Projectile fan"),
    ("ai_curse", "AI curse"),
    ("martial_charge", "Martial charge"),
    ("shadow", "Shadow"),
    ("corpse_find", "Corpse find"),
    ("raven", "Raven"),
    ("golem", "Golem"),
    ("passive", "Passive"),
    ("damage_aura", "Damage aura"),
    ("form", "Form"),
    ("vine", "Vine"),
    ("sentry", "Placed sentry"),
    ("warcry", "Warcry"),
    ("druid_summon", "Druid summon"),
    ("timed_buff", "Timed self buff"),
    ("call_script", "Call script / graph"),
    ("missile", "Missile"),
    ("aura", "Aura pulse"),
    ("curse", "Curse"),
    ("heal", "Heal"),
    ("wall", "Wall (Fire Wall)"),
    ("resurrect", "Resurrect (raise a dead monster)"),
    ("nest", "Nest (hatch a monster)"),
    ("summon", "Summon"),
    ("state", "Apply state / add stack"),
    ("melee_hit", "Melee hit"),
    ("area_hit", "Area hit"),
    ("missile_ring", "Missile ring (nova)"),
    ("missile_arc", "Missile arc"),
    ("missile_volley", "Missile volley (multi-shot)"),
    ("chain", "Chain"),
    ("bounce", "Bounce"),
    ("teleport", "Teleport"),
    ("raise", "Raise corpse"),
    ("minion_order", "Minion order"),
    ("engine_body", "Engine skill body"),
    ("repeat", "Repeat over time (channel, aura zone)"),
    ("leap", "Leap"),
    ("rush", "Rush (charge at the target)"),
];
pub const MINION_ORDERS: &[(&str, &str)] = &[
    ("recall", "Come to you"),
    ("follow", "Follow you"),
    ("attack", "Cast a skill at the target"),
];
pub const ELEMENTS: &[&str] = &["physical", "fire", "lightning", "magic", "cold", "poison"];
pub const AFFECTS: &[(&str, &str)] = &[
    ("enemies", "Enemies"),
    ("allies", "Allies (and you)"),
    ("everyone", "Everyone"),
    ("allied_players", "Allied players (not pets)"),
];
pub const STATE_OPS: &[(&str, &str)] = &[
    ("apply", "Apply"),
    ("add_stack", "Add stacks"),
    ("remove_stack", "Remove stacks"),
    ("remove", "End the state"),
];
pub const STATE_TARGETS: &[(&str, &str)] = &[
    ("caster", "You"),
    ("target", "The target"),
    ("area", "Units around the aim point"),
];
pub const AROUND: &[(&str, &str)] = &[("target", "The aim point"), ("caster", "You")];

impl PartEntry {
    pub fn controls_mut(&mut self) -> Option<&mut Vec<ControlEntry>> {
        match self {
            Self::Missile { cc, .. }
            | Self::MeleeHit { cc, .. }
            | Self::AreaHit { cc, .. }
            | Self::Chain { cc, .. }
            | Self::Bounce { cc, .. } => Some(cc),
            _ => None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            PartEntry::Sight { .. } => "sight",
            PartEntry::GuidedProjectile { .. } => "guided_projectile",
            PartEntry::ProjectileFan { .. } => "projectile_fan",
            PartEntry::AiCurse { .. } => "ai_curse",
            PartEntry::MartialCharge { .. } => "martial_charge",
            PartEntry::Shadow { .. } => "shadow",
            PartEntry::CorpseFind { .. } => "corpse_find",
            PartEntry::Raven { .. } => "raven",
            PartEntry::Golem { .. } => "golem",
            PartEntry::Passive { .. } => "passive",
            PartEntry::DamageAura { .. } => "damage_aura",
            PartEntry::Form { .. } => "form",
            PartEntry::Vine { .. } => "vine",
            PartEntry::Sentry { .. } => "sentry",
            PartEntry::Warcry { .. } => "warcry",
            PartEntry::DruidSummon { .. } => "druid_summon",
            PartEntry::TimedBuff { .. } => "timed_buff",
            PartEntry::CallScript { .. } => "call_script",
            PartEntry::Missile { .. } => "missile",
            PartEntry::Aura { .. } => "aura",
            PartEntry::Curse { .. } => "curse",
            PartEntry::Heal { .. } => "heal",
            PartEntry::Wall { .. } => "wall",
            PartEntry::Resurrect { .. } => "resurrect",
            PartEntry::Nest { .. } => "nest",
            PartEntry::Summon { .. } => "summon",
            PartEntry::State { .. } => "state",
            PartEntry::MeleeHit { .. } => "melee_hit",
            PartEntry::AreaHit { .. } => "area_hit",
            PartEntry::MissileRing { .. } => "missile_ring",
            PartEntry::MissileArc { .. } => "missile_arc",
            PartEntry::MissileVolley { .. } => "missile_volley",
            PartEntry::Chain { .. } => "chain",
            PartEntry::Bounce { .. } => "bounce",
            PartEntry::Teleport { .. } => "teleport",
            PartEntry::Raise { .. } => "raise",
            PartEntry::MinionOrder { .. } => "minion_order",
            PartEntry::EngineBody { .. } => "engine_body",
            PartEntry::Repeat { .. } => "repeat",
            PartEntry::Leap { .. } => "leap",
            PartEntry::Rush { .. } => "rush",
        }
    }
    pub fn label(&self) -> &'static str {
        PART_KINDS
            .iter()
            .find(|(k, _)| *k == self.kind())
            .map_or("Part", |(_, l)| l)
    }
    /// A new part of `kind` with values that already pass the engine's checks.
    pub fn new(kind: &str) -> Option<Self> {
        Some(match kind {
            "sight" => PartEntry::Sight {
                state: 1,
                stat: 31,
                value: Fm::new("lvl*2"),
                range: Fm::new("12"),
                frames: Fm::new("60"),
                filter: 0,
            },
            "guided_projectile" => PartEntry::GuidedProjectile {
                missile: 0,
                non_bow: None,
                damage_percent: Fm::new("0"),
            },
            "projectile_fan" => PartEntry::ProjectileFan {
                empty_ok: false,
                missile: 0,
                non_bow: None,
                count: Fm::new("3"),
                central: Fm::new("0"),
                activate: Fm::new("0"),
                jitter: false,
            },
            "ai_curse" => PartEntry::AiCurse {
                state: 1,
                range: Fm::new("12"),
                frames: Fm::new("60"),
                filter: 0,
                attract: false,
                stats: Vec::new(),
            },
            "martial_charge" => PartEntry::MartialCharge {
                state: 1,
                counter: 204,
                frames: Fm::new("60"),
                attack_rating: Fm::new("0"),
                source_damage: 128,
                claws: false,
                bonus: None,
            },
            "shadow" => PartEntry::Shadow {
                class: 0,
                pet_type: 0,
                mode: 1,
                pet_max: Fm::new("1"),
                life_percent: Fm::new("(lvl-1)*par1"),
                item_level: Fm::new("ln56"),
                frames: Fm::new("0"),
                state: None,
                umod: None,
                stats: Vec::new(),
            },
            "corpse_find" => PartEntry::CorpseFind {
                chance: Fm::new("50"),
                potion: false,
                band1: Fm::new("par1"),
                band2: Fm::new("par2"),
                band3: Fm::new("par3"),
                band4: Fm::new("par4"),
            },
            "raven" => PartEntry::Raven {
                class: 0,
                pet_type: 0,
                mode: 1,
                pet_max: Fm::new("1"),
                level: Fm::new("lvl"),
            },
            "golem" => PartEntry::Golem {
                class: 0,
                pet_type: 0,
                mode: 1,
                pet_max: Fm::new("1"),
                level: Fm::new("0"),
            },
            "passive" => PartEntry::Passive {
                state: 1,
                layer: 0,
                blocked_by: None,
                stats: Vec::new(),
            },
            "damage_aura" => PartEntry::DamageAura {
                state: 0,
                target_state: None,
                period: Fm::new("25"),
                range: Fm::new("12"),
                cost: Fm::new("0"),
                filter: 0,
                freeze: false,
                hit_class: 0,
                result_flags: 0,
                hit_flags: 0,
                own_stats: Vec::new(),
                stats: Vec::new(),
            },
            "form" => PartEntry::Form {
                state: 0,
                frames: Fm::new("250"),
                delay: Fm::new("13"),
                stats: Vec::new(),
            },
            "vine" => PartEntry::Vine {
                class: 0,
                pet_type: 0,
                state: 150,
                pet_max: Fm::new("1"),
                level: Fm::new("lvl"),
            },
            "sentry" => PartEntry::Sentry {
                class: 0,
                pet_type: 0,
                mode: 1,
                pet_max: Fm::new("5"),
                in_town: false,
            },
            "warcry" => PartEntry::Warcry {
                missile: 0,
                state: None,
                frames: Fm::new("250"),
                stats: Vec::new(),
            },
            "druid_summon" => PartEntry::DruidSummon {
                class: 0,
                pet_type: 0,
                mode: 1,
                pet_max: Fm::new("1"),
                level: Fm::new("lvl"),
            },
            "timed_buff" => PartEntry::TimedBuff {
                state: 0,
                frames: Fm::new("250"),
                stats: Vec::new(),
                events: Vec::new(),
            },
            "call_script" => PartEntry::CallScript {
                module: "main.luau".into(),
                function: "run".into(),
                argument: None,
            },
            "missile" => PartEntry::Missile {
                missile: 0,
                lob: false,
                at_aim: false,
                cc: Vec::new(),
                channel: false,
            },
            "resurrect" => PartEntry::Resurrect { overlay: None },
            "nest" => PartEntry::Nest { overlay: None },
            "wall" => PartEntry::Wall {
                missile: 0,
                centre: None,
            },
            "heal" => PartEntry::Heal {
                min: Fm::new("10"),
                max: Fm::new("20"),
            },
            "aura" => PartEntry::Aura {
                state: 0,
                target_state: None,
                range: Fm::new("12"),
                seconds: Fm::new("10"),
                affects: None,
                stats: Vec::new(),
            },
            "curse" => PartEntry::Curse {
                state: 0,
                range: Fm::new("12"),
                seconds: Fm::new("10"),
                affects: None,
                stats: Vec::new(),
            },
            "summon" => PartEntry::Summon {
                class: 0,
                pet_type: 0,
                pet_max: None,
                count: None,
                level: None,
            },
            "state" => PartEntry::State {
                state: 0,
                op: None,
                target: None,
                radius: None,
                affects: None,
                seconds: Some(Fm::new("5")),
                count: None,
            },
            "melee_hit" => PartEntry::MeleeHit {
                weapon_percent: 100,
                attack_rating: None,
                damage: None,
                cc: Vec::new(),
                enhance: None,
                own_element: false,
                sequence: None,
                life_leech: None,
                mana_leech: None,
                stun_frames: None,
                monster_mode: None,
                overlay: None,
                target_state: None,
                target_frames: None,
                target_stats: Vec::new(),
                result_flags: 0,
                hit_flags: 0,
                hit_class: 0,
            },
            "area_hit" => PartEntry::AreaHit {
                radius: Fm::new("12"),
                around: None,
                affects: None,
                damage: DamageEntry::new("fire", "10 + lvl * 4", "20 + lvl * 6"),
                cc: Vec::new(),
            },
            "missile_ring" => PartEntry::MissileRing {
                missile: 0,
                count: None,
                velocity: None,
                around: None,
            },
            "missile_arc" => PartEntry::MissileArc {
                missile: 0,
                count: None,
                span: None,
                velocity: None,
            },
            "missile_volley" => PartEntry::MissileVolley {
                missile: 0,
                count: None,
            },
            "chain" => PartEntry::Chain {
                radius: Fm::new("12"),
                jumps: Fm::new("3"),
                falloff: Some(Fm::new("20")),
                affects: None,
                damage: DamageEntry::new("lightning", "10 + lvl * 4", "20 + lvl * 6"),
                cc: Vec::new(),
            },
            "bounce" => PartEntry::Bounce {
                radius: Fm::new("12"),
                jumps: Fm::new("3"),
                falloff: None,
                affects: None,
                damage: DamageEntry::new("physical", "10 + lvl * 4", "20 + lvl * 6"),
                cc: Vec::new(),
            },
            "teleport" => PartEntry::Teleport { max_range: None },
            "raise" => PartEntry::Raise {
                pet_type: 0,
                pet_max: None,
            },
            "minion_order" => PartEntry::MinionOrder {
                order: "recall".into(),
                radius: Fm::new("30"),
                skill: None,
                level: None,
            },
            "engine_body" => PartEntry::EngineBody {
                slot: 1,
                skill: None,
                level: None,
            },
            "repeat" => PartEntry::Repeat {
                skill: None,
                every: None,
                times: None,
                now: true,
                while_state: None,
                stop_on_move: false,
            },
            "leap" => PartEntry::Leap {
                frames: None,
                max_range: None,
                land: None,
            },
            "rush" => PartEntry::Rush {
                speed: None,
                steps: None,
                stop_in_reach: true,
                each: None,
                land: None,
            },
            _ => return None,
        })
    }
    /// Every formula of the part with a label, for validation and the curve chart.
    pub fn formulas(&self) -> Vec<(String, &Fm)> {
        let mut out: Vec<(String, &Fm)> = Vec::new();
        let optional = push_optional;
        match self {
            PartEntry::Sight {
                value,
                range,
                frames,
                ..
            } => {
                for (k, v) in [("value", value), ("range", range), ("frames", frames)] {
                    out.push((k.into(), v));
                }
            }
            PartEntry::GuidedProjectile { damage_percent, .. } => {
                out.push(("damage percent".into(), damage_percent))
            }
            PartEntry::ProjectileFan {
                count,
                central,
                activate,
                ..
            } => {
                for (k, v) in [
                    ("count", count),
                    ("central", central),
                    ("activate", activate),
                ] {
                    out.push((k.into(), v));
                }
            }
            PartEntry::AiCurse {
                range,
                frames,
                stats,
                ..
            } => {
                out.push(("range".into(), range));
                out.push(("frames".into(), frames));
                for (i, s) in stats.iter().enumerate() {
                    out.push((format!("stat {}", i + 1), &s.value));
                }
            }
            PartEntry::MartialCharge {
                frames,
                attack_rating,
                bonus,
                ..
            } => {
                out.push(("duration".into(), frames));
                out.push(("attack rating".into(), attack_rating));
                if let Some(s) = bonus {
                    out.push(("charge bonus".into(), &s.value));
                }
            }
            PartEntry::Shadow {
                pet_max,
                life_percent,
                item_level,
                frames,
                stats,
                ..
            } => {
                for (key, value) in [
                    ("pet max", pet_max),
                    ("life percent", life_percent),
                    ("item level", item_level),
                    ("frames", frames),
                ] {
                    out.push((key.into(), value));
                }
                for (i, s) in stats.iter().enumerate() {
                    out.push((format!("stat {}", i + 1), &s.value));
                }
            }
            PartEntry::CorpseFind {
                chance,
                band1,
                band2,
                band3,
                band4,
                ..
            } => {
                for (key, f) in [
                    ("chance", chance),
                    ("band1", band1),
                    ("band2", band2),
                    ("band3", band3),
                    ("band4", band4),
                ] {
                    out.push((key.into(), f));
                }
            }
            PartEntry::Raven { pet_max, level, .. } => {
                out.push(("pet max".into(), pet_max));
                out.push(("level".into(), level));
            }
            PartEntry::Golem { pet_max, level, .. } => {
                out.push(("pet max".into(), pet_max));
                out.push(("level".into(), level));
            }
            PartEntry::Passive { stats, .. } => {
                for (i, s) in stats.iter().enumerate() {
                    out.push((format!("stat {}", i + 1), &s.value));
                }
            }
            PartEntry::DamageAura {
                period,
                range,
                cost,
                own_stats,
                stats,
                ..
            } => {
                out.push(("period".into(), period));
                out.push(("range".into(), range));
                out.push(("cost".into(), cost));
                for (key, items) in [("own stat", own_stats), ("stat", stats)] {
                    for stat in items {
                        out.push((key.into(), &stat.value));
                    }
                }
            }
            PartEntry::Form {
                frames,
                delay,
                stats,
                ..
            } => {
                out.push(("frames".into(), frames));
                out.push(("delay".into(), delay));
                for stat in stats {
                    out.push(("stat".into(), &stat.value));
                }
            }
            PartEntry::Vine { pet_max, level, .. } => {
                out.push(("pet max".into(), pet_max));
                out.push(("level".into(), level));
            }
            PartEntry::Sentry { pet_max, .. } => out.push(("pet max".into(), pet_max)),
            PartEntry::DruidSummon { pet_max, level, .. } => {
                out.push(("pet max".into(), pet_max));
                out.push(("level".into(), level));
            }
            PartEntry::TimedBuff { frames, stats, .. }
            | PartEntry::Warcry { frames, stats, .. } => {
                out.push(("frames".into(), frames));
                for (i, s) in stats.iter().enumerate() {
                    out.push((format!("stat {} value", i + 1), &s.value));
                }
            }
            PartEntry::CallScript { argument, .. } => {
                if let Some(f) = argument {
                    out.push(("argument".into(), f));
                }
            }
            PartEntry::Missile { .. }
            | PartEntry::Wall { .. }
            | PartEntry::Resurrect { .. }
            | PartEntry::Nest { .. } => {}
            PartEntry::Heal { min, max } => {
                out.push(("min".into(), min));
                out.push(("max".into(), max));
            }
            PartEntry::Aura {
                range,
                seconds,
                stats,
                ..
            }
            | PartEntry::Curse {
                range,
                seconds,
                stats,
                ..
            } => {
                out.push(("range".into(), range));
                out.push(("seconds".into(), seconds));
                for (i, s) in stats.iter().enumerate() {
                    out.push((format!("stat {} value", i + 1), &s.value));
                }
            }
            PartEntry::Summon {
                pet_max,
                count,
                level,
                ..
            } => {
                optional(
                    &mut out,
                    &[("pet max", pet_max), ("count", count), ("pet level", level)],
                );
            }
            PartEntry::State {
                radius,
                seconds,
                count,
                ..
            } => {
                optional(
                    &mut out,
                    &[("radius", radius), ("seconds", seconds), ("count", count)],
                );
            }
            PartEntry::MeleeHit {
                attack_rating,
                damage,
                enhance,
                sequence,
                life_leech,
                mana_leech,
                stun_frames,
                target_frames,
                target_stats,
                ..
            } => {
                if let Some(f) = attack_rating {
                    out.push(("attack rating".into(), f));
                }
                optional(
                    &mut out,
                    &[
                        ("enhance", enhance),
                        ("hits in a row", sequence),
                        ("life leech", life_leech),
                        ("mana leech", mana_leech),
                        ("stun frames", stun_frames),
                        ("target frames", target_frames),
                    ],
                );
                for (i, s) in target_stats.iter().enumerate() {
                    out.push((format!("target stat {} value", i + 1), &s.value));
                }
                if let Some(d) = damage {
                    damage_formulas(&mut out, d);
                }
            }
            PartEntry::AreaHit { radius, damage, .. } => {
                out.push(("radius".into(), radius));
                damage_formulas(&mut out, damage);
            }
            PartEntry::MissileRing {
                count, velocity, ..
            } => optional(&mut out, &[("count", count), ("velocity", velocity)]),
            PartEntry::MissileArc {
                count,
                span,
                velocity,
                ..
            } => optional(
                &mut out,
                &[("count", count), ("span", span), ("velocity", velocity)],
            ),
            PartEntry::MissileVolley { count, .. } => optional(&mut out, &[("count", count)]),
            PartEntry::Chain {
                radius,
                jumps,
                falloff,
                damage,
                ..
            }
            | PartEntry::Bounce {
                radius,
                jumps,
                falloff,
                damage,
                ..
            } => {
                out.push(("radius".into(), radius));
                out.push(("jumps".into(), jumps));
                optional(&mut out, &[("falloff", falloff)]);
                damage_formulas(&mut out, damage);
            }
            PartEntry::Teleport { max_range } => optional(&mut out, &[("max range", max_range)]),
            PartEntry::Raise { pet_max, .. } => optional(&mut out, &[("pet max", pet_max)]),
            PartEntry::MinionOrder { radius, level, .. } => {
                out.push(("radius".into(), radius));
                optional(&mut out, &[("level", level)]);
            }
            PartEntry::EngineBody { level, .. } => optional(&mut out, &[("level", level)]),
            PartEntry::Repeat { every, times, .. } => {
                optional(&mut out, &[("every", every), ("times", times)]);
            }
            PartEntry::Leap {
                frames, max_range, ..
            } => optional(&mut out, &[("frames", frames), ("max range", max_range)]),
            PartEntry::Rush { speed, steps, .. } => {
                optional(&mut out, &[("speed", speed), ("steps", steps)]);
            }
        }
        out
    }
}
fn push_optional<'a>(out: &mut Vec<(String, &'a Fm)>, items: &[(&str, &'a Option<Fm>)]) {
    for (label, f) in items {
        if let Some(f) = f {
            out.push(((*label).into(), f));
        }
    }
}
fn damage_formulas<'a>(out: &mut Vec<(String, &'a Fm)>, d: &'a DamageEntry) {
    out.push(("damage min".into(), &d.min));
    out.push(("damage max".into(), &d.max));
    if let Some(s) = &d.seconds {
        out.push(("damage seconds".into(), s));
    }
    if let Some(c) = &d.charge {
        out.push(("charge percent".into(), &c.percent));
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillEntry {
    pub id: u16,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, rename = "part")]
    pub parts: Vec<PartEntry>,
}
impl SkillEntry {
    pub fn new(id: u16, name: &str) -> Self {
        Self {
            id,
            name: name.into(),
            parts: vec![PartEntry::new("area_hit").expect("known kind")],
        }
    }
}

// --------------------------------------------------------------- triggers

pub const EVENTS: &[(&str, &str)] = &[
    ("attack", "attack"),
    ("hit", "deal damage"),
    ("being_hit", "take damage"),
    ("skill_used", "use a skill"),
    ("kill", "kill something"),
    ("state_start", "a state starts"),
    ("state_end", "a state runs out"),
];
pub const CONDITION_KINDS: &[(&str, &str)] = &[
    ("has_state", "is in a state"),
    ("lacks_state", "is not in a state"),
    ("stacks_at_least", "has at least N stacks"),
    ("life_below", "life is below N%"),
    ("life_above", "life is at or above N%"),
];
pub const ACTION_KINDS: &[(&str, &str)] = &[
    ("call_script", "Call script / graph"),
    ("apply_state", "Apply a state"),
    ("add_stack", "Add stacks"),
    ("remove_stack", "Remove stacks"),
    ("remove_state", "End a state"),
    ("cast_skill", "Cast a skill"),
];
pub const WHO: &[(&str, &str)] = &[("self", "You"), ("other", "The other unit")];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConditionEntry {
    HasState {
        state: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        who: Option<String>,
    },
    LacksState {
        state: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        who: Option<String>,
    },
    StacksAtLeast {
        state: u16,
        count: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        who: Option<String>,
    },
    LifeBelow {
        percent: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        who: Option<String>,
    },
    LifeAbove {
        percent: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        who: Option<String>,
    },
}
impl ConditionEntry {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::HasState { .. } => "has_state",
            Self::LacksState { .. } => "lacks_state",
            Self::StacksAtLeast { .. } => "stacks_at_least",
            Self::LifeBelow { .. } => "life_below",
            Self::LifeAbove { .. } => "life_above",
        }
    }
    pub fn new(kind: &str) -> Option<Self> {
        Some(match kind {
            "has_state" => Self::HasState {
                state: 0,
                who: None,
            },
            "lacks_state" => Self::LacksState {
                state: 0,
                who: None,
            },
            "stacks_at_least" => Self::StacksAtLeast {
                state: 0,
                count: 3,
                who: None,
            },
            "life_below" => Self::LifeBelow {
                percent: 50,
                who: None,
            },
            "life_above" => Self::LifeAbove {
                percent: 50,
                who: None,
            },
            _ => return None,
        })
    }
    pub fn who(&self) -> Option<&String> {
        match self {
            Self::HasState { who, .. }
            | Self::LacksState { who, .. }
            | Self::StacksAtLeast { who, .. }
            | Self::LifeBelow { who, .. }
            | Self::LifeAbove { who, .. } => who.as_ref(),
        }
    }
    pub fn who_mut(&mut self) -> &mut Option<String> {
        match self {
            Self::HasState { who, .. }
            | Self::LacksState { who, .. }
            | Self::StacksAtLeast { who, .. }
            | Self::LifeBelow { who, .. }
            | Self::LifeAbove { who, .. } => who,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionEntry {
    CallScript {
        module: String,
        function: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        argument: Option<Fm>,
    },
    ApplyState {
        state: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seconds: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        who: Option<String>,
    },
    AddStack {
        state: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seconds: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        who: Option<String>,
    },
    RemoveStack {
        state: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        who: Option<String>,
    },
    RemoveState {
        state: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        who: Option<String>,
    },
    CastSkill {
        skill: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level: Option<Fm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<String>,
    },
}
impl ActionEntry {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CallScript { .. } => "call_script",
            Self::ApplyState { .. } => "apply_state",
            Self::AddStack { .. } => "add_stack",
            Self::RemoveStack { .. } => "remove_stack",
            Self::RemoveState { .. } => "remove_state",
            Self::CastSkill { .. } => "cast_skill",
        }
    }
    pub fn new(kind: &str) -> Option<Self> {
        Some(match kind {
            "call_script" => Self::CallScript {
                module: "main.luau".into(),
                function: "run".into(),
                argument: None,
            },
            "apply_state" => Self::ApplyState {
                state: 0,
                seconds: None,
                who: None,
            },
            "add_stack" => Self::AddStack {
                state: 0,
                count: None,
                seconds: None,
                who: None,
            },
            "remove_stack" => Self::RemoveStack {
                state: 0,
                count: None,
                who: None,
            },
            "remove_state" => Self::RemoveState {
                state: 0,
                who: None,
            },
            "cast_skill" => Self::CastSkill {
                skill: 0,
                level: None,
                at: None,
            },
            _ => return None,
        })
    }
    pub fn formulas(&self) -> Vec<(&'static str, &Fm)> {
        match self {
            Self::CallScript { argument, .. } => argument.iter().map(|f| ("argument", f)).collect(),
            Self::ApplyState { seconds, .. } => seconds.iter().map(|f| ("seconds", f)).collect(),
            Self::AddStack { count, seconds, .. } => count
                .iter()
                .map(|f| ("count", f))
                .chain(seconds.iter().map(|f| ("seconds", f)))
                .collect(),
            Self::RemoveStack { count, .. } => count.iter().map(|f| ("count", f)).collect(),
            Self::RemoveState { .. } => Vec::new(),
            Self::CastSkill { level, .. } => level.iter().map(|f| ("level", f)).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerEntry {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub on: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub while_skill: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub while_stat: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub while_state: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chance: Option<Fm>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_skill: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<u16>,
    #[serde(default, rename = "condition", skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<ConditionEntry>,
    #[serde(default, rename = "action")]
    pub actions: Vec<ActionEntry>,
}
impl TriggerEntry {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            on: "attack".into(),
            while_skill: Some(0),
            while_stat: None,
            while_state: None,
            chance: Some(Fm::new("10")),
            skills: Vec::new(),
            by_skill: None,
            state: None,
            conditions: Vec::new(),
            actions: vec![ActionEntry::ApplyState {
                state: 0,
                seconds: Some(Fm::new("1")),
                who: None,
            }],
        }
    }
}

/// One quest as `quests.toml` has it. The objectives and rewards are the
/// engine's own blocks, so what Studio writes is what the game reads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestEntry {
    pub id: u16,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub done_text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub giver: Option<u16>,
    #[serde(default, rename = "objective")]
    pub objectives: Vec<Objective>,
    #[serde(default, rename = "reward", skip_serializing_if = "Vec::is_empty")]
    pub rewards: Vec<Reward>,
}
impl QuestEntry {
    /// A quest with one objective, ready to be edited.
    pub fn new(id: u16, name: &str) -> Self {
        Self {
            id,
            name: name.into(),
            text: String::new(),
            done_text: String::new(),
            requires: Vec::new(),
            giver: None,
            objectives: vec![Objective::Kill {
                monster: 0,
                count: 1,
            }],
            rewards: Vec::new(),
        }
    }
    /// The first quest id not used by `others`.
    pub fn free_id(others: &[QuestEntry]) -> u16 {
        (1..=u16::MAX)
            .find(|id| others.iter().all(|q| q.id != *id))
            .unwrap_or(u16::MAX)
    }
}

/// A new objective of the given block name, with the registry's sample values.
pub fn new_objective(kind: &str) -> Option<Objective> {
    Some(match kind {
        "kill" => Objective::Kill {
            monster: 0,
            count: 1,
        },
        "enter" => Objective::Enter { level: 1 },
        "talk" => Objective::Talk { npc: 0 },
        "use_skill" => Objective::UseSkill { skill: 0, count: 1 },
        "reach_stat" => Objective::ReachStat { stat: 0, value: 1 },
        "collect" => Objective::Collect {
            item: "hp1".into(),
            count: 1,
            consume: false,
        },
        _ => return None,
    })
}
/// A new reward of the given block name.
pub fn new_reward(kind: &str) -> Option<Reward> {
    Some(match kind {
        "stat" => Reward::Stat { stat: 0, value: 1 },
        "message" => Reward::Message {
            text: String::new(),
        },
        "item" => Reward::Item {
            item: "hp1".into(),
            count: 1,
        },
        _ => return None,
    })
}
/// The block name of an objective, as written in the file.
pub fn objective_kind(o: &Objective) -> &'static str {
    match o {
        Objective::Kill { .. } => "kill",
        Objective::Enter { .. } => "enter",
        Objective::Talk { .. } => "talk",
        Objective::UseSkill { .. } => "use_skill",
        Objective::ReachStat { .. } => "reach_stat",
        Objective::Collect { .. } => "collect",
    }
}
/// The block name of a reward.
pub fn reward_kind(r: &Reward) -> &'static str {
    match r {
        Reward::Stat { .. } => "stat",
        Reward::Message { .. } => "message",
        Reward::Item { .. } => "item",
    }
}

/// An objective in plain words, with row names where the tables are loaded.
pub fn objective_text(names: &Names, o: &Objective) -> String {
    let row = |list: &[(u32, String)], id: u16, what: &str| {
        Names::find(list, id).map_or_else(|| format!("{what} {id}"), str::to_owned)
    };
    match o {
        Objective::Kill { monster, count } => {
            format!(
                "Kill {count} x {}",
                row(&names.monsters, *monster, "monster")
            )
        }
        Objective::Enter { level } => format!("Enter level {level}"),
        Objective::Talk { npc } => format!("Talk to {}", row(&names.monsters, *npc, "NPC")),
        Objective::UseSkill { skill, count } => {
            format!(
                "Cast {} {count} time(s)",
                row(&names.skills, *skill, "skill")
            )
        }
        Objective::ReachStat { stat, value } => {
            format!("Have {value} or more {}", row(&names.stats, *stat, "stat"))
        }
        Objective::Collect {
            item,
            count,
            consume,
        } => format!(
            "Collect {count} x {item}{}",
            if *consume { " (handed in)" } else { "" }
        ),
    }
}
/// A reward in plain words.
pub fn reward_text(names: &Names, r: &Reward) -> String {
    match r {
        Reward::Stat { stat, value } => {
            let what = Names::find(&names.stats, *stat)
                .map_or_else(|| format!("stat {stat}"), str::to_owned);
            format!("{value:+} {what}")
        }
        Reward::Message { text } => format!("Say \"{text}\""),
        Reward::Item { item, count } => format!("Give {count} x {item}"),
    }
}
impl QuestEntry {
    /// One line for the quest list: its name and what it asks.
    pub fn summary(&self, names: &Names) -> String {
        let asks: Vec<String> = self
            .objectives
            .iter()
            .map(|o| objective_text(names, o))
            .collect();
        format!("{}: {}", self.name, asks.join(", "))
    }
}

// ------------------------------------------------------------------ files

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatesFile {
    #[serde(default = "file_version")]
    version: u32,
    #[serde(default, rename = "state")]
    states: Vec<StateEntry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillsFile {
    #[serde(default = "file_version")]
    version: u32,
    #[serde(default, rename = "skill")]
    skills: Vec<SkillEntry>,
}
/// The one skill a `[[skill]]` entry of `text` holds.
pub fn parse_skill(text: &str) -> Result<SkillEntry> {
    let mut file: SkillsFile = toml::from_str(text)?;
    if file.skills.len() != 1 {
        bail!("{} skills; expected one", file.skills.len());
    }
    Ok(file.skills.remove(0))
}
/// `skill` as the text of a skills file: the form the editor saves.
pub fn skill_toml(skill: &SkillEntry) -> String {
    toml::to_string(&SkillsFile {
        version: file_version(),
        skills: vec![skill.clone()],
    })
    .unwrap_or_default()
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TriggersFile {
    #[serde(default = "file_version")]
    version: u32,
    #[serde(default, rename = "trigger")]
    triggers: Vec<TriggerEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuestsFile {
    #[serde(default = "file_version")]
    version: u32,
    #[serde(default, rename = "quest")]
    quests: Vec<QuestEntry>,
}

/// A thing the engine refuses, with the file it is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub file: &'static str,
    pub message: String,
}

/// Row names of the working tables, for readable text. Empty when no install is loaded.
#[derive(Clone, Debug, Default)]
pub struct Names {
    pub states: Vec<(u32, String)>,
    pub skills: Vec<(u32, String)>,
    pub stats: Vec<(u32, String)>,
    pub overlays: Vec<(u32, String)>,
    pub missiles: Vec<(u32, String)>,
    pub monsters: Vec<(u32, String)>,
}
impl Names {
    fn find(list: &[(u32, String)], id: u16) -> Option<&str> {
        list.iter()
            .find(|(i, _)| *i == u32::from(id))
            .map(|(_, n)| n.as_str())
            .filter(|n| !n.is_empty())
    }
}

/// The three files plus the bookkeeping to save them safely.
#[derive(Clone, Debug, Default)]
pub struct Kit {
    pub root: PathBuf,
    pub states: Vec<StateEntry>,
    pub skills: Vec<SkillEntry>,
    pub triggers: Vec<TriggerEntry>,
    pub quests: Vec<QuestEntry>,
    pub dirty: bool,
    on_disk: [Option<String>; 4],
}

fn read(root: &Path, rel: &str) -> Result<Option<String>> {
    match fs::read_to_string(root.join(rel)) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {rel}")),
    }
}

impl Kit {
    /// Reads the project's three files; a missing file is an empty list. A file
    /// the editor cannot read is an error and nothing is overwritten.
    pub fn load(root: &Path) -> Result<Kit> {
        let texts = [
            read(root, STATES_FILE)?,
            read(root, SKILLS_FILE)?,
            read(root, TRIGGERS_FILE)?,
            read(root, QUESTS_FILE)?,
        ];
        let mut kit = Kit {
            root: root.to_owned(),
            ..Kit::default()
        };
        if let Some(t) = &texts[0] {
            kit.states = toml::from_str::<StatesFile>(t)
                .with_context(|| format!("{STATES_FILE} cannot be opened in the editor"))?
                .states;
        }
        if let Some(t) = &texts[1] {
            kit.skills = toml::from_str::<SkillsFile>(t)
                .with_context(|| format!("{SKILLS_FILE} cannot be opened in the editor"))?
                .skills;
        }
        if let Some(t) = &texts[2] {
            kit.triggers = toml::from_str::<TriggersFile>(t)
                .with_context(|| format!("{TRIGGERS_FILE} cannot be opened in the editor"))?
                .triggers;
        }
        if let Some(t) = &texts[3] {
            kit.quests = toml::from_str::<QuestsFile>(t)
                .with_context(|| format!("{QUESTS_FILE} cannot be opened in the editor"))?
                .quests;
        }
        kit.on_disk = texts;
        Ok(kit)
    }

    /// The canonical text of each file.
    pub fn texts(&self) -> Result<[String; 4]> {
        Ok([
            toml::to_string(&StatesFile {
                version: 1,
                states: self.states.clone(),
            })?,
            toml::to_string(&SkillsFile {
                version: 1,
                skills: self.skills.clone(),
            })?,
            toml::to_string(&TriggersFile {
                version: 1,
                triggers: self.triggers.clone(),
            })?,
            toml::to_string(&QuestsFile {
                version: 1,
                quests: self.quests.clone(),
            })?,
        ])
    }

    /// Writes the files that have entries (or exist already). Refuses when one
    /// of them was changed outside Studio since it was read.
    pub fn save(&mut self) -> Result<()> {
        let texts = self.texts()?;
        let files = [STATES_FILE, SKILLS_FILE, TRIGGERS_FILE, QUESTS_FILE];
        for (i, rel) in files.iter().enumerate() {
            if read(&self.root, rel)? != self.on_disk[i] {
                bail!("{rel} changed outside Studio. Reopen the project before saving.");
            }
        }
        let empty = [
            self.states.is_empty(),
            self.skills.is_empty(),
            self.triggers.is_empty(),
            self.quests.is_empty(),
        ];
        for (i, rel) in files.iter().enumerate() {
            if empty[i] && self.on_disk[i].is_none() {
                continue;
            }
            let path = self.root.join(rel);
            fs::create_dir_all(path.parent().context("files folder")?)?;
            fs::write(&path, &texts[i])?;
            self.on_disk[i] = Some(texts[i].clone());
        }
        self.dirty = false;
        Ok(())
    }

    /// Everything the engine objects to, file by file, using the engine's own
    /// readers and their cross-file checks (the same ones the game runs).
    pub fn problems(&self) -> Vec<Problem> {
        let mut out = Vec::new();
        let texts = match self.texts() {
            Ok(t) => t,
            Err(e) => {
                return vec![Problem {
                    file: "files",
                    message: e.to_string(),
                }];
            }
        };
        let mut push = |file: &'static str, e: &dyn std::fmt::Display| {
            out.push(Problem {
                file,
                message: e.to_string(),
            });
        };
        let mut states = match StateRules::from_toml(&texts[0]) {
            Ok(s) => s,
            Err(e) => {
                push(STATES_FILE, &e);
                StateRules::none()
            }
        };
        match SkillRules::from_toml(&texts[1]) {
            Ok(skills) => {
                if let Err(e) = skills
                    .check_states(&states)
                    .and_then(|()| skills.check_ticks())
                {
                    push(SKILLS_FILE, &e);
                }
            }
            Err(e) => push(SKILLS_FILE, &e),
        }
        match TriggerRules::from_toml(&texts[2]) {
            Ok(triggers) => {
                if let Err(e) = triggers.check_states(&mut states) {
                    push(TRIGGERS_FILE, &e);
                }
            }
            Err(e) => push(TRIGGERS_FILE, &e),
        }
        if let Err(e) = QuestRules::from_toml(&texts[3]) {
            push(QUESTS_FILE, &e);
        }
        out
    }

    pub fn state(&self, id: u16) -> Option<&StateEntry> {
        self.states.iter().find(|s| s.id == id)
    }

    /// The label of a state: the file's `name`, else the table row's, else `state N`.
    pub fn state_name(&self, names: &Names, id: u16) -> String {
        self.state(id)
            .map(|s| s.name.as_str())
            .filter(|n| !n.is_empty())
            .or_else(|| Names::find(&names.states, id))
            .map_or_else(|| format!("state {id}"), str::to_owned)
    }
    /// The label of a missiles row: the table row's name, else `missile N`.
    fn missile_name(&self, names: &Names, id: u16) -> String {
        Names::find(&names.missiles, id).map_or_else(|| format!("missile {id}"), str::to_owned)
    }
    pub fn skill_name(&self, names: &Names, id: u16) -> String {
        self.skills
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.as_str())
            .filter(|n| !n.is_empty())
            .or_else(|| Names::find(&names.skills, id))
            .map_or_else(|| format!("skill {id}"), str::to_owned)
    }
    pub fn stat_label(&self, names: &Names, id: u16) -> String {
        Names::find(&names.stats, id).map_or_else(|| format!("stat {id}"), str::to_owned)
    }

    /// "at 3 stacks: +25% damage", one line per count where something changes.
    pub fn state_preview(&self, names: &Names, s: &StateEntry) -> Vec<String> {
        let mut counts: BTreeSet<u32> = BTreeSet::new();
        counts.insert(1);
        counts.insert(s.max_stacks.max(1));
        for e in &s.effects {
            let n = u32::from(e.min_stacks);
            if n <= s.max_stacks {
                counts.insert(n);
            }
        }
        let mut lines = Vec::new();
        for n in counts {
            let parts: Vec<String> = s
                .effects
                .iter()
                .filter_map(|e| {
                    let have = n.min(u32::from(u16::MAX)) as u16;
                    if have < e.min_stacks {
                        return None;
                    }
                    let value = if e.flat {
                        e.per_stack
                    } else {
                        e.per_stack.saturating_mul(i32::from(have))
                    };
                    Some(effect_text(&self.stat_label(names, e.stat), value))
                })
                .collect();
            let what = if parts.is_empty() {
                "no bonus".to_owned()
            } else {
                parts.join(", ")
            };
            lines.push(format!(
                "at {n} stack{}: {what}",
                if n == 1 { "" } else { "s" }
            ));
        }
        lines
    }

    /// One sentence on how the state lives.
    pub fn state_summary(&self, s: &StateEntry) -> String {
        let time = s
            .duration_seconds
            .map_or("the applying skill's time".to_owned(), |d| format!("{d} s"));
        let reapply = match s.reapply.as_deref().unwrap_or("refresh") {
            "add_stack" => "applying it again adds a stack and restarts the timer",
            "ignore" => "applying it again does nothing",
            _ => "applying it again restarts the timer",
        };
        let decay = match s.decay.as_deref().unwrap_or("lose_all") {
            "lose_one" => "when time runs out it loses one stack and times again",
            _ => "when time runs out it ends",
        };
        format!(
            "Lasts {time}, up to {} stack{}; {reapply}; {decay}.",
            s.max_stacks,
            if s.max_stacks == 1 { "" } else { "s" }
        )
    }

    /// The trigger as one readable sentence.
    pub fn trigger_sentence(&self, names: &Names, t: &TriggerEntry) -> String {
        let state = |id: u16| self.state_name(names, id);
        let skill = |id: u16| self.skill_name(names, id);
        let event = match t.on.as_str() {
            "attack" => "you attack".to_owned(),
            "hit" => "you hit something".to_owned(),
            "being_hit" => "you are hit".to_owned(),
            "skill_used" if t.skills.is_empty() => "you use a skill".to_owned(),
            "skill_used" => format!(
                "you use {}",
                t.skills
                    .iter()
                    .map(|s| skill(*s))
                    .collect::<Vec<_>>()
                    .join(" or ")
            ),
            "kill" => "you kill something".to_owned(),
            "state_start" => format!("{} starts on you", state(t.state.unwrap_or(0))),
            "state_end" => format!("{} runs out", state(t.state.unwrap_or(0))),
            other => other.to_owned(),
        };
        let event = match t.by_skill {
            Some(s) => format!("{event} with {}", skill(s)),
            None => event,
        };
        let chance = match &t.chance {
            None => String::new(),
            Some(c) => format!(" ({}%)", c.0.trim()),
        };
        let who = |w: Option<&String>, me: &str, other: &str| {
            if w.is_some_and(|w| w == "other") {
                other.to_owned()
            } else {
                me.to_owned()
            }
        };
        let conditions: Vec<String> = t
            .conditions
            .iter()
            .map(|c| {
                let w = c.who();
                let (me, other) = ("you", "the other unit");
                let subject = who(w, me, other);
                match c {
                    ConditionEntry::HasState { state: s, .. } => {
                        format!("{subject} {} in {}", be(&subject), state(*s))
                    }
                    ConditionEntry::LacksState { state: s, .. } => {
                        format!("{subject} {} not in {}", be(&subject), state(*s))
                    }
                    ConditionEntry::StacksAtLeast {
                        state: s, count, ..
                    } => {
                        format!(
                            "{subject} {} {count}+ stacks of {}",
                            have(&subject),
                            state(*s)
                        )
                    }
                    ConditionEntry::LifeBelow { percent, .. } => {
                        format!("{} life is below {percent}%", poss(&subject))
                    }
                    ConditionEntry::LifeAbove { percent, .. } => {
                        format!("{} life is at least {percent}%", poss(&subject))
                    }
                }
            })
            .collect();
        let actions: Vec<String> = t
            .actions
            .iter()
            .map(|a| {
                let on = |w: &Option<String>| {
                    if w.as_deref() == Some("other") {
                        " on the other unit"
                    } else {
                        ""
                    }
                };
                let secs = |s: &Option<Fm>| {
                    s.as_ref()
                        .map_or(String::new(), |s| format!(" for {} s", s.0.trim()))
                };
                let n =
                    |c: &Option<Fm>| c.as_ref().map_or("1".to_owned(), |c| c.0.trim().to_owned());
                match a {
                    ActionEntry::CallScript {
                        module, function, ..
                    } => format!("call {module}::{function}"),
                    ActionEntry::ApplyState {
                        state: s,
                        seconds,
                        who,
                    } => {
                        format!("apply {}{}{}", state(*s), secs(seconds), on(who))
                    }
                    ActionEntry::AddStack {
                        state: s,
                        count,
                        seconds,
                        who,
                    } => {
                        let c = n(count);
                        format!(
                            "add {c} stack{} of {}{}{}",
                            if c == "1" { "" } else { "s" },
                            state(*s),
                            secs(seconds),
                            on(who)
                        )
                    }
                    ActionEntry::RemoveStack {
                        state: s,
                        count,
                        who,
                    } => {
                        let c = n(count);
                        format!(
                            "remove {c} stack{} of {}{}",
                            if c == "1" { "" } else { "s" },
                            state(*s),
                            on(who)
                        )
                    }
                    ActionEntry::RemoveState { state: s, who } => {
                        format!("end {}{}", state(*s), on(who))
                    }
                    ActionEntry::CastSkill {
                        skill: s,
                        level,
                        at,
                    } => {
                        let lv = level
                            .as_ref()
                            .map_or(String::new(), |l| format!(" at level {}", l.0.trim()));
                        let at = if at.as_deref() == Some("self") {
                            "yourself"
                        } else {
                            "the other unit"
                        };
                        format!("cast {}{lv} at {at}", skill(*s))
                    }
                }
            })
            .collect();
        let mut text = format!("When {event}{chance}");
        if !conditions.is_empty() {
            text.push_str(", if ");
            text.push_str(&conditions.join(" and "));
        }
        text.push_str(": ");
        text.push_str(&if actions.is_empty() {
            "(nothing yet)".to_owned()
        } else {
            actions.join(", then ")
        });
        text
    }

    /// Where a trigger lives, in words.
    pub fn trigger_source(&self, names: &Names, t: &TriggerEntry) -> String {
        if let Some(s) = t.while_skill {
            format!(
                "while you have {} (its level is lvl)",
                self.skill_name(names, s)
            )
        } else if let Some(s) = t.while_stat {
            format!(
                "while you have {} above 0 (its value is lvl)",
                self.stat_label(names, s)
            )
        } else if let Some(s) = t.while_state {
            format!(
                "while you are in {} (its stack count is lvl)",
                self.state_name(names, s)
            )
        } else {
            "no source chosen".into()
        }
    }

    /// One line on what a part does.
    pub fn part_summary(&self, names: &Names, p: &PartEntry) -> String {
        let f = |f: &Fm| f.0.trim().to_owned();
        let dmg = |d: &DamageEntry| {
            format!(
                "{} to {} {} damage",
                f(&d.min),
                f(&d.max),
                d.element.as_deref().unwrap_or("physical")
            )
        };
        match p {
            PartEntry::CallScript {
                module, function, ..
            } => format!("calls {module}::{function}"),
            PartEntry::Missile {
                missile,
                lob,
                at_aim,
                ..
            } => format!(
                "{} {}{}",
                if *at_aim { "drops" } else { "fires" },
                Names::find(&names.missiles, *missile)
                    .map_or_else(|| format!("missile {missile}"), str::to_owned),
                match (*at_aim, *lob) {
                    (true, _) => " on the aim point",
                    (false, true) => " (lobbed)",
                    (false, false) => "",
                }
            ),
            PartEntry::Nest { overlay } => match overlay {
                Some(o) => format!("hatches the nest it prepared, with overlay {o}"),
                None => "hatches the nest it prepared".to_owned(),
            },
            PartEntry::Resurrect { overlay } => match overlay {
                Some(o) => format!("raises the dead monster it targets, with overlay {o}"),
                None => "raises the dead monster it targets".to_owned(),
            },
            PartEntry::Wall { missile, centre } => format!(
                "lays a wall of {}{}",
                Names::find(&names.missiles, *missile)
                    .map_or_else(|| format!("missile {missile}"), str::to_owned),
                centre.map_or(String::new(), |c| format!(
                    " with {} at its middle",
                    Names::find(&names.missiles, c)
                        .map_or_else(|| format!("missile {c}"), str::to_owned)
                ))
            ),
            PartEntry::Heal { min, max } => {
                format!("heals the target by {} to {} % of its life", f(min), f(max))
            }
            PartEntry::Warcry { missile, .. } => format!("warcry ring of missile {}", missile),
            PartEntry::TimedBuff { state, frames, .. } => format!(
                "gives {} to the caster for {} frames",
                self.state_name(names, *state),
                f(frames)
            ),
            PartEntry::Aura {
                state,
                range,
                seconds,
                affects,
                ..
            } => format!(
                "gives {} to {} within {} for {} s",
                self.state_name(names, *state),
                affects_text(affects.as_deref().unwrap_or("allies")),
                f(range),
                f(seconds)
            ),
            PartEntry::Curse {
                state,
                range,
                seconds,
                affects,
                ..
            } => format!(
                "curses {} with {} within {} of the aim point for {} s",
                affects_text(affects.as_deref().unwrap_or("enemies")),
                self.state_name(names, *state),
                f(range),
                f(seconds)
            ),
            PartEntry::DruidSummon {
                class,
                pet_max,
                level,
                ..
            } => format!(
                "summons class {} at level {} (cap {})",
                class,
                f(level),
                f(pet_max)
            ),
            PartEntry::Sentry { class, pet_max, .. } => {
                format!("sentry class {} (cap {})", class, f(pet_max))
            }
            PartEntry::Vine { class, .. } => format!("vine class {}", class),
            PartEntry::Form { state, .. } => format!("form state {}", state),
            PartEntry::DamageAura { state, .. } => format!("damage aura state {}", state),
            PartEntry::Passive { state, .. } => format!("passive state {}", state),
            PartEntry::Golem { class, .. } => format!("golem class {}", class),
            PartEntry::Raven { class, .. } => format!("raven class {}", class),
            PartEntry::CorpseFind { potion, .. } => {
                if *potion {
                    "find potion".into()
                } else {
                    "find item".into()
                }
            }
            PartEntry::MartialCharge { counter, .. } => format!("martial charge stat {}", counter),
            PartEntry::Shadow { class, .. } => format!("shadow class {}", class),
            PartEntry::AiCurse { state, attract, .. } => {
                format!("AI curse state {} attract {}", state, attract)
            }
            PartEntry::ProjectileFan { missile, .. } => format!("projectile fan {}", missile),
            PartEntry::GuidedProjectile { missile, .. } => format!("guided projectile {}", missile),
            PartEntry::Sight { state, .. } => format!("sight state {}", state),
            PartEntry::Summon { class, count, .. } => format!(
                "summons {} of {}",
                count.as_ref().map_or("1".into(), f),
                Names::find(&names.monsters, *class)
                    .map_or_else(|| format!("monster {class}"), str::to_owned)
            ),
            PartEntry::State {
                state, op, target, ..
            } => {
                let verb = match op.as_deref().unwrap_or("apply") {
                    "add_stack" => "adds stacks of",
                    "remove_stack" => "removes stacks of",
                    "remove" => "ends",
                    _ => "applies",
                };
                let on = match target.as_deref().unwrap_or("caster") {
                    "target" => "the target",
                    "area" => "units around the aim point",
                    _ => "you",
                };
                format!("{verb} {} on {on}", self.state_name(names, *state))
            }
            PartEntry::MeleeHit {
                weapon_percent,
                damage,
                ..
            } => match damage {
                Some(d) => format!(
                    "melee hit at {weapon_percent}% weapon damage plus {}",
                    dmg(d)
                ),
                None => format!("melee hit at {weapon_percent}% weapon damage"),
            },
            PartEntry::AreaHit {
                radius,
                around,
                damage,
                ..
            } => format!(
                "{} within {} of {}",
                dmg(damage),
                f(radius),
                if around.as_deref() == Some("caster") {
                    "you"
                } else {
                    "the aim point"
                }
            ),
            PartEntry::MissileRing {
                missile,
                count,
                around,
                ..
            } => format!(
                "{} x {} in a ring around {}",
                count.as_ref().map_or("16".into(), f),
                self.missile_name(names, *missile),
                if around.as_deref() == Some("target") {
                    "the aim point"
                } else {
                    "you"
                }
            ),
            PartEntry::MissileArc {
                missile,
                count,
                span,
                ..
            } => format!(
                "{} x {} in an arc of {}/64 of a circle",
                count.as_ref().map_or("5".into(), f),
                self.missile_name(names, *missile),
                span.as_ref().map_or("16".into(), f)
            ),
            PartEntry::MissileVolley { missile, count } => format!(
                "{} x {} at the aim point",
                count.as_ref().map_or("3".into(), f),
                self.missile_name(names, *missile)
            ),
            PartEntry::Chain {
                radius,
                jumps,
                damage,
                ..
            } => format!(
                "{}, then up to {} jumps of {} (each unit once)",
                dmg(damage),
                f(jumps),
                f(radius)
            ),
            PartEntry::Bounce {
                radius,
                jumps,
                damage,
                ..
            } => format!(
                "{}, then up to {} bounces of {}",
                dmg(damage),
                f(jumps),
                f(radius)
            ),
            PartEntry::Teleport { max_range } => match max_range {
                Some(r) => format!("moves you to the aim point, at most {} away", f(r)),
                None => "moves you to the aim point".into(),
            },
            PartEntry::Raise { pet_type, .. } => {
                format!("raises the target corpse as a pet of type {pet_type}")
            }
            PartEntry::MinionOrder {
                order,
                radius,
                skill,
                ..
            } => match (order.as_str(), skill) {
                ("attack", Some(s)) => format!("minions within {} cast skill {s}", f(radius)),
                ("follow", _) => format!("minions within {} follow you", f(radius)),
                _ => format!("minions within {} come to you", f(radius)),
            },
            PartEntry::EngineBody { slot, .. } => {
                format!("runs the engine's own skill function {slot}")
            }
            PartEntry::Repeat {
                skill,
                every,
                times,
                now,
                ..
            } => format!(
                "runs {} {} times, one every {} ticks{}",
                match skill {
                    Some(s) => format!("skill {s}'s parts"),
                    None => "the parts after it".to_owned(),
                },
                times.as_ref().map_or("5".into(), f),
                every.as_ref().map_or("25".into(), f),
                if *now { "" } else { ", the first after a wait" }
            ),
            PartEntry::Rush {
                speed, each, land, ..
            } => format!(
                "runs at the target at {}% of its running speed{}{}",
                speed.as_ref().map_or("100".into(), f),
                match each {
                    Some(e) => format!(", skill {e}'s parts after each step"),
                    None => String::new(),
                },
                match land {
                    Some(l) => format!(", then skill {l}'s parts"),
                    None => ", then the parts after it".to_owned(),
                }
            ),
            PartEntry::Leap { frames, land, .. } => format!(
                "leaps to the aim point over {} ticks{}",
                frames.as_ref().map_or("10".into(), f),
                match land {
                    Some(l) => format!(", then runs skill {l}'s parts"),
                    None => String::new(),
                }
            ),
        }
    }

    // ------------------------------------------------------------ ids

    /// Calls `f` on every state, skill and stat id the entries hold.
    pub fn visit_ids(&mut self, f: &mut dyn FnMut(IdKind, &mut u16)) {
        use IdKind::{Skill, Stat, State};
        for s in &mut self.states {
            f(State, &mut s.id);
            for e in &mut s.effects {
                f(Stat, &mut e.stat);
            }
        }
        for s in &mut self.skills {
            f(Skill, &mut s.id);
            for p in &mut s.parts {
                if let PartEntry::MeleeHit {
                    target_state,
                    target_stats,
                    ..
                } = p
                {
                    if let Some(state) = target_state {
                        f(State, state);
                    }
                    for st in target_stats {
                        f(Stat, &mut st.stat);
                    }
                }
                match p {
                    PartEntry::Aura {
                        state,
                        target_state,
                        stats,
                        ..
                    } => {
                        f(State, state);
                        if let Some(t) = target_state {
                            f(State, t);
                        }
                        for st in stats {
                            f(Stat, &mut st.stat);
                        }
                    }
                    PartEntry::Warcry { state, stats, .. } => {
                        if let Some(state) = state {
                            f(State, state);
                        }
                        for stat in stats {
                            f(Stat, &mut stat.stat);
                        }
                    }
                    PartEntry::Vine { state, .. } => f(State, state),
                    PartEntry::Form { state, stats, .. } => {
                        f(State, state);
                        for stat in stats {
                            f(Stat, &mut stat.stat);
                        }
                    }
                    PartEntry::Passive {
                        state,
                        blocked_by,
                        stats,
                        ..
                    } => {
                        f(State, state);
                        if let Some(s) = blocked_by {
                            f(State, s);
                        }
                        for stat in stats {
                            f(Stat, &mut stat.stat);
                        }
                    }
                    PartEntry::MartialCharge {
                        state,
                        counter,
                        bonus,
                        ..
                    } => {
                        f(State, state);
                        f(Stat, counter);
                        if let Some(s) = bonus {
                            f(Stat, &mut s.stat);
                        }
                    }
                    PartEntry::Shadow { state, stats, .. } => {
                        if let Some(s) = state {
                            f(State, s);
                        }
                        for stat in stats {
                            f(Stat, &mut stat.stat);
                        }
                    }
                    PartEntry::AiCurse { state, stats, .. } => {
                        f(State, state);
                        for stat in stats {
                            f(Stat, &mut stat.stat);
                        }
                    }
                    PartEntry::Sight { state, stat, .. } => {
                        f(State, state);
                        f(Stat, stat);
                    }
                    PartEntry::DamageAura {
                        state,
                        target_state,
                        own_stats,
                        stats,
                        ..
                    } => {
                        f(State, state);
                        if let Some(s) = target_state {
                            f(State, s);
                        }
                        for stat in own_stats.iter_mut().chain(stats.iter_mut()) {
                            f(Stat, &mut stat.stat);
                        }
                    }
                    PartEntry::Curse { state, stats, .. }
                    | PartEntry::TimedBuff { state, stats, .. } => {
                        f(State, state);
                        for st in stats {
                            f(Stat, &mut st.stat);
                        }
                    }
                    PartEntry::State { state, .. } => f(State, state),
                    PartEntry::MeleeHit {
                        damage: Some(d), ..
                    }
                    | PartEntry::AreaHit { damage: d, .. }
                    | PartEntry::Chain { damage: d, .. }
                    | PartEntry::Bounce { damage: d, .. } => {
                        if let Some(b) = &mut d.bonus_stat {
                            f(Stat, b);
                        }
                        if let Some(c) = &mut d.charge {
                            f(State, &mut c.state);
                        }
                    }
                    PartEntry::MinionOrder { skill: Some(s), .. }
                    | PartEntry::EngineBody { skill: Some(s), .. }
                    | PartEntry::Leap { land: Some(s), .. } => f(Skill, s),
                    PartEntry::Rush { each, land, .. } => {
                        for s in [each, land].into_iter().flatten() {
                            f(Skill, s);
                        }
                    }
                    PartEntry::Repeat {
                        skill, while_state, ..
                    } => {
                        if let Some(s) = skill {
                            f(Skill, s);
                        }
                        if let Some(s) = while_state {
                            f(State, s);
                        }
                    }
                    _ => {}
                }
            }
        }
        for t in &mut self.triggers {
            for (kind, id) in [
                (Skill, &mut t.while_skill),
                (Stat, &mut t.while_stat),
                (State, &mut t.while_state),
                (State, &mut t.state),
                (Skill, &mut t.by_skill),
            ] {
                if let Some(id) = id {
                    f(kind, id);
                }
            }
            for s in &mut t.skills {
                f(Skill, s);
            }
            for c in &mut t.conditions {
                match c {
                    ConditionEntry::HasState { state, .. }
                    | ConditionEntry::LacksState { state, .. }
                    | ConditionEntry::StacksAtLeast { state, .. } => f(State, state),
                    _ => {}
                }
            }
            for a in &mut t.actions {
                match a {
                    ActionEntry::ApplyState { state, .. }
                    | ActionEntry::AddStack { state, .. }
                    | ActionEntry::RemoveStack { state, .. }
                    | ActionEntry::RemoveState { state, .. } => f(State, state),
                    ActionEntry::CastSkill { skill, .. } => f(Skill, skill),
                    ActionEntry::CallScript { .. } => {}
                }
            }
        }
    }

    /// Applies `map` to every id the files hold.
    pub fn remap(&mut self, map: &IdMap) {
        self.visit_ids(&mut |kind, id| *id = map.get(kind, *id));
    }

    /// Every id the entries use, defined or only referenced.
    pub fn ids(&self) -> IdSets {
        let mut sets = IdSets::default();
        self.clone().visit_ids(&mut |kind, id| {
            match kind {
                IdKind::State => sets.states.insert(*id),
                IdKind::Skill => sets.skills.insert(*id),
                IdKind::Stat => sets.stats.insert(*id),
            };
        });
        sets
    }

    /// Adds a template's entries, with its ids changed by `map`. Refuses when an
    /// id it defines is already defined here.
    pub fn add_template(&mut self, template: &Template, map: &IdMap) -> Result<()> {
        let mut add = Kit::load_texts(template.states, template.skills, template.triggers)?;
        add.remap(map);
        for s in &add.states {
            if self.state(s.id).is_some() {
                bail!(
                    "State {} is already defined in states.toml; choose another id",
                    s.id
                );
            }
        }
        for s in &add.skills {
            if self.skills.iter().any(|o| o.id == s.id) {
                bail!(
                    "Skill {} is already defined in skills.toml; choose another id",
                    s.id
                );
            }
        }
        self.states.extend(add.states);
        self.skills.extend(add.skills);
        self.triggers.extend(add.triggers);
        self.dirty = true;
        Ok(())
    }

    fn load_texts(states: &str, skills: &str, triggers: &str) -> Result<Kit> {
        Ok(Kit {
            states: toml::from_str::<StatesFile>(states)?.states,
            skills: toml::from_str::<SkillsFile>(skills)?.skills,
            triggers: toml::from_str::<TriggersFile>(triggers)?.triggers,
            ..Kit::default()
        })
    }
}

fn be(subject: &str) -> &'static str {
    if subject == "you" {
        "are"
    } else {
        "is"
    }
}
fn have(subject: &str) -> &'static str {
    if subject == "you" {
        "have"
    } else {
        "has"
    }
}
fn poss(subject: &str) -> String {
    if subject == "you" {
        "your".into()
    } else {
        format!("{subject}'s")
    }
}
fn affects_text(a: &str) -> &'static str {
    match a {
        "enemies" => "enemies",
        "everyone" => "everyone",
        "allied_players" => "allied players",
        _ => "allies",
    }
}

/// "+25% damage", "-10 mana": the stat's row name made readable, with its value.
pub fn effect_text(stat: &str, value: i32) -> String {
    let words = stat.replace('_', " ");
    let words = words
        .strip_prefix("item ")
        .unwrap_or(&words)
        .trim()
        .to_owned();
    let (words, percent) = ["percent", "pct"]
        .iter()
        .find_map(|s| words.strip_suffix(s).map(|w| (w.trim().to_owned(), true)))
        .unwrap_or((words, false));
    let sign = if value >= 0 { "+" } else { "-" };
    format!(
        "{sign}{}{} {words}",
        value.unsigned_abs(),
        if percent { "%" } else { "" }
    )
    .trim_end()
    .to_owned()
}

// -------------------------------------------------------------- templates

/// A set of the three files that works together.
pub struct Template {
    pub name: &'static str,
    pub about: &'static str,
    pub states: &'static str,
    pub skills: &'static str,
    pub triggers: &'static str,
    /// Table rows the files expect, for the "create rows" step.
    pub rows: &'static [TemplateRow],
}
pub struct TemplateRow {
    pub table: &'static str,
    /// What the template calls this row, as an id in its files.
    pub id: u16,
    pub kind: IdKind,
    pub key: &'static str,
    /// Sets `srvdofunc` to the block slot.
    pub block: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdKind {
    State,
    Skill,
    Stat,
}

pub const TEMPLATES: &[Template] = &[Template {
    name: "Proc → window → charges",
    about: "8% of melee attacks open a one-second reaction window; using a skill in it gives a charge \
            (up to 5) and closes it; a missed window costs a charge; with 3 charges skill 390 hits 25% harder.",
    states: include_str!("../../../templates/proc-charges/files/states.toml"),
    skills: include_str!("../../../templates/proc-charges/files/skills.toml"),
    triggers: include_str!("../../../templates/proc-charges/files/triggers.toml"),
    rows: &[
        TemplateRow { table: "states", id: 190, kind: IdKind::State, key: "reaction_window", block: false },
        TemplateRow { table: "states", id: 191, kind: IdKind::State, key: "charges", block: false },
        TemplateRow { table: "itemstatcost", id: 340, kind: IdKind::Stat, key: "charge_damage_percent", block: false },
        TemplateRow { table: "skills", id: 390, kind: IdKind::Skill, key: "Ember Lash", block: true },
        TemplateRow { table: "skills", id: 391, kind: IdKind::Skill, key: "Frost Nova", block: true },
        TemplateRow { table: "skills", id: 392, kind: IdKind::Skill, key: "Quick Strike", block: true },
        TemplateRow { table: "skills", id: 393, kind: IdKind::Skill, key: "Opportunist", block: false },
    ],
}];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdSets {
    pub states: BTreeSet<u16>,
    pub skills: BTreeSet<u16>,
    pub stats: BTreeSet<u16>,
}

/// New ids for old ones; an id not listed stays as it is.
#[derive(Clone, Debug, Default)]
pub struct IdMap {
    pub states: Vec<(u16, u16)>,
    pub skills: Vec<(u16, u16)>,
    pub stats: Vec<(u16, u16)>,
}
impl IdMap {
    pub fn get(&self, kind: IdKind, id: u16) -> u16 {
        let list = match kind {
            IdKind::State => &self.states,
            IdKind::Skill => &self.skills,
            IdKind::Stat => &self.stats,
        };
        list.iter().find(|(o, _)| *o == id).map_or(id, |(_, n)| *n)
    }
}

fn projectile_fan_zero() -> Fm {
    Fm::new("0")
}
