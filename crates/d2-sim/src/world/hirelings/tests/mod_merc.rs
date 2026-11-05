//! A mod game's hireling experience and growth
//! ([`crate::world::npc::NpcProvider::merc_exp`],
//! [`crate::world::npc::NpcProvider::merc_growth`]), with a synthetic
//! provider: level L needs L·1000 experience and gives strength 10·L and
//! skill 8 at level L. A provider without them leaves D2RUST's.

use std::sync::Arc;

use super::fake::*;
use crate::rules::{ModRules, NpcProvider, Ruleset};
use crate::units::UnitId;
use crate::world::hirelings::level::{add_experience, apply_level, restore_level_in};
use crate::world::hirelings::{stat, HirelingRow, HirelingState, HirelingTables, PetNode};
use crate::world::npc::npc_rules::MercGrowth;

const PLAYER_GUID: u32 = 0x10;
const MERC_GUID: u32 = 0x4433_2211;

#[derive(Debug)]
struct Growth;

impl NpcProvider for Growth {
    fn merc_exp(&self, _row: &HirelingRow, level: i32) -> Option<i32> {
        Some(level * 1000)
    }
    fn merc_growth(
        &self,
        _row: &HirelingRow,
        level: i32,
        req_level: &dyn Fn(u32) -> Option<i16>,
    ) -> Option<MercGrowth> {
        Some(MercGrowth {
            stats: vec![(stat::STRENGTH, level * 10)],
            skills: req_level(8).map(|_| (8, level)).into_iter().collect(),
        })
    }
}

#[derive(Debug)]
struct Bare;

impl NpcProvider for Bare {}

#[derive(Debug)]
struct Game<P>(P);

impl<P: NpcProvider + 'static> ModRules for Game<P> {
    fn name(&self) -> &str {
        "hireling test mod"
    }
    fn npcs(&self) -> Option<&dyn NpcProvider> {
        Some(&self.0)
    }
}

fn modded<P: NpcProvider + 'static>(p: P) -> Ruleset {
    Ruleset::Mod(Arc::new(Game(p)))
}

struct Merc {
    w: Fake,
    t: HirelingTables,
    st: HirelingState,
    player: UnitId,
    merc: UnitId,
}

/// A level-10 player's Act 1 Ice hireling (`Exp/Lvl` 105, Inner Sight in
/// its first skill slot) under `rules`.
fn merc(rules: Ruleset) -> Merc {
    let mut w = Fake::new(true);
    w.rules = rules;
    let player = w.add(1, 0, 0, PLAYER_GUID);
    let merc = w.add(2, 1, 271, MERC_GUID);
    w.set(player, stat::LEVEL, 10);
    w.unit_mut(merc).owner = Some((PLAYER_GUID, 0));
    w.reqlevel.insert(8, 1);
    let mut st = HirelingState::default();
    st.list_mut(player).nodes.push(PetNode {
        guid: MERC_GUID,
        id: 1,
        ..PetNode::default()
    });
    let mut rows = act1_ice_rows();
    for r in &mut rows {
        r.exp_lvl = 105;
        r.skills[0] = skill(8, 0, 1, 10);
    }
    Merc {
        w,
        t: tables(rows),
        st,
        player,
        merc,
    }
}

#[test]
fn a_mod_grows_its_hireling() {
    let mut s = merc(modded(Growth));
    let m = s.merc;
    apply_level(&mut s.w, &s.t, &s.st, s.player, Some(m), 6);
    assert_eq!(s.w.base(m, stat::LEVEL), 6);
    assert_eq!(s.w.base(m, stat::NEXTEXP), 7000);
    assert_eq!(s.w.base(m, stat::EXPERIENCE), 6000);
    assert_eq!(s.w.base(m, stat::STRENGTH), 60);
    assert_eq!(s.w.base(m, stat::MAXHP), 0, "D2RUST's growth did not run");
    assert_eq!(s.w.unit(m).skills.get(&8), Some(&6));
    // The mod's thresholds: 6000 + 2·1000 = 8000 is level 8's.
    add_experience(&mut s.w, &s.t, &s.st, s.player, m, 6, 1000).unwrap();
    assert_eq!(s.w.base(m, stat::LEVEL), 8);
    assert_eq!(s.w.base(m, stat::STRENGTH), 80);
    let rules = modded(Growth);
    assert_eq!(restore_level_in(&rules, &s.t, 1, 8500, true), Some(8));
}

#[test]
fn the_engines_hireling_growth_is_unchanged() {
    let run = |rules: Ruleset| {
        let restored = restore_level_in(&rules, &tables(act1_ice_rows()), 1, 85_050, true);
        let mut s = merc(rules);
        let m = s.merc;
        apply_level(&mut s.w, &s.t, &s.st, s.player, Some(m), 6);
        add_experience(&mut s.w, &s.t, &s.st, s.player, m, 6, 29_295).unwrap();
        (format!("{:?}", s.w.units), s.w.sent.clone(), restored)
    };
    let own = run(Ruleset::Original);
    assert!(own.0.contains("skills: {8: "), "D2RUST's Inner Sight");
    assert_eq!(run(modded(Bare)), own);
    assert_eq!(
        run(crate::rules::global::GlobalRules::default().into_ruleset()),
        own
    );
}

#[test]
fn global_curve_changes_level_stats_and_restore() {
    let rules =
        crate::rules::global::GlobalRules::from_toml("hireling_experience_curve_percent = 200")
            .unwrap()
            .into_ruleset();
    let mut s = merc(rules.clone());
    let m = s.merc;
    apply_level(&mut s.w, &s.t, &s.st, s.player, Some(m), 6);
    assert_eq!(s.w.base(m, stat::EXPERIENCE), 52920);
    assert_eq!(s.w.base(m, stat::NEXTEXP), 82320);
    assert_eq!(restore_level_in(&rules, &s.t, 1, 82319, true), Some(6));
    assert_eq!(restore_level_in(&rules, &s.t, 1, 82320, true), Some(7));
}
