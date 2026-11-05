//! Ravens keep their untargetable flag, zero regeneration and explicit base level.
use super::Formula;
use crate::rules::{SkillHost, Summon};
use crate::stats::ids;
use crate::units::UnitId;
use serde::Deserialize;
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RavenPart {
    pub class: u16,
    pub pet_type: u16,
    pub mode: u8,
    pub pet_max: Formula,
    pub level: Formula,
}
pub(super) fn run(w: &mut dyn SkillHost, u: UnitId, skill: i32, level: i32, p: &RavenPart) -> bool {
    w.set_unit_flags(u, w.unit_flags(u) | 0x40);
    let cap = p.pet_max.eval(w, u, skill, level);
    let Some(pet) = w.summon(Summon {
        flags: 0,
        owner: u,
        class: i32::from(p.class),
        ai: 0,
        mode: i32::from(p.mode),
        x: 0,
        y: 0,
        pet_type: i32::from(p.pet_type),
        pet_max: cap,
    }) else {
        return false;
    };
    w.set_unit_flags(pet, w.unit_flags(pet) & !4);
    w.set_stat(pet, ids::HPREGEN, 0);
    let monster_level = p.level.eval(w, u, skill, level).max(1);
    w.pet_base_stats(u, pet, monster_level, level);
    w.pet_skill_stats(u, pet, skill, level);
    true
}
#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn ravens_are_opt_in_and_bound_their_initial_mode_and_signed_pet_type() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='raven'\nclass=0\npet_type=2\nmode=1\npet_max='1'\nlevel='lvl'\n";
        assert!(matches!(
            SkillRules::from_toml(text).unwrap().get(7).unwrap().parts[0],
            Part::Raven(_)
        ));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&text.replace("pet_type=2", "pet_type=128")).is_err());
    }
}
