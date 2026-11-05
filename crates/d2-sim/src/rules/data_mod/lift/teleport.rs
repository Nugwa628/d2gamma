//! Sorceress movement reuses the registered teleport part. This lift does
//! not add another movement host or change the default game's restrictions.

pub(super) fn part(notes: &mut Vec<String>) -> String {
    notes.push("the data teleport uses unit placement without the Rust body's level teleport/line restriction check; use only where the mod permits teleporting".to_owned());
    "kind = \"teleport\"\n".to_owned()
}

#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};

    #[test]
    fn teleport_lifts_to_the_existing_part_without_an_engine_body() {
        let row = Rows::new(&[("skill", "Blink"), ("srvdofunc", "27")]);
        let lifted = lift(12, &row).unwrap();
        assert_eq!(lifted.family, Family::Teleport);
        assert_eq!(lifted.cells, [("srvdofunc", "400".to_owned())]);
        assert!(matches!(
            SkillRules::from_toml(&lifted.toml)
                .unwrap()
                .get(12)
                .unwrap()
                .parts[..],
            [Part::Teleport(_)]
        ));
        assert!(!lifted.toml.contains("engine_body"));
        assert!(lifted.notes[0].contains("restriction"));
    }
}
