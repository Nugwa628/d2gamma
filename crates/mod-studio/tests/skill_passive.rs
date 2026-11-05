//! Layered passive definitions survive editing and use the refresh lifecycle.
use d2_sim::rules::data_mod::{Part, SkillRules};
use mod_studio::kit::{parse_skill, skill_toml};

#[test]
fn passive_editor_round_trip_keeps_layer_suppression_and_formulas() {
    let text = "[[skill]]\nid=17\n[[skill.part]]\nkind='passive'\nstate=30\nlayer=4\nblocked_by=31\n[[skill.part.stat]]\nstat=31\nvalue='lvl*3'\n";
    let entry = parse_skill(text).unwrap();
    let saved = skill_toml(&entry);
    assert_eq!(parse_skill(&saved).unwrap(), entry);
    let rules = SkillRules::from_toml(&saved).unwrap();
    assert_eq!(rules, SkillRules::from_toml(text).unwrap());
    let Part::Passive(p) = &rules.get(17).unwrap().parts[0] else {
        panic!("passive")
    };
    assert_eq!(p.layer, 4);
    assert_eq!(p.blocked_by, Some(31));
}
