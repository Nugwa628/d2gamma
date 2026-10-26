//! World host seams a mod game's rules read and act through: one unit
//! query shared by the world hosts, the host's calendar, the mod message
//! host and the stat and skill point requests.
//!
//! - **Unit query.** [`UnitQuery`] answers the same unit facts (stats on
//!   any layer, skill levels, states, the unit's level, act and town, its
//!   class, the difficulty) from every world host a mod provider is
//!   handed: the cube's world, the NPC host, the item use host, the quest
//!   world and the event host each give one (`unit_query`).
//! - **Calendar.** [`Calendar`] is the date the host supplies to the game
//!   ([`crate::game::Game::calendar`]): the simulation never reads a clock.
//! - **Messages.** [`MessageHost`] is the world a mod game's own messages
//!   run on ([`super::ModRules::message_on`]): the event host plus the
//!   items the players hold.
//! - **Points.** [`WorldHooks`] may answer a player's stat point (C→S 0x3A)
//!   and skill point (C→S 0x3B) requests instead of the engine's rules.
//!
//! Under [`super::Ruleset::Original`] nothing here is asked, and every
//! default answers what the engine did before: no query, no date, the old
//! message call, the engine's point rules.

use std::fmt::Debug;

use crate::skills::{SkillEntry, SkillTables};
use crate::units::{UnitId, UnitType};

use super::EventHost;

/// The state that adds shrine skill levels to a unit's skills.
const STATE_SHRINE_SKILL: u16 = 134;

/// A unit's facts, as every world host answers them. Object safe: a host
/// hands out `&dyn UnitQuery`.
pub trait UnitQuery {
    /// The unit's total value of `stat` at `layer` (0 for a unit without
    /// stats).
    fn stat(&self, unit: UnitId, stat: u16, layer: u16) -> i32;
    /// The unit's base value of `stat` at `layer`.
    fn base_stat(&self, unit: UnitId, stat: u16, layer: u16) -> i32;
    /// The unit's level of `skill`: its own (hard) points, or with
    /// `total` the level with every bonus, the highest of its entries of
    /// the skill (an item's granted skill too). 0 without the skill.
    fn skill_level(&self, unit: UnitId, skill: u16, total: bool) -> i32;
    /// The unit has `state`.
    fn has_state(&self, unit: UnitId, state: u16) -> bool;
    /// The id of the level the unit stands in (`levels.txt` row); `None`
    /// outside a room.
    fn level_id(&self, unit: UnitId) -> Option<u32>;
    /// The act (0-4) of the unit's room; `None` outside a room.
    fn act(&self, unit: UnitId) -> Option<u8>;
    /// The unit stands in a town level.
    fn in_town(&self, unit: UnitId) -> bool;
    /// The unit's type and class (player class or monstats row); `None`
    /// for a unit not in the game.
    fn class(&self, unit: UnitId) -> Option<(UnitType, u32)>;
    /// The game's difficulty: 0 normal, 1 nightmare, 2 hell.
    fn difficulty(&self) -> u8;
}

/// A unit's skill level from its entries: the native entry's hard points,
/// or with `total` the level the skill formulas read (the entry
/// [`crate::skills::highest_entry`] picks, with its bonus levels).
/// `player`: the class of a player unit; `has_state` and `stat(stat,
/// layer)`: the unit's states and total stats.
pub fn skill_level_in(
    t: &SkillTables,
    entries: &[SkillEntry],
    skill: u16,
    total: bool,
    player: Option<i32>,
    has_state: impl Fn(u16) -> bool,
    stat: impl Fn(u16, u16) -> i32,
) -> i32 {
    let skill = i32::from(skill);
    if !total {
        return entries
            .iter()
            .find(|e| e.skill == skill && e.is_native())
            .map_or(0, |e| e.base.clamp(0, t.level_cap.max(0)));
    }
    let Some(e) = crate::skills::highest_entry(entries, skill) else {
        return 0;
    };
    let shrine = has_state(STATE_SHRINE_SKILL);
    crate::skills::skill_level_of(t, &e, true, player, shrine, stat)
}

/// The date the host supplies to the game: the simulation stays
/// deterministic because it never reads a clock itself. All zero (the
/// default): the host supplied none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Calendar {
    /// The year (e.g. 2026); 0 when none was supplied.
    pub year: u16,
    /// The month, 1-12; 0 when none was supplied.
    pub month: u8,
    /// The day of the month, 1-31; 0 when none was supplied.
    pub day: u8,
    /// The day of the week, 0 = Sunday .. 6 = Saturday.
    pub weekday: u8,
}

impl Calendar {
    /// The date `days` days after 1970-01-01 (the civil calendar). A host
    /// passes its local day count: `(unix seconds + UTC offset) / 86400`.
    pub fn from_unix_days(days: i64) -> Self {
        // Howard Hinnant's days-to-civil.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        // 1970-01-01 was a Thursday.
        let weekday = (days + 4).rem_euclid(7);
        Self {
            year: u16::try_from(year).unwrap_or(0),
            month: month as u8,
            day: day as u8,
            weekday: weekday as u8,
        }
    }

    /// The host supplied a date.
    pub fn is_set(&self) -> bool {
        self.month != 0
    }
}

/// Where an item sits, as a [`MessageHost`] reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemFacts {
    /// The item's row in the combined items array.
    pub record: usize,
    /// The item's quality (`items::q`).
    pub quality: u8,
    /// The item's inventory page (`items::moves::page`).
    pub page: u8,
    /// The item's flags.
    pub flags: u32,
}

/// The world a mod game's own messages run on: the event host (stats,
/// seeds, levels, sends) plus the items the units hold.
pub trait MessageHost: EventHost {
    /// The items `owner` holds (inventory, equipment, belt, cursor, cube
    /// and stash), in the inventory's list order; empty without an
    /// inventory model.
    fn items(&self, owner: UnitId) -> Vec<UnitId> {
        let _ = owner;
        Vec::new()
    }
    /// The item's facts; `None` for a unit that is not an item.
    fn item(&self, item: UnitId) -> Option<ItemFacts> {
        let _ = item;
        None
    }
}

/// A [`MessageHost`] over an [`EventHost`] that holds no items: what a
/// host without an inventory model hands a mod message.
pub struct EventsOnly<'h>(pub &'h mut dyn EventHost);

impl EventHost for EventsOnly<'_> {
    fn game(&mut self) -> &mut crate::game::Game {
        self.0.game()
    }
    fn unit_type(&self, unit: UnitId) -> Option<UnitType> {
        self.0.unit_type(unit)
    }
    fn class_id(&self, unit: UnitId) -> i32 {
        self.0.class_id(unit)
    }
    fn stat(&self, unit: UnitId, stat: u16, layer: u16) -> i32 {
        self.0.stat(unit, stat, layer)
    }
    fn set_stat(&mut self, unit: UnitId, stat: u16, value: i32) {
        self.0.set_stat(unit, stat, value)
    }
    fn has_state(&self, unit: UnitId, state: u16) -> bool {
        self.0.has_state(unit, state)
    }
    fn seed(&mut self, unit: UnitId) -> Option<&mut crate::rng::Seed> {
        self.0.seed(unit)
    }
    fn level(&self, unit: UnitId) -> Option<u32> {
        self.0.level(unit)
    }
    fn send(&mut self, player: UnitId, bytes: &[u8]) {
        self.0.send(player, bytes)
    }
    fn unit_query(&self) -> Option<&dyn UnitQuery> {
        self.0.unit_query()
    }
}

impl MessageHost for EventsOnly<'_> {}

/// A mod game's world rules (stat and skill point requests). Every method's
/// default leaves the engine's rules.
pub trait WorldHooks: Send + Sync + Debug {
    /// `player` asks to spend stat points (C→S 0x3A: `stat` the stat byte,
    /// `count` the message's count byte, the engine spends `count` + 1).
    /// `Some(code)` replaces the engine's handler with that result code (0
    /// done, 2 refused, 3 malformed); `None`: the engine's.
    fn stat_point(
        &self,
        h: &mut dyn EventHost,
        player: UnitId,
        stat: u8,
        count: u8,
    ) -> Option<i32> {
        let _ = (h, player, stat, count);
        None
    }
    /// `player` asks to spend a skill point on `skill` (C→S 0x3B).
    /// `Some(code)` replaces the engine's handler (0 done, 2 refused, 3
    /// malformed); `None`: the engine's.
    fn skill_point(&self, h: &mut dyn EventHost, player: UnitId, skill: u16) -> Option<i32> {
        let _ = (h, player, skill);
        None
    }
}

/// C→S 0x3A (stat point) and 0x3B (skill point).
pub mod point_msg {
    /// Spend stat points: `[0x3A, stat, count]`.
    pub const STAT: u8 = 0x3A;
    /// Spend a skill point: `[0x3B, skill lo, skill hi]`.
    pub const SKILL: u8 = 0x3B;
}

/// A stat or skill point request (`msg`, C→S 0x3A or 0x3B, at its full
/// size) of `player` asked of the mod game's [`WorldHooks`]: `Some(code)`
/// when the mod handled it (the engine's handler then doesn't run);
/// `None` under D2RUST's rules, for another message, a short one, or
/// when the mod leaves it to the engine.
pub fn point_request(
    rules: &super::Ruleset,
    h: &mut dyn EventHost,
    player: UnitId,
    msg: &[u8],
) -> Option<i32> {
    let hooks = rules.world_hooks()?;
    match *msg {
        [point_msg::STAT, stat, count] => hooks.stat_point(h, player, stat, count),
        [point_msg::SKILL, lo, hi, ..] => {
            hooks.skill_point(h, player, u16::from_le_bytes([lo, hi]))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_from_unix_days() {
        assert_eq!(
            Calendar::from_unix_days(0),
            Calendar {
                year: 1970,
                month: 1,
                day: 1,
                weekday: 4
            }
        );
        // 2026-10-26 is a Monday: 20,752 days after the epoch.
        assert_eq!(
            Calendar::from_unix_days(20_752),
            Calendar {
                year: 2026,
                month: 10,
                day: 26,
                weekday: 1
            }
        );
        // A leap day and the day before the epoch.
        let leap = Calendar::from_unix_days(11_016);
        assert_eq!((leap.year, leap.month, leap.day), (2000, 2, 29));
        let eve = Calendar::from_unix_days(-1);
        assert_eq!(
            (eve.year, eve.month, eve.day, eve.weekday),
            (1969, 12, 31, 3)
        );
        assert!(!Calendar::default().is_set());
        assert!(leap.is_set());
    }

    fn entry(skill: i32, base: i32, owner: i32) -> SkillEntry {
        SkillEntry {
            skill,
            base,
            level_bonus: 0,
            owner_guid: owner,
            charges: 0,
            has_charges: false,
        }
    }

    #[test]
    fn skill_level_hard_and_total() {
        let t = SkillTables {
            skills: Vec::new(),
            skilldesc: Vec::new(),
            missiles: Vec::new(),
            skills_code: Vec::new(),
            miss_code: Vec::new(),
            level_cap: 20,
            stat_count: 0,
        };
        let entries = [
            entry(5, 3, -1),
            entry(5, 7, 42),
            entry(6, 1, -1),
            entry(8, 4, 42),
        ];
        let none = |_: u16| false;
        let zero = |_: u16, _: u16| 0;
        // Hard points: the native entry only.
        assert_eq!(skill_level_in(&t, &entries, 5, false, None, none, zero), 3);
        // Total: the entry the formulas read (the native one; no bonus
        // source without a skills record), or an item's when it is the only one.
        assert_eq!(skill_level_in(&t, &entries, 5, true, None, none, zero), 3);
        assert_eq!(skill_level_in(&t, &entries, 8, true, None, none, zero), 4);
        assert_eq!(skill_level_in(&t, &entries, 8, false, None, none, zero), 0);
        assert_eq!(skill_level_in(&t, &entries, 9, true, None, none, zero), 0);
        assert_eq!(skill_level_in(&t, &entries, 9, false, None, none, zero), 0);
    }

    /// An event host over a few stats, for the point hooks.
    #[derive(Default)]
    struct Host {
        game: crate::game::Game,
        stats: std::collections::BTreeMap<u16, i32>,
    }

    impl EventHost for Host {
        fn game(&mut self) -> &mut crate::game::Game {
            &mut self.game
        }
        fn unit_type(&self, _: UnitId) -> Option<UnitType> {
            Some(UnitType::Player)
        }
        fn class_id(&self, _: UnitId) -> i32 {
            0
        }
        fn stat(&self, _: UnitId, stat: u16, _: u16) -> i32 {
            self.stats.get(&stat).copied().unwrap_or(0)
        }
        fn set_stat(&mut self, _: UnitId, stat: u16, value: i32) {
            self.stats.insert(stat, value);
        }
        fn has_state(&self, _: UnitId, _: u16) -> bool {
            false
        }
        fn seed(&mut self, _: UnitId) -> Option<&mut crate::rng::Seed> {
            None
        }
        fn level(&self, _: UnitId) -> Option<u32> {
            None
        }
        fn send(&mut self, _: UnitId, _: &[u8]) {}
    }

    /// Stat points go two at a time into stat 0; skill 7 is refused, other
    /// skills are left to the engine.
    #[derive(Debug)]
    struct Points;

    impl WorldHooks for Points {
        fn stat_point(
            &self,
            h: &mut dyn EventHost,
            player: UnitId,
            stat: u8,
            count: u8,
        ) -> Option<i32> {
            let v = h.stat(player, u16::from(stat), 0);
            h.set_stat(player, u16::from(stat), v + 2 * (i32::from(count) + 1));
            Some(0)
        }
        fn skill_point(&self, _: &mut dyn EventHost, _: UnitId, skill: u16) -> Option<i32> {
            (skill == 7).then_some(2)
        }
    }

    #[derive(Debug)]
    struct Rules;

    impl super::super::ModRules for Rules {
        fn name(&self) -> &str {
            "test-points"
        }
        fn world_hooks(&self) -> Option<&dyn WorldHooks> {
            Some(&Points)
        }
    }

    #[test]
    fn point_requests_go_to_the_mod() {
        let rules = super::super::Ruleset::Mod(std::sync::Arc::new(Rules));
        let mut h = Host::default();
        let p = UnitId(1);
        assert_eq!(point_request(&rules, &mut h, p, &[0x3A, 0, 1]), Some(0));
        assert_eq!(h.stats[&0], 4);
        assert_eq!(point_request(&rules, &mut h, p, &[0x3B, 7, 0]), Some(2));
        assert_eq!(point_request(&rules, &mut h, p, &[0x3B, 8, 0]), None);
        // A short message and other ids stay the engine's.
        assert_eq!(point_request(&rules, &mut h, p, &[0x3A, 0]), None);
        assert_eq!(point_request(&rules, &mut h, p, &[0x3C, 0, 0]), None);
    }

    #[test]
    fn d2rust_rules_keep_the_engine_point_rules() {
        let mut h = Host::default();
        let orig = super::super::Ruleset::Original;
        assert_eq!(point_request(&orig, &mut h, UnitId(1), &[0x3A, 0, 1]), None);
        assert!(h.stats.is_empty());
        // A mod without world hooks too.
        #[derive(Debug)]
        struct Bare;
        impl super::super::ModRules for Bare {
            fn name(&self) -> &str {
                "bare"
            }
        }
        let bare = super::super::Ruleset::Mod(std::sync::Arc::new(Bare));
        assert_eq!(point_request(&bare, &mut h, UnitId(1), &[0x3B, 7, 0]), None);
        assert!(h.unit_query().is_none());
    }
}
