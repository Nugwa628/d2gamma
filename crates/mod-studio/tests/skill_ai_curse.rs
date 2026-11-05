use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml};
#[test]
fn ai_curse_editor_round_trip_keeps_control_and_stats() {
    let text="[[skill]]\nid=17\n[[skill.part]]\nkind='ai_curse'\nstate=30\nrange='lvl+3'\nframes='60'\n[[skill.part.stat]]\nstat=31\nvalue='lvl*2'\n";
    let e = parse_skill(text).unwrap();
    let s = skill_toml(&e);
    assert_eq!(parse_skill(&s).unwrap(), e);
    assert_eq!(
        SkillRules::from_toml(&s).unwrap(),
        SkillRules::from_toml(text).unwrap()
    );
}
