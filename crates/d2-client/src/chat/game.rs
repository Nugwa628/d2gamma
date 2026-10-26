//! The game's side of the chat in a game: the Game tab. The in-game host
//! feeds [`GameFeed`] the game stream's player lines and every game or
//! system message the screen messages showed (quests, "joined the game",
//! hostility, the game's notes, in their text colour), and sends what the
//! player says in the Game tab on the game stream. [`Merged`] puts it next
//! to the realm's rooms, so one model holds every tab.

use std::collections::VecDeque;

use super::source::{code, ChatIn, ChatOut, ChatSource, Done, Room};

/// The Game tab's lines in, the player's Game-tab lines out.
#[derive(Debug, Default)]
pub struct GameFeed {
    inbox: VecDeque<ChatIn>,
    said: VecDeque<(bool, String)>,
}

impl GameFeed {
    /// A player of the game said `text`.
    pub fn player_line(&mut self, from: &str, text: &str, at_ms: u64) {
        self.inbox.push_back(ChatIn::GameLine {
            from: from.to_string(),
            text: text.to_string(),
            at_ms,
        });
    }

    /// A game or system message in the game's text colour (0-12).
    pub fn message(&mut self, text: &str, colour: u8, at_ms: u64) {
        self.inbox.push_back(ChatIn::GameMessage {
            text: text.to_string(),
            colour,
            at_ms,
        });
    }

    /// What the player said in the Game tab, oldest first: (emote, text),
    /// for the game stream.
    pub fn take_said(&mut self) -> Vec<(bool, String)> {
        self.said.drain(..).collect()
    }
}

impl ChatSource for GameFeed {
    fn send(&mut self, out: ChatOut) {
        match out {
            ChatOut::Say {
                room: Room::Game,
                emote,
                text,
            } => self.said.push_back((emote, text)),
            // No realm: the hello stays unanswered (only the Game tab
            // shows); anything else is refused.
            ChatOut::Hello { .. } => {}
            other => self.inbox.push_back(ChatIn::Done(Done {
                request: other.kind(),
                code: code::NOT_SUPPORTED,
                ..Done::default()
            })),
        }
    }

    fn poll(&mut self) -> Option<ChatIn> {
        self.inbox.pop_front()
    }
}

/// The game's feed and the realm's source as one: Game-tab lines go to
/// the game, the rest to the realm (none: a game without a realm session,
/// only the Game tab).
pub struct Merged {
    pub game: GameFeed,
    pub realm: Option<Box<dyn ChatSource>>,
}

impl Merged {
    pub fn new(realm: Option<Box<dyn ChatSource>>) -> Merged {
        Merged {
            game: GameFeed::default(),
            realm,
        }
    }
}

impl ChatSource for Merged {
    fn send(&mut self, out: ChatOut) {
        match (&out, self.realm.as_mut()) {
            (
                ChatOut::Say {
                    room: Room::Game, ..
                },
                _,
            )
            | (_, None) => self.game.send(out),
            (_, Some(r)) => r.send(out),
        }
    }

    fn poll(&mut self) -> Option<ChatIn> {
        self.game
            .poll()
            .or_else(|| self.realm.as_mut().and_then(|r| r.poll()))
    }
}
