//! The skill host seams a mod game's skill bodies get on the wired skill
//! view: a tiny mod game answers a do slot past the tables with a body
//! that calls a seam, and the test checks the seam reached the engine's
//! own code and the body used its answer. Under D2RUST's own rules no mod
//! body runs. The fixture is the skill use tests' (two 8×8 preset rooms,
//! streamed).

use std::fmt;
use std::sync::{Arc, Mutex};

use crate::rules::{
    AuraPulse, ModRules, Ruleset, SkillCharge, SkillFn, SkillHost, SkillProvider, Summon,
};
use crate::skills::list::{ListOwner, SkillList};
use crate::skills::use_::bodies::{self, b3_lvl30, BodyStat, BodyTables, BodyWorld};
use crate::skills::use_::SkillFunctions;
use crate::units::hooks::Sim;
use crate::units::lifecycle::LifecycleHooks;
use crate::units::{UnitId, UnitType};
use crate::wiring::action::Pending;

use super::skill_use::{skills, Fx, Open};

/// The mod's do slot (past the tables).
const MOD_DO: u16 = 200;
/// Stat 12, the unit's level.
const LEVEL: u16 = 12;
/// Stat 31, armor class.
const ARMORCLASS: u16 = 31;

type Body = Box<dyn Fn(&mut dyn SkillHost, UnitId) -> i32 + Send + Sync>;

/// A mod game whose do slot [`MOD_DO`] runs `body`.
struct BodyMod(Body);

impl fmt::Debug for BodyMod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BodyMod")
    }
}

impl SkillProvider for BodyMod {
    fn has(&self, kind: SkillFn, index: u16) -> bool {
        kind == SkillFn::Do && index == MOD_DO
    }
    fn srvdo(
        &self,
        w: &mut dyn SkillHost,
        _index: u16,
        u: UnitId,
        _skill: i32,
        _lvl: i32,
        _charge: bool,
        _item: bool,
        _aim: bool,
    ) -> Option<i32> {
        Some((self.0)(w, u))
    }
}

impl ModRules for BodyMod {
    fn name(&self) -> &str {
        "body-mod"
    }
    fn skills(&self) -> Option<&dyn SkillProvider> {
        Some(self)
    }
}

/// The fixture: a player at (10, 10), a monster at (12, 10) and one at
/// (30, 10), the monsters targetable, attackable and valid targets.
fn fx() -> (Fx, UnitId, UnitId, UnitId) {
    let mut fx = Fx::with(super::stat_data(), skills());
    let p = fx.spawn(UnitType::Player, 10, 10);
    let m = fx.spawn(UnitType::Monster, 12, 10);
    let far = fx.spawn(UnitType::Monster, 30, 10);
    for u in [m, far] {
        fx.sim.sys.units.get_mut(u).unwrap().flags |= 0xE;
    }
    (fx, p, m, far)
}

/// Installs a mod whose do slot runs `body`, then runs the slot for `u`
/// on the skill view: the slot's result.
fn run(
    fx: &mut Fx,
    u: UnitId,
    body: impl Fn(&mut dyn SkillHost, UnitId) -> i32 + Send + Sync + 'static,
) -> i32 {
    fx.sim.sys.hooks.rules = Ruleset::Mod(Arc::new(BodyMod(Box::new(body))));
    fx.sim.skill_use(&mut fx.game, |w| {
        w.srvdo(MOD_DO, u, 0, 1, false, false, false)
    })
}

/// What a mod body saw, for the test to read back.
fn seen<T: Send + 'static>() -> Arc<Mutex<Vec<T>>> {
    Arc::new(Mutex::new(Vec::new()))
}

#[test]
fn original_rules_run_no_mod_body() {
    let (mut fx, p, _, _) = fx();
    let r = fx.sim.skill_use(&mut fx.game, |w| {
        w.srvdo(MOD_DO, p, 0, 1, false, false, false)
    });
    assert_eq!(r, 0);
    // The slot went to the host's own seam, as before.
    assert_eq!(fx.sim.sys.hooks.x.log, ["srvdo 200"]);
}

// ---- unit search and AI ------------------------------------------------------------

#[test]
fn units_near_scans_the_rooms_around_the_point() {
    let (mut fx, p, m, far) = fx();
    let got = seen::<Vec<UnitId>>();
    let g = got.clone();
    let r = run(&mut fx, p, move |w, u| {
        let mut near = w.units_near(u, (10, 10), 3);
        near.sort();
        let mut wide = w.units_near(u, (10, 10), 25);
        wide.sort();
        let n = near.len() as i32;
        g.lock().unwrap().extend([near, wide]);
        n
    });
    // The body used the answer.
    assert_eq!(r, 2);
    let got = got.lock().unwrap();
    let mut near = vec![p, m];
    near.sort();
    let mut wide = vec![p, m, far];
    wide.sort();
    assert_eq!(*got, [near, wide]);
    fx.assert_clean();
}

#[test]
fn units_near_without_a_room_is_empty() {
    let (mut fx, p, _, _) = fx();
    let r = run(&mut fx, p, |w, u| {
        w.units_near(u, (5000, 5000), 10).len() as i32
    });
    assert_eq!(r, 0);
}

#[test]
fn passes_filter_is_the_skill_unit_filter() {
    let (mut fx, p, m, _) = fx();
    // Filter bit 0x2: monsters; 0x1: players.
    let r = run(&mut fx, p, move |w, u| {
        i32::from(w.passes_filter(u, m, 0x2))
            | i32::from(w.passes_filter(u, u, 0x2)) << 1
            | i32::from(w.passes_filter(u, u, 0x1)) << 2
    });
    assert_eq!(r, 0b101);
    // The same answers as the skill bodies' own filter.
    let t = fx.sim.sys.hooks.tables.clone();
    let direct = fx.sim.skill_use(&mut fx.game, |w| {
        i32::from(bodies::accepts(w, &t.skills, &t.combat, p, m, 0x2))
            | i32::from(bodies::accepts(w, &t.skills, &t.combat, p, p, 0x2)) << 1
            | i32::from(bodies::accepts(w, &t.skills, &t.combat, p, p, 0x1)) << 2
    });
    assert_eq!(r, direct);
}

#[test]
fn los_blocked_is_the_line_test() {
    let (mut fx, p, m, _) = fx();
    // The fixture's line test is clear everywhere.
    let r = run(&mut fx, p, move |w, u| i32::from(w.los_blocked(u, m, 4)));
    assert_eq!(r, 0);
}

#[test]
fn curse_allowed_gates_on_flags_and_alignment() {
    let (mut fx, p, m, _) = fx();
    let check = |fx: &mut Fx| {
        run(fx, p, move |w, u| {
            i32::from(w.curse_allowed(u)) | i32::from(w.curse_allowed(m)) << 1
        })
    };
    // Both pass.
    assert_eq!(check(&mut fx), 0b11);
    // Not attackable (unit flag 0x4 clear): refused.
    fx.sim.sys.units.get_mut(p).unwrap().flags &= !0x4;
    assert_eq!(check(&mut fx), 0b10);
    fx.sim.sys.units.get_mut(p).unwrap().flags |= 0x4;
    // Neutral: refused.
    fx.sim.sys.hooks.x.align.insert(m, 1);
    assert_eq!(check(&mut fx), 0b01);
    fx.sim.sys.hooks.x.align.remove(&m);
    assert_eq!(check(&mut fx), 0b11);
}

#[test]
fn accepts_aura_refuses_npcs_and_unattackable_monsters() {
    let (mut fx, p, m, _) = fx();
    let check = |fx: &mut Fx| {
        run(fx, p, move |w, u| {
            i32::from(w.accepts_aura(u)) | i32::from(w.accepts_aura(m)) << 1
        })
    };
    {
        let at = Arc::make_mut(&mut fx.sim.sys.hooks.tables);
        at.combat.monstats2[0].isatt = false;
    }
    assert_eq!(check(&mut fx), 0b01);
    {
        let at = Arc::make_mut(&mut fx.sim.sys.hooks.tables);
        at.combat.monstats2[0].isatt = true;
    }
    assert_eq!(check(&mut fx), 0b11);
    {
        let at = Arc::make_mut(&mut fx.sim.sys.hooks.tables);
        at.combat.monstats[0].npc = true;
    }
    assert_eq!(check(&mut fx), 0b01);
}

#[test]
fn notify_ai_sets_the_monster_ai_state() {
    let (mut fx, p, m, _) = fx();
    let r = run(&mut fx, p, move |w, _| {
        w.notify_ai(m, 10);
        1
    });
    assert_eq!(r, 1);
    // No AI control in the fixture: the host's AI seam gets the state.
    assert_eq!(fx.sim.sys.hooks.x.log, [format!("ai {} 10", m.0)]);
}

// ---- summons, pets, revives and paths ------------------------------------------------

/// The fixture with three pet types and a level 10 player.
fn pet_fx() -> (Fx, UnitId, UnitId) {
    let (mut fx, p, m, _) = fx();
    fx.sim.sys.hooks.bodies = Some(Arc::new(BodyTables {
        stats: vec![BodyStat::default(); 359],
        state_group: vec![0; super::N_STATES],
        state_aura: vec![false; super::N_STATES],
        pettype_count: 3,
        pettype_group: vec![0; 3],
        ..BodyTables::default()
    }));
    fx.set(p, &[(LEVEL, 10)]);
    (fx, p, m)
}

/// A summon of class 0 at (x, 10), pet type 2, at most `max`.
fn summon_at(x: i32, max: i32) -> impl Fn(&mut dyn SkillHost, UnitId) -> i32 + Send + Sync {
    move |w, u| {
        let req = Summon {
            flags: 1,
            owner: u,
            class: 0,
            ai: 0,
            mode: 1,
            x,
            y: 10,
            pet_type: 2,
            pet_max: max,
        };
        match w.summon(req) {
            Some(_) => w.pet_count(u, 2),
            None => -1,
        }
    }
}

/// The GUIDs in the player's pet list of type 2.
fn pets_of(fx: &Fx, p: UnitId) -> Vec<i32> {
    fx.sim
        .sys
        .hooks
        .pet_lists
        .get(&p)
        .map(|l| l.entries[2].nodes.iter().map(|n| n.guid).collect())
        .unwrap_or_default()
}

#[test]
fn summon_spawns_a_pet_the_owner_counts() {
    let (mut fx, p, _) = pet_fx();
    let before = fx.game.lists.units_of_type(UnitType::Monster).len();
    assert_eq!(run(&mut fx, p, summon_at(14, 2)), 1);
    assert_eq!(
        fx.game.lists.units_of_type(UnitType::Monster).len(),
        before + 1
    );
    assert_eq!(pets_of(&fx, p).len(), 1);
    // Past the maximum the oldest pet goes.
    assert_eq!(run(&mut fx, p, summon_at(15, 2)), 2);
    let two = pets_of(&fx, p);
    assert_eq!(run(&mut fx, p, summon_at(16, 2)), 2);
    assert_eq!(pets_of(&fx, p)[0], two[1]);
    fx.assert_clean();
}

#[test]
fn add_pet_lists_a_unit_and_pet_count_reads_it() {
    let (mut fx, p, m) = pet_fx();
    let r = run(&mut fx, p, move |w, u| {
        let before = w.pet_count(u, 2);
        w.add_pet(u, m, 2, 3);
        before * 10 + w.pet_count(u, 2)
    });
    assert_eq!(r, 1);
    let guid = fx.sim.sys.units.get(m).unwrap().guid as i32;
    assert_eq!(pets_of(&fx, p), [guid]);
    // No list of that type, or a unit without lists: 0.
    let r = run(&mut fx, p, move |w, u| {
        w.pet_count(u, 7) + w.pet_count(m, 2)
    });
    assert_eq!(r, 0);
}

#[test]
fn pet_base_stats_levels_the_pet_from_its_owner() {
    let (mut fx, p, m) = pet_fx();
    run(&mut fx, p, move |w, u| w.pet_base_stats(u, m, 0, 1));
    // 1 + 10 × 3 / 4 = 8.
    assert_eq!(fx.sim.with(&mut fx.game, |_, v| v.stat(m, LEVEL)), 8);
    run(&mut fx, p, move |w, u| w.pet_base_stats(u, m, 4, 1));
    assert_eq!(fx.sim.with(&mut fx.game, |_, v| v.stat(m, LEVEL)), 4);
}

#[test]
fn set_pet_skill_gives_the_monster_its_skill() {
    let (mut fx, p, m) = pet_fx();
    run(&mut fx, p, move |w, _| {
        w.set_pet_skill(m, 0, 3);
        1
    });
    assert_eq!(
        fx.sim
            .sys
            .hooks
            .monster_skills
            .get(&m)
            .and_then(|s| s.get(&0)),
        Some(&3)
    );
}

#[test]
fn revivable_is_the_revive_test() {
    let (mut fx, p, m) = pet_fx();
    // A live monster is no corpse.
    let r = run(&mut fx, p, move |w, u| i32::from(w.revivable(u, m)));
    assert_eq!(r, 0);
    let t = fx.sim.sys.hooks.tables.clone();
    let direct = fx
        .sim
        .skill_use(&mut fx.game, |w| b3_lvl30::revive_test(w, &t.combat, m));
    assert!(!direct);
}

#[test]
fn start_dash_needs_a_path() {
    let (mut fx, p, _) = pet_fx();
    let has = fx
        .sim
        .skill_use(&mut fx.game, |w| BodyWorld::has_path(w, p));
    let r = run(&mut fx, p, |w, u| {
        i32::from(w.start_dash(u, (20, 10), 9, 5 << 8))
    });
    assert_eq!(r, i32::from(has));
}

// ---- world actions -------------------------------------------------------------------

#[test]
fn teleport_places_the_unit_on_the_rooms() {
    // The server turns the path provider on before any unit is allocated.
    let mut fx = Fx::with(super::stat_data(), skills());
    fx.sim.hooks().enable_paths().expect("embedded tables");
    let p = fx.spawn(UnitType::Player, 10, 10);
    let r = run(&mut fx, p, |w, u| i32::from(w.teleport(u, (20, 12))));
    assert_eq!(r, 1);
    assert_eq!(fx.sim.sys.hooks.path_position(p), (20, 12));
    // No room holds the point: not placed.
    let r = run(&mut fx, p, |w, u| i32::from(w.teleport(u, (5000, 5000))));
    assert_eq!(r, 0);
    assert_eq!(fx.sim.sys.hooks.path_position(p), (20, 12));
}

#[test]
fn cant_do_sends_the_message() {
    let (mut fx, p, _, _) = fx();
    run(&mut fx, p, |w, u| {
        w.cant_do(u);
        1
    });
    assert_eq!(fx.sim.sys.hooks.x.log, [format!("send {} CantDo", p.0)]);
}

#[test]
fn only_a_player_casts_a_town_portal() {
    let (mut fx, p, m, _) = fx();
    let r = run(&mut fx, p, move |w, _| i32::from(w.open_town_portal(m)));
    assert_eq!(r, 0);
}

#[test]
fn aura_pulse_puts_the_state_on_the_units_around() {
    const STATE: u16 = 30;
    let (mut fx, p, m) = pet_fx();
    {
        let at = Arc::make_mut(&mut fx.sim.sys.hooks.tables);
        at.combat.monstats2[0].isatt = true;
    }
    let r = run(&mut fx, p, |w, u| {
        let stats = [(ARMORCLASS, 50)];
        w.aura_pulse(
            u,
            &AuraPulse {
                skill: 0,
                level: 1,
                state: STATE,
                range: 5,
                frames: 25,
                filter: 0x3,
                stats: &stats,
            },
        )
    });
    // The caster and the monster two sub-tiles away; not the one at 30.
    assert_eq!(r, 2);
    for u in [p, m] {
        assert!(fx.sim.sys.stats.has_state(u, u32::from(STATE)));
    }
    assert!(fx.sim.with(&mut fx.game, |_, v| v.stat(m, ARMORCLASS)) >= 50);
}

#[test]
fn engine_body_runs_the_engine_slot_not_the_mod() {
    let (mut fx, p, _, _) = fx();
    // Slot 53 has no D2RUST body: it goes to the host, as the core sends
    // it; the mod's slot 200 is past the table.
    let r = run(&mut fx, p, |w, u| {
        let past = w.engine_body(SkillFn::Do, MOD_DO, u, 0, 1);
        let own = w.engine_body(SkillFn::Do, 53, u, 0, 1);
        i32::from(past.is_none()) + 10 * own.map_or(-1, |v| v + 1)
    });
    assert_eq!(r, 11);
    assert_eq!(fx.sim.sys.hooks.x.log, ["srvdo 53"]);
}

// ---- charge-up skills -------------------------------------------------------------

/// A mod's charge-up body: no charge starts one of max 3; each use adds
/// one up to the max; at the max the charge is released (dropped) and
/// the body returns 2, else 1.
fn charge_body(w: &mut dyn SkillHost, u: UnitId) -> i32 {
    let mut c = w.skill_charge(u, 7).unwrap_or(SkillCharge {
        charge: 0,
        max: 3,
        held: false,
    });
    if c.charge >= c.max {
        w.set_skill_charge(u, 7, None);
        return 2;
    }
    c.charge += 1;
    c.held = true;
    w.set_skill_charge(u, 7, Some(c));
    1
}

#[test]
fn a_charge_is_kept_per_unit_and_skill_until_dropped() {
    let (mut fx, p, m, _) = fx();
    for _ in 0..3 {
        assert_eq!(run(&mut fx, p, charge_body), 1);
    }
    let held = SkillCharge {
        charge: 3,
        max: 3,
        held: true,
    };
    assert_eq!(fx.sim.sys.hooks.skill_charges.get(&(p, 7)), Some(&held));
    // Another unit, another skill: no charge.
    let r = run(&mut fx, p, move |w, u| {
        i32::from(w.skill_charge(m, 7).is_none()) + i32::from(w.skill_charge(u, 8).is_none())
    });
    assert_eq!(r, 2);
    // Released at the max.
    assert_eq!(run(&mut fx, p, charge_body), 2);
    assert!(fx.sim.sys.hooks.skill_charges.is_empty());
}

#[test]
fn a_charge_goes_with_its_unit() {
    let (mut fx, p, m, _) = fx();
    run(&mut fx, m, charge_body);
    run(&mut fx, p, charge_body);
    assert_eq!(fx.sim.sys.hooks.skill_charges.len(), 2);
    fx.sim.with(&mut fx.game, |g, v| v.remove(g, m));
    assert_eq!(
        fx.sim
            .sys
            .hooks
            .skill_charges
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [(p, 7)]
    );
}

// ---- respec and resurrection ------------------------------------------------------

/// A mod game whose skill reset refunds the hard points of skill 0 to
/// stat 5 and removes it, ends its states and resyncs; its resurrection
/// hook sets stat 300.
#[derive(Debug)]
struct RespecMod;

impl SkillProvider for RespecMod {
    fn has(&self, _: SkillFn, _: u16) -> bool {
        false
    }
    fn respec(&self, w: &mut dyn SkillHost, player: UnitId) -> bool {
        let points = w.hard_points(player, 0);
        w.remove_skill(player, 0);
        let refund = w.stat(player, 5, 0) + points;
        w.set_stat(player, 5, refund);
        w.skills_changed(player);
        true
    }
    fn resurrected(&self, w: &mut dyn SkillHost, player: UnitId) {
        let n = w.skill_states(player).len() as i32;
        w.set_stat(player, 300, 1 + n);
    }
}

impl ModRules for RespecMod {
    fn name(&self) -> &str {
        "respec-mod"
    }
    fn skills(&self) -> Option<&dyn SkillProvider> {
        Some(self)
    }
}

/// The player's skill list: skill 0 at base 3.
fn give_skill(fx: &mut Fx, p: UnitId) {
    let rows = fx.sim.sys.hooks.tables.skills.skills.clone();
    let mut l = SkillList::default();
    l.set_base(&rows, ListOwner::player(0), 0, 3);
    fx.sim.sys.hooks.skill_lists.insert(p, l);
}

fn ask_respec(fx: &mut Fx, p: UnitId) -> bool {
    let sys = &mut fx.sim.sys;
    let mut sim = Sim {
        game: &mut fx.game,
        units: &mut sys.units,
        stats: &mut sys.stats,
        data: &sys.data,
    };
    sys.hooks.mod_respec(&mut sim, p)
}

fn tell_resurrected(fx: &mut Fx, p: UnitId) {
    let sys = &mut fx.sim.sys;
    let mut sim = Sim {
        game: &mut fx.game,
        units: &mut sys.units,
        stats: &mut sys.stats,
        data: &sys.data,
    };
    <Open as Pending>::mod_resurrected(&mut sys.hooks, &mut sim, p);
}

#[test]
fn hard_points_and_remove_skill_read_and_change_the_skill_list() {
    let (mut fx, p, _, _) = fx();
    give_skill(&mut fx, p);
    let r = run(&mut fx, p, |w, u| {
        let before = w.hard_points(u, 0);
        w.remove_skill(u, 0);
        before * 10 + w.hard_points(u, 0)
    });
    assert_eq!(r, 30);
    assert!(!fx.sim.sys.hooks.skill_lists[&p].has(0));
}

#[test]
fn skill_states_lists_the_states_a_skill_made() {
    let (mut fx, p, _, _) = fx();
    fx.sim.with(&mut fx.game, |_, v| {
        let l = v.create_state_list(p, 30, None, 0).unwrap();
        v.stats.set_skill(l, 6, 2);
        // A state list no skill made.
        v.create_state_list(p, 31, None, 0).unwrap();
    });
    let got = seen::<(u16, i32)>();
    let g = got.clone();
    run(&mut fx, p, move |w, u| {
        g.lock().unwrap().extend(w.skill_states(u));
        1
    });
    assert_eq!(*got.lock().unwrap(), [(30, 6)]);
}

#[test]
fn a_mod_respec_replaces_the_skill_reset() {
    let (mut fx, p, _, _) = fx();
    give_skill(&mut fx, p);
    fx.set(p, &[(5, 2)]);
    // D2RUST's own rules: not answered, nothing changed.
    assert!(!ask_respec(&mut fx, p));
    assert!(fx.sim.sys.hooks.skill_lists[&p].has(0));
    fx.sim.sys.hooks.rules = Ruleset::Mod(Arc::new(RespecMod));
    assert!(ask_respec(&mut fx, p));
    assert!(!fx.sim.sys.hooks.skill_lists[&p].has(0));
    assert_eq!(fx.sim.with(&mut fx.game, |_, v| v.stat(p, 5)), 5);
}

#[test]
fn a_mod_without_a_respec_keeps_the_skill_reset() {
    let (mut fx, p, _, _) = fx();
    give_skill(&mut fx, p);
    // A mod whose skill provider has no respec of its own.
    run(&mut fx, p, |_, _| 1);
    assert!(!ask_respec(&mut fx, p));
    assert!(fx.sim.sys.hooks.skill_lists[&p].has(0));
}

#[test]
fn a_mod_hears_of_a_resurrection() {
    let (mut fx, p, _, _) = fx();
    tell_resurrected(&mut fx, p);
    assert_eq!(fx.sim.with(&mut fx.game, |_, v| v.stat(p, 300)), 0);
    fx.sim.sys.hooks.rules = Ruleset::Mod(Arc::new(RespecMod));
    tell_resurrected(&mut fx, p);
    assert_eq!(fx.sim.with(&mut fx.game, |_, v| v.stat(p, 300)), 1);
}
