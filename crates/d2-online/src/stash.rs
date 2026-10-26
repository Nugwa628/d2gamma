//! The account's shared stash on the main server (contracts 11.13, kinds
//! 0xA0-0xAF): one request and its whole answer. A put or a take waits
//! for the game server to move the goods and save the character, so the
//! game runs these off its frame loop, and reads the answer a slice at a
//! time ([`StashWait`]): between the slices the session is free for the
//! rest of the game (a reconnect, the lobby).
//!
//! Every stash answer that arrives while a request waits is read here:
//! the late answer of an earlier request that timed out (another
//! request number, another page) is dropped, never left for
//! [`Session::poll`].

use std::time::{Duration, Instant};

use d2_net::msg::stash::{kind, StashCell, StashEvent, StashItem, StashRequest, StashShapeMsg};

use crate::{OnlineError, Session};

/// How long a stash request waits for its answer: a move waits for the
/// game server (the main server gives up on it after its own wait and
/// answers then).
pub const STASH_WAIT: Duration = Duration::from_secs(30);

/// The stash's S→C kinds.
const ANSWERS: [u8; 5] = [
    kind::STASH_PAGE,
    kind::STASH_ITEM,
    kind::STASH_DONE,
    kind::STASH_FOUND,
    kind::STASH_SHAPE,
];

/// The last of the stash's kinds (contracts 11.13: 0xA0-0xAF).
const STASH_LAST: u8 = 0xAF;

/// Whether `body` is a stash answer (an `ERROR` for a stash request
/// too): it waits for its [`StashWait`], never for [`Session::poll`].
pub(crate) fn is_answer(body: &[u8]) -> bool {
    match body.first() {
        Some(&d2_net::msg::kind::ERROR) => d2_net::msg::ErrorMsg::parse(body)
            .is_ok_and(|e| (kind::STASH_OPEN..=STASH_LAST).contains(&e.request_kind)),
        Some(k) => ANSWERS.contains(k),
        None => false,
    }
}

/// A stash request's whole answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StashAnswer {
    /// `StashOpen`'s page with every item of it.
    Page {
        tab: u8,
        page: u16,
        gold: u64,
        stacks: Vec<(String, u32)>,
        items: Vec<StashItem>,
    },
    /// A changing request's `StashDone`: `code` 0 done, else the
    /// refusal's code.
    Done {
        code: u16,
        count: u64,
        left: Vec<(u32, u16)>,
    },
    /// `StashSearch`'s hits.
    Found { hits: Vec<(u32, StashCell)> },
    /// `StashLayout`'s answer: the item pages' shape and names.
    Shape(StashShapeMsg),
}

impl Session {
    /// Sends one stash request and waits at most `wait` for its whole
    /// answer. An `ERROR` for the request's kind (no stash on this server,
    /// no character selected) is [`OnlineError::Server`]; a refused move is
    /// a [`StashAnswer::Done`] with its code.
    pub fn stash(
        &mut self,
        req: &StashRequest,
        wait: Duration,
    ) -> Result<StashAnswer, OnlineError> {
        let mut w = self.stash_start(req)?;
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if let Some(a) = w.step(self, left)? {
                return Ok(a);
            }
            if left.is_zero() {
                return Err(OnlineError::TimedOut);
            }
        }
    }

    /// Sends one stash request; its answer is read with
    /// [`StashWait::step`], which holds the session only while it reads.
    pub fn stash_start(&mut self, req: &StashRequest) -> Result<StashWait, OnlineError> {
        self.send(req.body())?;
        Ok(StashWait {
            req: req.clone(),
            page: None,
        })
    }
}

/// A stash request under way ([`Session::stash_start`]).
#[derive(Clone, Debug)]
pub struct StashWait {
    req: StashRequest,
    /// A page whose items are still to come, and whether it is the one
    /// asked for (an earlier request's page is read and dropped).
    page: Option<(StashAnswer, u16, bool)>,
}

impl StashWait {
    /// Reads what arrived on `session` for at most `wait`: the whole
    /// answer once it is in, else `None` (call again). Another request's
    /// late answers are dropped; bodies of other kinds stay for
    /// [`Session::poll`], and stash bodies another reader left queued
    /// are taken from there.
    pub fn step(
        &mut self,
        session: &mut Session,
        wait: Duration,
    ) -> Result<Option<StashAnswer>, OnlineError> {
        let k = self.req.kind();
        let deadline = Instant::now() + wait;
        loop {
            let b = match session.take_queued(k, &ANSWERS) {
                Some(b) => b?,
                None => match session.answer_of(k, &ANSWERS, deadline) {
                    Ok(b) => b,
                    Err(OnlineError::TimedOut) => return Ok(None),
                    Err(e) => return Err(e),
                },
            };
            let ev = StashEvent::parse(&b).map_err(|_| OnlineError::Malformed)?;
            if let Some(a) = self.read(ev)? {
                return Ok(Some(a));
            }
        }
    }

    /// One stash body: the answer when it completes it.
    fn read(&mut self, ev: StashEvent) -> Result<Option<StashAnswer>, OnlineError> {
        // A page's items follow it (another request's late answer may
        // come between).
        if let Some((page, want, ours)) = &mut self.page {
            match ev {
                StashEvent::Item(i) => {
                    if let StashAnswer::Page { items, .. } = page {
                        items.push(i);
                        if items.len() < usize::from(*want) {
                            return Ok(None);
                        }
                    }
                    let ours = *ours;
                    let (page, _, _) = self.page.take().expect("a page is read");
                    return Ok(ours.then_some(page));
                }
                StashEvent::Done { .. } | StashEvent::Found { .. } | StashEvent::Shape(_) => {
                    return Ok(None)
                }
                StashEvent::Page { .. } => return Err(OnlineError::Malformed),
            }
        }
        match (&self.req, ev) {
            (
                req,
                StashEvent::Page {
                    tab: t,
                    page: p,
                    gold,
                    stacks,
                    items,
                },
            ) => {
                let ours =
                    matches!(req, StashRequest::Open { tab, page } if (t, p) == (*tab, *page));
                let page = StashAnswer::Page {
                    tab: t,
                    page: p,
                    gold,
                    stacks,
                    items: Vec::with_capacity(items.into()),
                };
                if items == 0 {
                    return Ok(ours.then_some(page));
                }
                self.page = Some((page, items, ours));
                Ok(None)
            }
            (StashRequest::Search { .. }, StashEvent::Found { hits }) => {
                Ok(Some(StashAnswer::Found { hits }))
            }
            (StashRequest::Layout, StashEvent::Shape(s)) => Ok(Some(StashAnswer::Shape(s))),
            (
                req,
                StashEvent::Done {
                    req: r,
                    code,
                    count,
                    left,
                },
            ) if req.req() == Some(r) => Ok(Some(StashAnswer::Done { code, count, left })),
            // An earlier request's late answer.
            _ => Ok(None),
        }
    }
}
