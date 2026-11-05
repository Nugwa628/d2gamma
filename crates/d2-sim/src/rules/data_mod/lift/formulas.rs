//! Resolve author table names once; emitted formulas use stable row numbers.
//! This is authoring-time translation, never a change to default evaluation.

use super::{LiftError, Source};
use crate::skills::special::SKILLCALC_CODES;
use d2_data::calc::{compile, CalcLinks, Family};

struct Links<'a>(&'a dyn Source);
impl CalcLinks for Links<'_> {
    fn skill(&self, key: &[u8]) -> Option<u32> {
        self.0
            .skill_row(std::str::from_utf8(key).ok()?)
            .map(u32::from)
    }
    fn missile(&self, key: &[u8]) -> Option<u32> {
        self.0
            .missile(std::str::from_utf8(key).ok()?)
            .map(u32::from)
    }
    fn stat(&self, key: &[u8]) -> Option<u32> {
        self.0.stat(std::str::from_utf8(key).ok()?).map(u32::from)
    }
    fn skillcalc(&self, code: u32) -> Option<u32> {
        SKILLCALC_CODES
            .iter()
            .position(|name| {
                let mut bytes = [b' '; 4];
                let n = name.len().min(4);
                bytes[..n].copy_from_slice(&name.as_bytes()[..n]);
                u32::from_le_bytes(bytes) == code
            })
            .map(|i| i as u32)
    }
    fn misscalc(&self, _: u32) -> Option<u32> {
        None
    }
}

pub(super) fn text(src: &dyn Source, column: &str) -> Result<String, LiftError> {
    let cell = src.skill(column);
    if cell.trim().is_empty() {
        return Ok("0".to_owned());
    }
    let compiled = compile(Family::Skills, &Links(src), cell.as_bytes())
        .map_err(|e| LiftError::Unreadable(format!("{column}: {e:?}")))?;
    if !compiled.diagnostics.is_empty() {
        return Err(LiftError::Unreadable(format!(
            "{column}: unresolved or malformed formula ({:?})",
            compiled.diagnostics
        )));
    }
    decompile(&compiled.code, 0).map_err(|e| LiftError::Unreadable(format!("{column}: {e}")))
}

/// Decode exactly the built-in skill formula vocabulary. Unknown calls and
/// malformed stacks stay errors, rather than becoming constants.
pub(crate) fn decompile(code: &[u8], offset: u32) -> Result<String, String> {
    if offset == u32::MAX {
        return Ok(String::new());
    }
    let mut at = usize::try_from(offset).map_err(|e| e.to_string())?;
    let mut stack: Vec<String> = Vec::new();
    let take = |at: &mut usize, n: usize| -> Result<&[u8], String> {
        let end = at.checked_add(n).ok_or("offset overflow")?;
        let bytes = code.get(*at..end).ok_or("it runs past the buffer")?;
        *at = end;
        Ok(bytes)
    };
    for _ in 0..1024 {
        let op = take(&mut at, 1)?[0];
        match op {
            0 => {
                return if stack.len() == 1 {
                    Ok(stack.pop().unwrap())
                } else {
                    Err("malformed formula stack".to_owned())
                }
            }
            1 => {
                let f = take(&mut at, 1)?[0];
                let name = Family::Skills
                    .keywords()
                    .get(usize::from(f))
                    .ok_or_else(|| format!("unknown formula function {f}"))?
                    .text();
                let arity = Family::Skills.arity(f) as usize;
                if stack.len() < arity {
                    return Err("missing function operand".to_owned());
                }
                let args = stack.split_off(stack.len() - arity);
                stack.push(format!("{name}({})", args.join(", ")));
            }
            4..=9 => {
                let n = [1, 2, 4][usize::from((op - 4) % 3)];
                let bytes = take(&mut at, n)?;
                let value = match n {
                    1 => i32::from(bytes[0] as i8),
                    2 => i32::from(i16::from_le_bytes([bytes[0], bytes[1]])),
                    _ => i32::from_le_bytes(bytes.try_into().unwrap()),
                };
                if op <= 6 {
                    let name = usize::try_from(value)
                        .ok()
                        .and_then(|i| SKILLCALC_CODES.get(i))
                        .ok_or("a parameter past the table")?;
                    stack.push((*name).to_owned());
                } else {
                    stack.push(value.to_string());
                }
            }
            0x0a..=0x14 => {
                let b = stack.pop().ok_or("a missing operand")?;
                let a = stack.pop().ok_or("a missing operand")?;
                let sign = ["<", ">", "<=", ">=", "==", "!=", "+", "-", "*", "/", "^"]
                    [usize::from(op - 0x0a)];
                stack.push(format!("({a} {sign} {b})"));
            }
            0x15 => {
                let a = stack.pop().ok_or("a missing operand")?;
                stack.push(format!("(0 - {a})"));
            }
            0x16 => {
                let no = stack.pop().ok_or("a missing operand")?;
                let yes = stack.pop().ok_or("a missing operand")?;
                let cond = stack.pop().ok_or("a missing operand")?;
                stack.push(format!("({cond} ? {yes} : {no})"));
            }
            _ => return Err(format!("unsupported opcode {op:#04x}")),
        }
        if stack.len() > 64 {
            return Err("formula stack exceeds64".to_owned());
        }
    }
    Err("formula exceeds1024 operations".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::data_mod::Formula;
    struct Named;
    impl Source for Named {
        fn skill(&self, column: &str) -> String {
            if column == "known" {
                "max(1,skill('focus'.blvl)*3)"
            } else {
                "skill('missing'.blvl)"
            }
            .to_owned()
        }
        fn skill_row(&self, name: &str) -> Option<u16> {
            (name == "focus").then_some(12)
        }
        fn state(&self, _: &str) -> Option<u16> {
            None
        }
        fn stat(&self, _: &str) -> Option<u16> {
            None
        }
        fn missile(&self, _: &str) -> Option<u16> {
            None
        }
        fn missile_cell(&self, _: u16, _: &str) -> String {
            String::new()
        }
    }
    #[test]
    fn named_skill_links_become_ids_and_unknown_names_stay_errors() {
        let decoded = text(&Named, "known").unwrap();
        assert!(decoded.contains("skill(12,"), "{decoded}");
        assert!(!decoded.contains("focus"));
        assert!(Formula::compile(&decoded).is_ok());
        assert!(text(&Named, "unknown").is_err());
    }
    #[test]
    fn formulas_resolve_rows_and_round_trip_every_supported_operator() {
        for expr in [
            "min(lvl,7)",
            "max(lvl,3)",
            "rand(3,8)",
            "skill(12,15)",
            "miss(2,0)",
            "stat(16,0)",
            "sklvl(12,15,3)",
            "lvl<7 ? (0-lvl)^2 : lvl>=3",
            "(lvl<=2) ? (lvl==1) : (lvl!=3)",
        ] {
            let code = compile(
                Family::Skills,
                &Links(&super::super::tests::Rows::new(&[])),
                expr.as_bytes(),
            )
            .unwrap();
            assert!(
                code.diagnostics.is_empty(),
                "{expr}: {:?}",
                code.diagnostics
            );
            let decoded = decompile(&code.code, 0).unwrap();
            assert!(Formula::compile(&decoded).is_ok(), "{decoded}");
            let again = compile(
                Family::Skills,
                &Links(&super::super::tests::Rows::new(&[])),
                decoded.as_bytes(),
            )
            .unwrap();
            assert_eq!(code.code, again.code, "{expr}");
        }
        assert!(decompile(&[1, 255, 0], 0).unwrap_err().contains("unknown"));
        assert!(decompile(&[7], 0).is_err());
        assert!(decompile(&[7, 1, 7, 2, 0], 0).is_err());
    }
}
