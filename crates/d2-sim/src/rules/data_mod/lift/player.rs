//! Player-class copy packs select rows from the author's own install. No
//! install cells are shipped: the existing SK2 lifter writes editable parts
//! at copy time and reports the rows that still need another vocabulary part.

/// A player-class identity in the source tables. Studio enables copy packs
/// independently of this catalog, so a report can also cover missing packs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerClass {
    pub id: u8,
    pub code: &'static str,
    pub name: &'static str,
}

pub const CLASSES: &[PlayerClass] = &[
    PlayerClass {
        id: 0,
        code: "ama",
        name: "Amazon",
    },
    PlayerClass {
        id: 1,
        code: "sor",
        name: "Sorceress",
    },
    PlayerClass {
        id: 2,
        code: "nec",
        name: "Necromancer",
    },
    PlayerClass {
        id: 3,
        code: "pal",
        name: "Paladin",
    },
    PlayerClass {
        id: 4,
        code: "bar",
        name: "Barbarian",
    },
    PlayerClass {
        id: 5,
        code: "dru",
        name: "Druid",
    },
    PlayerClass {
        id: 6,
        code: "ass",
        name: "Assassin",
    },
];

/// What an unsupported cast needs. These are descriptions, not new block
/// names; missing parts never silently become an engine-body wrapper.
pub fn needs_part(slot: u16) -> &'static str {
    match slot {
        0 => "passive/stat or start-only behaviour",
        2 | 7 | 9 | 11 | 13 | 14 | 32 | 34 | 35 | 42 | 46 | 50 | 64 | 70 | 120 | 121 | 122
        | 150 => "weapon/melee variant",
        3 | 5 | 74 => "weapon throw",
        6 | 20 | 21 | 33 | 51 | 55 | 63 => "target/area damage or debuff",
        8 | 10 | 12 | 17 | 26 | 36..=41 | 43 | 73 | 80 | 117 | 118 => "missile pattern/sequence",
        15 | 16 | 31 | 44 | 45 | 49 | 56..=58 | 60 | 62 | 114 | 115 | 119 | 144 => {
            "summon/spawn or corpse control"
        }
        18 | 25 | 47 | 54 | 116 | 145..=147 => "timed buff/form or attached state",
        19 | 48 | 95 | 152 => "channel/start-state",
        23 | 24 | 123 | 124 => "persistent ground effect",
        27 | 52 | 67 | 77 | 78 => "teleport/leap/charge movement",
        28 => "missile at aim (Meteor)",
        29 | 66 | 81 | 82 => "aura/pulse with damage or corpse effects",
        30 | 59 | 61 => "curse/AI control",
        68 => "shout missile ring and state",
        71 => "taunt/AI control",
        76 => "whirlwind movement/weapon sequence",
        69 | 72 | 75 => "corpse item/ward",
        79 => "conversion",
        _ => "special cast behaviour",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_packs_have_unique_ids_and_codes() {
        for (i, class) in CLASSES.iter().enumerate() {
            assert!(class.id < 7 && !class.name.is_empty());
            assert!(CLASSES[..i]
                .iter()
                .all(|c| c.id != class.id && c.code != class.code));
        }
        assert_eq!(CLASSES[0].code, "ama");
        assert_eq!(needs_part(7), "weapon/melee variant");
        assert_eq!(needs_part(24), "persistent ground effect");
    }
}
