//! Form and vine data remain editable without routing an engine body.
use d2_sim::rules::data_mod::SkillRules;
use mod_studio::kit::{parse_skill, skill_toml};
#[test]
fn form_and_vine_editor_round_trip_preserves_formulas_and_states() {
    for part in ["kind='form'\nstate=30\nframes='250+lvl'\ndelay='13'\n[[skill.part.stat]]\nstat=31\nvalue='lvl*3'\n", "kind='vine'\nclass=0\npet_type=2\nstate=150\npet_max='1'\nlevel='max(1,lvl)'\n"] {
  let text=format!("[[skill]]\nid=17\n[[skill.part]]\n{part}");let entry=parse_skill(&text).unwrap();let saved=skill_toml(&entry);
  assert_eq!(parse_skill(&saved).unwrap(),entry);assert_eq!(SkillRules::from_toml(&saved).unwrap(),SkillRules::from_toml(&text).unwrap());
 }
}
