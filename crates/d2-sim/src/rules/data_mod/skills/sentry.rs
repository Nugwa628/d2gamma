//! Placed class sentries reuse the summon, pet setup and monster-mode pipeline.
use super::Formula;
use crate::rules::{SentrySetup, SkillHost};
use crate::units::UnitId;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentryPart {
    pub class: u16,
    pub pet_type: u16,
    pub mode: u8,
    pub pet_max: Formula,
    #[serde(default)]
    pub in_town: bool,
}
pub(super) fn run(
    w: &mut dyn SkillHost,
    u: UnitId,
    skill: i32,
    level: i32,
    p: &SentryPart,
) -> bool {
    w.set_unit_flags(u, w.unit_flags(u) | 0x40);
    let Some(at) = w.target_position(u) else {
        return false;
    };
    let cap = p.pet_max.eval(w, u, skill, level);
    w.place_sentry(
        u,
        &SentrySetup {
            class: p.class,
            pet_type: p.pet_type,
            mode: p.mode,
            cap,
            skill,
            level,
            in_town: p.in_town,
            at,
        },
    )
    .is_some()
}
#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn sentry_is_opt_in_and_checks_class_pet_type_and_mode() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='sentry'\nclass=0\npet_type=2\nmode=1\npet_max='5'\n";
        let rules = SkillRules::from_toml(text).unwrap();
        assert!(matches!(rules.get(7).unwrap().parts[0], Part::Sentry(_)));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&text.replace("pet_type=2", "pet_type=128")).is_err());
        assert!(SkillRules::from_toml(&text.replace("mode=1", "mode=16")).is_err());
    }
}
