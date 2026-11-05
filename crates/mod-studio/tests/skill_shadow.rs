//! Companion equipment and lifetime survive Studio authoring.
use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml};
#[test]
fn shadow_editor_round_trip_keeps_stats_optional_rows_and_formulas() {
    let text="[[skill]]\nid=17\n[[skill.part]]\nkind='shadow'\nclass=0\npet_type=2\nmode=1\npet_max='1+lvl'\nlife_percent='(lvl-1)*par1'\nitem_level='ln56'\nframes='60'\nstate=30\numod=21\n[[skill.part.stat]]\nstat=31\nvalue='lvl*3'\n";
    let entry = parse_skill(text).unwrap();
    let saved = skill_toml(&entry);
    assert_eq!(parse_skill(&saved).unwrap(), entry);
    assert_eq!(
        SkillRules::from_toml(&saved).unwrap(),
        SkillRules::from_toml(text).unwrap()
    );
}
