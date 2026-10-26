//! `dev = test`: the main server's lines on standard input (contracts
//! 10.1).
//!
//! The main server keeps the characters, so it takes the line that changes
//! a stored character: `set-progression CHARACTER N` (or
//! `dev CHARACTER set-progression N`), while the character is out of a
//! game. Every other dev command runs in a game, on a game server: the main
//! server answers it `dev err` (the test tool sends those lines to the game
//! servers). The answer is one line, `dev ok TEXT` or `dev err TEXT`. A main
//! server without `dev = test` doesn't read its standard input.
//!
//! The market's test lines (contract section 9):
//!
//! - `econ-seed ACCOUNT gold:N|CODE:N [SPLIT]`: gold or a stackable count
//!   into the account's online stash out of nothing, in the economy
//!   `SPLIT` (`sc-l0-exp`, the default; `hc-l0-cls`, ...). Items come from
//!   a game (`dev CHARACTER item ...`, then a stash put).
//! - `econ-clock +N[s|m|h|d]`: the market's clock moves forward, then the
//!   expiry sweep runs.
//!
//! The ladder's lines (lobby contract sections 6 and 11):
//!
//! - `season-start NAME [test]`: opens a season named `NAME` (it may have
//!   spaces; a last word `test` makes it a test season), ending the open
//!   one; `season-end` ends the open season (its final ranks stay; its
//!   characters become non-ladder when next loaded).
//! - `ladder-seed CHARACTER LEVEL EXPERIENCE [dead]`: the character's
//!   ladder row and summary as a save would write them (`dead`: a hardcore
//!   character that died).
//!
//! The chat's lines (lobby contract sections 3.1, 9 and 11):
//!
//! - `notice TEXT`: a realm notice in the Global room;
//! - `mute ACCOUNT MINUTES [REASON]` and `unmute ACCOUNT`;
//! - `social-clock +N[s|m|h|d]`: the DM and invite clock moves forward,
//!   then the sweeps run.
//!
//! The upgrade's lines (`docs/plans/server/upgrades.md`):
//!
//! - `upgrade VERSION [MINUTES [MINUTE_MS]]`: the countdown to a restart
//!   for `VERSION` (10 minutes when not given; 0: at once), with minutes
//!   of `MINUTE_MS` (a test hears the whole countdown in seconds);
//! - `upgrade-cancel`: the countdown is called off.
//!
//! `stats`: what the main server holds, on one line of `key=value` words
//! (the load and soak tests read it): its sessions, the chat's rooms and
//! limits, and the accounts database's use (lobby plan L17).

use std::io::{BufRead, Write};

use d2_economy::Goods;

/// One request: a character's stored progression bits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetProgression {
    /// The character whose stored progression changes.
    pub character: String,
    /// Progression bits, 0 to 31.
    pub progression: u8,
}

/// `econ-seed`: goods into an account's online stash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EconSeed {
    pub account: String,
    pub goods: Goods,
    /// The economy (`sc-l0-exp`).
    pub split: String,
}

/// One line's request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    SetProgression(SetProgression),
    EconSeed(EconSeed),
    /// `econ-clock`: the market's clock forward by `ms`.
    EconClock {
        ms: i64,
    },
    /// `season-start`: a new ladder season.
    SeasonStart {
        name: String,
        test: bool,
    },
    /// `season-end`: the open season ends.
    SeasonEnd,
    LadderSeed(LadderSeed),
    /// `notice TEXT`.
    Notice {
        text: String,
    },
    /// `mute ACCOUNT MINUTES [REASON]`.
    Mute {
        account: String,
        minutes: u32,
        reason: String,
    },
    /// `unmute ACCOUNT`.
    Unmute {
        account: String,
    },
    /// `social-clock`: the social clock forward by `ms`.
    SocialClock {
        ms: i64,
    },
    /// `stats`.
    Stats,
    /// `upgrade VERSION [MINUTES [MINUTE_MS]]`.
    Upgrade(crate::upgrade::Request),
    /// `upgrade-cancel`.
    UpgradeCancel,
}

/// `ladder-seed`: a character's ladder row as a save would write it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LadderSeed {
    pub character: String,
    pub level: u8,
    pub experience: u64,
    /// A hardcore character that died.
    pub dead: bool,
}

/// Splits one line; a blank line is not a request.
pub fn parse(line: &str) -> Result<Option<Line>, String> {
    let mut toks = line.split_whitespace();
    let Some(first) = toks.next() else {
        return Ok(None);
    };
    if first.eq_ignore_ascii_case("econ-seed") {
        return econ_seed(toks).map(|s| Some(Line::EconSeed(s)));
    }
    if first.eq_ignore_ascii_case("season-start") {
        let mut words: Vec<&str> = toks.collect();
        let test = words.len() > 1 && words.last().is_some_and(|w| w.eq_ignore_ascii_case("test"));
        if test {
            words.pop();
        }
        if words.is_empty() {
            return Err("usage: season-start NAME [test]".into());
        }
        return Ok(Some(Line::SeasonStart {
            name: words.join(" "),
            test,
        }));
    }
    if first.eq_ignore_ascii_case("season-end") {
        if toks.next().is_some() {
            return Err("usage: season-end".into());
        }
        return Ok(Some(Line::SeasonEnd));
    }
    if first.eq_ignore_ascii_case("ladder-seed") {
        return ladder_seed(toks).map(|s| Some(Line::LadderSeed(s)));
    }
    if first.eq_ignore_ascii_case("econ-clock") {
        let (Some(by), None) = (toks.next(), toks.next()) else {
            return Err("usage: econ-clock +N[s|m|h|d]".into());
        };
        return clock_step("econ-clock", by).map(|ms| Some(Line::EconClock { ms }));
    }
    if first.eq_ignore_ascii_case("social-clock") {
        let (Some(by), None) = (toks.next(), toks.next()) else {
            return Err("usage: social-clock +N[s|m|h|d]".into());
        };
        return clock_step("social-clock", by).map(|ms| Some(Line::SocialClock { ms }));
    }
    if first.eq_ignore_ascii_case("stats") {
        if toks.next().is_some() {
            return Err("usage: stats".into());
        }
        return Ok(Some(Line::Stats));
    }
    if first.eq_ignore_ascii_case("upgrade") {
        let usage = || "usage: upgrade VERSION [MINUTES [MINUTE_MS]]".to_owned();
        let version = toks.next().ok_or_else(usage)?;
        let number = |t: Option<&str>, default: u64| match t {
            None => Ok(default),
            Some(t) => t.parse::<u64>().map_err(|_| usage()),
        };
        let minutes = number(toks.next(), u64::from(crate::upgrade::DEFAULT_MINUTES))?;
        let minute_ms = number(toks.next(), crate::upgrade::MINUTE_MS)?;
        if toks.next().is_some() {
            return Err(usage());
        }
        let minutes = u32::try_from(minutes).map_err(|_| usage())?;
        return crate::upgrade::Request::new(version, minutes, minute_ms)
            .map(|r| Some(Line::Upgrade(r)));
    }
    if first.eq_ignore_ascii_case("upgrade-cancel") {
        if toks.next().is_some() {
            return Err("usage: upgrade-cancel".into());
        }
        return Ok(Some(Line::UpgradeCancel));
    }
    if first.eq_ignore_ascii_case("notice") {
        let text = line.trim_start()[first.len()..].trim();
        if text.is_empty() {
            return Err("usage: notice TEXT".into());
        }
        return Ok(Some(Line::Notice { text: text.into() }));
    }
    if first.eq_ignore_ascii_case("mute") {
        let usage = || "usage: mute ACCOUNT MINUTES [REASON]".to_owned();
        let (Some(account), Some(minutes)) = (toks.next(), toks.next()) else {
            return Err(usage());
        };
        let minutes = minutes
            .parse::<u32>()
            .ok()
            .filter(|m| (1..=525_600).contains(m))
            .ok_or_else(|| format!("mute: {minutes:?} is not minutes in 1..=525600"))?;
        let reason = toks.collect::<Vec<_>>().join(" ");
        return Ok(Some(Line::Mute {
            account: account.into(),
            minutes,
            reason,
        }));
    }
    if first.eq_ignore_ascii_case("unmute") {
        let (Some(account), None) = (toks.next(), toks.next()) else {
            return Err("usage: unmute ACCOUNT".into());
        };
        return Ok(Some(Line::Unmute {
            account: account.into(),
        }));
    }
    let character = if first.eq_ignore_ascii_case("set-progression") {
        toks.next().ok_or_else(usage)?
    } else if first == "dev" {
        let character = toks.next().ok_or("usage: dev CHARACTER COMMAND")?;
        match toks.next() {
            Some(k) if k.eq_ignore_ascii_case("set-progression") => character,
            Some(_) => {
                return Err("game commands run on the game server of the character's game".into())
            }
            None => return Err("usage: dev CHARACTER COMMAND".into()),
        }
    } else {
        return Err("expected `set-progression CHARACTER N`".into());
    };
    let n = toks.next().ok_or_else(usage)?;
    if toks.next().is_some() {
        return Err(usage());
    }
    let progression = n
        .parse::<u8>()
        .ok()
        .filter(|n| *n <= 0x1F)
        .ok_or_else(|| format!("set-progression: {n:?} is not a number in 0..=31"))?;
    Ok(Some(Line::SetProgression(SetProgression {
        character: character.to_owned(),
        progression,
    })))
}

fn econ_seed<'a>(mut toks: impl Iterator<Item = &'a str>) -> Result<EconSeed, String> {
    let usage = || "usage: econ-seed ACCOUNT gold:N|CODE:N [SPLIT]".to_owned();
    let (Some(account), Some(goods)) = (toks.next(), toks.next()) else {
        return Err(usage());
    };
    let split = toks.next().unwrap_or("sc-l0-exp");
    if toks.next().is_some() {
        return Err(usage());
    }
    let (what, n) = goods.split_once(':').ok_or_else(usage)?;
    let n: u64 = n
        .parse()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("econ-seed: {n:?} is not a count above 0"))?;
    let goods = if what.eq_ignore_ascii_case("gold") {
        Goods::Gold(n)
    } else {
        let count = u32::try_from(n).map_err(|_| format!("econ-seed: {n} is too many"))?;
        Goods::Stack {
            code: what.to_owned(),
            count,
        }
    };
    Ok(EconSeed {
        account: account.to_owned(),
        goods,
        split: split.to_owned(),
    })
}

fn ladder_seed<'a>(mut toks: impl Iterator<Item = &'a str>) -> Result<LadderSeed, String> {
    let usage = || "usage: ladder-seed CHARACTER LEVEL EXPERIENCE [dead]".to_owned();
    let (Some(character), Some(level), Some(experience)) = (toks.next(), toks.next(), toks.next())
    else {
        return Err(usage());
    };
    let dead = match toks.next() {
        None => false,
        Some(w) if w.eq_ignore_ascii_case("dead") => true,
        Some(_) => return Err(usage()),
    };
    if toks.next().is_some() {
        return Err(usage());
    }
    let level = level
        .parse::<u8>()
        .ok()
        .filter(|l| (1..=99).contains(l))
        .ok_or_else(|| format!("ladder-seed: {level:?} is not a level in 1..=99"))?;
    let experience = experience
        .parse::<u32>()
        .map_err(|_| format!("ladder-seed: {experience:?} is not an experience (32 bits)"))?;
    Ok(LadderSeed {
        character: character.to_owned(),
        level,
        experience: u64::from(experience),
        dead,
    })
}

/// `+N` with a unit (`s`, `m`, `h`, `d`) in milliseconds, for line
/// `line`.
fn clock_step(line: &str, by: &str) -> Result<i64, String> {
    let bad = || format!("{line}: {by:?} is not +N[s|m|h|d]");
    let n = by.strip_prefix('+').ok_or_else(bad)?;
    let (n, unit) = match n.char_indices().last() {
        Some((i, c)) if c.is_ascii_alphabetic() => (&n[..i], c),
        _ => (n, 's'),
    };
    let unit_ms: i64 = match unit {
        's' => 1_000,
        'm' => 60_000,
        'h' => 3_600_000,
        'd' => 86_400_000,
        _ => return Err(bad()),
    };
    n.parse::<i64>()
        .ok()
        .filter(|n| (1..=3650 * 86_400).contains(n))
        .and_then(|n| n.checked_mul(unit_ms))
        .ok_or_else(bad)
}

/// The whole line's usage.
fn usage() -> String {
    "usage: set-progression CHARACTER N (N 0..=31)".into()
}

/// The answer to one line; `None` when the line is blank.
pub fn answer(line: &str, run: impl FnOnce(&Line) -> Result<String, String>) -> Option<String> {
    match parse(line) {
        Ok(None) => None,
        Ok(Some(req)) => Some(match run(&req) {
            Ok(text) => format!("dev ok {text}"),
            Err(text) => format!("dev err {text}"),
        }),
        Err(e) => Some(format!("dev err {e}")),
    }
}

/// Reads standard input and answers each line on standard output, until
/// it ends.
pub fn serve(run: impl Fn(&Line) -> Result<String, String>) {
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
    fn the_progression_line_has_both_forms() {
        let want = Some(Line::SetProgression(SetProgression {
            character: "Frankpal".into(),
            progression: 5,
        }));
        assert_eq!(parse("set-progression Frankpal 5"), Ok(want.clone()));
        assert_eq!(parse("  dev  Frankpal  SET-PROGRESSION  5 "), Ok(want));
        assert_eq!(parse("   "), Ok(None));
        for bad in [
            "set-progression",
            "set-progression Frankpal",
            "set-progression Frankpal 32",
            "set-progression Frankpal x",
            "set-progression Frankpal 5 6",
            "dev Frankpal",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn game_commands_belong_to_the_game_servers() {
        let e = parse("dev Frankpal spawn fallen").unwrap_err();
        assert!(e.contains("game server"), "{e}");
        assert!(parse("gold 5").unwrap_err().contains("expected"));
    }

    #[test]
    fn a_line_gets_one_answer() {
        let ok = answer("set-progression Frankpal 5", |r| match r {
            Line::SetProgression(r) => Ok(format!(
                "{} progression set to {}",
                r.character, r.progression
            )),
            other => panic!("{other:?}"),
        });
        assert_eq!(ok.as_deref(), Some("dev ok Frankpal progression set to 5"));
        let busy = answer("set-progression Frankpal 5", |_| Err("in a game".into()));
        assert_eq!(busy.as_deref(), Some("dev err in a game"));
        assert_eq!(answer("", |_| unreachable!()), None);
        let bad = answer("dev Frankpal god", |_| unreachable!()).unwrap();
        assert!(bad.starts_with("dev err "), "{bad}");
    }

    #[test]
    fn the_ladder_lines_read() {
        assert_eq!(
            parse("season-start Season 1"),
            Ok(Some(Line::SeasonStart {
                name: "Season 1".into(),
                test: false
            }))
        );
        assert_eq!(
            parse("season-start Test season test"),
            Ok(Some(Line::SeasonStart {
                name: "Test season".into(),
                test: true
            }))
        );
        assert_eq!(
            parse("season-start test"),
            Ok(Some(Line::SeasonStart {
                name: "test".into(),
                test: false
            })),
            "a name alone is the name"
        );
        assert_eq!(parse("season-end"), Ok(Some(Line::SeasonEnd)));
        assert_eq!(
            parse("ladder-seed Kaern 40 1500000"),
            Ok(Some(Line::LadderSeed(LadderSeed {
                character: "Kaern".into(),
                level: 40,
                experience: 1_500_000,
                dead: false
            })))
        );
        assert!(matches!(
            parse("ladder-seed Kaern 40 1500000 dead"),
            Ok(Some(Line::LadderSeed(LadderSeed { dead: true, .. })))
        ));
        for bad in [
            "season-start",
            "season-end now",
            "ladder-seed",
            "ladder-seed Kaern 40",
            "ladder-seed Kaern 0 5",
            "ladder-seed Kaern 100 5",
            "ladder-seed Kaern 40 -1",
            "ladder-seed Kaern 40 4294967296",
            "ladder-seed Kaern 40 5 alive",
            "ladder-seed Kaern 40 5 dead more",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_chat_lines_read() {
        assert_eq!(
            parse("  notice  The realm restarts at 20:00 "),
            Ok(Some(Line::Notice {
                text: "The realm restarts at 20:00".into()
            }))
        );
        assert_eq!(
            parse("mute ana 30 spam in Global"),
            Ok(Some(Line::Mute {
                account: "ana".into(),
                minutes: 30,
                reason: "spam in Global".into()
            }))
        );
        assert_eq!(
            parse("MUTE ana 5"),
            Ok(Some(Line::Mute {
                account: "ana".into(),
                minutes: 5,
                reason: String::new()
            }))
        );
        assert_eq!(
            parse("unmute ana"),
            Ok(Some(Line::Unmute {
                account: "ana".into()
            }))
        );
        assert_eq!(
            parse("social-clock +31d"),
            Ok(Some(Line::SocialClock {
                ms: 31 * 86_400_000
            }))
        );
        for bad in [
            "notice",
            "notice   ",
            "mute ana",
            "mute ana 0",
            "mute ana x",
            "unmute",
            "unmute ana bob",
            "social-clock 5m",
            "social-clock",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_market_lines_read() {
        let seed = |account: &str, goods, split: &str| {
            Ok(Some(Line::EconSeed(EconSeed {
                account: account.into(),
                goods,
                split: split.into(),
            })))
        };
        assert_eq!(
            parse("econ-seed ana gold:5000"),
            seed("ana", Goods::Gold(5000), "sc-l0-exp")
        );
        let runes = Goods::Stack {
            code: "r30".into(),
            count: 3,
        };
        assert_eq!(
            parse("econ-seed ana r30:3 hc-l0-cls"),
            seed("ana", runes, "hc-l0-cls")
        );
        assert_eq!(
            parse("econ-clock +8d"),
            Ok(Some(Line::EconClock { ms: 8 * 86_400_000 }))
        );
        assert_eq!(
            parse("econ-clock +90"),
            Ok(Some(Line::EconClock { ms: 90_000 }))
        );
        for bad in [
            "econ-seed",
            "econ-seed ana",
            "econ-seed ana gold",
            "econ-seed ana gold:0",
            "econ-seed ana gold:x",
            "econ-seed ana gold:5 sc-l0-exp more",
            "econ-clock",
            "econ-clock 5m",
            "econ-clock +5w",
            "econ-clock +0s",
            "econ-clock +5m +5m",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_upgrade_lines_read() {
        let up = |v: &str, minutes, minute_ms| {
            Ok(Some(Line::Upgrade(crate::upgrade::Request {
                version: v.into(),
                minutes,
                minute_ms,
            })))
        };
        assert_eq!(parse("upgrade 0.1.3.0"), up("0.1.3.0", 10, 60_000));
        assert_eq!(parse("UPGRADE 0.1.3.0 3"), up("0.1.3.0", 3, 60_000));
        assert_eq!(parse("upgrade 0.1.3.0 0"), up("0.1.3.0", 0, 60_000));
        assert_eq!(parse("upgrade 0.1.3.0 10 400"), up("0.1.3.0", 10, 400));
        assert_eq!(parse("upgrade-cancel"), Ok(Some(Line::UpgradeCancel)));
        for bad in [
            "upgrade",
            "upgrade v1",
            "upgrade 0.1.3.0 x",
            "upgrade 0.1.3.0 -1",
            "upgrade 0.1.3.0 10 5",
            "upgrade 0.1.3.0 99999",
            "upgrade 0.1.3.0 10 400 more",
            "upgrade-cancel now",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_stats_line_reads() {
        assert_eq!(parse("stats"), Ok(Some(Line::Stats)));
        assert_eq!(parse(" STATS "), Ok(Some(Line::Stats)));
        assert!(parse("stats now").is_err());
    }
}
