use super::*;
use d2_sim::rng::Seed;
use d2_sim::rules::*;
use d2_sim::units::UnitType;

struct World {
    frame: i32,
    value: i32,
    stacks: u16,
    seed: Seed,
    errors: Vec<String>,
    messages: Vec<String>,
    ui_variables: Vec<(String, i32)>,
    damage: i32,
    spawned: usize,
    reenter: Option<Arc<ScriptHost>>,
}
impl Default for World {
    fn default() -> Self {
        Self {
            frame: 0,
            value: 0,
            stacks: 0,
            seed: Seed::new(1, 666),
            errors: Vec::new(),
            messages: Vec::new(),
            ui_variables: Vec::new(),
            damage: 0,
            spawned: 0,
            reenter: None,
        }
    }
}
impl HostUnits for World {
    fn unit_type(&self, u: UnitId) -> Option<UnitType> {
        match u.0 {
            1 => Some(UnitType::Player),
            2 => Some(UnitType::Monster),
            _ => None,
        }
    }
}
impl HostSkills for World {}
impl HostStates for World {
    fn set_stat_layer(&mut self, _: UnitId, _: u16, _: u16, v: i32) {
        self.value = v;
    }
    fn state_stacks(&self, _: UnitId, _: u16) -> u16 {
        self.stacks
    }
    fn apply_ruled_state(&mut self, _: UnitId, _: UnitId, _: &RuledState) -> bool {
        self.stacks = 1;
        true
    }
    fn add_state_stacks(&mut self, _: UnitId, _: UnitId, _: &RuledState, n: u16) -> u16 {
        self.stacks += n;
        self.stacks
    }
    fn remove_state_stacks(&mut self, _: UnitId, _: u16, n: u16) -> u16 {
        self.stacks = self.stacks.saturating_sub(n);
        self.stacks
    }
    fn clear_state(&mut self, _: UnitId, _: u16) {
        self.stacks = 0;
    }
}
impl HostMissiles for World {
    fn fire_skill_missile(&mut self, _: UnitId, _: i32, _: i32, _: i32, _: bool) -> Option<UnitId> {
        self.spawned += 1;
        Some(UnitId(2))
    }
}
impl HostDamage for World {
    fn apply_damage(
        &mut self,
        _: UnitId,
        _: UnitId,
        _: bool,
        rec: &mut d2_sim::combat::DamageRecord,
    ) {
        self.damage += rec.physical;
        if let Some(runtime) = self.reenter.clone() {
            runtime.event(self, event(EventKind::Kill));
        }
    }
}
impl HostTimers for World {}
impl SkillActions for World {}
impl SkillSearch for World {}
impl SkillBar for World {}
impl SkillPets for World {
    fn summon(&mut self, _: d2_sim::skills::use_::bodies::Summon<UnitId>) -> Option<UnitId> {
        self.spawned += 1;
        Some(UnitId(2))
    }
}
impl SkillRespec for World {}
impl SkillHost for World {
    fn frame(&self) -> i32 {
        self.frame
    }
    fn class_id(&self, _: UnitId) -> i32 {
        0
    }
    fn stat(&self, _: UnitId, _: u16, _: u16) -> i32 {
        self.value
    }
    fn base_stat(&self, _: UnitId, _: u16, _: u16) -> i32 {
        self.value
    }
    fn has_state(&self, _: UnitId, _: u16) -> bool {
        self.stacks > 0
    }
    fn set_stat(&mut self, _: UnitId, _: u16, v: i32) {
        self.value = v;
    }
    fn seed(&mut self, _: UnitId) -> &mut Seed {
        &mut self.seed
    }
    fn script_id_valid(&self, _: &str, id: u16) -> bool {
        id < 10
    }
    fn script_error(&mut self, message: &str) {
        self.errors.push(message.to_owned());
    }
    fn script_message(&mut self, _: UnitId, message: &str) {
        self.messages.push(message.to_owned());
    }
    fn script_ui_variable(&mut self, _: UnitId, name: &str, value: i32) {
        self.ui_variables.push((name.into(), value));
    }
}

#[test]
fn scripts_publish_qualified_player_values_and_reject_invalid_names_and_targets() {
    let host = host("return { on_kill = function(e,api) api.ui_variable(e.unit, 'rage', 42) end }");
    let mut w = World::default();
    host.event(&mut w, event(EventKind::Kill));
    assert_eq!(w.ui_variables, [("test/rage".into(), 42)]);
    for call in [
        "api.ui_variable(e.other,'rage',1)",
        "api.ui_variable(e.unit,'',1)",
        "api.ui_variable(e.unit,'bad name',1)",
        "api.ui_variable(e.unit,'rage',1.5)",
    ] {
        let host = super::events_tests::host(&format!(
            "return {{ on_kill = function(e,api) {call} end }}"
        ));
        let mut w = World::default();
        host.event(&mut w, event(EventKind::Kill));
        assert!(w.ui_variables.is_empty());
        assert_eq!(w.errors.len(), 1);
    }
}

fn host(text: &str) -> ScriptHost {
    super::tests::source(text).instantiate().unwrap().unwrap()
}
fn event(kind: EventKind) -> Event {
    Event {
        kind,
        unit: UnitId(1),
        other: Some(UnitId(2)),
        id: 7,
    }
}

#[test]
fn visual_graph_runs_on_the_m2_host_with_bounded_loops_variables_and_math() {
    let graph = d2_modkit::graph::Graph::parse(
        r#"
version=1
[[nodes]]
id='entry'
kind='event'
event='kill'
next='loop'
[[nodes]]
id='loop'
kind='loop'
count=3
body='increment'
next='publish'
[[nodes]]
id='get'
kind='variable'
op='get'
name='kills'
[[nodes]]
id='add'
kind='math'
op='add'
[nodes.inputs]
a={node='get',pin='value'}
b=1
[[nodes]]
id='increment'
kind='variable'
op='set'
name='kills'
[nodes.inputs]
value={node='add',pin='value'}
[[nodes]]
id='publish'
kind='api'
name='set_stat'
[nodes.inputs]
unit={node='entry',pin='unit'}
stat=1
layer=0
value={node='get',pin='value'}
"#,
    )
    .unwrap();
    let runtime = host(&graph.compile().unwrap());
    let mut w = World::default();
    runtime.event(&mut w, event(EventKind::Kill));
    assert!(w.errors.is_empty(), "{:?}", w.errors);
    assert_eq!(w.value, 3);
    runtime.event(&mut w, event(EventKind::Kill));
    assert_eq!(w.value, 6);
    runtime.event(&mut w, event(EventKind::CastStart));
    assert_eq!(w.value, 6);
}

#[test]
fn visual_graph_nested_loops_stop_at_the_graph_budget_and_math_is_checked() {
    use d2_modkit::graph::{Graph, Input, Node};
    let entry = Node {
        id: "entry".into(),
        kind: "event".into(),
        event: "kill".into(),
        next: "outer".into(),
        ..Default::default()
    };
    let mut g = Graph {
        version: 1,
        nodes: vec![
            entry,
            Node {
                id: "outer".into(),
                kind: "loop".into(),
                count: 64,
                body: "inner".into(),
                ..Default::default()
            },
            Node {
                id: "inner".into(),
                kind: "loop".into(),
                count: 64,
                body: "set".into(),
                ..Default::default()
            },
            Node {
                id: "set".into(),
                kind: "variable".into(),
                op: "set".into(),
                name: "v".into(),
                inputs: BTreeMap::from([("value".into(), Input::Integer(0))]),
                ..Default::default()
            },
        ],
    };
    let runtime = host(&g.compile().unwrap());
    let mut w = World::default();
    runtime.event(&mut w, event(EventKind::Kill));
    assert_eq!(w.errors.len(), 1);
    assert!(
        w.errors[0].contains("graph step budget exceeded at node"),
        "{:?}",
        w.errors
    );
    g.nodes.truncate(1);
    g.nodes[0].next = "set".into();
    g.nodes.push(Node {
        id: "math".into(),
        kind: "math".into(),
        op: "divide".into(),
        inputs: BTreeMap::from([
            ("a".into(), Input::Integer(10)),
            ("b".into(), Input::Integer(0)),
        ]),
        ..Default::default()
    });
    g.nodes.push(Node {
        id: "set".into(),
        kind: "variable".into(),
        op: "set".into(),
        name: "v".into(),
        inputs: BTreeMap::from([(
            "value".into(),
            Input::Link {
                node: "math".into(),
                pin: "value".into(),
            },
        )]),
        ..Default::default()
    });
    let runtime = host(&g.compile().unwrap());
    let mut w = World::default();
    runtime.event(&mut w, event(EventKind::Kill));
    assert!(w.errors[0].contains("division by zero"));
    g.nodes[1].op = "multiply".into();
    g.nodes[1]
        .inputs
        .insert("a".into(), Input::Integer(i32::MAX));
    g.nodes[1].inputs.insert("b".into(), Input::Integer(2));
    let runtime = host(&g.compile().unwrap());
    let mut w = World::default();
    runtime.event(&mut w, event(EventKind::Kill));
    assert!(w.errors[0].contains("integer overflow"));
}

#[test]
fn visual_graph_branching_and_text_literals_execute_without_source_injection() {
    let graph = d2_modkit::graph::Graph::parse(
        r#"
version=1
[[nodes]]
id='entry'
kind='event'
event='kill'
next='branch'
[[nodes]]
id='branch'
kind='branch'
yes='message'
[nodes.inputs]
condition=true
[[nodes]]
id='message'
kind='api'
name='message'
[nodes.inputs]
unit={node='entry',pin='unit'}
text="'; error('injected'); --"
"#,
    )
    .unwrap();
    let runtime = host(&graph.compile().unwrap());
    let mut w = World::default();
    runtime.event(&mut w, event(EventKind::Kill));
    assert!(w.errors.is_empty(), "{:?}", w.errors);
    assert_eq!(w.messages, ["'; error('injected'); --"]);
}

#[test]
fn every_versioned_event_has_a_validated_handler_and_context() {
    for kind in EventKind::ALL {
        let source = format!("return {{ on_{} = function(e, api) assert(e.version == 1 and e.event == '{}' and e.id == 7 and e.other == 2); api.set_stat(e.unit, 1, 0, 42) end }}", kind.name(), kind.name());
        let host = host(&source);
        let mut w = World::default();
        host.event(&mut w, event(kind));
        assert!(w.errors.is_empty(), "{:?}", w.errors);
        assert_eq!(w.value, 42);
        assert!(
            super::tests::source(&format!("return {{ on_{} = 12 }}", kind.name()))
                .instantiate()
                .is_err()
        );
    }
}

#[test]
fn capabilities_read_the_live_world_and_write_through_engine_seams() {
    let host = host(
        r#"return { on_attack_start = function(e, api)
        api.set_stat(e.unit, 1, 0, api.stat(e.unit, 1, 0) + 12)
        assert(api.base_stat(e.unit, 1, 0) == 12)
        api.apply_state(e.unit, e.other, 1, 25, 0)
        api.add_stacks(e.unit, e.other, 1, 25, 0, 2)
        assert(api.has_state(e.other, 1) and api.state_stacks(e.other, 1) == 3)
        api.remove_stacks(e.other, 1, 1)
        assert(api.state_stacks(e.other, 1) == 2)
        api.clear_state(e.other, 1)
        api.damage(e.unit, e.other, 256, 0)
        assert(api.spawn_missile(e.unit, 1, 0, 1, 0) == 2)
        assert(api.spawn_monster(e.unit, 1, 10, 10, 1, 2) == 2)
        api.message(e.unit, "ready")
    end }"#,
    );
    let mut w = World::default();
    host.event(&mut w, event(EventKind::AttackStart));
    assert!(w.errors.is_empty(), "{:?}", w.errors);
    assert_eq!((w.value, w.stacks, w.damage, w.spawned), (12, 0, 256, 2));
    assert_eq!(w.messages, ["ready"]);
}

#[test]
fn strict_boundaries_and_scoped_capabilities_report_errors_without_panicking() {
    for body in [
        "api.set_stat(e.unit,1,0,0.5)",
        "api.set_stat(e.unit,99,0,12)",
        "api.stat(999,1,0)",
        "api.random(e.unit,0)",
        "api.message(e.unit,string.rep('x',201))",
        "api.timer(e.unit,0,0)",
        "api.damage(e.unit,e.other,-1,0)",
        "for i=1,129 do api.stat(e.unit,1,0) end",
    ] {
        let host = host(&format!(
            "return {{ on_attack_start = function(e, api) {body} end }}"
        ));
        let mut w = World::default();
        host.event(&mut w, event(EventKind::AttackStart));
        assert_eq!(w.errors.len(), 1, "{body}");
        assert!(w.errors[0].contains("test.luau::on_attack_start"));
        assert_eq!(w.value, 0);
    }
    let host = host("local saved; return { on_attack_start = function(e,api) saved=api.stat end, on_kill = function(e,api) saved(e.unit,1,0) end }");
    let mut w = World::default();
    host.event(&mut w, event(EventKind::AttackStart));
    host.event(&mut w, event(EventKind::Kill));
    assert_eq!(w.errors.len(), 1);
}

#[test]
fn timers_are_frame_ordered_module_local_and_rng_is_seeded() {
    let text = r#"return {
        on_attack_start = function(e,api)
            api.timer(e.unit,2,2); api.timer(e.unit,1,1); api.timer(e.unit,2,3)
            api.set_stat(e.unit,1,0,api.random(e.unit,100000))
        end,
        on_timer = function(e,api) api.message(e.unit,tostring(e.id)) end
    }"#;
    let mut a = World::default();
    let mut b = World::default();
    let host_a = host(text);
    let host_b = host(text);
    host_a.event(&mut a, event(EventKind::AttackStart));
    host_b.event(&mut b, event(EventKind::AttackStart));
    assert_eq!(a.value, b.value);
    assert_eq!(a.seed, b.seed);
    host_a.tick(&mut a);
    assert!(a.messages.is_empty());
    a.frame = 2;
    host_a.tick(&mut a);
    assert_eq!(a.messages, ["1", "2", "3"]);
    host_a.tick(&mut a);
    assert_eq!(a.messages.len(), 3);
    assert!(a.errors.is_empty(), "{:?}", a.errors);
}

#[test]
fn runtime_budget_resets_and_broadcast_order_is_lexical() {
    let host = host("return { on_attack_start = function() while true do end end, on_kill = function(e,a) a.set_stat(e.unit,1,0,3) end }");
    let mut w = World::default();
    host.event(&mut w, event(EventKind::AttackStart));
    host.event(&mut w, event(EventKind::Kill));
    assert!(w.errors[0].contains("budget"));
    assert_eq!(w.value, 3);
    let host = Sources(BTreeMap::from([
        (
            "b.luau".into(),
            b"return { on_kill=function(e,a) a.set_stat(e.unit,1,0,a.stat(e.unit,1,0)*10+2) end }"
                .to_vec(),
        ),
        (
            "a.luau".into(),
            b"return { on_kill=function(e,a) a.set_stat(e.unit,1,0,a.stat(e.unit,1,0)*10+1) end }"
                .to_vec(),
        ),
    ]))
    .instantiate()
    .unwrap()
    .unwrap();
    w.value = 0;
    host.event(&mut w, event(EventKind::Kill));
    assert_eq!(w.value, 12);
}

#[test]
fn timers_and_frame_callback_fanout_have_hard_limits() {
    let host = host("return { on_attack_start=function(e,a) for i=1,128 do a.timer(e.unit,1,i) end end, on_kill=function(e,a) a.set_stat(e.unit,1,0,a.stat(e.unit,1,0)+1) end }");
    let mut w = World::default();
    for _ in 0..3 {
        host.event(&mut w, event(EventKind::AttackStart));
    }
    assert_eq!(host.timers.lock().unwrap().len(), 256);
    assert!(w.errors[0].contains("timer limit"));
    for _ in 0..300 {
        host.event(&mut w, event(EventKind::Kill));
    }
    assert_eq!(w.value, 253);
    w.frame += 1;
    host.event(&mut w, event(EventKind::Kill));
    assert_eq!(w.value, 254);
}

#[test]
fn damage_events_cannot_reenter_the_same_world_borrow() {
    let host = Arc::new(host("return { on_attack_start=function(e,a) a.damage(e.unit,e.other,256,0) end, on_kill=function(e,a) a.set_stat(e.unit,1,0,999) end }"));
    let mut w = World {
        reenter: Some(host.clone()),
        ..Default::default()
    };
    host.event(&mut w, event(EventKind::AttackStart));
    assert_eq!(w.damage, 256);
    assert_eq!(w.value, 0);
    assert!(w.errors.is_empty());
    host.event(&mut w, event(EventKind::Kill));
    assert_eq!(w.value, 999);
}

#[test]
fn script_blocks_share_capabilities_and_require_integer_results() {
    use d2_sim::rules::scripts::{CallContext, CallSpec};
    let host = host("return { run=function(e,a) assert(e.event=='call' and e.skill==3 and e.level==4); a.set_stat(e.unit,1,0,e.argument); return 1 end, bad=function() return 0.5 end }");
    let mut w = World::default();
    let mut spec = CallSpec {
        module: "test.luau".into(),
        function: "run".into(),
        argument: Formula::constant(0),
    };
    let context = CallContext {
        unit: UnitId(1),
        other: Some(UnitId(2)),
        skill: 3,
        level: 4,
        argument: 17,
    };
    assert_eq!(Runtime::call(&host, &mut w, &spec, context).unwrap(), 1);
    assert_eq!(w.value, 17);
    spec.function = "bad".into();
    assert!(Runtime::call(&host, &mut w, &spec, context)
        .unwrap_err()
        .contains("integer"));
    spec.function = "missing".into();
    assert!(Runtime::call(&host, &mut w, &spec, context)
        .unwrap_err()
        .contains("test.luau::missing"));
}

#[test]
fn declared_call_targets_are_checked_before_gameplay() {
    let skill =
        "[[skill]]\nid=0\n[[skill.part]]\nkind='call_script'\nmodule='test.luau'\nfunction='run'\n";
    let rules = DataMod::from_files(
        "calls",
        &DataModFiles {
            skills: Some(("skills.toml", skill)),
            ..Default::default()
        },
    )
    .unwrap()
    .into_ruleset();
    for source in ["return {}", "return { run=12 }"] {
        let error = super::tests::source(source)
            .instantiate_for(&rules)
            .err()
            .unwrap();
        assert!(error.contains("test.luau::run"), "{error}");
    }
    assert!(
        super::tests::source("return { run=function(e,a) return 1 end }")
            .instantiate_for(&rules)
            .is_ok()
    );
    assert!(Sources::default().instantiate_for(&rules).is_err());
}

#[test]
fn shipped_script_template_loads_and_calls_without_added_table_rows() {
    let source = include_str!("../../../../templates/script-reaction/files/scripts/reaction.luau");
    let triggers = include_str!("../../../../templates/script-reaction/files/triggers.toml");
    let host = Sources(BTreeMap::from([(
        "reaction.luau".into(),
        source.as_bytes().to_vec(),
    )]));
    let rules = DataMod::from_files(
        "template",
        &DataModFiles {
            triggers: Some(("triggers.toml", triggers)),
            ..Default::default()
        },
    )
    .unwrap()
    .into_ruleset();
    let host = host.instantiate_for(&rules).unwrap().unwrap();
    let call = rules.mod_rules().unwrap().script_calls()[0].clone();
    let mut w = World::default();
    Runtime::call(
        &host,
        &mut w,
        &call,
        d2_sim::rules::scripts::CallContext {
            unit: UnitId(1),
            other: None,
            skill: 0,
            level: 5,
            argument: 5,
        },
    )
    .unwrap();
    assert_eq!(w.messages, ["Script reaction, level 5"]);
    w.frame = 25;
    host.tick(&mut w);
    assert_eq!(
        w.messages,
        ["Script reaction, level 5", "Script timer complete"]
    );
}

#[test]
fn visual_echo_template_runs_chance_timer_and_buff_end_on_the_m2_host() {
    let text = include_str!("../../../../templates/visual-echo/files/scripts/echo.graph.toml");
    let compiled = d2_modkit::graph::Graph::parse(text)
        .unwrap()
        .compile()
        .unwrap();
    let sources = Sources(BTreeMap::from([(
        "echo.graph.toml".into(),
        compiled.into_bytes(),
    )]));
    let runtime = sources.instantiate().unwrap().unwrap();
    let twin = sources.instantiate().unwrap().unwrap();
    let mut a = World::default();
    let mut b = World::default();
    // One kill per frame resets the shared M2 callback/API budgets.
    for frame in 1..=1000 {
        a.frame = frame;
        b.frame = frame;
        runtime.event(&mut a, event(EventKind::Kill));
        twin.event(&mut b, event(EventKind::Kill));
        assert_eq!(a.seed, b.seed);
        assert_eq!(a.ui_variables, b.ui_variables);
        if !a.ui_variables.is_empty() {
            break;
        }
    }
    assert!(
        !a.ui_variables.is_empty(),
        "a seeded success within 1000 kills"
    );
    assert_eq!(a.ui_variables, [("echo.graph.toml/pulse".into(), 1)]);
    assert_eq!(a.messages, ["A lucky echo answers your victory."]);
    let success_frame = a.frame;
    // Foreign cookies do not clear the HUD value.
    runtime.event(
        &mut a,
        Event {
            id: 72,
            ..event(EventKind::Timer)
        },
    );
    assert_eq!(a.ui_variables.len(), 1);
    a.frame = success_frame + 119;
    runtime.tick(&mut a);
    assert_eq!(a.ui_variables.len(), 1);
    a.frame += 1;
    runtime.tick(&mut a);
    assert_eq!(
        a.ui_variables.last(),
        Some(&("echo.graph.toml/pulse".into(), 0))
    );
    runtime.event(&mut a, event(EventKind::BuffEnd));
    assert_eq!(a.messages.last().unwrap(), "A buff has ended.");
    let messages = a.messages.len();
    for kind in [EventKind::Kill, EventKind::BuffEnd] {
        runtime.event(
            &mut a,
            Event {
                unit: UnitId(2),
                ..event(kind)
            },
        );
    }
    assert_eq!(a.messages.len(), messages);
    assert!(a.errors.is_empty(), "{:?}", a.errors);
    assert!(b.errors.is_empty(), "{:?}", b.errors);
}

#[test]
fn visual_graph_call_block_uses_m2_context_return_and_export_validation() {
    let graph = d2_modkit::graph::Graph::parse(
        r#"version=1
[[nodes]]
id='entry'
kind='call'
name='run'
next='set'
[[nodes]]
id='set'
kind='api'
name='set_stat'
next='result'
[nodes.inputs]
unit={node='entry',pin='unit'}
stat=1
layer=0
value={node='entry',pin='argument'}
[[nodes]]
id='result'
kind='return'
[nodes.inputs]
value={node='entry',pin='level'}
"#,
    )
    .unwrap();
    let sources = Sources(BTreeMap::from([(
        "combat.graph.toml".into(),
        graph.compile().unwrap().into_bytes(),
    )]));
    let skills = "[[skill]]\nid=0\n[[skill.part]]\nkind='call_script'\nmodule='combat.graph.toml'\nfunction='run'\n";
    let rules = DataMod::from_files(
        "graph-call",
        &DataModFiles {
            skills: Some(("skills.toml", skills)),
            ..Default::default()
        },
    )
    .unwrap()
    .into_ruleset();
    let runtime = sources.instantiate_for(&rules).unwrap().unwrap();
    let spec = rules.mod_rules().unwrap().script_calls()[0];
    let mut world = World::default();
    assert_eq!(
        Runtime::call(
            &runtime,
            &mut world,
            spec,
            d2_sim::rules::scripts::CallContext {
                unit: UnitId(1),
                other: None,
                skill: 0,
                level: 4,
                argument: 17
            }
        )
        .unwrap(),
        4
    );
    assert_eq!(world.value, 17);
    let missing = skills.replace("function='run'", "function='missing'");
    let rules = DataMod::from_files(
        "missing",
        &DataModFiles {
            skills: Some(("skills.toml", &missing)),
            ..Default::default()
        },
    )
    .unwrap()
    .into_ruleset();
    assert!(sources
        .instantiate_for(&rules)
        .err()
        .unwrap()
        .contains("combat.graph.toml::missing"));
    assert!(Sources::default().instantiate_for(&rules).is_err());
}
