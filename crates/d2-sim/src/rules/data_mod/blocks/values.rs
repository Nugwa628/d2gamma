//! The value family: the numbers a formula can read. Formulas are the
//! engine's own skill formula language (the `skillcalc` names); the compiler
//! is what accepts or refuses a formula, this table is the part of the
//! language tools list for a modder.

use super::{Block, Family};

const fn value(name: &'static str, about: &'static str) -> Block {
    Block {
        family: Family::Value,
        name,
        label: name,
        since: 1,
        deprecated: None,
        about,
        slot: "",
        fields: &[],
    }
}

pub(super) const BLOCKS: &[Block] = &[
    value(
        "lvl",
        "The skill's level (the stat's value or the state's stack count in a trigger).",
    ),
    value("par1", "The skill row's Param1 column."),
    value("par2", "The skill row's Param2 column."),
    value("par3", "The skill row's Param3 column."),
    value("par4", "The skill row's Param4 column."),
    value("par5", "The skill row's Param5 column."),
    value("par6", "The skill row's Param6 column."),
    value("par7", "The skill row's Param7 column."),
    value("par8", "The skill row's Param8 column."),
    value(
        "ln12",
        "Param1 plus Param2 per level above the first: a straight line.",
    ),
    value("ln34", "Param3 plus Param4 per level above the first."),
    value("ln56", "Param5 plus Param6 per level above the first."),
    value("ln78", "Param7 plus Param8 per level above the first."),
    value(
        "dm12",
        "Param1 and Param2 as a diminishing-returns curve over the level.",
    ),
    value("dm34", "Param3 and Param4 as a diminishing-returns curve."),
    value("dm56", "Param5 and Param6 as a diminishing-returns curve."),
    value("dm78", "Param7 and Param8 as a diminishing-returns curve."),
    value("ulvl", "The casting unit's character level."),
    value(
        "blvl",
        "The casting unit's hard points in this skill, ignoring bonuses.",
    ),
    value(
        "edmn",
        "The skill's elemental damage minimum, in whole points.",
    ),
    value(
        "edmx",
        "The skill's elemental damage maximum, in whole points.",
    ),
    value("edln", "The skill's elemental duration, in frames."),
    value("toht", "The skill's attack rating bonus."),
    value("mana", "The skill's mana cost, in whole points."),
];
