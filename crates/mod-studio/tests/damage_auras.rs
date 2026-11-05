//! Damage auras keep separate caster and target stats through authoring.
use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml};
#[test]
fn damage_aura_editor_round_trip_keeps_both_stat_lists_and_flags() {
    let text="[[skill]]\nid=17\n[[skill.part]]\nkind='damage_aura'\nstate=30\ntarget_state=31\nperiod='25+lvl'\nrange='12'\ncost='0'\nfilter=34179\nfreeze=true\nhit_class=13\nresult_flags=32\nhit_flags=2\n[[skill.part.own_stat]]\nstat=31\nvalue='lvl'\n[[skill.part.stat]]\nstat=16\nvalue='12'\n";
    let entry = parse_skill(text).unwrap();
    let saved = skill_toml(&entry);
    assert_eq!(parse_skill(&saved).unwrap(), entry);
    assert_eq!(
        SkillRules::from_toml(&saved).unwrap(),
        SkillRules::from_toml(text).unwrap()
    );
}
