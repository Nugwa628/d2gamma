//! Druid spirit, wolf and bear setup over the existing summon host.
use super::Formula;
use crate::rules::{SkillHost, Summon};
use crate::units::UnitId;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DruidSummonPart {
    pub class: u16,
    pub pet_type: u16,
    pub mode: u8,
    pub pet_max: Formula,
    pub level: Formula,
}

pub(super) fn run(
    w: &mut dyn SkillHost,
    u: UnitId,
    skill: i32,
    level: i32,
    p: &DruidSummonPart,
) -> bool {
    w.set_unit_flags(u, w.unit_flags(u) | 0x40);
    if w.target_position(u).is_none() {
        return false;
    }
    let pet_max = p.pet_max.eval(w, u, skill, level);
    let monster_level = p.level.eval(w, u, skill, level).max(1);
    let Some(pet) = w.summon(Summon {
        flags: 0,
        owner: u,
        class: i32::from(p.class),
        ai: 0,
        mode: i32::from(p.mode),
        x: 0,
        y: 0,
        pet_type: i32::from(p.pet_type),
        pet_max,
    }) else {
        return false;
    };
    w.join_owner_targets(u, pet);
    w.pet_base_stats(u, pet, monster_level, level);
    w.pet_skill_stats(u, pet, skill, level);
    true
}

#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn druid_summon_is_opt_in_and_limits_signed_pet_types_and_modes() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='druid_summon'\nclass=0\npet_type=2\nmode=1\npet_max='2+lvl/3'\nlevel='lvl'\n";
        let rules = SkillRules::from_toml(text).unwrap();
        assert!(matches!(
            rules.get(7).unwrap().parts[0],
            Part::DruidSummon(_)
        ));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&text.replace("pet_type=2", "pet_type=128")).is_err());
        assert!(SkillRules::from_toml(&text.replace("mode=1", "mode=16")).is_err());
    }
}
