//! Form lifter: state, duration, toggle delay and stat formulas are editable.
use super::{quote, LiftError, Source};

pub(super) fn part(src: &dyn Source) -> Result<String, LiftError> {
    let state = src.skill("aurastate");
    let state = src.state(state.trim()).ok_or_else(|| LiftError::Unknown {
        column: "aurastate",
        value: state.clone(),
    })?;
    let mut out = format!(
        "kind = \"form\"\nstate = {state}\nframes = {}\ndelay = {}\n",
        quote(&super::formulas::text(src, "auralencalc")?),
        quote(&super::formulas::text(src, "delay")?)
    );
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
    fn form_lifts_state_length_delay_and_stats_without_engine_body() {
        let row = Rows::new(&[
            ("srvdofunc", "116"),
            ("aurastate", "30"),
            ("auralencalc", "250"),
            ("delay", "13"),
            ("aurastat1", "31"),
            ("aurastatcalc1", "lvl*3"),
        ]);
        let lifted = lift(7, &row).unwrap();
        assert_eq!(lifted.family, Family::Form);
        let rules = SkillRules::from_toml(&lifted.toml).unwrap();
        let Part::Form(p) = &rules.get(7).unwrap().parts[0] else {
            panic!("form")
        };
        assert_eq!(p.state, 30);
        assert_eq!(p.stats.len(), 1);
        assert!(!lifted.toml.contains("engine_body"));
    }
}
