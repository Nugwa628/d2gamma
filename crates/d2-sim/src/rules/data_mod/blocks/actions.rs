//! The action family: what a trigger does (`kind = "..."` in a
//! `[[trigger.action]]`).

use super::{Block, Family, Field, FieldKind};

const WHO: Field = Field {
    key: "who",
    kind: FieldKind::Choice(&["self", "other"]),
    required: false,
    default: "self",
    about: "Whom it works on: the unit holding the trigger or the other unit of the event.",
};

const STATE: Field = Field {
    key: "state",
    kind: FieldKind::State,
    required: true,
    default: "",
    about: "The state.",
};

const SECONDS: Field = Field {
    key: "seconds",
    kind: FieldKind::Formula,
    required: false,
    default: "the state's duration_seconds",
    about: "How long it lasts, in seconds.",
};

const COUNT: Field = Field {
    key: "count",
    kind: FieldKind::Formula,
    required: false,
    default: "1",
    about: "How many stacks.",
};

const fn block(
    name: &'static str,
    label: &'static str,
    about: &'static str,
    fields: &'static [Field],
) -> Block {
    Block {
        family: Family::Action,
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
        "call_script",
        "Call script / graph",
        "Calls a function of a Luau module or a visual graph call entry for the unit holding the trigger. See scripts.md.",
        &[
            Field {
                key: "module",
                kind: FieldKind::ScriptModule,
                required: true,
                default: "",
                about: "The script module, a path under files/scripts ending in .luau or .graph.toml.",
            },
            Field {
                key: "function",
                kind: FieldKind::ScriptFunction,
                required: true,
                default: "",
                about: "The exported Luau function or graph call node name.",
            },
            Field {
                key: "argument",
                kind: FieldKind::Formula,
                required: false,
                default: "0",
                about: "The number passed to the function.",
            },
        ],
    ),
    block(
        "apply_state",
        "Apply a state",
        "Puts the state on; its reapply rule says what a second application does.",
        &[STATE, SECONDS, WHO],
    ),
    block(
        "add_stack",
        "Add stacks",
        "Adds stacks (the state comes on first when it is off).",
        &[STATE, COUNT, SECONDS, WHO],
    ),
    block(
        "remove_stack",
        "Remove stacks",
        "Takes stacks off; the state ends with the last.",
        &[STATE, COUNT, WHO],
    ),
    block("remove_state", "End a state", "Ends the state.", &[STATE, WHO]),
    block(
        "cast_skill",
        "Cast a skill",
        "Casts a skills row as the unit, at the other unit or at itself. Needs no mana and no ownership.",
        &[
            Field {
                key: "skill",
                kind: FieldKind::Skill,
                required: true,
                default: "",
                about: "The skills row.",
            },
            Field {
                key: "level",
                kind: FieldKind::Formula,
                required: false,
                default: "lvl",
                about: "The skill level cast.",
            },
            Field {
                key: "at",
                kind: FieldKind::Choice(&["other", "self"]),
                required: false,
                default: "other",
                about: "Whom the skill is cast at.",
            },
        ],
    ),
    block(
        "crowd_control",
        "Crowd control",
        "Hits with a hit that does no damage and causes the effect (interrupt, knockback, stun, freeze).",
        &[
            Field {
                key: "effect",
                kind: FieldKind::Effect,
                required: true,
                default: "",
                about: "The declared effect: kind, seconds (stun and freeze), chance.",
            },
            Field {
                key: "who",
                kind: FieldKind::Choice(&["other", "self"]),
                required: false,
                default: "other",
                about: "Whom it works on.",
            },
        ],
    ),
];
