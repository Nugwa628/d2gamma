//! Corpse finding consumes eligibility before its chance roll, including a failed roll.
use super::Formula;
use crate::rules::SkillHost;
use crate::units::UnitId;
use serde::Deserialize;
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpseFindPart {
    pub chance: Formula,
    #[serde(default)]
    pub potion: bool,
    pub band1: Formula,
    pub band2: Formula,
    pub band3: Formula,
    pub band4: Formula,
}
pub(super) fn run(
    w: &mut dyn SkillHost,
    u: UnitId,
    skill: i32,
    level: i32,
    p: &CorpseFindPart,
) -> bool {
    let Some(corpse) = w.target(u) else {
        return false;
    };
    if !w.consume_find_corpse(corpse, p.potion) {
        return false;
    }
    let chance = p.chance.eval(w, u, skill, level);
    if w.seed(u).roll(100) as i32 >= chance {
        return p.potion;
    }
    if p.potion {
        let bands = [
            p.band3.eval(w, u, skill, level),
            p.band4.eval(w, u, skill, level),
        ];
        w.corpse_potion_drop(u, corpse, bands);
    } else {
        let roll = w.seed(u).roll(100) as i32;
        let bands = [&p.band1, &p.band2, &p.band3, &p.band4].map(|f| f.eval(w, u, skill, level));
        let quality = crate::skills::use_::bodies::b3_lvl12::find_item_quality(roll, bands);
        w.corpse_item_drop(u, corpse, quality);
    }
    true
}
#[cfg(test)]
mod tests {
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn corpse_find_is_opt_in_and_keeps_chance_and_bands_explicit() {
        let text="[[skill]]\nid=7\n[[skill.part]]\nkind='corpse_find'\nchance='100'\nband1='par1'\nband2='par2'\nband3='par3'\nband4='par4'\n";
        assert!(matches!(
            SkillRules::from_toml(text).unwrap().get(7).unwrap().parts[0],
            Part::CorpseFind(_)
        ));
        assert!(SkillRules::none().get(7).is_none());
        assert!(SkillRules::from_toml(&text.replace("chance='100'", "chance='unknown('")).is_err());
    }
}
