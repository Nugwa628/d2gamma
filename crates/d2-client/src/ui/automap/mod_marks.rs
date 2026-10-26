//! A mod game's own automap marks (`crate::mods::ClientMods::with_map_marks`):
//! a shape (the markers' cross or line segments of its own), an icon and
//! a label at a world position, drawn after the unit markers in the
//! marker rectangle, placed as the unit markers are.

use super::draw::{AutomapDraw, Label, TextAlign};
use super::markers::{cross, MarkerCtx};
use crate::mods::ui_hooks::{MapMark, MarkShape};
use crate::rules::camera::ClientPos;

/// One mark, its colour already a palette index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModMark {
    /// The world position, in client pixels (as a unit marker's).
    pub pos: ClientPos,
    /// The shape's palette index; `None`: no shape.
    pub cross: Option<u8>,
    /// An icon centred on the mark: a DC6 file (no extension) and its
    /// frame.
    pub icon: Option<(String, u32)>,
    /// A label above the mark (UTF-16; empty: none), in the marker
    /// names' font, colour `label_color`.
    pub label: Vec<u16>,
    pub label_color: u16,
    /// The shape drawn in the `cross` colour.
    pub shape: MarkShape,
    /// The label's font and its top's rows from the mark.
    pub label_font: u16,
    pub label_dy: i32,
}

impl ModMark {
    /// Mark `m` with its colour matched: `cross`.
    pub fn of(m: MapMark, cross: Option<u8>) -> ModMark {
        ModMark {
            pos: m.at,
            cross,
            icon: m.icon,
            label: m.label,
            label_color: m.label_color,
            shape: m.shape,
            label_font: m.label_font,
            label_dy: m.label_dy,
        }
    }
}

/// The draws of `marks`, in order: those whose place lies in the marker
/// rectangle.
pub fn mod_marks(marks: &[ModMark], ctx: &MarkerCtx, out: &mut Vec<AutomapDraw>) {
    for m in marks {
        let (x, y) = super::markers::marker_screen(m.pos, ctx);
        if !ctx.rect.contains(x, y) {
            continue;
        }
        if let Some((path, frame)) = &m.icon {
            out.push(AutomapDraw::Icon {
                path: path.clone(),
                frame: *frame,
                x,
                y,
                clip: ctx.rect,
            });
        }
        if let Some(c) = m.cross {
            match &m.shape {
                MarkShape::Cross => cross(x, y, ctx.mini, c, out),
                MarkShape::Path(points) => {
                    for p in points.windows(2) {
                        out.push(AutomapDraw::Line {
                            from: (x + p[0].0, y + p[0].1),
                            to: (x + p[1].0, y + p[1].1),
                            color: c,
                        });
                    }
                }
            }
        }
        if !m.label.is_empty() {
            out.push(AutomapDraw::Text {
                text: Label::Text(m.label.clone()),
                font: m.label_font,
                color: m.label_color,
                x,
                y: y + m.label_dy,
                align: TextAlign::Centre,
            });
        }
    }
}

#[cfg(test)]
#[path = "mod_marks_tests.rs"]
mod tests;
