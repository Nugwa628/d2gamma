//! Formulas in a mod's `skills.toml`: the engine's own skills formula
//! language (`lvl * 5 + ln12`, `min(par1, par2)`, `rand(10, 20)`), compiled by
//! the engine's compiler and evaluated by the engine's evaluator, so a modder
//! writes damage and durations the way the skills table does.
//!
//! The `skillcalc` names (`lvl`, `ln12`, `dm12`, `par1` .. `par8` and the
//! rest of the table) are the engine's own and fixed; they read the skill
//! row's Param columns at the skill's level. Names of other rows (`skill(`,
//! `miss(`, `stat(` with a name) need the tables, which this file is read
//! without: use the row number or put the value in a Param column.

use std::sync::OnceLock;

use d2_data::calc::{compile, CalcDiag, CalcLinks, Family};
use serde::de::{self, Deserializer};
use serde::Deserialize;

use crate::rules::SkillHost;
use crate::skills::special::SKILLCALC_TSV;
use crate::units::UnitId;

/// A compiled formula with the text it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formula {
    text: String,
    code: Vec<u8>,
}

impl Formula {
    /// A formula that always gives `n`.
    pub fn constant(n: i32) -> Formula {
        Formula::compile(&n.to_string()).expect("a number compiles")
    }

    /// Compiles `text`. A formula the compiler refuses or flags (an unknown
    /// name, an unclosed bracket, text it stopped at) is an error that says
    /// which.
    pub fn compile(text: &str) -> Result<Formula, String> {
        let compiled = compile(Family::Skills, &Links, text.as_bytes())
            .map_err(|e| format!("formula `{text}`: {e:?}"))?;
        if let Some(d) = compiled.diagnostics.first() {
            let why = match d {
                CalcDiag::UnknownName => {
                    "a name that is not a `skillcalc` name (lvl, ln12, dm12, par1 .. par8, ...)"
                }
                CalcDiag::Stop => "text the formula language stops at",
                CalcDiag::OpenParen => "a bracket that is never closed",
                CalcDiag::OpenFunction => "a function call that is never closed",
                CalcDiag::Fail => "text that is not a formula",
            };
            return Err(format!("formula `{text}`: {why}"));
        }
        if compiled.code.is_empty() {
            return Err(format!("formula `{text}`: empty"));
        }
        Ok(Formula {
            text: text.to_owned(),
            code: compiled.code,
        })
    }

    /// The compiled code, for the evaluator.
    pub(super) fn code(&self) -> &[u8] {
        &self.code
    }

    /// The text as written.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The value for the unit using `skill` at `level`.
    pub fn eval(&self, w: &mut dyn SkillHost, u: UnitId, skill: i32, level: i32) -> i32 {
        w.eval_formula(u, &self.code, skill, level)
    }
}

impl<'de> Deserialize<'de> for Formula {
    /// A number or a string, so `radius = 6` and `radius = "4 + lvl"` both read.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Formula, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Number(i32),
            Text(String),
        }
        let text = match Raw::deserialize(d)? {
            Raw::Number(n) => n.to_string(),
            Raw::Text(t) => t,
        };
        Formula::compile(&text).map_err(de::Error::custom)
    }
}

/// The engine's `skillcalc` codes, in jump-table order; no other name resolves.
struct Links;

fn codes() -> &'static [(u32, u32)] {
    static CODES: OnceLock<Vec<(u32, u32)>> = OnceLock::new();
    CODES.get_or_init(|| {
        SKILLCALC_TSV
            .lines()
            .skip(1)
            .filter_map(|line| {
                let mut cols = line.split('\t');
                let index: u32 = cols.next()?.parse().ok()?;
                let code = cols.next()?.as_bytes();
                let mut padded = [b' '; 4];
                let n = code.len().min(4);
                padded[..n].copy_from_slice(&code[..n]);
                Some((u32::from_le_bytes(padded), index))
            })
            .collect()
    })
}

impl CalcLinks for Links {
    fn skill(&self, _: &[u8]) -> Option<u32> {
        None
    }
    fn missile(&self, _: &[u8]) -> Option<u32> {
        None
    }
    fn stat(&self, _: &[u8]) -> Option<u32> {
        None
    }
    fn skillcalc(&self, code: u32) -> Option<u32> {
        codes().iter().find(|(c, _)| *c == code).map(|(_, i)| *i)
    }
    fn misscalc(&self, _: u32) -> Option<u32> {
        None
    }
}
