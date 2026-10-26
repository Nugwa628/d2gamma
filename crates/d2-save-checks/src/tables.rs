//! The realm's tables the checks read: `d2-sim`'s item tables (items,
//! types, affixes, qualities, uniques, sets, runewords, properties and the
//! stat columns), the vitals tables (`charstats`, `experience`), the
//! `itemstatcost` save columns and the hireling rows of the save reader.
//! No maps, no level data.
//!
//! The property ranges ([`crate::items::ItemRanges`]) run `d2-sim`'s own
//! property functions on copies of the item tables in which every property
//! record's roll range is pinned to one end ([`CheckTables::corners`]): the
//! item's stats must lie between the lowest and the highest result.

use d2_data::bin::BinTable;
use d2_data::fixup::FixedSet;
use d2_data::tables::{decode_all, Itemstatcost, Record, WrongTable};
use d2_formats::d2s::{Hireling, SaveTables, StatSave};
use d2_sim::combat::vitals::VitalsTables;
use d2_sim::items::bitstream::read::read_save_entry;
use d2_sim::items::tables::{ItemTables, PropRec, TableError};
use d2_sim::world::hirelings::rows::HirelingRows;

/// A table the checks need is missing or malformed.
#[derive(Debug, thiserror::Error)]
pub enum TablesError {
    /// A table is not in the set.
    #[error("table {0} missing")]
    Missing(&'static str),
    /// A table has the wrong layout.
    #[error(transparent)]
    Wrong(#[from] WrongTable),
    /// The item tables did not project.
    #[error("item tables: {0}")]
    Items(#[from] TableError),
    /// The hireling rows did not read.
    #[error("hireling rows: {0}")]
    Hirelings(String),
}

/// The realm's tables, read once at start and shared by every check.
#[derive(Clone, Debug)]
pub struct CheckTables {
    /// The item tables, as item creation reads them.
    pub items: ItemTables,
    /// `charstats` and `experience`.
    pub vitals: VitalsTables,
    /// The `itemstatcost` save columns by stat id (the save reader's).
    pub stat_saves: Vec<StatSave>,
    /// The hireling rows (the save reader reads a hireling's items only
    /// when its row is found).
    pub hirelings: HirelingRows,
    corners: [ItemTables; 4],
}

/// Property functions whose result does not grow with the roll. Function 7
/// (enhanced damage) writes 1 maximum damage instead of its percent when
/// the base's maximum damage times the roll is below 100, so on a weapon
/// with little damage the low end of its roll gives more maximum damage
/// than the high end. [`CheckTables::corners`] also pins these records to
/// the end opposite the other records'.
pub const TURNING_FUNCS: [u8; 1] = [7];

fn table<'a>(set: &'a FixedSet, name: &'static str) -> Result<&'a BinTable, TablesError> {
    set.table(name).ok_or(TablesError::Missing(name))
}

fn rows<T: Record>(set: &FixedSet) -> Result<Vec<T>, TablesError> {
    Ok(decode_all::<T>(table(set, T::TABLE)?)?)
}

impl CheckTables {
    /// From the realm's fixed-up table set (`d2_data::fixup::apply`).
    pub fn from_fixed(set: &FixedSet) -> Result<Self, TablesError> {
        let stat_saves = rows::<Itemstatcost>(set)?
            .iter()
            .map(|r| StatSave {
                bits: r.csvbits,
                param: r.csvparam,
                signed: r.csvsigned,
            })
            .collect();
        let vitals = VitalsTables {
            charstats: rows(set)?,
            experience: rows(set)?,
        };
        let hirelings = HirelingRows::from_table(table(set, "hireling")?)
            .map_err(|e| TablesError::Hirelings(e.to_string()))?;
        Ok(Self::from_parts(
            ItemTables::from_fixed(set)?,
            vitals,
            stat_saves,
            hirelings,
        ))
    }

    /// From tables already built (tests, a mod's own projection).
    pub fn from_parts(
        items: ItemTables,
        vitals: VitalsTables,
        stat_saves: Vec<StatSave>,
        hirelings: HirelingRows,
    ) -> Self {
        let low = |r: &PropRec| r.min.min(r.max);
        let high = |r: &PropRec| r.min.max(r.max);
        let corners = [
            pinned(&items, |r, _| low(r)),
            pinned(&items, |r, _| high(r)),
            pinned(&items, |r, turning| if turning { high(r) } else { low(r) }),
            pinned(&items, |r, turning| if turning { low(r) } else { high(r) }),
        ];
        Self {
            items,
            vitals,
            stat_saves,
            hirelings,
            corners,
        }
    }

    /// The item tables with every property roll pinned to one end: all
    /// low, all high, and the two with the records of a
    /// [`TURNING_FUNCS`] function at the other end. Every other function's
    /// stats grow with its roll; each stat of a turning function grows or
    /// shrinks with it, the same way on every record, so each stat's lowest
    /// and highest values are among the four.
    pub fn corners(&self) -> &[ItemTables; 4] {
        &self.corners
    }

    /// The save reader's tables for a record of an `expansion` character.
    pub fn reading(&self, expansion: bool) -> Reading<'_> {
        Reading {
            tables: self,
            expansion,
        }
    }
}

/// `d2s::SaveTables` over [`CheckTables`] for one record.
pub struct Reading<'a> {
    tables: &'a CheckTables,
    expansion: bool,
}

impl SaveTables for Reading<'_> {
    fn stat_save(&self, id: u16) -> Option<StatSave> {
        self.tables.stat_saves.get(usize::from(id)).copied()
    }
    fn item_entry_len(&self, buf: &[u8]) -> Result<usize, String> {
        read_save_entry(buf, &self.tables.items)
            .map(|e| e.len)
            .map_err(|e| e.to_string())
    }
    fn hireling_restored(&self, h: &Hireling) -> bool {
        h.is_present()
            && self
                .tables
                .hirelings
                .row_at(self.expansion, u32::from(h.id), 1)
                .is_some()
    }
}

/// A copy of `t` with every property record's `min` and `max` set to
/// `end(record, turning)`, `turning` when its properties row has a
/// [`TURNING_FUNCS`] function.
fn pinned(src: &ItemTables, end: impl Fn(&PropRec, bool) -> i32) -> ItemTables {
    let turning = |r: &PropRec| {
        usize::try_from(r.code)
            .ok()
            .and_then(|c| src.properties.get(c))
            .is_some_and(|p| p.slots.iter().any(|s| TURNING_FUNCS.contains(&s.func)))
    };
    let mut t = src.clone();
    let pin = |r: &mut PropRec| {
        if r.code >= 0 {
            let v = end(r, turning(r));
            r.min = v;
            r.max = v;
        }
    };
    let all = |recs: &mut [PropRec]| recs.iter_mut().for_each(pin);
    t.magic.iter_mut().for_each(|a| all(&mut a.mods));
    t.qualityitems.iter_mut().for_each(|q| all(&mut q.mods));
    t.uniques.iter_mut().for_each(|u| all(&mut u.props));
    t.setitems.iter_mut().for_each(|s| {
        all(&mut s.props);
        all(&mut s.aprops);
    });
    t.runes.iter_mut().for_each(|r| all(&mut r.props));
    t.gems
        .iter_mut()
        .for_each(|g| g.mods.iter_mut().for_each(|m| all(m)));
    t
}
