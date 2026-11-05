//! Select direct content identities, rather than scenario setup dependencies.
use super::super::single_player::LiveData;
use super::super::sweep;
use anyhow::Result;
use d2_data::bin::DataRules;
use d2_data::patch::{self, PatchData};
use d2_native::source::NativeSource;
use std::collections::{BTreeMap, BTreeSet};

type Changes = BTreeMap<String, BTreeSet<usize>>;

/// Compare patched text, not formula bytecode offsets: changing a formula may
/// leave its offset unchanged, or move unrelated later formulas in the buffer.
pub(super) fn changed_rows(source: &NativeSource) -> Result<Changes> {
    let patches = source.layers.patches().map_err(anyhow::Error::msg)?;
    if patches.is_empty() {
        return Ok(Changes::new());
    }
    let rules = patch::rules_for(DataRules::Mod);
    let mut read_base = |file: &str| {
        source
            .layers
            .read_in(
                &source.layers.layers[0],
                &format!("data/global/excel/{}", file.to_ascii_lowercase()),
            )
            .map(|read| read.map(|bytes| ("native".into(), bytes)))
            .transpose()
            .map_err(|e| e.to_string())
    };
    let mut current = PatchData::from_base(&rules, &mut read_base)
        .map_err(|findings| anyhow::anyhow!("base row comparison: {findings:?}"))?;
    let previous = current.clone();
    let layers = patches
        .iter()
        .enumerate()
        .map(|(index, (path, bytes))| patch::parse_layer(path, bytes, index).0)
        .collect::<Vec<_>>();
    // The runtime loader has already validated this exact stack. Replay the
    // public patch operation to retain text identities, which NativeTables
    // deliberately discards after compilation (except runtime sound text).
    let findings = patch::apply_stack(&mut current, &layers, "check-mod");
    if patch::has_errors(&findings) {
        anyhow::bail!("row comparison: {findings:?}");
    }
    let mut changes = Changes::new();
    for (table, base) in current.tables.iter().zip(&previous.tables) {
        let rows: BTreeSet<_> = table
            .rows
            .iter()
            .enumerate()
            .filter_map(|(row, record)| {
                let equal = base.rows.get(row).is_some_and(|r| {
                    cells(&base.header, &r.cells) == cells(&table.header, &record.cells)
                });
                (!equal).then_some(row)
            })
            .collect();
        if !rows.is_empty() {
            changes.insert(table.name().to_owned(), rows);
        }
    }
    Ok(changes)
}

// Ignore empty added columns (notably the disable marker on untouched rows),
// and preserve duplicate-column identities by their occurrence number.
fn cells<'a>(header: &[Vec<u8>], row: &'a [Vec<u8>]) -> BTreeMap<(Vec<u8>, usize), &'a [u8]> {
    let mut occurrences = BTreeMap::new();
    let mut result = BTreeMap::new();
    for (name, value) in header.iter().zip(row) {
        let name = name.to_ascii_lowercase();
        let occurrence = occurrences.entry(name.clone()).or_insert(0);
        *occurrence += 1;
        if !value.is_empty() {
            result.insert((name, *occurrence), value.as_slice());
        }
    }
    result
}

fn has(changes: &Changes, table: &str, row: usize) -> bool {
    changes.get(table).is_some_and(|rows| rows.contains(&row))
}

fn selected(name: &str, family: &str, changes: &Changes, item_offsets: &[usize; 3]) -> bool {
    let parts: Vec<_> = name.split('-').collect();
    let number = |index: usize| parts.get(index).and_then(|s| s.parse::<usize>().ok());
    match (family, parts.first().copied()) {
        ("items", Some("item")) => number(1).is_some_and(|id| {
            ["weapons", "armor", "misc"]
                .iter()
                .enumerate()
                .any(|(i, table)| {
                    id.checked_sub(item_offsets[i])
                        .is_some_and(|row| has(changes, table, row))
                })
        }),
        ("items", Some("recipe")) => number(1).is_some_and(|id| has(changes, "cubemain", id)),
        ("items", Some("vendor")) | ("npcs", Some("npc")) | ("anims", Some("monster")) => {
            number(1).is_some_and(|id| has(changes, "monstats", id))
        }
        ("skills", Some("class")) => number(3).is_some_and(|id| has(changes, "skills", id)),
        ("anims", Some("class")) => {
            number(1).is_some_and(|id| has(changes, "charstats", id))
                || (parts.get(2) == Some(&"cast")
                    && number(3).is_some_and(|id| has(changes, "skills", id)))
        }
        ("areas", Some("area" | "link")) => number(1).is_some_and(|id| has(changes, "levels", id)),
        ("areas", Some("waypoint")) => number(2).is_some_and(|id| has(changes, "levels", id)),
        ("doors", Some("level")) => {
            number(1).is_some_and(|id| has(changes, "levels", id))
                || number(3).is_some_and(|id| has(changes, "objects", id))
        }
        _ => false,
    }
}

pub(super) fn select(
    live: &LiveData,
    family: &str,
    changes: &Changes,
    rules: &d2_sim::rules::Ruleset,
) -> Result<BTreeSet<String>> {
    let relevant: &[&str] = match family {
        "items" => &["weapons", "armor", "misc", "cubemain", "monstats"],
        "skills" => &["skills"],
        "npcs" => &["monstats"],
        "anims" => &["monstats", "charstats", "skills"],
        "areas" => &["levels"],
        "doors" => &["levels", "objects"],
        _ => &[],
    };
    if !relevant.iter().any(|table| changes.contains_key(*table)) {
        return Ok(BTreeSet::new());
    }
    let count = |name| live.tables.bins.table(name).map_or(0, |t| t.count);
    let offsets = [0, count("weapons"), count("weapons") + count("armor")];
    Ok(sweep::generate_with_rules(live, family, rules)?
        .cases
        .into_iter()
        .filter(|case| selected(&case.name, family, changes, &offsets))
        .map(|case| case.name)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2_data::txt::TxtTable;
    #[test]
    fn disable_bookkeeping_does_not_select_untouched_rows() {
        let base = TxtTable::parse("base", b"Id\tcalc1\r\na\tlvl*2\r\nb\tlvl*2\r\n").unwrap();
        let patched = TxtTable::parse(
            "mod",
            b"Id\tcalc1\t__d2_disabled\r\na\tlvl*2\t1\r\nb\tlvl*2\t\r\n",
        )
        .unwrap();
        assert_ne!(
            cells(&base.header, &base.records[0].cells),
            cells(&patched.header, &patched.records[0].cells)
        );
        assert_eq!(
            cells(&base.header, &base.records[1].cells),
            cells(&patched.header, &patched.records[1].cells)
        );
    }

    #[test]
    fn changed_and_added_identities_select_only_their_own_cases() {
        let changes = BTreeMap::from([
            ("weapons".into(), BTreeSet::from([2])),
            ("armor".into(), BTreeSet::from([1])),
            ("misc".into(), BTreeSet::from([5])),
            ("skills".into(), BTreeSet::from([9, 1000])),
            ("monstats".into(), BTreeSet::from([3, 800])),
            ("cubemain".into(), BTreeSet::from([4])),
        ]);
        for (family, name) in [
            ("items", "item-2-equip"),
            ("items", "item-11-stash"),
            ("items", "item-25-pickup-drop"),
            ("items", "recipe-4"),
            ("skills", "class-0-skill-9-self"),
            ("skills", "class-1-skill-1000-target"),
            ("anims", "monster-800"),
            ("npcs", "npc-3-talk-close"),
        ] {
            assert!(
                selected(name, family, &changes, &[0, 10, 20]),
                "{family}/{name}"
            );
        }
        for (family, name) in [
            ("items", "item-12-equip"),
            ("skills", "class-0-skill-8-self"),
            ("anims", "monster-2"),
            ("anims", "class-0-attack1"),
            ("items", "recipe-5"),
        ] {
            assert!(
                !selected(name, family, &changes, &[0, 10, 20]),
                "{family}/{name}"
            );
        }
    }
}
