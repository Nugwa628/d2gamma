//! A mod game's vendor records: the game builder makes them under the
//! game's rules, so an NPC class the mod gives a trade role keeps a store
//! record, while D2Gamma's own game keeps the records its NPC table says.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use d2_server::game::{Character, GameBuilder, GameData, Sim};
use d2_sim::rules::{ModRules, NpcProvider, Ruleset};
use d2_sim::world::npc::NpcRoles;

fn data() -> GameData {
    static DATA: OnceLock<GameData> = OnceLock::new();
    DATA.get_or_init(|| {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("mod-traders-{}", std::process::id()));
        test_fixtures::install::build(&dir, &test_fixtures::play::playable())
            .unwrap_or_else(|e| panic!("{e}"));
        GameData::select(Some(&dir)).expect("the playable install loads")
    })
    .clone()
}

/// Every `interact` class of the mod trades.
#[derive(Debug)]
struct EveryoneTrades;

impl NpcProvider for EveryoneTrades {
    fn roles(&self, _: u16) -> Option<NpcRoles> {
        Some(NpcRoles {
            trades: true,
            ..NpcRoles::default()
        })
    }
}

impl ModRules for EveryoneTrades {
    fn name(&self) -> &str {
        "everyone trades"
    }
    fn npcs(&self) -> Option<&dyn NpcProvider> {
        Some(self)
    }
}

/// The `trader` flag of every vendor record of a game under `rules`.
fn traders(rules: Ruleset) -> Vec<bool> {
    let sim: Sim = GameBuilder::new(data())
        .seed(7)
        .character(Character::New)
        .rules(rules)
        .build()
        .expect("the game builds");
    sim.world.state.vendors.iter().map(|r| r.trader).collect()
}

#[test]
fn the_game_builder_makes_the_vendor_records_under_the_games_rules() {
    let own = traders(Ruleset::Original);
    assert!(!own.is_empty(), "the install has an interact NPC");
    assert!(
        own.iter().all(|t| !t),
        "no class of the install is a D2Gamma trader"
    );
    let modded = traders(Ruleset::Mod(Arc::new(EveryoneTrades)));
    assert_eq!(modded.len(), own.len());
    assert!(modded.iter().all(|&t| t));
}
