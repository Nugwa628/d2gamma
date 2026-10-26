//! The config file reader every server program uses.
//!
//! A config file has one `key = value` per line. Blank lines and lines
//! that start with `#` are skipped. A relative path is taken from the
//! config file's folder, so a server folder can be moved as a whole. Each
//! program knows its own keys: it walks the [`entries`] and refuses a key
//! it doesn't know ([`Entry::unknown`]) or one given twice ([`set_once`]).
//!
//! ```text
//! # a comment
//! listen    = 0.0.0.0:6112
//! game_data = C:/Games/D2RUST/game
//! ```

use std::fmt::Display;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// A config file that can't be used.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// The file couldn't be read.
    #[error("reading {path}: {reason}")]
    Read {
        /// The file.
        path: PathBuf,
        /// Why.
        reason: String,
    },
    /// A line is not `key = value`.
    #[error("line {line}: expected `key = value`")]
    Syntax {
        /// The line number, from 1.
        line: usize,
    },
    /// A key the program doesn't know.
    #[error("line {line}: unknown key `{key}`")]
    UnknownKey {
        /// The line number, from 1.
        line: usize,
        /// The key.
        key: String,
    },
    /// A key given twice.
    #[error("line {line}: `{key}` is already set")]
    Repeated {
        /// The line number, from 1.
        line: usize,
        /// The key.
        key: String,
    },
    /// A value that doesn't fit its key.
    #[error("line {line}: bad `{key}`: {reason}")]
    Value {
        /// The line number, from 1.
        line: usize,
        /// The key.
        key: String,
        /// Why.
        reason: String,
    },
    /// `game_data` is missing.
    #[error("`game_data` is required: the server reads the rules from the game folder")]
    NoGameData,
    /// Another required key is missing.
    #[error("`{key}` is required: {why}")]
    Missing {
        /// The key.
        key: &'static str,
        /// What it is for.
        why: &'static str,
    },
}

/// Reads the config file at `path`.
pub fn read(path: &Path) -> Result<String, ConfigError> {
    std::fs::read_to_string(path).map_err(|e| ConfigError::Read {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })
}

/// The folder relative paths of the config file at `path` are taken from.
pub fn folder(path: &Path) -> &Path {
    path.parent().unwrap_or(Path::new(""))
}

/// One `key = value` line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    /// The line number, from 1.
    pub line: usize,
    pub key: &'a str,
    pub value: &'a str,
}

impl Entry<'_> {
    /// The value doesn't fit the key, because of `reason`.
    pub fn bad(&self, reason: impl Into<String>) -> ConfigError {
        ConfigError::Value {
            line: self.line,
            key: self.key.into(),
            reason: reason.into(),
        }
    }

    /// The key was given before.
    pub fn repeated(&self) -> ConfigError {
        ConfigError::Repeated {
            line: self.line,
            key: self.key.into(),
        }
    }

    /// The program doesn't know the key.
    pub fn unknown(&self) -> ConfigError {
        ConfigError::UnknownKey {
            line: self.line,
            key: self.key.into(),
        }
    }

    /// The value as a `T`.
    pub fn parse<T: FromStr>(&self) -> Result<T, ConfigError>
    where
        T::Err: Display,
    {
        self.value
            .parse()
            .map_err(|e: T::Err| self.bad(e.to_string()))
    }

    /// The value as a number in `range`.
    pub fn number<T>(&self, range: RangeInclusive<T>) -> Result<T, ConfigError>
    where
        T: FromStr + PartialOrd + Display,
        T::Err: Display,
    {
        let v: T = self.parse()?;
        if !range.contains(&v) {
            return Err(self.bad(format!("must be {} to {}", range.start(), range.end())));
        }
        Ok(v)
    }

    /// The value as a path, taken from `base` when relative.
    pub fn path(&self, base: &Path) -> PathBuf {
        base.join(self.value)
    }
}

/// The `key = value` lines of `text`, in order, without blank lines and
/// comments. A line that isn't `key = value` (or has an empty side) is
/// [`ConfigError::Syntax`].
pub fn entries(text: &str) -> impl Iterator<Item = Result<Entry<'_>, ConfigError>> {
    text.lines().enumerate().filter_map(|(i, raw)| {
        let line = i + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }
        Some(
            trimmed
                .split_once('=')
                .map(|(k, v)| (k.trim(), v.trim()))
                .filter(|(k, v)| !k.is_empty() && !v.is_empty())
                .map(|(key, value)| Entry { line, key, value })
                .ok_or(ConfigError::Syntax { line }),
        )
    })
}

/// Sets `slot` to `v` for `entry`; [`ConfigError::Repeated`] if the key
/// was set before.
pub fn set_once<T>(slot: &mut Option<T>, v: T, entry: &Entry<'_>) -> Result<(), ConfigError> {
    if slot.is_some() {
        return Err(entry.repeated());
    }
    *slot = Some(v);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_skip_blanks_and_comments_and_number_the_lines() {
        let text = "# head\n\n a = 1 \nb=two words\n";
        let got: Vec<_> = entries(text).collect::<Result<_, _>>().unwrap();
        assert_eq!(
            got,
            [
                Entry {
                    line: 3,
                    key: "a",
                    value: "1"
                },
                Entry {
                    line: 4,
                    key: "b",
                    value: "two words"
                },
            ]
        );
    }

    #[test]
    fn a_line_without_both_sides_is_a_syntax_error() {
        for text in ["a\n", "a =\n", "= 1\n"] {
            let first = entries(text).next().unwrap();
            assert_eq!(first, Err(ConfigError::Syntax { line: 1 }), "{text:?}");
        }
    }

    #[test]
    fn numbers_are_checked_against_their_range() {
        let e = Entry {
            line: 7,
            key: "max_games",
            value: "0",
        };
        assert_eq!(
            e.number(1..=1024usize),
            Err(ConfigError::Value {
                line: 7,
                key: "max_games".into(),
                reason: "must be 1 to 1024".into()
            })
        );
        let e = Entry { value: "12", ..e };
        assert_eq!(e.number(1..=1024usize), Ok(12));
        let e = Entry { value: "x", ..e };
        assert!(matches!(
            e.number(1..=1024usize),
            Err(ConfigError::Value { .. })
        ));
    }

    #[test]
    fn a_key_is_set_once() {
        let e = Entry {
            line: 2,
            key: "listen",
            value: "x",
        };
        let mut slot = None;
        assert_eq!(set_once(&mut slot, 1, &e), Ok(()));
        assert_eq!(
            set_once(&mut slot, 2, &e),
            Err(ConfigError::Repeated {
                line: 2,
                key: "listen".into()
            })
        );
        assert_eq!(slot, Some(1));
    }
}
