//! The pet icon row at the top left of the play screen: the hireling's
//! portrait (an expansion game with a living hireling of the local player),
//! then the local player's summoned pets ([`pet_icon_row`]).
//!
//! # The picture
//!
//! The portrait is the cel `hireables\<icon>` of the hireling pet type
//! (`pettype` row 7): the `micon` of the first `mclass` slot that names
//! the hireling's monster class, else the row's `baseicon` (a Rogue has no
//! slot of her own: `rogueicon`). It is drawn [`ICON_LEFT`] columns in from
//! the frame's left edge with its bottom row on [`ICON_BOTTOM`], in draw
//! mode 1 (see-through), and in mode 7 (brightened) once the pointer has
//! rested on it for [`HOVER_MS`].
//!
//! # The life bar
//!
//! Above the picture, [`BAR_WIDTH`] columns wide on rows
//! [`BAR_TOP`]..[`BAR_BOTTOM`]: the life percentage of the bar's width in
//! green (50 % and more), yellow (25–49 %) or red (below 25 %), the rest
//! black, both opaque (draw mode 5). At 0 % (the hireling is dying) no bar
//! is drawn. The percentage is her pet record's, which the server's S→C 0x7F
//! keeps (100 at the hire).
//!
//! # The pointer
//!
//! The portrait's area is columns `x`..=`x + 46`, rows 24..=60. A right
//! button release there, after the pointer has rested on the portrait for
//! [`HOVER_MS`], opens or closes the hireling's panel (ui 0x24), as the
//! hireling key does; the right press there is taken.
//!
//! # A held item
//!
//! A left press on the portrait with an item on the cursor drops it on
//! her ([`give_rule`]): a healing, antidote or thawing potion (she drinks
//! it, the player's speech event 0x54) or an item she can wear and whose
//! requirements she meets is sent to her (C→S 0x61, location 0; the
//! server equips it, and an item she already wears there comes back to
//! the cursor); an unidentified or broken item is refused with event 0x56,
//! an item she cannot use with event 0x55; one whose requirements she does
//! not meet does nothing. While such a usable item is held over the
//! portrait it is brightened at once.
//!
//! # The captions
//!
//! Her own name is under the portrait as the pets' names are (font 6, cut
//! to [`LABEL_WIDTH`], centred on [`LABEL_DX`], on the first name row).
//! Once the pointer has rested on the portrait for [`HOVER_MS`] with no
//! item on the cursor, the portrait brightens and the pop-up text
//! "Right-click to Open Inventory (<hireling key>)", a new line, "Drop
//! Potion on Portrait to Heal" is drawn at her column, [`CAPTION_DY`] rows
//! under the name row after hers.
//!
//! # The summons
//!
//! After the portrait, each `pettype` row with an `icontype`, in table
//! order, the player has pets of: one icon each, [`ICON_STEP`] columns on,
//! drawn as the portrait is (`hireables\<icon>` of the row, by the pet's
//! class; mode 1). Types 1 (a Valkyrie, a Grizzly, a Shadow) and 3 (a golem, a
//! druid spirit) draw their pet's life bar; type 2 (skeletons, mages,
//! revives, ravens, wolves) draws the count in font 6 at [`COUNT_DX`] on row
//! [`COUNT_Y`], and the life bar of its pet while it has one and the row's
//! `drawhp` is set. Their life percents come from S→C 0x7F as hers do.
//! Each pet icon has its type's `pettype` name under it in font 6, cut to
//! [`LABEL_WIDTH`] columns and centred on [`LABEL_DX`], the rows taking
//! turns between [`LABEL_Y`] and ten rows lower (the portrait takes the
//! first turn).

use std::cell::Cell;

use super::SharedRef;
use crate::bridge::world::{ClientWorld, UnitKey, MONSTER, PET_HIRELING};
use crate::ui::draw::{CelCall, CelLook, ImageRef, ImageRequest, RectRequest, Remap};
use crate::ui::draw::{TextRequest, TextStyle, UiDraw, UiDrawSink};
use crate::ui::geom::{Point, Rect};
use crate::ui::panel::{Panel, PanelId, UiCtx, UiEvent, UiResponse, WidgetId};
use crate::ui::panels::npc_menu::{MERC_NAME_ALT_ID, STR_MERC_NAME_ALT};
use crate::ui::panels::PanelOutput;
use crate::ui::text::TextOpts;
use crate::ui::PointerButton;

/// The panel's id: not a UI state.
pub const MERC_PORTRAIT_PANEL: PanelId = PanelId(0x119);
/// The portrait's left column, counted from the frame's left edge.
pub const ICON_LEFT: i32 = 15;
/// The portrait's bottom row.
pub const ICON_BOTTOM: i32 = 60;
/// The pointer area's top row.
pub const HIT_TOP: i32 = 24;
/// The pointer area's and the bar's width (the area spans one more column).
pub const BAR_WIDTH: i32 = 46;
/// The life bar's rows: `BAR_TOP` up to, not including, `BAR_BOTTOM`.
pub const BAR_TOP: i32 = 14;
pub const BAR_BOTTOM: i32 = 19;
/// How long the pointer rests on the portrait before it brightens and
/// a click opens the hireling panel.
pub const HOVER_MS: u32 = 700;
/// The portrait's draw modes: see-through, and brightened under the pointer.
pub const MODE_PLAIN: u8 = 1;
pub const MODE_HOVER: u8 = 7;
/// The rectangle's draw mode (opaque).
const BAR_MODE: u8 = 5;
/// Ui 0x24: the hireling's panel.
const UI_HIRELING_PANEL: u8 = 0x24;
/// The bar colours (RGB) the palette is matched for: green, yellow, red.
pub const BAR_RGB: [(u8, u8, u8); 3] = [(0, 0x80, 0), (0xC0, 0xC0, 0), (0xFF, 0, 0)];
/// The folder of the portrait cels under `data\global\ui\`.
pub const ICON_FOLDER: &str = "hireables";

/// One pet type's icons (`pettype`): by monster class, else the base one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MercIcons {
    /// `baseicon`.
    pub base: String,
    /// `(mclass, micon)` of the slots with an icon.
    pub classes: Vec<(u16, String)>,
}

impl MercIcons {
    /// The icon file of monster class `class` (`hireables\<icon>`).
    pub fn file(&self, class: u32) -> String {
        let icon = self
            .classes
            .iter()
            .find(|(c, i)| u32::from(*c) == class && !i.is_empty())
            .map_or(self.base.as_str(), |(_, i)| i.as_str());
        format!("{ICON_FOLDER}\\{icon}")
    }

    /// Every icon file of the type.
    pub fn files(&self) -> Vec<String> {
        std::iter::once(self.base.as_str())
            .chain(self.classes.iter().map(|(_, i)| i.as_str()))
            .filter(|i| !i.is_empty())
            .map(|i| format!("{ICON_FOLDER}\\{i}"))
            .collect()
    }
}

/// The icon row's kind of a pet type (`icontype`): 0 none, 1 one pet with
/// its life bar, 2 a count of pets (the life bar while there is one), 3 one
/// pet whose class picks the icon.
pub const ICON_SINGLE: u8 = 1;
pub const ICON_COUNT: u8 = 2;
pub const ICON_CLASS: u8 = 3;

/// One `pettype` row as the pet icons read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PetRow {
    /// `pet type` (the pet records' type).
    pub pet_type: u8,
    /// `icontype`.
    pub icontype: u8,
    /// `drawhp`: a count icon draws the life bar of its one pet.
    pub drawhp: bool,
    /// `name` (a string id).
    pub name: u16,
    pub icons: MercIcons,
}

/// The `pettype` rows, in table order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PetIcons {
    pub rows: Vec<PetRow>,
}

impl Default for PetIcons {
    /// The base game's `pettype` rows.
    fn default() -> Self {
        let row = |pet_type, icontype, drawhp, name, base: &str, classes: &[(u16, &str)]| PetRow {
            pet_type,
            icontype,
            drawhp,
            name,
            icons: MercIcons {
                base: base.into(),
                classes: classes
                    .iter()
                    .map(|(c, i)| (*c, (*i).to_string()))
                    .collect(),
            },
        };
        PetIcons {
            rows: vec![
                row(2, 1, true, 4056, "valkarieicon", &[]),
                row(
                    3,
                    3,
                    true,
                    4052,
                    "earthgolumicon",
                    &[
                        (290, "bloodgolumicon"),
                        (291, "metalgolumicon"),
                        (292, "firegolumicon"),
                    ],
                ),
                row(4, 2, true, 4053, "skeletonicon", &[]),
                row(5, 2, true, 4054, "skeletonmageicon", &[]),
                row(6, 2, true, 4055, "revivedicon", &[]),
                row(
                    7,
                    0,
                    true,
                    5382,
                    "rogueicon",
                    &[
                        (338, "act2hireableicon"),
                        (359, "act3hireableicon"),
                        (561, "barbhirable_icon"),
                    ],
                ),
                row(10, 2, false, 22320, "raven", &[]),
                row(11, 2, true, 22321, "wolf", &[]),
                row(12, 2, true, 22330, "wolf", &[]),
                row(
                    13,
                    3,
                    true,
                    22329,
                    "oaksage",
                    &[(423, "heartofwolverine"), (422, "spiritofbarbs")],
                ),
                row(
                    14,
                    3,
                    true,
                    22328,
                    "plaguepoppy",
                    &[(426, "cycleoflife"), (427, "vines")],
                ),
                row(15, 1, true, 22322, "bear", &[]),
                row(16, 1, true, 22332, "shadowassassin", &[]),
            ],
        }
    }
}

impl PetIcons {
    /// The hireling pet type's icons (type 7).
    pub fn hireling(&self) -> Option<&MercIcons> {
        self.rows
            .iter()
            .find(|r| r.pet_type == PET_HIRELING)
            .map(|r| &r.icons)
    }

    /// Every icon file, to register with the panel files.
    pub fn files(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for f in self.rows.iter().flat_map(|r| r.icons.files()) {
            if !out.contains(&f) {
                out.push(f);
            }
        }
        out
    }
}

/// The local player's living hireling: its unit key and class.
pub fn local_hireling(world: &ClientWorld) -> Option<(UnitKey, u32)> {
    if world.expansion == 0 {
        return None;
    }
    let me = world.local()?.key;
    let r = world
        .pets
        .iter()
        .find(|r| r.pet_type == PET_HIRELING && r.owner == me.guid && !r.gone)?;
    let key = UnitKey::new(MONSTER, r.pet);
    let class = world
        .units
        .get(&key)
        .map_or(u32::from(r.class), |u| u.class);
    Some((key, class))
}

/// A pet's life in percent (0–100): its pet record's percent (100 at
/// its creation, then each S→C 0x7F the server sends its owner).
pub fn life_percent(world: &ClientWorld, key: UnitKey) -> u32 {
    world
        .pets
        .iter()
        .find(|r| r.pet == key.guid)
        .map_or(0, |r| r.f1c.min(100))
}

/// Columns from one icon of the row to the next.
pub const ICON_STEP: i32 = 0x38;
/// The count's text row on a count icon, and its column from the icon's left.
pub const COUNT_Y: i32 = 0x37;
pub const COUNT_DX: i32 = 8;
/// The widest count, in columns (longer text loses its last characters).
pub const COUNT_WIDTH: i32 = 0x2E;
/// The font of the count and the names.
pub const LABEL_FONT: u16 = 6;
/// The first name row; the next icon's name is [`LABEL_STEP`] rows lower,
/// the one after back on it, and so on.
pub const LABEL_Y: i32 = 0x48;
pub const LABEL_STEP: i32 = 10;
/// A name is centred on this column from its icon's left.
pub const LABEL_DX: i32 = 0x16;
/// The widest name, in columns.
pub const LABEL_WIDTH: i32 = 0x42;
/// The `pettype` name of no name (`dummy`).
pub const NO_NAME: u16 = 0x1506;
/// The hireling's caption: "Right-click to Open Inventory (%s)" with the
/// hireling key (command 54), a new line, "Drop Potion on Portrait to Heal".
pub const STR_CAPTION_OPEN: u16 = 0x58A3;
pub const STR_NEW_LINE: u16 = 0xF9E;
pub const STR_CAPTION_POTION: u16 = 0x58A2;
/// The hireling key's command.
pub const HIRELING_CMD: i32 = 0x36;
/// The caption's row under the turn of the name row after the portrait.
pub const CAPTION_DY: i32 = 0x24;

/// `text` cut from its end until it is at most `max` columns wide.
fn fit(mut text: Vec<u16>, max: i32, width: &dyn Fn(&[u16]) -> i32) -> Vec<u16> {
    while !text.is_empty() && width(&text) > max {
        text.pop();
    }
    text
}

/// A label of the icon row: font 6, colour 0.
fn label(text: Vec<u16>, at: Point, clip: Rect) -> UiDraw {
    UiDraw::Text(TextRequest {
        text,
        at,
        style: TextStyle {
            font: LABEL_FONT,
            color: 0,
        },
        opts: TextOpts::default(),
        clip,
    })
}

/// One icon of the row: the hireling's portrait first, then a pet type's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PetIcon {
    /// The icon file (`hireables\<icon>`).
    pub file: String,
    /// The icon's left column.
    pub x: i32,
    /// The pet the icon stands for (a count icon: its first pet).
    pub unit: UnitKey,
    /// The life percent its bar draws; `None`: no bar.
    pub life: Option<u32>,
    /// A count icon's count.
    pub count: Option<u32>,
    /// The pet type's name string; the hireling's own name for her.
    pub name: u16,
    /// The hireling's portrait.
    pub hireling: bool,
}

/// The icon row of the local player from column `left`: the hireling's
/// portrait (an expansion game), then, in `pettype` order, each pet type
/// with an `icontype` the player has pets of. Types 1 and 3 show their
/// first pet with its life bar; type 2 shows the count, with the life bar
/// of the one pet when there is one and `drawhp` is set. Each icon takes
/// [`ICON_STEP`] columns.
pub fn pet_icon_row(world: &ClientWorld, icons: &PetIcons, left: i32) -> Vec<PetIcon> {
    let Some(me) = world.local().map(|u| u.key) else {
        return Vec::new();
    };
    let class_of = |r: &crate::bridge::world::PetRecord| {
        world
            .units
            .get(&UnitKey::new(MONSTER, r.pet))
            .map_or(u32::from(r.class), |u| u.class)
    };
    let mut out = Vec::new();
    let mut x = left;
    if let (Some((key, class)), Some(m)) = (local_hireling(world), icons.hireling()) {
        let name = world
            .pets
            .iter()
            .find(|r| r.pet == key.guid)
            .and_then(|r| r.extra)
            .map_or(0, |e| e[1] as u16);
        out.push(PetIcon {
            file: m.file(class),
            x,
            unit: key,
            life: Some(life_percent(world, key)),
            count: None,
            name,
            hireling: true,
        });
        x += ICON_STEP;
    }
    for row in &icons.rows {
        if !matches!(row.icontype, ICON_SINGLE | ICON_COUNT | ICON_CLASS)
            || row.pet_type == PET_HIRELING
        {
            continue;
        }
        let mine: Vec<_> = world
            .pets
            .iter()
            .filter(|r| r.pet_type == row.pet_type && r.owner == me.guid && !r.gone)
            .collect();
        let Some(first) = mine.first() else {
            continue;
        };
        let unit = UnitKey::new(MONSTER, first.pet);
        let n = mine.len() as u32;
        let (life, count) = if row.icontype == ICON_COUNT {
            let bar = (n == 1 && row.drawhp).then(|| life_percent(world, unit));
            (bar, Some(n))
        } else {
            (Some(life_percent(world, unit)), None)
        };
        out.push(PetIcon {
            file: row.icons.file(class_of(first)),
            x,
            unit,
            life,
            count,
            name: row.name,
            hireling: false,
        });
        x += ICON_STEP;
    }
    out
}

/// The bar colour index (0 green, 1 yellow, 2 red) of a life percent.
pub fn bar_shade(percent: u32) -> usize {
    match percent {
        50.. => 0,
        25.. => 1,
        _ => 2,
    }
}

/// The two bar rectangles of `percent` at column `x`; none at 0 %.
pub fn bar_rects(x: i32, percent: u32, colors: [u8; 3]) -> Vec<RectRequest> {
    if percent == 0 {
        return Vec::new();
    }
    let fill = x + (percent.min(100) as i32 * BAR_WIDTH) / 100;
    vec![
        RectRequest {
            x0: x,
            y0: BAR_TOP,
            x1: fill,
            y1: BAR_BOTTOM,
            color: colors[bar_shade(percent)],
            mode: BAR_MODE,
        },
        RectRequest {
            x0: fill,
            y0: BAR_TOP,
            x1: x + BAR_WIDTH,
            y1: BAR_BOTTOM,
            color: 0,
            mode: BAR_MODE,
        },
    ]
}

/// What the held item does dropped on the portrait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Give {
    /// A healing, antidote or thawing potion: she drinks it.
    Potion,
    /// An item she can wear or wield, and whose requirements she meets.
    Equip,
    /// An item she can wear or wield whose requirements she does not
    /// meet: nothing happens.
    Unmet,
    /// Refused with the player's speech event: an unidentified or broken
    /// item ([`SPEECH_NOT_NOW`]), or one she cannot use ([`SPEECH_CANNOT`]).
    Refused(u16),
}

/// The player's speech events of a drop on the portrait.
pub const SPEECH_POTION: u16 = 0x54;
pub const SPEECH_CANNOT: u16 = 0x55;
pub const SPEECH_NOT_NOW: u16 = 0x56;
/// Item header flags: identified, broken.
const ITEM_IDENTIFIED: u32 = 0x10;
const ITEM_BROKEN: u32 = 0x100;
/// The hireling classes: Act 1 Rogue, Act 2 guard, Act 3 iron wolf, the two
/// Act 5 barbarians.
const CLASS_ROGUE: u32 = 0x10F;
const CLASS_GUARD: u32 = 0x152;
const CLASS_IRON_WOLF: u32 = 0x167;
const CLASS_BARB_A: u32 = 0x230;
const CLASS_BARB_B: u32 = 0x231;

/// The rule of a drop on the hireling's portrait, over the held item's
/// header flags, code and `2handed`, her class and her base stats
/// (strength, dexterity, level):
/// - unidentified or broken: refused, "not now";
/// - a healing (76), antidote (80) or thawing (81) potion: she drinks it;
/// - body armour (3) or a helm (37) for anyone; a bow (27) for the Rogue; a
///   spear (33) or polearm (34) for the guard; a shield (2) or a one-handed
///   sword (30) for the iron wolf; a one-handed axe (28) for the first
///   barbarian; a sword (30) for the second: equipped when she meets its
///   requirements, else nothing;
/// - anything else: refused, "cannot use".
pub fn give_rule(
    flags: u32,
    code: [u8; 4],
    class: u32,
    stats: (i32, i32, i32),
    two_handed: bool,
    tips: &crate::ui::item_tip::ItemTips,
) -> Give {
    use d2_sim::items::moves::ty;
    if flags & ITEM_IDENTIFIED == 0 || flags & ITEM_BROKEN != 0 {
        return Give::Refused(SPEECH_NOT_NOW);
    }
    let t = tips.tables();
    let Some(i) = t.find_code(code) else {
        return Give::Refused(SPEECH_CANNOT);
    };
    let is = |k: u16| t.is_type(i, k as i16);
    if [ty::HPOT, ty::APOT, ty::WPOT].iter().any(|&k| is(k)) {
        return Give::Potion;
    }
    let one_handed = !two_handed;
    let wearable = is(ty::TORS)
        || is(ty::HELM)
        || match class {
            CLASS_ROGUE => is(ty::BOW),
            CLASS_GUARD => is(ty::SPEA) || is(ty::POLE),
            CLASS_IRON_WOLF => is(ty::SHIE) || (is(ty::SWOR) && one_handed),
            CLASS_BARB_A => is(ty::AXE) && one_handed,
            CLASS_BARB_B => is(ty::SWOR),
            _ => false,
        };
    if !wearable {
        return Give::Refused(SPEECH_CANNOT);
    }
    if tips.can_use(code, stats.0, stats.1, stats.2) {
        Give::Equip
    } else {
        Give::Unmet
    }
}

/// The drop rule for the local player's held item and living hireling
/// ([`give_rule`]); `None` without an item on the cursor, a hireling in
/// the model, or the item tables.
pub fn give_check(
    world: &ClientWorld,
    tips: Option<&crate::ui::item_tip::ItemTips>,
    inv: Option<&d2_sim::items::inventory::InvTables>,
) -> Option<Give> {
    let item = crate::bridge::items::cursor_item(world)?;
    let (key, class) = local_hireling(world)?;
    let merc = world.units.get(&key)?;
    let code = item.code?;
    let tips = tips?;
    // `2handed` of the item's record.
    let two_handed = tips
        .tables()
        .find_code(code)
        .and_then(|i| inv?.item(i))
        .is_some_and(|r| r.twohanded != 0);
    Some(give_rule(
        item.flags,
        code,
        class,
        (merc.stat(0), merc.stat(2), merc.stat(12)),
        two_handed,
        tips,
    ))
}

/// C→S 0x61 (`MercItem`, location 0): the held item goes to the hireling.
pub fn give_message() -> crate::ui::panel::ClientIntent {
    crate::ui::panel::ClientIntent(vec![0x61, 0, 0])
}

/// The portrait's pointer area at column `x`.
pub fn hit_rect(x: i32) -> Rect {
    Rect::new(
        x,
        HIT_TOP,
        (BAR_WIDTH + 1) as u16,
        (ICON_BOTTOM - HIT_TOP + 1) as u16,
    )
}

/// The panel.
pub struct MercPortraitUi {
    pub(super) sh: SharedRef,
    /// When the pointer came onto the portrait (UI clock), while it stays.
    pub(super) hover_since: Cell<Option<u32>>,
}

impl MercPortraitUi {
    /// The portrait's left column in UI box space.
    fn left() -> i32 {
        crate::ui::ui_frame::UiFrame::current().left() + ICON_LEFT
    }

    /// The portrait is drawn: not with the game menu (ui 9) or ui 0x0B
    /// open, nor in open modes 2 and 3.
    fn shown(&self) -> bool {
        let sh = self.sh.borrow();
        let mode = sh.states.open_mode().get();
        !(sh.states.is_open(9) || sh.states.is_open(0x0B) || mode == 2 || mode == 3)
    }

    /// How long the pointer has rested on the portrait at `now`; resets
    /// when it is elsewhere.
    fn rested(&self, pointer: Option<Point>, now: u32) -> Option<u32> {
        let on = pointer.is_some_and(|p| hit_rect(Self::left()).contains(p));
        if !on {
            self.hover_since.set(None);
            return None;
        }
        let since = self.hover_since.get().unwrap_or_else(|| {
            self.hover_since.set(Some(now));
            now
        });
        Some(now.wrapping_sub(since))
    }
}

impl Panel for MercPortraitUi {
    fn id(&self) -> PanelId {
        MERC_PORTRAIT_PANEL
    }

    fn rect(&self) -> Rect {
        hit_rect(Self::left())
    }

    fn draw(&self, ctx: &UiCtx, out: &mut dyn UiDrawSink) {
        if local_hireling(ctx.world).is_none() {
            self.hover_since.set(None);
        }
        if !self.shown() {
            return;
        }
        let sh = self.sh.borrow();
        let now = sh.ui_ms32(ctx.tick);
        let clip = crate::ui::ui_frame::UiFrame::current().rect();
        let text_width = |t: &[u16]| {
            sh.fonts
                .as_ref()
                .and_then(|f| f.width_a(LABEL_FONT, t))
                .unwrap_or(0)
        };
        let mut label_y = LABEL_Y;
        let (mut caption_x, mut caption_y) = (0, None);
        // A name stays on the frame: not left of its first column.
        let frame_left = crate::ui::ui_frame::UiFrame::current().left();
        for icon in pet_icon_row(ctx.world, &sh.pet_icons, Self::left()) {
            if let (Some(pct), Some(colors)) = (icon.life, sh.merc_bar_colors) {
                for r in bar_rects(icon.x, pct, colors) {
                    out.push(UiDraw::Rect(r));
                }
            }
            // Brightened: rested on with an empty cursor, or under the
            // pointer holding a potion or an item she can wear.
            let rested = self.rested(ctx.pointer(), now);
            let holds_usable = matches!(
                give_check(
                    ctx.world,
                    sh.items.tips.as_ref(),
                    sh.items.inv_tables.as_deref(),
                ),
                Some(Give::Potion | Give::Equip)
            );
            let hovered = icon.hireling
                && match crate::bridge::items::cursor_item(ctx.world) {
                    None => rested.is_some_and(|ms| ms >= HOVER_MS),
                    Some(_) => rested.is_some() && holds_usable,
                };
            if let Some(file) = sh.tables.files.id(&icon.file) {
                out.push(UiDraw::Image(ImageRequest {
                    image: ImageRef { file, frame: 0 },
                    at: Point::new(icon.x, ICON_BOTTOM),
                    clip,
                    look: CelLook {
                        mode: if hovered { MODE_HOVER } else { MODE_PLAIN },
                        remap: Remap::None,
                    },
                    call: CelCall::Draw,
                }));
            }
            if let Some(n) = icon.count {
                let text = fit(
                    n.to_string().encode_utf16().collect(),
                    COUNT_WIDTH,
                    &text_width,
                );
                out.push(label(text, Point::new(icon.x + COUNT_DX, COUNT_Y), clip));
            }
            // The pet type's name under its icon; the caption rows take
            // turns between [`LABEL_Y`] and ten rows lower.
            if icon.hireling {
                caption_y = Some(
                    if label_y == LABEL_Y {
                        LABEL_Y + LABEL_STEP
                    } else {
                        LABEL_Y
                    } + CAPTION_DY,
                );
                caption_x = icon.x;
            }
            // The hireling's own name; a pet type's name.
            let name_id = if icon.hireling && icon.name == MERC_NAME_ALT_ID {
                STR_MERC_NAME_ALT
            } else {
                icon.name
            };
            if (icon.hireling || name_id != NO_NAME) && name_id != 0 {
                let name = ctx.strings.get_id(name_id).map(<[u16]>::to_vec);
                if let Some(t) = name {
                    let t = fit(t, LABEL_WIDTH, &text_width);
                    let x = (icon.x + LABEL_DX - text_width(&t) / 2).max(frame_left);
                    out.push(label(t, Point::new(x, label_y), clip));
                }
            }
            label_y = if label_y == LABEL_Y {
                LABEL_Y + LABEL_STEP
            } else {
                LABEL_Y
            };
        }
        // The hireling's caption, once the pointer has rested on her
        // portrait with nothing on the cursor: [`STR_CAPTION_OPEN`] with the
        // hireling key's name, a new line, [`STR_CAPTION_POTION`], as the
        // pop-up text at the portrait's column, [`CAPTION_DY`] rows under
        // her name's turn.
        let hovered = caption_y.is_some()
            && crate::bridge::items::cursor_item(ctx.world).is_none()
            && self
                .rested(ctx.pointer(), now)
                .is_some_and(|ms| ms >= HOVER_MS);
        if let (true, Some(y)) = (hovered, caption_y) {
            let s = |id| {
                ctx.strings
                    .get_id(id)
                    .map(<[u16]>::to_vec)
                    .unwrap_or_default()
            };
            let key = crate::ui::original::hud::key_names(sh.hud.bindings.as_ref(), HIRELING_CMD)
                [0]
            .clone()
            .unwrap_or_default();
            let mut text = crate::ui::panels::npc_menu::fmt_ws(&s(STR_CAPTION_OPEN), &[key]);
            text.extend(s(STR_NEW_LINE));
            text.extend(s(STR_CAPTION_POTION));
            crate::ui::original::hud_tips::push_popup(
                text,
                Point::new(caption_x, y),
                0,
                false,
                (sh.config.screen.w, sh.config.screen.h),
                sh.fonts.as_ref(),
                out,
            );
        }
    }

    fn hit(&self, p: Point) -> Option<WidgetId> {
        self.rect().contains(p).then_some(WidgetId(0))
    }

    fn event(&mut self, e: UiEvent, ctx: &UiCtx) -> UiResponse {
        if local_hireling(ctx.world).is_none() || !self.shown() {
            return UiResponse::Ignored;
        }
        let now = self.sh.borrow().ui_ms32(ctx.tick);
        match e {
            UiEvent::CursorMoved(p) => {
                self.rested(Some(p), now);
                UiResponse::Ignored
            }
            // A held item dropped on the portrait ([`give_rule`]): a potion
            // or an item she can use goes to her (C→S 0x61), the potion with
            // the player's speech event; an item she cannot use is refused
            // with it. The press is taken when an item is held.
            UiEvent::Press {
                button: PointerButton::Left,
                ..
            } => {
                let give = {
                    let sh = self.sh.borrow();
                    give_check(
                        ctx.world,
                        sh.items.tips.as_ref(),
                        sh.items.inv_tables.as_deref(),
                    )
                };
                let Some(give) = give else {
                    return UiResponse::Ignored;
                };
                let mut sh = self.sh.borrow_mut();
                match give {
                    Give::Potion => {
                        sh.outputs.push(PanelOutput::PlayerEvent(SPEECH_POTION));
                        sh.outputs.push(PanelOutput::Intent(give_message()));
                    }
                    Give::Equip => sh.outputs.push(PanelOutput::Intent(give_message())),
                    Give::Unmet => {}
                    Give::Refused(speech) => sh.outputs.push(PanelOutput::PlayerEvent(speech)),
                }
                UiResponse::Consumed
            }
            // The right button opens the hireling panel: its release, after
            // the pointer has rested on the portrait for [`HOVER_MS`]; the
            // press is taken.
            UiEvent::Press {
                button: PointerButton::Right,
                ..
            } => UiResponse::Consumed,
            UiEvent::Release {
                button: PointerButton::Right,
                at,
            } => {
                if self.rested(Some(at), now).is_some_and(|ms| ms >= HOVER_MS) {
                    self.sh.borrow_mut().outputs.push(PanelOutput::SetUi {
                        ui: UI_HIRELING_PANEL,
                        mode: 2,
                        jump: true,
                    });
                }
                UiResponse::Consumed
            }
            _ => UiResponse::Ignored,
        }
    }
}

#[cfg(test)]
#[path = "merc_portrait_tests.rs"]
mod tests;
