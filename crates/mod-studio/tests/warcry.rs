//! Warcry state effects remain editable through a Studio save/load.
use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml, PartEntry};
#[test]
fn warcry_editor_round_trip_preserves_ring_and_state_stats() {
    let text="[[skill]]\nid=17\n[[skill.part]]\nkind='warcry'\nmissile=0\nstate=30\nframes='13+lvl'\n[[skill.part.stat]]\nstat=31\nvalue='3*lvl'\n";
    let entry = parse_skill(text).unwrap();
    let PartEntry::Warcry { state, stats, .. } = &entry.parts[0] else {
        panic!("warcry")
    };
    assert_eq!(*state, Some(30));
    assert_eq!(stats.len(), 1);
    let saved = skill_toml(&entry);
    assert_eq!(parse_skill(&saved).unwrap(), entry);
    assert_eq!(
        SkillRules::from_toml(&saved).unwrap(),
        SkillRules::from_toml(text).unwrap()
    );
}
