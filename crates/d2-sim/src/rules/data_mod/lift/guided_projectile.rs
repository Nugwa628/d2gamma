//! Guided Arrow and Bone Spirit expose their missile rows and damage initializer.
use super::{quote, LiftError, Source};
pub(super) fn part(src: &dyn Source) -> Result<String, LiftError> {
    let c = src.skill("srvmissilea");
    let a = src
        .missile(c.trim())
        .filter(|&v| v < 0x8000)
        .ok_or_else(|| LiftError::Unknown {
            column: "srvmissilea",
            value: c.clone(),
        })?;
    let c = src.skill("srvmissileb");
    let b = if c.trim().is_empty() {
        String::new()
    } else {
        let b = src
            .missile(c.trim())
            .filter(|&v| v < 0x8000)
            .ok_or_else(|| LiftError::Unknown {
                column: "srvmissileb",
                value: c.clone(),
            })?;
        format!("non_bow = {b}\n")
    };
    Ok(format!(
        "kind = \"guided_projectile\"\nmissile = {a}\n{b}damage_percent = {}\n",
        quote(&super::formulas::text(src, "calc1")?)
    ))
}
#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn guided_projectile_lift_keeps_damage_formula_and_alternative() {
        let s = Rows::new(&[
            ("srvdofunc", "10"),
            ("srvmissilea", "0"),
            ("srvmissileb", "1"),
            ("calc1", "lvl*3"),
        ]);
        let l = lift(7, &s).unwrap();
        assert_eq!(l.family, Family::GuidedProjectile);
        let r = SkillRules::from_toml(&l.toml).unwrap();
        let Part::GuidedProjectile(p) = &r.get(7).unwrap().parts[0] else {
            panic!("guided")
        };
        assert_eq!(p.non_bow, Some(1));
        assert!(!l.toml.contains("engine_body"));
    }
}
