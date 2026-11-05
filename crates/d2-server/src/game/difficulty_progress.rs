//! Per-difficulty progress of a data mod. Base save fields stay three-wide.
use super::{Character, Sim};
use d2_formats::d2m::progress::{self, Progress};
use d2_sim::units::UnitId;
use d2_sim::world::{quests::QuestFlags, waypoints::WaypointRecord};

pub fn restore(sim: &mut Sim, player: UnitId, character: &Character) {
    let count = sim
        .events
        .action
        .hooks()
        .rules
        .difficulties()
        .map_or(3, |d| d.count());
    let mut records = match character {
        Character::Save(save, _) => save
            .body
            .as_ref()
            .and_then(|b| progress::read(b, save.header.towns).ok())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let acts = sim.events.action.hooks().rules.acts().cloned();
    let mut initial = Progress::default();
    if let Some(acts) = &acts {
        initial.waypoints[2] = 0;
        let home = &acts.rows()[0];
        if let Some(w) = home.waypoints.iter().find(|w| w.level == home.town) {
            let mut record = WaypointRecord(initial.waypoints);
            record.set(u32::from(w.id)).expect("validated waypoint id");
            initial.waypoints = record.0;
        }
    }
    records.resize(records.len().max(count), initial);
    let Some(q) = sim.world.rest.quests.get_mut(&player) else {
        return;
    };
    q.ensure_difficulties(records.len());
    // Preserve the base loader's normalization and validation. Extended rows
    // have the same record semantics, without the three-record boundary.
    for (d, r) in records.iter().enumerate() {
        if let Ok(flags) = QuestFlags::copy_in(&r.quests, true) {
            q.flags[d] = flags;
        }
        q.first_talk[d] = r.npc_a;
        q.set_intro_bits(d, r.npc_b);
        q.towns[d] = r.town;
    }
    let waypoints = sim
        .events
        .action
        .hooks()
        .waypoints
        .entry(player)
        .or_default();
    for (d, r) in records.iter().enumerate() {
        let record = if acts.is_some() {
            WaypointRecord::load_mod_copy(&r.waypoints)
        } else {
            WaypointRecord::load_copy(&r.waypoints)
        };
        if let Ok(record) = record {
            *waypoints.get_mut(d as u8) = record;
        }
    }
}

pub fn capture(sim: &mut Sim, player: UnitId) -> Option<Vec<Progress>> {
    let h = sim.events.action.hooks();
    if h.rules.difficulties().is_none() && h.rules.acts().is_none() {
        return None;
    }
    let count = h.rules.difficulties().map_or(3, |d| d.count());
    let waypoints = h.waypoints.get(&player).cloned().unwrap_or_default();
    let q = sim.world.rest.quests.get(&player)?;
    let mut records: Vec<_> = (0..q.flags.len().max(count))
        .map(|d| {
            Some(Progress {
                quests: q.flags.get(d).map_or([0; 96], QuestFlags::copy_out),
                npc_a: q.first_talk.get(d).copied().unwrap_or_default(),
                npc_b: q.intro_bits(d),
                waypoints: waypoints
                    .0
                    .get(d)
                    .copied()
                    .unwrap_or_default()
                    .out_copy()
                    .ok()?,
                town: q.towns.get(d).copied().unwrap_or(0),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    if let Some((difficulty, act)) = super::save_gaps::read_gaps(sim, player).town {
        if let Some(r) = records.get_mut(usize::from(difficulty)) {
            r.town = act | 0x80;
        }
    }
    Some(records)
}
