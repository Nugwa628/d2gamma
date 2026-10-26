//! The realm lobby's load and soak driver (`mp-load`, lobby plan L17).
//!
//! Where a scenario drives its bots one step after another, this runs
//! every bot on its own thread against a real main server and game server
//! pair (a `split:1` setup), each doing what a player in the lobby does at
//! a player's pace, for a set time:
//!
//! - logs in (accounts `ldaaa`, `ldaab`, ..., characters `Ldaaax`, ...),
//!   selects its character and says hello to the chat;
//! - speaks in Global, Mod and its guild's room, whispers its partners
//!   (online, and the ones that are logged out: stored for them), reads its
//!   DM conversations and marks them read;
//! - adds and removes friends and reads its list; groups of eight share a
//!   guild whose members leave now and then and are invited back;
//! - reads the ladder (pages, find, around me), the market (searches) and
//!   the room's user list (a few watch it);
//! - a quarter of the bots create, join and leave games (game list churn
//!   on the game server), and `--flap` percent log out and back in.
//!
//! Every request's answer time is kept per kind (p50, p95, p99, max) with
//! its refusals; every Global and Mod line carries its send time, so each
//! bot that hears it gives a delivery time, and a line some bot didn't hear
//! is lost. The main server's CPU and memory come from `/proc` every
//! `--sample-ms`, and its `stats` line (sessions, rooms, limits, the
//! accounts database's lock waits) every ten samples. At the end the
//! logged-out bots come back (their stored DMs must all arrive), then every
//! bot logs out and the main server must hold no session.
//!
//! The paces stay inside the contract's rate limits (section 9) with the
//! real windows (`limits` normal), so a refusal is the server's to explain.
//! The report is `load.md` and `load.json` in `--out`, with every sample
//! in `samples.csv`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, Instant};

use d2_net::msg::social::{DmLine, RoomLine};
use serde_json::{json, Value};

use crate::bot::{Bot, CreateOpts, HelloDefaults, Outcome, ServerCert, Status};
use crate::net::Relay;
use crate::server::{ServerProc, StartError, Target};
use crate::topology::{NodeOpts, TopoRun};

/// What one run does.
#[derive(Clone, Debug)]
pub struct Opts {
    /// The setup (`split:1` of `d2rust-realm` and `d2rust-node`).
    pub target: Target,
    pub bots: usize,
    /// How long the bots act, after every bot is in.
    pub secs: u64,
    /// The logins are spread over this many seconds.
    pub ramp_s: u64,
    /// Percent of the bots that log out and back in.
    pub flap_pct: usize,
    /// A quarter of the bots create, join and leave games.
    pub games: bool,
    pub sample_ms: u64,
    /// The soak's windows, for the report's rows over time.
    pub window_s: u64,
    pub out: PathBuf,
    pub seed: u64,
    /// Main server settings beyond the driver's own.
    pub settings: Vec<(String, String)>,
    /// The groups of acts the bots do ([`ACT_GROUPS`]); empty for all.
    pub only: Vec<String>,
}

/// The groups `Opts::only` names.
pub const ACT_GROUPS: [&str; 9] = [
    "chat", "whisper", "friends", "guild", "ladder", "market", "users", "games", "flap",
];

/// How long one request may take before it counts as an error.
const WAIT: Duration = Duration::from_secs(10);

/// How often an idle bot reads its connection.
const IDLE: Duration = Duration::from_millis(5);

/// A guild's bots.
const GUILD_SIZE: usize = 8;

/// Letters for a number: names in this game are letters only.
fn letters(mut i: usize) -> String {
    let mut s = [b'a'; 3];
    for c in s.iter_mut().rev() {
        *c = b'a' + (i % 26) as u8;
        i /= 26;
    }
    String::from_utf8_lossy(&s).into_owned()
}

fn account_of(i: usize) -> String {
    format!("ld{}", letters(i))
}

fn char_of(i: usize) -> String {
    format!("Ld{}x", letters(i))
}

/// The loopback address of bot `i`'s relay: its own, so the main server's
/// per-address limits see one player each, away from the game servers'
/// 127.0.1.x.
fn bot_ip(i: usize) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(127, 2, (i / 250) as u8, 2 + (i % 250) as u8))
}

/// A small random source per bot.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    /// `d` give or take a fifth.
    fn jitter(&mut self, d: Duration) -> Duration {
        let f = 0.8 + (self.next() % 400) as f64 / 1000.0;
        d.mul_f64(f)
    }

    fn chance(&mut self, pct: u64) -> bool {
        self.next() % 100 < pct
    }
}

/// What a bot does now and then, and how often (inside contract 9's
/// limits: Say and Whisper 5 per 10 s together, Global 2 per 30 s, the
/// lists 30 a minute together, the ladder 30 a minute, friend adds and
/// guild invites 20 an hour).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Act {
    SayGlobal,
    SayMod,
    SayGuild,
    Whisper,
    WhisperOffline,
    DmCheck,
    Friends,
    FriendChurn,
    Guild,
    GuildChurn,
    Ladder,
    Market,
    Users,
    Games,
    GameChurn,
    Flap,
}

impl Act {
    /// Its group in [`ACT_GROUPS`].
    fn group(self) -> &'static str {
        match self {
            Act::SayGlobal | Act::SayMod | Act::SayGuild => "chat",
            Act::Whisper | Act::WhisperOffline | Act::DmCheck => "whisper",
            Act::Friends | Act::FriendChurn => "friends",
            Act::Guild | Act::GuildChurn => "guild",
            Act::Ladder => "ladder",
            Act::Market => "market",
            Act::Users => "users",
            Act::Games | Act::GameChurn => "games",
            Act::Flap => "flap",
        }
    }

    fn every(self) -> Duration {
        Duration::from_secs(match self {
            Act::SayGlobal => 20,
            Act::SayMod => 12,
            Act::SayGuild => 15,
            Act::Whisper => 15,
            Act::WhisperOffline => 30,
            Act::DmCheck => 60,
            Act::Friends => 20,
            Act::FriendChurn => 240,
            Act::Guild => 30,
            Act::GuildChurn => 300,
            Act::Ladder => 20,
            Act::Market => 30,
            Act::Users => 45,
            Act::Games => 15,
            Act::GameChurn => 40,
            Act::Flap => 90,
        })
    }
}

/// One request kind's answers.
#[derive(Clone, Debug, Default)]
struct Kind {
    /// Answer times of the accepted ones, microseconds.
    us: Vec<u32>,
    refused: BTreeMap<String, u64>,
    /// Failures that aren't refusals (no answer in time, a closed
    /// connection), with the last one's words.
    errors: u64,
    last_error: String,
}

impl Kind {
    fn add(&mut self, o: &Outcome, took: Duration) {
        match &o.status {
            Status::Ok => self
                .us
                .push(u32::try_from(took.as_micros()).unwrap_or(u32::MAX)),
            Status::Refused { code, .. } => *self.refused.entry(code.clone()).or_default() += 1,
            Status::Error(e) | Status::Closed(e) => {
                self.errors += 1;
                self.last_error.clone_from(e);
            }
        }
    }

    fn merge(&mut self, o: Kind) {
        self.us.extend(o.us);
        for (k, n) in o.refused {
            *self.refused.entry(k).or_default() += n;
        }
        self.errors += o.errors;
        if !o.last_error.is_empty() {
            self.last_error = o.last_error;
        }
    }
}

/// Delivery times: a count per millisecond up to 2 s, then per 100 ms up
/// to 60 s (a bot reads its connection every few milliseconds, so finer
/// says nothing). Fixed size, so an hour of lines costs no more memory
/// than a minute.
#[derive(Clone, Debug, Default)]
struct Hist {
    counts: Vec<u64>,
}

impl Hist {
    const FINE: u64 = 2000;
    const BUCKETS: usize = 2000 + 581;

    fn bucket(ms: u64) -> usize {
        if ms < Self::FINE {
            ms as usize
        } else {
            (Self::FINE + ((ms - Self::FINE) / 100).min(580)) as usize
        }
    }

    fn ms_of(b: usize) -> f64 {
        let b = b as u64;
        if b < Self::FINE {
            b as f64
        } else {
            (Self::FINE + (b - Self::FINE) * 100) as f64
        }
    }

    fn add(&mut self, ms: u64) {
        if self.counts.is_empty() {
            self.counts = vec![0; Self::BUCKETS];
        }
        self.counts[Self::bucket(ms)] += 1;
    }

    fn merge(&mut self, o: &Hist) {
        if o.counts.is_empty() {
            return;
        }
        if self.counts.is_empty() {
            self.counts = vec![0; Self::BUCKETS];
        }
        for (a, b) in self.counts.iter_mut().zip(&o.counts) {
            *a += b;
        }
    }

    /// The `p` quantile (0 to 1), milliseconds.
    fn pct(&self, p: f64) -> f64 {
        let n: u64 = self.counts.iter().sum();
        if n == 0 {
            return 0.0;
        }
        let want = ((n as f64 - 1.0) * p).round() as u64;
        let mut seen = 0;
        for (b, c) in self.counts.iter().enumerate() {
            seen += c;
            if seen > want {
                return Self::ms_of(b);
            }
        }
        Self::ms_of(Self::BUCKETS - 1)
    }
}

/// A line's way: who heard it, how long it took.
#[derive(Clone, Debug, Default)]
struct Delivery {
    /// Lines sent (accepted by the server).
    sent: u64,
    /// Times a bot that should hear them heard one.
    heard: u64,
    /// Delivery times.
    ms: Hist,
}

impl Delivery {
    fn merge(&mut self, o: Delivery) {
        self.sent += o.sent;
        self.heard += o.heard;
        self.ms.merge(&o.ms);
    }
}

/// What one bot counted.
#[derive(Debug, Default)]
struct Tally {
    kinds: BTreeMap<&'static str, Kind>,
    /// Per soak window, the answer times of the chat's requests.
    windows: BTreeMap<u64, Kind>,
    /// `global`, `mod`, `guild`, `dm`, `dm-stored`.
    delivery: BTreeMap<&'static str, Delivery>,
    /// Lines of the load's own rooms heard more than once.
    dupes: u64,
    setup_failed: Option<String>,
}

impl Tally {
    fn merge(&mut self, o: Tally) {
        for (k, v) in o.kinds {
            self.kinds.entry(k).or_default().merge(v);
        }
        for (k, v) in o.windows {
            self.windows.entry(k).or_default().merge(v);
        }
        for (k, v) in o.delivery {
            self.delivery.entry(k).or_default().merge(v);
        }
        self.dupes += o.dupes;
    }
}

/// What every bot thread shares.
struct Shared {
    epoch: Instant,
    opts: Opts,
    hello: HelloDefaults,
    cert: ServerCert,
    listen: std::net::SocketAddr,
    /// Bots that log out and in (the last `flappers` indices).
    flappers: usize,
    /// Flappers logged out now.
    offline: Mutex<BTreeSet<usize>>,
    /// Bots logged in and past their hello.
    ready: AtomicUsize,
    stop: AtomicBool,
    /// The games bots created, by name (the joiners pick from these).
    games: Mutex<BTreeSet<String>>,
}

impl Shared {
    fn ms(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn is_flapper(&self, i: usize) -> bool {
        i >= self.opts.bots - self.flappers
    }

    fn window(&self) -> u64 {
        self.epoch.elapsed().as_secs() / self.opts.window_s.max(1)
    }
}

/// A line's words: `TAG BOT SEQ MS`, so a hearer knows who sent it and
/// when.
fn stamp(tag: &str, bot: usize, seq: u64, ms: u64) -> String {
    format!("{tag} {bot} {seq} {ms}")
}

fn parse_stamp(text: &str) -> Option<(&str, usize, u64, u64)> {
    let mut w = text.split(' ');
    let tag = w.next()?;
    let bot = w.next()?.parse().ok()?;
    let seq = w.next()?.parse().ok()?;
    let ms = w.next()?.parse().ok()?;
    w.next().is_none().then_some((tag, bot, seq, ms))
}

/// One bot's run.
struct Player<'a> {
    i: usize,
    sh: &'a Shared,
    bot: Bot,
    rng: Rng,
    tally: Tally,
    seq: u64,
    /// The last sequence number heard per (room, sender): a sender's
    /// lines in one room come in order, so one at or below it is heard
    /// twice (or out of order).
    seen: BTreeMap<(u8, usize), u64>,
    in_guild: bool,
    /// Its whisper partners (online bots) and friends.
    partners: Vec<usize>,
    friends: BTreeSet<usize>,
    /// A game it is in, and when it leaves it.
    game_until: Option<Instant>,
    logged_in: bool,
    /// While it is logged out (a flapper), when it comes back.
    back_at: Option<Instant>,
}

impl<'a> Player<'a> {
    fn new(i: usize, sh: &'a Shared, bot: Bot) -> Self {
        let n = sh.opts.bots;
        let online = n - sh.flappers;
        // Partners: three online bots after it (a ring), so whispers stay
        // in a few conversations as a player's do.
        let partners: Vec<usize> = (1..=3)
            .map(|k| (i + k) % online.max(1))
            .filter(|&p| p != i)
            .collect();
        let friends = partners.iter().copied().collect();
        Self {
            i,
            sh,
            bot,
            rng: Rng((sh.opts.seed ^ (i as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)) | 1),
            tally: Tally::default(),
            seq: 0,
            seen: BTreeMap::new(),
            in_guild: false,
            partners,
            friends,
            game_until: None,
            logged_in: false,
            back_at: None,
        }
    }

    /// Runs `f` on the bot and counts it under `kind`.
    fn timed(&mut self, kind: &'static str, f: impl FnOnce(&mut Bot) -> Outcome) -> Outcome {
        let started = Instant::now();
        let o = f(&mut self.bot);
        let took = started.elapsed();
        self.tally.kinds.entry(kind).or_default().add(&o, took);
        if matches!(kind, "say" | "whisper") {
            let w = self.sh.window();
            self.tally.windows.entry(w).or_default().add(&o, took);
        }
        o
    }

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    fn social(&mut self, kind: &'static str, verb: &str, words: &[&str]) -> Outcome {
        let a = Self::args(words);
        self.timed(kind, |b| b.social(verb, &a, WAIT))
    }

    fn friends_verb(&mut self, kind: &'static str, verb: &str, words: &[&str]) -> Outcome {
        let a = Self::args(words);
        self.timed(kind, |b| b.friends(verb, &a, WAIT))
    }

    fn guild_verb(&mut self, kind: &'static str, verb: &str, words: &[&str]) -> Outcome {
        let a = Self::args(words);
        self.timed(kind, |b| b.guild(verb, &a, WAIT))
    }

    /// Connects, logs in (registering the first time), selects its
    /// character and says hello to the chat.
    fn log_in(&mut self, first: bool) -> Result<(), String> {
        let (hello, cert) = (self.sh.hello.clone(), self.sh.cert.clone());
        let o = self.timed("connect", |b| b.connect(&[], &hello, &cert, false));
        ok(&o, "connect")?;
        let (account, pass) = (account_of(self.i), format!("{}-pass-1", account_of(self.i)));
        if first {
            let o = self.timed("register", |b| b.register(&account, &pass, WAIT));
            ok(&o, "register")?;
            let name = char_of(self.i);
            let class = (self.i % 7) as u8;
            let o = self.timed("char-create", |b| {
                b.char_create(&name, class, false, false, true, WAIT)
            });
            ok(&o, "char-create")?;
        } else {
            let o = self.timed("login", |b| b.login(&account, &pass, WAIT));
            ok(&o, "login")?;
        }
        let name = char_of(self.i);
        let o = self.timed("char-select", |b| b.char_select(&name, WAIT));
        ok(&o, "char-select")?;
        let o = self.social("hello", "heard-in", &["global"]);
        ok(&o, "hello")?;
        self.logged_in = true;
        Ok(())
    }

    /// The standing relations, once every bot is in: its friends, a
    /// market listing (every fourth bot, from the goods the run seeded)
    /// and its group's guild.
    fn relations(&mut self) {
        let friends: Vec<usize> = self.friends.iter().copied().collect();
        for f in friends {
            self.friends_verb("friend-add", "friend-add", &[&char_of(f)]);
        }
        if self.i.is_multiple_of(4) {
            let a = Self::args(&["r30:1", "price=900"]);
            self.timed("market-post", |b| b.market("market-post", &a, WAIT));
        }
        self.guild_setup();
    }

    /// The guild of its group: the first bot of eight creates it and
    /// invites the others, who accept as they act.
    fn guild_setup(&mut self) {
        let group = self.i / GUILD_SIZE;
        if self.i.is_multiple_of(GUILD_SIZE) {
            let tag = format!("G{}", &letters(group)[1..]);
            let name = format!("Load guild {}", letters(group));
            let o = self.guild_verb("guild-create", "guild-create", &[&tag, &name]);
            if o.status != Status::Ok {
                return;
            }
            self.in_guild = true;
            let n = self.sh.opts.bots - self.sh.flappers;
            for k in 1..GUILD_SIZE {
                let m = self.i + k;
                if m < n {
                    self.guild_verb("guild-invite", "guild-invite", &[&char_of(m)]);
                }
            }
        }
    }

    /// A pushed line it heard (or a DM): counted when it is the load's.
    fn heard(&mut self) {
        let (lines, dms) = self.bot.take_heard();
        let now = self.sh.ms();
        for l in lines {
            self.heard_line(&l, now);
        }
        for d in dms {
            self.heard_dm(&d, now);
        }
    }

    fn heard_line(&mut self, l: &RoomLine, now: u64) {
        let Some((tag, from, seq, ms)) = parse_stamp(&l.text) else {
            return;
        };
        let (key, t) = match tag {
            "lg" => ("global", 0),
            "lm" => ("mod", 1),
            "lu" => ("guild", 2),
            _ => return,
        };
        // Only the bots online all along count: a flapper hears a room's
        // backlog again at each hello, and is in no guild.
        if self.sh.is_flapper(self.i) {
            return;
        }
        let last = self.seen.entry((t, from)).or_default();
        if seq <= *last {
            self.tally.dupes += 1;
            return;
        }
        *last = seq;
        let d = self.tally.delivery.entry(key).or_default();
        d.heard += 1;
        d.ms.add(now.saturating_sub(ms));
    }

    fn heard_dm(&mut self, d: &DmLine, now: u64) {
        if d.outgoing {
            return;
        }
        let Some((tag, _, _, ms)) = parse_stamp(&d.text) else {
            return;
        };
        if tag != "lw" && tag != "lo" {
            return;
        }
        // As the sender counted it: stored for a player who was out.
        let key = if d.stored { "dm-stored" } else { "dm" };
        let e = self.tally.delivery.entry(key).or_default();
        e.heard += 1;
        e.ms.add(now.saturating_sub(ms));
    }

    /// Waits until `at`, reading what arrives.
    fn idle_until(&mut self, at: Instant) {
        while Instant::now() < at && !self.sh.stop.load(Ordering::Relaxed) {
            if self.logged_in {
                if let Some(why) = self.bot.pump() {
                    self.note_error("pump", &why);
                    self.logged_in = false;
                }
                self.heard();
            }
            std::thread::sleep(IDLE);
        }
    }

    fn note_error(&mut self, kind: &'static str, why: &str) {
        let k = self.tally.kinds.entry(kind).or_default();
        k.errors += 1;
        k.last_error = why.to_owned();
    }

    fn say(&mut self, room: &str, tag: &str) {
        self.seq += 1;
        let text = stamp(tag, self.i, self.seq, self.sh.ms());
        let o = self.social("say", "say", &[room, &text]);
        if o.status == Status::Ok {
            let key = match room {
                "global" => "global",
                "mod" => "mod",
                _ => "guild",
            };
            self.tally.delivery.entry(key).or_default().sent += 1;
        }
    }

    fn whisper(&mut self, to: usize, offline: bool) {
        self.seq += 1;
        let tag = if offline { "lo" } else { "lw" };
        let text = stamp(tag, self.i, self.seq, self.sh.ms());
        let o = self.social("whisper", "whisper", &[&char_of(to), &text]);
        if o.status == Status::Ok {
            let stored = o.facts.get("stored").is_some_and(|s| s == "yes");
            let key = if stored { "dm-stored" } else { "dm" };
            self.tally.delivery.entry(key).or_default().sent += 1;
        }
    }

    /// One action, when it applies to this bot now.
    fn act(&mut self, a: Act) {
        let n = self.sh.opts.bots;
        match a {
            Act::SayGlobal => self.say("global", "lg"),
            Act::SayMod => self.say("mod", "lm"),
            Act::SayGuild if self.in_guild => self.say("guild", "lu"),
            Act::SayGuild => {}
            Act::Whisper => {
                if let Some(&p) = self.partners.get(self.rng.below(self.partners.len())) {
                    self.whisper(p, false);
                }
            }
            Act::WhisperOffline => {
                // One of the flappers its index points at, while it is out.
                if self.sh.flappers == 0 {
                    return;
                }
                let f = n - self.sh.flappers + self.i % self.sh.flappers;
                let out = self.sh.offline.lock().expect("offline").contains(&f);
                if out && f != self.i {
                    self.whisper(f, true);
                }
            }
            Act::DmCheck => {
                let o = self.social("dm-unread", "dm-unread", &[]);
                let peers: Vec<String> = o
                    .facts
                    .iter()
                    .filter(|(k, v)| k.ends_with(".unread") && v.as_str() != "0")
                    .map(|(k, _)| k.trim_end_matches(".unread").to_owned())
                    .collect();
                if let Some(p) = peers.first() {
                    let peer = format!("*{p}");
                    self.social("dm-history", "dm-history", &[&peer, "rows=20", "read=yes"]);
                }
            }
            Act::Friends => {
                self.friends_verb("friend-list", "friend-list", &[]);
            }
            Act::FriendChurn => {
                // Drops a friend that isn't a partner, or adds a random one.
                let extra: Vec<usize> = self
                    .friends
                    .iter()
                    .copied()
                    .filter(|f| !self.partners.contains(f))
                    .collect();
                if let Some(&f) = extra.first() {
                    let o = self.friends_verb("friend-remove", "friend-remove", &[&char_of(f)]);
                    if o.status == Status::Ok {
                        self.friends.remove(&f);
                    }
                } else {
                    let f = self.rng.below(n);
                    if f != self.i {
                        let o = self.friends_verb("friend-add", "friend-add", &[&char_of(f)]);
                        if o.status == Status::Ok {
                            self.friends.insert(f);
                        }
                    }
                }
            }
            Act::Guild => {
                let o = self.guild_verb("guild-get", "guild", &[]);
                self.in_guild = o.facts.get("in").is_some_and(|v| v == "yes");
                let invites: u32 = o
                    .facts
                    .get("invites")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                if !self.in_guild && invites > 0 {
                    let o = self.guild_verb("guild-answer", "guild-answer", &["accept"]);
                    self.in_guild = o.status == Status::Ok;
                }
                // A leader invites its group's bots that aren't members.
                if self.in_guild && o.facts.get("my_rank").is_some_and(|r| r == "leader") {
                    let online = n - self.sh.flappers;
                    let group = self.i / GUILD_SIZE * GUILD_SIZE;
                    let missing: Vec<usize> = (group..(group + GUILD_SIZE).min(online))
                        .filter(|&m| {
                            m != self.i && !o.facts.contains_key(&format!("{}.rank", char_of(m)))
                        })
                        .collect();
                    if let Some(&m) = missing.get(self.rng.below(missing.len())) {
                        self.guild_verb("guild-invite", "guild-invite", &[&char_of(m)]);
                    }
                }
            }
            Act::GuildChurn => {
                // A member (not the leader) leaves now and then.
                if self.in_guild && !self.i.is_multiple_of(GUILD_SIZE) && self.rng.chance(15) {
                    let o = self.guild_verb("guild-leave", "guild-leave", &[]);
                    if o.status == Status::Ok {
                        self.in_guild = false;
                    }
                }
            }
            Act::Ladder => {
                let a = match self.rng.below(3) {
                    0 => vec!["from=1".to_owned(), "rows=50".to_owned()],
                    1 => vec!["me=yes".to_owned()],
                    _ => vec![format!("from={}", 1 + self.rng.below(n)), "rows=25".into()],
                };
                if self.rng.chance(20) {
                    let name = char_of(self.rng.below(n));
                    self.timed("ladder-find", |b| {
                        b.ladder("ladder-find", std::slice::from_ref(&name), WAIT)
                    });
                } else {
                    self.timed("ladder", |b| b.ladder("ladder", &a, WAIT));
                }
            }
            Act::Market => {
                let searches: [&[&str]; 6] = [
                    &["rune"],
                    &["ring"],
                    &["charm"],
                    &[],
                    &["facets=yes"],
                    &["group=stackable", "sort=price"],
                ];
                let a = Self::args(searches[self.rng.below(searches.len())]);
                self.timed("market-search", |b| b.market("market-search", &a, WAIT));
            }
            Act::Users => {
                let watch = if self.i.is_multiple_of(10) {
                    "watch=yes"
                } else {
                    "watch=no"
                };
                self.social("users", "users", &["global", watch]);
            }
            Act::Games => {
                self.timed("games", |b| b.games(WAIT));
            }
            Act::GameChurn => self.game_churn(),
            Act::Flap => {
                self.timed("logout", |b| b.logout(WAIT));
                self.logged_in = false;
                self.sh.offline.lock().expect("offline").insert(self.i);
                let away = Duration::from_secs(30 + self.rng.below(30) as u64);
                self.back_at = Some(Instant::now() + away);
            }
        }
    }

    /// Game list churn: in a game until its time is up, then out; else
    /// into a game another bot made, or a new one.
    fn game_churn(&mut self) {
        if let Some(until) = self.game_until {
            if Instant::now() >= until || !self.bot.in_game() {
                if self.bot.in_game() {
                    self.timed("leave-game", |b| b.leave_game(WAIT));
                }
                self.game_until = None;
            }
            return;
        }
        let listed = self.timed("games", |b| b.games(WAIT));
        let open: Vec<String> = listed
            .facts
            .iter()
            .filter_map(|(k, v)| {
                let name = k.strip_suffix(".joinable")?;
                (v == "0").then(|| name.to_owned())
            })
            .filter(|g| self.sh.games.lock().expect("games").contains(g))
            .collect();
        let stay = Duration::from_secs(20 + self.rng.below(20) as u64);
        let until = Instant::now() + WAIT;
        if !open.is_empty() && self.rng.chance(60) {
            let g = open[self.rng.below(open.len())].clone();
            self.bot.set_join(false, None);
            let o = self.timed("join-game", |b| b.join_game(&g, "", WAIT, until));
            if o.status == Status::Ok {
                self.game_until = Some(Instant::now() + stay);
            }
        } else {
            self.seq += 1;
            let g = format!("ld{}{}", letters(self.i), self.seq);
            self.bot.set_join(false, None);
            let opts = CreateOpts {
                description: "load".into(),
                ..CreateOpts::default()
            };
            let o = self.timed("create-game", |b| b.create_game(&g, "", &opts, WAIT, until));
            if o.status == Status::Ok {
                self.sh.games.lock().expect("games").insert(g);
                self.game_until = Some(Instant::now() + stay);
            }
        }
    }

    /// The acts that apply to this bot.
    fn acts(&self) -> Vec<Act> {
        let mut acts = vec![
            Act::SayGlobal,
            Act::SayMod,
            Act::SayGuild,
            Act::Whisper,
            Act::WhisperOffline,
            Act::DmCheck,
            Act::Friends,
            Act::FriendChurn,
            Act::Guild,
            Act::GuildChurn,
            Act::Ladder,
            Act::Market,
            Act::Users,
        ];
        if self.sh.opts.games && self.i % 4 == 1 {
            acts.push(Act::GameChurn);
        } else {
            acts.push(Act::Games);
        }
        if self.sh.is_flapper(self.i) {
            acts.push(Act::Flap);
        }
        let only = &self.sh.opts.only;
        acts.retain(|a| only.is_empty() || only.iter().any(|g| g == a.group()));
        acts
    }

    /// The acting phase, until the stop flag.
    fn run(&mut self) {
        let start = Instant::now();
        let mut due: BTreeMap<Act, Instant> = BTreeMap::new();
        for a in self.acts() {
            // The first of each spread over its period, so the bots don't
            // all speak at once.
            let first = a.every().mul_f64((self.rng.next() % 1000) as f64 / 1000.0);
            due.insert(a, start + first);
        }
        while !self.sh.stop.load(Ordering::Relaxed) {
            if let Some(at) = self.back_at {
                self.idle_until(at);
                if self.sh.stop.load(Ordering::Relaxed) {
                    break;
                }
                self.back_at = None;
                self.come_back();
                continue;
            }
            let Some((&a, &at)) = due.iter().min_by_key(|(_, t)| **t) else {
                break;
            };
            self.idle_until(at);
            if self.sh.stop.load(Ordering::Relaxed) {
                break;
            }
            if self.logged_in {
                self.act(a);
            }
            let next = self.rng.jitter(a.every());
            due.insert(a, Instant::now() + next);
            self.heard();
        }
    }

    /// A flapper logs in again: its stored DMs come with the hello.
    fn come_back(&mut self) {
        let r = self.log_in(false);
        self.sh.offline.lock().expect("offline").remove(&self.i);
        if let Err(e) = r {
            self.note_error("come-back", &e);
        }
    }

    /// Reads what arrives for `d`, the stop flag set.
    fn drain(&mut self, d: Duration) {
        let until = Instant::now() + d;
        while Instant::now() < until {
            if let Some(why) = self.bot.pump() {
                self.note_error("pump", &why);
                break;
            }
            self.heard();
            std::thread::sleep(IDLE);
        }
    }

    /// The end: out of its game; a flapper that is out comes back.
    fn finish(&mut self) {
        if self.bot.in_game() {
            self.timed("leave-game", |b| b.leave_game(WAIT));
        }
        if self.back_at.take().is_some() || !self.logged_in {
            self.come_back();
        }
    }
}

fn ok(o: &Outcome, what: &str) -> Result<(), String> {
    match &o.status {
        Status::Ok => Ok(()),
        s => Err(format!("{what}: {s}")),
    }
}

/// The main server's use of the machine at one moment.
#[derive(Clone, Debug, Default)]
struct Sample {
    t_s: f64,
    /// CPU seconds so far (user + system), main server and game server.
    cpu_s: f64,
    node_cpu_s: f64,
    rss_kb: u64,
    hwm_kb: u64,
    threads: u64,
    node_rss_kb: u64,
    bots_ready: usize,
    /// The `stats` line's words, when this sample asked for it.
    stats: BTreeMap<String, String>,
}

/// `/proc/PID/stat`'s CPU seconds and `/proc/PID/status`' memory.
fn proc_use(pid: u32) -> Option<(f64, u64, u64, u64)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command may hold spaces: the fields start after its `)`.
    let rest = &stat[stat.rfind(')')? + 2..];
    let f: Vec<&str> = rest.split(' ').collect();
    // utime and stime are fields 14 and 15 of the whole line: 11 and 12
    // after the state (field 3).
    let ticks: f64 = f.get(11)?.parse::<f64>().ok()? + f.get(12)?.parse::<f64>().ok()?;
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let kb = |key: &str| -> u64 {
        status
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|v| v.split_whitespace().next())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    // USER_HZ is 100 on every Linux the CI runs.
    Some((ticks / 100.0, kb("VmRSS:"), kb("VmHWM:"), kb("Threads:")))
}

fn stats_words(line: &str) -> BTreeMap<String, String> {
    line.split_whitespace()
        .filter_map(|w| w.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn pct(v: &mut [u32], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_unstable();
    let i = ((v.len() as f64 - 1.0) * p).round() as usize;
    f64::from(v[i.min(v.len() - 1)]) / 1000.0
}

/// What the bots find once in: every character on the ladder (a save
/// writes its row; the bots never save), gold for every account and a
/// rune stack for every fourth one to list.
fn seed(server: &Mutex<ServerProc>, opts: &Opts) {
    let mut server = server.lock().expect("server");
    for i in 0..opts.bots {
        let level = 1 + (i * 7) % 98;
        let exp = level * level * 1000 + i;
        let _ = server.command_line(&format!("ladder-seed {} {level} {exp}", char_of(i)));
        let _ = server.command_line(&format!("econ-seed {} gold:5000", account_of(i)));
        if i.is_multiple_of(4) {
            let _ = server.command_line(&format!("econ-seed {} r30:1", account_of(i)));
        }
    }
}

/// The run's outcome: the report's JSON and whether it held (no line
/// lost, every stored DM delivered, no session left, no bot failed to
/// log in).
pub struct LoadReport {
    pub json: Value,
    pub markdown: String,
    pub held: bool,
}

/// Open files a bot takes (its relay's sockets and its own; about 10 on
/// Linux), and
/// the run's own.
const FILES_PER_BOT: u64 = 10;
const FILES_BASE: u64 = 16;

/// The process's open files limit (Linux; `None` elsewhere).
fn open_files_limit() -> Option<u64> {
    let limits = std::fs::read_to_string("/proc/self/limits").ok()?;
    let line = limits.lines().find(|l| l.starts_with("Max open files"))?;
    line.split_whitespace().nth(3)?.parse().ok()
}

/// Runs the load: starts the setup, logs every bot in, acts for
/// `opts.secs`, drains, logs out and checks what the main server holds.
pub fn run(mut opts: Opts) -> Result<LoadReport, String> {
    // Bots that can't open a socket would show as lost lines.
    let need = FILES_BASE + FILES_PER_BOT * opts.bots as u64;
    if let Some(have) = open_files_limit().filter(|&n| n < need) {
        return Err(format!(
            "{} bots need about {need} open files and the limit is {have}: raise it first (ulimit -n {need})",
            opts.bots
        ));
    }
    // The servers run in their own folders: every path absolute.
    std::fs::create_dir_all(&opts.out).map_err(|e| e.to_string())?;
    opts.out = std::fs::canonicalize(&opts.out).map_err(|e| e.to_string())?;
    let dir = opts.out.join("work");
    let _ = std::fs::remove_dir_all(&dir);
    let mut settings = vec![
        ("ladder".to_owned(), "on".to_owned()),
        ("max_sessions".to_owned(), (opts.bots + 16).to_string()),
        ("max_games".to_owned(), "64".to_owned()),
        ("market".to_owned(), "on".to_owned()),
    ];
    for (k, v) in &opts.settings {
        settings.retain(|(have, _)| have != k);
        settings.push((k.clone(), v.clone()));
    }
    if let Some(n) = opts.target.nodes() {
        n.add_realm_settings(opts.target.realm(), &mut settings);
    }
    let mut server = ServerProc::new(opts.target.clone(), dir.join("server"), settings)
        .map_err(|e| e.to_string())?;
    let ready = match server.start() {
        Ok(r) => r.clone(),
        Err(StartError::NotServing(t) | StartError::Failed(t)) => {
            return Err(format!("the main server didn't start: {t}"))
        }
    };
    let mut topo = match opts.target.nodes() {
        Some(nodes) => Some(
            TopoRun::start(
                nodes,
                &ready,
                server.dir(),
                &dir.join("nodes"),
                &NodeOpts::default(),
                opts.seed,
            )
            .map_err(|e| match e {
                StartError::NotServing(t) | StartError::Failed(t) => {
                    format!("the game servers didn't start: {t}")
                }
            })?,
        ),
        None => None,
    };
    let opened = server
        .command_line("season-start Load season test")
        .map_err(|e| format!("season-start: {e}"))?;
    let realm_pid = server.pid();
    let node_pid = topo
        .as_ref()
        .and_then(|t| t.nodes.first())
        .and_then(|n| n.pid());
    let flappers = (opts.bots * opts.flap_pct / 100).min(opts.bots.saturating_sub(GUILD_SIZE));
    let sh = Arc::new(Shared {
        epoch: Instant::now(),
        hello: crate::runner::hello_from(&ready),
        cert: ServerCert {
            der: ready.cert.clone(),
            sha256: ready.cert_sha256,
        },
        listen: ready.listen,
        flappers,
        offline: Mutex::new(BTreeSet::new()),
        ready: AtomicUsize::new(0),
        stop: AtomicBool::new(false),
        games: Mutex::new(BTreeSet::new()),
        opts: opts.clone(),
    });
    let server = Arc::new(Mutex::new(server));
    let samples: Arc<Mutex<Vec<Sample>>> = Arc::default();
    let sampling = Arc::new(AtomicBool::new(true));
    let sampler = {
        let (sh, server, samples, sampling) = (
            sh.clone(),
            server.clone(),
            samples.clone(),
            sampling.clone(),
        );
        std::thread::spawn(move || {
            let mut k = 0u64;
            while sampling.load(Ordering::Relaxed) {
                let mut s = Sample {
                    t_s: sh.epoch.elapsed().as_secs_f64(),
                    bots_ready: sh.ready.load(Ordering::Relaxed),
                    ..Sample::default()
                };
                if let Some((cpu, rss, hwm, threads)) = realm_pid.and_then(proc_use) {
                    (s.cpu_s, s.rss_kb, s.hwm_kb, s.threads) = (cpu, rss, hwm, threads);
                }
                if let Some((cpu, rss, _, _)) = node_pid.and_then(proc_use) {
                    (s.node_cpu_s, s.node_rss_kb) = (cpu, rss);
                }
                if k.is_multiple_of(10) {
                    if let Ok(line) = server.lock().expect("server").command_line("stats") {
                        s.stats = stats_words(&line);
                    }
                }
                k += 1;
                samples.lock().expect("samples").push(s);
                std::thread::sleep(Duration::from_millis(sh.opts.sample_ms.max(100)));
            }
        })
    };
    let barrier = Arc::new(Barrier::new(opts.bots + 1));
    // The bots' end: every one stops acting, then every one listens for the
    // last lines, then every one logs out.
    let end = Arc::new(Barrier::new(opts.bots));
    let mut threads = Vec::new();
    for i in 0..opts.bots {
        let relay = Relay::start(sh.listen, opts.seed ^ i as u64, bot_ip(i))
            .map_err(|e| format!("relay for bot {i}: {e}"))?;
        let (sh, barrier, end) = (sh.clone(), barrier.clone(), end.clone());
        let ramp = Duration::from_secs(opts.ramp_s).mul_f64(i as f64 / opts.bots.max(1) as f64);
        threads.push(
            std::thread::Builder::new()
                .name(format!("load-bot-{i}"))
                .spawn(move || {
                    let mut p = Player::new(i, &sh, Bot::new(i, relay));
                    std::thread::sleep(ramp);
                    let r = p.log_in(true);
                    if let Err(e) = &r {
                        p.tally.setup_failed = Some(e.clone());
                    } else {
                        sh.ready.fetch_add(1, Ordering::Relaxed);
                    }
                    // Every bot is in; the run seeds the ladder and the
                    // market's goods, then the relations form and the
                    // bots act.
                    barrier.wait();
                    barrier.wait();
                    if r.is_ok() {
                        p.relations();
                        p.idle_until(Instant::now() + Duration::from_secs(2));
                        p.run();
                        p.finish();
                    }
                    end.wait();
                    if r.is_ok() {
                        p.drain(Duration::from_secs(3));
                    }
                    end.wait();
                    if r.is_ok() {
                        p.timed("logout", |b| b.logout(WAIT));
                    } else {
                        // Half in or not: whatever session it has goes.
                        let _ = p.bot.logout(WAIT);
                    }
                    p.tally
                })
                .map_err(|e| e.to_string())?,
        );
    }
    barrier.wait();
    seed(&server, &opts);
    barrier.wait();
    let acting = Instant::now();
    let in_at = sh.epoch.elapsed().as_secs_f64();
    // The bots act; the guilds form in the first seconds.
    std::thread::sleep(Duration::from_secs(opts.secs + 2));
    // What the main server holds with every bot still in.
    let before_out = server
        .lock()
        .expect("server")
        .command_line("stats")
        .map(|l| stats_words(&l))
        .unwrap_or_default();
    sh.stop.store(true, Ordering::Relaxed);
    let mut tally = Tally::default();
    let mut setup_failed = Vec::new();
    for (i, t) in threads.into_iter().enumerate() {
        match t.join() {
            Ok(mut t) => {
                if let Some(e) = t.setup_failed.take() {
                    setup_failed.push(format!("bot {i}: {e}"));
                }
                tally.merge(t);
            }
            Err(_) => setup_failed.push(format!("bot {i}: its thread panicked")),
        }
    }
    let acted_s = acting.elapsed().as_secs_f64();
    // Every bot logged out: the main server must let every session go.
    std::thread::sleep(Duration::from_secs(1));
    let mut after = BTreeMap::new();
    // A session dropped without a logout is held for its grace (30 s by
    // default): wait past it before calling anything left.
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline {
        after = server
            .lock()
            .expect("server")
            .command_line("stats")
            .map(|l| stats_words(&l))
            .unwrap_or_default();
        if after.get("sessions").is_some_and(|v| v == "0")
            && after.get("social").is_some_and(|v| v == "0")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    sampling.store(false, Ordering::Relaxed);
    let _ = sampler.join();
    let samples = std::mem::take(&mut *samples.lock().expect("samples"));
    let end_use = realm_pid.and_then(proc_use);
    if let Some(t) = topo.as_mut() {
        t.stop_all();
    }
    server.lock().expect("server").stop();
    let report = build_report(
        &opts,
        &sh,
        Summary {
            tally,
            samples,
            setup_failed,
            in_at,
            acted_s,
            before_out,
            after,
            end_use,
            season: opened,
        },
    );
    std::fs::create_dir_all(&opts.out).map_err(|e| e.to_string())?;
    std::fs::write(opts.out.join("load.md"), &report.markdown).map_err(|e| e.to_string())?;
    std::fs::write(
        opts.out.join("load.json"),
        serde_json::to_string_pretty(&report.json).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;
    Ok(report)
}

/// What the run gathered, for the report.
struct Summary {
    tally: Tally,
    samples: Vec<Sample>,
    setup_failed: Vec<String>,
    /// When every bot was in, seconds from the start.
    in_at: f64,
    acted_s: f64,
    /// The `stats` line with every bot still in, and once they were gone.
    before_out: BTreeMap<String, String>,
    after: BTreeMap<String, String>,
    end_use: Option<(f64, u64, u64, u64)>,
    season: String,
}

fn num(m: &BTreeMap<String, String>, k: &str) -> u64 {
    m.get(k).and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn build_report(opts: &Opts, sh: &Shared, mut s: Summary) -> LoadReport {
    let mut md = String::new();
    let online = opts.bots - sh.flappers;
    let _ = writeln!(md, "# mp-load: {} bots, {} s", opts.bots, opts.secs);
    let _ = writeln!(
        md,
        "\nSetup: {}. Bots: {} ({} online all along, {} logging out and in), game churn {}. \
         Every bot in after {:.1} s; acted {:.1} s. {}.\n",
        opts.target.describe(),
        opts.bots,
        online,
        sh.flappers,
        if opts.games { "on" } else { "off" },
        s.in_at,
        s.acted_s,
        s.season
    );
    // Requests.
    let _ = writeln!(
        md,
        "## Requests\n\n| Kind | Answered | p50 ms | p95 ms | p99 ms | max ms | Refused | Errors |\n|---|---|---|---|---|---|---|---|"
    );
    let mut kinds = serde_json::Map::new();
    for (k, v) in &mut s.tally.kinds {
        let n = v.us.len();
        let (p50, p95, p99, max) = (
            pct(&mut v.us, 0.5),
            pct(&mut v.us, 0.95),
            pct(&mut v.us, 0.99),
            pct(&mut v.us, 1.0),
        );
        let refused: Vec<String> = v.refused.iter().map(|(c, n)| format!("{c} {n}")).collect();
        let _ = writeln!(
            md,
            "| {k} | {n} | {p50:.1} | {p95:.1} | {p99:.1} | {max:.1} | {} | {}{} |",
            if refused.is_empty() {
                "0".into()
            } else {
                refused.join(", ")
            },
            v.errors,
            if v.errors > 0 {
                format!(" ({})", v.last_error)
            } else {
                String::new()
            }
        );
        kinds.insert(
            (*k).into(),
            json!({"n": n, "p50_ms": p50, "p95_ms": p95, "p99_ms": p99, "max_ms": max,
                   "refused": v.refused, "errors": v.errors, "last_error": v.last_error}),
        );
    }
    // Rates against the limits.
    let said = s.tally.kinds.get("say").map_or(0, |k| k.us.len()) as f64;
    let whispered = s.tally.kinds.get("whisper").map_or(0, |k| k.us.len()) as f64;
    let secs = s.acted_s.max(1.0);
    let per_acct_10s = (said + whispered) / opts.bots.max(1) as f64 / secs * 10.0;
    let global = s.tally.delivery.get("global").map_or(0, |d| d.sent) as f64;
    let global_30s = global / opts.bots.max(1) as f64 / secs * 30.0;
    let _ = writeln!(
        md,
        "\n## Message rates\n\nLines accepted: Say {said:.0} ({:.1}/s), Whisper {whispered:.0} ({:.1}/s). \
         Per account: {per_acct_10s:.2} Say+Whisper per 10 s (limit 5), {global_30s:.2} Global per 30 s \
         (limit 2). Room lines pushed by the main server: about {:.0}/s.",
        said / secs,
        whispered / secs,
        (global + s.tally.delivery.get("mod").map_or(0, |d| d.sent) as f64) * online as f64 / secs
    );
    // Delivery.
    let _ = writeln!(
        md,
        "\n## Delivery\n\n| Line | Sent | Heard | Expected | Lost | p50 ms | p95 ms | max ms |\n|---|---|---|---|---|---|---|---|"
    );
    let mut lost_any = false;
    let mut delivery = serde_json::Map::new();
    for (k, d) in &mut s.tally.delivery {
        // Global and Mod: every bot online all along hears each line; a DM
        // reaches its one peer; the guild's members change, so it has no
        // expected count.
        let expected = match *k {
            "global" | "mod" => Some(d.sent * online as u64),
            "dm" | "dm-stored" => Some(d.sent),
            _ => None,
        };
        let lost = expected.map(|e| e.saturating_sub(d.heard));
        lost_any |= lost.is_some_and(|l| l > 0);
        let (p50, p95, max) = (d.ms.pct(0.5), d.ms.pct(0.95), d.ms.pct(1.0));
        let _ = writeln!(
            md,
            "| {k} | {} | {} | {} | {} | {p50:.1} | {p95:.1} | {max:.1} |",
            d.sent,
            d.heard,
            expected.map_or("-".into(), |e| e.to_string()),
            lost.map_or("-".into(), |l| l.to_string()),
        );
        delivery.insert(
            (*k).into(),
            json!({"sent": d.sent, "heard": d.heard, "expected": expected, "lost": lost,
                   "p50_ms": p50, "p95_ms": p95, "max_ms": max}),
        );
    }
    let _ = writeln!(md, "\nDuplicate lines heard: {}.", s.tally.dupes);
    // The main server.
    let first = s.samples.first().cloned().unwrap_or_default();
    let last = s.samples.last().cloned().unwrap_or_default();
    let mut peak_cpu = 0.0f64;
    for w in s.samples.windows(2) {
        let dt = (w[1].t_s - w[0].t_s).max(0.001);
        peak_cpu = peak_cpu.max((w[1].cpu_s - w[0].cpu_s) / dt * 100.0);
    }
    let acting: Vec<&Sample> = s
        .samples
        .iter()
        .filter(|x| x.t_s >= s.in_at && x.t_s <= s.in_at + s.acted_s)
        .collect();
    let avg_cpu = match (acting.first(), acting.last()) {
        (Some(a), Some(b)) if b.t_s > a.t_s => (b.cpu_s - a.cpu_s) / (b.t_s - a.t_s) * 100.0,
        _ => 0.0,
    };
    let node_cpu = match (acting.first(), acting.last()) {
        (Some(a), Some(b)) if b.t_s > a.t_s => {
            (b.node_cpu_s - a.node_cpu_s) / (b.t_s - a.t_s) * 100.0
        }
        _ => 0.0,
    };
    let rss_in = acting.first().map_or(0, |x| x.rss_kb);
    let rss_end = acting.last().map_or(0, |x| x.rss_kb);
    let peak = s.samples.iter().map(|x| x.hwm_kb).max().unwrap_or(0);
    let stats: Vec<&Sample> = s.samples.iter().filter(|x| !x.stats.is_empty()).collect();
    let max_stat = |k: &str| stats.iter().map(|x| num(&x.stats, k)).max().unwrap_or(0);
    let last_stats = stats.last().map(|x| x.stats.clone()).unwrap_or_default();
    let db_locks = num(&last_stats, "db_locks");
    let db_wait = num(&last_stats, "db_wait_us");
    let _ = writeln!(
        md,
        "\n## Main server\n\n| What | Value |\n|---|---|\n\
         | CPU while acting (avg, one core = 100 %) | {avg_cpu:.1} % |\n\
         | CPU peak over one sample | {peak_cpu:.1} % |\n\
         | Memory (RSS) at start / every bot in / end of acting | {} / {rss_in} / {rss_end} KiB |\n\
         | Memory peak (HWM) | {peak} KiB |\n\
         | Threads | {} |\n\
         | Game server CPU while acting (avg) | {node_cpu:.1} % |\n\
         | Sessions (peak) | {} |\n\
         | Room members (peak) / lines kept (peak) | {} / {} |\n\
         | Control queue (most frames one session had queued) | {} |\n\
         | Accounts database: locks / total wait / max wait | {db_locks} / {:.1} ms / {:.2} ms |\n\
         | Accounts database: avg wait per lock / max held | {:.1} us / {:.2} ms |",
        first.rss_kb,
        last.threads,
        max_stat("sessions"),
        max_stat("members"),
        max_stat("lines"),
        max_stat("out_peak"),
        db_wait as f64 / 1000.0,
        num(&last_stats, "db_wait_max_us") as f64 / 1000.0,
        if db_locks > 0 {
            db_wait as f64 / db_locks as f64
        } else {
            0.0
        },
        num(&last_stats, "db_held_max_us") as f64 / 1000.0,
    );
    // Over time (the soak's question: does it grow?).
    let _ = writeln!(
        md,
        "\n## Over time\n\n| From s | RSS KiB | CPU % | Sessions | Members | Lines | DB locks | DB wait ms | Say/Whisper p95 ms | Answered |\n|---|---|---|---|---|---|---|---|---|---|"
    );
    let w = opts.window_s.max(1) as f64;
    let mut rows = Vec::new();
    let (mut prev_locks, mut prev_wait) = (0u64, 0u64);
    let windows = (s.samples.last().map_or(0.0, |x| x.t_s) / w).ceil() as u64;
    for k in 0..windows {
        let (a, b) = (k as f64 * w, (k + 1) as f64 * w);
        let ws: Vec<&Sample> = s
            .samples
            .iter()
            .filter(|x| x.t_s >= a && x.t_s < b)
            .collect();
        let (Some(f), Some(l)) = (ws.first(), ws.last()) else {
            continue;
        };
        let cpu = if l.t_s > f.t_s {
            (l.cpu_s - f.cpu_s) / (l.t_s - f.t_s) * 100.0
        } else {
            0.0
        };
        let st = ws.iter().rev().find(|x| !x.stats.is_empty());
        let sv = |key: &str| st.map_or("-".into(), |x| num(&x.stats, key).to_string());
        let (p95, n) = s.tally.windows.get_mut(&k).map_or((0.0, 0), |kw| {
            let n = kw.us.len();
            (pct(&mut kw.us, 0.95), n)
        });
        let (locks, wait) = st.map_or((prev_locks, prev_wait), |x| {
            (num(&x.stats, "db_locks"), num(&x.stats, "db_wait_us"))
        });
        let (d_locks, d_wait) = (
            locks.saturating_sub(prev_locks),
            wait.saturating_sub(prev_wait),
        );
        (prev_locks, prev_wait) = (locks, wait);
        let _ = writeln!(
            md,
            "| {a:.0} | {} | {cpu:.1} | {} | {} | {} | {d_locks} | {:.1} | {p95:.1} | {n} |",
            l.rss_kb,
            sv("sessions"),
            sv("members"),
            sv("lines"),
            d_wait as f64 / 1000.0
        );
        rows.push(
            json!({"from_s": a, "rss_kb": l.rss_kb, "cpu_pct": cpu, "p95_ms": p95, "n": n,
                         "db_locks": d_locks, "db_wait_us": d_wait}),
        );
    }
    // The end.
    let left = num(&s.after, "sessions");
    let social_left = num(&s.after, "social");
    let _ = writeln!(
        md,
        "\n## After every bot left\n\nWith every bot still in: `{}`.\n\nOnce they were gone: `{}`.\n\nMemory after: {} KiB.",
        line_of(&s.before_out),
        line_of(&s.after),
        s.end_use.map_or(0, |u| u.1)
    );
    let leak = left != 0
        || social_left != 0
        || num(&s.after, "hello") != 0
        || num(&s.after, "members") != 0
        || num(&s.after, "views") != 0
        || num(&s.after, "ctxs") != 0;
    if !s.setup_failed.is_empty() {
        let _ = writeln!(md, "\n## Bots that didn't get in\n");
        for e in &s.setup_failed {
            let _ = writeln!(md, "- {e}");
        }
    }
    let held = !lost_any && !leak && s.setup_failed.is_empty();
    let _ = writeln!(
        md,
        "\n**{}**: {}",
        if held { "HELD" } else { "BROKE" },
        [
            if lost_any {
                "lines lost"
            } else {
                "no line lost"
            },
            if leak {
                "the main server still holds sessions"
            } else {
                "no session left"
            },
            if s.setup_failed.is_empty() {
                "every bot got in"
            } else {
                "some bots didn't get in"
            },
        ]
        .join(", ")
    );
    let mut csv =
        String::from("t_s,cpu_s,rss_kb,hwm_kb,threads,node_cpu_s,node_rss_kb,bots_ready,stats\n");
    for x in &s.samples {
        let _ = writeln!(
            csv,
            "{:.2},{:.2},{},{},{},{:.2},{},{},{}",
            x.t_s,
            x.cpu_s,
            x.rss_kb,
            x.hwm_kb,
            x.threads,
            x.node_cpu_s,
            x.node_rss_kb,
            x.bots_ready,
            line_of(&x.stats)
        );
    }
    let _ = std::fs::create_dir_all(&opts.out);
    let _ = std::fs::write(opts.out.join("samples.csv"), csv);
    let json = json!({
        "bots": opts.bots, "secs": opts.secs, "flappers": sh.flappers, "games": opts.games,
        "acted_s": s.acted_s, "in_at_s": s.in_at,
        "requests": kinds, "delivery": delivery, "dupes": s.tally.dupes,
        "realm": {"cpu_avg_pct": avg_cpu, "cpu_peak_pct": peak_cpu, "rss_start_kb": first.rss_kb,
                  "rss_in_kb": rss_in, "rss_end_kb": rss_end, "hwm_kb": peak,
                  "node_cpu_avg_pct": node_cpu, "stats_last": last_stats},
        "windows": rows, "after": s.after, "setup_failed": s.setup_failed, "held": held,
    });
    LoadReport {
        json,
        markdown: md,
        held,
    }
}

fn line_of(m: &BTreeMap<String, String>) -> String {
    m.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_letters_and_distinct() {
        let names: BTreeSet<String> = (0..1000).map(char_of).collect();
        assert_eq!(names.len(), 1000);
        assert!(names
            .iter()
            .all(|n| n.chars().all(|c| c.is_ascii_alphabetic())));
        assert_eq!(account_of(0), "ldaaa");
        assert_eq!(char_of(27), "Ldabbx");
    }

    #[test]
    fn stamps_read_back() {
        let t = stamp("lg", 12, 7, 123_456);
        assert_eq!(parse_stamp(&t), Some(("lg", 12, 7, 123_456)));
        assert_eq!(parse_stamp("hello world"), None);
        assert_eq!(parse_stamp("lg 1 2 3 4"), None);
    }

    #[test]
    fn percentiles() {
        let mut v: Vec<u32> = (1..=100).map(|x| x * 1000).collect();
        assert_eq!(pct(&mut v, 0.5), 51.0);
        assert_eq!(pct(&mut v, 0.95), 95.0);
        assert_eq!(pct(&mut v, 1.0), 100.0);
        assert_eq!(pct(&mut [], 0.5), 0.0);
    }

    #[test]
    fn the_histogram_keeps_its_quantiles() {
        let mut h = Hist::default();
        assert_eq!(h.pct(0.5), 0.0);
        for ms in 1..=100 {
            h.add(ms);
        }
        assert_eq!(h.pct(0.5), 51.0);
        assert_eq!(h.pct(0.95), 95.0);
        let mut other = Hist::default();
        other.add(5_250);
        other.add(999_999);
        h.merge(&other);
        assert_eq!(h.pct(1.0), 60_000.0);
        assert_eq!(Hist::ms_of(Hist::bucket(5_250)), 5_200.0);
    }

    #[test]
    fn bot_addresses_stay_apart() {
        let ips: BTreeSet<IpAddr> = (0..1000).map(bot_ip).collect();
        assert_eq!(ips.len(), 1000);
        assert!(ips.iter().all(|ip| ip.is_loopback()));
    }

    #[test]
    fn the_stats_line_splits() {
        let m = stats_words("sessions=3 social=2 rooms=2 db_wait_us=40");
        assert_eq!(num(&m, "sessions"), 3);
        assert_eq!(num(&m, "db_wait_us"), 40);
        assert_eq!(num(&m, "missing"), 0);
    }
}
