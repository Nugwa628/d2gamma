//! Warcry rings retain their state, duration and six aura stat formulas.
use super::{quote, LiftError, Source};

pub(super) fn part(src: &dyn Source) -> Result<String, LiftError> {
    if matches!(src.skill("progressive").trim(), "1" | "true") {
        return Err(LiftError::Unreadable(
            "warcry needs progressive missile selection".into(),
        ));
    }
    let missile = src.skill("srvmissilea");
    let missile = src
        .missile(missile.trim())
        .ok_or_else(|| LiftError::Unknown {
            column: "srvmissilea",
            value: missile.clone(),
        })?;
    let state = src.skill("aurastate");
    let mut out = format!(
        "kind = \"warcry\"\nmissile = {missile}\nframes = {}\n",
        quote(&super::formulas::text(src, "auralencalc")?)
    );
    if !state.trim().is_empty() {
        let state = src.state(state.trim()).ok_or_else(|| LiftError::Unknown {
            column: "aurastate",
            value: state.clone(),
        })?;
        out.push_str(&format!("state = {state}\n"));
    }
    for n in 1..=6 {
        let stat = src.skill(&format!("aurastat{n}"));
        if stat.trim().is_empty() {
            continue;
        }
        let stat = src.stat(stat.trim()).ok_or_else(|| LiftError::Unknown {
            column: "aurastat",
            value: stat.clone(),
        })?;
        let value = super::formulas::text(src, &format!("aurastatcalc{n}"))?;
        out.push_str(&format!(
            "[[skill.part.stat]]\nstat = {stat}\nvalue = {}\n",
            quote(&value)
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn warcry_lifts_ring_and_state_effect_without_engine_body() {
        let row = Rows::new(&[
            ("srvdofunc", "68"),
            ("srvmissilea", "0"),
            ("aurastate", "30"),
            ("auralencalc", "250"),
            ("aurastat1", "31"),
            ("aurastatcalc1", "7+lvl"),
        ]);
        let lifted = lift(7, &row).unwrap();
        assert_eq!(lifted.family, Family::Warcry);
        let rules = SkillRules::from_toml(&lifted.toml).unwrap();
        let Part::Warcry(p) = &rules.get(7).unwrap().parts[0] else {
            panic!("warcry")
        };
        assert_eq!(p.state, Some(30));
        assert_eq!(p.stats.len(), 1);
        assert!(!lifted.toml.contains("engine_body"));
    }
}
