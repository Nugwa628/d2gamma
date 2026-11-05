//! Warcries carry editable state effects through their outgoing missiles.
use super::{AuraStat, Formula};
use crate::rules::{BuffPulse, SkillHost};
use crate::skills::use_::bodies::helpers2::ring_offset;
use crate::skills::use_::bodies::MissileRequest;
use crate::units::UnitId;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WarcryPart {
    pub missile: u16,
    pub state: Option<u16>,
    pub frames: Formula,
    #[serde(default, rename = "stat")]
    pub stats: Vec<AuraStat>,
}

pub(super) fn hit(
    w: &mut dyn SkillHost,
    source: UnitId,
    target: UnitId,
    skill: i32,
    level: i32,
    p: &WarcryPart,
) -> bool {
    let Some(state) = p.state else {
        return false;
    };
    let stats: Vec<_> = p
        .stats
        .iter()
        .map(|s| (s.stat, s.value.eval(w, source, skill, level)))
        .collect();
    let frames = p.frames.eval(w, source, skill, level);
    w.warcry_state(
        source,
        target,
        &BuffPulse {
            skill,
            level,
            state,
            frames,
            stats: &stats,
            events: &[],
        },
    )
}
pub(super) fn run(
    w: &mut dyn SkillHost,
    u: UnitId,
    skill: i32,
    level: i32,
    p: &WarcryPart,
) -> bool {
    w.set_unit_flags(u, w.unit_flags(u) | 0x40);
    let at = w.position(u);
    let mut req = MissileRequest {
        flags: 3,
        x: at.0,
        y: at.1,
        skill,
        level,
        ..MissileRequest::new(u, i32::from(p.missile))
    };
    for step in 0..64 {
        let (x, y) = ring_offset(step);
        req.target_x = x;
        req.target_y = y;
        w.spawn_missile(req);
    }
    hit(w, u, u, skill, level, p);
    true
}

#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn warcry_is_explicit_opt_in_and_bounded() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='warcry'\nmissile=0\nstate=30\nframes='13+lvl'\n[[skill.part.stat]]\nstat=31\nvalue='lvl*3'\n";
        let rules = SkillRules::from_toml(text).unwrap();
        assert!(matches!(rules.get(7).unwrap().parts[0], Part::Warcry(_)));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&format!(
            "{text}{}",
            "[[skill.part.stat]]\nstat=31\nvalue='1'\n".repeat(6)
        ))
        .is_err());
    }
}
