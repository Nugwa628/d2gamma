//! Generated buff data round-trips through Studio and the engine reader.
use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml, PartEntry};

#[test]
fn timed_buff_editor_round_trip_preserves_stats_events_and_frames() {
    let text="[[skill]]\nid=17\n[[skill.part]]\nkind='timed_buff'\nstate=30\nframes='13+lvl'\n[[skill.part.stat]]\nstat=16\nvalue='7*lvl'\n[[skill.part.event]]\nevent=1\ncallback=2\n";
    let entry = parse_skill(text).unwrap();
    let PartEntry::TimedBuff { stats, events, .. } = &entry.parts[0] else {
        panic!("buff")
    };
    assert_eq!(stats.len(), 1);
    assert_eq!(events[0].callback, 2);
    let saved = skill_toml(&entry);
    assert_eq!(parse_skill(&saved).unwrap(), entry);
    assert_eq!(
        SkillRules::from_toml(&saved).unwrap(),
        SkillRules::from_toml(text).unwrap()
    );
}
