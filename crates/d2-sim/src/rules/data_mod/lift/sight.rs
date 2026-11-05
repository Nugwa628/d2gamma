//! Inner Sight and Slow Missiles retain their explicit single stat pulse.
use super::{quote, LiftError, Source};
pub(super) fn part(src: &dyn Source) -> Result<String, LiftError> {
    let c = src.skill("auratargetstate");
    let state = src
        .state(c.trim())
        .filter(|&s| s > 0 && s < 0x8000)
        .ok_or_else(|| LiftError::Unknown {
            column: "auratargetstate",
            value: c.clone(),
        })?;
    let c = src.skill("aurastat1");
    let stat = src
        .stat(c.trim())
        .filter(|&s| s < 0x8000)
        .ok_or_else(|| LiftError::Unknown {
            column: "aurastat1",
            value: c.clone(),
        })?;
    let filter = src.skill("aurafilter").trim().parse::<u32>().unwrap_or(0);
    Ok(format!("kind = \"sight\"\nstate = {state}\nstat = {stat}\nfilter = {filter}\nvalue = {}\nrange = {}\nframes = {}\n",quote(&super::formulas::text(src,"aurastatcalc1")?),quote(&super::formulas::text(src,"aurarangecalc")?),quote(&super::formulas::text(src,"auralencalc")?)))
}
#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn sight_lift_keeps_its_target_stat_and_formulas() {
        let s = Rows::new(&[
            ("srvdofunc", "6"),
            ("auratargetstate", "30"),
            ("aurastat1", "31"),
            ("aurastatcalc1", "lvl*2"),
            ("aurarangecalc", "12"),
            ("auralencalc", "60"),
        ]);
        let l = lift(7, &s).unwrap();
        assert_eq!(l.family, Family::Sight);
        let r = SkillRules::from_toml(&l.toml).unwrap();
        let Part::Sight(p) = &r.get(7).unwrap().parts[0] else {
            panic!("sight")
        };
        assert_eq!(p.stat, 31);
        assert!(!l.toml.contains("engine_body"));
    }
}
