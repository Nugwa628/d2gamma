//! A data mod's quests on the real wiring: the kill, the level change, the
//! periodic hook and the save hooks of a game reach the quest rules, the
//! reward lands on the player, and the progress comes back from the save.

use std::sync::Arc;

use crate::rules::data_mod::{DataMod, DataModFiles};
use crate::rules::Ruleset;
use crate::tick::TickHooks;
use crate::units::UnitType;
use crate::wiring::action::reaction::kill_by;
use crate::wiring::action::tests::{Fx, LEVEL};

/// Gold is a stat of the base game; the reward adds to it.
const GOLD: u16 = 14;

fn quests() -> String {
    format!(
        "[[quest]]\nid = 1\nname = \"First\"\n\
         [[quest.objective]]\nkind = \"enter\"\nlevel = {LEVEL}\n\
         [[quest.objective]]\nkind = \"kill\"\nmonster = 0\ncount = 2\n\
         [[quest.reward]]\nkind = \"stat\"\nstat = {GOLD}\nvalue = 7\n"
    )
}

fn modded(fx: &mut Fx, text: &str) {
    let rules = DataMod::from_files(
        "quests",
        &DataModFiles {
            quests: Some(("quests.toml", text)),
            ..DataModFiles::default()
        },
    )
    .unwrap();
    fx.sim.sys.hooks.rules = Ruleset::Mod(Arc::new(rules));
}

#[test]
fn kills_and_levels_finish_a_quest_whose_progress_survives_the_save() {
    let mut fx = Fx::new();
    modded(&mut fx, &quests());
    let a = fx.a;
    let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
    fx.sim.mod_load_sections(&mut fx.game, p, &[]);
    let c = fx.game.lists.add_client(Some(p), None, 0);
    fx.sim.join_sequence(&mut fx.game, c);
    // The first level the player arrives in is the `enter` objective.
    fx.sim.client_level_change(&mut fx.game, c);
    for n in 0..2 {
        let m = fx.spawn(UnitType::Monster, 0, a, 12 + n, 12);
        fx.sim.combat(&mut fx.game, |w, _| kill_by(w, m, Some(p)));
    }
    assert_eq!(fx.sim.sys.stats.unit_base(p, GOLD, 0), 7);
    // Done once: more kills give nothing more.
    let m = fx.spawn(UnitType::Monster, 0, a, 14, 12);
    fx.sim.combat(&mut fx.game, |w, _| kill_by(w, m, Some(p)));
    assert_eq!(fx.sim.sys.stats.unit_base(p, GOLD, 0), 7);

    // The save carries the progress; a new game reads it back and does not
    // pay the reward again.
    let saved = fx.sim.mod_save_sections(&mut fx.game, p).expect("sections");
    assert_eq!(saved.len(), 1);
    let mut again = Fx::new();
    modded(&mut again, &quests());
    let a = again.a;
    let q = again.spawn(UnitType::Player, 0, a, 10, 10);
    again.sim.mod_load_sections(&mut again.game, q, &saved);
    let c = again.game.lists.add_client(Some(q), None, 0);
    again.sim.join_sequence(&mut again.game, c);
    again.sim.client_level_change(&mut again.game, c);
    for _ in 0..12 {
        again.tick();
    }
    assert_eq!(again.sim.sys.stats.unit_base(q, GOLD, 0), 0);
    assert_eq!(
        again.sim.mod_save_sections(&mut again.game, q).as_deref(),
        Some(&saved[..])
    );
}

#[test]
fn a_kill_by_someone_else_or_under_the_base_rules_counts_for_nobody() {
    let mut fx = Fx::new();
    modded(&mut fx, &quests());
    let a = fx.a;
    let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
    fx.sim.mod_load_sections(&mut fx.game, p, &[]);
    let m = fx.spawn(UnitType::Monster, 0, a, 12, 12);
    fx.sim.combat(&mut fx.game, |w, _| kill_by(w, m, None));
    let m = fx.spawn(UnitType::Monster, 0, a, 13, 12);
    fx.sim.combat(&mut fx.game, |w, _| kill_by(w, m, Some(m)));
    // A kill with no killer, or by a monster, is no player's: nothing counts.
    assert_eq!(fx.sim.sys.stats.unit_base(p, GOLD, 0), 0);

    // D2Gamma's own rules: no hooks, no section, no change.
    let mut base = Fx::new();
    let a = base.a;
    let p = base.spawn(UnitType::Player, 0, a, 10, 10);
    let m = base.spawn(UnitType::Monster, 0, a, 12, 12);
    base.sim
        .combat(&mut base.game, |w, _| kill_by(w, m, Some(p)));
    base.sim.mod_load_sections(&mut base.game, p, &[]);
    assert_eq!(base.sim.mod_save_sections(&mut base.game, p), None);
    assert_eq!(base.sim.sys.stats.unit_base(p, GOLD, 0), 0);
}
