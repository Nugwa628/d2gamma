//! The Raven family keeps class, mode, cap and level editable.
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
    Ok(format!("kind = \"raven\"\nclass = {class}\npet_type = {pet}\nmode = {mode}\npet_max = {}\nlevel = {}\n",quote(&super::formulas::text(src,"petmax")?),quote(&super::formulas::text(src,"calc2")?)))
}

#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn raven_lift_keeps_its_level_cap_and_mode_as_data() {
        let row = Rows::new(&[
            ("srvdofunc", "114"),
            ("summon", "0"),
            ("pettype", "2"),
            ("summode", "1"),
            ("petmax", "3"),
            ("calc2", "lvl"),
        ]);
        let lifted = lift(7, &row).unwrap();
        assert_eq!(lifted.family, Family::Raven);
        let rules = SkillRules::from_toml(&lifted.toml).unwrap();
        let Part::Raven(p) = &rules.get(7).unwrap().parts[0] else {
            panic!("Raven")
        };
        assert_eq!(p.pet_type, 2);
        assert_eq!(p.mode, 1);
        assert!(!lifted.toml.contains("engine_body"));
    }
}
