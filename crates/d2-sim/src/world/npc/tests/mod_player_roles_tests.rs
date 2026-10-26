//! A mod game's NPC roles per player ([`NpcProvider::player_roles`]), with
//! a synthetic provider: class 527 (an interact class with no D2RUST role)
//! trades and heals only for a player of level 10 or more, read from the
//! world; every other class keeps its class roles.

use std::sync::Arc;

use super::*;
use crate::rules::{ModRules, NpcProvider, Ruleset};
use crate::world::npc::NpcRoles;

/// The mod's NPC class (an `interact` row with no D2RUST role).
const MOD_NPC: u16 = 527;
/// The player level the mod's NPC serves from.
const SERVES_FROM: u32 = 10;

#[derive(Debug)]
struct Roles;

impl NpcProvider for Roles {
    fn player_roles(&self, w: &dyn NpcWorld, class: u16, player: UnitId) -> Option<NpcRoles> {
        let serves = w.stat(player, stat::LEVEL) >= SERVES_FROM;
        (class == MOD_NPC).then_some(NpcRoles {
            trades: serves,
            heals: serves,
            ..NpcRoles::default()
        })
    }
}

#[derive(Debug)]
struct Game;

impl ModRules for Game {
    fn name(&self) -> &str {
        "npc player roles"
    }
    fn npcs(&self) -> Option<&dyn NpcProvider> {
        Some(&Roles)
    }
}

fn world(rules: Ruleset, level: u32) -> Fake {
    let mut w = Fake::new();
    w.rules = rules;
    w.set(PLAYER, stat::LEVEL, level);
    w
}

fn modded() -> Ruleset {
    Ruleset::Mod(Arc::new(Game))
}

/// Trade opens for the level-10 player only; under D2RUST's rules never.
#[test]
fn the_trade_follows_the_player() {
    for (rules, level, trades) in [
        (modded(), 9, false),
        (modded(), SERVES_FROM, true),
        (Ruleset::Original, SERVES_FROM, false),
    ] {
        let mut w = world(rules, level);
        let mut c = control(0);
        let npc = w.npc(MOD_NPC, 20);
        talk(&mut c, &mut w, npc);
        assert_eq!(c.menu_action(&mut w, PLAYER, &msg38(1, 20, 0)).unwrap(), 0);
        assert_eq!(
            w.has("trade 1020 single true gamble false"),
            trades,
            "level {level}"
        );
    }
}

/// The chat-open heal follows the player the same way.
#[test]
fn the_heal_follows_the_player() {
    for (rules, level, heals) in [
        (modded(), 9, false),
        (modded(), SERVES_FROM, true),
        (Ruleset::Original, SERVES_FROM, false),
    ] {
        let mut w = world(rules, level);
        let mut c = control(0);
        w.npc(MOD_NPC, 20);
        w.set(PLAYER, stat::LIFE, 40);
        c.interact(&mut w, PLAYER, &msg13(20)).unwrap();
        w.log.clear();
        c.chat_open(&mut w, PLAYER, &msg9(0x2F, 20));
        assert_eq!(w.has("send stat 6 100"), heals, "level {level}");
    }
}

/// A class the mod doesn't answer per player keeps its class roles.
#[test]
fn other_classes_keep_their_class_roles() {
    let w = world(modded(), SERVES_FROM);
    for c in [class::CHARSI, class::GHEED, class::KASHYA] {
        assert_eq!(NpcRoles::for_player(&w, c, PLAYER), NpcRoles::d2rust(c));
    }
    let o = world(Ruleset::Original, SERVES_FROM);
    assert_eq!(
        NpcRoles::for_player(&o, MOD_NPC, PLAYER),
        NpcRoles::default()
    );
}
