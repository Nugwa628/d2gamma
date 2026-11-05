//! Server-owned declarations behind the LCUI action service.
use crate::file_source::FileSource;
use d2_modkit::ui::{Action, Panel};
use d2_sim::rules::MessageHost;
use d2_sim::units::{UnitId, UnitType};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct UiActions {
    actions: BTreeMap<String, Resolved>,
    last: BTreeMap<(UnitId, String), i32>,
}
struct Resolved {
    definition: Action,
    stat: u16,
    cost_stat: Option<u16>,
}

impl UiActions {
    pub fn load(source: &dyn FileSource) -> Result<Self, String> {
        let mut definitions = Vec::new();
        for (path, bytes) in source.server_ui_documents()? {
            let panel =
                Panel::parse(std::str::from_utf8(&bytes).map_err(|e| format!("{path}: {e}"))?)
                    .map_err(|e| format!("{path}: {e}"))?;
            definitions.extend(
                panel
                    .actions
                    .into_iter()
                    .map(|a| (format!("{}/{}", panel.name, a.name), a)),
            );
        }
        Self::declared(source, definitions)
    }
    pub fn declared(
        source: &dyn FileSource,
        definitions: Vec<(String, Action)>,
    ) -> Result<Self, String> {
        let mut result = Self::default();
        if definitions.is_empty() {
            return Ok(result);
        }
        let names = stat_names(source)?;
        for (key, a) in definitions {
            let stat = resolve_name(&names, &a.stat)
                .ok_or_else(|| format!("{key}: unknown action stat {}", a.stat))?;
            let cost_stat = a
                .cost_stat
                .as_ref()
                .map(|n| {
                    resolve_name(&names, n).ok_or_else(|| format!("{key}: unknown cost stat {n}"))
                })
                .transpose()?;
            if cost_stat == Some(stat) {
                return Err(format!("{key}: action and cost must use different stats"));
            }
            if result
                .actions
                .insert(
                    key.clone(),
                    Resolved {
                        definition: a,
                        stat,
                        cost_stat,
                    },
                )
                .is_some()
            {
                return Err(format!("duplicate action {key}"));
            }
            if result.actions.len() > 256 {
                return Err("at most 256 action declarations".into());
            }
        }
        Ok(result)
    }

    pub fn validate_scripts(
        &self,
        scripts: Option<&crate::scripts::ScriptHost>,
    ) -> Result<(), String> {
        for (key, a) in &self.actions {
            if let Some(v) = &a.definition.validator {
                if !scripts.is_some_and(|s| s.export(&v.module, &v.function).is_ok()) {
                    return Err(format!(
                        "{key}: unknown validator {}::{}",
                        v.module, v.function
                    ));
                }
            }
        }
        Ok(())
    }

    /// The client supplies only an action name; identity and every value come
    /// from the authoritative world. All checks finish before the first write.
    pub fn execute(
        &mut self,
        key: &str,
        player: UnitId,
        frame: i32,
        h: &mut dyn MessageHost,
        scripts: Option<&crate::scripts::ScriptHost>,
    ) -> bool {
        let Some(a) = self.actions.get(key) else {
            return false;
        };
        if h.unit_type(player) != Some(UnitType::Player) {
            return false;
        }
        let Some(query) = h.unit_query() else {
            return false;
        };
        if a.definition.town && !query.in_town(player) {
            return false;
        }
        let current = query.base_stat(player, a.stat, 0);
        let cost = a.cost_stat.map(|id| query.base_stat(player, id, 0));
        let cooldown_key = (player, key.to_owned());
        if self
            .last
            .get(&cooldown_key)
            .is_some_and(|last| frame.wrapping_sub(*last) < i32::from(a.definition.cooldown))
        {
            return false;
        }
        self.last.insert(cooldown_key, frame);
        let Some(next) = current.checked_add(a.definition.delta) else {
            return false;
        };
        if current < a.definition.minimum
            || current > a.definition.maximum
            || next < a.definition.minimum
            || next > a.definition.maximum
            || cost.is_some_and(|v| v < a.definition.cost)
        {
            return false;
        }
        if let Some(validator) = &a.definition.validator {
            if !scripts.is_some_and(|s| {
                s.call(&validator.module, &validator.function, current)
                    .is_ok_and(|v| v > 0)
            }) {
                return false;
            }
        }
        if let (Some(id), Some(value)) = (a.cost_stat, cost) {
            h.set_stat(player, id, value - a.definition.cost);
        }
        h.set_stat(player, a.stat, next);
        true
    }
    pub fn forget(&mut self, player: UnitId) {
        self.last.retain(|(p, _), _| *p != player);
    }
}

/// The effective compiled table names plus the mod's explicit wide stat ids.
pub fn stat_names(source: &dyn FileSource) -> Result<BTreeMap<String, u16>, String> {
    let path = "data\\global\\excel\\itemstatcost.txt";
    let bytes = source
        .read_file(path)
        .ok_or("UI actions need itemstatcost.txt")??;
    let table = d2_data::txt::TxtTable::parse(path, &bytes).map_err(|e| e.to_string())?;
    let name = table
        .header
        .iter()
        .position(|h| h.eq_ignore_ascii_case(b"stat"))
        .ok_or("itemstatcost missing Stat")?;
    let mut linker = d2_data::compile::NameLinker::default();
    let mut names = BTreeMap::new();
    for row in table.records {
        let key = d2_data::compile::name_key(&row.cells[name]).ok_or("non-ASCII stat name")?;
        let id = linker.find_or_add(&key);
        if key.is_empty() {
            continue;
        }
        let n = String::from_utf8(key).map_err(|e| e.to_string())?;
        let i = u16::try_from(id)
            .ok()
            .filter(|id| *id != u16::MAX)
            .ok_or("stat ID space exhausted")?;
        if names.insert(n.clone(), i).is_some() {
            return Err(format!("duplicate stat {n}"));
        }
    }
    for (name, id) in source.custom_stat_names()? {
        if names.insert(name.clone(), id).is_some() {
            return Err(format!("custom stat collides with table stat {name}"));
        }
    }
    Ok(names)
}

/// Custom names can contain 63 bytes; table linkers keep their 31-byte key.
pub fn resolve_name(names: &BTreeMap<String, u16>, name: &str) -> Option<u16> {
    let name = name.to_ascii_lowercase();
    names
        .get(&name)
        .or_else(|| names.get(&name.chars().take(31).collect::<String>()))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2_sim::rules::{EventHost, UnitQuery};
    #[derive(Default)]
    struct World {
        stats: BTreeMap<u16, i32>,
        writes: usize,
        town: bool,
    }
    impl UnitQuery for World {
        fn stat(&self, _: UnitId, stat: u16, _: u16) -> i32 {
            self.stats.get(&stat).copied().unwrap_or(0)
        }
        fn base_stat(&self, u: UnitId, s: u16, l: u16) -> i32 {
            UnitQuery::stat(self, u, s, l)
        }
        fn skill_level(&self, _: UnitId, _: u16, _: bool) -> i32 {
            0
        }
        fn has_state(&self, _: UnitId, _: u16) -> bool {
            false
        }
        fn level_id(&self, _: UnitId) -> Option<u32> {
            Some(1)
        }
        fn act(&self, _: UnitId) -> Option<u8> {
            Some(0)
        }
        fn in_town(&self, _: UnitId) -> bool {
            self.town
        }
        fn class(&self, u: UnitId) -> Option<(UnitType, u32)> {
            (u == UnitId(1)).then_some((UnitType::Player, 0))
        }
        fn difficulty(&self) -> u8 {
            0
        }
    }
    impl EventHost for World {
        fn game(&mut self) -> &mut d2_sim::game::Game {
            panic!("actions must use the authoritative world capabilities")
        }
        fn unit_type(&self, u: UnitId) -> Option<UnitType> {
            self.class(u).map(|(t, _)| t)
        }
        fn class_id(&self, _: UnitId) -> i32 {
            0
        }
        fn stat(&self, u: UnitId, s: u16, l: u16) -> i32 {
            UnitQuery::stat(self, u, s, l)
        }
        fn set_stat(&mut self, _: UnitId, s: u16, v: i32) {
            self.stats.insert(s, v);
            self.writes += 1;
        }
        fn has_state(&self, _: UnitId, _: u16) -> bool {
            false
        }
        fn seed(&mut self, _: UnitId) -> Option<&mut d2_sim::rng::Seed> {
            None
        }
        fn level(&self, _: UnitId) -> Option<u32> {
            Some(1)
        }
        fn send(&mut self, _: UnitId, _: &[u8]) {}
        fn unit_query(&self) -> Option<&dyn UnitQuery> {
            Some(self)
        }
    }
    impl MessageHost for World {}
    struct Files(String);
    impl FileSource for Files {
        fn ui_documents(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
            Ok(vec![("ui/test.toml".into(), self.0.as_bytes().to_vec())])
        }
        fn read_file(&self, _: &str) -> Option<Result<Vec<u8>, String>> {
            Some(Ok(b"Stat\tID\r\nstrength\t0\r\npoints\t1\r\n".to_vec()))
        }
    }
    fn source(extra: &str) -> Files {
        Files(format!("version=1\nname='test'\n[[actions]]\nname='allocate'\nstat='strength'\ndelta=1\nmaximum=100\ncost_stat='points'\ncost=1\ncooldown=25\n{extra}"))
    }
    #[test]
    fn actions_charge_and_mutate_only_after_every_authoritative_check() {
        let mut actions = UiActions::load(&source("town=true")).unwrap();
        let mut world = World::default();
        world.stats.insert(1, 2);
        assert!(!actions.execute("test/allocate", UnitId(2), 0, &mut world, None));
        assert!(!actions.execute("test/allocate", UnitId(1), 0, &mut world, None));
        world.town = true;
        assert!(actions.execute("test/allocate", UnitId(1), 0, &mut world, None));
        assert_eq!((world.stats[&0], world.stats[&1], world.writes), (1, 1, 2));
        assert!(!actions.execute("test/allocate", UnitId(1), 0, &mut world, None));
        assert!(actions.execute("test/allocate", UnitId(1), 25, &mut world, None));
        assert!(!actions.execute("test/allocate", UnitId(1), 50, &mut world, None));
        assert_eq!(world.writes, 4);
        assert!(!UiActions::default().execute("test/allocate", UnitId(1), 100, &mut world, None));
    }
    #[test]
    fn script_denial_missing_script_limits_and_overflow_leave_both_stats_unchanged() {
        for (current, extra) in [
            (100, ""),
            (i32::MAX, ""),
            (
                0,
                "[actions.validator]\nmodule='missing.luau'\nfunction='allow'",
            ),
        ] {
            let mut actions = UiActions::load(&source(extra)).unwrap();
            let mut world = World::default();
            world.stats.insert(0, current);
            world.stats.insert(1, 2);
            assert!(!actions.execute("test/allocate", UnitId(1), 0, &mut world, None));
            assert_eq!(
                (world.stats[&0], world.stats[&1], world.writes),
                (current, 2, 0)
            );
        }
        assert!(UiActions::load(&Files(
            source("").0.replace("stat='strength'", "stat='typo'")
        ))
        .is_err());
    }
    #[test]
    fn validators_are_linked_and_run_before_costs_with_fail_closed_errors() {
        use d2_native::layers::{Layer, LayerStack};
        let root = std::env::temp_dir().join(format!("ui-validator-{}", std::process::id()));
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(root.join("scripts/test.luau"), "return { allow = function(value) return value < 2 and 1 or 0 end, fail = function() error('denied') end }").unwrap();
        let host = crate::scripts::Sources::from_layers(&LayerStack {
            layers: vec![
                Layer::dir("base", root.join("unused"), None),
                Layer::dir("mod", root.clone(), None),
            ],
        })
        .unwrap()
        .instantiate()
        .unwrap()
        .unwrap();
        let mut actions = UiActions::load(&source(
            "[actions.validator]\nmodule='test.luau'\nfunction='allow'",
        ))
        .unwrap();
        assert!(actions.validate_scripts(Some(&host)).is_ok());
        assert!(actions.validate_scripts(None).is_err());
        let mut world = World::default();
        world.stats.insert(1, 4);
        assert!(actions.execute("test/allocate", UnitId(1), 0, &mut world, Some(&host)));
        assert!(actions.execute("test/allocate", UnitId(1), 25, &mut world, Some(&host)));
        assert!(!actions.execute("test/allocate", UnitId(1), 50, &mut world, Some(&host)));
        assert_eq!((world.stats[&0], world.stats[&1], world.writes), (2, 2, 4));
        let mut bad = UiActions::load(&source(
            "[actions.validator]\nmodule='test.luau'\nfunction='fail'",
        ))
        .unwrap();
        assert!(!bad.execute("test/allocate", UnitId(1), 100, &mut world, Some(&host)));
        assert_eq!(world.writes, 4);
        bad.actions
            .get_mut("test/allocate")
            .unwrap()
            .definition
            .validator
            .as_mut()
            .unwrap()
            .function = "typo".into();
        assert!(bad.validate_scripts(Some(&host)).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn network_outbound_fairness_eventually_delivers_all_values_under_churn() {
        struct Net(String);
        impl FileSource for Net {
            fn read_file(&self, _: &str) -> Option<Result<Vec<u8>, String>> {
                None
            }
            fn net_document(&self) -> Result<Option<Vec<u8>>, String> {
                Ok(Some(self.0.as_bytes().to_vec()))
            }
        }
        let text = format!(
            "version=1\n{}",
            (1..=256)
                .map(|id| format!("[[values]]\nid={id}\nname='v{id}'\nsource='var.module/v{id}'\n"))
                .collect::<String>()
        );
        let source = Net(text);
        let mut n = crate::mod_network::Network::load(&source).unwrap();
        let hash = crate::mod_network::identity(&source, &n.definition);
        let p = UnitId(1);
        let ack = n
            .hello(p, 0, 25, &d2_proto::mod_ui::net_identity(false, &hash))
            .unwrap();
        n.queued(
            p,
            &d2_proto::envelope::encode(d2_proto::envelope::Direction::ToClient, &ack).unwrap(),
        );
        let world = World::default();
        let mut seen = std::collections::BTreeSet::new();
        for frame in 0..=60 {
            for id in 1..=128 {
                n.observe_variable(p, &format!("module/v{id}"), frame);
            }
            let messages = n.values(p, frame, 25, &world);
            assert!(messages.len() <= 128);
            for bytes in messages {
                let (id, _) =
                    d2_proto::envelope::payload(d2_proto::envelope::Direction::ToClient, &bytes)
                        .and_then(d2_proto::mod_ui::read_synced)
                        .unwrap();
                seen.insert(id);
                n.queued(p, &bytes);
            }
        }
        assert_eq!(seen.len(), 256);
        n.restart(p);
        assert!(!n.ready(p));
        assert!(n.values(p, 100, 25, &world).is_empty());
    }
    #[test]
    fn custom_stats_work_in_network_values_actions_and_legacy_actions() {
        struct Custom;
        impl FileSource for Custom {
            fn read_file(&self, _: &str) -> Option<Result<Vec<u8>, String>> {
                Some(Ok(b"Stat\tID\r\nstrength\t99\r\npoints\t1\r\n".to_vec()))
            }
            fn custom_stat_names(&self) -> Result<BTreeMap<String, u16>, String> {
                Ok(BTreeMap::from([
                    ("resonance".into(), 40000),
                    (
                        "a_long_custom_stat_name_over_thirty_one_bytes".into(),
                        40001,
                    ),
                ]))
            }
            fn ui_documents(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
                Ok(vec![("ui/hud.toml".into(), b"version=1\nname='hud'\n[[actions]]\nname='charge'\nstat='resonance'\ndelta=1\nmaximum=2".to_vec())])
            }
            fn net_document(&self) -> Result<Option<Vec<u8>>, String> {
                Ok(Some(b"version=1\n[[values]]\nid=1\nname='charge'\nsource='stat.resonance'\n[[values]]\nid=2\nname='long'\nsource='stat.a_long_custom_stat_name_over_thirty_one_bytes'\n[[actions]]\nid=1\nname='charge'\nstat='RESONANCE'\ndelta=1\nmaximum=2".to_vec()))
            }
        }
        let source = Custom;
        let names = stat_names(&source).unwrap();
        assert_eq!(
            resolve_name(&names, "strength"),
            Some(0),
            "compiled id, not decorative TXT id"
        );
        assert_eq!(
            resolve_name(&names, "a_long_custom_stat_name_over_thirty_one_bytes"),
            Some(40001)
        );
        let mut world = World::default();
        let p = UnitId(1);
        let mut legacy = UiActions::load(&source).unwrap();
        assert!(legacy.execute("hud/charge", p, 0, &mut world, None));
        let mut net = crate::mod_network::Network::load(&source).unwrap();
        let hash = crate::mod_network::identity(&source, &net.definition);
        let ack = net
            .hello(p, 0, 25, &d2_proto::mod_ui::net_identity(false, &hash))
            .unwrap();
        net.queued(
            p,
            &d2_proto::envelope::encode(d2_proto::envelope::Direction::ToClient, &ack).unwrap(),
        );
        assert!(net.execute(1, p, 1, &mut world, None));
        assert!(!net.execute(1, p, 2, &mut world, None));
        assert_eq!(world.stats[&40000], 2);
        let values: Vec<_> = net
            .values(p, 3, 25, &world)
            .iter()
            .filter_map(|b| {
                d2_proto::envelope::payload(d2_proto::envelope::Direction::ToClient, b)
                    .and_then(d2_proto::mod_ui::read_synced)
            })
            .collect();
        assert!(values.contains(&(1, 2)), "{values:?}");
        assert!(values.contains(&(2, 0)), "{values:?}");
    }
}
