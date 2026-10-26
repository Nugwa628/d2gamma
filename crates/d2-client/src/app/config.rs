//! The player's config files, kept next to the saves: `settings.toml`
//! (sound, picture, display, automap and HUD choices) and `controls.toml`
//! (key bindings). The Options pages read and write them, and the play app
//! applies the window settings while it runs ([`WindowChoice`]).
//!
//! A missing `controls.toml` is written from the `original` preset; the
//! `dev` preset applies only when a file names it.
//!
//! `settings.toml` is versioned and strict: an unknown key, a wrong type or
//! a number out of range is an error, so a typo never turns silently into a
//! default. The exception is the display keys that name one of a fixed set
//! of choices (`window_size`, `scale_mode`, `frame_limit`, `menu_sides`,
//! `animation_frames`): a choice this
//! build doesn't know (one a newer build wrote, say) falls back to the
//! default instead of refusing the whole file.
//!
//! The window is sized in physical pixels: a 1920 × 1080 window is 1920 ×
//! 1080 screen pixels at any Windows display scaling.

use std::path::{Path, PathBuf};

use crate::controls::{self, Bindings, ControlsFile, Preset};
use crate::rules::camera::FrameSize;

/// `settings.toml` format version (every persisted format has one).
///
/// - 1: the first layout.
/// - 2: adds the display keys `[video] window_size`, `scale_mode`, `vsync`,
///   `frame_limit` and `menu_sides`.
/// - 3: the window modes are `windowed`, `windowed_borderless` and
///   `fullscreen` (versions 1 and 2 called the second `borderless`); the
///   window size `maximized` takes the place of `native`; adds
///   `[video] alt_enter_return`; `[video] resolution` gains 2, the 16:9
///   frame 1068 × 600, the default, and older files move to it.
/// - 4: adds `[video] native_ui` (the UI drawn at the output resolution,
///   on by default).
/// - 5: adds `[video] hd_text` (a text switch).
/// - 6: writes `[video] text_mode` (upscaled or hd), retaining legacy hd_text input.
/// - 7: adds `[video] animation_frames` (`original` or `smooth`, smooth by
///   default).
/// - 8: adds `[video] hd_text` again, as HD Text: all text drawn from
///   D2RUST's shipped outline font (off by default:
///   `world_view::ui_art::hd_fonts`).
/// - 9: drops `[video] resolution`: every player sees the one 1280 × 720
///   view ([`FrameSize::PLAY`]). An older file's `resolution` is read and
///   ignored, and the next write leaves it out.
/// - 10: adds `[controls]`: one 0/1 switch per item-moving shortcut
///   (`crate::ui::item_moves::Shortcut::key`), all on by default.
/// - 11: adds `[controls] market_key`: the key that opens the market
///   window (`crate::ui::market::MarketKey`, 0 Ctrl+I by default).
///
/// The text switches of versions 5 to 7 (`hd_text`, `text_mode`) meant
/// something else: a file older than 8 still loads with them, they are
/// ignored, and the next write leaves HD Text off.
///
/// An older file, or one without some of these keys, loads with the keys it
/// lacks at their defaults and its old names read as the new ones; it is
/// written back as the current version with every key on the next change.
pub const SETTINGS_VERSION: i64 = 11;

/// The oldest `settings.toml` version this build still reads.
const OLDEST_SETTINGS_VERSION: i64 = 1;

/// How the window is shown.
///
/// - **Windowed:** a window with a border and a title bar, maximized or at
///   one of the listed sizes ([`WindowSize`]). The default.
/// - **Windowed Borderless:** a window without a border covering the whole
///   display at its own resolution; the display mode never changes.
/// - **Fullscreen:** exclusive fullscreen at the monitor's own (current)
///   resolution and refresh rate, so the display mode doesn't change
///   either.
///
/// Alt+Enter switches between the mode in use and Windowed Borderless
/// ([`Settings::toggle_borderless`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowMode {
    #[default]
    Windowed,
    WindowedBorderless,
    Fullscreen,
}

impl WindowMode {
    /// The Video page's order ([`WINDOW_MODE_NAMES`]).
    pub const ALL: [WindowMode; 3] = [
        WindowMode::Windowed,
        WindowMode::WindowedBorderless,
        WindowMode::Fullscreen,
    ];

    /// The `[video] window_mode` text.
    pub fn key(self) -> &'static str {
        match self {
            WindowMode::Windowed => "windowed",
            WindowMode::WindowedBorderless => "windowed_borderless",
            WindowMode::Fullscreen => "fullscreen",
        }
    }

    /// The mode a stored text names in a file of `version`; `None` for any
    /// other text. Version 1 and 2 files call Windowed Borderless
    /// `borderless`.
    pub fn from_key(text: &str, version: i64) -> Option<WindowMode> {
        if version < 3 && text == "borderless" {
            return Some(WindowMode::WindowedBorderless);
        }
        WindowMode::ALL.into_iter().find(|m| m.key() == text)
    }
}

/// The names the Video page shows for [`WindowMode::ALL`], in its order.
pub const WINDOW_MODE_NAMES: [&str; 3] = ["Windowed", "Windowed Borderless", "Fullscreen"];

/// The size of a Windowed window: maximized (filling the monitor's work
/// area, the screen less the taskbar, with its title bar showing), or a
/// fixed size in physical pixels. Windowed Borderless and Fullscreen always
/// cover the whole monitor at its own resolution, so they ignore it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowSize {
    /// Maximized on its monitor. The default.
    #[default]
    Maximized,
    /// A fixed `width × height`.
    Fixed(u32, u32),
}

/// The window sizes the player can pick, in the Video page's order:
/// maximized, the 16:9 sizes, then the 4:3 sizes in two families that
/// hold the 800 × 600 and the 640 × 480 frame at whole scales (1×, 2×, 3×).
pub const WINDOW_SIZES: [WindowSize; 10] = [
    WindowSize::Maximized,
    WindowSize::Fixed(1920, 1080),
    WindowSize::Fixed(2560, 1440),
    WindowSize::Fixed(3840, 2160),
    WindowSize::Fixed(800, 600),
    WindowSize::Fixed(1600, 1200),
    WindowSize::Fixed(2400, 1800),
    WindowSize::Fixed(640, 480),
    WindowSize::Fixed(1280, 960),
    WindowSize::Fixed(1920, 1440),
];

/// The names the Video page shows for [`WINDOW_SIZES`], in the same order.
pub const WINDOW_SIZE_NAMES: [&str; 10] = [
    "Maximized",
    "1920x1080",
    "2560x1440",
    "3840x2160",
    "800x600",
    "1600x1200",
    "2400x1800",
    "640x480",
    "1280x960",
    "1920x1440",
];

impl WindowSize {
    /// The `[video] window_size` text: `"maximized"` or `"WIDTHxHEIGHT"`.
    pub fn key(self) -> String {
        match self {
            WindowSize::Maximized => "maximized".into(),
            WindowSize::Fixed(w, h) => format!("{w}x{h}"),
        }
    }

    /// The listed size a stored text of a file of `version` names; `None`
    /// for any other text. Version 1 and 2 files call the monitor-filling
    /// window `native`: it is Maximized now.
    pub fn from_key(text: &str, version: i64) -> Option<WindowSize> {
        if version < 3 && text == "native" {
            return Some(WindowSize::Maximized);
        }
        WINDOW_SIZES.into_iter().find(|s| s.key() == text)
    }

    /// The size's place in [`WINDOW_SIZES`] (0, Maximized, for an unlisted
    /// one).
    pub fn index(self) -> usize {
        WINDOW_SIZES.iter().position(|&s| s == self).unwrap_or(0)
    }

    /// Whether the size can be picked: it fits on the monitor (when the
    /// monitor is known). Maximized always can. A window smaller than the
    /// frame shows it scaled down (fit).
    pub fn fits(self, monitor: Option<(u32, u32)>) -> bool {
        match self {
            WindowSize::Maximized => true,
            WindowSize::Fixed(w, h) => monitor.is_none_or(|(mw, mh)| w <= mw && h <= mh),
        }
    }

    /// The size a window gets on this monitor: this one when it fits, else
    /// the largest listed size that does, else maximized.
    pub fn or_fitting(self, monitor: Option<(u32, u32)>) -> WindowSize {
        if self.fits(monitor) {
            return self;
        }
        WINDOW_SIZES
            .into_iter()
            .filter(|s| *s != WindowSize::Maximized && s.fits(monitor))
            .max_by_key(|s| match s {
                WindowSize::Fixed(w, h) => u64::from(*w) * u64::from(*h),
                WindowSize::Maximized => 0,
            })
            .unwrap_or(WindowSize::Maximized)
    }
}

/// How the fixed game frame fills the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScaleMode {
    /// The largest scale that keeps the frame's shape, drawn sharp: every
    /// game pixel stays a solid block and only the screen pixels where two
    /// game pixels meet are blended.
    #[default]
    Fit,
    /// The largest whole-number scale, every game pixel exactly the same
    /// size, with black bars around.
    Integer,
}

impl ScaleMode {
    pub const ALL: [ScaleMode; 2] = [ScaleMode::Fit, ScaleMode::Integer];

    /// The `[video] scale_mode` text.
    pub fn key(self) -> &'static str {
        match self {
            ScaleMode::Fit => "fit",
            ScaleMode::Integer => "integer",
        }
    }
}

/// How large the mouse pointer (and an item held on it) is drawn: Auto
/// follows the picture's scale, rounded to a whole number so every pointer
/// pixel stays an even block (3x at 2160p); the others are that whole
/// scale on every screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorSize {
    #[default]
    Auto,
    X1,
    X2,
    X3,
    X4,
}

impl CursorSize {
    pub const ALL: [CursorSize; 5] = [
        CursorSize::Auto,
        CursorSize::X1,
        CursorSize::X2,
        CursorSize::X3,
        CursorSize::X4,
    ];

    /// The `[video] cursor_size` text.
    pub fn key(self) -> &'static str {
        match self {
            CursorSize::Auto => "auto",
            CursorSize::X1 => "1x",
            CursorSize::X2 => "2x",
            CursorSize::X3 => "3x",
            CursorSize::X4 => "4x",
        }
    }

    /// The whole scale the pointer is drawn at when the picture is shown
    /// at `sx` × `sy`: Auto rounds the smaller of the two, at least 1.
    pub fn scale(self, sx: f32, sy: f32) -> u32 {
        match self {
            CursorSize::Auto => (sx.min(sy).round() as u32).max(1),
            CursorSize::X1 => 1,
            CursorSize::X2 => 2,
            CursorSize::X3 => 3,
            CursorSize::X4 => 4,
        }
    }
}

/// The names the settings show for [`CursorSize::ALL`].
pub const CURSOR_SIZE_NAMES: [&str; 5] = ["Auto", "1x", "2x", "3x", "4x"];

/// How the game frame is scaled to the window
/// (`world_view::sharp`): Sharp Bilinear keeps every game pixel a solid
/// block and blends only where two meet; Nearest repeats pixels; the
/// cubic filters (Catmull-Rom, Lanczos) soften; Smooth Edges is Sharp
/// Bilinear with diagonal edges in the art read along their direction
/// (optional, never the default). At a whole-number scale Sharp Bilinear
/// and Nearest are the same picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScalingFilter {
    #[default]
    SharpBilinear,
    Nearest,
    CatmullRom,
    Lanczos,
    SmoothEdges,
}

impl ScalingFilter {
    pub const ALL: [ScalingFilter; 5] = [
        ScalingFilter::SharpBilinear,
        ScalingFilter::Nearest,
        ScalingFilter::CatmullRom,
        ScalingFilter::Lanczos,
        ScalingFilter::SmoothEdges,
    ];

    /// The `[video] scaling_filter` text.
    pub fn key(self) -> &'static str {
        match self {
            ScalingFilter::SharpBilinear => "sharp_bilinear",
            ScalingFilter::Nearest => "nearest",
            ScalingFilter::CatmullRom => "catmull_rom",
            ScalingFilter::Lanczos => "lanczos",
            ScalingFilter::SmoothEdges => "smooth_edges",
        }
    }

    /// The number the presenting shaders read (WGSL `Present.kind.x`): the
    /// filter's place in [`Self::ALL`].
    pub fn index(self) -> u8 {
        match self {
            ScalingFilter::SharpBilinear => 0,
            ScalingFilter::Nearest => 1,
            ScalingFilter::CatmullRom => 2,
            ScalingFilter::Lanczos => 3,
            ScalingFilter::SmoothEdges => 4,
        }
    }

    /// The filter of [`Self::index`] `i`; Sharp Bilinear for any other.
    pub fn from_index(i: u8) -> Self {
        Self::ALL.get(usize::from(i)).copied().unwrap_or_default()
    }
}

/// The names the settings show for [`ScalingFilter::ALL`].
pub const SCALING_FILTER_NAMES: [&str; 5] = [
    "Sharp Bilinear",
    "Nearest",
    "Catmull-Rom",
    "Lanczos",
    "Smooth Edges",
];

/// Edge smoothing of the scaled picture (`world_view::post`): Off, or
/// FXAA at three strengths (how far it follows an edge and how faint an
/// edge it smooths).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FxaaLevel {
    #[default]
    Off,
    Low,
    Medium,
    High,
}

impl FxaaLevel {
    pub const ALL: [FxaaLevel; 4] = [
        FxaaLevel::Off,
        FxaaLevel::Low,
        FxaaLevel::Medium,
        FxaaLevel::High,
    ];

    /// The `[video] fxaa` text.
    pub fn key(self) -> &'static str {
        match self {
            FxaaLevel::Off => "off",
            FxaaLevel::Low => "low",
            FxaaLevel::Medium => "medium",
            FxaaLevel::High => "high",
        }
    }
}

/// The names the settings show for [`FxaaLevel::ALL`].
pub const FXAA_NAMES: [&str; 4] = ["Off", "Low", "Medium", "High"];

/// A colour grade of the game world (`world_view::looks`): Original (the
/// art as it is) or one of our own looks. Vivid is the default look (with
/// Sharpen 25, the owner's pick of the look samples).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Look {
    Original,
    Warm,
    Cool,
    #[default]
    Vivid,
    Muted,
}

impl Look {
    pub const ALL: [Look; 5] = [
        Look::Original,
        Look::Warm,
        Look::Cool,
        Look::Vivid,
        Look::Muted,
    ];

    /// The `[video] look` text.
    pub fn key(self) -> &'static str {
        match self {
            Look::Original => "original",
            Look::Warm => "warm",
            Look::Cool => "cool",
            Look::Vivid => "vivid",
            Look::Muted => "muted",
        }
    }
}

/// The names the settings show for [`Look::ALL`].
pub const LOOK_NAMES: [&str; 5] = ["Original", "Warm", "Cool", "Vivid", "Muted"];

/// The post-processing sliders of `[video]`: `(key, min, max, default)`.
/// Brightness 50 and Saturation 100 are unchanged colour; Vibrance starts
/// at 0 (off) and Sharpen at 25 (the default look); Bloom Strength is the
/// glow's strength while the Bloom checkbox (`bloom`, off) is on.
pub const POST_KEYS: [(&str, u8, u8, u8); 5] = [
    ("brightness", 0, 100, 50),
    ("saturation", 0, 200, 100),
    ("vibrance", 0, 100, 0),
    ("sharpen", 0, 100, 25),
    ("bloom_strength", 0, 100, 50),
];

/// What fills the window beside the front end's 4:3 picture when the
/// window is wider than 4:3 (a 16:9 screen). A window of 4:3 or narrower
/// shows black bars whatever the choice. The default is black, or the
/// one a mod game sets ([`MenuSides::set_game_default`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuSides {
    /// Plain black.
    Black,
    /// A dim, blurred copy of the menu picture stretched to cover the
    /// window, so the art seems to carry on past its edges.
    Blur,
    /// The picture's own edge colours carried out to the window's edges:
    /// its outer columns smeared across each side, blurred, and shaded
    /// toward the window's edge (`super::front_fill::extend`).
    Extend,
}

/// [`MenuSides::default`] as an index of [`MenuSides::ALL`].
static GAME_DEFAULT_SIDES: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

impl Default for MenuSides {
    /// [`MenuSides::Black`], or the mod game's default.
    fn default() -> Self {
        let i = GAME_DEFAULT_SIDES.load(std::sync::atomic::Ordering::Relaxed);
        MenuSides::ALL
            .get(usize::from(i))
            .copied()
            .unwrap_or(MenuSides::Black)
    }
}

impl MenuSides {
    pub const ALL: [MenuSides; 3] = [MenuSides::Black, MenuSides::Blur, MenuSides::Extend];

    /// The `[video] menu_sides` text.
    pub fn key(self) -> &'static str {
        match self {
            MenuSides::Black => "black",
            MenuSides::Blur => "blur",
            MenuSides::Extend => "extend",
        }
    }

    /// Sets the default a settings file without `menu_sides` gets (a mod
    /// game's own; D2Gamma's is black). Called before the settings are
    /// read.
    pub fn set_game_default(sides: MenuSides) {
        let i = MenuSides::ALL.iter().position(|&m| m == sides).unwrap_or(0);
        GAME_DEFAULT_SIDES.store(i as u8, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Which pictures the compositor draws for an animation that has generated
/// frames resident: the source frames, or the generated in-between frames.
/// The source frames are always used for simulation and frame events; this
/// choice is visual only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationFrameMode {
    /// Today's source frames only.
    Original,
    /// Generated in-between frames where a valid pack frame is resident.
    #[default]
    Smooth,
}

impl AnimationFrameMode {
    pub const ALL: [AnimationFrameMode; 2] =
        [AnimationFrameMode::Original, AnimationFrameMode::Smooth];

    /// The `[video] animation_frames` text.
    pub fn key(self) -> &'static str {
        match self {
            AnimationFrameMode::Original => "original",
            AnimationFrameMode::Smooth => "smooth",
        }
    }
}

/// The names the Video page shows for [`AnimationFrameMode::ALL`].
pub const ANIMATION_FRAME_NAMES: [&str; 2] = ["Original", "Smooth"];

/// The frame limits the player can pick, in frames per second; 0 is Off
/// (no limit of our own).
pub const FRAME_LIMITS: [u16; 8] = [0, 30, 60, 120, 144, 165, 240, 360];

/// Everything `settings.toml` stores. The sound, picture and automap
/// numbers use the game's own scales (0–100 volumes, gamma 55–255, and so
/// on). The game view is not a setting: it is always 1280 × 720
/// ([`FrameSize::PLAY`]). `window_mode` and `window_size` are
/// how the window shows on the monitor; `scale_mode`, `vsync` and
/// `frame_limit` are how the frame is presented in it, and `menu_sides`
/// what fills the window beside the front end's 4:3 picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub master_volume: u8,
    pub music_volume: u8,
    pub mixer: u8,
    pub positional_bias: u8,
    pub npc_speech: u8,
    pub window_mode: WindowMode,
    pub window_size: WindowSize,
    /// The mode Alt+Enter goes back to from Windowed Borderless: Windowed
    /// or Fullscreen, whichever was in use last ([`Self::toggle_borderless`]).
    pub alt_enter_return: WindowMode,
    pub scale_mode: ScaleMode,
    /// VSync on: frames wait for the monitor's refresh.
    pub vsync: bool,
    /// Frames per second, one of [`FRAME_LIMITS`]; 0 is Off.
    pub frame_limit: u16,
    /// Generated in-between animation frames, or the source frames only.
    pub animation_frames: AnimationFrameMode,
    pub menu_sides: MenuSides,
    /// The HUD, panels, menus and text drawn at the output resolution
    /// when the frame is shown enlarged (`world_view::native_ui`).
    pub native_ui: bool,
    /// The mouse pointer's size ([`CursorSize`]).
    pub cursor_size: CursorSize,
    /// UI Scale: the HUD, panels and their text drawn at this percent of
    /// the picture's scale (`ui::ui_space`; 75 to 120, 100 unchanged).
    pub ui_scale: u16,
    /// How the frame is scaled to the window ([`ScalingFilter`]).
    pub scaling_filter: ScalingFilter,
    /// The post-processing pass (`world_view::post`, [`POST_KEYS`]):
    /// brightness 0–100 (50 unchanged), saturation 0–200 (100 unchanged),
    /// vibrance 0–100, sharpen 0–100, and FXAA.
    pub brightness: u8,
    pub saturation: u8,
    pub vibrance: u8,
    pub sharpen: u8,
    pub fxaa: FxaaLevel,
    /// HD Text: every text glyph drawn from the shipped outline font, at
    /// the bitmap fonts' layout (`world_view::ui_art::hd_fonts`); off, the
    /// game's own bitmap fonts.
    pub hd_text: bool,
    /// The glow around bright light (off by default) and its strength
    /// 0–100; the world's colour [`Look`] (Vivid by default).
    pub bloom: bool,
    pub bloom_strength: u8,
    pub look: Look,
    /// The realm lobby's background chosen in its picker.
    pub lobby_background: crate::ui::front_end::screens::online::backdrop::Backdrop,
    pub light_quality: u8,
    pub blended_shadows: u8,
    pub perspective: u8,
    pub gamma: u16,
    pub contrast: u8,
    pub automap_mode: u8,
    pub automap_fade: u8,
    pub automap_centers: u8,
    pub automap_party: u8,
    pub automap_party_names: u8,
    /// The control panel's life and mana globe text toggles, saved as soon
    /// as the player flips them.
    pub show_hp_text: u8,
    pub show_mp_text: u8,
    /// The FPS counter in the top-left corner (`ui::settings::fps`): 1
    /// shows it.
    pub fps_counter: u8,
    /// The minimap in the top-right corner of the world
    /// (`ui::settings::minimap`): 1 shows it.
    pub minimap: u8,
    /// The minimap's size: 0 small, 1 medium, 2 large
    /// ([`crate::ui::automap::minimap::SIZES`]).
    pub minimap_size: u8,
    /// The minimap's opacity in percent, 20 to 100.
    pub minimap_opacity: u8,
    /// The item-moving shortcuts switched on, one bit per
    /// [`crate::ui::item_moves::Shortcut`] (`[controls]`, all on by
    /// default).
    pub item_shortcuts: crate::ui::item_moves::ShortcutSet,
    /// The market window's key: the index of
    /// [`crate::ui::market::MarketKey::ALL`] (`[controls]`, 0 Ctrl+I).
    pub market_key: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            master_volume: 100,
            music_volume: 50,
            mixer: 0,
            positional_bias: 50,
            npc_speech: 2,
            // A new settings file: a maximized window with its title bar,
            // at the monitor's own resolution.
            window_mode: WindowMode::Windowed,
            window_size: WindowSize::Maximized,
            alt_enter_return: WindowMode::Windowed,
            scale_mode: ScaleMode::default(),
            vsync: true,
            frame_limit: 0,
            animation_frames: AnimationFrameMode::Smooth,
            menu_sides: MenuSides::default(),
            native_ui: true,
            cursor_size: CursorSize::Auto,
            ui_scale: crate::ui::ui_space::UI_SCALE_DEFAULT,
            scaling_filter: ScalingFilter::SharpBilinear,
            brightness: 50,
            saturation: 100,
            vibrance: 0,
            sharpen: 25,
            fxaa: FxaaLevel::Off,
            hd_text: false,
            bloom: false,
            bloom_strength: 50,
            look: Look::Vivid,
            lobby_background: Default::default(),
            light_quality: 2,
            blended_shadows: 1,
            perspective: 1,
            gamma: 155,
            contrast: 100,
            automap_mode: 0,
            automap_fade: 0,
            automap_centers: 1,
            automap_party: 1,
            automap_party_names: 1,
            show_hp_text: 0,
            show_mp_text: 0,
            fps_counter: 0,
            minimap: 0,
            minimap_size: 1,
            minimap_opacity: 85,
            item_shortcuts: crate::ui::item_moves::ShortcutSet::ALL,
            market_key: 0,
        }
    }
}

/// The integer keys: `(section, key, min, max)`, in file order.
pub const INT_KEYS: [(&str, &str, i64, i64); 32] = [
    ("audio", "master_volume", 0, 100),
    ("audio", "music_volume", 0, 100),
    ("audio", "mixer", 0, 2),
    ("audio", "positional_bias", 0, 100),
    ("audio", "npc_speech", 0, 2),
    ("video", "light_quality", 0, 2),
    ("video", "blended_shadows", 0, 1),
    ("video", "perspective", 0, 1),
    ("video", "gamma", 55, 255),
    ("video", "contrast", 0, 100),
    ("automap", "mode", 0, 1),
    ("automap", "fade", 0, 3),
    ("automap", "centers", 0, 1),
    ("automap", "party", 0, 1),
    ("automap", "party_names", 0, 1),
    ("hud", "show_hp_text", 0, 1),
    ("hud", "show_mp_text", 0, 1),
    ("hud", "fps_counter", 0, 1),
    ("hud", "minimap", 0, 1),
    ("hud", "minimap_size", 0, 2),
    ("hud", "minimap_opacity", 20, 100),
    // The market key, `MarketKey::ALL`'s index (Ctrl+I, M, K, J, U, off).
    ("controls", "market_key", 0, 5),
    // The item-moving shortcuts, in `Shortcut::ALL` order (keys
    // `Shortcut::key`; checked by a test).
    ("controls", "move_stash", 0, 1),
    ("controls", "move_cube", 0, 1),
    ("controls", "move_trade", 0, 1),
    ("controls", "stack_to_slot", 0, 1),
    ("controls", "take_count", 0, 1),
    ("controls", "take_all", 0, 1),
    ("controls", "drop_item", 0, 1),
    ("controls", "into_cube", 0, 1),
    ("controls", "quick_sell", 0, 1),
    ("controls", "tab_drop", 0, 1),
];

/// The first [`INT_KEYS`] index of the item-moving shortcuts.
const SHORTCUT_KEYS: usize = 22;

impl Settings {
    /// Picks window mode `m` (the Video page's Window row). Windowed and
    /// Fullscreen also become the mode Alt+Enter returns to.
    pub fn set_window_mode(&mut self, m: WindowMode) {
        self.window_mode = m;
        if m != WindowMode::WindowedBorderless {
            self.alt_enter_return = m;
        }
    }

    /// Alt+Enter: Windowed or Fullscreen goes to Windowed Borderless, and
    /// Windowed Borderless goes back to the mode it came from (Windowed
    /// when it was picked on the Video page with nothing before it).
    pub fn toggle_borderless(&mut self) {
        match self.window_mode {
            WindowMode::WindowedBorderless => self.window_mode = self.alt_enter_return,
            m => {
                self.alt_enter_return = m;
                self.window_mode = WindowMode::WindowedBorderless;
            }
        }
    }

    /// The value of integer key `i` of [`INT_KEYS`].
    pub fn get(&self, i: usize) -> i64 {
        match i {
            0 => self.master_volume.into(),
            1 => self.music_volume.into(),
            2 => self.mixer.into(),
            3 => self.positional_bias.into(),
            4 => self.npc_speech.into(),
            5 => self.light_quality.into(),
            6 => self.blended_shadows.into(),
            7 => self.perspective.into(),
            8 => self.gamma.into(),
            9 => self.contrast.into(),
            10 => self.automap_mode.into(),
            11 => self.automap_fade.into(),
            12 => self.automap_centers.into(),
            13 => self.automap_party.into(),
            14 => self.automap_party_names.into(),
            15 => self.show_hp_text.into(),
            16 => self.show_mp_text.into(),
            17 => self.fps_counter.into(),
            18 => self.minimap.into(),
            19 => self.minimap_size.into(),
            20 => self.minimap_opacity.into(),
            21 => self.market_key.into(),
            i => {
                let s = crate::ui::item_moves::Shortcut::ALL[i - SHORTCUT_KEYS];
                i64::from(self.item_shortcuts.has(s))
            }
        }
    }

    /// The value of post-processing slider `key` ([`POST_KEYS`]); 0 for a
    /// key that isn't one.
    pub fn post_value(&self, key: &str) -> u8 {
        match key {
            "brightness" => self.brightness,
            "saturation" => self.saturation,
            "vibrance" => self.vibrance,
            "sharpen" => self.sharpen,
            "bloom_strength" => self.bloom_strength,
            _ => 0,
        }
    }

    /// Sets post-processing slider `key` (the caller keeps `v` in range).
    pub fn set_post_value(&mut self, key: &str, v: u8) {
        match key {
            "brightness" => self.brightness = v,
            "saturation" => self.saturation = v,
            "vibrance" => self.vibrance = v,
            "sharpen" => self.sharpen = v,
            "bloom_strength" => self.bloom_strength = v,
            _ => {}
        }
    }

    /// Set integer key `i` (the caller keeps `v` in range).
    pub fn set(&mut self, i: usize, v: i64) {
        match i {
            0 => self.master_volume = v as u8,
            1 => self.music_volume = v as u8,
            2 => self.mixer = v as u8,
            3 => self.positional_bias = v as u8,
            4 => self.npc_speech = v as u8,
            5 => self.light_quality = v as u8,
            6 => self.blended_shadows = v as u8,
            7 => self.perspective = v as u8,
            8 => self.gamma = v as u16,
            9 => self.contrast = v as u8,
            10 => self.automap_mode = v as u8,
            11 => self.automap_fade = v as u8,
            12 => self.automap_centers = v as u8,
            13 => self.automap_party = v as u8,
            14 => self.automap_party_names = v as u8,
            15 => self.show_hp_text = v as u8,
            16 => self.show_mp_text = v as u8,
            17 => self.fps_counter = v as u8,
            18 => self.minimap = v as u8,
            19 => self.minimap_size = v as u8,
            20 => self.minimap_opacity = v as u8,
            21 => self.market_key = v as u8,
            i => {
                let s = crate::ui::item_moves::Shortcut::ALL[i - SHORTCUT_KEYS];
                self.item_shortcuts.set(s, v != 0);
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{path}: {message}")]
    Io { path: String, message: String },
    #[error("settings.toml: {0}")]
    Settings(String),
}

/// The config folder: the parent of the save folder (the player's D2RUST
/// folder for the default `...\D2RUST\saves`), else the save folder itself.
pub fn config_dir(save_dir: &Path) -> PathBuf {
    save_dir
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| save_dir.to_path_buf(), Path::to_path_buf)
}

pub fn settings_path(dir: &Path) -> PathBuf {
    dir.join("settings.toml")
}

pub fn controls_path(dir: &Path) -> PathBuf {
    dir.join("controls.toml")
}

/// The folder of `controls.toml`: the folder of `spec_path`, the file the
/// Configure Controls screen saves to
/// ([`crate::ui::front_end::screens::controls::config_path`]), so a
/// rebind is what the next start loads; `fallback` when there is none.
pub fn controls_dir(spec_path: Option<&Path>, fallback: &Path) -> PathBuf {
    spec_path
        .and_then(Path::parent)
        .map_or_else(|| fallback.to_path_buf(), Path::to_path_buf)
}

/// Reads one of the display keys of `[video]` of a file of `version` into
/// `s`; `false` when `key` isn't one. A wrong type is an error, and so is
/// an unknown window mode; a size, scale or limit this build doesn't list
/// is the default.
fn parse_display_key(
    s: &mut Settings,
    version: i64,
    key: &str,
    v: &toml_edit::Item,
) -> Result<bool, ConfigError> {
    let bad = |m: &str| ConfigError::Settings(m.into());
    let mode = |v: &toml_edit::Item, key: &str| {
        v.as_str()
            .and_then(|t| WindowMode::from_key(t, version))
            .ok_or_else(|| {
                bad(&format!(
                    "video.{key} must be windowed, windowed_borderless or fullscreen"
                ))
            })
    };
    match key {
        "window_mode" => s.window_mode = mode(v, key)?,
        "alt_enter_return" => s.alt_enter_return = mode(v, key)?,
        "window_size" => {
            let text = v
                .as_str()
                .ok_or_else(|| bad("video.window_size must be text, like \"1920x1080\""))?;
            s.window_size = WindowSize::from_key(text, version).unwrap_or_default();
        }
        "scale_mode" => {
            let text = v
                .as_str()
                .ok_or_else(|| bad("video.scale_mode must be \"fit\" or \"integer\""))?;
            s.scale_mode = ScaleMode::ALL
                .into_iter()
                .find(|m| m.key() == text)
                .unwrap_or_default();
        }
        "vsync" => {
            s.vsync = v
                .as_bool()
                .ok_or_else(|| bad("video.vsync must be true or false"))?;
        }
        "native_ui" => {
            s.native_ui = v
                .as_bool()
                .ok_or_else(|| bad("video.native_ui must be true or false"))?;
        }
        // HD Text (version 8 on). The switches of versions 5 to 7 under the
        // same names meant something else: ignored.
        "hd_text" if version >= 8 => {
            s.hd_text = v
                .as_bool()
                .ok_or_else(|| bad("video.hd_text must be true or false"))?;
        }
        "text_mode" | "hd_text" => {}
        "ui_scale" => {
            use crate::ui::ui_space::{UI_SCALE_MAX, UI_SCALE_MIN, UI_SCALE_SAVED_MAX};
            match v.as_integer() {
                Some(n) if (i64::from(UI_SCALE_MIN)..=i64::from(UI_SCALE_MAX)).contains(&n) => {
                    s.ui_scale = n as u16;
                }
                // A file saved while the slider went further: the largest.
                Some(n) if n > i64::from(UI_SCALE_MAX) && n <= i64::from(UI_SCALE_SAVED_MAX) => {
                    s.ui_scale = UI_SCALE_MAX;
                }
                _ => {
                    return Err(bad(&format!(
                        "video.ui_scale must be {UI_SCALE_MIN} to {UI_SCALE_MAX}"
                    )))
                }
            }
        }
        "fxaa" => {
            let text = v.as_str().ok_or_else(|| {
                bad("video.fxaa must be \"off\", \"low\", \"medium\" or \"high\"")
            })?;
            s.fxaa = FxaaLevel::ALL
                .into_iter()
                .find(|m| m.key() == text)
                .unwrap_or_default();
        }
        "bloom" => {
            s.bloom = v
                .as_bool()
                .ok_or_else(|| bad("video.bloom must be true or false"))?;
        }
        "look" => {
            let text = v.as_str().ok_or_else(|| {
                bad("video.look must be \"original\", \"warm\", \"cool\", \"vivid\" or \"muted\"")
            })?;
            s.look = Look::ALL
                .into_iter()
                .find(|m| m.key() == text)
                .unwrap_or_default();
        }
        "lobby_background" => {
            let text = v
                .as_str()
                .ok_or_else(|| bad("video.lobby_background must be a background's name"))?;
            s.lobby_background =
                crate::ui::front_end::screens::online::backdrop::Backdrop::from_key(text)
                    .unwrap_or_default();
        }
        k if POST_KEYS.iter().any(|p| p.0 == k) => {
            let (_, lo, hi, _) = POST_KEYS.into_iter().find(|p| p.0 == k).expect("listed");
            match v.as_integer() {
                Some(n) if (i64::from(lo)..=i64::from(hi)).contains(&n) => {
                    s.set_post_value(k, n as u8);
                }
                _ => return Err(bad(&format!("video.{k} must be {lo} to {hi}"))),
            }
        }
        // The retired view row (versions 1 to 8): every player sees the one
        // 1280 x 720 view now, so a stored frame has nothing left to set.
        "resolution" if version < 9 => {}
        "frame_limit" => {
            let n = v
                .as_integer()
                .ok_or_else(|| bad("video.frame_limit must be a number"))?;
            s.frame_limit = FRAME_LIMITS
                .into_iter()
                .find(|&l| i64::from(l) == n)
                .unwrap_or(0);
        }
        "animation_frames" => {
            let text = v
                .as_str()
                .ok_or_else(|| bad("video.animation_frames must be \"original\" or \"smooth\""))?;
            s.animation_frames = AnimationFrameMode::ALL
                .into_iter()
                .find(|m| m.key() == text)
                .unwrap_or_default();
        }
        "menu_sides" => {
            let text = v
                .as_str()
                .ok_or_else(|| bad("video.menu_sides must be \"black\", \"blur\" or \"extend\""))?;
            s.menu_sides = MenuSides::ALL
                .into_iter()
                .find(|m| m.key() == text)
                .unwrap_or_default();
        }
        "cursor_size" => {
            let text = v
                .as_str()
                .ok_or_else(|| bad("video.cursor_size must be \"auto\" or \"1x\" to \"4x\""))?;
            s.cursor_size = CursorSize::ALL
                .into_iter()
                .find(|m| m.key() == text)
                .unwrap_or_default();
        }
        "scaling_filter" => {
            let text = v.as_str().ok_or_else(|| {
                bad("video.scaling_filter must be \"sharp_bilinear\", \"nearest\", \"catmull_rom\", \"lanczos\" or \"smooth_edges\"")
            })?;
            s.scaling_filter = ScalingFilter::ALL
                .into_iter()
                .find(|m| m.key() == text)
                .unwrap_or_default();
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// Parse `settings.toml`. Strict: an unknown key, a wrong type or an
/// out-of-range value is an error naming the key.
pub fn parse_settings(text: &str) -> Result<Settings, ConfigError> {
    use toml_edit::Document;
    let bad = |m: String| ConfigError::Settings(m);
    let doc = Document::parse(text).map_err(|e| bad(e.message().to_string()))?;
    let mut s = Settings::default();
    let version = match doc.get("version").and_then(|v| v.as_integer()) {
        Some(v) if (OLDEST_SETTINGS_VERSION..=SETTINGS_VERSION).contains(&v) => v,
        Some(v) => return Err(bad(format!("version {v} is not supported"))),
        None => return Err(bad("missing version".into())),
    };
    for (k, item) in doc.as_table().iter() {
        match k {
            "version" => {}
            "controls" if version < 10 => {
                return Err(bad(format!(
                    "[controls] needs version 10 (this file is {version})"
                )))
            }
            "audio" | "video" | "automap" | "hud" | "controls" => {
                let t = item
                    .as_table_like()
                    .ok_or_else(|| bad(format!("[{k}] must be a table")))?;
                for (vk, v) in t.iter() {
                    if k == "video" && parse_display_key(&mut s, version, vk, v)? {
                        continue;
                    }
                    let Some(i) = INT_KEYS.iter().position(|e| e.0 == k && e.1 == vk) else {
                        return Err(bad(format!("unknown key {k}.{vk}")));
                    };
                    let (_, _, lo, hi) = INT_KEYS[i];
                    match v.as_integer() {
                        Some(n) if (lo..=hi).contains(&n) => s.set(i, n),
                        _ => return Err(bad(format!("{k}.{vk} must be {lo} to {hi}"))),
                    }
                }
            }
            other => return Err(bad(format!("unknown key {other}"))),
        }
    }
    // Every older version is a subset of the current one: the keys it
    // lacks were left at their defaults above, and its old names were read
    // as the new ones. Alt+Enter returns to Windowed or Fullscreen only,
    // and to the mode in use when that's one of them.
    if s.alt_enter_return == WindowMode::WindowedBorderless {
        return Err(bad(
            "video.alt_enter_return must be windowed or fullscreen".into()
        ));
    }
    if s.window_mode != WindowMode::WindowedBorderless {
        s.alt_enter_return = s.window_mode;
    }
    Ok(s)
}

/// The frame a game runs in: the one player view, [`FrameSize::PLAY`],
/// unless a test tool's `--res` chose another. `FrameSize::set_play` takes
/// it before the app starts.
pub fn play_frame(res: Option<FrameSize>) -> FrameSize {
    res.unwrap_or(FrameSize::PLAY)
}

/// Sets the process's play frame, the one player view, unless a test
/// tool's `--res` set it already ([`play_frame`]); returns the frame in
/// force. No setting and no window changes it: the window only changes how
/// large it is shown ([`crate::ui::Presentation`]).
pub fn set_play_frame() -> Result<FrameSize, crate::rules::camera::PlayFrameError> {
    let frame = play_frame(FrameSize::play_chosen());
    FrameSize::set_play(frame)?;
    Ok(frame)
}

pub fn write_settings(s: &Settings) -> String {
    let mut out = format!("version = {SETTINGS_VERSION}\n");
    let mut section = "";
    for (i, (sec, key, _, _)) in INT_KEYS.iter().enumerate() {
        if *sec != section {
            section = sec;
            out.push_str(&format!("\n[{sec}]\n"));
        }
        // The display keys open `[video]`, before its numbers.
        if *sec == "video" && *key == "light_quality" {
            out.push_str(&format!("window_mode = \"{}\"\n", s.window_mode.key()));
            out.push_str(&format!("window_size = \"{}\"\n", s.window_size.key()));
            out.push_str(&format!(
                "alt_enter_return = \"{}\"\n",
                s.alt_enter_return.key()
            ));
            out.push_str(&format!("scale_mode = \"{}\"\n", s.scale_mode.key()));
            out.push_str(&format!("vsync = {}\n", s.vsync));
            out.push_str(&format!("frame_limit = {}\n", s.frame_limit));
            out.push_str(&format!(
                "animation_frames = \"{}\"\n",
                s.animation_frames.key()
            ));
            out.push_str(&format!("menu_sides = \"{}\"\n", s.menu_sides.key()));
            out.push_str(&format!("native_ui = {}\n", s.native_ui));
            out.push_str(&format!("cursor_size = \"{}\"\n", s.cursor_size.key()));
            out.push_str(&format!("ui_scale = {}\n", s.ui_scale));
            out.push_str(&format!(
                "scaling_filter = \"{}\"\n",
                s.scaling_filter.key()
            ));
            for (key, ..) in POST_KEYS {
                out.push_str(&format!("{key} = {}\n", s.post_value(key)));
            }
            out.push_str(&format!("fxaa = \"{}\"\n", s.fxaa.key()));
            out.push_str(&format!("hd_text = {}\n", s.hd_text));
            out.push_str(&format!("bloom = {}\n", s.bloom));
            out.push_str(&format!("look = \"{}\"\n", s.look.key()));
            out.push_str(&format!(
                "lobby_background = \"{}\"\n",
                s.lobby_background.key()
            ));
        }
        out.push_str(&format!("{key} = {}\n", s.get(i)));
    }
    out
}

fn io_err(path: &Path, e: impl std::fmt::Display) -> ConfigError {
    ConfigError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    }
}

/// Write through a temp file and a rename, so a crash keeps the old file.
fn write_atomic(path: &Path, text: &str) -> Result<(), ConfigError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).map_err(|e| io_err(&tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| io_err(path, e))
}

/// Load `settings.toml` from `dir`; a missing file is the defaults (and
/// is not created until a setting changes). A bad file is an error: no
/// silent default.
pub fn load_settings(dir: &Path) -> Result<Settings, ConfigError> {
    let path = settings_path(dir);
    match std::fs::read_to_string(&path) {
        Ok(text) => parse_settings(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(io_err(&path, e)),
    }
}

pub fn save_settings(dir: &Path, s: &Settings) -> Result<(), ConfigError> {
    // A settings write is a `settings` event in the perf report.
    let _perf = d2_server::perf::span("settings");
    write_atomic(&settings_path(dir), &write_settings(s))
}

/// Load `controls.toml` from `dir`; a missing file is created from the
/// `original` preset and
/// returned. The `dev` preset is used only when a file names it.
pub fn load_controls(dir: &Path) -> anyhow::Result<Bindings> {
    let path = controls_path(dir);
    if !path.exists() {
        let preset = Preset::Original;
        let bindings = preset.bindings().expect("original preset");
        let file = ControlsFile::from_effective(preset, &bindings).expect("original preset");
        write_atomic(&path, &controls::write(&file))?;
        return Ok(bindings);
    }
    Ok(controls::load(&path)?.1)
}

/// The play app's config: the folder and the settings in force.
#[derive(bevy::prelude::Resource, Clone, Debug)]
pub struct ConfigRes {
    pub dir: PathBuf,
    pub settings: Settings,
    pub bindings: Bindings,
    /// The sound device is up: false under `--no-sound`, which disables the
    /// sound rows of the Options pages.
    pub sound_device: bool,
}

impl WindowMode {
    /// The Bevy window mode on the window's monitor `on`: its entity and,
    /// for Fullscreen, its own video mode ([`native_video_mode`]).
    /// Fullscreen needs both; until they're known it shows as Windowed
    /// Borderless, which covers the same pixels.
    pub fn bevy(
        self,
        on: Option<(bevy::prelude::Entity, Option<bevy::window::VideoMode>)>,
    ) -> bevy::window::WindowMode {
        use bevy::window::{MonitorSelection, VideoModeSelection};
        let monitor = on.map_or(MonitorSelection::Current, |(e, _)| {
            MonitorSelection::Entity(e)
        });
        match (self, on) {
            (WindowMode::Windowed, _) => bevy::window::WindowMode::Windowed,
            (WindowMode::Fullscreen, Some((_, Some(video)))) => {
                bevy::window::WindowMode::Fullscreen(monitor, VideoModeSelection::Specific(video))
            }
            (WindowMode::WindowedBorderless | WindowMode::Fullscreen, _) => {
                bevy::window::WindowMode::BorderlessFullscreen(monitor)
            }
        }
    }
}

/// The monitor's own video mode, the one exclusive fullscreen takes so
/// the display mode doesn't change: its resolution at its current refresh
/// rate (else the nearest listed rate), at the deepest colour.
pub fn native_video_mode(m: &bevy::window::Monitor) -> Option<bevy::window::VideoMode> {
    let size = bevy::math::UVec2::new(m.physical_width, m.physical_height);
    let refresh = m.refresh_rate_millihertz.unwrap_or(0);
    m.video_modes
        .iter()
        .filter(|v| v.physical_size == size)
        .min_by_key(|v| {
            (
                v.refresh_rate_millihertz.abs_diff(refresh),
                std::cmp::Reverse(v.bit_depth),
            )
        })
        .copied()
}

/// The primary window the settings ask for, before the app starts. The
/// size given here is in logical pixels, since the display scaling isn't
/// known until the window exists; [`apply_window_choice`] sets the exact
/// physical size (or maximizes the window) on the first frame. Fullscreen
/// opens as Windowed Borderless and turns exclusive on the first frame,
/// once the monitor and its video mode are known. VSync and the frame
/// latency come from [`super::vsync`].
pub fn window_for(s: &Settings) -> bevy::window::Window {
    let (w, h) = match s.window_size {
        WindowSize::Fixed(w, h) => (w, h),
        WindowSize::Maximized => (super::front_host::WIDTH, super::front_host::HEIGHT),
    };
    bevy::window::Window {
        title: crate::identity::current().window_title.clone(),
        resolution: bevy::window::WindowResolution::new(w, h),
        mode: s.window_mode.bevy(None),
        present_mode: super::vsync::present_mode(s.vsync),
        desired_maximum_frame_latency: Some(super::vsync::FRAME_LATENCY),
        ..Default::default()
    }
}

/// The window the player chose (mode, Windowed size and the mode Alt+Enter
/// returns to), and whether it still has to be applied to the window; with
/// it, what fills the window beside the front end's picture (read by the
/// front end each frame, nothing to apply).
#[derive(bevy::prelude::Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowChoice {
    pub mode: WindowMode,
    pub size: WindowSize,
    pub sides: MenuSides,
    pub alt_enter_return: WindowMode,
    /// Set when the choice changed (or the display scaling did) and the
    /// window doesn't show it yet.
    pub pending: bool,
    /// The window was last asked to be maximized: a fixed size first
    /// restores it, then sizes it on the next frame.
    pub maximized: bool,
}

impl WindowChoice {
    /// The choice of `s`, to be applied.
    pub fn of(s: &Settings) -> Self {
        WindowChoice {
            mode: s.window_mode,
            size: s.window_size,
            sides: s.menu_sides,
            alt_enter_return: s.alt_enter_return,
            pending: true,
            maximized: false,
        }
    }

    /// Takes the window part of `s`; marks it pending when the mode or the
    /// size differs. The menu sides and the Alt+Enter return mode are taken
    /// as they are.
    pub fn update(&mut self, s: &Settings) {
        self.alt_enter_return = s.alt_enter_return;
        if (self.mode, self.size) != (s.window_mode, s.window_size) {
            *self = WindowChoice {
                maximized: self.maximized,
                ..WindowChoice::of(s)
            };
        }
        self.sides = s.menu_sides;
    }

    /// The size a Windowed window gets on a monitor of `monitor` physical
    /// pixels: the chosen size when it fits there, else the largest listed
    /// size that does ([`WindowSize::or_fitting`]). `None` for Windowed
    /// Borderless and Fullscreen, which take the monitor's own size.
    pub fn windowed_size(&self, monitor: Option<(u32, u32)>) -> Option<WindowSize> {
        (self.mode == WindowMode::Windowed).then(|| self.size.or_fitting(monitor))
    }
}

/// The folder `settings.toml` is in, for Alt+Enter outside a game (the
/// main menu), where no [`ConfigRes`] holds the settings.
#[derive(bevy::prelude::Resource, Clone, Debug)]
pub struct SettingsDir(pub PathBuf);

/// Adds the window choice of `s` and the systems that apply it and that
/// switch it with Alt+Enter ([`alt_enter`]), and sets the scale mode the
/// frame presents with and the frame pacing (VSync and the frame limit,
/// [`super::vsync`]). `dir` is the folder of `settings.toml`.
pub fn add_window_choice(app: &mut bevy::prelude::App, s: &Settings, dir: PathBuf) {
    use bevy::prelude::IntoScheduleConfigs;
    crate::world_view::sharp::set_picture_options(s);
    crate::ui::set_scale_mode(s.scale_mode);
    crate::world_view::native_ui::set_enabled(s.native_ui);
    crate::world_view::hw_cursor::set_size(s.cursor_size);
    crate::ui::ui_space::set_ui_scale(s.ui_scale);
    crate::world_view::ui_art::hd_text::set_hd_fonts(s.hd_text);
    app.insert_resource(WindowChoice::of(s))
        .insert_resource(SettingsDir(dir))
        .add_systems(
            bevy::prelude::PreUpdate,
            alt_enter.after(bevy::input::InputSystems),
        )
        .add_systems(bevy::prelude::Update, apply_window_choice);
    super::vsync::add(app, s);
}

/// Every monitor the platform reported, and whether it is the primary one.
pub type Monitors<'w, 's> = bevy::prelude::Query<
    'w,
    's,
    (
        &'static bevy::window::Monitor,
        bevy::prelude::Has<bevy::window::PrimaryMonitor>,
    ),
>;

/// The primary monitor's entity.
pub type PrimaryMonitorId<'w, 's> = bevy::prelude::Query<
    'w,
    's,
    bevy::prelude::Entity,
    bevy::prelude::With<bevy::window::PrimaryMonitor>,
>;

/// The primary window, with the monitor it's on when the platform knows.
pub type PrimaryWindowOn<'w, 's, W> = bevy::prelude::Query<
    'w,
    's,
    (W, Option<&'static bevy::window::OnMonitor>),
    bevy::prelude::With<bevy::window::PrimaryWindow>,
>;

/// The monitor a window is on: the one the platform reports, else the
/// primary monitor.
pub fn monitor_of<'a>(
    on: Option<&bevy::window::OnMonitor>,
    monitors: &'a Monitors,
) -> Option<&'a bevy::window::Monitor> {
    on.and_then(|on| monitors.get(on.0).ok())
        .or_else(|| monitors.iter().find(|(_, primary)| *primary))
        .map(|(m, _)| m)
}

/// The entity of the monitor [`monitor_of`] picks.
fn monitor_entity(
    on: Option<&bevy::window::OnMonitor>,
    monitors: &Monitors,
    primary: &PrimaryMonitorId,
) -> Option<bevy::prelude::Entity> {
    on.map(|on| on.0)
        .filter(|&e| monitors.get(e).is_ok())
        .or_else(|| primary.iter().next())
}

/// A monitor's physical size, when it reported one.
pub fn monitor_size(m: Option<&bevy::window::Monitor>) -> Option<(u32, u32)> {
    m.map(|m| (m.physical_width, m.physical_height))
        .filter(|&(w, h)| w > 0 && h > 0)
}

/// What the settings menu shows about `window` now: its mode, its size in
/// physical pixels, and the game frame in force.
pub fn window_facts(window: &bevy::window::Window) -> crate::ui::settings::table::WindowFacts {
    let mode = match window.mode {
        bevy::window::WindowMode::Windowed => WindowMode::Windowed,
        bevy::window::WindowMode::BorderlessFullscreen(_) => WindowMode::WindowedBorderless,
        bevy::window::WindowMode::Fullscreen(..) => WindowMode::Fullscreen,
    };
    let size = (window.physical_width(), window.physical_height());
    let frame = FrameSize::play();
    crate::ui::settings::table::WindowFacts {
        mode: Some(mode),
        size: (size.0 > 0 && size.1 > 0).then_some(size),
        frame: Some((frame.width as u32, frame.height as u32)),
    }
}

/// Applies a pending [`WindowChoice`] to the primary window: the mode on
/// the window's monitor and, for a Windowed window, maximized or its fixed
/// size in physical pixels, centred on its monitor. A change of display
/// scaling (moving to another monitor, or a new Windows setting) re-applies
/// it, so the window keeps its pixel size. Between changes the player may
/// resize the window freely.
///
/// The frame's presentation (sharp Fit or Integer, the frame's place and
/// the mouse mapping) is worked out from the window's size every frame, so
/// it follows every switch; Bevy reconfigures the render surface on the
/// resize.
pub fn apply_window_choice(
    choice: Option<bevy::prelude::ResMut<WindowChoice>>,
    monitors: Monitors,
    primary: PrimaryMonitorId,
    mut windows: PrimaryWindowOn<&mut bevy::window::Window>,
    mut rescaled: bevy::prelude::MessageReader<bevy::window::WindowScaleFactorChanged>,
) {
    let Some(mut choice) = choice else {
        return;
    };
    if rescaled.read().count() > 0 {
        choice.pending = true;
    }
    if !choice.pending {
        return;
    }
    let Ok((mut w, on)) = windows.single_mut() else {
        return;
    };
    let monitor = monitor_of(on, &monitors);
    let entity = monitor_entity(on, &monitors, &primary);
    let mode = choice
        .mode
        .bevy(entity.map(|e| (e, monitor.and_then(native_video_mode))));
    if w.mode != mode {
        w.mode = mode;
    }
    if choice.mode == WindowMode::Fullscreen && entity.is_none() {
        // Fullscreen before the monitor is known: try again next frame.
        return;
    }
    match choice.windowed_size(monitor_size(monitor)) {
        None => {}
        Some(WindowSize::Maximized) => {
            w.set_maximized(true);
            choice.maximized = true;
        }
        Some(WindowSize::Fixed(width, height)) => {
            if choice.maximized {
                // Restore first; the size is set on the next frame.
                w.set_maximized(false);
                choice.maximized = false;
                return;
            }
            if w.physical_size() != bevy::math::UVec2::new(width, height) {
                w.resolution.set_physical_resolution(width, height);
                w.position =
                    bevy::window::WindowPosition::Centered(bevy::window::MonitorSelection::Current);
            }
        }
    }
    choice.pending = false;
}

/// Whether `keys` hold Alt with Enter (or keypad Enter) just pressed.
pub fn alt_enter_pressed(keys: &bevy::input::ButtonInput<bevy::input::keyboard::KeyCode>) -> bool {
    use bevy::input::keyboard::KeyCode;
    keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight])
        && keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter])
}

/// The settings Alt+Enter switches: the game's ([`ConfigRes`]) in game,
/// else what `settings.toml` holds (the main menu). `None` when the file
/// can't be read: the switch then isn't stored.
fn settings_to_switch(
    cfg: Option<&ConfigRes>,
    dir: Option<&SettingsDir>,
) -> Option<(Settings, PathBuf)> {
    if let Some(c) = cfg {
        return Some((c.settings, c.dir.clone()));
    }
    let dir = &dir?.0;
    match load_settings(dir) {
        Ok(s) => Some((s, dir.clone())),
        Err(e) => {
            bevy::log::error!("Alt+Enter: the switch isn't stored: {e}");
            None
        }
    }
}

/// Alt+Enter, in game and on the main menu: switches the window between
/// the mode in use and Windowed Borderless ([`Settings::toggle_borderless`])
/// and stores the new mode in `settings.toml`. In game the settings in
/// force and the Esc menu's Options page take it too. The Enter of the
/// shortcut reaches neither the game nor the menus: it is held back until
/// it's released.
pub fn alt_enter(
    keys: Option<bevy::prelude::ResMut<bevy::input::ButtonInput<bevy::input::keyboard::KeyCode>>>,
    mut events: bevy::prelude::MessageReader<bevy::input::keyboard::KeyboardInput>,
    choice: Option<bevy::prelude::ResMut<WindowChoice>>,
    dir: Option<bevy::prelude::Res<SettingsDir>>,
    mut cfg: Option<bevy::prelude::ResMut<ConfigRes>>,
    ui: Option<bevy::prelude::NonSendMut<crate::world_view::WorldViewUi>>,
    mut held: bevy::prelude::Local<bool>,
) {
    use bevy::input::keyboard::KeyCode;
    const ENTER: [KeyCode; 2] = [KeyCode::Enter, KeyCode::NumpadEnter];
    let Some(mut keys) = keys else {
        return;
    };
    let released = events
        .read()
        .any(|e| ENTER.contains(&e.key_code) && e.state == bevy::input::ButtonState::Released);
    let pressed = alt_enter_pressed(&keys);
    if *held || pressed {
        for k in ENTER {
            keys.reset(k);
        }
        *held = pressed || !released;
    }
    let (true, Some(mut choice)) = (pressed, choice) else {
        return;
    };
    match settings_to_switch(cfg.as_deref(), dir.as_deref()) {
        Some((mut s, dir)) => {
            s.toggle_borderless();
            if let Err(e) = save_settings(&dir, &s) {
                bevy::log::error!("settings not saved: {e}");
            }
            if let Some(c) = cfg.as_deref_mut() {
                c.settings = s;
            }
            if let Some(o) = ui.and_then(|ui| ui.into_inner().original.as_mut()) {
                o.set_settings(s);
            }
            choice.update(&s);
        }
        None => {
            // Switch the window only.
            let mut s = Settings {
                window_mode: choice.mode,
                window_size: choice.size,
                alt_enter_return: choice.alt_enter_return,
                ..Settings::default()
            };
            s.toggle_borderless();
            choice.update(&s);
        }
    }
}

/// Hands the settings to the Esc menu once, then writes `settings.toml`
/// and applies them on each change it hands over (the settings menu's, a
/// control panel toggle).
#[allow(clippy::too_many_arguments)]
pub fn apply_settings(
    ui: Option<bevy::prelude::NonSendMut<crate::world_view::WorldViewUi>>,
    cfg: Option<bevy::prelude::ResMut<ConfigRes>>,
    choice: Option<bevy::prelude::ResMut<WindowChoice>>,
    sync: Option<bevy::prelude::ResMut<super::vsync::SyncChoice>>,
    animation: Option<bevy::prelude::Res<crate::app::ui::AnimationFrames>>,
    mut state: Option<bevy::prelude::ResMut<crate::world_view::WorldViewState>>,
    mut bridge: Option<bevy::prelude::ResMut<crate::bridge::BridgeResource>>,
    mut init: bevy::prelude::Local<bool>,
) {
    let (Some(mut ui), Some(mut cfg)) = (ui, cfg) else {
        return;
    };
    if ui.original.is_none() {
        return;
    }
    if bevy::prelude::DetectChanges::is_added(&ui) {
        // A new game's UI (`app::session`): it gets the settings again.
        *init = false;
    }
    if !*init {
        if let Some(state) = state.as_deref_mut() {
            apply_game_options(state, &cfg.settings);
        }
        if let Some(bridge) = bridge.as_deref_mut() {
            bridge
                .0
                .set_high_light_quality(cfg.settings.light_quality == 2);
        }
        if let Some(animation) = &animation {
            animation.apply(cfg.settings.animation_frames);
        }
        ui.bindings = Some(cfg.bindings.clone());
        if let Some(o) = ui.original.as_mut() {
            o.set_settings(cfg.settings);
        }
        *init = true;
    }
    let Some(original) = ui.original.as_mut() else {
        return;
    };
    let Some(new) = original.take_settings_change() else {
        return;
    };
    if let Some(state) = state.as_deref_mut() {
        apply_game_options(state, &new);
    }
    if let Some(bridge) = bridge.as_deref_mut() {
        bridge.0.set_high_light_quality(new.light_quality == 2);
    }
    if let Some(animation) = &animation {
        animation.apply(new.animation_frames);
    }
    commit_settings(&mut cfg, choice, sync, new);
}

/// Apply on entry and menu edits, leaving hotkeys free between edits.
fn apply_game_options(state: &mut crate::world_view::WorldViewState, s: &Settings) {
    if let Some(session) = state.automap.as_mut() {
        let map = &mut session.map;
        let was_mini = map.view.mini;
        apply_automap_options(map, s);
        if was_mini != map.view.mini {
            if let Some(view) = state.automap_view.as_ref() {
                map.limits = view.limits(map.view.mini);
            }
        }
    }
}

fn apply_automap_options(map: &mut crate::ui::automap::Automap, s: &Settings) {
    map.options.fade = u32::from(s.automap_fade);
    map.options.fade_latch = u32::from(s.automap_fade);
    map.options.centers = s.automap_centers != 0;
    map.options.party = s.automap_party != 0;
    map.options.party_names = s.automap_party_names != 0;
    let mini = s.automap_mode != 0;
    if map.view.mini != mini {
        map.view.mini = mini;
        map.view.div = if mini {
            crate::ui::automap::view::DIV_MINI
        } else {
            crate::ui::automap::view::DIV_FULL
        };
        // The next map frame re-centres with its current panel geometry.
    }
}

/// Makes `new` the settings in force, wherever they were changed (the
/// game's Esc menu or the main menu's Options screen): saved to
/// `settings.toml`, and every part that can follow at once does, with no
/// Apply step and no restart:
/// - the window mode and plain-window size ([`WindowChoice`], applied by
///   [`apply_window_choice`]) and the menu sides beside the 4:3 menus;
/// - the scale mode the pictures are presented with;
/// - VSync and the frame limit ([`super::vsync::SyncChoice`]).
///
/// Sound and Music Volume are read from [`ConfigRes`] by the game's sound
/// each frame, and the front end's music takes them in [`front_settings`].
/// Picture adjustments and shadows follow the renderer immediately; lighting
/// and automap choices reach the running game through `apply_settings`.
pub fn commit_settings(
    cfg: &mut ConfigRes,
    choice: Option<bevy::prelude::ResMut<WindowChoice>>,
    sync: Option<bevy::prelude::ResMut<super::vsync::SyncChoice>>,
    new: Settings,
) {
    crate::world_view::sharp::set_picture_options(&new);
    crate::ui::set_scale_mode(new.scale_mode);
    crate::world_view::native_ui::set_enabled(new.native_ui);
    crate::world_view::hw_cursor::set_size(new.cursor_size);
    crate::ui::ui_space::set_ui_scale(new.ui_scale);
    crate::world_view::ui_art::hd_text::set_hd_fonts(new.hd_text);
    cfg.settings = new;
    if let Err(e) = save_settings(&cfg.dir, &new) {
        bevy::log::error!("settings not saved: {e}");
    }
    if let Some(mut choice) = choice {
        if choice.mode != new.window_mode
            || choice.size != new.window_size
            || choice.sides != new.menu_sides
            || choice.alt_enter_return != new.alt_enter_return
        {
            choice.update(&new);
        }
    }
    if let Some(mut sync) = sync {
        let new = super::vsync::SyncChoice::of(&new);
        if *sync != new {
            *sync = new;
        }
    }
}

/// The settings menu over the main menu edits the same settings as in
/// game. When a front end starts (the first screen, or back from a game)
/// it gets the settings in force, and each change the menu hands it goes
/// through [`commit_settings`], so it is saved and applied at once. The
/// menu music follows Sound and Music Volume as they move.
pub fn front_settings(
    host: Option<bevy::prelude::NonSendMut<crate::app::front_host::FrontHost>>,
    cfg: Option<bevy::prelude::ResMut<ConfigRes>>,
    choice: Option<bevy::prelude::ResMut<WindowChoice>>,
    sync: Option<bevy::prelude::ResMut<super::vsync::SyncChoice>>,
) {
    let (Some(mut host), Some(mut cfg)) = (host, cfg) else {
        return;
    };
    if bevy::prelude::DetectChanges::is_added(&host) {
        host.front.set_settings(cfg.settings);
        host.set_sound_levels(&cfg.settings);
    }
    let Some(new) = host.front.take_settings_change() else {
        return;
    };
    host.set_sound_levels(&new);
    commit_settings(&mut cfg, choice, sync, new);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh install starts with the default look F: Look Vivid and
    /// Sharpen 25, everything else as it was (Sharp Bilinear, no FXAA, no
    /// bloom, colour unchanged). A settings.toml without those keys gets
    /// them too; one that has them keeps its own values.
    #[test]
    fn the_default_look_is_vivid_with_sharpen_25() {
        let d = Settings::default();
        assert_eq!((d.look, d.sharpen), (Look::Vivid, 25));
        assert_eq!(
            (d.scaling_filter, d.fxaa, d.bloom, d.bloom_strength),
            (ScalingFilter::SharpBilinear, FxaaLevel::Off, false, 50)
        );
        assert_eq!((d.brightness, d.saturation, d.vibrance), (50, 100, 0));
        let without = format!("version = {SETTINGS_VERSION}\n[video]\ngamma = 155\n");
        let s = parse_settings(&without).unwrap();
        assert_eq!((s.look, s.sharpen), (Look::Vivid, 25));
        let own =
            format!("version = {SETTINGS_VERSION}\n[video]\nlook = \"original\"\nsharpen = 0\n");
        let s = parse_settings(&own).unwrap();
        assert_eq!((s.look, s.sharpen), (Look::Original, 0));
        // A saved file names both, so a later change of default leaves it be.
        let text = write_settings(&s);
        assert!(text.contains("look = \"original\"\n") && text.contains("sharpen = 0\n"));
        assert_eq!(parse_settings(&text).unwrap(), s);
    }

    #[test]
    fn post_options_are_saved_read_and_default_to_unchanged() {
        let d = Settings::default();
        for (key, lo, hi, default) in POST_KEYS {
            assert_eq!(d.post_value(key), default, "{key}");
            assert!((lo..=hi).contains(&default));
            for v in [lo, default, hi] {
                let mut s = Settings::default();
                s.set_post_value(key, v);
                let text = write_settings(&s);
                assert!(text.contains(&format!("{key} = {v}\n")));
                assert_eq!(parse_settings(&text).unwrap(), s, "{key} {v}");
            }
            let file = |v: &str| format!("version = {SETTINGS_VERSION}\n[video]\n{key} = {v}\n");
            assert!(parse_settings(&file(&(i64::from(hi) + 1).to_string())).is_err());
            assert!(parse_settings(&file("\"5\"")).is_err());
        }
        assert_eq!(d.fxaa, FxaaLevel::Off);
        for f in FxaaLevel::ALL {
            let s = Settings {
                fxaa: f,
                ..Settings::default()
            };
            assert_eq!(parse_settings(&write_settings(&s)).unwrap(), s);
        }
        let file = format!("version = {SETTINGS_VERSION}\n[video]\nfxaa = \"ultra\"\n");
        assert_eq!(parse_settings(&file).unwrap().fxaa, FxaaLevel::Off);
        assert_eq!(FXAA_NAMES.len(), FxaaLevel::ALL.len());
        // Bloom is off; the look is Vivid until another is chosen.
        assert!(!d.bloom && d.look == Look::Vivid);
        for look in Look::ALL {
            for bloom in [false, true] {
                let s = Settings {
                    look,
                    bloom,
                    ..Settings::default()
                };
                assert_eq!(parse_settings(&write_settings(&s)).unwrap(), s);
            }
        }
        let file = |k: &str, v: &str| format!("version = {SETTINGS_VERSION}\n[video]\n{k} = {v}\n");
        assert_eq!(
            parse_settings(&file("look", "\"sepia\"")).unwrap().look,
            Look::Vivid,
            "an unknown look is the default one"
        );
        assert!(parse_settings(&file("bloom", "1")).is_err());
        assert_eq!(LOOK_NAMES.len(), Look::ALL.len());
    }

    /// The lobby's background is saved and read back; the Worldstone
    /// Chamber until another is picked, and for a name no background has.
    #[test]
    fn the_lobby_background_is_saved_and_read_back() {
        use crate::ui::front_end::screens::online::backdrop::Backdrop;
        assert_eq!(
            Settings::default().lobby_background,
            Backdrop::WorldstoneChamber
        );
        for b in Backdrop::ALL {
            let s = Settings {
                lobby_background: b,
                ..Settings::default()
            };
            let text = write_settings(&s);
            assert!(text.contains(&format!("lobby_background = \"{}\"\n", b.key())));
            assert_eq!(parse_settings(&text).unwrap(), s);
        }
        let file =
            |v: &str| format!("version = {SETTINGS_VERSION}\n[video]\nlobby_background = {v}\n");
        assert_eq!(
            parse_settings(&file("\"kurast-docks\""))
                .unwrap()
                .lobby_background,
            Backdrop::WorldstoneChamber
        );
        assert!(parse_settings(&file("3")).is_err());
    }

    #[test]
    fn scaling_filter_is_saved_read_and_defaults_to_sharp_bilinear() {
        assert_eq!(
            Settings::default().scaling_filter,
            ScalingFilter::SharpBilinear
        );
        for (i, f) in ScalingFilter::ALL.into_iter().enumerate() {
            assert_eq!(usize::from(f.index()), i);
            assert_eq!(ScalingFilter::from_index(f.index()), f);
            let s = Settings {
                scaling_filter: f,
                ..Settings::default()
            };
            let text = write_settings(&s);
            assert!(text.contains(&format!("scaling_filter = \"{}\"\n", f.key())));
            assert_eq!(parse_settings(&text).unwrap(), s);
        }
        assert_eq!(ScalingFilter::from_index(9), ScalingFilter::SharpBilinear);
        let file =
            |v: &str| format!("version = {SETTINGS_VERSION}\n[video]\nscaling_filter = {v}\n");
        assert_eq!(
            parse_settings(&file("\"xbr\"")).unwrap().scaling_filter,
            ScalingFilter::SharpBilinear
        );
        assert_eq!(
            parse_settings(&file("\"smooth_edges\""))
                .unwrap()
                .scaling_filter,
            ScalingFilter::SmoothEdges
        );
        assert!(parse_settings(&file("2")).is_err());
        assert_eq!(SCALING_FILTER_NAMES.len(), ScalingFilter::ALL.len());
    }

    #[test]
    fn ui_scale_is_saved_read_range_checked_and_defaults_to_100() {
        use crate::ui::ui_space::{UI_SCALE_MAX, UI_SCALE_MIN};
        assert_eq!(Settings::default().ui_scale, 100);
        for v in [UI_SCALE_MIN, 90, 110, 115, UI_SCALE_MAX] {
            let s = Settings {
                ui_scale: v,
                ..Settings::default()
            };
            let text = write_settings(&s);
            assert!(text.contains(&format!("ui_scale = {v}\n")));
            assert_eq!(parse_settings(&text).unwrap(), s);
        }
        let file = |v: &str| format!("version = {SETTINGS_VERSION}\n[video]\nui_scale = {v}\n");
        assert!(parse_settings(&file("74")).is_err());
        assert!(parse_settings(&file("151")).is_err());
        // A file from when the slider went to 150 %: past 120 % reads as
        // 120 %, the largest UI that keeps the panels whole.
        for v in ["121", "133", "150"] {
            assert_eq!(parse_settings(&file(v)).unwrap().ui_scale, UI_SCALE_MAX);
        }
        assert!(parse_settings(&file("\"100\"")).is_err());
    }

    #[test]
    fn cursor_size_is_saved_read_and_defaults_to_auto() {
        assert_eq!(Settings::default().cursor_size, CursorSize::Auto);
        for size in CursorSize::ALL {
            let s = Settings {
                cursor_size: size,
                ..Settings::default()
            };
            let text = write_settings(&s);
            assert!(text.contains(&format!("cursor_size = \"{}\"\n", size.key())));
            assert_eq!(parse_settings(&text).unwrap(), s);
        }
        let file = |v: &str| format!("version = {SETTINGS_VERSION}\n[video]\ncursor_size = {v}\n");
        // A size this build doesn't list is Auto; a number is an error.
        assert_eq!(
            parse_settings(&file("\"9x\"")).unwrap().cursor_size,
            CursorSize::Auto
        );
        assert!(parse_settings(&file("3")).is_err());
        // An older file without the key keeps Auto.
        assert_eq!(
            parse_settings(&format!("version = {SETTINGS_VERSION}\n"))
                .unwrap()
                .cursor_size,
            CursorSize::Auto
        );
        assert_eq!(CURSOR_SIZE_NAMES.len(), CursorSize::ALL.len());
    }

    /// Owner bug B1: the play frame is the one player view, whatever the
    /// window; it takes no window or setting at all, so a big or maximized
    /// window never swaps in a smaller canvas of its own.
    #[test]
    fn the_play_frame_never_follows_the_window() {
        assert_eq!(set_play_frame(), Ok(FrameSize::PLAY));
        assert_eq!(FrameSize::play(), FrameSize::PLAY);
        assert_eq!(set_play_frame(), Ok(FrameSize::PLAY));
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("d2rust-config-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn saved_automap_options_reach_the_running_map() {
        use crate::ui::automap::{options::MemoryStore, Automap, SEED};
        let mut map = Automap::new(&MemoryStore::default(), SEED);
        let mut settings = Settings::default();
        for mini in [1, 0] {
            for fade in 0..4 {
                for on in [0, 1] {
                    settings.automap_mode = mini;
                    settings.automap_fade = fade;
                    settings.automap_centers = on;
                    settings.automap_party = on;
                    settings.automap_party_names = on;
                    let saved = parse_settings(&write_settings(&settings)).unwrap();
                    apply_automap_options(&mut map, &saved);
                    let facts = crate::ui::automap::FrameFacts {
                        width: 800,
                        height: 600,
                        open_mode: 0,
                        mini_down: false,
                        unit_origin: crate::rules::camera::ClientPos { x: 0, y: 0 },
                    };
                    map.view.frame(&map.options, &facts);
                    if on != 0 {
                        let mut expected = map.view;
                        expected.recentre(false, &map.options, &facts);
                        assert_eq!(map.view.offset, expected.offset);
                    }
                    assert_eq!(map.view.mini, mini != 0);
                    assert_eq!(map.view.div, if mini != 0 { 20 } else { 10 });
                    assert_eq!(
                        (map.options.fade, map.options.fade_latch),
                        (u32::from(fade), u32::from(fade))
                    );
                    assert_eq!(
                        (
                            map.options.centers,
                            map.options.party,
                            map.options.party_names
                        ),
                        (on != 0, on != 0, on != 0)
                    );
                }
            }
        }
    }

    #[test]
    fn missing_settings_are_the_defaults_and_roundtrip() {
        let d = tmp("rt");
        assert_eq!(load_settings(&d).unwrap(), Settings::default());
        let s = Settings {
            window_mode: WindowMode::Fullscreen,
            alt_enter_return: WindowMode::Fullscreen,
            music_volume: 45,
            gamma: 165,
            automap_fade: 3,
            ..Settings::default()
        };
        save_settings(&d, &s).unwrap();
        assert_eq!(load_settings(&d).unwrap(), s);
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// Every game runs the one 1280 x 720 view; a test tool's `--res`
    /// wins over it.
    #[test]
    fn every_game_runs_the_one_player_view_and_res_wins() {
        assert_eq!(play_frame(None), FrameSize::PLAY);
        for res in FrameSize::ALL {
            assert_eq!(play_frame(Some(res)), res);
        }
        // A written file has no view key.
        assert!(!write_settings(&Settings::default()).contains("resolution"));
    }

    /// An older file's view row (versions 1 to 8, any of its frames) is
    /// read and ignored: the file loads as if it had none, and the next
    /// write leaves it out. A version 9 file can't have it.
    #[test]
    fn older_files_drop_their_view_row() {
        for version in 1..9 {
            for r in 0..=2 {
                let text = format!("version = {version}\n[video]\nresolution = {r}\ngamma = 200\n");
                let s = parse_settings(&text).unwrap();
                let plain = parse_settings(&format!("version = {version}\n[video]\ngamma = 200\n"))
                    .unwrap();
                assert_eq!(s, plain, "{version} {r}");
                assert!(!write_settings(&s).contains("resolution"));
            }
        }
        assert!(parse_settings("version = 9\n[video]\nresolution = 2\n").is_err());
    }

    #[test]
    fn the_native_ui_is_on_unless_a_file_turns_it_off() {
        assert!(Settings::default().native_ui);
        // A version 3 file has no such key: on.
        let s = parse_settings("version = 3\n[video]\nvsync = true\n").unwrap();
        assert!(s.native_ui);
        let s = parse_settings("version = 4\n[video]\nnative_ui = false\n").unwrap();
        assert!(!s.native_ui);
        assert!(write_settings(&s).contains("\nnative_ui = false\n"));
        assert_eq!(parse_settings(&write_settings(&s)).unwrap(), s);
        assert!(parse_settings("version = 4\n[video]\nnative_ui = 1\n").is_err());
    }

    #[test]
    fn retired_text_switches_load_and_are_not_written_back() {
        // The text switches of versions 5 to 7 are read and dropped: HD Text
        // stays off and `text_mode` is never written.
        for keys in [
            "hd_text = true",
            "hd_text = false",
            "text_mode = \"hd\"",
            "text_mode = \"upscaled\"",
            "hd_text = true\ntext_mode = \"upscaled\"",
        ] {
            let text = format!("version = 6\n[video]\nvsync = false\n{keys}\n");
            let s = parse_settings(&text).unwrap();
            assert!(!s.vsync, "{keys}: the other keys still load");
            assert!(!s.hd_text, "{keys}: an old switch is not HD Text");
            let saved = write_settings(&s);
            assert!(saved.contains("\nhd_text = false\n") && !saved.contains("text_mode"));
            assert_eq!(parse_settings(&saved).unwrap(), s);
        }
        let plain = parse_settings("version = 6\n[video]\nvsync = false\n").unwrap();
        let legacy =
            parse_settings("version = 6\n[video]\nvsync = false\nhd_text = true\n").unwrap();
        assert_eq!(plain, legacy);
    }

    #[test]
    fn hd_text_is_off_by_default_and_roundtrips_from_version_8() {
        assert!(!Settings::default().hd_text);
        let on = parse_settings("version = 8\n[video]\nhd_text = true\n").unwrap();
        assert!(on.hd_text);
        let saved = write_settings(&on);
        assert!(saved.contains("\nhd_text = true\n"));
        assert_eq!(parse_settings(&saved).unwrap(), on);
        assert!(parse_settings("version = 8\n[video]\nhd_text = 1\n").is_err());
        // The same key in a version 7 file is the old switch: ignored.
        assert!(
            !parse_settings("version = 7\n[video]\nhd_text = true\n")
                .unwrap()
                .hd_text
        );
    }

    #[test]
    fn animation_frames_default_to_smooth_and_roundtrip() {
        assert_eq!(
            Settings::default().animation_frames,
            AnimationFrameMode::Smooth
        );
        // A version 6 file has no such key: Smooth.
        let s = parse_settings("version = 6\n[video]\nvsync = false\n").unwrap();
        assert_eq!(s.animation_frames, AnimationFrameMode::Smooth);
        for (text, mode) in [
            ("original", AnimationFrameMode::Original),
            ("smooth", AnimationFrameMode::Smooth),
        ] {
            let s = parse_settings(&format!(
                "version = 7\n[video]\nanimation_frames = \"{text}\"\n"
            ))
            .unwrap();
            assert_eq!(s.animation_frames, mode, "{text}");
            let written = write_settings(&s);
            assert!(written.contains(&format!("animation_frames = \"{text}\"")));
            assert_eq!(parse_settings(&written).unwrap(), s);
        }
        // An unknown choice falls back to the default, as other display rows do.
        let s = parse_settings("version = 7\n[video]\nanimation_frames = \"fast\"\n").unwrap();
        assert_eq!(s.animation_frames, AnimationFrameMode::Smooth);
        assert!(parse_settings("version = 7\n[video]\nanimation_frames = 1\n").is_err());
    }

    #[test]
    fn settings_are_strict() {
        assert!(parse_settings(&format!("version = {}\n", SETTINGS_VERSION + 1)).is_err());
        for bad in [
            "[video]\nvsync = true\n",
            "version = 0\n",
            "version = 1\nfoo = 1\n",
            "version = 9\n[video]\nresolution = 2\n",
            "version = 1\n[video]\nwindow_mode = \"big\"\n",
            "version = 1\n[video]\nbogus = 1\n",
            "version = 1\n[audio]\nmaster_volume = 101\n",
            "version = 1\n[video]\ngamma = 54\n",
            "version = 1\n[automap]\nfade = 4\n",
            "version = 2\n[video]\nwindow_size = 1080\n",
            "version = 2\n[video]\nscale_mode = 1\n",
            "version = 2\n[video]\nvsync = 1\n",
            "version = 2\n[video]\nframe_limit = \"60\"\n",
        ] {
            assert!(parse_settings(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_version_1_file_loads_with_the_display_keys_at_their_defaults() {
        let s = parse_settings(
            "version = 1\n[video]\nresolution = 0\nwindow_mode = \"borderless\"\ngamma = 200\n",
        )
        .unwrap();
        assert_eq!(s.window_mode, WindowMode::WindowedBorderless);
        assert_eq!(s.gamma, 200);
        let d = Settings::default();
        assert_eq!(
            (s.window_size, s.scale_mode, s.vsync, s.frame_limit),
            (d.window_size, d.scale_mode, d.vsync, d.frame_limit)
        );
        // Written back, it's the current version.
        assert!(write_settings(&s).starts_with(&format!("version = {SETTINGS_VERSION}\n")));
        assert_eq!(parse_settings(&write_settings(&s)).unwrap(), s);
    }

    /// `[controls]` has one switch per item-moving shortcut, in the
    /// shortcuts' order and under their keys; all on by default and in an
    /// older file; each one turns its own shortcut off; a file older than
    /// version 10 can't have the section.
    #[test]
    fn the_item_shortcut_switches_round_trip() {
        use crate::ui::item_moves::{Shortcut, ShortcutSet};
        for (i, s) in Shortcut::ALL.iter().enumerate() {
            assert_eq!(INT_KEYS[SHORTCUT_KEYS + i], ("controls", s.key(), 0, 1));
        }
        assert_eq!(INT_KEYS.len(), SHORTCUT_KEYS + Shortcut::ALL.len());
        assert_eq!(Settings::default().item_shortcuts, ShortcutSet::ALL);
        let old = parse_settings("version = 9\n[video]\ngamma = 155\n").unwrap();
        assert_eq!(old.item_shortcuts, ShortcutSet::ALL);
        for s in Shortcut::ALL {
            let text = format!("version = 10\n[controls]\n{} = 0\n", s.key());
            let got = parse_settings(&text).unwrap();
            assert_eq!(
                got.item_shortcuts,
                ShortcutSet::ALL.without(ShortcutSet(s.bit()))
            );
            let written = write_settings(&got);
            assert!(written.contains("\n[controls]\n"), "{written}");
            assert!(written.contains(&format!("\n{} = 0\n", s.key())));
            assert_eq!(parse_settings(&written).unwrap(), got);
        }
        assert!(parse_settings("version = 9\n[controls]\nmove_stash = 0\n").is_err());
        assert!(parse_settings("version = 10\n[controls]\nmove_stash = 2\n").is_err());
        assert!(parse_settings("version = 10\n[controls]\nbogus = 1\n").is_err());
    }

    /// Version 1 and 2 files keep their saved mode and size under the new
    /// names: `borderless` is Windowed Borderless, `native` is Maximized.
    #[test]
    fn old_window_names_load_as_the_new_modes() {
        for version in [1, 2] {
            for (old, mode) in [
                ("windowed", WindowMode::Windowed),
                ("borderless", WindowMode::WindowedBorderless),
                ("fullscreen", WindowMode::Fullscreen),
            ] {
                let s = parse_settings(&format!(
                    "version = {version}\n[video]\nwindow_mode = \"{old}\"\n"
                ))
                .unwrap();
                assert_eq!(s.window_mode, mode, "{version} {old}");
                // Alt+Enter goes back to the saved mode, or to Windowed.
                let back = if mode == WindowMode::Fullscreen {
                    WindowMode::Fullscreen
                } else {
                    WindowMode::Windowed
                };
                assert_eq!(s.alt_enter_return, back, "{version} {old}");
                let written = write_settings(&s);
                assert!(written.contains(&format!("window_mode = \"{}\"", mode.key())));
                assert_eq!(parse_settings(&written).unwrap(), s);
            }
        }
        let s = parse_settings("version = 2\n[video]\nwindow_size = \"native\"\n").unwrap();
        assert_eq!(s.window_size, WindowSize::Maximized);
        let s = parse_settings("version = 2\n[video]\nwindow_size = \"1920x1080\"\n").unwrap();
        assert_eq!(s.window_size, WindowSize::Fixed(1920, 1080));
        // The old names are only read from old files.
        assert!(parse_settings("version = 3\n[video]\nwindow_mode = \"borderless\"\n").is_err());
        let s = parse_settings("version = 3\n[video]\nwindow_size = \"native\"\n").unwrap();
        assert_eq!(s.window_size, Settings::default().window_size);
    }

    /// A new settings file: a maximized Windowed window (title bar showing)
    /// at the monitor's own resolution, sharp Fit, VSync on, no limit.
    #[test]
    fn the_display_defaults_are_a_maximized_window_fit_vsync_on_and_no_limit() {
        let d = Settings::default();
        assert_eq!(d.window_mode, WindowMode::Windowed);
        assert_eq!(d.window_size, WindowSize::Maximized);
        assert_eq!(d.alt_enter_return, WindowMode::Windowed);
        assert_eq!(d.scale_mode, ScaleMode::Fit);
        assert!(d.vsync);
        assert_eq!(d.frame_limit, 0);
        let d = tmp("new");
        assert_eq!(load_settings(&d).unwrap(), Settings::default());
        // The window opens Windowed; the first frame maximizes it.
        let w = window_for(&Settings::default());
        assert_eq!(w.mode, bevy::window::WindowMode::Windowed);
        assert_eq!(w.title, "D2Gamma");
        let c = WindowChoice::of(&Settings::default());
        assert_eq!(
            c.windowed_size(Some((2560, 1440))),
            Some(WindowSize::Maximized)
        );
    }

    /// Alt+Enter: Windowed and Fullscreen go to Windowed Borderless and back;
    /// Windowed Borderless picked on the Video page goes to the mode last in
    /// use. Every switch is stored.
    #[test]
    fn alt_enter_switches_the_mode_in_use_with_windowed_borderless() {
        use WindowMode::*;
        let mut s = Settings::default();
        let mut seen = vec![s.window_mode];
        for _ in 0..4 {
            s.toggle_borderless();
            seen.push(s.window_mode);
            assert_eq!(parse_settings(&write_settings(&s)).unwrap(), s);
        }
        assert_eq!(
            seen,
            [
                Windowed,
                WindowedBorderless,
                Windowed,
                WindowedBorderless,
                Windowed
            ]
        );
        s.set_window_mode(Fullscreen);
        s.toggle_borderless();
        assert_eq!(s.window_mode, WindowedBorderless);
        // Saved and loaded in Borderless, it still returns to Fullscreen.
        s = parse_settings(&write_settings(&s)).unwrap();
        assert_eq!(s.alt_enter_return, Fullscreen);
        s.toggle_borderless();
        assert_eq!(s.window_mode, Fullscreen);
        // Borderless picked on the page: back to the mode before it.
        s.set_window_mode(WindowedBorderless);
        s.toggle_borderless();
        assert_eq!(s.window_mode, Fullscreen);
        s.set_window_mode(Windowed);
        s.set_window_mode(WindowedBorderless);
        s.toggle_borderless();
        assert_eq!(s.window_mode, Windowed);
        // The return mode is never Borderless.
        assert!(parse_settings(
            "version = 3\n[video]\nalt_enter_return = \"windowed_borderless\"\n"
        )
        .is_err());
    }

    /// The Alt+Enter system switches the window and stores the mode, and
    /// its Enter doesn't reach the game or the menus.
    #[test]
    fn the_alt_enter_key_switches_the_window_and_is_stored() {
        use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
        use bevy::input::{ButtonInput, ButtonState};
        use bevy::prelude::*;
        let d = tmp("alt-enter");
        let mut app = App::new();
        app.add_message::<bevy::window::WindowScaleFactorChanged>()
            .add_message::<KeyboardInput>()
            .init_resource::<ButtonInput<KeyCode>>();
        add_window_choice(&mut app, &Settings::default(), d.clone());
        let press = |app: &mut App, alt: bool| {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.clear();
            if alt {
                keys.press(KeyCode::AltLeft);
            }
            keys.press(KeyCode::Enter);
            app.update();
            let enter = app
                .world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::Enter);
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release_all();
            keys.clear();
            app.world_mut().write_message(KeyboardInput {
                key_code: KeyCode::Enter,
                logical_key: Key::Enter,
                state: ButtonState::Released,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
            app.update();
            enter
        };
        // Enter alone is the game's.
        assert!(press(&mut app, false));
        assert_eq!(
            app.world().resource::<WindowChoice>().mode,
            WindowMode::Windowed
        );
        for want in [
            WindowMode::WindowedBorderless,
            WindowMode::Windowed,
            WindowMode::WindowedBorderless,
        ] {
            assert!(!press(&mut app, true), "the shortcut's Enter is held back");
            assert_eq!(app.world().resource::<WindowChoice>().mode, want);
            assert_eq!(load_settings(&d).unwrap().window_mode, want);
        }
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn fullscreen_takes_the_monitors_own_video_mode() {
        use bevy::math::UVec2;
        use bevy::window::{Monitor, VideoMode};
        let mode = |w, h, hz, depth| VideoMode {
            physical_size: UVec2::new(w, h),
            bit_depth: depth,
            refresh_rate_millihertz: hz,
        };
        let mut m = Monitor {
            name: None,
            physical_width: 2560,
            physical_height: 1440,
            physical_position: bevy::math::IVec2::ZERO,
            refresh_rate_millihertz: Some(143_998),
            scale_factor: 1.0,
            video_modes: vec![
                mode(1920, 1080, 144_000, 32),
                mode(2560, 1440, 60_000, 32),
                mode(2560, 1440, 144_000, 16),
                mode(2560, 1440, 144_000, 32),
            ],
        };
        // Its own size at the nearest rate to the current one, deepest colour.
        assert_eq!(native_video_mode(&m), Some(mode(2560, 1440, 144_000, 32)));
        let e = bevy::prelude::Entity::from_raw_u32(7).unwrap();
        let on = Some((e, native_video_mode(&m)));
        assert!(matches!(
            WindowMode::Fullscreen.bevy(on),
            bevy::window::WindowMode::Fullscreen(
                bevy::window::MonitorSelection::Entity(_),
                bevy::window::VideoModeSelection::Specific(_)
            )
        ));
        // No mode at its size: Fullscreen shows as Windowed Borderless.
        m.video_modes.retain(|v| v.physical_size.x != 2560);
        assert_eq!(native_video_mode(&m), None);
        assert!(matches!(
            WindowMode::Fullscreen.bevy(Some((e, None))),
            bevy::window::WindowMode::BorderlessFullscreen(_)
        ));
        // At start, before the monitor is known, it opens borderless.
        let s = Settings {
            window_mode: WindowMode::Fullscreen,
            ..Settings::default()
        };
        assert!(matches!(
            window_for(&s).mode,
            bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Current)
        ));
    }

    #[test]
    fn every_display_choice_round_trips() {
        for size in WINDOW_SIZES {
            for mode in ScaleMode::ALL {
                for vsync in [false, true] {
                    for limit in FRAME_LIMITS {
                        let s = Settings {
                            window_size: size,
                            scale_mode: mode,
                            vsync,
                            frame_limit: limit,
                            ..Settings::default()
                        };
                        assert_eq!(parse_settings(&write_settings(&s)).unwrap(), s);
                    }
                }
            }
        }
    }

    #[test]
    fn unknown_display_choices_fall_back_to_the_defaults() {
        let s = parse_settings(
            "version = 2\n[video]\nwindow_size = \"1234x567\"\nscale_mode = \"stretch\"\nframe_limit = 59\n",
        )
        .unwrap();
        let d = Settings::default();
        assert_eq!(
            (s.window_size, s.scale_mode, s.frame_limit),
            (d.window_size, d.scale_mode, d.frame_limit)
        );
    }

    #[test]
    fn menu_sides_default_to_black_round_trip_and_follow_the_options() {
        let d = Settings::default();
        assert_eq!(d.menu_sides, MenuSides::Black);
        // Missing (a version 1 file, or a version 2 file before the key):
        // black.
        for text in ["version = 1\n", "version = 2\n[video]\nvsync = true\n"] {
            assert_eq!(parse_settings(text).unwrap().menu_sides, MenuSides::Black);
        }
        for sides in MenuSides::ALL {
            let s = Settings {
                menu_sides: sides,
                ..Settings::default()
            };
            assert_eq!(parse_settings(&write_settings(&s)).unwrap(), s);
        }
        // An unknown choice is the default; a wrong type is an error.
        let s = parse_settings("version = 2\n[video]\nmenu_sides = \"mirror\"\n").unwrap();
        assert_eq!(s.menu_sides, MenuSides::Black);
        assert!(parse_settings("version = 2\n[video]\nmenu_sides = 1\n").is_err());
        // A change of sides alone needs nothing applied to the window.
        let mut c = WindowChoice {
            pending: false,
            ..WindowChoice::of(&d)
        };
        c.update(&Settings {
            menu_sides: MenuSides::Blur,
            ..d
        });
        assert_eq!(c.sides, MenuSides::Blur);
        assert!(!c.pending);
    }

    #[test]
    fn the_window_mode_and_size_lists_and_their_names_agree() {
        for (size, name) in WINDOW_SIZES.into_iter().zip(WINDOW_SIZE_NAMES) {
            match size {
                WindowSize::Maximized => assert_eq!(name, "Maximized"),
                WindowSize::Fixed(..) => assert_eq!(size.key(), name),
            }
            assert_eq!(
                WindowSize::from_key(&size.key(), SETTINGS_VERSION),
                Some(size)
            );
            assert_eq!(WINDOW_SIZES[size.index()], size);
        }
        assert_eq!(WINDOW_SIZES[0], WindowSize::default());
        for mode in WindowMode::ALL {
            assert_eq!(
                WindowMode::from_key(mode.key(), SETTINGS_VERSION),
                Some(mode)
            );
        }
        assert_eq!(
            WINDOW_MODE_NAMES,
            ["Windowed", "Windowed Borderless", "Fullscreen"]
        );
    }

    #[test]
    fn window_sizes_fit_the_monitor() {
        let at_1080: Vec<_> = WINDOW_SIZES
            .into_iter()
            .filter(|s| s.fits(Some((1920, 1080))))
            .collect();
        assert_eq!(
            at_1080,
            [
                WindowSize::Maximized,
                WindowSize::Fixed(1920, 1080),
                WindowSize::Fixed(800, 600),
                WindowSize::Fixed(640, 480),
                WindowSize::Fixed(1280, 960),
            ]
        );
        // A 2160p monitor takes every size.
        let at_2160 = WINDOW_SIZES
            .into_iter()
            .filter(|s| s.fits(Some((3840, 2160))))
            .count();
        assert_eq!(at_2160, WINDOW_SIZES.len());
        // An unknown monitor rules out nothing.
        assert!(WindowSize::Fixed(3840, 2160).fits(None));
    }

    #[test]
    fn a_size_too_big_for_the_monitor_falls_back_to_the_largest_that_fits() {
        let big = WindowSize::Fixed(3840, 2160);
        assert_eq!(
            big.or_fitting(Some((2560, 1440))),
            WindowSize::Fixed(2560, 1440)
        );
        assert_eq!(
            big.or_fitting(Some((1920, 1200))),
            WindowSize::Fixed(1920, 1080)
        );
        assert_eq!(big.or_fitting(Some((3840, 2160))), big);
        assert_eq!(
            big.or_fitting(Some((700, 500))),
            WindowSize::Fixed(640, 480)
        );
        // Nothing listed fits a tiny monitor: maximized.
        assert_eq!(big.or_fitting(Some((600, 400))), WindowSize::Maximized);
    }

    #[test]
    fn the_windowed_size_is_only_for_a_windowed_window() {
        let mut s = Settings {
            window_size: WindowSize::Fixed(1920, 1080),
            ..Settings::default()
        };
        let c = WindowChoice::of(&s);
        assert!(c.pending);
        assert_eq!(
            c.windowed_size(Some((2560, 1440))),
            Some(WindowSize::Fixed(1920, 1080))
        );
        let max = WindowChoice {
            size: WindowSize::Maximized,
            ..c
        };
        assert_eq!(max.windowed_size(None), Some(WindowSize::Maximized));
        for mode in [WindowMode::WindowedBorderless, WindowMode::Fullscreen] {
            let c = WindowChoice { mode, ..c };
            assert_eq!(c.windowed_size(Some((2560, 1440))), None, "{mode:?}");
        }
        // An unchanged choice stays applied; a new one is pending.
        let mut applied = WindowChoice {
            pending: false,
            ..c
        };
        applied.update(&s);
        assert!(!applied.pending);
        s.window_mode = WindowMode::WindowedBorderless;
        applied.update(&s);
        assert!(applied.pending);
        assert_eq!(applied.mode, WindowMode::WindowedBorderless);
    }

    /// A headless app with a primary window on a 2560 × 1440 monitor at
    /// display scaling `scale`, the window choice of `s` added.
    fn window_app(s: &Settings, scale: f32, dir: &str) -> bevy::prelude::App {
        use bevy::prelude::*;
        use bevy::window::{Monitor, PrimaryMonitor, PrimaryWindow, VideoMode, WindowResolution};
        let mut app = App::new();
        app.add_message::<bevy::window::WindowScaleFactorChanged>()
            .add_message::<bevy::input::keyboard::KeyboardInput>()
            .init_resource::<bevy::input::ButtonInput<bevy::input::keyboard::KeyCode>>();
        add_window_choice(&mut app, s, tmp(dir));
        let mut res = WindowResolution::new(800, 600);
        res.set_scale_factor(scale);
        app.world_mut().spawn((
            Window {
                resolution: res,
                mode: s.window_mode.bevy(None),
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut().spawn((
            Monitor {
                name: None,
                physical_width: 2560,
                physical_height: 1440,
                physical_position: IVec2::ZERO,
                refresh_rate_millihertz: Some(144_000),
                scale_factor: f64::from(scale),
                video_modes: vec![VideoMode {
                    physical_size: UVec2::new(2560, 1440),
                    bit_depth: 32,
                    refresh_rate_millihertz: 144_000,
                }],
            },
            PrimaryMonitor,
        ));
        app
    }

    fn primary_window(app: &mut bevy::prelude::App) -> bevy::window::Window {
        use bevy::prelude::*;
        let mut q = app
            .world_mut()
            .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>();
        q.single(app.world()).unwrap().clone()
    }

    /// The window's physical size is the chosen size whatever the display
    /// scaling (100, 125 and 150 % on Windows).
    #[test]
    fn the_window_gets_its_physical_size_at_every_display_scaling() {
        for scale in [1.0f32, 1.25, 1.5] {
            for (w, h) in [(1920, 1080), (800, 600)] {
                let s = Settings {
                    window_size: WindowSize::Fixed(w, h),
                    ..Settings::default()
                };
                let mut app = window_app(&s, scale, "scale");
                app.update();
                let win = primary_window(&mut app);
                assert_eq!(
                    (win.physical_width(), win.physical_height()),
                    (w, h),
                    "{w}x{h} at {scale}"
                );
                assert!(!app.world().resource::<WindowChoice>().pending);
            }
        }
    }

    /// Switching the mode many times (the Video page and Alt+Enter) always
    /// leaves the window in the chosen mode: a Windowed window maximized or
    /// at its size, Windowed Borderless on its monitor, Fullscreen exclusive
    /// in the monitor's own video mode. A fixed size after Maximized first
    /// restores the window.
    #[test]
    fn repeated_switches_leave_the_window_in_the_chosen_mode() {
        use bevy::window::{MonitorSelection, VideoModeSelection, WindowMode as Bw};
        let mut s = Settings::default();
        let mut app = window_app(&s, 1.25, "switch");
        app.update();
        assert!(app.world().resource::<WindowChoice>().maximized);
        let steps = [
            WindowMode::Fullscreen,
            WindowMode::WindowedBorderless,
            WindowMode::Windowed,
            WindowMode::Fullscreen,
            WindowMode::Windowed,
            WindowMode::WindowedBorderless,
            WindowMode::Fullscreen,
        ];
        for round in 0..3 {
            for (i, &mode) in steps.iter().enumerate() {
                if (round + i) % 2 == 0 {
                    s.set_window_mode(mode);
                } else {
                    s.toggle_borderless();
                }
                // The Windowed size changes too, now and then.
                if i == 4 {
                    s.window_size = if s.window_size == WindowSize::Maximized {
                        WindowSize::Fixed(1920, 1080)
                    } else {
                        WindowSize::Maximized
                    };
                }
                app.world_mut().resource_mut::<WindowChoice>().update(&s);
                for _ in 0..3 {
                    app.update();
                }
                let c = *app.world().resource::<WindowChoice>();
                assert!(!c.pending, "{round} {i}");
                let w = primary_window(&mut app);
                match s.window_mode {
                    WindowMode::Windowed => {
                        assert_eq!(w.mode, Bw::Windowed);
                        match s.window_size {
                            WindowSize::Maximized => assert!(c.maximized),
                            WindowSize::Fixed(fw, fh) => {
                                assert!(!c.maximized);
                                assert_eq!((w.physical_width(), w.physical_height()), (fw, fh));
                            }
                        }
                    }
                    WindowMode::WindowedBorderless => assert!(matches!(
                        w.mode,
                        Bw::BorderlessFullscreen(MonitorSelection::Entity(_))
                    )),
                    WindowMode::Fullscreen => assert!(matches!(
                        w.mode,
                        Bw::Fullscreen(MonitorSelection::Entity(_), VideoModeSelection::Specific(v))
                            if v.physical_size == bevy::math::UVec2::new(2560, 1440)
                    )),
                }
            }
        }
    }

    // Play loads `controls.toml` from the folder the Configure Controls
    // screen saves to (the player's D2Gamma folder); `fallback` only when
    // there is none.
    #[test]
    fn controls_dir_is_the_configure_controls_folder() {
        let spec = Path::new("/cfg/D2RUST/controls.toml");
        let fallback = Path::new("/docs/D2RUST");
        assert_eq!(controls_dir(Some(spec), fallback), Path::new("/cfg/D2RUST"));
        assert_eq!(controls_path(&controls_dir(Some(spec), fallback)), spec);
        assert_eq!(controls_dir(None, fallback), fallback);
    }

    #[test]
    fn controls_file_is_created_once_then_read_back() {
        let d = tmp("ctl");
        let first = load_controls(&d).unwrap();
        assert!(controls_path(&d).exists());
        // An edit to the file is what the next run reads.
        let text = std::fs::read_to_string(controls_path(&d)).unwrap();
        std::fs::write(
            controls_path(&d),
            text.replace("[unbind]\nlist = []", "[unbind]\nlist = [\"swap_weapons\"]"),
        )
        .unwrap();
        let second = load_controls(&d).unwrap();
        assert_ne!(first, second);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn config_dir_is_beside_the_saves() {
        assert_eq!(
            config_dir(Path::new("/h/Documents/D2RUST/saves")),
            PathBuf::from("/h/Documents/D2RUST")
        );
        assert_eq!(config_dir(Path::new("saves")), PathBuf::from("saves"));
    }

    /// The main menu's settings: the settings in force reach the front end,
    /// and a change the settings menu hands it is saved and heard at once
    /// (the menu music's level).
    #[test]
    fn the_main_menus_options_apply_live_and_are_saved() {
        use crate::app::front_host::FrontHost;
        use crate::assets::path::MemorySource;
        use crate::audio::environment::frontend_stream::FrontMusic;
        use crate::ui::front_end::FrontEnd;
        use bevy::prelude::*;
        let dir = tmp("front-live");
        let settings = Settings {
            music_volume: 50,
            ..Settings::default()
        };
        let front = FrontEnd::with_screens(true, Box::new(true));
        let music = FrontMusic::new(std::sync::Arc::new(MemorySource::default()), true, true, 1);
        let host = FrontHost::with_front(front, None, false).with_music(music);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ConfigRes {
                dir: dir.clone(),
                settings,
                bindings: Bindings::empty(),
                sound_device: true,
            })
            .insert_non_send(host)
            .add_systems(Update, front_settings);
        app.update();
        let level = |app: &App| app.world().non_send::<FrontHost>().music_level_db();
        let at_half = level(&app).expect("music");
        assert!(at_half < 0, "Music Volume 50 is quieter: {at_half}");
        // The settings menu turns Sound down a step (`menu_host`).
        {
            let mut host = app.world_mut().non_send_mut::<FrontHost>();
            assert_eq!(host.front.settings(), settings, "the settings in force");
            let mut s = host.front.settings();
            s.master_volume -= 5;
            host.front.edit_settings(s);
        }
        app.update();
        let cfg = app.world().resource::<ConfigRes>().settings;
        assert!(cfg.master_volume < 100, "{cfg:?}");
        assert!(level(&app).unwrap() < at_half, "heard at once");
        assert_eq!(load_settings(&dir).unwrap(), cfg, "saved");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
