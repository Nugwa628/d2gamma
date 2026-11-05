//! The lifter on made-up rows (no game table is in the repository).

use std::collections::BTreeMap;

use super::*;
use crate::rules::data_mod::{AttackMode, Part, SkillRules};

#[derive(Default)]
pub(super) struct Rows {
    skill: BTreeMap<&'static str, &'static str>,
    missile: BTreeMap<&'static str, &'static str>,
}

impl Rows {
    pub(super) fn new(cells: &[(&'static str, &'static str)]) -> Rows {
        Rows {
            skill: cells.iter().copied().collect(),
            // Missile 7 "orb": velocity 10, +8 a level.
            missile: [("vel", "10"), ("vellev", "8")].into_iter().collect(),
        }
    }
}

fn index(names: &[(&str, u16)], name: &str) -> Option<u16> {
    names
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, i)| *i)
        .or_else(|| name.parse().ok())
}

impl Source for Rows {
    fn skill(&self, column: &str) -> String {
        self.skill.get(column).copied().unwrap_or("").to_owned()
    }
    fn state(&self, name: &str) -> Option<u16> {
        index(&[("might", 30)], name)
    }
    fn stat(&self, name: &str) -> Option<u16> {
        index(&[("strength", 0), ("damagepercent", 25)], name)
    }
    fn missile(&self, name: &str) -> Option<u16> {
        index(&[("orb", 7), ("bolt", 3)], name)
    }
    fn missile_cell(&self, row: u16, column: &str) -> String {
        assert_eq!(row, 7, "only the orb has cells");
        self.missile.get(column).copied().unwrap_or("").to_owned()
    }
}

fn parts(l: &Lifted) -> Vec<Part> {
    SkillRules::from_toml(&l.toml)
        .expect("a lifted entry reads")
        .get(l.id.into())
        .expect("its id")
        .parts
        .clone()
}

#[test]
fn the_common_attack_becomes_a_melee_hit() {
    let l = lift(5, &Rows::new(&[("skill", "Swing"), ("srvdofunc", "1")])).unwrap();
    assert_eq!(l.family, Family::Attack);
    assert_eq!(l.cells, [("srvdofunc", "400".to_owned())]);
    assert!(matches!(parts(&l)[..], [Part::MeleeHit(_)]));
    assert!(l.toml.contains("name = \"Swing\""));
    assert!(!l.notes.is_empty());
}

#[test]
fn a_plain_missile_skill_is_one_missile_part_and_stops_the_core_firing_it_too() {
    let rows = Rows::new(&[("skill", "Bolt"), ("srvmissile", "bolt"), ("lob", "1")]);
    let l = lift(6, &rows).unwrap();
    assert_eq!(l.family, Family::Missile);
    // The core fires `srvmissile` after any do function: the cell goes.
    assert_eq!(
        l.cells,
        [
            ("srvdofunc", "400".to_owned()),
            ("srvmissile", String::new())
        ]
    );
    let [Part::Missile(m)] = &parts(&l)[..] else {
        panic!("one missile part");
    };
    assert_eq!((m.missile, m.lob), (3, true));
}

#[test]
fn a_jab_is_a_melee_hit_with_enhanced_damage_and_the_rows_element() {
    let rows = Rows::new(&[
        ("skill", "Jab"),
        ("srvdofunc", "7"),
        ("calc1", "20 + lvl"),
        ("srcdam", "64"),
        ("tohit", "30"),
        ("levtohit", "4"),
    ]);
    let l = lift(12, &rows).unwrap();
    assert_eq!(l.family, Family::Jab);
    let [Part::MeleeHit(m)] = &parts(&l)[..] else {
        panic!("one melee hit");
    };
    assert_eq!(m.weapon_percent, 50);
    assert!(m.own_element);
    assert_eq!(m.monster_mode, Some(AttackMode::S1));
    assert!(l.toml.contains("enhance = \"(20 + lvl)\""));
    assert!(l.toml.contains("attack_rating = \"30 + (lvl - 1) * 4\""));
    // A to-hit formula wins over the numbers; no srcdam is the whole weapon.
    let calc = Rows::new(&[("srvdofunc", "7"), ("tohitcalc", "lvl * 5")]);
    let l = lift(12, &calc).unwrap();
    assert!(l.toml.contains("attack_rating = \"(lvl * 5)\""));
    assert!(l.toml.contains("weapon_percent = 100"));
    // An elemental conversion is said to be left out.
    let conv = Rows::new(&[("srvdofunc", "7"), ("etype", "fire")]);
    assert!(lift(12, &conv)
        .unwrap()
        .notes
        .iter()
        .any(|n| n.contains("converts")));
}

#[test]
fn a_smite_is_a_melee_hit_that_stuns_for_calc2_frames() {
    let rows = Rows::new(&[
        ("skill", "Smite"),
        ("srvdofunc", "150"),
        ("calc2", "10 + lvl"),
    ]);
    let l = lift(13, &rows).unwrap();
    assert_eq!(l.family, Family::Smite);
    let [Part::MeleeHit(m)] = &parts(&l)[..] else {
        panic!("one melee hit");
    };
    assert_eq!(
        (m.weapon_percent, m.monster_mode),
        (100, Some(AttackMode::A2))
    );
    assert!(l.toml.contains("stun_frames = \"(10 + lvl)\""));
    assert!(l.notes.iter().any(|n| n.contains("shield")));
}

#[test]
fn a_charge_is_a_rush_that_ends_in_a_melee_hit() {
    let rows = Rows::new(&[
        ("skill", "Charge"),
        ("srvdofunc", "67"),
        ("srvstfunc", "31"),
        ("param1", "40"),
        ("calc1", "50 + lvl"),
    ]);
    let l = lift(14, &rows).unwrap();
    assert_eq!(l.family, Family::Charge);
    let [Part::Rush(r), Part::MeleeHit(m)] = &parts(&l)[..] else {
        panic!("a rush and a melee hit");
    };
    // Its melee hit runs where the rush ends: the rush has no `land` skill.
    assert!(r.land.is_none() && r.each.is_none() && r.stop_in_reach);
    assert!(l.toml.contains("speed = 90"));
    assert_eq!(m.monster_mode, Some(AttackMode::A1));
    assert!(l.toml.contains("enhance = \"(50 + lvl)\""));
    // The engine's start function that sets the unit running is dropped.
    assert!(l.cells.contains(&("srvstfunc", "0".to_owned())));
    assert!(l.notes.iter().any(|n| n.contains("rush")));
}

#[test]
fn a_power_strike_is_a_melee_hit_with_an_overlay_and_a_target_state() {
    let rows = Rows::new(&[
        ("skill", "Power Strike"),
        ("srvdofunc", "2"),
        ("srvstfunc", "6"),
        ("srvoverlay", "9"),
        ("auratargetstate", "might"),
        ("auralencalc", "100 + lvl"),
        ("aurastat1", "16"),
        ("aurastatcalc1", "lvl * 2"),
        ("calc1", "50 + lvl"),
        ("srcdam", "64"),
    ]);
    let l = lift(14, &rows).unwrap();
    assert_eq!(l.family, Family::MeleeState);
    let [Part::MeleeHit(m)] = &parts(&l)[..] else {
        panic!("one melee hit");
    };
    assert_eq!(m.weapon_percent, 50);
    assert_eq!((m.overlay, m.target_state), (Some(9), Some(30)));
    assert_eq!((m.target_stats.len(), m.target_stats[0].stat), (1, 16));
    assert!(m.monster_mode.is_none() && m.damage.is_none());
    // The start function that builds the hit is dropped: the part does.
    assert!(l.cells.contains(&("srvstfunc", "0".to_owned())));
    assert!(l.toml.contains("enhance = \"(50 + lvl)\""));
    assert!(l.toml.contains("target_frames = \"(100 + lvl)\""));
}

#[test]
fn a_hunger_is_a_melee_hit_with_calc2_and_calc3_as_its_leech() {
    let rows = Rows::new(&[
        ("skill", "Hunger"),
        ("srvdofunc", "122"),
        ("calc1", "30 + lvl"),
        ("calc2", "lvl"),
        ("calc3", "2"),
        ("srcdam", "128"),
    ]);
    let l = lift(14, &rows).unwrap();
    assert_eq!(l.family, Family::Hunger);
    let [Part::MeleeHit(m)] = &parts(&l)[..] else {
        panic!("one melee hit");
    };
    assert_eq!(m.weapon_percent, 100);
    assert!(l.toml.contains("life_leech = ") && l.toml.contains("mana_leech = "));
    assert!(l.notes.iter().any(|n| n.contains("miss")));
}

#[test]
fn a_zeal_is_a_melee_hit_in_a_row_with_calc2_as_its_enhanced_damage() {
    let rows = Rows::new(&[
        ("skill", "Zeal"),
        ("srvdofunc", "13"),
        ("srvstfunc", "37"),
        ("calc1", "lvl / 3 + 2"),
        ("calc2", "20 + lvl * 5"),
        ("srcdam", "128"),
    ]);
    let l = lift(14, &rows).unwrap();
    assert_eq!(l.family, Family::Sequence);
    let [Part::MeleeHit(m)] = &parts(&l)[..] else {
        panic!("one melee hit");
    };
    assert_eq!(m.weapon_percent, 100);
    let enhance = l.toml.lines().find(|l| l.starts_with("enhance")).unwrap();
    assert!(
        enhance.contains("20") && enhance.contains('5'),
        "{}",
        l.toml
    );
    assert!(l.toml.contains("sequence = "), "{}", l.toml);
    assert!(l.cells.contains(&("srvstfunc", "0".to_owned())));
    assert!(l.notes.iter().any(|n| n.contains("animation")));
    // Another start function is left out with the reason.
    let other = Rows::new(&[("srvdofunc", "13"), ("srvstfunc", "5")]);
    assert!(matches!(lift(14, &other), Err(LiftError::Needs(_))));
}

#[test]
fn a_bash_carries_its_hit_flags_and_a_flat_damage_row_is_left_out() {
    let rows = Rows::new(&[
        ("skill", "Bash"),
        ("srvdofunc", "2"),
        ("srvstfunc", "32"),
        ("calc3", "20"),
        ("resultflags", "0x4"),
        ("hitflags", "0x100"),
        ("hitclass", "5"),
    ]);
    let l = lift(14, &rows).unwrap();
    let [Part::MeleeHit(m)] = &parts(&l)[..] else {
        panic!("one melee hit");
    };
    assert!(m.damage.is_none());
    assert_eq!((m.result_flags, m.hit_flags, m.hit_class), (4, 256, 5));
    assert!(m.target_state.is_none() && m.overlay.is_none());
    assert!(l.notes.iter().any(|n| n.contains("attack-rate")));
    // `calc2` adds flat damage after the enhanced damage: not said yet.
    let flat = Rows::new(&[("srvdofunc", "2"), ("srvstfunc", "32"), ("calc2", "lvl")]);
    assert!(matches!(lift(14, &flat), Err(LiftError::Needs(_))));
}

#[test]
fn a_melee_state_row_the_parts_cannot_say_is_left_out_with_the_reason() {
    // Another start function; a state on the caster that lasts to its next mode change.
    for (start, state) in [("35", ""), ("6", "might")] {
        let rows = Rows::new(&[
            ("srvdofunc", "2"),
            ("srvstfunc", start),
            ("aurastate", state),
        ]);
        assert!(matches!(lift(14, &rows), Err(LiftError::Needs(_))));
    }
}

#[test]
fn a_fire_wall_is_a_wall_of_the_two_missile_rows() {
    let rows = Rows::new(&[
        ("skill", "Fire Wall"),
        ("srvdofunc", "24"),
        ("srvmissilea", "orb"),
        ("srvmissileb", "bolt"),
    ]);
    let l = lift(14, &rows).unwrap();
    assert_eq!(l.family, Family::Wall);
    let [Part::Wall(w)] = &parts(&l)[..] else {
        panic!("one wall");
    };
    assert_eq!((w.missile, w.centre), (7, Some(3)));
    // No second row: no middle missile.
    let one = Rows::new(&[("srvdofunc", "24"), ("srvmissilea", "orb")]);
    let l = lift(14, &one).unwrap();
    assert!(matches!(&parts(&l)[..], [Part::Wall(w)] if w.centre.is_none()));
    assert!(matches!(
        lift(
            14,
            &Rows::new(&[("srvdofunc", "24"), ("srvmissilea", "nope")])
        ),
        Err(LiftError::Unknown {
            column: "srvmissilea",
            ..
        })
    ));
}

#[test]
fn a_mon_inferno_repeats_its_flame_for_the_length_of_the_channel() {
    let rows = Rows::new(&[
        ("skill", "Inferno"),
        ("srvdofunc", "95"),
        ("srvstfunc", "53"),
        ("srvmissilea", "bolt"),
        ("calc1", "7"),
        ("calc2", "50"),
        ("calc3", "10"),
    ]);
    let l = lift(30, &rows).unwrap();
    assert_eq!(l.family, Family::Inferno);
    let [Part::Repeat(r), Part::Missile(m)] = &parts(&l)[..] else {
        panic!("a repeat and the flame it repeats");
    };
    // No `skill`: the part after the repeat is what it repeats.
    assert_eq!(
        (r.skill, r.now, m.missile, m.channel),
        (None, true, 3, true)
    );
    assert!(l.toml.contains("every = \"(10)\""));
    assert!(l.toml.contains("times = \"((50) + (10) - 1) / (10) + 1\""));
    assert!(l.notes.iter().any(|n| n.contains("channel animation")));
    // The Imp's start function keeps the length in `calc1`.
    let imp = Rows::new(&[
        ("srvdofunc", "95"),
        ("srvstfunc", "59"),
        ("srvmissilea", "bolt"),
        ("calc1", "7"),
        ("calc2", "50"),
        ("calc3", "10"),
    ]);
    assert!(lift(30, &imp)
        .unwrap()
        .toml
        .contains("times = \"((7) + (10) - 1) / (10) + 1\""));
    let nameless = Rows::new(&[("srvdofunc", "95"), ("srvmissilea", "frost")]);
    assert!(matches!(
        lift(30, &nameless),
        Err(LiftError::Unknown {
            column: "srvmissilea",
            ..
        })
    ));
}

#[test]
fn a_nest_carries_its_overlay_row_and_keeps_the_engines_start() {
    let rows = Rows::new(&[
        ("skill", "Nest"),
        ("srvdofunc", "91"),
        ("srvstfunc", "49"),
        ("sumoverlay", "7"),
    ]);
    let l = lift(16, &rows).unwrap();
    assert_eq!(l.family, Family::Nest);
    assert_eq!(l.cells, [("srvdofunc", "400".to_owned())]);
    assert!(matches!(&parts(&l)[..], [Part::Nest(n)] if n.overlay == Some(7)));
    assert!(l.notes.iter().any(|n| n.contains("start function")));
    let bare = Rows::new(&[("srvdofunc", "91")]);
    assert!(
        matches!(&parts(&lift(16, &bare).unwrap())[..], [Part::Nest(n)] if n.overlay.is_none())
    );
    assert!(matches!(
        lift(
            16,
            &Rows::new(&[("srvdofunc", "91"), ("sumoverlay", "egg")])
        ),
        Err(LiftError::Unknown {
            column: "sumoverlay",
            ..
        })
    ));
}

#[test]
fn a_resurrect_carries_its_overlay_row() {
    let rows = Rows::new(&[
        ("skill", "Resurrect"),
        ("srvdofunc", "97"),
        ("srvoverlay", "12"),
    ]);
    let l = lift(15, &rows).unwrap();
    assert_eq!(l.family, Family::Resurrect);
    assert!(matches!(&parts(&l)[..], [Part::Resurrect(r)] if r.overlay == Some(12)));
    let bare = Rows::new(&[("srvdofunc", "97")]);
    assert!(
        matches!(&parts(&lift(15, &bare).unwrap())[..], [Part::Resurrect(r)] if r.overlay.is_none())
    );
    assert!(matches!(
        lift(
            15,
            &Rows::new(&[("srvdofunc", "97"), ("srvoverlay", "flames")])
        ),
        Err(LiftError::Unknown {
            column: "srvoverlay",
            ..
        })
    ));
}

#[test]
fn a_meteor_drops_its_missile_on_the_aim_point() {
    let rows = Rows::new(&[
        ("skill", "Meteor"),
        ("srvdofunc", "28"),
        ("srvmissilea", "orb"),
    ]);
    let l = lift(10, &rows).unwrap();
    assert_eq!(l.family, Family::Meteor);
    assert_eq!(l.family.name(), "meteor");
    assert_eq!(l.cells, [("srvdofunc", "400".to_owned())]);
    let [Part::Missile(m)] = &parts(&l)[..] else {
        panic!("one missile part");
    };
    assert_eq!((m.missile, m.at_aim, m.lob), (7, true, false));
    // A `srvmissile` beside it would be fired by the core too: it goes.
    let both = Rows::new(&[
        ("srvdofunc", "28"),
        ("srvmissilea", "orb"),
        ("srvmissile", "bolt"),
    ]);
    let l = lift(10, &both).unwrap();
    assert!(l.cells.contains(&("srvmissile", String::new())));
    assert!(l.notes.iter().any(|n| n.contains("srvmissile")));
    assert!(matches!(
        lift(
            10,
            &Rows::new(&[("srvdofunc", "28"), ("srvmissilea", "nope")])
        ),
        Err(LiftError::Unknown {
            column: "srvmissilea",
            ..
        })
    ));
}

#[test]
fn a_zakarum_heal_carries_its_two_percent_formulas() {
    let rows = Rows::new(&[
        ("skill", "Bestow"),
        ("srvdofunc", "96"),
        ("calc1", "10 + lvl"),
        ("calc2", "20 + lvl * 2"),
    ]);
    let l = lift(11, &rows).unwrap();
    assert_eq!(l.family, Family::Heal);
    assert!(matches!(parts(&l)[..], [Part::Heal(_)]));
    assert!(l.toml.contains("min = \"(10 + lvl)\""));
    assert!(l.toml.contains("max = \"(20 + lvl * 2)\""));
    assert_eq!(l.cells, [("srvdofunc", "400".to_owned())]);
}

#[test]
fn a_nova_is_a_ring_of_64_with_the_engine_velocity() {
    let rows = Rows::new(&[
        ("skill", "Nova"),
        ("srvdofunc", "22"),
        ("srvmissilea", "orb"),
        ("calc1", "lvl * 2"),
    ]);
    let l = lift(7, &rows).unwrap();
    assert_eq!(l.family, Family::Nova);
    assert!(l.toml.contains("count = 64"));
    assert!(l
        .toml
        .contains("velocity = \"10 + 8 * lvl / 8 + (lvl * 2)\""));
    let [Part::MissileRing(r)] = &parts(&l)[..] else {
        panic!("one ring");
    };
    assert_eq!(r.missile, 7);
}

#[test]
fn a_nova_without_a_velocity_calc_adds_nothing() {
    let rows = Rows::new(&[("srvdofunc", "22"), ("srvmissilea", "orb")]);
    let l = lift(7, &rows).unwrap();
    assert!(l.toml.contains("velocity = \"10 + 8 * lvl / 8 + 0\""));
    // No name in the row: the family name is the label.
    assert!(l.toml.contains("name = \"nova\""));
}

#[test]
fn an_aura_carries_its_state_range_time_and_stats() {
    let rows = Rows::new(&[
        ("skill", "Might"),
        ("srvdofunc", "65"),
        ("aurastate", "might"),
        ("auratargetstate", "might"),
        ("aurafilter", "0x10083"),
        ("aurarangecalc", "10 + lvl"),
        ("auralencalc", "250"),
        ("aurastat1", "damagepercent"),
        ("aurastatcalc1", "lvl * 5"),
        ("aurastat2", "strength"),
        ("aurastatcalc2", "3"),
    ]);
    let l = lift(8, &rows).unwrap();
    assert_eq!(l.family, Family::Aura);
    let [Part::Aura(a)] = &parts(&l)[..] else {
        panic!("one aura");
    };
    assert_eq!(
        (a.state, a.affects, a.stats.len()),
        (30, Affects::Allies, 2)
    );
    // The same state for the caster and the units around: no second state.
    assert_eq!(a.target_state, None);
    assert!(l.toml.contains("range = \"(10 + lvl)\""));
    assert_eq!((a.stats[0].stat, a.stats[1].stat), (25, 0));
    assert!(l.toml.contains("seconds = \"(250) / 25\""));
    // The three filters round-trip; another mask is noted.
    for affects in [
        Affects::Enemies,
        Affects::Everyone,
        Affects::Allies,
        Affects::AlliedPlayers,
    ] {
        let bits: &'static str = Box::leak(format!("{}", affects.filter()).into_boxed_str());
        let rows = Rows::new(&[
            ("srvdofunc", "65"),
            ("aurastate", "might"),
            ("auratargetstate", "might"),
            ("aurafilter", bits),
        ]);
        let l = lift(8, &rows).unwrap();
        let [Part::Aura(a)] = &parts(&l)[..] else {
            panic!("one aura");
        };
        assert_eq!(a.affects, affects, "{bits}");
        assert!(l.notes.iter().all(|n| !n.contains("aurafilter")));
    }
    // Another mask gets the nearest filter by its hostile, allied and unit
    // bits, with a note: players only and allied is `allied_players`, players
    // and monsters and allied is `allies`.
    for (mask, near) in [
        ("0x12001", Affects::AlliedPlayers),
        ("0x12003", Affects::Allies),
        ("0x10103", Affects::Allies),
        ("0x8203", Affects::Enemies),
        ("0x203", Affects::Everyone),
        ("0x5", Affects::Everyone),
    ] {
        let odd = Rows::new(&[
            ("srvdofunc", "65"),
            ("aurastate", "might"),
            ("auratargetstate", "might"),
            ("aurafilter", mask),
        ]);
        let l = lift(8, &odd).unwrap();
        let [Part::Aura(a)] = &parts(&l)[..] else {
            panic!("one aura");
        };
        assert_eq!(a.affects, near, "{mask}");
        assert!(l.notes.iter().any(|n| n.contains("aurafilter")), "{mask}");
    }
}

#[test]
fn an_aura_gives_the_units_around_the_state_the_row_names_for_them() {
    // A different state for the units around.
    let rows = Rows::new(&[
        ("srvdofunc", "65"),
        ("aurastate", "might"),
        ("auratargetstate", "7"),
        ("aurafilter", "0x10003"),
        ("aurarangecalc", "12"),
    ]);
    let l = lift(8, &rows).unwrap();
    let [Part::Aura(a)] = &parts(&l)[..] else {
        panic!("one aura");
    };
    assert_eq!((a.state, a.target_state), (30, Some(7)));
    // No state for them: the engine's aura reaches only the caster, so the
    // data aura has no range.
    let rows = Rows::new(&[
        ("srvdofunc", "65"),
        ("aurastate", "might"),
        ("aurafilter", "0x10003"),
        ("aurarangecalc", "12"),
    ]);
    let l = lift(8, &rows).unwrap();
    assert!(l.toml.contains("range = \"0\""));
    assert!(l.notes.iter().any(|n| n.contains("only the caster")));
    // A target state the tables do not know is refused with the column.
    assert!(matches!(
        lift(
            8,
            &Rows::new(&[
                ("srvdofunc", "65"),
                ("aurastate", "might"),
                ("auratargetstate", "nope")
            ])
        ),
        Err(LiftError::Unknown {
            column: "auratargetstate",
            ..
        })
    ));
}

#[test]
fn a_curse_carries_its_state_range_time_filter_and_stats() {
    let bits: &'static str = Box::leak(Affects::Enemies.filter().to_string().into_boxed_str());
    let rows = Rows::new(&[
        ("skill", "Weaken"),
        ("srvdofunc", "30"),
        ("auratargetstate", "might"),
        ("aurafilter", bits),
        ("aurarangecalc", "10 + lvl"),
        ("auralencalc", "250"),
        ("aurastat1", "damagepercent"),
        ("aurastatcalc1", "0 - lvl * 5"),
    ]);
    let l = lift(9, &rows).unwrap();
    assert_eq!(l.family, Family::Curse);
    assert_eq!(l.family.name(), "curse");
    let [Part::Curse(c)] = &parts(&l)[..] else {
        panic!("one curse");
    };
    assert_eq!(
        (c.state, c.affects, c.stats.len()),
        (30, Affects::Enemies, 1)
    );
    assert_eq!(c.stats[0].stat, 25);
    assert!(l.toml.contains("seconds = \"(250) / 25\""));
    assert_eq!(l.cells[0], ("srvdofunc", BLOCK_SLOT.to_string()));
    // No AI states: no AI note. Dim Vision (23) says what the data part leaves out.
    assert!(l.notes.iter().all(|n| !n.contains("AI")));
    let ai = Rows::new(&[
        ("srvdofunc", "30"),
        ("auratargetstate", "23"),
        ("aurafilter", bits),
    ]);
    assert!(lift(9, &ai).unwrap().notes.iter().any(|n| n.contains("AI")));
    // A state the tables do not know is refused with the column.
    assert!(matches!(
        lift(
            9,
            &Rows::new(&[("srvdofunc", "30"), ("auratargetstate", "nope")])
        ),
        Err(LiftError::Unknown {
            column: "auratargetstate",
            ..
        })
    ));
}

#[test]
fn rows_the_lifter_does_not_know_are_refused_with_the_reason() {
    assert_eq!(
        lift(1, &Rows::new(&[("srvdofunc", "53")])).unwrap_err(),
        LiftError::Unsupported(53)
    );
    // A passive with no missile.
    assert_eq!(
        lift(1, &Rows::new(&[])).unwrap_err(),
        LiftError::Unsupported(0)
    );
    let e = lift(
        1,
        &Rows::new(&[("srvdofunc", "22"), ("srvmissilea", "nope")]),
    )
    .unwrap_err();
    assert_eq!(
        e,
        LiftError::Unknown {
            column: "srvmissilea",
            value: "nope".into()
        }
    );
    assert!(e.to_string().contains("srvmissilea"));
    // A formula the engine cannot read is reported by the engine reader.
    let e = lift(
        1,
        &Rows::new(&[
            ("srvdofunc", "22"),
            ("srvmissilea", "orb"),
            ("calc1", "lvl *"),
        ]),
    )
    .unwrap_err();
    assert!(matches!(e, LiftError::Unreadable(_)), "{e:?}");
}

#[test]
fn a_name_with_quotes_is_escaped() {
    let l = lift(
        1,
        &Rows::new(&[("skill", "A \"quoted\" \\ name"), ("srvdofunc", "1")]),
    )
    .unwrap();
    assert!(SkillRules::from_toml(&l.toml).is_ok(), "{}", l.toml);
}

#[test]
fn skills_are_ranked_by_the_classes_and_monsters_that_use_their_do_function() {
    let s = |name, slot, class| SkillUse { name, slot, class };
    let skills = [
        s("A", 1, "ama"),
        s("B", 1, "pal"),
        s("C", 22, "sor"),
        s("D", 65, ""),
        s("E", 65, "pal"),
        s("F", 7, "ama"),
    ];
    let monsters = [
        vec!["A", "C"],
        vec!["A", "B"],
        vec!["D"],
        vec!["D", "E", "nothing"],
    ];
    let r = rank(&skills, &monsters);
    // Slot 1: 2 classes and 2 monsters (a monster counts once per slot);
    // slot 65: 1 and 2; slot 22: 1 and 1; slot 7: 1 and 0.
    let got: Vec<_> = r
        .iter()
        .map(|u| (u.slot, u.skills, u.classes, u.monsters))
        .collect();
    assert_eq!(
        got,
        [(1, 2, 2, 2), (65, 2, 1, 2), (22, 1, 1, 1), (7, 1, 1, 0)]
    );
    assert_eq!(r[0].total(), 4);
}

#[test]
fn only_the_families_are_liftable() {
    for (slot, missile, family) in [
        (1, false, Some(Family::Attack)),
        (22, false, Some(Family::Nova)),
        (65, false, Some(Family::Aura)),
        (0, true, Some(Family::Missile)),
        (0, false, None),
        (81, false, Some(Family::DamageAura)),
        (999, true, None),
        (66, true, Some(Family::DamageAura)),
    ] {
        assert_eq!(Family::of(slot, missile), family, "{slot}");
    }
}
