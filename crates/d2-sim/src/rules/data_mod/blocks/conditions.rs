//! The condition family: tests a trigger needs to hold (`kind = "..."` in a
//! `[[trigger.condition]]`).

use super::{Block, Family, Field, FieldKind};

const WHO: Field = Field {
    key: "who",
    kind: FieldKind::Choice(&["self", "other"]),
    required: false,
    default: "self",
    about: "Whose side to look at: the unit holding the trigger or the other unit of the event.",
};

const STATE: Field = Field {
    key: "state",
    kind: FieldKind::State,
    required: true,
    default: "",
    about: "The state.",
};

const PERCENT: Field = Field {
    key: "percent",
    kind: FieldKind::Whole,
    required: true,
    default: "",
    about: "0 to 100.",
};

const fn block(
    name: &'static str,
    label: &'static str,
    about: &'static str,
    fields: &'static [Field],
) -> Block {
    Block {
        family: Family::Condition,
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
        "has_state",
        "Is in state",
        "The unit is in the state.",
        &[STATE, WHO],
    ),
    block(
        "lacks_state",
        "Is not in state",
        "The unit is not in the state.",
        &[STATE, WHO],
    ),
    block(
        "stacks_at_least",
        "Has stacks",
        "The state is on with at least this many stacks.",
        &[
            STATE,
            Field {
                key: "count",
                kind: FieldKind::Whole,
                required: true,
                default: "",
                about: "Stacks needed.",
            },
            WHO,
        ],
    ),
    block(
        "life_below",
        "Life below",
        "Life is below a percent of its maximum.",
        &[PERCENT, WHO],
    ),
    block(
        "life_above",
        "Life at or above",
        "Life is at or above a percent of its maximum.",
        &[PERCENT, WHO],
    ),
];
