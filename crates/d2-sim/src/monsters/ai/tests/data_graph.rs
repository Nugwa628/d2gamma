//! A mod's monster behaviour graph (`files/ai.toml`) on the AI fake world:
//! the graph installs through the monster's `AI` value, thinks with the
//! built-in movement and skill helpers, keeps its own place per monster,
//! reports state changes and wind-ups, stays inside its per-think budget, and
//! leaves every monster without a graph exactly as it was.

use super::*;
use crate::monsters::ai::graph::GraphEventKind;
use crate::rules::data_mod::ai::{
    AiRules, MAX_ACTIONS, MAX_CONDITIONS, MAX_HOPS, MAX_TRANSITIONS, THINK_BASE, THINK_BUDGET,
};
use crate::rules::{DataMod, DataModFiles, MonsterProvider};
use crate::tick::rate::TickRate;

/// The `AI` value the test graphs answer.
const AI: u16 = 200;

fn mod_rules(ai_toml: &str) -> crate::rules::Ruleset {
    let files = DataModFiles {
        ai: Some(("ai.toml", ai_toml)),
        ..DataModFiles::default()
    };
    DataMod::from_files("graphs", &files)
        .unwrap()
        .into_ruleset()
}

/// A world whose monster has AI [`AI`] under a mod with `ai_toml`, installed.
fn world(ai_toml: &str) -> World {
    let mut w = World::new(monstats(AI, [0; 5], 15));
    w.fake.rules = mod_rules(ai_toml);
    let room = w.room;
    w.game.lists.room_mut(room).unwrap().adjacent = vec![room];
    let mon = w.mon;
    w.with(|g, cx| install(g, cx, mon, 0));
    w
}

/// One graph, [`AI`], with `body` after its `[[ai]]` header.
fn graph(body: &str) -> String {
    format!("[[ai]]\nindex = {AI}\n{body}")
}

fn think(w: &mut World, p: TickParam) {
    let mon = w.mon;
    w.with(|g, cx| {
        let f = cx.store.control(mon).unwrap().function;
        run_function(g, cx, f, mon, &p)
    });
}

fn param(w: &World, target: bool, combat: bool, distance: i32) -> TickParam {
    TickParam {
        target: target.then_some(w.player),
        distance,
        combat,
        class: 0,
        class2: 0,
    }
}

fn run_with(w: &mut World, target: bool, combat: bool, distance: i32) {
    let p = param(w, target, combat, distance);
    think(w, p);
}

fn run_at(w: &mut World, frame: i32, combat: bool, distance: i32) {
    w.game.frame = frame;
    run_with(w, true, combat, distance);
}

fn state(w: &World) -> u8 {
    w.store.get(w.mon).unwrap().graph.unwrap().state
}

fn kinds(w: &World) -> Vec<GraphEventKind> {
    w.store.events.peek().iter().map(|e| e.kind).collect()
}

const FIGHT: &str = r#"
[[ai.state]]
name = "fight"
[[ai.state.action]]
kind = "attack"
when = [{ kind = "in_melee" }]
[[ai.state.action]]
kind = "move_to"
"#;

#[test]
fn a_monster_whose_ai_value_matches_installs_the_graph() {
    let w = world(&graph(FIGHT));
    let mon = w.mon;
    let c = w.store.control(mon).unwrap();
    assert_eq!(c.function, THINK_BASE);
    let run = w.store.get(mon).unwrap().graph.unwrap();
    assert_eq!((run.ai, run.state), (AI, 0));
    assert!(w.store.unhandled.is_empty());
}

#[test]
fn the_graph_picks_the_first_action_that_acts() {
    let mut w = world(&graph(FIGHT));
    run_with(&mut w, true, true, 1);
    assert_eq!(last_mode(&w), format!("mode 4 Unit({:?})", w.player));
    run_with(&mut w, true, false, 9);
    assert_eq!(last_mode(&w), format!("mode 2 Unit({:?})", w.player));
    // No target: both actions are passed over, the monster idles.
    let before = w.fake.modes().len();
    run_with(&mut w, false, false, 0);
    assert_eq!(w.fake.modes().len(), before + 1);
    assert_eq!(last_mode(&w), format!("mode 1 Unit({:?})", w.mon));
    assert!(!w.thinks().is_empty());
}

#[test]
fn a_transition_changes_the_state_and_is_reported() {
    let w = &mut world(&graph(
        r#"
[[ai.state]]
name = "far"
[[ai.state.transition]]
to = "near"
when = [{ kind = "distance_below", value = 5 }]
[[ai.state.action]]
kind = "move_to"
[[ai.state]]
name = "near"
[[ai.state.transition]]
to = "far"
when = [{ kind = "distance_above", value = 12 }]
[[ai.state.action]]
kind = "attack"
"#,
    ));
    run_at(w, 10, false, 8);
    assert_eq!(state(w), 0);
    assert!(kinds(w).is_empty());
    assert_eq!(last_mode(w), format!("mode 2 Unit({:?})", w.player));
    run_at(w, 20, false, 3);
    assert_eq!(state(w), 1);
    assert_eq!(last_mode(w), format!("mode 4 Unit({:?})", w.player));
    run_at(w, 30, false, 20);
    assert_eq!(state(w), 0);
    assert_eq!(
        kinds(w),
        [
            GraphEventKind::StateChanged { from: 0, to: 1 },
            GraphEventKind::StateChanged { from: 1, to: 0 },
        ]
    );
    let events = w.store.events.take();
    assert_eq!(
        (events[0].frame, events[0].unit, events[0].ai),
        (20, w.mon, AI)
    );
    assert!(w.store.events.peek().is_empty());
}

#[test]
fn a_ring_of_states_stops_after_the_hop_limit() {
    // Three states that always lead to the next: one think follows four
    // hops, not forever.
    let w = &mut world(&graph(
        r#"
[[ai.state]]
name = "a"
[[ai.state.transition]]
to = "b"
[[ai.state]]
name = "b"
[[ai.state.transition]]
to = "c"
[[ai.state]]
name = "c"
[[ai.state.transition]]
to = "a"
"#,
    ));
    run_at(w, 5, false, 0);
    assert_eq!(kinds(w).len(), MAX_HOPS);
    // a -> b -> c -> a -> b
    assert_eq!(state(w), 1);
}

#[test]
fn a_cooldown_holds_an_action_back_until_it_runs_out() {
    let w = &mut world(&graph(
        r#"
[[ai.cooldown]]
name = "swing"
seconds = 2
[[ai.state]]
name = "fight"
[[ai.state.action]]
kind = "attack"
when = [{ kind = "cooldown_ready", name = "swing" }]
starts_cooldown = "swing"
[[ai.state.action]]
kind = "idle"
seconds = 1
"#,
    ));
    let attack = format!("mode 4 Unit({:?})", w.player);
    run_at(w, 100, true, 1);
    assert_eq!(last_mode(w), attack);
    // Two seconds are 50 ticks.
    run_at(w, 120, true, 1);
    assert_eq!(last_mode(w), format!("mode 1 Unit({:?})", w.mon));
    run_at(w, 149, true, 1);
    assert_ne!(last_mode(w), attack);
    run_at(w, 150, true, 1);
    assert_eq!(last_mode(w), attack);
}

#[test]
fn a_wind_up_holds_the_strike_back_and_is_reported_once() {
    let w = &mut world(&graph(
        r#"
[[ai.state]]
name = "fight"
[[ai.state.action]]
kind = "wind_up"
seconds = 1
event = "slam"
[[ai.state.action]]
kind = "attack"
attack = 2
"#,
    ));
    let strike = format!("mode 5 Unit({:?})", w.player);
    run_at(w, 100, true, 1);
    assert_eq!(
        kinds(w),
        [GraphEventKind::WindUp {
            event: 0,
            ticks: 25
        }]
    );
    assert!(!w.fake.modes().contains(&strike));
    assert_eq!(w.thinks(), [125]);
    // Thinking early (a knock, a stun) keeps standing and reports nothing new.
    run_at(w, 110, true, 1);
    assert_eq!(kinds(w).len(), 1);
    assert!(!w.fake.modes().contains(&strike));
    assert_eq!(w.thinks(), [125]);
    // Over: the strike follows in the same think, and the next think starts
    // a new wind-up.
    run_at(w, 125, true, 1);
    assert_eq!(last_mode(w), strike);
    assert_eq!(w.store.get(w.mon).unwrap().graph.unwrap().windup, None);
    run_at(w, 140, true, 1);
    assert_eq!(kinds(w).len(), 2);
}

#[test]
fn a_state_change_cancels_a_wind_up() {
    let w = &mut world(&graph(
        r#"
[[ai.state]]
name = "a"
[[ai.state.transition]]
to = "b"
when = [{ kind = "life_below", percent = 50 }]
[[ai.state.action]]
kind = "wind_up"
seconds = 4
event = "charge"
[[ai.state]]
name = "b"
[[ai.state.action]]
kind = "attack"
"#,
    ));
    run_at(w, 10, true, 1);
    assert!(w.store.get(w.mon).unwrap().graph.unwrap().windup.is_some());
    w.fake.life = 40;
    run_at(w, 20, true, 1);
    let run = w.store.get(w.mon).unwrap().graph.unwrap();
    assert_eq!((run.state, run.windup), (1, None));
    assert_eq!(last_mode(w), format!("mode 4 Unit({:?})", w.player));
}

#[test]
fn durations_follow_the_game_rate() {
    // A second is 25 ticks of the base rate and 60 frames at the game rate.
    let w = &mut world(&graph(
        r#"
[[ai.state]]
name = "fight"
[[ai.state.action]]
kind = "wind_up"
seconds = 1
[[ai.state.action]]
kind = "attack"
"#,
    ));
    w.game.tick_rate = TickRate::new(60).unwrap();
    run_at(w, 100, true, 1);
    assert_eq!(w.thinks(), [160]);
    run_at(w, 159, true, 1);
    assert_ne!(last_mode(w), format!("mode 4 Unit({:?})", w.player));
    run_at(w, 160, true, 1);
    assert_eq!(last_mode(w), format!("mode 4 Unit({:?})", w.player));
}

/// Whether `cond` makes a one-hop graph change state, after `setup` ran.
fn holds(cond: &str, setup: impl FnOnce(&mut World)) -> bool {
    let toml = graph(&format!(
        "[[ai.state]]\nname = \"a\"\n[[ai.state.transition]]\nto = \"b\"\nwhen = [{cond}]\n[[ai.state]]\nname = \"b\"\n"
    ));
    let mut w = world(&toml);
    setup(&mut w);
    run_at(&mut w, 10, false, 6);
    state(&w) == 1
}

#[test]
fn conditions_read_the_world() {
    let none = |_: &mut World| {};
    assert!(holds(r#"{ kind = "has_target" }"#, none));
    assert!(!holds(r#"{ kind = "no_target" }"#, none));
    assert!(holds(r#"{ kind = "distance_below", value = 7 }"#, none));
    assert!(!holds(r#"{ kind = "distance_below", value = 6 }"#, none));
    assert!(holds(r#"{ kind = "distance_above", value = 6 }"#, none));
    assert!(!holds(r#"{ kind = "in_melee" }"#, none));
    assert!(holds(r#"{ kind = "out_of_melee" }"#, none));
    assert!(holds(r#"{ kind = "line_of_sight" }"#, none));
    assert!(!holds(r#"{ kind = "no_line_of_sight" }"#, none));
    assert!(holds(r#"{ kind = "no_line_of_sight" }"#, |w| {
        w.fake.line_blocked.insert(w.player);
    }));
    // Life: the fake monster has 100%.
    assert!(!holds(r#"{ kind = "life_below", percent = 100 }"#, none));
    assert!(holds(r#"{ kind = "life_above", percent = 100 }"#, none));
    assert!(holds(r#"{ kind = "life_below", percent = 60 }"#, |w| {
        w.fake.life = 59;
    }));
    // States, on the monster and on the target.
    assert!(!holds(r#"{ kind = "has_state", state = 9 }"#, none));
    assert!(holds(r#"{ kind = "lacks_state", state = 9 }"#, none));
    assert!(holds(r#"{ kind = "has_state", state = 9 }"#, |w| {
        w.fake.states.insert((w.mon, 9));
    }));
    assert!(holds(
        r#"{ kind = "has_state", state = 9, who = "target" }"#,
        |w| {
            w.fake.states.insert((w.player, 9));
        }
    ));
    // Without stack rules a state that is on counts as one stack.
    assert!(holds(
        r#"{ kind = "stacks_at_least", state = 9, count = 1 }"#,
        |w| {
            w.fake.states.insert((w.mon, 9));
        }
    ));
    assert!(!holds(
        r#"{ kind = "stacks_at_least", state = 9, count = 2 }"#,
        |w| {
            w.fake.states.insert((w.mon, 9));
        }
    ));
    // The roll comes from the monster's seed: 0 never holds, 100 always does.
    assert!(!holds(r#"{ kind = "random", percent = 0 }"#, none));
    assert!(holds(r#"{ kind = "random", percent = 100 }"#, none));
}

#[test]
fn enemies_and_allies_are_counted_within_a_radius() {
    // The fake player stands 5 subtiles away.
    let enemies = |radius: i32, at_least: i32| {
        holds(
            &format!(r#"{{ kind = "enemies_within", radius = {radius}, at_least = {at_least} }}"#),
            |w| w.fake.nodes = vec![vec![w.player]],
        )
    };
    assert!(enemies(10, 1));
    assert!(!enemies(3, 1));
    assert!(!enemies(10, 2));
    let allies = |radius: i32, at_least: i32, dead: bool| {
        holds(
            &format!(r#"{{ kind = "allies_within", radius = {radius}, at_least = {at_least} }}"#),
            |w| {
                let mate = add_mate(w, (103, 100));
                if dead {
                    w.fake.dead.insert(mate);
                }
            },
        )
    };
    assert!(allies(10, 1, false));
    assert!(!allies(1, 1, false));
    assert!(!allies(10, 2, false));
    assert!(!allies(10, 1, true), "a dead mate does not count");
}

/// A second monster (with an AI control) that is a minion of the world's
/// monster, standing at `at`.
fn add_mate(w: &mut World, at: (i32, i32)) -> UnitId {
    let mate = w
        .game
        .spawn_unit(UnitType::Monster, Some(w.room), false)
        .unwrap();
    w.fake.pos.insert(mate, at);
    w.store.entry(mate).control = Some(AiControl::default());
    let guid = w.game.lists.unit(mate).unwrap().guid;
    let leader = w.game.lists.unit(w.mon).map(|e| UnitRef {
        ty: e.ty,
        guid: e.guid,
    });
    w.store.control_mut(w.mon).unwrap().minions.push(guid);
    w.store.control_mut(mate).unwrap().minion_owner = leader;
    mate
}

#[test]
fn calling_allies_wakes_the_idle_pack_mates_near_by() {
    let mut w = world(&graph(
        r#"
[[ai.state]]
name = "fight"
[[ai.state.action]]
kind = "call_allies"
radius = 10
[[ai.state.action]]
kind = "attack"
"#,
    ));
    let near = add_mate(&mut w, (103, 100));
    let far = add_mate(&mut w, (160, 100));
    run_at(&mut w, 50, true, 1);
    // The caller still takes its own turn after the instant action.
    assert_eq!(last_mode(&w), format!("mode 4 Unit({:?})", w.player));
    let thinks = |w: &World, u: UnitId| -> Vec<i32> {
        w.game
            .timers
            .unit_timers(u)
            .into_iter()
            .filter_map(|t| w.game.timers.expire(t))
            .collect()
    };
    assert_eq!(thinks(&w, near), [51]);
    assert!(thinks(&w, far).is_empty());
    assert_ne!(w.store.control(near).unwrap().flags & flag::FORCE_LOS, 0);
    assert_eq!(w.store.control(far).unwrap().flags & flag::FORCE_LOS, 0);
}

#[test]
fn summoning_creates_minions_of_the_summoner() {
    let mut w = world(&graph(
        r#"
[[ai.state]]
name = "fight"
[[ai.state.action]]
kind = "summon"
class = 7
count = 2
when = [{ kind = "random", percent = 100 }]
[[ai.state.action]]
kind = "idle"
"#,
    ));
    let child = w
        .game
        .spawn_unit(UnitType::Monster, Some(w.room), false)
        .unwrap();
    w.store.entry(child).control = Some(AiControl::default());
    w.fake.x.spawn = Some(child);
    w.fake.y.free_spot = Some((101, 101));
    run_at(&mut w, 10, true, 1);
    let spawns: Vec<_> = w
        .fake
        .log
        .iter()
        .filter(|s| s.starts_with("spawn"))
        .collect();
    assert_eq!(spawns.len(), 2, "{spawns:?}");
    assert!(
        spawns[0].starts_with("spawn 7 101 101 1 2 0x42"),
        "{spawns:?}"
    );
    let guid = w.game.lists.unit(child).unwrap().guid;
    let leader = w.store.control(w.mon).unwrap();
    assert_eq!(leader.minions, [guid, guid]);
    assert_eq!(
        w.store.control(child).unwrap().minion_owner,
        leader.minion_owner
    );
    assert!(leader.minion_owner.is_some());
}

#[test]
fn a_skill_action_uses_the_monsters_skill_column() {
    let mut row = monstats(AI, [0; 5], 15);
    row.skill2 = 7;
    let mut w = World::new(row);
    w.modes[0][1] = mode::SKILL1;
    w.fake.rules = mod_rules(&graph(
        r#"
[[ai.state]]
name = "fight"
[[ai.state.action]]
kind = "use_skill"
slot = 3
[[ai.state.action]]
kind = "use_skill"
slot = 2
at = "self"
"#,
    ));
    let mon = w.mon;
    w.with(|g, cx| install(g, cx, mon, 0));
    run_at(&mut w, 10, true, 1);
    // Skill3 is empty and is passed over; Skill2 is used on the monster itself.
    assert!(
        w.fake.log.contains(&"skill 7".to_owned()),
        "{:?}",
        w.fake.log
    );
    assert_eq!(
        last_mode(&w),
        format!("mode {} Unit({:?})", mode::SKILL1, w.mon)
    );
}

#[test]
fn the_second_target_is_a_condition_and_an_aim() {
    let none = |_: &mut World| {};
    let second = |w: &mut World| {
        let s = add_mate(w, (5, 5));
        w.fake.secondary = Some((s, 8));
    };
    // No second target: none of the three holds.
    assert!(!holds(r#"{ kind = "second_target" }"#, none));
    assert!(!holds(
        r#"{ kind = "second_distance_below", value = 99 }"#,
        none
    ));
    assert!(!holds(
        r#"{ kind = "second_distance_above", value = 0 }"#,
        none
    ));
    // Its distance is 8.
    assert!(holds(r#"{ kind = "second_target" }"#, second));
    assert!(holds(
        r#"{ kind = "second_distance_below", value = 9 }"#,
        second
    ));
    assert!(!holds(
        r#"{ kind = "second_distance_below", value = 8 }"#,
        second
    ));
    assert!(holds(
        r#"{ kind = "second_distance_above", value = 8 }"#,
        second
    ));
    assert!(!holds(
        r#"{ kind = "second_distance_above", value = 9 }"#,
        second
    ));
    // A state on the second target.
    assert!(holds(
        r#"{ kind = "has_state", state = 9, who = "second" }"#,
        |w| {
            second(w);
            let (s, _) = w.fake.secondary.unwrap();
            w.fake.states.insert((s, 9));
        }
    ));
    assert!(!holds(
        r#"{ kind = "has_state", state = 9, who = "second" }"#,
        none
    ));
}

#[test]
fn an_action_can_aim_at_the_second_target() {
    let mut w = world(&graph(
        r#"
[[ai.state]]
name = "fight"
[[ai.state.action]]
kind = "attack"
attack = 2
at = "second"
when = [{ kind = "second_distance_below", value = 5 }]
[[ai.state.action]]
kind = "move_to"
at = "second"
steps = 3
when = [{ kind = "second_target" }]
[[ai.state.action]]
kind = "attack"
"#,
    ));
    let s = add_mate(&mut w, (5, 5));
    // The second target is too far for the swing: the walk goes to it.
    w.fake.secondary = Some((s, 8));
    run_at(&mut w, 10, true, 1);
    assert_eq!(last_mode(&w), format!("mode 2 Unit({s:?})"));
    // Close enough: the swing is aimed at it, not at the main target.
    w.fake.secondary = Some((s, 4));
    run_at(&mut w, 40, true, 1);
    assert_eq!(last_mode(&w), format!("mode 5 Unit({s:?})"));
    // Without a second target the swing falls to the main target.
    w.fake.secondary = None;
    run_at(&mut w, 80, true, 1);
    assert_eq!(last_mode(&w), format!("mode 4 Unit({:?})", w.player));
}

#[test]
fn a_hop_walks_in_radius_of_the_target_and_may_aim_at_the_second() {
    let mut w = world(&graph(
        r#"
[[ai.state]]
name = "close_in"
[[ai.state.action]]
kind = "hop_to"
steps = 9
keep = 4
"#,
    ));
    run_at(&mut w, 10, false, 20);
    assert_eq!(w.fake.log.last().unwrap(), "radius 9 4");
    let mut w = world(&graph(
        r#"
[[ai.state]]
name = "close_in"
[[ai.state.action]]
kind = "hop_to"
"#,
    ));
    run_at(&mut w, 10, false, 20);
    assert_eq!(w.fake.log.last().unwrap(), "radius 12 0");
    // With nothing to aim at, a hop at the second target does not happen.
    let mut w = world(&graph(
        r#"
[[ai.state]]
name = "close_in"
[[ai.state.action]]
kind = "hop_to"
at = "second"
"#,
    ));
    run_at(&mut w, 10, false, 20);
    assert!(!w.fake.log.iter().any(|l| l.starts_with("radius")));
}

/// An undead monster of the world's monster's side, `at` away along x.
fn undead_at(w: &mut World, at: i32) -> UnitId {
    w.monstats[0].lundead = true;
    let v = w.add_unit(UnitType::Monster, (100 + at, 100));
    w.fake.x.flags.insert(v, 0x2);
    v
}

#[test]
fn the_undead_scan_sees_corpses_and_hurt_mates_within_a_radius() {
    let none = |_: &mut World| {};
    let mate = |w: &mut World| {
        undead_at(w, 10);
    };
    let corpse = |w: &mut World| {
        let v = undead_at(w, 10);
        w.fake.anim.insert(v, mode::DEAD);
    };
    let hurt = |w: &mut World| {
        let v = undead_at(w, 10);
        w.fake.stats.insert((v, 6), 50);
        w.fake.x.max_life.insert(v, 100);
    };
    let any = r#"{ kind = "undead_within", radius = 24 }"#;
    assert!(!holds(any, none));
    assert!(holds(any, mate));
    assert!(holds(any, corpse));
    assert!(!holds(r#"{ kind = "undead_within", radius = 9 }"#, mate));
    assert!(!holds(
        r#"{ kind = "undead_within", radius = 24, at_least = 2 }"#,
        mate
    ));
    assert!(holds(
        r#"{ kind = "undead_within", radius = 24, at_least = 2 }"#,
        |w| {
            undead_at(w, 8);
            undead_at(w, 12);
        }
    ));
    let c = r#"{ kind = "corpse_within", radius = 24 }"#;
    assert!(holds(c, corpse) && !holds(c, mate) && !holds(c, hurt));
    let h = r#"{ kind = "hurt_within", radius = 24 }"#;
    assert!(holds(h, hurt) && !holds(h, mate) && !holds(h, corpse));
    assert!(!holds(r#"{ kind = "corpse_within", radius = 9 }"#, corpse));
}

#[test]
fn a_skill_can_be_aimed_at_the_corpse_or_the_hurt_mate() {
    let toml = graph(
        r#"
[[ai.state]]
name = "tend"
[[ai.state.action]]
kind = "use_skill"
slot = 2
at = "hurt"
when = [{ kind = "hurt_within", radius = 24 }]
[[ai.state.action]]
kind = "use_skill"
slot = 1
at = "corpse"
radius = 12
when = [{ kind = "corpse_within", radius = 12 }]
"#,
    );
    let setup = |w: &mut World| {
        w.monstats[0].skill1 = 7;
        w.monstats[0].skill2 = 8;
        w.modes[0][0] = 14;
        w.modes[0][1] = 14;
    };
    let mut w = world(&toml);
    setup(&mut w);
    let corpse = undead_at(&mut w, 10);
    w.fake.anim.insert(corpse, mode::DEAD);
    run_at(&mut w, 10, false, 6);
    assert_eq!(last_mode(&w), format!("mode 14 Unit({corpse:?})"));
    assert!(
        w.fake.log.contains(&"skill 7".to_owned()),
        "{:?}",
        w.fake.log
    );
    // A hurt mate comes first in the list.
    let mut w = world(&toml);
    setup(&mut w);
    let hurt = undead_at(&mut w, 10);
    w.fake.stats.insert((hurt, 6), 50);
    w.fake.x.max_life.insert(hurt, 100);
    run_at(&mut w, 10, false, 6);
    assert_eq!(last_mode(&w), format!("mode 14 Unit({hurt:?})"));
    assert!(
        w.fake.log.contains(&"skill 8".to_owned()),
        "{:?}",
        w.fake.log
    );
}

#[test]
fn living_monsters_of_a_class_are_counted_near_the_monster() {
    let crowd = |n: i32, class: i32| {
        move |w: &mut World| {
            super::act6::grow(w, 462);
            w.monstats[453].baseid = 453;
            w.monstats[461].baseid = 461;
            for i in 0..n {
                let v = w.add_unit(UnitType::Monster, (110 + i, 100));
                w.fake.class.insert(v, class);
            }
        }
    };
    let below = r#"{ kind = "class_count_below", class = 453, count = 3 }"#;
    let least = r#"{ kind = "class_count_at_least", class = 453, count = 3 }"#;
    assert!(holds(below, crowd(2, 453)) && !holds(least, crowd(2, 453)));
    assert!(!holds(below, crowd(3, 453)) && holds(least, crowd(3, 453)));
    // Another class does not count, unless `also` names it.
    assert!(holds(below, crowd(3, 461)));
    let both = r#"{ kind = "class_count_at_least", class = 453, also = 461, count = 3 }"#;
    assert!(holds(both, crowd(3, 461)));
    // A fallen monster is not alive.
    assert!(holds(below, |w| {
        crowd(3, 453)(w);
        let dead = w.fake.class.keys().copied().last().unwrap();
        w.fake.anim.insert(dead, mode::DEAD);
    }));
}

#[test]
fn thinks_are_counted_from_the_state_entered() {
    let mut w = world(&graph(
        r#"
[[ai.state]]
name = "a"
[[ai.state.transition]]
to = "b"
when = [{ kind = "thinks_at_least", count = 3 }]
[[ai.state]]
name = "b"
[[ai.state.transition]]
to = "a"
when = [{ kind = "thinks_at_least", count = 2 }]
"#,
    ));
    let mut seen = Vec::new();
    for i in 0..6 {
        run_at(&mut w, 10 + 20 * i, false, 6);
        seen.push(state(&w));
    }
    // The third think moves to b (and counts as b's first); b's second moves back.
    assert_eq!(seen, [0, 0, 1, 0, 0, 1]);
}

#[test]
fn a_state_can_be_switched_on_by_the_graph() {
    let mut w = world(&graph(
        r#"
[[ai.state]]
name = "enrage"
[[ai.state.action]]
kind = "set_state"
state = 77
[[ai.state.action]]
kind = "idle"
"#,
    ));
    run_at(&mut w, 10, true, 1);
    assert!(w.fake.states.contains(&(w.mon, 77)));
}

#[test]
fn a_graph_without_a_target_still_thinks_when_it_asks_to() {
    let mut w = world(&graph(
        r#"
target = "any"
[[ai.state]]
name = "guard"
[[ai.state.action]]
kind = "wander"
radius = 3
when = [{ kind = "no_target" }]
"#,
    ));
    let mon = w.mon;
    let rec = w.with(|_, cx| cx.record(mon));
    assert_eq!(rec.target_mode, 2);
    run_with(&mut w, false, false, 0);
    assert!(
        last_mode(&w).starts_with("mode 2 Point"),
        "{:?}",
        w.fake.log
    );
}

/// What a run of thinks does, for comparing replays.
fn replay(seed: u32, toml: &str) -> Vec<String> {
    let mut w = world(toml);
    w.seed(seed);
    for n in 0..6 {
        run_at(&mut w, 10 + 40 * n, n % 2 == 0, 3 + n);
    }
    w.fake.log.clone()
}

#[test]
fn the_same_seed_plays_the_same_game() {
    let toml = graph(
        r#"
[[ai.state]]
name = "fight"
[[ai.state.action]]
kind = "attack"
when = [{ kind = "random", percent = 40 }]
[[ai.state.action]]
kind = "move_around"
when = [{ kind = "random", percent = 50 }]
[[ai.state.action]]
kind = "wander"
"#,
    );
    let runs: Vec<_> = SEEDS.iter().map(|&s| replay(s, &toml)).collect();
    for (&s, run) in SEEDS.iter().zip(&runs) {
        assert_eq!(run, &replay(s, &toml), "seed {s}");
    }
    assert!(
        runs.iter().any(|r| r != &runs[0]),
        "the seeds all played alike"
    );
}

/// A graph at every limit whose think looks at as much as it can: four
/// phases that start in the think, four transitions that are followed after every other one failed on its last
/// condition, then the largest list of actions, each failing on its last
/// condition.
fn largest_graph() -> String {
    // True without a target / false without a target.
    let yes = r#"{ kind = "no_target" }"#;
    let no = r#"{ kind = "has_target" }"#;
    let when = |last: &str| {
        let mut c = vec![yes; MAX_CONDITIONS - 1];
        c.push(last);
        format!("when = [{}]\n", c.join(", "))
    };
    let mut s = String::from("target = \"any\"\n");
    // Four phases that all start in the one think.
    for n in 0..MAX_HOPS {
        s.push_str(&format!("[[ai.phase]]\nname = \"p{n}\"\n"));
        s.push_str(&when(yes));
    }
    let names = ["s0", "s1", "s2", "s3", "s4"];
    for (i, name) in names.iter().enumerate() {
        s.push_str(&format!("[[ai.state]]\nname = \"{name}\"\n"));
        if i < 4 {
            for _ in 0..MAX_TRANSITIONS - 1 {
                s.push_str(&format!("[[ai.state.transition]]\nto = \"s{}\"\n", 4));
                s.push_str(&when(no));
            }
            s.push_str(&format!("[[ai.state.transition]]\nto = \"s{}\"\n", i + 1));
            s.push_str(&when(yes));
        } else {
            for _ in 0..MAX_ACTIONS {
                s.push_str("[[ai.state.action]]\nkind = \"idle\"\n");
                s.push_str(&when(no));
            }
        }
    }
    graph(&s)
}

#[test]
fn one_think_stays_inside_the_budget_even_for_the_largest_graph() {
    let toml = largest_graph();
    let files = DataModFiles {
        ai: Some(("ai.toml", toml.as_str())),
        ..DataModFiles::default()
    };
    let host = DataMod::from_files("big", &files).unwrap();
    let rules = host.ai_rules();
    let mut w = world(&toml);
    let (mon, p) = (w.mon, param(&w, false, false, 0));
    let cost = w
        .with(|g, cx| rules.think_cost(g, &mut cx.as_dyn(), THINK_BASE, mon, &p))
        .unwrap();
    // Four hops of seven failed transitions and the one that is followed,
    // then every action tried and passed over.
    assert_eq!(cost, THINK_BUDGET);
    assert_eq!(state(&w), 4);
    // A think that follows nothing and tries nothing is far cheaper.
    let one = graph("[[ai.state]]\nname = \"a\"\n");
    let host = DataMod::from_files(
        "small",
        &DataModFiles {
            ai: Some(("ai.toml", one.as_str())),
            ..DataModFiles::default()
        },
    )
    .unwrap();
    let mut small = world(&one);
    let (mon, p) = (small.mon, param(&small, false, false, 0));
    let cost = small
        .with(|g, cx| {
            host.ai_rules()
                .think_cost(g, &mut cx.as_dyn(), THINK_BASE, mon, &p)
        })
        .unwrap();
    assert_eq!(cost, 0);
}

#[test]
fn only_graph_ids_are_answered() {
    let host = DataMod::from_files(
        "graphs",
        &DataModFiles {
            ai: Some(("ai.toml", &graph(FIGHT))),
            ..DataModFiles::default()
        },
    )
    .unwrap();
    let rules = host.ai_rules();
    assert!(rules.ai_record(AI).is_some());
    assert!(rules.ai_record(AI + 1).is_none());
    assert!(rules.ai_record(3).is_none());
    let mut w = world(&graph(FIGHT));
    let (mon, p) = (w.mon, param(&w, true, true, 1));
    for function in [0x005E_FE20, THINK_BASE + 1, 0] {
        let got = w.with(|g, cx| rules.think_cost(g, &mut cx.as_dyn(), function, mon, &p));
        assert_eq!(got, None, "{function:#x}");
    }
}

#[test]
fn monsters_without_a_graph_play_as_they_do_without_the_mod() {
    // Zombie (AI 3) under a mod that only knows AI 200: the same draws and
    // the same moves as under D2Gamma's own rules.
    let play = |with_mod: bool, seed: u32| {
        let mut w = World::new(monstats(3, [30, 10, 0, 20, 0], 15));
        if with_mod {
            w.fake.rules = mod_rules(&graph(FIGHT));
            let mon = w.mon;
            w.with(|g, cx| install(g, cx, mon, 0));
        }
        w.seed(seed);
        for n in 0..5 {
            run_at(&mut w, 10 + 30 * n, n % 2 == 0, 1 + n);
        }
        (
            w.fake.log.clone(),
            w.thinks(),
            w.store.control(w.mon).cloned(),
        )
    };
    for seed in SEEDS {
        assert_eq!(play(false, seed), play(true, seed), "seed {seed}");
    }
}

#[test]
fn a_graph_can_replace_one_of_the_built_in_ais() {
    let mut w = World::new(monstats(3, [30, 10, 0, 20, 0], 15));
    w.fake.rules = mod_rules(&format!(
        "[[ai]]\nindex = 3\n{}",
        r#"
[[ai.state]]
name = "stand"
[[ai.state.action]]
kind = "attack"
attack = 2
"#
    ));
    let mon = w.mon;
    w.with(|g, cx| install(g, cx, mon, 0));
    assert_eq!(w.store.control(mon).unwrap().function, THINK_BASE);
    run_at(&mut w, 10, true, 1);
    assert_eq!(last_mode(&w), format!("mode 5 Unit({:?})", w.player));
}

#[test]
fn a_reinstall_starts_the_graph_afresh() {
    let mut w = world(&graph(
        r#"
[[ai.state]]
name = "a"
[[ai.state.transition]]
to = "b"
[[ai.state]]
name = "b"
"#,
    ));
    run_at(&mut w, 10, false, 1);
    assert_eq!(state(&w), 1);
    // The reinstall of the same AI (a special state ending, a raise) goes
    // back to the start.
    w.store.control_mut(w.mon).unwrap().function = 0;
    let mon = w.mon;
    w.with(|g, cx| install(g, cx, mon, 0));
    assert_eq!(state(&w), 0);
}

const TEMPLATE: &str = include_str!("../../../../../../templates/monster-ai-graph/files/ai.toml");

#[test]
fn the_template_graph_plays() {
    let rules = AiRules::from_toml(TEMPLATE).unwrap();
    assert!(rules.get(201).unwrap().is_preset());
    let mut w = world(TEMPLATE);
    let target = format!("mode 2 Unit({:?})", w.player);
    // Far away it runs at the target.
    run_at(&mut w, 100, false, 20);
    assert_eq!(state(&w), 0);
    assert!(w.fake.modes().last().is_some());
    // In range and in sight it telegraphs the leap first.
    run_at(&mut w, 200, false, 10);
    assert_eq!(
        kinds(&w),
        [GraphEventKind::WindUp {
            event: 0,
            ticks: 13
        }]
    );
    // In reach it fights.
    run_at(&mut w, 300, true, 1);
    assert_eq!(state(&w), 1);
    assert_eq!(last_mode(&w), format!("mode 4 Unit({:?})", w.player));
    // Nearly dead, it runs.
    w.fake.life = 10;
    run_at(&mut w, 400, true, 1);
    assert_eq!(state(&w), 2);
    assert_ne!(last_mode(&w), target);
}

/// A boss graph: a calm state, then an enrage at half life and a rout at 20%.
const BOSS: &str = r#"
[[ai.phase]]
name = "enrage"
when = [{ kind = "life_below", percent = 50 }]
state = "rage"
immune_seconds = 3

[[ai.phase]]
name = "rout"
when = [{ kind = "life_below", percent = 20 }]
graph = 202

[[ai.state]]
name = "calm"
[[ai.state.action]]
kind = "attack"
[[ai.state]]
name = "rage"
[[ai.state.action]]
kind = "attack"
attack = 2

[[ai]]
index = 202
[[ai.state]]
name = "flee"
[[ai.state.action]]
kind = "flee"
"#;

fn boss_world(immune: bool) -> World {
    let mut w = World::new(monstats(AI, [0; 5], 15));
    let toml = graph(BOSS);
    let cc = "immune_state = 900\n";
    let files = DataModFiles {
        ai: Some(("ai.toml", toml.as_str())),
        crowd_control: immune.then_some(("crowd_control.toml", cc)),
        ..DataModFiles::default()
    };
    w.fake.rules = DataMod::from_files("boss", &files).unwrap().into_ruleset();
    let mon = w.mon;
    w.with(|g, cx| install(g, cx, mon, 0));
    w
}

#[test]
fn a_phase_starts_once_when_its_conditions_hold() {
    let w = &mut boss_world(true);
    run_at(w, 10, true, 1);
    assert_eq!(state(w), 0);
    assert!(kinds(w).is_empty());
    w.fake.life = 40;
    run_at(w, 20, true, 1);
    assert_eq!(state(w), 1);
    assert_eq!(
        kinds(w),
        [
            GraphEventKind::PhaseChanged { phase: 0 },
            GraphEventKind::StateChanged { from: 0, to: 1 },
        ]
    );
    assert_eq!(last_mode(w), format!("mode 5 Unit({:?})", w.player));
    // Still hurt: the phase does not start again.
    run_at(w, 30, true, 1);
    assert_eq!(kinds(w).len(), 2);
    let run = w.store.get(w.mon).unwrap().graph.unwrap();
    assert_eq!(run.phase, 1);
    assert_eq!(w.store.events.take().len(), 2);
}

#[test]
fn the_phase_names_are_known() {
    let toml = graph(BOSS);
    let host = DataMod::from_files(
        "boss",
        &DataModFiles {
            ai: Some(("ai.toml", toml.as_str())),
            crowd_control: Some(("crowd_control.toml", "immune_state = 900")),
            ..DataModFiles::default()
        },
    )
    .unwrap();
    assert_eq!(host.ai_rules().phase_name(AI, 1), Some("rout"));
    assert_eq!(host.ai_rules().phase_name(AI, 2), None);
}

#[test]
fn an_immunity_window_gives_the_mods_state_for_the_stated_time() {
    let w = &mut boss_world(true);
    w.fake.life = 40;
    run_at(w, 20, true, 1);
    // Three seconds are 75 base ticks.
    assert!(
        w.fake.log.contains(&"timed 900 75".to_owned()),
        "{:?}",
        w.fake.log
    );
    assert!(w.fake.states.contains(&(w.mon, 900)));
}

#[test]
fn a_mod_without_an_immune_state_is_refused_when_a_phase_wants_one() {
    let toml = graph(BOSS);
    let files = DataModFiles {
        ai: Some(("ai.toml", toml.as_str())),
        ..DataModFiles::default()
    };
    let e = DataMod::from_files("boss", &files).unwrap_err().to_string();
    assert!(e.contains("`ai[0].phase[0].immune_seconds`"), "{e}");
    assert!(e.contains("immune_state"), "{e}");
}

#[test]
fn phases_start_in_order_even_when_one_think_passes_several() {
    let w = &mut boss_world(true);
    // Straight to 10% life: both thresholds are passed in one think.
    w.fake.life = 10;
    run_at(w, 20, true, 1);
    let seen = kinds(w);
    assert_eq!(seen[0], GraphEventKind::PhaseChanged { phase: 0 });
    assert!(seen.contains(&GraphEventKind::PhaseChanged { phase: 1 }));
    // The second phase handed the monster to graph 202.
    let c = w.store.control(w.mon).unwrap();
    assert_eq!(c.function, THINK_BASE + 1);
    let run = w.store.get(w.mon).unwrap().graph.unwrap();
    assert_eq!((run.ai, run.state, run.phase), (202, 0, 0));
    // It stands for a frame, then the new graph takes its turn.
    assert_eq!(w.thinks(), [21]);
    run_at(w, 21, true, 1);
    assert_eq!(w.store.get(w.mon).unwrap().graph.unwrap().ai, 202);
}

#[test]
fn a_later_phase_waits_for_the_earlier_ones() {
    let w = &mut boss_world(true);
    w.fake.life = 40;
    run_at(w, 20, true, 1);
    assert_eq!(w.store.get(w.mon).unwrap().graph.unwrap().phase, 1);
    assert_eq!(w.store.control(w.mon).unwrap().function, THINK_BASE);
    w.fake.life = 15;
    run_at(w, 30, true, 1);
    assert_eq!(w.store.control(w.mon).unwrap().function, THINK_BASE + 1);
}

#[test]
fn a_timer_phase_starts_after_the_time() {
    let w = &mut world(&graph(
        r#"
[[ai.phase]]
name = "late"
when = [{ kind = "alive_for", seconds = 2 }]
state = "b"
[[ai.state]]
name = "a"
[[ai.state]]
name = "b"
"#,
    ));
    let born = w.game.frame;
    // Two seconds are 50 ticks.
    run_at(w, born + 49, false, 1);
    assert_eq!(state(w), 0);
    run_at(w, born + 50, false, 1);
    assert_eq!(state(w), 1);
    assert_eq!(kinds(w)[0], GraphEventKind::PhaseChanged { phase: 0 });
}

#[test]
fn a_phase_without_conditions_starts_on_the_first_think() {
    let w = &mut world(&graph(
        "[[ai.phase]]\nname = \"intro\"\n[[ai.state]]\nname = \"a\"\n",
    ));
    run_at(w, 10, false, 1);
    assert_eq!(kinds(w), [GraphEventKind::PhaseChanged { phase: 0 }]);
}

#[test]
fn phases_leave_a_monster_without_them_as_it_was() {
    // The same graph with and without a phase that never starts plays alike.
    let play = |with_phase: bool| {
        let phase = if with_phase {
            "[[ai.phase]]\nname = \"never\"\nwhen = [{ kind = \"life_below\", percent = 0 }]\n"
        } else {
            ""
        };
        let mut w = world(&graph(&format!("{phase}{FIGHT}")));
        for n in 0..4 {
            run_at(&mut w, 10 + 20 * n, n % 2 == 0, 1 + n);
        }
        (w.fake.log.clone(), w.thinks())
    };
    assert_eq!(play(false), play(true));
}

#[test]
fn a_move_to_with_steps_and_speed_asks_for_them() {
    let mut w = world(&graph(
        r#"
[[ai.state]]
name = "close_in"
[[ai.state.action]]
kind = "move_to"
steps = 9
speed = 10
"#,
    ));
    run_with(&mut w, true, false, 20);
    assert_eq!(last_mode(&w), format!("mode 2 Unit({:?})", w.player));
    assert!(w.fake.log.contains(&"steps 9".to_owned()));
    let v = w.store.get(w.mon).unwrap().velocity;
    assert_eq!((v.method, v.speed, v.steps), (0, 10, 0));
}

#[test]
fn a_plain_move_to_walks_all_the_way_at_the_usual_speed() {
    let mut w = world(&graph(FIGHT));
    run_with(&mut w, true, false, 20);
    assert!(w.fake.log.contains(&"steps 1".to_owned()));
    assert_eq!(w.store.get(w.mon).unwrap().velocity, Default::default());
}

/// A graph for a pet: the player owns the monster, the sides are good, the
/// owner stands 3 sub-tiles east.
fn pet_world(body: &str) -> World {
    let mut w = world(&graph(body));
    super::act6::own(&mut w);
    w.fake.align = 1;
    w.fake.pos.insert(w.player, (103, 100));
    w
}

#[test]
fn the_owner_is_a_condition_and_a_distance() {
    let none = |_: &mut World| {};
    let owned = |w: &mut World| super::act6::own(w);
    assert!(!holds(r#"{ kind = "has_owner" }"#, none));
    assert!(holds(r#"{ kind = "no_owner" }"#, none));
    assert!(holds(r#"{ kind = "has_owner" }"#, owned));
    assert!(!holds(r#"{ kind = "no_owner" }"#, owned));
    // No owner: no distance either way.
    assert!(!holds(
        r#"{ kind = "owner_distance_below", value = 99 }"#,
        none
    ));
    assert!(!holds(
        r#"{ kind = "owner_distance_above", value = 0 }"#,
        none
    ));
    let near = |w: &mut World| {
        super::act6::own(w);
        w.fake.pos.insert(w.player, (103, 100));
    };
    let far = |w: &mut World| {
        super::act6::own(w);
        w.fake.pos.insert(w.player, (150, 100));
    };
    let below = r#"{ kind = "owner_distance_below", value = 10 }"#;
    let above = r#"{ kind = "owner_distance_above", value = 10 }"#;
    assert!(holds(below, near) && !holds(above, near));
    assert!(!holds(below, far) && holds(above, far));
}

#[test]
fn the_owners_target_is_a_condition_and_an_aim() {
    let enemy = |flag: u32| {
        move |w: &mut World| {
            super::act6::own(w);
            w.fake.align = 1;
            let e = w.add_unit(UnitType::Monster, (112, 100));
            w.fake.good = Some((e, 10));
            w.fake.x.flags.insert(e, flag);
        }
    };
    let cond = r#"{ kind = "owner_target" }"#;
    assert!(holds(cond, enemy(0)));
    // An enemy flagged to be left alone does not count, nor does having no owner.
    assert!(!holds(cond, enemy(0x4000_0000)));
    assert!(!holds(cond, |w: &mut World| {
        w.fake.align = 1;
        let e = w.add_unit(UnitType::Monster, (112, 100));
        w.fake.good = Some((e, 10));
    }));
    let mut w = pet_world(
        r#"
[[ai.state]]
name = "help"
[[ai.state.action]]
kind = "attack"
at = "owner_target"
[[ai.state.action]]
kind = "use_skill"
slot = 1
at = "owner"
"#,
    );
    let e = w.add_unit(UnitType::Monster, (112, 100));
    w.fake.good = Some((e, 10));
    run_at(&mut w, 10, false, 6);
    assert_eq!(last_mode(&w), format!("mode 4 Unit({e:?})"));
    // Without an enemy for the owner the attack is passed over.
    let mut w = pet_world(
        r#"
[[ai.state]]
name = "help"
[[ai.state.action]]
kind = "attack"
at = "owner_target"
[[ai.state.action]]
kind = "move_to"
at = "owner"
"#,
    );
    run_at(&mut w, 10, false, 6);
    assert_eq!(last_mode(&w), format!("mode 2 Unit({:?})", w.player));
}

#[test]
fn a_pet_follows_its_owner_as_the_built_in_pets_do() {
    const BODY: &str = r#"
[[ai.state]]
name = "pet"
[[ai.state.action]]
kind = "follow_owner"
steps = 6
quiet = true
[[ai.state.action]]
kind = "idle"
seconds = 1
"#;
    // Left far behind (60 away) and nothing to fight: the pet catches up.
    let mut w = pet_world(BODY);
    w.fake.pos.insert(w.player, (160, 100));
    run_with(&mut w, false, false, 0);
    assert!(w.fake.log.contains(&"freespot2 0 -1".to_owned()));
    // Where it should be, and quiet: it does nothing and the next action runs.
    let mut w = pet_world(BODY);
    w.fake.pos.insert(w.player, (107, 100));
    run_with(&mut w, false, false, 0);
    assert_eq!(w.thinks(), [25]);
    // An enemy in reach (the target, within 80): the follow leaves it to the
    // actions after it.
    let mut w = pet_world(BODY);
    let e = w.add_unit(UnitType::Monster, (104, 100));
    let p = TickParam {
        target: Some(e),
        ..param(&w, true, true, 4)
    };
    think(&mut w, p);
    assert_eq!(w.thinks(), [25]);
    // No owner: nothing to follow, the next action runs.
    let mut w = world(&graph(BODY));
    run_with(&mut w, false, false, 0);
    assert_eq!(w.thinks(), [25]);
}
