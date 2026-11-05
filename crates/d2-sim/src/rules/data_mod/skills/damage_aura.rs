//! Damaging class auras roll once, then pulse states and apply that roll nearby.
use super::{AuraStat, Formula};
use crate::combat::{DamageRecord, RoomKind};
use crate::rules::{AuraPulse, SkillHost};
use crate::stats::ids;
use crate::units::{UnitId, UnitType};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageAuraPart {
    pub state: u16,
    #[serde(default)]
    pub target_state: Option<u16>,
    pub period: Formula,
    pub range: Formula,
    pub cost: Formula,
    pub filter: u32,
    #[serde(default)]
    pub freeze: bool,
    #[serde(default)]
    pub hit_class: u8,
    #[serde(default)]
    pub result_flags: u16,
    #[serde(default)]
    pub hit_flags: u32,
    #[serde(default, rename = "own_stat")]
    pub own_stats: Vec<AuraStat>,
    #[serde(default, rename = "stat")]
    pub stats: Vec<AuraStat>,
}

pub(super) fn run(
    w: &mut dyn SkillHost,
    u: UnitId,
    skill: i32,
    level: i32,
    p: &DamageAuraPart,
) -> bool {
    if !w.aura_state_valid(p.state) {
        return false;
    }
    let cost = p.cost.eval(w, u, skill, level);
    let mana = w.stat(u, ids::MANA, 0);
    let period = p.period.eval(w, u, skill, level);
    let duration = w.aura_period_frames(period);
    let player = w.unit_type(u) == Some(UnitType::Player);
    let own: Vec<_> = p
        .own_stats
        .iter()
        .map(|s| {
            (
                s.stat,
                if !player || mana >= cost {
                    s.value.eval(w, u, skill, level)
                } else {
                    0
                },
            )
        })
        .collect();
    let pulse = AuraPulse {
        skill,
        level,
        state: p.state,
        target_state: p.state,
        range: 0,
        frames: duration,
        filter: 0,
        stats: &own,
    };
    w.aura_state(u, u, &pulse, p.freeze);
    if w.room_kind(u) == RoomKind::Town {
        return true;
    }
    let target_state = p.target_state.filter(|&s| w.aura_state_valid(s));
    let stats: Vec<_> = if target_state.is_some() {
        p.stats
            .iter()
            .map(|s| {
                (
                    s.stat,
                    if mana >= cost {
                        s.value.eval(w, u, skill, level)
                    } else {
                        0
                    },
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut record = DamageRecord::default();
    if w.roll_skill_element(u, skill, level, &mut record) != 0 {
        record.hit_class |= 0xD;
    }
    if p.hit_class != 0 {
        record.hit_class = u32::from(p.hit_class);
    }
    record.result |= p.result_flags | 0x20;
    record.hit_flags |= p.hit_flags;
    let range = p.range.eval(w, u, skill, level);
    for target in w.aura_targets(u, range, p.filter) {
        if p.freeze && !w.freeze_aura_target(target) {
            continue;
        }
        if let Some(state) = target_state.filter(|_| stats.iter().any(|&(_, v)| v != 0)) {
            let pulse = AuraPulse {
                state,
                target_state: state,
                stats: &stats,
                ..pulse
            };
            w.aura_state(u, target, &pulse, false);
        }
        let mut damage = record;
        w.apply_damage(u, target, true, &mut damage);
        w.react(u, target, &mut damage);
        if p.freeze {
            let on = w.seed(target).step() % 100 < 20;
            w.state_on(target, 107, on);
        }
    }
    if cost > 0 && player {
        w.state_on(
            u,
            crate::skills::use_::bodies::helpers::state::NOMANAREGEN as u16,
            false,
        );
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn damage_aura_is_opt_in_and_bounds_its_two_stat_lists() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='damage_aura'\nstate=30\nperiod='25'\nrange='12'\ncost='0'\nfilter=0\n";
        assert!(matches!(
            SkillRules::from_toml(text).unwrap().get(7).unwrap().parts[0],
            Part::DamageAura(_)
        ));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&format!(
            "{text}{}",
            "[[skill.part.own_stat]]\nstat=31\nvalue='1'\n".repeat(6)
        ))
        .is_err());
    }
}
