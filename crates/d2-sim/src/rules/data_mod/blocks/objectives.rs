//! The objective family: what a quest asks of the player (`kind = "..."` in a
//! `[[quest.objective]]`).

use super::{Block, Family, Field, FieldKind};

const COUNT: Field = Field {
    key: "count",
    kind: FieldKind::Whole,
    required: false,
    default: "1",
    about: "How many times (1 to 10000).",
};

const fn block(
    name: &'static str,
    label: &'static str,
    about: &'static str,
    fields: &'static [Field],
) -> Block {
    Block {
        family: Family::Objective,
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
        "kill",
        "Kill monsters",
        "Kill monsters of one class. A kill by the player's summon counts.",
        &[
            Field {
                key: "monster",
                kind: FieldKind::Monster,
                required: true,
                default: "",
                about: "The monster class (a monstats row).",
            },
            COUNT,
        ],
    ),
    block(
        "enter",
        "Enter a level",
        "Walk, travel or be carried into a level.",
        &[Field {
            key: "level",
            kind: FieldKind::Whole,
            required: true,
            default: "",
            about: "The level (a levels row).",
        }],
    ),
    block(
        "talk",
        "Talk to an NPC",
        "Open the chat of an NPC of one class.",
        &[Field {
            key: "npc",
            kind: FieldKind::Monster,
            required: true,
            default: "",
            about: "The NPC class (a monstats row).",
        }],
    ),
    block(
        "use_skill",
        "Cast a skill",
        "Cast one skill, which may be a charge or an item's skill.",
        &[
            Field {
                key: "skill",
                kind: FieldKind::Skill,
                required: true,
                default: "",
                about: "The skills row.",
            },
            COUNT,
        ],
    ),
    block(
        "collect",
        "Collect items",
        "Carry items of one code (inventory, belt or cursor; the stash does not count).          It counts what the player holds, so dropping the items takes the progress back. Finishing the quest takes the items only when `consume` is set.",
        &[
            Field {
                key: "item",
                kind: FieldKind::ItemCode,
                required: true,
                default: "",
                about: "The item code (a misc, weapons or armor row's code), up to four characters.",
            },
            COUNT,
            Field {
                key: "consume",
                kind: FieldKind::Flag,
                required: false,
                default: "false",
                about: "Take the items from the player when the quest is completed.",
            },
        ],
    ),
    block(
        "reach_stat",
        "Reach a stat",
        "Have at least a value of a stat: gold, or a stat of the mod's own that its items or skills raise. \
         This is how to ask for collected things. Once met it stays met.",
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
                about: "The value to reach (1 or more).",
            },
        ],
    ),
];
