//! What the players of one game have done in the mod's quests.
//!
//! Progress is kept per player in the game (the rules object of a game is its
//! own, see [`crate::rules::ModRules::for_game`]) and in the character's mod
//! save as the section [`SECTION`]. Events reach it from the event hooks (a
//! kill, a level entered, the player joining), from the NPC provider (a chat
//! opened) and from the skill events (a skill cast). The last two have no
//! event host in hand, so they are queued and taken up at the next
//! [`QuestRules::periodic`], a fifth of a second later at the most. The
//! players' items are the server's: it lends them to [`QuestRules::item_pass`]
//! on the same beat, which counts what the `collect` objectives ask for and
//! hands out the item rewards.
//!
//! The log is published to the player's mod panels as UI variables (see
//! [`variable_name`]), whenever it changes and when the player joins, so a
//! panel file can show it.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use d2_formats::d2s::{mod_sections, ModSection};

use super::{code_key, Objective, QuestDef, QuestRules, Reward, MAX_QUESTS};
use crate::rules::stash::{ItemError, ItemHost, ItemPlace, PlaceAt};
use crate::rules::EventHost;
use crate::units::UnitId;

/// The tag of the save section that holds a player's quest progress.
pub const SECTION: [u8; 4] = *b"QSTS";
/// The layout of the section.
const SECTION_VERSION: u8 = 1;
/// How often the queue is taken up and stats are looked at, in base ticks.
pub const PERIOD: u32 = 5;
/// How many panel variables the quests publish to one player. The client
/// keeps 256 names in all, shared with the scripts' own.
pub const MAX_VARIABLES: usize = 192;

/// What has been published to one player: `(quest id, slot)` to value. Slot 0
/// is the quest, slot `n + 1` its objective `n`.
type Shown = BTreeMap<(u16, u8), i32>;

/// The panel variable of a quest (`slot` 0: 0 not begun, 1 open, 2 done, which
/// a panel binds as `var.quests/q<id>`) or of its objective `slot - 1` (the
/// count so far, `var.quests/q<id>o<n>`). A quest not begun has none yet, so a
/// panel shows a dash for it.
pub fn variable_name(id: u16, slot: u8) -> String {
    match slot {
        0 => format!("quests/q{id}"),
        n => format!("quests/q{id}o{}", n - 1),
    }
}

/// A quest the player has begun.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Progress {
    done: bool,
    /// One count per objective.
    counts: Vec<u32>,
}

type Log = BTreeMap<u16, Progress>;

/// Something that happened to a player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Happened {
    /// A monster of this class died by the player.
    Killed(u16),
    Entered(u32),
    Talked(u16),
    Cast(u16),
    /// Nothing in particular: look at the stats and at what can start.
    Look,
}

/// What a player carries of the items the quests ask for, by code.
type Have = BTreeMap<[u8; 4], u32>;

/// An item reward on its way to a player's inventory.
#[derive(Debug)]
struct Gift {
    player: UnitId,
    item: [u8; 4],
    left: u32,
    /// The player was told there is no room.
    told: bool,
}

/// Items a finished quest takes from a player (its `collect` objectives that
/// set `consume`).
#[derive(Debug)]
struct Take {
    player: UnitId,
    item: [u8; 4],
    left: u32,
}

#[derive(Debug, Default)]
struct State {
    players: BTreeMap<UnitId, Log>,
    waiting: Vec<(UnitId, Happened)>,
    have: BTreeMap<UnitId, Have>,
    gifts: Vec<Gift>,
    takes: Vec<Take>,
    notices: Vec<(UnitId, String)>,
    shown: BTreeMap<UnitId, Shown>,
    item_balance: BTreeMap<[u8; 4], i64>,
}

/// The parts of the game state a round of the quest log reads and writes
/// besides the log itself.
struct Round<'a> {
    have: Option<&'a Have>,
    gifts: &'a mut Vec<Gift>,
    takes: &'a mut Vec<Take>,
}

/// The progress of the players of one game. A copy of the rules starts empty:
/// it is the state of another game.
#[derive(Debug, Default)]
pub(crate) struct Runtime {
    state: Mutex<State>,
}

impl Clone for Runtime {
    fn clone(&self) -> Self {
        Runtime::default()
    }
}

impl Runtime {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn describe(o: &Objective) -> String {
    match o {
        Objective::Kill { monster, .. } => format!("kill monster {monster}"),
        Objective::Enter { level } => format!("enter level {level}"),
        Objective::Talk { npc } => format!("talk to NPC {npc}"),
        Objective::UseSkill { skill, .. } => format!("cast skill {skill}"),
        Objective::Collect { item, .. } => format!("collect {item}"),
        Objective::ReachStat { stat, .. } => format!("reach stat {stat}"),
    }
}

impl QuestRules {
    /// `player`'s quest progress is read from the character's save
    /// `sections` (nothing in them: a new start).
    pub(crate) fn load(&self, player: UnitId, sections: &[ModSection]) {
        let log = mod_sections::find(sections, &SECTION)
            .map(|s| self.parse(&s.data))
            .unwrap_or_default();
        let mut st = self.runtime.lock();
        st.players.insert(player, log);
        st.have.remove(&player);
        // A new character: everything is sent again at the next look.
        st.shown.remove(&player);
    }

    /// The save section of `player`'s progress.
    pub(crate) fn save(&self, player: UnitId) -> Vec<ModSection> {
        let st = self.runtime.lock();
        match st.players.get(&player) {
            Some(log) if !log.is_empty() => vec![ModSection::new(SECTION, encode(log))],
            _ => Vec::new(),
        }
    }

    /// `player` left the game.
    pub(crate) fn leave(&self, player: UnitId) {
        let mut st = self.runtime.lock();
        st.players.remove(&player);
        st.have.remove(&player);
        st.shown.remove(&player);
        st.waiting.retain(|(p, _)| *p != player);
        st.gifts.retain(|g| g.player != player);
        st.takes.retain(|t| t.player != player);
        st.notices.retain(|(p, _)| *p != player);
    }

    /// Progress read back from a save section. A quest the file no longer
    /// has is dropped; the counts follow the objectives the file has now.
    fn parse(&self, data: &[u8]) -> Log {
        let mut log = Log::new();
        let mut at = 0usize;
        let mut take = |n: usize| {
            let bytes = data.get(at..at + n)?;
            at += n;
            Some(bytes)
        };
        let Some([SECTION_VERSION]) = take(1) else {
            return log;
        };
        let Some(n) = take(2).map(|b| u16::from_le_bytes([b[0], b[1]])) else {
            return log;
        };
        for _ in 0..n.min(MAX_QUESTS as u16) {
            let Some(head) = take(4) else { break };
            let id = u16::from_le_bytes([head[0], head[1]]);
            let (done, k) = (head[2] & 1 != 0, usize::from(head[3]));
            let Some(raw) = take(k * 4) else { break };
            let Some(def) = self.get(id) else { continue };
            let mut counts: Vec<u32> = (0..k)
                .map(|i| {
                    u32::from_le_bytes([raw[i * 4], raw[i * 4 + 1], raw[i * 4 + 2], raw[i * 4 + 3]])
                })
                .collect();
            counts.resize(def.objectives.len(), 0);
            for (c, o) in counts.iter_mut().zip(&def.objectives) {
                *c = (*c).min(o.target());
            }
            log.insert(id, Progress { done, counts });
        }
        log
    }

    /// Something happened to `player`: progress, finish and start quests.
    pub(crate) fn happened(&self, h: &mut dyn EventHost, player: UnitId, what: Happened) {
        let mut guard = self.runtime.lock();
        let st = &mut *guard;
        if let Some(log) = st.players.get_mut(&player) {
            let mut round = Round {
                have: st.have.get(&player),
                gifts: &mut st.gifts,
                takes: &mut st.takes,
            };
            self.step(log, &mut round, h, player, what);
            self.show(log, st.shown.entry(player).or_default(), h, player);
        }
    }

    /// Notes that something happened to `player` for the next
    /// [`QuestRules::periodic`].
    pub(crate) fn queue(&self, player: UnitId, what: Happened) {
        let mut st = self.runtime.lock();
        if st.players.contains_key(&player) {
            st.waiting.push((player, what));
        }
    }

    /// Takes up the queued events and looks at every player's stats.
    pub(crate) fn periodic(&self, h: &mut dyn EventHost) {
        let mut guard = self.runtime.lock();
        let st = &mut *guard;
        let waiting = std::mem::take(&mut st.waiting);
        for (player, what) in waiting {
            if let Some(log) = st.players.get_mut(&player) {
                let mut round = Round {
                    have: st.have.get(&player),
                    gifts: &mut st.gifts,
                    takes: &mut st.takes,
                };
                self.step(log, &mut round, h, player, what);
            }
        }
        if self.wants_stat_checks() || self.wants_items() {
            for (&player, log) in &mut st.players {
                let mut round = Round {
                    have: st.have.get(&player),
                    gifts: &mut st.gifts,
                    takes: &mut st.takes,
                };
                self.step(log, &mut round, h, player, Happened::Look);
            }
        }
        for (&player, log) in &st.players {
            self.show(log, st.shown.entry(player).or_default(), h, player);
        }
        for (player, text) in std::mem::take(&mut st.notices) {
            h.say(player, &text);
        }
    }

    /// Publishes what changed in `log` since the last time to the player's
    /// panels. Past [`MAX_VARIABLES`] names nothing new is published.
    fn show(&self, log: &Log, shown: &mut Shown, h: &mut dyn EventHost, player: UnitId) {
        let mut put = |id: u16, slot: u8, value: i32| {
            let key = (id, slot);
            if shown.get(&key) == Some(&value) {
                return;
            }
            if !shown.contains_key(&key) && shown.len() >= MAX_VARIABLES {
                return;
            }
            shown.insert(key, value);
            h.ui_variable(player, &variable_name(id, slot), value);
        };
        for d in &self.defs {
            let Some(p) = log.get(&d.id) else { continue };
            put(d.id, 0, if p.done { 2 } else { 1 });
            for (k, c) in p.counts.iter().enumerate() {
                put(d.id, k as u8 + 1, i32::try_from(*c).unwrap_or(i32::MAX));
            }
        }
    }

    /// The server's beat on the players' items: counts what the `collect`
    /// objectives ask for (read at the next [`QuestRules::periodic`]) and
    /// puts the item rewards in the inventories. A reward with no room
    /// waits for room, and the player is told once.
    pub(crate) fn item_pass(&self, items: &mut dyn ItemHost) {
        if !self.wants_items() {
            return;
        }
        let (players, mut gifts, takes) = {
            let mut st = self.runtime.lock();
            (
                st.players.keys().copied().collect::<Vec<_>>(),
                std::mem::take(&mut st.gifts),
                std::mem::take(&mut st.takes),
            )
        };
        let mut balance = BTreeMap::<[u8; 4], i64>::new();
        for t in &takes {
            *balance.entry(t.item).or_default() -= take(items, t);
        }
        let wanted = self.collect_codes();
        let mut counted: Vec<(UnitId, Have)> = Vec::new();
        if !wanted.is_empty() {
            for &player in &players {
                let mut have = Have::new();
                for it in items.items(player) {
                    if !carried(it.place) || !wanted.contains(&it.code) {
                        continue;
                    }
                    *have.entry(it.code).or_default() += quantity(items, player, it.guid);
                }
                counted.push((player, have));
            }
        }
        let mut notices = Vec::new();
        for g in &mut gifts {
            while g.left > 0 {
                match items.create(g.player, g.item, &[], PlaceAt::Inventory(None)) {
                    Ok(guid) => {
                        *balance.entry(g.item).or_default() +=
                            i64::from(quantity(items, g.player, guid));
                        g.left -= 1;
                    }
                    Err(ItemError::NoRoom) => {
                        if !g.told {
                            g.told = true;
                            notices.push((
                                g.player,
                                "Make room in your inventory for the quest reward.".to_owned(),
                            ));
                        }
                        break;
                    }
                    Err(_) => {
                        // The game has no such item or refused it: it never will.
                        let code = String::from_utf8_lossy(&g.item).trim_end().to_owned();
                        notices.push((
                            g.player,
                            format!("The quest reward `{code}` is not in this game."),
                        ));
                        g.left = 0;
                    }
                }
            }
        }
        gifts.retain(|g| g.left > 0);
        let mut st = self.runtime.lock();
        for (code, delta) in balance {
            *st.item_balance.entry(code).or_default() += delta;
        }
        for (player, have) in counted {
            if st.players.contains_key(&player) {
                st.have.insert(player, have);
            }
        }
        // Rewards given since the pass began come after the ones kept.
        gifts.append(&mut st.gifts);
        gifts.retain(|g| st.players.contains_key(&g.player));
        st.gifts = gifts;
        st.notices.extend(notices);
    }

    pub(crate) fn item_balance(&self) -> BTreeMap<[u8; 4], i64> {
        self.runtime.lock().item_balance.clone()
    }

    /// The codes the `collect` objectives ask for.
    fn collect_codes(&self) -> Vec<[u8; 4]> {
        let mut codes: Vec<[u8; 4]> = self
            .defs
            .iter()
            .flat_map(|d| &d.objectives)
            .filter_map(|o| match o {
                Objective::Collect { item, .. } => Some(code_key(item)),
                _ => None,
            })
            .collect();
        codes.sort_unstable();
        codes.dedup();
        codes
    }

    /// The lines of `player`'s quest log, for the `/quests` command.
    pub(crate) fn log_lines(&self, player: UnitId) -> Vec<String> {
        let st = self.runtime.lock();
        let Some(log) = st.players.get(&player) else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        for d in &self.defs {
            let Some(p) = log.get(&d.id) else { continue };
            if p.done {
                let text = if d.done_text.is_empty() {
                    &d.name
                } else {
                    &d.done_text
                };
                lines.push(format!("{} (done): {text}", d.name));
                continue;
            }
            lines.push(format!("{}: {}", d.name, d.text));
            for (o, c) in d.objectives.iter().zip(&p.counts) {
                lines.push(format!("  {} {c}/{}", describe(o), o.target()));
            }
        }
        lines
    }

    /// One round of the quest log for `player`: starts what can start, counts
    /// `what`, finishes what is met, and goes again while something changed
    /// (a finished quest may open the next).
    fn step(
        &self,
        log: &mut Log,
        round: &mut Round<'_>,
        h: &mut dyn EventHost,
        player: UnitId,
        what: Happened,
    ) {
        let mut counted = false;
        for _ in 0..=MAX_QUESTS {
            let mut changed = self.start(log, h, player, what);
            if !counted {
                counted = true;
                changed |= self.count(log, round.have, h, player, what);
            }
            changed |= self.finish(log, round, h, player);
            if !changed {
                break;
            }
        }
    }

    fn start(&self, log: &mut Log, h: &mut dyn EventHost, player: UnitId, what: Happened) -> bool {
        let mut any = false;
        for d in &self.defs {
            let ready = !log.contains_key(&d.id)
                && d.requires
                    .iter()
                    .all(|r| log.get(r).is_some_and(|p| p.done))
                && match (d.giver, what) {
                    (None, _) => true,
                    (Some(npc), Happened::Talked(c)) => npc == c,
                    _ => false,
                };
            if ready {
                log.insert(
                    d.id,
                    Progress {
                        done: false,
                        counts: vec![0; d.objectives.len()],
                    },
                );
                h.say(player, &format!("Quest started: {}", d.name));
                if !d.text.is_empty() {
                    h.say(player, &d.text);
                }
                any = true;
            }
        }
        any
    }

    fn count(
        &self,
        log: &mut Log,
        have: Option<&Have>,
        h: &mut dyn EventHost,
        player: UnitId,
        what: Happened,
    ) -> bool {
        let mut any = false;
        for d in &self.defs {
            let Some(p) = log.get_mut(&d.id).filter(|p| !p.done) else {
                continue;
            };
            for (o, c) in d.objectives.iter().zip(p.counts.iter_mut()) {
                let target = o.target();
                let before = *c;
                match (o, what) {
                    (Objective::Kill { monster, .. }, Happened::Killed(m)) if m == *monster => {
                        *c = (*c + 1).min(target);
                    }
                    (Objective::UseSkill { skill, .. }, Happened::Cast(s)) if s == *skill => {
                        *c = (*c + 1).min(target);
                    }
                    (Objective::Enter { level }, Happened::Entered(l)) if l == *level => *c = 1,
                    (Objective::Talk { npc }, Happened::Talked(n)) if n == *npc => *c = 1,
                    (Objective::Collect { item, .. }, _) => {
                        // Joining precedes the first inventory scan. Preserve saved
                        // progress until we know what this player actually carries.
                        if let Some(have) = have {
                            let carried = have.get(&code_key(item)).copied().unwrap_or(0);
                            *c = carried.min(target);
                        }
                    }
                    (Objective::ReachStat { stat, value }, _)
                        if *c < target && h.stat(player, *stat, 0) >= *value =>
                    {
                        *c = target;
                    }
                    _ => {}
                }
                if *c != before {
                    any = true;
                    let counted = matches!(
                        o,
                        Objective::Kill { .. }
                            | Objective::UseSkill { .. }
                            | Objective::Collect { .. }
                    );
                    if counted && *c < target {
                        h.say(player, &format!("{}: {c}/{target}", d.name));
                    }
                }
            }
        }
        any
    }

    fn finish(
        &self,
        log: &mut Log,
        round: &mut Round<'_>,
        h: &mut dyn EventHost,
        player: UnitId,
    ) -> bool {
        let mut any = false;
        for d in &self.defs {
            let met = log.get(&d.id).is_some_and(|p| {
                !p.done
                    && d.objectives
                        .iter()
                        .zip(&p.counts)
                        .all(|(o, c)| *c >= o.target())
            });
            if !met {
                continue;
            }
            if let Some(p) = log.get_mut(&d.id) {
                p.done = true;
            }
            h.say(player, &format!("Quest complete: {}", d.name));
            reward(h, round, player, d);
            any = true;
        }
        any
    }
}

/// The places whose items a `collect` objective counts and takes.
fn carried(place: ItemPlace) -> bool {
    matches!(
        place,
        ItemPlace::Inventory | ItemPlace::Belt | ItemPlace::Cursor
    )
}

/// How many pieces the item is (a stack is its quantity).
fn quantity(items: &mut dyn ItemHost, player: UnitId, guid: u32) -> u32 {
    items
        .item_stat(player, guid, crate::stats::ids::QUANTITY)
        .filter(|q| *q > 0)
        .map_or(1, |q| q as u32)
}

/// Takes `t.left` pieces of the item from what the player carries. A stack
/// larger than what is left to take is replaced by a stack of the rest. Less
/// is taken when the player no longer holds it all.
fn take(items: &mut dyn ItemHost, t: &Take) -> i64 {
    let mut left = t.left;
    let mut removed = 0;
    for it in items.items(t.player) {
        if left == 0 {
            break;
        }
        if it.code != t.item || !carried(it.place) {
            continue;
        }
        let pieces = quantity(items, t.player, it.guid);
        if !items.destroy(t.player, it.guid) {
            continue;
        }
        removed += i64::from(pieces);
        if pieces > left {
            let rest = (pieces - left) as i32;
            if let Ok(guid) = items.create(
                t.player,
                it.code,
                &[(crate::stats::ids::QUANTITY, rest)],
                PlaceAt::Inventory(None),
            ) {
                removed -= i64::from(quantity(items, t.player, guid));
            }
            left = 0;
        } else {
            left -= pieces;
        }
    }
    removed
}

fn reward(h: &mut dyn EventHost, round: &mut Round<'_>, player: UnitId, d: &QuestDef) {
    for o in &d.objectives {
        if let Objective::Collect {
            item,
            count,
            consume: true,
        } = o
        {
            round.takes.push(Take {
                player,
                item: code_key(item),
                left: *count,
            });
        }
    }
    for r in &d.rewards {
        match r {
            Reward::Stat { stat, value } => {
                let base = h.unit_query().map_or_else(
                    || h.stat(player, *stat, 0),
                    |q| q.base_stat(player, *stat, 0),
                );
                h.set_stat(player, *stat, base.saturating_add(*value));
            }
            Reward::Item { item, count } => round.gifts.push(Gift {
                player,
                item: code_key(item),
                left: *count,
                told: false,
            }),
            Reward::Message { text } => h.say(player, text),
        }
    }
}

fn encode(log: &Log) -> Vec<u8> {
    let mut out = vec![SECTION_VERSION];
    out.extend_from_slice(&(log.len() as u16).to_le_bytes());
    for (id, p) in log {
        out.extend_from_slice(&id.to_le_bytes());
        out.push(u8::from(p.done));
        out.push(p.counts.len() as u8);
        for c in &p.counts {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    out
}
