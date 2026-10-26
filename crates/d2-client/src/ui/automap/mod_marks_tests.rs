//! A mod game's star marks and labels in its own font and place; a
//! plain mark draws as the unit markers do.

use super::*;
use crate::bridge::world::ClientWorld;
use crate::mods::ui_hooks::MapMarks;
use crate::mods::ClientMods;
use crate::ui::automap::markers::{name, MarkerCtx, MarkerPalette};
use crate::ui::automap::view::Bounds;

/// A four-point star.
const STAR: [(i32, i32); 5] = [(0, -4), (4, 0), (0, 4), (-4, 0), (0, -4)];

/// A mod game's marks: a star labelled in font 1, 16 rows up, and a
/// plain cross with a label.
struct Stars;

impl MapMarks for Stars {
    fn marks(&self, _world: &ClientWorld, out: &mut Vec<MapMark>) {
        let wide = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        out.push(
            MapMark::cross(ClientPos { x: 160, y: 320 }, (255, 0, 0))
                .with_shape(MarkShape::Path(STAR.to_vec()))
                .with_label(wide("Waypoint"), 4, 1, -16),
        );
        let mut plain = MapMark::cross(ClientPos { x: 400, y: 400 }, (0, 255, 0));
        plain.label = wide("NPC");
        plain.label_color = 2;
        out.push(plain);
    }
}

fn ctx() -> MarkerCtx {
    MarkerCtx {
        local_party: -1,
        party: false,
        names: false,
        player_gate: false,
        mini: false,
        div: 10,
        a: (0, 0),
        rect: Bounds {
            left: 0,
            top: 0,
            right: 799,
            bottom: 599,
        },
        palette: MarkerPalette::default(),
    }
}

/// The marks the client draws for `Stars`, colours 7 and 8.
fn draws() -> Vec<AutomapDraw> {
    let mods = ClientMods::new().with_map_marks(Stars);
    let marks: Vec<ModMark> = mods
        .map_marks(&ClientWorld::default())
        .into_iter()
        .zip([7, 8])
        .map(|(m, c)| ModMark::of(m, Some(c)))
        .collect();
    let mut out = Vec::new();
    mod_marks(&marks, &ctx(), &mut out);
    out
}

/// The star's segments at the mark's point in its colour, then its label
/// in font 1, 16 rows up.
#[test]
fn a_star_mark_draws_its_segments_and_its_label() {
    let out = draws();
    // (160 / 10 + 8, 320 / 10 − 8).
    let (x, y) = (24, 24);
    let lines: Vec<AutomapDraw> = STAR
        .windows(2)
        .map(|p| AutomapDraw::Line {
            from: (x + p[0].0, y + p[0].1),
            to: (x + p[1].0, y + p[1].1),
            color: 7,
        })
        .collect();
    assert_eq!(out[..4], lines[..]);
    assert_eq!(
        out[4],
        AutomapDraw::Text {
            text: Label::Text("Waypoint".encode_utf16().collect()),
            font: 1,
            color: 4,
            x,
            y: y - 16,
            align: TextAlign::Centre,
        }
    );
}

/// A plain mark draws the unit markers' cross and its label in the
/// marker names' font and place (as before shapes and label options).
#[test]
fn a_plain_mark_draws_as_the_unit_markers() {
    let out = draws();
    // (400 / 10 + 8, 400 / 10 − 8).
    let (x, y) = (48, 32);
    let mut want = Vec::new();
    cross(x, y, false, 8, &mut want);
    name(
        &"NPC".encode_utf16().collect::<Vec<u16>>(),
        x,
        y,
        2,
        &mut want,
    );
    assert_eq!(out[5..], want[..]);
}
