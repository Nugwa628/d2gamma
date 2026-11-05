//! Class golems combine explicit summon values with pet stats and summon resistance.
use super::Formula;
use crate::rules::{SkillHost, Summon};
use crate::units::UnitId;
use serde::Deserialize;
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GolemPart {
    pub class: u16,
    pub pet_type: u16,
    pub mode: u8,
    pub pet_max: Formula,
    /// Nonpositive selects the normal owner-level scaling.
    pub level: Formula,
}
pub(super) fn run(w: &mut dyn SkillHost, u: UnitId, skill: i32, level: i32, p: &GolemPart) -> bool {
    w.set_unit_flags(u, w.unit_flags(u) | 0x40);
    let cap = p.pet_max.eval(w, u, skill, level);
    let monster_level = p.level.eval(w, u, skill, level);
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
    w.pet_base_stats(u, pet, monster_level, level);
    w.pet_skill_stats(u, pet, skill, level);
    w.summon_resistance(u, pet);
    w.announce_pet(u, pet);
    w.join_owner_targets(u, pet);
    true
}
#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn golems_are_opt_in_and_validate_setup_rows() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='golem'\nclass=0\npet_type=2\nmode=1\npet_max='1'\nlevel='0'\n";
        let rules = SkillRules::from_toml(text).unwrap();
        assert!(matches!(rules.get(7).unwrap().parts[0], Part::Golem(_)));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&text.replace("mode=1", "mode=16")).is_err());
    }
}
