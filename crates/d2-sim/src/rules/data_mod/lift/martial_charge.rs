//! Martial charge-up rows expose their own counter, duration and incremental bonus.
use super::{quote, LiftError, Source};
pub(super) fn part(src: &dyn Source) -> Result<String, LiftError> {
    let c = src.skill("aurastate");
    let state = src
        .state(c.trim())
        .filter(|s| *s > 0 && *s < 0x8000)
        .ok_or_else(|| LiftError::Unknown {
            column: "aurastate",
            value: c.clone(),
        })?;
    let c = src.skill("aurastat1");
    let counter = src
        .stat(c.trim())
        .filter(|s| *s < 0x8000)
        .ok_or_else(|| LiftError::Unknown {
            column: "aurastat1",
            value: c.clone(),
        })?;
    let damage = src.skill("srcdam").trim().parse::<u8>().unwrap_or(0);
    let damage = if damage == 0 { 128 } else { damage };
    let rating = if src.skill("tohitcalc").trim().is_empty() {
        let b = src.skill("tohit").trim().parse::<i32>().unwrap_or(0);
        let p = src.skill("levtohit").trim().parse::<i32>().unwrap_or(0);
        format!("{b}+(lvl-1)*{p}")
    } else {
        super::formulas::text(src, "tohitcalc")?
    };
    let mut out=format!("kind = \"martial_charge\"\nstate = {state}\ncounter = {counter}\nframes = {}\nattack_rating = {}\nsource_damage = {damage}\nclaws = {}\n",quote(&super::formulas::text(src,"auralencalc")?),quote(&rating),src.skill("srvdofunc").trim()=="35");
    let c = src.skill("aurastat2");
    if !c.trim().is_empty() {
        let stat =
            src.stat(c.trim())
                .filter(|s| *s < 0x8000)
                .ok_or_else(|| LiftError::Unknown {
                    column: "aurastat2",
                    value: c.clone(),
                })?;
        out.push_str(&format!(
            "[skill.part.bonus]\nstat = {stat}\nvalue = {}\n",
            quote(&super::formulas::text(src, "aurastatcalc2")?)
        ));
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn martial_charge_lift_retains_state_counter_bonus_and_claw_variant() {
        for slot in ["34", "35"] {
            let src = Rows::new(&[
                ("srvdofunc", slot),
                ("aurastate", "30"),
                ("aurastat1", "204"),
                ("aurastat2", "31"),
                ("aurastatcalc2", "lvl*2"),
                ("auralencalc", "60"),
            ]);
            let lifted = lift(7, &src).unwrap();
            assert_eq!(lifted.family, Family::MartialCharge);
            let rules = SkillRules::from_toml(&lifted.toml).unwrap();
            let Part::MartialCharge(p) = &rules.get(7).unwrap().parts[0] else {
                panic!("charge")
            };
            assert_eq!(p.counter, 204);
            assert_eq!(p.source_damage, 128);
            assert_eq!(p.claws, slot == "35");
            assert!(p.bonus.is_some());
            assert!(!lifted.toml.contains("engine_body"));
        }
    }
}
