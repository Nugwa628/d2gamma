//! The target family: whom or where a block works on. A target name is
//! written under a **slot** (the key of the block that carries it), and is
//! unique within its slot.

use super::{Block, Family};

const fn target(
    slot: &'static str,
    name: &'static str,
    label: &'static str,
    about: &'static str,
) -> Block {
    Block {
        family: Family::Target,
        name,
        label,
        since: 1,
        deprecated: None,
        about,
        slot,
        fields: &[],
    }
}

pub(super) const BLOCKS: &[Block] = &[
    // `affects`: the unit filter of an area, aura or state part.
    target(
        "affects",
        "enemies",
        "Enemies",
        "Hostile players and monsters that can be attacked, not in town.",
    ),
    target(
        "affects",
        "allies",
        "Allies",
        "The caster's allies, the caster included.",
    ),
    target(
        "affects",
        "everyone",
        "Everyone",
        "Every player and monster that can be attacked, not in town.",
    ),
    target(
        "affects",
        "allied_players",
        "Allied players",
        "The caster's allies that are players, the caster included; pets and mercenaries are left out.",
    ),
    // `who`: a side of a trigger event.
    target("who", "self", "The holder", "The unit holding the trigger."),
    target(
        "who",
        "other",
        "The other unit",
        "The other unit of the event: the one hit, the attacker or the victim.",
    ),
    // `around`: the centre of an area part.
    target(
        "around",
        "target",
        "The aim point",
        "The point the skill is aimed at.",
    ),
    target(
        "around",
        "caster",
        "The caster",
        "The unit using the skill.",
    ),
    // `on`: whom a state part works on.
    target("on", "caster", "The caster", "The unit using the skill."),
    target(
        "on",
        "target",
        "The target",
        "The unit the skill is aimed at.",
    ),
    target(
        "on",
        "area",
        "Units around the aim point",
        "Every unit within the part's radius of the aim point that passes `affects`.",
    ),
];
