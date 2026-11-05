//! Class shadows retain their second-formula stat rule and equipment setup.
use super::{quote, LiftError, Source};

pub(super) fn part(src: &dyn Source) -> Result<String, LiftError> {
    let c = src.skill("summon");
    let class = src
        .monster(c.trim())
        .filter(|v| *v < 0x8000)
        .ok_or_else(|| LiftError::Unknown {
            column: "summon",
            value: c.clone(),
        })?;
    let c = src.skill("pettype");
    let pet = src
        .pet_type(c.trim())
        .filter(|v| *v < 128)
        .ok_or_else(|| LiftError::Unknown {
            column: "pettype",
            value: c.clone(),
        })?;
    let c = src.skill("summode");
    let mode = if c.trim().is_empty() {
        0
    } else {
        src.monster_mode(c.trim())
            .ok_or_else(|| LiftError::Unknown {
                column: "summode",
                value: c.clone(),
            })?
    };
    let mode = if mode < 16 { mode } else { 1 };
    let mut out = format!("kind = \"shadow\"\nclass = {class}\npet_type = {pet}\nmode = {mode}\n");
    for (field, column) in [("pet_max", "petmax"), ("frames", "auralencalc")] {
        out.push_str(&format!(
            "{field} = {}\n",
            quote(&super::formulas::text(src, column)?)
        ));
    }
    out.push_str("life_percent = \"(lvl-1)*par1\"\nitem_level = \"ln56\"\n");
    let c = src.skill("aurastate");
    if !c.trim().is_empty() && c.trim() != "0" {
        let state = src
            .state(c.trim())
            .filter(|v| *v > 0 && *v < 0x8000)
            .ok_or_else(|| LiftError::Unknown {
                column: "aurastate",
                value: c.clone(),
            })?;
        out.push_str(&format!("state = {state}\n"));
    }
    let umod = src.skill("sumumod").trim().parse::<u16>().unwrap_or(0);
    if (1..=42).contains(&umod) {
        out.push_str(&format!("umod = {umod}\n"));
    }
    for (prefix, count, calc) in [
        ("aurastat", 6, "aurastatcalc2"),
        ("passivestat", 5, "passivecalc2"),
    ] {
        for n in 1..=count {
            let c = src.skill(&format!("{prefix}{n}"));
            if c.trim().is_empty() {
                continue;
            }
            let stat =
                src.stat(c.trim())
                    .filter(|v| *v < 0x8000)
                    .ok_or_else(|| LiftError::Unknown {
                        column: "shadow stat",
                        value: c.clone(),
                    })?;
            out.push_str(&format!(
                "[[skill.part.stat]]\nstat = {stat}\nvalue = {}\n",
                quote(&super::formulas::text(src, calc)?)
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
    fn shadows_lift_explicit_equipment_life_and_second_stat_formulas() {
        let src = Rows::new(&[
            ("srvdofunc", "49"),
            ("summon", "0"),
            ("pettype", "2"),
            ("summode", "1"),
            ("petmax", "1"),
            ("aurastat1", "31"),
            ("aurastatcalc1", "99"),
            ("aurastatcalc2", "lvl*3"),
        ]);
        let lifted = lift(7, &src).unwrap();
        assert_eq!(lifted.family, Family::Shadow);
        let rules = SkillRules::from_toml(&lifted.toml).unwrap();
        let Part::Shadow(p) = &rules.get(7).unwrap().parts[0] else {
            panic!("shadow")
        };
        assert_eq!(p.stats[0].value.text(), "(lvl * 3)");
        assert!(!lifted.toml.contains("engine_body"));
    }
}
