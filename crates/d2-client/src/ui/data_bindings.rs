//! Named bindings are resolved against the effective tables once at load, then
//! read only from the server's client model during drawing.
use crate::assets::path::FileSource;
use crate::bridge::world::ClientWorld;
use d2_modkit::ui::Panel;
use std::collections::BTreeMap;

pub(crate) fn names(
    source: &dyn FileSource,
    table: &str,
    column: &str,
) -> Result<BTreeMap<String, u16>, String> {
    if table == "itemstatcost" {
        return d2_server::ui_actions::stat_names(source);
    }
    let path = format!("data\\global\\excel\\{table}.txt");
    let bytes = source
        .read_file(&path)
        .ok_or_else(|| format!("bindings need {path}"))??;
    let t = d2_data::txt::TxtTable::parse(&path, &bytes).map_err(|e| e.to_string())?;
    let col = t
        .header
        .iter()
        .position(|h| h.eq_ignore_ascii_case(column.as_bytes()))
        .ok_or_else(|| format!("{path}: missing {column}"))?;
    // The compiled name linker assigns IDs; decorative TXT ID columns do not.
    let mut linker = d2_data::compile::NameLinker::default();
    let mut names = BTreeMap::new();
    for row in &t.records {
        let key = d2_data::compile::name_key(&row.cells[col])
            .ok_or_else(|| format!("{path}: non-ASCII name"))?;
        let n = linker.find_or_add(&key);
        if key.is_empty() {
            continue;
        }
        let name = String::from_utf8(key).map_err(|e| e.to_string())?;
        let n = u16::try_from(n)
            .ok()
            .filter(|n| *n != u16::MAX)
            .ok_or_else(|| format!("{path}: name ID space exhausted"))?;
        if names.insert(name.clone(), n).is_some() {
            return Err(format!("{path}: duplicate name {name}"));
        }
    }
    Ok(names)
}

/// Resolve each referenced category lazily: an unmodified game reads no extra tables.
pub fn resolve(source: &dyn FileSource, panel: &mut Panel) -> Result<(), String> {
    let uses_network = panel.elements.iter().any(|e| {
        e.action.as_ref().is_some_and(|a| a.starts_with("net."))
            || e.binding.as_ref().is_some_and(|b| {
                std::iter::once(&b.source)
                    .chain(b.maximum_source.iter())
                    .any(|s| s.starts_with("sync."))
            })
    });
    if uses_network {
        d2_server::mod_network::definition(source)?.resolve_panel(panel)?;
    }
    let mut stats = None;
    let mut states = None;
    for element in &mut panel.elements {
        let Some(binding) = &mut element.binding else {
            continue;
        };
        for reference in
            std::iter::once(&mut binding.source).chain(binding.maximum_source.iter_mut())
        {
            let (kind, name) = reference.split_once('.').ok_or("invalid binding")?;
            let id = match kind {
                "stat" => {
                    if stats.is_none() {
                        stats = Some(names(source, "itemstatcost", "stat")?);
                    }
                    Some(
                        d2_server::ui_actions::resolve_name(stats.as_ref().unwrap(), name)
                            .ok_or_else(|| format!("unknown stat {name}"))?,
                    )
                }
                "state" | "stacks" => {
                    if states.is_none() {
                        states = Some(names(source, "states", "state")?);
                    }
                    Some(
                        *states
                            .as_ref()
                            .unwrap()
                            .get(
                                &name
                                    .to_ascii_lowercase()
                                    .chars()
                                    .take(31)
                                    .collect::<String>(),
                            )
                            .ok_or_else(|| format!("unknown state {name}"))?,
                    )
                }
                "quest" => {
                    if !panel.quests.iter().any(|q| q.name == name) {
                        return Err(format!(
                            "unknown quest flag {name}; declare its slot and bit"
                        ));
                    }
                    None
                }
                "var" | "sync" => None,
                _ => return Err("unknown binding category".into()),
            };
            if let Some(id) = id {
                *reference = format!("{kind}.{id}");
            }
        }
    }
    Ok(())
}

pub fn read(world: &ClientWorld, panel: &Panel, source: &str) -> Option<i32> {
    let (kind, name) = source.split_once('.')?;
    if kind == "sync" {
        return world.synced_values.get(name).copied();
    }
    if kind == "var" {
        return world.ui_variables.get(name).copied();
    }
    if kind == "quest" {
        let q = panel.quests.iter().find(|q| q.name == name)?;
        let record = world.quest_player?;
        let i = usize::from(q.slot) * 2;
        let word = u16::from_le_bytes([record[i], record[i + 1]]);
        return Some(i32::from(word & (1 << q.bit) != 0));
    }
    let id = name.parse().ok()?;
    let player = world.local()?;
    match kind {
        "stat" => Some(player.stat(id)),
        "state" => Some(i32::from(player.states.contains(&id))),
        "stacks" => Some(i32::from(
            player.state_stacks.get(&id).copied().unwrap_or(0),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Files;
    impl FileSource for Files {
        fn read_file(&self, name: &str) -> Option<Result<Vec<u8>, String>> {
            let text = if name.ends_with("itemstatcost.txt") {
                "Stat\tID\r\nstrength\t999\r\nRage\t511\r\n"
            } else {
                "state\tID\r\nCharged\t380\r\n"
            };
            Some(Ok(text.as_bytes().to_vec()))
        }
    }
    #[test]
    fn names_use_effective_ids_and_unknown_names_fail() {
        let text = "version=1\nname='hud'\n[[elements]]\nid='rage'\nkind='bar'\n[elements.bind]\nsource='stat.RAGE'\nmaximum_source='stacks.charged'\n";
        let mut p = Panel::parse(text).unwrap();
        resolve(&Files, &mut p).unwrap();
        let b = p.elements[0].binding.as_ref().unwrap();
        assert_eq!(b.source, "stat.1");
        assert_eq!(b.maximum_source.as_deref(), Some("stacks.0"));
        assert!(resolve(
            &Files,
            &mut Panel::parse(&text.replace("stat.RAGE", "stat.typo")).unwrap()
        )
        .is_err());
    }
    #[test]
    fn missing_values_stay_missing_and_quests_and_variables_read_server_data() {
        let p =
            Panel::parse("version=1\nname='hud'\n[[quests]]\nname='den.complete'\nslot=1\nbit=0")
                .unwrap();
        let mut world = ClientWorld::default();
        assert_eq!(read(&world, &p, "quest.den.complete"), None);
        let mut record = [0; 96];
        record[2] = 1;
        world.quest_player = Some(record);
        assert_eq!(read(&world, &p, "quest.den.complete"), Some(1));
        assert_eq!(read(&world, &p, "var.combat/rage"), None);
        world.ui_variables.insert("combat/rage".into(), 12);
        assert_eq!(read(&world, &p, "var.combat/rage"), Some(12));
        let key = crate::bridge::world::UnitKey::new(0, 1);
        let mut player = crate::bridge::world::ClientUnit::new(key);
        player.stats.insert(511, 42);
        player.states.insert(380);
        player.state_stacks.insert(380, 3);
        world.local_player = Some(key);
        world.units.insert(key, player);
        assert_eq!(read(&world, &p, "stat.511"), Some(42));
        assert_eq!(read(&world, &p, "state.380"), Some(1));
        assert_eq!(read(&world, &p, "stacks.380"), Some(3));
    }
    #[test]
    fn network_bindings_resolve_declared_names_and_read_only_synced_values() {
        struct NetFiles;
        impl FileSource for NetFiles {
            fn read_file(&self, _: &str) -> Option<Result<Vec<u8>, String>> {
                panic!("synced names do not require client table reads");
            }
            fn net_document(&self) -> Result<Option<Vec<u8>>, String> {
                Ok(Some(b"version=1\n[[values]]\nid=1\nname='rage'\nsource='var.combat/rage'\n[[values]]\nid=2\nname='maximum'\nsource='stat.maxrage'".to_vec()))
            }
        }
        let text="version=1\nname='hud'\n[[elements]]\nid='rage'\nkind='bar'\n[elements.bind]\nsource='sync.rage'\nmaximum_source='sync.maximum'";
        let mut p = Panel::parse(text).unwrap();
        resolve(&NetFiles, &mut p).unwrap();
        let mut world = ClientWorld::default();
        assert_eq!(read(&world, &p, "sync.rage"), None);
        world.ui_variables.insert("rage".into(), 99);
        assert_eq!(read(&world, &p, "sync.rage"), None);
        world.synced_values.insert("rage".into(), 42);
        assert_eq!(read(&world, &p, "sync.rage"), Some(42));
        assert!(resolve(
            &NetFiles,
            &mut Panel::parse(&text.replace("sync.rage", "sync.missing")).unwrap()
        )
        .is_err());
        assert!(resolve(
            &NetFiles,
            &mut Panel::parse(&text.replace("sync.maximum", "sync.missing")).unwrap()
        )
        .is_err());
    }
    #[test]
    fn custom_stat_bindings_keep_wide_ids_and_full_names() {
        struct Custom;
        impl FileSource for Custom {
            fn read_file(&self, path: &str) -> Option<Result<Vec<u8>, String>> {
                Files.read_file(path)
            }
            fn custom_stat_names(&self) -> Result<BTreeMap<String, u16>, String> {
                Ok(BTreeMap::from([(
                    "a_long_custom_stat_name_over_thirty_one_bytes".into(),
                    40000,
                )]))
            }
        }
        let mut panel = Panel::parse("version=1\nname='hud'\n[[elements]]\nid='value'\nkind='text'\n[elements.bind]\nsource='stat.A_LONG_CUSTOM_STAT_NAME_OVER_THIRTY_ONE_BYTES'").unwrap();
        resolve(&Custom, &mut panel).unwrap();
        assert_eq!(
            panel.elements[0].binding.as_ref().unwrap().source,
            "stat.40000"
        );
        let key = crate::bridge::world::UnitKey::new(0, 1);
        let mut player = crate::bridge::world::ClientUnit::new(key);
        player.stats.insert(40000, 17);
        let mut world = ClientWorld {
            local_player: Some(key),
            ..Default::default()
        };
        world.units.insert(key, player);
        assert_eq!(read(&world, &panel, "stat.40000"), Some(17));
    }
}
