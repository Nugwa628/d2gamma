//! Vine summons keep their special movement state and level-only setup.
use super::Formula;
use crate::rules::{SkillHost, Summon};
use crate::stats::ids;
use crate::units::UnitId;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VinePart {
    pub class: u16,
    pub pet_type: u16,
    pub state: u16,
    pub pet_max: Formula,
    pub level: Formula,
}

pub(super) fn run(w: &mut dyn SkillHost, u: UnitId, skill: i32, level: i32, p: &VinePart) -> bool {
    w.set_unit_flags(u, w.unit_flags(u) | 0x40);
    let pet_max = p.pet_max.eval(w, u, skill, level);
    let monster_level = p.level.eval(w, u, skill, level).max(1);
    let Some(pet) = w.summon(Summon {
        flags: 0,
        owner: u,
        class: i32::from(p.class),
        ai: 0,
        mode: 8,
        x: 0,
        y: 0,
        pet_type: i32::from(p.pet_type),
        pet_max,
    }) else {
        return false;
    };
    w.join_owner_targets(u, pet);
    w.state_on(pet, p.state, true);
    w.set_stat(pet, ids::LEVEL, monster_level);
    w.pet_skill_stats(u, pet, skill, level);
    true
}

#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn vines_are_opt_in_and_have_signed_pet_type_bounds() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='vine'\nclass=0\npet_type=2\nstate=30\npet_max='2'\nlevel='lvl'\n";
        let rules = SkillRules::from_toml(text).unwrap();
        assert!(matches!(rules.get(7).unwrap().parts[0], Part::Vine(_)));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&text.replace("pet_type=2", "pet_type=128")).is_err());
    }
}
