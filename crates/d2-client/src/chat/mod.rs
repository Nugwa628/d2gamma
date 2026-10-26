//! The one chat of the lobby and the game (`docs/plans/lobby-ui.md`, L9;
//! the wire terms of `docs/plans/server/lobby-contract.md`, section 3).
//!
//! - [`model`]: tabs (Game in a game; Global, Mod, Guild with a guild; DM),
//!   lines, unread counts, DM conversations, the open room's user list, the
//!   typed line and its history.
//! - [`commands`]: `/w`, `/r`, `/g`, `/global`, `/m`, `/me`, `/ignore`,
//!   `/unignore`, `/f add`, `/f remove`.
//! - [`source`]: what goes to and comes from a [`ChatSource`]:
//!   [`RealmChat`] (the realm), [`GameFeed`] (the game's player lines and
//!   game messages, the Game tab), [`Merged`] (both, in a game) and
//!   [`FakeChat`] (tests and captures).
//!
//! The view is `ui::chat`; the lobby draws it in its chat pane
//! (`screens::online::chat_pane`), the game in its chat window.

pub mod commands;
pub mod fake;
pub mod game;
pub mod model;
pub mod realm;
pub mod source;

pub use fake::FakeChat;
pub use game::{GameFeed, Merged};
pub use model::{ChatModel, Host, Line, LineKind, Tab};
pub use realm::RealmChat;
pub use source::{ChatIn, ChatOut, ChatSource, Room};

#[cfg(test)]
mod tests;
