//! The market's verbs (`docs/plans/server/market-contract.md`, kinds
//! 0xB0-0xCF), driven on the bot's control stream as the game's client
//! sends them. Every step's request goes out with the account's next
//! request number (from the hello's `last_req`); what the main server
//! pushes in between (`MarketNotice`, a `MarketState` with request 0) is
//! kept for the `market` query.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use d2_net::msg::market::{
    self as m, AnswerHow, Ask, FromRealm, ListingMode, OffersWhich, PriceOf, ShopOp, ShopTab,
    ToRealm,
};
use d2_net::msg::{kind as msg_kind, ErrorCode, ErrorMsg};

use super::{Bot, Failed, Outcome, Status};

/// What the bot remembers of the market between steps.
#[derive(Debug, Default)]
pub struct Memo {
    /// The last request number sent (0: no hello yet).
    req: u64,
    /// The fees' revision of the last state (sent with every change).
    fee_rev: u32,
    /// The listing `market-pick` chose, with its price's hash then.
    picked: Option<(u64, [u8; 32])>,
    /// The last change request's body (`market-repeat`).
    last_change: Option<Vec<u8>>,
    /// Notices pushed to it, by `what`.
    notices: Vec<m::Notice>,
    /// The last state pushed to it.
    pushed: Option<m::MarketState>,
    /// This step's refusal, with its facts (`wait_s`).
    refused: Option<Outcome>,
}

/// The step's words: plain ones and `key=value` ones.
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

    fn get(&self, k: &str) -> Option<&'a str> {
        self.kv.get(k).copied()
    }

    fn yes(&self, k: &str) -> bool {
        self.get(k) == Some("yes")
    }
}

/// `G`, `G+CODE:N+..` or `CODE:N+..`: gold and currency stacks.
fn parse_price(s: &str) -> Result<m::Price, String> {
    let mut p = m::Price::default();
    for part in s.split('+') {
        match part.split_once(':') {
            Some((code, n)) => {
                let n = n.parse().map_err(|_| format!("price: {part:?}"))?;
                p.stacks.push((code.to_owned(), n));
            }
            None => p.gold = part.parse().map_err(|_| format!("price: {part:?}"))?,
        }
    }
    Ok(p)
}

fn price_text(p: &m::Price) -> String {
    let mut parts = vec![p.gold.to_string()];
    parts.extend(p.stacks.iter().map(|(c, n)| format!("{c}:{n}")));
    parts.join("+")
}

fn goods_text(g: &m::Goods) -> String {
    match g {
        m::Goods::Item { item_id, .. } => format!("item#{item_id}"),
        m::Goods::Stack { code, count } => format!("{code}:{count}"),
        m::Goods::Gold(g) => format!("gold:{g}"),
    }
}

fn group_name(g: m::Group) -> String {
    format!("{g:?}").to_ascii_lowercase()
}

fn state_name<T: std::fmt::Debug>(s: T) -> String {
    format!("{s:?}").to_ascii_lowercase()
}

/// A delivery's `reason` as the contract names it.
fn reason_name(r: u8) -> &'static str {
    match r {
        0 => "none",
        1 => "declined",
        2 => "withdrawn",
        3 => "outbid",
        4 => "expired",
        5 => "cancelled",
        6 => "change",
        _ => "unknown",
    }
}

/// A notice's `what` as the contract names it.
fn notice_name(w: u8) -> &'static str {
    match w {
        1 => "sold",
        2 => "offer",
        3 => "countered",
        4 => "accepted",
        5 => "declined",
        6 => "returned",
        7 => "expired",
        8 => "withdrawn",
        _ => "unknown",
    }
}

fn code_name(c: ErrorCode) -> String {
    match c.name() {
        "Unknown" => c.0.to_string(),
        n => n.to_owned(),
    }
}

/// A `MarketDone` as the step's outcome: ok, or refused with its code.
fn done_outcome(d: &m::Done, ms: u128) -> Outcome {
    let o = if d.code.0 == 0 {
        Outcome::ok()
    } else {
        Outcome {
            status: Status::Refused {
                code: code_name(d.code),
                text: if d.text.is_empty() {
                    d.code.text().into()
                } else {
                    d.text.clone()
                },
            },
            facts: BTreeMap::new(),
        }
    };
    let ok = d.rows.iter().filter(|(_, c)| c.0 == 0).count();
    let mut o = o
        .fact("ms", ms)
        .fact("wait_s", d.wait_s)
        .fact("repeated", if d.repeated { "yes" } else { "no" })
        .fact("rows", d.rows.len())
        .fact("rows_ok", ok);
    if let Some(id) = d.id {
        o = o.fact("id", id);
    }
    if let Some((_, c)) = d.rows.iter().find(|(_, c)| c.0 != 0) {
        o = o.fact("row_refused", code_name(*c));
    }
    o
}

fn fail(s: impl Into<String>) -> Failed {
    Failed::Local(Status::Error(s.into()))
}

impl Bot {
    /// One market verb (`market`, `market-search`, ...).
    pub fn market(&mut self, verb: &str, args: &[String], wait: Duration) -> Outcome {
        let started = Instant::now();
        let a = Args::of(args);
        // The first market step's hello: the request numbers and the fees'
        // revision a change names.
        if self.market.req == 0 && verb != "market" {
            if let Err(f) = self.market_ready(wait) {
                return f.outcome(started.elapsed().as_millis());
            }
        }
        let wait = wait.saturating_sub(started.elapsed());
        self.market.refused = None;
        let r = match verb {
            "market" => self.market_hello(wait),
            "market-search" => self.market_search(&a, wait),
            "market-pick" => self.market_pick(&a, wait),
            "market-get" => self.market_get(wait),
            "market-buy" => self.market_buy(wait),
            "market-post" => self.market_post(&a, wait),
            "market-offer" => self.market_offer(&a, wait),
            "market-answer" => self.market_answer(&a, wait),
            "market-offers" => self.market_offers(&a, wait),
            "market-shop" => self.market_shop(&a, wait),
            "market-edit" => self.market_edit(&a, wait),
            "market-deliveries" => self.market_deliveries(wait),
            "market-claim" => self.market_claim(wait),
            "market-prices" => self.market_prices(&a, wait),
            "market-flood" => self.market_flood(&a, wait),
            "market-repeat" => self.market_repeat(wait),
            _ => Err(fail(format!("{verb} has no driver"))),
        };
        r.unwrap_or_else(|f| {
            let ms = started.elapsed().as_millis();
            self.market.refused.take().unwrap_or_else(|| f.outcome(ms))
        })
    }

    /// A hello: the account's last request number and the fees' revision.
    fn market_ready(&mut self, wait: Duration) -> Result<(), Failed> {
        let s = self.market_state(1, wait)?;
        self.market.req = s.last_req.max(1);
        self.market.fee_rev = s.fees.fee_rev;
        Ok(())
    }

    /// The next request number, after a hello when there was none.
    fn market_next(&mut self, wait: Duration) -> Result<u64, Failed> {
        if self.market.req == 0 {
            self.market_ready(wait)?;
        }
        self.market.req += 1;
        Ok(self.market.req)
    }

    /// A hello as request `req`: its state.
    fn market_state(&mut self, req: u64, wait: Duration) -> Result<m::MarketState, Failed> {
        let got = self.market_send(
            ToRealm {
                req,
                ask: Ask::Hello,
            },
            wait,
        )?;
        match got.into_iter().last() {
            Some(FromRealm::State(s)) => Ok(s),
            Some(FromRealm::Done(d)) => Err(Failed::Local(done_outcome(&d, 0).status)),
            _ => Err(fail("no MarketState")),
        }
    }

    /// Sends one request and reads its answers: every message with its
    /// request number, up to its last one (`MarketDone`, `MarketState`,
    /// `MarketListing` or the list's end). Pushes in between are kept.
    fn market_send(&mut self, msg: ToRealm, wait: Duration) -> Result<Vec<FromRealm>, Failed> {
        let kind = msg.kind();
        let req = msg.req;
        let body = msg.body();
        if msg.ask.changes() {
            self.market.last_change = Some(body.clone());
        }
        self.market_send_body(kind, req, body, wait)
    }

    fn market_send_body(
        &mut self,
        kind: u8,
        req: u64,
        body: Vec<u8>,
        wait: Duration,
    ) -> Result<Vec<FromRealm>, Failed> {
        self.send_control(body).map_err(Failed::Local)?;
        let deadline = Instant::now() + wait;
        let mut got = Vec::new();
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
                Some(k) if msg_kind::MARKET.contains(k) => {
                    let msg = FromRealm::parse(&b)
                        .map_err(|e| fail(format!("a malformed market message 0x{k:02X}: {e}")))?;
                    if msg.req() != Some(req) {
                        self.market_pushed(msg);
                        continue;
                    }
                    let last = matches!(
                        msg,
                        FromRealm::Done(_)
                            | FromRealm::State(_)
                            | FromRealm::Listing { .. }
                            | FromRealm::ListEnd { .. }
                    );
                    got.push(msg);
                    if last {
                        return Ok(got);
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

    /// A market message that isn't the answer waited for: a push, or a
    /// late answer to an earlier request (dropped).
    pub(super) fn market_pushed(&mut self, msg: FromRealm) {
        match msg {
            FromRealm::Notice(n) => self.market.notices.push(n),
            FromRealm::State(s) if s.req == 0 => {
                self.market.fee_rev = s.fees.fee_rev;
                self.market.pushed = Some(s);
            }
            _ => {}
        }
    }

    /// The answer's `MarketDone`, as the outcome.
    fn market_done(&mut self, ask: Ask, wait: Duration) -> Result<Outcome, Failed> {
        let started = Instant::now();
        let req = self.market_next(wait)?;
        let got = self.market_send(ToRealm { req, ask }, wait)?;
        match got.last() {
            Some(FromRealm::Done(d)) => Ok(done_outcome(d, started.elapsed().as_millis())),
            _ => Err(fail("no MarketDone")),
        }
    }

    /// The answer's messages, or the refusal of its `MarketDone`.
    fn market_list(&mut self, ask: Ask, wait: Duration) -> Result<Vec<FromRealm>, Failed> {
        let req = self.market_next(wait)?;
        let got = self.market_send(ToRealm { req, ask }, wait)?;
        if let Some(FromRealm::Done(d)) = got.last() {
            let o = done_outcome(d, 0);
            let status = o.status.clone();
            self.market.refused = Some(o);
            return Err(Failed::Local(status));
        }
        Ok(got)
    }

    /// `market`: a hello; facts on, why, last_req, the counts, limits, and
    /// the notices pushed so far.
    fn market_hello(&mut self, wait: Duration) -> Result<Outcome, Failed> {
        let req = self.market.req.max(1) + 1;
        let s = self.market_state(req, wait)?;
        self.market.req = s.last_req.max(req);
        self.market.fee_rev = s.fees.fee_rev;
        let why = match s.off_reason {
            m::OffReason::On => "on",
            m::OffReason::Switch => "switch",
            m::OffReason::NoCharacter => "no-character",
        };
        let mut o = Outcome::ok()
            .fact("on", if s.on { "yes" } else { "no" })
            .fact("why", why)
            .fact("last_req", s.last_req)
            .fact("hardcore", if s.hardcore { "yes" } else { "no" })
            .fact("listings", s.counts.active_listings)
            .fact("deliveries", s.counts.deliveries_waiting)
            .fact("offers_to_me", s.counts.live_offers_to_me)
            .fact("my_offers", s.counts.my_live_offers)
            .fact("max_listings", s.limits.max_listings)
            .fact("max_offer_items", s.limits.max_offer_items)
            .fact("searches_per_min", s.limits.searches_per_min)
            .fact("sale_fee_pct", s.fees.sale_fee_pct)
            .fact("stackables", s.stackables.len())
            .fact("notices", self.market.notices.len())
            .fact(
                "pushed",
                if self.market.pushed.is_some() {
                    "yes"
                } else {
                    "no"
                },
            );
        if let Some(n) = self.market.notices.last() {
            o = o
                .fact("notice", notice_name(n.what))
                .fact("notice.who", &n.who)
                .fact("notice.waiting", n.deliveries_waiting);
        }
        let mut kinds: Vec<&str> = self
            .market
            .notices
            .iter()
            .map(|n| notice_name(n.what))
            .collect();
        kinds.sort_unstable();
        kinds.dedup();
        Ok(o.fact("notice.kinds", kinds.join(",")))
    }

    fn query_of(&self, a: &Args) -> Result<m::MarketQuery, Failed> {
        let mut q = m::MarketQuery {
            text: a.plain.join(" "),
            facets: a.yes("facets"),
            ..m::MarketQuery::default()
        };
        if let Some(g) = a.get("group") {
            let g = m::Group::ALL
                .iter()
                .copied()
                .find(|x| group_name(*x) == g)
                .ok_or_else(|| fail(format!("no group {g}")))?;
            q.groups.push(g);
        }
        if let Some(t) = a.get("type") {
            q.types.push(t.to_owned());
        }
        if a.get("sort") == Some("price") {
            q.sort.by = m::SortBy::Price;
            q.sort.desc = false;
        }
        if let Some(n) = a.get("page_size") {
            q.page_size = n.parse().map_err(|_| fail("page_size"))?;
        }
        Ok(q)
    }

    /// The rows of a search (and its head and facets).
    fn search_rows(
        &mut self,
        q: m::MarketQuery,
        wait: Duration,
    ) -> Result<(m::ResultsHead, Option<m::Facets>, Vec<m::ListingRow>), Failed> {
        let got = self.market_list(Ask::Search(q), wait)?;
        let mut head = None;
        let mut facets = None;
        let mut rows = Vec::new();
        for msg in got {
            match msg {
                FromRealm::ResultsBegin { head: h, .. } => head = Some(h),
                FromRealm::Facets { facets: f, .. } => facets = Some(f),
                FromRealm::Row { row, .. } => rows.push(row),
                _ => {}
            }
        }
        let head = head.ok_or_else(|| fail("no MarketResultsBegin"))?;
        Ok((head, facets, rows))
    }

    /// `market-search [TEXT..] [group=G] [type=T] [sort=price] [facets=yes]`:
    /// facts total, rows, first.* (the first row), sellers, and with
    /// facets groups (count), group.NAME and type.CODE counts.
    fn market_search(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let q = self.query_of(a)?;
        let (head, facets, rows) = self.search_rows(q, wait)?;
        let mut o = Outcome::ok()
            .fact("total", head.total)
            .fact("rows", rows.len());
        if let Some(r) = rows.first() {
            o = o
                .fact("first.seller", &r.seller)
                .fact("first.group", group_name(r.group))
                .fact("first.goods", goods_text(&r.goods))
                .fact("first.mine", if r.mine { "yes" } else { "no" })
                .fact("first.offers", r.offers)
                .fact("first.note", &r.note)
                .fact(
                    "first.price",
                    r.price.as_ref().map_or("none".into(), price_text),
                );
        }
        let mut sellers: Vec<&str> = rows.iter().map(|r| r.seller.as_str()).collect();
        sellers.sort_unstable();
        sellers.dedup();
        o = o.fact("sellers", sellers.join(","));
        if let Some(f) = facets {
            o = o.fact("groups", f.groups.len());
            for (g, n) in &f.groups {
                o = o.fact(format!("group.{}", group_name(*g)), n);
            }
            for (t, n) in &f.types {
                o = o.fact(format!("type.{t}"), n);
            }
        }
        Ok(o)
    }

    /// `market-pick [TEXT..] [..]`: the first row of the search that isn't
    /// its own is the listing `market-buy`, `market-offer`, `market-get`
    /// and `market-prices picked` act on; facts listing, price, seller.
    fn market_pick(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let q = self.query_of(a)?;
        let (_, _, rows) = self.search_rows(q, wait)?;
        let Some(r) = rows.into_iter().find(|r| !r.mine) else {
            return Ok(Outcome::error("no listing of another seller found"));
        };
        self.market.picked = Some((r.listing, r.price_hash));
        Ok(Outcome::ok()
            .fact("listing", r.listing)
            .fact("seller", &r.seller)
            .fact("price", r.price.as_ref().map_or("none".into(), price_text)))
    }

    fn picked(&self) -> Result<(u64, [u8; 32]), Failed> {
        self.market
            .picked
            .ok_or_else(|| fail("no listing picked (market-pick first)"))
    }

    /// `market-get`: the picked listing's details; facts state, offers,
    /// price, my_offer (its state or none).
    fn market_get(&mut self, wait: Duration) -> Result<Outcome, Failed> {
        let (listing, _) = self.picked()?;
        let got = self.market_list(Ask::ListingGet { listing }, wait)?;
        let Some(FromRealm::Listing { detail, .. }) = got.last() else {
            return Err(fail("no MarketListing"));
        };
        Ok(Outcome::ok()
            .fact("state", state_name(detail.state))
            .fact("offers", detail.row.offers)
            .fact(
                "price",
                detail.row.price.as_ref().map_or("none".into(), price_text),
            )
            .fact(
                "my_offer",
                detail
                    .my_offer
                    .as_ref()
                    .map_or("none".into(), |o| state_name(o.state)),
            ))
    }

    /// `market-buy`: buys the picked listing at the price it was picked
    /// at; refused with `PriceChanged` when the seller changed it since.
    fn market_buy(&mut self, wait: Duration) -> Result<Outcome, Failed> {
        let (listing, price_hash) = self.picked()?;
        let ask = Ask::Buy {
            listing,
            price_hash,
            fee_rev: self.market.fee_rev,
        };
        self.market_done(ask, wait)
    }

    /// `market-post GOODS price=P [mode=price|offers|both] [days=N]
    /// [note=TEXT] [hidden=yes]`: GOODS is `CODE:N` (a stackable count) or
    /// an item of the stash (its name or code); facts id (the listing).
    fn market_post(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let started = Instant::now();
        let what = a
            .plain
            .first()
            .ok_or_else(|| fail("market-post takes GOODS"))?;
        let goods = match what.split_once(':') {
            Some((code, n)) => m::PostGoods::Stack {
                code: code.to_owned(),
                count: n.parse().map_err(|_| fail(format!("count {n:?}")))?,
            },
            None => {
                let hits = self.stash_find(what, wait)?;
                let &(item_id, _) = hits
                    .first()
                    .ok_or_else(|| fail(format!("no {what} in the stash")))?;
                m::PostGoods::Item { item_id }
            }
        };
        let mode = match a.get("mode").unwrap_or("price") {
            "price" => ListingMode::Price,
            "offers" => ListingMode::OffersOnly,
            "both" => ListingMode::PriceAndOffers,
            other => return Err(fail(format!("mode {other}"))),
        };
        let price = a.get("price").map(parse_price).transpose().map_err(fail)?;
        let post = m::Post {
            goods,
            mode,
            price,
            days: a
                .get("days")
                .map_or(Ok(3), str::parse)
                .map_err(|_| fail("days"))?,
            note: a.get("note").unwrap_or_default().replace('_', " "),
            hidden: a.yes("hidden"),
            fee_rev: self.market.fee_rev,
        };
        let left = wait.saturating_sub(started.elapsed());
        self.market_done(Ask::Post(post), left)
    }

    /// `market-offer price=P`: an offer on the picked listing.
    fn market_offer(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let (listing, _) = self.picked()?;
        let price = parse_price(a.get("price").ok_or_else(|| fail("price=P"))?).map_err(fail)?;
        let ask = Ask::Offer {
            listing,
            price,
            fee_rev: self.market.fee_rev,
        };
        self.market_done(ask, wait)
    }

    fn offer_rows(
        &mut self,
        which: OffersWhich,
        live_only: bool,
        wait: Duration,
    ) -> Result<Vec<m::OfferRow>, Failed> {
        let got = self.market_list(
            Ask::OffersOpen {
                which,
                live_only,
                page: 0,
            },
            wait,
        )?;
        Ok(got
            .into_iter()
            .filter_map(|msg| match msg {
                FromRealm::OfferRow { row, .. } => Some(row),
                _ => None,
            })
            .collect())
    }

    /// `market-answer accept|decline|counter|withdraw [price=P]
    /// [side=mine|made]`: answers the newest live offer on its listings
    /// (`side=mine`, the default but for withdraw) or of its own
    /// (`side=made`: a buyer accepting a counter, or withdrawing).
    fn market_answer(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let started = Instant::now();
        let how = match a.plain.first().copied() {
            Some("accept") => AnswerHow::Accept,
            Some("decline") => AnswerHow::Decline,
            Some("counter") => AnswerHow::Counter,
            Some("withdraw") => AnswerHow::Withdraw,
            _ => return Err(fail("market-answer takes accept|decline|counter|withdraw")),
        };
        let made = match a.get("side") {
            Some("made") => true,
            Some("mine") => false,
            _ => how == AnswerHow::Withdraw,
        };
        let which = if made {
            OffersWhich::MadeByMe
        } else {
            OffersWhich::OnMyListings
        };
        let rows = self.offer_rows(which, true, wait)?;
        let offer = rows.first().ok_or_else(|| fail("no live offer"))?.offer;
        let counter = a.get("price").map(parse_price).transpose().map_err(fail)?;
        let ask = Ask::Answer {
            offer,
            how,
            counter,
            fee_rev: self.market.fee_rev,
        };
        self.market_done(ask, wait.saturating_sub(started.elapsed()))
    }

    /// `market-offers [made=yes] [all=yes]`: the live offers on its
    /// listings (or made by it; `all`: not only live ones); facts count,
    /// first.state, first.offered, first.counter, first.buyer.
    fn market_offers(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let which = if a.yes("made") {
            OffersWhich::MadeByMe
        } else {
            OffersWhich::OnMyListings
        };
        let rows = self.offer_rows(which, !a.yes("all"), wait)?;
        let mut o = Outcome::ok().fact("count", rows.len());
        if let Some(r) = rows.first() {
            o = o
                .fact("first.state", state_name(r.state))
                .fact("first.offered", price_text(&r.offered))
                .fact(
                    "first.counter",
                    r.counter.as_ref().map_or("none".into(), price_text),
                )
                .fact("first.buyer", &r.buyer);
        }
        Ok(o)
    }

    fn tab_of(a: &Args) -> Result<ShopTab, Failed> {
        Ok(match a.get("tab").unwrap_or("shown") {
            "shown" => ShopTab::Shown,
            "hidden" => ShopTab::Hidden,
            "sold" => ShopTab::Sold,
            "expired" => ShopTab::Expired,
            t => return Err(fail(format!("tab {t}"))),
        })
    }

    fn shop_rows(
        &mut self,
        tab: ShopTab,
        wait: Duration,
    ) -> Result<(m::ShopHead, Vec<m::ShopRow>), Failed> {
        let got = self.market_list(
            Ask::ShopOpen {
                tab,
                page: 0,
                sort: m::ShopSort::Posted,
                desc: true,
            },
            wait,
        )?;
        let mut head = None;
        let mut rows = Vec::new();
        for msg in got {
            match msg {
                FromRealm::ShopBegin { head: h, .. } => head = Some(h),
                FromRealm::ShopRow { row, .. } => rows.push(row),
                _ => {}
            }
        }
        Ok((head.ok_or_else(|| fail("no MarketShopBegin"))?, rows))
    }

    /// `market-shop [tab=shown|hidden|sold|expired]`: facts rows, shown,
    /// hidden, sold, expired (the tabs' counts), slots, first.state,
    /// first.price, first.sold_to.
    fn market_shop(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let (head, rows) = self.shop_rows(Self::tab_of(a)?, wait)?;
        let mut o = Outcome::ok()
            .fact("rows", rows.len())
            .fact("shown", head.counts[0])
            .fact("hidden", head.counts[1])
            .fact("sold", head.counts[2])
            .fact("expired", head.counts[3])
            .fact("slots", head.slots_used);
        if let Some(r) = rows.first() {
            o = o
                .fact("first.state", state_name(r.state))
                .fact(
                    "first.price",
                    r.row.price.as_ref().map_or("none".into(), price_text),
                )
                .fact(
                    "first.sold_to",
                    r.sold.as_ref().map_or("none", |s| s.buyer.as_str()),
                )
                .fact("first.note", &r.row.note);
        }
        Ok(o)
    }

    /// `market-edit OP [ARG] [tab=T]`: one bulk edit of every listing on
    /// the tab's first page: `cancel`, `hide`, `show`, `relist DAYS`,
    /// `note TEXT` or `reprice P`; facts rows, rows_ok, row_refused.
    fn market_edit(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let started = Instant::now();
        let arg = a.plain.get(1).copied();
        let op = match a.plain.first().copied() {
            Some("cancel") => ShopOp::Cancel,
            Some("hide") => ShopOp::Hide,
            Some("show") => ShopOp::Show,
            Some("relist") => ShopOp::Relist {
                days: arg
                    .unwrap_or("3")
                    .parse()
                    .map_err(|_| fail("relist DAYS"))?,
            },
            Some("note") => ShopOp::Note(arg.unwrap_or_default().replace('_', " ")),
            Some("reprice") => ShopOp::Reprice {
                mode: ListingMode::Price,
                price: Some(parse_price(arg.ok_or_else(|| fail("reprice P"))?).map_err(fail)?),
            },
            _ => {
                return Err(fail(
                    "market-edit takes cancel|hide|show|relist|note|reprice",
                ))
            }
        };
        let (_, rows) = self.shop_rows(Self::tab_of(a)?, wait)?;
        let listings = rows.iter().map(|r| r.row.listing).collect();
        let ask = Ask::ShopEdit {
            op,
            listings,
            fee_rev: self.market.fee_rev,
        };
        self.market_done(ask, wait.saturating_sub(started.elapsed()))
    }

    /// `market-deliveries`: facts count, first.reason, first.goods,
    /// reasons (every row's, sorted).
    fn market_deliveries(&mut self, wait: Duration) -> Result<Outcome, Failed> {
        let got = self.market_list(Ask::DeliveriesOpen { page: 0 }, wait)?;
        let mut total = 0;
        let mut rows = Vec::new();
        for msg in got {
            match msg {
                FromRealm::DeliveriesBegin { head, .. } => total = head.total,
                FromRealm::DeliveryRow { row, .. } => rows.push(row),
                _ => {}
            }
        }
        let mut o = Outcome::ok().fact("count", total);
        if let Some(r) = rows.first() {
            o = o
                .fact("first.reason", reason_name(r.reason))
                .fact("first.goods", goods_text(&r.goods))
                .fact("first.about", &r.about);
        }
        let mut reasons: Vec<&str> = rows.iter().map(|r| reason_name(r.reason)).collect();
        reasons.sort_unstable();
        Ok(o.fact("reasons", reasons.join(",")))
    }

    /// `market-claim`: claims every delivery into the stash; facts rows,
    /// rows_ok, row_refused.
    fn market_claim(&mut self, wait: Duration) -> Result<Outcome, Failed> {
        let ask = Ask::Claim {
            all: true,
            deliveries: Vec::new(),
        };
        self.market_done(ask, wait)
    }

    /// `market-prices CODE|picked [days=N]`: a stackable's price history,
    /// or the picked listing's; facts series, sold, listed_now, recent,
    /// s0.sold, s0.buckets, s0.median.
    fn market_prices(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let of = match a.plain.first().copied() {
            Some("picked") => PriceOf::Listing(self.picked()?.0),
            Some(code) => PriceOf::Stackable(code.to_owned()),
            None => return Err(fail("market-prices takes CODE or picked")),
        };
        let days = a
            .get("days")
            .map_or(Ok(30), str::parse)
            .map_err(|_| fail("days"))?;
        let got = self.market_list(Ask::Prices { of, days }, wait)?;
        let mut head = None;
        let mut series: BTreeMap<u8, (u32, usize, Vec<u64>)> = BTreeMap::new();
        let mut recent = 0;
        for msg in got {
            match msg {
                FromRealm::PricesBegin { head: h, .. } => head = Some(h),
                FromRealm::PriceSeries { series: s, .. } => {
                    let e = series.entry(s.index).or_default();
                    e.0 = s.sold;
                    e.1 += s.buckets.len();
                    if !s.median_now.is_empty() {
                        e.2 = s.median_now;
                    }
                }
                FromRealm::PriceRecent { sales, .. } => recent = sales.len(),
                _ => {}
            }
        }
        let head = head.ok_or_else(|| fail("no MarketPricesBegin"))?;
        let mut o = Outcome::ok()
            .fact("series", head.series)
            .fact("sold", head.sold)
            .fact("listed_now", head.listed_now)
            .fact("recent", recent);
        if let Some((sold, buckets, median)) = series.get(&0) {
            o = o
                .fact("s0.sold", sold)
                .fact("s0.buckets", buckets)
                .fact("s0.median", median.first().copied().unwrap_or(0));
        }
        Ok(o)
    }

    /// `market-flood N`: N searches back to back; facts ok, limited (how
    /// many were `RateLimited`), wait_s (the first refusal's).
    fn market_flood(&mut self, a: &Args, wait: Duration) -> Result<Outcome, Failed> {
        let started = Instant::now();
        let n: u32 = a
            .plain
            .first()
            .and_then(|n| n.parse().ok())
            .ok_or_else(|| fail("market-flood takes N"))?;
        let (mut ok, mut limited, mut wait_s) = (0, 0, None);
        for _ in 0..n {
            let left = wait.saturating_sub(started.elapsed());
            let req = self.market_next(left)?;
            let ask = Ask::Search(m::MarketQuery::default());
            let got = self.market_send(ToRealm { req, ask }, left)?;
            match got.last() {
                Some(FromRealm::ListEnd { .. }) => ok += 1,
                Some(FromRealm::Done(d)) if d.code == ErrorCode::RATE_LIMITED => {
                    limited += 1;
                    wait_s.get_or_insert(d.wait_s);
                }
                Some(FromRealm::Done(d)) => {
                    return Ok(done_outcome(d, started.elapsed().as_millis()))
                }
                _ => return Err(fail("a search without its end")),
            }
        }
        Ok(Outcome::ok()
            .fact("ok", ok)
            .fact("limited", limited)
            .fact("wait_s", wait_s.unwrap_or(0)))
    }

    /// `market-repeat`: sends the last change request again, with its
    /// number: the stored answer comes back (`repeated=yes`), nothing
    /// changes twice.
    fn market_repeat(&mut self, wait: Duration) -> Result<Outcome, Failed> {
        let started = Instant::now();
        let body = self
            .market
            .last_change
            .clone()
            .ok_or_else(|| fail("no change request to repeat"))?;
        let msg = ToRealm::parse(&body).map_err(|e| fail(e.to_string()))?;
        let got = self.market_send_body(msg.kind(), msg.req, body, wait)?;
        match got.last() {
            Some(FromRealm::Done(d)) => Ok(done_outcome(d, started.elapsed().as_millis())),
            _ => Err(fail("no MarketDone")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_read_gold_and_stacks() {
        let p = parse_price("500+r30:2").unwrap();
        assert_eq!((p.gold, p.stacks.clone()), (500, vec![("r30".into(), 2)]));
        assert_eq!(price_text(&p), "500+r30:2");
        assert_eq!(parse_price("r01:1").unwrap().gold, 0);
        assert!(parse_price("x").is_err());
    }
}
