//! The ladder's verbs (`docs/plans/server/lobby-contract.md` section 6,
//! kinds 0x50-0x55), driven on the bot's control stream as the game's
//! client sends them: `ladder` lists the seasons and reads one page,
//! `ladder-find` reads the page with a character on it.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use d2_net::msg::ladder::{
    mode, row_flags, split, Ask, FromRealm, LadderEnd, LadderHead, LadderQuery, LadderRow,
    SeasonRow, ToRealm, ALL_CLASSES,
};
use d2_net::msg::{kind as msg_kind, ErrorCode, ErrorMsg};

use super::{Bot, Failed, Outcome, Status};

/// The step's `key=value` words, and its plain ones.
struct Args<'a> {
    plain: Vec<&'a str>,
    kv: BTreeMap<&'a str, &'a str>,
}

impl<'a> Args<'a> {
    fn of(args: &'a [String]) -> Self {
        let mut plain = Vec::new();
        let mut kv = BTreeMap::new();
        for a in args {
            match a.split_once('=') {
                Some((k, v)) => {
                    kv.insert(k, v);
                }
                None => plain.push(a.as_str()),
            }
        }
        Self { plain, kv }
    }

    fn yes(&self, k: &str) -> bool {
        self.kv.get(k) == Some(&"yes")
    }

    fn number<T: std::str::FromStr>(&self, k: &str) -> Result<Option<T>, Failed> {
        self.kv
            .get(k)
            .map(|v| {
                v.parse()
                    .map_err(|_| fail(format!("{k}={v} is not a number")))
            })
            .transpose()
    }
}

fn fail(s: impl Into<String>) -> Failed {
    Failed::Local(Status::Error(s.into()))
}

fn code_name(c: ErrorCode) -> String {
    match c.name() {
        "Unknown" => c.0.to_string(),
        n => n.to_owned(),
    }
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// One answer: what came before its `LadderEnd`, and the end.
struct Answer {
    seasons: Vec<SeasonRow>,
    head: Option<LadderHead>,
    rows: Vec<LadderRow>,
    end: LadderEnd,
}

impl Answer {
    /// The end's refusal as the step's outcome, with the facts so far.
    fn refused(&self, o: Outcome) -> Outcome {
        Outcome {
            status: Status::Refused {
                code: code_name(self.end.code),
                text: self.end.code.text().into(),
            },
            facts: o.facts,
        }
        .fact("refused", code_name(self.end.code))
        .fact("wait_s", self.end.wait_s)
    }
}

impl Bot {
    /// One ladder verb (`ladder`, `ladder-find`).
    pub fn ladder(&mut self, verb: &str, args: &[String], wait: Duration) -> Outcome {
        let started = Instant::now();
        let a = Args::of(args);
        let r = match verb {
            "ladder" => self.ladder_page(&a, wait),
            "ladder-find" => self.ladder_find(&a, wait),
            _ => Err(fail(format!("{verb} has no driver"))),
        };
        r.unwrap_or_else(|f| f.outcome(started.elapsed().as_millis()))
    }

    /// `ladder [season=N] [hc=yes] [classic=yes] [class=N] [from=N]
    /// [rows=N] [me=yes]`: the seasons, then one page of the season (the
    /// current one without `season`); `me=yes` asks for the page with its
    /// selected character.
    fn ladder_page(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let started = Instant::now();
        let seasons = self.ladder_send(Ask::Seasons, wait)?;
        let current = seasons.seasons.iter().find(|s| s.current).map(|s| s.season);
        let mut o = Outcome::ok()
            .fact("seasons", seasons.seasons.len())
            .fact("current", current.map_or("none".into(), |s| s.to_string()));
        for s in &seasons.seasons {
            o = o
                .fact(format!("season.{}", s.season), &s.name)
                .fact(format!("season.{}.test", s.season), yes_no(s.test))
                .fact(
                    format!("season.{}.ended", s.season),
                    yes_no(s.ended_ms != 0),
                );
        }
        if !seasons.end.is_ok() {
            return Ok(seasons.refused(o));
        }
        let mut q = self.ladder_query(a)?;
        q.season = match a.number("season")? {
            Some(s) => s,
            None => current.unwrap_or(0),
        };
        if let Some(from) = a.number("from")? {
            q.from_rank = from;
        }
        if a.yes("me") {
            q.mode = mode::AROUND_ME;
        }
        let wait = wait.saturating_sub(started.elapsed());
        let got = self.ladder_send(Ask::Query(q), wait)?;
        Ok(page_facts(o, &got))
    }

    /// `ladder-find NAME [season=N] [hc=yes] [classic=yes] [class=N]`: the
    /// page with NAME on it, in season N (the current one without it).
    fn ladder_find(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let Some(&name) = a.plain.first() else {
            return Err(fail("ladder-find takes NAME"));
        };
        let started = Instant::now();
        let mut q = self.ladder_query(a)?;
        q.season = match a.number("season")? {
            Some(s) => s,
            None => {
                let seasons = self.ladder_send(Ask::Seasons, wait)?;
                if !seasons.end.is_ok() {
                    return Ok(seasons.refused(Outcome::ok()));
                }
                seasons
                    .seasons
                    .iter()
                    .find(|s| s.current)
                    .map_or(0, |s| s.season)
            }
        };
        let wait = wait.saturating_sub(started.elapsed());
        q.mode = mode::FIND;
        q.find = name.to_owned();
        let got = self.ladder_send(Ask::Query(q), wait)?;
        let o = page_facts(Outcome::ok(), &got);
        let rank = got
            .rows
            .iter()
            .find(|r| r.char.eq_ignore_ascii_case(name))
            .map_or("none".into(), |r| r.rank.to_string());
        Ok(o.fact("rank", rank))
    }

    /// The query's split, class and page size from the step's words.
    fn ladder_query(&self, a: &Args) -> Result<LadderQuery, Failed> {
        let mut q = LadderQuery {
            split: if a.yes("classic") {
                0
            } else {
                split::EXPANSION
            },
            class: a.number("class")?.unwrap_or(ALL_CLASSES),
            ..LadderQuery::default()
        };
        if a.yes("hc") {
            q.split |= split::HARDCORE;
        }
        if let Some(rows) = a.number("rows")? {
            q.rows = rows;
        }
        Ok(q)
    }

    /// Sends one request with the next request number and reads its
    /// answer up to its `LadderEnd`. Late answers to earlier requests are
    /// dropped.
    fn ladder_send(&mut self, ask: Ask, wait: Duration) -> Result<Answer, Failed> {
        self.ladder_req = self.ladder_req.wrapping_add(1).max(1);
        let req = self.ladder_req;
        let msg = ToRealm { req, ask };
        let kind = msg.kind();
        self.send_control(msg.body()).map_err(Failed::Local)?;
        let deadline = Instant::now() + wait;
        let (mut seasons, mut head, mut rows) = (Vec::new(), None, Vec::new());
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let b = self.read_body(left).map_err(Failed::Local)?;
            match b.first() {
                Some(&msg_kind::ERROR) => {
                    let e = ErrorMsg::parse(&b).map_err(|_| fail("a malformed error"))?;
                    if e.request_kind == kind {
                        return Err(Failed::Server(e));
                    }
                    self.note_session_error(e);
                }
                Some(k) if msg_kind::LADDER.contains(k) => {
                    let msg = FromRealm::parse(&b)
                        .map_err(|e| fail(format!("a malformed ladder message 0x{k:02X}: {e}")))?;
                    if msg.req() != req {
                        continue;
                    }
                    match msg {
                        FromRealm::SeasonRow { row, .. } => seasons.push(row),
                        FromRealm::Head(h) => head = Some(h),
                        FromRealm::Row { row, .. } => rows.push(row),
                        FromRealm::End(end) => {
                            return Ok(Answer {
                                seasons,
                                head,
                                rows,
                                end,
                            })
                        }
                    }
                }
                Some(_) => {
                    if !self.handle_event(&b) {
                        return Err(fail(format!(
                            "unexpected answer 0x{:02X} to 0x{kind:02X}",
                            b[0]
                        )));
                    }
                }
                None => return Err(fail("the server sent an empty answer")),
            }
        }
    }
}

/// A page's facts: `total`, `first_rank`, `rows` (the page's, without a
/// pinned own row), `first`, `first.level`, `first.exp`, `my_rank`,
/// `pinned`, and for every row `NAME.rank`, `NAME.level`, `NAME.dead`,
/// `NAME.class`, `NAME.mine` (its account's), `NAME.me` (its selected
/// character), `NAME.online`, `NAME.tag`.
fn page_facts(o: Outcome, got: &Answer) -> Outcome {
    let Some(h) = &got.head else {
        return if got.end.is_ok() {
            o.fact("total", 0).fact("rows", 0)
        } else {
            got.refused(o)
        };
    };
    let pinned = got.rows.iter().any(|r| r.flags & row_flags::PINNED != 0);
    let page = got.rows.iter().filter(|r| r.flags & row_flags::PINNED == 0);
    let mut o = o
        .fact("total", h.total)
        .fact("first_rank", h.first_rank)
        .fact("rows", page.clone().count())
        .fact(
            "my_rank",
            h.my_rank.map_or("none".into(), |r| r.to_string()),
        )
        .fact("pinned", yes_no(pinned));
    if let Some(r) = page.clone().next() {
        o = o
            .fact("first", &r.char)
            .fact("first.level", r.level)
            .fact("first.exp", r.experience);
    }
    for r in &got.rows {
        let flag = |f: u8| yes_no(r.flags & f != 0);
        o = o
            .fact(format!("{}.rank", r.char), r.rank)
            .fact(format!("{}.level", r.char), r.level)
            .fact(format!("{}.class", r.char), r.class)
            .fact(format!("{}.dead", r.char), flag(row_flags::DEAD_HARDCORE))
            .fact(format!("{}.mine", r.char), flag(row_flags::MY_ACCOUNT))
            .fact(format!("{}.me", r.char), flag(row_flags::MY_CHARACTER))
            .fact(format!("{}.online", r.char), flag(row_flags::ONLINE))
            .fact(
                format!("{}.tag", r.char),
                r.tag.as_deref().unwrap_or("none"),
            );
    }
    if got.end.is_ok() {
        o
    } else {
        got.refused(o)
    }
}
