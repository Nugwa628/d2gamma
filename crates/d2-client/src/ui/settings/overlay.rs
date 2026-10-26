// D2RUST's own: the Ctrl+O quick settings overlay.
//! The quick settings overlay: Ctrl+O in game opens a small panel in the
//! top-right corner with the Picture, Display and Interface settings as
//! tabs, the same rows the full menu shows for them ([`super::table`]).
//! Every change applies at once while the game keeps running, and is saved
//! as the full menu's changes are.
//!
//! It does not capture the mouse: a press on the panel is the panel's, a
//! press anywhere else is the game's, as if the panel weren't there
//! ([`super::menu::InputClaim::pointer`] holds only while the pointer is
//! over the panel). It takes only its own keys: Ctrl+O closes it, Esc
//! closes it, F1 opens the full settings menu in its place. Every other key
//! is the game's. It never pauses the game, single player included.
//!
//! The overlay opens in game only (the main menu has the full menu), and
//! not while the full menu is open.

use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::ui::{FocusPolicy, RelativeCursorPosition};
use bevy::window::PrimaryWindow;

use super::menu::{
    facts, pointer_into, row, row_list, spawn_screen, OpenSettings, ScreenPointer, SettingsMenu,
};
use super::state::{Hit, MenuState, Open};
use super::table::Category;
use super::text::{MenuTextSource, TextCache, TextCtx, TextRole};
use super::widgets::{colour, rgb, rgba, Align, HitTag, Layer, Screen, SliderTrack};
use super::{window_metrics, z, Metrics};
use crate::app::config::{ConfigRes, Monitors, PrimaryWindowOn, Settings};

/// The overlay's layout (view units; see the approved mockup).
pub mod at {
    pub const X0: f32 = 902.0;
    pub const X1: f32 = 1262.0;
    pub const TOP: f32 = 18.0;
    pub const HEADER: f32 = 52.0;
    pub const TAB_TOP: f32 = 58.0;
    pub const TAB_W: f32 = 114.0;
    pub const FIRST_ROW: f32 = 104.0;
    pub const ROW_PITCH: f32 = 34.0;
    pub const LABEL_X: f32 = 914.0;
    pub const CONTROL_X0: f32 = 1064.0;
    pub const CONTROL_X1: f32 = 1250.0;
    /// Rows shown at most; more scroll.
    pub const MAX_ROWS: usize = 14;
    /// The foot's height under the rows: its rule, "Saved automatically"
    /// and the key hints.
    pub const FOOT: f32 = 44.0;
}

/// The overlay.
#[derive(Resource, Default)]
pub struct SettingsOverlay {
    pub open: bool,
    pub state: MenuState,
    /// The tab it opens on: the last one shown.
    last: Option<Category>,
    /// The pointer is over the panel (its presses are the panel's).
    pub over: bool,
    shown: Option<(MenuState, Metrics, u64, (u32, u32))>,
    drag: Option<usize>,
}

impl SettingsOverlay {
    /// Opens on the last tab with the settings in force.
    pub fn show(&mut self, settings: Settings) {
        self.open = true;
        self.state = MenuState::new(
            &Category::OVERLAY,
            self.last.unwrap_or(Category::Picture),
            settings,
        );
        self.drag = None;
    }

    pub fn hide(&mut self) {
        if self.open {
            self.last = Some(self.state.category);
        }
        self.open = false;
        self.over = false;
        self.drag = None;
    }
}

/// The overlay's root node.
#[derive(Component)]
pub struct OverlayRoot;

/// The panel's node: the pointer over it is the panel's.
#[derive(Component)]
pub struct OverlayPanel;

pub(super) fn register(app: &mut App) {
    app.init_resource::<SettingsOverlay>()
        .add_systems(
            PreUpdate,
            (
                overlay_keys.after(bevy::input::InputSystems),
                overlay_pointer.after(bevy::ui::UiSystems::Focus),
            )
                .before(super::menu::claim),
        )
        .add_systems(
            Update,
            (overlay_host, overlay_draw)
                .chain()
                .after(super::menu::menu_draw),
        );
}

/// What a key press does to the overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayKey {
    /// Ctrl+O: open or close.
    Toggle,
    /// F1 while open: the full menu instead.
    Full,
    /// Esc while open.
    Close,
}

/// The overlay's key for a press of `code` with Ctrl `ctrl` held, the
/// overlay `open` or not; `None`: the game's key.
pub fn overlay_key(code: KeyCode, ctrl: bool, open: bool) -> Option<OverlayKey> {
    match code {
        KeyCode::KeyO if ctrl => Some(OverlayKey::Toggle),
        KeyCode::F1 if open => Some(OverlayKey::Full),
        KeyCode::Escape if open => Some(OverlayKey::Close),
        _ => None,
    }
}

/// What a scripted key press (`play --input`, `play --control`) reaches of
/// the settings screens: the open full menu takes every key, the overlay
/// its own (Ctrl+O, and F1 and Esc while it is open), as with the keyboard.
#[derive(bevy::ecs::system::SystemParam)]
pub struct SettingsKeys<'w> {
    menu: Option<ResMut<'w, SettingsMenu>>,
    overlay: Option<ResMut<'w, SettingsOverlay>>,
    request: Option<ResMut<'w, OpenSettings>>,
    cfg: Option<Res<'w, ConfigRes>>,
}

impl SettingsKeys<'_> {
    /// A scripted press of `code` with Ctrl `ctrl` held; whether the
    /// settings screens took it.
    pub fn key(&mut self, code: KeyCode, ctrl: bool) -> bool {
        if let Some(m) = self.menu.as_deref_mut() {
            if m.script_key(code) {
                return true;
            }
        }
        let Some(overlay) = self.overlay.as_deref_mut() else {
            return false;
        };
        let Some(k) = overlay_key(code, ctrl, overlay.open) else {
            return false;
        };
        let settings = self
            .cfg
            .as_ref()
            .map_or_else(Settings::default, |c| c.settings);
        apply_key(overlay, k, settings, self.request.as_deref_mut());
        true
    }
}

/// Runs overlay key `k`.
fn apply_key(
    overlay: &mut SettingsOverlay,
    k: OverlayKey,
    settings: Settings,
    request: Option<&mut OpenSettings>,
) {
    match k {
        OverlayKey::Toggle if overlay.open => overlay.hide(),
        OverlayKey::Toggle => overlay.show(settings),
        OverlayKey::Full => {
            overlay.hide();
            if let Some(r) = request {
                r.0 = true;
            }
        }
        OverlayKey::Close => overlay.hide(),
    }
}

/// Ctrl+O, F1 and Esc; the keys it takes never reach the game.
#[allow(clippy::too_many_arguments)]
fn overlay_keys(
    mut overlay: ResMut<SettingsOverlay>,
    menu: Res<SettingsMenu>,
    mut request: ResMut<OpenSettings>,
    mut events: MessageReader<KeyboardInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    cfg: Option<Res<ConfigRes>>,
    ui: Option<NonSend<crate::world_view::WorldViewUi>>,
    scene: Option<Res<crate::world_view::scene_view::SceneView>>,
    mut was_in_game: Local<bool>,
) {
    // A game drawn as the lobby's scene is not played: its keys are the
    // lobby's.
    let in_game = ui.is_some_and(|u| u.original.is_some()) && scene.is_none();
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let mut taken = Vec::new();
    for k in events.read() {
        if k.state != ButtonState::Pressed || menu.open || !in_game {
            continue;
        }
        let Some(key) = overlay_key(k.key_code, ctrl, overlay.open) else {
            continue;
        };
        taken.push(k.key_code);
        let settings = cfg.as_ref().map_or_else(Settings::default, |c| c.settings);
        apply_key(&mut overlay, key, settings, Some(&mut *request));
    }
    if menu.open && overlay.open {
        overlay.hide();
    }
    // Leaving the game closes it.
    if *was_in_game && !in_game && overlay.open {
        overlay.hide();
    }
    *was_in_game = in_game;
    for code in taken {
        keys.reset(code);
    }
}

fn overlay_pointer(
    mut overlay: ResMut<SettingsOverlay>,
    changed: Query<(&Interaction, &HitTag), Changed<Interaction>>,
    all: Query<(&Interaction, &HitTag)>,
    tracks: Query<(&SliderTrack, &RelativeCursorPosition, &HitTag)>,
    panel: Query<&RelativeCursorPosition, With<OverlayPanel>>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    if !overlay.open {
        overlay.over = false;
        overlay.drag = None;
        return;
    }
    // A drag that began on the panel keeps it until the button is up.
    let over = panel.iter().any(|r| r.cursor_over())
        || (overlay.drag.is_some() && mouse.pressed(MouseButton::Left));
    if overlay.over != over {
        overlay.over = over;
    }
    let p = ScreenPointer::read(Screen::Overlay, &changed, &all, &tracks);
    let o = &mut *overlay;
    pointer_into(
        &mut o.state,
        &mut o.drag,
        p.pressed.iter().copied(),
        p.hovered,
        |row| p.track(row),
        mouse.pressed(MouseButton::Left),
    );
}

/// Hands the overlay's changes to the game, which saves and applies them.
#[allow(clippy::too_many_arguments)]
fn overlay_host(
    mut overlay: ResMut<SettingsOverlay>,
    ui: Option<NonSendMut<crate::world_view::WorldViewUi>>,
    mut cfg: Option<ResMut<ConfigRes>>,
    choice: Option<ResMut<crate::app::config::WindowChoice>>,
    sync: Option<ResMut<crate::app::vsync::SyncChoice>>,
    monitors: Monitors,
    windows: PrimaryWindowOn<&Window>,
) {
    if !overlay.open {
        return;
    }
    let mut ui = ui;
    overlay.state.facts = facts(cfg.as_deref(), &monitors, &windows);
    match overlay.state.take_changed() {
        Some(new) => {
            if let Some(o) = ui.as_mut().and_then(|u| u.original.as_mut()) {
                o.edit_settings(new);
            } else if let Some(cfg) = cfg.as_deref_mut() {
                crate::app::config::commit_settings(cfg, choice, sync, new);
            }
        }
        None => {
            if let Some(c) = cfg.as_ref().filter(|c| c.is_changed()) {
                overlay.state.settings = c.settings;
            }
        }
    }
    // The overlay has no action rows and asks nothing of the host.
    overlay.state.take_events();
}

/// The rows of the open tab with each one's line, scrolled to keep the
/// selected row in view; at most [`at::MAX_ROWS`].
pub fn overlay_lines(state: &MenuState) -> Vec<(usize, f32)> {
    let rows = state.rows();
    let first = state
        .selected
        .and_then(|s| rows.iter().position(|&r| r == s))
        .map_or(0, |p| (p + 1).saturating_sub(at::MAX_ROWS));
    rows.into_iter()
        .skip(first)
        .take(at::MAX_ROWS)
        .enumerate()
        .map(|(k, i)| (i, at::FIRST_ROW + at::ROW_PITCH * k as f32))
        .collect()
}

/// The panel's bottom: room for the longest tab's rows (so it doesn't
/// change size between tabs) and the foot.
pub fn panel_bottom() -> f32 {
    let most = Category::OVERLAY
        .iter()
        .map(|&c| super::table::rows_of(c).count())
        .max()
        .unwrap_or(0)
        .min(at::MAX_ROWS);
    at::FIRST_ROW + at::ROW_PITCH * most as f32 - at::ROW_PITCH / 2.0 + at::FOOT
}

#[allow(clippy::too_many_arguments)]
fn overlay_draw(
    mut commands: Commands,
    mut overlay: ResMut<SettingsOverlay>,
    roots: Query<Entity, With<OverlayRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    source: Option<Res<MenuTextSource>>,
    mut cache: ResMut<TextCache>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let (true, Some(metrics), Some(mut images)) = (overlay.open, window_metrics(&windows), images)
    else {
        for e in &roots {
            commands.entity(e).despawn();
        }
        overlay.shown = None;
        return;
    };
    let (source, text_key) = super::text::current(source.as_deref());
    let size = windows.single().map_or((0, 0), |w| {
        (
            w.resolution.physical_width(),
            w.resolution.physical_height(),
        )
    });
    let shown = (overlay.state.clone(), metrics, text_key, size);
    if !roots.is_empty() && overlay.shown.as_ref() == Some(&shown) {
        return;
    }
    overlay.shown = Some(shown);
    for e in &roots {
        commands.entity(e).despawn();
    }
    let state = overlay.state.clone();
    let text = TextCtx {
        source,
        cache: &mut cache,
        images: &mut images,
        metrics,
    };
    spawn_screen(
        &mut commands,
        OverlayRoot,
        Screen::Overlay,
        z::OVERLAY,
        0,
        size,
        text,
        |l| build(l, &state),
    );
}

fn build(l: &mut Layer, s: &MenuState) {
    let bottom = panel_bottom();
    let tag = l.tag(Hit::Panel);
    l.rect(
        (at::X0, at::TOP, at::X1, bottom),
        Some(rgba(colour::PANEL, 205)),
        Some(rgb(colour::PANEL_EDGE)),
        10.0,
    )
    .insert((
        Button,
        FocusPolicy::Block,
        tag,
        RelativeCursorPosition::default(),
        OverlayPanel,
    ));
    // Header.
    l.rect(
        (at::X0 + 1.0, at::TOP + 1.0, at::X1 - 1.0, at::HEADER),
        Some(rgb(colour::BAR)),
        None,
        9.0,
    );
    l.hline(at::X0 + 1.0, at::X1 - 1.0, at::HEADER, colour::RULE);
    let mid = (at::TOP + at::HEADER) / 2.0;
    l.text(
        TextRole::Label,
        "QUICK SETTINGS",
        colour::GOLD,
        916.0,
        mid,
        Align::Left,
    );
    l.rect(
        (1214.0, mid - 3.0, 1220.0, mid + 3.0),
        Some(rgb(colour::GREEN)),
        None,
        3.0,
    );
    l.text(
        TextRole::Label,
        "LIVE",
        colour::GREEN,
        1224.0,
        mid,
        Align::Left,
    );
    // Tabs.
    for (k, c) in s.categories.iter().enumerate() {
        let x0 = 914.0 + at::TAB_W * k as f32;
        l.tab(
            x0,
            x0 + at::TAB_W - 4.0,
            at::TAB_TOP,
            c.name(),
            *c == s.category,
            Hit::Category(*c),
        );
    }
    // Rows.
    let lines = overlay_lines(s);
    let cols = (at::LABEL_X, at::CONTROL_X0, at::CONTROL_X1);
    for &(i, y) in &lines {
        row(l, s, i, y, cols);
    }
    // Foot.
    let foot = bottom - at::FOOT + 14.0;
    l.hline(at::X0 + 1.0, at::X1 - 1.0, foot, colour::DIVIDER);
    let y = foot + 15.0;
    l.text(
        TextRole::Body,
        "Saved automatically",
        colour::DIM,
        916.0,
        y,
        Align::Left,
    );
    // The key hints end at the panel's right, whatever the font's widths.
    let cap = |l: &Layer, s: &str| l.measure(TextRole::Label, s) + 10.0;
    let width = cap(l, "Ctrl+O")
        + 4.0
        + l.measure(TextRole::Label, "close")
        + 8.0
        + cap(l, "F1")
        + 4.0
        + l.measure(TextRole::Label, "more");
    let mut x = at::X1 - 10.0 - width;
    x += l.keycap(x, y, "Ctrl+O") + 4.0;
    let w = l.measure(TextRole::Label, "close");
    l.text(TextRole::Label, "close", colour::DIM, x, y, Align::Left);
    x += w + 8.0;
    x += l.keycap(x, y, "F1") + 4.0;
    l.text(TextRole::Label, "more", colour::DIM, x, y, Align::Left);
    // An open list goes over everything.
    if let Open::Row(i) = s.open {
        if let Some(&(_, y)) = lines.iter().find(|(r, _)| *r == i) {
            row_list(l, s, i, y, at::CONTROL_X0, at::CONTROL_X1, 700.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::settings::table::TABLE;

    fn idx(id: &str) -> usize {
        TABLE.iter().position(|e| e.id == id).expect(id)
    }

    fn app(dir: &std::path::Path) -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Time<Real>>()
            .add_message::<KeyboardInput>()
            .init_resource::<Assets<Image>>()
            .insert_resource(ConfigRes {
                dir: dir.to_path_buf(),
                settings: Settings::default(),
                bindings: crate::controls::Preset::Original
                    .bindings()
                    .expect("original preset"),
                sound_device: true,
            });
        app.world_mut().spawn((
            Window::default(),
            PrimaryWindow,
            crate::app::session::hidden_cursor(),
        ));
        super::super::add(&mut app);
        app
    }

    /// Open, the overlay draws over the running game without pausing it or
    /// taking its keys; the pointer is its own only over the panel; a
    /// change applies and is saved at once. Ctrl+O does nothing outside a
    /// game.
    #[test]
    fn the_overlay_applies_live_and_leaves_the_game_running() {
        use super::super::menu::InputClaim;
        let dir = std::env::temp_dir().join(format!("d2rust-overlay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = app(&dir);
        app.update();
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::KeyO,
            logical_key: bevy::input::keyboard::Key::Character("o".into()),
            state: ButtonState::Pressed,
            text: Some("o".into()),
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        app.update();
        assert!(
            !app.world().resource::<SettingsOverlay>().open,
            "not in a game"
        );
        app.world_mut()
            .resource_mut::<SettingsOverlay>()
            .show(Settings::default());
        app.update();
        app.update();
        let roots = app
            .world_mut()
            .query_filtered::<Entity, With<OverlayRoot>>()
            .iter(app.world())
            .count();
        assert_eq!(roots, 1);
        let claim = *app.world().resource::<InputClaim>();
        assert_eq!(
            claim,
            InputClaim::default(),
            "no pause, keys and pointer free"
        );
        app.world_mut().resource_mut::<SettingsOverlay>().over = true;
        // `over` is measured each frame from the panel; set it as the
        // pointer would and read the claim the same frame.
        app.world_mut()
            .run_system_cached(super::super::menu::claim)
            .expect("claim runs");
        assert!(app.world().resource::<InputClaim>().pointer);
        assert!(!app.world().resource::<InputClaim>().pause);
        // Gamma one step up, as a click on its slider would.
        let g = idx("video.gamma");
        app.world_mut()
            .resource_mut::<SettingsOverlay>()
            .state
            .press(Hit::Slider(g, 1.0));
        app.update();
        assert_eq!(app.world().resource::<ConfigRes>().settings.gamma, 255);
        assert_eq!(
            crate::app::config::load_settings(&dir)
                .expect("saved")
                .gamma,
            255
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Scripted keys reach the overlay as the keyboard's do: Ctrl+O opens
    /// and closes it, F1 asks for the full menu, Esc closes; other keys
    /// and F1 with it closed stay the game's.
    #[test]
    fn scripted_keys_open_and_close_the_overlay() {
        let mut world = World::new();
        world.init_resource::<SettingsMenu>();
        world.init_resource::<SettingsOverlay>();
        world.init_resource::<OpenSettings>();
        let key = |world: &mut World, code: KeyCode, ctrl: bool| {
            world
                .run_system_cached_with(
                    |In((code, ctrl)): In<(KeyCode, bool)>, mut k: SettingsKeys| k.key(code, ctrl),
                    (code, ctrl),
                )
                .expect("runs")
        };
        assert!(!key(&mut world, KeyCode::F1, false));
        assert!(!key(&mut world, KeyCode::KeyO, false));
        assert!(key(&mut world, KeyCode::KeyO, true));
        assert!(world.resource::<SettingsOverlay>().open);
        assert!(!key(&mut world, KeyCode::KeyI, false), "the game's key");
        assert!(key(&mut world, KeyCode::Escape, false));
        assert!(!world.resource::<SettingsOverlay>().open);
        key(&mut world, KeyCode::KeyO, true);
        assert!(key(&mut world, KeyCode::F1, false));
        assert!(!world.resource::<SettingsOverlay>().open);
        assert!(
            world.resource::<OpenSettings>().0,
            "the full menu asked for"
        );
    }

    #[test]
    fn ctrl_o_toggles_and_f1_and_esc_only_while_open() {
        assert_eq!(
            overlay_key(KeyCode::KeyO, true, false),
            Some(OverlayKey::Toggle)
        );
        assert_eq!(
            overlay_key(KeyCode::KeyO, true, true),
            Some(OverlayKey::Toggle)
        );
        // O alone, F1 and Esc with the overlay closed are the game's.
        assert_eq!(overlay_key(KeyCode::KeyO, false, true), None);
        assert_eq!(overlay_key(KeyCode::F1, false, false), None);
        assert_eq!(overlay_key(KeyCode::Escape, false, false), None);
        assert_eq!(
            overlay_key(KeyCode::F1, false, true),
            Some(OverlayKey::Full)
        );
        assert_eq!(
            overlay_key(KeyCode::Escape, false, true),
            Some(OverlayKey::Close)
        );
    }

    #[test]
    fn the_overlay_shows_its_three_tabs_and_reopens_on_the_last() {
        let mut o = SettingsOverlay::default();
        o.show(Settings::default());
        assert_eq!(o.state.categories, Category::OVERLAY);
        assert_eq!(o.state.category, Category::Picture);
        o.state.press(Hit::Category(Category::Interface));
        o.hide();
        o.show(Settings::default());
        assert_eq!(o.state.category, Category::Interface);
        assert_eq!(o.state.selected, Some(idx("video.ui_scale")));
        // The HD Text row is on the Interface tab, as in the full menu.
        assert!(o.state.rows().contains(&idx("video.hd_text")));
        o.state.press(Hit::Toggle(idx("video.hd_text")));
        assert!(o.state.take_changed().is_some_and(|s| s.hd_text));
    }

    #[test]
    fn rows_fit_the_panel_and_scroll_past_the_most_it_shows() {
        let mut s = MenuState::new(&Category::OVERLAY, Category::Picture, Settings::default());
        let lines = overlay_lines(&s);
        assert_eq!(lines.len(), s.rows().len().min(at::MAX_ROWS));
        assert_eq!(lines[0].1, at::FIRST_ROW);
        let (_, last) = lines[lines.len() - 1];
        assert!(last + at::ROW_PITCH / 2.0 <= panel_bottom() - at::FOOT);
        // The selection stays in view.
        s.selected = s.rows().last().copied();
        assert!(overlay_lines(&s)
            .iter()
            .any(|(r, _)| Some(*r) == s.selected));
    }
}
