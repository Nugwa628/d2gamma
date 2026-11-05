//! Mod versions and the version ranges a mod names for the mod it builds on.
//!
//! A version is `MAJOR.MINOR.PATCH`, optionally followed by `-pre` (letters,
//! digits and dots). A range is a comma-separated list of comparisons that
//! must all hold: `>=1.2.0, <2.0.0`. A comparison is an operator and a full
//! version; the operators are `=`, `>`, `>=`, `<`, `<=` and `^` ("compatible":
//! the same left-most non-zero part). A bare version means `=`.
//!
//! Ordering follows semantic versioning: a pre-release sorts before the
//! release it leads to, and pre-release parts compare number-wise when both
//! are numbers.

use std::cmp::Ordering;
use std::fmt;

/// A semantic version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// The text after `-`, empty for a release.
    pub pre: String,
}

impl Version {
    /// A release version.
    pub fn new(major: u64, minor: u64, patch: u64) -> Version {
        Version {
            major,
            minor,
            patch,
            pre: String::new(),
        }
    }

    /// Parses `1.2.3` or `1.2.3-beta.1`. The error says what is wrong.
    pub fn parse(text: &str) -> Result<Version, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("is empty".into());
        }
        let (core, pre) = match text.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (text, None),
        };
        if let Some(pre) = pre {
            let ok = !pre.is_empty()
                && pre
                    .split('.')
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric()));
            if !ok {
                return Err(format!(
                    "`{text}` has a bad pre-release part (use letters, digits and dots, e.g. 1.0.0-beta.1)"
                ));
            }
        }
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() != 3 {
            return Err(format!(
                "`{text}` is not a version: write three numbers like 1.2.3"
            ));
        }
        let mut n = [0u64; 3];
        for (slot, part) in n.iter_mut().zip(&parts) {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(format!(
                    "`{text}` is not a version: `{part}` is not a number"
                ));
            }
            if part.len() > 1 && part.starts_with('0') {
                return Err(format!(
                    "`{text}` is not a version: `{part}` has a leading zero"
                ));
            }
            *slot = part
                .parse()
                .map_err(|_| format!("`{text}` is not a version: `{part}` is too large"))?;
        }
        Ok(Version {
            major: n[0],
            minor: n[1],
            patch: n[2],
            pre: pre.unwrap_or("").to_owned(),
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre)?;
        }
        Ok(())
    }
}

fn cmp_pre(a: &str, b: &str) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    let mut ai = a.split('.');
    let mut bi = b.split('.');
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let num = |s: &str| {
                    s.bytes()
                        .all(|b| b.is_ascii_digit())
                        .then(|| s.parse::<u64>().ok())
                        .flatten()
                };
                let o = match (num(x), num(y)) {
                    (Some(p), Some(q)) => p.cmp(&q),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => x.cmp(y),
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Version) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| cmp_pre(&self.pre, &other.pre))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Gt,
    Ge,
    Lt,
    Le,
    Caret,
}

/// A version range: every comparison must hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionReq {
    text: String,
    parts: Vec<(Op, Version)>,
}

impl VersionReq {
    /// Parses a range such as `>=1.2.0, <2.0.0` or `^1.4.0`.
    pub fn parse(text: &str) -> Result<VersionReq, String> {
        if text.trim().is_empty() {
            return Err("is empty (write a range like \">=1.0.0, <2.0.0\" or \"^1.0.0\")".into());
        }
        let mut parts = Vec::new();
        for raw in text.split(',') {
            let raw = raw.trim();
            if raw.is_empty() {
                return Err(format!("`{}` has an empty comparison", text.trim()));
            }
            let (op, rest) = if let Some(r) = raw.strip_prefix(">=") {
                (Op::Ge, r)
            } else if let Some(r) = raw.strip_prefix("<=") {
                (Op::Le, r)
            } else if let Some(r) = raw.strip_prefix('>') {
                (Op::Gt, r)
            } else if let Some(r) = raw.strip_prefix('<') {
                (Op::Lt, r)
            } else if let Some(r) = raw.strip_prefix('^') {
                (Op::Caret, r)
            } else if let Some(r) = raw.strip_prefix('=') {
                (Op::Eq, r)
            } else {
                (Op::Eq, raw)
            };
            let v = Version::parse(rest.trim()).map_err(|e| format!("comparison `{raw}`: {e}"))?;
            parts.push((op, v));
        }
        Ok(VersionReq {
            text: text.trim().to_owned(),
            parts,
        })
    }

    /// Whether `v` satisfies every comparison.
    pub fn matches(&self, v: &Version) -> bool {
        self.parts.iter().all(|(op, r)| match op {
            Op::Eq => v == r,
            Op::Gt => v > r,
            Op::Ge => v >= r,
            Op::Lt => v < r,
            Op::Le => v <= r,
            Op::Caret => v >= r && *v < caret_ceiling(r),
        })
    }

    /// The range as written.
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl fmt::Display for VersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// The first version a `^r` range excludes: the left-most non-zero part
/// goes up by one.
fn caret_ceiling(r: &Version) -> Version {
    if r.major > 0 {
        Version::new(r.major + 1, 0, 0)
    } else if r.minor > 0 {
        Version::new(0, r.minor + 1, 0)
    } else {
        Version::new(0, 0, r.patch + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn parses_and_prints() {
        assert_eq!(v("1.2.3"), Version::new(1, 2, 3));
        assert_eq!(v("0.10.0-beta.1").to_string(), "0.10.0-beta.1");
    }

    #[test]
    fn rejects_bad_versions() {
        for bad in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "1.x.3",
            "01.2.3",
            "1.2.3-",
            "1.2.3-a..b",
            "v1.2.3",
        ] {
            assert!(Version::parse(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn orders_like_semver() {
        let order = [
            "0.9.9",
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.2",
            "1.0.0-alpha.10",
            "1.0.0-beta",
            "1.0.0",
            "1.0.1",
            "1.10.0",
            "2.0.0",
        ];
        for w in order.windows(2) {
            assert!(v(w[0]) < v(w[1]), "{} < {}", w[0], w[1]);
        }
    }

    #[test]
    fn ranges_match() {
        let r = VersionReq::parse(">=1.2.0, <2.0.0").unwrap();
        assert!(r.matches(&v("1.2.0")));
        assert!(r.matches(&v("1.9.9")));
        assert!(!r.matches(&v("1.1.9")));
        assert!(!r.matches(&v("2.0.0")));
        let c = VersionReq::parse("^1.4.0").unwrap();
        assert!(c.matches(&v("1.4.0")) && c.matches(&v("1.99.0")));
        assert!(!c.matches(&v("2.0.0")) && !c.matches(&v("1.3.9")));
        let z = VersionReq::parse("^0.3.1").unwrap();
        assert!(z.matches(&v("0.3.9")) && !z.matches(&v("0.4.0")));
        let e = VersionReq::parse("1.0.0").unwrap();
        assert!(e.matches(&v("1.0.0")) && !e.matches(&v("1.0.1")));
    }

    #[test]
    fn rejects_bad_ranges() {
        for bad in ["", ">=", "1.0", ">=1.0.0,", "~1.0.0", ">=x.0.0"] {
            assert!(VersionReq::parse(bad).is_err(), "{bad:?} should fail");
        }
    }
}
