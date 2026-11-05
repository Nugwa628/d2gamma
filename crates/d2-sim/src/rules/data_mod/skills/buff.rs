//! Timed class buffs use explicit stats and event callbacks. The default
//! game's table-driven defensive-buff body is unaffected.

use serde::Deserialize;

use super::{AuraStat, Formula};
use crate::rules::{BuffPulse, SkillHost};
use crate::units::UnitId;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuffEvent {
    pub event: u16,
    pub callback: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuffPart {
    pub state: u16,
    /// Base frames, retaining sub-second and non-integral-second lengths.
    pub frames: Formula,
    #[serde(default, rename = "stat")]
    pub stats: Vec<AuraStat>,
    #[serde(default, rename = "event")]
    pub events: Vec<BuffEvent>,
}

pub(super) fn run(w: &mut dyn SkillHost, u: UnitId, skill: i32, level: i32, p: &BuffPart) -> bool {
    let frames = p.frames.eval(w, u, skill, level).max(0);
    let stats: Vec<_> = p
        .stats
        .iter()
        .map(|s| (s.stat, s.value.eval(w, u, skill, level)))
        .collect();
    let events: Vec<_> = p.events.iter().map(|e| (e.event, e.callback)).collect();
    w.timed_buff(
        u,
        &BuffPulse {
            skill,
            level,
            state: p.state,
            frames,
            stats: &stats,
            events: &events,
        },
    )
}

#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};

    #[test]
    fn timed_buff_is_explicit_bounded_and_opt_in() {
        let text = "[[skill]]\nid=7\n[[skill.part]]\nkind='timed_buff'\nstate=30\nframes='13+lvl'\n[[skill.part.stat]]\nstat=16\nvalue='5*lvl'\n[[skill.part.event]]\nevent=1\ncallback=2\n";
        let rules = SkillRules::from_toml(text).unwrap();
        let Part::TimedBuff(p) = &rules.get(7).unwrap().parts[0] else {
            panic!("buff")
        };
        assert_eq!(p.stats.len(), 1);
        assert_eq!(p.events[0].callback, 2);
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&text.replace("frames='13+lvl'", "seconds=1")).is_err());
        let excessive = format!(
            "{text}{}",
            "[[skill.part.event]]\nevent=1\ncallback=2\n".repeat(3)
        );
        assert!(SkillRules::from_toml(&excessive).is_err());
    }
}
