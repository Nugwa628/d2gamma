//! The market on a [`Session`] (`docs/plans/server/market-contract.md`
//! section 10): [`MarketClient`] numbers the requests, sends them, and puts
//! each answer back together from the bodies the session polls: a list's
//! Begin, its rows and the item records they name, and its end become one
//! [`MarketAnswer`], keyed by the request number.
//!
//! It never waits: requests go out with [`Session::send`], answers come in
//! with [`Session::poll`] (or [`MarketClient::feed`], for a caller that
//! drains the control stream itself), and several requests can be in
//! flight with their lists interleaved. Bodies of other areas (lobby, chat)
//! go back to the caller untouched.

use std::collections::{BTreeMap, VecDeque};

use d2_net::msg::market::{
    Ask, DeliveriesHead, DeliveryRow, Done, Facets, FromRealm, ItemRecord, ListingDetail,
    ListingRow, MarketState, Notice, OfferRow, OffersHead, PriceSeries, PricesHead, ResultsHead,
    Sale, ShopHead, ShopRow, ToRealm,
};
use d2_net::msg::{self, ErrorMsg};

use crate::connect::Session;
use crate::OnlineError;

/// One answer of the realm's market, put together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarketAnswer {
    /// The answer to a hello, or a push (`req` 0).
    State(MarketState),
    /// Pushed: something happened to the account's listing or offer.
    Notice(Notice),
    /// A request carried out or refused.
    Done(Done),
    /// The realm answered a market request with `ERROR` (it doesn't serve
    /// the market: `NotSupported` to the hello). `req` is the oldest
    /// request in flight of that kind.
    Error { req: Option<u64>, error: ErrorMsg },
    /// A search's page.
    Results {
        req: u64,
        head: ResultsHead,
        facets: Option<Facets>,
        rows: Vec<ListingRow>,
    },
    /// A listing's details, with the records of the asker's offered items.
    Listing {
        req: u64,
        detail: Box<ListingDetail>,
        items: Vec<ItemRecord>,
    },
    /// A page of My shop.
    Shop {
        req: u64,
        head: ShopHead,
        rows: Vec<ShopRow>,
    },
    /// A page of offers, with the records of their offered items.
    Offers {
        req: u64,
        head: OffersHead,
        rows: Vec<OfferRow>,
        items: Vec<ItemRecord>,
    },
    /// A page of Deliveries.
    Deliveries {
        req: u64,
        head: DeliveriesHead,
        rows: Vec<DeliveryRow>,
    },
    /// A price check: the series (a series sent in parts is one here) and
    /// the recent sales.
    Prices {
        req: u64,
        head: PricesHead,
        series: Vec<PriceSeries>,
        recent: Vec<Sale>,
    },
}

impl MarketAnswer {
    /// The request this answers; `None` for a notice, 0 for a pushed state.
    pub fn req(&self) -> Option<u64> {
        match self {
            Self::State(s) => Some(s.req),
            Self::Notice(_) => None,
            Self::Done(d) => Some(d.req),
            Self::Error { req, .. } => *req,
            Self::Results { req, .. }
            | Self::Listing { req, .. }
            | Self::Shop { req, .. }
            | Self::Offers { req, .. }
            | Self::Deliveries { req, .. }
            | Self::Prices { req, .. } => Some(*req),
        }
    }
}

/// A list being put together: the answer it becomes, and the rows its
/// Begin announced.
#[derive(Debug)]
struct Partial {
    answer: MarketAnswer,
    rows: usize,
}

impl Partial {
    fn rows_so_far(&self) -> usize {
        match &self.answer {
            MarketAnswer::Results { rows, .. } => rows.len(),
            MarketAnswer::Shop { rows, .. } => rows.len(),
            MarketAnswer::Offers { rows, .. } => rows.len(),
            MarketAnswer::Deliveries { rows, .. } => rows.len(),
            _ => 0,
        }
    }

    /// Whether the list is whole at its end: every announced row, the
    /// facets when announced, every announced price series.
    fn complete(&self) -> bool {
        match &self.answer {
            MarketAnswer::Results { head, facets, .. } => {
                self.rows_so_far() == self.rows && head.facets == facets.is_some()
            }
            MarketAnswer::Prices { head, series, .. } => series.len() == head.series as usize,
            _ => self.rows_so_far() == self.rows,
        }
    }
}

/// The client side of the market: request numbers, the requests in
/// flight, and the lists being put together.
#[derive(Debug)]
pub struct MarketClient {
    next_req: u64,
    /// A [`MarketState`] arrived, so `next_req` is above every request the
    /// realm has stored for the account.
    ready: bool,
    /// Requests sent and not answered yet: `req` → kind.
    in_flight: BTreeMap<u64, u8>,
    lists: BTreeMap<u64, Partial>,
    /// Item records sent before the single message that names them (a
    /// listing's details).
    loose: BTreeMap<u64, Vec<ItemRecord>>,
    answers: VecDeque<MarketAnswer>,
}

impl Default for MarketClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MarketClient {
    pub fn new() -> Self {
        Self {
            next_req: 1,
            ready: false,
            in_flight: BTreeMap::new(),
            lists: BTreeMap::new(),
            loose: BTreeMap::new(),
            answers: VecDeque::new(),
        }
    }

    /// Whether changing requests can go out: the realm's state said which
    /// request numbers it has stored.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Requests sent and not answered yet.
    pub fn in_flight(&self) -> usize {
        self.in_flight.len()
    }

    /// Forgets what was in flight (a new connection): the next state makes
    /// the client ready again. Request numbers keep rising.
    pub fn reset(&mut self) {
        self.ready = false;
        self.in_flight.clear();
        self.lists.clear();
        self.loose.clear();
    }

    /// Numbers `ask` and records it as in flight: the message to send.
    /// `None` for a changing request before the realm's first
    /// [`MarketState`] (its number could collide with a stored one); send
    /// [`Ask::Hello`] first.
    pub fn prepare(&mut self, ask: Ask) -> Option<ToRealm> {
        if ask.changes() && !self.ready {
            return None;
        }
        let req = self.next_req;
        self.next_req += 1;
        self.in_flight.insert(req, ask.kind());
        Some(ToRealm { req, ask })
    }

    /// Sends `ask` on `session`: its request number, or `None` when it
    /// could not go out yet ([`MarketClient::prepare`]).
    pub fn send(&mut self, session: &mut Session, ask: Ask) -> Result<Option<u64>, OnlineError> {
        let Some(m) = self.prepare(ask) else {
            return Ok(None);
        };
        if let Err(e) = session.send(m.body()) {
            self.in_flight.remove(&m.req);
            return Err(e);
        }
        Ok(Some(m.req))
    }

    /// Drains what has arrived on `session`: market bodies are taken (their
    /// answers wait in [`MarketClient::take`]); the others are returned in
    /// order, for their owners.
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
    pub fn take(&mut self) -> Vec<MarketAnswer> {
        self.answers.drain(..).collect()
    }

    /// Takes one S→C control body: `false` if it isn't the market's (the
    /// caller keeps it). A market body that doesn't read, or a list that
    /// breaks its framing, is [`OnlineError::Malformed`]: the realm broke
    /// the protocol. Answers to requests not in flight are dropped.
    pub fn feed(&mut self, body: &[u8]) -> Result<bool, OnlineError> {
        let Some(&k) = body.first() else {
            return Ok(false);
        };
        if k == msg::kind::ERROR {
            let e = ErrorMsg::parse(body).map_err(|_| OnlineError::Malformed)?;
            if !msg::kind::MARKET.contains(&e.request_kind) {
                return Ok(false);
            }
            let req = self
                .in_flight
                .iter()
                .find(|(_, kind)| **kind == e.request_kind)
                .map(|(req, _)| *req);
            if let Some(req) = req {
                self.finish(req);
            }
            self.answers
                .push_back(MarketAnswer::Error { req, error: e });
            return Ok(true);
        }
        if !msg::kind::MARKET.contains(&k) {
            return Ok(false);
        }
        let m = FromRealm::parse(body).map_err(|_| OnlineError::Malformed)?;
        self.take_message(m)?;
        Ok(true)
    }

    /// The request `req` is answered.
    fn finish(&mut self, req: u64) {
        self.in_flight.remove(&req);
        self.lists.remove(&req);
        self.loose.remove(&req);
    }

    fn begin(&mut self, req: u64, rows: usize, answer: MarketAnswer) -> Result<(), OnlineError> {
        if self.lists.contains_key(&req) {
            return Err(OnlineError::Malformed);
        }
        self.lists.insert(req, Partial { answer, rows });
        Ok(())
    }

    /// The list `req` is building; a row with no Begin is malformed.
    fn list(&mut self, req: u64) -> Result<&mut Partial, OnlineError> {
        self.lists.get_mut(&req).ok_or(OnlineError::Malformed)
    }

    fn take_message(&mut self, m: FromRealm) -> Result<(), OnlineError> {
        use MarketAnswer as A;
        match m {
            FromRealm::State(s) => {
                if s.last_req >= self.next_req {
                    self.next_req = s.last_req.saturating_add(1);
                }
                self.ready = true;
                self.in_flight.remove(&s.req);
                self.answers.push_back(A::State(s));
                return Ok(());
            }
            FromRealm::Notice(n) => {
                self.answers.push_back(A::Notice(n));
                return Ok(());
            }
            _ => {}
        }
        let req = m.req().unwrap_or(0);
        if !self.in_flight.contains_key(&req) {
            return Ok(());
        }
        match m {
            FromRealm::State(_) | FromRealm::Notice(_) => unreachable!("handled above"),
            FromRealm::Done(d) => {
                self.finish(req);
                self.answers.push_back(A::Done(d));
            }
            FromRealm::Listing { detail, .. } => {
                let items = self.loose.remove(&req).unwrap_or_default();
                self.finish(req);
                self.answers.push_back(A::Listing {
                    req,
                    detail: Box::new(detail),
                    items,
                });
            }
            FromRealm::ResultsBegin { head, .. } => self.begin(
                req,
                head.rows as usize,
                A::Results {
                    req,
                    head,
                    facets: None,
                    rows: Vec::new(),
                },
            )?,
            FromRealm::ShopBegin { head, .. } => self.begin(
                req,
                head.rows as usize,
                A::Shop {
                    req,
                    head,
                    rows: Vec::new(),
                },
            )?,
            FromRealm::OffersBegin { head, .. } => self.begin(
                req,
                head.rows as usize,
                A::Offers {
                    req,
                    head,
                    rows: Vec::new(),
                    items: Vec::new(),
                },
            )?,
            FromRealm::DeliveriesBegin { head, .. } => self.begin(
                req,
                head.rows as usize,
                A::Deliveries {
                    req,
                    head,
                    rows: Vec::new(),
                },
            )?,
            FromRealm::PricesBegin { head, .. } => self.begin(
                req,
                0,
                A::Prices {
                    req,
                    head,
                    series: Vec::new(),
                    recent: Vec::new(),
                },
            )?,
            FromRealm::Item { item, .. } => match self.lists.get_mut(&req) {
                Some(Partial {
                    answer: A::Offers { items, .. },
                    ..
                }) => items.push(item),
                Some(_) => return Err(OnlineError::Malformed),
                None => self.loose.entry(req).or_default().push(item),
            },
            FromRealm::Facets { facets: f, .. } => match &mut self.list(req)?.answer {
                A::Results { facets, .. } if facets.is_none() => *facets = Some(f),
                _ => return Err(OnlineError::Malformed),
            },
            FromRealm::PriceSeries { series: part, .. } => match &mut self.list(req)?.answer {
                A::Prices { series, .. } => match series.iter_mut().find(|s| s.index == part.index)
                {
                    Some(s) if s.shape == part.shape && s.currencies == part.currencies => {
                        s.buckets.extend(part.buckets);
                    }
                    Some(_) => return Err(OnlineError::Malformed),
                    None => series.push(part),
                },
                _ => return Err(OnlineError::Malformed),
            },
            FromRealm::PriceRecent { sales, .. } => match &mut self.list(req)?.answer {
                A::Prices { recent, .. } => *recent = sales,
                _ => return Err(OnlineError::Malformed),
            },
            FromRealm::Row { row, .. } => {
                let p = self.list(req)?;
                let full = p.rows_so_far() >= p.rows;
                match &mut p.answer {
                    A::Results { rows, .. } if !full => rows.push(row),
                    _ => return Err(OnlineError::Malformed),
                }
            }
            FromRealm::ShopRow { row, .. } => {
                let p = self.list(req)?;
                let full = p.rows_so_far() >= p.rows;
                match &mut p.answer {
                    A::Shop { rows, .. } if !full => rows.push(row),
                    _ => return Err(OnlineError::Malformed),
                }
            }
            FromRealm::OfferRow { row, .. } => {
                let p = self.list(req)?;
                let full = p.rows_so_far() >= p.rows;
                match &mut p.answer {
                    A::Offers { rows, .. } if !full => rows.push(row),
                    _ => return Err(OnlineError::Malformed),
                }
            }
            FromRealm::DeliveryRow { row, .. } => {
                let p = self.list(req)?;
                let full = p.rows_so_far() >= p.rows;
                match &mut p.answer {
                    A::Deliveries { rows, .. } if !full => rows.push(row),
                    _ => return Err(OnlineError::Malformed),
                }
            }
            FromRealm::ListEnd { .. } => {
                let p = self.lists.remove(&req).ok_or(OnlineError::Malformed)?;
                if !p.complete() {
                    return Err(OnlineError::Malformed);
                }
                self.finish(req);
                self.answers.push_back(p.answer);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "market_tests.rs"]
mod tests;
