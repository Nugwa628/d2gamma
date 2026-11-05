//! Caster-centered sight debuffs with one explicit timed stat.
use super::{Formula, MAX_AREA_TARGETS};
use crate::rules::{CursePulse, SkillHost};
use crate::units::UnitId;
use serde::Deserialize;
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SightPart {
    pub state: u16,
    pub stat: u16,
    pub value: Formula,
    pub range: Formula,
    pub frames: Formula,
    #[serde(default)]
    pub filter: u32,
}
pub(super) fn run(w: &mut dyn SkillHost, u: UnitId, skill: i32, level: i32, p: &SightPart) -> bool {
    if !w.sight_rows_valid(p.state, p.stat) {
        return false;
    }
    let frames = p.frames.eval(w, u, skill, level);
    let value = p.value.eval(w, u, skill, level);
    let range = p.range.eval(w, u, skill, level);
    let pulse = CursePulse {
        skill,
        level,
        state: p.state,
        range,
        frames,
        filter: p.filter,
        stats: &[(p.stat, value)],
    };
    for target in w
        .sight_targets(u, range, p.filter)
        .into_iter()
        .take(MAX_AREA_TARGETS)
    {
        w.sight_state(u, target, &pulse);
    }
    true
}
#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn sight_is_opt_in_and_requires_a_positive_state() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='sight'\nstate=30\nstat=31\nvalue='lvl*2'\nrange='12'\nframes='60'\n";
        assert!(matches!(
            SkillRules::from_toml(text).unwrap().get(7).unwrap().parts[0],
            Part::Sight(_)
        ));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&text.replace("state=30", "state=0")).is_err());
    }
}
