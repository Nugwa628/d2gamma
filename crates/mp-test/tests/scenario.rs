//! The scenario format: what parses, what is refused with its line, and
//! that every shipped scenario parses and uses known verbs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use mp_test::net::nat::NatMode;
use mp_test::runner::{self, Probe};
use mp_test::scenario::{parse, parse_duration, Op, Pred, Who};
use mp_test::server::Target;
use mp_test::topology::{NodeProgram, Nodes, Setup};

fn p(text: &str) -> Result<mp_test::scenario::Scenario, String> {
    parse(Path::new("t.mps"), text).map_err(|e| e.to_string())
}

#[test]
fn a_full_scenario_parses() {
    let s = p("# comment\n\
         name demo\n\
         about Shows things.\n\
         rule A  first rule text\n\
         rule B  second rule\n\
         bots 3\n\
         server players=2 grace=500\n\
         net latency=20\n\
         seed 9\n\
         all connect\n\
         b1-2 see expect players=2 b0=yes x>=1 y<=9 z!=0 within 2s rule A   # trailing\n\
         net b1 cut=on\n\
         wait 250ms\n\
         server restart\n\
         b0 connect build=x expect refused=Build rule B\n")
    .unwrap();
    assert_eq!(s.name, "demo");
    assert_eq!(s.bots, 3);
    assert_eq!(s.rules.len(), 2);
    assert_eq!(s.rules[0].text, "first rule text");
    assert_eq!(
        s.server,
        vec![
            ("players".into(), "2".into()),
            ("grace".into(), "500".into())
        ]
    );
    assert_eq!(s.net, vec![("latency".into(), "20".into())]);
    assert_eq!(s.seed, 9);
    assert_eq!(s.steps.len(), 6);
    assert_eq!(s.steps[0].who, Who::Bots(vec![0, 1, 2]));
    let see = &s.steps[1];
    assert_eq!(see.who, Who::Bots(vec![1, 2]));
    assert_eq!(see.verb, "see");
    assert_eq!(see.within, Some(Duration::from_secs(2)));
    assert_eq!(see.rule.as_deref(), Some("A"));
    assert_eq!(
        see.expect[0],
        Pred::Fact {
            key: "players".into(),
            op: Op::Eq,
            value: "2".into()
        }
    );
    assert!(matches!(&see.expect[2], Pred::Fact { op: Op::Ge, .. }));
    assert!(matches!(&see.expect[3], Pred::Fact { op: Op::Le, .. }));
    assert!(matches!(&see.expect[4], Pred::Fact { op: Op::Ne, .. }));
    assert_eq!(s.steps[2].who, Who::Net(vec![1]));
    assert_eq!(s.steps[2].args, vec!["cut=on"]);
    assert_eq!(s.steps[3].who, Who::Wait);
    assert_eq!(s.steps[3].verb, "250ms");
    assert_eq!(s.steps[4].who, Who::Server);
    assert_eq!(s.steps[5].expect, vec![Pred::Refused(Some("Build".into()))]);
    assert_eq!(s.steps[5].args, vec!["build=x"]);
}

#[test]
fn bots_get_their_own_addresses_unless_the_scenario_shares_one() {
    let own = p("rule A x\nbots 2\nall connect\n").unwrap();
    assert!(!own.shared_address, "one address per bot by default");
    let per_bot = p("rule A x\naddresses per-bot\nbots 2\nall connect\n").unwrap();
    assert!(!per_bot.shared_address);
    let shared = p("rule A x\naddresses shared\nbots 2\nall connect\n").unwrap();
    assert!(shared.shared_address);
    let bad = p("rule A x\naddresses both\nbots 2\nall connect\n");
    assert!(bad.is_err(), "addresses takes per-bot or shared");
}

#[test]
fn errors_name_their_line() {
    let cases = [
        ("bots 2\nall connect\n", "states at least one rule"),
        (
            "rule A x\nrule A y\nb0 connect\n",
            "t.mps:2: rule A is stated twice",
        ),
        ("rule A x\nbots 65\n", "t.mps:2: bots takes"),
        ("rule A x\nbots 2\nb2 connect\n", "t.mps:3: `b2`"),
        (
            "rule A x\nb0 connect rule Z\n",
            "t.mps:2: rule Z is not stated",
        ),
        (
            "rule A x\nrule B y\nb0 see expect players=1\n",
            "t.mps:3: a step with expect names its rule",
        ),
        ("rule A x\nserver explode\n", "t.mps:2: server takes"),
        ("rule A x\nserver admin\n", "t.mps:2: server takes"),
        ("rule A x\nwait soon\n", "t.mps:2: wait takes a duration"),
        ("rule A x\nb0 see expect x>=many\n", "not a number"),
        ("rule A x\nfrobnicate\n", "is not a header"),
        ("rule A x\n", "has steps"),
    ];
    for (text, want) in cases {
        let e = p(text).unwrap_err();
        assert!(e.contains(want), "{text:?}: {e}");
    }
}

#[test]
fn a_setup_step_with_plain_ok_needs_no_rule() {
    let s = p("rule A x\nrule B y\nb0 connect expect ok\n").unwrap();
    assert!(s.rule_of(&s.steps[0]).is_none(), "setup: core.step");
}

#[test]
fn a_single_rule_is_every_expecting_step_s_rule() {
    let s = p("rule ONLY the rule\nb0 connect\nb0 see expect players=1\n").unwrap();
    assert!(s.rule_of(&s.steps[0]).is_none(), "no expectation: setup");
    assert_eq!(s.rule_of(&s.steps[1]).map(|r| r.id.as_str()), Some("ONLY"));
}

#[test]
fn durations() {
    assert_eq!(parse_duration("250ms"), Some(Duration::from_millis(250)));
    assert_eq!(parse_duration("3s"), Some(Duration::from_secs(3)));
    assert_eq!(parse_duration("1.5s"), Some(Duration::from_millis(1500)));
    assert_eq!(parse_duration("3"), None);
    assert_eq!(parse_duration("-1s"), None);
}

fn shipped() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/mp-test/scenarios");
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|f| f.extension().is_some_and(|x| x == "mps"))
        .collect();
    v.sort();
    v
}

/// Every scenario in `scenarios/` parses and uses known verbs, and the
/// owner's listed scenario families are all there.
#[test]
fn every_shipped_scenario_parses() {
    let files = shipped();
    let mut names = Vec::new();
    for f in &files {
        let s = mp_test::scenario::load(f).unwrap_or_else(|e| panic!("{e}"));
        runner::validate(&s).unwrap_or_else(|e| panic!("{e}"));
        names.push(s.name);
    }
    for want in [
        "accounts-register-login",
        "accounts-rate-limit",
        "characters",
        "difficulty-rule",
        "lobby-passwords",
        "lobby-game-list",
        "join-leave",
        "move-together",
        "move-under-lag",
        "accounts-session-takeover",
        "fight-same-monster",
        "party",
        "trade",
        "items-drop-pickup",
        "waypoints-acts",
        "reconnect",
        "server-restart-persistence",
        "version-mismatch",
        "mod-mismatch",
        "net-conditions",
        "sixty-four",
    ] {
        assert!(names.iter().any(|n| n == want), "missing scenario {want}");
    }
}

/// Every market scenario (plan K1c) parses, uses known verbs, needs the
/// `market` capability, and the plan's list is there.
#[test]
fn every_market_scenario_parses() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/mp-test/scenarios/market");
    let mut names = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let f = e.unwrap().path();
        if f.extension().is_none_or(|x| x != "mps") {
            continue;
        }
        let s = mp_test::scenario::load(&f).unwrap_or_else(|e| panic!("{e}"));
        runner::validate(&s).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            runner::needs_of(&s).iter().any(|c| c == "market"),
            "{}: needs market",
            s.name
        );
        names.push(s.name);
    }
    for want in [
        "market-state-off",
        "market-post-search-buy",
        "market-buy-race",
        "market-price-changed",
        "market-facets",
        "market-offer-counter-accept",
        "market-shop-bulk",
        "market-expire-to-deliveries",
        "market-price-history",
        "market-split-mismatch",
        "market-repeat-req",
        "market-rate-limited",
        "market-restart-persistence",
    ] {
        assert!(
            names.iter().any(|n| n == want),
            "missing market scenario {want}"
        );
    }
}

#[test]
fn an_unknown_verb_is_refused() {
    let s = p("rule A x\nb0 teleport\n").unwrap();
    let e = runner::validate(&s).unwrap_err();
    assert!(e.contains("t.mps:2: unknown verb teleport"), "{e}");
}

/// The v2 headers and setup verbs (plan N1).
#[test]
fn a_v2_scenario_parses() {
    let s = p("rule A x\n\
         bots 2\n\
         setup split:2\n\
         nat n1=cone n2=blocked\n\
         hostile alter-save\n\
         node n1 expect linked=yes node n1 within 15s\n\
         cut-link n1\n\
         cut-link n1 off\n\
         kill-node n2\n\
         start-node n2\n\
         stop-node n2\n\
         restart-realm\n\
         b0 create-game g1 on-node n2 expect ok\n")
    .unwrap();
    assert_eq!(s.setup, Some(Setup::Split(2)));
    assert_eq!(
        s.nat,
        vec![
            ("n1".to_string(), NatMode::Cone),
            ("n2".to_string(), NatMode::Blocked)
        ]
    );
    assert_eq!(s.hostile.as_deref(), Some("alter-save"));
    let node = &s.steps[0];
    assert_eq!((&node.who, node.verb.as_str()), (&Who::Topo, "node"));
    assert_eq!(node.args, vec!["n1"]);
    assert_eq!(
        node.expect[1],
        Pred::Fact {
            key: "node".into(),
            op: Op::Eq,
            value: "n1".into()
        },
        "`node ID` is node=ID"
    );
    assert_eq!(s.steps[2].args, vec!["n1", "off"]);
    assert!(s.steps[1..7].iter().all(|st| st.who == Who::Topo));
    let create = &s.steps[7];
    assert_eq!(create.who, Who::Bots(vec![0]));
    assert_eq!(create.args, vec!["g1"], "on-node is not an argument");
    assert_eq!(create.node.as_deref(), Some("n2"));
    runner::validate(&s).unwrap();
    assert_eq!(
        p("rule A x\nsetup host\npending N8\n").unwrap().setup,
        Some(Setup::Host)
    );
    assert_eq!(
        p("rule A x\nsetup split\npending N4\n").unwrap().setup,
        Some(Setup::Split(1))
    );
}

#[test]
fn v2_errors_name_their_line() {
    let cases = [
        ("rule A x\nsetup ring\n", "t.mps:2:"),
        ("rule A x\nsetup split:0\n", "t.mps:2:"),
        ("rule A x\nnat n1=leaky\n", "t.mps:2:"),
        (
            "rule A x\nsetup split:2\nnat n3=cone\nwait 1s\n",
            "t.mps:3: nat n3",
        ),
        ("rule A x\nnat x1=cone\nwait 1s\n", "t.mps:2: nat x1"),
        (
            "rule A x\nbots 2\nnat b2=cone\nwait 1s\n",
            "t.mps:3: nat b2",
        ),
        (
            "rule A x\nhostile polite\n",
            "t.mps:2: hostile takes one of",
        ),
        (
            "rule A x\ncut-link n1 sideways\n",
            "t.mps:2: cut-link takes",
        ),
        ("rule A x\ncut-link\n", "t.mps:2: cut-link takes"),
        (
            "rule A x\nrestart-realm n1\n",
            "t.mps:2: restart-realm takes nothing",
        ),
        (
            "rule A x\nrestart-realm version=0.0.0.2 n1\n",
            "t.mps:2: restart-realm takes nothing, or version=V",
        ),
        ("rule A x\nstart-node\n", "t.mps:2: start-node takes"),
        (
            "rule A x\nstart-node n1 speed=2\n",
            "t.mps:2: start-node takes",
        ),
        ("rule A x\nserver upgrade\n", "t.mps:2: server takes"),
        (
            "rule A x\nserver upgrade-cancel now\n",
            "t.mps:2: server takes",
        ),
        (
            "rule A x\nkill-node\n",
            "t.mps:2: kill-node takes one game server",
        ),
        (
            "rule A x\nnode n1 n2\n",
            "t.mps:2: node takes one game server",
        ),
        ("rule A x\nb0 create-game g1 on-node\n", "t.mps:2:"),
    ];
    for (text, want) in cases {
        let e = p(text).unwrap_err();
        assert!(e.contains(want), "{text:?}: {e}");
    }
}

/// The upgrade's lines: the countdown and its cancel on the main server's
/// stdin, a game server started as another version, and the main server
/// restarted as the new one.
#[test]
fn the_upgrade_lines_parse() {
    let s = p("rule A x\nsetup split:2\n\
               server upgrade 0.0.0.2 3 2000\n\
               server upgrade-cancel\n\
               start-node n2 version=0.0.0.9 update=off\n\
               restart-realm version=0.0.0.2\n\
               b0 heard-game Restarting for an update now expect has=yes\n")
    .unwrap();
    let verbs: Vec<&str> = s.steps.iter().map(|st| st.verb.as_str()).collect();
    assert_eq!(
        verbs,
        [
            "upgrade",
            "upgrade-cancel",
            "start-node",
            "restart-realm",
            "heard-game"
        ]
    );
    assert_eq!(s.steps[0].args, ["0.0.0.2", "3", "2000"]);
    assert_eq!(s.steps[2].args, ["n2", "version=0.0.0.9", "update=off"]);
    assert_eq!(s.steps[3].args, ["version=0.0.0.2"]);
    assert_eq!(
        s.steps[4].args,
        ["Restarting", "for", "an", "update", "now"]
    );
}

/// A v2 scenario on a v1 target, on a setup too small or with a bot
/// behind a NAT is pending, with why; its own `pending` text comes first.
#[test]
fn v2_scenarios_are_pending_where_they_cannot_run() {
    let synth = PathBuf::from("mp-test-synth");
    let v1 = Target::Synth { bin: synth.clone() };
    let split = |n| Target::Topology {
        realm: Box::new(Target::Synth { bin: synth.clone() }),
        nodes: Nodes {
            program: NodeProgram::Stub {
                bin: PathBuf::from("mp-test-stub-node"),
            },
            setup: Setup::Split(n),
        },
    };
    let probe = Probe::Serving(Some(
        ["connect", "game", "nodes", "setup"]
            .map(String::from)
            .into(),
    ));
    let s = p("rule A x\nsetup split:2\nnode n1 expect linked=yes\n").unwrap();
    let why = runner::pending_on(&s, &v1, &probe).unwrap();
    assert!(why.contains("mp-test --setup split:2"), "{why}");
    let why = runner::pending_on(&s, &split(1), &probe).unwrap();
    assert!(why.contains("runs on setup split:2, not split"), "{why}");
    assert_eq!(runner::pending_on(&s, &split(2), &probe), None);
    assert_eq!(runner::pending_on(&s, &split(3), &probe), None);
    let bot = p("rule A x\nnat b0=cone\nnode n1 expect linked=yes\n").unwrap();
    let why = runner::pending_on(&bot, &split(1), &probe).unwrap();
    assert!(why.contains("nat b0"), "{why}");
    let own = p("rule A x\nsetup split:3\npending N16: not built yet\n").unwrap();
    assert_eq!(
        runner::pending_on(&own, &v1, &probe).as_deref(),
        Some("N16: not built yet")
    );
    let on_node = p("rule A x\nsetup split\nb0 connect on-node n1\n").unwrap();
    let why = runner::pending_on(&on_node, &split(1), &probe).unwrap();
    assert!(why.contains("on-node"), "{why}");
    let hosting = p("rule A x\nsetup host\nhost-quit b0\n").unwrap();
    let why = runner::pending_on(&hosting, &split(1), &probe).unwrap();
    assert!(why.contains("runs on setup host"), "{why}");
}

fn shipped_v2() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/mp-test/scenarios/v2");
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|f| f.extension().is_some_and(|x| x == "mps"))
        .collect();
    v.sort();
    v
}

/// Every v2 scenario parses, uses known verbs and asks for a setup; the
/// plan's named scenarios are all there; none of them is in the v1 set.
#[test]
fn every_shipped_v2_scenario_parses() {
    let mut names = Vec::new();
    for f in &shipped_v2() {
        let s = mp_test::scenario::load(f).unwrap_or_else(|e| panic!("{e}"));
        runner::validate(&s).unwrap_or_else(|e| panic!("{e}"));
        assert!(s.setup.is_some(), "{}: no setup header", f.display());
        names.push(s.name);
    }
    for want in [
        "node-links",
        "node-links-behind-nat",
        "cut-link-node-sees-loss",
        "node-relinks-after-realm-restart",
        "kill-node-others-unaffected",
        "host-process-lifecycle",
        "host-process-client-crash",
        "closed-game-cannot-save",
        "takeover-ends-lease",
        "wrong-character-join-45",
        "ticket-reuse-refused",
        "ticket-other-node-refused",
        "remote-game-play",
        "hostile-node-impossible-item-refused",
        "journal-arrives",
        "host-on-pc",
        "host-quits-players-to-lobby",
        "host-sleeps-players-to-lobby",
        "host-client-crash-saves-and-closes",
        "host-stale-token-refused",
        "host-behind-upnp-reachable",
        "punch-cone-to-cone",
        "punch-symmetric-fails-cleanly",
        "create-on-chosen-node",
        "create-auto-best-ping",
        "join-sees-node",
        "player-hosted-note",
        "kill-node-players-to-lobby",
        "host-pc-vanishes",
        "cut-link-node-closes-unsaved",
        "zombie-node-discarded",
        "drop-and-resume-on-node",
        "resume-after-node-death-goes-to-lobby",
        "realm-drop-during-game",
        "three-nodes-fill-and-spill",
        "audit-catches-changed-drop",
        "audit-catches-invented-intent",
        "audit-catches-altered-save",
        "host-character-audited-every-game",
        "report-queues-audit",
        "failed-audit-rolls-back",
        "duplicate-item-refused",
        "drain-no-new-games",
        "update-node-rolling",
        "restart-realm-game-plays-on",
        "realm-away-past-buffer",
        "two-builds-side-by-side",
        "two-realms-apart",
        "mods-economies-apart",
        "node-wrong-mod-refused",
        "trade-then-node-dies",
        "stash-put-take",
        "stash-across-characters",
        "stash-node-dies-mid-take",
        "stash-hostile-node-invents-item",
        "stash-runes-as-counts",
        "stash-take-to-cursor",
        "market-post-buy",
        "market-buy-race",
        "market-expire-to-deliveries",
        "market-search-by-stats",
        "market-offer-items",
        "market-counter-offer",
    ] {
        assert!(
            names.iter().any(|n| n == want),
            "missing v2 scenario {want}"
        );
    }
    let v1: Vec<String> = shipped()
        .iter()
        .map(|f| f.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().all(|n| !v1.contains(n)),
        "a v2 scenario in the v1 set"
    );
}
