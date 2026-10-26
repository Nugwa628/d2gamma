//! The progress rule: difficulty progress, quests and waypoints go only
//! forward and in order.

use d2_formats::d2s::{status, D2s, Quests, Waypoints};

use crate::{CheckFail, SaveCheck, SaveCtx};

/// The quest slots of a difficulty's record (u16 each) the rule reads.
pub const QUEST_SLOTS: usize = 42;
/// The completion bit of a quest slot.
pub const QUEST_DONE: u16 = 1;
/// The waypoints of a difficulty's record.
pub const WAYPOINTS: u8 = 39;

const DIFFICULTY: [&str; 3] = ["Normal", "Nightmare", "Hell"];

fn quests(save: &D2s) -> [[u8; 96]; 3] {
    save.body
        .as_ref()
        .map_or_else(|| Quests::default().records, |b| b.quests.records)
}

fn waypoints(save: &D2s) -> [[u8; 16]; 3] {
    save.body
        .as_ref()
        .map_or_else(|| Waypoints::default().records, |b| b.waypoints.records)
}

fn slot(record: &[u8; 96], q: usize) -> u16 {
    u16::from_le_bytes([record[2 * q], record[2 * q + 1]])
}

fn has_waypoint(record: &[u8; 16], n: u8) -> bool {
    Waypoints::bit(n).is_some_and(|(byte, mask)| record[byte] & mask != 0)
}

/// Difficulty progress (the status word's progression) never goes back;
/// a completed quest stays completed and an open waypoint stays open; a
/// quest or a waypoint is gained only in a difficulty the stored record
/// had open, and only in the difficulty of the game the save comes from
/// when the main server knows it ([`crate::SaveInfo::difficulty`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct Progress;

impl SaveCheck for Progress {
    fn check(&self, before: &D2s, after: &D2s, ctx: &SaveCtx) -> Result<(), CheckFail> {
        let (bs, s) = (before.header.status, after.header.status);
        let (bp, ap) = (status::progression(bs), status::progression(s));
        if ap < bp {
            return Err(CheckFail::new(format!(
                "difficulty progress went back from {bp} to {ap}"
            )));
        }
        if after.body.is_none() {
            return Ok(());
        }
        let open = status::difficulties(bs);
        let (bq, aq) = (quests(before), quests(after));
        let (bw, aw) = (waypoints(before), waypoints(after));
        for d in 0..3u8 {
            let i = usize::from(d);
            let name = DIFFICULTY[i];
            let mut gained = None;
            for q in 0..QUEST_SLOTS {
                let (b, a) = (slot(&bq[i], q), slot(&aq[i], q));
                if b & QUEST_DONE != 0 && a & QUEST_DONE == 0 {
                    return Err(CheckFail::new(format!(
                        "{name} quest {q} was completed and is not any more"
                    )));
                }
                if b & QUEST_DONE == 0 && a & QUEST_DONE != 0 {
                    gained.get_or_insert(format!("quest {q}"));
                }
            }
            for n in 0..WAYPOINTS {
                let (b, a) = (has_waypoint(&bw[i], n), has_waypoint(&aw[i], n));
                if b && !a {
                    return Err(CheckFail::new(format!(
                        "{name} waypoint {n} was open and is not any more"
                    )));
                }
                if !b && a {
                    gained.get_or_insert(format!("waypoint {n}"));
                }
            }
            let Some(what) = gained else {
                continue;
            };
            if !status::difficulty_open(open, d) {
                return Err(CheckFail::new(format!(
                    "{name} {what} gained but {name} was not open (progress {bp})"
                )));
            }
            if let Some(game) = ctx.info.difficulty.filter(|&g| g != d) {
                let game = DIFFICULTY.get(usize::from(game)).copied().unwrap_or("?");
                return Err(CheckFail::new(format!(
                    "{name} {what} gained in a {game} game"
                )));
            }
        }
        Ok(())
    }
}
