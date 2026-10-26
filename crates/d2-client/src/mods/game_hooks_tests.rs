//! G7: panel actions, ticks and key-downs; mod messages; automap
//! markers; the world in label filters; skill levels with bonuses. Each
//! with a synthetic mod hook, and the check that D2RUST's own UI is left
//! as it is.

use std::sync::{Arc, Mutex};

use d2_proto::envelope::Wire;

use crate::audio::driver::SoundRequest;
use crate::bridge::skills::{SkillEntry, SkillList, NATIVE};
use crate::bridge::world::{ClientTables, ClientUnit, ClientWorld, SkillRow, UnitKey, PLAYER};
use crate::mods::{AutomapMarker, AutomapMarkers, ClientMods, PanelFactory, PanelPlace};
use crate::ui::automap::draw::{AutomapDraw, Label};
use crate::ui::automap::markers::{extra_markers, ExtraMarker, MarkerCtx, MarkerPalette};
use crate::ui::automap::view::Bounds;
use crate::ui::draw::UiDrawSink;
use crate::ui::geom::{Point, Rect};
use crate::ui::layout::Screen;
use crate::ui::original::{OriginalUi, UiConfig};
use crate::ui::panel::{
    ClientIntent, Panel, PanelAction, PanelId, PanelTick, UiCtx, UiEvent, UiResponse, WidgetId,
};
use crate::ui::panels::UiFiles;
use crate::ui::{NoPanelRules, NoStrings, UiRoot};

const MOD: PanelId = PanelId(crate::mods::MOD_PANEL_IDS);
const OTHER: PanelId = PanelId(crate::mods::MOD_PANEL_IDS + 1);

fn world() -> ClientWorld {
    let mut w = ClientWorld::default();
    let key = UnitKey::new(PLAYER, 1);
    w.units.insert(key, ClientUnit::new(key));
    w.local_player = Some(key);
    w
}

fn ctx(w: &ClientWorld) -> UiCtx<'_> {
    UiCtx {
        tick: 7,
        world: w,
        strings: &NoStrings,
    }
}

/// What a [`Recorder`] saw.
#[derive(Default)]
struct Seen {
    ticks: Vec<(u64, bool)>,
    online: Vec<bool>,
    keys: Vec<u32>,
}

/// A mod panel that answers a press with `on_press`, and records its ticks
/// and key-downs (taking key `0x41`).
struct Recorder {
    id: PanelId,
    on_press: Vec<PanelAction>,
    seen: Arc<Mutex<Seen>>,
}

impl Panel for Recorder {
    fn id(&self) -> PanelId {
        self.id
    }
    fn rect(&self) -> Rect {
        Rect::new(100, 100, 200, 100)
    }
    fn draw(&self, _ctx: &UiCtx, _out: &mut dyn UiDrawSink) {}
    fn hit(&self, _p: Point) -> Option<WidgetId> {
        None
    }
    fn event(&mut self, e: UiEvent, _ctx: &UiCtx) -> UiResponse {
        match e {
            UiEvent::Press { .. } => UiResponse::Actions(self.on_press.clone()),
            _ => UiResponse::Ignored,
        }
    }
    fn tick(&mut self, t: &PanelTick) -> UiResponse {
        let mut seen = self.seen.lock().unwrap();
        seen.ticks.push((t.ui.tick, t.open));
        seen.online.push(t.online);
        UiResponse::Ignored
    }
    fn key_down(&mut self, vk: u32, _ctx: &UiCtx) -> UiResponse {
        self.seen.lock().unwrap().keys.push(vk);
        if vk == 0x41 {
            UiResponse::Actions(vec![PanelAction::Sound(9)])
        } else {
            UiResponse::Ignored
        }
    }
}

fn recorder(id: PanelId, on_press: Vec<PanelAction>) -> (Recorder, Arc<Mutex<Seen>>) {
    let seen = Arc::new(Mutex::new(Seen::default()));
    (
        Recorder {
            id,
            on_press,
            seen: seen.clone(),
        },
        seen,
    )
}

fn press() -> UiEvent {
    UiEvent::Press {
        button: crate::ui::panel::PointerButton::Left,
        at: Point::new(150, 150),
    }
}

fn note(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

// One answer carries several intents, opens another mod panel and leaves
// the UI state panel, the sound and the note for the UI's owner.
#[test]
fn panel_actions_queue_intents_open_mod_panels_and_leave_the_rest() {
    let w = world();
    let a = ClientIntent(vec![0x71, 1, 0, 1]);
    let b = ClientIntent(vec![0x71, 1, 0, 2]);
    let (p, _) = recorder(
        MOD,
        vec![
            PanelAction::Intent(a.clone()),
            PanelAction::Intent(b.clone()),
            PanelAction::Open(OTHER),
            PanelAction::Sound(5),
            PanelAction::Note(note("saved")),
            PanelAction::Open(PanelId(1)),
            // An unknown panel changes nothing.
            PanelAction::Close(PanelId(0x7FFF)),
        ],
    );
    let (other, _) = recorder(OTHER, Vec::new());
    let mut root = UiRoot::new(Box::new(NoPanelRules));
    root.add(Box::new(p)).unwrap();
    root.add(Box::new(other)).unwrap();
    root.open(MOD).unwrap();
    assert_eq!(
        root.dispatch(press(), &ctx(&w)),
        crate::ui::Routed::Panel(MOD)
    );
    assert_eq!(root.take_intents(), vec![a, b]);
    assert!(root.is_open(OTHER).unwrap());
    assert_eq!(
        root.take_requests(),
        vec![
            PanelAction::Sound(5),
            PanelAction::Note(note("saved")),
            PanelAction::Open(PanelId(1)),
        ]
    );
    // An empty list is a plain Consumed.
    let (q, _) = recorder(MOD, Vec::new());
    let mut root = UiRoot::new(Box::new(NoPanelRules));
    root.add(Box::new(q)).unwrap();
    root.open(MOD).unwrap();
    assert_eq!(
        root.dispatch(press(), &ctx(&w)),
        crate::ui::Routed::Panel(MOD)
    );
    assert!(root.take_intents().is_empty() && root.take_requests().is_empty());
}

// Every panel gets the tick with its open state; key-downs go to the open
// panels top-most first and stop at the first taker.
#[test]
fn ticks_reach_every_panel_and_key_downs_the_open_ones() {
    let w = world();
    let (top, top_seen) = recorder(OTHER, Vec::new());
    let (below, below_seen) = recorder(MOD, Vec::new());
    let mut root = UiRoot::new(Box::new(NoPanelRules));
    root.add(Box::new(below)).unwrap();
    root.add(Box::new(top)).unwrap();
    root.open(OTHER).unwrap();
    root.open(MOD).unwrap();
    root.push_key(0x42);
    root.push_key(0x41);
    root.run_keys(&ctx(&w));
    // 0x42 is ignored by both; 0x41 is taken by the top-most.
    assert_eq!(top_seen.lock().unwrap().keys, [0x42, 0x41]);
    assert_eq!(below_seen.lock().unwrap().keys, [0x42]);
    assert_eq!(root.take_requests(), vec![PanelAction::Sound(9)]);
    root.close(MOD).unwrap();
    root.tick(&ctx(&w), &ClientTables::default(), false);
    assert_eq!(below_seen.lock().unwrap().ticks, [(7, false)]);
    assert_eq!(top_seen.lock().unwrap().ticks, [(7, true)]);
    // A closed panel gets no key-down.
    root.push_key(0x43);
    root.run_keys(&ctx(&w));
    assert_eq!(below_seen.lock().unwrap().keys, [0x42]);
}

fn original(mods: Option<ClientMods>) -> (OriginalUi, UiRoot) {
    let config = UiConfig {
        screen: Screen::R800,
        expansion_installed: true,
    };
    let mut ui = OriginalUi::new(config, None).unwrap();
    if let Some(m) = mods {
        ui.set_mods(m);
    }
    let mut root = UiRoot::new(Box::new(NoPanelRules));
    ui.install(&mut root).unwrap();
    (ui, root)
}

// D2RUST's own panels ignore the tick and key-downs: nothing is queued
// and no panel opens or closes.
#[test]
fn the_games_panels_ignore_ticks_and_key_downs() {
    let w = world();
    let (ui, mut root) = original(None);
    assert!(!ui.has_mod_panels());
    let open = root.open_panels();
    for vk in [0x41, 0x1B, 0x0D, 0x49] {
        root.push_key(vk);
    }
    root.run_keys(&ctx(&w));
    root.tick(&ctx(&w), &ClientTables::default(), false);
    assert_eq!(root.open_panels(), open);
    assert!(root.take_intents().is_empty());
    assert!(root.take_requests().is_empty());
}

struct MakeRecorder(Vec<PanelAction>);

impl PanelFactory for MakeRecorder {
    fn make(&self, _files: &UiFiles) -> Box<dyn Panel> {
        Box::new(recorder(MOD, self.0.clone()).0)
    }
}

// The game's UI carries out what the root left: a UI state opens and
// closes through its rules, the sound plays as a UI sound, the note goes
// to the screen messages.
#[test]
fn the_original_ui_carries_out_panel_requests() {
    let w = world();
    let mods = ClientMods::new().with_panel(
        PanelPlace::Add { open: true },
        MakeRecorder(vec![
            PanelAction::Open(PanelId(1)),
            PanelAction::Sound(5),
            PanelAction::Note(note("saved")),
        ]),
    );
    let (mut ui, mut root) = original(Some(mods));
    assert!(ui.has_mod_panels());
    assert!(!ui.is_open(1));
    root.dispatch(press(), &ctx(&w));
    ui.apply_requests(&mut root).unwrap();
    assert!(ui.is_open(1));
    assert!(ui
        .take_sounds()
        .iter()
        .any(|s| matches!(s, SoundRequest::Ui(5))));
    assert_eq!(ui.pending_notes(), ["saved"]);
    // Close goes the same way.
    let (p, _) = recorder(OTHER, vec![PanelAction::Close(PanelId(1))]);
    root.add(Box::new(p)).unwrap();
    root.open(OTHER).unwrap();
    root.dispatch(press(), &ctx(&w));
    ui.apply_requests(&mut root).unwrap();
    assert!(!ui.is_open(1));
}

/// A mod message hook that counts its messages.
struct Count(Arc<Mutex<usize>>);

impl crate::bridge::ModMessages for Count {
    fn message(&mut self, _world: &mut ClientWorld, _payload: &[u8]) {
        *self.0.lock().unwrap() += 1;
    }
}

/// A link that delivers nothing.
struct Idle;

impl crate::bridge::link::ServerLink for Idle {
    fn protocol_version(&self) -> u32 {
        d2_proto::PROTOCOL_VERSION
    }
    fn tick_rate(&self) -> d2_sim::tick::TickRate {
        d2_sim::tick::TickRate::BASE
    }
    fn send(
        &mut self,
        _: crate::bridge::link::SendQueue,
        _: &[u8],
    ) -> Result<crate::bridge::link::Sent, crate::bridge::link::LinkError> {
        Ok(crate::bridge::link::Sent::Queued)
    }
    fn pump(&mut self) -> Result<crate::bridge::link::Pumped, crate::bridge::link::LinkError> {
        Ok(crate::bridge::link::Pumped { ticked: false })
    }
    fn receive(&mut self) -> Vec<Vec<u8>> {
        Vec::new()
    }
}

/// An idle link whose answer to "is the server elsewhere" the test flips.
struct Switch(Arc<std::sync::atomic::AtomicBool>);

impl crate::bridge::link::ServerLink for Switch {
    fn protocol_version(&self) -> u32 {
        Idle.protocol_version()
    }
    fn tick_rate(&self) -> d2_sim::tick::TickRate {
        d2_sim::tick::TickRate::BASE
    }
    fn send(
        &mut self,
        q: crate::bridge::link::SendQueue,
        m: &[u8],
    ) -> Result<crate::bridge::link::Sent, crate::bridge::link::LinkError> {
        Idle.send(q, m)
    }
    fn pump(&mut self) -> Result<crate::bridge::link::Pumped, crate::bridge::link::LinkError> {
        Idle.pump()
    }
    fn receive(&mut self) -> Vec<Vec<u8>> {
        Vec::new()
    }
    fn is_remote(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

// A mod panel reads whether the game is online from its tick, each frame,
// from the game's link as it is wrapped in play (the walk recorder): a game
// that becomes remote shows as online on the next frame, and an offline
// game as offline.
#[test]
fn a_mod_panel_reads_the_links_online_answer_each_frame() {
    use crate::world_view::ui_bind::{run_ui_with, UiQueue};
    let remote = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let link = crate::bridge::predict::PredictLink::new(Switch(remote.clone()));
    let mut bridge = crate::bridge::Bridge::new(link).unwrap();
    let (rec, seen) = recorder(MOD, Vec::new());
    let mods = ClientMods::new().with_panel(PanelPlace::Add { open: true }, OnePanel(rec));
    let (mut ui, mut root) = original(Some(mods));
    let mut frame = |bridge: &mut crate::bridge::Bridge<_>| {
        run_ui_with(
            &mut root,
            &mut UiQueue::default(),
            bridge,
            &NoStrings,
            Some(&mut ui),
        )
        .unwrap();
    };
    frame(&mut bridge);
    remote.store(true, std::sync::atomic::Ordering::SeqCst);
    frame(&mut bridge);
    frame(&mut bridge);
    remote.store(false, std::sync::atomic::Ordering::SeqCst);
    frame(&mut bridge);
    assert_eq!(seen.lock().unwrap().online, [false, true, true, false]);
}

/// Hands out one ready-made panel.
struct OnePanel(Recorder);

impl PanelFactory for OnePanel {
    fn make(&self, _files: &UiFiles) -> Box<dyn Panel> {
        Box::new(Recorder {
            id: self.0.id,
            on_press: Vec::new(),
            seen: self.0.seen.clone(),
        })
    }
}

fn game_wire(mods: Option<ClientMods>) -> Wire {
    let mut app = bevy::app::App::new();
    if let Some(m) = mods {
        app.insert_resource(m);
    }
    crate::app::play::install_game(&mut app, Box::new(Idle)).unwrap();
    app.world()
        .resource::<crate::bridge::BridgeResource>()
        .0
        .wire()
}

// The mod game's hook makes the game's bridge speak the mod wire; each
// game gets its own hook. Without one the bridge stays on D2RUST's wire.
#[test]
fn mod_messages_put_the_games_bridge_on_the_mod_wire() {
    assert!(ClientMods::new().mod_messages().is_none());
    assert_eq!(game_wire(None), Wire::Original);
    assert_eq!(game_wire(Some(ClientMods::new())), Wire::Original);
    let made = Arc::new(Mutex::new(0));
    let n = Arc::new(Mutex::new(0));
    let (m2, n2) = (made.clone(), n.clone());
    let mods = ClientMods::new().with_mod_messages(move || {
        *m2.lock().unwrap() += 1;
        Box::new(Count(n2.clone()))
    });
    assert!(!mods.is_empty());
    assert_eq!(game_wire(Some(mods.clone())), Wire::Mod);
    assert_eq!(game_wire(Some(mods)), Wire::Mod);
    assert_eq!(*made.lock().unwrap(), 2);
}

/// Marks a spot near the local player and one far away.
struct Spots;

impl AutomapMarkers for Spots {
    fn markers(&self, world: &ClientWorld, out: &mut Vec<AutomapMarker>) {
        let n = world.units.len() as i32;
        out.push(AutomapMarker {
            at: (10 * n, 0),
            rgb: (255, 0, 0),
            name: "here".encode_utf16().collect(),
            name_color: 4,
        });
        out.push(AutomapMarker {
            at: (100_000, 0),
            rgb: (0, 255, 0),
            name: Vec::new(),
            name_color: 0,
        });
    }
}

// The hook's markers come in registration order, read from the world;
// the pass draws a cross and a name for each one inside the rectangle.
#[test]
fn automap_markers_are_read_from_the_world_and_drawn_as_crosses() {
    let w = world();
    assert!(!ClientMods::new().has_automap_markers());
    assert!(ClientMods::new().automap_markers(&w).is_empty());
    let mods = ClientMods::new().with_automap_markers(Spots);
    assert!(mods.has_automap_markers() && !mods.is_empty());
    let marks = mods.automap_markers(&w);
    assert_eq!(marks.len(), 2);
    assert_eq!(marks[0].at, (10, 0));
    let ctx = MarkerCtx {
        local_party: -1,
        party: false,
        names: false,
        player_gate: false,
        mini: false,
        div: 10,
        a: (0, 0),
        rect: Bounds {
            left: -16,
            top: -16,
            right: 800,
            bottom: 600,
        },
        palette: MarkerPalette {
            index: [10, 11, 12, 13, 14, 15, 16, 18, 0],
        },
    };
    let extra: Vec<ExtraMarker> = marks
        .into_iter()
        .map(|m| ExtraMarker {
            pos: crate::rules::camera::static_to_client(m.at.0, m.at.1),
            color: 77,
            name: m.name,
            name_color: m.name_color,
        })
        .collect();
    let mut out = Vec::new();
    extra_markers(&extra, &ctx, &mut out);
    // The far marker is outside the rectangle: 12 lines and one name.
    assert_eq!(out.len(), 13);
    assert!(out[..12]
        .iter()
        .all(|d| matches!(d, AutomapDraw::Line { color: 77, .. })));
    assert!(matches!(
        &out[12],
        AutomapDraw::Text { text: Label::Text(t), color: 4, .. } if *t == note("here")
    ));
    // None: no draw.
    let mut out = Vec::new();
    extra_markers(&[], &ctx, &mut out);
    assert!(out.is_empty());
}

/// Hides every label while the client knows more than one unit.
struct Crowded;

impl crate::mods::ItemNameFilter for Crowded {
    fn filter(
        &self,
        world: &ClientWorld,
        _item: &d2_proto::item_bits::ItemBits,
        label: &mut crate::mods::ItemLabel,
    ) {
        if world.units.len() > 1 {
            label.shown = false;
        }
    }
}

// A label filter reads the client's world.
#[test]
fn item_name_filters_read_the_world() {
    let mut tips = crate::ui::item_tip::tests::tips();
    tips.mods = ClientMods::new().with_item_name_filter(Crowded);
    let ctx = crate::ui::item_tip_build::TipCtx::default();
    let item = d2_proto::item_bits::ItemBits {
        code: *b"cap ",
        quality: 2,
        ..Default::default()
    };
    let mut w = world();
    assert!(tips.label_of(&item, &ctx, &w).is_some());
    let key = UnitKey::new(PLAYER, 2);
    w.units.insert(key, ClientUnit::new(key));
    assert_eq!(tips.label_of(&item, &ctx, &w), None);
}

// The level with bonuses: the base level plus +all skills (stat 127);
// none for a skill the unit doesn't own or a unit not in the model.
#[test]
fn skill_level_adds_the_bonuses_to_the_base_level() {
    let mut w = world();
    let key = UnitKey::new(PLAYER, 1);
    let u = w.units.get_mut(&key).unwrap();
    u.skills = Some(SkillList {
        entries: vec![SkillEntry {
            skill: 3,
            base: 4,
            owner: NATIVE,
            ..SkillEntry::default()
        }],
        ..SkillList::default()
    });
    u.stats.insert(127, 2);
    let tables = ClientTables {
        skills: vec![SkillRow::default(); 5],
        ..ClientTables::default()
    };
    assert_eq!(crate::bridge::skill_level(&w, &tables, key, 3), Some(6));
    assert_eq!(crate::bridge::skill_level(&w, &tables, key, 2), None);
    assert_eq!(
        crate::bridge::skill_level(&w, &tables, UnitKey::new(PLAYER, 9), 3),
        None
    );
}
