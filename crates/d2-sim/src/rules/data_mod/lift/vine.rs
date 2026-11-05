//! Vines keep their special state and level-only pet setup in editable data.
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
    let state = crate::skills::use_::bodies::helpers2::st::VINE_BEAST;
    Ok(format!("kind = \"vine\"\nclass = {class}\npet_type = {pet}\nstate = {state}\npet_max = {}\nlevel = {}\n",quote(&super::formulas::text(src,"petmax")?),quote(&super::formulas::text(src,"calc2")?)))
}
#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn vine_lift_retains_level_cap_and_special_state_without_engine_body() {
        let row = Rows::new(&[
            ("srvdofunc", "115"),
            ("summon", "0"),
            ("pettype", "2"),
            ("petmax", "1"),
            ("calc2", "lvl"),
        ]);
        let lifted = lift(7, &row).unwrap();
        assert_eq!(lifted.family, Family::Vine);
        let rules = SkillRules::from_toml(&lifted.toml).unwrap();
        let Part::Vine(p) = &rules.get(7).unwrap().parts[0] else {
            panic!("vine")
        };
        assert_eq!(p.state, 150);
        assert!(!lifted.toml.contains("engine_body"));
    }
}
