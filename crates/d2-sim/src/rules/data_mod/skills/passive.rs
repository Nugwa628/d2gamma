//! Passive definitions run during skill refresh, never as a cast.
use super::AuraStat;
use crate::rules::SkillHost;
use crate::units::UnitId;
use serde::Deserialize;
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PassivePart {
    pub state: u16,
    #[serde(default)]
    pub layer: u16,
    #[serde(default)]
    pub blocked_by: Option<u16>,
    #[serde(default, rename = "stat")]
    pub stats: Vec<AuraStat>,
}
pub(super) fn refresh(w: &mut dyn SkillHost, u: UnitId, skill: i32, p: &PassivePart) {
    let level = if p.blocked_by.is_some_and(|s| w.has_state(u, s)) {
        0
    } else {
        w.skill_level(u, skill)
    };
    if level <= 0 {
        w.passive_values(u, p.state, skill, 0, &[]);
        return;
    }
    if w.state_stat(u, p.state, 351) == Some(level) {
        return;
    }
    let stats: Vec<_> = p
        .stats
        .iter()
        .map(|s| crate::rules::StateStat {
            stat: s.stat,
            layer: p.layer,
            value: s.value.eval(w, u, skill, level),
        })
        .collect();
    w.passive_values(u, p.state, skill, level, &stats);
}
#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn passive_refresh_is_opt_in_and_limits_the_stat_list() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='passive'\nstate=30\nlayer=4\n[[skill.part.stat]]\nstat=31\nvalue='lvl'\n";
        assert!(matches!(
            SkillRules::from_toml(text).unwrap().get(7).unwrap().parts[0],
            Part::Passive(_)
        ));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&format!(
            "{text}{}",
            "[[skill.part.stat]]\nstat=31\nvalue='1'\n".repeat(5)
        ))
        .is_err());
    }
}
