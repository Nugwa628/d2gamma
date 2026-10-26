//! The column right of the lines: in a room, "In chat (38)" and its
//! players (name, class and level), "+N more" when they don't fit; in the
//! DM tab, the conversations with their unread counts.

use super::paint::{colour, ChatFont, ChatPaint};
use crate::chat::{ChatModel, Tab};
use crate::ui::geom::Rect;

/// What a click in the column picks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    /// A player: whisper them.
    User(String),
    /// A conversation (`None`: all of them).
    Conversation(Option<String>),
}

const CLASSES: [&str; 7] = ["Ama", "Sor", "Nec", "Pal", "Bar", "Dru", "Asn"];

/// A class's short name ("Bar"); empty for none.
pub fn class_short(class: u8) -> &'static str {
    CLASSES.get(usize::from(class)).copied().unwrap_or("")
}

/// The column's rows: (text, right text, colour, unread, what a click picks).
type Row = (String, String, u8, u16, Option<Pick>);

fn column(model: &ChatModel, fit: usize) -> (String, Option<Pick>, Vec<Row>) {
    if model.open_tab_now() == Tab::Dm {
        let head = format!("DMs ({})", model.conversations.len());
        let rows = model
            .conversations
            .iter()
            .take(fit)
            .map(|c| {
                let name = if c.row.peer_char.is_empty() {
                    c.row.peer_account.clone()
                } else {
                    c.row.peer_char.clone()
                };
                let open = model.open_dm.as_deref() == Some(c.row.peer_account.as_str());
                (
                    name,
                    String::new(),
                    if open { colour::GOLD } else { colour::WHITE },
                    c.row.unread,
                    Some(Pick::Conversation(Some(c.row.peer_account.clone()))),
                )
            })
            .collect();
        return (head, Some(Pick::Conversation(None)), rows);
    }
    let u = &model.users;
    let head = if u.loaded {
        format!("In chat ({})", u.count)
    } else {
        "In chat".to_string()
    };
    let total = usize::from(u.count).max(u.rows.len());
    let more = total > fit;
    let shown = if more { fit.saturating_sub(1) } else { fit };
    let mut rows: Vec<Row> = u
        .rows
        .iter()
        .take(shown)
        .map(|r| {
            let tail = match class_short(r.who.class) {
                "" => String::new(),
                c => format!("{c} {}", r.who.level),
            };
            (
                r.who.char.clone(),
                tail,
                colour::WHITE,
                0,
                Some(Pick::User(r.who.char.clone())),
            )
        })
        .collect();
    if more {
        rows.push((
            format!("+{} more", total - shown),
            String::new(),
            colour::GREY,
            0,
            None,
        ));
    }
    (head, None, rows)
}

fn fit(area: Rect, paint: &dyn ChatPaint) -> (i32, usize) {
    let row_h = paint.row_height(ChatFont::Small).max(1) + 3;
    let n = ((i32::from(area.h) - row_h) / row_h).max(0) as usize;
    (row_h, n)
}

/// The room between a name and its class and level (a glyph's cel may
/// reach past its advance).
const NAME_GAP: i32 = 4;

/// Draws the column in `area`: the names on the left, cut with "..." where
/// they would reach their class and level, which are right-aligned at the
/// column's right edge; a conversation's unread count in a badge.
pub fn draw(model: &ChatModel, area: Rect, paint: &mut dyn ChatPaint) {
    let (row_h, n) = fit(area, paint);
    let (head, _, rows) = column(model, n);
    paint.text(ChatFont::Small, area.x, area.y + row_h, colour::GREY, &head);
    let right = area.x + i32::from(area.w);
    for (i, (name, tail, c, unread, _)) in rows.iter().enumerate() {
        let y = area.y + row_h * (i as i32 + 2);
        let mut end = right;
        if *unread > 0 {
            let (bw, bh) = super::tabs::badge_size(paint, *unread);
            end -= bw;
            let r = Rect::new(end, y + 1 - bh, bw as u16, bh as u16);
            paint.badge(r, &unread.to_string());
            end -= NAME_GAP;
        }
        if !tail.is_empty() {
            let w = paint.width(ChatFont::Small, tail);
            end -= w;
            paint.text(ChatFont::Small, end, y, colour::GREY, tail);
            end -= NAME_GAP;
        }
        let name = fit_text(paint, name, end - area.x);
        paint.text(ChatFont::Small, area.x, y, *c, &name);
    }
}

/// What a click at row `y` of the column picks.
pub fn pick(model: &ChatModel, area: Rect, paint: &dyn ChatPaint, y: i32) -> Option<Pick> {
    let (row_h, n) = fit(area, paint);
    let (_, head, rows) = column(model, n);
    let i = (y - area.y) / row_h;
    match i {
        0 => head,
        i if i >= 1 => rows.get(i as usize - 1).and_then(|r| r.4.clone()),
        _ => None,
    }
}

/// `s` cut to `w` pixels in the small font, ending in "..." when cut.
fn fit_text(paint: &dyn ChatPaint, s: &str, w: i32) -> String {
    if paint.width(ChatFont::Small, s) <= w {
        return s.to_string();
    }
    let mut out = s.to_string();
    while !out.is_empty() {
        out.pop();
        let cut = format!("{}...", out.trim_end());
        if paint.width(ChatFont::Small, &cut) <= w {
            return cut;
        }
    }
    String::new()
}
