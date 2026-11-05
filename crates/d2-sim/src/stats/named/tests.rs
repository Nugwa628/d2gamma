//! Validation and real-list propagation of named stat behaviors.
use super::*;
use crate::stats::{
    key,
    lists::{flag, NoHost},
    StatLists,
};
use crate::units::{UnitId, UnitType};
use std::sync::Arc;

fn lists(text: &str) -> (StatRules, StatLists, UnitId) {
    let rules = StatRules::from_toml(text).unwrap();
    let mut data = (*crate::stats::tests::data()).clone();
    rules.install(&mut data.stats).unwrap();
    let mut lists = StatLists::new(Arc::new(data));
    let unit = UnitId(1);
    lists.alloc_extended(&mut NoHost, unit, UnitType::Player, 1, 0, 0, None);
    (rules, lists, unit)
}
const DECLARE: &str = "version=1\n[[stat]]\nname='focus'\n[[stat]]\nname='ward'\n";

#[test]
fn names_resolve_without_numbers_and_assignments_ignore_declaration_order() {
    let a = StatRules::from_toml(DECLARE).unwrap();
    let b = StatRules::from_toml("[[stat]]\nname='ward'\n[[stat]]\nname='focus'\n").unwrap();
    assert_eq!(a.custom, b.custom);
    assert_eq!(a.id("Focus"), Some(FIRST_CUSTOM_STAT));
    assert_eq!(a.id("life"), Some(6));
    assert_eq!(a.id("armorclass"), Some(31));
    let text = a
        .resolve_fields("[[state]]\nid=30\n[[state.effect]]\nstat='focus'\nper_stack=3\n")
        .unwrap();
    let states = crate::rules::StateRules::from_toml(&text).unwrap();
    assert_eq!(states.get(30).unwrap().effects[0].stat, FIRST_CUSTOM_STAT);
}

#[test]
fn add_and_remove_a_custom_equipment_stat_refreshes_derived_totals() {
    let text = format!(
        "{DECLARE}[[behavior]]\nkind='add'\nsource='focus'\ntarget='armorclass'\npercent=200\n"
    );
    let (rules, mut lists, u) = lists(&text);
    lists.unit_set(&mut NoHost, u, 31, 10, 0);
    let item = lists.alloc(flag::BASIC, 0, 4, 2);
    lists.set(&mut NoHost, item, rules.id("focus").unwrap(), 7, 0, None);
    lists.attach(&mut NoHost, u, item, true);
    assert_eq!(lists.unit_total(u, 31, 0), 24);
    lists.set(&mut NoHost, item, rules.id("focus").unwrap(), 0, 0, None);
    assert_eq!(lists.unit_total(u, 31, 0), 10);
    lists.set(&mut NoHost, item, rules.id("focus").unwrap(), 9, 0, None);
    assert_eq!(lists.unit_total(u, 31, 0), 28);
    lists.detach(&mut NoHost, item);
    assert_eq!(lists.unit_total(u, 31, 0), 10);
}

#[test]
fn conversion_consumes_a_fraction_once_and_respects_fixed_point_units() {
    let text = format!(
        "{DECLARE}[[behavior]]\nkind='convert'\nsource='maxmana'\ntarget='ward'\npercent=25\n"
    );
    let (rules, mut lists, u) = lists(&text);
    lists.unit_set(&mut NoHost, u, 9, 100 * 256, 0);
    assert_eq!(lists.unit_total(u, 9, 0), 75 * 256);
    assert_eq!(lists.unit_total(u, rules.id("ward").unwrap(), 0), 25);
    lists.unit_set(&mut NoHost, u, 9, 80 * 256, 0);
    assert_eq!(lists.unit_total(u, 9, 0), 60 * 256);
    assert_eq!(lists.unit_total(u, rules.id("ward").unwrap(), 0), 20);
    lists.unit_set(&mut NoHost, u, 9, 0, 0);
    assert_eq!(lists.unit_total(u, rules.id("ward").unwrap(), 0), 0);
}

#[test]
fn percent_of_reads_pre_behavior_values_and_refreshes_both_inputs() {
    let text = format!(
        "{DECLARE}[[behavior]]\nkind='percent_of'\nsource='focus'\nof='maxmana'\ntarget='ward'\n"
    );
    let (rules, mut lists, u) = lists(&text);
    let focus = rules.id("focus").unwrap();
    let ward = rules.id("ward").unwrap();
    lists.unit_set(&mut NoHost, u, 9, 80 * 256, 0);
    lists.unit_set(&mut NoHost, u, focus, 25, 0);
    assert_eq!(lists.unit_total(u, ward, 0), 20);
    lists.unit_set(&mut NoHost, u, 9, 120 * 256, 0);
    assert_eq!(lists.unit_total(u, ward, 0), 30);
    lists.unit_set(&mut NoHost, u, focus, 50, 0);
    assert_eq!(lists.unit_total(u, ward, 0), 60);
    let list = lists.unit_list(u).unwrap();
    assert_eq!(lists.eval(&NoHost, list, key(ward, 0)), 60);
}

#[test]
fn behavior_order_and_mutual_references_do_not_recurse() {
    let operations = [
        "[[behavior]]\nkind='add'\nsource='focus'\ntarget='ward'\n",
        "[[behavior]]\nkind='add'\nsource='ward'\ntarget='focus'\n",
    ];
    for order in [&[0, 1][..], &[1, 0][..]] {
        let text = format!("{DECLARE}{}{}", operations[order[0]], operations[order[1]]);
        let (rules, mut lists, u) = lists(&text);
        lists.unit_set(&mut NoHost, u, rules.id("focus").unwrap(), 5, 0);
        lists.unit_set(&mut NoHost, u, rules.id("ward").unwrap(), 7, 0);
        assert_eq!(lists.unit_total(u, rules.id("focus").unwrap(), 0), 12);
        assert_eq!(lists.unit_total(u, rules.id("ward").unwrap(), 0), 12);
    }
}

#[test]
fn invalid_names_versions_fields_references_and_overconversion_are_rejected() {
    for text in [
        "version=2",
        "typo=1",
        "[[stat]]\nname='strength'",
        "[[stat]]\nname='a'\n[[stat]]\nname='A'",
        "[[stat]]\nname='a-b'",
        "[[stat]]\nname=''",
        "[[stat]]\nname='a'\nid=65535",
        "[[stat]]\nname='a'\nid=7",
        "[[stat]]\nname='a'\nid=40000\n[[stat]]\nname='b'\nid=40000",
        "[[behavior]]\nkind='add'\nsource='missing'\ntarget='armorclass'",
        "[[behavior]]\nkind='convert'\nsource='mana'\ntarget='mana'",
        "[[behavior]]\nkind='percent_of'\nsource='mana'\ntarget='armorclass'",
        "[[behavior]]\nkind='convert'\nsource='mana'\ntarget='armorclass'\npercent=101",
    ] {
        assert!(StatRules::from_toml(text).is_err(), "{text}");
    }
    let oversized = (0..257)
        .map(|i| format!("[[stat]]\nname='s{i}'\n"))
        .collect::<String>();
    assert!(StatRules::from_toml(&oversized).is_err());
}

#[test]
fn explicit_ids_are_stable_and_table_install_does_not_replace_rows() {
    let a = StatRules::from_toml("[[stat]]\nname='ward'\nid=40000").unwrap();
    assert_eq!(a.id("ward"), Some(40000));
    let mut table = crate::stats::tests::data().stats.clone();
    let before = table.clone();
    StatRules::default().install(&mut table).unwrap();
    assert_eq!(table, before);
    assert!(StatRules::default()
        .resolve_fields("stat='missing'")
        .is_err());
    assert_eq!(scaled(i32::MAX, 10000, 0, 8), i32::MAX);
    assert_eq!(scaled(-5, 25, 0, 0), -1);
}

#[test]
fn empty_mod_stat_rules_keep_native_totals_and_keys_identical() {
    let (_, mut lists, u) = lists("");
    lists.unit_set(&mut NoHost, u, 0, 10, 0);
    lists.unit_set(&mut NoHost, u, 31, 17, 0);
    assert_eq!(lists.unit_total(u, 0, 0), 10);
    assert_eq!(lists.unit_total(u, 31, 0), 17);
    assert!(lists.data().stats.get(FIRST_CUSTOM_STAT).is_none());
}

#[test]
fn bounded_behaviors_and_atomic_collision_validation() {
    let oversized =
        "[[behavior]]\nkind='add'\nsource='strength'\ntarget='armorclass'\n".repeat(513);
    assert!(StatRules::from_toml(&oversized).is_err());
    let rules = StatRules::from_toml("[[stat]]\nname='focus'").unwrap();
    let mut table = StatTable::default();
    table.stats.resize(32769, StatInfo::custom());
    let before = table.clone();
    assert!(rules.install(&mut table).is_err());
    assert_eq!(table, before);
    let mut table = StatTable::default();
    rules.install(&mut table).unwrap();
    assert_eq!(table.len(), 32769);
    assert!(!table.is_empty());
    assert!(table.get(32767).is_none());
    assert!(table.get(32768).is_some());
}
