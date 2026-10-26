//! The chat's view, the same in the lobby and the game: the tab strip with
//! unread badges, the open tab's lines, the column of the open room's
//! players (or the DM conversations) and the input line, drawn through a
//! host's [`ChatPaint`] in the game's fonts and colours. The host places it
//! with a [`ChatLayout`] and routes clicks through [`hit`].

pub mod input;
pub mod lines;
pub mod paint;
pub mod tabs;
pub mod users;

pub use paint::{colour, ChatFont, ChatPaint};

use crate::chat::{ChatModel, Tab};
use crate::ui::geom::{Point, Rect};

/// The DM tab's empty state (lobby contract section 10).
pub const DM_KEPT_NOTE: &str = "Messages are kept on the realm for 30 days.";

/// Where the parts go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatLayout {
    /// The tab strip.
    pub tabs: Rect,
    /// The lines and, at its right, the column.
    pub body: Rect,
    /// The column's width (with its divider).
    pub column_w: u16,
    /// The input line.
    pub input: Rect,
}

/// What a click hit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hit {
    Tab(Tab),
    /// A player in the column: whisper them.
    User(String),
    /// A DM conversation (`None`: all of them).
    Conversation(Option<String>),
    Input,
    /// The lines.
    Lines,
}

impl ChatLayout {
    /// The lines' area and the column's (none when the open tab has no
    /// column).
    pub fn split(&self, model: &ChatModel) -> (Rect, Option<Rect>) {
        let column = match model.open_tab_now() {
            Tab::Dm => true,
            Tab::Room(crate::chat::Room::Game) => false,
            Tab::Room(_) => model.users_ok(),
        };
        if !column || self.column_w == 0 {
            return (self.body, None);
        }
        let b = self.body;
        let cw = i32::from(self.column_w);
        let lines = Rect::new(b.x, b.y, (i32::from(b.w) - cw).max(0) as u16, b.h);
        let col = Rect::new(b.x + i32::from(b.w) - cw + 8, b.y, (cw - 8) as u16, b.h);
        (lines, Some(col))
    }
}

/// Draws the chat: `focused` blinks the caret; `off` replaces the input
/// line (a realm without chat).
pub fn draw(
    model: &ChatModel,
    layout: &ChatLayout,
    paint: &mut dyn ChatPaint,
    focused: bool,
    now_ms: u64,
    off: Option<&str>,
) {
    tabs::draw(model, layout.tabs, paint);
    let (body, column) = layout.split(model);
    if let Some(c) = column {
        paint.outline(Rect::new(c.x - 4, c.y + 2, 1, c.h.saturating_sub(4)));
        users::draw(model, c, paint);
    }
    if model.open_tab_now() == Tab::Dm && model.lines(Tab::Dm).is_empty() {
        let y = body.y + paint.row_height(ChatFont::Text);
        paint.text(ChatFont::Small, body.x, y, colour::GREY, DM_KEPT_NOTE);
    }
    lines::draw(model, body, paint);
    input::draw(model, layout.input, paint, focused, now_ms, off);
}

/// What is at `p`.
pub fn hit(model: &ChatModel, layout: &ChatLayout, paint: &dyn ChatPaint, p: Point) -> Option<Hit> {
    if layout.tabs.contains(p) {
        return tabs::boxes(model, layout.tabs, paint)
            .into_iter()
            .find(|b| b.at.contains(p))
            .map(|b| Hit::Tab(b.tab));
    }
    if layout.input.contains(p) {
        return Some(Hit::Input);
    }
    let (body, column) = layout.split(model);
    if let Some(c) = column.filter(|c| c.contains(p)) {
        return users::pick(model, c, paint, p.y).map(|k| match k {
            users::Pick::User(n) => Hit::User(n),
            users::Pick::Conversation(c) => Hit::Conversation(c),
        });
    }
    body.contains(p).then_some(Hit::Lines)
}

/// Applies a click to the model: a tab opens, a player gets a whisper
/// line, a conversation opens; `true` when the click was the chat's.
pub fn click(model: &mut ChatModel, h: &Hit) -> bool {
    match h {
        Hit::Tab(t) => model.open_tab(*t),
        Hit::User(n) => model.whisper_to(n),
        Hit::Conversation(c) => model.open_conversation(c.as_deref()),
        Hit::Input | Hit::Lines => {}
    }
    true
}

#[cfg(test)]
mod tests;
