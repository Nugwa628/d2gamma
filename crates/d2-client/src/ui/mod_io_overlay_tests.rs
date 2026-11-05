//! Always-on mod overlays: the overhead texts and screen messages they
//! read (with the game's own left out on request), a unit's point, the
//! NPC whose menu is up, and a chat prompt of the mod game's own.

use super::*;
use crate::bridge::output::Output;
use crate::bridge::world::{ClientUnit, ClientWorld, UnitKey, MONSTER, PLAYER};
use crate::controls::Action;
use crate::mods::ui_hooks::GamePanel;
use crate::mods::{ClientMods, PanelFactory, PanelPlace, MOD_PANEL_IDS};
use crate::ui::draw::{UiDraw, UiDrawSink};
use crate::ui::geom::{Point, Rect};
use crate::ui::layout::Screen;
use crate::ui::messages::FONT_CHAT;
use crate::ui::original::UiConfig;
use crate::ui::panel::{ActionId, Panel, PanelId, UiEvent, UiResponse, WidgetId};
use crate::ui::panels::UiFiles;
use crate::ui::{NoPanelRules, NoStrings, Routed};

const ME: UnitKey = UnitKey {
    unit_type: PLAYER,
    guid: 1,
};
const NPC: UnitKey = UnitKey {
    unit_type: MONSTER,
    guid: 5,
};

/// An always-open overlay that draws nothing: its presence makes the
/// client fill the feeds.
#[derive(Clone)]
struct Overlay;

impl Panel for Overlay {
    fn id(&self) -> PanelId {
        PanelId(MOD_PANEL_IDS)
    }
    fn rect(&self) -> Rect {
        Rect::new(0, 0, 0, 0)
    }
    fn draw(&self, _ctx: &UiCtx, _out: &mut dyn UiDrawSink) {}
    fn hit(&self, _p: Point) -> Option<WidgetId> {
        None
    }
    fn event(&mut self, _e: UiEvent, _ctx: &UiCtx) -> UiResponse {
        UiResponse::Ignored
    }
}

impl PanelFactory for Overlay {
    fn make(&self, _files: &UiFiles) -> Box<dyn Panel> {
        Box::new(Overlay)
    }
}

/// A chat prompt: collects typed text, says it on Enter and closes.
#[derive(Default)]
struct Prompt {
    text: String,
}

const ENTER: u16 = 0x0D;

impl Panel for Prompt {
    fn id(&self) -> PanelId {
        GamePanel::ChatPrompt.id()
    }
    fn rect(&self) -> Rect {
        Rect::new(0, 500, 400, 20)
    }
    fn draw(&self, _ctx: &UiCtx, _out: &mut dyn UiDrawSink) {}
    fn hit(&self, _p: Point) -> Option<WidgetId> {
        None
    }
    fn event(&mut self, e: UiEvent, ctx: &UiCtx) -> UiResponse {
        match e {
            UiEvent::Char(ENTER) => {
                ctx.request(ModRequest::Say(std::mem::take(&mut self.text)));
                ctx.request(ModRequest::SetUi {
                    state: GamePanel::ChatPrompt.id().0 as u8,
                    open: false,
                });
                UiResponse::Consumed
            }
            UiEvent::Char(c) => {
                self.text.extend(char::from_u32(u32::from(c)));
                UiResponse::Consumed
            }
            _ => UiResponse::Ignored,
        }
    }
}

struct MakePrompt;

impl PanelFactory for MakePrompt {
    fn make(&self, _files: &UiFiles) -> Box<dyn Panel> {
        Box::<Prompt>::default()
    }
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
    ui.set_mods(mods);
    let mut root = UiRoot::new(Box::new(NoPanelRules));
    ui.install(&mut root).unwrap();
    let mut w = ClientWorld::default();
    for (key, cell) in [(ME, (1000, 1000)), (NPC, (1002, 1000))] {
        let mut u = ClientUnit::new(key);
        u.position = Some(cell);
        u.mode = 1;
        w.units.insert(key, u);
    }
    w.local_player = Some(ME);
    let mut u = Ui { ui, root, world: w };
    u.send(UiEvent::CursorMoved(Point::new(799, 0)));
    u
}

impl Ui {
    fn ctx(&self) -> UiCtx<'_> {
        UiCtx {
            tick: 2,
            world: &self.world,
            strings: &NoStrings,
        }
    }

    fn send(&mut self, e: UiEvent) -> Routed {
        let ctx = UiCtx {
            tick: 2,
            world: &self.world,
            strings: &NoStrings,
        };
        self.ui.before_event(e, &self.world);
        let r = self.root.dispatch(e, &ctx);
        self.ui.after_event(&mut self.root, e, r).unwrap();
        r
    }

    /// The chat-font texts one draw makes.
    fn chat_texts(&self) -> Vec<String> {
        self.ui.refresh_mod_io();
        let mut out: Vec<UiDraw> = Vec::new();
        self.root.draw(&self.ctx(), &mut out);
        out.iter()
            .filter_map(|d| match d {
                UiDraw::Text(t) if t.style.font == FONT_CHAT => {
                    Some(String::from_utf16_lossy(&t.text))
                }
                _ => None,
            })
            .collect()
    }
}

fn chat5(unit: UnitKey, text: &str) -> Output {
    Output::ChatLine {
        kind: 5,
        lang: 0,
        unit,
        b8: 0,
        b9: 0,
        name: Vec::new(),
        text: text.as_bytes().to_vec(),
        present: true,
        player_name: None,
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// The overhead texts reach a mod overlay with their units' points; with
/// the bubbles turned off the game draws none of them.
#[test]
fn an_overlay_reads_the_overhead_texts_at_their_units() {
    for hidden in [false, true] {
        let mut mods = ClientMods::new().with_panel(PanelPlace::Add { open: true }, Overlay);
        if hidden {
            mods = mods.without_overhead_bubbles();
        }
        let mut u = ui(mods);
        u.ui.apply_output(&chat5(ME, "hi"), &u.world.clone())
            .unwrap();
        let drawn = u.chat_texts();
        let ctx = u.ctx();
        let at = ctx.unit_point(ME).expect("the player stands somewhere");
        assert_eq!(
            ctx.speech(),
            vec![Speech {
                unit: ME,
                text: wide("hi"),
                at,
            }]
        );
        assert_eq!(
            drawn.contains(&"hi".to_string()),
            !hidden,
            "hidden {hidden}"
        );
    }
}

/// A unit two cells east of the player stands right of it on the UI; a
/// unit without a position has no point.
#[test]
fn unit_points_follow_the_world() {
    let mut u = ui(ClientMods::new().with_panel(PanelPlace::Add { open: true }, Overlay));
    u.ui.refresh_mod_io();
    let ctx = u.ctx();
    let (me, npc) = (ctx.unit_point(ME).unwrap(), ctx.unit_point(NPC).unwrap());
    assert!(npc.x > me.x, "{me:?} {npc:?}");
    u.world.units.get_mut(&NPC).unwrap().position = None;
    assert_eq!(u.ctx().unit_point(NPC), None);
}

/// The screen messages reach a mod overlay; with the chat lines turned off
/// the game draws none of them.
#[test]
fn an_overlay_reads_the_screen_messages() {
    for hidden in [false, true] {
        let mut mods = ClientMods::new().with_panel(PanelPlace::Add { open: true }, Overlay);
        if hidden {
            mods = mods.without_chat_lines();
        }
        let mut u = ui(mods);
        u.ui.add_note("hello");
        let drawn = u.chat_texts();
        let got = u.ctx().chat_lines();
        assert_eq!(
            got,
            vec![ChatLine {
                lines: vec![wide("hello")],
                color: 0,
                expiry: got[0].expiry,
            }]
        );
        assert!(got[0].expiry > 0, "a message leaves the screen later");
        assert_eq!(
            drawn.contains(&"hello".to_string()),
            !hidden,
            "hidden {hidden}"
        );
    }
}

/// D2Gamma's own UI fills no feed and draws its bubbles and messages.
#[test]
fn d2gamma_draws_its_bubbles_and_messages_and_feeds_nothing() {
    let mut u = ui(ClientMods::new());
    u.ui.apply_output(&chat5(ME, "hi"), &u.world.clone())
        .unwrap();
    u.ui.add_note("hello");
    let drawn = u.chat_texts();
    assert!(drawn.contains(&"hi".to_string()));
    assert!(drawn.contains(&"hello".to_string()));
    assert!(u.ctx().speech().is_empty());
    assert!(u.ctx().chat_lines().is_empty());
    assert_eq!(u.ctx().npc_menu_npc(), None);
}

/// The NPC whose menu is up.
#[test]
fn an_overlay_reads_the_npc_whose_menu_is_up() {
    let mut u = ui(ClientMods::new().with_panel(PanelPlace::Add { open: true }, Overlay));
    let w = u.world.clone();
    u.ui.open_npc_menu(NPC.guid, 150, 1, &w);
    u.ui.refresh_mod_io();
    assert_eq!(u.ctx().npc_menu_npc(), Some(NPC.guid));
}

/// The mod game's chat prompt: the Chat key opens it, the typed text
/// reaches it with the key bindings off, Enter says the line and closes
/// it.
#[test]
fn a_mod_chat_prompt_types_and_says_a_line() {
    let chat = GamePanel::ChatPrompt.id().0 as u8;
    let mut u = ui(ClientMods::new().with_game_panel(GamePanel::ChatPrompt, MakePrompt));
    assert!(!u.ui.chat_typing());
    u.send(UiEvent::Action(ActionId(Action::OpenChat.index() as u16)));
    assert!(u.ui.is_open(chat));
    assert!(u.ui.chat_typing());
    for c in "hi".encode_utf16() {
        assert_eq!(u.send(UiEvent::Char(c)), Routed::Panel(PanelId(5)));
    }
    u.send(UiEvent::Char(ENTER));
    assert_eq!(
        u.root.take_intents(),
        vec![ClientIntent(vec![0x15, 1, 0, b'h', b'i', 0, 0, 0])]
    );
    assert!(!u.ui.is_open(chat));
    assert!(!u.ui.chat_typing());
}

/// D2Gamma off a realm: the Chat key opens nothing.
#[test]
fn d2gamma_has_no_chat_prompt_off_a_realm() {
    let mut u = ui(ClientMods::new());
    u.send(UiEvent::Action(ActionId(Action::OpenChat.index() as u16)));
    assert!(!u.ui.is_open(GamePanel::ChatPrompt.id().0 as u8));
    assert!(!u.ui.chat_typing());
}

/// The quest log's inputs and the waypoint menu the server opened reach a
/// mod panel as the game's own panels read them.
#[test]
fn a_panel_reads_the_quest_records_and_the_open_waypoint_menu() {
    let mut u = ui(ClientMods::new().with_panel(PanelPlace::Add { open: true }, Overlay));
    let w = u.world.clone();
    assert_eq!(u.ctx().waypoint_open(), None);
    let mut status = [0u8; 41];
    status[1] = 1;
    status[4] = 2;
    u.ui.apply_output(&Output::QuestLog { status }, &w).unwrap();
    let mut record = [0u8; 96];
    record[2] = 0x01;
    u.ui.apply_output(&Output::GameQuestFlags { record }, &w)
        .unwrap();
    let q = u.ctx().quest_inputs().expect("installed");
    assert_eq!(q.status, status);
    assert_eq!(q.game.map(|g| g.word(1)), Some(1));
    let record = [2, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    u.ui.apply_output(&Output::WaypointMenu { guid: 0x0A, record }, &w)
        .unwrap();
    let open = u.ctx().waypoint_open().expect("the menu is open");
    assert_eq!(open.guid, 0x0A);
    assert_eq!(open.record.0, record);
}
