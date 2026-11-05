//! A mod game's triggers on the skill use view: the unit events the engine
//! raises are handed to the game's skill provider
//! ([`crate::rules::SkillProvider::unit_event`]), with the depth limit that
//! keeps triggers from setting off each other without end.

use crate::rules::{UnitEvent, MAX_DEPTH};
use crate::units::UnitId;
use crate::wiring::action::Pending;
use crate::wiring::interaction::{UseRest, UseView};

impl<X: Pending + UseRest> UseView<'_, X> {
    /// Tells the game's skill provider that `ev` happened to `u`, when it
    /// wants to know.
    pub(in crate::wiring::interaction) fn fire_unit_event(
        &mut self,
        ev: UnitEvent,
        u: UnitId,
        other: Option<UnitId>,
    ) {
        let rules = self.cv.v.h.rules.clone();
        if let Some(p) = rules.skills().filter(|p| p.wants_unit_events()) {
            p.unit_event(self, ev, u, other);
        }
        use crate::rules::scripts::EventKind as K;
        let (kind, id) = match ev {
            UnitEvent::Attack => (K::AttackStart, 0),
            UnitEvent::Hit => (K::AttackHit, 0),
            UnitEvent::BeingHit => (K::HitTaken, 0),
            UnitEvent::SkillUsed(id) => (K::SkillUsed, id),
            UnitEvent::Kill => (K::Kill, 0),
            UnitEvent::StateStart(_) | UnitEvent::StateEnd(_) => return,
        };
        self.script_event(kind, u, other, id);
    }

    /// Whether the event comes from the game and not from a trigger's own
    /// action.
    pub(in crate::wiring::interaction) fn outside_triggers(&self) -> bool {
        self.cv.v.h.trigger_depth == 0
    }

    /// One level deeper into trigger actions; false at [`MAX_DEPTH`].
    pub(in crate::wiring::interaction) fn enter(&mut self) -> bool {
        let h = &mut *self.cv.v.h;
        if h.trigger_depth >= MAX_DEPTH {
            return false;
        }
        h.trigger_depth += 1;
        true
    }

    pub(in crate::wiring::interaction) fn leave(&mut self) {
        let h = &mut *self.cv.v.h;
        h.trigger_depth = h.trigger_depth.saturating_sub(1);
    }
}

impl<X: Pending + UseRest> UseView<'_, X> {
    pub(in crate::wiring::interaction) fn script_event(
        &mut self,
        kind: crate::rules::scripts::EventKind,
        unit: UnitId,
        other: Option<UnitId>,
        id: i32,
    ) {
        if let Some(runtime) = self.cv.v.h.scripts.clone() {
            runtime.event(
                self,
                crate::rules::scripts::Event {
                    kind,
                    unit,
                    other,
                    id,
                },
            );
        }
    }
}

impl<X: Pending + UseRest> crate::wiring::action::ActionSim<X> {
    /// Deliver cleanup/placement events, then due script timers, once per tick.
    pub fn run_scripts(&mut self, game: &mut crate::game::Game) {
        let Some(runtime) = self.sys.hooks.scripts.clone() else {
            return;
        };
        let pending = std::mem::take(&mut self.sys.hooks.script_events);
        self.combat(game, |cv, _| {
            let mut view = UseView {
                cv: crate::wiring::action::combat::CombatView {
                    game: &mut *cv.game,
                    v: crate::wiring::action::View::of(
                        &mut *cv.v.units,
                        &mut *cv.v.stats,
                        cv.v.data,
                        &mut *cv.v.h,
                    ),
                },
            };
            for event in pending {
                runtime.event(&mut view, event);
            }
            runtime.tick(&mut view);
        });
    }
}
