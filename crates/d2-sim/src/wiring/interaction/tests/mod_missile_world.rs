//! A mod game's missile bodies on the wired world: the
//! [`crate::missiles::MissileBodies`] calls its server-do and server-hit
//! bodies make reach the real unit filter, state lists, stats, formulas,
//! path facing, item casts and monster world. Under D2RUST's own rules
//! the calls D2RUST's bodies make keep their defaults.

use std::sync::{Arc, Mutex};

use d2_data::tables::Skills;

use crate::game::Game;
use crate::missiles::catalogue::run_srv_do;
use crate::missiles::{CastAt, CastRequest, Ctx, MissileBodies, MissileWorld};
use crate::rules::{MissileProvider, MissileSlot, ModRules, Ruleset};
use crate::skills::fake::skill_tables;
use crate::skills::use_::bodies::{self, BodyTables};
use crate::stats::states::StateTable;
use crate::stats::StatData;
use crate::units::hooks::Sim;
use crate::units::{RoomId, UnitId, UnitType};
use crate::wiring::action::monsters::MonsterWorld;
use crate::wiring::action::ActionHooks;

use super::skill_bodies::body_rec;
use super::skill_use::Fx;
use super::{stat_data, N_STATES};

/// The mod's server-do slot (past D2RUST's 1-52).
const MOD_DO: i16 = 60;
/// A state with no group, flags or callbacks.
const STATE: i32 = 40;
/// A buff skill with an item effect: srvdo 18, state 41 for 200 frames
/// with armorclass 25.
const BUFF: i32 = 1;
const BUFF_STATE: u16 = 41;
const ARMORCLASS: u16 = 31;
/// A layered stat (a skill-tab-like param).
const LAYERED: u16 = 188;
/// Formula offsets in the code buffer: `push i16 v; end`.
const F_200: u32 = 0;
const F_25: u32 = 4;
const F_77: u32 = 8;

fn code() -> Vec<u8> {
    let mut c = Vec::new();
    for v in [200i16, 25, 77] {
        c.push(0x08);
        c.extend(v.to_le_bytes());
        c.push(0x00);
    }
    c
}

/// What a body sees of the world, logged by the test's body.
type Body = fn(&mut Game, &mut Ctx<'_, dyn MissileWorld + '_>, UnitId, UnitId) -> Vec<i32>;

/// A mod game whose server-do `MOD_DO` runs `body` on (player, monster)
/// and logs what it answered; the body's last value is the result.
#[derive(Debug)]
struct ModBody {
    body: Body,
    pair: (UnitId, UnitId),
    log: Mutex<Vec<i32>>,
}

impl MissileProvider for ModBody {
    fn has(&self, slot: MissileSlot, index: i16) -> bool {
        slot == MissileSlot::Do && index == MOD_DO
    }
    fn srv_do(
        &self,
        game: &mut Game,
        cx: &mut Ctx<'_, dyn MissileWorld + '_>,
        index: i16,
        _m: UnitId,
    ) -> Option<i32> {
        if index != MOD_DO {
            return None;
        }
        let v = (self.body)(game, cx, self.pair.0, self.pair.1);
        let r = v.last().copied().unwrap_or(0);
        self.log.lock().unwrap().extend(v);
        Some(r)
    }
}

impl ModRules for ModBody {
    fn name(&self) -> &str {
        "mod-missile-world"
    }
    fn missiles(&self) -> Option<&dyn MissileProvider> {
        Some(self)
    }
}

fn tables() -> crate::skills::SkillTables {
    let mut v: Vec<Skills> = (0..2).map(|_| body_rec()).collect();
    let b = &mut v[BUFF as usize];
    b.srvdofunc = 18;
    b.itemeffect = 1;
    b.aurastate = BUFF_STATE;
    b.auralencalc = F_200;
    b.aurastat1 = ARMORCLASS;
    b.aurastatcalc1 = F_25;
    let mut t = skill_tables(v);
    t.skills_code = code();
    t
}

fn stats() -> Arc<StatData> {
    let mut d = (*stat_data()).clone();
    d.states = StateTable::synthetic(N_STATES, &[]);
    Arc::new(d)
}

/// The skill use fixture with a player at (10, 10) and a monster at
/// (12, 10) (targetable, flags 0x2 | 0x4 | 0x8).
fn fx_with(paths: bool) -> (Fx, UnitId, UnitId) {
    let mut fx = Fx::with(stats(), tables());
    let h = &mut fx.sim.sys.hooks;
    h.bodies = Some(Arc::new(BodyTables {
        stats: vec![bodies::BodyStat::default(); 359],
        state_group: vec![0; N_STATES],
        state_aura: vec![false; N_STATES],
        ..BodyTables::default()
    }));
    // States take on the monster class (monstats2 `isAtt`).
    Arc::make_mut(&mut h.tables).combat.monstats2[0].isatt = true;
    if paths {
        h.enable_paths().expect("embedded tables");
    }
    let p = fx.spawn(UnitType::Player, 10, 10);
    let m = fx.spawn(UnitType::Monster, 12, 10);
    fx.sim.sys.units.get_mut(m).unwrap().flags |= 0xE;
    (fx, p, m)
}

fn fx() -> (Fx, UnitId, UnitId) {
    fx_with(false)
}

/// Installs the mod with `body`.
fn install(fx: &mut Fx, pair: (UnitId, UnitId), body: Body) -> Arc<ModBody> {
    let m = Arc::new(ModBody {
        body,
        pair,
        log: Mutex::new(Vec::new()),
    });
    fx.sim.sys.hooks.rules = Ruleset::Mod(m.clone());
    m
}

/// Runs the mod's server-do through the missile catalogue, as the class
/// handler calls it; its result.
fn run(fx: &mut Fx, m: UnitId) -> i32 {
    fx.sim
        .missiles(&mut fx.game, |g, cx| run_srv_do(g, cx, MOD_DO, m))
        .expect("missile store")
}

fn has_state(fx: &Fx, u: UnitId, s: i32) -> bool {
    fx.sim.sys.stats.has_state(u, s as u32)
}

/// The world calls D2RUST's own bodies make, on `fx`'s view as a `dyn`
/// world: (accepts, applied, monstats count).
fn body_calls(fx: &mut Fx, p: UnitId, m: UnitId) -> (bool, bool, i32) {
    fx.sim
        .missiles(&mut fx.game, |g, cx| {
            let cx = cx.as_dyn();
            let w = cx.world;
            let a = w.accepts(g, p, m, 0x2);
            let s = w.apply_state(g, p, m, BUFF, 1, 100, STATE);
            (a, s, w.monstats_count())
        })
        .expect("missile store")
}

#[test]
fn original_rules_keep_the_defaults_of_the_calls_d2rust_bodies_make() {
    let (mut fx, p, m) = fx();
    assert_eq!(body_calls(&mut fx, p, m), (false, false, 0));
    assert!(!has_state(&fx, m, STATE));
    let units = fx.game.lists.units_of_type(UnitType::Object).len();
    fx.sim.missiles(&mut fx.game, |g, cx| {
        let room = g.lists.unit(m).and_then(|e| e.room());
        cx.world.create_portal(g, Some(p), room, (10, 10), 3, 59);
        cx.world.spawn_monster(g, room, 0, (11, 11), 1);
    });
    assert_eq!(fx.game.lists.units_of_type(UnitType::Object).len(), units);
    fx.assert_clean();
}

#[test]
fn a_mod_body_filters_units_and_puts_states() {
    let (mut fx, p, m) = fx();
    let body: Body = |g, cx, p, m| {
        let w = &mut *cx.world;
        let player_only = w.accepts(g, p, m, 0x1);
        let monsters = w.accepts(g, p, m, 0x2);
        let applied = w.apply_state(g, p, m, BUFF, 1, 100, STATE);
        w.set_state_stat(m, STATE, ARMORCLASS, 0, 25);
        vec![player_only as i32, monsters as i32, applied as i32, 1]
    };
    let modded = install(&mut fx, (p, m), body);
    assert_eq!(run(&mut fx, m), 1);
    assert_eq!(*modded.log.lock().unwrap(), [0, 1, 1, 1]);
    assert!(has_state(&fx, m, STATE));
    let st = fx.sim.with(&mut fx.game, |_, v| {
        v.state_stat(m, STATE as u16, ARMORCLASS)
    });
    assert_eq!(st, Some(25));
    // The calls D2RUST's own bodies make answer for real under the mod.
    let (a, _, _) = body_calls(&mut fx, p, m);
    assert!(a);
    fx.assert_clean();
}

#[test]
fn a_mod_body_removes_a_state_list_and_refreshes_the_unit() {
    let (mut fx, p, m) = fx();
    let body: Body = |g, cx, p, m| {
        let w = &mut *cx.world;
        let made = w.apply_state(g, p, m, BUFF, 1, 100, STATE);
        let listed = w.state_list_expiry(m, STATE).is_some();
        w.remove_state_list(g, m, STATE);
        let after = w.state_list_expiry(m, STATE).is_some();
        w.refresh_unit(g, m);
        vec![made as i32, listed as i32, after as i32, 1]
    };
    let modded = install(&mut fx, (p, m), body);
    let room = fx.game.lists.unit(m).and_then(|e| e.room()).unwrap();
    fx.game.lists.clear_update_queue(room).unwrap();
    assert_eq!(run(&mut fx, m), 1);
    assert_eq!(*modded.log.lock().unwrap(), [1, 1, 0, 1]);
    assert!(!has_state(&fx, m, STATE));
    assert!(fx.game.lists.update_queue(room).contains(&m));
    fx.assert_clean();
}

#[test]
fn a_mod_body_reads_and_writes_stat_layers_and_evaluates_any_formula() {
    let (mut fx, p, m) = fx();
    let body: Body = |g, cx, p, _| {
        let w = &mut *cx.world;
        w.set_layer_stat(p, LAYERED, 5, 3);
        vec![
            w.layer_base_stat(p, LAYERED, 5),
            w.layer_stat(p, LAYERED, 5),
            w.layer_stat(p, LAYERED, 0),
            w.eval_calc(g, Some(p), F_77, BUFF, 1),
            w.eval_calc(g, None, F_25, BUFF, 1),
        ]
    };
    let modded = install(&mut fx, (p, m), body);
    assert_eq!(run(&mut fx, m), 25);
    assert_eq!(*modded.log.lock().unwrap(), [3, 3, 0, 77, 25]);
    fx.assert_clean();
}

#[test]
fn a_mod_body_turns_a_unit() {
    let (mut fx, p, m) = fx_with(true);
    let body: Body = |_, cx, p, _| {
        let w = &mut *cx.world;
        w.set_direction(p, 64 + 20);
        vec![w.direction(p)]
    };
    let modded = install(&mut fx, (p, m), body);
    assert_eq!(run(&mut fx, m), 20);
    assert_eq!(*modded.log.lock().unwrap(), [20]);
}

#[test]
fn a_mod_body_casts_a_skill_as_an_item_event() {
    let (mut fx, p, m) = fx();
    let body: Body = |g, cx, p, _| {
        let w = &mut *cx.world;
        let none = CastRequest {
            skill: 0,
            level: 1,
            at: CastAt::Unit(p),
            aim: false,
        };
        let buff = CastRequest {
            skill: BUFF,
            ..none
        };
        vec![
            w.cast_skill(g, p, none) as i32,
            w.cast_skill(g, p, buff) as i32,
        ]
    };
    let modded = install(&mut fx, (p, m), body);
    assert_eq!(run(&mut fx, m), 1);
    assert_eq!(*modded.log.lock().unwrap(), [0, 1]);
    assert!(fx.sim.sys.stats.has_state(p, u32::from(BUFF_STATE)));
    fx.assert_clean();
}

/// A monster world that logs the creation calls and answers `made`.
#[derive(Default)]
struct Spawns {
    made: Option<UnitId>,
    log: Vec<String>,
}

impl<X> MonsterWorld<X> for Spawns {
    fn type_init(&mut self, _: &mut Sim<'_>, _: &mut ActionHooks<X>, _: UnitId) {}
    fn umods(
        &mut self,
        _: &mut Sim<'_>,
        _: &mut ActionHooks<X>,
        _: UnitId,
        _: Option<UnitId>,
        _: u8,
    ) {
    }
    fn assign_umod(&mut self, _: &mut Sim<'_>, _: &mut ActionHooks<X>, u: UnitId, umod: u8) {
        self.log.push(format!("umod {} {umod}", u.0));
    }
    fn forget(&mut self, _: UnitId) {}
    fn monster(&self, _: UnitId) -> Option<&crate::monsters::init::MonsterData> {
        None
    }
    fn monstats_count(&self) -> u32 {
        7
    }
    fn spawn_at(
        &mut self,
        _: &mut Sim<'_>,
        _: &mut ActionHooks<X>,
        _: RoomId,
        x: i32,
        y: i32,
        class: i32,
        mode: u8,
        spread: i32,
        flags: u16,
    ) -> Option<Option<UnitId>> {
        self.log
            .push(format!("at {x} {y} {class} {mode} {spread} {flags}"));
        Some(self.made)
    }
    fn spawn_preset(
        &mut self,
        _: &mut Sim<'_>,
        _: &mut ActionHooks<X>,
        _: RoomId,
        x: i32,
        y: i32,
        class: i32,
        mode: u8,
    ) -> Option<Option<UnitId>> {
        self.log.push(format!("preset {x} {y} {class} {mode}"));
        Some(self.made)
    }
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

#[test]
fn a_mod_body_spawns_monsters_and_superuniques_and_adds_umods() {
    let (mut fx, p, m) = fx();
    fx.sim.sys.hooks.monster_world = Some(Box::new(Spawns {
        made: Some(m),
        ..Spawns::default()
    }));
    let body: Body = |g, cx, _, m| {
        let w = &mut *cx.world;
        let room = g.lists.unit(m).and_then(|e| e.room());
        let a = w.spawn_monster_unit(g, room, 3, (11, 12), 8);
        w.spawn_monster(g, room, 4, (13, 14), 1);
        let b = w.spawn_superunique(g, room, 2, (15, 16));
        let none = w.spawn_monster_unit(g, None, 3, (11, 12), 8);
        w.add_umod(g, m, 9);
        let made = [a, b, none].map(|u| u.map_or(-1, |u| u.0 as i32));
        vec![made[0], made[1], made[2], w.monstats_count()]
    };
    let modded = install(&mut fx, (p, m), body);
    assert_eq!(run(&mut fx, m), 7);
    let id = m.0 as i32;
    assert_eq!(*modded.log.lock().unwrap(), [id, id, -1, 7]);
    let w = fx.sim.sys.hooks.monster_world.take().unwrap();
    let w = w.into_any().downcast::<Spawns>().unwrap();
    // The superunique is the preset class past the monstats rows.
    let rows = fx.sim.sys.hooks.tables.combat.monstats.len();
    assert_eq!(
        w.log,
        [
            "at 11 12 3 8 5 0".to_string(),
            "at 13 14 4 1 5 0".to_string(),
            format!("preset 15 16 {} 1", rows + 2),
            format!("umod {} 9", m.0),
        ]
    );
    fx.assert_clean();
}
