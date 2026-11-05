//! Form changes use the normal restrictive-state and skill-gating tables.
use super::{AuraStat, Formula};
use crate::rules::{BuffPulse, SkillHost};
use crate::units::UnitId;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormPart {
    pub state: u16,
    pub frames: Formula,
    pub delay: Formula,
    #[serde(default, rename = "stat")]
    pub stats: Vec<AuraStat>,
}

pub(super) fn run(w: &mut dyn SkillHost, u: UnitId, skill: i32, level: i32, p: &FormPart) -> bool {
    let frames = p.frames.eval(w, u, skill, level);
    let delay = p.delay.eval(w, u, skill, level);
    let stats: Vec<_> = p
        .stats
        .iter()
        .map(|s| (s.stat, s.value.eval(w, u, skill, level)))
        .collect();
    w.shift_form(
        u,
        &BuffPulse {
            skill,
            level,
            state: p.state,
            frames,
            stats: &stats,
            events: &[],
        },
        delay,
    )
}

#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn form_is_opt_in_and_has_bounded_explicit_stats() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='form'\nstate=30\nframes='250'\ndelay='13'\n[[skill.part.stat]]\nstat=31\nvalue='lvl*3'\n";
        let rules = SkillRules::from_toml(text).unwrap();
        assert!(matches!(rules.get(7).unwrap().parts[0], Part::Form(_)));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&format!(
            "{text}{}",
            "[[skill.part.stat]]\nstat=31\nvalue='1'\n".repeat(6)
        ))
        .is_err());
    }
}
