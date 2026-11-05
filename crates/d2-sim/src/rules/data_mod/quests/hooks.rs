//! The quest rules on the game's seams: the event hooks (kills, levels,
//! joining, the `/quests` line), the save hooks (the progress section) and
//! the NPC provider (a chat opened).

use super::runtime::{Happened, PERIOD};
use crate::rules::data_mod::DataMod;
use crate::rules::{EventHooks, EventHost, ItemHost, ModSection, SaveHooks};
use crate::units::{UnitId, UnitType};
use crate::world::npc::NpcProvider;
use crate::world::quests::{QuestWorld, TextList};

/// The line a player types for the quest log.
pub const COMMAND: &[u8] = b"/quests";

/// The player a kill counts for: the killer, or the owner of a summon.
fn credited(h: &dyn EventHost, killer: Option<UnitId>) -> Option<UnitId> {
    let killer = killer?;
    match h.unit_type(killer)? {
        UnitType::Player => Some(killer),
        _ => h
            .owner(killer)
            .filter(|&o| h.unit_type(o) == Some(UnitType::Player)),
    }
}

impl EventHooks for DataMod {
    fn unit_killed(&self, h: &mut dyn EventHost, killer: Option<UnitId>, victim: UnitId) {
        if h.unit_type(victim) != Some(UnitType::Monster) {
            return;
        }
        if let Some(player) = credited(h, killer) {
            let class = h.class_id(victim);
            if let Ok(class) = u16::try_from(class) {
                self.quests.happened(h, player, Happened::Killed(class));
            }
        }
    }

    fn level_entered(&self, h: &mut dyn EventHost, player: UnitId, _from: Option<u32>, to: u32) {
        self.quests.happened(h, player, Happened::Entered(to));
    }

    fn player_joined(&self, h: &mut dyn EventHost, player: UnitId) {
        self.quests.happened(h, player, Happened::Look);
    }

    fn player_left(&self, _h: &mut dyn EventHost, player: UnitId) {
        self.quests.leave(player);
    }

    fn period(&self) -> u32 {
        PERIOD
    }

    fn periodic(&self, h: &mut dyn EventHost) {
        self.quests.periodic(h);
    }

    fn item_pass(&self, items: &mut dyn ItemHost) {
        self.quests.item_pass(items);
    }

    fn item_balance(&self) -> std::collections::BTreeMap<[u8; 4], i64> {
        self.quests.item_balance()
    }

    fn chat_command(&self, h: &mut dyn EventHost, player: UnitId, text: &[u8]) -> bool {
        if text.trim_ascii() != COMMAND {
            return false;
        }
        let lines = self.quests.log_lines(player);
        if lines.is_empty() {
            h.say(player, "No quests yet.");
        }
        for line in lines {
            h.say(player, &line);
        }
        true
    }
}

impl SaveHooks for DataMod {
    fn save_sections(&self, _h: &mut dyn EventHost, player: UnitId) -> Vec<ModSection> {
        self.quests.save(player)
    }

    fn load_sections(&self, _h: &mut dyn EventHost, player: UnitId, sections: &[ModSection]) {
        self.quests.load(player, sections);
    }
}

impl NpcProvider for DataMod {
    fn chat_text(
        &self,
        _w: &mut dyn QuestWorld,
        player: UnitId,
        _npc: UnitId,
        class: u16,
        _list: &mut TextList,
    ) {
        self.quests.queue(player, Happened::Talked(class));
    }
}
