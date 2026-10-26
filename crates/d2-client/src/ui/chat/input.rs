//! The input line: the typed text (its end, when longer than the box) with
//! the blinking caret while focused; empty, the command hint in grey. A
//! realm without chat says so instead.

use super::paint::{colour, ChatFont, ChatPaint};
use crate::chat::{ChatModel, Tab};
use crate::ui::geom::Rect;

/// The hint of an empty line.
pub fn hint(model: &ChatModel) -> &'static str {
    match model.open_tab_now() {
        Tab::Dm => "/w name  /r reply",
        _ if model.tabs().contains(&Tab::Dm) => "/w name   /g guild   /global",
        _ => "",
    }
}

/// Draws the line in `area`; `off`: the text shown instead of the input.
pub fn draw(
    model: &ChatModel,
    area: Rect,
    paint: &mut dyn ChatPaint,
    focused: bool,
    now_ms: u64,
    off: Option<&str>,
) {
    let row_h = paint.row_height(ChatFont::Text);
    let y = area.y + (i32::from(area.h) + row_h) / 2 - 1;
    if let Some(t) = off {
        paint.text(ChatFont::Text, area.x, y, colour::GREY, t);
        return;
    }
    let caret = if focused && (now_ms / 250) % 2 == 1 {
        "_"
    } else {
        ""
    };
    if model.input.is_empty() {
        paint.text(ChatFont::Text, area.x, y, colour::WHITE, caret);
        let x = area.x + paint.width(ChatFont::Text, "_") + 2;
        paint.text(ChatFont::Small, x, y, colour::GREY, hint(model));
        return;
    }
    let w = i32::from(area.w) - paint.width(ChatFont::Text, "_");
    let mut shown: &str = &model.input;
    while !shown.is_empty() && paint.width(ChatFont::Text, shown) > w {
        let mut cut = 1;
        while !shown.is_char_boundary(cut) {
            cut += 1;
        }
        shown = &shown[cut..];
    }
    paint.text(
        ChatFont::Text,
        area.x,
        y,
        colour::WHITE,
        &format!("{shown}{caret}"),
    );
}
