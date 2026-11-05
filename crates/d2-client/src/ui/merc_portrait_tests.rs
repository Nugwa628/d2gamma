use super::*;
use crate::bridge::world::{ClientUnit, KindData, PetRecord, PlayerData, PLAYER};
use crate::ui::layout::Screen;
use crate::ui::original::{OriginalUi, UiConfig};
use crate::ui::{NoPanelRules, NoStrings, UiRoot};

const ME: UnitKey = UnitKey {
    unit_type: PLAYER,
    guid: 1,
};
const MERC: u32 = 900;
/// The palette indices the test gives green, yellow and red.
const COLORS: [u8; 3] = [10, 20, 30];

/// The portrait's left column in box space (15 in from the frame's left).
fn x0() -> i32 {
    crate::ui::ui_frame::UiFrame::current().left() + ICON_LEFT
}

/// An expansion game: the local player and, with `class`, her hireling
/// at `life` percent (her pet record's, as S→C 0x7F sets it).
fn world(class: Option<u32>, life: i32) -> ClientWorld {
    let mut w = ClientWorld {
        expansion: 1,
        ..ClientWorld::default()
    };
    let mut p = ClientUnit::new(ME);
    p.kind = KindData::Player(PlayerData::default());
    p.mode = 1;
    w.units.insert(ME, p);
    w.local_player = Some(ME);
    if let Some(class) = class {
        let key = UnitKey::new(MONSTER, MERC);
        let mut m = ClientUnit::new(key);
        m.class = class;
        w.units.insert(key, m);
        w.pets.push(PetRecord {
            class: class as u16,
            pet_type: PET_HIRELING,
            pet: MERC,
            owner: ME.guid,
            f1c: life as u32,
            gone: false,
            extra: Some([5, 0x0D53, 0]),
        });
    }
    w
}

fn setup() -> (OriginalUi, UiRoot) {
    let config = UiConfig {
        screen: Screen::R800,
        expansion_installed: true,
    };
    let ui = OriginalUi::new(config, None).unwrap();
    ui.shared.borrow_mut().merc_bar_colors = Some(COLORS);
    let mut root = UiRoot::new(Box::new(NoPanelRules));
    ui.install(&mut root).unwrap();
    (ui, root)
}

fn draws(root: &UiRoot, w: &ClientWorld, tick: u64) -> Vec<UiDraw> {
    let ctx = UiCtx {
        tick,
        world: w,
        strings: &NoStrings,
    };
    let mut out: Vec<UiDraw> = Vec::new();
    root.draw(&ctx, &mut out);
    out
}

/// The portrait's draws: the bar rectangles in the bar's rows, and the
/// cel whose bottom is the portrait's.
fn portrait(ui: &OriginalUi, d: &[UiDraw]) -> (Vec<RectRequest>, Vec<(String, Point, u8)>) {
    let files = ui.files();
    let rects = d
        .iter()
        .filter_map(|d| match d {
            UiDraw::Rect(r) if r.y0 == BAR_TOP && r.y1 == BAR_BOTTOM => Some(*r),
            _ => None,
        })
        .collect();
    let cels = d
        .iter()
        .filter_map(|d| match d {
            UiDraw::Image(i) => {
                let name = files.name(i.image.file)?.to_string();
                if !name.starts_with("hireables\\") {
                    return None;
                }
                Some((name, i.at, i.look.mode))
            }
            _ => None,
        })
        .collect();
    (rects, cels)
}

fn send(ui: &mut OriginalUi, root: &mut UiRoot, w: &ClientWorld, tick: u64, e: UiEvent) {
    let ctx = UiCtx {
        tick,
        world: w,
        strings: &NoStrings,
    };
    ui.before_event(e, w);
    let r = root.dispatch(e, &ctx);
    ui.after_event(root, e, r).unwrap();
}

// A hired Rogue: her portrait (`rogueicon`, see-through) 15 columns in with
// its bottom on row 60, and above it the life bar: 30 % of 46 columns in
// yellow, the rest black.
#[test]
fn a_hireling_gets_her_portrait_and_life_bar() {
    let (ui, root) = setup();
    let w = world(Some(271), 30);
    let (rects, cels) = portrait(&ui, &draws(&root, &w, 0));
    assert_eq!(
        cels,
        vec![(
            "hireables\\rogueicon".to_string(),
            Point::new(x0(), 60),
            MODE_PLAIN
        )]
    );
    assert_eq!(
        rects,
        vec![
            RectRequest {
                x0: x0(),
                y0: 14,
                x1: x0() + 13,
                y1: 19,
                color: COLORS[1],
                mode: 5
            },
            RectRequest {
                x0: x0() + 13,
                y0: 14,
                x1: x0() + 46,
                y1: 19,
                color: 0,
                mode: 5
            },
        ]
    );
}

// The class picks the portrait; the bar is green from 50 %, red below 25 %,
// and there is none at 0 %.
#[test]
fn the_class_picks_the_portrait_and_life_the_bar_colour() {
    let (ui, root) = setup();
    let w = world(Some(338), 50);
    let (rects, cels) = portrait(&ui, &draws(&root, &w, 0));
    assert_eq!(cels[0].0, "hireables\\act2hireableicon");
    assert_eq!((rects[0].x1, rects[0].color), (x0() + 23, COLORS[0]));
    let w = world(Some(561), 24);
    let (rects, cels) = portrait(&ui, &draws(&root, &w, 0));
    assert_eq!(cels[0].0, "hireables\\barbhirable_icon");
    assert_eq!(rects[0].color, COLORS[2]);
    let w = world(Some(271), 0);
    let (rects, cels) = portrait(&ui, &draws(&root, &w, 0));
    assert!(rects.is_empty(), "no bar at 0 %");
    assert_eq!(cels.len(), 1);
}

// No portrait without a hireling, for a dead one (her pet record gone), in a
// classic game, or with the game menu open.
#[test]
fn no_portrait_without_a_living_hireling_or_with_the_game_menu() {
    let (ui, root) = setup();
    let none = |ui: &OriginalUi, root: &UiRoot, w: &ClientWorld| {
        let (r, c) = portrait(ui, &draws(root, w, 0));
        r.is_empty() && c.is_empty()
    };
    assert!(none(&ui, &root, &world(None, 0)));
    let mut gone = world(Some(271), 80);
    gone.pets[0].gone = true;
    assert!(none(&ui, &root, &gone));
    let mut classic = world(Some(271), 80);
    classic.expansion = 0;
    assert!(none(&ui, &root, &classic));
    let w = world(Some(271), 80);
    assert!(!none(&ui, &root, &w));
    ui.shared.borrow_mut().states.force(9, true);
    assert!(none(&ui, &root, &w), "the game menu hides it");
}

// The pointer rests on the portrait: after 700 ms it brightens (mode 7) and
// a right click opens the hireling panel (ui 0x24); a right click sooner does nothing.
// The open panel (a left panel) hides the portrait, so it takes no click
// until the panel closes; off the portrait the rest starts again.
#[test]
fn a_rested_click_on_the_portrait_toggles_the_hireling_panel() {
    let (mut ui, mut root) = setup();
    let w = world(Some(271), 80);
    let at = Point::new(x0() + 15, 40);
    let button = PointerButton::Right;
    send(&mut ui, &mut root, &w, 0, UiEvent::CursorMoved(at));
    send(&mut ui, &mut root, &w, 1, UiEvent::Press { button, at });
    send(&mut ui, &mut root, &w, 2, UiEvent::Release { button, at });
    assert!(!ui.is_open(0x24), "80 ms is too soon");
    // 18 frames of 40 ms: 720 ms on the portrait.
    let (_, cels) = portrait(&ui, &draws(&root, &w, 18));
    assert_eq!(cels[0].2, MODE_HOVER);
    send(&mut ui, &mut root, &w, 18, UiEvent::Press { button, at });
    send(&mut ui, &mut root, &w, 18, UiEvent::Release { button, at });
    assert!(ui.is_open(0x24), "the hireling panel opens");
    let (r, c) = portrait(&ui, &draws(&root, &w, 19));
    assert!(r.is_empty() && c.is_empty(), "hidden by the open panel");
    send(&mut ui, &mut root, &w, 19, UiEvent::Press { button, at });
    send(&mut ui, &mut root, &w, 19, UiEvent::Release { button, at });
    assert!(ui.is_open(0x24), "the hidden portrait takes no click");
    ui.set_ui(0x24, 1, false).unwrap();
    assert!(!ui.is_open(0x24));
    send(
        &mut ui,
        &mut root,
        &w,
        20,
        UiEvent::CursorMoved(Point::new(x0() + 400, 300)),
    );
    let (_, cels) = portrait(&ui, &draws(&root, &w, 40));
    assert_eq!(cels[0].2, MODE_PLAIN);
}

/// A pet record of the local player.
fn summon(pet_type: u8, guid: u32, class: u32, life: u32) -> PetRecord {
    PetRecord {
        class: class as u16,
        pet_type,
        pet: guid,
        owner: ME.guid,
        f1c: life,
        gone: false,
        extra: None,
    }
}

// The summons follow the portrait, one icon each 56 columns on, in `pettype`
// order: a golem (type 3) by its class with its life bar; skeletons (type 2)
// with their count and no bar for three; a raven (type 2, no `drawhp`) with
// count 1 and no bar; one skeleton mage (type 2, `drawhp`) with its bar.
// Another player's pet and a gone record draw nothing.
#[test]
fn the_summons_follow_the_portrait_in_pettype_order() {
    let (ui, root) = setup();
    let mut w = world(Some(271), 80);
    w.pets.push(summon(4, 31, 363, 90));
    w.pets.push(summon(4, 32, 363, 90));
    w.pets.push(summon(4, 33, 363, 90));
    w.pets.push(summon(3, 40, 291, 40));
    w.pets.push(summon(10, 50, 419, 100));
    w.pets.push(summon(5, 60, 364, 10));
    w.pets.push(PetRecord {
        owner: 99,
        ..summon(15, 70, 428, 100)
    });
    w.pets.push(PetRecord {
        gone: true,
        ..summon(16, 80, 417, 100)
    });
    let icons = pet_icon_row(&w, &PetIcons::default(), 0);
    let row: Vec<(&str, i32, Option<u32>, Option<u32>)> = icons
        .iter()
        .map(|i| (i.file.as_str(), i.x, i.life, i.count))
        .collect();
    assert_eq!(
        row,
        vec![
            (r"hireables\rogueicon", 0, Some(80), None),
            (r"hireables\metalgolumicon", 56, Some(40), None),
            (r"hireables\skeletonicon", 112, None, Some(3)),
            (r"hireables\skeletonmageicon", 168, Some(10), Some(1)),
            (r"hireables\raven", 224, None, Some(1)),
        ]
    );
    // Drawn: five icons, the counts as text on row 0x37, four bars.
    let d = draws(&root, &w, 0);
    let (rects, cels) = portrait(&ui, &d);
    assert_eq!(cels.len(), 5);
    assert_eq!(rects.len(), 2 * 3, "three bars of two rectangles");
    let counts: Vec<(String, i32)> = d
        .iter()
        .filter_map(|d| match d {
            UiDraw::Text(t) if t.at.y == COUNT_Y => {
                Some((String::from_utf16_lossy(&t.text), t.at.x - x0()))
            }
            _ => None,
        })
        .map(|(t, x)| (t, x / ICON_STEP))
        .collect();
    assert_eq!(
        counts,
        vec![("3".into(), 2), ("1".into(), 3), ("1".into(), 4)]
    );
}

// Without a hireling the summons start at the portrait's place.
#[test]
fn without_a_hireling_the_summons_start_at_the_left() {
    let mut w = world(None, 0);
    w.pets.push(summon(2, 31, 357, 100));
    let icons = pet_icon_row(&w, &PetIcons::default(), 15);
    assert_eq!(icons.len(), 1);
    assert_eq!(
        (icons[0].file.as_str(), icons[0].x),
        (r"hireables\valkarieicon", 15)
    );
}

/// The `pettype` names the test reads: Clay Golem (4052), Skeleton (4053).
struct Names;

impl crate::ui::StringLookup for Names {
    fn get(&self, _: &str) -> Option<&[u16]> {
        None
    }
    fn get_id(&self, id: u16) -> Option<&[u16]> {
        let leak = |t: &str| -> &'static [u16] {
            Box::leak(t.encode_utf16().collect::<Vec<_>>().into_boxed_slice())
        };
        match id {
            4052 => Some(leak("Golem")),
            4053 => Some(leak("Skeleton")),
            _ => None,
        }
    }
}

// Each pet icon has its type's name under it in font 6, centred on its
// column 0x16; the name rows take turns between 0x48 and 0x52, the
// portrait taking the first turn (so the golem's name is on 0x52 and the
// skeletons' on 0x48). The count is on row 0x37, eight columns in. The
// hireling's row name (`dummy`, 0x1506) draws nothing.
#[test]
fn pet_names_take_turns_under_their_icons() {
    let (_ui, root) = setup();
    let mut w = world(Some(271), 80);
    w.pets.push(summon(3, 40, 289, 100));
    w.pets.push(summon(4, 31, 363, 100));
    w.pets.push(summon(4, 32, 363, 100));
    let ctx = UiCtx {
        tick: 0,
        world: &w,
        strings: &Names,
    };
    let mut out: Vec<UiDraw> = Vec::new();
    root.draw(&ctx, &mut out);
    let labels: Vec<(String, i32, u16)> = out
        .iter()
        .filter_map(|d| match d {
            UiDraw::Text(t)
                if t.style.font == LABEL_FONT && t.at.y < 100 && t.at.x < x0() + 300 =>
            {
                Some((String::from_utf16_lossy(&t.text), t.at.y, t.style.color))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        labels,
        vec![
            ("Golem".to_string(), LABEL_Y + LABEL_STEP, 0),
            ("2".to_string(), COUNT_Y, 0),
            ("Skeleton".to_string(), LABEL_Y, 0),
        ]
    );
    let count = out
        .iter()
        .find_map(|d| match d {
            UiDraw::Text(t) if t.at.y == COUNT_Y => Some(t.at.x),
            _ => None,
        })
        .unwrap();
    assert_eq!(count, x0() + 2 * ICON_STEP + COUNT_DX);
    // The names sit under their icons (no fonts here: width 0).
    let name_x = |name: &str| {
        out.iter()
            .find_map(|d| match d {
                UiDraw::Text(t) if String::from_utf16_lossy(&t.text) == name => Some(t.at.x),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(name_x("Golem"), x0() + ICON_STEP + LABEL_DX);
    assert_eq!(name_x("Skeleton"), x0() + 2 * ICON_STEP + LABEL_DX);
}

/// The hireling's name and the caption's strings.
struct Caption;

impl crate::ui::StringLookup for Caption {
    fn get(&self, _: &str) -> Option<&[u16]> {
        None
    }
    fn get_id(&self, id: u16) -> Option<&[u16]> {
        let leak = |t: &str| -> &'static [u16] {
            Box::leak(t.encode_utf16().collect::<Vec<_>>().into_boxed_slice())
        };
        match id {
            0x0D53 => Some(leak("Aliza")),
            STR_CAPTION_OPEN => Some(leak("Right-click to Open Inventory (%s)")),
            STR_NEW_LINE => Some(leak("\n")),
            STR_CAPTION_POTION => Some(leak("Drop Potion on Portrait to Heal")),
            _ => None,
        }
    }
}

fn texts_with(root: &UiRoot, w: &ClientWorld, tick: u64) -> Vec<(String, Point)> {
    let ctx = UiCtx {
        tick,
        world: w,
        strings: &Caption,
    };
    let mut out: Vec<UiDraw> = Vec::new();
    root.draw(&ctx, &mut out);
    out.iter()
        .filter_map(|d| match d {
            UiDraw::Text(t) if t.at.y < 200 && t.at.x < x0() + 300 => {
                Some((String::from_utf16_lossy(&t.text), t.at))
            }
            _ => None,
        })
        .collect()
}

// Her name is always under the portrait, on the first name row, centred on
// column 0x16. Once the pointer has rested on the portrait for 700 ms with
// an empty cursor, the caption pop-up shows at her column, 0x24 rows under
// the second name row: the open line with the hireling key, a new line and
// the potion line. Sooner, or off the portrait, there is none.
#[test]
fn her_name_is_under_the_portrait_and_the_caption_follows_a_rest() {
    let (mut ui, mut root) = setup();
    let w = world(Some(271), 80);
    let name = ("Aliza".to_string(), Point::new(x0() + LABEL_DX, LABEL_Y));
    assert_eq!(texts_with(&root, &w, 0), vec![name.clone()]);
    let at = Point::new(x0() + 15, 40);
    send(&mut ui, &mut root, &w, 0, UiEvent::CursorMoved(at));
    assert_eq!(texts_with(&root, &w, 10), vec![name.clone()], "400 ms");
    let t = texts_with(&root, &w, 18);
    assert_eq!(t[0], name);
    assert_eq!(
        t[1],
        (
            "Right-click to Open Inventory ()\nDrop Potion on Portrait to Heal".to_string(),
            Point::new(x0(), LABEL_Y + LABEL_STEP + CAPTION_DY)
        )
    );
    send(
        &mut ui,
        &mut root,
        &w,
        19,
        UiEvent::CursorMoved(Point::new(x0() + 400, 300)),
    );
    assert_eq!(texts_with(&root, &w, 40), vec![name]);
}

/// The item tables of the drop rule: the fixture's cap (helm, level 3), its
/// scrolls, and a healing potion (76), a short bow (27), a short sword (30),
/// a spear (33), a hand axe (28).
fn give_tips() -> crate::ui::item_tip::ItemTips {
    use d2_sim::items::tables::{ItemRec, ItemTables};
    let rec = |code: &[u8; 4], ty: i16| ItemRec {
        code: *code,
        type_: ty,
        ..ItemRec::default()
    };
    crate::ui::item_tip::tests::tips_with(ItemTables {
        items: vec![
            rec(b"hp1 ", 76),
            rec(b"sbw ", 27),
            rec(b"ssd ", 30),
            rec(b"spr ", 33),
            rec(b"hax ", 28),
        ],
        ..ItemTables::default()
    })
}

const IDENTIFIED: u32 = 0x10;

// The drop rule of the original: unidentified or broken items are refused
// with "not now" (0x56); potions are drunk; body armour and helms suit
// everyone, the class weapons their class (a bow the Rogue, a spear the
// guard, a one-handed sword the iron wolf, a one-handed axe the first
// barbarian, any sword the second); a wearable item whose requirements she
// misses does nothing; anything else is refused with "cannot use" (0x55).
#[test]
fn the_drop_rule_follows_her_class_and_the_item() {
    let tips = give_tips();
    let lvl = |l| (100, 100, l);
    let rule = |flags, code: &[u8; 4], class, stats, two| {
        give_rule(flags, *code, class, stats, two, &tips)
    };
    assert_eq!(
        rule(0, b"cap ", 271, lvl(30), false),
        Give::Refused(SPEECH_NOT_NOW),
        "unidentified"
    );
    assert_eq!(
        rule(IDENTIFIED | 0x100, b"cap ", 271, lvl(30), false),
        Give::Refused(SPEECH_NOT_NOW),
        "broken"
    );
    assert_eq!(rule(IDENTIFIED, b"hp1 ", 271, lvl(1), false), Give::Potion);
    assert_eq!(rule(IDENTIFIED, b"cap ", 338, lvl(30), false), Give::Equip);
    assert_eq!(
        rule(IDENTIFIED, b"cap ", 338, lvl(2), false),
        Give::Unmet,
        "level 3"
    );
    assert_eq!(rule(IDENTIFIED, b"sbw ", 271, lvl(30), true), Give::Equip);
    assert_eq!(
        rule(IDENTIFIED, b"sbw ", 338, lvl(30), true),
        Give::Refused(SPEECH_CANNOT),
        "a bow is the Rogue's"
    );
    assert_eq!(rule(IDENTIFIED, b"spr ", 338, lvl(30), true), Give::Equip);
    assert_eq!(rule(IDENTIFIED, b"ssd ", 359, lvl(30), false), Give::Equip);
    assert_eq!(
        rule(IDENTIFIED, b"ssd ", 359, lvl(30), true),
        Give::Refused(SPEECH_CANNOT),
        "the iron wolf's sword is one-handed"
    );
    assert_eq!(rule(IDENTIFIED, b"hax ", 560, lvl(30), false), Give::Equip);
    assert_eq!(rule(IDENTIFIED, b"ssd ", 561, lvl(30), true), Give::Equip);
    assert_eq!(
        rule(IDENTIFIED, b"hax ", 561, lvl(30), false),
        Give::Refused(SPEECH_CANNOT)
    );
    assert_eq!(
        rule(IDENTIFIED, b"isc ", 271, lvl(30), false),
        Give::Refused(SPEECH_CANNOT),
        "a scroll"
    );
    // The give itself is C→S 0x61 with location 0.
    assert_eq!(give_message().0, vec![0x61, 0, 0]);
}
