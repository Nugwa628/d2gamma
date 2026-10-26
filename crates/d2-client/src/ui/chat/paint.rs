//! What the chat view needs from its host: text in the game's fonts and
//! colours, a dark box and a thin gold outline. The lobby draws through
//! the front end's items, the game through its own UI.

use crate::ui::geom::Rect;

/// The chat's two fonts: the line font and the small font of the user
/// column, badges and hints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatFont {
    Text,
    Small,
}

/// The game's text colours (the `k` of its text drawing).
pub mod colour {
    pub const WHITE: u8 = 0;
    pub const RED: u8 = 1;
    pub const GREEN: u8 = 2;
    pub const BLUE: u8 = 3;
    pub const GOLD: u8 = 4;
    pub const GREY: u8 = 5;
    pub const TAN: u8 = 7;
    pub const ORANGE: u8 = 8;
    pub const YELLOW: u8 = 9;
    pub const PURPLE: u8 = 11;
}

pub trait ChatPaint {
    /// The width of `text` in `font`.
    fn width(&self, font: ChatFont, text: &str) -> i32;
    /// A row's height in `font`.
    fn row_height(&self, font: ChatFont) -> i32;
    /// `text` with the pen at (x, bottom y).
    fn text(&mut self, font: ChatFont, x: i32, y: i32, colour: u8, text: &str);
    /// A dark box.
    fn panel(&mut self, r: Rect);
    /// A 1 px gold outline (a 1 px wide rect: a divider).
    fn outline(&mut self, r: Rect);
    /// An unread count's badge filling `r` (its baseline the small font's
    /// on `r`'s bottom less 1). By default a dark box with a gold outline
    /// and the count in red; a host that can draw the round red badge of
    /// the approved preview does that.
    fn badge(&mut self, r: Rect, count: &str) {
        self.panel(r);
        self.outline(r);
        let x = r.x + (i32::from(r.w) - self.width(ChatFont::Small, count)) / 2;
        self.text(
            ChatFont::Small,
            x,
            r.y + i32::from(r.h) - 1,
            colour::RED,
            count,
        );
    }
}
