use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml};
#[test]
fn projectile_fan_editor_round_trip_keeps_modes_and_formulas() {
    let text="[[skill]]\nid=7\n[[skill.part]]\nkind='projectile_fan'\nmissile=0\nnon_bow=1\ncount='lvl+2'\ncentral='lvl'\nactivate='3'\njitter=false\n";
    let e = parse_skill(text).unwrap();
    let s = skill_toml(&e);
    assert_eq!(parse_skill(&s).unwrap(), e);
    assert_eq!(
        SkillRules::from_toml(&s).unwrap(),
        SkillRules::from_toml(text).unwrap()
    );
}
