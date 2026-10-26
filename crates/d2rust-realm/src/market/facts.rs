//! An item's facts from its record: the realm reads the save format's
//! item record with its own game data, so what the market indexes and
//! groups by (quality, names, type tree, runeword, identified, stats)
//! never comes from a client.

use std::collections::BTreeMap;
use std::sync::Arc;

use d2_data::fixup::FixedSet;
use d2_data::strings::StringTables;
use d2_data::tables::{
    decode_all, Armor, Magicprefix, Magicsuffix, Misc, Rareprefix, Raresuffix, Record, Setitems,
    Uniqueitems, Weapons,
};
use d2_economy::{ItemFacts, Stackable};
use d2_save_checks::CheckTables;
use d2_sim::items::bitstream::read::read_save_entry;
use d2_sim::items::bitstream::StreamItem;
use d2_sim::items::tables::ItemTables;
use d2_sim::items::{flag, q, ty};

/// The display names the facts need, by table row.
#[derive(Debug, Default)]
struct Names {
    /// Base names by item code.
    base: BTreeMap<[u8; 4], String>,
    /// Base level requirements by item code.
    base_level: BTreeMap<[u8; 4], u8>,
    unique: Vec<String>,
    set: Vec<String>,
    prefix: Vec<String>,
    suffix: Vec<String>,
    rare_prefix: Vec<String>,
    rare_suffix: Vec<String>,
}

/// Reads item records with the realm's game data.
pub struct ItemReader {
    tables: Arc<CheckTables>,
    names: Names,
    strings: StringTables,
}

impl std::fmt::Debug for ItemReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemReader")
            .field("bases", &self.names.base.len())
            .finish_non_exhaustive()
    }
}

fn rows<T: Record>(set: &FixedSet) -> Result<Vec<T>, String> {
    let t = set
        .table(T::TABLE)
        .ok_or_else(|| format!("{} not loaded", T::TABLE))?;
    decode_all::<T>(t).map_err(|e| format!("{}: {e}", T::TABLE))
}

/// A string-table key as stored in a record: NUL-ended bytes.
fn key(b: &[u8]) -> &[u8] {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    &b[..n]
}

/// Display text without its colour codes (`ÿc` and one character).
fn plain(units: &[u16]) -> String {
    let s = String::from_utf16_lossy(units);
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == 'ÿ' {
            if chars.next() == Some('c') {
                chars.next();
            }
            continue;
        }
        if c == '\n' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out.trim().to_owned()
}

fn code_text(code: [u8; 4]) -> String {
    String::from_utf8_lossy(&code).trim_end().to_owned()
}

impl ItemReader {
    /// The names of the fixed-up tables `fixed` with its strings, over the
    /// item tables of `tables`.
    pub fn new(
        fixed: &FixedSet,
        strings: StringTables,
        tables: Arc<CheckTables>,
    ) -> Result<Self, String> {
        let text_of_key = |k: &[u8]| -> String {
            let id = strings.id(k);
            match strings.by_index(id) {
                Ok(t) if id != 0 => plain(&t),
                _ => String::from_utf8_lossy(k).into_owned(),
            }
        };
        let text_of_id = |id: u16| -> String {
            strings
                .by_index(u32::from(id))
                .map(|t| plain(&t))
                .unwrap_or_default()
        };
        let mut names = Names::default();
        for r in rows::<Weapons>(fixed)? {
            names.base.insert(r.code, text_of_id(r.namestr));
            names.base_level.insert(r.code, r.levelreq);
        }
        for r in rows::<Armor>(fixed)? {
            names.base.insert(r.code, text_of_id(r.namestr));
            names.base_level.insert(r.code, r.levelreq);
        }
        for r in rows::<Misc>(fixed)? {
            names.base.insert(r.code, text_of_id(r.namestr));
            names.base_level.insert(r.code, r.levelreq);
        }
        names.unique = rows::<Uniqueitems>(fixed)?
            .iter()
            .map(|r| text_of_key(key(&r.index)))
            .collect();
        names.set = rows::<Setitems>(fixed)?
            .iter()
            .map(|r| text_of_key(key(&r.index)))
            .collect();
        names.prefix = rows::<Magicprefix>(fixed)?
            .iter()
            .map(|r| text_of_key(key(&r.name)))
            .collect();
        names.suffix = rows::<Magicsuffix>(fixed)?
            .iter()
            .map(|r| text_of_key(key(&r.name)))
            .collect();
        names.rare_prefix = rows::<Rareprefix>(fixed)?
            .iter()
            .map(|r| text_of_key(key(&r.name)))
            .collect();
        names.rare_suffix = rows::<Raresuffix>(fixed)?
            .iter()
            .map(|r| text_of_key(key(&r.name)))
            .collect();
        Ok(Self {
            tables,
            names,
            strings,
        })
    }

    /// The item tables the records are read with.
    pub fn items(&self) -> &ItemTables {
        &self.tables.items
    }

    /// The base name of an item code (`"cap"` → "Cap").
    pub fn base_name(&self, code: &str) -> Option<String> {
        self.names.base.get(&code_key(code)?).cloned()
    }

    /// The item type codes item row `i` is: its types and every type they
    /// are a kind of, in table order.
    fn types_of(&self, i: usize) -> Vec<String> {
        let t = self.items();
        let Some(rec) = t.item(i) else {
            return Vec::new();
        };
        (0..t.itemtypes.len())
            .filter(|&n| {
                let n = n as i16;
                let is = |a: i16| a >= 0 && t.equiv.get(a as usize, n as usize);
                is(rec.type_) || (rec.type2 > 0 && is(rec.type2))
            })
            .map(|n| code_text(t.itemtypes[n].code))
            .collect()
    }

    /// The realm's default stackables: every rune and gem of the game data
    /// with its name and type tree.
    pub fn stackables(&self) -> Vec<Stackable> {
        let t = self.items();
        (0..t.items.len())
            .filter(|&i| t.is_type(i, ty::RUNE as i16) || t.is_type(i, ty::GEM as i16))
            .map(|i| self.stackable_row(i))
            .collect()
    }

    /// The stackable of `code` with its name and type tree; `None` for a
    /// code the game data doesn't have.
    pub fn stackable(&self, code: &str) -> Option<Stackable> {
        let i = self.items().find_code(code_key(code)?)?;
        Some(self.stackable_row(i))
    }

    fn stackable_row(&self, i: usize) -> Stackable {
        let code = self.items().items[i].code;
        let name = self
            .names
            .base
            .get(&code)
            .cloned()
            .unwrap_or_else(|| code_text(code));
        Stackable {
            code: code_text(code),
            name,
            types: self.types_of(i),
        }
    }

    /// The facts of an item record (the item, then its socketed items).
    pub fn facts(&self, record: &[u8]) -> Result<ItemFacts, String> {
        let entry =
            read_save_entry(record, self.items()).map_err(|e| format!("item record: {e:?}"))?;
        if entry.item.failed {
            return Err("item record: unreadable".into());
        }
        Ok(self.facts_of(entry.item.record, &entry.item.item))
    }

    fn facts_of(&self, row: usize, it: &StreamItem) -> ItemFacts {
        let t = self.items();
        let ty = t.itype_of(row);
        let identified = it.flags & flag::IDENTIFIED != 0;
        let runeword = it.flags & flag::RUNEWORD != 0;
        let base = self
            .names
            .base
            .get(&it.code)
            .cloned()
            .unwrap_or_else(|| code_text(it.code));
        let file_row = usize::try_from(it.file_index).ok();
        let mut level_req = i32::from(self.names.base_level.get(&it.code).copied().unwrap_or(0));
        let mut parts: Vec<String> = Vec::new();
        let affix = |names: &[String], id: u16| -> Option<String> {
            let row = usize::from(id).checked_sub(1)?;
            names.get(row).filter(|n| !n.is_empty()).cloned()
        };
        let name = match it.quality {
            q::UNIQUE => {
                let u = file_row.and_then(|r| self.names.unique.get(r)).cloned();
                if let Some(rq) = file_row.and_then(|r| t.uniques.get(r)) {
                    level_req = level_req.max(i32::from(rq.lvl_req));
                }
                u.unwrap_or_else(|| base.clone())
            }
            q::SET => {
                let s = file_row.and_then(|r| self.names.set.get(r)).cloned();
                if let Some(rq) = file_row.and_then(|r| t.setitems.get(r)) {
                    level_req = level_req.max(i32::from(rq.lvl_req));
                }
                s.unwrap_or_else(|| base.clone())
            }
            q::MAGIC => {
                let p = affix(&self.names.prefix, it.prefix[0]);
                let s = affix(&self.names.suffix, it.suffix[0]);
                for (id, list) in [(it.prefix[0], true), (it.suffix[0], false)] {
                    let all = if list { t.n_suffix } else { 0 };
                    let combined = usize::from(id).checked_sub(1).map(|r| r + all);
                    if let Some(a) = combined.and_then(|c| t.magic.get(c)) {
                        level_req = level_req.max(a.levelreq);
                    }
                }
                parts.extend(p.iter().cloned());
                parts.extend(s.iter().cloned());
                [p, Some(base.clone()), s]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ")
            }
            q::RARE | q::CRAFTED => {
                let rare = |id: u16, prefix: bool| -> Option<String> {
                    let combined = usize::from(id).checked_sub(1)?;
                    let n = self.names.rare_suffix.len();
                    if prefix {
                        self.names
                            .rare_prefix
                            .get(combined.checked_sub(n)?)
                            .cloned()
                    } else {
                        self.names.rare_suffix.get(combined).cloned()
                    }
                };
                let p = rare(it.rare_prefix, true);
                let s = rare(it.rare_suffix, false);
                for id in it.prefix.iter().chain(&it.suffix) {
                    parts.extend(
                        affix(&self.names.prefix, *id)
                            .into_iter()
                            .chain(affix(&self.names.suffix, *id)),
                    );
                }
                match (p, s) {
                    (Some(p), Some(s)) => format!("{p} {s}"),
                    (Some(n), None) | (None, Some(n)) => n,
                    (None, None) => base.clone(),
                }
            }
            _ if runeword => {
                let n = self
                    .strings
                    .by_index(u32::from(it.runeword))
                    .map(|t| plain(&t))
                    .unwrap_or_default();
                if n.is_empty() {
                    base.clone()
                } else {
                    n
                }
            }
            _ => base.clone(),
        };
        let mut words = vec![name.clone()];
        if name != base {
            words.push(base.clone());
        }
        for p in parts {
            if !words.contains(&p) {
                words.push(p);
            }
        }
        let mut stats: BTreeMap<u16, i32> = BTreeMap::new();
        for list in [it.main.as_ref(), it.runeword_list.as_ref()]
            .into_iter()
            .flatten()
        {
            for s in list {
                let v = stats.entry(s.stat).or_insert(0);
                *v = v.saturating_add(s.value);
            }
        }
        ItemFacts {
            code: code_text(it.code),
            item_type: ty.map(|t| code_text(t.code)).unwrap_or_default(),
            quality: it.quality,
            name,
            name_parts: words,
            level_req: level_req.clamp(0, 255) as u8,
            ilvl: it.ilvl.clamp(0, 255) as u8,
            sockets: it.base_sockets.clamp(0, 255) as u8,
            ethereal: it.flags & flag::ETHEREAL != 0,
            class: ty.map(|t| t.class).filter(|&c| c < 7),
            stats: stats.into_iter().collect(),
            runeword,
            identified,
            types: self.types_of(row),
        }
    }
}

/// A 4-byte, space-padded item code; `None` for one that doesn't fit.
pub fn code_key(code: &str) -> Option<[u8; 4]> {
    let b = code.as_bytes();
    if b.is_empty() || b.len() > 4 {
        return None;
    }
    let mut key = [b' '; 4];
    key[..b.len()].copy_from_slice(b);
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_codes_and_breaks_leave_the_text() {
        let s: Vec<u16> = "ÿc4Grim\nCrown".encode_utf16().collect();
        assert_eq!(plain(&s), "Grim Crown");
        assert_eq!(code_key("r24"), Some(*b"r24 "));
        assert_eq!(code_key("toolong"), None);
    }
}
