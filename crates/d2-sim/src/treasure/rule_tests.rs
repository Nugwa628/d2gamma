//! Seeded drop outcomes for the curated rules, including unchanged default draws.
use super::tests::{abcd, args, game, h2h, Sink, World, OBJECT};
use super::*;
use crate::rng::Seed;
use crate::rules::global::GlobalRules;

struct RulesSink {
    inner: Sink,
    rules: rules::DropRules,
}
impl DropSink for RulesSink {
    type Spot = ();
    type Item = usize;
    fn drop_rules(&self) -> rules::DropRules {
        self.rules
    }
    fn place(&mut self, x: i32, y: i32) -> Option<()> {
        self.inner.place(x, y)
    }
    fn create(&mut self, req: DropRequest<()>) -> Option<usize> {
        self.inner.create(req)
    }
    fn gold(&self, i: usize) -> i32 {
        self.inner.gold(i)
    }
    fn set_gold(&mut self, i: usize, v: i32) {
        self.inner.set_gold(i, v);
    }
}

fn outcomes(text: &str, facts: GameFacts, by: Dropper, party: i32, mf: i32) -> (usize, Vec<u8>) {
    let rules = GlobalRules::from_toml(text).unwrap().drops;
    let mut world = World::new(vec![h2h()], abcd());
    world.itemtypes[10].rare = 1;
    world.itemtypes[10].magic = 1;
    let recipient = Recipient {
        party: Some(party),
        magic_find: mf,
        gold_find: 0,
    };
    let mut qualities = Vec::new();
    for i in 0..256 {
        let mut sink = RulesSink {
            inner: Sink::default(),
            rules,
        };
        walk(
            &world.data(),
            &facts,
            &by,
            &mut Seed::new(i, 666),
            Some(&recipient),
            &args(1, 0),
            &mut sink,
        )
        .unwrap();
        qualities.extend(sink.inner.reqs.iter().map(|r| r.quality));
    }
    (qualities.len(), qualities)
}

#[test]
fn drop_rules_players_controls_change_drop_counts() {
    let mut single = game(true, 1);
    single.game_type = 3;
    single.players_setting = 8;
    let monster = Dropper {
        kind: DropperKind::Monster {
            class: 1,
            level: 5,
            playercount: 1,
        },
        ..OBJECT
    };
    for (text, facts, by, party) in [
        ("drops.players_setting = false", single, OBJECT, 1),
        ("drops.party_bonus = false", game(true, 8), OBJECT, 8),
        ("drops.other_player_divisor = 1", game(true, 8), OBJECT, 1),
        ("drops.monster_spawn_cap = false", game(true, 8), monster, 1),
        ("drops.nodrop_percent = 0", game(true, 1), OBJECT, 1),
        ("drops.nodrop_percent = 200", game(true, 1), OBJECT, 1),
    ] {
        assert_ne!(
            outcomes(text, facts, by, party, 0).0,
            outcomes("", facts, by, party, 0).0,
            "{text}"
        );
    }
    assert_eq!(
        outcomes("drops.nodrop_percent = 0", game(true, 1), OBJECT, 1, 0).0,
        512
    );
}

#[test]
fn drop_rules_mf_changes_quality() {
    let plain = outcomes("", game(true, 1), OBJECT, 1, 100);
    for text in ["drops.magic_find_percent = 0", "drops.magic_find_cap = 0"] {
        let modified = outcomes(text, game(true, 1), OBJECT, 1, 100);
        // Quality draws can change the subsequent pick, so compare the quality sequence.
        assert_ne!(plain.1, modified.1, "{text}");
        assert_eq!(modified, outcomes("", game(true, 1), OBJECT, 1, 0));
    }
    assert_eq!(rules::DropRules::defaults().magic_find(-100), -100);
}

#[test]
fn drop_rules_defaults_keep_requests_and_rng() {
    let world = World::new(vec![h2h()], abcd());
    for players in 1..=8 {
        for i in 0..128 {
            let mut plain = Sink::default();
            let mut configured = RulesSink {
                inner: Sink::default(),
                rules: GlobalRules::default().drops,
            };
            let mut a = Seed::new(i, 666);
            let mut b = a;
            walk(
                &world.data(),
                &game(true, players),
                &OBJECT,
                &mut a,
                None,
                &args(1, 4),
                &mut plain,
            )
            .unwrap();
            walk(
                &world.data(),
                &game(true, players),
                &OBJECT,
                &mut b,
                None,
                &args(1, 4),
                &mut configured,
            )
            .unwrap();
            assert_eq!(a, b);
            assert_eq!(plain.reqs, configured.inner.reqs);
        }
    }
    let default = rules::DropRules::defaults();
    let expected = [1, 1, 2, 2, 3, 3, 4, 4];
    for (i, n) in expected.into_iter().enumerate() {
        assert_eq!(
            default.player_factor(&game(true, i as i32 + 1), &OBJECT, None),
            n
        );
    }
}

#[test]
fn drop_rules_nodrop_preserves_find_item_and_ordered_picks() {
    for ordered in [false, true] {
        let mut world = World::new(vec![h2h()], abcd());
        if ordered {
            world.tcs.tcs[1].picks = -2;
        }
        let mut args = args(1, 4);
        args.find_item = !ordered;
        for i in 0..64 {
            let mut a = Seed::new(i, 666);
            let mut b = a;
            let mut plain = Sink::default();
            let mut changed = RulesSink {
                inner: Sink::default(),
                rules: GlobalRules::from_toml("drops.nodrop_percent = 10000")
                    .unwrap()
                    .drops,
            };
            walk(
                &world.data(),
                &game(true, 1),
                &OBJECT,
                &mut a,
                None,
                &args,
                &mut plain,
            )
            .unwrap();
            walk(
                &world.data(),
                &game(true, 1),
                &OBJECT,
                &mut b,
                None,
                &args,
                &mut changed,
            )
            .unwrap();
            assert_eq!(plain.reqs, changed.inner.reqs);
            assert_eq!(a, b);
        }
    }
}
