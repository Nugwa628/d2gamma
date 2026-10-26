// D2RUST's own: the settings table.
//! The one table of every setting the settings menu shows: its id, its
//! category and group, its kind and range, its one-line description,
//! whether it applies at once, and how it reads and writes [`Settings`].
//! The full menu, the Ctrl+O overlay, search, Reset to defaults and the
//! presets all read this table and nothing else, so a setting added here
//! appears everywhere at once.
//!
//! A setting's default is what [`Settings::default`] holds: the table
//! never repeats it. Values are the stored numbers ([`Entry::get`]): a
//! choice's index, a slider's own value, 0/1 for a switch.
//!
//! Some rows depend on the machine ([`Facts`]): Window Size only applies to
//! a plain window and offers the sizes that fit the monitor; with VSync on,
//! frame limits above the monitor's refresh rate are greyed; the sound rows
//! need a sound device; Party Names need the party shown.

use crate::app::config::{
    AnimationFrameMode, CursorSize, FxaaLevel, Look, MenuSides, ScaleMode, ScalingFilter, Settings,
    WindowMode, WindowSize, CURSOR_SIZE_NAMES, FRAME_LIMITS, FXAA_NAMES, LOOK_NAMES,
    SCALING_FILTER_NAMES, WINDOW_MODE_NAMES, WINDOW_SIZES, WINDOW_SIZE_NAMES,
};
use crate::app::frame_limit::limit_greyed;
use crate::ui::automap::minimap::SIZE_NAMES as MINIMAP_SIZE_NAMES;
use crate::ui::item_moves::Shortcut;
use crate::ui::ui_space::{UI_SCALE_MAX, UI_SCALE_MIN};

/// The menu's categories, in the order of its left rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Category {
    #[default]
    Display,
    Picture,
    Interface,
    Gameplay,
    Sound,
    Controls,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Display,
        Category::Picture,
        Category::Interface,
        Category::Gameplay,
        Category::Sound,
        Category::Controls,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::Display => "Display",
            Category::Picture => "Picture",
            Category::Interface => "Interface",
            Category::Gameplay => "Gameplay",
            Category::Sound => "Sound",
            Category::Controls => "Controls",
        }
    }

    /// The categories the Ctrl+O overlay shows, as its tabs.
    pub const OVERLAY: [Category; 3] = [Category::Picture, Category::Display, Category::Interface];
}

/// How a setting is shown and changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Off (0) or on (1).
    Toggle,
    /// A value from `min` to `max` in steps of `step`; `suffix` follows the
    /// number ("%").
    Slider {
        min: i64,
        max: i64,
        step: i64,
        suffix: &'static str,
    },
    /// One of a few short values, side by side.
    Segmented(&'static [&'static str]),
    /// One of a list, in a drop-down.
    Choice(&'static [&'static str]),
    /// A button that opens another screen.
    Action,
}

/// When a change takes effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applies {
    /// At once, while the menu is open.
    Live,
    /// At the next start of the game.
    NextStart,
}

/// What a row does when it is an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Opens the Configure Controls screen.
    ConfigureControls,
}

/// One setting.
#[derive(Clone, Copy)]
pub struct Entry {
    /// The key in `settings.toml` (`section.key`), or the action's name.
    pub id: &'static str,
    pub category: Category,
    /// The heading the row sits under in its category.
    pub group: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    /// One line on what it does, shown in the info panel.
    pub description: &'static str,
    pub applies: Applies,
    pub get: fn(&Settings) -> i64,
    pub set: fn(&mut Settings, i64),
    /// The action of an [`Kind::Action`] row.
    pub action: Option<Action>,
}

impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entry").field("id", &self.id).finish()
    }
}

impl Entry {
    /// The setting's value in `s`.
    pub fn value(&self, s: &Settings) -> i64 {
        (self.get)(s)
    }

    /// The default value.
    pub fn default_value(&self) -> i64 {
        (self.get)(&Settings::default())
    }

    /// The lowest and highest value.
    pub fn range(&self) -> (i64, i64) {
        match self.kind {
            Kind::Toggle => (0, 1),
            // UI Scale stops where the UI frame would drop below the
            // game's floor (`ui_space::ui_scale_max`).
            Kind::Slider { min, .. } if self.id == "video.ui_scale" => (
                min,
                crate::ui::ui_space::ui_scale_max(crate::rules::camera::FrameSize::play()).into(),
            ),
            Kind::Slider { min, max, .. } => (min, max),
            Kind::Segmented(v) | Kind::Choice(v) => (0, v.len() as i64 - 1),
            Kind::Action => (0, 0),
        }
    }

    /// Writes `v` (kept in range, a slider's on its steps) into `s`.
    pub fn store(&self, s: &mut Settings, v: i64) {
        if self.kind == Kind::Action {
            return;
        }
        let (lo, hi) = self.range();
        let mut v = v.clamp(lo, hi);
        if let Kind::Slider { min, step, .. } = self.kind {
            if step > 1 {
                v = min + ((v - min) + step / 2) / step * step;
                v = v.clamp(lo, hi);
            }
        }
        (self.set)(s, v);
    }

    /// The names of a segmented or choice row's values.
    pub fn names(&self) -> &'static [&'static str] {
        match self.kind {
            Kind::Segmented(v) | Kind::Choice(v) => v,
            _ => &[],
        }
    }

    /// Words to find the row by: its label, group, category and
    /// description, lower case.
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        let hay = format!(
            "{} {} {} {}",
            self.label,
            self.group,
            self.category.name(),
            self.description
        )
        .to_lowercase();
        q.split_whitespace().all(|w| hay.contains(w))
    }
}

/// What the platform reports about the game's window right now, for the
/// Display and Picture lines ([`super::info`]) and the Window Size row. The
/// host reads it every frame, so it follows each setting change and
/// Alt+Enter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowFacts {
    /// The mode the window is in now (Alt+Enter included).
    pub mode: Option<WindowMode>,
    /// The window's size in physical pixels.
    pub size: Option<(u32, u32)>,
    /// The game frame in force.
    pub frame: Option<(u32, u32)>,
}

/// What the rows need to know about the machine, from the host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    /// The sound device is up (not under `--no-sound`).
    pub sound_device: bool,
    /// The monitor's physical size, when known.
    pub monitor: Option<(u32, u32)>,
    /// The monitor's refresh rate in Hz, when known.
    pub refresh_hz: Option<u32>,
    /// The window as the platform reports it now.
    pub window: WindowFacts,
}

const OFF_ON: &[&str] = &["Off", "On"];

fn index_of<T: PartialEq + Copy>(all: &[T], v: T) -> i64 {
    all.iter().position(|&x| x == v).unwrap_or(0) as i64
}

fn pick<T: Copy>(all: &[T], i: i64) -> T {
    all[(i.max(0) as usize).min(all.len() - 1)]
}

/// The Frame Limit row's values, one per [`FRAME_LIMITS`] entry.
pub const FRAME_LIMIT_NAMES: [&str; FRAME_LIMITS.len()] =
    ["Off", "30", "60", "120", "144", "165", "240", "360"];

/// The Controls page's switch of item-moving shortcut `$s`
/// ([`crate::ui::item_moves`]), under the "Item Shortcuts" heading.
macro_rules! shortcut_row {
    ($id:literal, $s:ident) => {
        Entry {
            id: $id,
            category: Category::Controls,
            group: "Item Shortcuts",
            label: Shortcut::$s.label(),
            kind: Kind::Toggle,
            description: Shortcut::$s.description(),
            applies: Applies::Live,
            get: |s| i64::from(s.item_shortcuts.has(Shortcut::$s)),
            set: |s, v| s.item_shortcuts.set(Shortcut::$s, v != 0),
            action: None,
        }
    };
}

/// Every setting, by category and group, in menu order.
pub const TABLE: &[Entry] = &[
    // Display.
    Entry {
        id: "video.window_mode",
        category: Category::Display,
        group: "Window",
        label: "Window Mode",
        kind: Kind::Choice(&WINDOW_MODE_NAMES),
        description: "Windowed, Windowed Borderless (covers the monitor, switches instantly) or \
                      Fullscreen. Alt+Enter switches to Windowed Borderless and back.",
        applies: Applies::Live,
        get: |s| index_of(&WindowMode::ALL, s.window_mode),
        set: |s, v| s.set_window_mode(pick(&WindowMode::ALL, v)),
        action: None,
    },
    Entry {
        id: "video.window_size",
        category: Category::Display,
        group: "Window",
        label: "Window Size",
        kind: Kind::Choice(&WINDOW_SIZE_NAMES),
        description: "The size of a plain window, in screen pixels. Borderless and Fullscreen \
                      always use the monitor's own size.",
        applies: Applies::Live,
        get: |s| s.window_size.index() as i64,
        set: |s, v| s.window_size = pick(&WINDOW_SIZES, v),
        action: None,
    },
    Entry {
        id: "video.menu_sides",
        category: Category::Display,
        group: "Window",
        label: "Menu Sides",
        kind: Kind::Segmented(&["Black", "Blur", "Extend"]),
        description: "What fills a wide screen beside the main menu's picture.",
        applies: Applies::Live,
        get: |s| index_of(&MenuSides::ALL, s.menu_sides),
        set: |s, v| s.menu_sides = pick(&MenuSides::ALL, v),
        action: None,
    },
    Entry {
        id: "video.vsync",
        category: Category::Display,
        group: "Frames",
        label: "VSync",
        kind: Kind::Toggle,
        description: "Waits for the monitor's refresh before showing each frame: no tearing, \
                      a little more input delay.",
        applies: Applies::Live,
        get: |s| i64::from(s.vsync),
        set: |s, v| s.vsync = v != 0,
        action: None,
    },
    Entry {
        id: "video.frame_limit",
        category: Category::Display,
        group: "Frames",
        label: "Frame Limit",
        kind: Kind::Choice(&FRAME_LIMIT_NAMES),
        description: "The most frames drawn per second. With VSync on, limits above the \
                      monitor's refresh rate have no effect.",
        applies: Applies::Live,
        get: |s| index_of(&FRAME_LIMITS, s.frame_limit),
        set: |s, v| s.frame_limit = pick(&FRAME_LIMITS, v),
        action: None,
    },
    // Picture.
    Entry {
        id: "video.scale_mode",
        category: Category::Picture,
        group: "Scaling",
        label: "Scale Mode",
        kind: Kind::Segmented(&["Fit", "Integer"]),
        description: "Fit fills the screen and keeps every game pixel sharp; Integer uses the \
                      largest whole scale, with thin bars.",
        applies: Applies::Live,
        get: |s| index_of(&ScaleMode::ALL, s.scale_mode),
        set: |s, v| s.scale_mode = pick(&ScaleMode::ALL, v),
        action: None,
    },
    Entry {
        id: "video.scaling_filter",
        category: Category::Picture,
        group: "Scaling",
        label: "Scaling Filter",
        kind: Kind::Choice(&SCALING_FILTER_NAMES),
        description: "How the picture is enlarged to the screen. Sharp Bilinear and Nearest keep \
                      pixels crisp; Catmull-Rom and Lanczos are softer; Smooth Edges smooths \
                      diagonal edges in the art.",
        applies: Applies::Live,
        get: |s| index_of(&ScalingFilter::ALL, s.scaling_filter),
        set: |s, v| s.scaling_filter = pick(&ScalingFilter::ALL, v),
        action: None,
    },
    Entry {
        id: "video.native_ui",
        category: Category::Picture,
        group: "Scaling",
        label: "Sharp Interface",
        kind: Kind::Toggle,
        description: "Draws the HUD, panels and text at your screen's own resolution instead of \
                      enlarging them with the picture.",
        applies: Applies::Live,
        get: |s| i64::from(s.native_ui),
        set: |s, v| s.native_ui = v != 0,
        action: None,
    },
    Entry {
        id: "video.gamma",
        category: Category::Picture,
        group: "Colour",
        label: "Gamma",
        kind: Kind::Slider {
            min: 55,
            max: 255,
            step: 10,
            suffix: "",
        },
        description: "Brightens the dark and middle tones; 155 shows the picture as it is.",
        applies: Applies::Live,
        get: |s| s.gamma.into(),
        set: |s, v| s.gamma = v as u16,
        action: None,
    },
    Entry {
        id: "video.contrast",
        category: Category::Picture,
        group: "Colour",
        label: "Contrast",
        kind: Kind::Slider {
            min: 0,
            max: 100,
            step: 5,
            suffix: "",
        },
        description: "Spreads the tones apart; 100 shows the picture as it is.",
        applies: Applies::Live,
        get: |s| s.contrast.into(),
        set: |s, v| s.contrast = v as u8,
        action: None,
    },
    Entry {
        id: "video.brightness",
        category: Category::Picture,
        group: "Colour",
        label: "Brightness",
        kind: Kind::Slider {
            min: 0,
            max: 100,
            step: 5,
            suffix: "",
        },
        description: "Lightens or darkens every tone alike; 50 shows the picture as it is.",
        applies: Applies::Live,
        get: |s| s.brightness.into(),
        set: |s, v| s.brightness = v as u8,
        action: None,
    },
    Entry {
        id: "video.saturation",
        category: Category::Picture,
        group: "Colour",
        label: "Saturation",
        kind: Kind::Slider {
            min: 0,
            max: 200,
            step: 10,
            suffix: "%",
        },
        description: "How strong the colours are: 0% is grey, 100% shows the picture as it is.",
        applies: Applies::Live,
        get: |s| s.saturation.into(),
        set: |s, v| s.saturation = v as u8,
        action: None,
    },
    Entry {
        id: "video.vibrance",
        category: Category::Picture,
        group: "Colour",
        label: "Vibrance",
        kind: Kind::Slider {
            min: 0,
            max: 100,
            step: 5,
            suffix: "",
        },
        description: "Strengthens the dull colours and leaves the strong ones; 0 is off.",
        applies: Applies::Live,
        get: |s| s.vibrance.into(),
        set: |s, v| s.vibrance = v as u8,
        action: None,
    },
    Entry {
        id: "video.look",
        category: Category::Picture,
        group: "Colour",
        label: "Look",
        kind: Kind::Choice(&LOOK_NAMES),
        description: "A colour grade for the game world: Original shows the art as it is; Warm, \
                      Cool, Vivid and Muted are mild grades of our own. The interface keeps its \
                      colours.",
        applies: Applies::Live,
        get: |s| index_of(&Look::ALL, s.look),
        set: |s, v| s.look = pick(&Look::ALL, v),
        action: None,
    },
    Entry {
        id: "video.sharpen",
        category: Category::Picture,
        group: "Effects",
        label: "Sharpen",
        kind: Kind::Slider {
            min: 0,
            max: 100,
            step: 5,
            suffix: "",
        },
        description: "Brings out edges and fine detail in the enlarged picture; 0 is off. \
                      The interface is never sharpened.",
        applies: Applies::Live,
        get: |s| s.sharpen.into(),
        set: |s, v| s.sharpen = v as u8,
        action: None,
    },
    Entry {
        id: "video.fxaa",
        category: Category::Picture,
        group: "Effects",
        label: "FXAA",
        kind: Kind::Segmented(&FXAA_NAMES),
        description: "Smooths jagged edges in the enlarged picture; higher follows longer and \
                      fainter edges. The interface stays untouched.",
        applies: Applies::Live,
        get: |s| index_of(&FxaaLevel::ALL, s.fxaa),
        set: |s, v| s.fxaa = pick(&FxaaLevel::ALL, v),
        action: None,
    },
    Entry {
        id: "video.bloom",
        category: Category::Picture,
        group: "Effects",
        label: "Bloom",
        kind: Kind::Toggle,
        description: "A soft glow around the bright parts of the game world, such as fire and \
                      spells. The interface does not glow.",
        applies: Applies::Live,
        get: |s| s.bloom.into(),
        set: |s, v| s.bloom = v != 0,
        action: None,
    },
    Entry {
        id: "video.bloom_strength",
        category: Category::Picture,
        group: "Effects",
        label: "Bloom Strength",
        kind: Kind::Slider {
            min: 0,
            max: 100,
            step: 5,
            suffix: "",
        },
        description: "How strong the glow is while Bloom is on.",
        applies: Applies::Live,
        get: |s| s.bloom_strength.into(),
        set: |s, v| s.bloom_strength = v as u8,
        action: None,
    },
    Entry {
        id: "video.light_quality",
        category: Category::Picture,
        group: "Detail",
        label: "Light Quality",
        kind: Kind::Segmented(&["Low", "Medium", "High"]),
        description: "How finely light from torches, spells and the player is drawn.",
        applies: Applies::Live,
        get: |s| s.light_quality.into(),
        set: |s, v| s.light_quality = v as u8,
        action: None,
    },
    Entry {
        id: "video.blended_shadows",
        category: Category::Picture,
        group: "Detail",
        label: "Blended Shadows",
        kind: Kind::Toggle,
        description: "Soft see-through shadows under units instead of solid ones.",
        applies: Applies::Live,
        get: |s| s.blended_shadows.into(),
        set: |s, v| s.blended_shadows = v as u8,
        action: None,
    },
    Entry {
        id: "video.animation_frames",
        category: Category::Picture,
        group: "Detail",
        label: "Animation Frames",
        kind: Kind::Segmented(&["Original", "Smooth"]),
        description: "Smooth adds in-between pictures to the animations; Original plays the \
                      source frames only.",
        applies: Applies::Live,
        get: |s| index_of(&AnimationFrameMode::ALL, s.animation_frames),
        set: |s, v| s.animation_frames = pick(&AnimationFrameMode::ALL, v),
        action: None,
    },
    // Interface.
    Entry {
        id: "video.ui_scale",
        category: Category::Interface,
        group: "Scale",
        label: "UI Scale",
        kind: Kind::Slider {
            min: UI_SCALE_MIN as i64,
            max: UI_SCALE_MAX as i64,
            step: 5,
            suffix: "%",
        },
        description: "Draws the HUD, the panels and their text larger or smaller: \
                      100% is the picture's own scale, from 75% up to 120% (the \
                      largest that keeps the panels whole). The game world keeps \
                      its size.",
        applies: Applies::Live,
        get: |s| s.ui_scale.into(),
        set: |s, v| s.ui_scale = v as u16,
        action: None,
    },
    Entry {
        id: "hud.fps_counter",
        category: Category::Interface,
        group: "Screen",
        label: "FPS Counter",
        kind: Kind::Toggle,
        description: "Shows frames per second and the frame time in the top-left corner.",
        applies: Applies::Live,
        get: |s| s.fps_counter.into(),
        set: |s, v| s.fps_counter = v as u8,
        action: None,
    },
    Entry {
        id: "video.hd_text",
        category: Category::Interface,
        group: "Text",
        label: "HD Text",
        kind: Kind::Toggle,
        description: "Draws all text, these menus included, with a sharp outline font. \
                      Off: the game's own letters.",
        applies: Applies::Live,
        get: |s| i64::from(s.hd_text),
        set: |s, v| s.hd_text = v != 0,
        action: None,
    },
    Entry {
        id: "video.cursor_size",
        category: Category::Interface,
        group: "Pointer",
        label: "Cursor Size",
        kind: Kind::Choice(&CURSOR_SIZE_NAMES),
        description: "The pointer's size. Auto follows the picture's scale (3x at 2160p); \
                      the others are that size on every screen.",
        applies: Applies::Live,
        get: |s| index_of(&CursorSize::ALL, s.cursor_size),
        set: |s, v| s.cursor_size = pick(&CursorSize::ALL, v),
        action: None,
    },
    Entry {
        id: "hud.show_hp_text",
        category: Category::Interface,
        group: "Control Panel",
        label: "Life Text",
        kind: Kind::Toggle,
        description: "Always shows the life numbers over the red globe.",
        applies: Applies::Live,
        get: |s| s.show_hp_text.into(),
        set: |s, v| s.show_hp_text = v as u8,
        action: None,
    },
    Entry {
        id: "hud.show_mp_text",
        category: Category::Interface,
        group: "Control Panel",
        label: "Mana Text",
        kind: Kind::Toggle,
        description: "Always shows the mana numbers over the blue globe.",
        applies: Applies::Live,
        get: |s| s.show_mp_text.into(),
        set: |s, v| s.show_mp_text = v as u8,
        action: None,
    },
    Entry {
        id: "hud.minimap",
        category: Category::Interface,
        group: "Minimap",
        label: "Minimap",
        kind: Kind::Toggle,
        description: "Shows the map around you in the top-right corner.",
        applies: Applies::Live,
        get: |s| s.minimap.into(),
        set: |s, v| s.minimap = v as u8,
        action: None,
    },
    Entry {
        id: "hud.minimap_size",
        category: Category::Interface,
        group: "Minimap",
        label: "Minimap Size",
        kind: Kind::Segmented(&MINIMAP_SIZE_NAMES),
        description: "How big the minimap is in the corner.",
        applies: Applies::Live,
        get: |s| s.minimap_size.into(),
        set: |s, v| s.minimap_size = v as u8,
        action: None,
    },
    Entry {
        id: "hud.minimap_opacity",
        category: Category::Interface,
        group: "Minimap",
        label: "Minimap Opacity",
        kind: Kind::Slider {
            min: 20,
            max: 100,
            step: 5,
            suffix: "%",
        },
        description: "How solid the minimap is: lower lets more of the game show through.",
        applies: Applies::Live,
        get: |s| s.minimap_opacity.into(),
        set: |s, v| s.minimap_opacity = v as u8,
        action: None,
    },
    // Gameplay.
    Entry {
        id: "automap.mode",
        category: Category::Gameplay,
        group: "Automap",
        label: "Automap Size",
        kind: Kind::Segmented(&["Full Screen", "Mini"]),
        description: "The map over the whole view, or a small one in the corner.",
        applies: Applies::Live,
        get: |s| s.automap_mode.into(),
        set: |s, v| s.automap_mode = v as u8,
        action: None,
    },
    Entry {
        id: "automap.fade",
        category: Category::Gameplay,
        group: "Automap",
        label: "Automap Fade",
        kind: Kind::Choice(&["No", "Center", "Everything", "Auto"]),
        description: "Which part of the map is drawn see-through over the world.",
        applies: Applies::Live,
        get: |s| s.automap_fade.into(),
        set: |s, v| s.automap_fade = v as u8,
        action: None,
    },
    Entry {
        id: "automap.centers",
        category: Category::Gameplay,
        group: "Automap",
        label: "Center when Cleared",
        kind: Kind::Toggle,
        description: "Centres the map on the player each time it is opened.",
        applies: Applies::Live,
        get: |s| s.automap_centers.into(),
        set: |s, v| s.automap_centers = v as u8,
        action: None,
    },
    Entry {
        id: "automap.party",
        category: Category::Gameplay,
        group: "Automap",
        label: "Show Party",
        kind: Kind::Toggle,
        description: "Marks the other players of your party on the map.",
        applies: Applies::Live,
        get: |s| s.automap_party.into(),
        set: |s, v| s.automap_party = v as u8,
        action: None,
    },
    Entry {
        id: "automap.party_names",
        category: Category::Gameplay,
        group: "Automap",
        label: "Show Party Names",
        kind: Kind::Toggle,
        description: "Writes each party member's name beside their mark.",
        applies: Applies::Live,
        get: |s| i64::from(s.automap_party != 0 && s.automap_party_names != 0),
        set: |s, v| s.automap_party_names = v as u8,
        action: None,
    },
    // Sound.
    Entry {
        id: "audio.master_volume",
        category: Category::Sound,
        group: "Volume",
        label: "Sound",
        kind: Kind::Slider {
            min: 0,
            max: 100,
            step: 5,
            suffix: "%",
        },
        description: "The volume of the game's sounds.",
        applies: Applies::Live,
        get: |s| s.master_volume.into(),
        set: |s, v| s.master_volume = v as u8,
        action: None,
    },
    Entry {
        id: "audio.music_volume",
        category: Category::Sound,
        group: "Volume",
        label: "Music",
        kind: Kind::Slider {
            min: 0,
            max: 100,
            step: 5,
            suffix: "%",
        },
        description: "The volume of the music.",
        applies: Applies::Live,
        get: |s| s.music_volume.into(),
        set: |s, v| s.music_volume = v as u8,
        action: None,
    },
    Entry {
        id: "audio.npc_speech",
        category: Category::Sound,
        group: "Speech",
        label: "NPC Speech",
        kind: Kind::Choice(&["Audio Only", "Text Only", "Audio and Text"]),
        description: "Whether the townsfolk's words are spoken, written, or both.",
        applies: Applies::Live,
        get: |s| s.npc_speech.into(),
        set: |s, v| s.npc_speech = v as u8,
        action: None,
    },
    // Controls.
    Entry {
        id: "controls.configure",
        category: Category::Controls,
        group: "Keys",
        label: "Configure Controls",
        kind: Kind::Action,
        description: "Opens the key bindings: every command, its keys and the mouse buttons.",
        applies: Applies::Live,
        get: |_| 0,
        set: |_, _| {},
        action: Some(Action::ConfigureControls),
    },
    Entry {
        id: "controls.market_key",
        category: Category::Controls,
        group: "Keys",
        label: "Market",
        kind: Kind::Choice(&crate::ui::market::MarketKey::NAMES),
        description: "Opens and closes the market window online. With Ctrl held, the letter's \
                      own key (I: the inventory) does not run.",
        applies: Applies::Live,
        get: |s| i64::from(s.market_key),
        set: |s, v| s.market_key = crate::ui::market::MarketKey::of_index(v).index() as u8,
        action: None,
    },
    shortcut_row!("controls.move_stash", Stash),
    shortcut_row!("controls.move_cube", Cube),
    shortcut_row!("controls.move_trade", Trade),
    shortcut_row!("controls.stack_to_slot", StackSlot),
    shortcut_row!("controls.take_count", TakeCount),
    shortcut_row!("controls.take_all", TakeAll),
    shortcut_row!("controls.drop_item", Drop),
    shortcut_row!("controls.into_cube", IntoCube),
    shortcut_row!("controls.quick_sell", Sell),
    shortcut_row!("controls.tab_drop", TabDrop),
];

/// The entry with id `id`.
pub fn entry(id: &str) -> Option<&'static Entry> {
    TABLE.iter().find(|e| e.id == id)
}

/// The table's rows of category `c`, in order.
pub fn rows_of(c: Category) -> impl Iterator<Item = usize> {
    TABLE
        .iter()
        .enumerate()
        .filter(move |(_, e)| e.category == c)
        .map(|(i, _)| i)
}

/// The rows whose words match `query`, in table order.
pub fn search(query: &str) -> Vec<usize> {
    TABLE
        .iter()
        .enumerate()
        .filter(|(_, e)| e.matches(query))
        .map(|(i, _)| i)
        .collect()
}

/// Whether row `e` can be changed now.
pub fn enabled(e: &Entry, s: &Settings, f: &Facts) -> bool {
    match e.id {
        "video.window_size" => s.window_mode == WindowMode::Windowed,
        "audio.master_volume" | "audio.music_volume" | "audio.npc_speech" => f.sound_device,
        "automap.party_names" => s.automap_party != 0,
        "video.bloom_strength" => s.bloom,
        "hud.minimap_size" | "hud.minimap_opacity" => s.minimap != 0,
        _ => true,
    }
}

/// Whether value `v` of a choice row can be picked now: the window sizes
/// that fit the monitor, and the frame limits VSync leaves in force.
pub fn value_enabled(e: &Entry, v: i64, s: &Settings, f: &Facts) -> bool {
    match e.id {
        "video.window_size" => pick(&WINDOW_SIZES, v).fits(f.monitor),
        "video.frame_limit" => !limit_greyed(s.vsync, pick(&FRAME_LIMITS, v), f.refresh_hz),
        _ => true,
    }
}

/// The text a row's value shows. Window Size says what the window really
/// is: the monitor's size in Borderless and Fullscreen, and a maximized
/// window's size now.
pub fn value_text(e: &Entry, s: &Settings, f: &Facts) -> String {
    let v = e.value(s);
    match e.kind {
        Kind::Toggle => OFF_ON[v.clamp(0, 1) as usize].to_string(),
        Kind::Slider { suffix, .. } => format!("{v}{suffix}"),
        Kind::Action => String::new(),
        Kind::Segmented(names) | Kind::Choice(names) => {
            let name = names.get(v.max(0) as usize).copied().unwrap_or("");
            if e.id != "video.window_size" {
                return name.to_string();
            }
            let size = |(w, h): (u32, u32)| format!("{w}x{h}");
            if s.window_mode != WindowMode::Windowed {
                return match f.monitor.or(f.window.size) {
                    Some(m) => format!("Native ({})", size(m)),
                    None => "Native".into(),
                };
            }
            match (s.window_size, f.window.mode, f.window.size) {
                (WindowSize::Maximized, Some(WindowMode::Windowed), Some(w)) => {
                    format!("{name} ({})", size(w))
                }
                _ => name.to_string(),
            }
        }
    }
}

/// The next value of row `e` from `s` one step in `dir` (−1 or +1): a
/// slider moves one step, a toggle flips, a choice skips the values that
/// can't be picked now (and stays put at either end).
pub fn stepped(e: &Entry, s: &Settings, f: &Facts, dir: i64) -> i64 {
    let v = e.value(s);
    match e.kind {
        Kind::Toggle => 1 - v.clamp(0, 1),
        Kind::Slider { step, .. } => v + dir * step.max(1),
        Kind::Action => v,
        Kind::Segmented(_) | Kind::Choice(_) => {
            let (lo, hi) = e.range();
            let mut n = v + dir;
            while (lo..=hi).contains(&n) {
                if value_enabled(e, n, s, f) {
                    return n;
                }
                n += dir;
            }
            v
        }
    }
}

/// Puts every setting of category `c` back to its default.
pub fn reset(s: &mut Settings, c: Category) {
    let d = Settings::default();
    for e in TABLE.iter().filter(|e| e.category == c) {
        e.store(s, e.value(&d));
    }
}

/// A named set of values for some settings; the rest are left as they are.
#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub name: &'static str,
    pub description: &'static str,
    /// `(id, value)`; `None` as value: the setting's default.
    pub values: &'static [(&'static str, Option<i64>)],
}

/// The presets the menu's Preset drop-down offers. Each is a whole picture:
/// every setting Default names, at Default's value unless the preset names
/// it too.
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Default",
        description: "D2Gamma's own picture and frame settings.",
        values: &[
            ("video.scale_mode", None),
            ("video.scaling_filter", None),
            ("video.native_ui", None),
            ("video.gamma", None),
            ("video.contrast", None),
            ("video.brightness", None),
            ("video.saturation", None),
            ("video.vibrance", None),
            ("video.look", None),
            ("video.sharpen", None),
            ("video.fxaa", None),
            ("video.bloom", None),
            ("video.bloom_strength", None),
            ("video.light_quality", None),
            ("video.blended_shadows", None),
            ("video.animation_frames", None),
            ("video.vsync", None),
            ("video.frame_limit", None),
        ],
    },
    Preset {
        name: "Crisp Pixels",
        description: "Whole-number scaling, the Nearest filter and the source animation frames.",
        values: &[
            ("video.scale_mode", Some(1)),
            ("video.scaling_filter", Some(1)),
            ("video.native_ui", Some(1)),
            ("video.animation_frames", Some(0)),
        ],
    },
    Preset {
        name: "Performance",
        description: "Lighter light and shadows, source frames, no sharpen, FXAA or bloom, at \
                      most 60 frames per second.",
        values: &[
            ("video.sharpen", Some(0)),
            ("video.fxaa", Some(0)),
            ("video.bloom", Some(0)),
            ("video.light_quality", Some(0)),
            ("video.blended_shadows", Some(0)),
            ("video.animation_frames", Some(0)),
            ("video.frame_limit", Some(2)),
        ],
    },
];

impl Preset {
    /// The preset's value for setting `e` (one Default names).
    fn value_for(&self, e: &Entry) -> i64 {
        self.values
            .iter()
            .find(|&&(id, _)| id == e.id)
            .and_then(|&(_, v)| v)
            .unwrap_or_else(|| e.default_value())
    }

    /// The settings the presets set, each with its row.
    fn entries() -> impl Iterator<Item = &'static Entry> {
        PRESETS[0].values.iter().filter_map(|&(id, _)| entry(id))
    }

    /// Sets the preset's whole picture in `s`.
    pub fn apply(&self, s: &mut Settings) {
        for e in Self::entries() {
            e.store(s, self.value_for(e));
        }
    }

    /// Whether `s` is exactly the preset's picture.
    pub fn holds(&self, s: &Settings) -> bool {
        Self::entries().all(|e| e.value(s) == self.value_for(e))
    }
}

/// The preset `s` is exactly (the first, if several are the same picture),
/// `None`: Custom.
pub fn current_preset(s: &Settings) -> Option<usize> {
    PRESETS.iter().position(|p| p.holds(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::config::{parse_settings, write_settings};

    #[test]
    fn ids_are_unique_and_name_real_settings_keys() {
        let text = write_settings(&Settings::default());
        for (i, e) in TABLE.iter().enumerate() {
            assert!(TABLE[..i].iter().all(|o| o.id != e.id), "{} twice", e.id);
            if e.kind == Kind::Action {
                continue;
            }
            let (sec, key) = e.id.split_once('.').expect("section.key");
            let in_section = text.split(&format!("[{sec}]")).nth(1).is_some_and(|rest| {
                rest.split("\n[")
                    .next()
                    .unwrap()
                    .contains(&format!("\n{key} = "))
            });
            assert!(in_section, "{} is not a key of settings.toml", e.id);
        }
    }

    /// Every setting the old Video page reached is reachable here (the
    /// View row aside: there is one view), and each value the menu sets is
    /// saved and read back.
    #[test]
    fn every_setting_round_trips_through_settings_toml() {
        for e in TABLE.iter().filter(|e| e.kind != Kind::Action) {
            let (lo, hi) = e.range();
            let mut v = lo;
            while v <= hi {
                let mut s = Settings::default();
                if e.id == "automap.party_names" {
                    s.automap_party = 1;
                }
                e.store(&mut s, v);
                let back = parse_settings(&write_settings(&s)).expect("parses");
                assert_eq!(e.value(&back), e.value(&s), "{} = {v}", e.id);
                v += match e.kind {
                    Kind::Slider { step, .. } => step,
                    _ => 1,
                };
            }
        }
        for id in [
            "video.window_mode",
            "video.window_size",
            "video.vsync",
            "video.frame_limit",
            "video.scale_mode",
            "video.native_ui",
            "video.gamma",
            "video.contrast",
            "video.light_quality",
            "video.blended_shadows",
            "video.animation_frames",
            "video.menu_sides",
            "video.hd_text",
            "video.cursor_size",
            "video.scaling_filter",
            "video.brightness",
            "video.saturation",
            "video.vibrance",
            "video.sharpen",
            "video.fxaa",
            "video.bloom",
            "video.bloom_strength",
            "video.look",
            "video.ui_scale",
            "hud.minimap",
            "hud.minimap_size",
            "hud.minimap_opacity",
        ] {
            assert!(entry(id).is_some(), "{id} has no row");
        }
    }

    /// The info panel shows a description as one line of words: no runs of
    /// spaces from a string broken over source lines, no line breaks.
    #[test]
    fn descriptions_are_single_spaced_sentences() {
        for e in TABLE {
            let d = e.description;
            assert!(!d.contains("  ") && !d.contains('\n'), "{}: {d:?}", e.id);
            assert!(d.ends_with('.'), "{}: {d:?}", e.id);
        }
        for p in PRESETS {
            assert!(!p.description.contains("  "), "{}", p.name);
        }
    }

    #[test]
    fn defaults_come_from_the_settings_defaults() {
        let d = Settings::default();
        for e in TABLE {
            assert_eq!(e.default_value(), e.value(&d), "{}", e.id);
        }
        assert_eq!(entry("video.gamma").unwrap().default_value(), 155);
    }

    #[test]
    fn sliders_keep_to_their_steps_and_range() {
        let g = entry("video.gamma").unwrap();
        let mut s = Settings::default();
        g.store(&mut s, 161);
        assert_eq!(s.gamma, 165);
        g.store(&mut s, 9999);
        assert_eq!(s.gamma, 255);
        g.store(&mut s, 0);
        assert_eq!(s.gamma, 55);
        let f = Facts::default();
        s.gamma = 155;
        assert_eq!(stepped(g, &s, &f, 1), 165);
        assert_eq!(stepped(g, &s, &f, -1), 145);
    }

    #[test]
    fn reset_puts_back_one_category_only() {
        let mut s = Settings {
            gamma: 255,
            vsync: false,
            master_volume: 10,
            ..Settings::default()
        };
        reset(&mut s, Category::Picture);
        assert_eq!(s.gamma, 155);
        assert!(!s.vsync, "Display untouched");
        assert_eq!(s.master_volume, 10, "Sound untouched");
        reset(&mut s, Category::Display);
        assert!(s.vsync);
    }

    #[test]
    fn choices_skip_what_cant_be_picked() {
        let e = entry("video.frame_limit").unwrap();
        let f = Facts {
            refresh_hz: Some(60),
            ..Facts::default()
        };
        let mut s = Settings {
            vsync: true,
            ..Settings::default()
        };
        e.store(&mut s, 2); // 60
        assert_eq!(stepped(e, &s, &f, 1), 2, "120 and up are greyed at 60 Hz");
        s.vsync = false;
        assert_eq!(stepped(e, &s, &f, 1), 3);
        let size = entry("video.window_size").unwrap();
        assert!(!enabled(
            size,
            &Settings {
                window_mode: WindowMode::Fullscreen,
                ..s
            },
            &f
        ));
        assert!(enabled(
            size,
            &Settings {
                window_mode: WindowMode::Windowed,
                ..s
            },
            &f
        ));
        // Bloom Strength only while Bloom is on.
        let strength = entry("video.bloom_strength").unwrap();
        assert!(!enabled(strength, &Settings::default(), &f));
        let bloom = Settings {
            bloom: true,
            ..Settings::default()
        };
        assert!(enabled(strength, &bloom, &f));
        let minimap = Settings {
            minimap: 1,
            ..Settings::default()
        };
        for id in ["hud.minimap_size", "hud.minimap_opacity"] {
            let e = entry(id).unwrap();
            assert!(
                !enabled(e, &Settings::default(), &f),
                "{id} with the minimap off"
            );
            assert!(enabled(e, &minimap, &f), "{id} with the minimap on");
        }
    }

    /// The new [hud] and UI Scale rows start at the settings' defaults
    /// (minimap off, Medium, 85%; UI Scale 100%) and reach every value
    /// settings.toml takes.
    #[test]
    fn the_minimap_and_ui_scale_rows_cover_their_keys() {
        let d = Settings::default();
        let val = |id: &str| entry(id).unwrap().value(&d);
        assert_eq!(
            [
                val("hud.minimap"),
                val("hud.minimap_size"),
                val("hud.minimap_opacity"),
                val("video.ui_scale")
            ],
            [0, 1, 85, 100]
        );
        assert_eq!(
            entry("hud.minimap_size").unwrap().names(),
            &MINIMAP_SIZE_NAMES
        );
        assert_eq!(entry("hud.minimap_opacity").unwrap().range(), (20, 100));
        assert_eq!(
            entry("video.ui_scale").unwrap().range(),
            (UI_SCALE_MIN.into(), UI_SCALE_MAX.into())
        );
        // The slider says the range it has.
        assert_eq!(entry("video.ui_scale").unwrap().range(), (75, 120));
        let text = entry("video.ui_scale").unwrap().description;
        assert!(text.contains("from 75% up to 120%"), "{text}");
        let mut s = d;
        entry("video.ui_scale").unwrap().store(&mut s, 113);
        assert_eq!(s.ui_scale, 115, "on its 5% steps");
    }

    /// Bloom is off and the look Vivid by default, and every preset puts
    /// them back: Performance names Bloom (off) and keeps Default's look.
    #[test]
    fn bloom_starts_off_the_look_vivid_and_the_presets_reset_them() {
        let (bloom, look) = (entry("video.bloom").unwrap(), entry("video.look").unwrap());
        let mut s = Settings::default();
        assert_eq!((bloom.value(&s), look.value(&s)), (0, 3), "off, Vivid");
        bloom.store(&mut s, 1);
        look.store(&mut s, 1);
        assert!(s.bloom && s.look == Look::Warm);
        let mut perf = s;
        PRESETS[2].apply(&mut perf);
        assert!(!perf.bloom && perf.look == Look::Vivid);
        PRESETS[0].apply(&mut s);
        assert!(!s.bloom && s.look == Look::Vivid);
        assert_eq!(current_preset(&s), Some(0));
    }

    #[test]
    fn the_window_size_says_what_the_window_is() {
        let e = entry("video.window_size").unwrap();
        let f = Facts {
            monitor: Some((3840, 2160)),
            window: WindowFacts {
                mode: Some(WindowMode::Windowed),
                size: Some((3840, 2097)),
                frame: None,
            },
            ..Facts::default()
        };
        let s = Settings::default();
        assert_eq!(value_text(e, &s, &f), "Maximized (3840x2097)");
        let mut b = s;
        b.window_mode = WindowMode::WindowedBorderless;
        assert_eq!(value_text(e, &b, &f), "Native (3840x2160)");
    }

    #[test]
    fn search_finds_rows_by_any_word() {
        let label = |i: usize| TABLE[i].label;
        assert_eq!(
            search("gamma").into_iter().map(label).collect::<Vec<_>>(),
            ["Gamma"]
        );
        // A description word, any case.
        assert!(search("TEARING")
            .into_iter()
            .map(label)
            .any(|l| l == "VSync"));
        // Every word must match.
        assert!(search("party names")
            .into_iter()
            .map(label)
            .eq(["Show Party Names"]));
        assert_eq!(search("").len(), TABLE.len());
        assert!(search("zzz").is_empty());
    }

    /// Smooth Edges is offered but never chosen for the player: not the
    /// default and not part of any preset.
    #[test]
    fn no_preset_picks_smooth_edges() {
        for p in PRESETS {
            let mut s = Settings::default();
            p.apply(&mut s);
            assert_ne!(s.scaling_filter, ScalingFilter::SmoothEdges, "{}", p.name);
        }
        let row = entry("video.scaling_filter").unwrap();
        assert_ne!(
            row.default_value(),
            index_of(&ScalingFilter::ALL, ScalingFilter::SmoothEdges)
        );
    }

    #[test]
    fn presets_set_their_values_and_are_recognised() {
        let mut s = Settings::default();
        assert_eq!(
            current_preset(&s),
            Some(0),
            "the defaults are the Default preset"
        );
        PRESETS[2].apply(&mut s);
        assert_eq!(s.frame_limit, 60);
        assert_eq!(current_preset(&s), Some(2));
        s.light_quality = 2;
        assert_eq!(current_preset(&s), None, "Custom");
        PRESETS[0].apply(&mut s);
        assert_eq!((s.gamma, s.frame_limit), (155, 0));
    }

    /// A setting no preset names a different value for still counts: the
    /// label says Custom, not the first preset whose own values happen to
    /// hold.
    #[test]
    fn a_changed_picture_setting_is_custom() {
        for (p, preset) in PRESETS.iter().enumerate() {
            let mut s = Settings::default();
            preset.apply(&mut s);
            assert!(preset.holds(&s), "{} holds once set", preset.name);
            s.gamma = 200;
            assert_eq!(current_preset(&s), None, "{}, then gamma", preset.name);
            preset.apply(&mut s);
            assert_eq!(s.gamma, 155, "{} sets its whole picture", preset.name);
            assert!(
                current_preset(&s).is_some_and(|q| PRESETS[q].holds(&s)),
                "{p}"
            );
        }
        let mut s = Settings::default();
        PRESETS[1].apply(&mut s);
        PRESETS[2].apply(&mut s);
        assert_eq!(
            current_preset(&s),
            Some(2),
            "Performance after Crisp Pixels"
        );
    }

    /// The Preset list is Default, Crisp Pixels and Performance, no two the
    /// same picture. Only the values are saved, never the preset, so a
    /// settings.toml written under the old Smooth preset (Fit, smooth
    /// animations, the finest light: all Default's) reads Default.
    #[test]
    fn the_presets_are_default_crisp_pixels_and_performance() {
        let names: Vec<_> = PRESETS.iter().map(|p| p.name).collect();
        assert_eq!(names, ["Default", "Crisp Pixels", "Performance"]);
        for (i, p) in PRESETS.iter().enumerate() {
            let mut s = Settings::default();
            p.apply(&mut s);
            assert_eq!(current_preset(&s), Some(i), "{}", p.name);
        }
        let mut was_smooth = Settings::default();
        for (id, v) in [
            ("video.scale_mode", 0),
            ("video.animation_frames", 1),
            ("video.light_quality", 2),
            ("video.blended_shadows", 1),
        ] {
            entry(id).unwrap().store(&mut was_smooth, v);
        }
        let back = parse_settings(&write_settings(&was_smooth)).expect("parses");
        assert_eq!(
            current_preset(&back),
            Some(0),
            "the old Smooth reads Default"
        );
    }

    /// Every setting a preset names is one Default names (and puts back).
    #[test]
    fn default_names_every_preset_setting() {
        for p in PRESETS {
            for &(id, _) in p.values {
                assert!(entry(id).is_some(), "{id} is a row");
                assert!(
                    PRESETS[0].values.iter().any(|&(d, _)| d == id),
                    "{}: Default does not name {id}",
                    p.name
                );
            }
        }
    }

    /// The Controls page has one on/off row per item-moving shortcut, each
    /// on by default and switching only its own shortcut.
    #[test]
    fn each_item_shortcut_has_its_own_controls_row() {
        use crate::ui::item_moves::Shortcut;
        let d = Settings::default();
        for sc in Shortcut::ALL {
            let id = format!("controls.{}", sc.key());
            let e = entry(&id).unwrap_or_else(|| panic!("no row {id}"));
            assert_eq!(e.category, Category::Controls, "{id}");
            assert_eq!(e.kind, Kind::Toggle, "{id}");
            assert_eq!(e.group, "Item Shortcuts", "{id}");
            assert_eq!(e.label, sc.label(), "{id}");
            assert_eq!(e.value(&d), 1, "{id} is on by default");
            let mut s = d;
            e.store(&mut s, 0);
            assert!(!s.item_shortcuts.has(sc), "{id}");
            assert!(Shortcut::ALL
                .iter()
                .filter(|&&o| o != sc)
                .all(|&o| s.item_shortcuts.has(o)));
            let back = parse_settings(&write_settings(&s)).expect("parses");
            assert_eq!(back.item_shortcuts, s.item_shortcuts, "{id}");
        }
    }
}
