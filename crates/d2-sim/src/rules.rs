//! The ruleset a game runs under (`docs/plans/mods/seams.md` MS0).
//!
//! Each mod is its own game: its own binary, built on the D2RUST crates,
//! launched directly. That binary installs its overrides by building its
//! games under [`Ruleset::Mod`]. D2Gamma's exe builds [`Ruleset::Original`]
//! unless it was started with a mod-kit mod (`--mod`), whose files-only
//! rules the built-in [`DataMod`] hosts.
//!
//! [`Ruleset::Original`] is D2RUST's own rules: the seams change nothing,
//! so behaviour, frame hashes, determinism and save formats are D2RUST's.
//! [`Ruleset::Mod`] carries a mod game's [`ModRules`]; each seam asks it
//! first and falls back to D2RUST's rules where it has no answer. The
//! seams are override points, not a closed plugin API: when a mod game
//! needs to override something that has no seam yet, one is added here.
//!
//! A game's ruleset is set when the game is built and never changes.

use std::fmt::Debug;
use std::sync::Arc;

use d2_data::bin::DataRules;
use d2_data::calc::{CalcRegistry, Family};
use d2_data::schema::{ExtStore, ExtType, ExtValue};

use crate::combat::hit::{BlockResult, HitTerms};
use crate::combat::DamageRecord;
pub use crate::drlg::TownLevels;
use crate::game::Game;
use crate::items::bitstream::Isc;
use crate::items::provider::ItemProvider;
pub use crate::missiles::provider::{MissileProvider, MissileSlot};
use crate::monsters::init::{GameInfo, PlayerBonus};
pub use crate::monsters::provider::MonsterProvider;
use crate::rng::Seed;
use crate::skills::calc::CalcContext;
pub use crate::skills::use_::form::{FormSkill, UnitForm};
pub use crate::skills::use_::table::Kind as SkillFn;
use crate::units::{UnitId, UnitType};
pub use crate::world::cube::CubeProvider;
pub use crate::world::npc::NpcProvider;
pub use crate::world::objects::ObjectProvider;
pub use crate::world::quests::QuestProvider;
pub use crate::world::vendors::StoreProvider;

pub mod acts;
pub mod data_mod;
pub mod difficulties;
mod events;
pub mod global;
pub mod scripts;
pub use data_mod::{
    DataMod, DataModError, DataModFiles, Decay, Formula, Reapply, SkillDef, SkillRules,
    SkillsError, StackEffect, StateDef, StateRules, StatesError, TriggerDef, TriggerRules,
    TriggersError, BLOCK_SLOT, MAX_DEPTH,
};
pub use events::{Entry, EventHooks, EventHost, MonsterType};
mod saves;
pub use saves::{ModSection, SaveHooks};
mod skill_host;
pub use skill_host::*;
mod skill_actions;
mod skill_bar;
mod skill_pets;
mod skill_respec;
mod skill_search;
pub use skill_actions::{AuraPulse, BuffPulse, CursePulse, SkillActions};
pub use skill_bar::{SkillBar, SkillCharge};
pub use skill_pets::{SentrySetup, SkillPets, Summon};
pub use skill_respec::SkillRespec;
pub use skill_search::SkillSearch;
mod formulas;
pub use formulas::{AvoidTerms, WeaponView};
mod game_state;
pub use game_state::NewGame;
pub mod world;
pub use world::WorldRules;
mod host_world;
pub use host_world::{
    point_msg, point_request, skill_level_in, Calendar, EventsOnly, ItemFacts, MessageHost,
    UnitQuery, WorldHooks,
};
mod calc;
pub use calc::CalcUnit;
pub mod stash;
pub use stash::{Credit, ItemHost, StashHooks};

/// The overrides a mod game installs in the simulation.
///
/// Every method has a default that changes nothing, so a mod game
/// overrides only the seams it uses. Overrides must keep the simulation
/// deterministic: no floats in game state, no I/O, no RNG other than the
/// game's own.
pub trait ModRules: Send + Sync + Debug {
    /// Curated game-wide settings, fixed by the host before the game starts.
    fn global_rules(&self) -> &global::GlobalRules {
        &global::DEFAULT
    }

    /// Named stat definitions and transformations; absent for base play.
    /// Use the versioned mod stat codec; default Rust mods keep their codec.
    fn difficulties(&self) -> Option<&difficulties::Difficulties> {
        None
    }

    fn wide_stat_ids(&self) -> bool {
        false
    }

    fn stat_rules(&self) -> Option<&crate::stats::named::StatRules> {
        None
    }

    /// Referenced script exports, checked before a game starts.
    fn script_calls(&self) -> Vec<&scripts::CallSpec> {
        Vec::new()
    }
    /// The mod game's short name, for logs, reports and the dev console.
    fn name(&self) -> &str;

    /// The rules of the mod's states (duration, stacks, what the stacks
    /// do), `None` when the mod defines none: every state then plays as
    /// the `states` table and the applying skill say.
    fn state_rules(&self) -> Option<&data_mod::StateRules> {
        None
    }

    /// The crowd control a hit's `source` declares (see `combat::cc`): empty
    /// when the mod declares none for it.
    fn declared_cc(&self, source: crate::combat::cc::CcSource) -> Vec<crate::combat::cc::CcEffect> {
        let _ = source;
        Vec::new()
    }

    /// The state that makes its holder immune to stun, freeze, interrupt and
    /// knockback ([`crate::rules::data_mod::CcRules::immune_state`]); `None`: no such state.
    fn cc_immune_state(&self) -> Option<u16> {
        None
    }

    /// The mod game's skill bodies (MS1); `None`: D2RUST's.
    fn skills(&self) -> Option<&dyn SkillProvider> {
        None
    }

    /// The mod game's event hooks (MS11); `None`: none.
    fn events(&self) -> Option<&dyn EventHooks> {
        None
    }

    /// The mod game's world rules (stat and skill point requests); `None`:
    /// D2RUST's.
    fn world_hooks(&self) -> Option<&dyn WorldHooks> {
        None
    }

    /// The mod game's formula functions (MS9); `None`: D2RUST's only.
    fn calc(&self) -> Option<&dyn CalcProvider> {
        None
    }

    /// The mod game's combat and progression formulas (MS3); `None`:
    /// D2RUST's.
    fn formulas(&self) -> Option<&dyn FormulaHooks> {
        None
    }

    /// The mod game's item overrides (MS7); `None`: D2RUST's.
    fn items(&self) -> Option<&dyn ItemProvider> {
        None
    }

    /// The mod game's cube ops and outputs (MS8a); `None`: D2RUST's.
    fn cube(&self) -> Option<&dyn CubeProvider> {
        None
    }

    /// The mod game's object operate functions (MS8b); `None`: D2RUST's.
    fn objects(&self) -> Option<&dyn ObjectProvider> {
        None
    }

    /// The mod game's NPC roles and menu actions (MS8c); `None`: D2RUST's.
    fn npcs(&self) -> Option<&dyn NpcProvider> {
        None
    }

    /// The mod game's quest callbacks (MS8d); `None`: D2RUST's.
    fn quests(&self) -> Option<&dyn QuestProvider> {
        None
    }

    /// The mod game's town levels (read from its own data); `None`:
    /// D2RUST's five act towns.
    fn acts(&self) -> Option<&acts::Acts> {
        None
    }

    fn town_levels(&self) -> Option<&TownLevels> {
        None
    }

    /// The mod game's store roles (MS8e); `None`: D2RUST's.
    fn stores(&self) -> Option<&dyn StoreProvider> {
        None
    }

    /// The mod game's world rules (level entry, town portals, object
    /// drops, interact reach); `None`: the engine's.
    fn world(&self) -> Option<&dyn WorldRules> {
        None
    }

    /// The mod game's monster AI and umods (MS5); `None`: D2RUST's.
    fn monsters(&self) -> Option<&dyn MonsterProvider> {
        None
    }

    /// The mod game's missile server-do and server-hit functions (MS6);
    /// `None`: D2RUST's.
    fn missiles(&self) -> Option<&dyn MissileProvider> {
        None
    }

    /// The rules the game `game` runs under, made when it is built
    /// (before anything of the game exists); `None`: this object, shared
    /// by every game. A mod game that keeps per-game state (behind its own
    /// lock) returns a fresh object here, so the games of one server never
    /// mix their state.
    fn for_game(&self, game: &NewGame) -> Option<Arc<dyn ModRules>> {
        let _ = game;
        None
    }

    /// The mod game's own stash (more pages, currency counters); `None`:
    /// D2Gamma's stash only. See [`stash`].
    fn stash(&self) -> Option<&dyn StashHooks> {
        None
    }

    /// A mod message from `player`'s client (MS13): the payload of a C→S
    /// envelope (`d2_proto::envelope`), in order with the client's game
    /// messages. The payload layout is the mod game's own. Replies and
    /// other mod messages to clients go through `out`. Default: ignored.
    fn message(&self, game: &mut Game, player: UnitId, payload: &[u8], out: &mut dyn ModOutbox) {
        let _ = (game, player, payload, out);
    }

    /// The mod game's save hooks: its own sections in a character save;
    /// `None`: none.
    fn saves(&self) -> Option<&dyn SaveHooks> {
        None
    }

    /// A mod message from `player`'s client, as [`ModRules::message`], on
    /// the game's world: stats, seeds, levels and sends of the event host,
    /// the unit query and the items the units hold. The host calls this;
    /// the default hands the game to [`ModRules::message`].
    fn message_on(
        &self,
        h: &mut dyn MessageHost,
        player: UnitId,
        payload: &[u8],
        out: &mut dyn ModOutbox,
    ) {
        self.message(h.game(), player, payload, out);
    }
}

/// Where a mod game's server sends its own messages (MS13): each payload
/// becomes one S→C envelope to the client of a player.
pub trait ModOutbox {
    /// Queues `payload` for `player`'s client. `false`: not sent, because
    /// the player has no client in the game or the payload is larger than
    /// one envelope carries.
    fn send(&mut self, player: UnitId, payload: &[u8]) -> bool;
}

/// D2RUST's own rules, for seams that hand out a ruleset by reference
/// where the game has none of its own (test worlds, tools).
pub static ORIGINAL: Ruleset = Ruleset::Original;

/// The rules a game runs under.
#[derive(Clone, Debug, Default)]
pub enum Ruleset {
    /// D2RUST's own rules.
    #[default]
    Original,
    /// A mod game's overrides over D2RUST's rules.
    Mod(Arc<dyn ModRules>),
}

impl Ruleset {
    /// The host's game-wide settings. No client message can install overrides.
    pub fn global_rules(&self) -> &global::GlobalRules {
        self.mod_rules()
            .map_or(&global::DEFAULT, ModRules::global_rules)
    }

    /// `true` under [`Ruleset::Original`].
    pub fn is_original(&self) -> bool {
        matches!(self, Ruleset::Original)
    }

    /// The authoritative mod's named stat registry.
    pub fn difficulties(&self) -> Option<&difficulties::Difficulties> {
        self.mod_rules()?.difficulties()
    }

    pub fn wide_stat_ids(&self) -> bool {
        self.mod_rules().is_some_and(|m| m.wide_stat_ids())
    }

    pub fn stat_rules(&self) -> Option<&crate::stats::named::StatRules> {
        self.mod_rules()?.stat_rules()
    }

    /// The mod's state rules ([`ModRules::state_rules`]); `None` for
    /// D2Gamma's own rules and a mod without any.
    pub fn state_rules(&self) -> Option<&data_mod::StateRules> {
        self.mod_rules()?.state_rules()
    }

    /// The crowd control a hit's `source` declares ([`ModRules::declared_cc`]);
    /// empty for D2Gamma's own rules.
    pub fn declared_cc(
        &self,
        source: crate::combat::cc::CcSource,
    ) -> Vec<crate::combat::cc::CcEffect> {
        self.mod_rules()
            .map_or_else(Vec::new, |m| m.declared_cc(source))
    }

    /// The state that makes its holder immune to crowd control
    /// ([`ModRules::cc_immune_state`]); `None` for D2Gamma's own rules.
    pub fn cc_immune_state(&self) -> Option<u16> {
        self.mod_rules()?.cc_immune_state()
    }

    /// The mod game's overrides, `None` under [`Ruleset::Original`].
    pub fn mod_rules(&self) -> Option<&dyn ModRules> {
        match self {
            Ruleset::Original => None,
            Ruleset::Mod(rules) => Some(rules.as_ref()),
        }
    }

    /// The mod game's event hooks (MS11), `None` under
    /// [`Ruleset::Original`] or when the mod has none.
    pub fn events(&self) -> Option<&dyn EventHooks> {
        self.mod_rules()?.events()
    }

    /// The mod game's world rules, `None` under [`Ruleset::Original`] or
    /// when the mod has none.
    pub fn world_hooks(&self) -> Option<&dyn WorldHooks> {
        self.mod_rules()?.world_hooks()
    }

    /// The mod game's formula functions (MS9), `None` under
    /// [`Ruleset::Original`] or when the mod adds none.
    pub fn calc(&self) -> Option<&dyn CalcProvider> {
        self.mod_rules()?.calc()
    }

    /// The mod game's item overrides (MS7), `None` under
    /// [`Ruleset::Original`] or when the mod has none.
    pub fn items(&self) -> Option<&dyn ItemProvider> {
        self.mod_rules()?.items()
    }

    /// The data rules the game's tables load under (MS10): D2RUST's own
    /// under [`Ruleset::Original`], a mod game's under [`Ruleset::Mod`]
    /// (row counts from the loaded data, MS10b). The game builder passes
    /// them to `d2_data::bin::load_from_with`.
    pub fn data_rules(&self) -> DataRules {
        match self {
            Ruleset::Original => DataRules::Original,
            Ruleset::Mod(_) => DataRules::Mod,
        }
    }

    /// The mod game's formulas (MS3), `None` under [`Ruleset::Original`]
    /// or when the mod changes none.
    pub fn formulas(&self) -> Option<&dyn FormulaHooks> {
        self.mod_rules()?.formulas()
    }

    /// The players-X scaling of a monster created in a game of `info`
    /// (`align` its monstats alignment): the mod game's
    /// [`FormulaHooks::player_bonus`] first, else D2RUST's.
    pub fn player_bonus(&self, info: &GameInfo, align: u8) -> PlayerBonus {
        self.formulas()
            .and_then(|f| f.player_bonus(info, align))
            .unwrap_or_else(|| crate::monsters::init::player_bonus(info, align))
    }

    /// The mod game's cube ops and outputs (MS8a), `None` under
    /// [`Ruleset::Original`] or when the mod has none.
    pub fn cube(&self) -> Option<&dyn CubeProvider> {
        self.mod_rules()?.cube()
    }

    /// The mod game's object operate functions (MS8b), `None` under
    /// [`Ruleset::Original`] or when the mod has none.
    pub fn objects(&self) -> Option<&dyn ObjectProvider> {
        self.mod_rules()?.objects()
    }

    /// The mod game's NPC roles and menu actions (MS8c), `None` under
    /// [`Ruleset::Original`] or when the mod has none.
    pub fn npcs(&self) -> Option<&dyn NpcProvider> {
        self.mod_rules()?.npcs()
    }

    /// The mod game's quest callbacks (MS8d), `None` under
    /// [`Ruleset::Original`] or when the mod has none.
    pub fn quests(&self) -> Option<&dyn QuestProvider> {
        self.mod_rules()?.quests()
    }

    /// The optional mod act registry; built-in acts apply when absent.
    pub fn acts(&self) -> Option<&acts::Acts> {
        self.mod_rules()?.acts()
    }
    pub fn act_of_level(&self, level: u32) -> u8 {
        self.acts().map_or_else(
            || crate::drlg::act_of_level(level),
            |a| a.act_of_level(level),
        )
    }
    pub fn act_town(&self, act: u8) -> Option<u32> {
        self.acts().map_or_else(
            || crate::drlg::TOWN_LEVELS.get(act as usize).copied(),
            |a| a.town(act),
        )
    }
    pub fn act_count(&self) -> usize {
        self.acts().map_or(5, |a| a.count())
    }

    /// The game's town levels: an explicit provider, act data, or built-in towns.
    pub fn town_levels(&self) -> TownLevels {
        self.mod_rules()
            .and_then(|m| m.town_levels())
            .cloned()
            .or_else(|| self.acts().map(|a| a.towns()))
            .unwrap_or_default()
    }

    /// Whether `level` is a town of the game ([`Ruleset::town_levels`]).
    pub fn is_town(&self, level: u32) -> bool {
        match self.mod_rules().and_then(|m| m.town_levels()) {
            Some(t) => t.is_town(level),
            None => self.acts().map_or_else(
                || crate::drlg::is_town(level),
                |a| a.rows().iter().any(|a| a.town == level),
            ),
        }
    }

    /// The mod game's store roles (MS8e), `None` under
    /// [`Ruleset::Original`] or when the mod has none.
    pub fn stores(&self) -> Option<&dyn StoreProvider> {
        self.mod_rules()?.stores()
    }

    /// The mod game's monster AI and umods (MS5), `None` under
    /// [`Ruleset::Original`] or when the mod has none.
    pub fn monsters(&self) -> Option<&dyn MonsterProvider> {
        self.mod_rules()?.monsters()
    }

    /// The mod game's missile functions (MS6), `None` under
    /// [`Ruleset::Original`] or when the mod has none.
    pub fn missiles(&self) -> Option<&dyn MissileProvider> {
        self.mod_rules()?.missiles()
    }

    /// The mod game's own stash, `None` under [`Ruleset::Original`] or
    /// when the mod has none.
    pub fn stash(&self) -> Option<&dyn StashHooks> {
        self.mod_rules()?.stash()
    }
}

/// A mod game's formula functions (MS9).
///
/// The mod game registers its keywords in [`CalcProvider::registry`] and
/// compiles its tables with that registry
/// (`d2_data::compile_set::compile_all_with`), so a keyword compiles to a
/// CALL with a function index past the family's own. The evaluator sends
/// exactly those calls here; calls to D2RUST's own functions, and to an
/// index nothing is registered at, never reach the provider.
pub trait CalcProvider: Send + Sync + Debug {
    /// The keywords this mod game adds, per family.
    fn registry(&self) -> &CalcRegistry;
    /// Function `index` of `family` (registered in
    /// [`CalcProvider::registry`]) with its arguments in source order.
    /// `ctx` is the evaluation's own context: its `param` and `call` give
    /// D2RUST's parameters and functions (`rand` draws from the game's
    /// seeded RNG).
    fn call(&self, family: Family, index: u8, args: &[i32], ctx: &mut dyn CalcContext) -> i32;
    /// Parameter `code` of `family` (its `skillcalc` / `misscalc` row, as
    /// the mod game's own tables compiled it), asked before D2RUST's: a
    /// mod game answers the rows its tables add past D2RUST's, or
    /// replaces one. `None`: D2RUST's value (0 past its rows).
    fn param(&self, family: Family, code: u8, ctx: &mut dyn CalcContext) -> Option<i32> {
        let _ = (family, code, ctx);
        None
    }
}

/// The unit of a formula a [`FormulaHost`] reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The unit attacking; absent in formulas of one unit.
    Attacker,
    /// The unit hit, blocking or resisting.
    Defender,
}

/// What a mod game's combat formulas read (MS3): the formula's units, the
/// game's difficulty and expansion flag, and the units' seeds. A side the
/// formula has no unit for reads as absent (`unit_type` `None`, stats 0).
pub trait FormulaHost {
    /// The side's unit type; `None` when the formula has no such unit.
    fn unit_type(&self, side: Side) -> Option<UnitType>;
    /// The side's class (monstats or player class row).
    fn class_id(&self, side: Side) -> i32;
    /// The side's total value of `stat` at `layer`.
    fn stat(&self, side: Side, stat: u16, layer: u16) -> i32;
    /// The side has `state`.
    fn has_state(&self, side: Side, state: u16) -> bool;
    /// The game's difficulty (0 normal, 1 nightmare, 2 hell).
    fn difficulty(&self) -> usize;
    /// The game runs the expansion rules.
    fn expansion(&self) -> bool;
    /// The side's seed, the only RNG a formula may draw from. `None` in
    /// the formulas that never draw (resist and block chance) and for an
    /// absent side.
    fn seed(&mut self, side: Side) -> Option<&mut Seed>;
    /// The side is a hireling: a monster unit that is a player's ally and
    /// takes a player's resist cap. `false` for an absent side.
    fn is_hireling(&self, side: Side) -> bool {
        let _ = side;
        false
    }
    /// The side's current mode (player or monster mode); 0 for an absent
    /// side.
    fn mode(&self, side: Side) -> i32 {
        let _ = side;
        0
    }
    /// The side is in a moving mode (walk or run). `false` for an absent
    /// side.
    fn moving(&self, side: Side) -> bool {
        let _ = side;
        false
    }
    /// The weapon the side attacks with in the formula's hand; `None`
    /// without one (or for an absent side).
    fn weapon(&self, side: Side) -> Option<WeaponView> {
        let _ = side;
        None
    }
}

/// A mod game's combat and progression formulas (MS3).
///
/// Each hook answers first; `None` runs D2RUST's own formula. A hook gets
/// the formula's inputs: its units through a [`FormulaHost`], or the plain
/// values D2RUST's formula reads.
pub trait FormulaHooks: Send + Sync + Debug {
    /// The attacker's chance in percent to hit the defender (D2RUST: the
    /// attack rating against the defense and both levels, 5 to 95). The
    /// roll is one draw from the attacker's seed against the answer.
    fn hit_chance(&self, h: &mut dyn FormulaHost, terms: HitTerms) -> Option<i32> {
        let _ = (h, terms);
        None
    }
    /// The defender's block chance in percent (D2RUST: shield, block
    /// stat, class block factor, dexterity and level, capped at
    /// [`crate::combat::hit::BLOCK_CAP`]). The roll still divides it by 3 for a running
    /// player.
    fn block_chance(&self, h: &mut dyn FormulaHost) -> Option<i32> {
        let _ = h;
        None
    }
    /// The defender's chance in percent of one way to avoid a hit:
    /// `kind` is [`BlockResult::Evade`] (while moving),
    /// [`BlockResult::WeaponBlock`], [`BlockResult::Avoid`] (missiles) or
    /// [`BlockResult::Dodge`] (melee). D2RUST: the passive stat's value.
    /// 0 or less never rolls.
    fn avoid_chance(&self, h: &mut dyn FormulaHost, kind: BlockResult) -> Option<i32> {
        let _ = (h, kind);
        None
    }
    /// The resist penalty a player or hireling takes at `difficulty`,
    /// added to each resist (D2RUST: the difficulty table's penalty in
    /// the expansion, -20 / -50 in the classic game).
    fn resist_penalty(&self, difficulty: usize, expansion: bool) -> Option<i32> {
        let _ = (difficulty, expansion);
        None
    }
    /// The cap on a player's or hireling's positive resist of stat
    /// `resist` (`None` for the leech rows). D2RUST: 75 plus the matching
    /// max-resist stat, at most 95; damage reduction 50. Monsters have no
    /// cap.
    fn resist_cap(&self, h: &mut dyn FormulaHost, resist: Option<u16>) -> Option<i32> {
        let _ = (h, resist);
        None
    }
    /// The attacker's weapon and elemental damage against the defender:
    /// `rec` is the record before the roll, `offhand` the hand used and
    /// `scale` the skill's damage share (128 = all). The answer is the
    /// rolled record; resists and absorbs still apply after it.
    fn damage(
        &self,
        h: &mut dyn FormulaHost,
        rec: &DamageRecord,
        offhand: bool,
        scale: i32,
    ) -> Option<DamageRecord> {
        let _ = (h, rec, offhand, scale);
        None
    }
    /// The players-X scaling of a monster created in a game of `info`
    /// (`align` its monstats alignment): the player count it counts and
    /// its life and experience bonus percents.
    fn player_bonus(&self, info: &GameInfo, align: u8) -> Option<PlayerBonus> {
        let _ = (info, align);
        None
    }
    /// The highest level of player class `class` (D2RUST: the experience
    /// table's max level row). Levels past the table need
    /// [`FormulaHooks::exp_threshold`].
    fn max_level(&self, class: i32) -> Option<u32> {
        let _ = class;
        None
    }
    /// The experience a player of `class` needs to reach level
    /// `level + 1` (D2RUST: the experience table).
    fn exp_threshold(&self, class: i32, level: u32) -> Option<u32> {
        let _ = (class, level);
        None
    }
    /// The resist penalty of the defender of `h`, of any unit type, added
    /// to each resist that takes the difficulty penalty. Asked before
    /// [`FormulaHooks::resist_penalty`]; `None` keeps D2RUST's: that hook
    /// or the difficulty penalty for players and hirelings, none for
    /// monsters.
    fn defender_resist_penalty(&self, h: &mut dyn FormulaHost) -> Option<i32> {
        let _ = h;
        None
    }
    /// The whole block and avoid sequence of a hit on the defender of `h`
    /// (D2RUST: shield block, then evade while moving, weapon block, and
    /// avoid or dodge, each chance through the hooks above). Its draws come
    /// from the defender's seed. A block that sets the 0x8000 result flag
    /// (blocked by the defender's animation) answers
    /// [`BlockResult::WeaponBlock`], which sets that flag.
    fn block_or_avoid(&self, h: &mut dyn FormulaHost, terms: AvoidTerms) -> Option<BlockResult> {
        let _ = (h, terms);
        None
    }
    /// The result flags of a hit on the defender of `h`
    /// ([`crate::combat::hit::result`]) once the hit test, the block and
    /// avoid sequence and the get-hit reaction are in: `flags` is D2RUST's
    /// result, the answer replaces it. `missile`: the hit of a missile.
    fn hit_result(&self, h: &mut dyn FormulaHost, flags: u16, missile: bool) -> Option<u16> {
        let _ = (h, flags, missile);
        None
    }
}

impl Ruleset {
    /// The mod game's skill bodies (MS1), `None` under [`Ruleset::Original`]
    /// or when the mod has none.
    pub fn skills(&self) -> Option<&dyn SkillProvider> {
        self.mod_rules()?.skills()
    }

    /// The mod game answers slot `index` of the `kind` table.
    pub fn has_skill_slot(&self, kind: SkillFn, index: u16) -> bool {
        self.skills().is_some_and(|p| p.has(kind, index))
    }
}

/// What a mod game's skill bodies read and change (MS1): the base reads
/// here, and the unit, skill, state, missile, damage and timer parts of
/// [`skill_host`] and the other seam groups among its supertraits. The
/// world a provider is handed is the skill use view
/// (`crate::wiring::interaction::UseView`), which answers them all.
pub trait SkillHost:
    HostUnits
    + HostSkills
    + HostStates
    + HostMissiles
    + HostDamage
    + HostTimers
    + SkillActions
    + SkillSearch
    + SkillBar
    + SkillPets
    + SkillRespec
{
    /// Run a reusable script block; zero means no action, nonzero means acted.
    fn call_script(
        &mut self,
        _spec: &scripts::CallSpec,
        _context: scripts::CallContext,
    ) -> Result<i32, String> {
        Err("script runtime is not installed".into())
    }
    /// Script diagnostics and player messages. Hosts without scripts do nothing.
    fn script_error(&mut self, _message: &str) {}
    fn script_message(&mut self, _unit: UnitId, _message: &str) {}
    /// Publish a transient, namespaced integer to this player's mod HUD.
    fn script_ui_variable(&mut self, _unit: UnitId, _name: &str, _value: i32) {}
    /// Validate table IDs before a script passes them to engine capabilities.
    fn script_id_valid(&self, _kind: &str, _id: u16) -> bool {
        false
    }
    /// Game frame.
    fn frame(&self) -> i32;
    /// The unit's class (monstats or player class row).
    fn class_id(&self, u: UnitId) -> i32;
    /// The unit's total value of `stat` at `layer`.
    fn stat(&self, u: UnitId, stat: u16, layer: u16) -> i32;
    /// The unit's base value of `stat` at `layer`.
    fn base_stat(&self, u: UnitId, stat: u16, layer: u16) -> i32;
    /// The unit has `state`.
    fn has_state(&self, u: UnitId, state: u16) -> bool;
    /// Sets the unit's base `stat` (layer 0).
    fn set_stat(&mut self, u: UnitId, stat: u16, value: i32);
    /// The unit's seed: the only RNG a mod may draw from.
    fn seed(&mut self, u: UnitId) -> &mut Seed;
}

/// What happened to a unit that a mod game's skill provider is told about
/// ([`SkillProvider::unit_event`]); the unit is the one it happened to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitEvent {
    /// The unit swung at something in melee.
    Attack,
    /// The unit dealt damage.
    Hit,
    /// The unit took damage.
    BeingHit,
    /// The unit used skill `.0` (a skill a provider's own trigger cast is
    /// not told).
    SkillUsed(i32),
    /// The unit killed something.
    Kill,
    /// State `.0` came on for the unit (a state with stack rules).
    StateStart(u16),
    /// State `.0` ran out on the unit (a state with stack rules).
    StateEnd(u16),
}

/// A mod game's skill start and do bodies, by slot (MS1).
///
/// The skill use core calls a slot the provider
/// [`has`](SkillProvider::has) even where D2RUST's tables have no
/// function, and past their end. The provider answers first; `None`
/// keeps what D2RUST does for that slot (its own body, or nothing for
/// an empty slot).
pub trait SkillProvider: Send + Sync + Debug {
    /// Handle a learned skill's passive refresh; false keeps its table behavior.
    fn refresh_passive(&self, w: &mut dyn SkillHost, u: UnitId, skill: i32) -> bool {
        let _ = (w, u, skill);
        false
    }
    /// A data form ended; its stat list may already have been freed.
    fn form_removed(&self, w: &mut dyn SkillHost, u: UnitId, state: u16) {
        let _ = (w, u, state);
    }
    /// Data effects of a warcry missile hit. None retains the table body's effect.
    fn warcry_hit(
        &self,
        w: &mut dyn SkillHost,
        source: UnitId,
        target: UnitId,
        skill: i32,
        level: i32,
    ) -> Option<bool> {
        let _ = (w, source, target, skill, level);
        None
    }
    /// The provider answers slot `index` of the `kind` table.
    fn has(&self, kind: SkillFn, index: u16) -> bool;
    /// `srvst[index]`; 0 refuses.
    fn srvst(
        &self,
        w: &mut dyn SkillHost,
        index: u16,
        u: UnitId,
        skill: i32,
        lvl: i32,
    ) -> Option<i32> {
        let _ = (w, index, u, skill, lvl);
        None
    }
    /// `srvdo[index]` with the core's arguments; 0 = nothing happened.
    #[allow(clippy::too_many_arguments)]
    fn srvdo(
        &self,
        w: &mut dyn SkillHost,
        index: u16,
        u: UnitId,
        skill: i32,
        lvl: i32,
        charge: bool,
        item: bool,
        aim: bool,
    ) -> Option<i32> {
        let _ = (w, index, u, skill, lvl, charge, item, aim);
        None
    }
    /// `true` when the provider wants [`Self::unit_event`] calls.
    fn wants_unit_events(&self) -> bool {
        false
    }
    /// `ev` happened to `u`; `other` is the other unit of the event (the
    /// one hit, the attacker, the victim). Runs after D2RUST's own handlers
    /// of the event.
    fn unit_event(&self, w: &mut dyn SkillHost, ev: UnitEvent, u: UnitId, other: Option<UnitId>) {
        let _ = (w, ev, u, other);
    }
    /// The unit's stat list of `state` that a mod body applied with
    /// [`StateRemove::Mod`] was removed (expired, cleared or replaced);
    /// the state is still on, for the mod to switch off.
    fn state_removed(&self, w: &mut dyn SkillHost, u: UnitId, state: u16) {
        let _ = (w, u, state);
    }
    /// Resets `player`'s skills in place of D2RUST's own skill reset (the
    /// NPC respec and the reset item; the stat reset still runs): true
    /// when the mod did it, false keeps D2RUST's own.
    fn respec(&self, w: &mut dyn SkillHost, player: UnitId) -> bool {
        let _ = (w, player);
        false
    }
    /// A tick the provider asked for with [`HostTimers::schedule_tick`] is
    /// due for `u`. The tick is spent: the provider schedules the next one
    /// itself.
    fn tick(&self, w: &mut dyn SkillHost, u: UnitId, tick: &Tick) {
        let _ = (w, u, tick);
    }
    /// Called after D2RUST resurrected `player` (passive states back on,
    /// life, mana and stamina full, in town), for the mod's own part.
    fn resurrected(&self, w: &mut dyn SkillHost, player: UnitId) {
        let _ = (w, player);
    }
    /// The form test of a skill use (`use_state` test 7, the skill's
    /// `restrict` column) on the server and the client: `Some` answers
    /// in place of D2RUST's own rule
    /// ([`crate::skills::use_::form::form_ok`]), `None` keeps it.
    fn form_ok(&self, skill: &FormSkill, unit: &dyn UnitForm) -> Option<bool> {
        let _ = (skill, unit);
        None
    }
}

/// The `itemstatcost` extension columns (`addcol`, MS4) a mod game's item
/// bit layout reads its stat save widths from (MS10c).
///
/// A mod game that changes how wide its item stats are saved and sent adds
/// its own `int` columns and names them here; D2RUST names none and keeps
/// its `Save Bits`, `Save Add` and `Save Param Bits`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveColumns {
    /// The column holding `Save Bits`.
    pub save_bits: Option<String>,
    /// The column holding `Save Add`.
    pub save_add: Option<String>,
    /// The column holding `Save Param Bits`.
    pub save_param_bits: Option<String>,
}

/// Overrides the save widths in `isc` (the item tables' itemstatcost rows
/// by stat id) with the values of `columns` in `ext`, for each record whose
/// cell (or the column's default) is a number; other records keep theirs.
/// Called once by a mod game's builder after its tables are compiled, so
/// the item writer and reader use the same layout. Returns the number of
/// values changed.
pub fn apply_save_columns(isc: &mut [Isc], ext: &ExtStore, columns: &SaveColumns) -> usize {
    let column = |name: &Option<String>| {
        let name = name.as_deref()?;
        ext.columns("itemstatcost")
            .iter()
            .find(|c| c.ty == ExtType::Int && c.name.eq_ignore_ascii_case(name.as_bytes()))
    };
    let value = |c: Option<&d2_data::schema::ExtColumn>, r: usize| -> Option<i32> {
        let c = c?;
        let cell = c.cells.get(r)?;
        let text = if cell.is_empty() { &c.default } else { cell };
        match c.ty.value(text)? {
            ExtValue::Int(n) => Some(n),
            ExtValue::Text(_) => None,
        }
    };
    let (bits, add, param) = (
        column(&columns.save_bits),
        column(&columns.save_add),
        column(&columns.save_param_bits),
    );
    let mut changed = 0;
    for (r, row) in isc.iter_mut().enumerate() {
        if let Some(v) = value(bits, r).and_then(|v| u8::try_from(v).ok()) {
            changed += usize::from(row.save_bits != v);
            row.save_bits = v;
        }
        if let Some(v) = value(add, r) {
            let v = v as u32;
            changed += usize::from(row.save_add != v);
            row.save_add = v;
        }
        if let Some(v) = value(param, r).and_then(|v| u32::try_from(v).ok()) {
            changed += usize::from(row.save_param_bits != v);
            row.save_param_bits = v;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Named;

    impl ModRules for Named {
        fn name(&self) -> &str {
            "named"
        }
    }

    #[test]
    fn default_is_original() {
        let r = Ruleset::default();
        assert!(r.is_original());
        assert!(r.mod_rules().is_none());
    }

    #[test]
    fn mod_exposes_its_rules() {
        let r = Ruleset::Mod(Arc::new(Named));
        assert!(!r.is_original());
        assert_eq!(r.mod_rules().map(ModRules::name), Some("named"));
        assert_eq!(r.clone().mod_rules().map(ModRules::name), Some("named"));
    }

    #[test]
    fn save_columns_override_the_widths_they_name() {
        use d2_data::schema::ExtColumn;
        let col = |name: &str, default: &str, cells: &[&str]| ExtColumn {
            name: name.as_bytes().to_vec(),
            ty: ExtType::Int,
            default: default.as_bytes().to_vec(),
            cells: cells.iter().map(|c| c.as_bytes().to_vec()).collect(),
        };
        let mut ext = ExtStore::default();
        ext.push("itemstatcost", col("ModBits", "", &["", "12", "300"]));
        ext.push("itemstatcost", col("ModAdd", "5", &["", "", "7"]));
        let base = Isc {
            valshift: 1,
            save_bits: 9,
            save_add: 2,
            save_param_bits: 3,
        };
        let mut isc = vec![base; 3];
        // No columns named: nothing changes.
        assert_eq!(
            apply_save_columns(&mut isc, &ext, &SaveColumns::default()),
            0
        );
        let cols = SaveColumns {
            save_bits: Some("modbits".into()),
            save_add: Some("ModAdd".into()),
            save_param_bits: Some("Missing".into()),
        };
        assert_eq!(apply_save_columns(&mut isc, &ext, &cols), 4);
        // Row 0: empty bits cell and no default, the add column's default.
        assert_eq!((isc[0].save_bits, isc[0].save_add), (9, 5));
        assert_eq!((isc[1].save_bits, isc[1].save_add), (12, 5));
        // 300 doesn't fit a width byte: kept.
        assert_eq!((isc[2].save_bits, isc[2].save_add), (9, 7));
        assert!(isc
            .iter()
            .all(|r| r.valshift == 1 && r.save_param_bits == 3));
    }

    #[test]
    fn data_rules_follow_the_ruleset() {
        assert_eq!(Ruleset::Original.data_rules(), DataRules::Original);
        let r = Ruleset::Mod(Arc::new(Named));
        assert_eq!(r.data_rules(), DataRules::Mod);
    }
}
