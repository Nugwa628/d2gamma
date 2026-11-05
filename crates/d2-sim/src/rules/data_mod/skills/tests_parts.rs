//! The grown part vocabulary (rings, arcs, volleys, chains, bounces,
//! teleports, raises, minion orders, charge-ups) run on a recording host:
//! what each part asks of the world, to the number.

use std::collections::BTreeMap;

use super::*;
use crate::combat::DamageRecord;
use crate::rng::Seed;
use crate::rules::{
    AuraPulse, BuffPulse, CursePulse, HostDamage, HostMissiles, HostSkills, HostStates, HostTimers,
    HostUnits, MissileRequest, RuledState, SkillActions, SkillBar, SkillFn, SkillPets, SkillRespec,
    SkillSearch, Tick,
};
use crate::skills::fake::{blank, skill_tables, FUnit, Fake};
use crate::skills::use_::bodies::{init_cb, ring_offset};
use crate::skills::{eval_desc, SkillTables};
use crate::units::UnitType;

const CASTER: UnitId = UnitId(1);

/// A state put on a hit target: the target, the state, its frames and its stats.
type Marked = (UnitId, u16, i32, Vec<(u16, i32)>);

/// A host that holds a few units and writes down what the parts do. Its
/// formulas run on the engine's own evaluation over a stand-in world.
struct Rec {
    world: Fake,
    tables: SkillTables,
    me: usize,
    seed: Seed,
    at: BTreeMap<UnitId, (i32, i32)>,
    dead: Vec<UnitId>,
    owned: Vec<(UnitId, UnitId)>,
    target: Option<UnitId>,
    aim: Option<(i32, i32)>,
    stacks: BTreeMap<u16, u16>,
    corpse_ok: bool,
    missiles: Vec<MissileRequest<UnitId>>,
    damage: Vec<(UnitId, i32)>,
    cleared: Vec<u16>,
    teleports: Vec<(UnitId, (i32, i32))>,
    casts: Vec<(UnitId, i32, i32, Option<UnitId>)>,
    bodies: Vec<(u16, i32, i32)>,
    body_result: Option<i32>,
    log: Vec<String>,
    /// The caster's states.
    on: Vec<u16>,
    /// The pending ticks: unit, frames asked, tick.
    ticks: Vec<(UnitId, i32, Tick)>,
    /// The host has no tick seam.
    no_ticks: bool,
    /// The `(skill, level, state, target state)` of each aura pulse.
    pulses: Vec<(i32, i32, u16, u16)>,
    /// Each curse the skills asked for.
    curses: Vec<CurseAsked>,
    /// The host has no point to curse around.
    curse_none: bool,
    /// The `(missile, skill, level)` of each drop on the aim point.
    drops: Vec<(i32, i32, i32)>,
    /// The `(target, percent)` of each heal.
    heals: Vec<(UnitId, i32)>,
    /// The attack mode of each monster damage setup.
    modes: Vec<i32>,
    /// The overlay of each resurrection asked for.
    raised: Vec<i32>,
    /// The overlay of each nest hatched.
    nests: Vec<i32>,
    /// The `(missile, centre, skill, level)` of each wall.
    walls: Vec<(i32, i32, i32, i32)>,
    stacks_added: u16,
    /// Teleports past this many fail.
    blocked_after: Option<usize>,
    /// The caster's run velocity.
    run: i32,
    /// Each time the caster went in (true) or out of the skill-move state.
    moves: Vec<bool>,
    /// The melee swing lands.
    lands: bool,
    /// The `(life, mana)` leech of each combat started.
    leeches: Vec<(i32, i32)>,
    /// The `(unit, overlay)` of each overlay played.
    overlays: Vec<(UnitId, i32)>,
    /// The `(target, state, frames, stats)` of each state put on a hit target.
    marked: Vec<Marked>,
}

impl Rec {
    /// The caster at (0, 0).
    fn new() -> Rec {
        let mut world = Fake::default();
        let me = world.add(FUnit::new(UnitType::Player, 0));
        let mut r = Rec {
            world,
            tables: skill_tables(vec![blank()]),
            me,
            seed: Seed::init(),
            at: BTreeMap::new(),
            dead: Vec::new(),
            owned: Vec::new(),
            target: None,
            aim: None,
            stacks: BTreeMap::new(),
            corpse_ok: false,
            missiles: Vec::new(),
            damage: Vec::new(),
            cleared: Vec::new(),
            teleports: Vec::new(),
            casts: Vec::new(),
            bodies: Vec::new(),
            body_result: Some(1),
            log: Vec::new(),
            on: Vec::new(),
            ticks: Vec::new(),
            no_ticks: false,
            pulses: Vec::new(),
            curses: Vec::new(),
            curse_none: false,
            drops: Vec::new(),
            heals: Vec::new(),
            modes: Vec::new(),
            walls: Vec::new(),
            raised: Vec::new(),
            nests: Vec::new(),
            stacks_added: 0,
            blocked_after: None,
            run: 0,
            moves: Vec::new(),
            lands: false,
            leeches: Vec::new(),
            overlays: Vec::new(),
            marked: Vec::new(),
        };
        r.at.insert(CASTER, (0, 0));
        r
    }

    fn unit(mut self, id: u32, at: (i32, i32)) -> Rec {
        self.at.insert(UnitId(id), at);
        self
    }

    fn targets(&self) -> Vec<(i32, i32)> {
        self.missiles
            .iter()
            .map(|m| (m.target_x, m.target_y))
            .collect()
    }
}

impl HostUnits for Rec {
    fn position(&self, u: UnitId) -> (i32, i32) {
        self.at.get(&u).copied().unwrap_or((0, 0))
    }
    fn target(&self, _u: UnitId) -> Option<UnitId> {
        self.target
    }
    fn target_position(&self, _u: UnitId) -> Option<(i32, i32)> {
        self.aim
    }
    fn is_dead(&self, u: UnitId) -> bool {
        self.dead.contains(&u)
    }
    fn owner(&self, u: UnitId) -> Option<UnitId> {
        self.owned.iter().find(|(m, _)| *m == u).map(|(_, o)| *o)
    }
}

impl HostSkills for Rec {
    fn eval_formula(&mut self, _u: UnitId, code: &[u8], skill: i32, lvl: i32) -> i32 {
        let (world, tables) = (&mut self.world, &self.tables);
        eval_desc(world, tables, code, Some(self.me), 0, skill, lvl)
    }
    fn cast_skill(&mut self, u: UnitId, skill: i32, lvl: i32, target: Option<UnitId>) -> bool {
        self.casts.push((u, skill, lvl, target));
        true
    }
}

impl HostStates for Rec {
    fn state_stacks(&self, _u: UnitId, state: u16) -> u16 {
        self.stacks.get(&state).copied().unwrap_or(0)
    }
    fn clear_state(&mut self, _u: UnitId, state: u16) {
        self.cleared.push(state);
    }
    fn add_state_stacks(
        &mut self,
        _source: UnitId,
        _target: UnitId,
        _spec: &RuledState,
        count: u16,
    ) -> u16 {
        self.stacks_added += count;
        self.stacks_added
    }
}

impl HostMissiles for Rec {
    fn drop_skill_missile(
        &mut self,
        _u: UnitId,
        missile: i32,
        skill: i32,
        lvl: i32,
    ) -> Option<UnitId> {
        self.drops.push((missile, skill, lvl));
        self.aim.map(|_| UnitId(200))
    }
    fn spawn_missile(&mut self, req: MissileRequest<UnitId>) -> Option<UnitId> {
        self.missiles.push(req);
        Some(UnitId(100 + self.missiles.len() as u32))
    }
}

impl HostDamage for Rec {
    fn melee_result(&mut self, _a: UnitId, _d: UnitId, _bonus: i32, _range: i32) -> u16 {
        u16::from(self.lands)
    }
    fn start_combat(&mut self, _a: UnitId, _d: UnitId, rec: &mut DamageRecord, _src_dam: i32) {
        self.leeches.push((rec.life_leech, rec.mana_leech));
    }
    fn apply_damage(&mut self, _a: UnitId, d: UnitId, _missile: bool, rec: &mut DamageRecord) {
        self.damage.push((d, rec.physical));
    }
}

impl HostTimers for Rec {
    fn schedule_tick(&mut self, u: UnitId, frames: i32, tick: Tick) -> bool {
        if self.no_ticks {
            return false;
        }
        self.ticks.push((u, frames, tick));
        true
    }
    fn cancel_ticks(&mut self, u: UnitId, skill: Option<i32>) {
        self.ticks
            .retain(|(o, _, t)| *o != u || skill.is_some_and(|s| t.skill != s));
    }
}

impl SkillBar for Rec {}

impl SkillRespec for Rec {}

impl SkillActions for Rec {
    fn unit_overlay(&mut self, u: UnitId, id: i32) {
        self.overlays.push((u, id));
    }
    fn warcry_state(&mut self, _s: UnitId, target: UnitId, pulse: &BuffPulse<'_>) -> bool {
        self.marked
            .push((target, pulse.state, pulse.frames, pulse.stats.to_vec()));
        true
    }
    fn teleport(&mut self, u: UnitId, to: (i32, i32)) -> bool {
        self.teleports.push((u, to));
        let placed = self.blocked_after.is_none_or(|n| self.teleports.len() <= n);
        if placed {
            self.at.insert(u, to);
        }
        placed
    }
    fn run_speed(&self, _u: UnitId) -> i32 {
        self.run
    }
    fn in_reach(&mut self, u: UnitId, target: UnitId) -> bool {
        let (a, b) = (self.position(u), self.position(target));
        (a.0 - b.0).abs() + (a.1 - b.1).abs() <= 2
    }
    fn skill_move(&mut self, _u: UnitId, on: bool) {
        self.moves.push(on);
    }
    fn aura_pulse(&mut self, _u: UnitId, pulse: &AuraPulse<'_>) -> i32 {
        self.pulses
            .push((pulse.skill, pulse.level, pulse.state, pulse.target_state));
        1
    }
    fn nest(&mut self, _u: UnitId, overlay: i32) -> i32 {
        self.nests.push(overlay);
        1
    }
    fn resurrect(&mut self, _u: UnitId, overlay: i32) -> i32 {
        self.raised.push(overlay);
        i32::from(self.target.is_some())
    }
    fn lay_wall(&mut self, _u: UnitId, missile: i32, centre: i32, skill: i32, lvl: i32) -> i32 {
        self.walls.push((missile, centre, skill, lvl));
        i32::from(self.aim.is_some())
    }
    fn mode_damage(&mut self, _u: UnitId, mode: i32) {
        self.modes.push(mode);
    }
    fn heal(&mut self, _u: UnitId, target: UnitId, percent: i32) -> bool {
        self.heals.push((target, percent));
        true
    }
    fn curse_pulse(&mut self, _u: UnitId, pulse: &CursePulse<'_>) -> i32 {
        self.curses.push(CurseAsked {
            level: pulse.level,
            state: pulse.state,
            range: pulse.range,
            frames: pulse.frames,
            filter: pulse.filter,
            stats: pulse.stats.to_vec(),
        });
        i32::from(!self.curse_none)
    }
    fn engine_body(
        &mut self,
        kind: SkillFn,
        index: u16,
        _u: UnitId,
        skill: i32,
        level: i32,
    ) -> Option<i32> {
        assert_eq!(kind, SkillFn::Do);
        self.bodies.push((index, skill, level));
        self.body_result
    }
}

impl SkillSearch for Rec {
    fn units_near(&self, _near: UnitId, at: (i32, i32), radius: i32) -> Vec<UnitId> {
        let mut found: Vec<UnitId> = self
            .at
            .iter()
            .filter(|(_, p)| {
                let (dx, dy) = (i64::from(p.0 - at.0), i64::from(p.1 - at.1));
                dx * dx + dy * dy <= i64::from(radius) * i64::from(radius)
            })
            .map(|(u, _)| *u)
            .collect();
        found.sort();
        found
    }
    fn passes_filter(&mut self, _source: UnitId, _target: UnitId, _filter: u32) -> bool {
        true
    }
}

impl SkillPets for Rec {
    fn revivable(&mut self, _caster: UnitId, _corpse: UnitId) -> bool {
        self.corpse_ok
    }
    fn revive_prepare(&mut self, _caster: UnitId, corpse: UnitId) {
        self.log.push(format!("prepare {}", corpse.0));
    }
    fn revive_finish(&mut self, owner: UnitId, pet: UnitId) {
        self.log.push(format!("finish {} {}", owner.0, pet.0));
    }
    fn pet_skill_stats(&mut self, owner: UnitId, pet: UnitId, skill: i32, level: i32) {
        self.log
            .push(format!("stats {} {} {skill} {level}", owner.0, pet.0));
    }
    fn add_pet(&mut self, owner: UnitId, pet: UnitId, pet_type: i32, pet_max: i32) {
        self.log
            .push(format!("add {} {} {pet_type} {pet_max}", owner.0, pet.0));
    }
    fn follow_owner(&mut self, owner: UnitId, pet: UnitId) {
        self.log.push(format!("follow {} {}", owner.0, pet.0));
    }
}

impl SkillHost for Rec {
    fn frame(&self) -> i32 {
        0
    }
    fn class_id(&self, _u: UnitId) -> i32 {
        0
    }
    fn stat(&self, _u: UnitId, _stat: u16, _layer: u16) -> i32 {
        0
    }
    fn base_stat(&self, _u: UnitId, _stat: u16, _layer: u16) -> i32 {
        0
    }
    fn has_state(&self, _u: UnitId, state: u16) -> bool {
        self.on.contains(&state)
    }
    fn set_stat(&mut self, _u: UnitId, _stat: u16, _value: i32) {}
    fn seed(&mut self, _u: UnitId) -> &mut Seed {
        &mut self.seed
    }
}

/// Casts the one skill of `toml` at `lvl`: the do core's answer.
fn cast(w: &mut Rec, toml: &str, lvl: i32) -> i32 {
    let rules = SkillRules::from_toml(toml).expect("reads");
    rules
        .srvdo(w, BLOCK_SLOT, CASTER, 0, lvl, false, false, false)
        .expect("the block slot")
}

/// The message of the setting error `text` is refused with.
fn setting(text: &str) -> String {
    match SkillRules::from_toml(text) {
        Err(SkillsError::Setting(s)) => s,
        Err(SkillsError::Toml(e)) => e.to_string(),
        Ok(_) => panic!("accepted"),
    }
}

fn skill(part: &str) -> String {
    format!("[[skill]]\nid = 0\n[[skill.part]]\n{part}")
}

#[test]
fn a_ring_spaces_its_missiles_evenly_on_the_engines_ring() {
    let mut w = Rec::new();
    let toml = skill("kind = \"missile_ring\"\nmissile = 9\ncount = 8\n");
    assert_eq!(cast(&mut w, &toml, 1), 1);
    assert_eq!(w.missiles.len(), 8);
    for (i, m) in w.missiles.iter().enumerate() {
        assert_eq!((m.target_x, m.target_y), ring_offset(i * 8));
        assert_eq!((m.flags, m.class, m.owner), (3, 9, CASTER));
        assert_eq!((m.x, m.y), (0, 0));
    }
}

#[test]
fn a_ring_of_sixty_four_is_the_engines_nova() {
    let mut w = Rec::new();
    let toml = skill("kind = \"missile_ring\"\nmissile = 9\ncount = 64\nvelocity = \"10 + lvl\"\n");
    cast(&mut w, &toml, 2);
    assert_eq!(w.missiles.len(), 64);
    for (i, m) in w.missiles.iter().enumerate() {
        assert_eq!((m.target_x, m.target_y), ring_offset(i));
        // A velocity is the engine's flag 4 with the value.
        assert_eq!((m.flags, m.velocity, m.level), (7, 12, 2));
    }
    // More than the ring holds stops at the ring.
    let mut w = Rec::new();
    cast(
        &mut w,
        &skill("kind = \"missile_ring\"\nmissile = 9\ncount = 500\n"),
        1,
    );
    assert_eq!(w.missiles.len(), 64);
}

#[test]
fn a_ring_can_be_around_the_aim_point() {
    let mut w = Rec::new();
    w.aim = Some((40, 20));
    let toml = skill("kind = \"missile_ring\"\nmissile = 9\ncount = 4\naround = \"target\"\n");
    cast(&mut w, &toml, 1);
    assert_eq!(w.missiles.len(), 4);
    assert!(w.missiles.iter().all(|m| (m.x, m.y) == (40, 20)));
}

#[test]
fn an_arc_is_centred_on_the_way_to_the_aim_point() {
    let toml = skill("kind = \"missile_arc\"\nmissile = 9\ncount = 3\nspan = 16\n");
    let mut w = Rec::new();
    w.aim = Some((100, 0));
    assert_eq!(cast(&mut w, &toml, 1), 1);
    // Step 0 of the ring points along +x; the arc runs 8 steps either side.
    assert_eq!(
        w.targets(),
        [ring_offset(56), ring_offset(0), ring_offset(8)]
    );

    // Aimed the other way, the arc turns half a circle.
    let mut w = Rec::new();
    w.aim = Some((-100, 0));
    cast(&mut w, &toml, 1);
    assert_eq!(
        w.targets(),
        [ring_offset(24), ring_offset(32), ring_offset(40)]
    );

    // One missile goes straight.
    let mut w = Rec::new();
    w.aim = Some((100, 0));
    let one = skill("kind = \"missile_arc\"\nmissile = 9\ncount = 1\n");
    cast(&mut w, &one, 1);
    assert_eq!(w.targets(), [ring_offset(0)]);
}

#[test]
fn a_volley_fires_at_the_aim_point_with_the_engines_spread() {
    let mut w = Rec::new();
    w.aim = Some((30, 40));
    let toml = skill("kind = \"missile_volley\"\nmissile = 9\ncount = \"lvl\"\n");
    assert_eq!(cast(&mut w, &toml, 3), 1);
    assert_eq!(w.missiles.len(), 3);
    for (i, m) in w.missiles.iter().enumerate() {
        assert_eq!(m.flags, 0x21);
        assert_eq!((m.target_x, m.target_y), (30, 40));
        assert_eq!(m.init, Some((init_cb::JITTER, i as u32)));
    }
}

const HIT: &str = "damage = { min = 100, max = 100 }\n";

/// Units 2, 3 and 4 in a row a few sub-tiles apart, 5 far off; the caster
/// is out of everyone's reach; unit 2 is the target.
fn chain_world() -> Rec {
    let mut w = Rec::new()
        .unit(2, (0, 0))
        .unit(3, (5, 0))
        .unit(4, (9, 0))
        .unit(5, (200, 0));
    w.at.insert(CASTER, (900, 900));
    w.target = Some(UnitId(2));
    w
}

#[test]
fn a_chain_jumps_to_the_nearest_unit_not_yet_hit_and_loses_damage() {
    let mut w = chain_world();
    let toml = skill(&format!(
        "kind = \"chain\"\nradius = 6\njumps = 5\nfalloff = 25\n{HIT}"
    ));
    assert_eq!(cast(&mut w, &toml, 1), 1);
    // 2, then 3, then 4; nobody else is within 6 of 4.
    assert_eq!(
        w.damage,
        [
            (UnitId(2), 100 * 256),
            (UnitId(3), 75 * 256),
            (UnitId(4), 50 * 256)
        ]
    );
}

#[test]
fn a_chain_skips_the_dead_and_needs_a_target() {
    let mut w = chain_world();
    w.dead.push(UnitId(3));
    let toml = skill(&format!("kind = \"chain\"\nradius = 10\njumps = 5\n{HIT}"));
    cast(&mut w, &toml, 1);
    let hits: Vec<_> = w.damage.iter().map(|(u, _)| u.0).collect();
    assert_eq!(hits, [2, 4]);

    let mut w = chain_world();
    w.target = None;
    assert_eq!(cast(&mut w, &toml, 1), 0);
    assert!(w.damage.is_empty());
}

#[test]
fn chains_and_bounces_are_bounded_by_their_jumps() {
    let mut w = chain_world();
    let toml = skill(&format!("kind = \"chain\"\nradius = 50\njumps = 1\n{HIT}"));
    cast(&mut w, &toml, 1);
    assert_eq!(w.damage.len(), 2);

    let mut w = chain_world();
    let toml = skill(&format!(
        "kind = \"bounce\"\nradius = 50\njumps = 1000\n{HIT}"
    ));
    cast(&mut w, &toml, 1);
    assert_eq!(w.damage.len(), MAX_JUMPS as usize + 1);
}

#[test]
fn a_bounce_returns_to_units_it_hit_but_never_twice_in_a_row() {
    let mut w = Rec::new().unit(2, (0, 0)).unit(3, (5, 0));
    w.at.insert(CASTER, (900, 900));
    w.target = Some(UnitId(2));
    let toml = skill(&format!(
        "kind = \"bounce\"\nradius = 6\njumps = 3\nfalloff = 10\n{HIT}"
    ));
    cast(&mut w, &toml, 1);
    let hits: Vec<_> = w.damage.iter().map(|(u, d)| (u.0, d / 256)).collect();
    assert_eq!(hits, [(2, 100), (3, 90), (2, 80), (3, 70)]);
}

#[test]
fn a_charge_raises_damage_by_the_stacks_and_can_spend_them() {
    let area = |charge: &str| {
        skill(&format!(
            "kind = \"area_hit\"\nradius = 10\naround = \"caster\"\n\
             damage = {{ min = 100, max = 100, charge = {charge} }}\n"
        ))
    };
    let mut w = Rec::new().unit(2, (3, 0));
    w.stacks.insert(7, 3);
    cast(&mut w, &area("{ state = 7, percent = 10 }"), 1);
    // 100 plus 3 stacks of 10%, on both units in the area.
    assert_eq!(w.damage.len(), 2);
    assert!(w.damage.iter().all(|(_, d)| *d == 130 * 256));
    assert!(w.cleared.is_empty());

    let spend = area("{ state = 7, percent = \"lvl * 5\", consume = true }");
    let mut w = Rec::new().unit(2, (3, 0));
    w.stacks.insert(7, 2);
    cast(&mut w, &spend, 2);
    // 2 stacks of 10%; the stacks are spent once, after the part.
    assert_eq!(w.damage.len(), 2);
    assert!(w.damage.iter().all(|(_, d)| *d == 120 * 256));
    assert_eq!(w.cleared, [7]);

    // No stacks, no bonus.
    let mut w = Rec::new().unit(2, (3, 0));
    cast(&mut w, &spend, 2);
    assert!(w.damage.iter().all(|(_, d)| *d == 100 * 256));
}

#[test]
fn a_teleport_goes_to_the_aim_point_and_no_farther_than_its_range() {
    let mut w = Rec::new();
    w.aim = Some((300, 400));
    assert_eq!(cast(&mut w, &skill("kind = \"teleport\"\n"), 1), 1);
    assert_eq!(w.teleports, [(CASTER, (300, 400))]);

    let mut w = Rec::new();
    w.aim = Some((300, 400));
    let limited = skill("kind = \"teleport\"\nmax_range = \"50 * lvl\"\n");
    cast(&mut w, &limited, 2);
    assert_eq!(w.teleports, [(CASTER, (60, 80))]);

    // Within range it goes all the way.
    let mut w = Rec::new();
    w.aim = Some((30, 40));
    cast(&mut w, &limited, 2);
    assert_eq!(w.teleports, [(CASTER, (30, 40))]);
}

#[test]
fn a_raise_binds_the_corpse_in_the_engines_order_and_needs_a_corpse() {
    let toml = skill("kind = \"raise\"\npet_type = 4\npet_max = \"lvl + 1\"\n");
    let mut w = Rec::new();
    w.target = Some(UnitId(2));
    // Not a raisable corpse: nothing happens, the cast is not spent.
    assert_eq!(cast(&mut w, &toml, 3), 0);
    assert!(w.log.is_empty());

    w.corpse_ok = true;
    assert_eq!(cast(&mut w, &toml, 3), 1);
    assert_eq!(
        w.log,
        ["prepare 2", "stats 1 2 0 3", "finish 1 2", "add 1 2 4 4"]
    );

    let mut w = Rec::new();
    w.corpse_ok = true;
    assert_eq!(cast(&mut w, &toml, 1), 0, "no target");
}

#[test]
fn minion_orders_reach_only_the_casters_living_minions() {
    let mut w = Rec::new()
        .unit(2, (14, 10))
        .unit(3, (16, 10))
        .unit(4, (12, 10))
        .unit(5, (300, 10))
        .unit(6, (11, 11));
    w.at.insert(CASTER, (10, 10));
    w.owned = vec![
        (UnitId(2), CASTER),
        (UnitId(3), CASTER),
        (UnitId(4), UnitId(9)),
        (UnitId(5), CASTER),
    ];
    w.dead.push(UnitId(3));
    w.target = Some(UnitId(6));

    let recall = skill("kind = \"minion_order\"\norder = \"recall\"\nradius = 20\n");
    assert_eq!(cast(&mut w, &recall, 1), 1);
    // Unit 2 only: 3 is dead, 4 is someone else's, 5 is out of reach.
    assert_eq!(w.teleports, [(UnitId(2), (10, 10))]);

    let attack = skill("kind = \"minion_order\"\norder = \"attack\"\nradius = 20\nskill = 5\n");
    cast(&mut w, &attack, 4);
    assert_eq!(w.casts, [(UnitId(2), 5, 4, Some(UnitId(6)))]);

    let stronger = skill(
        "kind = \"minion_order\"\norder = \"attack\"\nradius = 20\nskill = 5\nlevel = \"lvl * 2\"\n",
    );
    cast(&mut w, &stronger, 4);
    assert_eq!(w.casts[1], (UnitId(2), 5, 8, Some(UnitId(6))));

    // Nothing to attack: the order is not given.
    w.target = None;
    assert_eq!(cast(&mut w, &attack, 4), 0);
    assert_eq!(w.casts.len(), 2);

    let follow = skill("kind = \"minion_order\"\norder = \"follow\"\nradius = 20\n");
    assert_eq!(cast(&mut w, &follow, 1), 1);
    assert_eq!(w.log, ["follow 1 2"]);
}

#[test]
fn an_attack_order_needs_the_skill_the_minions_cast() {
    let text = skill("kind = \"minion_order\"\norder = \"attack\"\nradius = 20\n");
    assert!(setting(&text).contains("skill[0].part[0].skill"));
}

#[test]
fn the_new_parts_refuse_unknown_and_missing_keys() {
    for bad in [
        "kind = \"missile_ring\"\nmissile = 1\ncounts = 4\n",
        "kind = \"chain\"\nradius = 4\n",
        "kind = \"raise\"\n",
        "kind = \"teleport\"\nrange = 4\n",
    ] {
        assert!(setting(&skill(bad)).contains("skill[0].part[0]"));
    }
}

#[test]
fn an_engine_body_runs_the_engines_slot_for_this_skill_and_level() {
    let mut w = Rec::new();
    let text = skill(
        "kind = \"engine_body\"
slot = 22
",
    );
    assert_eq!(cast(&mut w, &text, 5), 1);
    assert_eq!(w.bodies, [(22, 0, 5)]);

    // The row and level can be another's.
    let text = skill(
        "kind = \"engine_body\"
slot = 1
skill = 7
level = \"lvl + 2\"
",
    );
    cast(&mut w, &text, 5);
    assert_eq!(w.bodies[1], (1, 7, 7));
}

#[test]
fn an_engine_body_that_does_nothing_leaves_the_cast_unspent() {
    let mut w = Rec::new();
    let text = skill(
        "kind = \"engine_body\"
slot = 1
",
    );
    w.body_result = Some(0);
    assert_eq!(cast(&mut w, &text, 1), 0);
    w.body_result = None;
    assert_eq!(cast(&mut w, &text, 1), 0);
    // Parts around it still run.
    let both = format!(
        "{text}[[skill.part]]
kind = \"teleport\"
"
    );
    assert_eq!(cast(&mut w, &both, 1), 1);
}

#[test]
fn an_engine_body_slot_must_be_one_the_engine_has() {
    for slot in [0, 191, 400, 9000] {
        let text = skill(&format!(
            "kind = \"engine_body\"
slot = {slot}
"
        ));
        assert!(setting(&text).contains("skill[0].part[0].slot"), "{slot}");
    }
    assert!(SkillRules::from_toml(&skill(
        "kind = \"engine_body\"
slot = 190
"
    ))
    .is_ok());
}

// ---- ticks: repeat and leap ----------------------------------------------

/// Fires the soonest pending tick of the caster as the provider's `tick`;
/// the frame it was due, `None` when none is pending.
fn fire(w: &mut Rec, rules: &SkillRules) -> Option<i32> {
    let pos = (0..w.ticks.len()).min_by_key(|&i| w.ticks[i].1)?;
    let (u, due, tick) = w.ticks.remove(pos);
    rules.tick(w, u, &tick);
    Some(due)
}

const CHANNEL: &str = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"repeat\"\nskill = 1\n\
                       every = 10\ntimes = 3\n[[skill]]\nid = 1\n[[skill.part]]\n\
                       kind = \"missile_ring\"\nmissile = 9\ncount = 1\n";

fn channel_with(options: &str) -> SkillRules {
    SkillRules::from_toml(&CHANNEL.replace("times = 3\n", &format!("times = 3\n{options}")))
        .unwrap()
}

fn cast_rules(w: &mut Rec, rules: &SkillRules, lvl: i32) -> i32 {
    rules
        .srvdo(w, BLOCK_SLOT, CASTER, 0, lvl, false, false, false)
        .expect("the block slot")
}

#[test]
fn a_repeat_runs_the_other_skill_now_and_on_each_tick_as_this_skill() {
    let rules = SkillRules::from_toml(CHANNEL).unwrap();
    let mut w = Rec::new();
    assert_eq!(cast_rules(&mut w, &rules, 4), 1);
    // One run at once, the next two waiting.
    assert_eq!(w.missiles.len(), 1);
    assert_eq!(w.ticks.len(), 1);
    assert_eq!(w.ticks[0].1, 10);
    assert_eq!((w.ticks[0].2.left, w.ticks[0].2.of), (2, 3));
    // The repeated skill runs as the skill that repeats it, at its level.
    assert_eq!((w.missiles[0].skill, w.missiles[0].level), (0, 4));
    assert_eq!(fire(&mut w, &rules), Some(10));
    assert_eq!(w.missiles.len(), 2);
    assert_eq!(fire(&mut w, &rules), Some(10));
    assert_eq!(w.missiles.len(), 3);
    // The last run schedules nothing.
    assert_eq!(fire(&mut w, &rules), None);
    assert!(w.missiles.iter().all(|m| (m.skill, m.level) == (0, 4)));
}

/// A channel with no other skill: the part after the repeat is what repeats.
const CHANNEL_TAIL: &str = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"repeat\"\nevery = 10\n\
                            times = 3\n[[skill.part]]\nkind = \"missile_ring\"\nmissile = 9\n\
                            count = 1\n";

#[test]
fn a_repeat_with_no_skill_repeats_the_parts_after_it_and_not_at_the_cast_alone() {
    let rules = SkillRules::from_toml(CHANNEL_TAIL).unwrap();
    rules.check_ticks().unwrap();
    let mut w = Rec::new();
    assert_eq!(cast_rules(&mut w, &rules, 4), 1);
    // One run at once (the part after the repeat, not twice), two waiting.
    assert_eq!(w.missiles.len(), 1);
    assert_eq!(
        (w.ticks[0].1, w.ticks[0].2.left, w.ticks[0].2.of),
        (10, 2, 3)
    );
    assert_eq!((w.missiles[0].skill, w.missiles[0].level), (0, 4));
    while fire(&mut w, &rules).is_some() {}
    assert_eq!(w.missiles.len(), 3);
    // With a wait first, the cast itself runs nothing.
    let later =
        SkillRules::from_toml(&CHANNEL_TAIL.replace("times = 3\n", "times = 3\nnow = false\n"))
            .unwrap();
    let mut w = Rec::new();
    assert_eq!(cast_rules(&mut w, &later, 1), 1);
    assert!(w.missiles.is_empty());
    // A part before the repeat runs once, at the cast.
    let before = format!(
        "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"missile_ring\"\nmissile = 8\ncount = 1\n{}",
        CHANNEL_TAIL.trim_start_matches("[[skill]]\nid = 0\n")
    );
    let rules = SkillRules::from_toml(&before).unwrap();
    let mut w = Rec::new();
    cast_rules(&mut w, &rules, 1);
    while fire(&mut w, &rules).is_some() {}
    assert_eq!(w.missiles.iter().filter(|m| m.class == 8).count(), 1);
    assert_eq!(w.missiles.iter().filter(|m| m.class == 9).count(), 3);
}

#[test]
fn a_repeat_with_no_skill_needs_plain_parts_after_it() {
    let check = |toml: &str| {
        SkillRules::from_toml(toml)
            .unwrap()
            .check_ticks()
            .map_err(|e| e.to_string())
    };
    let none = check("[[skill]]\nid = 0\n[[skill.part]]\nkind = \"repeat\"\n").unwrap_err();
    assert!(none.contains("no part after"), "{none}");
    let nested = check(&format!(
        "{CHANNEL_TAIL}[[skill.part]]\nkind = \"repeat\"\nskill = 1\n"
    ))
    .unwrap_err();
    assert!(nested.contains("cannot repeat, leap or rush"), "{nested}");
}

#[test]
fn a_repeat_can_wait_one_period_before_its_first_run() {
    let rules = channel_with("now = false\n");
    let mut w = Rec::new();
    assert_eq!(cast_rules(&mut w, &rules, 1), 1);
    assert!(w.missiles.is_empty());
    assert_eq!(w.ticks[0].2.left, 3);
    while fire(&mut w, &rules).is_some() {}
    assert_eq!(w.missiles.len(), 3);
}

#[test]
fn a_new_cast_ends_the_repeats_before_it() {
    let rules = SkillRules::from_toml(CHANNEL).unwrap();
    let mut w = Rec::new();
    cast_rules(&mut w, &rules, 1);
    cast_rules(&mut w, &rules, 1);
    // The first cast's tick is gone; the second's is the only one.
    assert_eq!(w.ticks.len(), 1);
    assert_eq!(w.missiles.len(), 2);
    while fire(&mut w, &rules).is_some() {}
    assert_eq!(w.missiles.len(), 4);
}

#[test]
fn a_channel_stops_when_the_caster_moves_dies_or_loses_the_state() {
    let moving = channel_with("stop_on_move = true\n");
    let mut w = Rec::new();
    cast_rules(&mut w, &moving, 1);
    w.at.insert(CASTER, (3, 0));
    fire(&mut w, &moving);
    assert_eq!(w.missiles.len(), 1);
    assert!(w.ticks.is_empty());
    // Standing still it goes on.
    let mut w = Rec::new();
    cast_rules(&mut w, &moving, 1);
    while fire(&mut w, &moving).is_some() {}
    assert_eq!(w.missiles.len(), 3);

    let held = channel_with("while_state = 7\n");
    let mut w = Rec::new();
    w.on.push(7);
    cast_rules(&mut w, &held, 1);
    fire(&mut w, &held);
    w.on.clear();
    fire(&mut w, &held);
    assert_eq!(w.missiles.len(), 2);
    assert!(w.ticks.is_empty());

    let rules = SkillRules::from_toml(CHANNEL).unwrap();
    let mut w = Rec::new();
    cast_rules(&mut w, &rules, 1);
    w.dead.push(CASTER);
    fire(&mut w, &rules);
    assert_eq!(w.missiles.len(), 1);
    assert!(w.ticks.is_empty());
}

#[test]
fn a_repeat_is_bounded_and_needs_the_hosts_tick_seam() {
    let long = SkillRules::from_toml(&CHANNEL.replace("times = 3", "times = 100000")).unwrap();
    let mut w = Rec::new();
    cast_rules(&mut w, &long, 1);
    assert_eq!(w.ticks[0].2.of, MAX_REPEATS as u16);
    // A host with no seam runs the first one and keeps no ticks.
    let rules = SkillRules::from_toml(CHANNEL).unwrap();
    let mut w = Rec::new();
    w.no_ticks = true;
    assert_eq!(cast_rules(&mut w, &rules, 1), 1);
    assert_eq!(w.missiles.len(), 1);
    assert!(w.ticks.is_empty());
    let later = channel_with("now = false\n");
    let mut w = Rec::new();
    w.no_ticks = true;
    assert_eq!(cast_rules(&mut w, &later, 1), 0);
}

/// What a curse part asked the host for.
#[derive(Debug, PartialEq, Eq)]
struct CurseAsked {
    level: i32,
    state: u16,
    range: i32,
    frames: i32,
    filter: u32,
    stats: Vec<(u16, i32)>,
}

#[test]
fn a_curse_part_asks_for_the_state_and_its_stats_around_the_aim_point() {
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"curse\"\nstate = 7\n\
                range = \"6 + lvl\"\nseconds = \"lvl * 2\"\n[[skill.part.stat]]\nstat = 9\n\
                value = \"0 - lvl * 3\"\n";
    let rules = SkillRules::from_toml(toml).unwrap();
    let mut w = Rec::new();
    assert_eq!(cast_rules(&mut w, &rules, 4), 1);
    assert_eq!(
        w.curses,
        [CurseAsked {
            level: 4,
            state: 7,
            range: 10,
            frames: 8 * crate::TICKS_PER_SECOND as i32,
            // Enemies by default, as the engine's curses reach them.
            filter: Affects::Enemies.filter(),
            stats: vec![(9, -12)],
        }]
    );
    // A host that does not curse (no point to scan) is a cast that did nothing.
    let mut none = Rec::new();
    none.curse_none = true;
    assert_eq!(cast_rules(&mut none, &rules, 4), 0);
}

#[test]
fn a_missile_part_that_drops_asks_the_host_for_the_drop_not_the_shot() {
    let toml =
        "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"missile\"\nmissile = 4\nat_aim = true\n";
    let rules = SkillRules::from_toml(toml).unwrap();
    // A point to drop on: the missile lands there.
    let mut w = Rec::new();
    w.aim = Some((30, 0));
    assert_eq!(cast_rules(&mut w, &rules, 3), 1);
    assert_eq!(w.drops, [(4, 0, 3)]);
    assert!(w.missiles.is_empty());
    // No point (or a blocked one): the host makes nothing, the cast did nothing.
    let mut none = Rec::new();
    assert_eq!(cast_rules(&mut none, &rules, 3), 0);
    assert_eq!(none.drops.len(), 1);
}

#[test]
fn a_melee_part_takes_the_monster_attack_mode_before_the_swing() {
    for (name, mode) in [("a1", 1), ("a2", 5), ("s1", 8)] {
        let toml = format!(
            "[[skill]]
id = 0
[[skill.part]]
kind = \"melee_hit\"
monster_mode = \"{name}\"
             enhance = \"lvl\"
stun_frames = 10
own_element = true
"
        );
        let rules = SkillRules::from_toml(&toml).unwrap();
        let mut w = Rec::new().unit(2, (1, 1));
        w.target = Some(UnitId(2));
        assert_eq!(cast_rules(&mut w, &rules, 2), 1);
        assert_eq!(w.modes, [mode], "{name}");
    }
    // No mode: the caster is left as it is. A mode that is none of the three is refused.
    let none = SkillRules::from_toml(
        "[[skill]]
id = 0
[[skill.part]]
kind = \"melee_hit\"
",
    )
    .unwrap();
    let mut w = Rec::new().unit(2, (1, 1));
    w.target = Some(UnitId(2));
    cast_rules(&mut w, &none, 1);
    assert!(w.modes.is_empty());
    assert!(SkillRules::from_toml(
        "[[skill]]
id = 0
[[skill.part]]
kind = \"melee_hit\"
monster_mode = \"a3\"
"
    )
    .is_err());
}

#[test]
fn a_melee_hit_that_lands_plays_its_overlay_and_marks_its_target() {
    let toml = "[[skill]]
id = 0
[[skill.part]]
kind = \"melee_hit\"
overlay = 9
target_state = 30
target_frames = \"100 + lvl\"
[[skill.part.target_stat]]
stat = 16
value = \"lvl * 2\"
";
    let rules = SkillRules::from_toml(toml).unwrap();
    let mut w = Rec::new().unit(2, (1, 1));
    w.target = Some(UnitId(2));
    w.lands = true;
    assert_eq!(cast_rules(&mut w, &rules, 3), 1);
    assert_eq!(w.overlays, [(UnitId(2), 9)]);
    assert_eq!(w.marked, [(UnitId(2), 30, 103, vec![(16, 6)])]);
    // A swing that misses does neither; no frames still lasts a frame.
    let mut miss = Rec::new().unit(2, (1, 1));
    miss.target = Some(UnitId(2));
    assert_eq!(cast_rules(&mut miss, &rules, 3), 1);
    assert!(miss.overlays.is_empty() && miss.marked.is_empty());
    let bare = SkillRules::from_toml(
        "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"melee_hit\"\ntarget_state = 30\n",
    )
    .unwrap();
    let mut w = Rec::new().unit(2, (1, 1));
    w.target = Some(UnitId(2));
    w.lands = true;
    cast_rules(&mut w, &bare, 1);
    assert_eq!(w.marked, [(UnitId(2), 30, 1, vec![])]);
    assert!(w.overlays.is_empty());
}

#[test]
fn a_melee_hit_in_a_row_swings_at_each_enemy_in_reach_once() {
    let row = |n: &str| {
        SkillRules::from_toml(&format!(
            "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"melee_hit\"\noverlay = 9\nsequence = \"{n}\"\n"
        ))
        .unwrap()
    };
    let world = || {
        // 2 is the target; 3 and 4 are in reach too, 5 is far away.
        let mut w = Rec::new()
            .unit(2, (1, 1))
            .unit(3, (1, 0))
            .unit(4, (0, 1))
            .unit(5, (50, 0));
        w.target = Some(UnitId(2));
        w.lands = true;
        w
    };
    let hit = |w: &Rec| w.overlays.iter().map(|(u, _)| u.0).collect::<Vec<_>>();
    // The target, then the others in reach in the unit lists' order.
    let mut w = world();
    assert_eq!(cast_rules(&mut w, &row("5"), 1), 1);
    assert_eq!(hit(&w), [2, 3, 4]);
    // The count caps it, and a formula reads the level.
    let mut w = world();
    cast_rules(&mut w, &row("lvl"), 2);
    assert_eq!(hit(&w), [2, 3]);
    // Zero and one are the single hit; the most is eight.
    for n in ["0", "1"] {
        let mut w = world();
        cast_rules(&mut w, &row(n), 1);
        assert_eq!(hit(&w), [2]);
    }
    let mut w = world();
    for id in 6..20 {
        w = w.unit(id, (0, 1));
    }
    cast_rules(&mut w, &row("99"), 1);
    assert_eq!(hit(&w).len(), 8);
}

#[test]
fn a_melee_hit_that_lands_takes_life_and_mana_on_top_of_the_weapons() {
    let rules = SkillRules::from_toml(
        "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"melee_hit\"\nlife_leech = \"lvl * 2\"\nmana_leech = \"5\"\n",
    )
    .unwrap();
    let mut w = Rec::new().unit(2, (1, 1));
    w.target = Some(UnitId(2));
    w.lands = true;
    cast_rules(&mut w, &rules, 3);
    assert_eq!(w.leeches, [(6, 5)]);
    // A miss takes none.
    let mut miss = Rec::new().unit(2, (1, 1));
    miss.target = Some(UnitId(2));
    cast_rules(&mut miss, &rules, 3);
    assert_eq!(miss.leeches, [(0, 0)]);
}

#[test]
fn a_melee_hit_has_at_most_six_target_stats() {
    let mut toml = String::from(
        "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"melee_hit\"\ntarget_state = 30\n",
    );
    for _ in 0..7 {
        toml.push_str("[[skill.part.target_stat]]\nstat = 16\nvalue = \"1\"\n");
    }
    assert!(SkillRules::from_toml(&toml).is_err());
}

#[test]
fn a_resurrect_part_asks_the_host_to_raise_the_target_with_its_overlay() {
    let with = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"resurrect\"\noverlay = 12\n";
    let rules = SkillRules::from_toml(with).unwrap();
    let mut w = Rec::new().unit(2, (1, 1));
    w.target = Some(UnitId(2));
    assert_eq!(cast_rules(&mut w, &rules, 1), 1);
    // No overlay is -1; no target is a cast that did nothing.
    let bare =
        SkillRules::from_toml("[[skill]]\nid = 0\n[[skill.part]]\nkind = \"resurrect\"\n").unwrap();
    let mut none = Rec::new();
    assert_eq!(cast_rules(&mut none, &bare, 1), 0);
    assert_eq!(w.raised, [12]);
    assert_eq!(none.raised, [-1]);
}

#[test]
fn a_nest_part_asks_the_host_to_hatch_with_its_overlay() {
    let with = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"nest\"\noverlay = 5\n";
    let rules = SkillRules::from_toml(with).unwrap();
    let mut w = Rec::new();
    assert_eq!(cast_rules(&mut w, &rules, 1), 1);
    let bare =
        SkillRules::from_toml("[[skill]]\nid = 0\n[[skill.part]]\nkind = \"nest\"\n").unwrap();
    let mut none = Rec::new();
    cast_rules(&mut none, &bare, 1);
    assert_eq!((w.nests, none.nests), (vec![5], vec![-1]));
}

#[test]
fn a_wall_part_asks_the_host_for_the_wall_with_its_rows() {
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"wall\"\nmissile = 4\ncentre = 6\n";
    let rules = SkillRules::from_toml(toml).unwrap();
    let mut w = Rec::new();
    w.aim = Some((30, 0));
    assert_eq!(cast_rules(&mut w, &rules, 3), 1);
    assert_eq!(w.walls, [(4, 6, 0, 3)]);
    // No middle row is -1, the engine's none; a host that lays nothing is a cast that did nothing.
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"wall\"\nmissile = 4\n";
    let rules = SkillRules::from_toml(toml).unwrap();
    let mut none = Rec::new();
    assert_eq!(cast_rules(&mut none, &rules, 1), 0);
    assert_eq!(none.walls, [(4, -1, 0, 1)]);
}

#[test]
fn a_heal_part_rolls_its_share_on_the_casters_seed_as_the_engines_heal_does() {
    let toml =
        "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"heal\"\nmin = \"10 + lvl\"\nmax = \"30\"\n";
    let rules = SkillRules::from_toml(toml).unwrap();
    let mut w = Rec::new().unit(2, (1, 1));
    w.target = Some(UnitId(2));
    assert_eq!(cast_rules(&mut w, &rules, 5), 1);
    // lo = 15, hi = 30; one roll of 15 on the caster's seed.
    let want = 15 + Seed::init().roll(15) as i32;
    assert_eq!(w.heals, [(UnitId(2), want)]);
    // Bounds past 100 are held there; a least above the most is held to it,
    // which rolls nothing.
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"heal\"\nmin = 90\nmax = 400\n";
    let wide = SkillRules::from_toml(toml).unwrap();
    let mut w = Rec::new().unit(2, (1, 1));
    w.target = Some(UnitId(2));
    cast_rules(&mut w, &wide, 1);
    assert!((90..=100).contains(&w.heals[0].1));
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"heal\"\nmin = 60\nmax = 20\n";
    let inverted = SkillRules::from_toml(toml).unwrap();
    let mut w = Rec::new().unit(2, (1, 1));
    w.target = Some(UnitId(2));
    cast_rules(&mut w, &inverted, 1);
    assert_eq!(w.heals, [(UnitId(2), 20)]);
    // Without a unit target there is no one to heal.
    let mut none = Rec::new();
    assert_eq!(cast_rules(&mut none, &rules, 5), 0);
    assert!(none.heals.is_empty());
}

#[test]
fn a_curse_part_holds_six_stats_and_reads_its_affects() {
    let stats: String = (0..7)
        .map(|n| format!("[[skill.part.stat]]\nstat = {n}\nvalue = 1\n"))
        .collect();
    let head =
        "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"curse\"\nstate = 7\nrange = 6\nseconds = 1\n";
    assert!(SkillRules::from_toml(&format!("{head}{stats}")).is_err());
    let six: String = (0..6)
        .map(|n| format!("[[skill.part.stat]]\nstat = {n}\nvalue = 1\n"))
        .collect();
    assert!(SkillRules::from_toml(&format!("{head}{six}")).is_ok());
    let everyone = format!("{head}affects = \"everyone\"\n");
    let rules = SkillRules::from_toml(&everyone).unwrap();
    let mut w = Rec::new();
    cast_rules(&mut w, &rules, 1);
    assert_eq!(w.curses[0].filter, Affects::Everyone.filter());
}

#[test]
fn a_repeat_of_an_aura_pulses_around_the_caster_each_period() {
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"repeat\"\nskill = 1\nevery = 25\n\
                times = 4\n[[skill]]\nid = 1\n[[skill.part]]\nkind = \"aura\"\nstate = 5\n\
                range = 10\nseconds = 2\n";
    let rules = SkillRules::from_toml(toml).unwrap();
    let mut w = Rec::new();
    cast_rules(&mut w, &rules, 2);
    while fire(&mut w, &rules).is_some() {}
    assert_eq!(w.pulses, [(0, 2, 5, 5); 4]);
}

#[test]
fn an_aura_part_can_give_the_units_around_another_state() {
    let toml = "[[skill]]
id = 0
[[skill.part]]
kind = \"aura\"
state = 5
                target_state = 9
range = 10
seconds = 2
";
    let rules = SkillRules::from_toml(toml).unwrap();
    let mut w = Rec::new();
    cast_rules(&mut w, &rules, 3);
    assert_eq!(w.pulses, [(0, 3, 5, 9)]);
}

#[test]
fn a_charge_builds_one_stack_per_period() {
    let toml = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"repeat\"\nskill = 1\nevery = 25\n\
                times = 3\nnow = false\n[[skill]]\nid = 1\n[[skill.part]]\nkind = \"state\"\n\
                state = 6\nop = \"add_stack\"\n";
    let rules = SkillRules::from_toml(toml).unwrap();
    let mut w = Rec::new();
    cast_rules(&mut w, &rules, 1);
    assert_eq!(w.stacks_added, 0);
    while fire(&mut w, &rules).is_some() {}
    assert_eq!(w.stacks_added, 3);
}

const LEAP: &str = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"leap\"\nframes = 8\n\
                    land = 1\n[[skill]]\nid = 1\n[[skill.part]]\nkind = \"missile_ring\"\nmissile = 9\ncount = 1\n";

#[test]
fn a_leap_moves_the_caster_in_steps_then_lands_with_the_other_skill() {
    let rules = SkillRules::from_toml(LEAP).unwrap();
    let mut w = Rec::new();
    w.aim = Some((100, 0));
    assert_eq!(cast_rules(&mut w, &rules, 1), 1);
    // Nothing moves in the cast itself: the first step is a step away.
    assert!(w.teleports.is_empty());
    assert_eq!(w.ticks[0].1, 2);
    while fire(&mut w, &rules).is_some() {}
    let to: Vec<_> = w.teleports.iter().map(|t| t.1).collect();
    assert_eq!(to, [(25, 0), (50, 0), (75, 0), (100, 0)]);
    // The landing ran once, at the end.
    assert_eq!(w.missiles.len(), 1);
}

#[test]
fn a_leap_is_limited_by_its_range_and_ends_where_a_step_finds_no_room() {
    let limited =
        SkillRules::from_toml(&LEAP.replace("frames = 8", "frames = 4\nmax_range = 30")).unwrap();
    let mut w = Rec::new();
    w.aim = Some((100, 0));
    cast_rules(&mut w, &limited, 1);
    while fire(&mut w, &limited).is_some() {}
    let to: Vec<_> = w.teleports.iter().map(|t| t.1).collect();
    assert_eq!(to, [(15, 0), (30, 0)]);

    let rules = SkillRules::from_toml(LEAP).unwrap();
    let mut w = Rec::new();
    w.aim = Some((100, 0));
    w.blocked_after = Some(2);
    cast_rules(&mut w, &rules, 1);
    while fire(&mut w, &rules).is_some() {}
    // The third step failed: the caster lands where it stands.
    assert_eq!(w.teleports.len(), 3);
    assert_eq!(w.missiles.len(), 1);
    assert!(w.ticks.is_empty());

    // Aimed at its own spot there is nothing to cross: it lands at once.
    let mut w = Rec::new();
    w.aim = Some((0, 0));
    assert_eq!(cast_rules(&mut w, &rules, 1), 1);
    assert!(w.ticks.is_empty());
    assert_eq!(w.missiles.len(), 1);
}

#[test]
fn the_skills_a_repeat_or_leap_runs_are_checked() {
    let check = |toml: &str| {
        SkillRules::from_toml(toml)
            .unwrap()
            .check_ticks()
            .map_err(|e| e.to_string())
    };
    assert!(check(CHANNEL).is_ok());
    assert!(check(LEAP).is_ok());
    let missing = check(&CHANNEL.replace("skill = 1", "skill = 5")).unwrap_err();
    assert!(missing.contains("skill[0].part[0].skill"), "{missing}");
    assert!(missing.contains("not defined"), "{missing}");
    let itself = check(&CHANNEL.replace("skill = 1", "skill = 0")).unwrap_err();
    assert!(itself.contains("cannot run itself"), "{itself}");
    let chained =
        format!("{CHANNEL}[[skill]]\nid = 2\n[[skill.part]]\nkind = \"repeat\"\nskill = 0\n");
    let e = check(&chained).unwrap_err();
    assert!(e.contains("repeats, leaps or rushes itself"), "{e}");
    let landing = check(&LEAP.replace("land = 1", "land = 9")).unwrap_err();
    assert!(landing.contains("part[0].land"), "{landing}");
}

const RUSH: &str = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"rush\"\nsteps = 10\neach = 2\n\
                    land = 1\n[[skill]]\nid = 1\n[[skill.part]]\nkind = \"missile_ring\"\nmissile = 9\ncount = 1\n\
                    [[skill]]\nid = 2\n[[skill.part]]\nkind = \"missile_ring\"\nmissile = 8\ncount = 1\n";

/// A caster running at 45 (nine sub-tiles a step) at a target 40 sub-tiles away.
fn rusher() -> Rec {
    let mut w = Rec::new().unit(2, (40, 0));
    w.run = 45;
    w.target = Some(UnitId(2));
    w.aim = Some((40, 0));
    w
}

#[test]
fn a_rush_runs_at_its_target_a_step_a_tick_and_lands_in_reach() {
    let rules = SkillRules::from_toml(RUSH).unwrap();
    let mut w = rusher();
    assert_eq!(cast_rules(&mut w, &rules, 1), 1);
    // The cast itself does not move: the first step is a tick away.
    assert!(w.teleports.is_empty());
    assert_eq!(w.ticks[0].1, 1);
    assert_eq!(w.moves, [false, true]);
    while fire(&mut w, &rules).is_some() {}
    let to: Vec<_> = w.teleports.iter().map(|t| t.1).collect();
    // Nine sub-tiles a step, a sub-tile short of the target at the end.
    assert_eq!(to, [(9, 0), (18, 0), (27, 0), (36, 0), (39, 0)]);
    // `each` after every step, `land` once at the end.
    assert_eq!(w.missiles.iter().filter(|m| m.class == 8).count(), 5);
    assert_eq!(w.missiles.iter().filter(|m| m.class == 9).count(), 1);
    assert_eq!(w.moves, [false, true, false]);
    assert!(w.ticks.is_empty());
}

#[test]
fn a_rush_ends_at_its_steps_a_blocked_step_or_a_target_already_in_reach() {
    let rules = SkillRules::from_toml(RUSH).unwrap();
    let steps = SkillRules::from_toml(&RUSH.replace("steps = 10", "steps = 2")).unwrap();
    let mut w = rusher();
    cast_rules(&mut w, &steps, 1);
    while fire(&mut w, &steps).is_some() {}
    assert_eq!(w.teleports.len(), 2);
    assert_eq!(w.missiles.iter().filter(|m| m.class == 9).count(), 1);

    let mut w = rusher();
    w.blocked_after = Some(1);
    cast_rules(&mut w, &rules, 1);
    while fire(&mut w, &rules).is_some() {}
    // The second step found no room: it lands where it stands.
    assert_eq!(w.teleports.len(), 2);
    assert_eq!(w.missiles.iter().filter(|m| m.class == 9).count(), 1);
    assert_eq!(w.moves, [false, true, false]);

    // A target in reach is hit at once, with no run.
    let mut w = rusher();
    w.at.insert(UnitId(2), (1, 0));
    assert_eq!(cast_rules(&mut w, &rules, 1), 1);
    assert!(w.ticks.is_empty());
    assert_eq!(w.moves, [false, false]);
    assert_eq!(w.missiles.iter().filter(|m| m.class == 9).count(), 1);

    // Nothing to run at is a cast that did nothing; a host with no tick
    // seam gets the caster back out of the move state.
    let mut w = Rec::new();
    assert_eq!(cast_rules(&mut w, &rules, 1), 0);
    let mut w = rusher();
    w.no_ticks = true;
    assert_eq!(cast_rules(&mut w, &rules, 1), 0);
    assert_eq!(w.moves, [false, true, false]);
}

#[test]
fn a_rush_with_no_target_runs_to_the_aim_point_and_a_new_cast_ends_the_old() {
    let rules = SkillRules::from_toml(RUSH).unwrap();
    let mut w = Rec::new();
    w.run = 45;
    w.aim = Some((20, 0));
    cast_rules(&mut w, &rules, 1);
    while fire(&mut w, &rules).is_some() {}
    let to: Vec<_> = w.teleports.iter().map(|t| t.1).collect();
    assert_eq!(to, [(9, 0), (18, 0), (20, 0)]);
    // No target to keep off: the last step reaches the point itself.
    // A new cast drops the pending step and leaves the move state first.
    let mut w = rusher();
    cast_rules(&mut w, &rules, 1);
    cast_rules(&mut w, &rules, 1);
    assert_eq!(w.ticks.len(), 1);
    assert_eq!(w.moves, [false, true, false, true]);
}

#[test]
fn the_skills_a_rush_runs_are_checked() {
    let check = |toml: &str| {
        SkillRules::from_toml(toml)
            .unwrap()
            .check_ticks()
            .map_err(|e| e.to_string())
    };
    assert!(check(RUSH).is_ok());
    let missing = check(&RUSH.replace("land = 1", "land = 9")).unwrap_err();
    assert!(missing.contains("part[0].land"), "{missing}");
    let each = check(&RUSH.replace("each = 2", "each = 0")).unwrap_err();
    assert!(each.contains("cannot run itself"), "{each}");
    let nested = format!("{RUSH}[[skill]]\nid = 3\n[[skill.part]]\nkind = \"rush\"\nland = 1\n");
    assert!(check(&nested).is_ok());
    let chained = RUSH.replace("each = 2", "each = 3")
        + "[[skill]]\nid = 3\n[[skill.part]]\nkind = \"rush\"\n";
    let e = check(&chained).unwrap_err();
    assert!(e.contains("repeats, leaps or rushes itself"), "{e}");
}

#[test]
fn a_tick_for_a_skill_that_is_gone_does_nothing() {
    let rules = SkillRules::from_toml(CHANNEL).unwrap();
    let mut w = Rec::new();
    cast_rules(&mut w, &rules, 1);
    let (u, _, mut tick) = w.ticks.remove(0);
    tick.skill = 77;
    rules.tick(&mut w, u, &tick);
    tick.skill = 0;
    tick.part = 9;
    rules.tick(&mut w, u, &tick);
    assert_eq!(w.missiles.len(), 1);
    assert!(w.ticks.is_empty());
}
