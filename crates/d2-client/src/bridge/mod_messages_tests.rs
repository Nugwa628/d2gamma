//! Mod messages through the bridge: a mod game's wire sends C→S envelopes
//! and hands S→C envelopes to its hook; D2RUST's own wire refuses both.

use std::sync::{Arc, Mutex};

use d2_proto::envelope::{Wire, CLIENT_ID, SERVER_ID};
use d2_proto::PROTOCOL_VERSION;

use super::link::{LinkError, Pumped, SendQueue, Sent, ServerLink};
use super::world::ClientWorld;
use super::{Bridge, ModMessages};

/// The sends a [`Link`] saw: queue and bytes.
type Sends = Arc<Mutex<Vec<(SendQueue, Vec<u8>)>>>;

#[test]
fn mod_ui_values_reach_the_world_without_installing_a_hook_and_are_bounded() {
    let mut bridge = Bridge::new(Link::default()).unwrap();
    bridge.set_wire(Wire::Mod);
    for i in 0..=d2_proto::mod_ui::MAX_VARIABLES {
        let p = d2_proto::mod_ui::variable(&format!("test/v{i}"), i as i32).unwrap();
        let msg = d2_proto::envelope::encode(d2_proto::envelope::Direction::ToClient, &p).unwrap();
        bridge.receive_chunk(&msg).unwrap();
    }
    assert_eq!(
        bridge.world().ui_variables.len(),
        d2_proto::mod_ui::MAX_VARIABLES
    );
    let p = d2_proto::mod_ui::variable("test/v0", 99).unwrap();
    bridge
        .receive_chunk(
            &d2_proto::envelope::encode(d2_proto::envelope::Direction::ToClient, &p).unwrap(),
        )
        .unwrap();
    assert_eq!(bridge.world().ui_variables["test/v0"], 99);
}

/// A link that records sends and delivers one chunk list per pump.
#[derive(Clone, Default)]
struct Link {
    sent: Sends,
    chunks: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl ServerLink for Link {
    fn protocol_version(&self) -> u32 {
        PROTOCOL_VERSION
    }
    fn tick_rate(&self) -> d2_sim::tick::TickRate {
        d2_sim::tick::TickRate::BASE
    }
    fn send(&mut self, queue: SendQueue, msg: &[u8]) -> Result<Sent, LinkError> {
        self.sent.lock().unwrap().push((queue, msg.to_vec()));
        Ok(Sent::Queued)
    }
    fn pump(&mut self) -> Result<Pumped, LinkError> {
        Ok(Pumped { ticked: false })
    }
    fn receive(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut *self.chunks.lock().unwrap())
    }
}

/// A mod hook that keeps the payloads it is handed.
struct Seen(Arc<Mutex<Vec<Vec<u8>>>>);

impl ModMessages for Seen {
    fn message(&mut self, _world: &mut ClientWorld, payload: &[u8]) {
        self.0.lock().unwrap().push(payload.to_vec());
    }
}

#[test]
fn a_mod_hook_sends_and_receives_envelopes() {
    let link = Link::default();
    let mut b = Bridge::new(link.clone()).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    b.set_mod_messages(Box::new(Seen(seen.clone())));
    assert_eq!(b.wire(), Wire::Mod);

    b.send_mod(&[1, 2, 3]).unwrap();
    assert_eq!(
        *link.sent.lock().unwrap(),
        [(SendQueue::Game, vec![CLIENT_ID, 3, 0, 1, 2, 3])]
    );

    // A chunk with a D2RUST message between two envelopes: the envelopes'
    // payloads reach the hook in order, the D2RUST message its handler.
    let chunk = [SERVER_ID, 1, 0, 9, 0x1A, 0x07, SERVER_ID, 0, 0];
    let report = b.receive_chunk(&chunk).unwrap();
    assert_eq!(report.messages, 3);
    assert_eq!(*seen.lock().unwrap(), [vec![9], vec![]]);

    // The frame loop hands them over the same way.
    link.chunks
        .lock()
        .unwrap()
        .push(vec![SERVER_ID, 2, 0, 4, 5]);
    b.frame().unwrap();
    assert_eq!(seen.lock().unwrap().last(), Some(&vec![4, 5]));
}

#[test]
fn d2rusts_own_wire_refuses_envelopes_both_ways() {
    let link = Link::default();
    let mut b = Bridge::new(link.clone()).unwrap();
    assert_eq!(b.wire(), Wire::Original);
    assert!(b.send_mod(&[1]).is_err());
    assert!(link.sent.lock().unwrap().is_empty());
    // S→C: the size rule refuses the id, so the split discards the
    // envelope's bytes and nothing in it is dispatched.
    let report = b.receive_chunk(&[SERVER_ID, 1, 0, 9]).unwrap();
    assert_eq!(report.messages, 0);
    assert_eq!(report.discarded_bytes, 4);
}

#[test]
fn mod_visual_events_are_bounded_coalesced_and_reset_on_resume() {
    let mut b = Bridge::new(Link::default()).unwrap();
    b.set_wire(Wire::Mod);
    let mut event = d2_proto::mod_ui::VisualEvent {
        name: "ember".into(),
        source_type: 1,
        source_guid: 7,
        x: 100,
        y: 100,
        duration: 25,
        follow: true,
    };
    let send = |b: &mut Bridge<Link>, e: &d2_proto::mod_ui::VisualEvent| {
        b.receive_chunk(
            &d2_proto::envelope::encode(
                d2_proto::envelope::Direction::ToClient,
                &d2_proto::mod_ui::visual(e).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    };
    for guid in 0..300 {
        event.source_guid = guid;
        send(&mut b, &event);
    }
    assert_eq!(b.visual_events.len(), 256);
    event.source_guid = 0;
    event.duration = 0;
    send(&mut b, &event);
    let events = b.take_visual_events();
    assert_eq!(events.len(), 256);
    assert_eq!(events.last().unwrap().duration, 0);
    assert!(b.take_visual_events().is_empty());
    send(&mut b, &event);
    let epoch = b.visual_epoch();
    b.clear_after_resume();
    assert!(b.take_visual_events().is_empty());
    assert_ne!(b.visual_epoch(), epoch);
    let mut original = Bridge::new(Link::default()).unwrap();
    send(&mut original, &event);
    assert!(original.take_visual_events().is_empty());
}

#[test]
fn wide_mod_totals_reach_the_model_without_counting_equipment_twice() {
    use super::world::{ClientUnit, UnitKey};
    use d2_proto::wide_stat::{encode, WideStat};
    let key = UnitKey {
        unit_type: 0,
        guid: 9,
    };
    let mut b = Bridge::new(Link::default()).unwrap();
    b.set_wire(Wire::Mod);
    let mut unit = ClientUnit::new(key);
    unit.stats.insert(40000, 10);
    unit.state_lists
        .insert(30, [((40000, 0), 7)].into_iter().collect());
    b.world.units.insert(key, unit);
    let msg = |value| {
        encode(WideStat {
            unit_type: 0,
            guid: 9,
            stat: 40000,
            value,
        })
    };
    b.receive_chunk(&msg(42)).unwrap();
    assert_eq!(b.world.total(key, 40000, 0), 42);
    b.receive_chunk(&msg(0)).unwrap();
    assert_eq!(b.world.total(key, 40000, 0), 0);
    let mut base = Bridge::new(Link::default()).unwrap();
    assert_eq!(base.receive_chunk(&msg(1)).unwrap().discarded_bytes, 13);
}

#[test]
fn network_version_refusal_stops_the_bridge_with_upgrade_reason() {
    let mut b = Bridge::new(Link::default()).unwrap();
    b.set_wire(Wire::Mod);
    let reason = "This mod needs updating: server alpha 2.0.0";
    let bytes = d2_proto::envelope::encode(
        d2_proto::envelope::Direction::ToClient,
        &d2_proto::mod_ui::net_rejection(reason).unwrap(),
    )
    .unwrap();
    let error = b.receive_chunk(&bytes).unwrap_err();
    assert!(matches!(error, super::BridgeError::ModVersion(_)));
    assert_eq!(error.to_string(), reason);
}
