//! The formula hooks' views of a hit (MS3): the hireling flag, the
//! defender's resist penalty for any unit type, the attacking weapon, the
//! block and avoid sequence and the hit result flags. Each hook's answer
//! is used, and D2RUST's games (and a mod game without the hook) are
//! unchanged.

use std::sync::{Arc, Mutex};

use super::*;
use crate::rng::Seed;
use crate::rules::{AvoidTerms, FormulaHooks, FormulaHost, ModRules, Ruleset, Side};
use crate::skills::fake::*;
use crate::units::UnitType;

const STR: u16 = 0;
const DEX: u16 = 2;
const LEVEL: u16 = 12;
const TOHIT: u16 = 19;
const TOBLOCK: u16 = 20;
const FIRERESIST: u16 = 39;
const PASSIVE_EVADE: u16 = 340;
/// A state the test mod's hit result reads.
const STEADFAST: u16 = 77;
/// The mode the test mod blocks in.
const BLOCK_MODE: i32 = 13;

/// The test mod game's hooks; `seen` records the sequence terms asked.
#[derive(Debug, Default)]
struct Hooks {
    seen: Mutex<Vec<AvoidTerms>>,
}

impl FormulaHooks for Hooks {
    fn avoid_chance(&self, h: &mut dyn FormulaHost, kind: BlockResult) -> Option<i32> {
        // Hirelings always evade; everyone else keeps D2RUST's chance.
        (kind == BlockResult::Evade && h.is_hireling(Side::Defender)).then_some(100)
    }
    fn defender_resist_penalty(&self, h: &mut dyn FormulaHost) -> Option<i32> {
        // A penalty for monsters only (any other answer: D2RUST's).
        (h.unit_type(Side::Defender) == Some(UnitType::Monster) && !h.is_hireling(Side::Defender))
            .then_some(-30)
    }
    fn damage(
        &self,
        h: &mut dyn FormulaHost,
        rec: &DamageRecord,
        _: bool,
        _: i32,
    ) -> Option<DamageRecord> {
        // The physical damage spells out the weapon's columns.
        let w = h.weapon(Side::Attacker)?;
        let physical = i32::from(w.two_handed) * 1000
            + i32::from(w.one_or_two_handed) * 100
            + w.str_bonus * 10
            + w.dex_bonus;
        Some(DamageRecord { physical, ..*rec })
    }
    fn block_or_avoid(&self, h: &mut dyn FormulaHost, terms: AvoidTerms) -> Option<BlockResult> {
        self.seen.lock().unwrap().push(terms);
        // A block by the defender's animation: one draw, then the 0x8000 flag.
        if h.mode(Side::Defender) != BLOCK_MODE {
            return None;
        }
        let r = h.seed(Side::Defender)?.roll(100) as i32;
        (r < terms.block).then_some(BlockResult::WeaponBlock)
    }
    fn hit_result(&self, h: &mut dyn FormulaHost, flags: u16, _: bool) -> Option<u16> {
        h.has_state(Side::Defender, STEADFAST)
            .then_some(flags & !result::GET_HIT)
    }
}

#[derive(Debug, Default)]
struct Game {
    hooks: Hooks,
}

impl ModRules for Game {
    fn name(&self) -> &str {
        "formula seams"
    }
    fn formulas(&self) -> Option<&dyn FormulaHooks> {
        Some(&self.hooks)
    }
}

/// A mod game whose hooks answer nothing.
#[derive(Debug)]
struct Silent;

impl FormulaHooks for Silent {}

impl ModRules for Silent {
    fn name(&self) -> &str {
        "silent"
    }
    fn formulas(&self) -> Option<&dyn FormulaHooks> {
        Some(&Silent)
    }
}

fn world(rules: Ruleset) -> Fake {
    Fake {
        hostile: true,
        in_range: true,
        expansion: true,
        rules,
        ..Fake::default()
    }
}

fn ct() -> CombatTables {
    combat_tables(vec![monster_rec()])
}

fn st() -> crate::skills::SkillTables {
    skill_tables(vec![skill_rec()])
}

/// A monster defender, moving, with 50% evade and `seed`.
fn evader(f: &mut Fake, hireling: bool, seed: u32) -> usize {
    let d = f.add(FUnit::new(UnitType::Monster, 0).with(PASSIVE_EVADE, 50));
    f.units[d].mode = 2;
    f.units[d].hireling = hireling;
    f.units[d].seed = Seed::new(0, seed);
    d
}

#[test]
fn hireling_flag_reaches_the_hooks() {
    // A draw of 80 misses D2RUST's 50% evade; the mod's hireling evades.
    for (rules, hireling, want) in [
        (Ruleset::Original, true, BlockResult::None),
        (
            Ruleset::Mod(Arc::new(Game::default())),
            false,
            BlockResult::None,
        ),
        (
            Ruleset::Mod(Arc::new(Game::default())),
            true,
            BlockResult::Evade,
        ),
    ] {
        let mut f = world(rules);
        let a = f.add(FUnit::new(UnitType::Player, 0));
        let d = evader(&mut f, hireling, 80);
        assert_eq!(dodge(&mut f, a, d, false), want, "hireling {hireling}");
    }
}

/// The fire left of 1000 on a defender of `ty` with 50 fire resist in hell.
fn fire_after(rules: Ruleset, ty: UnitType, hireling: bool) -> i32 {
    let c = ct();
    let mut f = world(rules);
    f.difficulty = 2;
    let a = f.add(FUnit::new(UnitType::Monster, 0));
    let d = f.add(FUnit::new(ty, 0).with(FIRERESIST, 50));
    f.units[d].hireling = hireling;
    let mut rec = DamageRecord {
        fire: 1000,
        ..DamageRecord::default()
    };
    totals(&mut f, &c, Some(a), d, &mut rec);
    rec.fire
}

#[test]
fn defender_resist_penalty_reaches_monsters() {
    let m = || Ruleset::Mod(Arc::new(Game::default()));
    // D2RUST: monsters take no penalty; players and hirelings the test
    // tables' hell penalty of -100.
    assert_eq!(fire_after(Ruleset::Original, UnitType::Monster, false), 500);
    assert_eq!(fire_after(Ruleset::Original, UnitType::Player, false), 1500);
    assert_eq!(fire_after(Ruleset::Original, UnitType::Monster, true), 1500);
    // The mod: monsters -30; players and hirelings keep D2RUST's.
    assert_eq!(fire_after(m(), UnitType::Monster, false), 800);
    assert_eq!(fire_after(m(), UnitType::Player, false), 1500);
    assert_eq!(fire_after(m(), UnitType::Monster, true), 1500);
    let silent = || Ruleset::Mod(Arc::new(Silent));
    assert_eq!(fire_after(silent(), UnitType::Monster, false), 500);
    assert_eq!(fire_after(silent(), UnitType::Player, false), 1500);
}

#[test]
fn damage_hook_sees_the_weapon() {
    let (s, c) = (st(), ct());
    let mut f = world(Ruleset::Mod(Arc::new(Game::default())));
    let a = f.add(FUnit::new(UnitType::Player, 0).with(STR, 50).with(DEX, 50));
    let d = f.add(FUnit::new(UnitType::Monster, 0));
    let sword = f.add_item(FItem {
        wield: 2,
        one_or_two: true,
        str_dex: (7, 3),
        ..FItem::default()
    });
    f.units[a].weapon = Some(sword);
    let mut rec = DamageRecord::default();
    fill(&mut f, &s, &c, a, d, &mut rec, false, 0);
    assert_eq!(rec.physical, 1000 + 100 + 70 + 3);
    let axe = f.add_item(FItem {
        wield: 1,
        str_dex: (110, 0),
        ..FItem::default()
    });
    f.units[a].weapon = Some(axe);
    let mut rec = DamageRecord::default();
    fill(&mut f, &s, &c, a, d, &mut rec, false, 0);
    assert_eq!(rec.physical, 1100);
    // No weapon: the view is `None` and the hook keeps D2RUST's roll.
    f.units[a].weapon = None;
    let mut rec = DamageRecord::default();
    let mut own = world(Ruleset::Original);
    let oa = own.add(FUnit::new(UnitType::Player, 0).with(STR, 50).with(DEX, 50));
    let od = own.add(FUnit::new(UnitType::Monster, 0));
    let mut want = DamageRecord::default();
    fill(&mut f, &s, &c, a, d, &mut rec, false, 0);
    fill(&mut own, &s, &c, oa, od, &mut want, false, 0);
    assert_eq!(rec.physical, want.physical);
}

/// A player defender with a shield (block chance 75) in `mode`.
fn blocker(f: &mut Fake, mode: i32, seed: u32) -> usize {
    let d = f.add(
        FUnit::new(UnitType::Player, 0)
            .with(TOBLOCK, 30)
            .with(DEX, 100)
            .with(LEVEL, 20),
    );
    f.units[d].shield = true;
    f.units[d].mode = mode;
    f.units[d].seed = Seed::new(0, seed);
    d
}

fn attacker(f: &mut Fake) -> usize {
    f.add(
        FUnit::new(UnitType::Monster, 0)
            .with(TOHIT, 300)
            .with(LEVEL, 10),
    )
}

#[test]
fn block_sequence_hook_answers_first() {
    let game = Arc::new(Game::default());
    let c = ct();
    let mut f = world(Ruleset::Mod(game.clone()));
    let a = attacker(&mut f);
    let d = blocker(&mut f, BLOCK_MODE, 5);
    let mut expect = Seed::new(0, 5);
    let blocks = (expect.roll(100) as i32) < 75;
    let want = if blocks {
        BlockResult::WeaponBlock
    } else {
        BlockResult::None
    };
    assert_eq!(block_or_dodge(&mut f, &c, a, d, true, true), want);
    // One draw, from the defender's seed.
    assert_eq!(f.units[d].seed, expect);
    assert_eq!(
        *game.hooks.seen.lock().unwrap(),
        [AvoidTerms {
            block: 75,
            block_third: false,
            missile: true,
        }]
    );
    // Not blockable: the chance is 0.
    game.hooks.seen.lock().unwrap().clear();
    block_or_dodge(&mut f, &c, a, d, false, false);
    assert_eq!(game.hooks.seen.lock().unwrap()[0].block, 0);
}

#[test]
fn block_sequence_unchanged_without_the_hook() {
    // D2RUST's sequence for a range of seeds and modes: a mod game that
    // doesn't answer (or answers `None`) gives the same result and the
    // same draws.
    let c = ct();
    for mode in [0, 3, BLOCK_MODE] {
        for seed in 0..40u32 {
            let run = |rules: Ruleset| {
                let mut f = world(rules);
                let a = attacker(&mut f);
                let d = blocker(&mut f, mode, seed * 7919);
                f.units[d].moving = mode == 3;
                let b = block_or_dodge(&mut f, &c, a, d, false, true);
                (b, f.units[d].seed)
            };
            let own = run(Ruleset::Original);
            assert_eq!(run(Ruleset::Mod(Arc::new(Silent))), own);
            if mode != BLOCK_MODE {
                assert_eq!(run(Ruleset::Mod(Arc::new(Game::default()))), own);
            }
        }
    }
}

#[test]
fn hit_result_hook_replaces_the_flags() {
    let (s, c) = (st(), ct());
    let run = |rules: Ruleset, steadfast: bool| {
        let mut f = world(rules);
        let a = attacker(&mut f);
        // A running player is always hit; with no evade it takes the hit.
        let d = f.add(FUnit::new(UnitType::Player, 0).with(LEVEL, 1));
        f.units[d].mode = 3;
        if steadfast {
            f.units[d].states.push(STEADFAST);
        }
        melee_result(&mut f, &s, &c, Some(a), Some(d), 100_000, 0)
    };
    let own = run(Ruleset::Original, true);
    assert_eq!(
        own & (result::HIT | result::GET_HIT),
        result::HIT | result::GET_HIT
    );
    let m = || Ruleset::Mod(Arc::new(Game::default()));
    assert_eq!(run(m(), true), own & !result::GET_HIT);
    assert_eq!(run(m(), false), own);
}
