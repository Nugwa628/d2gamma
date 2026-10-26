//! The chat's slash commands (lobby contract section 3.3): `/w NAME TEXT`
//! and `/whisper` (DM), `/r TEXT` (reply), `/g TEXT` (Guild), `/global TEXT`,
//! `/m TEXT` (Mod), `/me TEXT` (emote in the open tab), `/ignore NAME`,
//! `/unignore NAME`, `/f add NAME`, `/f remove NAME`. Anything else that
//! starts with `/` is an unknown command: it stays local and is never sent.
//! A line without a command goes to the open tab.

use super::source::{FriendOp, Room};

/// A typed line, parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// A plain line for the open tab.
    Plain(String),
    /// `/me`: an emote in the open tab.
    Emote(String),
    /// `/g`, `/global`, `/m`: a line for that room.
    Say(Room, String),
    /// `/w`, `/whisper`: a DM to a character name or `*account`.
    Whisper { to: String, text: String },
    /// `/r`: a DM to the last account that whispered.
    Reply(String),
    /// `/f add`, `/f remove`, `/ignore`, `/unignore`.
    Friend(FriendOp, String),
    /// A known command without what it needs: the usage line.
    Usage(&'static str),
    /// An unknown command (the word typed, with its `/`).
    Unknown(String),
    /// Nothing to send (empty or only spaces).
    Empty,
}

/// The longest chat text in bytes (`MAX_CHAT`).
pub const MAX_TEXT: usize = 200;
/// The longest character name in bytes (`MAX_FROM`).
pub const MAX_NAME: usize = 32;

/// `text` trimmed and cut to [`MAX_TEXT`] bytes at a character boundary.
pub fn clean(text: &str) -> String {
    let t = text.trim();
    let mut end = t.len().min(MAX_TEXT);
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    t[..end].trim_end().to_string()
}

/// The first word of `s` and the rest (trimmed).
fn word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(i) => (&s[..i], s[i..].trim()),
        None => (s, ""),
    }
}

/// Parses a typed line.
pub fn parse(line: &str) -> Command {
    let line = line.trim();
    if line.is_empty() {
        return Command::Empty;
    }
    let Some(body) = line.strip_prefix('/') else {
        return Command::Plain(clean(line));
    };
    let (cmd, rest) = word(body);
    let text = || clean(rest);
    let needs = |usage: &'static str, c: Command| {
        if rest.is_empty() {
            Command::Usage(usage)
        } else {
            c
        }
    };
    match cmd.to_ascii_lowercase().as_str() {
        "w" | "whisper" => {
            let (to, text) = word(rest);
            if to.is_empty() || text.is_empty() || to.len() > MAX_NAME + 1 {
                return Command::Usage("/w NAME TEXT");
            }
            Command::Whisper {
                to: to.to_string(),
                text: clean(text),
            }
        }
        "r" => needs("/r TEXT", Command::Reply(text())),
        "g" => needs("/g TEXT", Command::Say(Room::Guild, text())),
        "global" => needs("/global TEXT", Command::Say(Room::Global, text())),
        "m" => needs("/m TEXT", Command::Say(Room::Mod, text())),
        "me" => needs("/me TEXT", Command::Emote(text())),
        "ignore" | "unignore" => {
            let (name, _) = word(rest);
            let op = if cmd.eq_ignore_ascii_case("ignore") {
                FriendOp::Ignore
            } else {
                FriendOp::Unignore
            };
            if name.is_empty() {
                return Command::Usage(if op == FriendOp::Ignore {
                    "/ignore NAME"
                } else {
                    "/unignore NAME"
                });
            }
            Command::Friend(op, name.to_string())
        }
        "f" => {
            let (sub, rest) = word(rest);
            let (name, _) = word(rest);
            let op = match sub.to_ascii_lowercase().as_str() {
                "add" => FriendOp::Add,
                "remove" => FriendOp::Remove,
                _ => return Command::Usage("/f add NAME, /f remove NAME"),
            };
            if name.is_empty() {
                return Command::Usage("/f add NAME, /f remove NAME");
            }
            Command::Friend(op, name.to_string())
        }
        _ => Command::Unknown(format!("/{cmd}")),
    }
}
