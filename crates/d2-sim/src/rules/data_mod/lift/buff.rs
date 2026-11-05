//! Defensive-buff cells become editable state, duration, stats and events.

use super::{number, quote, LiftError, Source};

pub(super) fn part(src: &dyn Source) -> Result<String, LiftError> {
    let cell = src.skill("aurastate");
    let state = src.state(cell.trim()).ok_or_else(|| LiftError::Unknown {
        column: "aurastate",
        value: cell.clone(),
    })?;
    let mut out = format!(
        "kind = \"timed_buff\"\nstate = {state}\nframes = {}\n",
        quote(&super::formulas::text(src, "auralencalc")?)
    );
    for (name, calc, count) in [
        ("aurastat", "aurastatcalc", 6),
        ("passivestat", "passivecalc", 5),
    ] {
        for n in 1..=count {
            let cell = src.skill(&format!("{name}{n}"));
            if cell.trim().is_empty() {
                continue;
            }
            let stat = src.stat(cell.trim()).ok_or_else(|| LiftError::Unknown {
                column: "buff stat",
                value: cell.clone(),
            })?;
            out.push_str(&format!(
                "[[skill.part.stat]]\nstat = {stat}\nvalue = {}\n",
                quote(&super::formulas::text(src, &format!("{calc}{n}"))?)
            ));
        }
    }
    for n in 1..=3 {
        let cell = src.skill(&format!("auraevent{n}"));
        if cell.trim().is_empty() {
            break;
        }
        let event = src
            .event(cell.trim())
            .filter(|&v| v < 0x8000)
            .ok_or_else(|| LiftError::Unknown {
                column: "auraevent",
                value: cell.clone(),
            })?;
        let callback = number(&src.skill(&format!("auraeventfunc{n}")))
            .filter(|&v| v < 0xffff)
            .ok_or_else(|| LiftError::Unknown {
                column: "auraeventfunc",
                value: src.skill(&format!("auraeventfunc{n}")),
            })?;
        out.push_str(&format!(
            "[[skill.part.event]]\nevent = {event}\ncallback = {callback}\n"
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};

    #[test]
    fn defensive_buff_lifts_stats_and_events_without_engine_body() {
        let row = Rows::new(&[
            ("srvdofunc", "18"),
            ("aurastate", "30"),
            ("auralencalc", "13+lvl"),
            ("aurastat1", "16"),
            ("aurastatcalc1", "lvl*3"),
            ("passivestat1", "31"),
            ("passivecalc1", "7"),
            ("auraevent1", "1"),
            ("auraeventfunc1", "2"),
        ]);
        let lifted = lift(8, &row).unwrap();
        assert_eq!(lifted.family, Family::Buff);
        let rules = SkillRules::from_toml(&lifted.toml).unwrap();
        let Part::TimedBuff(p) = &rules.get(8).unwrap().parts[0] else {
            panic!("buff")
        };
        assert_eq!(p.stats.len(), 2);
        assert_eq!(p.events[0].event, 1);
        assert!(!lifted.toml.contains("engine_body"));
    }
}
