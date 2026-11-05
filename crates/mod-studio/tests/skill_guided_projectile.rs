use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml};
#[test]
fn guided_projectile_editor_round_trip_keeps_rows_and_formula() {
    let text="[[skill]]\nid=7\n[[skill.part]]\nkind='guided_projectile'\nmissile=0\nnon_bow=1\ndamage_percent='lvl*3'\n";
    let e = parse_skill(text).unwrap();
    let s = skill_toml(&e);
    assert_eq!(parse_skill(&s).unwrap(), e);
    assert_eq!(
        SkillRules::from_toml(&s).unwrap(),
        SkillRules::from_toml(text).unwrap()
    );
}
