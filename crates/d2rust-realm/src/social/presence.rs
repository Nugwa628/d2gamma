//! Who is where (lobby contract sections 2 and 11): every logged-in
//! session with its account, its selected character, the game it is in
//! and its control stream, kept from its login (or resume) to its end.
//!
//! The network side tells it what changes ([`Presence::seat`],
//! [`Presence::set_game`], [`Presence::leave`]); the chat reads it for
//! room members, user lists and pushes by account; the friends read it for
//! a friend's place.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard};

use d2_formats::d2s::status;
use d2_net::msg::account::CharSummary;
use d2_net::msg::social::{place, who_flags, Who, NO_CLASS};

use crate::accounts::AccountId;
use crate::session::{ControlTx, SessionId};

use super::Member;

/// One logged-in session as the chat sees it.
#[derive(Clone, Debug)]
pub struct Here {
    pub session: SessionId,
    pub account: AccountId,
    /// The account's stored name.
    pub account_name: String,
    /// The selected character.
    pub character: Option<CharSummary>,
    /// The game it is in or holds a seat in.
    pub game: Option<u32>,
    pub out: ControlTx,
}

impl Here {
    /// [`place`]: in a game or in the lobby (character select too).
    pub fn place(&self) -> u8 {
        if self.game.is_some() {
            place::IN_GAME
        } else {
            place::LOBBY
        }
    }

    /// The session as lines and lists show it: its character, with the
    /// guild `tag`; `account` only when the asker may see it.
    pub fn who(&self, tag: Option<&str>, show_account: bool) -> Who {
        let mut w = match &self.character {
            Some(c) => who_of(c),
            None => Who {
                class: NO_CLASS,
                ..Who::default()
            },
        };
        if self.game.is_some() {
            w.flags |= who_flags::IN_GAME;
        }
        w.tag = tag.map(str::to_owned);
        if show_account {
            w.account = self.account_name.clone();
        }
        w
    }
}

/// A character as a [`Who`]: name, class, level and the status flags.
pub fn who_of(c: &CharSummary) -> Who {
    let mut flags = 0;
    for (bit, flag) in [
        (status::HARDCORE, who_flags::HARDCORE),
        (status::DEAD, who_flags::DEAD),
        (status::EXPANSION, who_flags::EXPANSION),
        (status::LADDER, who_flags::LADDER),
    ] {
        if c.status & bit != 0 {
            flags |= flag;
        }
    }
    Who {
        char: c.name.clone(),
        account: String::new(),
        tag: None,
        class: if c.class < 7 { c.class } else { NO_CLASS },
        level: c.level,
        flags,
    }
}

/// What a [`Presence`] change did to its account.
#[derive(Clone, Debug)]
pub struct Changed {
    /// The session after the change (before it, for a leave).
    pub here: Here,
    /// Its account had no other session before (a seat) or has none left
    /// (a leave).
    pub account_edge: bool,
    /// What it was before: its game (for [`Presence::set_game`]) or its
    /// character (for [`Presence::seat`]).
    pub game_before: Option<u32>,
    pub character_before: Option<CharSummary>,
}

#[derive(Debug, Default)]
struct Inner {
    sessions: BTreeMap<SessionId, Here>,
    by_account: BTreeMap<i64, BTreeSet<SessionId>>,
}

/// Every logged-in session.
#[derive(Debug, Default)]
pub struct Presence {
    inner: Mutex<Inner>,
}

impl Presence {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A session logged in, came back or changed its character: its entry
    /// now. `None` when it is the same as before (nothing to tell).
    pub fn seat(&self, m: &Member) -> Option<Changed> {
        let mut st = self.lock();
        let before = st.sessions.get(&m.session).cloned();
        let game = before.as_ref().and_then(|h| h.game);
        let here = Here {
            session: m.session,
            account: m.account,
            account_name: m.account_name.clone(),
            character: m.character.clone(),
            game,
            out: m.out.clone(),
        };
        let sessions = st.by_account.entry(m.account.0).or_default();
        let account_edge = sessions.is_empty();
        sessions.insert(m.session);
        st.sessions.insert(m.session, here.clone());
        let character_before = before.as_ref().and_then(|h| h.character.clone());
        if before.is_some() && character_before == here.character {
            return None;
        }
        Some(Changed {
            here,
            account_edge,
            game_before: game,
            character_before,
        })
    }

    /// The session's game now (`None`: in the lobby); the change, if it
    /// changed.
    pub fn set_game(&self, session: SessionId, game: Option<u32>) -> Option<Changed> {
        let mut st = self.lock();
        let h = st.sessions.get_mut(&session)?;
        if h.game == game {
            return None;
        }
        let game_before = std::mem::replace(&mut h.game, game);
        Some(Changed {
            here: h.clone(),
            account_edge: false,
            game_before,
            character_before: h.character.clone(),
        })
    }

    /// The session ended; what it was.
    pub fn leave(&self, session: SessionId) -> Option<Changed> {
        let mut st = self.lock();
        let here = st.sessions.remove(&session)?;
        let account_edge = match st.by_account.get_mut(&here.account.0) {
            Some(s) => {
                s.remove(&session);
                let empty = s.is_empty();
                if empty {
                    st.by_account.remove(&here.account.0);
                }
                empty
            }
            None => true,
        };
        Some(Changed {
            game_before: here.game,
            character_before: here.character.clone(),
            here,
            account_edge,
        })
    }

    /// Session `session`, while it is logged in.
    pub fn get(&self, session: SessionId) -> Option<Here> {
        self.lock().sessions.get(&session).cloned()
    }

    /// Every session of `account`.
    pub fn sessions_of(&self, account: AccountId) -> Vec<Here> {
        let st = self.lock();
        st.by_account
            .get(&account.0)
            .into_iter()
            .flatten()
            .filter_map(|s| st.sessions.get(s).cloned())
            .collect()
    }

    /// Whether `account` has a session.
    pub fn online(&self, account: AccountId) -> bool {
        self.lock().by_account.contains_key(&account.0)
    }

    /// Where `account` is ([`place`]: offline, lobby or in a game; a game
    /// counts when any of its sessions is in one) and that game.
    pub fn place_of(&self, account: AccountId) -> (u8, Option<u32>) {
        let sessions = self.sessions_of(account);
        if sessions.is_empty() {
            return (place::OFFLINE, None);
        }
        match sessions.iter().find_map(|h| h.game) {
            Some(g) => (place::IN_GAME, Some(g)),
            None => (place::LOBBY, None),
        }
    }

    /// Every logged-in session.
    pub fn all(&self) -> Vec<Here> {
        self.lock().sessions.values().cloned().collect()
    }

    /// Sends `body` to every session of `account`; how many.
    pub fn send_to(&self, account: AccountId, body: &[u8]) -> usize {
        let sessions = self.sessions_of(account);
        for h in &sessions {
            h.out.send(body.to_vec());
        }
        sessions.len()
    }

    /// How many sessions are logged in.
    pub fn len(&self) -> usize {
        self.lock().sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().sessions.is_empty()
    }
}
