//! Running a behaviour graph: the [`MonsterProvider`] the data mod host
//! answers with.
//!
//! The provider gives each graph an AI table row (so monsters whose `AI`
//! value matches install it like any other AI), resets a monster's place in
//! its graph when it installs, and runs the graph on each think. The think is
//! the built-in one: the prechecks and the target search already ran, so the
//! graph starts from a [`TickParam`] with the target, its distance and
//! whether it is in melee range, and uses the same movement, skill and idle
//! helpers D2Gamma's own AIs do.

use super::{
    ActionDef, ActionKind, AiDef, AiRules, Condition, StateDef, Whose, INIT_BASE, MAX_HOPS,
    THINK_BASE,
};
use crate::game::Game;
use crate::monsters::ai::census::{self, UndeadScan};
use crate::monsters::ai::graph::{GraphEvent, GraphEventKind, GraphRun, WindUp};
use crate::monsters::ai::AI_TABLE;
use crate::monsters::ai::{
    self, delete_thinks, flag, idle, idle_raw, mode, mode_at, schedule_think, AiHost, AiRecord,
    Ctx, ModeTarget, TickParam, UnitRef,
};
use crate::monsters::provider::MonsterProvider;
use crate::units::{UnitId, UnitType};

/// Spawn flags the built-in spawners give the monsters they create.
const SPAWN_FLAGS: u32 = 0x42;
/// Most pack members one condition or action looks at.
const MAX_PACK_SCAN: usize = 32;
/// The default radius of the undead scan, in subtiles.
const SCAN_RADIUS: i32 = 24;
/// How far from its owner a following pet may stray, in subtiles.
const FOLLOW_STEPS: i32 = 7;
/// The default size of a hop, in subtiles.
const HOP_STEPS: i32 = 12;
/// The default distance of a flee, in subtiles.
const FLEE_STEPS: i32 = 20;
/// The default distance of a move away, in subtiles.
const AWAY_STEPS: i32 = 12;
/// The default size of a circling step, in subtiles.
const AROUND_STEPS: i32 = 3;
/// The default radius of a wander and of waking the pack, in subtiles.
const WANDER_RADIUS: i32 = 6;
const CALL_RADIUS: i32 = 20;
/// The default spread of a summon.
const SUMMON_SPREAD: i32 = 2;

impl AiRules {
    fn index_of_think(&self, function: u32) -> Option<usize> {
        let i = function.checked_sub(THINK_BASE)? as usize;
        (i < self.defs.len() && !self.defs[i].is_preset()).then_some(i)
    }

    fn index_of_init(&self, function: u32) -> Option<usize> {
        let i = function.checked_sub(INIT_BASE)? as usize;
        (i < self.defs.len() && function < INIT_BASE + 0x0100_0000 && !self.defs[i].is_preset())
            .then_some(i)
    }
}

impl MonsterProvider for AiRules {
    fn ai_record(&self, ai: u16) -> Option<AiRecord> {
        let i = self.position(ai)?;
        if let Some(row) = self.defs[i].preset_row {
            return AI_TABLE.get(usize::from(row)).copied();
        }
        Some(AiRecord {
            target_mode: self.defs[i].target_or_default().mode(),
            init: INIT_BASE + i as u32,
            think: THINK_BASE + i as u32,
            alt: 0,
        })
    }

    fn aip(&self, ai: u16, n: usize, difficulty: u8) -> Option<i32> {
        self.get(ai)?.param(n, difficulty)
    }

    fn ai_init(
        &self,
        game: &mut Game,
        cx: &mut Ctx<'_, dyn AiHost + '_>,
        function: u32,
        unit: UnitId,
    ) -> bool {
        let Some(i) = self.index_of_init(function) else {
            return false;
        };
        let def = &self.defs[i];
        cx.store.entry(unit).graph = Some(GraphRun::new(def.index, def.start_state, game.frame));
        true
    }

    fn ai_think(
        &self,
        game: &mut Game,
        cx: &mut Ctx<'_, dyn AiHost + '_>,
        function: u32,
        unit: UnitId,
        p: &TickParam,
    ) -> bool {
        self.think_cost(game, cx, function, unit, p).is_some()
    }
}

impl AiRules {
    /// Runs one think of the graph whose think id is `function`; the number
    /// of conditions and actions it looked at, or `None` when `function` is
    /// not a graph's.
    pub(crate) fn think_cost(
        &self,
        game: &mut Game,
        cx: &mut Ctx<'_, dyn AiHost + '_>,
        function: u32,
        unit: UnitId,
        p: &TickParam,
    ) -> Option<u32> {
        let i = self.index_of_think(function)?;
        Some(think(self, i, game, cx, unit, p).steps)
    }
}

/// What one think looked at, for the cost budget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cost {
    /// Conditions evaluated plus actions tried.
    pub steps: u32,
}

/// Runs one think of `def` for `unit` and returns what it cost.
pub(super) fn think(
    rules: &AiRules,
    at: usize,
    game: &mut Game,
    cx: &mut Ctx<'_, dyn AiHost + '_>,
    unit: UnitId,
    p: &TickParam,
) -> Cost {
    let def = &rules.defs[at];
    let mut run = match cx.store.get(unit).and_then(|m| m.graph) {
        Some(r) if r.ai == def.index => r,
        _ => GraphRun::new(def.index, def.start_state, game.frame),
    };
    run.thinks = run.thinks.saturating_add(1);
    let mut cost = Cost::default();
    let mut walk = Walk {
        rules,
        def,
        game,
        cx,
        unit,
        p,
        cost: &mut cost,
        second: None,
        scan: None,
        owner_target: None,
    };
    if let Some(next) = walk.phases(&mut run) {
        walk.switch_graph(next);
        return cost;
    }
    walk.hops(&mut run);
    let state = &def.states[usize::from(run.state)];
    if !walk.actions(&mut run, state) {
        let n = idle_delay(walk.cx, unit);
        idle(walk.game, walk.cx, unit, n);
    }
    walk.cx.store.entry(unit).graph = Some(run);
    cost
}

fn idle_delay(cx: &Ctx<'_, dyn AiHost + '_>, unit: UnitId) -> i32 {
    cx.aidel(cx.world.class(unit))
}

struct Walk<'a, 'c, 'w> {
    rules: &'a AiRules,
    def: &'a AiDef,
    game: &'a mut Game,
    cx: &'a mut Ctx<'c, dyn AiHost + 'w>,
    unit: UnitId,
    p: &'a TickParam,
    cost: &'a mut Cost,
    /// The second target and its distance, found on first use in the think.
    second: Option<(Option<UnitId>, i32)>,
    /// The undead scan and the radius it was made with, run on first use.
    scan: Option<(i32, UndeadScan)>,
    /// What the owner's own target search finds, run on first use.
    owner_target: Option<Option<UnitId>>,
}

impl Walk<'_, '_, '_> {
    /// Starts the phases whose time has come, in order, at most [`MAX_HOPS`]
    /// in one think. `Some(position)`: the last one switched the monster to
    /// the graph at that position in the file.
    fn phases(&mut self, run: &mut GraphRun) -> Option<usize> {
        for _ in 0..MAX_HOPS {
            let def = self.def;
            let n = run.phase;
            let phase = def.phases.get(usize::from(n))?;
            if !self.all(run, &phase.when) {
                return None;
            }
            run.phase += 1;
            self.report(GraphEventKind::PhaseChanged { phase: n });
            if let Some(ticks) = phase.immune_ticks {
                self.immune_for(ticks);
            }
            if let Some(to) = phase.state_index {
                if to != run.state {
                    self.report(GraphEventKind::StateChanged {
                        from: run.state,
                        to,
                    });
                    run.state = to;
                    run.entered = self.game.frame;
                    run.thinks = 1;
                    run.windup = None;
                }
            }
            if phase.graph_position.is_some() {
                return phase.graph_position;
            }
        }
        None
    }

    /// Hands the monster to the graph at `position` in the file: it starts
    /// in that graph's first state and takes its first turn a moment later.
    fn switch_graph(&mut self, position: usize) {
        let next = &self.rules.defs[position];
        let run = GraphRun::new(next.index, next.start_state, self.game.frame);
        self.cx.store.entry(self.unit).graph = Some(run);
        if let Some(c) = self.cx.store.control_mut(self.unit) {
            c.function = THINK_BASE + position as u32;
        }
        idle_raw(self.game, self.cx, self.unit, 1);
    }

    /// Makes the monster immune to crowd control for `ticks`, if the mod
    /// names the state that does.
    fn immune_for(&mut self, ticks: i32) {
        if let Some(state) = self.cx.world.rules().cc_immune_state() {
            self.cx
                .world
                .timed_state(self.game, self.unit, state, ticks);
        }
    }

    fn report(&mut self, kind: GraphEventKind) {
        self.cx.store.events.push(GraphEvent {
            frame: self.game.frame,
            unit: self.unit,
            ai: self.def.index,
            kind,
        });
    }

    /// Follows transitions until none holds, at most [`MAX_HOPS`].
    fn hops(&mut self, run: &mut GraphRun) {
        for _ in 0..MAX_HOPS {
            let def = self.def;
            let state = &def.states[usize::from(run.state)];
            let Some(t) = state.transitions.iter().find(|t| self.all(run, &t.when)) else {
                return;
            };
            self.cx.store.events.push(GraphEvent {
                frame: self.game.frame,
                unit: self.unit,
                ai: self.def.index,
                kind: GraphEventKind::StateChanged {
                    from: run.state,
                    to: t.target,
                },
            });
            run.state = t.target;
            run.entered = self.game.frame;
            run.thinks = 1;
            run.windup = None;
        }
    }

    /// Tries the actions in order; `true` when one took the turn.
    fn actions(&mut self, run: &mut GraphRun, state: &StateDef) -> bool {
        for (n, a) in state.actions.iter().enumerate() {
            self.cost.steps += 1;
            if !self.all(run, &a.when) {
                continue;
            }
            let acted = self.act(run, n as u8, a);
            if !acted {
                continue;
            }
            if let Some(c) = a.cooldown {
                let ticks = self.def.cooldowns[usize::from(c)].ticks;
                run.ready_at[usize::from(c)] = self.game.after(ticks);
            }
            if !a.kind.is_instant() {
                return true;
            }
        }
        false
    }

    fn all(&mut self, run: &GraphRun, list: &[Condition]) -> bool {
        list.iter().all(|c| {
            self.cost.steps += 1;
            self.holds(run, c)
        })
    }

    fn holds(&mut self, run: &GraphRun, c: &Condition) -> bool {
        let (u, p) = (self.unit, self.p);
        let target = p.target;
        match c {
            Condition::HasTarget => target.is_some(),
            Condition::NoTarget => target.is_none(),
            Condition::DistanceBelow { value } => target.is_some() && p.distance < *value,
            Condition::DistanceAbove { value } => target.is_some() && p.distance >= *value,
            Condition::InMelee => target.is_some() && p.combat,
            Condition::OutOfMelee => target.is_none() || !p.combat,
            Condition::HasOwner => self.owner().is_some(),
            Condition::NoOwner => self.owner().is_none(),
            Condition::OwnerDistanceBelow { value } => {
                self.owner().is_some_and(|o| self.near(o) < *value)
            }
            Condition::OwnerDistanceAbove { value } => {
                self.owner().is_some_and(|o| self.near(o) >= *value)
            }
            Condition::OwnerTarget => self.owner_target().is_some(),
            Condition::SecondTarget => self.second().0.is_some(),
            Condition::SecondDistanceBelow { value } => {
                let (s, e) = self.second();
                s.is_some() && e < *value
            }
            Condition::SecondDistanceAbove { value } => {
                let (s, e) = self.second();
                s.is_some() && e >= *value
            }
            Condition::UndeadWithin { radius, at_least } => {
                self.undead(*radius).seen >= i32::from(*at_least)
            }
            Condition::CorpseWithin { radius } => self.undead(*radius).corpse.is_some(),
            Condition::HurtWithin { radius } => self.undead(*radius).hurt.is_some(),
            Condition::ClassCountBelow { class, also, count } => {
                self.class_count(*class, *also) < usize::from(*count)
            }
            Condition::ClassCountAtLeast { class, also, count } => {
                self.class_count(*class, *also) >= usize::from(*count)
            }
            Condition::ThinksAtLeast { count } => run.thinks >= *count,
            Condition::LineOfSight => {
                target.is_some_and(|t| !self.cx.world.line_blocked(self.game, u, t))
            }
            Condition::NoLineOfSight => {
                target.is_some_and(|t| self.cx.world.line_blocked(self.game, u, t))
            }
            Condition::LifeBelow { percent } => self.cx.world.life_percent(u) < i32::from(*percent),
            Condition::LifeAbove { percent } => {
                self.cx.world.life_percent(u) >= i32::from(*percent)
            }
            Condition::HasState { state, who } => self
                .whose(*who)
                .is_some_and(|w| self.cx.world.has_state(w, *state)),
            Condition::LacksState { state, who } => self
                .whose(*who)
                .is_some_and(|w| !self.cx.world.has_state(w, *state)),
            Condition::StacksAtLeast { state, count, who } => self
                .whose(*who)
                .is_some_and(|w| self.cx.world.state_stacks(w, *state) >= *count),
            Condition::AlliesWithin { radius, at_least } => {
                let near = self.pack().into_iter().filter(|&m| self.within(m, *radius));
                near.count() >= usize::from(*at_least)
            }
            Condition::EnemiesWithin { radius, at_least } => {
                let lists = self.cx.world.target_nodes(self.game);
                let near = lists
                    .players
                    .iter()
                    .flatten()
                    .chain(&lists.good)
                    .filter(|&&e| !self.cx.world.is_dead(e) && self.within(e, *radius));
                near.count() >= usize::from(*at_least)
            }
            Condition::CooldownReady { index, .. } => {
                self.game
                    .frame
                    .wrapping_sub(run.ready_at[usize::from(*index)])
                    >= 0
            }
            Condition::AliveFor { ticks } => {
                self.game.frame.wrapping_sub(run.born) >= self.game.ticks(*ticks)
            }
            Condition::InStateFor { ticks } => {
                self.game.frame.wrapping_sub(run.entered) >= self.game.ticks(*ticks)
            }
            Condition::Random { percent } => self.cx.chance(u, i32::from(*percent)),
        }
    }

    fn whose(&mut self, who: Whose) -> Option<UnitId> {
        match who {
            Whose::Itself => Some(self.unit),
            Whose::Target => self.p.target,
            Whose::Second => self.second().0,
            Whose::Corpse => self.undead(SCAN_RADIUS).corpse,
            Whose::Hurt => self.undead(SCAN_RADIUS).hurt,
            Whose::Owner => self.owner(),
            Whose::OwnerTarget => self.owner_target(),
        }
    }

    /// The unit the monster is a minion of.
    fn owner(&self) -> Option<UnitId> {
        ai::minion_owner(self.game, self.cx, self.unit)
    }

    /// The distance to `other`, as the pet AIs measure it.
    fn near(&self, other: UnitId) -> i32 {
        ai::unit_distance(self.cx, self.unit, other)
    }

    /// The enemy the owner's target search finds, searched once per think.
    fn owner_target(&mut self) -> Option<UnitId> {
        if let Some(found) = self.owner_target {
            return found;
        }
        let found = self
            .owner()
            .and_then(|o| census::owner_target(self.game, self.cx, self.unit, o));
        self.owner_target = Some(found);
        found
    }

    /// The undead scan within `radius`, made once per think and radius.
    fn undead(&mut self, radius: i32) -> UndeadScan {
        match self.scan {
            Some((r, found)) if r == radius => found,
            _ => {
                let found = census::undead_scan(self.game, self.cx, self.unit, radius);
                self.scan = Some((radius, found));
                found
            }
        }
    }

    fn class_count(&self, class: u16, also: Option<u16>) -> usize {
        let classes: Vec<u16> = std::iter::once(class).chain(also).collect();
        census::count_of_classes(self.game, self.cx, self.unit, &classes)
    }

    /// The second target and its distance, searched once per think.
    fn second(&mut self) -> (Option<UnitId>, i32) {
        if let Some(found) = self.second {
            return found;
        }
        let (s, e, _) = self
            .cx
            .world
            .secondary_target(self.game, self.unit, self.cx.store);
        self.second = Some((s, e));
        (s, e)
    }

    fn within(&self, other: UnitId, radius: i32) -> bool {
        ai::unit_distance(self.cx, self.unit, other) <= radius
    }

    /// The living members of the monster's pack other than itself: its
    /// leader and the leader's minions.
    fn pack(&self) -> Vec<UnitId> {
        let Some(control) = self.cx.store.control(self.unit) else {
            return Vec::new();
        };
        let leader = if control.minions.is_empty() {
            ai::minion_owner(self.game, self.cx, self.unit)
        } else {
            Some(self.unit)
        };
        let Some(leader) = leader else {
            return Vec::new();
        };
        let guids = self
            .cx
            .store
            .control(leader)
            .map(|c| c.minions.clone())
            .unwrap_or_default();
        std::iter::once(leader)
            .chain(
                guids
                    .into_iter()
                    .filter_map(|g| self.game.lists.find_unit(UnitType::Monster, g)),
            )
            .filter(|&m| m != self.unit && !self.cx.world.is_dead(m))
            .take(MAX_PACK_SCAN)
            .collect()
    }

    /// Does the action. `false`: it could not be done now and the walk
    /// through the list goes on.
    fn act(&mut self, run: &mut GraphRun, index: u8, a: &ActionDef) -> bool {
        let aim = match a.at {
            Some(Whose::Second) => self.second().0,
            Some(Whose::Corpse) => self.undead(a.radius.unwrap_or(SCAN_RADIUS)).corpse,
            Some(Whose::Hurt) => self.undead(a.radius.unwrap_or(SCAN_RADIUS)).hurt,
            Some(Whose::Owner) => self.owner(),
            Some(Whose::OwnerTarget) => self.owner_target(),
            _ => self.p.target,
        };
        let (game, cx, u) = (&mut *self.game, &mut *self.cx, self.unit);
        let target = self.p.target;
        match a.kind {
            ActionKind::MoveTo => {
                let Some(t) = aim else { return false };
                if let Some(speed) = a.speed {
                    ai::set_velocity(cx, u, 0, speed, 0);
                }
                let m = if a.run.unwrap_or(false) {
                    mode::RUN
                } else {
                    mode::WALK
                };
                let steps = a.steps.unwrap_or(1);
                ai::move_to(game, cx, u, ModeTarget::Unit(t), m, steps, MOVE_FLAGS);
                true
            }
            ActionKind::HopTo => {
                let Some(t) = aim else { return false };
                if let Some(speed) = a.speed {
                    ai::set_velocity(cx, u, 0, speed, 0);
                }
                let hop = a.steps.unwrap_or(HOP_STEPS);
                ai::walk_in_radius(game, cx, u, t, hop, a.keep.unwrap_or(0));
                true
            }
            ActionKind::FollowOwner => {
                let Some(o) = ai::minion_owner(game, cx, u) else {
                    return false;
                };
                let quiet = a.quiet.unwrap_or(false);
                let steps = a.steps.unwrap_or(FOLLOW_STEPS);
                census::follow_owner(game, cx, u, aim, o, self.p.combat, quiet, steps)
            }
            ActionKind::MoveAway => ai::escape(
                game,
                cx,
                u,
                target,
                a.steps.unwrap_or(AWAY_STEPS),
                true,
                a.run.unwrap_or(false),
            ),
            ActionKind::Flee => ai::escape(
                game,
                cx,
                u,
                target,
                a.steps.unwrap_or(FLEE_STEPS),
                true,
                true,
            ),
            ActionKind::MoveAround => {
                if target.is_none() {
                    return false;
                }
                ai::circle(game, cx, u, target, a.steps.unwrap_or(AROUND_STEPS), true);
                true
            }
            ActionKind::Attack => {
                let Some(t) = aim else { return false };
                let m = if a.attack == Some(2) {
                    mode::ATTACK2
                } else {
                    mode::ATTACK1
                };
                mode_at(game, cx, u, m, Some(t));
                true
            }
            ActionKind::UseSkill => {
                let (skill, skill_mode) = cx.skill(self.p, usize::from(a.slot.unwrap_or(1)));
                let at = match a.at.unwrap_or(Whose::Target) {
                    Whose::Itself => ModeTarget::Unit(u),
                    Whose::Target
                    | Whose::Second
                    | Whose::Corpse
                    | Whose::Hurt
                    | Whose::Owner
                    | Whose::OwnerTarget => match aim {
                        Some(t) => ModeTarget::Unit(t),
                        None => return false,
                    },
                };
                if skill < 0 {
                    return false;
                }
                ai::use_skill(game, cx, u, skill_mode, skill, at);
                true
            }
            ActionKind::Wander => {
                ai::wander(game, cx, u, a.radius.unwrap_or(WANDER_RADIUS));
                true
            }
            ActionKind::Idle => {
                let n = a.ticks.unwrap_or_else(|| idle_delay(cx, u));
                idle(game, cx, u, n);
                true
            }
            ActionKind::WindUp => self.wind_up(run, index, a),
            ActionKind::CallAllies => {
                self.call_allies(a.radius.unwrap_or(CALL_RADIUS));
                true
            }
            ActionKind::Summon => self.summon(a),
            ActionKind::SetState => {
                let state = a.state.unwrap_or(0);
                cx.world.set_state(game, u, state, a.on.unwrap_or(true));
                true
            }
        }
    }

    /// A wind-up: starts it (and reports it), holds the monster still while
    /// it runs, and lets the list go on once it is over.
    fn wind_up(&mut self, run: &mut GraphRun, index: u8, a: &ActionDef) -> bool {
        let ticks = a.ticks.unwrap_or(0);
        let frames = self.game.ticks(ticks);
        match run.windup {
            Some(w) if w.action == index => {
                let left = w.until.wrapping_sub(self.game.frame);
                if left <= 0 {
                    run.windup = None;
                    return false;
                }
                idle_raw(self.game, self.cx, self.unit, left);
                true
            }
            _ => {
                if ticks == 0 {
                    return false;
                }
                run.windup = Some(WindUp {
                    action: index,
                    until: self.game.frame.wrapping_add(frames),
                });
                self.cx.store.events.push(GraphEvent {
                    frame: self.game.frame,
                    unit: self.unit,
                    ai: self.def.index,
                    kind: GraphEventKind::WindUp {
                        event: a.event_index.unwrap_or(0),
                        ticks,
                    },
                });
                idle_raw(self.game, self.cx, self.unit, frames);
                true
            }
        }
    }

    /// Wakes the pack mates within `radius` that are standing idle: they
    /// think at once and see the target whether or not they could.
    fn call_allies(&mut self, radius: i32) {
        for m in self.pack() {
            if !self.within(m, radius) || self.cx.world.anim_mode(m) != mode::NEUTRAL {
                continue;
            }
            if let Some(c) = self.cx.store.control_mut(m) {
                c.flags |= flag::FORCE_LOS;
            }
            delete_thinks(self.game, m);
            let at = self.game.frame.wrapping_add(1);
            schedule_think(self.game, self.cx, m, at);
        }
    }

    /// Creates the monsters and makes them the summoner's minions.
    fn summon(&mut self, a: &ActionDef) -> bool {
        let class = i32::from(a.class.unwrap_or(0));
        let (game, cx, u) = (&mut *self.game, &mut *self.cx, self.unit);
        let Some(room) = game.lists.unit(u).and_then(|e| e.room()) else {
            return false;
        };
        let leader = game.lists.unit(u).map(|e| UnitRef {
            ty: e.ty,
            guid: e.guid,
        });
        let mut made = false;
        for _ in 0..a.count.unwrap_or(1) {
            let Some((x, y)) = cx.world.free_spot(game, Some(room), 0, class) else {
                break;
            };
            let spread = a.spread.unwrap_or(SUMMON_SPREAD);
            let Some(m) =
                cx.spawn_monster(game, room, x, y, class, mode::NEUTRAL, spread, SPAWN_FLAGS)
            else {
                break;
            };
            made = true;
            let guid = game.lists.unit(m).map_or(0, |e| e.guid);
            if let Some(ctl) = cx.store.control_mut(u) {
                if ctl.minion_owner.is_none() {
                    ctl.minion_owner = leader;
                }
                ctl.minions.push(guid);
            }
            if let Some(ctl) = cx.store.control_mut(m) {
                ctl.minion_owner = leader;
            }
        }
        made
    }
}

/// Fail-safe flags of a move toward the target: line of sight is forced on
/// the next search, a failed path falls back to a short wander or a wait,
/// and the pending thinks are dropped. The built-in walkers use the same.
const MOVE_FLAGS: u32 =
    ai::move_flag::FORCE_LOS | ai::move_flag::FALLBACK | ai::move_flag::DELETE_THINKS;
