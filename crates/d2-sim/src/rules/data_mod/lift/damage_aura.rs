//! Damaging auras keep separate caster passive and target aura stats.
use super::{number, quote, LiftError, Source};
pub(super) fn part(src: &dyn Source) -> Result<String, LiftError> {
    let state = src.skill("aurastate");
    let state = src.state(state.trim()).ok_or_else(|| LiftError::Unknown {
        column: "aurastate",
        value: state.clone(),
    })?;
    let signed = |column: &'static str| -> Result<i32, LiftError> {
        let v = src.skill(column);
        if v.trim().is_empty() {
            Ok(0)
        } else {
            v.trim()
                .parse::<i32>()
                .map(|n| i32::from(n as u16 as i16))
                .map_err(|_| LiftError::Unknown { column, value: v })
        }
    };
    let shift = number(&src.skill("manashift")).unwrap_or(0) as u8;
    let scale = 1i32.wrapping_shl(u32::from(shift));
    let cost = format!(
        "({}+{}*(lvl-1))*{}",
        signed("mana")?,
        signed("lvlmana")?,
        scale
    );
    let freeze = src.skill("srvdofunc").trim() == "81";
    let mut out=format!("kind = \"damage_aura\"\nstate = {state}\nperiod = {}\nrange = {}\ncost = {}\nfilter = {}\nfreeze = {freeze}\nhit_class = {}\nresult_flags = {}\nhit_flags = {}\n",quote(&super::formulas::text(src,"perdelay")?),quote(&super::formulas::text(src,"aurarangecalc")?),quote(&cost),number(&src.skill("aurafilter")).unwrap_or(0),number(&src.skill("hitclass")).unwrap_or(0),number(&src.skill("resultflags")).unwrap_or(0),number(&src.skill("hitflags")).unwrap_or(0));
    let target = src.skill("auratargetstate");
    if !target.trim().is_empty() {
        let target = src.state(target.trim()).ok_or_else(|| LiftError::Unknown {
            column: "auratargetstate",
            value: target.clone(),
        })?;
        out.push_str(&format!("target_state = {target}\n"));
    }
    for (name, calc, count, key) in [
        ("passivestat", "passivecalc", 5, "own_stat"),
        ("aurastat", "aurastatcalc", 6, "stat"),
    ] {
        for n in 1..=count {
            let cell = src.skill(&format!("{name}{n}"));
            if cell.trim().is_empty() {
                continue;
            }
            let stat = src.stat(cell.trim()).ok_or_else(|| LiftError::Unknown {
                column: "aura stat",
                value: cell.clone(),
            })?;
            out.push_str(&format!(
                "[[skill.part.{key}]]\nstat = {stat}\nvalue = {}\n",
                quote(&super::formulas::text(src, &format!("{calc}{n}"))?)
            ));
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn damage_aura_lifts_distinct_stats_period_and_freeze() {
        for slot in ["66", "81"] {
            let row = Rows::new(&[
                ("srvdofunc", slot),
                ("aurastate", "30"),
                ("auratargetstate", "31"),
                ("perdelay", "25"),
                ("aurarangecalc", "12"),
                ("passivestat1", "31"),
                ("passivecalc1", "lvl"),
                ("aurastat1", "16"),
                ("aurastatcalc1", "12"),
            ]);
            let lifted = lift(7, &row).unwrap();
            assert_eq!(lifted.family, Family::DamageAura);
            let rules = SkillRules::from_toml(&lifted.toml).unwrap();
            let Part::DamageAura(p) = &rules.get(7).unwrap().parts[0] else {
                panic!("aura")
            };
            assert_eq!(p.freeze, slot == "81");
            assert_eq!(p.own_stats.len(), 1);
            assert_eq!(p.stats.len(), 1);
            assert!(!lifted.toml.contains("engine_body"));
        }
    }
}
