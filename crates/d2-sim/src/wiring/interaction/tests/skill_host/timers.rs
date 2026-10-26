//! [`crate::rules::HostTimers`] through a mod body.

use crate::units::UnitType;

use super::{fx, probe};

/// The skill delay end event (type 5).
const DELAY: u8 = 5;

#[test]
fn a_mod_body_schedules_lists_and_removes_timers() {
    let mut fx = fx();
    let p = fx.spawn(UnitType::Player, 10, 10);
    let frame = fx.game.frame;
    let got = probe(&mut fx, p, 7, 3, move |w, u, skill, lvl| {
        w.schedule_timer(u, DELAY, frame + 10, skill, lvl);
        w.schedule_timer(u, DELAY, frame + 20, 8, 1);
        let mut all = w.timers(u, DELAY);
        all.sort();
        w.remove_timers(u, DELAY, Some(skill));
        let left = w.timers(u, DELAY);
        format!("{all:?} {left:?}")
    });
    assert_eq!(
        got,
        Some(format!(
            "{:?} {:?}",
            [(frame + 10, 7, 3), (frame + 20, 8, 1)],
            [(frame + 20, 8, 1)]
        ))
    );
    // The view's timers are the game's.
    let t = &fx.game.timers;
    let ev: Vec<_> = t
        .unit_timers(p)
        .into_iter()
        .filter_map(|id| t.event(id))
        .filter(|e| e.0 == DELAY)
        .collect();
    assert_eq!(ev, [(DELAY, 8, 1)]);
    let got = probe(&mut fx, p, 7, 3, |w, u, _, _| {
        w.remove_timers(u, DELAY, None);
        format!("{:?}", w.timers(u, DELAY))
    });
    assert_eq!(got.as_deref(), Some("[]"));
    fx.assert_clean();
}
