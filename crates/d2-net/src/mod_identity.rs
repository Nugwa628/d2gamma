//! Optional mod identity in online control messages. No extension means stock bytes.
use crate::{
    msg::ErrorCode,
    msg::{Reader, Writer},
    wire::Refused,
    NetError,
};

/// Independent of the engine build version. Cosmetic screen add-ons are excluded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModIdentity {
    pub id: String,
    pub version: String,
}
impl ModIdentity {
    pub fn valid(&self) -> bool {
        !self.id.is_empty()
            && self.id.len() <= 40
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !self.version.is_empty()
            && self.version.len() <= 64
            && self
                .version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b))
    }
    /// Versioned trailing extension, emitted only for a mod.
    pub fn write_tail(value: Option<&Self>, w: &mut Writer) {
        if let Some(value) = value {
            w.u8(0x4d).u8(1).str8(&value.id).str8(&value.version);
        }
    }
    pub fn read_tail(r: &mut Reader) -> Result<Option<Self>, NetError> {
        if r.is_empty() {
            return Ok(None);
        }
        if r.u8()? != 0x4d || r.u8()? != 1 {
            return Err(NetError::Malformed);
        }
        let value = Self {
            id: r.str8(40)?,
            version: r.str8(64)?,
        };
        if !value.valid() {
            return Err(NetError::Malformed);
        }
        Ok(Some(value))
    }
}
/// Same BUILD refusal and update/rejoin path as an engine upgrade mismatch.
pub fn check(
    required: Option<&ModIdentity>,
    installed: Option<&ModIdentity>,
) -> Result<(), Refused> {
    if required == installed {
        return Ok(());
    }
    let label = |identity: Option<&ModIdentity>| {
        identity
            .map(|m| format!("{} {}", m.id, m.version))
            .unwrap_or_else(|| "plain D2Gamma".into())
    };
    Err(Refused::with_text(
        ErrorCode::BUILD,
        format!(
            "Server requires {}. Update through the launcher and rejoin.",
            label(required)
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tail_is_absent_by_default_bounded_versioned_and_requires_exact_identity() {
        let mut w = Writer::new(1);
        ModIdentity::write_tail(None, &mut w);
        assert_eq!(w.finish(), [1]);
        let identity = ModIdentity {
            id: "test-mod".into(),
            version: "1.2.3".into(),
        };
        let mut w = Writer::new(1);
        ModIdentity::write_tail(Some(&identity), &mut w);
        let bytes = w.finish();
        let (_, mut r) = Reader::new(&bytes).unwrap();
        assert_eq!(
            ModIdentity::read_tail(&mut r).unwrap(),
            Some(identity.clone())
        );
        r.finish().unwrap();
        for n in 2..bytes.len() {
            let (_, mut r) = Reader::new(&bytes[..n]).unwrap();
            assert!(ModIdentity::read_tail(&mut r).is_err());
        }
        let mut bad = bytes.clone();
        bad[2] = 2;
        let (_, mut r) = Reader::new(&bad).unwrap();
        assert!(ModIdentity::read_tail(&mut r).is_err());
        assert!(check(None, None).is_ok());
        assert!(check(Some(&identity), Some(&identity)).is_ok());
        let mismatch = check(Some(&identity), None).unwrap_err();
        assert_eq!(mismatch.code, ErrorCode::BUILD);
        assert!(mismatch.text.contains("test-mod 1.2.3"));
        assert!(check(None, Some(&identity)).is_err());
    }
}
