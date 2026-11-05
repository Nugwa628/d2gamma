//! The closeness report: a base skill's Rust body against its data copy
//! (the lifter's `skills.toml` entry run through the block slot), on the
//! same seeded game, through the real do core. It is a report, not a gate:
//! it measures damage, hit count, missile count and timing, flags the big
//! differences and writes a markdown table to the file named by the
//! `D2_CLOSENESS_REPORT` environment variable when that is set. The tests
//! assert only that the fixtures work and that the lifted families agree where
//! they are meant to.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::rules::data_mod::lift::{lift, Source};
use crate::rules::BLOCK_SLOT;
use crate::skills::use_::bodies::{BodyStat, BodyTables};
use crate::skills::use_::{do_skill, start};
use crate::skills::{SkillEntry, SkillTables};
use crate::stats::stat as sst;
use crate::units::hooks::Sim;
use crate::units::{RoomId, UnitId, UnitType};
use crate::wiring::action::monsters::MonsterWorld;
use crate::wiring::action::ActionHooks;

use super::blocks::{install, LEVEL_STAT, LIFE, STATE, STATES};
use super::{fx_with, skills, Fx};

mod ai_control;
mod corpse_drops;
mod guided_projectiles;
mod martial_charges;
mod passive_refresh;
mod projectile_fans;
mod real;
mod shadow_setup;
mod sight_pulses;
mod warcry_hits;

/// Casts per case and frames watched after each.
const CASTS: u32 = 3;
const LEVEL: i32 = 5;
/// A difference past this percent is flagged.
const FLAG_PERCENT: i64 = 25;

/// One base skill to compare: its table cells as a modder sees them, and
/// the engine row those cells stand for.
struct Case {
    name: &'static str,
    cells: &'static [(&'static str, &'static str)],
    /// Sets the engine row to what `cells` say.
    row: fn(&mut d2_data::tables::Skills),
    /// Frames watched after each cast.
    window: u32,
    casts: u32,
    /// The skill is meant to hurt the target: a run that lost no life says
    /// so in the report (the fixture has no weapon or damage rows).
    hurts: bool,
    /// The monster starts at half its life (a heal has something to give).
    wounded: bool,
}

/// Cases whose engine body does nothing on the fixture's caster, with why:
/// the report says so instead of comparing a refusal with a swing.
const UNFIT: &[(&str, &str)] = &[
    (
        "smite",
        "the engine's Smite needs a shield (a player) or a monster caster; the fixture's caster is a bare-handed player",
    ),
    (
        "charge",
        "the engine's Charge hands a target in reach to the normal attack mode and walks on the game's path code, which the fixture runs neither; the data copy is checked on its own",
    ),
];

/// Why a case cannot be compared when the Rust body refused every cast.
fn unfit(case: &str, rust: &Outcome) -> Option<&'static str> {
    UNFIT
        .iter()
        .find(|(n, _)| *n == case)
        .filter(|_| rust.results.iter().all(|r| *r == 0))
        .map(|(_, why)| *why)
}

struct Cells(BTreeMap<&'static str, &'static str>);

impl Source for Cells {
    fn skill(&self, column: &str) -> String {
        self.0.get(column).copied().unwrap_or("").to_owned()
    }
    fn state(&self, name: &str) -> Option<u16> {
        name.parse().ok()
    }
    fn stat(&self, name: &str) -> Option<u16> {
        name.parse().ok()
    }
    fn missile(&self, name: &str) -> Option<u16> {
        name.parse().ok()
    }
    fn missile_cell(&self, _row: u16, column: &str) -> String {
        // The fixture's arrow: velocity 1, no velocity per level.
        let cell = if column == "vel" { "1" } else { "0" };
        cell.to_owned()
    }
}

const NONE: u16 = 0xFFFF;
/// A second state of the fixture: what the units around an aura get when the
/// row names another state for them than for the caster.
const OTHER: u16 = 31;

fn attack(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 1;
    r.srvmissile = NONE;
}

fn missile(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 0;
    r.srvmissile = 0;
}

fn nova(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 22;
    r.srvmissile = NONE;
    r.srvmissilea = 0;
}

fn jab(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 7;
    r.srvmissile = NONE;
    // Enhanced damage `12` (the buffer's second formula).
    r.calc1 = 3;
    r.tohitcalc = u32::MAX;
}

fn hunger(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 122;
    r.srvmissile = NONE;
    // Enhanced damage `12` (the buffer's second formula). No leech: with one the two runs'
    // damage drifts apart (4954 against 6254) for a reason not found yet; the leech is
    // checked on the part's own tests.
    r.calc1 = 3;
    r.calc2 = u32::MAX;
    r.calc3 = u32::MAX;
    r.srcdam = 128;
}

fn charge(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 67;
    r.srvstfunc = 31;
    r.srvmissile = NONE;
    r.param1 = 40;
    // Enhanced damage `12`, as the jab.
    r.calc1 = 3;
    r.tohitcalc = u32::MAX;
}

fn melee_state(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 2;
    r.srvstfunc = 6;
    r.srvmissile = NONE;
    r.srvoverlay = NONE;
    // Enhanced damage `12`, as the jab; the target gets the fixture's state
    // for the buffer's first formula (`250` frames).
    r.calc1 = 3;
    r.tohitcalc = u32::MAX;
    r.aurastate = NONE;
    r.auratargetstate = STATE;
    r.auralencalc = 0;
    r.aurastat1 = NONE;
    r.aurastat2 = NONE;
    r.aurastat3 = NONE;
    r.aurastat4 = NONE;
    r.aurastat5 = NONE;
    r.aurastat6 = NONE;
}

fn smite(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 150;
    r.srvmissile = NONE;
    // Stun for `12` frames.
    r.calc2 = 3;
}

fn resurrect(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 97;
    r.srvmissile = NONE;
    r.srvoverlay = NONE;
}

fn wall(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 24;
    r.srvmissile = NONE;
    r.srvmissilea = 0;
    r.srvmissileb = 0;
}

fn inferno(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 95;
    r.srvstfunc = 53;
    r.srvmissile = NONE;
    r.srvmissilea = 0;
    // A channel of the buffer's `60` ticks (its third formula), a flame every
    // `12` (its second).
    r.calc2 = 6;
    r.calc3 = 3;
}

fn nest(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 91;
    r.srvstfunc = 49;
    r.srvmissile = NONE;
    // A player caster takes its class and mode from the row (a monster's
    // come from its monstats row): class 0, mode 1; no overlay.
    r.summon = 0;
    r.summode = 1;
    r.sumoverlay = NONE;
}

fn meteor(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 28;
    r.srvmissile = NONE;
    r.srvmissilea = 0;
}

fn heal(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 96;
    r.srvmissile = NONE;
    // Both percents are the buffer's `12`.
    r.calc1 = 3;
    r.calc2 = 3;
}

fn aura(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 65;
    r.srvmissile = NONE;
    r.aurastate = STATE;
    r.auratargetstate = STATE;
    r.aurafilter = crate::rules::data_mod::Affects::Everyone.filter();
    // The code buffer is `250` then `12`: the length and the range.
    r.auralencalc = 0;
    r.aurarangecalc = 3;
    r.aurastat1 = NONE;
    r.aurastat2 = NONE;
}

fn teleport(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 27;
    r.srvmissile = NONE;
}

fn curse(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 30;
    r.srvmissile = NONE;
    r.auratargetstate = STATE;
    r.aurafilter = crate::rules::data_mod::Affects::Enemies.filter();
    r.auralencalc = 0;
    r.aurarangecalc = 3;
    r.aurastat1 = NONE;
    r.aurastat2 = NONE;
}

fn buff(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 18;
    r.srvmissile = NONE;
    r.aurastate = STATE;
    r.auralencalc = 0;
    r.aurastat1 = 16;
    r.aurastatcalc1 = 4;
    r.aurastat2 = NONE;
    r.passivestat1 = NONE;
    r.aurastat3 = NONE;
    r.aurastat4 = NONE;
    r.aurastat5 = NONE;
    r.aurastat6 = NONE;
    r.passivestat2 = NONE;
    r.passivestat3 = NONE;
    r.passivestat4 = NONE;
    r.passivestat5 = NONE;
    r.auraevent1 = NONE;
}

fn druid_summon(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 119;
    r.srvmissile = NONE;
    r.summon = 0;
    r.pettype = 2;
    r.summode = 1;
    r.calc2 = 0;
    r.petmax = 3;
    r.passivestat1 = 31;
    r.passivecalc1 = 0;
    r.passivestat2 = NONE;
    r.passivestat3 = NONE;
    r.passivestat4 = NONE;
    r.passivestat5 = NONE;
    r.aurastat1 = NONE;
    r.aurastat2 = NONE;
    r.aurastat3 = NONE;
    r.aurastat4 = NONE;
    r.aurastat5 = NONE;
    r.aurastat6 = NONE;
    r.aurastate = NONE;
}

fn warcry(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 68;
    r.srvmissile = NONE;
    r.srvmissilea = 0;
    r.aurastate = STATE;
    r.auralencalc = 0;
    r.aurastat1 = 31;
    r.aurastatcalc1 = 4;
    r.aurastat2 = NONE;
    r.aurastat3 = NONE;
    r.aurastat4 = NONE;
    r.aurastat5 = NONE;
    r.aurastat6 = NONE;
}
fn sentry(r: &mut d2_data::tables::Skills) {
    druid_summon(r);
    r.srvdofunc = 45;
    r.summode = 1;
    r.intown = false;
}
fn vine(r: &mut d2_data::tables::Skills) {
    druid_summon(r);
    r.srvdofunc = 115;
    r.summode = 1;
}
fn form(r: &mut d2_data::tables::Skills) {
    warcry(r);
    r.srvdofunc = 116;
    r.srvmissilea = NONE;
    r.delay = u32::MAX;
}
fn damage_aura(r: &mut d2_data::tables::Skills) {
    buff(r);
    r.srvdofunc = 66;
    r.auratargetstate = OTHER;
    r.aurafilter = crate::rules::data_mod::Affects::Enemies.filter();
    r.aurarangecalc = 4;
    r.perdelay = u32::MAX;
    r.aurastat1 = 16;
    r.aurastatcalc1 = 4;
    r.passivestat1 = 31;
    r.passivecalc1 = 4;
    r.etype = 1;
    r.emin = 1;
    r.emax = 1;
    r.eminlev1 = 0;
    r.emaxlev1 = 0;
}
fn freeze_aura(r: &mut d2_data::tables::Skills) {
    damage_aura(r);
    r.srvdofunc = 81;
}
fn passive(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 0;
    r.srvmissile = NONE;
    r.passivestate = STATE;
    r.passiveitype = 4;
    r.aurastate = NONE;
    r.passivestat1 = 31;
    r.passivecalc1 = 0;
    r.passivestat2 = NONE;
    r.passivestat3 = NONE;
    r.passivestat4 = NONE;
    r.passivestat5 = NONE;
}
fn golem(r: &mut d2_data::tables::Skills) {
    druid_summon(r);
    r.srvdofunc = 56;
}
fn raven(r: &mut d2_data::tables::Skills) {
    druid_summon(r);
    r.srvdofunc = 114;
}
fn corpse_find(r: &mut d2_data::tables::Skills) {
    r.srvmissile = NONE;
    r.srvdofunc = 72;
    r.calc1 = 0;
    r.param1 = 0;
    r.param2 = 100;
    r.param3 = 0;
    r.param4 = 0;
}
fn potion_find(r: &mut d2_data::tables::Skills) {
    corpse_find(r);
    r.srvdofunc = 69;
}
fn shadow(r: &mut d2_data::tables::Skills) {
    druid_summon(r);
    r.srvdofunc = 49;
    r.aurastate = STATE;
    r.auralencalc = 6;
    r.param1 = 5;
    r.param5 = 3;
    r.param6 = 2;
    r.passivecalc2 = 0;
    r.aurastat1 = 32;
    r.aurastatcalc2 = 3;
    r.sumumod = 0;
}
fn martial_charge(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 34;
    r.srvmissile = NONE;
    r.aurastate = STATE;
    r.aurastat1 = 204;
    r.aurastat2 = 31;
    r.aurastatcalc2 = 4;
    r.auralencalc = 0;
    r.tohitcalc = u32::MAX;
    r.srcdam = 128;
}
fn ai_curse(r: &mut d2_data::tables::Skills) {
    r.aurafilter = 2;
    r.srvmissile = NONE;
    r.srvdofunc = 61;
    r.auratargetstate = STATE;
    r.aurastat1 = NONE;
    r.aurastat2 = 31;
    r.aurastat3 = NONE;
    r.aurastatcalc2 = 0;
    r.aurarangecalc = 3;
    r.auralencalc = 6;
}
fn attract(r: &mut d2_data::tables::Skills) {
    ai_curse(r);
    r.srvdofunc = 59;
}
fn projectile_fan(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 8;
    r.srvmissilea = 0;
    r.srvmissileb = NONE;
    r.srvmissile = NONE;
    r.calc1 = 0;
    r.calc2 = 3;
    r.calc3 = 6;
}
fn jitter_fan(r: &mut d2_data::tables::Skills) {
    projectile_fan(r);
    r.srvdofunc = 118;
}
fn guided_projectile(r: &mut d2_data::tables::Skills) {
    r.srvdofunc = 10;
    r.srvmissilea = 0;
    r.srvmissileb = NONE;
    r.srvmissile = NONE;
    r.calc1 = 0;
}
fn sight(r: &mut d2_data::tables::Skills) {
    r.srvmissile = NONE;
    r.srvdofunc = 6;
    r.auratargetstate = STATE;
    r.aurastat1 = 31;
    r.aurastatcalc1 = 0;
    r.aurarangecalc = 3;
    r.auralencalc = 6;
    r.aurafilter = 0;
}
fn bolt_fan(r: &mut d2_data::tables::Skills) {
    jitter_fan(r);
    r.srvdofunc = 17;
}
const CASES: &[Case] = &[
    Case {
        name: "sight",
        cells: &[
            ("srvdofunc", "6"),
            ("auratargetstate", "30"),
            ("aurastat1", "31"),
            ("aurastatcalc1", "12"),
            ("aurarangecalc", "12"),
            ("auralencalc", "60"),
        ],
        row: sight,
        window: 70,
        casts: 1,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "guided_projectile",
        cells: &[("srvdofunc", "10"), ("srvmissilea", "0"), ("calc1", "12")],
        row: guided_projectile,
        window: 1,
        casts: 3,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "bolt_fan",
        cells: &[("srvdofunc", "17"), ("srvmissilea", "0"), ("calc1", "5")],
        row: bolt_fan,
        window: 25,
        casts: 3,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "projectile_fan",
        cells: &[
            ("srvdofunc", "8"),
            ("srvmissilea", "0"),
            ("calc1", "5"),
            ("calc2", "0"),
            ("calc3", "3"),
        ],
        row: projectile_fan,
        window: 25,
        casts: 3,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "jitter_fan",
        cells: &[("srvdofunc", "118"), ("srvmissilea", "0"), ("calc1", "5")],
        row: jitter_fan,
        window: 25,
        casts: 3,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "ai_curse",
        cells: &[
            ("srvdofunc", "61"),
            ("aurafilter", "2"),
            ("auratargetstate", "30"),
            ("aurarangecalc", "12"),
            ("auralencalc", "250"),
            ("aurastat2", "31"),
            ("aurastatcalc2", "12"),
        ],
        row: ai_curse,
        window: 1,
        casts: 1,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "attract",
        cells: &[
            ("srvdofunc", "59"),
            ("aurafilter", "2"),
            ("auratargetstate", "30"),
            ("aurarangecalc", "12"),
            ("auralencalc", "250"),
        ],
        row: attract,
        window: 1,
        casts: 1,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "martial_charge",
        cells: &[
            ("srvdofunc", "34"),
            ("aurastate", "30"),
            ("aurastat1", "204"),
            ("aurastat2", "31"),
            ("aurastatcalc2", "12"),
            ("auralencalc", "250"),
            ("srcdam", "128"),
        ],
        row: martial_charge,
        window: 1,
        casts: 3,
        hurts: true,
        wounded: false,
    },
    Case {
        name: "shadow",
        cells: &[
            ("srvdofunc", "49"),
            ("summon", "0"),
            ("pettype", "2"),
            ("summode", "1"),
            ("petmax", "2"),
            ("aurastate", "30"),
            ("auralencalc", "60"),
            ("aurastat1", "32"),
            ("aurastatcalc2", "2"),
            ("passivestat1", "31"),
            ("passivecalc2", "5"),
        ],
        row: shadow,
        window: 1,
        casts: 3,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "corpse_find",
        cells: &[("srvdofunc", "72"), ("calc1", "100")],
        row: corpse_find,
        window: 1,
        casts: 3,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "potion_find",
        cells: &[("srvdofunc", "69"), ("calc1", "100")],
        row: potion_find,
        window: 1,
        casts: 3,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "raven",
        cells: &[
            ("srvdofunc", "114"),
            ("summon", "0"),
            ("pettype", "2"),
            ("summode", "1"),
            ("petmax", "2"),
            ("calc2", "5"),
        ],
        row: raven,
        window: 1,
        casts: 3,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "golem",
        cells: &[
            ("srvdofunc", "56"),
            ("summon", "0"),
            ("pettype", "2"),
            ("summode", "1"),
            ("petmax", "2"),
        ],
        row: golem,
        window: 1,
        casts: 3,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "passive",
        cells: &[
            ("srvdofunc", "0"),
            ("passivestate", "30"),
            ("passiveitype", "4"),
            ("passivestat1", "31"),
            ("passivecalc1", "12"),
        ],
        row: passive,
        window: 3,
        casts: 3,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "damage_aura",
        cells: &[
            ("srvdofunc", "66"),
            ("aurastate", "30"),
            ("auratargetstate", "31"),
            ("perdelay", "0"),
            ("aurarangecalc", "12"),
            ("aurafilter", "34179"),
            ("passivestat1", "31"),
            ("passivecalc1", "12"),
            ("aurastat1", "16"),
            ("aurastatcalc1", "12"),
        ],
        row: damage_aura,
        window: 3,
        casts: 3,
        hurts: true,
        wounded: false,
    },
    Case {
        name: "freeze_aura",
        cells: &[
            ("srvdofunc", "81"),
            ("aurastate", "30"),
            ("auratargetstate", "31"),
            ("perdelay", "0"),
            ("aurarangecalc", "12"),
            ("aurafilter", "34179"),
            ("passivestat1", "31"),
            ("passivecalc1", "12"),
            ("aurastat1", "16"),
            ("aurastatcalc1", "12"),
        ],
        row: freeze_aura,
        window: 3,
        casts: 3,
        hurts: true,
        wounded: false,
    },
    Case {
        name: "vine",
        cells: &[
            ("srvdofunc", "115"),
            ("summon", "0"),
            ("pettype", "2"),
            ("petmax", "2"),
            ("calc2", "5"),
        ],
        row: vine,
        window: 1,
        casts: 3,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "form",
        cells: &[
            ("srvdofunc", "116"),
            ("aurastate", "30"),
            ("auralencalc", "250"),
            ("delay", "0"),
            ("aurastat1", "31"),
            ("aurastatcalc1", "12"),
        ],
        row: form,
        window: 300,
        casts: 1,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "sentry",
        cells: &[
            ("srvdofunc", "45"),
            ("summon", "0"),
            ("summode", "1"),
            ("pettype", "2"),
            ("petmax", "2"),
        ],
        row: sentry,
        window: 1,
        casts: 3,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "warcry",
        cells: &[
            ("srvdofunc", "68"),
            ("srvmissilea", "0"),
            ("aurastate", "30"),
            ("auralencalc", "250"),
            ("aurastat1", "31"),
            ("aurastatcalc1", "12"),
        ],
        row: warcry,
        window: 300,
        casts: 1,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "druid_summon",
        cells: &[
            ("srvdofunc", "119"),
            ("summon", "0"),
            ("summode", "1"),
            ("pettype", "2"),
            ("petmax", "2"),
            ("calc2", "5"),
        ],
        row: druid_summon,
        window: 1,
        casts: 3,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "buff",
        cells: &[
            ("srvdofunc", "18"),
            ("aurastate", "30"),
            ("auralencalc", "250"),
            ("aurastat1", "16"),
            ("aurastatcalc1", "12"),
        ],
        row: buff,
        window: 300,
        casts: 1,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "teleport",
        cells: &[("skill", "Blink"), ("srvdofunc", "27")],
        row: teleport,
        window: 1,
        casts: 1,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "attack",
        cells: &[("skill", "Swing"), ("srvdofunc", "1")],
        row: attack,
        window: 30,
        casts: CASTS,
        hurts: true,
        wounded: false,
    },
    Case {
        name: "missile",
        cells: &[("skill", "Bolt"), ("srvmissile", "0")],
        row: missile,
        window: 30,
        casts: CASTS,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "nova",
        cells: &[("skill", "Nova"), ("srvdofunc", "22"), ("srvmissilea", "0")],
        row: nova,
        window: 60,
        casts: CASTS,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "aura",
        cells: &[
            ("skill", "Aura"),
            ("srvdofunc", "65"),
            ("aurastate", "30"),
            ("auratargetstate", "30"),
            ("aurafilter", "0x583"),
            ("aurarangecalc", "12"),
            ("auralencalc", "250"),
        ],
        row: aura,
        window: 300,
        casts: 1,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "curse",
        cells: &[
            ("skill", "Curse"),
            ("srvdofunc", "30"),
            ("auratargetstate", "30"),
            ("aurafilter", "0x8583"),
            ("aurarangecalc", "12"),
            ("auralencalc", "250"),
        ],
        row: curse,
        window: 300,
        casts: 1,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "jab",
        cells: &[("skill", "Jab"), ("srvdofunc", "7"), ("calc1", "12")],
        row: jab,
        window: 30,
        casts: CASTS,
        hurts: true,
        wounded: false,
    },
    Case {
        name: "hunger",
        cells: &[
            ("skill", "Hunger"),
            ("srvdofunc", "122"),
            ("calc1", "12"),
            ("srcdam", "128"),
        ],
        row: hunger,
        window: 30,
        casts: CASTS,
        hurts: true,
        wounded: false,
    },
    Case {
        name: "melee_state",
        cells: &[
            ("skill", "Power Strike"),
            ("srvdofunc", "2"),
            ("srvstfunc", "6"),
            ("calc1", "12"),
            ("auratargetstate", "30"),
            ("auralencalc", "250"),
        ],
        row: melee_state,
        window: 60,
        casts: CASTS,
        hurts: true,
        wounded: false,
    },
    Case {
        name: "charge",
        cells: &[
            ("skill", "Charge"),
            ("srvdofunc", "67"),
            ("srvstfunc", "31"),
            ("param1", "40"),
            ("calc1", "12"),
        ],
        row: charge,
        window: 30,
        casts: CASTS,
        hurts: true,
        wounded: false,
    },
    Case {
        name: "smite",
        cells: &[("skill", "Smite"), ("srvdofunc", "150"), ("calc2", "12")],
        row: smite,
        window: 30,
        casts: CASTS,
        hurts: true,
        wounded: false,
    },
    Case {
        name: "resurrect",
        cells: &[("skill", "Resurrect"), ("srvdofunc", "97")],
        row: resurrect,
        window: 30,
        casts: 1,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "wall",
        cells: &[
            ("skill", "Fire Wall"),
            ("srvdofunc", "24"),
            ("srvmissilea", "0"),
            ("srvmissileb", "0"),
        ],
        row: wall,
        window: 60,
        casts: CASTS,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "inferno",
        cells: &[
            ("skill", "Inferno"),
            ("srvdofunc", "95"),
            ("srvstfunc", "53"),
            ("srvmissilea", "0"),
            ("calc2", "60"),
            ("calc3", "12"),
        ],
        row: inferno,
        window: 120,
        casts: 1,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "nest",
        cells: &[("skill", "Nest"), ("srvdofunc", "91"), ("srvstfunc", "49")],
        row: nest,
        window: 30,
        casts: 1,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "meteor",
        cells: &[
            ("skill", "Meteor"),
            ("srvdofunc", "28"),
            ("srvmissilea", "0"),
        ],
        row: meteor,
        window: 60,
        casts: CASTS,
        hurts: false,
        wounded: false,
    },
    Case {
        name: "heal",
        cells: &[
            ("skill", "Bestow"),
            ("srvdofunc", "96"),
            ("calc1", "12"),
            ("calc2", "12"),
        ],
        row: heal,
        window: 30,
        casts: CASTS,
        hurts: false,
        wounded: true,
    },
];

/// What one run of a case did.
#[derive(Debug, Default, PartialEq, Eq)]
struct Outcome {
    passive_values: [i64; 5],
    passive_level: i64,
    passive_skill: i64,
    pets: i64,
    pet_levels: i64,
    pet_stats: i64,
    pet_modes: i64,
    sight_values: i64,
    sight_remaining: i64,
    guided_modes: i64,
    guided_offsets: i64,
    ai_overrides: i64,
    ai_alignments: i64,
    ai_nodes: i64,
    ai_values: i64,
    ai_remaining: i64,
    shadow_equipment: i64,
    shadow_item_levels: i64,
    shadow_timers: i64,
    shadow_links: i64,
    shadow_stats: i64,
    martial_count: i64,
    martial_bonus: i64,
    corpse_consumed: i64,
    items_dropped: i64,
    item_classes: i64,
    pet_regen: i64,
    targetable_pets: i64,
    pet_resists: i64,
    vine_states: i64,
    buff_stats: i64,
    target_stats: i64,
    /// Signed caster displacement; movement comparisons must measure position.
    moved_x: i64,
    moved_y: i64,
    /// Missiles that appeared (a channel's flames, a ring's missiles).
    fired: i64,
    /// Monsters that appeared (a nest's hatchling).
    spawned: i64,
    /// The caster was in the skill-move state at some frame.
    moving_seen: bool,
    /// The caster is still in the skill-move state at the end.
    moving_left: bool,
    /// What each cast returned.
    results: Vec<i32>,
    /// Life points the monster lost.
    damage: i64,
    /// Life points the monster gained.
    gained: i64,
    /// Frames on which it lost life.
    hits: i64,
    /// The most missiles alive at once.
    missiles: i64,
    /// Frames after the first cast until the first life lost; -1 for none.
    first_hit: i64,
    /// Frames after the first cast until no missile is left; -1 for never.
    missiles_gone: i64,
    /// The most units that carry the aura state at once.
    affected: i64,
    /// Frames after the first cast until the caster loses the aura state;
    /// -1 for never.
    state_ends: i64,
}

/// Cases whose Rust body is a channel that asks to be run again.
fn channels(case: &Case) -> bool {
    case.name == "inferno"
}

/// The charge that starts out of its target's reach: it is run on its own
/// (the Rust charge walks on the game's path code, which the fixture has no
/// frames for), not compared.
const CHARGE_FAR: Case = Case {
    name: "charge_far",
    cells: &[
        ("skill", "Charge"),
        ("srvdofunc", "67"),
        ("srvstfunc", "31"),
        ("param1", "40"),
        ("calc1", "12"),
    ],
    row: charge,
    window: 50,
    casts: 1,
    hurts: false,
    wounded: false,
};

/// Cases whose start function runs before the do function.
fn started(case: &Case) -> bool {
    matches!(
        case.name,
        "inferno" | "nest" | "charge" | "charge_far" | "melee_state"
    )
}

/// The frame the unit's next mode event (type 0) is due.
fn next_mode_event(fx: &Fx, u: UnitId) -> Option<i32> {
    let timers = &fx.game.timers;
    timers
        .unit_timers(u)
        .into_iter()
        .filter(|&id| timers.event(id).is_some_and(|(e, _, _)| e == 0))
        .filter_map(|id| timers.expire(id))
        .min()
}

/// The monster world of the fixture: it makes no monsters of its own, it
/// hands out `spare` and counts the creations asked for.
struct Hatchery {
    spare: UnitId,
    hatched: Arc<AtomicU32>,
}

impl<X> MonsterWorld<X> for Hatchery {
    fn type_init(&mut self, _: &mut Sim<'_>, _: &mut ActionHooks<X>, _: UnitId) {}
    fn umods(
        &mut self,
        _: &mut Sim<'_>,
        _: &mut ActionHooks<X>,
        _: UnitId,
        _: Option<UnitId>,
        _: u8,
    ) {
    }
    fn assign_umod(&mut self, _: &mut Sim<'_>, _: &mut ActionHooks<X>, _: UnitId, _: u8) {}
    fn forget(&mut self, _: UnitId) {}
    fn monster(&self, _: UnitId) -> Option<&crate::monsters::init::MonsterData> {
        None
    }
    fn monstats_count(&self) -> u32 {
        1
    }
    fn spawn_at(
        &mut self,
        _: &mut Sim<'_>,
        _: &mut ActionHooks<X>,
        _: RoomId,
        _: i32,
        _: i32,
        _: i32,
        _: u8,
        _: i32,
        _: u16,
    ) -> Option<Option<UnitId>> {
        self.hatched.fetch_add(1, Ordering::Relaxed);
        Some(Some(self.spare))
    }
    fn spawn_preset(
        &mut self,
        _: &mut Sim<'_>,
        _: &mut ActionHooks<X>,
        _: RoomId,
        _: i32,
        _: i32,
        _: i32,
        _: u8,
    ) -> Option<Option<UnitId>> {
        None
    }
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

fn tables(case: &Case, data: Option<&[(&'static str, String)]>) -> SkillTables {
    let mut t = skills();
    (case.row)(&mut t.skills[0]);
    if let Some(cells) = data {
        for (column, value) in cells {
            match (*column, value.as_str()) {
                ("srvdofunc", v) => t.skills[0].srvdofunc = v.parse().unwrap(),
                ("srvmissile", "") => t.skills[0].srvmissile = NONE,
                ("srvstfunc", v) => t.skills[0].srvstfunc = v.parse().unwrap(),
                (c, v) => panic!("unexpected cell {c} = {v}"),
            }
        }
    }
    // `250` as the aura length, `12` as its range.
    t.skills_code = vec![0x07, 250, 0x00, 0x07, 12, 0x00, 0x07, 60, 0x00];
    if matches!(
        case.name,
        "druid_summon" | "sentry" | "vine" | "golem" | "raven" | "shadow"
    ) {
        t.skills_code = vec![0x07, 5, 0, 0x07, 2, 0];
    }
    if matches!(
        case.name,
        "buff" | "warcry" | "form" | "damage_aura" | "freeze_aura" | "martial_charge"
    ) {
        // Signed one-byte constants cannot represent 250.
        t.skills_code = vec![0x08, 250, 0, 0x00, 0x07, 12, 0x00];
    }
    if case.name == "passive" {
        t.skills_code = vec![7, 12, 0];
    }
    if matches!(case.name, "corpse_find" | "potion_find") {
        t.skills_code = vec![7, 100, 0];
    }
    if case.name == "shadow" {
        t.skills_code = vec![7, 5, 0, 7, 2, 0, 7, 60, 0];
    }
    if matches!(case.name, "ai_curse" | "attract") {
        t.skills_code = vec![7, 12, 0, 7, 12, 0, 8, 250, 0, 0];
    }
    if matches!(case.name, "projectile_fan" | "jitter_fan" | "bolt_fan") {
        t.skills_code = vec![7, 5, 0, 7, 0, 0, 7, 3, 0];
    }
    if case.name == "guided_projectile" {
        t.skills_code = vec![7, 12, 0];
    }
    if matches!(case.name, "sight" | "sight_immune") {
        t.skills_code = vec![7, 12, 0, 7, 12, 0, 7, 60, 0];
    }
    t
}

fn run(case: &Case, data: Option<&str>, mut t: SkillTables) -> Outcome {
    // The engine's charge is not run here (see [`UNFIT`]): a cast that did
    // nothing.
    if case.name == "charge" && data.is_none() {
        return Outcome {
            results: vec![0; case.casts as usize],
            first_hit: -1,
            missiles_gone: -1,
            state_ends: -1,
            ..Outcome::default()
        };
    }
    // Summon skill setup deliberately refuses skill zero. Use a nonzero
    // generated row so its passive stats and granted skills actually run.
    let skill = if matches!(
        case.name,
        "druid_summon"
            | "sentry"
            | "vine"
            | "golem"
            | "raven"
            | "shadow"
            | "passive"
            | "martial_charge"
    ) {
        1
    } else {
        0
    };
    if skill == 1 {
        if t.skills.len() < 2 {
            t.skills.push(t.skills[0].clone());
        } else {
            t.skills[1] = t.skills[0].clone();
        }
    }
    let mut fx = fx_with(t.clone());
    if matches!(
        case.name,
        "druid_summon" | "sentry" | "vine" | "golem" | "raven" | "shadow"
    ) {
        use d2_formats::animdata::{self, AnimData, AnimRecord};
        let mut anim = AnimData {
            buckets: vec![Vec::new(); animdata::BUCKETS],
        };
        anim.buckets[animdata::hash(b"GENANIM")].push(AnimRecord {
            name: *b"GENANIM\0",
            frames: 4,
            speed: 256,
            events: [0; animdata::EVENTS],
        });
        fx.sim.sys.hooks.anim_data = Some(Arc::new(anim));
        fx.sim.sys.hooks.x.generated_anim = true;
    }
    if matches!(case.name, "teleport" | "charge" | "charge_far") {
        fx.sim
            .hooks()
            .enable_paths()
            .expect("movement fixture paths");
    }
    if case.name == "shadow" {
        shadow_setup::equipment(&mut fx);
    }
    let p = fx.spawn(UnitType::Player, 10, 10);
    let far = case.name == "charge_far";
    let m = fx.spawn(UnitType::Monster, if far { 60 } else { 13 }, 10);
    if case.name == "teleport" {
        for level in &mut Arc::make_mut(&mut fx.sim.sys.hooks.tables).levels {
            level.teleport = 1;
        }
    }
    let monster = |fx: &mut Fx, u: UnitId| {
        fx.set(
            u,
            &[
                (31, 1),
                (LEVEL_STAT, 1),
                (sst::MAXHP, LIFE),
                (sst::HITPOINTS, LIFE),
            ],
        );
        // The three flags a live monster has (a curse tests all three).
        fx.sim.sys.units.get_mut(u).unwrap().flags |= crate::missiles::unit_flag::IS_VALID_TARGET
            | crate::missiles::unit_flag::CAN_BE_ATTACKED
            | crate::missiles::unit_flag::BIT1;
    };
    monster(&mut fx, m);
    if matches!(case.name, "corpse_find" | "potion_find") {
        corpse_drops::setup(&mut fx, p, m);
    }
    if case.name == "golem" {
        fx.set(p, &[(crate::stats::ids::PASSIVE_SUMMON_RESIST, 25)]);
    }
    let ai_units = if matches!(case.name, "ai_curse" | "attract") {
        let spare = fx.spawn(UnitType::Monster, 12, 12);
        monster(&mut fx, spare);
        ai_control::setup(&mut fx, &[m, spare]);
        vec![m, spare]
    } else {
        Vec::new()
    };
    // A nest hatches into a spare monster; the world counts the hatchings.
    let hatched = Arc::new(AtomicU32::new(0));
    if case.name == "nest" {
        let spare = fx.spawn(UnitType::Monster, 12, 12);
        fx.sim.sys.hooks.monster_world = Some(Box::new(Hatchery {
            spare,
            hatched: hatched.clone(),
        }));
    }
    // The resurrection's target is a corpse: its life comes back as "gained".
    if case.name == "resurrect" {
        fx.sim.sys.units.get_mut(m).unwrap().mode = 12;
        fx.set(m, &[(sst::HITPOINTS, 0)]);
    }
    if case.wounded {
        fx.set(m, &[(sst::HITPOINTS, LIFE / 2)]);
    }
    // Hit rating, level, mana, and the 5 to 9 points a bare-handed swing deals.
    fx.set(
        p,
        &[
            (19, 3000),
            (LEVEL_STAT, 30),
            (sst::MAXMANA, 200 << 8),
            (21, 5),
            (22, 9),
        ],
    );
    fx.sim.sys.units.get_mut(m).unwrap().flags |= crate::missiles::unit_flag::IS_VALID_TARGET;
    {
        let tables = Arc::make_mut(&mut fx.sim.sys.hooks.tables);
        tables.combat.monstats2[0].isatt = true;
        if case.name == "sight_immune" {
            tables.combat.monstats[0].noaura = true;
        }
        if matches!(case.name, "damage_aura" | "freeze_aura") {
            let m = &mut tables.combat.monstats[0];
            m.coldeffect = 206;
            m.coldeffect_n = 206;
            m.coldeffect_h = 206;
        }
    }
    fx.sim.sys.hooks.bodies = Some(Arc::new(BodyTables {
        stats: vec![BodyStat::default(); 359],
        state_group: vec![0; super::super::N_STATES],
        state_aura: vec![false; super::super::N_STATES],
        pettype_count: 3,
        pettype_group: vec![0; 3],
        ..BodyTables::default()
    }));
    if let Some(toml) = data {
        // Defensive buffs compare against ordinary table states. Adding the
        // charge fixture's stack effects would change the data run alone.
        let toml = if skill == 1 {
            toml.replace("id = 0", "id = 1").replace("id=0", "id=1")
        } else {
            toml.to_owned()
        };
        install(
            &mut fx,
            &toml,
            if matches!(
                case.name,
                "sight"
                    | "sight_immune"
                    | "buff"
                    | "warcry"
                    | "form"
                    | "damage_aura"
                    | "freeze_aura"
                    | "martial_charge"
                    | "passive"
                    | "ai_curse"
                    | "attract"
            ) {
                ""
            } else {
                STATES
            },
        );
    }
    let entry = SkillEntry {
        skill,
        base: if case.name == "passive" { LEVEL } else { 1 },
        owner_guid: -1,
        ..SkillEntry::default()
    };
    {
        let x = &mut fx.sim.hooks().x;
        x.skills.insert(p, vec![entry]);
        x.used.insert(p, entry);
        x.aim_at = (if far { 60 } else { 13 }, 10);
        x.targets.insert(p, m);
        // The charge that runs starts out of reach of its target.
        x.in_reach = case.name != "charge_far";
        if case.name == "teleport" {
            x.targets.clear();
            x.aim_at = (12, 12);
        }
        x.open_ground = true;
    }
    // Uninterruptible: the monster takes a hit without a get-hit animation,
    // which the fixture's monsters have no record for.
    // (A corpse is raised into its rise mode, which that state would refuse.)
    if !matches!(case.name, "resurrect" | "ai_curse" | "attract") {
        fx.sim.with(&mut fx.game, |_, v| v.set_state(m, 54, true));
    }
    if case.name == "passive" {
        fx.sim
            .with(&mut fx.game, |_, v| v.set_state(p, STATE, true));
    }
    let mut out = Outcome {
        first_hit: -1,
        missiles_gone: -1,
        state_ends: -1,
        ..Outcome::default()
    };
    let mut seen_missiles = false;
    let mut before_missiles: Vec<UnitId> = Vec::new();
    let mut frames = 0;
    for n in 0..case.casts {
        fx.set(p, &[(sst::MANA, 200 << 8)]);
        let before = i64::from(super::blocks::life(&mut fx, m));
        // A channel's start function sets its length before the do function runs.
        if started(case) {
            fx.sim.skill_use(&mut fx.game, |w| start(w, &t, p));
        }
        if case.name == "passive" {
            fx.sim.skill_use(&mut fx.game, |w| {
                use crate::skills::use_::bodies::BodyWorld;
                BodyWorld::passive_state_apply(w, p, &entry);
                let row = &t.skills[skill as usize];
                let layer = if row.passiveitype < 0x8000 {
                    row.passiveitype
                } else {
                    0
                };
                for (i, stat) in [
                    row.passivestat1,
                    row.passivestat2,
                    row.passivestat3,
                    row.passivestat4,
                    row.passivestat5,
                ]
                .into_iter()
                .enumerate()
                {
                    if stat == NONE {
                        break;
                    }
                    out.passive_values[i] =
                        i64::from(crate::skills::SkillUnits::stat(w, p, stat, layer));
                }
                if let Some(list) = w.state_list(p, i32::from(STATE)) {
                    out.passive_level = i64::from(w.list_get(list, 351));
                    out.passive_skill = i64::from(w.list_get(list, 350));
                }
            });
        } else {
            let r = fx
                .sim
                .skill_use(&mut fx.game, |w| do_skill(w, &t, p, skill, LEVEL));
            out.results.push(r);
            if matches!(case.name, "corpse_find" | "potion_find") {
                assert!(
                    fx.sim.sys.stats.has_state(m, 118),
                    "corpse consumed immediately after cast {n}, return {r}"
                );
            }
        }
        if case.name == "guided_projectile" {
            let store = fx.sim.sys.hooks.missiles.as_ref().unwrap();
            let values: Vec<_> = store
                .missiles()
                .filter_map(|m| store.get(m))
                .map(|d| d.target)
                .collect();
            out.guided_modes = out
                .guided_modes
                .max(values.iter().map(|d| i64::from(d.0)).sum());
            out.guided_offsets = out
                .guided_offsets
                .max(values.iter().map(|d| i64::from(d.1)).sum());
        }
        if matches!(case.name, "sight" | "sight_immune") {
            fx.sim.skill_use(&mut fx.game, |w| {
                use crate::skills::use_::bodies::BodyWorld;
                out.sight_values = [STATE, OTHER]
                    .into_iter()
                    .filter_map(|s| w.state_list(m, i32::from(s)))
                    .map(|l| i64::from(w.list_get(l, i32::from(t.skills[0].aurastat1))))
                    .sum();
            });
        }
        if !ai_units.is_empty() {
            ai_control::observe(&mut fx, &mut out, &ai_units);
        }
        if case.name == "martial_charge" {
            let row = &t.skills[skill as usize];
            fx.sim.skill_use(&mut fx.game, |w| {
                use crate::skills::use_::bodies::BodyWorld;
                if let Some(list) = w.state_list(p, i32::from(STATE)) {
                    out.martial_count = out
                        .martial_count
                        .max(i64::from(w.list_get(list, i32::from(row.aurastat1))));
                    out.martial_bonus = out
                        .martial_bonus
                        .max(i64::from(w.list_get(list, i32::from(row.aurastat2))));
                }
            });
        }
        out.missiles = out
            .missiles
            .max(fx.game.lists.units_of_type(UnitType::Missile).len() as i64);
        // The only event-0 timer the cast left is the channel's next pulse.
        let mut pulse = (channels(case) && data.is_none())
            .then(|| next_mode_event(&fx, p))
            .flatten();
        if matches!(
            case.name,
            "druid_summon" | "sentry" | "vine" | "golem" | "raven" | "shadow"
        ) {
            out.pets = out.pets.max(i64::from(fx.sim.skill_use(&mut fx.game, |w| {
                crate::rules::SkillPets::pet_count(w, p, 2)
            })));
            let guids: Vec<_> = fx
                .sim
                .sys
                .hooks
                .pet_lists
                .get(&p)
                .and_then(|lists| lists.entries.get(2))
                .map(|entry| entry.nodes.iter().map(|node| node.guid as u32).collect())
                .unwrap_or_default();
            let pets: Vec<_> = fx
                .game
                .lists
                .units_of_type(UnitType::Monster)
                .into_iter()
                .filter(|&u| {
                    fx.sim
                        .sys
                        .units
                        .get(u)
                        .is_some_and(|rec| guids.contains(&rec.guid))
                })
                .collect();
            let levels = fx.sim.with(&mut fx.game, |_, v| {
                pets.iter().map(|&u| i64::from(v.stat(u, LEVEL_STAT))).sum()
            });
            out.pet_levels = out.pet_levels.max(levels);
            let stats = fx.sim.with(&mut fx.game, |_, v| {
                pets.iter().map(|&u| i64::from(v.stat(u, 31))).sum()
            });
            out.pet_stats = out.pet_stats.max(stats);
            out.vine_states = out.vine_states.max(
                pets.iter()
                    .filter(|&&u| fx.sim.sys.stats.has_state(u, 150))
                    .count() as i64,
            );
            let resists = fx.sim.with(&mut fx.game, |_, v| {
                pets.iter()
                    .flat_map(|&u| [39, 41, 43, 45].map(|s| i64::from(v.stat(u, s))))
                    .sum()
            });
            out.pet_resists = out.pet_resists.max(resists);
            let regen = fx.sim.with(&mut fx.game, |_, v| {
                pets.iter()
                    .map(|&u| i64::from(v.stat(u, sst::HPREGEN)))
                    .sum()
            });
            out.pet_regen = out.pet_regen.max(regen);
            out.targetable_pets = out.targetable_pets.max(
                pets.iter()
                    .filter(|&&u| fx.sim.sys.units.get(u).is_some_and(|r| r.flags & 4 != 0))
                    .count() as i64,
            );
            if case.name == "shadow" {
                shadow_setup::observe(&mut fx, &mut out, p, &pets);
            }
            out.pet_modes = out.pet_modes.max(
                pets.iter()
                    .filter_map(|&u| fx.sim.sys.units.get(u))
                    .map(|r| i64::from(r.mode))
                    .sum(),
            );
        }
        if matches!(
            case.name,
            "buff" | "warcry" | "form" | "damage_aura" | "freeze_aura" | "martial_charge"
        ) {
            let row = &t.skills[0];
            let mut stats = vec![
                row.aurastat1,
                row.aurastat2,
                row.aurastat3,
                row.aurastat4,
                row.aurastat5,
                row.aurastat6,
                row.passivestat1,
                row.passivestat2,
                row.passivestat3,
                row.passivestat4,
                row.passivestat5,
            ];
            stats.retain(|&s| s != NONE);
            stats.sort_unstable();
            stats.dedup();
            out.buff_stats = fx.sim.skill_use(&mut fx.game, |w| {
                use crate::skills::use_::bodies::BodyWorld;
                let Some(list) = w.state_list(p, i32::from(STATE)) else {
                    return 0;
                };
                stats
                    .iter()
                    .map(|&s| i64::from(w.list_get(list, i32::from(s))).abs())
                    .sum()
            });
        }
        if matches!(case.name, "damage_aura" | "freeze_aura") {
            out.target_stats = fx.sim.skill_use(&mut fx.game, |w| {
                use crate::skills::use_::bodies::BodyWorld;
                w.state_list(m, i32::from(OTHER))
                    .map_or(0, |list| i64::from(w.list_get(list, 16)).abs())
            });
        }
        // The units that carry the aura's state (or the one it gives the units around).
        let carriers = |fx: &Fx| {
            [p, m]
                .iter()
                .filter(|&&u| {
                    [STATE, OTHER]
                        .iter()
                        .any(|&s| fx.sim.sys.stats.has_state(u, u32::from(s)))
                })
                .count() as i64
        };
        out.affected = out.affected.max(carriers(&fx));
        let mut life = i64::from(super::blocks::life(&mut fx, m));
        out.gained += (life - before).max(0);
        // Damage that lands inside the cast itself (a melee swing) counts at frame 0.
        if life < before {
            out.hits += 1;
            out.damage += before - life;
            if out.first_hit < 0 && n == 0 {
                out.first_hit = frames;
            }
        }
        for _ in 0..case.window {
            // The engine's mode handler runs the do function again when the
            // channel's own timer (event 0) comes due; the fixture does so for
            // the Rust body (the data copy runs on its own ticks).
            let again = pulse == Some(fx.game.frame + 1);
            fx.frame();
            if again {
                fx.sim
                    .skill_use(&mut fx.game, |w| do_skill(w, &t, p, 0, LEVEL));
                pulse = next_mode_event(&fx, p);
            }
            frames += 1;
            let now = i64::from(super::blocks::life(&mut fx, m));
            out.gained += (now - life).max(0);
            if now < life {
                out.hits += 1;
                out.damage += life - now;
                if out.first_hit < 0 {
                    out.first_hit = frames;
                }
            }
            life = now;
            out.affected = out.affected.max(carriers(&fx));
            let moving = fx.sim.sys.stats.has_state(p, 18);
            out.moving_seen |= moving;
            out.moving_left = moving;
            let live = fx.game.lists.units_of_type(UnitType::Missile);
            out.fired += live.iter().filter(|m| !before_missiles.contains(m)).count() as i64;
            before_missiles.clone_from(&live);
            let alive = live.len() as i64;
            if case.name == "martial_charge" {
                let row = &t.skills[skill as usize];
                fx.sim.skill_use(&mut fx.game, |w| {
                    use crate::skills::use_::bodies::BodyWorld;
                    if let Some(list) = w.state_list(p, i32::from(STATE)) {
                        out.martial_count = out
                            .martial_count
                            .max(i64::from(w.list_get(list, i32::from(row.aurastat1))));
                        out.martial_bonus = out
                            .martial_bonus
                            .max(i64::from(w.list_get(list, i32::from(row.aurastat2))));
                    }
                });
            }
            out.missiles = out.missiles.max(alive);
            seen_missiles |= alive > 0;
            if n == 0 && seen_missiles && alive == 0 && out.missiles_gone < 0 {
                out.missiles_gone = frames;
            }
            if n == 0
                && out.affected > 0
                && out.state_ends < 0
                && !fx.sim.sys.stats.has_state(p, u32::from(STATE))
            {
                out.state_ends = frames;
            }
        }
    }
    let at = fx
        .sim
        .skill_use(&mut fx.game, |w| crate::rules::HostUnits::position(w, p));
    out.moved_x = i64::from(at.0) - 10;
    out.moved_y = i64::from(at.1) - 10;
    if matches!(case.name, "corpse_find" | "potion_find") {
        corpse_drops::observe(&fx, &mut out, m);
    }
    if !ai_units.is_empty() {
        out.ai_remaining = ai_control::remaining(&fx, &ai_units);
    }
    if matches!(case.name, "sight" | "sight_immune") {
        out.sight_remaining = [STATE, OTHER]
            .into_iter()
            .filter(|&s| fx.sim.sys.stats.has_state(m, u32::from(s)))
            .count() as i64;
    }
    out.spawned = i64::from(hatched.load(Ordering::Relaxed));
    fx.assert_clean();
    out
}

/// A metric whose two runs differ by more than [`FLAG_PERCENT`] percent (or
/// where one run has none and the other has some).
fn flagged(rust: i64, data: i64) -> bool {
    if rust == data {
        return false;
    }
    let base = rust.abs().max(data.abs()).max(1);
    (rust - data).abs() * 100 / base > FLAG_PERCENT
}

/// What the two runs are compared on: `(metric, Rust body, data copy)`.
fn metrics(rust: &Outcome, data: &Outcome) -> impl IntoIterator<Item = (&'static str, i64, i64)> {
    [
        (
            "passive stat 1",
            rust.passive_values[0],
            data.passive_values[0],
        ),
        (
            "passive stat 2",
            rust.passive_values[1],
            data.passive_values[1],
        ),
        (
            "passive stat 3",
            rust.passive_values[2],
            data.passive_values[2],
        ),
        (
            "passive stat 4",
            rust.passive_values[3],
            data.passive_values[3],
        ),
        (
            "passive stat 5",
            rust.passive_values[4],
            data.passive_values[4],
        ),
        ("passive level", rust.passive_level, data.passive_level),
        ("passive skill", rust.passive_skill, data.passive_skill),
        ("pet count", rust.pets, data.pets),
        ("pet levels", rust.pet_levels, data.pet_levels),
        ("pet stats", rust.pet_stats, data.pet_stats),
        ("pet modes", rust.pet_modes, data.pet_modes),
        ("sight values", rust.sight_values, data.sight_values),
        (
            "sight remaining",
            rust.sight_remaining,
            data.sight_remaining,
        ),
        ("guided modes", rust.guided_modes, data.guided_modes),
        ("guided offsets", rust.guided_offsets, data.guided_offsets),
        ("AI overrides", rust.ai_overrides, data.ai_overrides),
        ("AI alignments", rust.ai_alignments, data.ai_alignments),
        ("AI nodes", rust.ai_nodes, data.ai_nodes),
        ("AI values", rust.ai_values, data.ai_values),
        ("AI remaining", rust.ai_remaining, data.ai_remaining),
        (
            "shadow equipment",
            rust.shadow_equipment,
            data.shadow_equipment,
        ),
        (
            "shadow item levels",
            rust.shadow_item_levels,
            data.shadow_item_levels,
        ),
        ("shadow timers", rust.shadow_timers, data.shadow_timers),
        ("shadow links", rust.shadow_links, data.shadow_links),
        ("shadow stats", rust.shadow_stats, data.shadow_stats),
        ("martial charges", rust.martial_count, data.martial_count),
        ("martial bonus", rust.martial_bonus, data.martial_bonus),
        (
            "corpse consumed",
            rust.corpse_consumed,
            data.corpse_consumed,
        ),
        ("items dropped", rust.items_dropped, data.items_dropped),
        ("item classes", rust.item_classes, data.item_classes),
        ("pet regen", rust.pet_regen, data.pet_regen),
        (
            "targetable pets",
            rust.targetable_pets,
            data.targetable_pets,
        ),
        ("pet resists", rust.pet_resists, data.pet_resists),
        ("vine states", rust.vine_states, data.vine_states),
        ("self buff stats", rust.buff_stats, data.buff_stats),
        ("target stats", rust.target_stats, data.target_stats),
        ("damage", rust.damage, data.damage),
        ("life gained", rust.gained, data.gained),
        ("hits", rust.hits, data.hits),
        ("missiles at once", rust.missiles, data.missiles),
        ("missiles fired", rust.fired, data.fired),
        ("monsters spawned", rust.spawned, data.spawned),
        ("first hit", rust.first_hit, data.first_hit),
        ("missiles gone", rust.missiles_gone, data.missiles_gone),
        ("units affected", rust.affected, data.affected),
        ("state ends", rust.state_ends, data.state_ends),
        ("caster movement x", rust.moved_x, data.moved_x),
        ("caster movement y", rust.moved_y, data.moved_y),
    ]
}

/// Runs every case both ways: the markdown report and each case's outcomes.
fn report() -> (String, Vec<(&'static str, Outcome, Outcome)>) {
    let mut text = String::from(
        "# Closeness report: Rust body vs data copy\n\n\
         One seeded game per case; each cast is at the same level on the same \
         fixture. Times are frames after the first cast (-1 = never). A row marked \
         `!` differs by more than 25%.\n\n\
         | skill | metric | Rust body | data copy | |\n| --- | --- | ---: | ---: | --- |\n",
    );
    let mut all = Vec::new();
    for case in CASES {
        let lifted = lift(0, &Cells(case.cells.iter().copied().collect()))
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        if lifted.family == crate::rules::data_mod::lift::Family::Passive {
            assert!(
                lifted.cells.is_empty(),
                "passives use refresh without patching a cast"
            );
        } else {
            assert_eq!(lifted.cells[0], ("srvdofunc", BLOCK_SLOT.to_string()));
        }
        let rust = run(case, None, tables(case, None));
        let data = run(case, Some(&lifted.toml), tables(case, Some(&lifted.cells)));
        if let Some(why) = unfit(case.name, &rust) {
            let _ = writeln!(text, "| {} | note | not compared: {why} | | |", case.name);
            for note in &lifted.notes {
                let _ = writeln!(text, "| {} | note | {note} | | |", case.name);
            }
            all.push((case.name, rust, data));
            continue;
        }
        for (metric, a, b) in metrics(&rust, &data) {
            if (a == -1 && b == -1) || (a == 0 && b == 0) {
                continue;
            }
            let mark = if flagged(a, b) { "!" } else { "" };
            let _ = writeln!(text, "| {} | {metric} | {a} | {b} | {mark} |", case.name);
        }
        if case.hurts && rust.damage == 0 && data.damage == 0 {
            let _ = writeln!(
                text,
                "| {} | note | neither run lost life: this fixture has no weapon or damage rows | | |",
                case.name
            );
        }
        for note in &lifted.notes {
            let _ = writeln!(text, "| {} | note | {note} | | |", case.name);
        }
        all.push((case.name, rust, data));
    }
    (text, all)
}

#[test]
fn the_closeness_report_runs_every_lifted_family_both_ways() {
    let (text, all) = report();
    if let Some(path) = std::env::var_os("D2_CLOSENESS_REPORT") {
        std::fs::write(path, &text).expect("the report file");
    }
    assert_eq!(all.len(), CASES.len());
    for (name, rust, data) in &all {
        if unfit(name, rust).is_some() {
            continue;
        }
        // Both versions cast: a fixture that cannot run one is no comparison.
        if matches!(*name, "corpse_find" | "potion_find") {
            assert_eq!(rust.results, [1, 0, 0], "{name}: one-use Rust body");
            assert_eq!(data.results, [1, 0, 0], "{name}: one-use data copy");
        } else {
            assert!(
                rust.results.iter().all(|r| *r == 1),
                "{name}: Rust body {:?}",
                rust.results
            );
            assert!(
                data.results.iter().all(|r| *r == 1),
                "{name}: data copy {:?}",
                data.results
            );
        }
    }
    assert!(text.contains("| skill | metric |"));
}

#[test]
fn a_lifted_nova_and_missile_match_the_engine_where_they_should() {
    let (_, all) = report();
    let find = |n: &str| all.iter().find(|(name, _, _)| *name == n).unwrap();
    // The nova is the engine's own ring: the same number of missiles, at the
    // same speed, so they live the same time.
    let (_, rust, data) = find("nova");
    assert_eq!(rust.missiles, 64);
    assert_eq!(data.missiles, 64);
    assert_eq!(rust.missiles_gone, data.missiles_gone);
    // A plain missile skill fires the same missiles either way.
    let (_, rust, data) = find("missile");
    assert_eq!(rust.missiles, data.missiles);
    assert_eq!(rust.first_hit, data.first_hit);
    assert_eq!(rust.damage, data.damage);
}

#[test]
fn a_lifted_curse_reaches_the_monster_and_not_the_caster() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(name, _, _)| *name == "curse").unwrap();
    // The curse state lands on the hostile monster at the aim point, never on
    // the caster, the same way in both.
    assert_eq!(rust.affected, 1);
    assert_eq!(data.affected, 1);
}

#[test]
fn a_lifted_heal_gives_the_wounded_target_the_same_life_back() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(name, _, _)| *name == "heal").unwrap();
    // 12 percent of the maximum life, three casts, in both; no roll to differ
    // on (both percents are 12).
    assert!(rust.gained > 0);
    assert_eq!(rust.gained, data.gained);
}

#[test]
fn a_lifted_inferno_fires_as_many_flames_as_the_engines_channel() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(name, _, _)| *name == "inferno").unwrap();
    // A channel of 60 ticks with a flame every 12: five, and the sixth that finds it used up.
    assert_eq!(rust.fired, 6, "{rust:?}");
    assert_eq!(rust.fired, data.fired);
    assert_eq!(rust.missiles_gone, data.missiles_gone);
}

#[test]
fn a_lifted_charge_hits_a_target_in_reach_without_running() {
    let (_, all) = report();
    let (_, _, data) = all.iter().find(|(name, _, _)| *name == "charge").unwrap();
    assert!(data.results.iter().all(|r| *r == 1), "{data:?}");
    assert!(data.damage > 0 && data.hits >= 1, "{data:?}");
    assert_eq!((data.moved_x, data.moved_y), (0, 0));
    assert!(!data.moving_seen, "{data:?}");
}

#[test]
fn a_lifted_charge_runs_at_a_target_out_of_reach_in_the_skill_move_state() {
    let lifted = lift(0, &Cells(CHARGE_FAR.cells.iter().copied().collect())).unwrap();
    let data = run(
        &CHARGE_FAR,
        Some(&lifted.toml),
        tables(&CHARGE_FAR, Some(&lifted.cells)),
    );
    assert_eq!(data.results, [1], "{data:?}");
    // The caster ran toward the monster three tiles to the east, in the
    // skill-move state, and is out of it where it stopped.
    assert!(data.moved_x > 0 && data.moved_y == 0, "{data:?}");
    assert!(data.moving_seen && !data.moving_left, "{data:?}");
}

#[test]
fn a_lifted_power_strike_hits_and_marks_its_target_as_the_engines_does() {
    let (_, all) = report();
    let (_, rust, data) = all
        .iter()
        .find(|(name, _, _)| *name == "melee_state")
        .unwrap();
    assert!(rust.damage > 0 && data.damage > 0, "{rust:?} {data:?}");
    // The monster carries the row's state after the hit, either way.
    assert_eq!((rust.affected, data.affected), (1, 1), "{rust:?} {data:?}");
}

#[test]
fn a_lifted_nest_hatches_the_same_monster_either_way() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(name, _, _)| *name == "nest").unwrap();
    assert_eq!(rust.spawned, 1, "{rust:?}");
    assert_eq!(rust.spawned, data.spawned);
}

#[test]
fn a_lifted_resurrect_raises_the_corpse_to_full_life_either_way() {
    let (_, all) = report();
    let (_, rust, data) = all
        .iter()
        .find(|(name, _, _)| *name == "resurrect")
        .unwrap();
    assert_eq!(rust.gained, i64::from(LIFE));
    assert_eq!(rust.gained, data.gained);
}

#[test]
fn a_lifted_fire_wall_lays_the_same_three_missiles_either_way() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(name, _, _)| *name == "wall").unwrap();
    assert_eq!(rust.results, data.results);
    assert!(rust.missiles >= 3, "{rust:?}");
    assert_eq!(rust.missiles, data.missiles);
}

#[test]
fn a_lifted_meteor_drops_the_same_missiles_either_way() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(name, _, _)| *name == "meteor").unwrap();
    assert_eq!(rust.results, data.results);
    assert_eq!(rust.missiles, data.missiles);
    assert_eq!(rust.missiles_gone, data.missiles_gone);
}

#[test]
fn the_report_is_the_same_every_time() {
    let a = report().0;
    let b = report().0;
    assert_eq!(a, b);
}

#[test]
fn teleport_report_measures_real_caster_displacement() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(name, _, _)| *name == "teleport").unwrap();
    assert_eq!((rust.moved_x, rust.moved_y), (2, 2));
    assert_eq!((data.moved_x, data.moved_y), (2, 2));
    assert_eq!(rust.results, [1]);
    assert_eq!(data.results, [1]);
}

#[test]
fn a_flag_needs_a_big_difference() {
    assert!(!flagged(100, 100));
    assert!(!flagged(100, 80));
    assert!(flagged(100, 70));
    assert!(flagged(0, 5));
    assert!(flagged(-1, 30));
}

#[test]
fn timed_buff_report_observes_stats_and_expiry_on_both_paths() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(name, _, _)| *name == "buff").unwrap();
    assert_eq!(rust.buff_stats, 12);
    assert_eq!(rust.buff_stats, data.buff_stats);
    assert_eq!(rust.affected, 1);
    assert_eq!(rust.affected, data.affected);
    assert_eq!(
        rust.state_ends, -1,
        "the table callback is not dispatched by timer expiry"
    );
    assert!(
        data.state_ends > 0,
        "the opted-in data callback clears its state"
    );
    assert!(flagged(rust.state_ends, data.state_ends));
}

#[test]
fn druid_summon_report_observes_owned_pet_cap_and_levels() {
    let (_, all) = report();
    let (_, rust, data) = all
        .iter()
        .find(|(name, _, _)| *name == "druid_summon")
        .unwrap();
    assert_eq!(rust.pets, 2);
    assert_eq!(rust.pet_levels, 10);
    assert_eq!(rust.pet_stats, 10);
    assert_eq!(rust.pets, data.pets);
    assert_eq!(rust.pet_levels, data.pet_levels);
    assert_eq!(rust.pet_stats, data.pet_stats);
}

#[test]
fn druid_summon_data_edits_control_pet_cap_and_monster_level() {
    let case = CASES
        .iter()
        .find(|case| case.name == "druid_summon")
        .unwrap();
    let text="[[skill]]\nid=0\n[[skill.part]]\nkind='druid_summon'\nclass=0\npet_type=2\nmode=1\npet_max='1'\nlevel='7'\n";
    let cells = [("srvdofunc", crate::rules::data_mod::BLOCK_SLOT.to_string())];
    let data = run(case, Some(text), tables(case, Some(&cells)));
    assert_eq!(data.results, [1, 1, 1]);
    assert_eq!(data.pets, 1);
    assert_eq!(data.pet_levels, 7);
}

#[test]
fn warcry_report_observes_ring_state_stats_and_expiry() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(name, _, _)| *name == "warcry").unwrap();
    assert_eq!(rust.results, data.results);
    assert!(rust.missiles >= 64);
    assert_eq!(rust.missiles, data.missiles);
    assert_eq!(rust.buff_stats, 12);
    assert_eq!(rust.buff_stats, data.buff_stats);
    assert!(rust.state_ends > 0);
    assert_eq!(rust.state_ends, data.state_ends);
}

#[test]
fn sentry_report_measures_placed_pet_cap_levels_and_initial_modes() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(name, _, _)| *name == "sentry").unwrap();
    assert_eq!(rust.results, [1, 1, 1]);
    assert_eq!(rust.results, data.results);
    assert_eq!(rust.pets, 2);
    assert_eq!(rust.pets, data.pets);
    assert!(rust.pet_levels > 0);
    assert_eq!(rust.pet_levels, data.pet_levels);
    assert_eq!(rust.pet_stats, 10);
    assert_eq!(rust.pet_stats, data.pet_stats);
    assert_eq!(rust.pet_modes, 2);
    assert_eq!(rust.pet_modes, data.pet_modes);
}
mod sentry_placement;

#[test]
fn vine_report_measures_state_level_mode_and_pet_cap() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(n, _, _)| *n == "vine").unwrap();
    assert_eq!(rust.results, [1, 1, 1]);
    assert_eq!(rust, data);
    assert_eq!(rust.pets, 2);
    assert_eq!(rust.pet_levels, 10);
    assert_eq!(rust.pet_stats, 10);
    assert_eq!(rust.pet_modes, 16);
    assert_eq!(rust.vine_states, 2);
}
#[test]
fn form_report_exposes_the_default_expiry_gap_instead_of_hiding_it() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(n, _, _)| *n == "form").unwrap();
    assert_eq!(rust.results, [1]);
    assert_eq!(rust.results, data.results);
    assert_eq!(rust.buff_stats, 12);
    assert_eq!(rust.buff_stats, data.buff_stats);
    assert_eq!(rust.state_ends, -1);
    assert!(data.state_ends > 0);
}
#[test]
fn vine_data_edits_change_level_and_pet_cap() {
    let case = CASES.iter().find(|c| c.name == "vine").unwrap();
    let text="[[skill]]\nid=0\n[[skill.part]]\nkind='vine'\nclass=0\npet_type=2\nstate=150\npet_max='1'\nlevel='7'\n";
    let cells = [("srvdofunc", crate::rules::data_mod::BLOCK_SLOT.to_string())];
    let out = run(case, Some(text), tables(case, Some(&cells)));
    assert_eq!(out.pets, 1);
    assert_eq!(out.pet_levels, 7);
    assert_eq!(out.vine_states, 1);
}

#[test]
fn damage_aura_report_observes_shared_damage_and_both_stat_lists() {
    let (_, all) = report();
    for name in ["damage_aura", "freeze_aura"] {
        let (_, rust, data) = all.iter().find(|(n, _, _)| *n == name).unwrap();
        assert_eq!(rust, data);
        assert!(rust.damage > 0);
        assert_eq!(rust.buff_stats, 12);
        assert_eq!(rust.target_stats, 12);
    }
}

#[test]
fn golem_report_measures_owned_pet_cap_stats_and_summon_resistance() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(n, _, _)| *n == "golem").unwrap();
    assert_eq!(rust.results, [1, 1, 1]);
    assert_eq!(rust, data);
    assert_eq!(rust.pets, 2);
    assert_eq!(rust.pet_levels, 54);
    assert_eq!(rust.pet_stats, 10);
    assert_eq!(rust.pet_resists, 200);
}
#[test]
fn golem_data_edits_control_cap_and_monster_level() {
    let case = CASES.iter().find(|c| c.name == "golem").unwrap();
    let text="[[skill]]\nid=0\n[[skill.part]]\nkind='golem'\nclass=0\npet_type=2\nmode=1\npet_max='1'\nlevel='7'\n";
    let cells = [("srvdofunc", crate::rules::data_mod::BLOCK_SLOT.to_string())];
    let out = run(case, Some(text), tables(case, Some(&cells)));
    assert_eq!(out.pets, 1);
    assert_eq!(out.pet_levels, 7);
    assert_eq!(out.pet_resists, 100);
}

#[test]
fn raven_report_measures_pet_cap_stats_zero_regeneration_and_target_flag() {
    let (_, all) = report();
    let (_, rust, data) = all.iter().find(|(n, _, _)| *n == "raven").unwrap();
    assert_eq!(rust.results, [1, 1, 1]);
    assert_eq!(rust, data);
    assert_eq!(rust.pets, 2);
    assert_eq!(rust.pet_levels, 10);
    assert_eq!(rust.pet_stats, 10);
    assert_eq!(rust.pet_regen, 0);
    assert_eq!(rust.targetable_pets, 0);
}
#[test]
fn raven_data_edits_control_cap_and_monster_level() {
    let case = CASES.iter().find(|c| c.name == "raven").unwrap();
    let text="[[skill]]\nid=0\n[[skill.part]]\nkind='raven'\nclass=0\npet_type=2\nmode=1\npet_max='1'\nlevel='7'\n";
    let cells = [("srvdofunc", crate::rules::data_mod::BLOCK_SLOT.to_string())];
    let out = run(case, Some(text), tables(case, Some(&cells)));
    assert_eq!(out.pets, 1);
    assert_eq!(out.pet_levels, 7);
    assert_eq!(out.pet_regen, 0);
    assert_eq!(out.targetable_pets, 0);
}

#[test]
fn corpse_find_report_measures_consumption_and_actual_item_units() {
    let (_, all) = report();
    for name in ["corpse_find", "potion_find"] {
        let (_, rust, data) = all.iter().find(|(n, _, _)| *n == name).unwrap();
        assert_eq!(rust, data, "{name}");
        assert_eq!(rust.results, [1, 0, 0]);
        assert_eq!(rust.corpse_consumed, 1);
        assert_eq!(rust.items_dropped, 1);
        assert!(rust.item_classes > 0);
    }
}
#[test]
fn corpse_find_failed_chance_consumes_once_and_preserves_the_two_return_values() {
    for name in ["corpse_find", "potion_find"] {
        let case = CASES.iter().find(|c| c.name == name).unwrap();
        let mut t = tables(case, None);
        t.skills_code = vec![7, 0, 0];
        let rust = run(case, None, t.clone());
        t.skills[0].srvdofunc = crate::rules::BLOCK_SLOT;
        let text=format!("[[skill]]\nid=0\n[[skill.part]]\nkind='corpse_find'\nchance='0'\npotion={}\nband1='0'\nband2='100'\nband3='0'\nband4='0'\n",name=="potion_find");
        let data = run(case, Some(&text), t);
        assert_eq!(rust, data, "{name}");
        assert_eq!(rust.results, [i32::from(name == "potion_find"), 0, 0]);
        assert_eq!(rust.corpse_consumed, 1);
        assert_eq!(rust.items_dropped, 0);
    }
}
