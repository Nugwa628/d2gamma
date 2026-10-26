use super::*;
use crate::ui::float::Scene;
use crate::ui::front_end::screens::online::chat_pane;
use crate::ui::front_end::screens::online::lobby::{BG, PANEL};
use crate::ui::front_end::Action;

fn key(account: &str) -> Option<LayoutKey> {
    Some(LayoutKey::new("eu1", account, Scene::Lobby))
}

/// A fresh config folder for one test.
fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("d2rust-lobby-layout-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Drags from `from` to `to` and lets go.
fn drag(l: &mut LobbyLayout, from: Point, to: Point) -> Option<String> {
    assert!(l.press(from, true), "nothing taken at {from:?}");
    l.pointer(to);
    l.release(to)
}

/// The chat's title strip (its tab row) and the right panel's (over its
/// buttons).
const CHAT_TITLE: Point = Point::new(300, 165);
const RIGHT_TITLE: Point = Point::new(600, 258);

/// Rule (plan L15): the classic rects are where the art frames the panes:
/// the chat lays out exactly as before and the frames a moved panel draws
/// are the painted ones.
#[test]
fn the_classic_rects_are_the_arts_frames() {
    assert_eq!(chat_layout(CHAT_CLASSIC), chat_pane::LAYOUT);
    let chat = panel_frames(CHAT_ID, CHAT_CLASSIC);
    assert_eq!(chat[0], (CHAT_ZONE, Style::Knot));
    assert_eq!(chat[1], (Rect::new(118, 477, 280, 25), Style::Rope));
    assert_eq!(
        panel_frames(RIGHT_ID, RIGHT_CLASSIC),
        vec![(RIGHT_ZONE, Style::Rope)]
    );
    // The right panel holds the list's pane and its buttons.
    let inside = |outer: Rect, r: Rect| {
        r.x >= outer.x
            && r.y >= outer.y
            && r.right() <= outer.right()
            && r.bottom() <= outer.bottom()
    };
    assert!(inside(RIGHT_CLASSIC, PANEL));
    assert!(inside(RIGHT_ZONE, PANEL));
    let l = LobbyLayout::new();
    assert_eq!((l.chat(), l.right()), (CHAT_CLASSIC, RIGHT_CLASSIC));
    assert_eq!(l.right_offset(), (0, 0));
    assert_eq!(l.right_grow(), (0, 0));
    assert_eq!(l.chat_offset(), (0, 0));
}

/// Rule (plan L15): the classic layout draws nothing over the art, so the
/// lobby looks as it did.
#[test]
fn the_classic_layout_draws_nothing_over_the_art() {
    let l = LobbyLayout::new();
    assert!(l.is_classic());
    assert!(l.underlay(BG).is_empty());
    assert!(l.outline().is_empty());
}

/// The patch covers each painted zone exactly once, and every piece comes
/// from the frameless parts of the art: the strip between the panes above
/// the ground line, the ground under the panes below it.
#[test]
fn the_patch_covers_each_zone_once_from_frameless_art() {
    for zone in [CHAT_ZONE, RIGHT_ZONE] {
        let mut hits = vec![0u8; 800 * 600];
        for b in cover_blits(zone) {
            let (w, h) = (i32::from(b.src.w), i32::from(b.src.h));
            assert!(w > 0 && h > 0, "{b:?}");
            let strip = b.src.x >= STRIP_X
                && b.src.x + w <= STRIP_X + STRIP_W
                && b.src.y == b.at.y
                && b.src.y + h <= GROUND_Y;
            let ground = b.src.x >= GROUND.x
                && b.src.right() <= GROUND.right()
                && b.src.y >= GROUND.y
                && b.src.bottom() <= GROUND.bottom()
                && b.at.y >= GROUND_Y;
            assert!(strip || ground, "{b:?} copies from a framed part");
            for y in b.at.y..b.at.y + h {
                for x in b.at.x..b.at.x + w {
                    assert!(zone.contains(Point::new(x, y)), "{b:?} leaves {zone:?}");
                    hits[(y * 800 + x) as usize] += 1;
                }
            }
        }
        for y in zone.y..zone.y + i32::from(zone.h) {
            for x in zone.x..zone.x + i32::from(zone.w) {
                assert_eq!(hits[(y * 800 + x) as usize], 1, "({x}, {y}) of {zone:?}");
            }
        }
    }
    // The palisade tiles line up with the strip itself.
    let b = cover_blits(RIGHT_ZONE);
    assert_eq!(b[0].src.x, STRIP_X);
    assert_eq!(b[0].at.x, RIGHT_ZONE.x);
}

/// A moved panel: its painted frame covered, its own pane and frame where
/// it is now; the other panel's art untouched.
#[test]
fn a_moved_panel_covers_its_frame_and_draws_its_own() {
    let mut l = LobbyLayout::new();
    drag(&mut l, CHAT_TITLE, Point::new(330, 185));
    assert!(l.take_moved());
    let chat = l.chat();
    assert_eq!((chat.x, chat.y), (78, 178));
    assert_eq!(l.chat_offset(), (30, 20));
    assert_eq!(l.right(), RIGHT_CLASSIC);
    assert_eq!(l.chat_layout(), chat_layout(chat));
    let items = l.underlay(BG);
    let cuts: Vec<Point> = items
        .iter()
        .filter_map(|i| match i {
            DrawItem::Cut { at, .. } => Some(*at),
            _ => None,
        })
        .collect();
    assert!(!cuts.is_empty());
    assert!(cuts.iter().all(|p| CHAT_ZONE.contains(*p)), "{cuts:?}");
    let frames: Vec<(Rect, Style)> = items
        .iter()
        .filter_map(|i| match i {
            DrawItem::Frame { rect, style, .. } => Some((*rect, *style)),
            _ => None,
        })
        .collect();
    assert_eq!(frames, panel_frames(CHAT_ID, chat));
    // The covers come before the panes and frames.
    let last_cut = items
        .iter()
        .rposition(|i| matches!(i, DrawItem::Cut { .. }))
        .unwrap();
    let first_pane = items
        .iter()
        .position(|i| matches!(i, DrawItem::Rect { .. }))
        .unwrap();
    assert!(last_cut < first_pane);
}

/// The other cover: the same rect of another background.
#[test]
fn a_scene_cover_cuts_the_zone_from_its_background() {
    let mut l = LobbyLayout::new();
    l.cover = Cover::Scene(r"CharSelect\charselectbckg");
    drag(&mut l, RIGHT_TITLE, Point::new(560, 238));
    let cuts: Vec<DrawItem> = l
        .underlay(BG)
        .into_iter()
        .filter(|i| matches!(i, DrawItem::Cut { .. }))
        .collect();
    assert_eq!(
        cuts,
        vec![DrawItem::Cut {
            file: r"CharSelect\charselectbckg",
            src: RIGHT_ZONE,
            at: RIGHT_ZONE.origin(),
        }]
    );
}

/// The right panel grows from its edges and never shrinks below classic.
#[test]
fn the_right_panel_grows_and_never_shrinks() {
    let mut l = LobbyLayout::new();
    // The bottom-right corner.
    let corner = Point::new(RIGHT_CLASSIC.x + 318, RIGHT_CLASSIC.y + 288);
    drag(&mut l, corner, Point::new(corner.x - 40, corner.y - 40));
    assert_eq!(l.right(), RIGHT_CLASSIC);
    // Move it left first, then grow it.
    drag(
        &mut l,
        RIGHT_TITLE,
        Point::new(RIGHT_TITLE.x - 100, RIGHT_TITLE.y - 50),
    );
    let r = l.right();
    let corner = Point::new(r.x + i32::from(r.w) - 2, r.y + i32::from(r.h) - 2);
    drag(&mut l, corner, Point::new(corner.x + 60, corner.y + 41));
    let (dw, dh) = l.right_grow();
    assert!(dw >= 50 && dh >= 40, "{dw} {dh}");
    let off = l.right_offset();
    assert_eq!(
        l.to_right(Point::new(500 + off.0, 400 + off.1)),
        Point::new(500, 400)
    );
}

/// Rule (plan L15): RESET puts both panels back on the art's frames.
#[test]
fn reset_gives_the_classic_layout() {
    let mut l = LobbyLayout::new();
    drag(&mut l, CHAT_TITLE, Point::new(350, 200));
    drag(&mut l, RIGHT_TITLE, Point::new(520, 230));
    assert!(!l.is_classic());
    assert!(l.reset().is_none());
    assert!(l.is_classic());
    assert_eq!((l.chat(), l.right()), (CHAT_CLASSIC, RIGHT_CLASSIC));
    assert!(l.underlay(BG).is_empty());
}

/// Rule (plan L15): locked panels don't move or resize, and show no
/// outline.
#[test]
fn locked_panels_stay() {
    let mut l = LobbyLayout::new();
    l.toggle_lock();
    assert!(l.locked());
    assert!(!l.press(CHAT_TITLE, true));
    l.pointer(CHAT_TITLE);
    assert!(l.outline().is_empty());
    l.toggle_lock();
    assert!(!l.locked());
    l.pointer(CHAT_TITLE);
    assert_eq!(l.outline().len(), 1);
}

/// A press on a control or on a chat tab is theirs, not a grab.
#[test]
fn controls_and_tabs_keep_their_presses() {
    let mut l = LobbyLayout::new();
    let mut b = Control::new(
        crate::ui::front_end::ControlKind::Button,
        CHAT_TITLE.x - 10,
        CHAT_TITLE.y + 10,
        40,
        20,
    );
    b.action = Action::Custom(1);
    l.set_hot(&[b]);
    assert!(!l.press(CHAT_TITLE, true));
    l.pointer(CHAT_TITLE);
    assert!(l.outline().is_empty(), "no outline over a control");
    l.set_hot(&[]);
    assert!(!l.press(CHAT_TITLE, false), "a tab");
    assert!(l.press(CHAT_TITLE, true));
}

/// Rule (plan L15): the layout is saved per realm and account and comes
/// back on the next entry; RESET forgets it.
#[test]
fn a_saved_layout_comes_back() {
    let dir = temp_dir("saved");
    let mut l = LobbyLayout::new();
    assert!(l.enter(Some(dir.clone()), key("Nugwa")).is_none());
    assert!(drag(&mut l, CHAT_TITLE, Point::new(340, 195)).is_none());
    let moved = l.chat();
    assert!(l.toggle_lock().is_none());

    let mut again = LobbyLayout::new();
    assert!(again.enter(Some(dir.clone()), key("nugwa")).is_none());
    assert_eq!(again.chat(), moved);
    assert_eq!(again.right(), RIGHT_CLASSIC);
    assert!(again.locked());

    // Another account has its own (none yet: classic).
    let mut other = LobbyLayout::new();
    other.enter(Some(dir.clone()), key("someone"));
    assert!(other.is_classic() && !other.locked());

    // Unlocked and reset: the entry is gone.
    again.toggle_lock();
    again.reset();
    let store = LayoutStore::load(&dir).unwrap();
    assert!(store.get(&key("nugwa").unwrap()).is_none());
    let mut back = LobbyLayout::new();
    back.enter(Some(dir.clone()), key("Nugwa"));
    assert!(back.is_classic());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Without a config folder (tests, captures) nothing is written.
#[test]
fn without_a_folder_nothing_is_saved() {
    let mut l = LobbyLayout::new();
    l.enter(None, key("nugwa"));
    assert!(drag(&mut l, CHAT_TITLE, Point::new(340, 195)).is_none());
    let mut again = LobbyLayout::new();
    again.enter(None, key("nugwa"));
    assert!(again.is_classic());
}

/// A damaged layouts file: the classic layout, and the chat says why.
#[test]
fn a_damaged_file_keeps_the_classic_layout() {
    let dir = temp_dir("damaged");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(crate::ui::float::store::layouts_path(&dir), "version = [").unwrap();
    let mut l = LobbyLayout::new();
    let note = l.enter(Some(dir.clone()), key("nugwa"));
    assert!(note.is_some_and(|n| n.contains("could not be read")));
    assert!(l.is_classic());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Shifting moves every positioned item, control and element.
#[test]
fn shifting_moves_items_controls_and_elements() {
    let mut items = vec![
        DrawItem::Rect {
            at: Point::new(1, 2),
            w: 3,
            h: 4,
        },
        DrawItem::Frame {
            file: BG,
            rect: Rect::new(1, 2, 3, 4),
            style: Style::Rope,
        },
        DrawItem::Dim { mode: 1 },
    ];
    shift_items(&mut items, (10, 20));
    assert_eq!(
        items,
        vec![
            DrawItem::Rect {
                at: Point::new(11, 22),
                w: 3,
                h: 4,
            },
            DrawItem::Frame {
                file: BG,
                rect: Rect::new(11, 22, 3, 4),
                style: Style::Rope,
            },
            DrawItem::Dim { mode: 1 },
        ]
    );
    let mut c = [Control::new(
        crate::ui::front_end::ControlKind::Text,
        5,
        6,
        7,
        8,
    )];
    shift_controls(&mut c, (-5, 4));
    assert_eq!((c[0].x, c[0].y), (0, 10));
    let mut e = [UiElement::new("x", Rect::new(0, 0, 10, 10))];
    shift_elements(&mut e, (3, 4));
    assert_eq!(e[0].rect, Rect::new(3, 4, 10, 10));
    assert_eq!(e[0].at, Point::new(8, 9));
}
