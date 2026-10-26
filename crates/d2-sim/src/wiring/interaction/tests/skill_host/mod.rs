//! Tests for [`crate::rules::SkillHost`] on the skill use view: a mod
//! game's skill body (a [`Probe`] on a do slot past the tables) is handed
//! the view, reads or changes the game through one part of the host, and
//! its answer comes back through the do core. Under D2RUST's own rules
//! the slot is empty and the probe never runs. The fixture is the skill
//! use tests' (two 8×8 preset rooms, streamed).

use std::fmt;
use std::sync::{Arc, Mutex};

use crate::rules::{ModRules, Ruleset, SkillFn, SkillHost, SkillProvider};
use crate::skills::use_::SkillFunctions;
use crate::skills::SkillTables;
use crate::units::UnitId;

use super::skill_use::{skills, Fx};
use super::stat_data;

mod damage;
mod missiles;
mod skills;
mod states;
mod timers;
mod units;

/// The do slot the probe answers: past the tables, so D2RUST has no body
/// there.
const SLOT: u16 = 200;

type Body = dyn Fn(&mut dyn SkillHost, UnitId, i32, i32) -> String + Send + Sync;

/// A mod game whose do slot [`SLOT`] runs `body` and keeps its answer.
struct Probe {
    body: Box<Body>,
    answer: Mutex<Option<String>>,
    /// The `(unit, state)` of each [`SkillProvider::state_removed`] call.
    removed: Mutex<Vec<(UnitId, u16)>>,
}

impl fmt::Debug for Probe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Probe")
    }
}

impl SkillProvider for Probe {
    fn has(&self, kind: SkillFn, index: u16) -> bool {
        kind == SkillFn::Do && index == SLOT
    }
    fn srvdo(
        &self,
        w: &mut dyn SkillHost,
        _index: u16,
        u: UnitId,
        skill: i32,
        lvl: i32,
        _charge: bool,
        _item: bool,
        _aim: bool,
    ) -> Option<i32> {
        let a = (self.body)(w, u, skill, lvl);
        *self.answer.lock().unwrap() = Some(a);
        Some(1)
    }
    fn state_removed(&self, w: &mut dyn SkillHost, u: UnitId, state: u16) {
        w.state_on(u, state, false);
        self.removed.lock().unwrap().push((u, state));
    }
}

impl ModRules for Probe {
    fn name(&self) -> &str {
        "probe"
    }
    fn skills(&self) -> Option<&dyn SkillProvider> {
        Some(self)
    }
}

/// The fixture on `t`.
fn fx_with(t: SkillTables) -> Fx {
    Fx::with(stat_data(), t)
}

fn fx() -> Fx {
    fx_with(skills())
}

/// Installs a probe mod running `body`; it stays the game's rules until
/// [`uninstall`].
fn install(
    fx: &mut Fx,
    body: impl Fn(&mut dyn SkillHost, UnitId, i32, i32) -> String + Send + Sync + 'static,
) -> Arc<Probe> {
    let p = Arc::new(Probe {
        body: Box::new(body),
        answer: Mutex::new(None),
        removed: Mutex::new(Vec::new()),
    });
    fx.sim.sys.hooks.rules = Ruleset::Mod(p.clone());
    p
}

fn uninstall(fx: &mut Fx) {
    fx.sim.sys.hooks.rules = Ruleset::Original;
}

/// Runs the installed probe's body as unit `u`'s do body of `skill` at
/// `lvl`: the body's answer, `None` when it never ran.
fn run(fx: &mut Fx, p: &Probe, u: UnitId, skill: i32, lvl: i32) -> Option<String> {
    let r = fx.sim.skill_use(&mut fx.game, |w| {
        w.srvdo(SLOT, u, skill, lvl, false, false, false)
    });
    let a = p.answer.lock().unwrap().take();
    assert_eq!(
        r,
        i32::from(a.is_some()),
        "the core returns the body's result"
    );
    a
}

/// Runs `body` once as the mod's do body of unit `u` with `skill` at
/// `lvl`, under the probe's rules: the body's answer, `None` when it never
/// ran.
fn probe(
    fx: &mut Fx,
    u: UnitId,
    skill: i32,
    lvl: i32,
    body: impl Fn(&mut dyn SkillHost, UnitId, i32, i32) -> String + Send + Sync + 'static,
) -> Option<String> {
    let p = install(fx, body);
    let a = run(fx, &p, u, skill, lvl);
    uninstall(fx);
    a
}

#[test]
fn original_rules_have_no_body_on_the_probe_slot() {
    let mut fx = fx();
    let p = fx.spawn(crate::units::UnitType::Player, 3, 3);
    let r = fx.sim.skill_use(&mut fx.game, |w| {
        w.srvdo(SLOT, p, 0, 1, false, false, false)
    });
    assert_eq!(r, 0);
    fx.assert_clean();
}
