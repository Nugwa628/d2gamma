//! Lifting base skills to data: turns one row of the `skills` table into a
//! `skills.toml` entry a mod can copy and edit, plus the table cells that
//! hand the row to the block slot.
//!
//! The lifter reads the user's own table cells (the caller supplies them), so
//! no game table is built into the engine. It lifts the families most
//! classes and monsters share: the common attack, the plain missile, the nova,
//! the basic aura, the curse, Meteor's drop and the Zakarum heal. A lifted skill is *close to* the engine's body, not a
//! copy of it: [`Lifted::notes`] lists what the data version does differently
//! and the closeness report (a test fixture, see `docs/modkit/skills.md`)
//! measures it. D2Gamma's own bodies stay in Rust and run unless a mod swaps a
//! row's `srvdofunc` for the block slot.

use std::collections::{BTreeMap, BTreeSet};

use super::{Affects, SkillRules, BLOCK_SLOT};

mod ai_curse;
mod buff;
mod corpse_find;
mod damage_aura;
mod druid_summon;
mod form;
pub(crate) mod formulas;
mod golem;
mod guided_projectile;
mod martial_charge;
mod passive;
pub mod player;
mod projectile_fan;
mod raven;
mod sentry;
mod shadow;
mod sight;
mod teleport;
mod vine;
mod warcry;

/// The do-function slots of the families that can be lifted.
pub const ATTACK: u16 = 1;
pub const MELEE_STATE: u16 = 2;
pub const JAB: u16 = 7;
pub const SEQUENCE: u16 = 13;
pub const HUNGER: u16 = 122;
pub const FIRE_WALL: u16 = 24;
pub const SMITE: u16 = 150;
pub const CHARGE: u16 = 67;
pub const METEOR: u16 = 28;
pub const CURSE: u16 = 30;
pub const ZAKARUM_HEAL: u16 = 96;
pub const RESURRECT: u16 = 97;
pub const MON_INFERNO: u16 = 95;
pub const NEST: u16 = 91;
pub const NOVA: u16 = 22;
pub const AURA: u16 = 65;

/// A family of base skills the lifter knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Family {
    /// `srvdofunc` 1: the common melee attack.
    Attack,
    /// No do function and a `srvmissile`: the core fires the missile.
    Missile,
    /// `srvdofunc` 22: a ring of 64 missiles.
    Nova,
    /// `srvdofunc` 65: the basic aura.
    Aura,
    /// `srvdofunc` 30: a state with stats on the units around the aim point.
    Curse,
    /// `srvdofunc` 2: a melee hit with an overlay and a state on the target
    /// (Power Strike, Bash, Stun, Concentrate).
    MeleeState,
    /// `srvdofunc` 7: a melee hit with enhanced damage and the row's own element.
    Jab,
    /// `srvdofunc` 13: melee hits in a row on the enemies within reach (Fend,
    /// Zeal, Fury).
    Sequence,
    /// `srvdofunc` 122: a melee hit with life and mana leech on top (Hunger).
    Hunger,
    /// `srvdofunc` 150: a melee hit that stuns.
    Smite,
    /// `srvdofunc` 67: a run at the target that ends in a melee hit.
    Charge,
    /// `srvdofunc` 24: a wall of missiles across the target.
    Wall,
    /// `srvdofunc` 27: move to the aim point using the existing teleport part.
    Teleport,
    /// `srvdofunc` 28: a missile dropped on the aim point.
    Meteor,
    /// `srvdofunc` 97: a dead monster raised.
    Resurrect,
    /// `srvdofunc` 91: a nest hatched.
    Nest,
    /// `srvdofunc` 95: a channel that fires a missile every few ticks.
    Inferno,
    /// `srvdofunc` 96: the target's life restored by a share of its maximum.
    Heal,
    /// Defensive self buff, with explicit stats and event callbacks.
    Sentry,
    Form,
    Vine,
    DamageAura,
    Passive,
    Golem,
    Raven,
    CorpseFind,
    MartialCharge,
    Shadow,
    AiCurse,
    ProjectileFan,
    GuidedProjectile,
    Sight,
    Buff,
    /// Warcry ring with editable effects on the caster and missile-hit units.
    Warcry,
    /// Spirit, wolf and bear setup over the existing summon seam.
    DruidSummon,
}

impl Family {
    /// The name used in reports.
    pub fn name(self) -> &'static str {
        match self {
            Family::Attack => "attack",
            Family::Missile => "missile",
            Family::Nova => "nova",
            Family::Aura => "aura",
            Family::Curse => "curse",
            Family::MeleeState => "melee_state",
            Family::Jab => "jab",
            Family::Sequence => "sequence",
            Family::Hunger => "hunger",
            Family::Smite => "smite",
            Family::Charge => "charge",
            Family::Wall => "wall",
            Family::Teleport => "teleport",
            Family::Meteor => "meteor",
            Family::Heal => "heal",
            Family::Sentry => "sentry",
            Family::Form => "form",
            Family::Vine => "vine",
            Family::DamageAura => "damage_aura",
            Family::Passive => "passive",
            Family::Golem => "golem",
            Family::Raven => "raven",
            Family::CorpseFind => "corpse_find",
            Family::MartialCharge => "martial_charge",
            Family::Shadow => "shadow",
            Family::AiCurse => "ai_curse",
            Family::ProjectileFan => "projectile_fan",
            Family::GuidedProjectile => "guided_projectile",
            Family::Sight => "sight",
            Family::Buff => "buff",
            Family::Warcry => "warcry",
            Family::DruidSummon => "druid_summon",
            Family::Resurrect => "resurrect",
            Family::Nest => "nest",
            Family::Inferno => "inferno",
        }
    }

    /// The family of a row with this `srvdofunc` (and whether it names a
    /// `srvmissile`), when the lifter knows it.
    pub fn of(slot: u16, has_missile: bool) -> Option<Family> {
        match slot {
            ATTACK => Some(Family::Attack),
            NOVA => Some(Family::Nova),
            AURA => Some(Family::Aura),
            CURSE => Some(Family::Curse),
            MELEE_STATE => Some(Family::MeleeState),
            JAB => Some(Family::Jab),
            SEQUENCE => Some(Family::Sequence),
            HUNGER => Some(Family::Hunger),
            SMITE => Some(Family::Smite),
            CHARGE => Some(Family::Charge),
            FIRE_WALL => Some(Family::Wall),
            27 => Some(Family::Teleport),
            METEOR => Some(Family::Meteor),
            ZAKARUM_HEAL => Some(Family::Heal),
            45 => Some(Family::Sentry),
            116 => Some(Family::Form),
            115 => Some(Family::Vine),
            66 | 81 => Some(Family::DamageAura),
            56 => Some(Family::Golem),
            114 => Some(Family::Raven),
            69 | 72 => Some(Family::CorpseFind),
            34 | 35 => Some(Family::MartialCharge),
            49 => Some(Family::Shadow),
            59 | 61 => Some(Family::AiCurse),
            8 | 17 | 118 => Some(Family::ProjectileFan),
            10 => Some(Family::GuidedProjectile),
            6 => Some(Family::Sight),
            18 => Some(Family::Buff),
            68 => Some(Family::Warcry),
            119 => Some(Family::DruidSummon),
            RESURRECT => Some(Family::Resurrect),
            NEST => Some(Family::Nest),
            MON_INFERNO => Some(Family::Inferno),
            0 if has_missile => Some(Family::Missile),
            _ => None,
        }
    }
}

/// The user's tables, as the lifter reads them. Cells are the text of the
/// table (names and formulas), the way a modder sees them.
pub trait Source {
    fn item_type(&self, name: &str) -> Option<u16> {
        name.parse().ok()
    }
    fn monster(&self, name: &str) -> Option<u16> {
        name.parse().ok()
    }
    fn pet_type(&self, name: &str) -> Option<u16> {
        name.parse().ok()
    }
    fn monster_mode(&self, name: &str) -> Option<u16> {
        name.parse().ok()
    }
    /// Resolve a skill name while lifting linked formulas.
    fn skill_row(&self, name: &str) -> Option<u16> {
        name.parse().ok()
    }
    /// The events table row (or a numeric id).
    fn event(&self, name: &str) -> Option<u16> {
        name.parse().ok()
    }
    /// A cell of the skills row being lifted; empty for none.
    fn skill(&self, column: &str) -> String;
    /// The `states` row of a name (or a number).
    fn state(&self, name: &str) -> Option<u16>;
    /// The `itemstatcost` row of a name (or a number).
    fn stat(&self, name: &str) -> Option<u16>;
    /// The `missiles` row of a name (or a number).
    fn missile(&self, name: &str) -> Option<u16>;
    /// A cell of a `missiles` row; empty for none.
    fn missile_cell(&self, row: u16, column: &str) -> String;
}

/// Why a row was not lifted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiftError {
    /// The row's do function is not one of the families.
    Unsupported(u16),
    /// A cell the lift needs is not a name the tables know.
    Unknown { column: &'static str, value: String },
    /// The entry the lift wrote does not read; the engine's own message.
    Unreadable(String),
    /// The row does something the data parts cannot say yet (the reason).
    Needs(String),
}

impl std::fmt::Display for LiftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiftError::Unsupported(slot) => write!(f, "do function {slot} is not lifted"),
            LiftError::Unknown { column, value } => write!(f, "`{column}` = `{value}` is unknown"),
            LiftError::Unreadable(e) => write!(f, "the lifted entry does not read: {e}"),
            LiftError::Needs(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for LiftError {}

/// One lifted skill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lifted {
    pub id: u16,
    pub family: Family,
    /// The `[[skill]]` entry for `files/skills.toml`.
    pub toml: String,
    /// The cells of the skills row to change: `("srvdofunc", "400")` always.
    pub cells: Vec<(&'static str, String)>,
    /// What the data version does differently from the engine's body.
    pub notes: Vec<String>,
}

/// A TOML basic string.
fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A formula cell as formula text: parenthesized, `0` when empty.
fn formula(cell: &str) -> String {
    let cell = cell.trim();
    if cell.is_empty() {
        "0".to_owned()
    } else {
        format!("({cell})")
    }
}

fn number(cell: &str) -> Option<u32> {
    let cell = cell.trim();
    match cell.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => cell.parse().ok(),
    }
}

/// Lifts skills row `id` (a row of `src`) by its do function.
pub fn lift(id: u16, src: &dyn Source) -> Result<Lifted, LiftError> {
    let slot = number(&src.skill("srvdofunc")).map_or(0, |n| n as u16);
    let missile = src.skill("srvmissile");
    let passive = src.skill("passivestate");
    let family = if slot == 0
        && missile.trim().is_empty()
        && !passive.trim().is_empty()
        && passive.trim() != "0"
    {
        Family::Passive
    } else {
        Family::of(slot, !missile.trim().is_empty()).ok_or(LiftError::Unsupported(slot))?
    };
    let name = src.skill("skill");
    let mut notes = Vec::new();
    let mut cells = vec![("srvdofunc", BLOCK_SLOT.to_string())];
    let part = match family {
        Family::DruidSummon => druid_summon::part(src)?,
        Family::Warcry => warcry::part(src)?,
        Family::Sentry => sentry::part(src)?,
        Family::Form => form::part(src)?,
        Family::Vine => vine::part(src)?,
        Family::DamageAura => damage_aura::part(src)?,
        Family::Passive => {
            cells.clear();
            notes.push(
                "Passive data runs during learned-skill refresh, without changing the cast slot."
                    .into(),
            );
            passive::part(src)?
        }
        Family::Golem => golem::part(src)?,
        Family::Raven => raven::part(src)?,
        Family::CorpseFind => corpse_find::part(src)?,
        Family::MartialCharge => martial_charge::part(src)?,
        Family::Shadow => shadow::part(src)?,
        Family::AiCurse => ai_curse::part(src)?,
        Family::ProjectileFan => {
            if !missile.trim().is_empty() {
                cells.push(("srvmissile", String::new()));
            }
            notes.push("Projectile fans are bounded to 64 missiles; central shots are bounded by the total.".into());
            projectile_fan::part(src)?
        }
        Family::GuidedProjectile => {
            if !missile.trim().is_empty() {
                cells.push(("srvmissile", String::new()));
            }
            guided_projectile::part(src)?
        }
        Family::Sight => sight::part(src)?,
        Family::Buff => {
            notes.push("Timed data buffs dispatch cleanup on timer expiry; the current table-buff timer path leaves its state flag and callbacks behind. The closeness report exposes expiry differences.".to_owned());
            buff::part(src)?
        }
        Family::Teleport => teleport::part(&mut notes),
        Family::Attack => {
            notes.push(
                "a bow or crossbow fires its arrow in the engine's attack; the data attack \
                 swings and needs a target"
                    .to_owned(),
            );
            "kind = \"melee_hit\"\n".to_owned()
        }
        Family::Missile => {
            let row = src
                .missile(missile.trim())
                .ok_or_else(|| LiftError::Unknown {
                    column: "srvmissile",
                    value: missile.clone(),
                })?;
            // The core fires `srvmissile` after any do function, so the
            // data part must be the only one that does.
            cells.push(("srvmissile", String::new()));
            let lob = number(&src.skill("lob")).is_some_and(|n| n != 0)
                || src.skill("lob").trim().eq_ignore_ascii_case("true");
            format!("kind = \"missile\"\nmissile = {row}\nlob = {lob}\n")
        }
        Family::Charge => {
            // The engine runs at `param1` percent plus the unit's speed
            // bonus (at least 50) of its run speed; the bonus is the unit's.
            let speed = number(&src.skill("param1")).unwrap_or(0) + 50;
            cells.push(("srvstfunc", "0".to_owned()));
            notes.push(
                "the engine's Charge start function (`srvstfunc` 31) sets the unit running on \
                 its path, which the game's movement then walks; the lifted row drops it and \
                 the `rush` part runs the caster in steps (a fifth of its run velocity times \
                 the speed percent, a sub-tile at least, one a tick, at most 40) that place \
                 it, so a wall does not stop it and the unit's own speed bonus is not read. \
                 The hit is the Jab's melee hit in attack mode A1; the engine also plays \
                 overlay 147 on the target and sends the landing message"
                    .to_owned(),
            );
            let mut part = format!("kind = \"rush\"\nspeed = {speed}\nsteps = 40\n");
            part.push_str("[[skill.part]]\n");
            part.push_str(&melee_hit(src, &mut notes, Some("a1"), "calc1"));
            part
        }
        Family::MeleeState => melee_state(src, &mut cells, &mut notes)?,
        Family::Jab => melee_hit(src, &mut notes, Some("s1"), "calc1"),
        Family::Sequence => sequence(src, &mut cells, &mut notes)?,
        Family::Hunger => hunger(src, &mut notes),
        Family::Smite => {
            notes.push(
                "a player's Smite hits with the shield's damage in the engine; the data hit \
                 uses the weapon's. The data hit does not carry the row's `resultflags` and \
                 `hitflags`, and swings even when the target is out of reach"
                    .to_owned(),
            );
            format!(
                "kind = \"melee_hit\"\nstun_frames = {}\nmonster_mode = \"a2\"\n",
                quote(&formula(&src.skill("calc2")))
            )
        }
        Family::Wall => {
            let cell = src.skill("srvmissilea");
            let row = src.missile(cell.trim()).ok_or_else(|| LiftError::Unknown {
                column: "srvmissilea",
                value: cell.clone(),
            })?;
            let centre = src.skill("srvmissileb");
            let centre = if centre.trim().is_empty() {
                String::new()
            } else {
                let b = src
                    .missile(centre.trim())
                    .ok_or_else(|| LiftError::Unknown {
                        column: "srvmissileb",
                        value: centre.clone(),
                    })?;
                format!("centre = {b}\n")
            };
            if !missile.trim().is_empty() {
                cells.push(("srvmissile", String::new()));
                notes.push(
                    "the row's `srvmissile` is not fired: the data wall lays `srvmissilea` and \
                     `srvmissileb` alone, as the engine's does"
                        .to_owned(),
                );
            }
            format!("kind = \"wall\"\nmissile = {row}\n{centre}")
        }
        Family::Meteor => {
            let cell = src.skill("srvmissilea");
            let row = src.missile(cell.trim()).ok_or_else(|| LiftError::Unknown {
                column: "srvmissilea",
                value: cell.clone(),
            })?;
            // The core fires `srvmissile` after any do function; the engine's
            // meteor does not, so the data part must be the only missile.
            if !missile.trim().is_empty() {
                cells.push(("srvmissile", String::new()));
                notes.push(
                    "the row's `srvmissile` is not fired: the data meteor drops `srvmissilea` \
                     alone, as the engine's does"
                        .to_owned(),
                );
            }
            format!("kind = \"missile\"\nmissile = {row}\nat_aim = true\n")
        }
        Family::Inferno => {
            let cell = src.skill("srvmissilea");
            let row = src.missile(cell.trim()).ok_or_else(|| LiftError::Unknown {
                column: "srvmissilea",
                value: cell.clone(),
            })?;
            // The start function says which calc holds the channel's length.
            let length = if number(&src.skill("srvstfunc")) == Some(59) {
                "calc1"
            } else {
                "calc2"
            };
            let every = formula(&src.skill("calc3"));
            // A flame is fired before the length is looked at, so the one that finds it
            // used up is fired too.
            let times = format!(
                "({} + {every} - 1) / {every} + 1",
                formula(&src.skill(length))
            );
            notes.push(
                "the engine holds the monster in its channel animation and winds it down \
                 after the last flame; the data copy leaves the monster in the cast \
                 animation and fires a flame every `calc3` ticks until the channel's length \
                 is used up"
                    .to_owned(),
            );
            format!(
                "kind = \"repeat\"\nevery = {}\ntimes = {}\n[[skill.part]]\nkind = \"missile\"\nmissile = {row}\nchannel = true\n",
                quote(&every),
                quote(&times)
            )
        }
        Family::Nest => {
            let cell = src.skill("sumoverlay");
            let overlay = match (cell.trim(), number(&cell)) {
                ("", _) => String::new(),
                (_, Some(n)) => format!("overlay = {n}\n"),
                _ => {
                    return Err(LiftError::Unknown {
                        column: "sumoverlay",
                        value: cell,
                    })
                }
            };
            notes.push(
                "the lifted row keeps the engine's Nest start function (`srvstfunc` 49), which \
                 picks the monster, the place and the mode from the caster's `monstats` spawn \
                 columns and holds the caster still; the part hatches it"
                    .to_owned(),
            );
            format!("kind = \"nest\"\n{overlay}")
        }
        Family::Resurrect => {
            let cell = src.skill("srvoverlay");
            let overlay = match (cell.trim(), number(&cell)) {
                ("", _) => String::new(),
                (_, Some(n)) => format!("overlay = {n}\n"),
                _ => {
                    return Err(LiftError::Unknown {
                        column: "srvoverlay",
                        value: cell,
                    })
                }
            };
            format!("kind = \"resurrect\"\n{overlay}")
        }
        Family::Heal => {
            notes.push(
                "the engine's heal needs a unit target; the data heal does nothing without one"
                    .to_owned(),
            );
            format!(
                "kind = \"heal\"\nmin = {}\nmax = {}\n",
                quote(&formula(&src.skill("calc1"))),
                quote(&formula(&src.skill("calc2")))
            )
        }
        Family::Nova => {
            let cell = src.skill("srvmissilea");
            let row = src.missile(cell.trim()).ok_or_else(|| LiftError::Unknown {
                column: "srvmissilea",
                value: cell.clone(),
            })?;
            if src.skill("progressive").trim() == "1" {
                notes.push(
                    "the nova is progressive in the engine (its missile follows the charges); \
                     the data nova always fires `srvmissilea`"
                        .to_owned(),
                );
            }
            let vel = number(&src.missile_cell(row, "vel")).unwrap_or(0);
            let vellev = number(&src.missile_cell(row, "vellev")).unwrap_or(0);
            format!(
                "kind = \"missile_ring\"\nmissile = {row}\ncount = 64\nvelocity = {}\n",
                quote(&format!(
                    "{vel} + {vellev} * lvl / 8 + {}",
                    formula(&src.skill("calc1"))
                ))
            )
        }
        Family::Aura => {
            let cell = src.skill("aurastate");
            let state = src.state(cell.trim()).ok_or_else(|| LiftError::Unknown {
                column: "aurastate",
                value: cell.clone(),
            })?;
            let affects = affects_of(&src.skill("aurafilter"), &mut notes);
            notes.push(
                "the aura lasts `auralencalc` frames in the engine; the data aura counts whole \
                 seconds (rounded down)"
                    .to_owned(),
            );
            // The engine gives the caster `aurastate` and the units around
            // `auratargetstate`; with none, nobody else is reached.
            let target = src.skill("auratargetstate");
            let (target_state, range) = if target.trim().is_empty() {
                notes.push(
                    "the row has no `auratargetstate`, so the engine's aura reaches only the \
                     caster; the data aura has range 0"
                        .to_owned(),
                );
                (String::new(), "0".to_owned())
            } else {
                let row = src.state(target.trim()).ok_or_else(|| LiftError::Unknown {
                    column: "auratargetstate",
                    value: target.clone(),
                })?;
                let line = if row == state {
                    String::new()
                } else {
                    format!("target_state = {row}\n")
                };
                (line, formula(&src.skill("aurarangecalc")))
            };
            let mut part = format!(
                "kind = \"aura\"\nstate = {state}\n{target_state}range = {}\nseconds = {}\naffects = \"{affects}\"\n",
                quote(&range),
                quote(&format!(
                    "{} / {}",
                    formula(&src.skill("auralencalc")),
                    crate::TICKS_PER_SECOND
                )),
            );
            part.push_str(&aura_stats(src)?);
            part
        }
        Family::Curse => {
            let cell = src.skill("auratargetstate");
            let state = src.state(cell.trim()).ok_or_else(|| LiftError::Unknown {
                column: "auratargetstate",
                value: cell.clone(),
            })?;
            let affects = affects_of(&src.skill("aurafilter"), &mut notes);
            notes.push(
                "the curse lasts `auralencalc` frames in the engine; the data curse counts whole \
                 seconds (rounded down)"
                    .to_owned(),
            );
            // The engine changes a monster's AI for these three curse states
            // (Dim Vision, Terror, Taunt); the data part only puts the state.
            if matches!(state, 23 | 27 | 56) {
                notes.push(format!(
                    "state {state} also changes the monster's AI in the engine's curse; the data \
                     curse only puts the state and its stats on"
                ));
            }
            let mut part = format!(
                "kind = \"curse\"\nstate = {state}\nrange = {}\nseconds = {}\naffects = \"{affects}\"\n",
                quote(&formula(&src.skill("aurarangecalc"))),
                quote(&format!(
                    "{} / {}",
                    formula(&src.skill("auralencalc")),
                    crate::TICKS_PER_SECOND
                )),
            );
            part.push_str(&aura_stats(src)?);
            part
        }
    };
    let toml = format!(
        "[[skill]]\nid = {id}\nname = {}\n[[skill.part]]\n{part}",
        quote(if name.trim().is_empty() {
            family.name()
        } else {
            name.trim()
        })
    );
    SkillRules::from_toml(&toml).map_err(|e| LiftError::Unreadable(e.to_string()))?;
    Ok(Lifted {
        id,
        family,
        toml,
        cells,
        notes,
    })
}

/// Melee skills with a state or an overlay on the target (`srvdofunc` 2): the
/// hit of the row's start function (6: Power Strike, 32: Bash, Stun,
/// Concentrate), and what the do function adds on a hit that lands.
fn melee_state(
    src: &dyn Source,
    cells: &mut Vec<(&'static str, String)>,
    notes: &mut Vec<String>,
) -> Result<String, LiftError> {
    let start = number(&src.skill("srvstfunc")).unwrap_or(0);
    if !matches!(start, 6 | 32) {
        return Err(LiftError::Needs(format!(
            "the melee start function {start} is not lifted (only 6 and 32 are)"
        )));
    }
    if !src.skill("aurastate").trim().is_empty() {
        return Err(LiftError::Needs(
            "the row's own state (`aurastate`) lasts to the unit's next mode change, which a \
             timed state cannot say"
                .to_owned(),
        ));
    }
    // The engine adds `calc2` points to the hit's stored record after the
    // enhanced damage; the closeness fixture does not see them dealt, and a
    // `damage` part would be multiplied by the enhanced damage.
    if start == 32 && !src.skill("calc2").trim().is_empty() {
        return Err(LiftError::Needs(
            "the row adds `calc2` points of damage after the enhanced damage, which the melee \
             hit cannot say yet"
                .to_owned(),
        ));
    }
    // The start function builds the hit; the part does, so the row drops it.
    cells.push(("srvstfunc", "0".to_owned()));
    let mut part = melee_hit(src, notes, None, "calc1");
    let mut scalars = String::new();
    let mut tables = String::new();
    if start == 32 {
        for (key, column) in [
            ("result_flags", "resultflags"),
            ("hit_flags", "hitflags"),
            ("hit_class", "hitclass"),
        ] {
            if let Some(n) = number(&src.skill(column)).filter(|&n| n != 0) {
                scalars.push_str(&format!("{key} = {n}\n"));
            }
        }
        if !src.skill("calc3").trim().is_empty() {
            notes.push(
                "the engine gives the caster a temporary attack-rate bonus (`calc3`) until \
                 its next mode change; the data hit does not"
                    .to_owned(),
            );
        }
    }
    let overlay = src.skill("srvoverlay");
    if !overlay.trim().is_empty() {
        let n = number(&overlay).ok_or_else(|| LiftError::Unknown {
            column: "srvoverlay",
            value: overlay.clone(),
        })?;
        scalars.push_str(&format!("overlay = {n}\n"));
    }
    let cell = src.skill("auratargetstate");
    if !cell.trim().is_empty() {
        let state = src.state(cell.trim()).ok_or_else(|| LiftError::Unknown {
            column: "auratargetstate",
            value: cell.clone(),
        })?;
        scalars.push_str(&format!(
            "target_state = {state}\ntarget_frames = {}\n",
            quote(&formula(&src.skill("auralencalc")))
        ));
        tables.push_str(&aura_stats_as(src, "target_stat")?);
    }
    part.push_str(&scalars);
    part.push_str(&tables);
    Ok(part)
}

/// A melee hit that also takes life (`calc2`) and mana (`calc3`) from the target
/// (`srvdofunc` 122).
fn hunger(src: &dyn Source, notes: &mut Vec<String>) -> String {
    notes.push(
        "the engine's Hunger does nothing on a miss; the data hit counts a miss as an attack"
            .to_owned(),
    );
    let mut part = melee_hit(src, notes, None, "calc1");
    for (key, column) in [("life_leech", "calc2"), ("mana_leech", "calc3")] {
        let f = src.skill(column);
        if !f.trim().is_empty() {
            part.push_str(&format!("{key} = {}\n", quote(&formula(&f))));
        }
    }
    part
}

/// Melee hits in a row (`srvdofunc` 13): the start function (9 Fend, 37 Zeal)
/// counts the enemies in reach up to `calc1`, and each swing the do function
/// makes hits the next one with `calc2` as the enhanced damage.
fn sequence(
    src: &dyn Source,
    cells: &mut Vec<(&'static str, String)>,
    notes: &mut Vec<String>,
) -> Result<String, LiftError> {
    let start = number(&src.skill("srvstfunc")).unwrap_or(0);
    if !matches!(start, 9 | 37) {
        return Err(LiftError::Needs(format!(
            "the sequence start function {start} is not lifted (only 9 and 37 are)"
        )));
    }
    cells.push(("srvstfunc", "0".to_owned()));
    notes.push(
        "the engine swings once per animation pass and rewinds it for the next enemy; the \
         data hit makes every swing in the one cast"
            .to_owned(),
    );
    let mut part = melee_hit(src, notes, None, "calc2");
    part.push_str(&format!(
        "sequence = {}\n",
        quote(&formula(&src.skill("calc1")))
    ));
    Ok(part)
}

/// A `melee_hit` part for the row: the weapon's damage at `srcdam` percent with
/// the row's enhanced damage (the formula in column `enhance`), its own
/// element and its to-hit, in the attack mode `mode` when a monster casts it.
/// It ends in scalar lines, so a caller can add more before it adds a table.
fn melee_hit(
    src: &dyn Source,
    notes: &mut Vec<String>,
    mode: Option<&str>,
    enhance: &str,
) -> String {
    let srcdam = number(&src.skill("srcdam")).unwrap_or(0);
    let percent = if srcdam == 0 {
        100
    } else {
        (srcdam * 100 + 64) / 128
    };
    if percent > 400 {
        notes.push(format!(
            "`srcdam` is {percent}% of the weapon's damage; the data hit holds at 400%"
        ));
    }
    let etype = src.skill("etype");
    if !etype.trim().is_empty() && etype.trim() != "0" {
        notes.push(
            "the row converts damage to an element (`etype` with `calc4`); the data hit \
             adds the row's own elemental damage but does not convert"
                .to_owned(),
        );
    }
    let mut part = format!(
        "kind = \"melee_hit\"\nweapon_percent = {}\nenhance = {}\nown_element = true\n",
        percent.min(400),
        quote(&formula(&src.skill(enhance))),
    );
    if let Some(mode) = mode {
        part.push_str(&format!("monster_mode = \"{mode}\"\n"));
    }
    part.push_str(&to_hit_line(src));
    part
}

/// The `attack_rating` line of a melee hit from the row's `tohitcalc`, or
/// `tohit` and `levtohit` per level; empty when the row adds nothing.
fn to_hit_line(src: &dyn Source) -> String {
    let calc = src.skill("tohitcalc");
    if !calc.trim().is_empty() {
        return format!("attack_rating = {}\n", quote(&formula(&calc)));
    }
    let base = number(&src.skill("tohit")).unwrap_or(0);
    let per = number(&src.skill("levtohit")).unwrap_or(0);
    if base == 0 && per == 0 {
        return String::new();
    }
    format!(
        "attack_rating = {}\n",
        quote(&format!("{base} + (lvl - 1) * {per}"))
    )
}

/// The `[[skill.part.stat]]` entries of the row's `aurastat1` to `aurastat6`
/// (and their `aurastatcalc` formulas), up to the first empty one.
fn aura_stats(src: &dyn Source) -> Result<String, LiftError> {
    aura_stats_as(src, "stat")
}

/// The same, as the `[[skill.part.<table>]]` entries of the part.
fn aura_stats_as(src: &dyn Source, table: &str) -> Result<String, LiftError> {
    let mut out = String::new();
    for n in 1..=6 {
        let stat = src.skill(&format!("aurastat{n}"));
        if stat.trim().is_empty() {
            break;
        }
        let row = src.stat(stat.trim()).ok_or_else(|| LiftError::Unknown {
            column: "aurastat",
            value: stat.clone(),
        })?;
        out.push_str(&format!(
            "[[skill.part.{table}]]\nstat = {row}\nvalue = {}\n",
            quote(&formula(&src.skill(&format!("aurastatcalc{n}"))))
        ));
    }
    Ok(out)
}

/// The `affects` name of an `aurafilter` cell. A mask that is none of the
/// four filters `affects` offers gets the nearest by its allied, hostile and
/// unit-type bits (allies when none is set, the engine's aura default), with a
/// note.
fn affects_of(cell: &str, notes: &mut Vec<String>) -> &'static str {
    let bits = number(cell).unwrap_or(0);
    let all = [
        (Affects::Allies, "allies"),
        (Affects::Enemies, "enemies"),
        (Affects::Everyone, "everyone"),
        (Affects::AlliedPlayers, "allied_players"),
    ];
    for (a, name) in all {
        if a.filter() == bits {
            return name;
        }
    }
    // 0x1 players, 0x2 monsters, 0x8000 hostile, 0x10000 allied.
    let near = if bits & 0x8000 != 0 {
        Affects::Enemies
    } else if bits & 0x10000 != 0 {
        if bits & 0x2 == 0 {
            Affects::AlliedPlayers
        } else {
            Affects::Allies
        }
    } else if bits & 0x3 != 0 {
        Affects::Everyone
    } else {
        Affects::Allies
    };
    let name = all
        .iter()
        .find(|(a, _)| *a == near)
        .map_or("allies", |x| x.1);
    notes.push(format!(
        "`aurafilter` {bits:#x} is not one of the four filters `affects` offers; `{name}` is the nearest          (the extra bits of the mask are left out)"
    ));
    name
}

/// One row of the `skills` table, for the usage ranking.
#[derive(Clone, Copy, Debug)]
pub struct SkillUse<'a> {
    /// The row's `skill` name (what `monstats` lists).
    pub name: &'a str,
    pub slot: u16,
    /// The class that owns it (`charclass`); empty for a monster skill.
    pub class: &'a str,
}

/// How many classes and monsters use the skills of one do function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotUse {
    pub slot: u16,
    /// Skills rows with this slot.
    pub skills: u32,
    pub classes: u32,
    pub monsters: u32,
}

impl SlotUse {
    pub fn total(&self) -> u32 {
        self.classes + self.monsters
    }
}

/// The do functions ranked by the classes and monsters that use them, the
/// most used first (ties by slot). `monsters` is one list of skill names
/// per `monstats` row.
pub fn rank(skills: &[SkillUse<'_>], monsters: &[Vec<&str>]) -> Vec<SlotUse> {
    let slot_of: BTreeMap<&str, u16> = skills.iter().map(|s| (s.name, s.slot)).collect();
    let mut rows: BTreeMap<u16, (u32, BTreeSet<&str>, u32)> = BTreeMap::new();
    for s in skills {
        let e = rows.entry(s.slot).or_default();
        e.0 += 1;
        if !s.class.is_empty() {
            e.1.insert(s.class);
        }
    }
    for m in monsters {
        let slots: BTreeSet<u16> = m.iter().filter_map(|n| slot_of.get(n).copied()).collect();
        for slot in slots {
            rows.entry(slot).or_default().2 += 1;
        }
    }
    let mut out: Vec<SlotUse> = rows
        .into_iter()
        .map(|(slot, (skills, classes, monsters))| SlotUse {
            slot,
            skills,
            classes: classes.len() as u32,
            monsters,
        })
        .collect();
    out.sort_by(|a, b| b.total().cmp(&a.total()).then(a.slot.cmp(&b.slot)));
    out
}

#[cfg(test)]
mod tests;
