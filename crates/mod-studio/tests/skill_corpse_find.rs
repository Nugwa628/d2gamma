//! Corpse-find authoring keeps chance, category and quality bands explicit.
use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml};
#[test]
fn corpse_find_editor_round_trip_keeps_chance_and_all_bands() {
    let text="[[skill]]\nid=17\n[[skill.part]]\nkind='corpse_find'\nchance='25+lvl'\npotion=true\nband1='par1'\nband2='par2'\nband3='par3'\nband4='par4'\n";
    let entry = parse_skill(text).unwrap();
    let saved = skill_toml(&entry);
    assert_eq!(parse_skill(&saved).unwrap(), entry);
    assert_eq!(
        SkillRules::from_toml(&saved).unwrap(),
        SkillRules::from_toml(text).unwrap()
    );
}
