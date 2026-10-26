// D2RUST's own: what the world view hands the minimap widget.
//! The minimap's facts of a drawn frame (`ui::automap::minimap`): the local
//! player's place, the open side panels, the automap's marker units and
//! where the game's UI was drawn. Read-only: nothing here changes the frame.

use crate::bridge::world::ClientWorld;
use crate::rules::camera::{ClientPos, OpenMode};
use crate::scene::order::pass;
use crate::scene::{DrawItem, ItemTag};
use crate::ui::automap::markers::{MarkerSubject, MarkerUnit};
use crate::ui::automap::minimap::{MinimapFacts, UiCover};
use crate::ui::ui_space::UiSpace;
use crate::ui::Point;

use super::ViewAssets;

/// The facts of a frame that drew the world around the local player at
/// `player_at` with the side panels of `mode` open; `units`: the automap's
/// marker units (`None`: the local player alone). `None` without a local
/// player. The UI cover is filled in once the frame is complete
/// ([`ui_cover`]).
pub fn facts(
    world: &ClientWorld,
    mode: OpenMode,
    player_at: ClientPos,
    units: Option<&[MarkerUnit]>,
) -> Option<MinimapFacts> {
    let player = world.local()?;
    let markers = match units {
        Some(u) => u.to_vec(),
        None => vec![MarkerUnit {
            subject: MarkerSubject::Player {
                local: true,
                mode: player.mode,
                party: -1,
                own_inventory: true,
                state_7: false,
                name: Vec::new(),
            },
            pos: player_at,
        }],
    };
    Some(MinimapFacts {
        player: player_at,
        open_mode: mode,
        markers,
        ui: UiCover::default(),
    })
}

/// Whether `item` is the game's own UI over the world: everything the UI
/// pass draws (panels, the control panel, tooltips, menus, text) and the
/// UI elements drawn among the world, except the automap's own draws.
fn covers(item: &DrawItem) -> bool {
    let automap = item.key.pass() == pass::UI && item.key.major() == pass::UI_AUTOMAP_MAJOR;
    super::native_ui::is_ui(item) && !automap
}

/// Where the UI items among `items` were drawn in the frame of `space`:
/// each item's box, cut to its clip. An item of a UI draw laid out on the
/// UI frame (`in_ui[i]` for UI draw `i`, UI Scale) is mapped from that
/// frame to the frame, where the native UI pass shows it.
pub fn ui_cover(
    items: &[DrawItem],
    assets: &ViewAssets,
    space: UiSpace,
    in_ui: &[bool],
) -> UiCover {
    let mut cover = UiCover::new(space.frame);
    for item in items.iter().filter(|i| covers(i)) {
        let Some(f) = assets.frames.frame(item.frame) else {
            continue;
        };
        let c = item.clip;
        let left = item.x.max(c.x);
        let top = item.y.max(c.y);
        let right = (item.x + f.width as i32).min(c.x + c.width as i32);
        let bottom = (item.y + f.height as i32).min(c.y + c.height as i32);
        let scaled = match item.tag {
            ItemTag::Ui(i) => in_ui.get(i as usize).copied().unwrap_or(false),
            _ => false,
        };
        if scaled && right > left && bottom > top {
            let (a, b) = (
                space.to_frame(Point::new(left, top)),
                space.to_frame(Point::new(right, bottom)),
            );
            cover.mark(a.x, a.y, b.x, b.y);
        } else {
            cover.mark(left, top, right, bottom);
        }
    }
    cover
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::{FramePart, FrameSet, FrameSetKey, IndexFrame};
    use crate::rules::camera::FrameSize;
    use crate::scene::{DrawKey, ItemTag, Rect};
    use crate::ui::automap::minimap::Place;

    fn assets_with_box(w: u32, h: u32) -> (ViewAssets, crate::scene::FrameId) {
        let mut assets = ViewAssets::new(d2_formats::palette::Palette {
            colors: [d2_formats::palette::Rgb { r: 0, g: 0, b: 0 }; 256],
        });
        let key = FrameSetKey::new("test/box", FramePart::Tile(0)).unwrap();
        let frame = IndexFrame::new(w, h, 0, 0, vec![1; (w * h) as usize]).unwrap();
        assets
            .frames
            .insert(
                key.clone(),
                FrameSet {
                    frames: vec![frame],
                },
            )
            .unwrap();
        let id = assets.id(&key, 0).unwrap();
        (assets, id)
    }

    fn item(id: crate::scene::FrameId, x: i32, y: i32, major: u32, tag: ItemTag) -> DrawItem {
        let mut i = DrawItem::new(id, x, y);
        i.clip = FrameSize::PLAY.rect();
        i.key = DrawKey::new(pass::UI, major, 0, 0).unwrap();
        i.tag = tag;
        i
    }

    /// A tooltip reaching into the widget's box covers it; the automap's
    /// own draws and the world never do, and a UI item clipped away is not
    /// counted.
    #[test]
    fn only_the_games_ui_covers_the_minimap() {
        let (assets, id) = assets_with_box(40, 30);
        let plain = UiSpace::identity(FrameSize::PLAY);
        let place = Place {
            left: 1030,
            top: 10,
            side: 240,
        };
        let tooltip = item(id, 1000, 20, pass::UI_PANELS_MAJOR, ItemTag::Ui(1));
        assert!(ui_cover(&[tooltip], &assets, plain, &[]).covers(&place));
        let left_of_it = item(id, 980, 20, pass::UI_PANELS_MAJOR, ItemTag::Ui(1));
        assert!(!ui_cover(&[left_of_it], &assets, plain, &[]).covers(&place));
        let automap = item(id, 1100, 50, pass::UI_AUTOMAP_MAJOR, ItemTag::Ui(2));
        assert!(!ui_cover(&[automap], &assets, plain, &[]).covers(&place));
        let mut world = DrawItem::new(id, 1100, 50);
        world.clip = FrameSize::PLAY.rect();
        world.key = DrawKey::new(pass::WALLS_UNITS, 0, 0, 0).unwrap();
        world.tag = ItemTag::Unit(7);
        assert!(!ui_cover(&[world], &assets, plain, &[]).covers(&place));
        let mut clipped = item(id, 1100, 50, pass::UI_PANELS_MAJOR, ItemTag::Ui(3));
        clipped.clip = Rect::new(0, 0, 400, 400);
        assert!(!ui_cover(&[clipped], &assets, plain, &[]).covers(&place));
    }

    /// Under UI Scale a draw laid out on the UI frame is shown over the
    /// whole frame, so its box is mapped from the UI frame: at 125 % the
    /// right panel's first UI column (704 of 1024) is frame column 880, and
    /// the widget left of it is not covered; a draw in frame space is not
    /// mapped.
    #[test]
    fn a_ui_scale_draw_covers_the_frame_where_it_is_shown() {
        let (assets, id) = assets_with_box(40, 30);
        let space = UiSpace {
            frame: FrameSize::PLAY,
            ui: FrameSize {
                width: 1024,
                height: 576,
            },
        };
        let panel = item(id, 704, 20, pass::UI_PANELS_MAJOR, ItemTag::Ui(1));
        let left_of_panel = Place {
            left: 630,
            top: 10,
            side: 240,
        };
        let under_panel = Place {
            left: 880,
            top: 25,
            side: 8,
        };
        let scaled = ui_cover(std::slice::from_ref(&panel), &assets, space, &[false, true]);
        assert!(!scaled.covers(&left_of_panel));
        assert!(scaled.covers(&under_panel));
        // The same box as a frame-space draw stays where it is.
        let frame_space = ui_cover(&[panel], &assets, space, &[false, false]);
        assert!(frame_space.covers(&left_of_panel));
        assert!(!frame_space.covers(&Place {
            left: 750,
            top: 10,
            side: 8,
        }));
    }
}
