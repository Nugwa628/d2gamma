//! The game-server choice's messages: round trips, decode rules and the
//! automatic choice.

use super::*;

fn node(id: &str, load_pct: u8, state: NodeState, suggested: bool) -> NodeInfo {
    NodeInfo {
        node: id.into(),
        name: format!("D2RUST {id}"),
        location: "Helsinki, Finland".into(),
        ping_addr: "node.example:6113".into(),
        cert_sha256: [7; 32],
        load_pct,
        state,
        suggested,
    }
}

#[test]
fn node_list_round_trips_in_the_lobby_range() {
    let list = NodeList(vec![
        node("hel-1", 40, NodeState::Open, true),
        node("fra-1", 100, NodeState::Full, false),
        node("nyc-1", 0, NodeState::Down, false),
    ]);
    let body = list.body();
    assert_eq!(body[0], kind::NODE_LIST);
    assert!(super::super::kind::LOBBY.contains(&body[0]));
    assert_eq!(NodeList::parse(&body).unwrap(), list);
    assert_eq!(NodeList::parse(&NodeList::default().body()).unwrap().0, []);
}

#[test]
fn node_list_decode_rules() {
    let good = NodeList(vec![node("hel-1", 40, NodeState::Open, false)]).body();
    // Bytes left over.
    let mut long = good.clone();
    long.push(0);
    assert!(NodeList::parse(&long).is_err());
    // Cut short.
    assert!(NodeList::parse(&good[..good.len() - 1]).is_err());
    // A load over 100, an unknown state, an empty id.
    let at_end = |n: &NodeInfo, back: usize, v: u8| {
        let mut b = NodeList(vec![n.clone()]).body();
        let i = b.len() - back;
        b[i] = v;
        b
    };
    let n = node("hel-1", 40, NodeState::Open, false);
    assert!(NodeList::parse(&at_end(&n, 3, 101)).is_err());
    assert!(NodeList::parse(&at_end(&n, 2, 4)).is_err());
    assert!(NodeList::parse(&at_end(&n, 1, 2)).is_err());
    assert!(NodeList::parse(&NodeList(vec![node("", 0, NodeState::Open, false)]).body()).is_err());
    // Another kind.
    assert!(NodeList::parse(&GameStatus { game_id: 1 }.body()).is_err());
}

#[test]
fn placement_tail_round_trips() {
    for p in [
        Placement::Automatic,
        Placement::AutomaticPick("hel-1".into()),
        Placement::Official("fra-1".into()),
        Placement::HostOnMyPc,
    ] {
        let mut w = Writer::new(0x45);
        p.encode(&mut w);
        let body = w.finish();
        let (_, mut r) = Reader::new(&body).unwrap();
        assert_eq!(Placement::decode(&mut r).unwrap(), p);
        r.finish().unwrap();
    }
    // An official choice without a server, a host with one, an unknown place.
    for bad in [&[1u8, 0][..], &[2, 1, 1, b'x'], &[3, 0]] {
        let mut body = vec![0x45];
        body.extend_from_slice(bad);
        let (_, mut r) = Reader::new(&body).unwrap();
        assert!(Placement::decode(&mut r).is_err(), "{bad:?}");
    }
}

#[test]
fn game_status_round_trips() {
    let m = GameStatus { game_id: 42 };
    assert_eq!(GameStatus::parse(&m.body()).unwrap(), m);
    let mut long = m.body();
    long.push(1);
    assert!(GameStatus::parse(&long).is_err());
}

fn pings<'a>(table: &'a [(&'a str, u32)]) -> impl Fn(&str) -> Option<u32> + 'a {
    move |id| table.iter().find(|(n, _)| *n == id).map(|&(_, p)| p)
}

fn pick<'a>(nodes: &'a [NodeInfo], table: &[(&str, u32)]) -> Option<&'a str> {
    pick_auto(nodes, &pings(table)).map(|n| n.node.as_str())
}

#[test]
fn automatic_takes_the_lowest_ping() {
    let nodes = [
        node("a", 10, NodeState::Open, false),
        node("b", 10, NodeState::Open, false),
    ];
    assert_eq!(pick(&nodes, &[("a", 80), ("b", 20)]), Some("b"));
    assert_eq!(pick(&nodes, &[("a", 20), ("b", 80)]), Some("a"));
}

#[test]
fn automatic_within_15_ms_takes_the_lower_load() {
    let nodes = [
        node("near", 90, NodeState::Open, false),
        node("close", 20, NodeState::Open, false),
        node("far", 0, NodeState::Open, false),
    ];
    // 15 ms is a tie, 16 ms is not.
    assert_eq!(
        pick(&nodes, &[("near", 20), ("close", 35), ("far", 36)]),
        Some("close")
    );
    assert_eq!(
        pick(&nodes, &[("near", 20), ("close", 36), ("far", 37)]),
        Some("near")
    );
    // Equal load: the lower ping.
    let even = [
        node("x", 50, NodeState::Open, false),
        node("y", 50, NodeState::Open, false),
    ];
    assert_eq!(pick(&even, &[("x", 30), ("y", 25)]), Some("y"));
}

#[test]
fn automatic_skips_servers_that_take_no_games() {
    let nodes = [
        node("full", 0, NodeState::Full, false),
        node("drain", 0, NodeState::Draining, true),
        node("down", 0, NodeState::Down, false),
        node("open", 70, NodeState::Open, false),
    ];
    assert_eq!(
        pick(
            &nodes,
            &[("full", 5), ("drain", 5), ("down", 5), ("open", 200)]
        ),
        Some("open")
    );
}

#[test]
fn automatic_without_pings_takes_the_hint_or_leaves_it_to_the_server() {
    let nodes = [
        node("a", 0, NodeState::Open, false),
        node("b", 50, NodeState::Open, true),
    ];
    assert_eq!(pick(&nodes, &[]), Some("b"));
    // A ping only to a closed server counts as none.
    let closed_hint = [
        node("a", 0, NodeState::Down, false),
        node("b", 0, NodeState::Draining, true),
    ];
    assert_eq!(pick(&closed_hint, &[("a", 5)]), None);
    assert_eq!(pick(&[], &[]), None);
}
