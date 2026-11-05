//! Running a mod's triggers for one unit event on the skill host.

use super::{Action, Condition, On, TriggerDef, TriggerRules, Who};
use crate::rules::{RuledState, SkillHost, UnitEvent};
use crate::units::UnitId;

/// Stat 6 `hitpoints` and stat 7 `maxhp`.
const LIFE: u16 = 6;
const MAX_LIFE: u16 = 7;

/// Runs every trigger of `rules` that `ev` sets off for the unit `u`, in
/// file order. `other` is the other unit of the event, when it has one.
pub(super) fn run(
    rules: &TriggerRules,
    w: &mut dyn SkillHost,
    ev: UnitEvent,
    u: UnitId,
    other: Option<UnitId>,
) {
    if w.is_dead(u) {
        return;
    }
    for d in rules.defs() {
        if !reacts_to(d, ev) || !used_by(w, d, u) {
            continue;
        }
        let Some((skill, lvl)) = source(w, d, u) else {
            continue;
        };
        if !d.conditions.iter().all(|c| holds(w, c, u, other)) {
            continue;
        }
        if !chance_passes(w, d, u, skill, lvl) {
            continue;
        }
        // A trigger whose actions set off triggers that set off triggers
        // stops here.
        if !w.enter_trigger() {
            continue;
        }
        for a in &d.actions {
            act(w, a, u, other, skill, lvl);
        }
        w.leave_trigger();
    }
}

fn reacts_to(d: &TriggerDef, ev: UnitEvent) -> bool {
    match (d.on, ev) {
        (On::Attack, UnitEvent::Attack)
        | (On::Hit, UnitEvent::Hit)
        | (On::BeingHit, UnitEvent::BeingHit)
        | (On::Kill, UnitEvent::Kill) => true,
        (On::SkillUsed, UnitEvent::SkillUsed(s)) => {
            d.skills.is_empty() || u16::try_from(s).is_ok_and(|s| d.skills.contains(&s))
        }
        (On::StateStart, UnitEvent::StateStart(s)) | (On::StateEnd, UnitEvent::StateEnd(s)) => {
            d.state == Some(s)
        }
        _ => false,
    }
}

/// `true` when the trigger has no `by_skill`, or the unit is using it.
fn used_by(w: &mut dyn SkillHost, d: &TriggerDef, u: UnitId) -> bool {
    d.by_skill
        .is_none_or(|s| w.used_skill(u).is_some_and(|e| e.skill == i32::from(s)))
}

/// The skill and level the formulas see, when the trigger's source is
/// there: the held skill and its level, the stat (0 as the skill) and its
/// value, or the state (0 as the skill) and its stack count.
fn source(w: &mut dyn SkillHost, d: &TriggerDef, u: UnitId) -> Option<(i32, i32)> {
    if let Some(skill) = d.while_skill {
        let lvl = w.skill_level(u, i32::from(skill));
        return (lvl > 0).then_some((i32::from(skill), lvl));
    }
    if let Some(stat) = d.while_stat {
        let v = w.stat(u, stat, 0);
        return (v > 0).then_some((0, v));
    }
    let n = w.state_stacks(u, d.while_state?);
    (n > 0).then_some((0, i32::from(n)))
}

fn unit_of(who: Who, u: UnitId, other: Option<UnitId>) -> Option<UnitId> {
    match who {
        Who::Holder => Some(u),
        Who::Other => other,
    }
}

fn holds(w: &mut dyn SkillHost, c: &Condition, u: UnitId, other: Option<UnitId>) -> bool {
    match *c {
        Condition::HasState { state, who } => {
            unit_of(who, u, other).is_some_and(|t| w.state_stacks(t, state) > 0)
        }
        Condition::LacksState { state, who } => {
            unit_of(who, u, other).is_some_and(|t| w.state_stacks(t, state) == 0)
        }
        Condition::StacksAtLeast { state, count, who } => {
            unit_of(who, u, other).is_some_and(|t| w.state_stacks(t, state) >= count)
        }
        Condition::LifeBelow { percent, who } => {
            unit_of(who, u, other).is_some_and(|t| life_percent(w, t) < i64::from(percent))
        }
        Condition::LifeAbove { percent, who } => {
            unit_of(who, u, other).is_some_and(|t| life_percent(w, t) >= i64::from(percent))
        }
    }
}

/// Life as a percent of the maximum (100 for a unit with no maximum).
fn life_percent(w: &mut dyn SkillHost, t: UnitId) -> i64 {
    let max = i64::from(w.stat(t, MAX_LIFE, 0));
    if max <= 0 {
        return 100;
    }
    i64::from(w.stat(t, LIFE, 0)) * 100 / max
}

/// The percent roll. A chance of 100 or more always passes and rolls
/// nothing; 0 or less never does.
fn chance_passes(w: &mut dyn SkillHost, d: &TriggerDef, u: UnitId, skill: i32, lvl: i32) -> bool {
    let chance = d.chance.eval(w, u, skill, lvl);
    if chance >= 100 {
        return true;
    }
    chance > 0 && crate::skills::calc::rand(w.seed(u), 0, 99) < chance
}

fn act(w: &mut dyn SkillHost, a: &Action, u: UnitId, other: Option<UnitId>, skill: i32, lvl: i32) {
    let spec = |state: u16, seconds: &Option<super::Formula>, w: &mut dyn SkillHost| RuledState {
        state,
        skill,
        level: lvl,
        base_ticks: seconds.as_ref().map_or(0, |f| {
            f.eval(w, u, skill, lvl)
                .max(0)
                .saturating_mul(crate::TICKS_PER_SECOND as i32)
        }),
    };
    match a {
        Action::CallScript(spec) => {
            let context = crate::rules::scripts::CallContext {
                unit: u,
                other,
                skill,
                level: lvl,
                argument: spec.argument.eval(w, u, skill, lvl),
            };
            let _ = w.call_script(spec, context);
        }
        Action::ApplyState {
            state,
            seconds,
            who,
        } => {
            if let Some(t) = unit_of(*who, u, other) {
                let s = spec(*state, seconds, w);
                w.apply_ruled_state(u, t, &s);
            }
        }
        Action::AddStack {
            state,
            count,
            seconds,
            who,
        } => {
            if let Some(t) = unit_of(*who, u, other) {
                let s = spec(*state, seconds, w);
                let n = count.eval(w, u, skill, lvl).clamp(0, 255) as u16;
                w.add_state_stacks(u, t, &s, n);
            }
        }
        Action::RemoveStack { state, count, who } => {
            if let Some(t) = unit_of(*who, u, other) {
                let n = count.eval(w, u, skill, lvl).clamp(0, 255) as u16;
                w.remove_state_stacks(t, *state, n);
            }
        }
        Action::RemoveState { state, who } => {
            if let Some(t) = unit_of(*who, u, other) {
                w.clear_state(t, *state);
            }
        }
        Action::CrowdControl { effect, who } => {
            if let Some(t) = unit_of(*who, u, other) {
                w.crowd_control(u, t, std::slice::from_ref(effect));
            }
        }
        Action::CastSkill {
            skill: id,
            level,
            at,
        } => {
            let l = level.eval(w, u, skill, lvl).max(1);
            w.cast_skill(u, i32::from(*id), l, unit_of(*at, u, other));
        }
    }
}
