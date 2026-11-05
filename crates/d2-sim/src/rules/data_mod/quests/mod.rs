//! Quests made of blocks: a mod's `files/quests.toml`.
//!
//! A `[[quest]]` has a name, a log text, optional requirements (quests that
//! must be done first) and an optional giver (an NPC class the player talks
//! to for it to start), a list of `[[quest.objective]]` blocks and a list of
//! `[[quest.reward]]` blocks.
//!
//! ```toml
//! [[quest]]
//! id = 1
//! name = "Pest Control"
//! text = "Kill five rats in the cellar."
//! done_text = "The cellar is quiet."
//! giver = 7
//! [[quest.objective]]
//! kind = "kill"
//! monster = 12
//! count = 5
//! [[quest.reward]]
//! kind = "stat"
//! stat = 14
//! value = 500
//! ```
//!
//! The objectives are `kill`, `enter`, `talk`, `use_skill`, `collect` and
//! `reach_stat` (see [`Objective`]); the rewards are `stat`, `item` and
//! `message` (see [`Reward`]). A quest is done when all its objectives are. A player's
//! progress lives in the game ([`runtime`]) and in the character's mod
//! save, tagged [`runtime::SECTION`]. D2Gamma's own quests stay Rust code;
//! this file adds quests next to them and a game without it is unchanged.

use serde::{Deserialize, Serialize};

mod hooks;
pub(crate) mod runtime;
pub(super) use runtime::Happened;
pub use runtime::{variable_name, MAX_VARIABLES};

#[cfg(test)]
mod tests;

/// The one version of `quests.toml` this build reads.
pub const VERSION: u32 = 1;
/// Most quests in one file.
pub const MAX_QUESTS: usize = 128;
/// Most objectives, and most rewards, in one quest.
pub const MAX_PARTS: usize = 8;
/// The largest count an objective may ask for.
pub const MAX_COUNT: u32 = 10_000;
/// The longest name, in characters.
pub const MAX_NAME: usize = 60;
/// The longest log text or message, in characters.
pub const MAX_TEXT: usize = 240;
/// The most items one reward gives.
pub const MAX_GIVE: u32 = 20;

/// An item code as the item tables key it: 1 to 4 printable characters.
fn item_code_ok(s: &str) -> bool {
    (1..=4).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_graphic())
}

/// The 4-byte, space-padded form of a checked item code.
pub(crate) fn code_key(s: &str) -> [u8; 4] {
    let mut key = [b' '; 4];
    for (k, b) in key.iter_mut().zip(s.bytes()) {
        *k = b;
    }
    key
}

/// A `quests.toml` that cannot be used. The message names the quest and the
/// key.
#[derive(Debug, thiserror::Error)]
pub enum QuestsError {
    #[error("invalid quests.toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid quests.toml setting: {0}")]
    Setting(String),
}

fn bad(
    quest: usize,
    part: Option<(&str, usize)>,
    key: &str,
    what: impl std::fmt::Display,
) -> QuestsError {
    let at = match part {
        Some((kind, j)) => format!("quest[{quest}].{kind}[{j}].{key}"),
        None => format!("quest[{quest}].{key}"),
    };
    QuestsError::Setting(format!("`{at}`: {what}"))
}

/// What a quest asks of the player. Progress counts up to the objective's
/// number; the objective is met when it gets there.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Objective {
    /// Kill `count` monsters of monstats row `monster` (a kill by the
    /// player's summon counts).
    Kill {
        monster: u16,
        #[serde(default = "one")]
        count: u32,
    },
    /// Enter level `level` (a `levels` row).
    Enter { level: u32 },
    /// Talk to an NPC of monstats row `npc`: open its chat.
    Talk { npc: u16 },
    /// Cast skill `skill` `count` times.
    UseSkill {
        skill: u16,
        #[serde(default = "one")]
        count: u32,
    },
    /// Carry `count` items with code `item` (an `items` code such as a
    /// key or a potion; the pieces of a stack count by quantity). The
    /// inventory, belt and cursor count; worn items, the stash and the cube
    /// do not. With `consume` the quest takes the items when it is
    /// completed. Once met it stays met.
    Collect {
        item: String,
        #[serde(default = "one")]
        count: u32,
        #[serde(default, skip_serializing_if = "is_false")]
        consume: bool,
    },
    /// Have `value` of stat `stat` (an `itemstatcost` row) or more: gold,
    /// or a stat of the mod's own that its items or skills raise. Once met it
    /// stays met.
    ReachStat { stat: u16, value: i32 },
}

fn one() -> u32 {
    1
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Objective {
    /// What progress counts up to.
    pub fn target(&self) -> u32 {
        match self {
            Objective::Kill { count, .. }
            | Objective::UseSkill { count, .. }
            | Objective::Collect { count, .. } => *count,
            Objective::Enter { .. } | Objective::Talk { .. } => 1,
            Objective::ReachStat { value, .. } => (*value).max(1) as u32,
        }
    }
}

/// What a finished quest gives.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reward {
    /// Adds `value` (it may be negative) to the player's base value of
    /// stat `stat`: gold, stat points, skill points or a stat of the mod's
    /// own.
    Stat { stat: u16, value: i32 },
    /// Puts `count` items with code `item` in the player's inventory; with no
    /// room they wait until there is.
    Item {
        item: String,
        #[serde(default = "one")]
        count: u32,
    },
    /// Tells the player `text`.
    Message { text: String },
}

/// One quest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestDef {
    pub id: u16,
    pub name: String,
    /// The log text while the quest is open.
    pub text: String,
    /// The log text once it is done (the name when empty).
    pub done_text: String,
    /// Quests that must be done before this one starts.
    pub requires: Vec<u16>,
    /// The NPC class whose chat starts the quest; without one it starts as
    /// soon as its requirements are done.
    pub giver: Option<u16>,
    pub objectives: Vec<Objective>,
    pub rewards: Vec<Reward>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawQuest {
    id: u16,
    name: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    done_text: String,
    #[serde(default)]
    requires: Vec<u16>,
    #[serde(default)]
    giver: Option<u16>,
    #[serde(default, rename = "objective")]
    objectives: Vec<Objective>,
    #[serde(default, rename = "reward")]
    rewards: Vec<Reward>,
}

fn text_ok(s: &str, max: usize) -> Result<(), String> {
    if s.chars().count() > max {
        return Err(format!("is longer than {max} characters"));
    }
    if s.chars().any(char::is_control) {
        return Err("has a control character".into());
    }
    Ok(())
}

/// A mod's quests.
#[derive(Clone, Debug, Default)]
pub struct QuestRules {
    defs: Vec<QuestDef>,
    runtime: runtime::Runtime,
}

// The progress of the players of one game is not part of what the rules are.
impl PartialEq for QuestRules {
    fn eq(&self, other: &Self) -> bool {
        self.defs == other.defs
    }
}
impl Eq for QuestRules {}

impl QuestRules {
    /// No quests: D2Gamma's.
    pub fn none() -> QuestRules {
        QuestRules::default()
    }

    /// Reads a `quests.toml`.
    pub fn from_toml(text: &str) -> Result<QuestRules, QuestsError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct File {
            #[serde(default = "version")]
            version: u32,
            #[serde(default, rename = "quest")]
            quests: Vec<RawQuest>,
        }
        fn version() -> u32 {
            VERSION
        }
        let file: File = toml::from_str(text)?;
        if file.version != VERSION {
            return Err(QuestsError::Setting(format!(
                "`version`: {} is not supported (this build reads version {VERSION})",
                file.version
            )));
        }
        if file.quests.len() > MAX_QUESTS {
            return Err(QuestsError::Setting(format!(
                "{} quests; a file has at most {MAX_QUESTS}",
                file.quests.len()
            )));
        }
        let mut defs: Vec<QuestDef> = Vec::new();
        for (i, raw) in file.quests.into_iter().enumerate() {
            let def = QuestDef {
                id: raw.id,
                name: raw.name,
                text: raw.text,
                done_text: raw.done_text,
                requires: raw.requires,
                giver: raw.giver,
                objectives: raw.objectives,
                rewards: raw.rewards,
            };
            check(i, &def)?;
            if defs.iter().any(|o| o.id == def.id) {
                return Err(bad(
                    i,
                    None,
                    "id",
                    format!("quest {} is defined twice", def.id),
                ));
            }
            defs.push(def);
        }
        check_requires(&defs)?;
        Ok(QuestRules {
            defs,
            runtime: runtime::Runtime::default(),
        })
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn defs(&self) -> &[QuestDef] {
        &self.defs
    }

    /// The quest `id`.
    pub fn get(&self, id: u16) -> Option<&QuestDef> {
        self.defs.iter().find(|d| d.id == id)
    }

    /// Some quest has a `use_skill` objective: the rules want skill events.
    pub fn wants_skill_events(&self) -> bool {
        self.defs.iter().any(|d| {
            d.objectives
                .iter()
                .any(|o| matches!(o, Objective::UseSkill { .. }))
        })
    }

    /// Some quest has a `collect` objective or an item reward: the server
    /// runs [`QuestRules::item_pass`] on the players' items.
    pub fn wants_items(&self) -> bool {
        self.defs.iter().any(|d| {
            d.objectives
                .iter()
                .any(|o| matches!(o, Objective::Collect { .. }))
                || d.rewards.iter().any(|r| matches!(r, Reward::Item { .. }))
        })
    }

    /// Some quest has a `reach_stat` objective: the rules look at stats now
    /// and then.
    pub fn wants_stat_checks(&self) -> bool {
        self.defs.iter().any(|d| {
            d.objectives
                .iter()
                .any(|o| matches!(o, Objective::ReachStat { .. }))
        })
    }
}

fn check(i: usize, d: &QuestDef) -> Result<(), QuestsError> {
    if d.id == 0 {
        return Err(bad(i, None, "id", "0 is not a quest id (they start at 1)"));
    }
    if d.name.trim().is_empty() {
        return Err(bad(i, None, "name", "is empty"));
    }
    for (key, value, max) in [
        ("name", &d.name, MAX_NAME),
        ("text", &d.text, MAX_TEXT),
        ("done_text", &d.done_text, MAX_TEXT),
    ] {
        text_ok(value, max).map_err(|e| bad(i, None, key, e))?;
    }
    if d.objectives.is_empty() {
        return Err(bad(
            i,
            None,
            "objective",
            "a quest needs at least one objective",
        ));
    }
    if d.objectives.len() > MAX_PARTS {
        return Err(bad(
            i,
            None,
            "objective",
            format!(
                "{} objectives; a quest has at most {MAX_PARTS}",
                d.objectives.len()
            ),
        ));
    }
    if d.rewards.len() > MAX_PARTS {
        return Err(bad(
            i,
            None,
            "reward",
            format!(
                "{} rewards; a quest has at most {MAX_PARTS}",
                d.rewards.len()
            ),
        ));
    }
    if d.requires.len() > MAX_PARTS {
        return Err(bad(
            i,
            None,
            "requires",
            format!("more than {MAX_PARTS} quests"),
        ));
    }
    for (j, o) in d.objectives.iter().enumerate() {
        let count = match o {
            Objective::Kill { count, .. }
            | Objective::UseSkill { count, .. }
            | Objective::Collect { count, .. } => Some(*count),
            _ => None,
        };
        if let Some(c) = count.filter(|c| !(1..=MAX_COUNT).contains(c)) {
            return Err(bad(
                i,
                Some(("objective", j)),
                "count",
                format!("{c} is not in 1..={MAX_COUNT}"),
            ));
        }
        if let Objective::Collect { item, .. } = o {
            if !item_code_ok(item) {
                return Err(bad(
                    i,
                    Some(("objective", j)),
                    "item",
                    format!("`{item}` is not an item code (1 to 4 characters)"),
                ));
            }
        }
        if let Objective::ReachStat { value, .. } = *o {
            if value < 1 {
                return Err(bad(
                    i,
                    Some(("objective", j)),
                    "value",
                    format!("{value} is not above 0"),
                ));
            }
        }
    }
    for (j, r) in d.rewards.iter().enumerate() {
        match r {
            Reward::Stat { value: 0, .. } => {
                return Err(bad(i, Some(("reward", j)), "value", "0 gives nothing"));
            }
            Reward::Message { text } => {
                text_ok(text, MAX_TEXT).map_err(|e| bad(i, Some(("reward", j)), "text", e))?;
            }
            Reward::Item { item, count } => {
                if !item_code_ok(item) {
                    return Err(bad(
                        i,
                        Some(("reward", j)),
                        "item",
                        format!("`{item}` is not an item code (1 to 4 characters)"),
                    ));
                }
                if !(1..=MAX_GIVE).contains(count) {
                    return Err(bad(
                        i,
                        Some(("reward", j)),
                        "count",
                        format!("{count} is not in 1..={MAX_GIVE}"),
                    ));
                }
            }
            Reward::Stat { .. } => {}
        }
    }
    Ok(())
}

/// Every required quest exists, none requires itself, and the requirements
/// have no loop.
fn check_requires(defs: &[QuestDef]) -> Result<(), QuestsError> {
    for (i, d) in defs.iter().enumerate() {
        for &r in &d.requires {
            if r == d.id {
                return Err(bad(i, None, "requires", "a quest cannot require itself"));
            }
            if !defs.iter().any(|o| o.id == r) {
                return Err(bad(
                    i,
                    None,
                    "requires",
                    format!("quest {r} is not defined"),
                ));
            }
        }
    }
    for (i, d) in defs.iter().enumerate() {
        // Follow the requirements from `d`: a way back to `d` is a loop. The
        // walk is bounded by the number of quests.
        let mut open: Vec<u16> = d.requires.clone();
        let mut seen: Vec<u16> = Vec::new();
        while let Some(id) = open.pop() {
            if id == d.id {
                return Err(bad(
                    i,
                    None,
                    "requires",
                    "the requirements loop back to this quest",
                ));
            }
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            if let Some(o) = defs.iter().find(|o| o.id == id) {
                open.extend(o.requires.iter().copied());
            }
        }
    }
    Ok(())
}
