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
