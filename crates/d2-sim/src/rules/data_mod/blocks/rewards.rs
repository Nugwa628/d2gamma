//! The reward family: what a finished quest gives (`kind = "..."` in a
//! `[[quest.reward]]`).

use super::{Block, Family, Field, FieldKind};

const fn block(
    name: &'static str,
    label: &'static str,
    about: &'static str,
    fields: &'static [Field],
) -> Block {
    Block {
        family: Family::Reward,
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
        "stat",
        "Add to a stat",
        "Adds to the player's base value of a stat: gold, stat points, skill points or a stat of the mod's own.",
        &[
            Field {
                key: "stat",
                kind: FieldKind::Stat,
                required: true,
                default: "",
                about: "The stat (an itemstatcost row).",
            },
            Field {
                key: "value",
                kind: FieldKind::Whole,
                required: true,
                default: "",
                about: "What to add (it may be negative, not 0).",
            },
        ],
    ),
    block(
        "item",
        "Give an item",
        "Puts fresh items in the player's inventory when the quest is done. With no room they wait for room,          and the player is told once.",
        &[
            Field {
                key: "item",
                kind: FieldKind::ItemCode,
                required: true,
                default: "",
                about: "The item code (a misc, weapons or armor row's code), up to four characters.",
            },
            Field {
                key: "count",
                kind: FieldKind::Whole,
                required: false,
                default: "1",
                about: "How many items (1 to 20).",
            },
        ],
    ),
    block(
        "message",
        "Tell the player",
        "Puts a line of text in the player's chat log.",
        &[Field {
            key: "text",
            kind: FieldKind::Text,
            required: true,
            default: "",
            about: "The line (up to 240 characters).",
        }],
    ),
];
