//! The lines of the open tab: coloured parts per line (names gold, DMs
//! purple, `[Market]` blue, realm notices orange, guild events green, the
//! client's notes grey, game messages in their own colour), wrapped to the
//! pane, newest at the bottom.

use super::paint::{colour, ChatFont, ChatPaint};
use crate::chat::{ChatModel, Line, LineKind, Room, Tab};
use crate::ui::geom::Rect;

/// A run of text in one colour.
pub type Seg = (String, u8);

/// The speaker as lines show them: `<TAG> Name`.
pub fn speaker(l: &Line) -> String {
    match &l.who.tag {
        Some(t) => format!("<{t}> {}", l.who.char),
        None => l.who.char.clone(),
    }
}

/// "D2Gamma chat", "Global chat", "guild chat", for joined and left lines.
fn room_chat(model: &ChatModel, t: Tab) -> String {
    match t {
        Tab::Room(Room::Mod) => match model.state().map(|s| s.mod_label.as_str()) {
            Some(l) if !l.is_empty() => format!("{l} chat"),
            _ => "the chat".into(),
        },
        Tab::Room(Room::Global) => "Global chat".into(),
        Tab::Room(Room::Guild) => "guild chat".into(),
        _ => "the chat".into(),
    }
}

/// A line's coloured parts in tab `t`.
pub fn segments(model: &ChatModel, t: Tab, l: &Line) -> Vec<Seg> {
    let text = l.text.clone();
    match l.kind {
        LineKind::Said => vec![
            (format!("{}: ", speaker(l)), colour::GOLD),
            (text, colour::WHITE),
        ],
        LineKind::Emote => vec![
            (format!("{} ", speaker(l)), colour::GOLD),
            (text, colour::TAN),
        ],
        LineKind::Joined => vec![(
            format!("{} joined {}.", l.who.char, room_chat(model, t)),
            colour::GREY,
        )],
        LineKind::Left => vec![(
            format!("{} left {}.", l.who.char, room_chat(model, t)),
            colour::GREY,
        )],
        LineKind::Notice => vec![(format!("[Realm] {text}"), colour::ORANGE)],
        LineKind::Guild => vec![(text, colour::GREEN)],
        LineKind::Market => vec![("[Market] ".into(), colour::BLUE), (text, colour::WHITE)],
        LineKind::System => vec![(text, colour::GREY)],
        LineKind::Game(c) => vec![(text, c)],
        LineKind::DmIn | LineKind::DmOut => {
            let head = if l.kind == LineKind::DmIn {
                format!("{} -> You: ", l.who.char)
            } else {
                format!("You -> {}: ", l.who.char)
            };
            let mut v = vec![(head, colour::PURPLE), (text, colour::WHITE)];
            if l.stored {
                v.push((" (waited)".into(), colour::GREY));
            }
            v
        }
    }
}

/// The parts cut into rows no wider than `w`, at spaces (a word wider than
/// a row is cut where it overflows); a row's same-coloured runs merge.
pub fn wrap(segs: &[Seg], w: i32, width: &dyn Fn(&str) -> i32) -> Vec<Vec<Seg>> {
    // Words with their trailing space, in their colour.
    let mut tokens: Vec<(String, u8)> = Vec::new();
    for (s, c) in segs {
        let mut word = String::new();
        for ch in s.chars() {
            word.push(ch);
            if ch == ' ' {
                tokens.push((std::mem::take(&mut word), *c));
            }
        }
        if !word.is_empty() {
            tokens.push((word, *c));
        }
    }
    let mut rows: Vec<Vec<Seg>> = Vec::new();
    let mut row: Vec<Seg> = Vec::new();
    let mut at = 0;
    let put = |row: &mut Vec<Seg>, s: &str, c: u8| match row.last_mut() {
        Some((t, k)) if *k == c => t.push_str(s),
        _ => row.push((s.to_string(), c)),
    };
    for (tok, c) in tokens {
        let fits = at + width(tok.trim_end()) <= w;
        if fits || row.is_empty() && width(tok.trim_end()) <= w {
            put(&mut row, &tok, c);
            at += width(&tok);
            continue;
        }
        if !row.is_empty() {
            rows.push(std::mem::take(&mut row));
        }
        if width(tok.trim_end()) <= w {
            put(&mut row, &tok, c);
            at = width(&tok);
            continue;
        }
        // A word wider than the row.
        let mut piece = String::new();
        for ch in tok.chars() {
            piece.push(ch);
            if width(&piece) > w && piece.chars().count() > 1 {
                piece.pop();
                put(&mut row, &piece, c);
                rows.push(std::mem::take(&mut row));
                piece.clear();
                piece.push(ch);
            }
        }
        at = width(&piece);
        put(&mut row, &piece, c);
    }
    if !row.is_empty() || rows.is_empty() {
        rows.push(row);
    }
    for r in &mut rows {
        if let Some((t, _)) = r.last_mut() {
            let n = t.trim_end().len();
            t.truncate(n);
        }
    }
    rows
}

/// The rows the pane shows: wrapped from the newest line back, `scroll`
/// rows up (clamped), oldest first; and the most it can scroll.
pub fn rows(
    model: &ChatModel,
    t: Tab,
    area: Rect,
    paint: &dyn ChatPaint,
    scroll: usize,
) -> (Vec<Vec<Seg>>, usize) {
    let row_h = paint.row_height(ChatFont::Text).max(1);
    let fit = (i32::from(area.h) / row_h).max(1) as usize;
    let width = |s: &str| paint.width(ChatFont::Text, s);
    let lines = model.lines(t);
    // Enough rows for the page `scroll` up (and one more to know the end).
    let need = fit + scroll + 1;
    let mut back: Vec<Vec<Seg>> = Vec::new();
    let mut all = true;
    for l in lines.iter().rev() {
        let mut r = wrap(&segments(model, t, l), i32::from(area.w), &width);
        r.reverse();
        back.extend(r);
        if back.len() >= need {
            all = false;
            break;
        }
    }
    let max = if all {
        back.len().saturating_sub(fit)
    } else {
        scroll + 1
    };
    let scroll = scroll.min(max);
    let shown: Vec<Vec<Seg>> = back.into_iter().skip(scroll).take(fit).collect();
    (shown.into_iter().rev().collect(), max)
}

/// Draws the open tab's lines in `area`.
pub fn draw(model: &ChatModel, area: Rect, paint: &mut dyn ChatPaint) {
    let t = model.open_tab_now();
    let (rows, _) = rows(model, t, area, paint, model.scroll);
    let row_h = paint.row_height(ChatFont::Text);
    for (i, row) in rows.iter().enumerate() {
        let y = area.y + row_h * (i as i32 + 1);
        let mut x = area.x;
        for (s, c) in row {
            paint.text(ChatFont::Text, x, y, *c, s);
            x += paint.width(ChatFont::Text, s);
        }
    }
}
