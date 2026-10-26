//! The ready line: once a program serves, it prints one line
//! `ready key=value key=value ...` on standard output (contracts 10.1 and
//! 11.10). Test tools and `server-ctl` wait for it and read its fields.

use std::fmt::Display;
use std::io::Write;

/// Writes `line` and a line end on standard output, and flushes it. A
/// write error is ignored: the reader (a test tool, the client that
/// started a hosting process) may have gone, and a closed pipe must not
/// stop the program while it saves and closes.
pub fn print_line(line: impl Display) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// A ready line, built field by field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReadyLine {
    fields: Vec<(&'static str, String)>,
}

impl ReadyLine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `key=value`. Values are written as they are: they hold no
    /// spaces (addresses, hex, names without spaces, comma lists).
    pub fn field(mut self, key: &'static str, value: impl Display) -> Self {
        self.fields.push((key, value.to_string()));
        self
    }

    /// Prints the line on standard output ([`print_line`]).
    pub fn print(&self) {
        print_line(self);
    }
}

impl Display for ReadyLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ready")?;
        for (k, v) in &self.fields {
            write!(f, " {k}={v}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_follow_the_word_ready_in_order() {
        let line = ReadyLine::new()
            .field("listen", "127.0.0.1:6112")
            .field("protocol", 7)
            .field("caps", ["connect", "lobby"].join(","));
        assert_eq!(
            line.to_string(),
            "ready listen=127.0.0.1:6112 protocol=7 caps=connect,lobby"
        );
        assert_eq!(ReadyLine::new().to_string(), "ready");
    }
}
