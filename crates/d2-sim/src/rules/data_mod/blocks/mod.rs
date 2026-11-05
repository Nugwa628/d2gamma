//! The block vocabulary: the one list of every named building block a mod
//! file may use, with a description and the version it came in.
//!
//! Skills, states, triggers and quests (and later monster AI) are all built
//! from the same kinds of named things, grouped in **families**:
//!
//! | Family | What it names | Written as |
//! | --- | --- | --- |
//! | [`Family::Event`] | what a trigger reacts to | `on = "attack"` |
//! | [`Family::Condition`] | a test that must hold | `kind = "has_state"` in a `condition` |
//! | [`Family::Action`] | something a trigger does | `kind = "apply_state"` in an `action` |
//! | [`Family::SkillPart`] | one step of a skill | `kind = "area_hit"` in a skill `part` |
//! | [`Family::Target`] | whom or where a block works on | `affects = "enemies"`, `who = "other"` |
//! | [`Family::Value`] | a number a formula can read | `lvl`, `par1`, `ln12` |
//! | [`Family::Objective`] | what a quest asks of the player | `kind = "kill"` in a quest `objective` |
//! | [`Family::Reward`] | what a finished quest gives | `kind = "stat"` in a quest `reward` |
//!
//! The registry is data about the vocabulary, not the code that runs it: each
//! family has one file here (`events.rs`, `conditions.rs`, `actions.rs`,
//! `skill_parts.rs`, `targets.rs`, `values.rs`, `objectives.rs`, `rewards.rs`) holding one table of
//! [`Block`]s. Mod Studio lists blocks and their descriptions from these
//! tables and `check-mod` validates the names a mod uses against them.
//!
//! # Names are versioned
//!
//! A block's `name` is part of the modding API. A name is never reused for a
//! different meaning and is not removed without warning: a block that is to go
//! is marked [`Deprecated`] (with what to use instead) and keeps working for
//! the API support window (12 months, see `d2_modkit::api`). [`Block::since`]
//! is the API version that introduced the block, so a mod made for an older
//! API that uses a newer block is told so.
//!
//! # Adding a block (a small, local change)
//!
//! 1. Add the variant and its run code where the family is run (a [`Part`],
//!    [`Condition`], [`Action`] or [`On`] variant and its arm).
//! 2. Add one [`Block`] row to the family's table here.
//! 3. Run the tests of this module: they check that the table lists exactly
//!    the names the file readers accept, and that a sample file built from
//!    each row's [`Field`]s reads back with the real reader (so a missing or
//!    misspelled key in the row fails the build).
//!
//! A new family of another lane (a monster AI) adds its own table
//! file, a [`Family`] variant and the [`Place`] rows for the keys that carry its
//! names in its file.
//!
//! [`Part`]: super::Part
//! [`Condition`]: super::Condition
//! [`Action`]: super::Action
//! [`On`]: super::On

mod actions;
mod conditions;
mod events;
mod objectives;
mod rewards;
mod skill_parts;
mod targets;
mod values;

#[cfg(test)]
mod tests;

/// A group of blocks that share one place in a mod file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    Event,
    Condition,
    Action,
    SkillPart,
    Target,
    Value,
    Objective,
    Reward,
}

impl Family {
    /// Every family, in the order tools list them.
    pub const ALL: [Family; 8] = [
        Family::SkillPart,
        Family::Event,
        Family::Condition,
        Family::Action,
        Family::Target,
        Family::Value,
        Family::Objective,
        Family::Reward,
    ];

    /// The singular name used in messages ("unknown skill part `x`").
    pub fn noun(self) -> &'static str {
        match self {
            Family::Event => "event",
            Family::Condition => "condition",
            Family::Action => "action",
            Family::SkillPart => "skill part",
            Family::Target => "target",
            Family::Value => "value",
            Family::Objective => "quest objective",
            Family::Reward => "quest reward",
        }
    }

    /// The plural heading tools show.
    pub fn title(self) -> &'static str {
        match self {
            Family::Event => "Events",
            Family::Condition => "Conditions",
            Family::Action => "Actions",
            Family::SkillPart => "Skill parts",
            Family::Target => "Targets",
            Family::Value => "Values",
            Family::Objective => "Quest objectives",
            Family::Reward => "Quest rewards",
        }
    }

    fn table(self) -> &'static [Block] {
        match self {
            Family::Event => events::BLOCKS,
            Family::Condition => conditions::BLOCKS,
            Family::Action => actions::BLOCKS,
            Family::SkillPart => skill_parts::BLOCKS,
            Family::Target => targets::BLOCKS,
            Family::Value => values::BLOCKS,
            Family::Objective => objectives::BLOCKS,
            Family::Reward => rewards::BLOCKS,
        }
    }
}

/// What a key of a block holds. Tools pick the editing widget from it and
/// the sample files of the tests are built from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// A number or a skill formula (`"10 + lvl * 4"`).
    Formula,
    /// A whole number (a count, a percent).
    Whole,
    /// `true` or `false`.
    Flag,
    /// One of these names.
    Choice(&'static [&'static str]),
    /// A row of the `states` table.
    State,
    /// A row of the `skills` table.
    Skill,
    /// A row of the `itemstatcost` table.
    Stat,
    /// A row of the `missiles` table.
    Missile,
    /// A row of the `monstats` table.
    Monster,
    /// A `pettype` row.
    PetType,
    /// A relative `scripts/` path ending in `.luau` or `.graph.toml`.
    ScriptModule,
    /// An exported Luau function or a visual graph call entry name.
    ScriptFunction,
    /// An `overlay` or other table row the block only passes through.
    Row,
    /// A damage block (`element`, `min`, `max`, ...).
    Damage,
    /// A list of declared crowd control effects (`[[...cc]]`).
    CrowdControl,
    /// One crowd control effect (`{ kind = "stun", seconds = 1 }`).
    Effect,
    /// One `{ stat, value }` pair.
    StatValue,
    /// A list of `{ stat, value }` pairs.
    StatList,
    /// A list of combat event / callback pairs.
    BuffEvents,
    /// A list of skills rows.
    SkillList,
    /// A line of text.
    Text,
    /// The code of an item (`"hp1"`, up to four characters).
    ItemCode,
}

impl FieldKind {
    /// A TOML value of this kind that the readers accept, for sample files.
    pub fn sample(self) -> String {
        match self {
            FieldKind::Formula => "\"1 + lvl\"".into(),
            FieldKind::Whole => "1".into(),
            FieldKind::Flag => "true".into(),
            FieldKind::ScriptModule => "\"main.luau\"".into(),
            FieldKind::ScriptFunction => "\"run\"".into(),
            FieldKind::Choice(names) => format!("\"{}\"", names.first().copied().unwrap_or("")),
            FieldKind::State
            | FieldKind::Skill
            | FieldKind::Stat
            | FieldKind::Missile
            | FieldKind::Monster
            | FieldKind::PetType
            | FieldKind::Row => "1".into(),
            FieldKind::Damage => "{ element = \"fire\", min = 1, max = 2 }".into(),
            FieldKind::CrowdControl => "[{ kind = \"stun\", seconds = 1 }]".into(),
            FieldKind::Effect => "{ kind = \"stun\", seconds = 1 }".into(),
            FieldKind::StatValue => "{ stat = 1, value = 1 }".into(),
            FieldKind::StatList => "[{ stat = 1, value = 1 }]".into(),
            FieldKind::BuffEvents => "[{ event = 1, callback = 1 }]".into(),
            FieldKind::SkillList => "[1, 2]".into(),
            FieldKind::Text => "\"Well done.\"".into(),
            FieldKind::ItemCode => "\"hp1\"".into(),
        }
    }
}

/// One key of a block, as it is written in the mod file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    pub key: &'static str,
    pub kind: FieldKind,
    /// The file is refused when the key is missing.
    pub required: bool,
    /// What the key means when left out ("" for none, or for a required key).
    pub default: &'static str,
    pub about: &'static str,
}

/// A block that is on its way out. It keeps working for the API support
/// window; [`check`] warns about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deprecated {
    /// The API version that deprecated it.
    pub since: u32,
    /// The block to use instead ("" when there is none).
    pub use_instead: &'static str,
    pub note: &'static str,
}

/// One entry of the vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub family: Family,
    /// The name as written in the mod file. Versioned: see the module header.
    pub name: &'static str,
    /// The name tools show.
    pub label: &'static str,
    /// The modding API version that introduced the block.
    pub since: u32,
    pub deprecated: Option<Deprecated>,
    /// What the block does, in plain words.
    pub about: &'static str,
    /// For targets: the key the name is written under (`affects`, `who`,
    /// `on`, `around`); empty for the other families.
    pub slot: &'static str,
    pub fields: &'static [Field],
}

/// The modding API version this vocabulary is made for: no block has a
/// `since` above it. (`d2_modkit::api::CURRENT_API` is the number a mod's
/// manifest is checked against; a test in the check-mod stage keeps the two
/// in step.)
pub const VOCABULARY_API: u32 = 1;

/// Every block of every family, [`Family::ALL`] order, each family in table
/// order.
pub fn all() -> impl Iterator<Item = &'static Block> {
    Family::ALL.into_iter().flat_map(|f| f.table().iter())
}

/// The blocks of one family, in table order.
pub fn family(family: Family) -> &'static [Block] {
    family.table()
}

/// The block `name` of `family`. For targets use [`target`] (a name is only
/// unique within its slot).
pub fn find(family: Family, name: &str) -> Option<&'static Block> {
    family.table().iter().find(|b| b.name == name)
}

/// The target block `name` written under `slot` (`affects`, `who`, ...).
pub fn target(slot: &str, name: &str) -> Option<&'static Block> {
    targets::BLOCKS
        .iter()
        .find(|b| b.slot == slot && b.name == name)
}

/// A sample mod file text that uses `block` once, built from its fields
/// (`all`: every key; else only the required ones). `None` for the families
/// that are not written as a file entry (targets, values).
pub fn sample(block: &Block, all: bool) -> Option<String> {
    let mut keys = String::new();
    for f in block
        .fields
        .iter()
        .filter(|f| all || f.required)
        .filter(|f| f.key != "kind")
    {
        // A declared flag default can select a mutually exclusive mode.
        let value = if matches!(f.kind, FieldKind::Flag) && matches!(f.default, "true" | "false") {
            f.default.to_owned()
        } else {
            f.kind.sample()
        };
        keys.push_str(&format!("{} = {value}\n", f.key));
    }
    Some(match block.family {
        Family::SkillPart => format!(
            "[[skill]]\nid = 1\n[[skill.part]]\nkind = \"{}\"\n{keys}",
            block.name
        ),
        Family::Event => {
            let extra = match block.name {
                "state_start" | "state_end" if !keys.contains("state =") => "state = 1\n",
                _ => "",
            };
            format!(
                "[[trigger]]\non = \"{}\"\nwhile_skill = 1\n{extra}{keys}\
                 [[trigger.action]]\nkind = \"remove_state\"\nstate = 1\n",
                block.name
            )
        }
        Family::Condition => format!(
            "[[trigger]]\non = \"attack\"\nwhile_skill = 1\n\
             [[trigger.condition]]\nkind = \"{}\"\n{keys}\
             [[trigger.action]]\nkind = \"remove_state\"\nstate = 1\n",
            block.name
        ),
        Family::Action => format!(
            "[[trigger]]\non = \"attack\"\nwhile_skill = 1\n\
             [[trigger.action]]\nkind = \"{}\"\n{keys}",
            block.name
        ),
        Family::Objective => format!(
            "[[quest]]
id = 1
name = \"Q\"
[[quest.objective]]
kind = \"{}\"
{keys}",
            block.name
        ),
        Family::Reward => format!(
            "[[quest]]
id = 1
name = \"Q\"
[[quest.objective]]
kind = \"enter\"
level = 1
             [[quest.reward]]
kind = \"{}\"
{keys}",
            block.name
        ),
        Family::Target | Family::Value => return None,
    })
}

/// The mod files whose names the registry checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// `files/skills.toml`.
    Skills,
    /// `files/triggers.toml`.
    Triggers,
    /// `files/quests.toml`.
    Quests,
}

/// A key of a mod file that carries a block name: the tables
/// [`scan`] reads. Another lane's file adds its rows here.
struct Place {
    file: FileKind,
    /// The array-of-tables path from the file root (`["skill", "part"]`).
    path: &'static [&'static str],
    /// The key holding the name.
    key: &'static str,
    family: Family,
    /// For [`Family::Target`]: the slot the names are checked under.
    slot: &'static str,
}

const PLACES: &[Place] = &[
    // skills.toml
    Place {
        file: FileKind::Skills,
        path: &["skill", "part"],
        key: "kind",
        family: Family::SkillPart,
        slot: "",
    },
    Place {
        file: FileKind::Skills,
        path: &["skill", "part"],
        key: "affects",
        family: Family::Target,
        slot: "affects",
    },
    Place {
        file: FileKind::Skills,
        path: &["skill", "part"],
        key: "on",
        family: Family::Target,
        slot: "on",
    },
    Place {
        file: FileKind::Skills,
        path: &["skill", "part"],
        key: "around",
        family: Family::Target,
        slot: "around",
    },
    // triggers.toml
    Place {
        file: FileKind::Triggers,
        path: &["trigger"],
        key: "on",
        family: Family::Event,
        slot: "",
    },
    Place {
        file: FileKind::Triggers,
        path: &["trigger", "condition"],
        key: "kind",
        family: Family::Condition,
        slot: "",
    },
    Place {
        file: FileKind::Triggers,
        path: &["trigger", "condition"],
        key: "who",
        family: Family::Target,
        slot: "who",
    },
    Place {
        file: FileKind::Triggers,
        path: &["trigger", "action"],
        key: "kind",
        family: Family::Action,
        slot: "",
    },
    Place {
        file: FileKind::Triggers,
        path: &["trigger", "action"],
        key: "who",
        family: Family::Target,
        slot: "who",
    },
    Place {
        file: FileKind::Triggers,
        path: &["trigger", "action"],
        key: "at",
        family: Family::Target,
        slot: "who",
    },
    // quests.toml
    Place {
        file: FileKind::Quests,
        path: &["quest", "objective"],
        key: "kind",
        family: Family::Objective,
        slot: "",
    },
    Place {
        file: FileKind::Quests,
        path: &["quest", "reward"],
        key: "kind",
        family: Family::Reward,
        slot: "",
    },
];

/// One use of a block name in a mod file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Use {
    pub family: Family,
    pub slot: &'static str,
    /// The name as written.
    pub name: String,
    /// Where: `skill[0].part[1].kind`.
    pub at: String,
}

/// Every block name a file of `kind` uses, in file order. A text that is not
/// TOML is an error (the file readers report it with more context).
pub fn scan(kind: FileKind, text: &str) -> Result<Vec<Use>, toml::de::Error> {
    let root: toml::Table = toml::from_str(text)?;
    let mut uses = Vec::new();
    for place in PLACES.iter().filter(|p| p.file == kind) {
        walk(&root, place, place.path, String::new(), &mut uses);
    }
    // File order: by the position text of each use.
    uses.sort_by_cached_key(|u| order_key(&u.at));
    Ok(uses)
}

fn walk(table: &toml::Table, place: &Place, rest: &[&str], at: String, out: &mut Vec<Use>) {
    let Some((head, tail)) = rest.split_first() else {
        if let Some(toml::Value::String(name)) = table.get(place.key) {
            out.push(Use {
                family: place.family,
                slot: place.slot,
                name: name.clone(),
                at: format!("{at}.{}", place.key)
                    .trim_start_matches('.')
                    .to_owned(),
            });
        }
        return;
    };
    let Some(toml::Value::Array(items)) = table.get(*head) else {
        return;
    };
    for (i, item) in items.iter().enumerate() {
        if let toml::Value::Table(t) = item {
            let here = if at.is_empty() {
                format!("{head}[{i}]")
            } else {
                format!("{at}.{head}[{i}]")
            };
            walk(t, place, tail, here, out);
        }
    }
}

/// Sorts uses by the numbers in their location, so `trigger[1]` follows
/// `trigger[0].action[3]`.
fn order_key(at: &str) -> Vec<usize> {
    let mut key = Vec::new();
    let mut rest = at;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find(']') else {
            break;
        };
        if let Ok(n) = rest[open + 1..open + close].parse() {
            key.push(n);
        }
        rest = &rest[open + close + 1..];
    }
    key
}

/// What is wrong or noteworthy about a block name a mod uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub at: String,
    pub severity: Severity,
    pub message: String,
}

/// How serious a [`Problem`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The mod cannot play (an unknown name, a block newer than the mod's API).
    Error,
    /// The mod plays but should change (a deprecated block).
    Warning,
}

/// Checks every block name `text` uses against the vocabulary for a mod made
/// for API version `api`: unknown names (with a suggestion), blocks newer than
/// `api` and deprecated blocks.
pub fn check(kind: FileKind, text: &str, api: u32) -> Result<Vec<Problem>, toml::de::Error> {
    let mut problems = Vec::new();
    for u in scan(kind, text)? {
        let found = if u.family == Family::Target {
            target(u.slot, &u.name)
        } else {
            find(u.family, &u.name)
        };
        problems.extend(judge(&u, found, api));
    }
    Ok(problems)
}

/// What is wrong with one use, given the block it names (if any).
fn judge(u: &Use, found: Option<&Block>, api: u32) -> Vec<Problem> {
    let Some(block) = found else {
        return vec![Problem {
            message: unknown_message(u),
            at: u.at.clone(),
            severity: Severity::Error,
        }];
    };
    let mut problems = Vec::new();
    if block.since > api {
        problems.push(Problem {
            message: format!(
                "{} `{}` came in with modding API {}, and this mod is made for API {api};                  raise `api` in mod.toml",
                u.family.noun(),
                u.name,
                block.since
            ),
            at: u.at.clone(),
            severity: Severity::Error,
        });
    }
    if let Some(d) = block.deprecated {
        let instead = if d.use_instead.is_empty() {
            String::new()
        } else {
            format!("; use `{}` instead", d.use_instead)
        };
        problems.push(Problem {
            message: format!(
                "{} `{}` is deprecated since API {}{instead}. {}",
                u.family.noun(),
                u.name,
                d.since,
                d.note
            ),
            at: u.at.clone(),
            severity: Severity::Warning,
        });
    }
    problems
}

fn candidates(u: &Use) -> Vec<&'static str> {
    if u.family == Family::Target {
        targets::BLOCKS
            .iter()
            .filter(|b| b.slot == u.slot)
            .map(|b| b.name)
            .collect()
    } else {
        u.family.table().iter().map(|b| b.name).collect()
    }
}

fn unknown_message(u: &Use) -> String {
    let known = candidates(u);
    let mut msg = format!("unknown {} `{}`", u.family.noun(), u.name);
    if let Some(near) = closest(&u.name, &known) {
        msg.push_str(&format!("; did you mean `{near}`?"));
    }
    msg.push_str(&format!(
        " The {}s are: {}",
        u.family.noun(),
        known
            .iter()
            .map(|k| format!("`{k}`"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    msg
}

/// The known name nearest to `name` when one is close enough to be a typo.
fn closest(name: &str, known: &[&'static str]) -> Option<&'static str> {
    let limit = (name.chars().count() / 3).clamp(1, 3);
    known
        .iter()
        .map(|k| (distance(name, k), *k))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k)
}

/// The edit distance of two short names (insert, delete, change).
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let up = row[j + 1];
            row[j + 1] = if ca == cb {
                diagonal
            } else {
                1 + diagonal.min(up).min(row[j])
            };
            diagonal = up;
        }
    }
    row[b.len()]
}
