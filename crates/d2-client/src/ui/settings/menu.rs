// D2RUST's own: the settings menu screen.
//! The full settings menu: categories on the left, the category's rows in
//! the middle, the info panel on the right, the Preset drop-down and the
//! search box on top, key hints, Reset and Done at the bottom. It is laid
//! out in view units (the 1280 x 720 view, centred in the window) and
//! drawn as UI nodes over everything at the window's resolution.
//!
//! It opens from the main menu's Options button and from the Esc menu's
//! Options row in game (the hosts ask for it: `FrontEnd::take_settings_request`,
//! `OriginalUi::take_settings_request`), and from the Ctrl+O overlay's F1
//! ([`OpenSettings`]). While it is open:
//! - it takes every key and click (the game and the main menu get none);
//! - a single-player game is paused, as under the Esc menu ([`InputClaim`]);
//!   an online game keeps running;
//! - every change applies at once and is saved: the menu hands the new
//!   settings to the host the change would come from anyway (the Esc
//!   menu's or the main menu's settings), which saves and applies them, so
//!   the game reacts exactly as it does to those menus;
//! - the picture behind it is dimmed by half (a quarter on the Picture
//!   category, so its changes can be judged).
//!
//! Done or Esc closes it: back to the Esc menu or the main menu it came
//! from.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::ui::{FocusPolicy, RelativeCursorPosition};
use bevy::window::PrimaryWindow;

use super::state::{Hit, MenuEvent, MenuKey, MenuState, Open};
use super::table::{self, Applies, Category, Facts, Kind, PRESETS, TABLE};
use super::text::{MenuTextSource, TextCache, TextCtx, TextRole};
use super::widgets::{colour, rgb, Align, HitTag, Layer, Screen, SliderTrack};
use super::{window_metrics, z, Metrics, VIEW_H, VIEW_W};
use crate::app::config::{ConfigRes, Monitors, PrimaryWindowOn, Settings};

/// The settings menu.
#[derive(Resource, Default)]
pub struct SettingsMenu {
    pub open: bool,
    pub state: MenuState,
    /// The category it opens on: the last one shown.
    last: Category,
    /// What is drawn now.
    shown: Option<(MenuState, Metrics, u64, (u32, u32))>,
    /// The slider row held by the pointer.
    drag: Option<usize>,
    /// It was opened from the Esc menu, which closes under it and opens
    /// again when it closes.
    back_to_esc: bool,
}

impl SettingsMenu {
    /// A scripted key press (`play --input`): the menu's, as the window's
    /// keys are, while it is open. Whether it took the key.
    pub fn script_key(&mut self, code: KeyCode) -> bool {
        if !self.open {
            return false;
        }
        let key = match code {
            KeyCode::ArrowUp => Some(MenuKey::Up),
            KeyCode::ArrowDown => Some(MenuKey::Down),
            KeyCode::ArrowLeft => Some(MenuKey::Left),
            KeyCode::ArrowRight => Some(MenuKey::Right),
            KeyCode::Enter | KeyCode::NumpadEnter => Some(MenuKey::Enter),
            KeyCode::Escape => Some(MenuKey::Escape),
            KeyCode::Tab | KeyCode::PageDown => Some(MenuKey::NextCategory),
            KeyCode::PageUp => Some(MenuKey::PrevCategory),
            KeyCode::Backspace => Some(MenuKey::Backspace),
            KeyCode::Space => Some(MenuKey::Char(' ')),
            other => key_letter(other).map(MenuKey::Char),
        };
        if let Some(k) = key {
            self.state.key(k);
        }
        true
    }
}

/// The lower-case letter or digit a letter or digit key types.
pub fn key_letter(code: KeyCode) -> Option<char> {
    use KeyCode as C;
    const LETTERS: [KeyCode; 26] = [
        C::KeyA,
        C::KeyB,
        C::KeyC,
        C::KeyD,
        C::KeyE,
        C::KeyF,
        C::KeyG,
        C::KeyH,
        C::KeyI,
        C::KeyJ,
        C::KeyK,
        C::KeyL,
        C::KeyM,
        C::KeyN,
        C::KeyO,
        C::KeyP,
        C::KeyQ,
        C::KeyR,
        C::KeyS,
        C::KeyT,
        C::KeyU,
        C::KeyV,
        C::KeyW,
        C::KeyX,
        C::KeyY,
        C::KeyZ,
    ];
    const DIGITS: [KeyCode; 10] = [
        C::Digit0,
        C::Digit1,
        C::Digit2,
        C::Digit3,
        C::Digit4,
        C::Digit5,
        C::Digit6,
        C::Digit7,
        C::Digit8,
        C::Digit9,
    ];
    if let Some(i) = LETTERS.iter().position(|&c| c == code) {
        return Some(char::from(b'a' + i as u8));
    }
    DIGITS
        .iter()
        .position(|&c| c == code)
        .map(|i| char::from(b'0' + i as u8))
}

/// A request to open the full menu (the overlay's F1).
#[derive(Resource, Default)]
pub struct OpenSettings(pub bool);

/// What the settings screens take from the game's input this frame.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputClaim {
    /// Every key and pointer press (the full menu is open).
    pub all: bool,
    /// The pointer's presses and the wheel (it is over the overlay).
    pub pointer: bool,
    /// A single-player game holds still (the full menu is open).
    pub pause: bool,
}

/// The menu's root node.
#[derive(Component)]
pub struct MenuRoot;

pub(super) fn register(app: &mut App) {
    app.init_resource::<SettingsMenu>()
        .init_resource::<OpenSettings>()
        .init_resource::<InputClaim>()
        .add_systems(
            PreUpdate,
            (
                menu_keys.after(bevy::input::InputSystems),
                menu_pointer.after(bevy::ui::UiSystems::Focus),
                claim,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (menu_host, menu_draw)
                .chain()
                .before(crate::app::config::front_settings),
        )
        .add_systems(Last, menu_pointer_shown);
}

/// The system pointer shows while the menu is open or the pointer is over
/// the overlay: the game's own cursor is drawn into the picture under them
/// (on the main menu, and in game while an item is held). After, the
/// pointer is as the game had it: the hardware cursor's state in game,
/// hidden on the main menu.
fn menu_pointer_shown(
    menu: Res<SettingsMenu>,
    claim: Res<InputClaim>,
    hw: Option<Res<crate::world_view::hw_cursor::HardwareCursorShown>>,
    mut cursor: Query<&mut bevy::window::CursorOptions, With<PrimaryWindow>>,
    mut was_open: Local<bool>,
) {
    let shown = menu.open || claim.pointer;
    let want = if shown {
        Some(true)
    } else if *was_open {
        Some(hw.is_some_and(|h| h.0))
    } else {
        None
    };
    *was_open = shown;
    if let Some(visible) = want {
        for mut c in &mut cursor {
            if c.visible != visible {
                c.visible = visible;
            }
        }
    }
}

/// What the full menu and the overlay take from the game this frame.
pub(super) fn claim(
    menu: Res<SettingsMenu>,
    overlay: Option<Res<super::overlay::SettingsOverlay>>,
    mut claim: ResMut<InputClaim>,
) {
    let over = overlay.is_some_and(|o| o.open && o.over);
    let new = InputClaim {
        all: menu.open,
        pointer: menu.open || over,
        pause: menu.open,
    };
    if *claim != new {
        *claim = new;
    }
}

/// The menu's key for a key press, `None` for keys it ignores.
pub fn menu_key(k: &KeyboardInput, shift: bool) -> Vec<MenuKey> {
    let one = |m| vec![m];
    match k.key_code {
        KeyCode::ArrowUp => one(MenuKey::Up),
        KeyCode::ArrowDown => one(MenuKey::Down),
        KeyCode::ArrowLeft => one(MenuKey::Left),
        KeyCode::ArrowRight => one(MenuKey::Right),
        KeyCode::Enter | KeyCode::NumpadEnter => one(MenuKey::Enter),
        KeyCode::Escape => one(MenuKey::Escape),
        KeyCode::Tab if shift => one(MenuKey::PrevCategory),
        KeyCode::Tab | KeyCode::PageDown => one(MenuKey::NextCategory),
        KeyCode::PageUp => one(MenuKey::PrevCategory),
        KeyCode::Backspace => one(MenuKey::Backspace),
        _ => match &k.logical_key {
            Key::Character(_) | Key::Space => k
                .text
                .as_deref()
                .unwrap_or_default()
                .chars()
                .filter(|c| !c.is_control())
                .map(MenuKey::Char)
                .collect(),
            _ => Vec::new(),
        },
    }
}

/// While open, every key goes to the menu and none to the game.
fn menu_keys(
    mut menu: ResMut<SettingsMenu>,
    mut events: MessageReader<KeyboardInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
) {
    if !menu.open {
        events.read().for_each(drop);
        return;
    }
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    for k in events.read() {
        if k.state != ButtonState::Pressed || alt {
            continue;
        }
        for m in menu_key(k, shift) {
            menu.state.key(m);
        }
    }
    keys.reset_all();
}

/// Which row a widget belongs to.
fn hit_row(h: Hit) -> Option<usize> {
    match h {
        Hit::Row(i)
        | Hit::Toggle(i)
        | Hit::Segment(i, _)
        | Hit::Slider(i, _)
        | Hit::Dropdown(i)
        | Hit::Action(i) => Some(i),
        _ => None,
    }
}

/// Presses, hover and slider drags of the widgets of `root`'s tree.
pub fn pointer_into(
    state: &mut MenuState,
    drag: &mut Option<usize>,
    pressed: impl Iterator<Item = (Interaction, Hit)>,
    hovered: Option<usize>,
    track: impl Fn(usize) -> Option<f32>,
    held: bool,
) {
    for (i, hit) in pressed {
        if i != Interaction::Pressed {
            continue;
        }
        match hit {
            Hit::Slider(row, f) => {
                state.press(Hit::Slider(row, track(row).unwrap_or(f)));
                *drag = Some(row);
            }
            h => state.press(h),
        }
    }
    if state.hovered != hovered {
        state.hover(hovered);
    }
    match (*drag, held) {
        (Some(row), true) => {
            if let Some(f) = track(row) {
                state.drag(row, f);
            }
        }
        (Some(_), false) => *drag = None,
        _ => {}
    }
}

/// The widgets of one screen: what was pressed this frame, the row the
/// pointer is over, and where the pointer is on each row's slider track.
pub struct ScreenPointer {
    pub pressed: Vec<(Interaction, Hit)>,
    pub hovered: Option<usize>,
    pub tracks: Vec<(usize, f32)>,
}

impl ScreenPointer {
    pub fn read(
        screen: Screen,
        changed: &Query<(&Interaction, &HitTag), Changed<Interaction>>,
        all: &Query<(&Interaction, &HitTag)>,
        tracks: &Query<(&SliderTrack, &RelativeCursorPosition, &HitTag)>,
    ) -> Self {
        ScreenPointer {
            pressed: changed
                .iter()
                .filter(|(_, t)| t.screen == screen)
                .map(|(i, t)| (*i, t.hit))
                .collect(),
            hovered: all
                .iter()
                .filter(|(i, t)| t.screen == screen && **i != Interaction::None)
                .find_map(|(_, t)| hit_row(t.hit)),
            tracks: tracks
                .iter()
                .filter(|(_, _, t)| t.screen == screen)
                .filter_map(|(t, r, _)| r.normalized.map(|n| (t.row, t.fraction(n.x))))
                .collect(),
        }
    }

    pub fn track(&self, row: usize) -> Option<f32> {
        self.tracks.iter().find(|(r, _)| *r == row).map(|(_, f)| *f)
    }
}

fn menu_pointer(
    mut menu: ResMut<SettingsMenu>,
    changed: Query<(&Interaction, &HitTag), Changed<Interaction>>,
    all: Query<(&Interaction, &HitTag)>,
    tracks: Query<(&SliderTrack, &RelativeCursorPosition, &HitTag)>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    if !menu.open {
        menu.drag = None;
        return;
    }
    let p = ScreenPointer::read(Screen::Menu, &changed, &all, &tracks);
    let menu = &mut *menu;
    pointer_into(
        &mut menu.state,
        &mut menu.drag,
        p.pressed.iter().copied(),
        p.hovered,
        |row| p.track(row),
        mouse.pressed(MouseButton::Left),
    );
}

/// The machine facts the rows read.
pub fn facts(
    cfg: Option<&ConfigRes>,
    monitors: &Monitors,
    windows: &PrimaryWindowOn<&Window>,
) -> Facts {
    let window = windows.single().ok();
    let on = window.and_then(|(_, on)| on);
    Facts {
        sound_device: cfg.is_some_and(|c| c.sound_device),
        monitor: crate::app::config::monitor_size(crate::app::config::monitor_of(on, monitors)),
        refresh_hz: crate::app::frame_limit::refresh_hz(on, monitors),
        window: window
            .map(|(w, _)| crate::app::config::window_facts(w))
            .unwrap_or_default(),
    }
}

/// Opens the menu when a host asks, hands its changes to the host and
/// runs its events.
#[allow(clippy::too_many_arguments)]
fn menu_host(
    mut menu: ResMut<SettingsMenu>,
    mut request: ResMut<OpenSettings>,
    ui: Option<NonSendMut<crate::world_view::WorldViewUi>>,
    host: Option<NonSendMut<crate::app::front_host::FrontHost>>,
    mut cfg: Option<ResMut<ConfigRes>>,
    choice: Option<ResMut<crate::app::config::WindowChoice>>,
    sync: Option<ResMut<crate::app::vsync::SyncChoice>>,
    monitors: Monitors,
    windows: PrimaryWindowOn<&Window>,
) {
    let mut ui = ui;
    let mut host = host;
    let mut open = std::mem::take(&mut request.0);
    if let Some(o) = ui.as_mut().and_then(|u| u.original.as_mut()) {
        if o.take_settings_request() && !menu.open {
            open = true;
            menu.back_to_esc = o.hide_game_menu().unwrap_or_else(|e| {
                bevy::log::warn!("settings menu: {e}");
                false
            });
        }
    }
    if let Some(h) = host.as_mut() {
        open |= h.front.take_settings_request();
    }
    let facts = facts(cfg.as_deref(), &monitors, &windows);
    let settings = cfg.as_ref().map_or_else(Settings::default, |c| c.settings);
    if open && !menu.open {
        menu.open = true;
        menu.state = MenuState::full(menu.last, settings);
        menu.drag = None;
    }
    if !menu.open {
        return;
    }
    menu.state.facts = facts;
    let changed = menu.state.take_changed();
    match changed {
        Some(new) => {
            if let Some(o) = ui.as_mut().and_then(|u| u.original.as_mut()) {
                o.edit_settings(new);
            } else if let Some(h) = host.as_mut() {
                h.front.edit_settings(new);
            } else if let Some(cfg) = cfg.as_deref_mut() {
                crate::app::config::commit_settings(cfg, choice, sync, new);
            }
        }
        // Another path changed them (Alt+Enter, a hotkey).
        None if cfg.as_ref().is_some_and(|c| c.is_changed()) => menu.state.settings = settings,
        None => {}
    }
    for ev in menu.state.take_events() {
        match ev {
            MenuEvent::Close => {
                menu.last = menu.state.category;
                menu.open = false;
                if std::mem::take(&mut menu.back_to_esc) {
                    if let Some(o) = ui.as_mut().and_then(|u| u.original.as_mut()) {
                        if let Err(e) = o.show_game_menu() {
                            bevy::log::warn!("settings menu: {e}");
                        }
                    }
                }
            }
            MenuEvent::ConfigureControls => {
                menu.last = menu.state.category;
                menu.open = false;
                menu.back_to_esc = false;
                if let Some(o) = ui.as_mut().and_then(|u| u.original.as_mut()) {
                    if let Err(e) = o.open_controls_screen() {
                        bevy::log::warn!("configure controls: {e}");
                    }
                } else if let Some(h) = host.as_mut() {
                    h.front.open_controls_screen();
                }
            }
        }
    }
}

/// The window's size in physical pixels.
fn window_size(windows: &Query<&Window, With<PrimaryWindow>>) -> (u32, u32) {
    windows.single().map_or((0, 0), |w| {
        (
            w.resolution.physical_width(),
            w.resolution.physical_height(),
        )
    })
}

/// Spawns a screen's root: the whole window, with the view's 1280 x 720
/// layer centred in it. `dim` darkens what is under the window (0 none);
/// `f` builds the layer.
#[allow(clippy::too_many_arguments)]
pub fn spawn_screen(
    commands: &mut Commands,
    root: impl Bundle,
    screen: Screen,
    z: i32,
    dim: u8,
    size: (u32, u32),
    text: TextCtx<'_>,
    f: impl FnOnce(&mut Layer),
) -> Entity {
    let m = text.metrics;
    let ox = (size.0 as f32 / m.scale - VIEW_W) / 2.0;
    let oy = (size.1 as f32 / m.scale - VIEW_H) / 2.0;
    commands
        .spawn((
            root,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            GlobalZIndex(z),
        ))
        .with_children(|p| {
            if dim > 0 {
                p.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba_u8(0, 0, 0, dim)),
                    Button,
                    FocusPolicy::Block,
                    HitTag {
                        hit: Hit::Panel,
                        screen,
                    },
                ));
            }
            p.spawn((Node {
                position_type: PositionType::Absolute,
                left: m.px(ox),
                top: m.px(oy),
                width: m.px(VIEW_W),
                height: m.px(VIEW_H),
                ..default()
            },))
                .with_children(|v| {
                    let mut l = Layer { p: v, text, screen };
                    f(&mut l);
                });
        })
        .id()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn menu_draw(
    mut commands: Commands,
    mut menu: ResMut<SettingsMenu>,
    roots: Query<Entity, With<MenuRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    source: Option<Res<MenuTextSource>>,
    mut cache: ResMut<TextCache>,
    images: Option<ResMut<Assets<Image>>>,
    from_game: Option<NonSend<crate::world_view::WorldViewUi>>,
) {
    let (true, Some(metrics), Some(mut images)) = (menu.open, window_metrics(&windows), images)
    else {
        for e in &roots {
            commands.entity(e).despawn();
        }
        menu.shown = None;
        return;
    };
    let (source, text_key) = super::text::current(source.as_deref());
    let size = window_size(&windows);
    let shown = (menu.state.clone(), metrics, text_key, size);
    if !roots.is_empty() && menu.shown.as_ref() == Some(&shown) {
        return;
    }
    menu.shown = Some(shown);
    for e in &roots {
        commands.entity(e).despawn();
    }
    let in_game = from_game.is_some_and(|u| u.original.is_some());
    let state = menu.state.clone();
    let dim = if state.category == Category::Picture && state.search.is_empty() {
        64
    } else {
        128
    };
    let text = TextCtx {
        source,
        cache: &mut cache,
        images: &mut images,
        metrics,
    };
    spawn_screen(
        &mut commands,
        MenuRoot,
        Screen::Menu,
        z::MENU,
        dim,
        size,
        text,
        |l| build(l, &state, in_game),
    );
}

/// The menu's layout (view units; see the approved mockup).
pub mod at {
    pub const PANEL: (f32, f32, f32, f32) = (70.0, 40.0, 1210.0, 680.0);
    pub const TOP_BAR: f32 = 98.0;
    pub const FOOT: f32 = 628.0;
    pub const RAIL_X: f32 = 290.0;
    pub const INFO_X: f32 = 910.0;
    pub const ROWS_X0: f32 = 312.0;
    pub const ROWS_X1: f32 = 900.0;
    pub const CONTROL_X0: f32 = 600.0;
    pub const SLIDER_X1: f32 = 856.0;
    pub const ROWS_TOP: f32 = 118.0;
    pub const ROW_PITCH: f32 = 36.0;
    pub const GROUP_PITCH: f32 = 26.0;
    pub const PRESET: (f32, f32, f32, f32) = (262.0, 55.0, 420.0, 85.0);
    pub const SEARCH: (f32, f32, f32, f32) = (928.0, 55.0, 1188.0, 85.0);
    pub const INFO_TEXT_X: f32 = 928.0;
    pub const INFO_W: f32 = 262.0;
    /// The middle of the first Display and Picture line's first text line.
    pub const SCREEN_LINES_Y: f32 = 540.0;
}

/// One line of the rows column.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Group(String),
    Row(usize),
}

/// The rows column's lines with the middle of each, scrolled so the
/// selected row shows; lines outside the column are left out.
pub fn lines(state: &MenuState) -> Vec<(Line, f32)> {
    let mut out = Vec::new();
    let mut y = at::ROWS_TOP;
    let mut group = None;
    let searching = !state.search.trim().is_empty();
    for i in state.rows() {
        let e = &TABLE[i];
        let g = if searching {
            format!("{} - {}", e.category.name(), e.group)
        } else {
            e.group.to_string()
        };
        if group.as_ref() != Some(&g) {
            out.push((Line::Group(g.to_uppercase()), y));
            y += at::GROUP_PITCH;
            group = Some(g);
        }
        out.push((Line::Row(i), y));
        y += at::ROW_PITCH;
    }
    let bottom = at::FOOT - 18.0;
    let sel = state.selected.and_then(|s| {
        out.iter()
            .find(|(l, _)| *l == Line::Row(s))
            .map(|(_, y)| *y)
    });
    let shift = sel.map_or(0.0, |y| (y + 18.0 - bottom).max(0.0));
    out.into_iter()
        .map(|(l, y)| (l, y - shift))
        .filter(|(_, y)| *y - 17.0 >= at::TOP_BAR && *y + 17.0 <= at::FOOT)
        .collect()
}

/// Draws row `i` on line `y`, its label from `x0` and its control from
/// `c0` to `x1`.
pub fn row(l: &mut Layer, s: &MenuState, i: usize, y: f32, (x0, c0, x1): (f32, f32, f32)) {
    let e = &TABLE[i];
    let on = s.enabled(i);
    l.row_band(
        (x0 - 8.0, y - 17.0, x1, y + 17.0),
        i,
        s.selected == Some(i),
        s.hovered == Some(i) && s.selected != Some(i),
    );
    let label = if on { colour::LIGHT } else { colour::GREYED };
    l.text(TextRole::Label, e.label, label, x0 + 4.0, y, Align::Left);
    let v = e.value(&s.settings);
    match e.kind {
        Kind::Toggle => l.toggle(i, x1, y, v != 0, on),
        Kind::Slider { .. } => l.slider(
            i,
            c0,
            x1 - 44.0,
            y,
            s.slider_fraction(i),
            &table::value_text(e, &s.settings, &s.facts),
            x1,
            on,
        ),
        Kind::Segmented(names) => l.segmented(i, c0, x1, y, names, v, on),
        Kind::Choice(_) => l.dropdown(
            c0,
            x1,
            y,
            &table::value_text(e, &s.settings, &s.facts),
            Hit::Dropdown(i),
            on,
        ),
        Kind::Action => l.button((c0, y - 14.0, x1, y + 14.0), "Open", false, Hit::Action(i)),
    }
}

/// The open drop-down of row `i` on line `y`, from `c0` to `x1`, kept
/// above `bottom`.
pub fn row_list(l: &mut Layer, s: &MenuState, i: usize, y: f32, c0: f32, x1: f32, bottom: f32) {
    let e = &TABLE[i];
    let v = e.value(&s.settings);
    let items: Vec<(String, bool, bool)> = e
        .names()
        .iter()
        .enumerate()
        .map(|(k, n)| {
            let k = k as i64;
            (
                n.to_string(),
                table::value_enabled(e, k, &s.settings, &s.facts),
                k == v,
            )
        })
        .collect();
    let pitch = 24.0;
    let h = pitch * items.len() as f32;
    let top = if y + 16.0 + h <= bottom {
        y + 17.0
    } else {
        y - 17.0 - h
    };
    l.list(c0, x1, top, pitch, &items, |k| Hit::Option(i, k as i64));
}

/// The Display and Picture lines ([`super::info::lines`]) the menu shows
/// at the foot of the info panel: on the Display and Picture categories,
/// not while searching.
pub fn screen_lines(s: &MenuState) -> Option<[String; 2]> {
    (s.search.is_empty() && matches!(s.category, Category::Display | Category::Picture))
        .then(|| super::info::lines(&s.settings, &s.facts))
}

/// The info panel's text for row `i` from `x`, its title's middle on `y`;
/// `width` wide.
pub fn info(l: &mut Layer, s: &MenuState, i: usize, x: f32, y: f32, width: f32) {
    let e = &TABLE[i];
    l.text(TextRole::Label, e.label, colour::GOLD, x, y, Align::Left);
    let mut y = l.wrapped(
        TextRole::Body,
        e.description,
        colour::LIGHT,
        x,
        y + 28.0,
        width,
        18.0,
    );
    y += 10.0;
    let applies = match e.applies {
        Applies::Live => "instantly",
        Applies::NextStart => "at the next start",
    };
    let mut meta = vec![("Applies:", applies.to_string())];
    if e.kind != Kind::Action {
        meta.push((
            "Default:",
            table::value_text(e, &Settings::default(), &s.facts),
        ));
    }
    if !s.enabled(i) {
        meta.push(("Now:", "not available".to_string()));
    }
    for (a, b) in meta {
        let w = l.measure(TextRole::Body, a);
        l.text(TextRole::Body, a, colour::MUTED, x, y, Align::Left);
        l.text(
            TextRole::Body,
            &b,
            colour::LIGHT,
            x + w + 6.0,
            y,
            Align::Left,
        );
        y += 18.0;
    }
}

fn build(l: &mut Layer, s: &MenuState, in_game: bool) {
    l.panel(at::PANEL, 225);
    let (px0, _, px1, py1) = at::PANEL;
    // Top bar.
    l.rect(
        (px0 + 1.0, 41.0, px1 - 1.0, at::TOP_BAR),
        Some(rgb(colour::BAR)),
        None,
        9.0,
    );
    l.hline(px0 + 1.0, px1 - 1.0, at::TOP_BAR, colour::RULE);
    l.text(
        TextRole::Title,
        "SETTINGS",
        colour::GOLD,
        92.0,
        70.0,
        Align::Left,
    );
    let p = at::PRESET;
    let tag = l.tag(Hit::Preset);
    l.rect(
        p,
        Some(rgb(colour::BAR)),
        Some(rgb(colour::FIELD_EDGE)),
        6.0,
    )
    .insert((Button, FocusPolicy::Block, tag));
    let w = l.measure(TextRole::Label, "Preset:");
    l.text(
        TextRole::Label,
        "Preset:",
        colour::DIM,
        p.0 + 12.0,
        70.0,
        Align::Left,
    );
    let preset = s.preset().map_or("Custom", |k| PRESETS[k].name);
    l.text(
        TextRole::Label,
        preset,
        colour::WHITE,
        p.0 + 18.0 + w,
        70.0,
        Align::Left,
    );
    l.caret(p.2 - 12.0, 70.0, colour::LIGHT);
    let q = at::SEARCH;
    let tag = l.tag(Hit::Search);
    l.rect(
        q,
        Some(rgb(colour::SEARCH)),
        Some(rgb(colour::FIELD_EDGE)),
        6.0,
    )
    .insert((Button, FocusPolicy::Block, tag));
    if s.search.is_empty() && !s.search_focus {
        l.text(
            TextRole::Body,
            "Search settings...",
            colour::HINT,
            q.0 + 14.0,
            70.0,
            Align::Left,
        );
    } else {
        let caret = if s.search_focus { "_" } else { "" };
        let t = format!("{}{caret}", s.search);
        l.text(
            TextRole::Body,
            &t,
            colour::LIGHT,
            q.0 + 14.0,
            70.0,
            Align::Left,
        );
    }
    // The category rail.
    l.vline(at::RAIL_X, at::TOP_BAR + 1.0, at::FOOT, colour::DIVIDER);
    for (k, c) in s.categories.iter().enumerate() {
        let y = 130.0 + 42.0 * k as f32;
        let on = *c == s.category && s.search.trim().is_empty();
        if on {
            l.rect(
                (80.0, y - 18.0, 280.0, y + 18.0),
                Some(rgb(colour::RAIL_ON)),
                None,
                6.0,
            );
            l.rect(
                (80.0, y - 18.0, 83.0, y + 18.0),
                Some(rgb(colour::GOLD)),
                None,
                0.0,
            );
        }
        l.hit((80.0, y - 18.0, 280.0, y + 18.0), Hit::Category(*c));
        let col = if on { colour::WHITE } else { colour::DIM };
        l.text(TextRole::Label, c.name(), col, 100.0, y, Align::Left);
    }
    // The rows.
    let cols = (at::ROWS_X0, at::CONTROL_X0, at::ROWS_X1);
    let shown = lines(s);
    if shown.is_empty() {
        l.text(
            TextRole::Body,
            "No setting matches the search.",
            colour::DIM,
            at::ROWS_X0,
            at::ROWS_TOP,
            Align::Left,
        );
    }
    for (line, y) in &shown {
        match line {
            Line::Group(g) => l.text(
                TextRole::Label,
                g,
                colour::MUTED,
                at::ROWS_X0,
                *y,
                Align::Left,
            ),
            Line::Row(i) => row(l, s, *i, *y, cols),
        }
    }
    // The info panel, and under it what the screen shows now.
    l.vline(at::INFO_X, at::TOP_BAR + 1.0, at::FOOT, colour::DIVIDER);
    if let Some(i) = s.info_row() {
        info(l, s, i, at::INFO_TEXT_X, 124.0, at::INFO_W);
    }
    if let Some(lines) = screen_lines(s) {
        let mut y = at::SCREEN_LINES_Y;
        for line in lines {
            y = l.wrapped(
                TextRole::Body,
                &line,
                colour::MUTED,
                at::INFO_TEXT_X,
                y,
                at::INFO_W,
                18.0,
            ) + 6.0;
        }
    }
    // The foot.
    l.rect(
        (px0 + 1.0, at::FOOT, px1 - 1.0, py1 - 1.0),
        Some(rgb(colour::BAR_FOOT)),
        None,
        9.0,
    );
    l.hline(px0 + 1.0, px1 - 1.0, at::FOOT, colour::RULE);
    let mut x = 92.0;
    let mut hints = vec![("Up/Down", "move"), ("Left/Right", "change")];
    hints.push(if in_game {
        ("Ctrl+O", "quick overlay")
    } else {
        ("Ctrl+O", "quick overlay in game")
    });
    for (k, t) in hints {
        x += l.keycap(x, 654.0, k) + 6.0;
        let w = l.measure(TextRole::Label, t);
        l.text(TextRole::Label, t, colour::DIM, x, 654.0, Align::Left);
        x += w + 16.0;
    }
    let reset = format!("Reset {}", s.category.name());
    l.button((960.0, 638.0, 1110.0, 670.0), &reset, false, Hit::Reset);
    l.button((1122.0, 638.0, 1192.0, 670.0), "Done", true, Hit::Done);
    // An open list goes over everything.
    match s.open {
        Open::Row(i) => {
            if let Some((_, y)) = shown.iter().find(|(l, _)| *l == Line::Row(i)) {
                row_list(l, s, i, *y, at::CONTROL_X0, at::ROWS_X1, at::FOOT);
            }
        }
        Open::Preset(k) => {
            let items: Vec<(String, bool, bool)> = PRESETS
                .iter()
                .enumerate()
                .map(|(j, p)| (p.name.to_string(), true, j == k))
                .collect();
            l.list(p.0, p.2 + 60.0, p.3 + 3.0, 24.0, &items, Hit::PresetOption);
        }
        Open::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(id: &str) -> usize {
        TABLE.iter().position(|e| e.id == id).expect(id)
    }

    /// Display and Picture say what the screen shows now (the monitor, the
    /// window, the picture's scale); the other categories and a search
    /// don't.
    #[test]
    fn display_and_picture_show_what_the_screen_shows_now() {
        use crate::app::config::{ScaleMode, WindowMode};
        use table::WindowFacts;
        let mut s = MenuState::full(Category::Display, Settings::default());
        s.facts = Facts {
            monitor: Some((3840, 2160)),
            refresh_hz: Some(60),
            window: WindowFacts {
                mode: Some(WindowMode::WindowedBorderless),
                size: Some((3840, 2160)),
                frame: Some((1280, 720)),
            },
            ..Facts::default()
        };
        let both = [
            "Display: 3840x2160 at 60 Hz, Windowed Borderless",
            "Picture: 1280x720 shown at 3x (Sharp)",
        ];
        assert_eq!(screen_lines(&s).unwrap(), both);
        s.go(Category::Picture);
        assert_eq!(screen_lines(&s).unwrap(), both);
        // A change shows at once.
        s.settings.scale_mode = ScaleMode::Integer;
        assert_eq!(
            screen_lines(&s).unwrap()[1],
            "Picture: 1280x720 shown at 3x (Integer)"
        );
        s.search = "gamma".into();
        assert_eq!(screen_lines(&s), None);
        s.search.clear();
        s.go(Category::Interface);
        assert_eq!(screen_lines(&s), None);
    }

    #[test]
    fn a_long_list_scrolls_to_keep_the_selection_in_view() {
        let mut s = MenuState::full(Category::Picture, Settings::default());
        // Every row: the search for a word every description shares.
        s.search = "e".into();
        let last = *s.rows().last().unwrap();
        s.selected = Some(last);
        let shown = lines(&s);
        let (_, y) = shown
            .iter()
            .find(|(l, _)| *l == Line::Row(last))
            .expect("the selected row is shown");
        assert!(*y + 17.0 <= at::FOOT);
        assert!(shown.iter().all(|(_, y)| *y - 17.0 >= at::TOP_BAR));
    }

    #[test]
    fn rows_sit_under_their_group_headings() {
        let s = MenuState::full(Category::Picture, Settings::default());
        let shown = lines(&s);
        assert_eq!(shown[0], (Line::Group("SCALING".into()), at::ROWS_TOP));
        assert_eq!(
            shown[1],
            (Line::Row(idx("video.scale_mode")), at::ROWS_TOP + 26.0)
        );
    }

    #[test]
    fn a_held_slider_follows_the_pointer_until_released() {
        let mut s = MenuState::full(Category::Picture, Settings::default());
        let g = idx("video.gamma");
        let mut drag = None;
        pointer_into(
            &mut s,
            &mut drag,
            [(Interaction::Pressed, Hit::Slider(g, 0.0))].into_iter(),
            Some(g),
            |_| Some(1.0),
            true,
        );
        assert_eq!((s.settings.gamma, drag), (255, Some(g)));
        pointer_into(
            &mut s,
            &mut drag,
            std::iter::empty(),
            Some(g),
            |_| Some(0.0),
            true,
        );
        assert_eq!(s.settings.gamma, 55);
        pointer_into(
            &mut s,
            &mut drag,
            std::iter::empty(),
            None,
            |_| Some(0.5),
            false,
        );
        assert_eq!((s.settings.gamma, drag), (55, None));
        assert_eq!(s.hovered, None);
    }

    /// Every box that takes the pointer stops it there. A box that let it
    /// through had the panel under it pressed in the same frame: a press on
    /// Preset opened its list and the panel's press closed it again.
    #[test]
    fn every_box_that_takes_the_pointer_blocks_it() {
        let dir = std::env::temp_dir().join(format!("d2rust-menu-block-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = app(&dir);
        app.insert_resource(MenuTextSource(std::sync::Arc::new(
            super::super::text::TextImages::without_game_files(),
        )));
        app.world_mut().resource_mut::<OpenSettings>().0 = true;
        app.update();
        let mut q = app.world_mut().query::<(&HitTag, Option<&FocusPolicy>)>();
        let hits: Vec<_> = q
            .iter(app.world())
            .map(|(t, f)| (t.hit, f.copied()))
            .collect();
        assert!(hits.iter().any(|(h, _)| *h == Hit::Preset));
        assert!(hits.iter().any(|(h, _)| *h == Hit::Search));
        for (h, f) in hits {
            assert_eq!(f, Some(FocusPolicy::Block), "{h:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn app(dir: &std::path::Path) -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_message::<KeyboardInput>()
            .init_resource::<TextCache>()
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
        register(&mut app);
        app
    }

    fn pointer_shown(app: &mut App) -> bool {
        app.world_mut()
            .query::<&bevy::window::CursorOptions>()
            .iter(app.world())
            .all(|c| c.visible)
    }

    fn press(app: &mut App, key_code: KeyCode, logical_key: Key) {
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key_code);
        app.update();
    }

    fn roots(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<MenuRoot>>()
            .iter(app.world())
            .count()
    }

    /// Opened, the menu takes the keys, pauses single player, applies and
    /// saves each change at once, and Esc closes it.
    #[test]
    fn the_menu_opens_applies_and_saves_at_once_and_closes() {
        let dir = std::env::temp_dir().join(format!("d2rust-settings-menu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = app(&dir);
        app.update();
        assert_eq!(roots(&mut app), 0);
        app.world_mut().resource_mut::<OpenSettings>().0 = true;
        app.update();
        assert!(app.world().resource::<SettingsMenu>().open);
        assert_eq!(roots(&mut app), 1);
        assert!(pointer_shown(&mut app), "the pointer shows over the menu");
        // Display opens first; Tab to Picture, down past Scaling Filter and
        // Sharp Interface to Gamma, right.
        press(&mut app, KeyCode::Tab, Key::Tab);
        for _ in 0..3 {
            press(&mut app, KeyCode::ArrowDown, Key::ArrowDown);
        }
        press(&mut app, KeyCode::ArrowRight, Key::ArrowRight);
        assert_eq!(app.world().resource::<ConfigRes>().settings.gamma, 165);
        let saved = crate::app::config::load_settings(&dir).expect("saved");
        assert_eq!(saved.gamma, 165);
        let claim = *app.world().resource::<InputClaim>();
        assert!(claim.all && claim.pause);
        // The game saw none of the keys.
        assert!(!app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .just_pressed(KeyCode::ArrowRight));
        press(&mut app, KeyCode::Escape, Key::Escape);
        app.update();
        assert!(!app.world().resource::<SettingsMenu>().open);
        assert_eq!(roots(&mut app), 0);
        assert!(!pointer_shown(&mut app), "hidden again: no hardware cursor");
        assert_eq!(*app.world().resource::<InputClaim>(), InputClaim::default());
        // It reopens on the category it closed on.
        app.world_mut().resource_mut::<OpenSettings>().0 = true;
        app.update();
        assert_eq!(
            app.world().resource::<SettingsMenu>().state.category,
            Category::Picture
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The HD Text row switches the menu's own text at once: off, the
    /// game's bitmap fonts (none in this test, so the built-in font
    /// draws); on, images of the shipped outline font.
    #[test]
    fn hd_text_switches_the_menus_own_text_live() {
        let dir = std::env::temp_dir().join(format!("d2rust-menu-hd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        crate::world_view::ui_art::hd_text::set_hd_fonts(false);
        let mut app = app(&dir);
        app.insert_resource(MenuTextSource(std::sync::Arc::new(
            super::super::text::TextImages::without_game_files(),
        )));
        app.world_mut().resource_mut::<OpenSettings>().0 = true;
        app.update();
        let images = |app: &mut App| {
            app.world_mut()
                .query::<&ImageNode>()
                .iter(app.world())
                .count()
        };
        assert_eq!(images(&mut app), 0, "HD Text off, no game fonts here");
        // Display, Picture, Interface; UI Scale, FPS Counter, then HD Text: on.
        press(&mut app, KeyCode::Tab, Key::Tab);
        press(&mut app, KeyCode::Tab, Key::Tab);
        press(&mut app, KeyCode::ArrowDown, Key::ArrowDown);
        press(&mut app, KeyCode::ArrowDown, Key::ArrowDown);
        assert_eq!(
            app.world().resource::<SettingsMenu>().state.selected,
            Some(idx("video.hd_text"))
        );
        press(&mut app, KeyCode::ArrowRight, Key::ArrowRight);
        app.update();
        assert!(app.world().resource::<ConfigRes>().settings.hd_text);
        assert!(crate::world_view::ui_art::hd_text::hd_fonts());
        assert!(
            images(&mut app) > 10,
            "the menu's text is the outline font now"
        );
        press(&mut app, KeyCode::ArrowLeft, Key::ArrowLeft);
        app.update();
        assert_eq!(images(&mut app), 0, "and back");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Scripted keys (`play --input`, `play --control`) drive the open menu
    /// as the window's keys do, letters included; a closed menu takes none.
    #[test]
    fn scripted_keys_drive_the_open_menu() {
        let mut m = SettingsMenu::default();
        assert!(!m.script_key(KeyCode::ArrowDown));
        m.open = true;
        m.state = MenuState::full(Category::Display, Settings::default());
        for code in [KeyCode::KeyG, KeyCode::KeyA, KeyCode::KeyM, KeyCode::KeyM] {
            assert!(m.script_key(code));
        }
        assert_eq!(m.state.search, "gamm");
        assert_eq!(m.state.rows(), vec![idx("video.gamma")]);
        assert!(m.script_key(KeyCode::Escape));
        assert!(m.state.search.is_empty());
        assert_eq!(key_letter(KeyCode::Digit7), Some('7'));
        assert_eq!(key_letter(KeyCode::F1), None);
    }

    #[test]
    fn keys_map_to_the_menus_keys() {
        let k = |code, logical: Key, text: Option<&str>| KeyboardInput {
            key_code: code,
            logical_key: logical,
            state: ButtonState::Pressed,
            text: text.map(Into::into),
            repeat: false,
            window: Entity::PLACEHOLDER,
        };
        assert_eq!(
            menu_key(&k(KeyCode::Tab, Key::Tab, None), true),
            [MenuKey::PrevCategory]
        );
        assert_eq!(
            menu_key(
                &k(KeyCode::KeyG, Key::Character("g".into()), Some("g")),
                false
            ),
            [MenuKey::Char('g')]
        );
        assert!(menu_key(&k(KeyCode::F5, Key::F5, None), false).is_empty());
    }
}
