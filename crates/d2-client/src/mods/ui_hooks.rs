//! The UI layout hooks of a mod game (E-UI1…E-UI5): settings and
//! replacements a mod game whose UI is laid out differently from
//! D2Gamma's installs on its [`ClientMods`]. With none set the client lays
//! its UI out exactly as D2Gamma does.
//!
//! - [`ClientMods::with_ui_frame_min`]: the smallest UI frame the mod
//!   game's panels are laid out for (`crate::ui::ui_space::set_ui_floor`,
//!   [`crate::ui::ui_frame::UiFrame`]).
//! - [`ClientMods::with_game_panel`]: a mod game's own panel in place of
//!   one of the game's panels ([`GamePanel`]), or for a UI state the game
//!   draws no panel for. Mod panels lay themselves out with
//!   [`crate::ui::ui_frame::UiFrame`] (`UiCtx::ui_frame`) and read the
//!   pointer with `UiCtx::pointer`.
//! - [`ClientMods::with_hud`]: the mod game's own HUD in place of the
//!   control panel, orbs, skill buttons, belt and mini menu, with the rows
//!   it covers; [`ClientMods::without_panel_shift`]: open side panels
//!   leave the view where it is.
//! - The front end: [`ClientMods::with_front_screen`] puts the mod game's
//!   own screens in place of the menus and character screens,
//!   [`ClientMods::with_front_canvas`] sets the canvas they are drawn on,
//!   and [`ClientMods::with_menu_sides_default`] the fill beside it. The
//!   palette a screen draws with is its own
//!   (`crate::ui::front_end::Screen::palette`); the game's name and version
//!   line are its identity's.
//! - The world: [`ClientMods::with_map_marks`] adds marks to the automap,
//!   [`ClientMods::with_level_backdrops`] draws a backdrop behind a level.

use std::sync::Arc;

use super::{ClientMods, PanelFactory, PanelPlace};
use crate::app::config::MenuSides;
use crate::rules::camera::FrameSize;
use crate::ui::front_end::{FrontEnd, Screen, ScreenId};
use crate::ui::panel::PanelId;
use crate::ui::states::id;

/// The UI layout settings of a mod game, held by [`ClientMods`].
#[derive(Clone, Default)]
pub(super) struct UiHooks {
    /// [`ClientMods::with_front_screen`], in registration order.
    front_screens: Vec<(ScreenId, Arc<dyn FrontScreenFactory>)>,
    /// [`ClientMods::with_front_canvas`].
    front_canvas: Option<FrameSize>,
    /// [`ClientMods::with_menu_sides_default`].
    menu_sides: Option<MenuSides>,
    /// [`ClientMods::with_map_marks`], in registration order.
    map_marks: Vec<Arc<dyn MapMarks>>,
    /// [`ClientMods::with_level_backdrops`], in registration order.
    backdrops: Vec<Arc<dyn LevelBackdrops>>,
    /// [`ClientMods::with_ui_frame_min`].
    ui_frame_min: Option<FrameSize>,
    /// [`ClientMods::with_hud`]: the UI rows the mod game's HUD covers.
    hud_rows: Option<i32>,
    /// [`ClientMods::without_panel_shift`].
    no_panel_shift: bool,
}

impl UiHooks {
    pub(super) fn is_empty(&self) -> bool {
        self.front_screens.is_empty()
            && self.map_marks.is_empty()
            && self.backdrops.is_empty()
            && self.front_canvas.is_none()
            && self.menu_sides.is_none()
            && self.ui_frame_min.is_none()
            && self.hud_rows.is_none()
            && !self.no_panel_shift
    }
}

impl std::fmt::Debug for UiHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiHooks")
            .field(
                "front_screens",
                &self
                    .front_screens
                    .iter()
                    .map(|(id, _)| id.0)
                    .collect::<Vec<_>>(),
            )
            .field("map_marks", &self.map_marks.len())
            .field("backdrops", &self.backdrops.len())
            .field("front_canvas", &self.front_canvas)
            .field("menu_sides", &self.menu_sides)
            .field("ui_frame_min", &self.ui_frame_min)
            .field("hud_rows", &self.hud_rows)
            .field("no_panel_shift", &self.no_panel_shift)
            .finish()
    }
}

/// One mark a mod game adds to the automap ([`MapMarks`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapMark {
    /// The world position, in client pixels (where a unit there has its
    /// marker).
    pub at: crate::rules::camera::ClientPos,
    /// The colour of the mark's `shape` (matched in the act's palette as
    /// the unit markers' colours are); `None`: no shape.
    pub cross: Option<(u8, u8, u8)>,
    /// An icon centred on the mark: a DC6 file (no extension) from the
    /// game's files and its frame.
    pub icon: Option<(String, u32)>,
    /// A label above the mark (UTF-16; empty: none) in the marker names'
    /// font, text colour `label_color`.
    pub label: Vec<u16>,
    pub label_color: u16,
    /// The shape drawn in the `cross` colour: the unit markers' cross,
    /// or line segments of the mod game's own.
    pub shape: MarkShape,
    /// The label's font: the marker names' font (6) by default.
    pub label_font: u16,
    /// Automap rows from the mark's point to the label's top: −10 (as the
    /// marker names') by default.
    pub label_dy: i32,
}

/// The shape of a [`MapMark`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MarkShape {
    /// The unit markers' cross.
    #[default]
    Cross,
    /// Line segments through these points in turn, in automap pixels from
    /// the mark's point (x right, y down): a star, a ring, an arrow. The
    /// same on the minimap.
    Path(Vec<(i32, i32)>),
}

impl MapMark {
    /// A cross of colour `rgb` at `at`, without icon or label (a label
    /// set later draws in the marker names' font and place).
    pub fn cross(at: crate::rules::camera::ClientPos, rgb: (u8, u8, u8)) -> MapMark {
        MapMark {
            at,
            cross: Some(rgb),
            icon: None,
            label: Vec::new(),
            label_color: 0,
            shape: MarkShape::Cross,
            label_font: 6,
            label_dy: -10,
        }
    }

    /// The mark with `shape` (in the `cross` colour).
    pub fn with_shape(mut self, shape: MarkShape) -> MapMark {
        self.shape = shape;
        self
    }

    /// The mark with label `text` in colour `color`, font `font`, its top
    /// `dy` automap rows from the mark's point.
    pub fn with_label(mut self, text: Vec<u16>, color: u16, font: u16, dy: i32) -> MapMark {
        (self.label, self.label_color, self.label_font, self.label_dy) = (text, color, font, dy);
        self
    }
}

/// A source of automap marks (E-UI5): the places a mod game shows on the
/// map (loot it highlights, its own map icons). Asked each frame the
/// automap is drawn; the marks are drawn after the unit markers, inside
/// the marker rectangle.
pub trait MapMarks: Send + Sync {
    fn marks(&self, world: &crate::bridge::world::ClientWorld, out: &mut Vec<MapMark>);
}

/// A backdrop drawn behind a level (E-UI5), first in the world draw:
/// the frames of a DC6 file laid out as a grid, scaled to cover the frame
/// and centred (`crate::world_view::backdrop_cover`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelBackdrop {
    /// The DC6 file (no extension) in the game's files.
    pub file: String,
    /// Frames per grid row (frame 0 top-left, left to right).
    pub cols: u32,
    /// The game's own background of the level (if any) is left out.
    pub replaces_game: bool,
}

/// A mod game's level backdrops (E-UI5).
pub trait LevelBackdrops: Send + Sync {
    /// The backdrop of level `level`; `None`: the game's own (if any).
    fn backdrop(&self, level: u32) -> Option<LevelBackdrop>;
}

/// Makes a mod game's front-end screen ([`ClientMods::with_front_screen`]).
/// Screens hold state that is not shared between threads, so the registry
/// holds the factory and the front end makes the screen each time it
/// starts.
pub trait FrontScreenFactory: Send + Sync {
    fn make(&self) -> Box<dyn Screen>;
}

impl ClientMods {
    /// Sets the smallest UI frame the mod game's panels are laid out for:
    /// the UI frame never drops below it, whatever the UI Scale setting
    /// (the slider stops where it would). A floor taller or wider than the
    /// play frame draws the UI smaller than the picture at every setting
    /// (panels laid out for 1024 × 768 on the 1280 × 720 frame get a 1366 ×
    /// 768 UI frame), drawn by the native UI pass at every scale.
    /// D2Gamma's floor is the 800 × 600 box (`crate::ui::ui_space::MIN_UI`).
    pub fn with_ui_frame_min(mut self, floor: FrameSize) -> ClientMods {
        self.ui.ui_frame_min = Some(floor);
        self
    }

    /// The UI frame floor the mod game set, if any.
    pub fn ui_frame_min(&self) -> Option<FrameSize> {
        self.ui.ui_frame_min
    }

    /// Puts the mod game's own HUD in place of the game's: the factory's
    /// panel (it must report [`HUD_PANEL`]) replaces the control panel's
    /// overlays (orbs, skill buttons, belt, mini menu and their input), the
    /// border, control panel and side bars are no longer drawn, the world
    /// is drawn under the HUD's rows, and presses and hovers reach the
    /// world down to the row above the HUD's `rows` bottom UI rows
    /// ([`crate::ui::ui_frame::UiFrame::bottom_band`]).
    pub fn with_hud(mut self, rows: i32, factory: impl PanelFactory + 'static) -> ClientMods {
        self.ui.hud_rows = Some(rows.max(0));
        self.with_panel(PanelPlace::Replace, factory)
    }

    /// The UI rows the mod game's own HUD covers, if it has one.
    pub fn hud_rows(&self) -> Option<i32> {
        self.ui.hud_rows
    }

    /// Open side panels leave the view (and the cursor) where it is: for a
    /// mod game whose panels leave the player in view between them. D2Gamma
    /// moves the view by half a panel side.
    pub fn without_panel_shift(mut self) -> ClientMods {
        self.ui.no_panel_shift = true;
        self
    }

    /// Puts the mod game's own screen in place of front-end screen `id`
    /// (`crate::ui::front_end::screens::ids`: the trademark splash, the main
    /// menu, character select and create, difficulty, credits, loading),
    /// with the same flow: the screen fires the flow's triggers as the
    /// game's does.
    pub fn with_front_screen(
        mut self,
        id: ScreenId,
        factory: impl FrontScreenFactory + 'static,
    ) -> ClientMods {
        self.ui.front_screens.push((id, Arc::new(factory)));
        self
    }

    /// Draws the front end on a `canvas` picture instead of 800 × 600 (a
    /// mod game's menus laid out for 1024 × 768), presented in the window
    /// as the 800 × 600 picture is, with the side fill beside it on a wider
    /// window.
    pub fn with_front_canvas(mut self, canvas: FrameSize) -> ClientMods {
        self.ui.front_canvas = Some(canvas);
        self
    }

    /// The menu side fill a player who never chose one gets
    /// (`[video] menu_sides`; D2Gamma's is black).
    pub fn with_menu_sides_default(mut self, sides: MenuSides) -> ClientMods {
        self.ui.menu_sides = Some(sides);
        self
    }

    /// Adds a source of automap marks.
    pub fn with_map_marks(mut self, hook: impl MapMarks + 'static) -> ClientMods {
        self.ui.map_marks.push(Arc::new(hook));
        self
    }

    /// This frame's automap marks of every source, in registration order.
    pub fn map_marks(&self, world: &crate::bridge::world::ClientWorld) -> Vec<MapMark> {
        let mut out = Vec::new();
        for hook in &self.ui.map_marks {
            hook.marks(world, &mut out);
        }
        out
    }

    /// Adds a source of level backdrops.
    pub fn with_level_backdrops(mut self, hook: impl LevelBackdrops + 'static) -> ClientMods {
        self.ui.backdrops.push(Arc::new(hook));
        self
    }

    /// The backdrop of `level`: the first source's that has one.
    pub fn level_backdrop(&self, level: u32) -> Option<LevelBackdrop> {
        self.ui.backdrops.iter().find_map(|h| h.backdrop(level))
    }

    /// Registers the mod game's own screens in `front` (when the front end
    /// starts).
    pub fn apply_front(&self, front: &mut FrontEnd) {
        for (id, factory) in &self.ui.front_screens {
            front.register_screen(*id, factory.make());
        }
    }

    /// Applies the mod game's process-wide UI settings (the UI frame
    /// floor, the HUD's rows, the panel shift, the front canvas, the menu
    /// sides default); D2Gamma's own where it set none. Called when the
    /// front end starts and when the play UI is installed; a mod game's
    /// binary calls it first, before the settings are read, so its menu
    /// sides default applies to them.
    pub fn apply_ui_settings(&self) {
        crate::app::front_host::set_front_canvas(self.ui.front_canvas);
        MenuSides::set_game_default(self.ui.menu_sides.unwrap_or(MenuSides::Black));
        crate::ui::ui_space::set_ui_floor(
            self.ui.ui_frame_min.unwrap_or(crate::ui::ui_space::MIN_UI),
        );
        crate::ui::ui_frame::set_mod_hud_rows(self.ui.hud_rows);
        crate::rules::camera::set_panel_shift(!self.ui.no_panel_shift);
    }
}

/// A panel of the game a mod game's panel can stand in for
/// ([`ClientMods::with_game_panel`]), by the UI state or role it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GamePanel {
    /// The inventory (right side); drawn while any panel of its family
    /// (stash, cube, store) is open too.
    Inventory,
    /// The character sheet (left side).
    Character,
    /// The skill tree (right side).
    SkillTree,
    /// The stash (left side).
    Stash,
    /// The cube (left side).
    Cube,
    /// An NPC's store (left side).
    Store,
    /// The waypoint list (left side).
    Waypoints,
    /// The quest log (left side).
    Quests,
    /// An NPC's talk menu.
    NpcMenu,
    /// The hireling list an NPC offers.
    HireList,
    /// The message log.
    MessageLog,
    /// The Esc game menu with its option pages.
    EscMenu,
    /// The party screen: a UI state the game draws no panel for.
    Party,
    /// The hireling's sheet and gear: a UI state the game draws no panel
    /// for.
    HirelingSheet,
}

impl GamePanel {
    pub const ALL: [GamePanel; 14] = [
        GamePanel::Inventory,
        GamePanel::Character,
        GamePanel::SkillTree,
        GamePanel::Stash,
        GamePanel::Cube,
        GamePanel::Store,
        GamePanel::Waypoints,
        GamePanel::Quests,
        GamePanel::NpcMenu,
        GamePanel::HireList,
        GamePanel::MessageLog,
        GamePanel::EscMenu,
        GamePanel::Party,
        GamePanel::HirelingSheet,
    ];

    /// The panel's id: the id the mod game's panel must report. A UI
    /// state's panel has the state's number as its id, so it opens and
    /// closes with the state.
    pub fn id(self) -> PanelId {
        let state = |s: u8| PanelId(u16::from(s));
        match self {
            GamePanel::Inventory => state(id::INVENTORY),
            GamePanel::Character => state(id::CHARACTER),
            GamePanel::SkillTree => state(id::SKILL_TREE),
            GamePanel::Stash => state(id::STASH),
            GamePanel::Cube => state(id::CUBE),
            GamePanel::Store => state(id::NPC_SHOP),
            GamePanel::Waypoints => state(id::WAYPOINT),
            GamePanel::Quests => state(id::QUEST_SCREEN),
            GamePanel::NpcMenu => state(id::NPC_MENU),
            GamePanel::HireList => crate::ui::hire_list::HIRE_PANEL,
            GamePanel::MessageLog => state(MESSAGE_LOG),
            GamePanel::EscMenu => state(id::ESC_MENU),
            GamePanel::Party => state(id::PARTY),
            GamePanel::HirelingSheet => state(id::MERC_INV),
        }
    }

    /// Whether the game draws a panel of its own here (one to replace);
    /// else the mod game's panel is added and follows the UI state.
    pub fn is_built_in(self) -> bool {
        !matches!(self, GamePanel::Party | GamePanel::HirelingSheet)
    }
}

/// The id of the game's HUD panel, the one a mod game's own HUD reports
/// ([`ClientMods::with_hud`]).
pub const HUD_PANEL: PanelId = crate::ui::original::hud::HUD_PANEL;

/// The message log's UI state.
const MESSAGE_LOG: u8 = 0x18;

impl ClientMods {
    /// Puts the mod game's panel in `which`'s place: it replaces the
    /// game's panel there (same draw and input order, same open state), or,
    /// for a UI state the game draws no panel for, is added closed and
    /// opens and closes with that state. The factory's panel must report
    /// [`GamePanel::id`]; installing the UI fails otherwise.
    pub fn with_game_panel(
        self,
        which: GamePanel,
        factory: impl PanelFactory + 'static,
    ) -> ClientMods {
        let place = if which.is_built_in() {
            PanelPlace::Replace
        } else {
            PanelPlace::Add { open: false }
        };
        self.with_panel(place, factory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::ui_space::{ui_floor, UiSpace, MIN_UI};

    const FLOOR: FrameSize = FrameSize {
        width: 1024,
        height: 768,
    };

    /// A mod game's floor reaches the UI space when the UI is installed:
    /// the 1280 × 720 frame lays its UI out on 1366 × 768.
    #[test]
    fn the_mod_games_floor_sets_the_ui_frame() {
        let mods = ClientMods::new().with_ui_frame_min(FLOOR);
        assert!(!mods.is_empty());
        assert_eq!(mods.ui_frame_min(), Some(FLOOR));
        mods.apply_ui_settings();
        assert_eq!(ui_floor(), FLOOR);
        assert_eq!(
            UiSpace::new(FrameSize::PLAY, 100).ui,
            FrameSize {
                width: 1366,
                height: 768
            }
        );
        ClientMods::new().apply_ui_settings();
        assert_eq!(ui_floor(), MIN_UI);
    }

    /// No hook: D2Gamma's floor and UI frames.
    #[test]
    fn no_floor_keeps_d2gammas_ui_frame() {
        let mods = ClientMods::new();
        assert!(mods.is_empty());
        assert_eq!(mods.ui_frame_min(), None);
        mods.apply_ui_settings();
        assert_eq!(ui_floor(), MIN_UI);
        assert!(UiSpace::new(FrameSize::PLAY, 100).is_identity());
    }

    use crate::bridge::world::ClientWorld;
    use crate::ui::draw::{TextRequest, TextStyle, UiDraw, UiDrawSink};
    use crate::ui::geom::{Point, Rect, FRAME};
    use crate::ui::layout::Screen;
    use crate::ui::original::{OriginalUi, UiConfig};
    use crate::ui::panel::{Panel, UiCtx, UiEvent, UiResponse, WidgetId};
    use crate::ui::panels::UiFiles;
    use crate::ui::states::UiStates;
    use crate::ui::text::TextOpts;
    use crate::ui::{NoPanelRules, NoStrings, UiRoot};

    /// A mod game's panel standing in for `id`: one text line naming it,
    /// in the left slot of the UI frame.
    struct StandIn {
        id: PanelId,
    }

    impl Panel for StandIn {
        fn id(&self) -> PanelId {
            self.id
        }
        fn rect(&self) -> Rect {
            Rect::new(0, 0, 50, 50)
        }
        fn draw(&self, ctx: &UiCtx, out: &mut dyn UiDrawSink) {
            let slot = ctx
                .ui_frame()
                .slot(crate::ui::ui_frame::SlotSide::Left, 512, 704);
            out.push(UiDraw::Text(TextRequest {
                text: stand_in_text(self.id),
                at: slot.origin(),
                style: TextStyle { font: 1, color: 0 },
                opts: TextOpts::default(),
                clip: FRAME,
            }));
        }
        fn hit(&self, _p: Point) -> Option<WidgetId> {
            None
        }
        fn event(&mut self, _e: UiEvent, _ctx: &UiCtx) -> UiResponse {
            UiResponse::Ignored
        }
    }

    struct MakeStandIn(PanelId);

    impl PanelFactory for MakeStandIn {
        fn make(&self, _files: &UiFiles) -> Box<dyn Panel> {
            Box::new(StandIn { id: self.0 })
        }
    }

    fn stand_in_text(id: PanelId) -> Vec<u16> {
        format!("stand-in {:#x}", id.0).encode_utf16().collect()
    }

    fn installed(mods: ClientMods) -> UiRoot {
        installed_ui(mods).1
    }

    fn installed_ui(mods: ClientMods) -> (OriginalUi, UiRoot) {
        let mut ui = OriginalUi::new(
            UiConfig {
                screen: Screen::PLAY,
                expansion_installed: true,
            },
            None,
        )
        .unwrap();
        ui.set_mods(mods);
        let mut root = UiRoot::new(Box::new(NoPanelRules));
        ui.install(&mut root).unwrap();
        (ui, root)
    }

    /// `root` with only UI state `state` open (none for a panel that is
    /// not a UI state's).
    fn with_state(root: &mut UiRoot, state: Option<u8>) {
        let mut states = UiStates::new().unwrap();
        if let Some(s) = state {
            states.force(s, true);
        }
        root.sync_states(&states);
    }

    fn draws_stand_in(root: &UiRoot, id: PanelId) -> bool {
        let world = ClientWorld::default();
        let ctx = UiCtx {
            tick: 0,
            world: &world,
            strings: &NoStrings,
        };
        let mut out: Vec<UiDraw> = Vec::new();
        root.draw(&ctx, &mut out);
        out.iter()
            .any(|d| matches!(d, UiDraw::Text(t) if t.text == stand_in_text(id)))
    }

    fn state_of(id: PanelId) -> Option<u8> {
        (usize::from(id.0) < crate::ui::layout::UI_STATE_COUNT).then_some(id.0 as u8)
    }

    /// Every built-in panel a mod game names is replaced in place: the
    /// root opens the same panels as the game's own, and the mod game's
    /// panel draws while its state is open.
    #[test]
    fn a_mod_panel_stands_in_for_each_game_panel() {
        for which in GamePanel::ALL {
            let id = which.id();
            let mut plain = installed(ClientMods::new());
            let mut root = installed(ClientMods::new().with_game_panel(which, MakeStandIn(id)));
            let state = state_of(id);
            with_state(&mut plain, state);
            with_state(&mut root, state);
            if which.is_built_in() {
                assert_eq!(plain.open_panels(), root.open_panels(), "{which:?}");
            } else {
                // A state the game draws no panel for: the mod panel is
                // the only one added, open with its state.
                let mut extra = root.open_panels();
                extra.retain(|p| !plain.open_panels().contains(p));
                assert_eq!(extra, vec![id], "{which:?}");
            }
            assert!(root.open_panels().contains(&id), "{which:?}");
            assert!(draws_stand_in(&root, id), "{which:?}");
            assert!(!draws_stand_in(&plain, id), "{which:?}");
            // Closed again with its state.
            if state.is_some() {
                with_state(&mut root, None);
                assert!(!draws_stand_in(&root, id), "{which:?}");
            }
        }
    }

    /// The mod game's HUD replaces the game's: the border, control panel
    /// and side bars are gone, its rows bound world clicks, and the world
    /// draws under it. Without one, D2Gamma's control panel and click
    /// limit.
    #[test]
    fn a_mod_hud_replaces_the_control_panel() {
        use crate::ui::ui_frame::{mod_hud_rows, world_click_limit};
        let image_files = |root: &UiRoot| {
            let world = ClientWorld::default();
            let ctx = UiCtx {
                tick: 0,
                world: &world,
                strings: &NoStrings,
            };
            let mut out: Vec<UiDraw> = Vec::new();
            root.draw(&ctx, &mut out);
            out.iter()
                .filter_map(|d| match d {
                    UiDraw::Image(r) => Some(r.image.file),
                    _ => None,
                })
                .collect::<Vec<u32>>()
        };
        let (plain_ui, plain) = installed_ui(ClientMods::new());
        let ctrlpnl = plain_ui.files().id(r"panel\800ctrlpnl7").unwrap();
        assert!(image_files(&plain).contains(&ctrlpnl));
        let mods = ClientMods::new().with_hud(64, MakeStandIn(HUD_PANEL));
        assert_eq!(mods.hud_rows(), Some(64));
        let (_, root) = installed_ui(mods.clone());
        assert!(!image_files(&root).contains(&ctrlpnl));
        assert!(draws_stand_in(&root, HUD_PANEL));
        assert_eq!(plain.open_panels(), root.open_panels());

        let space = UiSpace::with_floor(FrameSize::PLAY, 100, FLOOR);
        mods.apply_ui_settings();
        assert_eq!(mod_hud_rows(), Some(64));
        // 64 of 768 UI rows: 60 of 720 frame rows.
        assert_eq!(world_click_limit(space), 720 - 60 - 1);
        ClientMods::new().apply_ui_settings();
        assert_eq!(mod_hud_rows(), None);
        let play = UiSpace::identity(FrameSize::PLAY);
        assert_eq!(world_click_limit(play), FrameSize::PLAY.skill_y_limit());
    }

    /// Without the panel shift an open side panel leaves the view and the
    /// cursor where they are; D2Gamma moves them.
    #[test]
    fn panels_can_leave_the_view_in_place() {
        use crate::rules::camera::{OpenMode, ViewRect};
        let right = OpenMode::new(1).unwrap();
        assert_eq!(ViewRect::new(FrameSize::PLAY, right).shift_x, -160);
        assert_eq!(Screen::PLAY.panel_shift(), 160);
        ClientMods::new().without_panel_shift().apply_ui_settings();
        assert_eq!(ViewRect::new(FrameSize::PLAY, right).shift_x, 0);
        assert_eq!(Screen::PLAY.panel_shift(), 0);
        ClientMods::new().apply_ui_settings();
        assert_eq!(ViewRect::new(FrameSize::PLAY, right).shift_x, -160);
    }

    /// A mod game's front-end screen replaces the game's, the current one
    /// too; others stay the game's.
    #[test]
    fn a_mod_screen_replaces_a_front_end_screen() {
        use crate::ui::front_end::control::{vk, Action, Control};
        use crate::ui::front_end::flow::Trigger;
        use crate::ui::front_end::{FrontCtx, Screen as FrontScreen, MAIN_MENU};

        /// One key-only control: Esc exits (a screen with no art).
        struct OwnMenu;
        impl FrontScreen for OwnMenu {
            fn build(&mut self, _ctx: &mut FrontCtx) -> Vec<Control> {
                vec![Control::key_only(vk::ESC, Action::Trigger(Trigger::Exit))]
            }
            fn wide(&self) -> bool {
                true
            }
        }
        struct MakeOwnMenu;
        impl FrontScreenFactory for MakeOwnMenu {
            fn make(&self) -> Box<dyn FrontScreen> {
                Box::new(OwnMenu)
            }
        }

        let mut plain = FrontEnd::with_screens(true, Box::new(true));
        plain.start(false);
        assert_eq!(plain.current(), MAIN_MENU);
        assert!(!plain.wide());
        let mods = ClientMods::new().with_front_screen(MAIN_MENU, MakeOwnMenu);
        assert!(!mods.is_empty());
        let mut front = FrontEnd::with_screens(true, Box::new(true));
        front.start(false);
        mods.apply_front(&mut front);
        assert_eq!(front.current(), MAIN_MENU);
        assert!(front.wide(), "the mod's screen is the current one");
        assert_eq!(front.controls().len(), 1);
        assert_ne!(plain.controls().len(), 1);
        // No front hook: the game's screens.
        ClientMods::new().apply_front(&mut plain);
        assert!(!plain.wide());
    }

    /// The front canvas and the menu sides default reach the front host
    /// and the settings; without them, D2Gamma's.
    #[test]
    fn the_front_canvas_and_menu_sides_default_apply() {
        use crate::app::front_host::picture_shape;
        let mods = ClientMods::new()
            .with_front_canvas(FLOOR)
            .with_menu_sides_default(MenuSides::Extend);
        mods.apply_ui_settings();
        assert_eq!(picture_shape(), (1024, 768));
        assert_eq!(MenuSides::default(), MenuSides::Extend);
        assert_eq!(
            crate::app::config::Settings::default().menu_sides,
            MenuSides::Extend
        );
        ClientMods::new().apply_ui_settings();
        assert_eq!(picture_shape(), (800, 600));
        assert_eq!(MenuSides::default(), MenuSides::Black);
    }

    /// A mod game's marks reach the automap's draws at the markers'
    /// places: icon, cross and label; marks off the marker rectangle are
    /// not drawn. No hook, no marks.
    #[test]
    fn map_marks_are_drawn_with_the_markers() {
        use crate::rules::camera::ClientPos;
        use crate::ui::automap::draw::AutomapDraw;
        use crate::ui::automap::markers::{MarkerCtx, MarkerPalette};
        use crate::ui::automap::mod_marks::{mod_marks, ModMark};
        use crate::ui::automap::view::Bounds;

        struct TwoMarks;
        impl MapMarks for TwoMarks {
            fn marks(&self, _world: &ClientWorld, out: &mut Vec<MapMark>) {
                out.push(MapMark {
                    icon: Some(("mod\\map\\chest".into(), 2)),
                    label: "chest".encode_utf16().collect(),
                    label_color: 4,
                    ..MapMark::cross(ClientPos { x: 160, y: 320 }, (255, 0, 0))
                });
                out.push(MapMark::cross(ClientPos { x: 90_000, y: 0 }, (0, 255, 0)));
            }
        }
        let world = ClientWorld::default();
        assert!(ClientMods::new().map_marks(&world).is_empty());
        let mods = ClientMods::new().with_map_marks(TwoMarks);
        assert!(!mods.is_empty());
        let marks: Vec<ModMark> = mods
            .map_marks(&world)
            .into_iter()
            .map(|m| {
                let cross = m.cross.map(|_| 7);
                ModMark::of(m, cross)
            })
            .collect();
        assert_eq!(marks.len(), 2);
        let ctx = MarkerCtx {
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
        };
        let mut out = Vec::new();
        mod_marks(&marks, &ctx, &mut out);
        // (160 / 10 + 8, 320 / 10 − 8): the unit markers' placement.
        assert!(matches!(
            &out[0],
            AutomapDraw::Icon { frame: 2, x: 24, y: 24, path, .. } if path == "mod\\map\\chest"
        ));
        assert!(out[1..out.len() - 1]
            .iter()
            .all(|d| matches!(d, AutomapDraw::Line { color: 7, .. })));
        assert!(matches!(
            out.last(),
            Some(AutomapDraw::Text {
                color: 4,
                x: 24,
                y: 14,
                ..
            })
        ));
        // The second mark is far off the rectangle: nothing of it.
        assert!(out
            .iter()
            .all(|d| !matches!(d, AutomapDraw::Line { color: c, .. } if *c != 7)));
    }

    /// A level backdrop is the first source's; its frames lay out as a
    /// grid of its columns. No hook: none, the game's own backgrounds.
    #[test]
    fn level_backdrops_lay_their_frames_out_as_a_grid() {
        use crate::world_view::background_view::{backdrop_cells, BackdropCell};
        struct Summit;
        impl LevelBackdrops for Summit {
            fn backdrop(&self, level: u32) -> Option<LevelBackdrop> {
                (level == 120).then(|| LevelBackdrop {
                    file: "mod\\landscape\\summit".into(),
                    cols: 4,
                    replaces_game: true,
                })
            }
        }
        assert_eq!(ClientMods::new().level_backdrop(120), None);
        let mods = ClientMods::new().with_level_backdrops(Summit);
        assert_eq!(mods.level_backdrop(1), None);
        let b = mods.level_backdrop(120).unwrap();
        assert!(b.replaces_game);
        // 4 x 3 tiles of 256, the last row 192 tall: a 1024 x 704 grid.
        let size = |i: usize| (i < 12).then_some((256, if i >= 8 { 192 } else { 256 }));
        let cells = backdrop_cells(&b, size);
        assert_eq!(cells.len(), 12);
        assert_eq!(
            cells[5],
            BackdropCell {
                frame: 5,
                x: 256,
                y: 256,
                w: 256,
                h: 256
            }
        );
        assert_eq!(
            cells[11],
            BackdropCell {
                frame: 11,
                x: 768,
                y: 512,
                w: 256,
                h: 192
            }
        );
    }

    /// A stand-in that reports another id than the panel it names fails
    /// the UI's install, as any replacement of a panel the game lacks.
    #[test]
    fn a_stand_in_must_report_its_panels_id() {
        let mut ui = OriginalUi::new(
            UiConfig {
                screen: Screen::R800,
                expansion_installed: true,
            },
            None,
        )
        .unwrap();
        ui.set_mods(
            ClientMods::new().with_game_panel(GamePanel::Stash, MakeStandIn(PanelId(0x7FFF))),
        );
        let mut root = UiRoot::new(Box::new(NoPanelRules));
        assert!(ui.install(&mut root).is_err());
    }
}
