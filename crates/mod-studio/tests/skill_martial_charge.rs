//! Explicit charge state survives Studio authoring.
use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml};
#[test]
fn martial_charge_editor_round_trip_keeps_counter_bonus_and_hit_settings() {
    let text="[[skill]]\nid=7\n[[skill.part]]\nkind='martial_charge'\nstate=30\ncounter=204\nframes='60+lvl'\nattack_rating='lvl*3'\nsource_damage=128\nclaws=true\n[skill.part.bonus]\nstat=31\nvalue='lvl'\n";
    let entry = parse_skill(text).unwrap();
    let saved = skill_toml(&entry);
    assert_eq!(parse_skill(&saved).unwrap(), entry);
    assert_eq!(
        SkillRules::from_toml(&saved).unwrap(),
        SkillRules::from_toml(text).unwrap()
    );
}
