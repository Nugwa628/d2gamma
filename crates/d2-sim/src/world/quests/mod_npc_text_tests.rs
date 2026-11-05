//! A mod game's NPC lines ([`crate::world::npc::NpcProvider::chat_text`]),
//! with a synthetic provider: Warriv greets the player by class after the
//! quest records' lines. A provider without lines leaves D2RUST's.

use std::sync::Arc;

use super::tests::{control, Fake, P1, WARRIV_U};
use super::*;
use crate::rules::{ModRules, NpcProvider, Ruleset};

/// The mod's greeting string of player class 0.
const GREETING: u16 = 1000;

#[derive(Debug)]
struct Lines;

impl NpcProvider for Lines {
    fn chat_text(
        &self,
        w: &mut dyn QuestWorld,
        player: UnitId,
        _npc: UnitId,
        class: u16,
        list: &mut TextList,
    ) {
        if class == npc::WARRIV1 {
            list.push((GREETING + u16::from(w.player_class(player)), 0));
        }
    }
}

#[derive(Debug)]
struct Bare;

impl NpcProvider for Bare {}

#[derive(Debug)]
struct Game<P>(P);

impl<P: NpcProvider + 'static> ModRules for Game<P> {
    fn name(&self) -> &str {
        "npc text test mod"
    }
    fn npcs(&self) -> Option<&dyn NpcProvider> {
        Some(&self.0)
    }
}

/// Warriv's lines to a player of `class` under `rules`.
fn lines(rules: Ruleset, class: u8) -> TextList {
    let (mut ctl, _) = control();
    let mut f = Fake::new();
    f.rules = rules;
    f.p(P1).class = class;
    let mut list = TextList::new();
    ctl.npc_activate(&mut f, P1, WARRIV_U, &mut list);
    list
}

#[test]
fn a_mod_adds_an_npc_greeting() {
    for class in [0, act1::PALADIN] {
        let own = lines(Ruleset::Original, class);
        assert!(!own.is_empty(), "Warriv's gossip");
        let m = lines(Ruleset::Mod(Arc::new(Game(Lines))), class);
        assert_eq!(m[..own.len()], own[..]);
        assert_eq!(m[own.len()..], [(GREETING + u16::from(class), 0)]);
    }
}

#[test]
fn the_engines_npc_lines_are_unchanged() {
    for class in [0, act1::PALADIN] {
        assert_eq!(
            lines(Ruleset::Mod(Arc::new(Game(Bare))), class),
            lines(Ruleset::Original, class)
        );
    }
}
