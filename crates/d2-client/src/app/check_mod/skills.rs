//! Skill and missile formula evaluation from the engine's content sweeps.
//! Every formula is evaluated at levels 0 through 99 with no unit and a bare
//! unit, covering passive skills and special targets without UI setup.
use super::{Report, Severity};
use crate::app::single_player::LiveData;
use anyhow::Result;
use d2_data::tables::{Missiles, Skills};
use d2_sim::rng::Seed;
use d2_sim::skills::{eval_missile, eval_skill, SkillEntry, SkillTables, SkillUnits, NO_CALC};
use d2_sim::units::UnitType;

pub(super) fn check(live: &LiveData, report: &mut Report) -> Result<()> {
    let tables = SkillTables::from_bin(&live.tables.bins, 99)?;
    let mut unit = Bare::default();
    let mut evaluations = 0usize;
    for (row, skill) in tables.skills.iter().enumerate() {
        for field in skill_calcs(skill) {
            for level in 0..=99 {
                let value = eval_skill(&mut unit, &tables, None, field, row as i32, level);
                eval_skill(&mut unit, &tables, Some(0), field, row as i32, level);
                evaluations += 2;
                if field == NO_CALC && value != 0 {
                    report.finding(
                        Severity::Error,
                        "broken skill formulas",
                        &format!("skills row {row}"),
                        "absent formula evaluated to a nonzero value",
                    );
                }
            }
        }
    }
    for (row, missile) in tables.missiles.iter().enumerate() {
        for field in miss_calcs(missile) {
            for level in 0..=99 {
                let value = eval_missile(&mut unit, &tables, None, None, field, row as i32, level);
                eval_missile(
                    &mut unit,
                    &tables,
                    Some(1),
                    Some(0),
                    field,
                    row as i32,
                    level,
                );
                evaluations += 2;
                if field == NO_CALC && value != 0 {
                    report.finding(
                        Severity::Error,
                        "broken skill formulas",
                        &format!("missiles row {row}"),
                        "absent formula evaluated to a nonzero value",
                    );
                }
            }
        }
    }
    report.finding(
        Severity::Info,
        "skill formula summaries",
        "skills and missiles",
        format!("{evaluations} formula evaluations at levels 0 through 99"),
    );
    report
        .completed
        .push("skill and missile formula sweeps".into());
    Ok(())
}

#[derive(Default)]
struct Bare {
    seed: Seed,
}

impl SkillUnits for Bare {
    type Unit = u8;
    type Item = u8;
    fn unit_type(&self, u: u8) -> UnitType {
        if u == 1 {
            UnitType::Missile
        } else {
            UnitType::Player
        }
    }
    fn class_id(&self, _: u8) -> i32 {
        0
    }
    fn stat(&self, _: u8, _: u16, _: u16) -> i32 {
        0
    }
    fn item_stat(&self, _: u8, _: u16, _: u16) -> i32 {
        0
    }
    fn base_stat(&self, _: u8, _: u16, _: u16) -> i32 {
        0
    }
    fn formula_stat(&self, _: u8, _: u16, _: i32) -> i32 {
        0
    }
    fn stat_entries(&self, _: u8, _: u16, _: usize) -> Vec<(u16, i32)> {
        Vec::new()
    }
    fn has_state(&self, _: u8, _: u16) -> bool {
        false
    }
    fn state_stat(&self, _: u8, _: u16, _: u16) -> Option<i32> {
        None
    }
    fn seed(&mut self, _: u8) -> &mut Seed {
        &mut self.seed
    }
    fn skill_list(&self, _: u8) -> Vec<SkillEntry> {
        Vec::new()
    }
    fn used_skill(&self, _: u8) -> Option<SkillEntry> {
        None
    }
    fn current_weapon(&self, _: u8) -> Option<u8> {
        None
    }
    fn weapon(&self, _: u8) -> Option<u8> {
        None
    }
    fn item_at(&self, _: u8, _: u8) -> Option<u8> {
        None
    }
    fn item_is(&self, _: u8, _: i32) -> bool {
        false
    }
    fn itype_is(&self, _: i32, _: i32) -> bool {
        false
    }
    fn wield_type(&self, _: u8) -> i32 {
        0
    }
    fn item_damage(&self, _: u8, _: bool) -> i32 {
        0
    }
    fn str_dex_bonus(&self, _: u8) -> (i32, i32) {
        (0, 0)
    }
    fn item_flag_throw(&self, _: u8) -> bool {
        false
    }
    fn missile_level(&self, _: u8) -> i32 {
        0
    }
}

/// Every `skillscode` formula field of a skills record.
fn skill_calcs(s: &Skills) -> [u32; 36] {
    [
        s.prgcalc1,
        s.prgcalc2,
        s.prgcalc3,
        s.auralencalc,
        s.aurarangecalc,
        s.aurastatcalc1,
        s.aurastatcalc2,
        s.aurastatcalc3,
        s.aurastatcalc4,
        s.aurastatcalc5,
        s.aurastatcalc6,
        s.passivecalc1,
        s.passivecalc2,
        s.passivecalc3,
        s.passivecalc4,
        s.passivecalc5,
        s.petmax,
        s.sumsk1calc,
        s.sumsk2calc,
        s.sumsk3calc,
        s.sumsk4calc,
        s.sumsk5calc,
        s.cltcalc1,
        s.cltcalc2,
        s.cltcalc3,
        s.perdelay,
        s.skpoints,
        s.delay,
        s.calc1,
        s.calc2,
        s.calc3,
        s.calc4,
        s.tohitcalc,
        s.dmgsympercalc,
        s.edmgsympercalc,
        s.elensympercalc,
    ]
}

/// Every `misscode` formula field of a missiles record.
fn miss_calcs(m: &Missiles) -> [u32; 7] {
    [
        m.srvcalc1,
        m.cltcalc1,
        m.shitcalc1,
        m.chitcalc1,
        m.dmgcalc1,
        m.dmgsympercalc,
        m.edmgsympercalc,
    ]
}
