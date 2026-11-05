//! Validates the declared table links on effective patched text, before a
//! missing key can compile to the legal no-reference sentinel.

use super::{Code, Finding, PatchData};
use crate::compile::callbacks::{reference_choices, reference_linkers, ReferenceKey};
use crate::compile::{code4, name_key};
use crate::schema::{additional_reference, schema, CellReference, Link, LinkerKind};
use crate::txt::bind;
use std::collections::{BTreeMap, BTreeSet};

fn normalize(value: &[u8], kind: LinkerKind) -> Vec<u8> {
    match kind {
        LinkerKind::Code => code4(value).to_vec(),
        LinkerKind::Name => name_key(value).unwrap_or_default(),
    }
}

/// All broken declared lookup columns. Empty cells mean no reference.
/// Disabled rows keep their ids but cannot be a link target or source.
/// Includes references embedded in the compiler's table-specific callbacks.
pub fn references(data: &PatchData) -> Vec<Finding> {
    let mut keys: BTreeMap<String, BTreeSet<Vec<u8>>> = BTreeMap::new();
    let mut reserved: BTreeMap<String, BTreeSet<Vec<u8>>> = BTreeMap::new();
    for def in schema().called() {
        for f in def.fields.iter().filter(|f| f.field_type.is_own_key()) {
            if let Some(linker) = f.link.linker() {
                keys.entry(linker.to_owned()).or_default();
            }
        }
        let Some(t) = data
            .tables
            .iter()
            .find(|t| t.rules.txt_name == def.txt_name)
        else {
            continue;
        };
        let names: Vec<_> = def.fields.iter().map(|f| f.column.as_slice()).collect();
        let binding = bind(&t.header, &names);
        for (i, f) in def
            .fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.field_type.is_own_key())
        {
            let (Some(linker), Some(kind), Some(c)) = (
                f.link.linker(),
                f.field_type.linker_kind(),
                binding.field_column[i],
            ) else {
                continue;
            };
            let set = keys.entry(linker.to_owned()).or_default();
            let reserved = reserved.entry(linker.to_owned()).or_default();
            for (r, row) in t.rows.iter().enumerate() {
                reserved.insert(normalize(&row.cells[c], kind));
                if !t.disabled(r) {
                    set.insert(normalize(&row.cells[c], kind));
                }
            }
        }
    }
    keys.insert(
        "@range".into(),
        ["none", "h2h", "rng", "both", "loc"]
            .into_iter()
            .map(|s| code4(s.as_bytes()).to_vec())
            .collect(),
    );
    for (name, linker) in [
        ("uniqueitems", "@uniques"),
        ("setitems", "@sets"),
        ("treasureclassex", "@treasureclass"),
    ] {
        let set = keys.entry(linker.into()).or_default();
        let reserved = reserved.entry(linker.into()).or_default();
        if let Some(t) = data.table(name) {
            for r in 0..t.rows.len() {
                if name == "treasureclassex" && t.key(r).is_empty() {
                    break;
                }
                reserved.insert(normalize(t.key(r), LinkerKind::Name));
                if !t.disabled(r) {
                    set.insert(normalize(t.key(r), LinkerKind::Name));
                }
            }
        }
    }
    if let Some(t) = data.table("itemtypes") {
        if let Some(c) = t
            .header
            .iter()
            .position(|h| h.eq_ignore_ascii_case(b"treasureclass"))
        {
            let set = keys.entry("@treasureclass".into()).or_default();
            for (r, row) in t.rows.iter().enumerate() {
                if t.disabled(r) || crate::compile::parse_int(&row.cells[c]) as u8 == 0 {
                    continue;
                }
                let code = code4(t.key(r));
                let end = code.iter().rposition(|b| *b != b' ').map_or(0, |i| i + 1);
                let code = &code[..end];
                for level in (3..=96).step_by(3) {
                    let mut key = code.to_vec();
                    key.extend_from_slice(level.to_string().as_bytes());
                    set.insert(normalize(&key, LinkerKind::Name));
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut definitions: Vec<_> = schema().called().cloned().collect();
    definitions.push(crate::schema::TableDef {
        name: "soundenviron".into(),
        txt_name: "soundenviron.txt".into(),
        bin_name: String::new(),
        record_size: crate::sounds::SOUNDENV_RECORD_SIZE,
        load_step: None,
        key_column: String::new(),
        live_source: None,
        notes: String::new(),
        fields: crate::sounds::soundenviron_fields(),
    });
    for def in &definitions {
        let Some(t) = data
            .tables
            .iter()
            .find(|t| t.rules.txt_name == def.txt_name)
        else {
            continue;
        };
        let names: Vec<_> = def.fields.iter().map(|f| f.column.as_slice()).collect();
        let binding = bind(&t.header, &names);
        for (i, f) in def.fields.iter().enumerate().filter(|(_, f)| {
            f.field_type.is_lookup()
                || additional_reference(&def.name, &f.column).is_some()
                || matches!(&f.link, Link::Table(name) if !reference_linkers(name).is_empty())
        }) {
            let Some(c) = binding.field_column[i] else {
                continue;
            };
            let extra = additional_reference(&def.name, &f.column);
            let callback = match &f.link {
                Link::Table(name) if !reference_linkers(name).is_empty() => Some(name.as_str()),
                _ => None,
            };
            let (linker, kind) = match extra {
                Some(CellReference::Code(linker)) => (linker.to_owned(), LinkerKind::Code),
                Some(CellReference::Id { table, .. }) => (table.to_owned(), LinkerKind::Name),
                None => {
                    if let Some(callback) = callback {
                        (callback.to_owned(), LinkerKind::Name)
                    } else {
                        let (Some(linker), Some(kind)) =
                            (f.link.linker(), f.field_type.linker_kind())
                        else {
                            continue;
                        };
                        (linker.to_owned(), kind)
                    }
                }
            };
            for (r, row) in t.rows.iter().enumerate() {
                let value = &row.cells[c];
                if t.disabled(r) || value.is_empty() {
                    continue;
                }
                let valid = if let Some(callback) = callback {
                    let inactive = callback == "monstats.skillmode" && {
                        let skill = format!("Skill{}", f.offset + 1);
                        t.header
                            .iter()
                            .position(|h| h.eq_ignore_ascii_case(skill.as_bytes()))
                            .is_none_or(|c| row.cells[c].is_empty())
                    };
                    inactive
                        || reference_choices(callback, value).iter().all(|group| {
                            // Linkers retain disabled keys to reserve ids. Resolve
                            // the same first alternative as the compiler, then
                            // require that exact target to remain active.
                            let chosen = group.iter().find(|target| match target {
                                ReferenceKey::Lookup { linker, key } => reserved
                                    .get(*linker)
                                    .is_some_and(|valid| valid.contains(key)),
                                ReferenceKey::Row { .. } => true,
                            });
                            chosen.is_some_and(|target| match target {
                                ReferenceKey::Lookup { linker, key } => {
                                    keys.get(*linker).is_some_and(|valid| valid.contains(key))
                                }
                                ReferenceKey::Row { table, id } => data
                                    .table(table)
                                    .is_some_and(|t| *id < t.rows.len() && !t.disabled(*id)),
                            })
                        })
                } else {
                    match extra {
                        Some(CellReference::Id {
                            table,
                            column,
                            zero_is_none,
                        }) => valid_id(data, table, column, zero_is_none, value),
                        _ => keys
                            .get(&linker)
                            .is_some_and(|valid| valid.contains(&normalize(value, kind))),
                    }
                };
                if valid {
                    continue;
                }
                if !seen.insert((t.name().to_owned(), r, c)) {
                    continue;
                }
                let mut finding = Finding::new(
                    Code::C03,
                    &super::base_file(&t.source, &t.rules.txt_name),
                    r + 2,
                    c + 1,
                );
                finding.table = Some(t.name().to_owned());
                finding.row = Some((r, t.key(r).to_vec()));
                finding.column = Some(t.canonical(c));
                finding.found = Some(value.clone());
                finding.detail =
                    format!("broken reference to {linker}: target is missing or disabled");
                out.push(finding);
            }
        }
    }
    super::sort_report(&mut out);
    out
}

/// Checks patched references against the engine's accepted base data.
/// Existing unresolved base tokens denote absent optional content. Only an
/// identical cell still unresolved in the base is accepted; changing a bad
/// value to another bad value is always reported, as is disabling its target.
pub fn check_references(base: &PatchData, patched: &PatchData) -> Vec<Finding> {
    let accepted: BTreeSet<_> = references(base)
        .into_iter()
        .map(|f| (f.table, f.row, f.column, f.found))
        .collect();
    references(patched)
        .into_iter()
        .filter(|f| {
            !accepted.contains(&(
                f.table.clone(),
                f.row.clone(),
                f.column.clone(),
                f.found.clone(),
            ))
        })
        .collect()
}

fn valid_id(
    data: &PatchData,
    name: &str,
    column: Option<&str>,
    zero_is_none: bool,
    value: &[u8],
) -> bool {
    if value == b"-1" {
        return true;
    }
    let Some(id) = std::str::from_utf8(value)
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
    else {
        return false;
    };
    if id == 0 && zero_is_none {
        return true;
    }
    let Some(target) = data.table(name) else {
        return false;
    };
    if let Some(column) = column {
        let Some(c) = target
            .header
            .iter()
            .position(|h| h.eq_ignore_ascii_case(column.as_bytes()))
        else {
            return false;
        };
        target.rows.iter().enumerate().any(|(r, row)| {
            !target.disabled(r) && crate::compile::parse_int(&row.cells[c]) as usize == id
        })
    } else {
        id < target.rows.len() && !target.disabled(id)
    }
}
