//! A mod game's world rules: who may enter a level, where a town portal
//! leads, what an object drops and how far an interaction reaches.
//!
//! [`super::ModRules::world`] hands out a [`WorldRules`]; each method is
//! asked at one fixed point and its default answer is the engine's own, so
//! under [`super::Ruleset::Original`] (or a mod without world rules) the
//! game runs exactly as before. The functions below ([`may_enter`],
//! [`town_portal_level`], [`object_treasure_class`], [`interact_limit`])
//! are those points: the engine calls them, not the provider.
//!
//! A hook that needs the game (stats, levels, seeds, the frame) gets it as
//! an [`EventHost`]; one that only decides from the engine's facts gets a
//! plain record. Hooks stay deterministic: no I/O, no clock, draws only from
//! the game's seeds.

use std::fmt::Debug;

use super::{EventHost, Ruleset};
use crate::units::{UnitId, UnitType};

/// How a player is moving to another level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// Walking into a warp tile (stairs, cave entrances, level exits).
    Warp,
    /// Travelling with a waypoint.
    Waypoint,
    /// Taking a portal object (town portals, permanent portals).
    Portal,
}

/// One level change a player asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LevelEntry {
    /// The player moving.
    pub player: UnitId,
    /// The level the player leaves; `None` when it is not known (no room).
    pub from: Option<u32>,
    /// The level the player would enter.
    pub to: u32,
    /// How.
    pub via: Entry,
    /// The game's difficulty (0 normal, 1 nightmare, 2 hell).
    pub difficulty: u8,
}

/// One object drop (a chest, a weapon rack, any object whose operate drops
/// from a treasure class).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectDrop {
    /// The object.
    pub object: UnitId,
    /// Its `objects.txt` row.
    pub class: u16,
    /// The id of its level (0 without a room).
    pub level: u32,
    /// The game's difficulty (0 normal, 1 nightmare, 2 hell).
    pub difficulty: u8,
    /// The treasure class the engine picks (its chest table), if any.
    pub tc: Option<u16>,
}

/// A mod game's world rules. Every method's default is the engine's
/// answer.
pub trait WorldRules: Send + Sync + Debug {
    /// Whether the player may make `entry`. `false` refuses it the way the
    /// engine refuses a closed quest gate: the warp, waypoint or portal does
    /// nothing (a portal plays its refusal sound).
    fn may_enter(&self, h: &mut dyn EventHost, entry: &LevelEntry) -> bool {
        let _ = (h, entry);
        true
    }

    /// The level a town portal cast by `player` in level `from` opens to;
    /// `town` is the engine's answer (the town of `from`'s act). `None`:
    /// `town`. An answer in another act than `from` is ignored: a portal
    /// pair stays within an act.
    fn town_portal_level(
        &self,
        h: &mut dyn EventHost,
        player: UnitId,
        from: u32,
        town: u32,
    ) -> Option<u32> {
        let _ = (h, player, from, town);
        None
    }

    /// The treasure class (`treasureclassex` row) of an object drop:
    /// `Some(tc)` replaces the engine's pick (`drop.tc`); `None` keeps it.
    fn object_treasure_class(&self, drop: &ObjectDrop) -> Option<u16> {
        let _ = drop;
        None
    }

    /// The largest unit distance from which a player's interact request
    /// (C→S 0x13) on a unit of type `target` starts: past it the request
    /// does nothing; within it the player walks up or interacts. Asked for
    /// objects and warp tiles. `None`: the engine's [`INTERACT_LIMIT`].
    fn interact_limit(&self, target: UnitType) -> Option<i32> {
        let _ = target;
        None
    }
}

/// The engine's interact limit: a request from farther than this unit
/// distance does nothing.
pub const INTERACT_LIMIT: i32 = 50;

impl Ruleset {
    /// The mod game's world rules, `None` under [`Ruleset::Original`] or
    /// when the mod has none.
    pub fn world(&self) -> Option<&dyn WorldRules> {
        self.mod_rules()?.world()
    }
}

/// Whether `entry` may happen under `rules` ([`WorldRules::may_enter`]);
/// always under the engine's own rules.
pub fn may_enter(rules: &Ruleset, h: &mut dyn EventHost, entry: &LevelEntry) -> bool {
    rules.world().is_none_or(|w| w.may_enter(h, entry))
}

/// The level of a town portal from `from` ([`WorldRules::town_portal_level`]):
/// the mod's answer when it is in `from`'s act, else `town`.
pub fn town_portal_level(
    rules: &Ruleset,
    h: &mut dyn EventHost,
    player: UnitId,
    from: u32,
    town: u32,
) -> u32 {
    let Some(w) = rules.world() else {
        return town;
    };
    match w.town_portal_level(h, player, from, town) {
        Some(l) if crate::drlg::act_of_level(l) == crate::drlg::act_of_level(from) => l,
        _ => town,
    }
}

/// The treasure class of an object drop ([`WorldRules::object_treasure_class`]):
/// the mod's, else the engine's `drop.tc`.
pub fn object_treasure_class(rules: &Ruleset, drop: &ObjectDrop) -> Option<u16> {
    rules
        .world()
        .and_then(|w| w.object_treasure_class(drop))
        .or(drop.tc)
}

/// The interact limit for a target of type `target`
/// ([`WorldRules::interact_limit`]): the mod's, else [`INTERACT_LIMIT`].
pub fn interact_limit(rules: &Ruleset, target: UnitType) -> i32 {
    rules
        .world()
        .and_then(|w| w.interact_limit(target))
        .unwrap_or(INTERACT_LIMIT)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::game::Game;
    use crate::rng::Seed;
    use crate::rules::ModRules;

    /// An event host with nothing in it but a game.
    #[derive(Default)]
    struct Host(Game);

    impl EventHost for Host {
        fn game(&mut self) -> &mut Game {
            &mut self.0
        }
        fn unit_type(&self, _: UnitId) -> Option<UnitType> {
            Some(UnitType::Player)
        }
        fn class_id(&self, _: UnitId) -> i32 {
            0
        }
        fn stat(&self, _: UnitId, stat: u16, _: u16) -> i32 {
            // Character level 30.
            if stat == 12 {
                30
            } else {
                0
            }
        }
        fn set_stat(&mut self, _: UnitId, _: u16, _: i32) {}
        fn has_state(&self, _: UnitId, _: u16) -> bool {
            false
        }
        fn seed(&mut self, _: UnitId) -> Option<&mut Seed> {
            None
        }
        fn level(&self, _: UnitId) -> Option<u32> {
            None
        }
        fn send(&mut self, _: UnitId, _: &[u8]) {}
    }

    /// Levels past 100 need character level 50; town portals from act 1
    /// open to level 3; chests of class 5 drop from treasure class 77;
    /// warps reach 75.
    #[derive(Debug, Default)]
    struct Mod {
        asked: Mutex<Vec<LevelEntry>>,
    }

    impl WorldRules for Mod {
        fn may_enter(&self, h: &mut dyn EventHost, e: &LevelEntry) -> bool {
            self.asked.lock().unwrap().push(*e);
            e.to <= 100 || h.stat(e.player, 12, 0) >= 50
        }
        fn town_portal_level(
            &self,
            _: &mut dyn EventHost,
            _: UnitId,
            from: u32,
            _: u32,
        ) -> Option<u32> {
            // Act 1 → level 3; level 50 (act 2) → level 1 (act 1, refused).
            match from {
                2 => Some(3),
                50 => Some(1),
                _ => None,
            }
        }
        fn object_treasure_class(&self, d: &ObjectDrop) -> Option<u16> {
            (d.class == 5).then_some(77)
        }
        fn interact_limit(&self, target: UnitType) -> Option<i32> {
            (target == UnitType::Tile).then_some(75)
        }
    }

    #[derive(Debug)]
    struct Game1(Arc<Mod>);

    impl ModRules for Game1 {
        fn name(&self) -> &str {
            "world-test"
        }
        fn world(&self) -> Option<&dyn WorldRules> {
            Some(self.0.as_ref())
        }
    }

    fn entry(to: u32) -> LevelEntry {
        LevelEntry {
            player: UnitId(1),
            from: Some(2),
            to,
            via: Entry::Warp,
            difficulty: 0,
        }
    }

    fn drop(class: u16) -> ObjectDrop {
        ObjectDrop {
            object: UnitId(9),
            class,
            level: 2,
            difficulty: 1,
            tc: Some(10),
        }
    }

    /// The mod is asked and its answers are used.
    #[test]
    fn the_mods_world_rules_answer() {
        let m = Arc::new(Mod::default());
        let rules = Ruleset::Mod(Arc::new(Game1(m.clone())));
        let mut h = Host::default();
        assert!(may_enter(&rules, &mut h, &entry(40)));
        assert!(!may_enter(&rules, &mut h, &entry(120)));
        assert_eq!(m.asked.lock().unwrap().len(), 2);
        assert_eq!(town_portal_level(&rules, &mut h, UnitId(1), 2, 1), 3);
        // An answer in another act keeps the engine's town.
        assert_eq!(town_portal_level(&rules, &mut h, UnitId(1), 50, 40), 40);
        assert_eq!(town_portal_level(&rules, &mut h, UnitId(1), 4, 1), 1);
        assert_eq!(object_treasure_class(&rules, &drop(5)), Some(77));
        assert_eq!(object_treasure_class(&rules, &drop(6)), Some(10));
        assert_eq!(interact_limit(&rules, UnitType::Tile), 75);
        assert_eq!(interact_limit(&rules, UnitType::Object), INTERACT_LIMIT);
    }

    /// Under the engine's own rules, and for a mod without world rules,
    /// every answer is the engine's.
    #[test]
    fn the_engines_own_rules_are_unchanged() {
        #[derive(Debug)]
        struct Bare;
        impl ModRules for Bare {
            fn name(&self) -> &str {
                "bare"
            }
        }
        for rules in [Ruleset::Original, Ruleset::Mod(Arc::new(Bare))] {
            let mut h = Host::default();
            assert!(may_enter(&rules, &mut h, &entry(120)));
            assert_eq!(town_portal_level(&rules, &mut h, UnitId(1), 2, 1), 1);
            assert_eq!(object_treasure_class(&rules, &drop(5)), Some(10));
            assert_eq!(interact_limit(&rules, UnitType::Tile), INTERACT_LIMIT);
        }
    }
}
