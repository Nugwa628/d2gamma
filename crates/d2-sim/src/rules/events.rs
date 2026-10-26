//! A mod game's event hooks (`docs/plans/mods/seams.md` MS11).
//!
//! A mod game reacts to what happens in a game through [`EventHooks`],
//! handed out by [`super::ModRules::events`]: the game created and freed,
//! a player joined, a unit killed, a periodic tick, a level entered and a
//! chat line that may be the mod's command. Each hook runs at a fixed
//! point, after D2RUST's own work for that point (the chat command, which
//! may take the line, is asked before the line goes out). Under
//! [`super::Ruleset::Original`] nothing is asked.
//!
//! A hook acts on the game through an [`EventHost`]. Per-game and
//! per-player state a mod keeps lives in its own `ModRules` object (behind
//! its own lock) and in its own saves, never in D2RUST's; a mod game that
//! keeps such state makes one object per game
//! ([`super::ModRules::for_game`]). Hooks stay
//! deterministic: no I/O, no clock, randomness only from a unit's seed.

use std::fmt::Debug;

use crate::game::Game;
use crate::rng::Seed;
use crate::units::{UnitId, UnitType};

/// The game as an event hook sees it: the game's lists (frame, units,
/// rooms, clients, timers), the units' stats and seeds, and the clients.
pub trait EventHost {
    /// The game: its frame, unit, room and client lists and its timers.
    fn game(&mut self) -> &mut Game;
    /// The unit's type; `None` for a unit not in the game.
    fn unit_type(&self, unit: UnitId) -> Option<UnitType>;
    /// The unit's class (monstats or player class row); 0 when absent.
    fn class_id(&self, unit: UnitId) -> i32;
    /// The unit's total value of `stat` at `layer`.
    fn stat(&self, unit: UnitId, stat: u16, layer: u16) -> i32;
    /// Sets the unit's base value of `stat` (layer 0); its client is told
    /// as for any stat change.
    fn set_stat(&mut self, unit: UnitId, stat: u16, value: i32);
    /// The unit has `state`.
    fn has_state(&self, unit: UnitId, state: u16) -> bool;
    /// The unit's seed, the only RNG a hook may draw from; `None` for a
    /// unit not in the game.
    fn seed(&mut self, unit: UnitId) -> Option<&mut Seed>;
    /// The id of the level the unit stands in; `None` outside a room.
    fn level(&self, unit: UnitId) -> Option<u32>;
    /// Queues the server message `bytes` to the client of `player` (a
    /// player without a client receives nothing).
    fn send(&mut self, player: UnitId, bytes: &[u8]);
    /// The game's own seed (the one its game-wide rolls draw from), for a
    /// hook's game-wide rolls; `None` when the host has none.
    fn game_seed(&mut self) -> Option<&mut Seed> {
        None
    }
    /// The game's difficulty (0 normal, 1 nightmare, 2 hell).
    fn difficulty(&self) -> u8 {
        0
    }
    /// The character of `player` is hardcore.
    fn hardcore(&self, player: UnitId) -> bool {
        let _ = player;
        false
    }
    /// The unit facts every world host answers ([`super::UnitQuery`]);
    /// `None` from a host that has none.
    fn unit_query(&self) -> Option<&dyn super::UnitQuery> {
        None
    }
    /// Sets the unit's base value of `stat` at `layer`; its client is told
    /// as for any stat change. Default: layer 0 only.
    fn set_stat_layer(&mut self, unit: UnitId, stat: u16, layer: u16, value: i32) {
        if layer == 0 {
            self.set_stat(unit, stat, value);
        }
    }
    /// The unit's values of `stat`, one `(layer, value)` per layer it has,
    /// in layer order. Default: none.
    fn stat_layers(&self, unit: UnitId, stat: u16) -> Vec<(u16, i32)> {
        let _ = (unit, stat);
        Vec::new()
    }
    /// The player owning the unit: a summon's or a hireling's master;
    /// `None` for a unit no player owns.
    fn owner(&self, unit: UnitId) -> Option<UnitId> {
        let _ = unit;
        None
    }
    /// The monster's type; `None` for a unit that is not a monster.
    fn monster_type(&self, unit: UnitId) -> Option<MonsterType> {
        let _ = unit;
        None
    }
    /// Spawns a monster of monstats row `class` within `radius` tiles of
    /// the unit `near` (the game's own placement and creation, drawing on
    /// the game's seeds); the new monster, `None` when nothing was placed.
    fn spawn_monster(&mut self, class: u32, near: UnitId, radius: i32) -> Option<UnitId> {
        let _ = (class, near, radius);
        None
    }
    /// Gives a monster (a hireling, a summon) `skill` at `level`, or sets
    /// the level of one it has; `false` when the unit can't take it.
    fn grant_skill(&mut self, unit: UnitId, skill: i32, level: i32) -> bool {
        let _ = (unit, skill, level);
        false
    }
}

/// How a player moves to another level ([`EventHooks::may_enter`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// Walking into a warp tile (stairs, cave and dungeon entrances).
    WarpTile,
    /// A waypoint's travel.
    Waypoint,
    /// A portal (town portals and object portals).
    Portal,
}

/// A monster's type ([`EventHost::monster_type`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MonsterType {
    /// A champion.
    pub champion: bool,
    /// A unique (random or superunique) boss.
    pub unique: bool,
    /// A boss's minion.
    pub minion: bool,
    /// A superunique's `superuniques` row.
    pub superunique: Option<u16>,
}

/// A mod game's event hooks (MS11). Every hook does nothing by default.
pub trait EventHooks: Send + Sync + Debug {
    /// The game was created, before its first client loads.
    fn game_created(&self, h: &mut dyn EventHost) {
        let _ = h;
    }
    /// The game's last client left: the game ends and is freed.
    fn game_freed(&self, h: &mut dyn EventHost) {
        let _ = h;
    }
    /// `player` joined the game, after its join messages.
    fn player_joined(&self, h: &mut dyn EventHost, player: UnitId) {
        let _ = (h, player);
    }
    /// `victim` (a player or a monster) was killed, by `killer` when there
    /// is one, after the kill's experience, drops and death mode request.
    fn unit_killed(&self, h: &mut dyn EventHost, killer: Option<UnitId>, victim: UnitId) {
        let _ = (h, killer, victim);
    }
    /// How often [`EventHooks::periodic`] runs, in base ticks (25 a
    /// second, whatever the game's tick rate); 0: never.
    fn period(&self) -> u32 {
        0
    }
    /// Runs every [`EventHooks::period`] base ticks, at the end of the
    /// tick.
    fn periodic(&self, h: &mut dyn EventHost) {
        let _ = h;
    }
    /// `player` entered level `to`, from level `from` (`None`: the
    /// player's first level in the game), after D2RUST's level change.
    fn level_entered(&self, h: &mut dyn EventHost, player: UnitId, from: Option<u32>, to: u32) {
        let _ = (h, player, from, to);
    }
    /// `player` is about to move from level `from` (`None`: no level) to
    /// level `to` `by` a warp tile, a waypoint or a portal, after D2RUST's
    /// own checks passed. `false` refuses: the player stays where it is,
    /// as for any refused move; the hook tells the player why (its own
    /// message through [`EventHost::send`]).
    fn may_enter(
        &self,
        h: &mut dyn EventHost,
        player: UnitId,
        from: Option<u32>,
        to: u32,
        by: Entry,
    ) -> bool {
        let _ = (h, player, from, to, by);
        true
    }
    /// The character `name` was loaded as `player` (its stats and skills
    /// read, not yet in a room) for a client joining the game. `Some(code)`
    /// refuses the join with load result `code` (the client is told it,
    /// as for a failed load, and the player leaves the game); `None` lets
    /// it in.
    fn may_join(&self, h: &mut dyn EventHost, player: UnitId, name: &[u8]) -> Option<u32> {
        let _ = (h, player, name);
        None
    }
    /// `player` is leaving the game with its client, before it and what
    /// it owns are removed.
    fn player_left(&self, h: &mut dyn EventHost, player: UnitId) {
        let _ = (h, player);
    }
    /// The summon `pet` of `owner` joined its owner's pets, as pet type
    /// `pet_type` (`pettype` row), after D2RUST's summon setup.
    fn pet_created(&self, h: &mut dyn EventHost, owner: UnitId, pet: UnitId, pet_type: i32) {
        let _ = (h, owner, pet, pet_type);
    }
    /// The hireling `hireling` of `owner` was hired, revived, restored
    /// from the save or levelled up. Asked at the end of that tick.
    fn hireling_ready(&self, h: &mut dyn EventHost, owner: UnitId, hireling: UnitId) {
        let _ = (h, owner, hireling);
    }
    /// `player` said `text` (an open chat line, not a whisper). `true`
    /// takes the line as the mod's command: it goes to no one. Asked after
    /// the text filter, before the line goes out.
    fn chat_command(&self, h: &mut dyn EventHost, player: UnitId, text: &[u8]) -> bool {
        let _ = (h, player, text);
        false
    }
}
