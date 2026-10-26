//! `dev = test`: dev commands on the game server's standard input
//! (contracts 10.1), for the test setups.
//!
//! One line names the character and the command: `dev CHARACTER COMMAND`;
//! it runs in the game that character plays in here. The answer is one
//! line, `dev ok TEXT` or `dev err TEXT`. A character's progression is the
//! main server's (`set-progression` there). A game server that doesn't run
//! `dev = test` doesn't read its standard input.

use std::io::{BufRead, Write};

use d2_proto::dev::{self, DevCommand};

/// The answer to one line; `None` when the line is blank.
pub fn answer(
    line: &str,
    run: impl FnOnce(&str, &DevCommand) -> Result<String, String>,
) -> Option<String> {
    let mut words = line.split_whitespace();
    let first = words.next()?;
    if first != "dev" {
        return Some("dev err expected `dev CHARACTER COMMAND`".into());
    }
    let (Some(character), command) = (words.next(), words.collect::<Vec<_>>().join(" ")) else {
        return Some("dev err usage: dev CHARACTER COMMAND".into());
    };
    if command.is_empty() {
        return Some("dev err usage: dev CHARACTER COMMAND".into());
    }
    if command
        .split_whitespace()
        .next()
        .is_some_and(|k| k.eq_ignore_ascii_case("set-progression"))
    {
        return Some("dev err the main server keeps progression: set it there".into());
    }
    let reply = match dev::parse(&command) {
        Ok(Some(cmd)) => run(character, &cmd),
        Ok(None) => Err(format!("{command} is a client-only command")),
        Err(e) => Err(e.to_string()),
    };
    Some(match reply {
        Ok(text) => format!("dev ok {text}"),
        Err(text) => format!("dev err {text}"),
    })
}

/// Reads standard input and answers each line on standard output, until
/// it ends.
pub fn serve(run: impl Fn(&str, &DevCommand) -> Result<String, String>) {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let Some(reply) = answer(&line, &run) else {
            continue;
        };
        if writeln!(out, "{reply}").and_then(|()| out.flush()).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_runs_in_the_characters_game() {
        assert_eq!(answer("  ", |_, _| unreachable!()), None);
        let line = answer("dev Hero heal", |character, cmd| {
            assert_eq!((character, cmd), ("Hero", &DevCommand::Heal));
            Ok("healed".into())
        });
        assert_eq!(line.as_deref(), Some("dev ok healed"));
        let line = answer("dev Hero gold 5", |_, _| Err("not here".into()));
        assert_eq!(line.as_deref(), Some("dev err not here"));
        assert!(answer("dev Hero", |_, _| unreachable!())
            .is_some_and(|l| l.starts_with("dev err usage")));
        assert!(answer("gold 5", |_, _| unreachable!())
            .is_some_and(|l| l.starts_with("dev err expected")));
        assert!(answer("dev Hero set-progression 5", |_, _| unreachable!())
            .is_some_and(|l| l.contains("main server")));
    }
}
