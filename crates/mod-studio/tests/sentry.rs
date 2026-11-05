//! Placed sentry settings survive an authoring round trip.
use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml, PartEntry};
#[test]
fn sentry_editor_round_trip_retains_town_mode_and_cap() {
    let text="[[skill]]\nid=17\n[[skill.part]]\nkind='sentry'\nclass=0\npet_type=2\nmode=1\npet_max='2+lvl/3'\nin_town=true\n";
    let entry = parse_skill(text).unwrap();
    assert!(matches!(
        entry.parts[0],
        PartEntry::Sentry { in_town: true, .. }
    ));
    let saved = skill_toml(&entry);
    assert_eq!(parse_skill(&saved).unwrap(), entry);
    assert_eq!(
        SkillRules::from_toml(&saved).unwrap(),
        SkillRules::from_toml(text).unwrap()
    );
}
