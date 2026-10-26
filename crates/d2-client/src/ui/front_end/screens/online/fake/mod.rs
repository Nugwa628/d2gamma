//! An in-memory server behind [`OnlineLink`] for the online screens' tests
//! and offscreen captures. It answers with the contract's rules (accounts,
//! characters, the lobby's password, difficulty, kind and level rules; the
//! realm lobby's rooms, DMs, guilds, friends and ladder), so a screen test
//! drives the same answers the real server gives. It is not the server: no
//! network, no persistence, no rate limits.
//!
//! - [`lobby`]: accounts, characters and the game list.
//! - [`social`]: chat rooms, DMs, user lists, guilds and friends, seeded
//!   with a realm's worth of players (37 others online, a guild of three,
//!   friends in each place, two DM conversations).
//! - [`ladder`]: seasons and 60 ranked characters (plus a few hardcore
//!   and classic ones).

pub mod ladder;
pub mod lobby;
pub mod social;

use std::collections::VecDeque;

use d2_formats::d2s::status;

pub use ladder::{FakeLadder, LadderChar};
pub use lobby::{account_name_ok, game_name_ok, join_rule};
pub use social::{FakeGuild, FakeSocial, Player, StoredDm};

use super::link::{
    who_flag, Asked, CharInfo, ErrorCode, Event, Failure, GameInfo, NodeInfo, OnlineLink,
    PingTable, Request, Who,
};

#[derive(Clone, Debug)]
struct Account {
    name: String,
    password: String,
    chars: Vec<CharInfo>,
}

#[derive(Clone, Debug)]
struct Game {
    info: GameInfo,
    password: String,
}

/// The fake realm's name.
pub const FAKE_REALM: &str = "D2Gamma Test";

/// The fake realm's clock at the start (2026-10-23 12:00 UTC, Unix ms):
/// seeded lines, DMs and "last seen" times are before it.
pub const FAKE_NOW_MS: u64 = 1_792_756_800_000;

/// The fake server's state; every field is open so a test can set up a
/// scene (games already running, a character with Hell unlocked).
#[derive(Clone, Debug)]
pub struct FakeServer {
    pub server_name: String,
    pub registration_open: bool,
    /// Every request is answered with this failure (0: none); `Connect`
    /// fails with it as `Disconnected`.
    pub refuse_connect: Option<Failure>,
    /// Chat answers `NotSupported`.
    pub chat_off: bool,
    /// Requests are answered only when [`FakeServer::release`] is called
    /// (captures of the "please wait" box).
    pub hold: bool,
    pub max_characters: u8,
    /// The official game servers it lists after a character is selected
    /// (none: a main server without game servers, the v1 screens), and the
    /// pings the client "measured" to them (a stub list until the main
    /// server sends one).
    pub nodes: Vec<NodeInfo>,
    pub pings: PingTable,
    /// Whether "Host on my PC" is allowed.
    pub hosting: bool,
    /// The realm's clock (Unix ms): lines, DMs and games are stamped with
    /// it; a test moves it.
    pub now_ms: u64,
    /// Chat rooms, DMs, user lists, guilds and friends.
    pub social: FakeSocial,
    pub ladder: FakeLadder,
    /// A realm without the social areas (rooms, DMs, guilds, friends) or
    /// without the ladder: their requests answer `NotSupported`.
    pub social_off: bool,
    pub ladder_off: bool,
    accounts: Vec<Account>,
    games: Vec<Game>,
    next_game: u32,
    connected: bool,
    me: Option<usize>,
    selected: Option<CharInfo>,
    in_lobby: bool,
    held: VecDeque<Request>,
    out: VecDeque<Event>,
    /// Every request seen, in order (tests read it).
    pub seen: Vec<Request>,
}

impl Default for FakeServer {
    fn default() -> Self {
        FakeServer {
            server_name: FAKE_REALM.into(),
            registration_open: true,
            refuse_connect: None,
            chat_off: false,
            hold: false,
            max_characters: 16,
            nodes: Vec::new(),
            pings: PingTable::new(),
            hosting: true,
            now_ms: FAKE_NOW_MS,
            social: FakeSocial::seeded(FAKE_NOW_MS),
            ladder: FakeLadder::seeded(FAKE_NOW_MS),
            social_off: false,
            ladder_off: false,
            accounts: Vec::new(),
            games: Vec::new(),
            next_game: 1,
            connected: false,
            me: None,
            selected: None,
            in_lobby: false,
            held: VecDeque::new(),
            out: VecDeque::new(),
            seen: Vec::new(),
        }
    }
}

/// A character for a scene: `progression` sets the unlocked difficulties.
pub fn character(name: &str, class: u8, level: u8, progression: u8, flags: u16) -> CharInfo {
    let st = (u16::from(progression) << 8) | flags;
    CharInfo {
        name: name.into(),
        class,
        level,
        status: st,
        difficulties: status::difficulties(st),
        look: vec![0xFF; 32],
        last_played: 0,
        season: 0,
    }
}

fn fail(code: ErrorCode, text: &str) -> Event {
    Event::Error(Failure {
        code,
        wait_s: 0,
        text: text.into(),
    })
}

fn refused(asked: Asked, code: ErrorCode, text: &str) -> Event {
    Event::Refused {
        asked,
        failure: Failure {
            code,
            wait_s: 0,
            text: text.into(),
        },
    }
}

impl FakeServer {
    /// An account that exists before the test logs in.
    pub fn add_account(&mut self, name: &str, password: &str, chars: Vec<CharInfo>) {
        self.accounts.push(Account {
            name: name.into(),
            password: password.into(),
            chars,
        });
    }

    /// The logged-in account's name.
    fn my_account(&self) -> Option<&str> {
        self.me
            .and_then(|i| self.accounts.get(i))
            .map(|a| a.name.as_str())
    }

    /// The logged-in account's characters.
    fn my_chars(&self) -> &[CharInfo] {
        self.me
            .and_then(|i| self.accounts.get(i))
            .map_or(&[], |a| a.chars.as_slice())
    }

    /// The session's player as lines and lists show it: the selected
    /// character (or none, at character select) with the guild's tag.
    fn me_who(&self) -> Who {
        let c = self.selected.as_ref();
        let st = c.map_or(0, |c| c.status);
        let mut flags = 0;
        for (bit, on) in [
            (who_flag::HARDCORE, st & status::HARDCORE != 0),
            (who_flag::DEAD, st & status::DEAD != 0),
            (who_flag::EXPANSION, st & status::EXPANSION != 0),
            (who_flag::LADDER, c.is_some_and(|c| c.season != 0)),
            (who_flag::IN_GAME, false),
        ] {
            if on {
                flags |= bit;
            }
        }
        Who {
            char: c.map(|c| c.name.clone()).unwrap_or_default(),
            account: self.my_account().unwrap_or_default().to_string(),
            tag: self.social.my_guild().map(|g| g.tag.clone()),
            class: c.map_or(255, |c| c.class),
            level: c.map_or(0, |c| c.level),
            flags,
        }
    }

    /// Answer the held requests.
    pub fn release(&mut self) {
        self.hold = false;
        while let Some(r) = self.held.pop_front() {
            self.answer(r);
        }
    }

    fn answer(&mut self, r: Request) {
        if let Some(f) = &self.refuse_connect {
            self.out.push_back(Event::Disconnected(f.clone()));
            return;
        }
        match r.asked() {
            Some(asked) => self.answer_realm_lobby(asked, r),
            None => self.answer_lobby(r),
        }
    }

    /// The realm lobby's requests: a realm without the area answers
    /// `NotSupported`; every one needs a login (a selected character only
    /// to speak in a room).
    fn answer_realm_lobby(&mut self, asked: Asked, r: Request) {
        let ladder = matches!(asked, Asked::LadderSeasons | Asked::LadderQuery);
        let off = if ladder {
            self.ladder_off
        } else {
            self.social_off
        };
        if off {
            self.out
                .push_back(refused(asked, ErrorCode::NOT_SUPPORTED, ""));
        } else if self.me.is_none() {
            self.out
                .push_back(refused(asked, ErrorCode::NOT_LOGGED_IN, ""));
        } else if ladder {
            self.answer_ladder(asked, r);
        } else {
            self.answer_social(asked, r);
        }
    }
}

impl OnlineLink for FakeServer {
    fn send(&mut self, r: Request) {
        self.seen.push(r.clone());
        if self.hold {
            self.held.push_back(r);
        } else {
            self.answer(r);
        }
    }

    fn poll(&mut self) -> Option<Event> {
        self.out.pop_front()
    }
}

/// A [`FakeServer`] shared between a test and the screens' link.
#[derive(Clone, Default)]
pub struct SharedFake(pub std::rc::Rc<std::cell::RefCell<FakeServer>>);

impl OnlineLink for SharedFake {
    fn send(&mut self, r: Request) {
        self.0.borrow_mut().send(r);
    }

    fn poll(&mut self) -> Option<Event> {
        self.0.borrow_mut().poll()
    }
}

#[cfg(test)]
mod tests;
