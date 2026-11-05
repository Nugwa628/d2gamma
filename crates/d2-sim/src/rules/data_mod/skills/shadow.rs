//! Shadow companions compose the existing summon, equipment and source-link paths.
use super::{AuraStat, Formula};
use crate::rules::{SkillHost, Summon};
use crate::stats::ids;
use crate::units::UnitId;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowPart {
    pub class: u16,
    pub pet_type: u16,
    pub mode: u8,
    pub pet_max: Formula,
    pub life_percent: Formula,
    pub item_level: Formula,
    pub frames: Formula,
    #[serde(default)]
    pub state: Option<u16>,
    #[serde(default)]
    pub umod: Option<u8>,
    #[serde(default, rename = "stat")]
    pub stats: Vec<AuraStat>,
}

pub(super) fn run(
    w: &mut dyn SkillHost,
    u: UnitId,
    skill: i32,
    level: i32,
    p: &ShadowPart,
) -> bool {
    w.set_unit_flags(u, w.unit_flags(u) | 0x40);
    let cap = p.pet_max.eval(w, u, skill, level);
    let Some(pet) = w.summon(Summon {
        flags: 0,
        owner: u,
        class: i32::from(p.class),
        ai: 0,
        mode: i32::from(p.mode),
        x: 0,
        y: 0,
        pet_type: i32::from(p.pet_type),
        pet_max: cap,
    }) else {
        return false;
    };
    w.set_stat(pet, ids::LEVEL, w.stat(u, ids::LEVEL, 0));
    let percent = if level > 1 {
        p.life_percent.eval(w, pet, skill, level)
    } else {
        0
    };
    let stats: Vec<_> = p
        .stats
        .iter()
        .filter(|_| level > 1)
        .map(|s| crate::rules::StateStat {
            stat: s.stat,
            layer: 0,
            value: s.value.eval(w, pet, skill, level),
        })
        .collect();
    w.shadow_stats(pet, level, percent, &stats, p.umod);
    let item_level = p.item_level.eval(w, u, skill, level);
    w.equip_pet(u, pet, skill, level, item_level);
    w.remove_timers(pet, 2, Some(0));
    w.pet_timer(pet, 2, 20);
    if let Some(state) = p.state {
        w.state_on(pet, state, true);
    }
    w.link_source(pet, Some(u));
    let frames = p.frames.eval(w, u, skill, level);
    if frames > 0 {
        w.pet_timer(pet, 7, frames);
        w.pet_umod(pet, 21, 0);
    }
    w.join_owner_targets(u, pet);
    true
}

#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn shadow_setup_is_opt_in_and_bounds_monster_and_effect_rows() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='shadow'\nclass=0\npet_type=2\nmode=1\npet_max='1'\nlife_percent='(lvl-1)*par1'\nitem_level='ln56'\nframes='0'\n";
        assert!(matches!(
            SkillRules::from_toml(text).unwrap().get(7).unwrap().parts[0],
            Part::Shadow(_)
        ));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&text.replace("mode=1", "mode=16")).is_err());
        assert!(SkillRules::from_toml(&format!("{text}umod=43\n")).is_err());
    }
}
