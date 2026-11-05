//! What `crowd_control.toml` says and refuses, and declared crowd control on the
//! real combat code: a plain hit does not interrupt, a declared one does, and
//! classic rules are D2Gamma's.

use super::*;
use crate::combat::cc::{self, Declared};
use crate::combat::{self, DamageRecord};
use crate::rules::data_mod::tests::wired;
use crate::rules::{DataMod, DataModFiles, ModRules, Ruleset, SkillRules};
use crate::units::UnitType;

fn setting(text: &str) -> String {
    match CcRules::from_toml(text) {
        Err(CcError::Setting(s)) => s,
        Err(CcError::Toml(e)) => e.to_string(),
        Ok(_) => panic!("accepted"),
    }
}

const FILE: &str = r#"
version = 1

[[source]]
monster = 156
attack = "a1"
[[source.effect]]
kind = "stun"
seconds = 1.5
[[source.effect]]
kind = "interrupt"
chance = 50

[[source]]
missile = 214
[[source.effect]]
kind = "knockback"

[[source]]
skill = 60
[[source.effect]]
kind = "freeze"
seconds = 2
"#;

fn stun(ticks: i32) -> CcEffect {
    CcEffect {
        kind: CcKind::Stun,
        chance: 100,
        ticks,
    }
}

fn monster(class: u32, mode: u32) -> CcSource {
    CcSource::Monster { class, mode }
}

#[test]
fn the_file_reads_into_sources() {
    let rules = CcRules::from_toml(FILE).unwrap();
    assert_eq!(rules.len(), 3);
    // 1.5 seconds are 38 base ticks (25 to the second, rounded).
    let boss = rules.declared(monster(156, 4));
    assert_eq!(boss.len(), 2);
    assert_eq!(boss[0], stun(38));
    assert_eq!((boss[1].kind, boss[1].chance), (CcKind::Interrupt, 50));
    // Another attack slot, another monster: none.
    assert!(rules.declared(monster(156, 5)).is_empty());
    assert!(rules.declared(monster(157, 4)).is_empty());
    assert_eq!(
        rules.declared(CcSource::Missile {
            row: 214,
            skill: -1
        })[0]
            .kind,
        CcKind::Knockback
    );
    // A skill source answers the missiles the skill made.
    let by_skill = rules.declared(CcSource::Missile { row: 1, skill: 60 });
    assert_eq!((by_skill[0].kind, by_skill[0].ticks), (CcKind::Freeze, 50));
    assert!(rules
        .declared(CcSource::Missile { row: 1, skill: 61 })
        .is_empty());
}

#[test]
fn an_attack_slot_left_out_means_every_attack() {
    let rules =
        CcRules::from_toml("[[source]]\nmonster = 5\n[[source.effect]]\nkind = \"interrupt\"\n")
            .unwrap();
    for mode in [4, 5, 8, 11] {
        assert_eq!(rules.declared(monster(5, mode)).len(), 1);
    }
}

#[test]
fn a_bad_file_names_the_entry_and_the_key() {
    const INTERRUPT: &str = "[[source.effect]]\nkind = \"interrupt\"\n";
    for (text, key) in [
        ("version = 2\n".to_owned(), "version"),
        (
            "[[source]]\n[[source.effect]]\nkind = \"stun\"\nseconds = 1\n".into(),
            "source[0].monster",
        ),
        (
            format!("[[source]]\nmonster = 1\nmissile = 2\n{INTERRUPT}"),
            "source[0].monster",
        ),
        (
            format!("[[source]]\nmissile = 2\nattack = \"a1\"\n{INTERRUPT}"),
            "source[0].attack",
        ),
        (
            format!("[[source]]\nmonster = 2\nattack = \"a3\"\n{INTERRUPT}"),
            "source[0].attack",
        ),
        ("[[source]]\nmonster = 2\n".into(), "source[0].effect"),
        (
            "[[source]]\nmonster = 2\n[[source.effect]]\nkind = \"stun\"\n".into(),
            "source[0].effect[0]",
        ),
        (
            "[[source]]\nmonster = 2\n[[source.effect]]\nkind = \"stun\"\nseconds = 11\n".into(),
            "source[0].effect[0]",
        ),
        (
            format!("[[source]]\nmonster = 2\n{INTERRUPT}seconds = 1\n"),
            "source[0].effect[0]",
        ),
        (
            format!("[[source]]\nmonster = 2\n{INTERRUPT}chance = 0\n"),
            "source[0].effect[0]",
        ),
        (
            "[[source]]\nmonster = 2\n[[source.effect]]\nkind = \"slow\"\n".into(),
            "source[0].effect[0]",
        ),
    ] {
        let message = setting(&text);
        assert!(message.contains(key), "{key}: {message}");
    }
    // A typo in a key is an error too.
    assert!(setting("[[source]]\nmonter = 2\n").contains("monter"));
}

#[test]
fn the_host_reads_the_file_and_answers_the_seam() {
    let files = DataModFiles {
        crowd_control: Some(("crowd_control.toml", FILE)),
        ..DataModFiles::default()
    };
    let host = DataMod::from_files("t", &files).unwrap();
    assert_eq!(host.crowd_control_rules().len(), 3);
    assert_eq!(host.declared_cc(monster(156, 4)).len(), 2);
    assert_eq!(host.into_ruleset().declared_cc(monster(156, 4)).len(), 2);
    // D2Gamma's own rules and a mod without the file declare nothing.
    assert!(Ruleset::Original.declared_cc(monster(156, 4)).is_empty());
    assert!(DataMod::new("t").declared_cc(monster(156, 4)).is_empty());
    let bad = DataModFiles {
        crowd_control: Some(("crowd_control.toml", "[[source]]\nmonster = 1\n")),
        ..DataModFiles::default()
    };
    let e = DataMod::from_files("t", &bad).unwrap_err().to_string();
    assert!(
        e.contains("crowd_control.toml") && e.contains("source[0].effect"),
        "{e}"
    );
}

#[test]
fn a_skill_part_declares_and_its_missile_hits_carry_it() {
    let skills = "[[skill]]\nid = 7\n\
        [[skill.part]]\nkind = \"missile\"\nmissile = 3\n\
        [[skill.part.cc]]\nkind = \"stun\"\nseconds = 0.5\n\
        [[skill.part]]\nkind = \"area_hit\"\nradius = 4\n[skill.part.damage]\nmin = 1\nmax = 1\n\
        [[skill.part.cc]]\nkind = \"knockback\"\n";
    let rules = SkillRules::from_toml(skills).unwrap();
    // 0.5 seconds are 13 ticks.
    assert_eq!(rules.missile_cc(7), [stun(13)]);
    assert!(rules.missile_cc(8).is_empty());
    let files = DataModFiles {
        skills: Some(("skills.toml", skills)),
        ..DataModFiles::default()
    };
    let host = DataMod::from_files("t", &files).unwrap();
    assert_eq!(
        host.declared_cc(CcSource::Missile { row: 3, skill: 7 }),
        [stun(13)]
    );
    // A part may not declare more than 8 effects.
    let many = format!(
        "[[skill]]\nid = 1\n[[skill.part]]\nkind = \"missile\"\nmissile = 3\n{}",
        "[[skill.part.cc]]\nkind = \"interrupt\"\n".repeat(9)
    );
    assert!(SkillRules::from_toml(&many).is_err());
}

// --------------------------------------------------------------------- combat

fn rules_from(rules_toml: &str) -> Ruleset {
    let files = DataModFiles {
        rules: Some(("rules.toml", rules_toml)),
        ..DataModFiles::default()
    };
    DataMod::from_files("cc", &files).unwrap().into_ruleset()
}

/// Whether a hit with `flags`, `total` damage and `declared` moves the defender
/// out of neutral (a get-hit or knockback reaction).
fn reacts(rules: &Ruleset, ty: UnitType, flags: u16, total: i32, declared: Declared) -> bool {
    let neutral = 1;
    let (mut fx, a, d) = wired(rules.clone(), ty);
    fx.sim.sys.units.get_mut(d).unwrap().mode = neutral;
    let mut rec = DamageRecord {
        result: combat::result::HIT | flags,
        total,
        declared,
        ..Default::default()
    };
    fx.sim.combat(&mut fx.game, |w, _| {
        crate::wiring::action::reaction::reaction(w, a, d, &mut rec)
    });
    fx.sim.sys.units.get(d).unwrap().mode != neutral
}

const PLAIN: Declared = Declared {
    interrupt: false,
    knockback: false,
    stun_len: 0,
    freeze_len: 0,
};
const INTERRUPT: Declared = Declared {
    interrupt: true,
    ..PLAIN
};
const KNOCK: Declared = Declared {
    knockback: true,
    ..PLAIN
};
const BOTH: [UnitType; 2] = [UnitType::Player, UnitType::Monster];
const GET_HIT: u16 = combat::result::GET_HIT;
const KNOCKBACK: u16 = combat::result::KNOCKBACK;

#[test]
fn declared_mode_a_plain_hit_does_not_interrupt_but_a_declared_one_does() {
    let declared = rules_from("crowd_control = \"declared\"\n");
    for ty in BOTH {
        for flags in [GET_HIT, KNOCKBACK] {
            // D2Gamma interrupts and shoves; declared mode does not.
            assert!(
                reacts(&Ruleset::Original, ty, flags, 20_000, PLAIN),
                "{ty:?} {flags}"
            );
            assert!(
                !reacts(&declared, ty, flags, 20_000, PLAIN),
                "{ty:?} {flags}"
            );
        }
        // The source's own declaration brings the reaction back.
        assert!(reacts(&declared, ty, GET_HIT, 20_000, INTERRUPT));
        assert!(reacts(&declared, ty, KNOCKBACK, 20_000, KNOCK));
    }
    // A declared interrupt is not a knockback (a monster class with no
    // knockback mode turns one into a get-hit, so only the player shows it).
    assert!(!reacts(
        &declared,
        UnitType::Player,
        KNOCKBACK,
        20_000,
        INTERRUPT
    ));
}

#[test]
fn a_declared_interrupt_does_not_depend_on_the_size_of_the_hit() {
    let declared = rules_from("crowd_control = \"declared\"\n");
    // A hit under one life point: D2Gamma only makes it a soft hit.
    let small = 200;
    for ty in BOTH {
        assert!(!reacts(&Ruleset::Original, ty, GET_HIT, small, PLAIN));
        assert!(reacts(&declared, ty, GET_HIT, small, INTERRUPT));
        // The hit-recovery switch still wins: no recovery, no interrupt.
        let off = rules_from(
            "crowd_control = \"declared\"\n[players.hit_recovery]\nenabled = false\n\
             [monsters.hit_recovery]\nenabled = false\n",
        );
        assert!(!reacts(&off, ty, GET_HIT, 20_000, INTERRUPT));
    }
}

#[test]
fn players_and_monsters_are_configured_apart() {
    // Only the player is protected from undeclared crowd control.
    let players = rules_from("[players]\ncrowd_control = \"declared\"\n");
    assert!(!reacts(&players, UnitType::Player, GET_HIT, 20_000, PLAIN));
    assert!(reacts(&players, UnitType::Monster, GET_HIT, 20_000, PLAIN));
    // "declared" for all, the monsters put back to classic.
    let monsters =
        rules_from("crowd_control = \"declared\"\n[monsters]\ncrowd_control = \"classic\"\n");
    assert!(!reacts(&monsters, UnitType::Player, GET_HIT, 20_000, PLAIN));
    assert!(reacts(&monsters, UnitType::Monster, GET_HIT, 20_000, PLAIN));
}

#[test]
fn classic_mode_is_d2gamma_and_declarations_work_without_the_switch() {
    let classic = rules_from("crowd_control = \"classic\"\n");
    for ty in BOTH {
        for flags in [GET_HIT, KNOCKBACK] {
            assert_eq!(
                reacts(&Ruleset::Original, ty, flags, 20_000, PLAIN),
                reacts(&classic, ty, flags, 20_000, PLAIN)
            );
        }
        // A declaration adds to the classic rules.
        assert!(reacts(&classic, ty, GET_HIT, 200, INTERRUPT));
    }
    let bad = DataMod::from_files(
        "x",
        &DataModFiles {
            rules: Some(("rules.toml", "crowd_control = \"sometimes\"\n")),
            ..DataModFiles::default()
        },
    );
    assert!(bad.unwrap_err().to_string().contains("crowd_control"));
}

/// The defender's stun length in frames after a hit with `stun_len` ticks on
/// the record, `declared` of them declared.
fn stun_after(rules: &Ruleset, stun_len: i32, declared: i32) -> Option<i32> {
    use crate::combat::CombatWorld;
    let (mut fx, a, d) = wired(rules.clone(), UnitType::Player);
    let mut rec = DamageRecord {
        result: combat::result::HIT,
        physical: 256,
        stun_len,
        declared: Declared {
            stun_len: declared,
            ..PLAIN
        },
        ..Default::default()
    };
    fx.sim.combat(&mut fx.game, |w, t| {
        combat::apply(w, &t.combat, a, d, true, &mut rec)
    });
    fx.sim
        .combat(&mut fx.game, |w, _| w.state_list_expiry(d, 21))
        .map(|e| e - fx.game.frame)
}

#[test]
fn declared_mode_keeps_only_the_stun_a_source_declared() {
    let declared = rules_from("crowd_control = \"declared\"\n");
    let classic = stun_after(&Ruleset::Original, 38, 0);
    assert!(classic.is_some_and(|t| t > 0), "D2Gamma stuns");
    // An undeclared stun is dropped; the declared one runs for its own length.
    assert_eq!(stun_after(&declared, 38, 0), None);
    assert_eq!(stun_after(&declared, 38, 38), classic);
    assert_eq!(
        stun_after(&declared, 0, 38),
        classic,
        "the declared length is what runs"
    );
}

#[test]
fn declare_puts_the_effects_on_the_record_and_rolls_only_for_a_chance() {
    let (mut fx, a, _) = wired(Ruleset::Original, UnitType::Monster);
    let seed_before = fx.sim.sys.units.get(a).unwrap().seed;
    let hit = || DamageRecord {
        result: combat::result::HIT,
        ..Default::default()
    };
    let always = [
        stun(38),
        CcEffect {
            kind: CcKind::Freeze,
            chance: 100,
            ticks: 50,
        },
        CcEffect {
            kind: CcKind::Knockback,
            chance: 100,
            ticks: 0,
        },
    ];
    let mut rec = hit();
    fx.sim
        .combat(&mut fx.game, |w, _| cc::declare(w, a, &mut rec, &always));
    assert_eq!(
        rec.declared,
        Declared {
            interrupt: true,
            knockback: true,
            stun_len: 38,
            freeze_len: 50
        }
    );
    assert_eq!((rec.stun_len, rec.freeze_len), (38, 50));
    assert_ne!(rec.result & GET_HIT, 0, "a stun interrupts");
    assert_ne!(rec.result & KNOCKBACK, 0);
    assert_eq!(
        fx.sim.sys.units.get(a).unwrap().seed,
        seed_before,
        "a chance of 100 draws nothing"
    );
    // A chance draws one number from the attacker's seed.
    let chancy = [CcEffect {
        kind: CcKind::Interrupt,
        chance: 50,
        ticks: 0,
    }];
    let mut rec = hit();
    fx.sim
        .combat(&mut fx.game, |w, _| cc::declare(w, a, &mut rec, &chancy));
    assert_ne!(fx.sim.sys.units.get(a).unwrap().seed, seed_before);
    // A miss declares nothing.
    let mut miss = DamageRecord::default();
    fx.sim
        .combat(&mut fx.game, |w, _| cc::declare(w, a, &mut miss, &always));
    assert_eq!(miss, DamageRecord::default());
}

#[test]
fn item_stun_and_freeze_count_as_declared() {
    let mut rec = DamageRecord::default();
    cc::declare_stun(&mut rec, 0);
    assert!(!rec.declared.any());
    cc::declare_stun(&mut rec, 13);
    cc::declare_freeze(&mut rec, 5000);
    assert_eq!(
        rec.declared,
        Declared {
            interrupt: true,
            stun_len: 13,
            freeze_len: cc::MAX_TICKS,
            ..PLAIN
        }
    );
}

#[test]
fn a_monster_attack_source_declares_on_the_melee_record() {
    let files = DataModFiles {
        crowd_control: Some((
            "crowd_control.toml",
            "[[source]]\nmonster = 0\nattack = \"a1\"\n\
             [[source.effect]]\nkind = \"stun\"\nseconds = 1.5\n",
        )),
        rules: Some(("rules.toml", "crowd_control = \"declared\"\n")),
        ..DataModFiles::default()
    };
    let rules = DataMod::from_files("boss", &files).unwrap().into_ruleset();
    let attack = |rules: &Ruleset, mode: u32| {
        // `wired` puts a player at one end and the second unit (here a monster
        // of class 0) at the other; the monster attacks the player.
        let (mut fx, p, m) = wired(rules.clone(), UnitType::Monster);
        fx.sim.sys.units.get_mut(m).unwrap().mode = mode;
        let mut rec = DamageRecord {
            result: combat::result::HIT,
            hit_flags: combat::hitflag::SKIP_ROLL,
            physical: 256,
            ..Default::default()
        };
        fx.sim.combat(&mut fx.game, |w, t| {
            combat::start_combat(w, &t.skills, &t.combat, Some(m), Some(p), &mut rec, 128)
        });
        rec.declared
    };
    // Attack 1 of monster class 0 stuns for 1.5 seconds (and interrupts);
    // attack 2, and a game without the file, declare nothing.
    assert_eq!(
        attack(&rules, 4),
        Declared {
            interrupt: true,
            stun_len: 38,
            ..PLAIN
        }
    );
    assert_eq!(attack(&rules, 5), PLAIN);
    assert_eq!(attack(&Ruleset::Original, 4), PLAIN);
}

#[test]
fn an_immune_state_is_read() {
    let rules = CcRules::from_toml("immune_state = 900").unwrap();
    assert_eq!(rules.immune_state(), Some(900));
    assert!(!rules.is_empty());
    assert_eq!(
        CcRules::from_toml("version = 1").unwrap().immune_state(),
        None
    );
    assert!(CcRules::from_toml("immune_state = 70000").is_err());
    assert!(CcRules::from_toml("immune_state = -1").is_err());
}
