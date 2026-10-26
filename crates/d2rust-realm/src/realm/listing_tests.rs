use super::*;
use d2_quic::fingerprint_hex;

fn entry(id: &str, cert: u8) -> NodeEntry {
    NodeEntry {
        id: id.into(),
        name: format!("DEVPC {id}"),
        location: "Finland".into(),
        display_name: String::new(),
        game: "d2rust".into(),
        cert_sha256: fingerprint_hex(&[cert; 32]),
        public_addr: String::new(),
        max_games: 10,
        max_players_total: 80,
    }
}

fn load(games: u16, players: u16, draining: bool) -> Load {
    Load {
        games,
        players,
        cpu_permille: 0,
        tick_overruns: 0,
        draining,
    }
}

fn addr(port: u16) -> SocketAddr {
    SocketAddr::from(([198, 51, 100, 7], port))
}

/// n1 at 40 %, n2 at 10 %, n3 full, n4 draining.
fn four() -> Vec<NodeInfo> {
    let (e1, e2, e3, e4) = (
        entry("n1", 1),
        entry("n2", 2),
        entry("n3", 3),
        entry("n4", 4),
    );
    let (l1, l2, l3, l4) = (
        load(4, 8, false),
        load(1, 2, false),
        load(10, 20, false),
        load(0, 0, true),
    );
    listing([
        (&e1, addr(1), &l1),
        (&e2, addr(2), &l2),
        (&e3, addr(3), &l3),
        (&e4, addr(4), &l4),
    ])
}

#[test]
fn an_entry_carries_what_the_picker_shows() {
    let mut e = entry("n1", 1);
    let l = load(2, 40, false);
    let list = listing([(&e, addr(6113), &l)]);
    assert_eq!(
        list,
        vec![NodeInfo {
            node: "n1".into(),
            // The short region players see, never the machine's name.
            name: "Finland".into(),
            location: "Finland".into(),
            ping_addr: "198.51.100.7:6113".into(),
            cert_sha256: [1; 32],
            // Players 40/80 beat games 2/10.
            load_pct: 50,
            state: NodeState::Open,
            suggested: true,
        }]
    );
    // The listed public address is the one clients ping.
    e.public_addr = "203.0.113.20:6113".into();
    assert_eq!(
        listing([(&e, addr(1), &l)])[0].ping_addr,
        "203.0.113.20:6113"
    );
    // It goes out and comes back whole.
    let body = NodeList(list.clone()).body();
    assert_eq!(NodeList::parse(&body).unwrap().0, list);
}

#[test]
fn players_see_the_region_else_the_name_else_the_id() {
    let mut e = entry("n1", 1);
    assert_eq!(display_name(&e), "Finland");
    // The admin's name wins (`--node-name`).
    e.display_name = "JP South".into();
    assert_eq!(display_name(&e), "JP South");
    e.display_name = String::new();
    e.location = "  ".into();
    assert_eq!(display_name(&e), "DEVPC n1");
    e.name = String::new();
    assert_eq!(display_name(&e), "n1");
}

#[test]
fn full_and_draining_servers_are_listed_with_their_state() {
    let list = four();
    let by = |id: &str| list.iter().find(|n| n.node == id).unwrap();
    assert_eq!(
        list.iter().map(|n| n.node.as_str()).collect::<Vec<_>>(),
        ["n1", "n2", "n3", "n4"]
    );
    assert_eq!((by("n1").load_pct, by("n1").state), (40, NodeState::Open));
    assert_eq!((by("n2").load_pct, by("n2").state), (10, NodeState::Open));
    assert_eq!((by("n3").load_pct, by("n3").state), (100, NodeState::Full));
    assert_eq!(by("n4").state, NodeState::Draining);
    // The player limit makes a server full too.
    let e = entry("n5", 5);
    let l = load(1, 80, false);
    assert_eq!(listing([(&e, addr(5), &l)])[0].state, NodeState::Full);
    // Only the least loaded open server is suggested.
    let suggested: Vec<&str> = list
        .iter()
        .filter(|n| n.suggested)
        .map(|n| n.node.as_str())
        .collect();
    assert_eq!(suggested, ["n2"]);
}

#[test]
fn over_long_names_are_cut_so_the_list_still_decodes() {
    let mut e = entry("n1", 1);
    e.location = format!("Ä\t{}", "Ä".repeat(40));
    e.public_addr = format!("{}.example:6113", "a".repeat(70));
    let l = load(0, 0, false);
    let list = listing([(&e, addr(1), &l)]);
    // No tab, cut at a character: 16 two-byte letters.
    assert_eq!(list[0].name, "Ä".repeat(16));
    assert!(
        list[0].location.len() <= MAX_LOCATION,
        "{}",
        list[0].location
    );
    assert_eq!(
        list[0].ping_addr, "",
        "an address that doesn't fit isn't pinged"
    );
    assert_eq!(
        NodeList::parse(&NodeList(list.clone()).body()).unwrap().0,
        list
    );
    // A server whose fingerprint doesn't parse isn't listed.
    e.cert_sha256 = "zz".into();
    assert!(listing([(&e, addr(1), &l)]).is_empty());
}

#[test]
fn a_named_server_is_honoured_or_refused_with_its_reason() {
    let list = four();
    let pick = |p: Placement| choose(&list, &p);
    assert_eq!(pick(Placement::Official("n1".into())), Ok("n1".into()));
    assert_eq!(pick(Placement::Official("n2".into())), Ok("n2".into()));
    assert_eq!(pick(Placement::Official("n3".into())), Err(code::NODE_FULL));
    assert_eq!(
        pick(Placement::Official("n4".into())),
        Err(code::NODE_DRAINING)
    );
    // Not linked (lost, or never added).
    assert_eq!(
        pick(Placement::Official("n9".into())),
        Err(code::NODE_UNAVAILABLE)
    );
    assert_eq!(pick(Placement::HostOnMyPc), Err(code::HOSTING_OFF));
}

#[test]
fn automatic_takes_the_clients_pick_else_the_least_loaded() {
    let list = four();
    let pick = |p: Placement| choose(&list, &p);
    assert_eq!(pick(Placement::Automatic), Ok("n2".into()));
    // The client chose by ping.
    assert_eq!(pick(Placement::AutomaticPick("n1".into())), Ok("n1".into()));
    // Its pick filled up or went away meanwhile: the main server chooses.
    assert_eq!(pick(Placement::AutomaticPick("n3".into())), Ok("n2".into()));
    assert_eq!(pick(Placement::AutomaticPick("n9".into())), Ok("n2".into()));
    // Equal loads: the lower id.
    let (a, b) = (entry("b", 1), entry("a", 2));
    let l = load(1, 1, false);
    let tie = listing([(&a, addr(1), &l), (&b, addr(2), &l)]);
    assert_eq!(choose(&tie, &Placement::Automatic), Ok("a".into()));
    // No open server at all.
    let full: Vec<NodeInfo> = list.into_iter().filter(|n| !n.open()).collect();
    assert_eq!(
        choose(&full, &Placement::Automatic),
        Err(code::NODE_UNAVAILABLE)
    );
    assert_eq!(
        choose(&[], &Placement::Automatic),
        Err(code::NODE_UNAVAILABLE)
    );
}

#[test]
fn changes_go_out_at_most_every_five_seconds() {
    let t0 = Instant::now();
    let at = |s: u64| t0 + Duration::from_millis(s);
    let mut p = Pushes::default();
    let one = NodeList(four()[..1].to_vec());
    let two = NodeList(four()[..2].to_vec());
    assert!(p.due(&one, at(0)), "the first list goes out");
    assert!(!p.due(&one, at(10_000)), "no change, no push");
    assert!(p.due(&two, at(10_000)), "a change goes out");
    assert!(!p.due(&one, at(11_000)), "too soon after the last");
    assert!(!p.due(&one, at(14_999)));
    assert!(
        p.due(&one, at(15_000)),
        "five seconds on, the change goes out"
    );
    // A session that entered with another list: the next check pushes.
    p.sent_one(&one);
    assert!(!p.due(&one, at(30_000)), "it had the same list");
    p.sent_one(&two);
    assert!(p.due(&one, at(30_000)));
}
