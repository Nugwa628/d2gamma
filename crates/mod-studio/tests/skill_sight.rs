use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml};
#[test]
fn sight_editor_round_trip_keeps_rows_and_formulas() {
    let text="[[skill]]\nid=7\n[[skill.part]]\nkind='sight'\nstate=30\nstat=31\nvalue='lvl*2'\nrange='12'\nframes='60'\n";
    let e = parse_skill(text).unwrap();
    let s = skill_toml(&e);
    assert_eq!(parse_skill(&s).unwrap(), e);
    assert_eq!(
        SkillRules::from_toml(&s).unwrap(),
        SkillRules::from_toml(text).unwrap()
    );
}
