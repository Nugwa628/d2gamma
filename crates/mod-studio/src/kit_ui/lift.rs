//! Copying a base skill into the kit as data, and how the copy differs from
//! the base. The lifter (`d2_sim::rules::data_mod::lift`) reads the skill's
//! row of the user's own tables; the copy takes the row's id, so the table row
//! is handed to the block slot and the parts replace the Rust body.

use super::{set_cell, table_index};
use crate::kit::{parse_skill, skill_toml, Kit, SkillEntry, BLOCK_SKILL_SLOT};
use crate::tables::{self, Tables};
use anyhow::{bail, Result};
use d2_data::patch::{PatchData, PatchTable};
use d2_sim::rules::data_mod::lift::{lift, Family, Lifted, Source};

pub mod player;

/// A row of the reference tables, read as the lifter's source.
struct Row<'a> {
    data: &'a PatchData,
    row: usize,
}

fn table<'a>(data: &'a PatchData, name: &str) -> Option<&'a PatchTable> {
    data.tables
        .iter()
        .find(|t| t.name().eq_ignore_ascii_case(name))
}

impl Row<'_> {
    fn cell_of(&self, name: &str, row: usize, column: &str) -> String {
        let Some(t) = table(self.data, name) else {
            return String::new();
        };
        let Some(c) = t
            .header
            .iter()
            .position(|h| h.eq_ignore_ascii_case(column.as_bytes()))
        else {
            return String::new();
        };
        t.rows
            .get(row)
            .and_then(|r| r.cells.get(c))
            .map(|cell| tables::text(cell).trim().to_owned())
            .unwrap_or_default()
    }

    /// The row of table `name` whose key is `value` (or the number it is).
    fn index_of(&self, name: &str, value: &str) -> Option<u16> {
        let t = table(self.data, name)?;
        let found = (0..t.rows.len()).find(|r| t.key(*r).eq_ignore_ascii_case(value.as_bytes()));
        u16::try_from(found?).ok()
    }
}

impl Source for Row<'_> {
    fn item_type(&self, name: &str) -> Option<u16> {
        self.index_of("itemtypes", name)
            .or_else(|| name.parse().ok())
    }
    fn monster(&self, name: &str) -> Option<u16> {
        self.index_of("monstats", name)
            .or_else(|| name.parse().ok())
    }
    fn pet_type(&self, name: &str) -> Option<u16> {
        self.index_of("pettype", name).or_else(|| name.parse().ok())
    }
    fn monster_mode(&self, name: &str) -> Option<u16> {
        self.index_of("monmode", name).or_else(|| name.parse().ok())
    }
    fn skill_row(&self, name: &str) -> Option<u16> {
        self.index_of("skills", name).or_else(|| name.parse().ok())
    }
    fn event(&self, name: &str) -> Option<u16> {
        self.index_of("events", name).or_else(|| name.parse().ok())
    }
    fn skill(&self, column: &str) -> String {
        self.cell_of("skills", self.row, column)
    }
    fn state(&self, name: &str) -> Option<u16> {
        self.index_of("states", name).or_else(|| name.parse().ok())
    }
    fn stat(&self, name: &str) -> Option<u16> {
        self.index_of("itemstatcost", name)
            .or_else(|| name.parse().ok())
    }
    fn missile(&self, name: &str) -> Option<u16> {
        self.index_of("missiles", name)
            .or_else(|| name.parse().ok())
    }
    fn missile_cell(&self, row: u16, column: &str) -> String {
        self.cell_of("missiles", usize::from(row), column)
    }
}

/// A base skill that can be copied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Liftable {
    pub id: u16,
    pub name: String,
    pub family: Family,
}

fn lift_row(t: &Tables, id: u16) -> Result<Lifted> {
    let row = Row {
        data: t.reference(),
        row: usize::from(id),
    };
    if table(row.data, "skills").is_none_or(|s| usize::from(id) >= s.rows.len()) {
        bail!("There is no skills row {id}");
    }
    Ok(lift(id, &row)?)
}

/// The rows of the base skills table the lifter knows, in table order.
pub fn liftable(t: &Tables) -> Vec<Liftable> {
    let Some(skills) = table(t.reference(), "skills") else {
        return Vec::new();
    };
    (0..skills.rows.len().min(usize::from(u16::MAX)))
        .filter_map(|r| {
            let id = r as u16;
            let lifted = lift_row(t, id).ok()?;
            Some(Liftable {
                id,
                name: tables::row_name(skills, r),
                family: lifted.family,
            })
        })
        .collect()
}

/// Copies base skill `id` into the kit as parts and hands its table row to
/// the block slot. The lifted entry is the one the engine's reader accepts;
/// its notes (what differs from the Rust body) are returned with it.
pub fn copy_base(kit: &mut Kit, t: &mut Tables, id: u16) -> Result<Lifted> {
    if kit.skills.iter().any(|s| s.id == id) {
        bail!("Skill {id} is already in this kit");
    }
    let lifted = lift_row(t, id)?;
    let entry = parse_skill(&lifted.toml)?;
    if table_index(t, "skills").is_none() {
        bail!("The skills table is not loaded");
    }
    for (column, value) in &lifted.cells {
        set_cell(t, "skills", usize::from(id), column, value);
    }
    debug_assert!(
        lifted.family == Family::Passive
            || lifted
                .cells
                .iter()
                .any(|(c, v)| *c == "srvdofunc" && *v == BLOCK_SKILL_SLOT.to_string())
    );
    kit.skills.push(entry);
    kit.dirty = true;
    Ok(lifted)
}

/// The base of skill `id` as the text the editor saves, when the reference
/// row can still be lifted.
pub fn base_text(t: &Tables, id: u16) -> Option<String> {
    let entry = parse_skill(&lift_row(t, id).ok()?.toml).ok()?;
    Some(skill_toml(&entry))
}

/// How a line of a diff relates to the two texts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Same,
    /// Only in the base.
    Removed,
    /// Only in the edited copy.
    Added,
}

/// A line diff of `base` against `edited`, in order.
pub fn diff(base: &str, edited: &str) -> Vec<(Change, String)> {
    let a: Vec<&str> = base.lines().collect();
    let b: Vec<&str> = edited.lines().collect();
    // Longest common subsequence of the lines.
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            out.push((Change::Same, a[i].to_owned()));
            i += 1;
            j += 1;
        } else if i < a.len() && (j == b.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push((Change::Removed, a[i].to_owned()));
            i += 1;
        } else {
            out.push((Change::Added, b[j].to_owned()));
            j += 1;
        }
    }
    out
}

/// The diff of the kit's skill `id` against its base; `None` when the
/// skill is not in the kit or its base cannot be lifted.
pub fn diff_against_base(kit: &Kit, t: &Tables, id: u16) -> Option<Vec<(Change, String)>> {
    let current: &SkillEntry = kit.skills.iter().find(|s| s.id == id)?;
    Some(diff(&base_text(t, id)?, &skill_toml(current)))
}

/// `true` when the diff has any line that is not shared.
pub fn differs(lines: &[(Change, String)]) -> bool {
    lines.iter().any(|(c, _)| *c != Change::Same)
}
