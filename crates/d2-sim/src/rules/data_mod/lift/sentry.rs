//! Placed sentries retain their class, mode, pet cap and town-placement rule.
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
    let town = matches!(src.skill("InTown").trim(), "1" | "true");
    Ok(format!("kind = \"sentry\"\nclass = {class}\npet_type = {pet}\nmode = {mode}\nin_town = {town}\npet_max = {}\n",quote(&super::formulas::text(src,"petmax")?)))
}
#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn sentry_lift_keeps_placement_rule_and_cap_editable() {
        let row = Rows::new(&[
            ("srvdofunc", "45"),
            ("summon", "0"),
            ("pettype", "2"),
            ("summode", "1"),
            ("petmax", "2+lvl"),
            ("InTown", "1"),
        ]);
        let lifted = lift(7, &row).unwrap();
        assert_eq!(lifted.family, Family::Sentry);
        let rules = SkillRules::from_toml(&lifted.toml).unwrap();
        let Part::Sentry(p) = &rules.get(7).unwrap().parts[0] else {
            panic!("sentry")
        };
        assert!(p.in_town);
        assert!(!lifted.toml.contains("engine_body"));
    }
}
