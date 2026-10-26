//! The ladder on a [`Session`] (`docs/plans/server/lobby-contract.md`
//! section 6): [`LadderClient`] numbers the requests, sends them, and puts
//! each answer back together: the seasons, or a page (its head, rows, the
//! pinned own row) up to its `LadderEnd`, keyed by the request number.
//!
//! It never waits, like [`crate::SocialClient`], and shares that client's
//! request numbers when made with [`LadderClient::sharing`]. Ladder kinds
//! sit inside the lobby's range: a caller that also reads the lobby feeds
//! this client first (`lobby_event` skips them). Bodies of other areas go
//! back to the caller untouched.

use std::collections::{BTreeMap, VecDeque};

use d2_net::msg::ladder::{self, Ask, FromRealm, LadderEnd, LadderHead, LadderRow, SeasonRow};
use d2_net::msg::{self, ErrorMsg};

use crate::connect::Session;
use crate::social::ReqCounter;
use crate::OnlineError;

/// One ladder answer, put together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LadderAnswer {
    /// The seasons, newest first, season 0 last; `end` says whether it was
    /// refused.
    Seasons {
        req: u32,
        rows: Vec<SeasonRow>,
        end: LadderEnd,
    },
    /// A page. A refused query (`NoSuchSeason`, `UnknownPlayer` for a
    /// find, `RateLimited`) has no head and no rows, only the end's code.
    Page {
        req: u32,
        head: Option<LadderHead>,
        rows: Vec<LadderRow>,
        /// The asker's own row, after a page it isn't on.
        pinned: Option<LadderRow>,
        end: LadderEnd,
    },
    /// The realm answered with `ERROR` (it doesn't serve the ladder:
    /// `NotSupported`). `req` is the oldest request in flight of that
    /// kind.
    Error { req: Option<u32>, error: ErrorMsg },
}

impl LadderAnswer {
    pub fn req(&self) -> Option<u32> {
        match self {
            Self::Seasons { req, .. } | Self::Page { req, .. } => Some(*req),
            Self::Error { req, .. } => *req,
        }
    }
}

/// The client side of the ladder.
#[derive(Debug, Default)]
pub struct LadderClient {
    reqs: ReqCounter,
    /// Requests in flight and what they become so far.
    in_flight: BTreeMap<u32, LadderAnswer>,
    answers: VecDeque<LadderAnswer>,
}

impl LadderClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// A client numbering its requests with `counter` (the session's
    /// [`crate::SocialClient::counter`]).
    pub fn sharing(counter: &ReqCounter) -> Self {
        Self {
            reqs: counter.clone(),
            ..Self::default()
        }
    }

    /// Requests sent and not answered yet.
    pub fn in_flight(&self) -> usize {
        self.in_flight.len()
    }

    /// Forgets what was in flight (a new connection).
    pub fn reset(&mut self) {
        self.in_flight.clear();
    }

    /// Numbers `ask` and records it as in flight: its number and the body
    /// to send.
    pub fn prepare(&mut self, ask: Ask) -> (u32, Vec<u8>) {
        let req = self.reqs.next();
        let start = match ask {
            Ask::Seasons => LadderAnswer::Seasons {
                req,
                rows: Vec::new(),
                end: LadderEnd::ok(req),
            },
            Ask::Query(_) => LadderAnswer::Page {
                req,
                head: None,
                rows: Vec::new(),
                pinned: None,
                end: LadderEnd::ok(req),
            },
        };
        self.in_flight.insert(req, start);
        (req, ladder::ToRealm { req, ask }.body())
    }

    /// Sends `ask` on `session`: its request number.
    pub fn send(&mut self, session: &mut Session, ask: Ask) -> Result<u32, OnlineError> {
        let (req, body) = self.prepare(ask);
        if let Err(e) = session.send(body) {
            self.in_flight.remove(&req);
            return Err(e);
        }
        Ok(req)
    }

    /// Drains what has arrived on `session`: ladder bodies are taken; the
    /// others are returned in order, for their owners.
    pub fn poll(&mut self, session: &mut Session) -> Result<Vec<Vec<u8>>, OnlineError> {
        let mut others = Vec::new();
        while let Some(body) = session.poll()? {
            if !self.feed(&body)? {
                others.push(body);
            }
        }
        Ok(others)
    }

    /// The answers put together so far, oldest first.
    pub fn take(&mut self) -> Vec<LadderAnswer> {
        self.answers.drain(..).collect()
    }

    /// Takes one S→C control body: `false` if it isn't the ladder's. A
    /// ladder body that doesn't read, or a page that breaks its framing,
    /// is [`OnlineError::Malformed`]. Answers to requests not in flight
    /// are dropped.
    pub fn feed(&mut self, body: &[u8]) -> Result<bool, OnlineError> {
        let Some(&k) = body.first() else {
            return Ok(false);
        };
        if k == msg::kind::ERROR {
            let e = ErrorMsg::parse(body).map_err(|_| OnlineError::Malformed)?;
            if !msg::kind::LADDER.contains(&e.request_kind) {
                return Ok(false);
            }
            let want: fn(&LadderAnswer) -> bool = match e.request_kind {
                ladder::kind::SEASONS => {
                    |a: &LadderAnswer| matches!(a, LadderAnswer::Seasons { .. })
                }
                _ => |a: &LadderAnswer| matches!(a, LadderAnswer::Page { .. }),
            };
            let req = self
                .in_flight
                .iter()
                .find(|(_, a)| want(a))
                .map(|(req, _)| *req);
            if let Some(req) = req {
                self.in_flight.remove(&req);
            }
            self.answers
                .push_back(LadderAnswer::Error { req, error: e });
            return Ok(true);
        }
        if !msg::kind::LADDER.contains(&k) {
            return Ok(false);
        }
        let m = FromRealm::parse(body).map_err(|_| OnlineError::Malformed)?;
        let req = m.req();
        let Some(answer) = self.in_flight.get_mut(&req) else {
            return Ok(true);
        };
        match (m, answer) {
            (FromRealm::SeasonRow { row, .. }, LadderAnswer::Seasons { rows, .. }) => {
                rows.push(row);
            }
            (FromRealm::Head(h), LadderAnswer::Page { head, rows, .. })
                if head.is_none() && rows.is_empty() =>
            {
                *head = Some(h);
            }
            (
                FromRealm::Row { row, .. },
                LadderAnswer::Page {
                    head: Some(h),
                    rows,
                    pinned,
                    ..
                },
            ) => {
                if rows.len() < usize::from(h.rows) {
                    rows.push(row);
                } else if pinned.is_none()
                    && h.pinned()
                    && row.flags & ladder::row_flags::PINNED != 0
                {
                    *pinned = Some(row);
                } else {
                    return Err(OnlineError::Malformed);
                }
            }
            (FromRealm::End(e), answer) => {
                let whole = match answer {
                    LadderAnswer::Page {
                        head: Some(h),
                        rows,
                        ..
                    } => e.is_ok() && rows.len() == usize::from(h.rows),
                    LadderAnswer::Page { head: None, .. } => !e.is_ok(),
                    LadderAnswer::Seasons { rows, .. } => e.is_ok() || rows.is_empty(),
                    LadderAnswer::Error { .. } => false,
                };
                if !whole {
                    return Err(OnlineError::Malformed);
                }
                let mut done = self.in_flight.remove(&req).expect("in flight");
                if let LadderAnswer::Seasons { end, .. } | LadderAnswer::Page { end, .. } =
                    &mut done
                {
                    *end = e;
                }
                self.answers.push_back(done);
            }
            _ => return Err(OnlineError::Malformed),
        }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "ladder_tests.rs"]
mod tests;
