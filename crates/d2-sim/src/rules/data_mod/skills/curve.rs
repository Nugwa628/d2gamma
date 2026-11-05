//! Evaluating a mod formula for a chart, with the engine's own evaluator.
//!
//! A tool that draws "damage by skill level" has no game world: no unit, and a
//! skills table of one row that holds the skill's `Param1` to `Param8`. This
//! is the engine's formula evaluator ([`eval_desc`]) run on that row with no
//! unit, so `lvl`, `ln12`, `dm34`, `par1` .. `par8`, `min`, `max` and the rest
//! of the arithmetic give what the game gives at that level. Names that need a
//! unit (`rand`, stat reads) give what the evaluator gives without one.

use d2_data::tables::{Record, Skilldesc, Skills};

use super::Formula;
use crate::rng::Seed;
use crate::skills::{eval_desc, SkillEntry, SkillTables, SkillUnits, LEVEL_CAP_114D};
use crate::units::UnitType;

/// The `Param1` .. `Param8` columns of a skills row.
pub type SkillParams = [i32; 8];

/// A world with no unit state at all: every read gives zero.
#[derive(Default)]
struct NoUnits {
    seed: Seed,
}

impl SkillUnits for NoUnits {
    type Unit = u8;
    type Item = u8;
    fn unit_type(&self, _: u8) -> UnitType {
        UnitType::Player
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

/// A one-row skills table whose only content is `params`.
fn tables(params: &SkillParams) -> SkillTables {
    let mut row = Skills::decode(&vec![0u8; Skills::SIZE]);
    row.param1 = params[0] as u32;
    row.param2 = params[1] as u32;
    row.param3 = params[2] as u32;
    row.param4 = params[3] as u32;
    row.param5 = params[4] as u32;
    row.param6 = params[5] as u32;
    row.param7 = params[6] as u32;
    row.param8 = params[7] as u32;
    SkillTables {
        skills: vec![row],
        skilldesc: vec![Skilldesc::decode(&vec![0u8; Skilldesc::SIZE])],
        missiles: Vec::new(),
        skills_code: Vec::new(),
        miss_code: Vec::new(),
        level_cap: LEVEL_CAP_114D,
        stat_count: 0,
        data_rules: d2_data::bin::DataRules::Mod,
    }
}

impl Formula {
    /// The formula's value for a skill whose row has `params`, at each of
    /// `levels`, by the engine's evaluator (no unit; see the module header).
    pub fn curve(
        &self,
        params: &SkillParams,
        levels: std::ops::RangeInclusive<i32>,
    ) -> Vec<(i32, i32)> {
        let t = tables(params);
        let mut w = NoUnits::default();
        levels
            .map(|lvl| (lvl, eval_desc(&mut w, &t, self.code(), None, 0, 0, lvl)))
            .collect()
    }
}
