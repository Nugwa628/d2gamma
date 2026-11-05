//! The data mod host: the one [`ModRules`] behind every mod made with the mod
//! kit, a mod that is files only and brings no Rust of its own.
//!
//! A game that loads such a mod runs under [`Ruleset::Mod`] holding a
//! [`DataMod`] (and so under `DataRules::Mod`, which lets the mod's tables
//! grow past the vanilla row counts). A game with no mod keeps
//! [`Ruleset::Original`]: the host is never built for it, so D2Gamma's own
//! play is untouched.
//!
//! What the mod's files say is read once, before the game starts, by whoever
//! hosts the game, and handed in as text: this module does no I/O. The
//! authoritative host decides; an online client never installs or overrides a
//! mod's rules.
//!
//! # Where later parts plug in
//!
//! A [`DataMod`] answers a [`ModRules`] seam only for the parts the mod's files
//! define; every other seam keeps the trait's default, which changes nothing,
//! so a mod with an empty `rules.toml` plays exactly as D2Gamma does.
//! Each part of the mod kit is one field of [`DataMod`], read in
//! [`DataMod::from_files`] from its own file (a new member of
//! [`DataModFiles`]) and answered by overriding the matching [`ModRules`]
//! method. Today that is the global rules (`rules.toml`, answered by
//! [`ModRules::global_rules`]) and the states and stacks (`states.toml`,
//! answered by [`ModRules::state_rules`]) and the building-block skills
//! (`skills.toml`, answered by [`ModRules::skills`]) and the triggers
//! (`triggers.toml`, answered through the same skill provider) and the
//! declared crowd control (`crowd_control.toml`, answered by
//! [`ModRules::declared_cc`]) and the monster behaviour graphs (`ai.toml`,
//! answered by [`ModRules::monsters`]).

use std::sync::Arc;

use super::global::{GlobalRules, RulesError};
use super::{
    EventHooks, ModRules, NewGame, NpcProvider, Ruleset, SaveHooks, SkillFn, SkillHost,
    SkillProvider, Tick, UnitEvent,
};
use crate::combat::cc::{CcEffect, CcSource};
use crate::monsters::provider::MonsterProvider;
use crate::units::UnitId;

pub mod blocks;
pub mod lift;
mod skills;
pub use skills::{
    Affects, AiCursePart, AreaHitPart, Around, AttackMode, AuraPart, AuraStat, BuffEvent, BuffPart,
    CorpseFindPart, CursePart, Damage, DamageAuraPart, DruidSummonPart, Element, FormPart, Formula,
    GolemPart, GuidedProjectilePart, HealPart, MartialChargePart, MeleeHitPart, MissilePart,
    NestPart, Part, PassivePart, ProjectileFanPart, RavenPart, ResurrectPart, RushPart, SentryPart,
    ShadowPart, SightPart, SkillDef, SkillParams, SkillRules, SkillsError, StateOp, StatePart,
    StateTarget, SummonPart, VinePart, WallPart, WarcryPart, BLOCK_SLOT, MAX_AREA_TARGETS,
    MAX_AURA_STATS, MAX_PARTS, MAX_SUMMONS,
};
mod triggers;
pub use triggers::{
    Action, Condition, On, TriggerDef, TriggerRules, TriggersError, Who, MAX_ACTIONS,
    MAX_CONDITIONS, MAX_DEPTH, MAX_TRIGGERS,
};
mod cc;
pub use cc::{CcError, CcRules};
pub mod ai;
pub use ai::{
    ActionDef, ActionKind, AiDef, AiError, AiRules, Condition as AiCondition, CooldownDef,
    Param as AiParam, PhaseDef, Preset as AiPreset, StateDef as AiStateDef, Targeting,
    TransitionDef, Whose, MAX_AIS, MAX_HOPS, THINK_BUDGET,
};
pub mod quests;
pub use quests::{Objective, QuestDef, QuestRules, QuestsError, Reward};
mod states;
pub use states::{
    Decay, Reapply, StackEffect, StateDef, StateRules, StatesError, MAX_STACKS as MAX_STATE_STACKS,
};

/// The built-in mod host. See the module header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataMod {
    name: String,
    global: GlobalRules,
    states: StateRules,
    skills: SkillRules,
    triggers: TriggerRules,
    crowd_control: CcRules,
    monster_ai: AiRules,
    quests: QuestRules,
    stats: crate::stats::named::StatRules,
    difficulties: Option<super::difficulties::Difficulties>,
    acts: Option<super::acts::Acts>,
}

/// A mod's file that could not be used. The message names the file and the
/// setting (the key) that is wrong.
#[derive(Debug, thiserror::Error)]
#[error("{file}: {source}")]
pub struct DataModError {
    /// The file as the modder knows it, for the message.
    pub file: String,
    #[source]
    pub source: Box<dyn std::error::Error + Send + Sync>,
}

/// The files of a mod the host reads. A file the mod does not have is `None`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DataModFiles<'a> {
    /// `files/acts.toml` as (path, text).
    pub acts: Option<(&'a str, &'a str)>,
    /// `files/difficulties.toml` as (path, text).
    pub difficulties: Option<(&'a str, &'a str)>,
    /// `files/stats.toml` as `(path for messages, text)`.
    pub stats: Option<(&'a str, &'a str)>,
    /// `files/rules.toml` as `(path for messages, text)`.
    pub rules: Option<(&'a str, &'a str)>,
    /// `files/states.toml` as `(path for messages, text)`.
    pub states: Option<(&'a str, &'a str)>,
    /// `files/skills.toml` as `(path for messages, text)`.
    pub skills: Option<(&'a str, &'a str)>,
    /// `files/triggers.toml` as `(path for messages, text)`.
    pub triggers: Option<(&'a str, &'a str)>,
    /// `files/crowd_control.toml` as `(path for messages, text)`.
    pub crowd_control: Option<(&'a str, &'a str)>,
    /// `files/ai.toml` as `(path for messages, text)`.
    pub ai: Option<(&'a str, &'a str)>,
    /// `files/quests.toml` as `(path for messages, text)`.
    pub quests: Option<(&'a str, &'a str)>,
}

impl<'a> DataModFiles<'a> {
    /// Start a file set with global rules; unspecified modules keep their defaults.
    pub fn with_rules(rules: Option<(&'a str, &'a str)>) -> Self {
        Self {
            rules,
            ..Self::default()
        }
    }
}

impl DataMod {
    /// A host for the mod `name` that changes nothing: D2Gamma's rules.
    pub fn new(name: impl Into<String>) -> DataMod {
        DataMod {
            name: name.into(),
            global: GlobalRules::default(),
            states: StateRules::none(),
            skills: SkillRules::none(),
            triggers: TriggerRules::none(),
            crowd_control: CcRules::none(),
            monster_ai: AiRules::none(),
            quests: QuestRules::none(),
            stats: crate::stats::named::StatRules::default(),
            difficulties: None,
            acts: None,
        }
    }

    /// A host for the mod `name` built from its `files`. Any file that does
    /// not parse is an error naming the file and the setting.
    pub fn from_files(
        name: impl Into<String>,
        files: &DataModFiles<'_>,
    ) -> Result<DataMod, DataModError> {
        let mut host = DataMod::new(name);
        if let Some((path, text)) = files.acts {
            host.acts =
                Some(
                    super::acts::Acts::from_toml(text).map_err(|source| DataModError {
                        file: path.into(),
                        source: Box::new(source),
                    })?,
                );
        }
        if let Some((path, text)) = files.difficulties {
            host.difficulties = Some(super::difficulties::Difficulties::from_toml(text).map_err(
                |source| DataModError {
                    file: path.into(),
                    source: Box::new(source),
                },
            )?);
        }
        if let Some((path, text)) = files.stats {
            host.stats =
                crate::stats::named::StatRules::from_toml(text).map_err(|source| DataModError {
                    file: path.into(),
                    source: Box::new(source),
                })?;
        }
        let mut resolved = Vec::new();
        for file in [files.states, files.skills, files.triggers] {
            resolved.push(match file {
                Some((path, text)) if files.stats.is_some() => Some((
                    path,
                    host.stats
                        .resolve_fields(text)
                        .map_err(|source| DataModError {
                            file: path.into(),
                            source: Box::new(source),
                        })?,
                )),
                _ => None,
            });
        }
        let files = DataModFiles {
            states: resolved[0]
                .as_ref()
                .map(|(p, t)| (*p, t.as_str()))
                .or(files.states),
            skills: resolved[1]
                .as_ref()
                .map(|(p, t)| (*p, t.as_str()))
                .or(files.skills),
            triggers: resolved[2]
                .as_ref()
                .map(|(p, t)| (*p, t.as_str()))
                .or(files.triggers),
            ..*files
        };
        if let Some((path, text)) = files.rules {
            host.global = GlobalRules::from_toml(text).map_err(|source| DataModError {
                file: path.to_owned(),
                source: Box::new(name_the_key(source, text)),
            })?;
        }
        if let Some((path, text)) = files.states {
            host.states = StateRules::from_toml(text).map_err(|source| DataModError {
                file: path.to_owned(),
                source: Box::new(source),
            })?;
        }
        if let Some((path, text)) = files.skills {
            let file_error = |source: SkillsError| DataModError {
                file: path.to_owned(),
                source: Box::new(source),
            };
            host.skills = SkillRules::from_toml(text).map_err(file_error)?;
            host.skills
                .check_states(&host.states)
                .and_then(|()| host.skills.check_ticks())
                .map_err(|source| DataModError {
                    file: path.to_owned(),
                    source: Box::new(source),
                })?;
        }
        if let Some((path, text)) = files.triggers {
            let file_error = |source: TriggersError| DataModError {
                file: path.to_owned(),
                source: Box::new(source),
            };
            host.triggers = TriggerRules::from_toml(text).map_err(file_error)?;
            host.triggers
                .check_states(&mut host.states)
                .map_err(file_error)?;
        }
        if let Some((path, text)) = files.crowd_control {
            host.crowd_control = CcRules::from_toml(text).map_err(|source| DataModError {
                file: path.to_owned(),
                source: Box::new(source),
            })?;
        }
        if let Some((path, text)) = files.ai {
            let file_error = |source: AiError| DataModError {
                file: path.to_owned(),
                source: Box::new(source),
            };
            host.monster_ai = AiRules::from_toml(text).map_err(file_error)?;
            if let Some((i, j)) = host.monster_ai.wants_immunity() {
                if host.crowd_control.immune_state().is_none() {
                    return Err(file_error(AiError::Setting(format!(
                        "`ai[{i}].phase[{j}].immune_seconds`: needs `immune_state` in crowd_control.toml"
                    ))));
                }
            }
        }
        if let Some((path, text)) = files.quests {
            host.quests = QuestRules::from_toml(text).map_err(|source| DataModError {
                file: path.to_owned(),
                source: Box::new(source),
            })?;
        }
        Ok(host)
    }

    /// The mod's global rules (`rules.toml`).
    pub fn global(&self) -> &GlobalRules {
        &self.global
    }

    /// The mod's state and stack rules (`states.toml`).
    pub fn state_rules(&self) -> &StateRules {
        &self.states
    }

    /// The mod's building-block skills (`skills.toml`).
    pub fn skill_rules(&self) -> &SkillRules {
        &self.skills
    }

    /// The mod's triggers (`triggers.toml`).
    pub fn trigger_rules(&self) -> &TriggerRules {
        &self.triggers
    }

    /// The mod's declared crowd control sources (`crowd_control.toml`).
    pub fn crowd_control_rules(&self) -> &CcRules {
        &self.crowd_control
    }

    /// The mod's monster behaviour graphs (`ai.toml`).
    pub fn ai_rules(&self) -> &AiRules {
        &self.monster_ai
    }

    /// The ruleset a game of this mod runs under.
    pub fn into_ruleset(self) -> Ruleset {
        Ruleset::Mod(Arc::new(self))
    }
}

impl ModRules for DataMod {
    fn acts(&self) -> Option<&super::acts::Acts> {
        self.acts.as_ref()
    }
    fn difficulties(&self) -> Option<&super::difficulties::Difficulties> {
        self.difficulties.as_ref()
    }
    fn wide_stat_ids(&self) -> bool {
        true
    }
    fn stat_rules(&self) -> Option<&crate::stats::named::StatRules> {
        (!self.stats.is_empty()).then_some(&self.stats)
    }

    fn script_calls(&self) -> Vec<&super::scripts::CallSpec> {
        self.skills
            .defs()
            .iter()
            .flat_map(|d| d.parts.iter())
            .filter_map(|p| match p {
                Part::CallScript(call) => Some(call),
                _ => None,
            })
            .chain(
                self.triggers
                    .defs()
                    .iter()
                    .flat_map(|d| d.actions.iter())
                    .filter_map(|a| match a {
                        Action::CallScript(call) => Some(call),
                        _ => None,
                    }),
            )
            .collect()
    }
    fn name(&self) -> &str {
        &self.name
    }

    fn global_rules(&self) -> &GlobalRules {
        &self.global
    }

    fn state_rules(&self) -> Option<&StateRules> {
        (!self.states.is_empty()).then_some(&self.states)
    }

    fn declared_cc(&self, source: CcSource) -> Vec<CcEffect> {
        let mut out = self.crowd_control.declared(source);
        if let CcSource::Missile { skill, .. } = source {
            out.extend_from_slice(self.skills.missile_cc(skill));
        }
        out
    }

    fn cc_immune_state(&self) -> Option<u16> {
        self.crowd_control.immune_state()
    }

    fn monsters(&self) -> Option<&dyn MonsterProvider> {
        (!self.monster_ai.is_empty()).then_some(&self.monster_ai as &dyn MonsterProvider)
    }

    fn skills(&self) -> Option<&dyn SkillProvider> {
        (!self.skills.is_empty() || !self.triggers.is_empty() || self.quests.wants_skill_events())
            .then_some(self as &dyn SkillProvider)
    }

    fn events(&self) -> Option<&dyn EventHooks> {
        (!self.quests.is_empty()).then_some(self as &dyn EventHooks)
    }

    fn saves(&self) -> Option<&dyn SaveHooks> {
        (!self.quests.is_empty()).then_some(self as &dyn SaveHooks)
    }

    fn npcs(&self) -> Option<&dyn NpcProvider> {
        (!self.quests.is_empty()).then_some(self as &dyn NpcProvider)
    }

    /// Quest progress is kept per game, so each game gets its own copy of
    /// the rules (a copy starts with no progress).
    fn for_game(&self, _game: &NewGame) -> Option<Arc<dyn ModRules>> {
        (!self.quests.is_empty()).then(|| Arc::new(self.clone()) as Arc<dyn ModRules>)
    }
}

/// The skill provider of the host: the block skills answer the do slot, the
/// triggers answer the unit events. Either may be missing.
impl SkillProvider for DataMod {
    fn form_removed(&self, w: &mut dyn SkillHost, u: UnitId, state: u16) {
        self.skills.form_removed(w, u, state)
    }
    fn warcry_hit(
        &self,
        w: &mut dyn SkillHost,
        source: UnitId,
        target: UnitId,
        skill: i32,
        level: i32,
    ) -> Option<bool> {
        self.skills.warcry_hit(w, source, target, skill, level)
    }
    fn has(&self, kind: SkillFn, index: u16) -> bool {
        self.skills.has(kind, index)
    }

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
        self.skills
            .srvdo(w, index, u, skill, lvl, charge, item, aim)
    }

    fn tick(&self, w: &mut dyn SkillHost, u: UnitId, tick: &Tick) {
        self.skills.tick(w, u, tick);
    }

    fn wants_unit_events(&self) -> bool {
        !self.triggers.is_empty() || self.quests.wants_skill_events()
    }

    fn unit_event(&self, w: &mut dyn SkillHost, ev: UnitEvent, u: UnitId, other: Option<UnitId>) {
        self.triggers.run(w, ev, u, other);
        if let UnitEvent::SkillUsed(skill) = ev {
            if let Ok(skill) = u16::try_from(skill) {
                self.quests.queue(u, quests::Happened::Cast(skill));
            }
        }
    }

    fn state_removed(&self, w: &mut dyn SkillHost, u: UnitId, state: u16) {
        self.triggers.run(w, UnitEvent::StateEnd(state), u, None);
    }
}

/// A TOML type error shows the offending line but not the whole key; says
/// which setting it is (`players.hit_recovery.enabled`).
fn name_the_key(error: RulesError, text: &str) -> RulesError {
    let RulesError::Toml(e) = &error else {
        return error;
    };
    let Some(span) = e.span() else {
        return error;
    };
    match key_at(text, span.start) {
        Some(key) => RulesError::Setting(format!("`{key}`: {}", e.message())),
        None => error,
    }
}

/// The dotted key whose line holds byte `at` of `text`: the `[table]` header
/// above it joined to the key before the `=`. `None` when the position is not
/// on a `key = value` line.
fn key_at(text: &str, at: usize) -> Option<String> {
    let at = at.min(text.len());
    let mut table = String::new();
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let end = start + line.len();
        let trimmed = line.trim();
        if at >= end {
            if let Some(header) = trimmed.strip_prefix('[') {
                if !header.starts_with('[') {
                    table = header.split(']').next()?.trim().to_owned();
                }
            }
        } else {
            let key = trimmed.split('=').next()?.trim();
            if trimmed.contains('=') && !key.is_empty() && !key.starts_with('[') {
                let key = key.replace(char::is_whitespace, "");
                return Some(if table.is_empty() {
                    key
                } else {
                    format!("{table}.{key}")
                });
            }
            return None;
        }
        start = end;
    }
    None
}

#[cfg(test)]
mod tests;
