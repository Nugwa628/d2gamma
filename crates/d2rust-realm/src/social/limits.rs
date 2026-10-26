//! The chat's rate limits (lobby contract section 9), per account:
//!
//! - `Say` and `Whisper` together: `chat_lines` per `chat_window_s` (5 per
//!   10 s, which is 30 a minute);
//! - `Say` in Global: `global_lines` per `global_window_s` (2 per 30 s),
//!   inside the limit above;
//! - the same text again in the same room within 30 s;
//! - whispers to accounts with no conversation yet: 10 per 10 minutes;
//! - the list requests (`UserList`, `DmConversations`, `DmHistory` and the
//!   guild and friends lists): 30 a minute together.
//!
//! `limits = test` divides every window by 60. A refusal is `RateLimited`
//! with its wait.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use d2_limits::{Limiter, Rate};
use d2_net::msg::social::room;
use d2_net::msg::ErrorCode;

use super::SocialConfig;
use crate::denied::Denied;

/// The window in which a room refuses the same text again.
pub const REPEAT_WINDOW: Duration = Duration::from_secs(30);

/// Whispers to new accounts per [`NEW_PEER_WINDOW_S`].
pub const NEW_PEERS: u32 = 10;
pub const NEW_PEER_WINDOW_S: u64 = 600;

/// List requests per minute.
pub const LISTS_PER_MIN: u32 = 30;

/// Every limit's buckets.
#[derive(Debug)]
pub struct ChatLimits {
    lines: Limiter<i64>,
    global: Limiter<i64>,
    new_peers: Limiter<i64>,
    lists: Limiter<i64>,
    repeat_window: Duration,
    /// The last text each account said in each room, and when.
    last_said: HashMap<(i64, u8), (String, Instant)>,
}

fn limited(wait: Duration) -> Denied {
    let r = d2_limits::Refused { wait };
    Denied::wait(ErrorCode::RATE_LIMITED, r.wait_s())
}

impl ChatLimits {
    pub fn new(cfg: &SocialConfig, test: bool) -> Self {
        let rate = |n: u32, s: u64| {
            let r = Rate::per_secs(n.max(1), s.max(1));
            if test {
                r.test()
            } else {
                r
            }
        };
        Self {
            lines: Limiter::new(rate(cfg.chat_lines.into(), cfg.chat_window_s.into())),
            global: Limiter::new(rate(cfg.global_lines.into(), cfg.global_window_s.into())),
            new_peers: Limiter::new(rate(NEW_PEERS, NEW_PEER_WINDOW_S)),
            lists: Limiter::new(rate(LISTS_PER_MIN, 60)),
            repeat_window: if test {
                REPEAT_WINDOW / 60
            } else {
                REPEAT_WINDOW
            },
            last_said: HashMap::new(),
        }
    }

    /// A `Say` of `text` in `room` by `account` at `now`: every limit of a
    /// room line passes, and the line counts.
    pub fn say(&mut self, account: i64, room: u8, text: &str, now: Instant) -> Result<(), Denied> {
        if let Some((last, at)) = self.last_said.get(&(account, room)) {
            let since = now.saturating_duration_since(*at);
            if last == text && since < self.repeat_window {
                return Err(limited(self.repeat_window - since));
            }
        }
        let global = room == room::GLOBAL;
        let mut wait = self.lines.wait(&account, now);
        if global {
            wait = wait.max(self.global.wait(&account, now));
        }
        if !wait.is_zero() {
            return Err(limited(wait));
        }
        self.lines
            .check(&account, now)
            .map_err(|r| limited(r.wait))?;
        if global {
            self.global
                .check(&account, now)
                .map_err(|r| limited(r.wait))?;
        }
        self.last_said
            .insert((account, room), (text.to_owned(), now));
        Ok(())
    }

    /// A `Whisper` by `account`; `new_peer` when the two have no
    /// conversation yet.
    pub fn whisper(&mut self, account: i64, new_peer: bool, now: Instant) -> Result<(), Denied> {
        let mut wait = self.lines.wait(&account, now);
        if new_peer {
            wait = wait.max(self.new_peers.wait(&account, now));
        }
        if !wait.is_zero() {
            return Err(limited(wait));
        }
        self.lines
            .check(&account, now)
            .map_err(|r| limited(r.wait))?;
        if new_peer {
            self.new_peers
                .check(&account, now)
                .map_err(|r| limited(r.wait))?;
        }
        Ok(())
    }

    /// A list request by `account`.
    pub fn list(&mut self, account: i64, now: Instant) -> Result<(), Denied> {
        self.lists.check(&account, now).map_err(|r| limited(r.wait))
    }

    /// The accounts each limit holds and the texts the repeat rule keeps,
    /// for the realm's `stats` line.
    pub fn stats(&self) -> String {
        format!(
            "limit_lines={} limit_global={} limit_peers={} limit_lists={} said={}",
            self.lines.len(),
            self.global.len(),
            self.new_peers.len(),
            self.lists.len(),
            self.last_said.len()
        )
    }

    /// Forgets what the repeat rule keeps of texts older than its window.
    pub fn prune(&mut self, now: Instant) {
        let w = self.repeat_window;
        self.last_said
            .retain(|_, (_, at)| now.saturating_duration_since(*at) < w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> ChatLimits {
        ChatLimits::new(&SocialConfig::default(), false)
    }

    #[test]
    fn five_lines_per_ten_seconds_across_rooms() {
        let mut l = limits();
        let t = Instant::now();
        for (i, r) in [room::MOD, room::GUILD, room::MOD, room::MOD]
            .iter()
            .enumerate()
        {
            l.say(1, *r, &format!("line {i}"), t).unwrap();
        }
        l.whisper(1, false, t).unwrap();
        let e = l.say(1, room::MOD, "six", t).unwrap_err();
        assert_eq!(e.code, ErrorCode::RATE_LIMITED);
        assert!(e.wait_s >= 1);
        assert!(l.whisper(1, false, t).is_err());
        // Another account has its own bucket; time refills it.
        l.say(2, room::MOD, "six", t).unwrap();
        l.say(1, room::MOD, "later", t + Duration::from_secs(10))
            .unwrap();
    }

    #[test]
    fn global_takes_two_per_thirty_seconds() {
        let mut l = limits();
        let t = Instant::now();
        l.say(1, room::GLOBAL, "a", t).unwrap();
        l.say(1, room::GLOBAL, "b", t).unwrap();
        let e = l.say(1, room::GLOBAL, "c", t).unwrap_err();
        assert_eq!(e.code, ErrorCode::RATE_LIMITED);
        // The refused line took nothing of the shared limit.
        for i in 0..3 {
            l.say(1, room::MOD, &format!("m{i}"), t).unwrap();
        }
        assert!(l.say(1, room::MOD, "m3", t).is_err());
        l.say(1, room::GLOBAL, "d", t + Duration::from_secs(16))
            .unwrap();
    }

    #[test]
    fn the_same_text_waits_thirty_seconds_per_room() {
        let mut l = limits();
        let t = Instant::now();
        l.say(1, room::MOD, "wts shako", t).unwrap();
        let e = l
            .say(1, room::MOD, "wts shako", t + Duration::from_secs(5))
            .unwrap_err();
        assert_eq!((e.code, e.wait_s), (ErrorCode::RATE_LIMITED, 25));
        l.say(1, room::GUILD, "wts shako", t).unwrap();
        l.say(1, room::MOD, "wts shako", t + Duration::from_secs(31))
            .unwrap();
    }

    #[test]
    fn ten_new_whisper_peers_per_ten_minutes() {
        let mut l = limits();
        let mut t = Instant::now();
        for _ in 0..10 {
            l.whisper(1, true, t).unwrap();
            t += Duration::from_secs(3);
        }
        assert!(l.whisper(1, true, t).is_err());
        // An existing conversation passes.
        l.whisper(1, false, t).unwrap();
    }

    #[test]
    fn test_limits_divide_windows_by_sixty() {
        let mut l = ChatLimits::new(&SocialConfig::default(), true);
        let t = Instant::now();
        l.say(1, room::MOD, "x", t).unwrap();
        assert!(l.say(1, room::MOD, "x", t).is_err());
        l.say(1, room::MOD, "x", t + Duration::from_millis(600))
            .unwrap();
    }
}
