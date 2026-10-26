//! The v2 setups (plan N1) with real processes on loopback: the stand-in
//! main server (`mp-test-synth`) and stand-in game servers
//! (`mp-test-stub-node`), each its own process, linked through the NAT
//! fabric. The fault verbs act on them, and planted failures are reported
//! with the rule they break.

use std::path::{Path, PathBuf};

use mp_test::report::Verdict;
use mp_test::runner::{self, Probe};
use mp_test::scenario::parse;
use mp_test::server::Target;
use mp_test::topology::{NodeProgram, Nodes, Setup};

fn setup(setup: Setup, node_bin: PathBuf) -> Target {
    Target::Topology {
        realm: Box::new(Target::Synth {
            bin: PathBuf::from(env!("CARGO_BIN_EXE_mp-test-synth")),
        }),
        nodes: Nodes {
            program: NodeProgram::Stub { bin: node_bin },
            setup,
        },
    }
}

fn stub() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mp-test-stub-node"))
}

fn work(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("mp-topo-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn run(name: &str, on: Setup, text: &str) -> mp_test::report::ScenarioReport {
    let s = parse(Path::new(&format!("{name}.mps")), text).unwrap();
    runner::validate(&s).unwrap();
    let t = setup(on, stub());
    let why = runner::pending_on(&s, &t, &runner::probe(&t, work(&format!("{name}-probe"))));
    assert_eq!(why, None, "{name} should run");
    runner::run(&s, &t, work(name), 1)
}

#[test]
fn a_split_setup_announces_its_game_servers() {
    match runner::probe(&setup(Setup::Split(2), stub()), work("probe")) {
        Probe::Serving(Some(caps)) => {
            for c in ["connect", "nodes", "setup"] {
                assert!(caps.contains(c), "{c}: {caps:?}");
            }
            assert!(!caps.contains("hosting"), "{caps:?}");
        }
        p => panic!("{p:?}"),
    }
    match runner::probe(&setup(Setup::Host, stub()), work("probe-host")) {
        Probe::Serving(Some(caps)) => assert!(caps.contains("hosting"), "{caps:?}"),
        p => panic!("{p:?}"),
    }
}

/// Until `d2rust-node` exists the setup is "not built yet": every v2
/// scenario is then pending, not failed.
#[test]
fn a_missing_game_server_program_is_not_built_yet() {
    let t = setup(Setup::Split(2), work("nowhere").join("d2rust-node"));
    match runner::probe(&t, work("probe-missing")) {
        Probe::NotServing(why) => assert!(why.contains("not built yet"), "{why}"),
        p => panic!("{p:?}"),
    }
}

#[test]
fn game_servers_link_and_survive_each_other() {
    let r = run(
        "kill",
        Setup::Split(2),
        "rule UP every game server links\n\
         rule OTHERS a crash takes only its own process\n\
         setup split:2\n\
         node n1 expect running=yes linked=yes node n1 starts=1 within 15s rule UP\n\
         node n2 expect running=yes linked=yes node n2 within 15s rule UP\n\
         kill-node n1\n\
         node n1 expect running=no rule OTHERS\n\
         node n2 expect linked=yes rule OTHERS\n\
         start-node n1\n\
         node n1 expect running=yes linked=yes starts=2 within 15s rule UP\n",
    );
    assert_eq!(r.verdict, Verdict::Pass, "{:#?}", r.failures);
    assert_eq!(r.steps_run, 7);
}

/// A cut link is seen as lost, and a game server links again after the
/// cut and after a main-server restart (the fabric keeps its address).
#[test]
fn a_cut_link_and_a_realm_restart_are_survived() {
    let r = run(
        "cut",
        Setup::Split(1),
        "rule LINK a game server links again\n\
         setup split\n\
         nat n1=symmetric\n\
         node n1 expect linked=yes nat=symmetric within 15s\n\
         cut-link n1\n\
         node n1 expect linked=no cut=yes running=yes within 15s\n\
         cut-link n1 off\n\
         node n1 expect linked=yes links=2 within 20s\n\
         restart-realm\n\
         node n1 expect linked=yes links=3 starts=1 within 30s\n",
    );
    assert_eq!(r.verdict, Verdict::Pass, "{:#?}", r.failures);
}

#[test]
fn a_hosting_process_follows_its_stdin_lines() {
    let r = run(
        "host",
        Setup::Host,
        "rule HOST a hosting process follows contracts 11.12\n\
         setup host\n\
         node b0 expect running=yes linked=yes host=yes within 15s\n\
         host-sleep b0\n\
         node b0 expect closed=sleep running=yes within 10s\n\
         host-quit b0\n\
         node b0 expect running=no closed=stop within 10s\n",
    );
    assert_eq!(r.verdict, Verdict::Pass, "{:#?}", r.failures);
    let r = run(
        "host-crash",
        Setup::Host,
        "rule GONE a hosting process whose client is gone exits\n\
         setup host\n\
         node b0 expect linked=yes within 15s\n\
         host-client-crash b0\n\
         node b0 expect running=no closed=client-gone within 10s\n",
    );
    assert_eq!(r.verdict, Verdict::Pass, "{:#?}", r.failures);
}

/// A planted wrong expectation on a game server fails, naming its rule.
#[test]
fn a_planted_wrong_node_fact_names_its_rule() {
    let r = run(
        "planted",
        Setup::Split(1),
        "rule UP every game server links\n\
         rule CUT a cut game server stays linked (wrong on purpose)\n\
         setup split\n\
         node n1 expect linked=yes within 15s rule UP\n\
         cut-link n1\n\
         wait 5s\n\
         node n1 expect linked=yes within 1s rule CUT\n",
    );
    assert_eq!(r.verdict, Verdict::Fail);
    let f = &r.failures[0];
    assert_eq!(f.rule.id, "CUT");
    assert_eq!(f.line, 7);
    assert!(f.actual.contains("linked=no"), "{}", f.actual);
}

/// A setup verb that can't act fails its step: a game server that is
/// already running, a name the setup doesn't have.
#[test]
fn a_setup_verb_on_the_wrong_process_is_an_error() {
    let r = run(
        "wrong",
        Setup::Split(1),
        "rule UP every game server links\n\
         setup split\n\
         node n1 expect linked=yes within 15s\n\
         start-node n1 expect error\n\
         kill-node n7 expect error\n\
         node n1 expect running=yes\n",
    );
    assert_eq!(r.verdict, Verdict::Pass, "{:#?}", r.failures);
}

/// A game server that exits by itself breaks core.node-alive.
#[cfg(unix)]
#[test]
fn an_unasked_node_exit_breaks_core_node_alive() {
    use std::os::unix::fs::PermissionsExt;
    let dir = work("alive-bin");
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("short-lived-node");
    std::fs::write(
        &bin,
        format!(
            "#!/bin/sh\necho ready program=node node=n1 listen=127.0.0.1:9 cert_sha256={} \
             realm=127.0.0.1:9 linked=1 caps=nodes\nsleep 1\nexit 3\n",
            "0".repeat(64)
        ),
    )
    .unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let s = parse(
        Path::new("alive.mps"),
        "rule UP every game server links\n\
         setup split\n\
         node n1 expect running=yes\n\
         wait 3s\n",
    )
    .unwrap();
    let r = runner::run(&s, &setup(Setup::Split(1), bin), work("alive"), 1);
    assert_eq!(r.verdict, Verdict::Fail);
    let f = &r.failures[0];
    assert_eq!(f.rule.id, "core.node-alive");
    assert!(f.step.contains("n1"), "{}", f.step);
}
