//! Invented tables for the state, skill and trigger editors.
#[path = "../workbench_support/mod.rs"]
mod workbench_support;
use d2_data::{
    bin::DataRules,
    patch::{self, PatchTable},
};
use mod_studio::tables::Tables;
use std::{collections::BTreeMap, path::Path};

pub const STATES: &str = include_str!("../../../../templates/proc-charges/files/states.toml");
pub const SKILLS: &str = include_str!("../../../../templates/proc-charges/files/skills.toml");
pub const TRIGGERS: &str = include_str!("../../../../templates/proc-charges/files/triggers.toml");

/// Replaces `table` with `count` rows keyed `<prefix>N`, naming a few of them.
fn fill(
    tables: &mut Tables,
    table: &str,
    count: usize,
    prefix: &str,
    named: &[(usize, &str)],
    extra: &[(usize, &str, &str)],
) {
    let rule = patch::rules_for(DataRules::Mod)
        .into_iter()
        .find(|r| r.name == table)
        .unwrap();
    let key = String::from_utf8_lossy(rule.key_field.as_ref().unwrap()).into_owned();
    let mut columns: Vec<String> = mod_studio::tables::schemas()[table]
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    columns.retain(|c| seen.insert(c.to_ascii_lowercase()));
    if !columns.iter().any(|c| c.eq_ignore_ascii_case(&key)) {
        columns.push(key.clone());
    }
    let named: BTreeMap<usize, &str> = named.iter().copied().collect();
    let mut text = format!("{}\r\n", columns.join("\t"));
    for r in 0..count {
        let cells: Vec<String> = columns
            .iter()
            .map(|c| {
                if c.eq_ignore_ascii_case(&key) {
                    named
                        .get(&r)
                        .map_or_else(|| format!("{prefix}{r}"), |n| (*n).to_owned())
                } else {
                    extra
                        .iter()
                        .find(|(row, col, _)| *row == r && col.eq_ignore_ascii_case(c))
                        .map_or(String::new(), |(_, _, v)| (*v).to_owned())
                }
            })
            .collect();
        text.push_str(&format!("{}\r\n", cells.join("\t")));
    }
    let index = tables
        .edited
        .tables
        .iter()
        .position(|t| t.name() == table)
        .unwrap();
    let new = PatchTable::from_base(&rule, "synthetic", text.as_bytes()).unwrap();
    tables.baseline.tables[index] = new.clone();
    tables.edited.tables[index] = new;
}

/// Install-shaped tables with the rows the proc-charges template names.
pub fn tables(path: &Path) -> Tables {
    let mut tables = Tables::from_data(
        workbench_support::snapshot().data,
        path.join("files/patches/~studio.d2patch"),
    );
    fill(
        &mut tables,
        "states",
        192,
        "state_",
        &[(190, "reaction_window"), (191, "charges")],
        &[],
    );
    fill(
        &mut tables,
        "skills",
        394,
        "skill_",
        &[
            (390, "ember_lash"),
            (391, "frost_nova"),
            (392, "quick_strike"),
            (393, "opportunist"),
        ],
        &[
            (390, "srvdofunc", "400"),
            (391, "srvdofunc", "400"),
            (392, "srvdofunc", "400"),
            (390, "Param1", "4"),
        ],
    );
    fill(
        &mut tables,
        "itemstatcost",
        341,
        "stat_",
        &[
            (21, "test_stat_21"),
            (25, "item_maxdamage_percent"),
            (340, "item_damagepercent"),
        ],
        &[],
    );
    fill(
        &mut tables,
        "overlay",
        20,
        "overlay_",
        &[(17, "smolder")],
        &[],
    );
    fill(
        &mut tables,
        "missiles",
        10,
        "missile_",
        &[(3, "firebolt")],
        &[],
    );
    fill(
        &mut tables,
        "monstats",
        6,
        "monster_",
        &[(1, "skeleton")],
        &[],
    );
    tables.dirty = false;
    tables
}
