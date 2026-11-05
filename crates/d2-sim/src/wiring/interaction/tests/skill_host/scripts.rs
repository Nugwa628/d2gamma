//! Script events use the same authoritative wiring as data triggers.
use super::blocks::fixture;
use crate::rules::scripts::{Event, EventKind as K, Runtime};
use crate::rules::{HostDamage, HostStates, RuledState, SkillHost};
use crate::skills::use_::UseWorld;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Recorder(
    Mutex<Vec<Event>>,
    Mutex<Vec<crate::rules::scripts::CallContext>>,
);
impl Runtime for Recorder {
    fn call(
        &self,
        _: &mut dyn SkillHost,
        _: &crate::rules::scripts::CallSpec,
        c: crate::rules::scripts::CallContext,
    ) -> Result<i32, String> {
        self.1.lock().unwrap().push(c);
        Ok(1)
    }
    fn event(&self, _: &mut dyn SkillHost, e: Event) {
        self.0.lock().unwrap().push(e);
    }
    fn tick(&self, _: &mut dyn SkillHost) {}
}

#[test]
fn combat_cast_and_state_events_reach_the_runtime_in_order() {
    let (mut fx, player, monster, _) = fixture("");
    let recorder = Arc::new(Recorder::default());
    fx.sim.sys.hooks.scripts = Some(recorder.clone());
    fx.sim.sys.hooks.enable_unit_events();
    fx.sim.skill_use(&mut fx.game, |w| {
        for code in [7, 5, 1, 9] {
            w.unit_event(code, player, Some(monster), None);
        }
        w.cast_started(player, 0);
        w.skill_done(player, 0, 1);
        w.apply_ruled_state(
            player,
            player,
            &RuledState {
                state: 30,
                skill: 0,
                level: 1,
                base_ticks: 25,
            },
        );
    });
    let kinds = || {
        recorder
            .0
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.kind)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        kinds(),
        [
            K::AttackStart,
            K::AttackHit,
            K::HitTaken,
            K::Kill,
            K::CastStart,
            K::CastComplete,
            K::SkillUsed
        ]
    );
    fx.sim.run_scripts(&mut fx.game);
    assert_eq!(kinds().last(), Some(&K::StateStart));
    fx.sim
        .skill_use(&mut fx.game, |w| w.clear_state(player, 30));
    fx.sim.run_scripts(&mut fx.game);
    let events = kinds();
    assert!(events.contains(&K::BuffEnd));
    assert!(events.contains(&K::StateEnd));
    assert_eq!(events.iter().filter(|&&k| k == K::StateStart).count(), 1);
    fx.assert_clean();
}

#[test]
fn default_game_never_queues_or_delivers_script_events() {
    let (mut fx, player, _, _) = fixture("");
    fx.sim.sys.hooks.rules = crate::rules::Ruleset::Original;
    assert!(fx.sim.sys.hooks.scripts.is_none());
    fx.sim.with(&mut fx.game, |_, v| {
        v.set_state(player, 30, true);
        v.set_state(player, 30, false);
    });
    fx.sim.run_scripts(&mut fx.game);
    assert!(fx.sim.sys.hooks.script_events.is_empty());
    fx.assert_clean();
}

#[test]
fn skills_and_triggers_call_the_same_runtime_with_formula_context() {
    let skill = "[[skill]]\nid=0\n[[skill.part]]\nkind='call_script'\nmodule='test.luau'\nfunction='run'\nargument='lvl * 3'\n";
    let (mut fx, player, monster, _) = fixture(skill);
    let recorder = Arc::new(Recorder::default());
    fx.sim.sys.hooks.scripts = Some(recorder.clone());
    fx.sim.sys.hooks.x.targets.insert(player, monster);
    assert_eq!(super::blocks::cast(&mut fx, player, 4), 1);
    let trigger = "[[trigger]]\non='attack'\nwhile_stat=12\n[[trigger.action]]\nkind='call_script'\nmodule='test.luau'\nfunction='run'\nargument='lvl + 1'\n";
    let rules = crate::rules::DataMod::from_files(
        "calls",
        &crate::rules::DataModFiles {
            triggers: Some(("triggers.toml", trigger)),
            ..Default::default()
        },
    )
    .unwrap();
    fx.sim.sys.hooks.rules = rules.into_ruleset();
    fx.sim.skill_use(&mut fx.game, |w| {
        w.unit_event(7, player, Some(monster), None);
    });
    let calls = recorder.1.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        (
            calls[0].unit,
            calls[0].skill,
            calls[0].level,
            calls[0].argument
        ),
        (player, 0, 4, 12)
    );
    assert_eq!(
        (calls[1].unit, calls[1].other, calls[1].argument),
        (player, Some(monster), 31)
    );
    fx.assert_clean();
}

#[test]
fn script_call_paths_and_identifiers_are_validated() {
    for (module, function) in [
        ("../bad.luau", "run"),
        ("/bad.luau", "run"),
        ("good.luau", "bad()"),
        ("bad.lua", "run"),
    ] {
        let text = format!("[[skill]]\nid=0\n[[skill.part]]\nkind='call_script'\nmodule='{module}'\nfunction='{function}'\n");
        let error = crate::rules::SkillRules::from_toml(&text)
            .unwrap_err()
            .to_string();
        assert!(error.contains("skill[0].part[0]"), "{error}");
    }
}
