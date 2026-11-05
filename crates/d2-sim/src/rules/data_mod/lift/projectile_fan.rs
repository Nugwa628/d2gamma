//! Multiple Shot/Teeth/Shock Wave spreads and Twister/Tornado jitter counts.
use super::{quote, LiftError, Source};
pub(super) fn part(src: &dyn Source) -> Result<String, LiftError> {
    let cell = src.skill("srvmissilea");
    let missile = src
        .missile(cell.trim())
        .filter(|&v| v < 0x8000)
        .ok_or_else(|| LiftError::Unknown {
            column: "srvmissilea",
            value: cell.clone(),
        })?;
    let slot = src.skill("srvdofunc");
    let jitter = matches!(slot.trim(), "17" | "118");
    let empty_ok = slot.trim() == "17";
    let mut secondary = String::new();
    let cell = src.skill("srvmissileb");
    if !jitter && !cell.trim().is_empty() {
        let b = src
            .missile(cell.trim())
            .filter(|&v| v < 0x8000)
            .ok_or_else(|| LiftError::Unknown {
                column: "srvmissileb",
                value: cell.clone(),
            })?;
        secondary = format!("non_bow = {b}\n");
    }
    Ok(format!("kind = \"projectile_fan\"\nmissile = {missile}\n{secondary}jitter = {jitter}\nempty_ok = {empty_ok}\ncount = {}\ncentral = {}\nactivate = {}\n",quote(&super::formulas::text(src,"calc1")?),quote(&if jitter{"0".into()}else{super::formulas::text(src,"calc3")?}),quote(&if jitter{"0".into()}else{super::formulas::text(src,"calc2")?})))
}
#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn projectile_fan_lift_retains_explicit_formula_modes() {
        for slot in ["8", "118"] {
            let s = Rows::new(&[
                ("srvdofunc", slot),
                ("srvmissilea", "0"),
                ("srvmissileb", "1"),
                ("calc1", "lvl+2"),
                ("calc2", "3"),
                ("calc3", "2"),
            ]);
            let l = lift(7, &s).unwrap();
            assert_eq!(l.family, Family::ProjectileFan);
            let r = SkillRules::from_toml(&l.toml).unwrap();
            let Part::ProjectileFan(p) = &r.get(7).unwrap().parts[0] else {
                panic!("fan")
            };
            assert_eq!(p.jitter, slot == "118");
            assert_eq!(p.non_bow, if slot == "118" { None } else { Some(1) });
            assert!(!l.toml.contains("engine_body"));
        }
    }
}
