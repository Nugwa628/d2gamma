//! Martial charges combine an existing queued melee hit with explicit charge state.
use super::{AuraStat, Formula};
use crate::combat::DamageRecord;
use crate::rules::{BuffPulse, SkillHost};
use crate::units::UnitId;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MartialChargePart {
    pub state: u16,
    pub counter: u16,
    pub frames: Formula,
    pub attack_rating: Formula,
    pub source_damage: u8,
    #[serde(default)]
    pub claws: bool,
    #[serde(default)]
    pub bonus: Option<AuraStat>,
}
pub(super) fn run(
    w: &mut dyn SkillHost,
    u: UnitId,
    skill: i32,
    level: i32,
    p: &MartialChargePart,
) -> bool {
    if p.claws {
        w.martial_claw_frame(u);
    }
    let Some(target) = w.target(u) else {
        return p.claws;
    };
    if !w.martial_charge_valid(p.state, p.counter) {
        return p.claws;
    }
    let bonus = p.attack_rating.eval(w, u, skill, level);
    let mut rec = DamageRecord {
        result: w.melee_result(u, target, bonus, 0),
        ..DamageRecord::default()
    };
    w.start_combat(u, target, &mut rec, i32::from(p.source_damage));
    if w.queued_hit(u, target) {
        let frames = p.frames.eval(w, u, skill, level);
        let stats: Vec<_> = p
            .bonus
            .iter()
            .map(|s| (s.stat, s.value.eval(w, u, skill, level)))
            .collect();
        w.martial_charge(
            u,
            p.counter,
            &BuffPulse {
                skill,
                level,
                state: p.state,
                frames,
                stats: &stats,
                events: &[],
            },
        );
    }
    w.apply_queued(u, target);
    true
}

#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn martial_charge_is_opt_in_with_explicit_counter_and_source_scale() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='martial_charge'\nstate=30\ncounter=204\nframes='60'\nattack_rating='0'\nsource_damage=128\n";
        assert!(matches!(
            SkillRules::from_toml(text).unwrap().get(7).unwrap().parts[0],
            Part::MartialCharge(_)
        ));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&text.replace("state=30", "state=0")).is_err());
        assert!(SkillRules::from_toml(&text.replace("counter=204", "counter=32768")).is_err());
    }
}
