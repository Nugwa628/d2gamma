//! Golem authoring keeps cap and owner-level scaling explicit.
use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml};

#[test]
fn golem_editor_round_trip_keeps_setup_formulas() {
    let text="[[skill]]\nid=17\n[[skill.part]]\nkind='golem'\nclass=0\npet_type=2\nmode=1\npet_max='1+lvl'\nlevel='0'\n";
    let entry = parse_skill(text).unwrap();
    let saved = skill_toml(&entry);
    assert_eq!(parse_skill(&saved).unwrap(), entry);
    assert_eq!(
        SkillRules::from_toml(&saved).unwrap(),
        SkillRules::from_toml(text).unwrap()
    );
}
