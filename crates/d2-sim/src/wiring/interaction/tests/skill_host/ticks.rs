//! The tick seam on the real wiring: a block skill's `repeat` and `leap`
//! parts put timers on the game's list (event 14, the mod tick callback), the
//! tick dispatch hands each back to the data mod when it is due, and a game
//! with no mod never schedules one.

use crate::missiles::unit_flag;
use crate::rules::HostTimers;
use crate::stats::stat as sst;
use crate::tick::events::event;
use crate::units::dispatch::MOD_TICK;
use crate::units::{UnitId, UnitType};

use super::blocks::{cast, fixture, install, life, LIFE, STATES};
use super::{fx, fx_with, skills, Fx};

/// Every tick the unit has waiting, as `(frame, skill, left)`.
fn waiting(fx: &mut Fx, u: UnitId) -> Vec<(i32, i32, u16)> {
    fx.sim
        .skill_use(&mut fx.game, |w| HostTimers::ticks(w, u))
        .into_iter()
        .map(|(f, t)| (f, t.skill, t.left))
        .collect()
}

/// The unit's timers that carry the mod tick callback.
fn tick_timers(fx: &Fx, u: UnitId) -> usize {
    let t = &fx.game.timers;
    t.unit_timers(u)
        .into_iter()
        .filter(|&id| {
            t.event(id)
                .is_some_and(|e| e.0 == event::REMOVE_SKILL_COOLDOWN)
        })
        .count()
}

fn frames(fx: &mut Fx, n: i32) {
    for _ in 0..n {
        fx.frame();
    }
}

/// A channel: skill 0 repeats skill 1 (an area of fire around the caster)
/// every 5 ticks, 3 runs.
const CHANNEL: &str = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"repeat\"\nskill = 1\n\
    every = 5\ntimes = 3\n[[skill]]\nid = 1\n[[skill.part]]\nkind = \"area_hit\"\n\
    radius = 6\naround = \"caster\"\n[skill.part.damage]\nelement = \"fire\"\n\
    min = 10\nmax = 10\n";

#[test]
fn a_channel_hits_now_and_then_once_per_period_through_the_timer_list() {
    let (mut fx, p, m, far) = fixture(CHANNEL);
    assert_eq!(cast(&mut fx, p, 1), 1);
    // The first run is in the cast; the other two wait on the timer list.
    assert_eq!(life(&mut fx, m), LIFE - 10 * 256);
    assert_eq!(tick_timers(&fx, p), 1);
    let at = fx.game.frame;
    assert_eq!(waiting(&mut fx, p), [(at + 5, 0, 2)]);
    frames(&mut fx, 4);
    assert_eq!(life(&mut fx, m), LIFE - 10 * 256);
    frames(&mut fx, 1);
    assert_eq!(life(&mut fx, m), LIFE - 20 * 256);
    assert_eq!(waiting(&mut fx, p), [(at + 10, 0, 1)]);
    frames(&mut fx, 5);
    assert_eq!(life(&mut fx, m), LIFE - 30 * 256);
    // Three runs in all; the list and the job store are empty again.
    assert_eq!(tick_timers(&fx, p), 0);
    assert!(fx.sim.sys.hooks.mod_ticks.is_empty());
    frames(&mut fx, 20);
    assert_eq!(life(&mut fx, m), LIFE - 30 * 256);
    assert_eq!(life(&mut fx, far), LIFE);
    fx.assert_clean();
}

#[test]
fn a_new_cast_replaces_the_channel_running() {
    let (mut fx, p, m, _) = fixture(CHANNEL);
    cast(&mut fx, p, 1);
    frames(&mut fx, 2);
    cast(&mut fx, p, 1);
    // One timer, not two: the first cast's was taken off the list.
    assert_eq!(tick_timers(&fx, p), 1);
    assert_eq!(fx.sim.sys.hooks.mod_ticks.len(), 1);
    frames(&mut fx, 30);
    assert_eq!(life(&mut fx, m), LIFE - 40 * 256);
    fx.assert_clean();
}

#[test]
fn a_caster_that_left_the_game_drops_its_ticks() {
    let (mut fx, p, m, _) = fixture(CHANNEL);
    cast(&mut fx, m, 1);
    assert_eq!(tick_timers(&fx, m), 1);
    fx.game.remove_unit(m).unwrap();
    // The next job pruned the dead unit's; the timer went with the unit.
    cast(&mut fx, p, 1);
    assert!(fx.sim.sys.hooks.mod_ticks.values().all(|j| j.unit == p));
    frames(&mut fx, 12);
    fx.assert_clean();
}

#[test]
fn a_leap_carries_the_caster_over_a_few_ticks_and_lands_with_a_blow() {
    let toml = "[[skill]]
id = 0
[[skill.part]]
kind = \"leap\"
frames = 6
land = 1
        [[skill]]
id = 1
[[skill.part]]
kind = \"area_hit\"
radius = 8
around = \"caster\"
        [skill.part.damage]
element = \"fire\"
min = 40
max = 40
";
    let mut fx = fx_with(skills());
    fx.sim.hooks().enable_paths().expect("embedded tables");
    let p = fx.spawn(UnitType::Player, 10, 10);
    let far = fx.spawn(UnitType::Monster, 30, 10);
    fx.set(
        far,
        &[(31, 1), (12, 1), (sst::MAXHP, LIFE), (sst::HITPOINTS, LIFE)],
    );
    fx.sim.sys.units.get_mut(far).unwrap().flags |=
        unit_flag::IS_VALID_TARGET | unit_flag::CAN_BE_ATTACKED;
    install(&mut fx, toml, STATES);
    fx.sim.sys.hooks.x.aim_at = (25, 10);
    assert_eq!(cast(&mut fx, p, 1), 1);
    // Nothing moved in the cast itself.
    assert_eq!(fx.sim.sys.hooks.path_position(p), (10, 10));
    assert_eq!(life(&mut fx, far), LIFE);
    frames(&mut fx, 2);
    assert_eq!(fx.sim.sys.hooks.path_position(p), (15, 10));
    frames(&mut fx, 2);
    assert_eq!(fx.sim.sys.hooks.path_position(p), (20, 10));
    assert_eq!(life(&mut fx, far), LIFE);
    frames(&mut fx, 2);
    assert_eq!(fx.sim.sys.hooks.path_position(p), (25, 10));
    // Landed within 8 of the far monster at (30, 10): the blow hit it, once.
    assert_eq!(life(&mut fx, far), LIFE - 40 * 256);
    // (The placements left timers of their own on the list.)
    assert!(fx.sim.sys.hooks.mod_ticks.is_empty());
    frames(&mut fx, 10);
    assert_eq!(life(&mut fx, far), LIFE - 40 * 256);
    fx.assert_clean();
}

#[test]
fn a_game_without_a_mod_never_schedules_a_tick() {
    let mut fx = fx();
    let p = fx.spawn(UnitType::Player, 10, 10);
    // Under D2Gamma's own rules the block slot is empty and the seam is
    // unused: nothing is kept and no timer carries the callback.
    let r = fx.sim.skill_use(&mut fx.game, |w| {
        use crate::skills::use_::SkillFunctions;
        w.srvdo(crate::rules::BLOCK_SLOT, p, 0, 1, false, false, false)
    });
    assert_eq!(r, 0);
    frames(&mut fx, 30);
    assert!(fx.sim.sys.hooks.mod_ticks.is_empty());
    assert_eq!(tick_timers(&fx, p), 0);
    // The callback is not one the base game uses.
    assert_ne!(MOD_TICK, crate::units::dispatch::SKILL_COOLDOWN);
    fx.assert_clean();
}
