//! A mod's triggers (`triggers.toml`) on the real wiring: the unit events
//! the combat code raises, a skill used, and a state starting or running
//! out reach the data mod host's triggers, which roll their chance, check
//! their conditions and run their actions on the skill host.

use crate::rules::{DataMod, DataModFiles, HostDamage, HostStates, Ruleset, MAX_DEPTH};
use crate::skills::use_::UseWorld;
use crate::units::UnitId;

use super::super::skill_use::Fx;
use super::blocks::{fixture, life, stacks, LEVEL_STAT, LIFE, STATE};

/// A short state with no stack rules but a time: the "window".
const WINDOW: u16 = 32;

/// State 30 stacks (3 at most) for 4 seconds; state 32 lasts 1 second.
const STATES: &str =
    "[[state]]\nid = 30\nduration_seconds = 4\nmax_stacks = 3\nreapply = \"add_stack\"\n\
                      [[state.effect]]\nstat = 31\nper_stack = 5\n\
                      [[state]]\nid = 32\nduration_seconds = 1\n";

const ATTACK: u8 = 7;
const DAMAGED_IN_MELEE: u8 = 1;
const DO_MELEE_DAMAGE: u8 = 5;
const KILL: u8 = 9;

/// The player holds the triggers through stat 12 (level 30); the monster
/// has level 1.
fn game(triggers: &str) -> (Fx, UnitId, UnitId) {
    let (mut fx, p, m, _) = fixture("");
    // The monster can be given states.
    let tables = std::sync::Arc::make_mut(&mut fx.sim.sys.hooks.tables);
    tables.combat.monstats2[0].isatt = true;
    install(&mut fx, triggers, STATES);
    (fx, p, m)
}

fn install(fx: &mut Fx, triggers: &str, states: &str) {
    let files = DataModFiles {
        states: Some(("states.toml", states)),
        triggers: Some(("triggers.toml", triggers)),
        ..DataModFiles::default()
    };
    fx.sim.sys.hooks.rules = DataMod::from_files("triggers", &files)
        .unwrap()
        .into_ruleset();
}

/// A trigger on `on` with the holder stat 12 as its source and `rest`
/// (conditions, actions) after it.
fn trigger(on: &str, extra: &str, rest: &str) -> String {
    format!("[[trigger]]\non = \"{on}\"\nwhile_stat = 12\n{extra}\n{rest}")
}

const APPLY_WINDOW: &str = "[[trigger.action]]\nkind = \"apply_state\"\nstate = 32\n";

fn fire(fx: &mut Fx, event: u8, u: UnitId, other: Option<UnitId>) {
    fx.sim.skill_use(&mut fx.game, |w| {
        HostDamage::unit_event(w, event, u, other, None);
    });
}

fn on(fx: &Fx, u: UnitId, state: u16) -> bool {
    fx.sim.sys.stats.has_state(u, u32::from(state))
}

fn frames(fx: &mut Fx, n: usize) {
    for _ in 0..n {
        fx.frame();
    }
}

#[test]
fn an_attack_runs_the_triggers_of_the_attacker_only() {
    let (mut fx, p, m) = game(&trigger("attack", "", APPLY_WINDOW));
    fire(&mut fx, ATTACK, p, Some(m));
    assert!(on(&fx, p, WINDOW));
    // Another event, and another unit's attack (its level 1 is a source
    // too, but only the attacker's own triggers run), do not.
    let (mut fx, p, m) = game(&trigger("attack", "", APPLY_WINDOW));
    fire(&mut fx, DO_MELEE_DAMAGE, p, Some(m));
    fire(&mut fx, KILL, p, Some(m));
    assert!(!on(&fx, p, WINDOW));
    fire(&mut fx, ATTACK, m, Some(p));
    assert!(!on(&fx, p, WINDOW));
    fx.assert_clean();
}

#[test]
fn hit_being_hit_and_kill_are_the_units_own_events() {
    let to_other = "[[trigger.action]]\nkind = \"apply_state\"\nstate = 32\nwho = \"other\"\n";
    for (on_name, event, holder_is_p) in [
        ("hit", DO_MELEE_DAMAGE, true),
        ("being_hit", DAMAGED_IN_MELEE, true),
        ("kill", KILL, true),
    ] {
        let (mut fx, p, m) = game(&trigger(on_name, "", to_other));
        assert!(holder_is_p);
        fire(&mut fx, event, p, Some(m));
        assert!(
            on(&fx, m, WINDOW),
            "{on_name} puts the state on the other unit"
        );
        assert!(!on(&fx, p, WINDOW));
    }
    // Damage by missile counts as being hit and as a hit too.
    let (mut fx, p, m) = game(&trigger("being_hit", "", to_other));
    fire(&mut fx, 2, p, Some(m));
    assert!(on(&fx, m, WINDOW));
    let (mut fx, p, m) = game(&trigger("hit", "", to_other));
    fire(&mut fx, 6, p, Some(m));
    assert!(on(&fx, m, WINDOW));
}

#[test]
fn the_source_decides_whether_a_trigger_is_there() {
    let (mut fx, p, m) = game(&trigger("attack", "", APPLY_WINDOW));
    fx.set(p, &[(LEVEL_STAT, 0)]);
    fire(&mut fx, ATTACK, p, Some(m));
    assert!(!on(&fx, p, WINDOW), "a stat of 0 is no source");

    // A state as the source: its stack count is `lvl`.
    let t = "[[trigger]]\non = \"attack\"\nwhile_state = 30\nchance = \"lvl * 100\"\n\
             [[trigger.action]]\nkind = \"apply_state\"\nstate = 32\n";
    let (mut fx, p, m) = game(t);
    fire(&mut fx, ATTACK, p, Some(m));
    assert!(!on(&fx, p, WINDOW), "the state is off");
    fx.sim.skill_use(&mut fx.game, |w| {
        let spec = crate::rules::RuledState {
            state: STATE,
            skill: 0,
            level: 1,
            base_ticks: 0,
        };
        w.add_state_stacks(p, p, &spec, 1);
    });
    fire(&mut fx, ATTACK, p, Some(m));
    assert!(on(&fx, p, WINDOW));
    fx.assert_clean();
}

#[test]
fn a_held_skill_is_a_source_with_its_level() {
    let t = "[[trigger]]\non = \"attack\"\nwhile_skill = 0\nchance = \"lvl * 100\"\n\
             [[trigger.action]]\nkind = \"apply_state\"\nstate = 32\n";
    let (mut fx, p, m) = game(t);
    fire(&mut fx, ATTACK, p, Some(m));
    assert!(!on(&fx, p, WINDOW), "the unit has no such skill");
    fx.sim.sys.hooks.x.skills.insert(
        p,
        vec![crate::skills::SkillEntry {
            skill: 0,
            base: 1,
            owner_guid: -1,
            ..crate::skills::SkillEntry::default()
        }],
    );
    fire(&mut fx, ATTACK, p, Some(m));
    assert!(on(&fx, p, WINDOW));
}

#[test]
fn chance_is_a_percent_rolled_from_the_holders_seed() {
    let count = |chance: &str| {
        let (mut fx, p, m) = game(&trigger(
            "attack",
            &format!("chance = \"{chance}\""),
            APPLY_WINDOW,
        ));
        let mut hits = 0;
        for _ in 0..200 {
            fire(&mut fx, ATTACK, p, Some(m));
            if on(&fx, p, WINDOW) {
                hits += 1;
                fx.sim.skill_use(&mut fx.game, |w| w.clear_state(p, WINDOW));
            }
        }
        hits
    };
    assert_eq!(count("0"), 0);
    assert_eq!(count("100"), 200);
    // Level 30 as the formula's `lvl`: 30 percent.
    let thirty = count("lvl");
    assert!((30..=90).contains(&thirty), "{thirty} of 200");
    // The same game rolls the same.
    assert_eq!(count("lvl"), thirty);
    // Over 100 is certain, below 0 never.
    assert_eq!(count("lvl * 10"), 200);
    assert_eq!(count("0 - lvl"), 0);
}

#[test]
fn conditions_all_have_to_hold() {
    let (mut fx, p, m) = game(&trigger(
        "attack",
        "",
        &format!(
            "[[trigger.condition]]\nkind = \"stacks_at_least\"\nstate = 30\ncount = 2\n\
             [[trigger.condition]]\nkind = \"lacks_state\"\nstate = 32\n{APPLY_WINDOW}"
        ),
    ));
    let add = |fx: &mut Fx, n: u16| {
        fx.sim.skill_use(&mut fx.game, |w| {
            let spec = crate::rules::RuledState {
                state: STATE,
                skill: 0,
                level: 1,
                base_ticks: 0,
            };
            w.add_state_stacks(p, p, &spec, n)
        });
    };
    fire(&mut fx, ATTACK, p, Some(m));
    assert!(!on(&fx, p, WINDOW), "no stacks");
    add(&mut fx, 1);
    fire(&mut fx, ATTACK, p, Some(m));
    assert!(!on(&fx, p, WINDOW), "one stack of two");
    add(&mut fx, 1);
    fire(&mut fx, ATTACK, p, Some(m));
    assert!(on(&fx, p, WINDOW));
    assert_eq!(stacks(&fx, p), 2);
}

#[test]
fn life_and_the_other_unit_can_be_conditions() {
    let low = format!(
        "[[trigger.condition]]\nkind = \"life_below\"\npercent = 50\nwho = \"other\"\n{APPLY_WINDOW}"
    );
    let (mut fx, p, m) = game(&trigger("hit", "", &low));
    fire(&mut fx, DO_MELEE_DAMAGE, p, Some(m));
    assert!(!on(&fx, p, WINDOW), "the monster is at full life");
    fx.set(m, &[(crate::stats::stat::HITPOINTS, LIFE / 4)]);
    assert!(life(&mut fx, m) < LIFE / 2);
    fire(&mut fx, DO_MELEE_DAMAGE, p, Some(m));
    assert!(on(&fx, p, WINDOW));

    let high = format!(
        "[[trigger.condition]]\nkind = \"life_above\"\npercent = 50\nwho = \"other\"\n{APPLY_WINDOW}"
    );
    let (mut fx, p, m) = game(&trigger("hit", "", &high));
    fx.set(m, &[(crate::stats::stat::HITPOINTS, LIFE / 4)]);
    fire(&mut fx, DO_MELEE_DAMAGE, p, Some(m));
    assert!(!on(&fx, p, WINDOW));
    // An event with no other unit fails a condition on it.
    fire(&mut fx, DO_MELEE_DAMAGE, p, None);
    assert!(!on(&fx, p, WINDOW));
}

#[test]
fn skill_used_can_be_filtered_by_skill_and_ignores_the_triggers_own_casts() {
    let only = trigger("skill_used", "skills = [7, 8]", APPLY_WINDOW);
    let (mut fx, p, _) = game(&only);
    let used = |fx: &mut Fx, skill: i32| {
        fx.sim
            .skill_use(&mut fx.game, |w| UseWorld::skill_done(w, p, skill, 1));
    };
    used(&mut fx, 5);
    assert!(!on(&fx, p, WINDOW), "a skill outside the list");
    used(&mut fx, 8);
    assert!(on(&fx, p, WINDOW));

    // Any skill when the list is empty.
    let (mut fx, p, _) = game(&trigger("skill_used", "", APPLY_WINDOW));
    fx.sim
        .skill_use(&mut fx.game, |w| UseWorld::skill_done(w, p, 5, 1));
    assert!(on(&fx, p, WINDOW));

    // Inside a trigger's own actions a used skill is not told.
    let (mut fx, p, _) = game(&trigger("skill_used", "", APPLY_WINDOW));
    fx.sim.skill_use(&mut fx.game, |w| {
        assert!(HostStates::enter_trigger(w));
        UseWorld::skill_done(w, p, 5, 1);
        HostStates::leave_trigger(w);
    });
    assert!(!on(&fx, p, WINDOW));
}

#[test]
fn triggers_stop_setting_each_other_off_past_the_depth_limit() {
    let (mut fx, _, _) = game(&trigger("attack", "", APPLY_WINDOW));
    fx.sim.skill_use(&mut fx.game, |w| {
        for _ in 0..MAX_DEPTH {
            assert!(HostStates::enter_trigger(w));
        }
        assert!(!HostStates::enter_trigger(w), "one level too deep");
        for _ in 0..MAX_DEPTH {
            HostStates::leave_trigger(w);
        }
        assert!(HostStates::enter_trigger(w), "back at the top");
        HostStates::leave_trigger(w);
    });
    // A trigger past the limit does nothing at all.
    let (mut fx, p, m) = game(&trigger("attack", "", APPLY_WINDOW));
    fx.sim.skill_use(&mut fx.game, |w| {
        for _ in 0..MAX_DEPTH {
            HostStates::enter_trigger(w);
        }
        HostDamage::unit_event(w, ATTACK, p, Some(m), None);
    });
    assert!(!on(&fx, p, WINDOW));
}

#[test]
fn a_state_starting_sets_off_state_start_triggers() {
    let t = "[[trigger]]\non = \"state_start\"\nstate = 30\nwhile_stat = 12\n\
             [[trigger.action]]\nkind = \"apply_state\"\nstate = 32\n";
    let (mut fx, p, _) = game(t);
    let spec = crate::rules::RuledState {
        state: STATE,
        skill: 0,
        level: 1,
        base_ticks: 0,
    };
    fx.sim
        .skill_use(&mut fx.game, |w| w.apply_ruled_state(p, p, &spec));
    assert!(on(&fx, p, STATE) && on(&fx, p, WINDOW));
    // Another stack is not another start.
    fx.sim.skill_use(&mut fx.game, |w| w.clear_state(p, WINDOW));
    fx.sim
        .skill_use(&mut fx.game, |w| w.add_state_stacks(p, p, &spec, 1));
    assert!(!on(&fx, p, WINDOW));
    fx.assert_clean();
}

#[test]
fn a_state_running_out_sets_off_state_end_triggers_but_a_mod_removal_does_not() {
    let t = "[[trigger]]\non = \"state_end\"\nstate = 32\nwhile_stat = 12\n\
             [[trigger.action]]\nkind = \"remove_stack\"\nstate = 30\n";
    let setup = |fx: &mut Fx, p: UnitId| {
        let spec = crate::rules::RuledState {
            state: STATE,
            skill: 0,
            level: 1,
            base_ticks: 0,
        };
        let window = crate::rules::RuledState {
            state: WINDOW,
            ..spec
        };
        fx.sim.skill_use(&mut fx.game, |w| {
            w.add_state_stacks(p, p, &spec, 2);
            w.apply_ruled_state(p, p, &window);
        });
    };
    // It runs out after its second: the state end takes a stack off.
    let (mut fx, p, _) = game(t);
    setup(&mut fx, p);
    assert_eq!(stacks(&fx, p), 2);
    frames(&mut fx, 24);
    assert!(on(&fx, p, WINDOW));
    frames(&mut fx, 1);
    assert!(!on(&fx, p, WINDOW));
    assert_eq!(stacks(&fx, p), 1, "the end took a stack off");

    // The mod removing it itself (a skill part, another trigger) is no end.
    let (mut fx, p, _) = game(t);
    setup(&mut fx, p);
    fx.sim.skill_use(&mut fx.game, |w| w.clear_state(p, WINDOW));
    frames(&mut fx, 30);
    assert!(!on(&fx, p, WINDOW));
    assert_eq!(stacks(&fx, p), 2);
    fx.assert_clean();
}

#[test]
fn a_trigger_can_cast_a_skill_at_the_other_unit() {
    let skills = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"area_hit\"\nradius = 6\n\
                  [skill.part.damage]\nelement = \"fire\"\nmin = 40\nmax = 40\n";
    let t = trigger(
        "hit",
        "",
        "[[trigger.action]]\nkind = \"cast_skill\"\nskill = 0\nlevel = 1\n",
    );
    let (mut fx, p, m) = game(&t);
    let files = DataModFiles {
        skills: Some(("skills.toml", skills)),
        states: Some(("states.toml", STATES)),
        triggers: Some(("triggers.toml", &t)),
        ..DataModFiles::default()
    };
    {
        // The row names the block slot and fires no missile of its own.
        let row = &mut std::sync::Arc::make_mut(&mut fx.sim.sys.hooks.tables)
            .skills
            .skills[0];
        row.srvdofunc = crate::rules::BLOCK_SLOT;
        row.srvmissile = 0xFFFF;
    }
    fx.sim.sys.hooks.rules = DataMod::from_files("cast", &files).unwrap().into_ruleset();
    // The fixture's aim point is where the monster stands.
    fx.sim.sys.hooks.x.aim_at = (11, 10);
    fire(&mut fx, DO_MELEE_DAMAGE, p, Some(m));
    assert!(life(&mut fx, m) < LIFE, "the cast skill hurt the monster");
    fx.assert_clean();
}

#[test]
fn a_game_without_triggers_hears_of_nothing() {
    let (mut fx, p, m) = game(&trigger("attack", "", APPLY_WINDOW));
    let with = fx.sim.sys.hooks.rules.clone();
    assert!(with.skills().is_some_and(|s| s.wants_unit_events()));
    // A mod with block skills but no triggers does not want the events.
    let skills = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"area_hit\"\nradius = 6\n\
                  [skill.part.damage]\nmin = 1\nmax = 1\n";
    let files = DataModFiles {
        skills: Some(("skills.toml", skills)),
        ..DataModFiles::default()
    };
    fx.sim.sys.hooks.rules = DataMod::from_files("a", &files).unwrap().into_ruleset();
    assert!(!fx
        .sim
        .sys
        .hooks
        .rules
        .skills()
        .is_some_and(|s| s.wants_unit_events()));
    fire(&mut fx, ATTACK, p, Some(m));
    assert!(!on(&fx, p, WINDOW));
    // And D2Gamma's own rules.
    fx.sim.sys.hooks.rules = Ruleset::Original;
    fire(&mut fx, ATTACK, p, Some(m));
    assert!(!on(&fx, p, WINDOW));
    fx.assert_clean();
}

/// The game's row 0 is a block skill `skills` that the unit owns.
fn block_skill(fx: &mut Fx, p: UnitId, skills: &str, triggers: &str) {
    {
        let row = &mut std::sync::Arc::make_mut(&mut fx.sim.sys.hooks.tables)
            .skills
            .skills[0];
        row.srvdofunc = crate::rules::BLOCK_SLOT;
        row.srvmissile = 0xFFFF;
    }
    fx.sim.sys.hooks.x.skills.insert(
        p,
        vec![crate::skills::SkillEntry {
            skill: 0,
            base: 1,
            owner_guid: -1,
            ..crate::skills::SkillEntry::default()
        }],
    );
    let files = DataModFiles {
        skills: Some(("skills.toml", skills)),
        states: Some(("states.toml", STATES)),
        triggers: Some(("triggers.toml", triggers)),
        ..DataModFiles::default()
    };
    fx.sim.sys.hooks.rules = DataMod::from_files("block", &files).unwrap().into_ruleset();
}

#[test]
fn using_a_skill_through_the_use_path_sets_off_skill_used_triggers() {
    let skills = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"state\"\nstate = 30\n";
    let t = trigger("skill_used", "skills = [0]", APPLY_WINDOW);
    let (mut fx, p, _) = game(&t);
    block_skill(&mut fx, p, skills, &t);
    let table = fx.sim.sys.hooks.tables.clone();
    let r = fx.sim.skill_use(&mut fx.game, |w| {
        crate::skills::use_::do_skill(w, &table.skills, p, 0, 1)
    });
    assert_eq!(r, 1, "the skill did something");
    assert_eq!(stacks(&fx, p), 1);
    assert!(on(&fx, p, WINDOW), "and the unit used it");
    // A skill that did nothing was not used.
    let (mut fx, p, _) = game(&t);
    let none = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"melee_hit\"\n";
    block_skill(&mut fx, p, none, &t);
    let r = fx.sim.skill_use(&mut fx.game, |w| {
        crate::skills::use_::do_skill(w, &table.skills, p, 0, 1)
    });
    assert_eq!(r, 0);
    assert!(!on(&fx, p, WINDOW));
    fx.assert_clean();
}

#[test]
fn a_real_melee_hit_sets_off_attack_hit_and_being_hit_triggers() {
    let skills = "[[skill]]\nid = 0\n[[skill.part]]\nkind = \"melee_hit\"\nweapon_percent = 0\n\
                  [skill.part.damage]\nmin = 40\nmax = 40\n";
    let triggers = format!(
        "{}{}{}",
        trigger("attack", "", APPLY_WINDOW),
        trigger(
            "hit",
            "",
            "[[trigger.action]]\nkind = \"add_stack\"\nstate = 30\n"
        ),
        trigger(
            "being_hit",
            "",
            "[[trigger.action]]\nkind = \"add_stack\"\nstate = 30\nwho = \"other\"\n"
        ),
    );
    let (mut fx, p, m) = game(&triggers);
    block_skill(&mut fx, p, skills, &triggers);
    // The real server turns the unit event registry on.
    fx.sim.sys.hooks.enable_unit_events();
    fx.sim.sys.hooks.x.in_reach = true;
    fx.sim.sys.hooks.x.targets.insert(p, m);
    fx.sim.with(&mut fx.game, |_, v| v.set_state(m, 54, true));
    let before = life(&mut fx, m);
    let table = fx.sim.sys.hooks.tables.clone();
    fx.sim.skill_use(&mut fx.game, |w| {
        crate::skills::use_::do_skill(w, &table.skills, p, 0, 1)
    });
    assert!(life(&mut fx, m) < before, "the hit landed");
    assert!(on(&fx, p, WINDOW), "the swing set off the attack trigger");
    // The hit trigger of the attacker and the being-hit trigger of the
    // monster (which puts the stack on the attacker) each added one.
    assert_eq!(stacks(&fx, p), 2);
    fx.assert_clean();
}

#[test]
fn by_skill_runs_a_trigger_only_while_that_skill_is_used() {
    use crate::skills::SkillEntry;
    let used = |skill: i32| SkillEntry {
        skill,
        base: 1,
        owner_guid: -1,
        ..SkillEntry::default()
    };
    for (event, on_name) in [(ATTACK, "attack"), (DO_MELEE_DAMAGE, "hit"), (KILL, "kill")] {
        let (mut fx, p, m) = game(&trigger(on_name, "by_skill = 7", APPLY_WINDOW));
        // Using another skill, or none: no.
        fx.sim.skill_use(&mut fx.game, |w| {
            w.set_used_skill(p, Some(used(8)));
        });
        fire(&mut fx, event, p, Some(m));
        assert!(!on(&fx, p, WINDOW), "{on_name}: another skill");
        fx.sim
            .skill_use(&mut fx.game, |w| w.set_used_skill(p, None));
        fire(&mut fx, event, p, Some(m));
        assert!(!on(&fx, p, WINDOW), "{on_name}: no skill");
        // Using it: yes.
        fx.sim.skill_use(&mut fx.game, |w| {
            w.set_used_skill(p, Some(used(7)));
        });
        fire(&mut fx, event, p, Some(m));
        assert!(on(&fx, p, WINDOW), "{on_name}: the skill");
    }
    // Without by_skill any skill, or none, will do.
    let (mut fx, p, m) = game(&trigger("attack", "", APPLY_WINDOW));
    fire(&mut fx, ATTACK, p, Some(m));
    assert!(on(&fx, p, WINDOW));
}
