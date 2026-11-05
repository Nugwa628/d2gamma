//! Synthetic graphs exercise format limits, type checking and deterministic output.
use super::*;
pub(super) fn sample() -> Graph {
    Graph::parse(
        r#"
version=1
[[nodes]]
id='event'
kind='event'
event='kill'
next='roll'
[[nodes]]
id='roll'
kind='api'
name='random'
next='choose'
[nodes.inputs]
unit={node='event',pin='unit'}
upper=100
[[nodes]]
id='chance'
kind='condition'
op='less'
[nodes.inputs]
a={node='roll',pin='value'}
b=10
[[nodes]]
id='choose'
kind='branch'
yes='repeat'
[nodes.inputs]
condition={node='chance',pin='value'}
[[nodes]]
id='repeat'
kind='loop'
count=3
body='message'
[[nodes]]
id='message'
kind='api'
name='message'
[nodes.inputs]
unit={node='event',pin='unit'}
text='A friendly echo'
"#,
    )
    .unwrap()
}
#[test]
fn graph_output_is_stable_for_node_order_and_canvas_positions() {
    let mut g = sample();
    let source = g.compile().unwrap();
    g.nodes.reverse();
    for n in &mut g.nodes {
        n.x = 123;
        n.y = -400;
    }
    assert_eq!(source, g.compile().unwrap());
    assert_eq!(g, Graph::parse(&toml::to_string(&g).unwrap()).unwrap());
    assert!(source.contains("-- node choose"));
}
#[test]
fn every_m2_event_and_api_has_a_typed_declaration() {
    for event in EVENTS {
        let mut g = sample();
        g.nodes[0].event = event.into();
        g.compile().unwrap();
    }
    for a in APIS {
        let mut g = Graph {
            version: 1,
            nodes: vec![Node {
                id: "entry".into(),
                kind: "event".into(),
                event: "kill".into(),
                next: "action".into(),
                ..Default::default()
            }],
        };
        let inputs = a
            .inputs
            .iter()
            .map(|(name, ty)| {
                (
                    name.to_string(),
                    match ty {
                        U => Input::Link {
                            node: "entry".into(),
                            pin: "unit".into(),
                        },
                        I => Input::Integer(1),
                        B => Input::Boolean(true),
                        S => Input::Text("hello".into()),
                    },
                )
            })
            .collect();
        g.nodes.push(Node {
            id: "action".into(),
            kind: "api".into(),
            name: a.name.into(),
            inputs,
            ..Default::default()
        });
        g.compile().unwrap();
    }
}
#[test]
fn errors_point_to_invalid_nodes_types_cycles_and_unassigned_outputs() {
    type Edit = Box<dyn Fn(&mut Graph)>;
    let cases: [(&str, Edit); 6] = [
        ("repeat", Box::new(|g| g.nodes[4].count = 65)),
        (
            "roll",
            Box::new(|g| {
                g.nodes[1]
                    .inputs
                    .insert("upper".into(), Input::Boolean(true));
            }),
        ),
        ("choose", Box::new(|g| g.nodes[5].next = "choose".into())),
        ("choose", Box::new(|g| g.nodes[0].next = "choose".into())),
        ("roll", Box::new(|g| g.nodes[1].name = "os.execute".into())),
        (
            "chance",
            Box::new(|g| {
                g.nodes[2].inputs.insert(
                    "a".into(),
                    Input::Link {
                        node: "chance".into(),
                        pin: "value".into(),
                    },
                );
            }),
        ),
    ];
    for (node, edit) in cases {
        let mut g = sample();
        edit(&mut g);
        let e = g.compile().unwrap_err();
        assert!(e.to_string().contains(node), "{e}");
    }
}
#[test]
fn format_and_expansion_limits_are_checked_before_compilation() {
    assert!(Graph::parse(&" ".repeat(MAX_BYTES + 1)).is_err());
    assert!(Graph::parse("version=1\nunknown=2\nnodes=[]").is_err());
    assert!(Graph::default().compile().is_err());
    let mut g = sample();
    g.version = 2;
    assert!(g.compile().is_err());
    g.version = 1;
    for i in 0..MAX_NODES {
        g.nodes.push(Node {
            id: format!("n{i}"),
            kind: "constant".into(),
            inputs: BTreeMap::from([("value".into(), Input::Integer(1))]),
            ..Default::default()
        });
    }
    assert!(g.compile().is_err());
}
