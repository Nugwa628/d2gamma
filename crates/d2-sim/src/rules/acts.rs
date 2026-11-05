//! Ordered act boundaries, towns, waypoint ids and NPC travel for data mods.
use serde::Deserialize;

pub const MAX_ACTS: usize = 128;
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Act {
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub town: u32,
    /// Explicit, stable waypoint bit -> level pairs.
    #[serde(default)]
    pub waypoints: Vec<Waypoint>,
    #[serde(default)]
    pub travel: Vec<Travel>,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Waypoint {
    pub id: u8,
    pub level: u32,
}
/// Runs instead of built-in NPC travel. Quest gates can be supplied later
/// through the existing quest/NPC providers; data travel is unrestricted.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Travel {
    #[serde(default)]
    pub text: u16,
    pub npc: u16,
    pub action: u32,
    pub to: u32,
    #[serde(default)]
    pub tile: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acts {
    rows: Vec<Act>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    acts: Vec<Act>,
}
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ActsError(String);
impl Default for Acts {
    fn default() -> Self {
        let boundaries = crate::drlg::ACT_BOUNDARIES;
        Self {
            rows: (0..5)
                .map(|i| Act {
                    name: format!("Act {}", i + 1),
                    start: boundaries[i],
                    end: boundaries[i + 1],
                    town: crate::drlg::TOWN_LEVELS[i],
                    waypoints: Vec::new(),
                    travel: Vec::new(),
                })
                .collect(),
        }
    }
}
impl Acts {
    pub fn from_toml(text: &str) -> Result<Self, ActsError> {
        let f: File = toml::from_str(text).map_err(|e| ActsError(e.to_string()))?;
        if f.version != 1 || f.acts.is_empty() || f.acts.len() > MAX_ACTS {
            return Err(ActsError(
                "version must be 1; acts needs 1..128 rows".into(),
            ));
        }
        let mut end = 0;
        let mut names = std::collections::BTreeSet::new();
        let mut bits = std::collections::BTreeSet::new();
        let mut levels = std::collections::BTreeSet::new();
        for a in &f.acts {
            if a.name.is_empty()
                || a.name.trim() != a.name
                || a.name.chars().count() > 63
                || a.name.chars().any(char::is_control)
                || !names.insert(a.name.to_lowercase())
                || a.start == 0
                || a.start < end
                || a.start >= a.end
                || a.end > 256
                || !(a.start..a.end).contains(&a.town)
                || a.waypoints.len() > 9
            {
                return Err(ActsError("invalid name, ordered nonoverlapping range, town, or more than 9 waypoints per act".into()));
            }
            end = a.end;
            let mut routes = std::collections::BTreeSet::new();
            let mut menus = std::collections::BTreeMap::<u16, usize>::new();
            for w in &a.waypoints {
                if w.id >= 112
                    || !(a.start..a.end).contains(&w.level)
                    || !bits.insert(w.id)
                    || !levels.insert(w.level)
                {
                    return Err(ActsError(
                        "waypoints need unique ids below 112 and unique levels inside their act"
                            .into(),
                    ));
                }
            }
            for t in &a.travel {
                if t.text != 0 {
                    let n = menus.entry(t.npc).or_default();
                    *n += 1;
                    if *n > 4 {
                        return Err(ActsError(
                            "at most four visible travel rows per NPC per act".into(),
                        ));
                    }
                }
            }
            if a.travel.len() > 256 || a.travel.iter().any(|t| !routes.insert((t.npc, t.action))) {
                return Err(ActsError(
                    "travel needs unique NPC/action pairs and at most 256 routes per act".into(),
                ));
            }
        }
        for a in &f.acts {
            for t in &a.travel {
                if !f.acts.iter().any(|a| (a.start..a.end).contains(&t.to)) {
                    return Err(ActsError("travel destination outside act ranges".into()));
                }
            }
        }
        Ok(Self { rows: f.acts })
    }
    pub fn rows(&self) -> &[Act] {
        &self.rows
    }
    pub fn count(&self) -> usize {
        self.rows.len()
    }
    pub fn find_act(&self, level: u32) -> Option<u8> {
        self.rows
            .iter()
            .position(|a| (a.start..a.end).contains(&level))
            .map(|i| i as u8)
    }
    pub fn act_of_level(&self, level: u32) -> u8 {
        self.find_act(level).unwrap_or(0)
    }
    pub fn town(&self, act: u8) -> Option<u32> {
        self.rows.get(usize::from(act)).map(|a| a.town)
    }
    pub fn towns(&self) -> crate::drlg::TownLevels {
        crate::drlg::TownLevels::new(self.rows.iter().map(|a| a.town))
    }
    pub fn travel(&self, level: u32, npc: u16, action: u32) -> Option<&Travel> {
        self.rows
            .get(usize::from(self.find_act(level)?))?
            .travel
            .iter()
            .find(|t| t.npc == npc && t.action == action)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acts_default_matches_all_legacy_boundaries_and_towns() {
        let a = Acts::default();
        assert_eq!(a.count(), 5);
        for id in 0..=65536 {
            assert_eq!(a.act_of_level(id), crate::drlg::act_of_level(id));
        }
        for id in 0..5 {
            assert_eq!(a.town(id), Some(crate::drlg::TOWN_LEVELS[id as usize]));
        }
        assert!(crate::rules::Ruleset::Original.acts().is_none());
    }
    #[test]
    fn sixth_act_keeps_rooms_inactive_records_and_ticks_separate() {
        use crate::game::Game;
        use crate::tick::{EventDispatch, TickHooks};
        let mut game = Game::new();
        assert_eq!(game.lists.act_count(), 5);
        assert!(game.lists.ensure_act(5).is_err());
        game.lists.set_act_count(6);
        game.lists.ensure_act(4).unwrap();
        game.lists.ensure_act(5).unwrap();
        let r = game.lists.create_room(5).unwrap();
        assert_eq!(game.lists.room(r).unwrap().act, 5);
        let mut inactive = crate::units::inactive::InactiveStore::default();
        inactive.node_mut(4, 1, 2).items.clear();
        inactive.node_mut(5, 1, 2).items.clear();
        assert!(inactive.take(5, 1, 2).is_some());
        assert!(inactive.take(4, 1, 2).is_some());
        #[derive(Default)]
        struct Hooks(Vec<u8>);
        impl EventDispatch for Hooks {
            fn run_event(&mut self, _: &mut Game, _: &crate::tick::timer::TimerRun) {}
        }
        impl TickHooks for Hooks {
            fn free_inactive_rooms(&mut self, _: &mut Game, act: u8) {
                self.0.push(act);
            }
        }
        let mut h = Hooks::default();
        game.frame = 0;
        crate::tick::tick_from_client_pass(&mut game, &mut h);
        assert_eq!(h.0, vec![4, 5]);
    }
    #[test]
    fn acts_reject_an_overfull_npc_travel_menu() {
        let routes = (0..5)
            .map(|i| format!("{{npc=155,action={i},to=1,text=5000}}"))
            .collect::<Vec<_>>()
            .join(",");
        let text =
            format!("version=1\n[[acts]]\nname='Home'\nstart=1\nend=40\ntown=1\ntravel=[{routes}]");
        assert!(Acts::from_toml(&text).is_err());
        assert!(Acts::from_toml(&text.replace("text=5000", "text=0")).is_ok());
    }
    #[test]
    fn acts_count_and_wire_level_bounds_are_checked() {
        let mut text = String::from("version=1\n");
        for id in 1..=128 {
            use std::fmt::Write;
            writeln!(
                text,
                "[[acts]]\nname='A{id}'\nstart={id}\nend={}\ntown={id}",
                id + 1
            )
            .unwrap();
        }
        assert_eq!(Acts::from_toml(&text).unwrap().count(), 128);
        assert!(Acts::from_toml(
            &(text + "[[acts]]\nname='Too many'\nstart=129\nend=130\ntown=129")
        )
        .is_err());
        assert!(
            Acts::from_toml("version=1\n[[acts]]\nname='Too wide'\nstart=1\nend=257\ntown=1")
                .is_err()
        );
    }
    #[test]
    fn acts_custom_boundaries_waypoints_travel_and_validation() {
        let valid="version=1\n[[acts]]\nname='Home'\nstart=1\nend=40\ntown=2\nwaypoints=[{id=0,level=2}]\ntravel=[{npc=155,action=0,to=41}]\n[[acts]]\nname='Beyond'\nstart=40\nend=100\ntown=41\nwaypoints=[{id=111,level=41}]";
        let a = Acts::from_toml(valid).unwrap();
        assert_eq!(a.act_of_level(41), 1);
        assert_eq!(a.town(0), Some(2));
        assert_eq!(a.travel(2, 155, 0).unwrap().to, 41);
        for bad in [
            valid.replace("version=1", "version=2"),
            valid.replace("town=2", "town=40"),
            valid.replace("id=111", "id=0"),
            valid.replace("to=41", "to=100"),
            valid.replace("start=40", "start=39"),
            valid.replace("name='Beyond'", "name='home'"),
        ] {
            assert!(Acts::from_toml(&bad).is_err(), "{bad}");
        }
    }
}
