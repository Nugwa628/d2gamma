//! Golem lifts keep placement separate from item conversion and corpse revives.
use super::{quote, LiftError, Source};
pub(super) fn part(src: &dyn Source) -> Result<String, LiftError> {
    let class = src.skill("summon");
    let class = src
        .monster(class.trim())
        .filter(|&v| v < 0x8000)
        .ok_or_else(|| LiftError::Unknown {
            column: "summon",
            value: class.clone(),
        })?;
    let pet = src.skill("pettype");
    let pet = src
        .pet_type(pet.trim())
        .filter(|&v| v < 128)
        .ok_or_else(|| LiftError::Unknown {
            column: "pettype",
            value: pet.clone(),
        })?;
    let mode = src.skill("summode");
    let mode = if mode.trim().is_empty() {
        0
    } else {
        src.monster_mode(mode.trim())
            .ok_or_else(|| LiftError::Unknown {
                column: "summode",
                value: mode.clone(),
            })?
    };
    let mode = if mode <= 15 { mode } else { 1 };
    Ok(format!("kind = \"golem\"\nclass = {class}\npet_type = {pet}\nmode = {mode}\nlevel = \"0\"\npet_max = {}\n",quote(&super::formulas::text(src,"petmax")?)))
}
#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn golem_lifter_keeps_cap_mode_and_owner_scaling_explicit() {
        let row = Rows::new(&[
            ("srvdofunc", "56"),
            ("summon", "0"),
            ("pettype", "2"),
            ("summode", "1"),
            ("petmax", "1+lvl"),
        ]);
        let lifted = lift(7, &row).unwrap();
        assert_eq!(lifted.family, Family::Golem);
        let rules = SkillRules::from_toml(&lifted.toml).unwrap();
        let Part::Golem(p) = &rules.get(7).unwrap().parts[0] else {
            panic!("golem")
        };
        assert_eq!(p.level.text(), "0");
        assert_eq!(p.pet_max.text(), "(1 + lvl)");
        assert!(!lifted.toml.contains("engine_body"));
    }
}
