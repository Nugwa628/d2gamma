//! Confuse and Attract expose AI control separately from ordinary stat curses.
use super::{quote, LiftError, Source};
pub(super) fn part(src: &dyn Source) -> Result<String, LiftError> {
    let c = src.skill("auratargetstate");
    let state = src
        .state(c.trim())
        .filter(|s| *s > 0 && *s < 0x8000)
        .ok_or_else(|| LiftError::Unknown {
            column: "auratargetstate",
            value: c.clone(),
        })?;
    let attract = src.skill("srvdofunc").trim() == "59";
    let filter = src.skill("aurafilter").trim().parse::<u32>().unwrap_or(0);
    let mut out=format!("kind = \"ai_curse\"\nstate = {state}\nrange = {}\nframes = {}\nfilter = {filter}\nattract = {attract}\n",quote(&super::formulas::text(src,"aurarangecalc")?),quote(&super::formulas::text(src,"auralencalc")?));
    if !attract {
        for i in 2..=6 {
            let c = src.skill(&format!("aurastat{i}"));
            if c.trim().is_empty() {
                break;
            }
            let stat =
                src.stat(c.trim())
                    .filter(|s| *s < 0x8000)
                    .ok_or_else(|| LiftError::Unknown {
                        column: "AI curse stat",
                        value: c.clone(),
                    })?;
            out.push_str(&format!(
                "[[skill.part.stat]]\nstat = {stat}\nvalue = {}\n",
                quote(&super::formulas::text(src, &format!("aurastatcalc{i}"))?)
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
    fn ai_curse_lift_keeps_control_mode_and_confuse_second_stat_slot() {
        for slot in ["59", "61"] {
            let src = Rows::new(&[
                ("srvdofunc", slot),
                ("auratargetstate", "30"),
                ("aurastat1", "16"),
                ("aurastatcalc1", "99"),
                ("aurastat2", "31"),
                ("aurastatcalc2", "lvl*2"),
            ]);
            let l = lift(7, &src).unwrap();
            assert_eq!(l.family, Family::AiCurse);
            let r = SkillRules::from_toml(&l.toml).unwrap();
            let Part::AiCurse(p) = &r.get(7).unwrap().parts[0] else {
                panic!("AI curse")
            };
            assert_eq!(p.attract, slot == "59");
            assert_eq!(p.stats.len(), usize::from(slot == "61"));
            assert!(!l.toml.contains("engine_body"));
        }
    }
}
