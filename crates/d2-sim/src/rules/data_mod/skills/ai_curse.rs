//! AI curses select targets and explicit state values without dispatching a table body.
use super::{AuraStat, Formula, MAX_AREA_TARGETS};
use crate::rules::{CursePulse, SkillHost};
use crate::units::UnitId;
use serde::Deserialize;
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiCursePart {
    pub state: u16,
    pub range: Formula,
    pub frames: Formula,
    #[serde(default)]
    pub filter: u32,
    #[serde(default)]
    pub attract: bool,
    #[serde(default, rename = "stat")]
    pub stats: Vec<AuraStat>,
}
pub(super) fn run(
    w: &mut dyn SkillHost,
    u: UnitId,
    skill: i32,
    level: i32,
    p: &AiCursePart,
) -> bool {
    if !w.aura_state_valid(p.state) {
        return false;
    }
    let target = if p.attract {
        let Some(target) = w.target(u).filter(|&t| w.ai_curse_eligible(u, t, true)) else {
            return false;
        };
        Some(target)
    } else {
        None
    };
    w.set_unit_flags(u, w.unit_flags(u) | 0x40);
    let range = p.range.eval(w, u, skill, level);
    let frames = p.frames.eval(w, u, skill, level);
    let frames = w.ai_curse_duration(frames);
    let stats: Vec<_> = p
        .stats
        .iter()
        .map(|s| (s.stat, s.value.eval(w, u, skill, level)))
        .collect();
    let pulse = CursePulse {
        skill,
        level,
        state: p.state,
        range,
        frames,
        filter: p.filter,
        stats: &stats,
    };
    if let Some(target) = target {
        w.ai_curse_align(target, true);
    }
    let found = w.ai_curse_targets(u, range, p.filter);
    if found.is_none() && !p.attract {
        return false;
    }
    for unit in found.unwrap_or_default().into_iter().take(MAX_AREA_TARGETS) {
        if !w.ai_curse_eligible(u, unit, p.attract) {
            continue;
        }
        if let Some(target) = target {
            w.ai_curse_redirect(unit, Some(target), frames);
        } else if w.ai_curse_state(u, unit, &pulse, false) {
            w.ai_curse_align(unit, false);
            w.ai_curse_redirect(unit, None, frames);
        }
    }
    if let Some(target) = target {
        w.ai_curse_state(u, target, &pulse, true);
    }
    true
}
#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn ai_curse_is_opt_in_and_attract_has_no_unused_stats() {
        let text =
            "[[skill]]\nid=7\n[[skill.part]]\nkind='ai_curse'\nstate=30\nrange='12'\nframes='60'\n";
        assert!(matches!(
            SkillRules::from_toml(text).unwrap().get(7).unwrap().parts[0],
            Part::AiCurse(_)
        ));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&text.replace("state=30", "state=0")).is_err());
        assert!(SkillRules::from_toml(&format!(
            "{text}attract=true\n[[skill.part.stat]]\nstat=31\nvalue='1'\n"
        ))
        .is_err());
    }
}
