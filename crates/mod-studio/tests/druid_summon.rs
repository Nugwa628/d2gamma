//! Generated Druid summon data stays editable through a Studio save/load.
use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml, PartEntry};

#[test]
fn druid_summon_editor_preserves_mode_cap_and_level() {
    let text = "[[skill]]\nid=17\n[[skill.part]]\nkind='druid_summon'\nclass=0\npet_type=2\nmode=1\npet_max='2+lvl/3'\nlevel='max(1,lvl)'\n";
    let entry = parse_skill(text).unwrap();
    assert!(matches!(
        &entry.parts[0],
        PartEntry::DruidSummon {
            mode: 1,
            pet_type: 2,
            ..
        }
    ));
    let saved = skill_toml(&entry);
    assert_eq!(parse_skill(&saved).unwrap(), entry);
    assert_eq!(
        SkillRules::from_toml(&saved).unwrap(),
        SkillRules::from_toml(text).unwrap()
    );
}
