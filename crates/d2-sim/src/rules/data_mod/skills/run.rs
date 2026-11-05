//! Running a block skill's parts on the skill host.
//!
//! Each part is built from the host's own seams (missiles, aura pulses,
//! summons, states with stack rules, the hit test, damage records and the
//! engine's damage application), so a block skill is hit, blocked, resisted,
//! killed and credited exactly as an engine skill's damage is.

use super::is_timed;
use super::{
    Affects, AreaHitPart, Around, AuraPart, ChainPart, CursePart, Damage, EngineBodyPart, Formula,
    HealPart, LeapPart, MeleeHitPart, MinionOrderPart, MissileArcPart, MissilePart,
    MissileRingPart, MissileVolleyPart, NestPart, Order, Part, RaisePart, RepeatPart,
    ResurrectPart, RushPart, SkillDef, SkillRules, StateOp, StatePart, StateTarget, SummonPart,
    TeleportPart, WallPart, LEAP_STEP_FRAMES, MAX_AREA_TARGETS, MAX_JUMPS, MAX_LEAP_STEPS,
    MAX_MISSILES, MAX_REPEATS, MAX_RUSH_STEPS, MAX_SEQUENCE, MAX_SUMMONS,
};
use crate::combat::damage::hitflag;
use crate::combat::hit::result;
use crate::combat::DamageRecord;
use crate::rules::{
    AuraPulse, BuffPulse, CursePulse, MissileRequest, RuledState, SkillFn, SkillHost, Summon, Tick,
};
use crate::skills::use_::bodies::{init_cb, ring_offset};
use crate::units::UnitId;

/// How far from the caster a hit in a row looks for its next enemy, in
/// sub-tiles (the unit scan; each must then be within melee reach).
const SEQUENCE_SCAN: i32 = 8;

/// The caster, skill and level a part runs for.
#[derive(Clone, Copy)]
struct Cast {
    u: UnitId,
    skill: i32,
    lvl: i32,
}

impl Cast {
    fn eval(&self, w: &mut dyn SkillHost, f: &super::Formula) -> i32 {
        f.eval(w, self.u, self.skill, self.lvl)
    }
}

/// Runs the skill's parts in order. 1 when any part did something (the cast
/// is spent), 0 when none did (no target, nothing summoned).
pub(super) fn run(
    rules: &SkillRules,
    def: &SkillDef,
    w: &mut dyn SkillHost,
    u: UnitId,
    skill: i32,
    lvl: i32,
) -> i32 {
    // A new cast ends the repeats, the leap and the rush this skill started
    // before.
    if def.parts.iter().any(is_timed) {
        w.cancel_ticks(u, Some(skill));
    }
    if def.parts.iter().any(|p| matches!(p, Part::Rush(_))) {
        w.skill_move(u, false);
    }
    let c = Cast { u, skill, lvl };
    i32::from(run_parts(rules, &def.parts, w, c, false))
}

/// Runs `parts` in order for `c`; `plain` skips the parts that start ticks
/// (the parts of a repeated skill run on a tick, which is not a cast).
fn run_parts(
    rules: &SkillRules,
    parts: &[Part],
    w: &mut dyn SkillHost,
    c: Cast,
    plain: bool,
) -> bool {
    let (u, skill, lvl) = (c.u, c.skill, c.lvl);
    let mut acted = false;
    for (j, part) in parts.iter().enumerate() {
        acted |= match part {
            Part::CallScript(spec) => {
                let context = crate::rules::scripts::CallContext {
                    unit: u,
                    other: w.target(u),
                    skill,
                    level: lvl,
                    argument: c.eval(w, &spec.argument),
                };
                w.call_script(spec, context).is_ok_and(|n| n != 0)
            }
            Part::Missile(p) => missile(w, c, p),
            Part::Aura(p) => aura(w, c, p),
            Part::Curse(p) => curse(w, c, p),
            Part::Heal(p) => heal(w, c, p),
            Part::Sentry(p) => super::sentry::run(w, u, skill, lvl, p),
            Part::Form(p) => super::form::run(w, u, skill, lvl, p),
            Part::Vine(p) => super::vine::run(w, u, skill, lvl, p),
            Part::DamageAura(p) => super::damage_aura::run(w, u, skill, lvl, p),
            Part::Passive(_) => false, // Passive parts only run during refresh.
            Part::Golem(p) => super::golem::run(w, u, skill, lvl, p),
            Part::Raven(p) => super::raven::run(w, u, skill, lvl, p),
            Part::CorpseFind(p) => super::corpse_find::run(w, u, skill, lvl, p),
            Part::MartialCharge(p) => super::martial_charge::run(w, u, skill, lvl, p),
            Part::Shadow(p) => super::shadow::run(w, u, skill, lvl, p),
            Part::AiCurse(p) => super::ai_curse::run(w, u, skill, lvl, p),
            Part::ProjectileFan(p) => super::projectile_fan::run(w, u, skill, lvl, p),
            Part::GuidedProjectile(p) => super::guided_projectile::run(w, u, skill, lvl, p),
            Part::Sight(p) => super::sight::run(w, u, skill, lvl, p),
            Part::TimedBuff(p) => super::buff::run(w, u, skill, lvl, p),
            Part::Warcry(p) => super::warcry::run(w, u, skill, lvl, p),
            Part::DruidSummon(p) => super::druid_summon::run(w, u, skill, lvl, p),
            Part::Wall(p) => wall(w, c, p),
            Part::Resurrect(p) => resurrect(w, c, p),
            Part::Nest(p) => nest(w, c, p),
            Part::Summon(p) => summon(w, c, p),
            Part::State(p) => state(w, c, p),
            Part::MeleeHit(p) => melee(w, c, p),
            Part::AreaHit(p) => area(w, c, p),
            Part::MissileRing(p) => ring(w, c, p),
            Part::MissileArc(p) => arc(w, c, p),
            Part::MissileVolley(p) => volley(w, c, p),
            Part::Chain(p) => chain(w, c, p, false),
            Part::Bounce(p) => chain(w, c, p, true),
            Part::Teleport(p) => teleport(w, c, p),
            Part::Raise(p) => raise(w, c, p),
            Part::MinionOrder(p) => minions(w, c, p),
            Part::EngineBody(p) => engine_body(w, c, p),
            // A repeat of the parts after it takes them over: they run on its
            // ticks, not at the cast.
            Part::Repeat(p) if !plain => {
                let ran = repeat(rules, w, c, j, p);
                if p.skill.is_none() {
                    return acted | ran;
                }
                ran
            }
            Part::Leap(p) if !plain => leap(rules, w, c, j, p),
            // A rush with no `land` skill takes the parts after it over: they
            // run where it ends, not at the cast.
            Part::Rush(p) if !plain => {
                let ran = rush(rules, w, c, j, p);
                if p.land.is_none() {
                    return acted | ran;
                }
                ran
            }
            Part::Repeat(_) | Part::Leap(_) | Part::Rush(_) => false,
        };
        if let Some(charge) = damage_of(part).and_then(|d| d.charge.as_ref()) {
            if charge.consume {
                w.clear_state(u, charge.state);
            }
        }
    }
    acted
}

/// A tick the part `t.part` of skill `t.skill` asked for is due.
pub(super) fn tick(rules: &SkillRules, w: &mut dyn SkillHost, u: UnitId, t: &Tick) {
    if w.is_dead(u) {
        return;
    }
    let Some(part) = rules
        .get(t.skill)
        .and_then(|d| d.parts.get(usize::from(t.part)))
    else {
        return;
    };
    let c = Cast {
        u,
        skill: t.skill,
        lvl: t.level,
    };
    match part {
        Part::Repeat(p) => repeat_tick(rules, w, c, t, p),
        Part::Leap(p) => leap_tick(rules, w, c, t, p),
        Part::Rush(p) => rush_tick(rules, w, c, t, p),
        _ => {}
    }
}

/// The parts of the skill row `id`, run as the cast `c`.
fn run_skill(rules: &SkillRules, w: &mut dyn SkillHost, c: Cast, id: u16) -> bool {
    match rules.get(i32::from(id)) {
        Some(d) => run_parts(rules, &d.parts, w, c, true),
        None => false,
    }
}

/// What a repeat runs once: the other skill's parts, or the parts after the
/// repeat (part `part`) in the skill that has it.
fn run_repeated(
    rules: &SkillRules,
    w: &mut dyn SkillHost,
    c: Cast,
    part: usize,
    p: &RepeatPart,
) -> bool {
    match p.skill {
        Some(id) => run_skill(rules, w, c, id),
        None => rules
            .get(c.skill)
            .and_then(|d| d.parts.get(part + 1..))
            .is_some_and(|tail| run_parts(rules, tail, w, c, true)),
    }
}

fn repeat(rules: &SkillRules, w: &mut dyn SkillHost, c: Cast, part: usize, p: &RepeatPart) -> bool {
    let times = c.eval(w, &p.times).clamp(1, MAX_REPEATS);
    let every = c.eval(w, &p.every).max(1);
    let mut left = times;
    let mut acted = false;
    if p.now {
        acted = run_repeated(rules, w, c, part, p);
        left -= 1;
    }
    if left > 0 {
        let tick = Tick {
            skill: c.skill,
            level: c.lvl,
            part: part as u16,
            left: left as u16,
            of: times as u16,
            from: w.position(c.u),
            to: (0, 0),
        };
        acted |= w.schedule_tick(c.u, every, tick);
    }
    acted
}

fn repeat_tick(rules: &SkillRules, w: &mut dyn SkillHost, c: Cast, t: &Tick, p: &RepeatPart) {
    if p.stop_on_move && w.position(c.u) != t.from {
        return;
    }
    if p.while_state.is_some_and(|s| !w.has_state(c.u, s)) {
        return;
    }
    run_repeated(rules, w, c, usize::from(t.part), p);
    if t.left > 1 {
        let every = c.eval(w, &p.every).max(1);
        w.schedule_tick(
            c.u,
            every,
            Tick {
                left: t.left - 1,
                ..*t
            },
        );
    }
}

fn leap(rules: &SkillRules, w: &mut dyn SkillHost, c: Cast, part: usize, p: &LeapPart) -> bool {
    let from = w.position(c.u);
    let aim = aim_point(w, c.u);
    let to = clamp_range(w, c, from, aim, p.max_range.as_ref());
    if to == from {
        return land(rules, w, c, p);
    }
    let frames = c.eval(w, &p.frames).max(LEAP_STEP_FRAMES);
    let steps = (frames / LEAP_STEP_FRAMES).clamp(1, MAX_LEAP_STEPS);
    let tick = Tick {
        skill: c.skill,
        level: c.lvl,
        part: part as u16,
        left: steps as u16,
        of: steps as u16,
        from,
        to,
    };
    w.schedule_tick(c.u, LEAP_STEP_FRAMES, tick)
}

fn leap_tick(rules: &SkillRules, w: &mut dyn SkillHost, c: Cast, t: &Tick, p: &LeapPart) {
    let step = i64::from(t.of - t.left) + 1;
    let of = i64::from(t.of).max(1);
    let along = |a: i32, b: i32| a + ((i64::from(b) - i64::from(a)) * step / of) as i32;
    let at = (along(t.from.0, t.to.0), along(t.from.1, t.to.1));
    // A step with no free spot ends the leap where the caster stands.
    if w.teleport(c.u, at) && t.left > 1 {
        w.schedule_tick(
            c.u,
            LEAP_STEP_FRAMES,
            Tick {
                left: t.left - 1,
                ..*t
            },
        );
        return;
    }
    land(rules, w, c, p);
}

/// Starts a rush: the caster is already in reach of its target (the parts
/// of `land` run at once), or it is put in the skill-move state and takes
/// its first step on the next tick.
fn rush(rules: &SkillRules, w: &mut dyn SkillHost, c: Cast, part: usize, p: &RushPart) -> bool {
    let target = w.target(c.u);
    if let Some(k) = target.filter(|_| p.stop_in_reach) {
        if w.in_reach(c.u, k) {
            return rush_land(rules, w, c, part, p);
        }
    }
    let Some(goal) = w.target_position(c.u) else {
        return false;
    };
    let steps = c.eval(w, &p.steps).clamp(1, MAX_RUSH_STEPS);
    let tick = Tick {
        skill: c.skill,
        level: c.lvl,
        part: part as u16,
        left: steps as u16,
        of: steps as u16,
        from: w.position(c.u),
        to: goal,
    };
    w.skill_move(c.u, true);
    if w.schedule_tick(c.u, 1, tick) {
        return true;
    }
    w.skill_move(c.u, false);
    false
}

fn rush_tick(rules: &SkillRules, w: &mut dyn SkillHost, c: Cast, t: &Tick, p: &RushPart) {
    // The target's place now; the point first aimed at when it is gone.
    let target = w.target(c.u).filter(|&k| !w.is_dead(k));
    let goal = target.and_then(|_| w.target_position(c.u)).unwrap_or(t.to);
    if let Some(k) = target.filter(|_| p.stop_in_reach) {
        if w.in_reach(c.u, k) {
            rush_land(rules, w, c, usize::from(t.part), p);
            return;
        }
    }
    let here = w.position(c.u);
    let speed = i64::from(c.eval(w, &p.speed).max(0));
    let step = (i64::from(w.run_speed(c.u)) * speed + 499) / 500;
    // A target is not stood on: the rush keeps a sub-tile off it.
    let keep = i64::from(target.is_some());
    let next = toward(here, goal, step.max(1), keep);
    if next == here || !w.teleport(c.u, next) {
        rush_land(rules, w, c, usize::from(t.part), p);
        return;
    }
    if let Some(id) = p.each {
        run_skill(rules, w, c, id);
    }
    if t.left > 1 {
        w.schedule_tick(
            c.u,
            1,
            Tick {
                left: t.left - 1,
                ..*t
            },
        );
    } else {
        rush_land(rules, w, c, usize::from(t.part), p);
    }
}

/// The caster leaves the skill-move state and the `land` skill's parts (or
/// the parts after the rush, part `part`) run where it stands.
fn rush_land(
    rules: &SkillRules,
    w: &mut dyn SkillHost,
    c: Cast,
    part: usize,
    p: &RushPart,
) -> bool {
    w.skill_move(c.u, false);
    match p.land {
        Some(id) => run_skill(rules, w, c, id),
        None => rules
            .get(c.skill)
            .and_then(|d| d.parts.get(part + 1..))
            .is_some_and(|tail| run_parts(rules, tail, w, c, true)),
    }
}

/// The point `step` sub-tiles from `from` toward `goal` (rounded to the
/// nearest sub-tile), stopping `keep` sub-tiles short of it; `from` when the
/// goal is that close.
fn toward(from: (i32, i32), goal: (i32, i32), step: i64, keep: i64) -> (i32, i32) {
    let (dx, dy) = (
        i64::from(goal.0) - i64::from(from.0),
        i64::from(goal.1) - i64::from(from.1),
    );
    let d = (dx * dx + dy * dy).isqrt();
    let go = (d - keep).min(step);
    if go <= 0 {
        return from;
    }
    let share = |n: i64| ((2 * n * go + n.signum() * d) / (2 * d)) as i32;
    (from.0 + share(dx), from.1 + share(dy))
}

fn land(rules: &SkillRules, w: &mut dyn SkillHost, c: Cast, p: &LeapPart) -> bool {
    p.land.is_some_and(|id| run_skill(rules, w, c, id))
}

fn missile(w: &mut dyn SkillHost, c: Cast, p: &MissilePart) -> bool {
    if p.at_aim {
        return w
            .drop_skill_missile(c.u, i32::from(p.missile), c.skill, c.lvl)
            .is_some();
    }
    let fired = w.fire_skill_missile(c.u, i32::from(p.missile), c.skill, c.lvl, p.lob);
    if let (Some(m), true) = (fired, p.channel) {
        w.channel_length(c.u, m, c.skill, c.lvl);
    }
    fired.is_some()
}

/// Lays the wall through the host's own wall code: the same missiles in the
/// same places as the engine's Fire Wall.
fn wall(w: &mut dyn SkillHost, c: Cast, p: &WallPart) -> bool {
    let centre = p.centre.map_or(-1, i32::from);
    w.lay_wall(c.u, i32::from(p.missile), centre, c.skill, c.lvl) != 0
}

/// Hatches the nest the start function prepared, through the host's own
/// nest code.
fn nest(w: &mut dyn SkillHost, c: Cast, p: &NestPart) -> bool {
    w.nest(c.u, p.overlay.map_or(-1, i32::from)) != 0
}

/// Raises the target through the host's own resurrection code: the revive
/// and the rise animation mode request are the engine's.
fn resurrect(w: &mut dyn SkillHost, c: Cast, p: &ResurrectPart) -> bool {
    w.resurrect(c.u, p.overlay.map_or(-1, i32::from)) != 0
}

/// The target's life goes up by a share rolled on the caster's seed: the
/// bounds are clamped and the roll drawn as the engine's Zakarum heal does.
fn heal(w: &mut dyn SkillHost, c: Cast, p: &HealPart) -> bool {
    let Some(target) = w.target(c.u) else {
        return false;
    };
    let hi = c.eval(w, &p.max).clamp(0, 100);
    let lo = c.eval(w, &p.min).max(0).min(hi);
    let share = lo.wrapping_add(w.seed(c.u).roll(hi.wrapping_sub(lo)) as i32);
    w.heal(c.u, target, share)
}

fn aura(w: &mut dyn SkillHost, c: Cast, p: &AuraPart) -> bool {
    let range = c.eval(w, &p.range).max(0);
    let seconds = c.eval(w, &p.seconds).max(0);
    let stats: Vec<(u16, i32)> = p
        .stats
        .iter()
        .map(|s| (s.stat, c.eval(w, &s.value)))
        .collect();
    w.aura_pulse(
        c.u,
        &AuraPulse {
            skill: c.skill,
            level: c.lvl,
            state: p.state,
            target_state: p.target_state.unwrap_or(p.state),
            range,
            frames: seconds.saturating_mul(crate::TICKS_PER_SECOND as i32),
            filter: p.affects.filter(),
            stats: &stats,
        },
    );
    true
}

fn curse(w: &mut dyn SkillHost, c: Cast, p: &CursePart) -> bool {
    let range = c.eval(w, &p.range).max(0);
    let seconds = c.eval(w, &p.seconds).max(0);
    let stats: Vec<(u16, i32)> = p
        .stats
        .iter()
        .map(|s| (s.stat, c.eval(w, &s.value)))
        .collect();
    w.curse_pulse(
        c.u,
        &CursePulse {
            skill: c.skill,
            level: c.lvl,
            state: p.state,
            range,
            frames: seconds.saturating_mul(crate::TICKS_PER_SECOND as i32),
            filter: p.affects.filter(),
            stats: &stats,
        },
    ) != 0
}

fn summon(w: &mut dyn SkillHost, c: Cast, p: &SummonPart) -> bool {
    let count = c.eval(w, &p.count).clamp(0, MAX_SUMMONS as i32);
    let pet_max = c.eval(w, &p.pet_max).max(1);
    let level = c.eval(w, &p.level);
    let mut made = false;
    for _ in 0..count {
        let req = Summon {
            flags: 0,
            owner: c.u,
            class: i32::from(p.class),
            ai: 0,
            mode: 1,
            x: 0,
            y: 0,
            pet_type: i32::from(p.pet_type),
            pet_max,
        };
        let Some(pet) = w.summon(req) else { break };
        w.pet_base_stats(c.u, pet, level, c.lvl);
        w.pet_skill_stats(c.u, pet, c.skill, c.lvl);
        w.follow_owner(c.u, pet);
        made = true;
    }
    made
}

/// The units a part with an area reaches: around `at`, within `radius`
/// sub-tiles, that pass the filter, at most [`MAX_AREA_TARGETS`], in the unit
/// lists' order.
fn in_area(
    w: &mut dyn SkillHost,
    caster: UnitId,
    at: (i32, i32),
    radius: i32,
    affects: Affects,
) -> Vec<UnitId> {
    let filter = affects.filter();
    let mut found = w.units_near(caster, at, radius.max(0));
    found.retain(|&t| w.passes_filter(caster, t, filter));
    found.truncate(MAX_AREA_TARGETS);
    found
}

/// The aim point: the target point, else the target unit, else the caster.
fn aim_point(w: &dyn SkillHost, u: UnitId) -> (i32, i32) {
    w.target_position(u)
        .or_else(|| w.target(u).map(|t| w.position(t)))
        .unwrap_or_else(|| w.position(u))
}

fn state(w: &mut dyn SkillHost, c: Cast, p: &StatePart) -> bool {
    let base_ticks = match &p.seconds {
        Some(f) => c
            .eval(w, f)
            .max(0)
            .saturating_mul(crate::TICKS_PER_SECOND as i32),
        None => 0,
    };
    let spec = RuledState {
        state: p.state,
        skill: c.skill,
        level: c.lvl,
        base_ticks,
    };
    let count = u16::try_from(c.eval(w, &p.count).clamp(0, 255)).unwrap_or(1);
    let targets: Vec<UnitId> = match p.target {
        StateTarget::Caster => vec![c.u],
        StateTarget::Target => w.target(c.u).into_iter().collect(),
        StateTarget::Area => {
            let radius = p.radius.as_ref().map_or(0, |f| c.eval(w, f));
            let at = aim_point(w, c.u);
            in_area(w, c.u, at, radius, p.affects)
        }
    };
    let mut acted = false;
    for t in targets {
        acted |= match p.op {
            StateOp::Apply => w.apply_ruled_state(c.u, t, &spec),
            StateOp::AddStack => w.add_state_stacks(c.u, t, &spec, count) > 0,
            StateOp::RemoveStack => {
                let had = w.state_stacks(t, p.state) > 0;
                w.remove_state_stacks(t, p.state, count);
                had
            }
            StateOp::Remove => {
                let had = w.state_stacks(t, p.state) > 0;
                w.clear_state(t, p.state);
                had
            }
        };
    }
    // A cast on a target area or unit that found nobody still happened.
    acted || matches!(p.target, StateTarget::Area)
}

/// The damage a part deals, if it has any.
fn damage_of(p: &Part) -> Option<&Damage> {
    match p {
        Part::MeleeHit(m) => m.damage.as_ref(),
        Part::AreaHit(a) => Some(&a.damage),
        Part::Chain(c) | Part::Bounce(c) => Some(&c.damage),
        _ => None,
    }
}

/// Rolls the part's damage into `rec`, in the engine's 1/256 points, scaled
/// to `scale` percent.
fn add_damage(w: &mut dyn SkillHost, c: Cast, d: &Damage, scale: i32, rec: &mut DamageRecord) {
    let (lo, hi) = (c.eval(w, &d.min), c.eval(w, &d.max));
    let (lo, hi) = (lo.min(hi), lo.max(hi));
    let mut points = crate::skills::calc::rand(w.seed(c.u), lo, hi).max(0);
    if let Some(stat) = d.bonus_stat {
        let percent = w.stat(c.u, stat, 0).max(-100);
        points = points.saturating_mul(100 + percent) / 100;
    }
    if let Some(charge) = &d.charge {
        let stacks = i32::from(w.state_stacks(c.u, charge.state));
        let percent = c.eval(w, &charge.percent).max(0).saturating_mul(stacks);
        points = points.saturating_mul(100i32.saturating_add(percent)) / 100;
    }
    points = points.saturating_mul(scale.clamp(0, 100)) / 100;
    let value = points.saturating_mul(256);
    let len = c
        .eval(w, &d.seconds)
        .max(0)
        .saturating_mul(crate::TICKS_PER_SECOND as i32);
    match d.element.id() {
        0 => rec.physical = rec.physical.saturating_add(value),
        e => {
            w.add_element(c.u, rec, e, value, len);
        }
    }
}

fn melee(w: &mut dyn SkillHost, c: Cast, p: &MeleeHitPart) -> bool {
    let Some(first) = w.target(c.u) else {
        return false;
    };
    let count = c.eval(w, &p.sequence).clamp(1, MAX_SEQUENCE as i32) as usize;
    let mut hit = vec![first];
    if count > 1 {
        // The unit lists' order, as the engine's next-unit scan walks them.
        let at = w.position(c.u);
        for t in in_area(w, c.u, at, SEQUENCE_SCAN, Affects::Enemies) {
            if hit.len() >= count {
                break;
            }
            if t != c.u && !hit.contains(&t) && w.in_reach(c.u, t) {
                hit.push(t);
            }
        }
    }
    for target in hit {
        melee_one(w, c, target, p);
    }
    true
}

fn melee_one(w: &mut dyn SkillHost, c: Cast, target: UnitId, p: &MeleeHitPart) {
    let bonus = c.eval(w, &p.attack_rating);
    if let Some(mode) = p.monster_mode {
        w.mode_damage(c.u, mode.mode());
    }
    let mut rec = DamageRecord {
        result: w.melee_result(c.u, target, bonus, 0),
        ..DamageRecord::default()
    };
    // The record's amounts are set here; the engine does not roll it again.
    rec.hit_flags |= hitflag::SKIP_ROLL;
    let landed = rec.result & result::HIT != 0;
    if landed {
        rec.result |= p.result_flags;
        rec.hit_flags |= p.hit_flags;
        if p.hit_class != 0 {
            rec.hit_class = u32::from(p.hit_class);
        }
        rec.enh_pct = c.eval(w, &p.enhance);
        if p.own_element {
            w.roll_skill_element(c.u, c.skill, c.lvl, &mut rec);
        }
    }
    rec.stun_len = c.eval(w, &p.stun_frames);
    let src_dam = (p.weapon_percent * 128 / 100) as i32;
    if p.weapon_percent > 0 {
        w.fill_damage(c.u, target, &mut rec, false, src_dam);
    }
    if let Some(d) = &p.damage {
        add_damage(w, c, d, 100, &mut rec);
    }
    if landed {
        rec.life_leech = rec.life_leech.wrapping_add(c.eval(w, &p.life_leech));
        rec.mana_leech = rec.mana_leech.wrapping_add(c.eval(w, &p.mana_leech));
    }
    w.declare_cc(c.u, &mut rec, &p.cc);
    w.start_combat(c.u, target, &mut rec, src_dam.max(1));
    if landed {
        hit_effects(w, c, target, p);
    }
    w.apply_queued(c.u, target);
}

/// What a melee hit that landed does to its target besides damage.
fn hit_effects(w: &mut dyn SkillHost, c: Cast, target: UnitId, p: &MeleeHitPart) {
    if let Some(overlay) = p.overlay {
        w.unit_overlay(target, i32::from(overlay));
    }
    if let Some(state) = p.target_state {
        let stats: Vec<_> = p
            .target_stats
            .iter()
            .map(|s| (s.stat, c.eval(w, &s.value)))
            .collect();
        let frames = c.eval(w, &p.target_frames).max(1);
        w.warcry_state(
            c.u,
            target,
            &BuffPulse {
                skill: c.skill,
                level: c.lvl,
                state,
                frames,
                stats: &stats,
                events: &[],
            },
        );
    }
}

fn area(w: &mut dyn SkillHost, c: Cast, p: &AreaHitPart) -> bool {
    let radius = c.eval(w, &p.radius);
    let at = match p.around {
        Around::Caster => w.position(c.u),
        Around::Target => aim_point(w, c.u),
    };
    for t in in_area(w, c.u, at, radius, p.affects) {
        let mut rec = DamageRecord {
            result: result::HIT,
            hit_flags: hitflag::ROLLED,
            ..DamageRecord::default()
        };
        add_damage(w, c, &p.damage, 100, &mut rec);
        w.declare_cc(c.u, &mut rec, &p.cc);
        w.apply_damage(c.u, t, true, &mut rec);
        if !p.cc.is_empty() {
            w.react(c.u, t, &mut rec);
        }
    }
    true
}

/// A missile request from `at`; the caller sets the target.
fn missile_request(c: Cast, class: u16, at: (i32, i32), flags: u32) -> MissileRequest<UnitId> {
    MissileRequest {
        flags,
        x: at.0,
        y: at.1,
        skill: c.skill,
        level: c.lvl,
        ..MissileRequest::new(c.u, i32::from(class))
    }
}

/// With a velocity the request flies at it (flag 4); without, at the missile
/// row's own.
fn with_velocity(
    w: &mut dyn SkillHost,
    c: Cast,
    v: Option<&Formula>,
    req: &mut MissileRequest<UnitId>,
) {
    if let Some(v) = v {
        let v = c.eval(w, v);
        if v != 0 {
            req.flags |= 4;
            req.velocity = v;
        }
    }
}

fn ring(w: &mut dyn SkillHost, c: Cast, p: &MissileRingPart) -> bool {
    let count = c.eval(w, &p.count).clamp(1, MAX_MISSILES) as usize;
    let at = match p.around {
        Around::Caster => w.position(c.u),
        Around::Target => aim_point(w, c.u),
    };
    let mut req = missile_request(c, p.missile, at, 3);
    with_velocity(w, c, p.velocity.as_ref(), &mut req);
    let mut made = false;
    for i in 0..count {
        // Evenly spaced on the engine's 64-step ring.
        let (ox, oy) = ring_offset(i * 64 / count);
        req.target_x = ox;
        req.target_y = oy;
        made |= w.spawn_missile(req).is_some();
    }
    made
}

/// The ring step whose direction is closest to the way from `from` to `to`;
/// the first of equal steps, so a replay picks the same one.
fn ring_step(from: (i32, i32), to: (i32, i32)) -> i32 {
    let (dx, dy) = (i64::from(to.0 - from.0), i64::from(to.1 - from.1));
    (0..64usize)
        .max_by_key(|&i| {
            let (ox, oy) = ring_offset(i);
            (i64::from(ox) * dx + i64::from(oy) * dy, -(i as i64))
        })
        .unwrap_or(0) as i32
}

fn arc(w: &mut dyn SkillHost, c: Cast, p: &MissileArcPart) -> bool {
    let count = c.eval(w, &p.count).clamp(1, MAX_MISSILES);
    let span = c.eval(w, &p.span).clamp(0, 64);
    let from = w.position(c.u);
    let centre = ring_step(from, aim_point(w, c.u));
    let mut req = missile_request(c, p.missile, from, 3);
    with_velocity(w, c, p.velocity.as_ref(), &mut req);
    let mut made = false;
    for k in 0..count {
        // From one edge of the arc to the other.
        let step = if count == 1 {
            0
        } else {
            k * span / (count - 1) - span / 2
        };
        let (ox, oy) = ring_offset((centre + step).rem_euclid(64) as usize);
        req.target_x = ox;
        req.target_y = oy;
        made |= w.spawn_missile(req).is_some();
    }
    made
}

fn volley(w: &mut dyn SkillHost, c: Cast, p: &MissileVolleyPart) -> bool {
    let count = c.eval(w, &p.count).clamp(1, MAX_MISSILES);
    let to = aim_point(w, c.u);
    let mut req = missile_request(c, p.missile, w.position(c.u), 0x21);
    req.target_x = to.0;
    req.target_y = to.1;
    let mut made = false;
    for i in 0..count {
        req.init = Some((init_cb::JITTER, i as u32));
        made |= w.spawn_missile(req).is_some();
    }
    made
}

fn chain(w: &mut dyn SkillHost, c: Cast, p: &ChainPart, bounce: bool) -> bool {
    let Some(first) = w.target(c.u) else {
        return false;
    };
    let radius = c.eval(w, &p.radius).max(0);
    let jumps = c.eval(w, &p.jumps).clamp(0, MAX_JUMPS);
    let falloff = c.eval(w, &p.falloff).clamp(0, 100);
    let filter = p.affects.filter();
    let mut hit: Vec<UnitId> = Vec::new();
    let mut at = first;
    let mut scale = 100;
    for _ in 0..=jumps {
        let mut rec = DamageRecord {
            result: result::HIT,
            hit_flags: hitflag::ROLLED,
            ..DamageRecord::default()
        };
        add_damage(w, c, &p.damage, scale, &mut rec);
        w.declare_cc(c.u, &mut rec, &p.cc);
        w.apply_damage(c.u, at, true, &mut rec);
        if !p.cc.is_empty() {
            w.react(c.u, at, &mut rec);
        }
        hit.push(at);
        scale -= falloff;
        if scale <= 0 {
            break;
        }
        // The nearest unit in reach; the first of equals in the lists' order.
        let from = w.position(at);
        let mut best: Option<(i64, UnitId)> = None;
        for t in w.units_near(c.u, from, radius) {
            let fresh = if bounce { t != at } else { !hit.contains(&t) };
            if !fresh || w.is_dead(t) || !w.passes_filter(c.u, t, filter) {
                continue;
            }
            let there = w.position(t);
            let d = i64::from(there.0 - from.0).pow(2) + i64::from(there.1 - from.1).pow(2);
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, t));
            }
        }
        match best {
            Some((_, t)) => at = t,
            None => break,
        }
    }
    true
}

/// `to`, or the point `range` sub-tiles from `from` toward it when it is
/// farther.
fn clamp_range(
    w: &mut dyn SkillHost,
    c: Cast,
    from: (i32, i32),
    to: (i32, i32),
    range: Option<&Formula>,
) -> (i32, i32) {
    let Some(range) = range else { return to };
    let range = i64::from(c.eval(w, range).max(0));
    let (dx, dy) = (i64::from(to.0 - from.0), i64::from(to.1 - from.1));
    let dist = (dx * dx + dy * dy).isqrt();
    if dist > range {
        return (
            from.0 + (dx * range / dist) as i32,
            from.1 + (dy * range / dist) as i32,
        );
    }
    to
}

fn teleport(w: &mut dyn SkillHost, c: Cast, p: &TeleportPart) -> bool {
    let from = w.position(c.u);
    let aim = aim_point(w, c.u);
    let to = clamp_range(w, c, from, aim, p.max_range.as_ref());
    w.teleport(c.u, to)
}

fn engine_body(w: &mut dyn SkillHost, c: Cast, p: &EngineBodyPart) -> bool {
    let skill = p.skill.map_or(c.skill, i32::from);
    let lvl = p.level.as_ref().map_or(c.lvl, |f| c.eval(w, f));
    w.engine_body(SkillFn::Do, p.slot, c.u, skill, lvl)
        .is_some_and(|v| v != 0)
}

fn raise(w: &mut dyn SkillHost, c: Cast, p: &RaisePart) -> bool {
    let Some(corpse) = w.target(c.u) else {
        return false;
    };
    if !w.revivable(c.u, corpse) {
        return false;
    }
    let pet_max = c.eval(w, &p.pet_max).max(1);
    w.revive_prepare(c.u, corpse);
    w.pet_skill_stats(c.u, corpse, c.skill, c.lvl);
    w.revive_finish(c.u, corpse);
    w.add_pet(c.u, corpse, i32::from(p.pet_type), pet_max);
    true
}

fn minions(w: &mut dyn SkillHost, c: Cast, p: &MinionOrderPart) -> bool {
    let radius = c.eval(w, &p.radius).max(0);
    let from = w.position(c.u);
    let mut found = w.units_near(c.u, from, radius);
    found.retain(|&t| t != c.u && !w.is_dead(t) && w.owner(t) == Some(c.u));
    found.truncate(MAX_AREA_TARGETS);
    let target = w.target(c.u);
    let mut acted = false;
    for m in found {
        acted |= match p.order {
            Order::Recall => w.teleport(m, from),
            Order::Follow => {
                w.follow_owner(c.u, m);
                true
            }
            Order::Attack => {
                let skill = p.skill.map_or(0, i32::from);
                let lvl = match &p.level {
                    Some(f) => c.eval(w, f),
                    None => c.lvl,
                };
                target.is_some() && w.cast_skill(m, skill, lvl, target)
            }
        };
    }
    acted
}
