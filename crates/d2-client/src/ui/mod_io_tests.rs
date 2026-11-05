//! Mod panels reading the keys and hire offers, asking for the game's
//! requests, closing from outside, and a mod HUD's widgets above its rows;
//! D2Gamma's own UI without any of them.

use super::*;
use crate::bridge::world::{ClientUnit, ClientWorld, UnitKey, PLAYER};
use crate::mods::ui_hooks::{GamePanel, HUD_PANEL};
use crate::mods::{ClientMods, PanelFactory, PanelPlace, MOD_PANEL_IDS};
use crate::rules::camera::FrameSize;
use crate::ui::draw::UiDrawSink;
use crate::ui::geom::{Point, Rect};
use crate::ui::hire_list::{Offer, HIRE_PANEL};
use crate::ui::layout::Screen;
use crate::ui::original::UiConfig;
use crate::ui::panel::{
    OutsideClose, Panel, PanelId, PointerButton, UiEvent, UiResponse, WidgetId,
};
use crate::ui::panels::UiFiles;
use crate::ui::{NoPanelRules, NoStrings, Routed};

/// A mod panel: `rect`, one widget at `widget`, closing from outside as
/// `close` says, asking for `asks` when its widget is pressed.
#[derive(Clone)]
struct ModPanel {
    id: PanelId,
    rect: Rect,
    widget: Rect,
    close: OutsideClose,
    asks: Vec<ModRequest>,
    /// Hides the world only at its widget (a HUD's way).
    widget_covers_only: bool,
}

impl ModPanel {
    fn new(id: PanelId, rect: Rect) -> ModPanel {
        ModPanel {
            id,
            rect,
            widget: rect,
            close: OutsideClose::Never,
            asks: Vec::new(),
            widget_covers_only: false,
        }
    }
}

impl Panel for ModPanel {
    fn id(&self) -> PanelId {
        self.id
    }
    fn rect(&self) -> Rect {
        self.rect
    }
    fn draw(&self, _ctx: &UiCtx, _out: &mut dyn UiDrawSink) {}
    fn hit(&self, p: Point) -> Option<WidgetId> {
        self.widget.contains(p).then_some(WidgetId(1))
    }
    fn event(&mut self, e: UiEvent, ctx: &UiCtx) -> UiResponse {
        match e {
            UiEvent::Press { at, .. } if self.hit(at).is_some() => {
                for r in &self.asks {
                    ctx.request(r.clone());
                }
                UiResponse::Consumed
            }
            _ => UiResponse::Ignored,
        }
    }
    fn covers_world_at(&self, p: Point) -> bool {
        !self.widget_covers_only || self.hit(p).is_some()
    }
    fn outside_close(&self) -> OutsideClose {
        self.close
    }
}

impl PanelFactory for ModPanel {
    fn make(&self, _files: &UiFiles) -> Box<dyn Panel> {
        Box::new(self.clone())
    }
}

fn world() -> ClientWorld {
    let mut w = ClientWorld::default();
    let key = UnitKey::new(PLAYER, 1);
    let mut p = ClientUnit::new(key);
    // Standing (mode 1): a dead player opens no panel.
    p.mode = 1;
    w.units.insert(key, p);
    w.local_player = Some(key);
    w.expansion = 1;
    w
}

struct Ui {
    ui: OriginalUi,
    root: UiRoot,
    world: ClientWorld,
}

fn ui(mods: ClientMods) -> Ui {
    let mut ui = OriginalUi::new(
        UiConfig {
            screen: Screen::R800,
            expansion_installed: true,
        },
        None,
    )
    .unwrap();
    mods.apply_ui_settings();
    ui.set_mods(mods);
    let mut root = UiRoot::new(Box::new(NoPanelRules));
    ui.install(&mut root).unwrap();
    let mut u = Ui {
        ui,
        root,
        world: world(),
    };
    // The gate reads the player from the model at the first event.
    u.send(UiEvent::CursorMoved(Point::new(799, 0)));
    u
}

impl Ui {
    fn send(&mut self, e: UiEvent) -> Routed {
        let ctx = UiCtx {
            tick: 0,
            world: &self.world,
            strings: &NoStrings,
        };
        self.ui.before_event(e, &self.world);
        let r = self.root.dispatch(e, &ctx);
        self.ui.after_event(&mut self.root, e, r).unwrap();
        r
    }

    fn press(&mut self, x: i32, y: i32) -> Routed {
        let at = Point::new(x, y);
        let r = self.send(UiEvent::Press {
            button: PointerButton::Left,
            at,
        });
        self.send(UiEvent::Release {
            button: PointerButton::Left,
            at,
        });
        r
    }

    fn with_ctx<R>(&self, f: impl FnOnce(&UiCtx) -> R) -> R {
        f(&UiCtx {
            tick: 0,
            world: &self.world,
            strings: &NoStrings,
        })
    }
}

const CHARACTER: u8 = 2;
const LEFT_HALF: Rect = Rect::new(0, 0, 320, 432);

/// A mod HUD spanning the frame with an orb above its 64 rows: the orb
/// takes hovers and presses before the world, the rest of the frame above
/// the rows reaches the world, the rows themselves do not.
#[test]
fn a_mod_huds_widgets_above_its_rows_come_before_the_world() {
    use crate::ui::ui_space::UiSpace;
    use crate::world_view::present::pick_point_in;
    let mut hud = ModPanel::new(HUD_PANEL, Rect::new(0, 0, 800, 600));
    hud.widget = Rect::new(0, 440, 100, 100);
    hud.widget_covers_only = true;
    let mut u = ui(ClientMods::new().with_hud(64, hud));
    let space = UiSpace::identity(FrameSize::D2RUST);
    let pick = |root: &UiRoot, x, y| pick_point_in(space, Some((x, y)), root);
    assert_eq!(pick(&u.root, 50, 480), None, "the orb");
    assert_eq!(pick(&u.root, 400, 300), Some((400, 300)), "the world");
    assert_eq!(pick(&u.root, 400, 520), Some((400, 520)), "above the rows");
    assert_eq!(pick(&u.root, 400, 560), None, "the HUD's rows");
    assert_eq!(u.press(50, 480), Routed::Panel(HUD_PANEL));
    assert_eq!(u.press(400, 300), Routed::Unhandled);
    ClientMods::new().apply_ui_settings();
}

/// D2Gamma's HUD: the world is hovered above the control panel and not
/// over it, as before.
#[test]
fn d2gammas_hud_bounds_the_pick_as_before() {
    use crate::ui::ui_space::UiSpace;
    use crate::world_view::present::pick_point_in;
    let u = ui(ClientMods::new());
    let space = UiSpace::identity(FrameSize::D2RUST);
    let pick = |x, y| pick_point_in(space, Some((x, y)), &u.root);
    assert_eq!(pick(400, 300), Some((400, 300)));
    assert_eq!(pick(50, 560), None, "the life orb");
    assert_eq!(pick(400, 560), None);
    assert_eq!(pick(400, 590), None);
}

/// A mod panel in the character sheet's place that closes from outside: a
/// press outside closes the state and still reaches the world, a press
/// inside does not close it, and a key closes it too.
#[test]
fn a_press_or_key_outside_closes_a_panel_and_goes_on() {
    let mut sheet = ModPanel::new(PanelId(u16::from(CHARACTER)), LEFT_HALF);
    sheet.close = OutsideClose::PressOrKey;
    let mut u = ui(ClientMods::new().with_game_panel(GamePanel::Character, sheet));
    let open = |u: &mut Ui| {
        u.ui.set_ui(u32::from(CHARACTER), 0, false).unwrap();
        u.ui.sync_root(&mut u.root);
        assert!(u.ui.is_open(CHARACTER));
    };
    open(&mut u);
    assert_eq!(u.press(100, 100), Routed::Panel(PanelId(2)));
    assert!(u.ui.is_open(CHARACTER), "a press inside");
    assert_eq!(u.press(600, 200), Routed::Unhandled, "the world gets it");
    assert!(!u.ui.is_open(CHARACTER));
    assert!(!u.root.is_open(PanelId(2)).unwrap());
    open(&mut u);
    assert_eq!(u.send(UiEvent::Char(u16::from(b'a'))), Routed::Unhandled);
    assert!(!u.ui.is_open(CHARACTER), "a key");
}

/// A panel that closes on presses only stays open over keys; a panel that
/// is no UI state closes in the root.
#[test]
fn press_only_and_added_panels_close_from_outside() {
    let mut sheet = ModPanel::new(PanelId(u16::from(CHARACTER)), LEFT_HALF);
    sheet.close = OutsideClose::Press;
    let mut overlay = ModPanel::new(PanelId(MOD_PANEL_IDS), Rect::new(500, 100, 100, 100));
    overlay.close = OutsideClose::Press;
    let mut u = ui(ClientMods::new()
        .with_game_panel(GamePanel::Character, sheet)
        .with_panel(PanelPlace::Add { open: true }, overlay));
    u.ui.set_ui(u32::from(CHARACTER), 0, false).unwrap();
    u.ui.sync_root(&mut u.root);
    u.send(UiEvent::Char(u16::from(b'a')));
    assert!(u.ui.is_open(CHARACTER));
    assert!(u.root.is_open(PanelId(MOD_PANEL_IDS)).unwrap());
    u.press(100, 100);
    assert!(u.ui.is_open(CHARACTER));
    assert!(!u.root.is_open(PanelId(MOD_PANEL_IDS)).unwrap());
}

/// A mod panel's requests: the game's add-skill-point and select-skill
/// messages leave through the root, a UI state opens, a sound plays.
#[test]
fn a_mod_panels_requests_are_carried_out_in_order() {
    let mut p = ModPanel::new(PanelId(MOD_PANEL_IDS), Rect::new(500, 100, 100, 100));
    p.asks = vec![
        ModRequest::AddSkillPoint { skill: 0x1A },
        ModRequest::SelectSkill {
            skill: 0x1A,
            left: false,
            owner: u32::MAX,
        },
        ModRequest::SetUi {
            state: CHARACTER,
            open: true,
        },
        ModRequest::Sound(7),
    ];
    let mut u = ui(ClientMods::new().with_panel(PanelPlace::Add { open: true }, p));
    u.press(550, 150);
    assert_eq!(
        u.root.take_intents(),
        vec![
            ClientIntent(vec![0x3B, 0x1A, 0x00]),
            ClientIntent(vec![0x3C, 0x1A, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF]),
        ]
    );
    assert!(u.ui.is_open(CHARACTER));
    assert!(u
        .ui
        .take_sounds()
        .contains(&crate::audio::driver::SoundRequest::Ui(7)));
    // And closes it again.
    let mut q = ModPanel::new(PanelId(MOD_PANEL_IDS), Rect::new(500, 100, 100, 100));
    q.asks = vec![ModRequest::SetUi {
        state: CHARACTER,
        open: false,
    }];
    let mut u = ui(ClientMods::new().with_panel(PanelPlace::Add { open: true }, q));
    u.ui.set_ui(u32::from(CHARACTER), 0, false).unwrap();
    u.press(550, 150);
    assert!(!u.ui.is_open(CHARACTER));
}

/// The hire list in the mod game's NPC menu: the game's box is gone, the
/// NPC menu panel reads the offers and hires through the game's message.
#[test]
fn the_hire_list_shows_in_the_mod_npc_menu() {
    let menu = ModPanel::new(PanelId(8), Rect::new(0, 0, 0, 0));
    let mods = ClientMods::new()
        .with_game_panel(GamePanel::NpcMenu, menu)
        .with_hire_list_in_npc_menu();
    assert!(mods.hire_list_in_npc_menu());
    let mut u = ui(mods);
    assert!(u.root.is_open(HIRE_PANEL).unwrap());
    let draws = |u: &Ui| {
        let mut out: Vec<crate::ui::draw::UiDraw> = Vec::new();
        u.with_ctx(|ctx| u.root.draw(ctx, &mut out));
        out
    };
    u.send(UiEvent::CursorMoved(Point::new(1, 1)));
    let closed = draws(&u);
    // The list opens for NPC 77 with one offer.
    u.with_ctx(|ctx| ctx.request(ModRequest::HireList(Some(77))));
    u.send(UiEvent::CursorMoved(Point::new(1, 1)));
    u.ui.hire.borrow_mut().offer(5, 99);
    u.ui.refresh_mod_io();
    assert_eq!(
        u.with_ctx(|ctx| ctx.hire_offers()),
        Some(HireOffers {
            npc: 77,
            offers: vec![Offer { name: 5, seed: 99 }],
        })
    );
    // The game's hire box draws nothing over it.
    assert_eq!(draws(&u), closed);
    u.with_ctx(|ctx| ctx.request(ModRequest::Hire { npc: 77, name: 5 }));
    u.send(UiEvent::CursorMoved(Point::new(2, 2)));
    assert_eq!(
        u.root.take_intents(),
        vec![ClientIntent(vec![0x36, 77, 0, 0, 0, 5, 0, 0, 0])]
    );
    u.ui.refresh_mod_io();
    assert_eq!(u.with_ctx(|ctx| ctx.hire_offers()), None);
}

/// D2Gamma's own hire list: the game's box, and no offers for mod panels.
#[test]
fn d2gamma_keeps_its_hire_list_box() {
    let u = ui(ClientMods::new());
    assert!(!ClientMods::new().hire_list_in_npc_menu());
    u.ui.hire.borrow_mut().up = Some(77);
    u.ui.hire.borrow_mut().offer(5, 99);
    u.ui.refresh_mod_io();
    assert_eq!(u.with_ctx(|ctx| ctx.hire_offers()), None);
}

/// The keys the host recorded reach every panel's context.
#[test]
fn panels_read_the_modifier_keys() {
    let u = ui(ClientMods::new());
    assert_eq!(u.with_ctx(|ctx| ctx.keys()), Keys::default());
    let held = Keys {
        shift: true,
        ctrl: false,
        alt: true,
    };
    set_keys(held);
    assert_eq!(u.with_ctx(|ctx| ctx.keys()), held);
}
