//! The action wiring's side of a mod game's event hooks (MS11,
//! [`crate::rules::EventHooks`]): [`CombatView`] is the hooks'
//! [`EventHost`], and [`fire`] / [`ActionSim::mod_event`] ask the game's
//! hooks at each event point. Without hooks (every D2RUST game) nothing
//! is asked: one check of the ruleset.

use crate::game::Game;
use crate::monsters::init::type_flag;
use crate::rng::Seed;
use crate::rules::{Entry, EventHooks, EventHost, MonsterType};
use crate::skills::SkillUnits;
use crate::units::hooks::Sim;
use crate::units::lifecycle::AllocRequest;
use crate::units::modes::monster_mode;
use crate::units::{UnitId, UnitType};

use super::combat::CombatView;
use super::{ActionSim, Pending, View};

impl<X: Pending> EventHost for CombatView<'_, X> {
    fn game(&mut self) -> &mut Game {
        self.game
    }
    fn unit_type(&self, unit: UnitId) -> Option<UnitType> {
        self.game.lists.unit(unit).map(|e| e.ty)
    }
    fn class_id(&self, unit: UnitId) -> i32 {
        self.v.units.get(unit).map_or(0, |r| r.class as i32)
    }
    fn stat(&self, unit: UnitId, stat: u16, layer: u16) -> i32 {
        self.v.stats.unit_total(unit, stat, layer)
    }
    fn set_stat(&mut self, unit: UnitId, stat: u16, value: i32) {
        self.v.set_base(unit, stat, value);
    }
    fn has_state(&self, unit: UnitId, state: u16) -> bool {
        self.v.stats.has_state(unit, u32::from(state))
    }
    fn seed(&mut self, unit: UnitId) -> Option<&mut Seed> {
        self.v.units.get(unit)?;
        Some(self.v.seed(unit))
    }
    fn level(&self, unit: UnitId) -> Option<u32> {
        let room = self.game.lists.unit(unit)?.room()?;
        self.v.h.drlg.level_id(&*self.game, room)
    }
    fn send(&mut self, player: UnitId, bytes: &[u8]) {
        self.v.h.x.send(player, bytes);
    }
    fn game_seed(&mut self) -> Option<&mut Seed> {
        Some(&mut self.v.h.game_seed)
    }
    fn difficulty(&self) -> u8 {
        self.v.data.difficulty
    }
    fn hardcore(&self, player: UnitId) -> bool {
        self.v.h.x.client_hardcore(player)
    }
    fn unit_query(&self) -> Option<&dyn crate::rules::UnitQuery> {
        Some(self)
    }
    fn set_stat_layer(&mut self, unit: UnitId, stat: u16, layer: u16, value: i32) {
        self.v
            .stats
            .unit_set(&mut *self.v.h, unit, stat, value, layer);
    }
    fn stat_layers(&self, unit: UnitId, stat: u16) -> Vec<(u16, i32)> {
        let mut layers = SkillUnits::stat_entries(self, unit, stat, usize::MAX);
        layers.sort_unstable_by_key(|&(layer, _)| layer);
        layers
    }
    /// A summon: the player whose pet lists hold it; a hireling: its
    /// master in the hireling lists, else the AI's minion owner. Only
    /// monsters have owners.
    fn owner(&self, unit: UnitId) -> Option<UnitId> {
        // Pet lists hold monster GUIDs (each unit type has its own).
        let e = self.game.lists.unit(unit)?;
        if e.ty != UnitType::Monster {
            return None;
        }
        let guid = e.guid as i32;
        let h = &*self.v.h;
        let pet_of = h.pet_lists.iter().find(|(_, l)| {
            l.entries
                .iter()
                .any(|e| e.nodes.iter().any(|n| n.guid == guid))
        });
        if let Some((&player, _)) = pet_of {
            return Some(player);
        }
        if let Some(&(player, _)) = h.hireling_ai.ids.get(&unit) {
            return Some(player);
        }
        h.minion_owner_of(self.game, unit)
            .filter(|&o| self.game.lists.unit(o).map(|e| e.ty) == Some(UnitType::Player))
    }
    fn monster_type(&self, unit: UnitId) -> Option<MonsterType> {
        if self.game.lists.unit(unit)?.ty != UnitType::Monster {
            return None;
        }
        let h = &*self.v.h;
        let flag = |f: u16| h.monster_flag(unit, u32::from(f));
        Some(MonsterType {
            champion: flag(type_flag::CHAMPION),
            unique: flag(type_flag::UNIQUE),
            minion: flag(type_flag::MINION),
            superunique: h.superunique(unit),
        })
    }
    /// Population's placement search and creation on the lent monster
    /// world; without one, the plain allocation at the unit's point.
    fn spawn_monster(&mut self, class: u32, near: UnitId, radius: i32) -> Option<UnitId> {
        let room = self.game.lists.unit(near)?.room()?;
        let (x, y) = self.v.h.path_position(near);
        let class_row = i32::try_from(class).ok()?;
        let mode = monster_mode::NU as u8;
        let mut sim = Sim {
            game: &mut *self.game,
            units: &mut *self.v.units,
            stats: &mut *self.v.stats,
            data: self.v.data,
        };
        let placed = self.v.h.with_monster_world(|w, h| {
            w.spawn_at(&mut sim, h, room, x, y, class_row, mode, radius, 0)
        });
        if let Some(unit) = placed.flatten() {
            return unit;
        }
        let allied = (self.v.h.tables.combat.monstats)
            .get(class as usize)
            .is_some_and(|m| m.npc);
        let req = AllocRequest {
            ty: UnitType::Monster,
            class,
            room: Some(room),
            add: true,
            fixed_guid: None,
            mode: monster_mode::NU,
            allied,
        };
        let unit = self.v.allocate(self.game, &req, x, y)?;
        self.v.link_boss_chains(unit, class);
        Some(unit)
    }
    fn grant_skill(&mut self, unit: UnitId, skill: i32, level: i32) -> bool {
        if self.v.units.get(unit).map(|r| r.ty) != Some(UnitType::Monster) {
            return false;
        }
        let mut sim = Sim {
            game: &mut *self.game,
            units: &mut *self.v.units,
            stats: &mut *self.v.stats,
            data: self.v.data,
        };
        self.v.h.assign_monster_skill(&mut sim, unit, skill, level);
        true
    }
}

/// Asks the game's event hooks with the view as their host; nothing under
/// D2RUST's own rules or when the mod game has no event hooks.
pub fn fire<X: Pending, R>(
    cv: &mut CombatView<'_, X>,
    f: impl FnOnce(&dyn EventHooks, &mut dyn EventHost) -> R,
) -> Option<R> {
    cv.v.h.rules.events()?;
    // The ruleset is cloned (one `Arc`) so the hook can borrow the view.
    let rules = cv.v.h.rules.clone();
    let hooks = rules.events()?;
    Some(f(hooks, cv))
}

/// Whether `player` may move to level `to` `by` a warp tile, waypoint or
/// portal ([`EventHooks::may_enter`]); always under D2RUST's own rules.
pub fn may_enter<X: Pending>(
    game: &mut Game,
    v: &mut View<'_, X>,
    player: UnitId,
    to: u32,
    by: Entry,
) -> bool {
    if v.h.rules.events().is_none() {
        return true;
    }
    let mut cv = v.combat(game);
    let from = EventHost::level(&cv, player);
    fire(&mut cv, |e, h| e.may_enter(h, player, from, to, by)).unwrap_or(true)
}

impl<X: Pending> ActionSim<X> {
    /// Asks the game's event hooks ([`fire`]) over this game.
    pub fn mod_event<R>(
        &mut self,
        game: &mut Game,
        f: impl FnOnce(&dyn EventHooks, &mut dyn EventHost) -> R,
    ) -> Option<R> {
        self.sys.hooks.rules.events()?;
        self.combat(game, |cv, _| fire(cv, f))
    }

    /// The game was created ([`EventHooks::game_created`]); its builder or
    /// session layer calls this once, before the first client loads.
    pub fn game_created(&mut self, game: &mut Game) {
        self.mod_event(game, |e, h| e.game_created(h));
    }

    /// The game's last client left ([`EventHooks::game_freed`]); its
    /// session layer calls this once.
    pub fn game_freed(&mut self, game: &mut Game) {
        self.mod_event(game, |e, h| e.game_freed(h));
    }

    /// `player` leaves the game with its client
    /// ([`EventHooks::player_left`]); its session layer calls this before
    /// the player is removed.
    pub fn player_left(&mut self, game: &mut Game, player: UnitId) {
        self.mod_event(game, |e, h| e.player_left(h, player));
    }

    /// The end of the tick: the hireling events of the tick
    /// ([`EventHooks::hireling_ready`], queued by
    /// [`crate::units::lifecycle::LifecycleHooks::hireling_ready`]), then the
    /// periodic hook when its period is due (counted in base ticks, so a
    /// faster tick rate keeps its timing).
    pub(super) fn periodic_events(&mut self, game: &mut Game) {
        for (owner, hireling) in std::mem::take(&mut self.sys.hooks.mod_hirelings) {
            self.mod_event(game, |e, h| e.hireling_ready(h, owner, hireling));
        }
        let Some(period) = self.sys.hooks.rules.events().map(|e| e.period()) else {
            return;
        };
        let Ok(period) = i32::try_from(period) else {
            return;
        };
        if period == 0 || !crate::tick::is_due(game.frame, game.ticks(period)) {
            return;
        }
        self.mod_event(game, |e, h| e.periodic(h));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::rules::{ModRules, Ruleset};
    use crate::tick::TickHooks;
    use crate::wiring::action::reaction::kill_by;
    use crate::wiring::action::tests::Fx;

    /// Test event hooks: every call logged; a chat line starting with `!`
    /// is taken and answered.
    #[derive(Debug, Default)]
    struct Log {
        seen: Mutex<Vec<String>>,
        period: u32,
    }

    impl Log {
        fn push(&self, s: String) {
            self.seen.lock().unwrap().push(s);
        }
        fn seen(&self) -> Vec<String> {
            self.seen.lock().unwrap().clone()
        }
    }

    impl EventHooks for Log {
        fn game_created(&self, h: &mut dyn EventHost) {
            self.push(format!("created {}", h.game().frame));
        }
        fn game_freed(&self, h: &mut dyn EventHost) {
            self.push(format!("freed {}", h.game().frame));
        }
        fn player_joined(&self, _: &mut dyn EventHost, player: UnitId) {
            self.push(format!("joined {}", player.0));
        }
        fn unit_killed(&self, h: &mut dyn EventHost, killer: Option<UnitId>, victim: UnitId) {
            let ty = h.unit_type(victim);
            self.push(format!(
                "killed {:?} {} {ty:?}",
                killer.map(|k| k.0),
                victim.0
            ));
        }
        fn period(&self) -> u32 {
            self.period
        }
        fn periodic(&self, h: &mut dyn EventHost) {
            self.push(format!("periodic {}", h.game().frame));
        }
        fn level_entered(&self, _: &mut dyn EventHost, player: UnitId, from: Option<u32>, to: u32) {
            self.push(format!("level {} {from:?} {to}", player.0));
        }
        fn may_enter(
            &self,
            _: &mut dyn EventHost,
            player: UnitId,
            from: Option<u32>,
            to: u32,
            by: Entry,
        ) -> bool {
            self.push(format!("enter {} {from:?} {to} {by:?}", player.0));
            to != CLOSED
        }
        fn player_left(&self, _: &mut dyn EventHost, player: UnitId) {
            self.push(format!("left {}", player.0));
        }
        fn hireling_ready(&self, _: &mut dyn EventHost, owner: UnitId, hireling: UnitId) {
            self.push(format!("hireling {} {}", owner.0, hireling.0));
        }
        fn chat_command(&self, h: &mut dyn EventHost, player: UnitId, text: &[u8]) -> bool {
            self.push(format!(
                "chat {} {}",
                player.0,
                String::from_utf8_lossy(text)
            ));
            if text.starts_with(b"!") {
                h.send(player, b"ok");
                return true;
            }
            false
        }
    }

    /// The level the test hooks refuse.
    const CLOSED: u32 = 99;

    #[derive(Debug)]
    struct TestGame(Arc<Log>);

    impl ModRules for TestGame {
        fn name(&self) -> &str {
            "test-events"
        }
        fn events(&self) -> Option<&dyn EventHooks> {
            Some(self.0.as_ref())
        }
    }

    fn modded(fx: &mut Fx, period: u32) -> Arc<Log> {
        let log = Arc::new(Log {
            period,
            ..Log::default()
        });
        fx.sim.sys.hooks.rules = Ruleset::Mod(Arc::new(TestGame(log.clone())));
        log
    }

    /// A kill tells the hooks the killer and the victim, after D2RUST's
    /// kill (the monster is in its death mode by then).
    #[test]
    fn a_kill_reaches_the_hooks_after_the_kill() {
        let mut fx = Fx::new();
        let log = modded(&mut fx, 0);
        let a = fx.a;
        let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
        let m = fx.spawn(UnitType::Monster, 0, a, 12, 12);
        fx.sim.combat(&mut fx.game, |w, _| kill_by(w, m, Some(p)));
        assert_eq!(
            log.seen(),
            [format!("killed Some({}) {} Some(Monster)", p.0, m.0)]
        );
        assert_eq!(
            fx.sim.sys.units.get(m).map(|r| r.mode),
            Some(crate::units::modes::monster_mode::DT)
        );
    }

    /// The periodic hook runs at the end of every tick its period (in base
    /// ticks) is due; a period of 0 never runs.
    #[test]
    fn the_periodic_hook_runs_every_period() {
        let mut fx = Fx::new();
        let log = modded(&mut fx, 5);
        for _ in 0..12 {
            fx.tick();
        }
        assert_eq!(log.seen(), ["periodic 5", "periodic 10"]);
        let mut fx = Fx::new();
        let log = modded(&mut fx, 0);
        for _ in 0..12 {
            fx.tick();
        }
        assert!(log.seen().is_empty());
    }

    /// Created and freed come from the game's session layer; join and the
    /// first level come from the tick's client steps.
    #[test]
    fn game_join_and_level_events() {
        let mut fx = Fx::new();
        let log = modded(&mut fx, 0);
        fx.sim.game_created(&mut fx.game);
        let a = fx.a;
        let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
        let c = fx.game.lists.add_client(Some(p), None, 0);
        fx.sim.join_sequence(&mut fx.game, c);
        fx.sim.client_level_change(&mut fx.game, c);
        fx.sim.game_freed(&mut fx.game);
        let level = crate::wiring::action::tests::LEVEL;
        assert_eq!(
            log.seen(),
            [
                "created 0".to_string(),
                format!("joined {}", p.0),
                format!("level {} None {level}", p.0),
                "freed 0".to_string(),
            ]
        );
    }

    /// The move gate asks the hooks with the player's level; under
    /// D2RUST's own rules every move is let through.
    #[test]
    fn the_move_gate_asks_the_hooks() {
        let mut fx = Fx::new();
        let a = fx.a;
        let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
        let ask = |fx: &mut Fx, to: u32| {
            fx.sim
                .with(&mut fx.game, |g, v| may_enter(g, v, p, to, Entry::Waypoint))
        };
        assert!(ask(&mut fx, CLOSED));
        let log = modded(&mut fx, 0);
        assert!(!ask(&mut fx, CLOSED));
        assert!(ask(&mut fx, 4));
        let level = crate::wiring::action::tests::LEVEL;
        assert_eq!(
            log.seen(),
            [
                format!("enter {} Some({level}) {CLOSED} Waypoint", p.0),
                format!("enter {} Some({level}) 4 Waypoint", p.0),
            ]
        );
    }

    /// A leaving player reaches the hooks; a hireling hired, revived or
    /// restored reaches them at the end of its tick, once.
    #[test]
    fn leaving_players_and_ready_hirelings() {
        use crate::units::lifecycle::LifecycleHooks;
        let mut fx = Fx::new();
        let a = fx.a;
        let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
        let m = fx.spawn(UnitType::Monster, 0, a, 12, 10);
        // D2RUST's own rules queue nothing.
        fx.sim.sys.hooks.hireling_ready(p, m);
        assert!(fx.sim.sys.hooks.mod_hirelings.is_empty());
        let log = modded(&mut fx, 0);
        fx.sim.sys.hooks.hireling_ready(p, m);
        assert!(log.seen().is_empty(), "not before the end of the tick");
        fx.tick();
        fx.tick();
        fx.sim.player_left(&mut fx.game, p);
        assert_eq!(
            log.seen(),
            [format!("hireling {} {}", p.0, m.0), format!("left {}", p.0)]
        );
    }

    /// A chat command is the hook's answer; its host sends to the player.
    #[test]
    fn a_chat_command_is_taken_by_the_hook() {
        let mut fx = Fx::new();
        let log = modded(&mut fx, 0);
        let a = fx.a;
        let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
        fx.sim.sys.hooks.x.sent.clear();
        let taken = |fx: &mut Fx, text: &'static [u8]| {
            fx.sim
                .mod_event(&mut fx.game, |e, h| e.chat_command(h, p, text))
        };
        assert_eq!(taken(&mut fx, b"!where"), Some(true));
        assert_eq!(taken(&mut fx, b"hello"), Some(false));
        assert_eq!(fx.sim.sys.hooks.x.sent, [(p, b"ok".to_vec())]);
        assert_eq!(log.seen().len(), 2);
    }

    /// A hook reads the game's difficulty, a player's hardcore flag and
    /// rolls on the game's own seed.
    #[test]
    fn the_host_gives_the_game_values() {
        let mut fx = Fx::new();
        modded(&mut fx, 0);
        let a = fx.a;
        let (p, q) = (
            fx.spawn(UnitType::Player, 0, a, 10, 10),
            fx.spawn(UnitType::Player, 0, a, 12, 10),
        );
        fx.sim.sys.data.difficulty = 2;
        fx.sim.sys.hooks.x.hardcore.push(q);
        let mut want = fx.sim.sys.hooks.game_seed;
        let want = (want.roll(100), want);
        let got = fx.sim.mod_event(&mut fx.game, |_, h| {
            let roll = h.game_seed().map(|s| s.roll(100));
            (h.difficulty(), h.hardcore(p), h.hardcore(q), roll)
        });
        assert_eq!(got, Some((2, false, true, Some(want.0))));
        // The roll drew from the game's seed.
        assert_eq!(fx.sim.sys.hooks.game_seed, want.1);
    }

    /// Stats at any layer: set, listed in layer order and read back.
    #[test]
    fn a_hook_sets_and_lists_stat_layers() {
        const STAT: u16 = 88;
        let mut fx = Fx::new();
        modded(&mut fx, 0);
        let a = fx.a;
        let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
        let got = fx.sim.mod_event(&mut fx.game, |_, h| {
            h.set_stat_layer(p, STAT, 0x2003, 7);
            h.set_stat_layer(p, STAT, 0x2000, 5);
            h.set_stat_layer(p, STAT, 0, 1);
            (
                h.stat_layers(p, STAT),
                h.stat(p, STAT, 0x2003),
                h.stat(p, STAT, 0x2001),
            )
        });
        assert_eq!(got, Some((vec![(0, 1), (0x2000, 5), (0x2003, 7)], 7, 0)));
    }

    /// A summon's and a hireling's owner is their player; a monster's
    /// type flags and superunique row reach the hook.
    #[test]
    fn a_hook_reads_owners_and_monster_types() {
        use crate::player::pets::{PetEntry, PetLists, PetNode};
        let mut fx = Fx::new();
        modded(&mut fx, 0);
        let a = fx.a;
        let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
        let summon = fx.spawn(UnitType::Monster, 0, a, 11, 10);
        let merc = fx.spawn(UnitType::Monster, 0, a, 12, 10);
        let wild = fx.spawn(UnitType::Monster, 0, a, 13, 10);
        let guid = fx.game.lists.unit(summon).unwrap().guid as i32;
        let h = &mut fx.sim.sys.hooks;
        h.pet_lists.insert(
            p,
            PetLists {
                entries: vec![PetEntry {
                    nodes: vec![PetNode {
                        flags: 0,
                        guid,
                        extra: [0; 3],
                    }],
                    count: 1,
                    max: 1,
                }],
            },
        );
        h.hireling_ai.ids.insert(merc, (p, 1));
        let flags = u32::from(type_flag::UNIQUE | type_flag::SUPERUNIQUE);
        h.x.type_flags.insert(wild, flags);
        h.x.superuniques.insert(wild, 42);
        h.x.type_flags
            .insert(merc, u32::from(type_flag::CHAMPION | type_flag::MINION));
        let got = fx.sim.mod_event(&mut fx.game, |_, h| {
            let owners = [summon, merc, wild, p].map(|u| h.owner(u));
            let types = [wild, merc, p].map(|u| h.monster_type(u));
            (owners, types)
        });
        let (owners, types) = got.unwrap();
        assert_eq!(owners, [Some(p), Some(p), None, None]);
        assert_eq!(
            types,
            [
                Some(MonsterType {
                    unique: true,
                    superunique: Some(42),
                    ..MonsterType::default()
                }),
                Some(MonsterType {
                    champion: true,
                    minion: true,
                    ..MonsterType::default()
                }),
                None,
            ]
        );
    }

    /// A hook spawns a monster next to a player and gives it a skill.
    #[test]
    fn a_hook_spawns_a_monster_and_grants_it_a_skill() {
        let mut fx = Fx::new();
        modded(&mut fx, 0);
        let a = fx.a;
        let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
        let got = fx.sim.mod_event(&mut fx.game, |_, h| {
            let m = h.spawn_monster(0, p, 8)?;
            let granted = h.grant_skill(m, 5, 3);
            Some((m, granted, h.grant_skill(p, 5, 3)))
        });
        let (m, granted, to_player) = got.flatten().expect("spawned");
        assert!(granted && !to_player, "only monsters take skills");
        let e = fx.game.lists.unit(m).expect("in the game");
        assert_eq!((e.ty, e.room()), (UnitType::Monster, Some(a)));
        assert_eq!(fx.sim.sys.units.get(m).map(|r| r.class), Some(0));
        assert!(fx
            .sim
            .sys
            .hooks
            .skill_list_of(m)
            .iter()
            .any(|s| s.skill == 5 && s.base == 3));
    }

    /// Under D2RUST's own rules nothing is asked.
    #[test]
    fn d2rusts_own_rules_ask_nothing() {
        let mut fx = Fx::new();
        let a = fx.a;
        let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
        let asked = fx
            .sim
            .mod_event(&mut fx.game, |e, h| e.chat_command(h, p, b"!x"));
        assert_eq!(asked, None);
    }
}
