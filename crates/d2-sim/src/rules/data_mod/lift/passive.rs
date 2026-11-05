//! Passive rows retain their refresh lifecycle, state, item-type layer and formulas.
use super::{quote, LiftError, Source};
pub(super) fn part(src: &dyn Source) -> Result<String, LiftError> {
    let cell = src.skill("passivestate");
    let state = src
        .state(cell.trim())
        .filter(|s| *s > 0 && *s < 0x8000)
        .ok_or_else(|| LiftError::Unknown {
            column: "passivestate",
            value: cell.clone(),
        })?;
    let cell = src.skill("passiveitype");
    let layer = if cell.trim().is_empty() {
        0
    } else {
        src.item_type(cell.trim())
            .filter(|s| *s < 0x8000)
            .ok_or_else(|| LiftError::Unknown {
                column: "passiveitype",
                value: cell.clone(),
            })?
    };
    let mut out = format!("kind = \"passive\"\nstate = {state}\nlayer = {layer}\n");
    let cell = src.skill("aurastate");
    if !cell.trim().is_empty() && cell.trim() != "0" {
        let state = src
            .state(cell.trim())
            .filter(|s| *s < 0x8000)
            .ok_or_else(|| LiftError::Unknown {
                column: "aurastate",
                value: cell.clone(),
            })?;
        out.push_str(&format!("blocked_by = {state}\n"));
    }
    for n in 1..=5 {
        let cell = src.skill(&format!("passivestat{n}"));
        if cell.trim().is_empty() {
            break;
        }
        let stat = src
            .stat(cell.trim())
            .filter(|s| *s < 0x8000)
            .ok_or_else(|| LiftError::Unknown {
                column: "passive stat",
                value: cell.clone(),
            })?;
        out.push_str(&format!(
            "[[skill.part.stat]]\nstat = {stat}\nvalue = {}\n",
            quote(&super::formulas::text(src, &format!("passivecalc{n}"))?)
        ));
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn passive_lift_uses_refresh_instead_of_patching_the_cast_slot() {
        let row = Rows::new(&[
            ("srvdofunc", "0"),
            ("passivestate", "30"),
            ("passiveitype", "4"),
            ("passivestat1", "31"),
            ("passivecalc1", "lvl*3"),
        ]);
        let lifted = lift(7, &row).unwrap();
        assert_eq!(lifted.family, Family::Passive);
        assert!(lifted.cells.is_empty());
        let rules = SkillRules::from_toml(&lifted.toml).unwrap();
        let Part::Passive(p) = &rules.get(7).unwrap().parts[0] else {
            panic!("passive")
        };
        assert_eq!(p.layer, 4);
        assert_eq!(p.stats.len(), 1);
        assert!(lift(7, &Rows::new(&[("srvdofunc", "0")])).is_err());
    }
}
