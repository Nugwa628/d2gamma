//! The words of `ai.toml`: every condition and action kind with the keys it
//! takes, for tools that build a graph from lists instead of text.
//!
//! The tables here describe what the reader in this module accepts. Tests
//! check them against it: every kind the reader knows is described, every
//! described kind loads with its stated defaults, and the stated bounds are the
//! ones the reader enforces.

use super::{ActionKind, Condition, MAX_DISTANCE, MAX_STEPS, MAX_SUMMON, SKILL_SLOTS};

/// What a key holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FieldKind {
    /// A whole number between `min` and `max`.
    Int { min: i32, max: i32, default: i32 },
    /// A time in seconds, whole or decimal.
    Seconds { default: f64 },
    /// `true` or `false`.
    Bool { default: bool },
    /// One of the listed words, each with a label.
    Choice {
        options: &'static [(&'static str, &'static str)],
        default: &'static str,
    },
    /// A state id (the mod's `states.toml` or a stock state).
    State,
    /// A monster class id (a `monstats` row).
    Class,
    /// The name of a cooldown the graph declares.
    Cooldown,
    /// A free name (a wind-up event).
    Name,
}

/// One key of a condition or an action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    /// The kind cannot be used without it.
    pub required: bool,
}

/// A kind of condition or action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KindInfo {
    /// The `kind = "..."` word.
    pub name: &'static str,
    pub label: &'static str,
    pub summary: &'static str,
    pub fields: &'static [Field],
}

impl KindInfo {
    /// Whether the kind is an instant action (it happens and the walk through
    /// the actions goes on). Always `false` for a condition.
    pub fn is_instant(&self) -> bool {
        matches!(self.name, "call_allies" | "summon" | "set_state")
    }
}

impl Field {
    /// The value a new use of the key starts with.
    pub fn default_value(&self) -> toml::Value {
        match self.kind {
            FieldKind::Int { default, .. } => toml::Value::Integer(i64::from(default)),
            FieldKind::Seconds { default } => {
                if default.fract() == 0.0 {
                    toml::Value::Integer(default as i64)
                } else {
                    toml::Value::Float(default)
                }
            }
            FieldKind::Bool { default } => toml::Value::Boolean(default),
            FieldKind::Choice { default, .. } => toml::Value::String(default.to_owned()),
            FieldKind::State => toml::Value::Integer(1),
            FieldKind::Class => toml::Value::Integer(0),
            FieldKind::Cooldown | FieldKind::Name => toml::Value::String(String::new()),
        }
    }
}

const WHO: FieldKind = FieldKind::Choice {
    options: &[
        ("self", "the monster"),
        ("target", "its target"),
        ("second", "its second target"),
        ("corpse", "a fallen undead"),
        ("hurt", "a hurt undead"),
        ("owner", "its owner"),
        ("owner_target", "its owner's target"),
    ],
    default: "self",
};
const AIM: Field = field(
    "at",
    "On",
    FieldKind::Choice {
        options: &[
            ("target", "its target"),
            ("second", "its second target"),
            ("corpse", "a fallen undead"),
            ("hurt", "a hurt undead"),
            ("owner", "its owner"),
            ("owner_target", "its owner's target"),
        ],
        default: "target",
    },
    false,
);
const SCAN_RADIUS: Field = field("radius", "Search radius", int(1, MAX_DISTANCE, 24), false);

const fn int(min: i32, max: i32, default: i32) -> FieldKind {
    FieldKind::Int { min, max, default }
}

const fn field(key: &'static str, label: &'static str, kind: FieldKind, required: bool) -> Field {
    Field {
        key,
        label,
        kind,
        required,
    }
}

const fn kind(
    name: &'static str,
    label: &'static str,
    summary: &'static str,
    fields: &'static [Field],
) -> KindInfo {
    KindInfo {
        name,
        label,
        summary,
        fields,
    }
}

const STATE_AND_WHO: &[Field] = &[
    field("state", "State", FieldKind::State, true),
    field("who", "On", WHO, false),
];
const STACKS: &[Field] = &[
    field("state", "State", FieldKind::State, true),
    field("count", "Stacks", int(1, 65535, 2), true),
    field("who", "On", WHO, false),
];
const RADIUS: &[Field] = &[field("radius", "Radius", int(1, MAX_DISTANCE, 24), true)];
const CLASS_COUNT: &[Field] = &[
    field("class", "Monster class", FieldKind::Class, true),
    field("also", "Or this class", FieldKind::Class, false),
    field("count", "Count", int(1, 255, 5), true),
];
const RADIUS_AND_COUNT: &[Field] = &[
    field("radius", "Radius", int(1, MAX_DISTANCE, 10), true),
    field("at_least", "At least", int(1, 255, 1), false),
];

/// Every condition kind.
pub const CONDITIONS: &[KindInfo] = &[
    kind(
        "has_target",
        "Has a target",
        "The monster has a target.",
        &[],
    ),
    kind(
        "no_target",
        "Has no target",
        "The monster has no target.",
        &[],
    ),
    kind(
        "distance_below",
        "Target is closer than",
        "The target is closer than this many subtiles.",
        &[field("value", "Subtiles", int(0, MAX_DISTANCE, 3), true)],
    ),
    kind(
        "distance_above",
        "Target is at least",
        "The target is this many subtiles or farther.",
        &[field("value", "Subtiles", int(0, MAX_DISTANCE, 10), true)],
    ),
    kind(
        "in_melee",
        "Target in melee reach",
        "The target is within the monster's melee reach.",
        &[],
    ),
    kind(
        "out_of_melee",
        "Target out of melee reach",
        "The target is not within the monster's melee reach (or there is none).",
        &[],
    ),
    kind(
        "has_owner",
        "Has an owner",
        "The monster is somebody's minion (a pet).",
        &[],
    ),
    kind(
        "no_owner",
        "Has no owner",
        "The monster is nobody's minion.",
        &[],
    ),
    kind(
        "owner_distance_below",
        "Owner is closer than",
        "The owner is closer than this many subtiles.",
        &[field("value", "Subtiles", int(0, MAX_DISTANCE, 8), true)],
    ),
    kind(
        "owner_distance_above",
        "Owner is at least",
        "The owner is this many subtiles or farther.",
        &[field("value", "Subtiles", int(0, MAX_DISTANCE, 12), true)],
    ),
    kind(
        "owner_target",
        "Owner has an enemy",
        "The owner has an enemy in sight: the one its own target search finds.",
        &[],
    ),
    kind(
        "second_target",
        "Has a second target",
        "The monster sees an enemy besides its target: the nearest one.",
        &[],
    ),
    kind(
        "second_distance_below",
        "Second target is closer than",
        "The second target is closer than this many subtiles.",
        &[field("value", "Subtiles", int(0, MAX_DISTANCE, 3), true)],
    ),
    kind(
        "second_distance_above",
        "Second target is at least",
        "The second target is this many subtiles or farther.",
        &[field("value", "Subtiles", int(0, MAX_DISTANCE, 10), true)],
    ),
    kind(
        "undead_within",
        "Undead nearby",
        "Undead of the monster's side (not itself) are within the radius.",
        RADIUS_AND_COUNT,
    ),
    kind(
        "corpse_within",
        "Fallen undead nearby",
        "An undead of the monster's side lies fallen, and can be raised, within the radius.",
        RADIUS,
    ),
    kind(
        "hurt_within",
        "Hurt undead nearby",
        "An undead of the monster's side is hurt within the radius.",
        RADIUS,
    ),
    kind(
        "class_count_below",
        "Fewer of a class than",
        "Fewer living monsters of the class (and its family) than this are in the rooms near the monster.",
        CLASS_COUNT,
    ),
    kind(
        "class_count_at_least",
        "At least this many of a class",
        "At least this many living monsters of the class (and its family) are in the rooms near the monster.",
        CLASS_COUNT,
    ),
    kind(
        "thinks_at_least",
        "Thought this often",
        "The monster has thought this many times since it entered the state.",
        &[field("count", "Thinks", int(1, 65535, 10), true)],
    ),
    kind(
        "line_of_sight",
        "Can see the target",
        "Nothing blocks the way to the target.",
        &[],
    ),
    kind(
        "no_line_of_sight",
        "Cannot see the target",
        "Something blocks the way to the target.",
        &[],
    ),
    kind(
        "life_below",
        "Life below",
        "Life is below this percent of its maximum.",
        &[field("percent", "Percent", int(0, 100, 50), true)],
    ),
    kind(
        "life_above",
        "Life at or above",
        "Life is at or above this percent of its maximum.",
        &[field("percent", "Percent", int(0, 100, 50), true)],
    ),
    kind(
        "has_state",
        "Has state",
        "The monster (or its target) is in the state.",
        STATE_AND_WHO,
    ),
    kind(
        "lacks_state",
        "Lacks state",
        "The monster (or its target) is not in the state.",
        STATE_AND_WHO,
    ),
    kind(
        "stacks_at_least",
        "Has stacks of state",
        "The state is on with at least this many stacks.",
        STACKS,
    ),
    kind(
        "allies_within",
        "Allies nearby",
        "Living pack mates are within the radius.",
        RADIUS_AND_COUNT,
    ),
    kind(
        "enemies_within",
        "Enemies nearby",
        "Players (or their allies) are within the radius.",
        RADIUS_AND_COUNT,
    ),
    kind(
        "cooldown_ready",
        "Cooldown ready",
        "The cooldown has run out or was never started.",
        &[field("name", "Cooldown", FieldKind::Cooldown, true)],
    ),
    kind(
        "alive_for",
        "Alive for",
        "The monster has had this graph for this long.",
        &[field(
            "seconds",
            "Seconds",
            FieldKind::Seconds { default: 5.0 },
            true,
        )],
    ),
    kind(
        "in_state_for",
        "In this state for",
        "The monster has been in this state for this long.",
        &[field(
            "seconds",
            "Seconds",
            FieldKind::Seconds { default: 2.0 },
            true,
        )],
    ),
    kind(
        "random",
        "Chance",
        "A roll on the monster's own seed comes up under this percent.",
        &[field("percent", "Percent", int(0, 100, 25), true)],
    ),
];

const RUN: Field = field("run", "Run", FieldKind::Bool { default: true }, false);
const fn steps(default: i32) -> Field {
    field("steps", "Subtiles", int(1, MAX_STEPS, default), false)
}

/// Every action kind.
pub const ACTIONS: &[KindInfo] = &[
    kind(
        "move_to",
        "Move to target",
        "Walk (or run) toward the target, all the way or stopping short of it.",
        &[
            RUN,
            field("steps", "Stop short (steps)", int(1, MAX_STEPS, 9), false),
            field("speed", "Speed (percent)", int(-100, 100, 25), false),
            AIM,
            SCAN_RADIUS,
        ],
    ),
    kind(
        "hop_to",
        "Hop toward target",
        "Walk one hop toward the target, keeping a distance from it, then think again.",
        &[
            field("steps", "Hop (subtiles)", int(1, MAX_STEPS, 12), false),
            field("keep", "Keep (subtiles)", int(0, MAX_DISTANCE, 0), false),
            field("speed", "Speed (percent)", int(-100, 100, 25), false),
            AIM,
            SCAN_RADIUS,
        ],
    ),
    kind(
        "follow_owner",
        "Follow the owner",
        "Stay with the owner as the built-in pets do: keep up, make room, catch up.",
        &[
            field("steps", "Stray (subtiles)", int(1, MAX_STEPS, 7), false),
            field(
                "at",
                "Enemy",
                FieldKind::Choice {
                    options: &[
                        ("target", "its target"),
                        ("second", "its second target"),
                        ("owner_target", "its owner's target"),
                    ],
                    default: "target",
                },
                false,
            ),
            field("quiet", "Quiet", FieldKind::Bool { default: false }, false),
        ],
    ),
    kind(
        "move_away",
        "Move away",
        "Walk or run straight away from the target.",
        &[RUN, steps(8)],
    ),
    kind(
        "flee",
        "Flee",
        "Run away from the target, far.",
        &[steps(16)],
    ),
    kind(
        "move_around",
        "Circle the target",
        "Circle the target a few subtiles at a time.",
        &[steps(4)],
    ),
    kind(
        "attack",
        "Melee attack",
        "A melee swing: attack 1 or 2.",
        &[
            field("attack", "Attack", int(1, 2, 1), false),
            AIM,
            SCAN_RADIUS,
        ],
    ),
    kind(
        "use_skill",
        "Use a skill",
        "Use the skill in one of the monster's skill columns.",
        &[
            field("slot", "Skill column", int(1, SKILL_SLOTS as i32, 1), true),
            field(
                "at",
                "On",
                FieldKind::Choice {
                    options: &[
                        ("target", "its target"),
                        ("second", "its second target"),
                        ("corpse", "a fallen undead"),
                        ("hurt", "a hurt undead"),
                        ("owner", "its owner"),
                        ("owner_target", "its owner's target"),
                        ("self", "the monster"),
                    ],
                    default: "target",
                },
                false,
            ),
            SCAN_RADIUS,
        ],
    ),
    kind(
        "wander",
        "Wander",
        "Walk to a random spot nearby.",
        &[field("radius", "Radius", int(1, MAX_DISTANCE, 6), false)],
    ),
    kind(
        "idle",
        "Stand still",
        "Stand still for a while.",
        &[field(
            "seconds",
            "Seconds",
            FieldKind::Seconds { default: 1.0 },
            false,
        )],
    ),
    kind(
        "wind_up",
        "Wind up",
        "Stand still, report the event, and hold back the actions after it.",
        &[
            field(
                "seconds",
                "Seconds",
                FieldKind::Seconds { default: 1.0 },
                true,
            ),
            field("event", "Event name", FieldKind::Name, false),
        ],
    ),
    kind(
        "call_allies",
        "Call allies",
        "Wake the monster's pack mates so they join in. Instant.",
        &[field("radius", "Radius", int(1, MAX_DISTANCE, 10), false)],
    ),
    kind(
        "summon",
        "Summon",
        "Create minions near the monster. Instant.",
        &[
            field("class", "Monster class", FieldKind::Class, true),
            field("count", "How many", int(1, MAX_SUMMON as i32, 1), false),
            field("spread", "Spread", int(0, MAX_STEPS, 3), false),
        ],
    ),
    kind(
        "set_state",
        "Set a state",
        "Turn a state on or off on the monster. Instant.",
        &[
            field("state", "State", FieldKind::State, true),
            field("on", "On", FieldKind::Bool { default: true }, false),
        ],
    ),
];

/// The `target` words of a graph.
pub const TARGETS: &[(&str, &str)] = &[
    ("enemy", "Waits until it sees an enemy"),
    ("any", "Thinks even with no target"),
    ("none", "Never has a target"),
];

/// The condition kind with this name.
pub fn condition_kind(name: &str) -> Option<&'static KindInfo> {
    CONDITIONS.iter().find(|k| k.name == name)
}

/// The action kind with this name.
pub fn action_kind(name: &str) -> Option<&'static KindInfo> {
    ACTIONS.iter().find(|k| k.name == name)
}

impl Condition {
    /// The `kind = "..."` word of this condition.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Condition::HasTarget => "has_target",
            Condition::NoTarget => "no_target",
            Condition::DistanceBelow { .. } => "distance_below",
            Condition::DistanceAbove { .. } => "distance_above",
            Condition::InMelee => "in_melee",
            Condition::OutOfMelee => "out_of_melee",
            Condition::HasOwner => "has_owner",
            Condition::NoOwner => "no_owner",
            Condition::OwnerDistanceBelow { .. } => "owner_distance_below",
            Condition::OwnerDistanceAbove { .. } => "owner_distance_above",
            Condition::OwnerTarget => "owner_target",
            Condition::SecondTarget => "second_target",
            Condition::SecondDistanceBelow { .. } => "second_distance_below",
            Condition::SecondDistanceAbove { .. } => "second_distance_above",
            Condition::UndeadWithin { .. } => "undead_within",
            Condition::CorpseWithin { .. } => "corpse_within",
            Condition::HurtWithin { .. } => "hurt_within",
            Condition::ClassCountBelow { .. } => "class_count_below",
            Condition::ClassCountAtLeast { .. } => "class_count_at_least",
            Condition::ThinksAtLeast { .. } => "thinks_at_least",
            Condition::LineOfSight => "line_of_sight",
            Condition::NoLineOfSight => "no_line_of_sight",
            Condition::LifeBelow { .. } => "life_below",
            Condition::LifeAbove { .. } => "life_above",
            Condition::HasState { .. } => "has_state",
            Condition::LacksState { .. } => "lacks_state",
            Condition::StacksAtLeast { .. } => "stacks_at_least",
            Condition::AlliesWithin { .. } => "allies_within",
            Condition::EnemiesWithin { .. } => "enemies_within",
            Condition::CooldownReady { .. } => "cooldown_ready",
            Condition::AliveFor { .. } => "alive_for",
            Condition::InStateFor { .. } => "in_state_for",
            Condition::Random { .. } => "random",
        }
    }
}

impl ActionKind {
    /// The `kind = "..."` word of this action.
    pub fn kind_name(self) -> &'static str {
        match self {
            ActionKind::MoveTo => "move_to",
            ActionKind::HopTo => "hop_to",
            ActionKind::FollowOwner => "follow_owner",
            ActionKind::MoveAway => "move_away",
            ActionKind::Flee => "flee",
            ActionKind::MoveAround => "move_around",
            ActionKind::Attack => "attack",
            ActionKind::UseSkill => "use_skill",
            ActionKind::Wander => "wander",
            ActionKind::Idle => "idle",
            ActionKind::WindUp => "wind_up",
            ActionKind::CallAllies => "call_allies",
            ActionKind::Summon => "summon",
            ActionKind::SetState => "set_state",
        }
    }
}

#[cfg(test)]
mod tests;
