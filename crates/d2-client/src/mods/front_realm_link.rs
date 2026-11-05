//! The realm session a mod game's lobby screens talk to.
//!
//! The engine's online screens (login, characters, create, lobby) share one
//! session: requests go into an [`OnlineLink`](crate::ui::front_end::screens::online::OnlineLink),
//! events come back, and the shared [`OnlineState`] keeps what the realm told so
//! far (account, characters, game list, chat, the join). A mod game that
//! replaces those screens ([`super::ClientMods::with_front_screen`] on
//! `ONLINE_LOGIN`, `ONLINE_CHARS`, `ONLINE_CREATE`, `ONLINE_LOBBY`) gets the same
//! session as a [`RealmLink`] through [`super::front_hand_back::FrontStart::realm`]:
//! it sends the existing lobby protocol's requests ([`Request`]: connect, log
//! in, list and select characters, list, create and join games, chat, social
//! and ladder requests) and reads the events ([`Event`]) once per tick. No
//! protocol of its own: this is the engine's own lobby client.
//!
//! The flow is the engine's: a screen returns [`Trigger::Ok`] / `Exit` as the
//! engine's lobby screens do (login to characters to lobby; a join the realm
//! answered with [`Event::JoinOk`] and an `Ok` from the lobby start the game
//! over the network with the realm's ticket; [`Trigger::OnlineLost`] after
//! [`Event::Disconnected`]).
//!
//! [`Trigger::Ok`]: crate::ui::front_end::flow::Trigger::Ok
//! [`Trigger::OnlineLost`]: crate::ui::front_end::flow::Trigger::OnlineLost

use std::cell::Ref;

pub use crate::ui::front_end::screens::online::{
    CharInfo, Event, Failure, GameInfo, OnlineHandle, OnlineState, Request, ServerEntry,
};

/// The realm session of a front-end visit: cheap to clone, shared with the
/// engine's own online screens and the game start.
#[derive(Clone)]
pub struct RealmLink(OnlineHandle);

impl std::fmt::Debug for RealmLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.state();
        f.debug_struct("RealmLink")
            .field("connected", &s.connected)
            .field("account", &s.account)
            .finish()
    }
}

impl RealmLink {
    /// The link over the engine's online session.
    pub fn new(session: OnlineHandle) -> RealmLink {
        RealmLink(session)
    }

    /// Sends a request to the realm; its answer arrives as events.
    pub fn send(&self, request: Request) {
        self.0.borrow_mut().send(request);
    }

    /// The events that arrived since the last call, in order. The session's
    /// state has applied each one by the time it is returned (the game list,
    /// the characters, the chat lines, the join), as for the engine's screens.
    pub fn take_events(&self) -> Vec<Event> {
        self.0.borrow_mut().take_events()
    }

    /// What the realm told so far.
    pub fn state(&self) -> Ref<'_, OnlineState> {
        Ref::map(self.0.borrow(), |o| &o.state)
    }

    /// Picks the realm the next [`Request::Connect`] uses by its id in the
    /// servers list (an unknown id keeps the current one).
    pub fn select_server(&self, id: &str) {
        self.0.borrow_mut().select_server(id);
    }

    /// The engine's online session itself, for what this link does not
    /// wrap.
    pub fn session(&self) -> &OnlineHandle {
        &self.0
    }
}
