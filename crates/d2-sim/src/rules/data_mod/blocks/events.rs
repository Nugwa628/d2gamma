//! The event family: what a trigger reacts to (`on = "..."`).

use super::{Block, Family, Field, FieldKind};

const fn block(
    name: &'static str,
    label: &'static str,
    about: &'static str,
    fields: &'static [Field],
) -> Block {
    Block {
        family: Family::Event,
        name,
        label,
        since: 1,
        deprecated: None,
        about,
        slot: "",
        fields,
    }
}

pub(super) const BLOCKS: &[Block] = &[
    block(
        "attack",
        "Attack",
        "The unit swings at something in melee (hit or miss).",
        &[Field {
            key: "by_skill",
            kind: FieldKind::Skill,
            required: false,
            default: "any skill",
            about: "Only while the unit is using this skill (a hit during the cast; not a missile's later impact).",
        }],
    ),
    block(
        "hit",
        "Deal damage",
        "The unit deals damage (melee or missile).",
        &[Field {
            key: "by_skill",
            kind: FieldKind::Skill,
            required: false,
            default: "any skill",
            about: "Only while the unit is using this skill (a hit during the cast; not a missile's later impact).",
        }],
    ),
    block(
        "being_hit",
        "Take damage",
        "The unit takes damage (melee or missile).",
        &[],
    ),
    block(
        "skill_used",
        "Use a skill",
        "The unit used a skill that did something. A skill a trigger casts does not count.",
        &[Field {
            key: "skills",
            kind: FieldKind::SkillList,
            required: false,
            default: "any skill",
            about: "Only these skills set the trigger off.",
        }],
    ),
    block(
        "kill",
        "Kill",
        "The unit kills something.",
        &[Field {
            key: "by_skill",
            kind: FieldKind::Skill,
            required: false,
            default: "any skill",
            about: "Only while the unit is using this skill (a hit during the cast; not a missile's later impact).",
        }],
    ),
    block(
        "state_start",
        "State starts",
        "A state with rules in states.toml turns on for the unit. Another stack is not another start.",
        &[Field {
            key: "state",
            kind: FieldKind::State,
            required: true,
            default: "",
            about: "The state that starts.",
        }],
    ),
    block(
        "state_end",
        "State ends",
        "A state with rules in states.toml runs out or is cleared by the game. A state the mod removes does not end.",
        &[Field {
            key: "state",
            kind: FieldKind::State,
            required: true,
            default: "",
            about: "The state that ends.",
        }],
    ),
];
