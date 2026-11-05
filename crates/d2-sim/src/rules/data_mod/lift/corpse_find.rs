//! Find Potion and Find Item retain their distinct failed-roll return values.
use super::{quote, LiftError, Source};
pub(super) fn part(src: &dyn Source) -> Result<String, LiftError> {
    let potion = src.skill("srvdofunc").trim() == "69";
    Ok(format!("kind = \"corpse_find\"\nchance = {}\npotion = {potion}\nband1 = \"par1\"\nband2 = \"par2\"\nband3 = \"par3\"\nband4 = \"par4\"\n",quote(&super::formulas::text(src,"calc1")?)))
}
#[cfg(test)]
mod tests {
    use super::super::{lift, tests::Rows, Family};
    use crate::rules::data_mod::{Part, SkillRules};
    #[test]
    fn corpse_find_lifts_chance_and_class_specific_drop_selection() {
        for slot in ["69", "72"] {
            let row = Rows::new(&[("srvdofunc", slot), ("calc1", "min(100,lvl*10)")]);
            let lifted = lift(7, &row).unwrap();
            assert_eq!(lifted.family, Family::CorpseFind);
            let rules = SkillRules::from_toml(&lifted.toml).unwrap();
            let Part::CorpseFind(p) = &rules.get(7).unwrap().parts[0] else {
                panic!("corpse find")
            };
            assert_eq!(p.potion, slot == "69");
            assert_eq!(p.band3.text(), "par3");
            assert!(!lifted.toml.contains("engine_body"));
        }
    }
}
