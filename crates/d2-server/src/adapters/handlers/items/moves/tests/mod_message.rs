//! A mod game's messages (`ModRules::message_on`) run on the world host's
//! `MessageHost`: the player's stats (read and written), its items and
//! their facts, and the unit query.

use d2_sim::rules::{MessageHost, ModOutbox, ModRules, Ruleset};

use super::*;
use crate::seams::Intents;

/// What the rules saw: the player's stat 0 (event host and unit query),
/// its items' records and the payload.
#[derive(Debug, Default)]
struct Seen {
    stat: Vec<(i32, i32)>,
    records: Vec<usize>,
    payloads: Vec<Vec<u8>>,
}

#[derive(Debug, Default)]
struct Rules(Mutex<Seen>);

impl ModRules for Rules {
    fn name(&self) -> &str {
        "test-messages"
    }
    fn message_on(
        &self,
        h: &mut dyn MessageHost,
        player: UnitId,
        payload: &[u8],
        _: &mut dyn ModOutbox,
    ) {
        let q = h.unit_query().map_or(-1, |q| q.stat(player, 0, 0));
        let v = h.stat(player, 0, 0);
        let records = h
            .items(player)
            .into_iter()
            .filter_map(|i| h.item(i))
            .map(|f| f.record);
        let mut s = self.0.lock().unwrap();
        s.stat.push((v, q));
        s.records.extend(records);
        s.payloads.push(payload.to_vec());
        h.set_stat(player, 0, v + 5);
    }
}

/// Only the old `message` (on the bare game): still called.
#[derive(Debug, Default)]
struct Bare(Mutex<Vec<Vec<u8>>>);

impl ModRules for Bare {
    fn name(&self) -> &str {
        "test-bare"
    }
    fn message(&self, _: &mut Game, _: UnitId, payload: &[u8], _: &mut dyn ModOutbox) {
        self.0.lock().unwrap().push(payload.to_vec());
    }
}

#[derive(Default)]
struct Sink;

impl MessageSink for Sink {
    fn queue(&mut self, _: ClientId, _: &[u8]) -> Result<(), crate::buffers::QueueError> {
        Ok(())
    }
}

#[test]
fn mod_messages_see_the_world() {
    let mut t = setup();
    t.picked(KEY);
    let rules = Arc::new(Rules::default());
    t.sim().rules = Ruleset::Mod(rules.clone());
    let p = t.player;
    assert!(t.sim().mod_message(0, &[1, 2], &mut Sink));
    let s = rules.0.lock().unwrap();
    assert_eq!(s.stat, [(10, 10)]);
    assert_eq!(s.records, [KEY]);
    assert_eq!(s.payloads, [vec![1, 2]]);
    drop(s);
    assert_eq!(t.stat(p, 0), 15);
}

#[test]
fn the_old_message_rule_still_runs() {
    let mut t = setup();
    let rules = Arc::new(Bare::default());
    t.sim().rules = Ruleset::Mod(rules.clone());
    assert!(t.sim().mod_message(0, &[7], &mut Sink));
    assert_eq!(*rules.0.lock().unwrap(), [vec![7]]);
    // D2RUST's rules take no mod message.
    t.sim().rules = Ruleset::Original;
    assert!(!t.sim().mod_message(0, &[8], &mut Sink));
    assert_eq!(rules.0.lock().unwrap().len(), 1);
}

#[test]
fn ui_actions_use_joined_identity_and_claim_invalid_service_messages() {
    struct Files;
    impl crate::file_source::FileSource for Files {
        fn read_file(&self, _: &str) -> Option<Result<Vec<u8>, String>> {
            Some(Ok(b"Stat\tID\r\nstrength\t99\r\n".to_vec()))
        }
        fn ui_documents(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
            Ok(vec![("ui/test.toml".into(), b"version=1\nname='hud'\n[[actions]]\nname='allocate'\nstat='strength'\ndelta=1\nmaximum=20".to_vec())])
        }
    }
    let mut t = setup();
    t.picked(KEY);
    let rules = Arc::new(Bare::default());
    t.sim().rules = Ruleset::Mod(rules.clone());
    t.sim()
        .mod_network
        .set_legacy(crate::ui_actions::UiActions::load(&Files).unwrap());
    let p = t.player;
    let payload = d2_proto::mod_ui::action("hud/allocate").unwrap();
    assert!(!t.sim().mod_message(99, &payload, &mut Sink));
    assert_eq!(t.stat(p, 0), 10);
    assert!(t.sim().mod_message(0, &payload, &mut Sink));
    assert_eq!(t.stat(p, 0), 11);
    assert!(t.sim().mod_message(0, &payload, &mut Sink));
    assert_eq!(t.stat(p, 0), 11);
    assert!(t
        .sim()
        .mod_message(0, b"LCUI\xff\x02hud/allocate", &mut Sink));
    assert!(t.sim().mod_message(
        0,
        &d2_proto::mod_ui::action("hud/unknown").unwrap(),
        &mut Sink
    ));
    let forged = d2_proto::mod_ui::visual(&d2_proto::mod_ui::VisualEvent {
        name: "hud/allocate".into(),
        source_type: 1,
        source_guid: 77,
        x: 100,
        y: 100,
        duration: 25,
        follow: true,
    })
    .unwrap();
    assert!(t.sim().mod_message(0, &forged, &mut Sink));
    assert_eq!(t.stat(p, 0), 11);
    assert!(rules.0.lock().unwrap().is_empty());
    t.sim().rules = Ruleset::Original;
    assert!(!t.sim().mod_message(0, &payload, &mut Sink));
    assert_eq!(t.stat(p, 0), 11);
}

#[derive(Default)]
struct NetworkSink(Vec<Vec<u8>>);
impl MessageSink for NetworkSink {
    fn queue(&mut self, _: ClientId, b: &[u8]) -> Result<(), crate::buffers::QueueError> {
        self.0.push(b.to_vec());
        Ok(())
    }
}
struct NetworkFiles;
impl crate::file_source::FileSource for NetworkFiles {
    fn read_file(&self, _: &str) -> Option<Result<Vec<u8>, String>> {
        Some(Ok(b"Stat\tID\r\nstrength\t99\r\n".to_vec()))
    }
    fn net_document(&self) -> Result<Option<Vec<u8>>, String> {
        Ok(Some(b"version=1\n[[values]]\nid=1\nname='strength'\nsource='stat.strength'\n[[values]]\nid=2\nname='rage'\nsource='var.combat/rage'\n[[actions]]\nid=7\nname='allocate'\nstat='strength'\ndelta=1\nmaximum=20".to_vec()))
    }
}
fn network_updates(s: &NetworkSink) -> Vec<(u16, i32)> {
    s.0.iter()
        .filter_map(|b| {
            d2_proto::envelope::payload(d2_proto::envelope::Direction::ToClient, b)
                .and_then(d2_proto::mod_ui::read_synced)
        })
        .collect()
}
#[test]
fn network_uses_joined_identity_shared_rules_and_queued_delta_values() {
    use crate::seams::Tick;
    let mut t = setup();
    t.picked(KEY);
    let rules = Arc::new(Bare::default());
    t.sim().rules = Ruleset::Mod(rules.clone());
    t.sim().mod_network = crate::mod_network::Network::load(&NetworkFiles).unwrap();
    let player = t.player;
    let definition = crate::mod_network::definition(&NetworkFiles).unwrap();
    let hash = crate::mod_network::identity(&NetworkFiles, &definition);
    assert!(t
        .sim()
        .mod_message(0, &d2_proto::mod_ui::net_identity(false, &hash), &mut Sink));
    let payload = d2_proto::mod_ui::server_action(7).unwrap();
    assert!(!t.sim().mod_message(99, &payload, &mut Sink));
    assert_eq!(t.stat(player, 0), 10);
    assert!(t.sim().mod_message(0, &payload, &mut Sink));
    assert_eq!(t.stat(player, 0), 11);
    assert!(t.sim().mod_message(0, &payload, &mut Sink));
    assert_eq!(t.stat(player, 0), 11);
    assert!(t
        .sim()
        .mod_message(0, &d2_proto::mod_ui::synced(1, 999).unwrap(), &mut Sink));
    assert_eq!(t.stat(player, 0), 11);
    assert!(rules.0.lock().unwrap().is_empty());
    struct Full;
    impl MessageSink for Full {
        fn queue(&mut self, _: ClientId, b: &[u8]) -> Result<(), crate::buffers::QueueError> {
            Err(crate::buffers::QueueError::TooLarge(b.len()))
        }
    }
    t.sim().flush_sync(&mut Full);
    let mut out = NetworkSink::default();
    t.sim().flush_sync(&mut out);
    assert_eq!(network_updates(&out), [(1, 11), (2, 0)]);
    out.0.clear();
    t.sim().flush_sync(&mut out);
    assert!(network_updates(&out).is_empty());
    let legacy = d2_proto::envelope::encode(
        d2_proto::envelope::Direction::ToClient,
        &d2_proto::mod_ui::variable("combat/rage", 42).unwrap(),
    )
    .unwrap();
    t.sim().send_or_mark(&mut out, player, legacy.clone());
    assert!(out.0.contains(&legacy));
    t.sim().flush_sync(&mut out);
    assert_eq!(network_updates(&out), [(2, 42)]);
    t.sim().mod_network.forget(player);
    t.sim()
        .mod_message(0, &d2_proto::mod_ui::net_identity(false, &hash), &mut Sink);
    out.0.clear();
    t.sim().flush_sync(&mut out);
    assert_eq!(network_updates(&out), [(1, 11), (2, 0)]);
}
#[test]
fn network_absence_preserves_original_flush_bytes() {
    use crate::seams::Tick;
    let mut a = setup();
    let mut b = setup();
    let mut first = NetworkSink::default();
    let mut second = NetworkSink::default();
    a.sim().rules = Ruleset::Original;
    b.sim().rules = Ruleset::Original;
    b.sim().mod_network = crate::mod_network::Network::load(&NetworkFiles).unwrap();
    a.sim().flush_sync(&mut first);
    b.sim().flush_sync(&mut second);
    assert_eq!(first.0, second.0);
    assert!(network_updates(&first).is_empty());
}

#[test]
fn network_handshake_checks_schema_and_mod_versions_before_actions_and_times_out() {
    use crate::file_source::FileSource;
    let mut t = setup();
    t.sim().rules = Ruleset::Mod(Arc::new(Bare::default()));
    t.sim().mod_network = crate::mod_network::Network::load(&NetworkFiles).unwrap();
    let player = t.player;
    let action = d2_proto::mod_ui::server_action(7).unwrap();
    assert!(t.sim().mod_message(0, &action, &mut Sink));
    assert_eq!(t.stat(player, 0), 10);
    let definition = crate::mod_network::definition(&NetworkFiles).unwrap();
    let wrong = d2_modkit::net::fingerprint(&definition, &[("alpha".into(), "2.0.0".into())]);
    assert!(t
        .sim()
        .mod_message(0, &d2_proto::mod_ui::net_identity(false, &wrong), &mut Sink));
    let kicks = t.sim().mod_kicks();
    assert_eq!(kicks.len(), 1);
    assert!(kicks[0].1.contains("This mod needs updating"));
    assert!(!t.sim().mod_network.ready(player));
    t.sim().mod_network.restart(player);
    assert!(t.sim().mod_kicks().is_empty());
    t.sim().game.frame += 126;
    let kicks = t.sim().mod_kicks();
    assert!(kicks[0].1.contains("handshake missing"));
    struct Versioned;
    impl FileSource for Versioned {
        fn read_file(&self, n: &str) -> Option<Result<Vec<u8>, String>> {
            NetworkFiles.read_file(n)
        }
        fn net_document(&self) -> Result<Option<Vec<u8>>, String> {
            NetworkFiles.net_document()
        }
        fn mod_versions(&self) -> Vec<(String, String)> {
            vec![("alpha".into(), "2.0.0".into())]
        }
    }
    t.sim().mod_network = crate::mod_network::Network::load(&Versioned).unwrap();
    let old = d2_modkit::net::fingerprint(&definition, &[("alpha".into(), "1.0.0".into())]);
    t.sim()
        .mod_message(0, &d2_proto::mod_ui::net_identity(false, &old), &mut Sink);
    assert!(t.sim().mod_kicks()[0].1.contains("alpha 2.0.0"));
}

#[test]
fn network_absence_preserves_original_item_frame_bytes() {
    let mut a = setup();
    let mut b = setup();
    b.sim().mod_network = crate::mod_network::Network::load(&NetworkFiles).unwrap();
    let ga = a.ground_item(KEY, 11, 11);
    let gb = b.ground_item(KEY, 11, 11);
    assert_eq!(ga, gb);
    let (result, bytes) = a.frame(&pick(ga, 0));
    let (other, actual) = b.frame(&pick(gb, 0));
    assert_eq!(result, ResultCode::Done);
    assert_eq!(other, result);
    assert!(!bytes.is_empty());
    assert_eq!(actual, bytes);
    assert!(!actual.iter().any(|b| b.first() == Some(&0xB5)));
}

#[test]
fn network_handshake_ack_retries_after_a_full_output_queue() {
    use crate::seams::Tick;
    struct Full;
    impl MessageSink for Full {
        fn queue(&mut self, _: ClientId, b: &[u8]) -> Result<(), crate::buffers::QueueError> {
            Err(crate::buffers::QueueError::TooLarge(b.len()))
        }
    }
    let mut t = setup();
    t.sim().rules = Ruleset::Mod(Arc::new(Bare::default()));
    t.sim().mod_network = crate::mod_network::Network::load(&NetworkFiles).unwrap();
    let definition = crate::mod_network::definition(&NetworkFiles).unwrap();
    let hash = crate::mod_network::identity(&NetworkFiles, &definition);
    t.sim()
        .mod_message(0, &d2_proto::mod_ui::net_identity(false, &hash), &mut Full);
    t.sim().flush_sync(&mut Full);
    let mut out = NetworkSink::default();
    t.sim().flush_sync(&mut out);
    assert!(out.0.iter().any(|b| d2_proto::envelope::payload(
        d2_proto::envelope::Direction::ToClient,
        b
    )
    .and_then(|p| d2_proto::mod_ui::read_net_identity(p, true))
        == Some(hash)));
    assert!(
        network_updates(&out).is_empty(),
        "values wait for queued ACK"
    );
    out.0.clear();
    t.sim().flush_sync(&mut out);
    assert_eq!(network_updates(&out), [(1, 10), (2, 0)]);
}
