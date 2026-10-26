//! The market on the main server (`docs/plans/server/market-contract.md`,
//! module K1c): the realm's side of the market messages (kinds
//! 0xB0-0xCF) on a player's session.
//!
//! - Every request runs as one job on the economy worker ([`Bank`], the
//!   online stash's), so the market and the stash never race on
//!   `realm.sqlite`. The job answers on the session's control stream:
//!   the data asked for, or `MarketDone` ([`handler`]).
//! - The session's account and **selected character** decide the economy
//!   (mod, softcore or hardcore, ladder, classic or expansion). No
//!   character: the hello's state says so and every other request is
//!   `NoCharSelected` (47). The `market` switch off: every request but
//!   the hello is `MarketOff` (90).
//! - Searches and posts per minute are limited per account here
//!   (`d2-limits`, from the config), so a refusal carries its `wait_s`.
//! - After a change, the economy's notices go to every seated session of
//!   their account in the same economy, with a fresh `MarketState`
//!   ([`push`]). The expiry sweep ([`Market::sweep`]) runs every 60 s.
//! - The realm reads a posted item's facts from its record with its own
//!   game data ([`facts`]), never from a client.
//! - Test servers (`dev = test`): `econ-seed` puts gold or stackables
//!   into a stash, `econ-clock` moves the market's clock ([`Clock`]).

pub mod facts;
mod handler;
pub mod push;

use std::ops::RangeInclusive;
use std::str::FromStr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use d2_accounts::AccountId;
use d2_economy::{EconomyConfig, EconomyKey, Goods, Owner, Req, Scope};
use d2_limits::{Limiter, Rate};
use d2_net::msg::account::CharSummary;
use d2_net::msg::market::ToRealm;

use crate::economy::Bank;
use crate::log::ServerLog;
use crate::session::{ControlTx, SessionId};
use facts::ItemReader;
use push::{Seat, Seats};

/// How often the expiry sweep runs.
pub const SWEEP_S: u64 = 60;

/// The realm's market settings (contract section 9): the economy's, and
/// the `stackables` codes (empty: the game data's runes and gems).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarketConfig {
    /// Everything but the stackables, which come from the game data.
    pub economy: EconomyConfig,
    pub stackables: Vec<String>,
}

fn number<T>(value: &str, range: RangeInclusive<T>) -> Result<T, String>
where
    T: FromStr + PartialOrd + std::fmt::Display,
{
    value
        .parse::<T>()
        .ok()
        .filter(|v| range.contains(v))
        .ok_or_else(|| format!("a number in {}..={}", range.start(), range.end()))
}

impl MarketConfig {
    /// The keys it reads.
    pub const KEYS: &'static [&'static str] = &[
        "market",
        "market_fee_pct",
        "market_listing_fee",
        "market_buyer_fee",
        "market_max_listings",
        "market_max_offers",
        "market_offer_items",
        "market_days",
        "market_posts_per_min",
        "market_searches_per_min",
        "stash_items",
        "stash_gold_max",
        "stackables",
    ];

    /// Sets `key` (one of [`MarketConfig::KEYS`]) from its config value.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let e = &mut self.economy;
        match key {
            "market" => {
                e.market = match value {
                    "on" => true,
                    "off" => false,
                    _ => return Err("`on` or `off`".into()),
                }
            }
            "market_fee_pct" => e.market_fee_pct = number(value, 0..=100)?,
            "market_listing_fee" => e.market_listing_fee = number(value, 0..=1_000_000_000)?,
            "market_buyer_fee" => {
                e.market_buyer_fee = number(value, 0..=1_000_000_000)?;
                if e.market_buyer_fee != 0 {
                    return Err("reserved: 0 until buyer fees are built".into());
                }
            }
            "market_max_listings" => e.market_max_listings = number(value, 1..=1000)?,
            "market_max_offers" => e.market_max_offers = number(value, 1..=1000)?,
            "market_offer_items" => {
                e.market_offer_items =
                    number(value, 0..=d2_net::msg::market::MAX_PRICE_ITEMS as u32)?
            }
            "market_days" => e.market_days = number(value, 1..=90)?,
            "market_posts_per_min" => e.market_posts_per_min = number(value, 1..=10_000)?,
            "market_searches_per_min" => e.market_searches_per_min = number(value, 1..=10_000)?,
            "stash_items" => e.stash_items = number(value, 1..=100_000)?,
            "stash_gold_max" => e.stash_gold_max = number(value, 0..=1_000_000_000_000)?,
            "stackables" => {
                let codes: Vec<String> = value
                    .split(',')
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .map(str::to_owned)
                    .collect();
                if let Some(bad) = codes.iter().find(|c| facts::code_key(c).is_none()) {
                    return Err(format!("`{bad}` is not an item code (1-4 bytes)"));
                }
                self.stackables = codes;
            }
            _ => return Err(format!("not a market key: {key}")),
        }
        Ok(())
    }

    /// The economy's settings with the stackables looked up in the game
    /// data. Without the game data's item reader the market can't be on,
    /// and the stash's slots are the base game's (those of `stackables`
    /// when it names some), as single player's shared stash has them.
    pub fn economy_config(&self, reader: Option<&ItemReader>) -> Result<EconomyConfig, String> {
        let mut c = self.economy.clone();
        let Some(r) = reader else {
            if c.market {
                return Err("the market needs the game data".into());
            }
            c.stackables = d2_shared_stash::Catalog::base(|_| None)
                .stackables()
                .into_iter()
                .filter(|s| self.stackables.is_empty() || self.stackables.contains(&s.code))
                .collect();
            return Ok(c);
        };
        c.stackables = if self.stackables.is_empty() {
            r.stackables()
        } else {
            self.stackables
                .iter()
                .map(|code| {
                    r.stackable(code)
                        .ok_or_else(|| format!("stackables: no item `{code}` in the game data"))
                })
                .collect::<Result<_, _>>()?
        };
        Ok(c)
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// The market's clock: the system's, moved forward on a test server
/// (`econ-clock`), so expiry runs without waiting days.
#[derive(Debug, Default)]
pub struct Clock {
    offset_ms: AtomicI64,
}

impl Clock {
    pub fn now(&self) -> i64 {
        now_ms() + self.offset_ms.load(Ordering::Relaxed)
    }

    /// Moves the clock `ms` forward; the new offset.
    pub fn advance(&self, ms: i64) -> i64 {
        self.offset_ms.fetch_add(ms, Ordering::Relaxed) + ms
    }
}

/// The session a market request comes from.
#[derive(Clone, Debug)]
pub struct Who {
    pub session: SessionId,
    pub account: AccountId,
    /// The selected character.
    pub character: Option<CharSummary>,
    pub out: ControlTx,
}

/// The seam between the network side and the market.
pub trait MarketService: Send + Sync {
    /// Answers one request on `who`'s control stream (on a task of its
    /// own; it never blocks the caller).
    fn request(self: Arc<Self>, who: Who, msg: ToRealm);

    /// The session's character changed (`None`: no character): its seat
    /// moves, or it leaves it.
    fn seat(&self, who: &Who);

    /// The session ended.
    fn leave(&self, session: SessionId);
}

/// The main server's market.
pub struct Market {
    bank: Bank,
    mod_id: String,
    config: EconomyConfig,
    reader: Option<Arc<ItemReader>>,
    seats: Seats,
    searches: Mutex<Limiter<i64>>,
    posts: Mutex<Limiter<i64>>,
    clock: Clock,
    log: Arc<ServerLog>,
}

impl std::fmt::Debug for Market {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Market")
            .field("mod_id", &self.mod_id)
            .field("on", &self.config.market)
            .finish_non_exhaustive()
    }
}

impl Market {
    /// The market of `mod_id` on the economy worker `bank`, whose economy
    /// was opened with `config`.
    pub fn new(
        bank: Bank,
        mod_id: &str,
        config: EconomyConfig,
        reader: Option<Arc<ItemReader>>,
        log: Arc<ServerLog>,
    ) -> Arc<Self> {
        let per_min = |n: u32| Mutex::new(Limiter::new(Rate::per_secs(n.max(1), 60)));
        Arc::new(Self {
            bank,
            mod_id: mod_id.to_owned(),
            searches: per_min(config.market_searches_per_min),
            posts: per_min(config.market_posts_per_min),
            config,
            reader,
            seats: Seats::default(),
            clock: Clock::default(),
            log,
        })
    }

    pub fn is_on(&self) -> bool {
        self.config.market
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// The economy of a character with save status `status`.
    pub fn economy_of(&self, status: u16) -> EconomyKey {
        use d2_formats::d2s::status as st;
        EconomyKey {
            mod_id: self.mod_id.clone(),
            hardcore: status & st::HARDCORE != 0,
            // Ladder seasons don't run yet.
            ladder: 0,
            expansion: status & st::EXPANSION != 0,
        }
    }

    /// The account's limit `which` allows one more now; else the wait.
    fn limited(which: &Mutex<Limiter<i64>>, account: i64) -> Option<u32> {
        let mut l = which.lock().unwrap_or_else(|e| e.into_inner());
        l.check(&account, Instant::now()).err().map(|r| r.wait_s())
    }

    /// The expiry sweep: listings and offers past their time go to
    /// Deliveries (with their notices), old stored answers are dropped.
    pub fn sweep(self: &Arc<Self>) {
        if self.config.market {
            let me = self.clone();
            self.bank.spawn(move |e| me.sweep_on(e));
        }
    }

    /// [`Market::sweep`], waited for (`econ-clock`).
    pub fn sweep_now(self: &Arc<Self>) -> Result<(), String> {
        if !self.config.market {
            return Err("the market is off".into());
        }
        let me = self.clone();
        self.bank.run_blocking(move |e| me.sweep_on(e))
    }

    fn sweep_on(&self, e: &mut d2_economy::Economy) {
        let now = self.clock.now();
        if let Err(err) = e.expire(now) {
            self.log.line(&format!("market: expiry sweep: {err}"));
        }
        if let Err(err) = e.prune_requests(now) {
            self.log.line(&format!("market: pruning answers: {err}"));
        }
        self.push_notices(e, None);
    }

    /// Test servers: `econ-seed`: `goods` (gold or a stackable count) into
    /// the stash of `account` in the economy `split` (`sc-l0-exp`).
    pub fn seed(&self, account: AccountId, split: &str, goods: Goods) -> Result<String, String> {
        let key = self.key_of_split(split)?;
        let owner = Owner::new(account.0, key);
        let n = self.clock.now() as u64;
        let what = format!("{goods:?}");
        let req = Req {
            scope: Scope::Character("econ-seed".into()),
            n,
        };
        let r = self
            .bank
            .run_blocking(move |e| e.seed(&owner, &req, &goods, n as i64))?;
        r.map(|_| format!("seeded {what} for account {} in {split}", account.0))
            .map_err(|e| e.to_string())
    }

    /// The economy key of a split as rows store it (`hc-l0-cls`).
    fn key_of_split(&self, split: &str) -> Result<EconomyKey, String> {
        let bad = || format!("`{split}` is not a split (sc|hc-lN-exp|cls)");
        let mut parts = split.split('-');
        let (Some(core), Some(ladder), Some(exp), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(bad());
        };
        Ok(EconomyKey {
            mod_id: self.mod_id.clone(),
            hardcore: match core {
                "sc" => false,
                "hc" => true,
                _ => return Err(bad()),
            },
            ladder: ladder
                .strip_prefix('l')
                .and_then(|n| n.parse().ok())
                .ok_or_else(bad)?,
            expansion: match exp {
                "exp" => true,
                "cls" => false,
                _ => return Err(bad()),
            },
        })
    }
}

impl MarketService for Market {
    fn request(self: Arc<Self>, who: Who, msg: ToRealm) {
        tokio::spawn(self.handle(who, msg));
    }

    fn seat(&self, who: &Who) {
        match &who.character {
            Some(c) => self.seats.seat(
                who.session,
                Seat {
                    account: who.account.0,
                    key: self.economy_of(c.status),
                    out: who.out.clone(),
                },
            ),
            None => self.seats.leave(who.session),
        }
    }

    fn leave(&self, session: SessionId) {
        self.seats.leave(session);
    }
}

#[cfg(test)]
mod tests;
