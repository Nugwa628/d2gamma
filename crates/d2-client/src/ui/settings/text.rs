// D2RUST's own: the text of the settings screens.
//! How the settings menu, the Ctrl+O overlay and the FPS counter draw a
//! string: by its role, never by a font name.
//!
//! The roles are those of the screens' layout ([`TextRole`]): `Title` (the
//! menu's heading), `Label` (row labels, tabs, values, key caps), `Body`
//! (descriptions and long text) and `Button` (the words on the
//! stone-coloured buttons). The text images ([`TextImages`],
//! `world_view::ui_art::text_image`) pick the font for each and follow the
//! HD Text setting live: on, D2RUST's shipped outline font; off, the game's
//! own bitmap fonts as the game uses them (Font30, Font16, FontFormal10,
//! and the engraved FontExocet10 on buttons).
//!
//! Every string is drawn as an image at the window's physical pixels
//! ([`TextImage`]) and shown as a UI image node of exactly that many
//! pixels, so nothing is scaled after it is drawn. The game and the main
//! menu install the text images over the player's game files
//! ([`MenuTextSource`], [`install_source`]). Only an app without them (a
//! test) draws with the UI's built-in font, at the role's size.
//!
//! Sizes are in view units: one unit is one pixel of the 1280 x 720 game
//! view ([`super::Metrics`]), so 3 output pixels at 2160p, 2 at 1440p and
//! 1.5 at 1080p.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub use crate::world_view::ui_art::text_image::{TextImage, TextImages, TextRole};

use super::Metrics;

/// The role's line size in view units, the size the UI's built-in font
/// draws it at when there is no source.
pub fn units(role: TextRole) -> f32 {
    role.hd_em()
}

/// A source of the screens' text.
pub trait MenuText: Send + Sync {
    /// `text` in `role` at `scale` output pixels per view unit, coloured
    /// `color` (sRGB; a role whose font has its own colours may ignore
    /// it). `None`: no font for this role.
    fn render(&self, role: TextRole, text: &str, scale: f32, color: [u8; 3]) -> Option<TextImage>;

    /// The width [`Self::render`] would give, in output pixels.
    fn measure(&self, role: TextRole, text: &str, scale: f32) -> Option<f32>;

    /// Changes whenever the same call would draw differently (the HD Text
    /// setting flipped): drawn strings are drawn again.
    fn revision(&self) -> u64 {
        0
    }
}

/// The text images draw the screens' text; their look follows HD Text, so
/// the revision is the setting.
impl MenuText for TextImages {
    fn render(&self, role: TextRole, text: &str, scale: f32, color: [u8; 3]) -> Option<TextImage> {
        TextImages::render(self, role, text, scale, color)
    }

    fn measure(&self, role: TextRole, text: &str, scale: f32) -> Option<f32> {
        TextImages::measure(self, role, text, scale)
    }

    fn revision(&self) -> u64 {
        u64::from(crate::world_view::ui_art::hd_text::hd_fonts())
    }
}

/// The installed text source.
#[derive(Resource, Clone)]
pub struct MenuTextSource(pub Arc<dyn MenuText>);

impl MenuTextSource {
    /// The text images over the player's game files `source`.
    pub fn from_files(source: Arc<dyn crate::assets::path::FileSource>) -> Self {
        MenuTextSource(Arc::new(TextImages::new(source)))
    }
}

/// Installs the text images when there are none yet and the main menu's
/// art knows the game files (the game installs them with its UI,
/// `app::ui::add_original_ui_with`).
pub fn install_source(
    mut commands: Commands,
    installed: Option<Res<MenuTextSource>>,
    host: Option<NonSend<crate::app::front_host::FrontHost>>,
) {
    if installed.is_some() {
        return;
    }
    if let Some(source) = host.and_then(|h| h.art.as_ref().map(|a| a.source())) {
        commands.insert_resource(MenuTextSource::from_files(source));
    }
}

/// The source in force (the built-in font's without one) and the key its
/// drawings depend on: whether one is installed, and its revision. A screen
/// draws again when the key changes.
pub fn current(installed: Option<&MenuTextSource>) -> (&dyn MenuText, u64) {
    match installed {
        Some(s) => (s.0.as_ref(), 1 + 2 * s.0.revision()),
        None => (&BuiltInText, 0),
    }
}

/// The source of an app without game files (tests): no font for any
/// role, so every string is drawn by the UI's built-in font.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltInText;

impl MenuText for BuiltInText {
    fn render(&self, _: TextRole, _: &str, _: f32, _: [u8; 3]) -> Option<TextImage> {
        None
    }

    fn measure(&self, _: TextRole, _: &str, _: f32) -> Option<f32> {
        None
    }
}

/// One drawn string, kept while the same call can reuse it.
#[derive(Clone, Debug)]
struct Drawn {
    image: Handle<Image>,
    width: u32,
    height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct DrawnKey {
    role: TextRole,
    text: String,
    scale_bits: u32,
    color: [u8; 3],
}

/// Strings drawn so far, by role, text, scale and colour. Emptied when
/// the source's revision changes or it grows past [`TextCache::MAX`].
#[derive(Resource, Default)]
pub struct TextCache {
    revision: Option<u64>,
    drawn: HashMap<DrawnKey, Drawn>,
}

impl TextCache {
    /// Strings kept at most; a screen shows far fewer.
    pub const MAX: usize = 512;

    fn get(
        &mut self,
        images: &mut Assets<Image>,
        source: &dyn MenuText,
        key: DrawnKey,
    ) -> Option<Drawn> {
        let revision = source.revision();
        if self.revision != Some(revision) || self.drawn.len() >= Self::MAX {
            self.drawn.clear();
            self.revision = Some(revision);
        }
        if let Some(d) = self.drawn.get(&key) {
            return Some(d.clone());
        }
        let scale = f32::from_bits(key.scale_bits);
        let t = source.render(key.role, &key.text, scale, key.color)?;
        let d = Drawn {
            image: images.add(text_image(&t)?),
            width: t.width,
            height: t.height,
        };
        self.drawn.insert(key, d.clone());
        Some(d)
    }
}

/// The image of a drawn string; `None` for an empty or malformed one.
fn text_image(t: &TextImage) -> Option<Image> {
    let len = (t.width as usize) * (t.height as usize) * 4;
    if t.width == 0 || t.height == 0 || t.rgba.len() != len {
        return None;
    }
    Some(Image::new(
        Extent3d {
            width: t.width,
            height: t.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        t.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ))
}

/// What the screens draw text with: the source, the drawn strings and
/// the window's metrics.
pub struct TextCtx<'a> {
    pub source: &'a dyn MenuText,
    pub cache: &'a mut TextCache,
    pub images: &'a mut Assets<Image>,
    pub metrics: Metrics,
}

impl TextCtx<'_> {
    /// Spawns `text` in `role` and `color` as a child of `p`: an image
    /// node of the drawn string, or the built-in font's text at the role's
    /// size.
    pub fn spawn(
        &mut self,
        p: &mut ChildSpawnerCommands,
        role: TextRole,
        text: &str,
        color: [u8; 3],
    ) -> Entity {
        if text.is_empty() {
            return p.spawn(Node::default()).id();
        }
        let key = DrawnKey {
            role,
            text: text.to_string(),
            scale_bits: self.metrics.scale.to_bits(),
            color,
        };
        let sf = self.metrics.scale_factor.max(f32::EPSILON);
        match self.cache.get(self.images, self.source, key) {
            Some(d) => p
                .spawn((
                    ImageNode::new(d.image),
                    Node {
                        width: Val::Px(d.width as f32 / sf),
                        height: Val::Px(d.height as f32 / sf),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ))
                .id(),
            None => p
                .spawn((
                    Text::new(text),
                    TextFont {
                        font_size: bevy::text::FontSize::Px(units(role) * self.metrics.scale / sf),
                        ..default()
                    },
                    TextColor(Color::srgb_u8(color[0], color[1], color[2])),
                    Node {
                        flex_shrink: 0.0,
                        ..default()
                    },
                ))
                .id(),
        }
    }

    /// The width of `text` in `role`, in view units: the source's measure,
    /// or an estimate for the built-in font.
    pub fn measure(&self, role: TextRole, text: &str) -> f32 {
        let scale = self.metrics.scale.max(f32::EPSILON);
        self.source
            .measure(role, text, scale)
            .map(|w| w / scale)
            .unwrap_or_else(|| estimate(role, text))
    }
}

/// The built-in font's width of `text` in view units: a monospaced font
/// whose advance is 0.6 of its size.
pub fn estimate(role: TextRole, text: &str) -> f32 {
    text.chars().count() as f32 * units(role) * 0.6
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source that draws every string as a solid block, one output pixel
    /// per character per unit of scale, and counts its draws.
    #[derive(Default)]
    struct Blocks {
        draws: std::sync::atomic::AtomicUsize,
        revision: std::sync::atomic::AtomicU64,
    }

    impl MenuText for Blocks {
        fn render(&self, _: TextRole, text: &str, scale: f32, c: [u8; 3]) -> Option<TextImage> {
            self.draws
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let (w, h) = ((text.len() as f32 * scale) as u32, (10.0 * scale) as u32);
            Some(TextImage {
                width: w,
                height: h,
                baseline: h,
                rgba: [c[0], c[1], c[2], 255].repeat((w * h) as usize),
            })
        }
        fn measure(&self, _: TextRole, text: &str, scale: f32) -> Option<f32> {
            Some(text.len() as f32 * scale)
        }
        fn revision(&self) -> u64 {
            self.revision.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    fn key(text: &str, scale: f32) -> DrawnKey {
        DrawnKey {
            role: TextRole::Label,
            text: text.into(),
            scale_bits: scale.to_bits(),
            color: [1, 2, 3],
        }
    }

    #[test]
    fn a_string_is_drawn_once_until_the_source_changes() {
        let src = Blocks::default();
        let mut images = Assets::<Image>::default();
        let mut cache = TextCache::default();
        let a = cache.get(&mut images, &src, key("Gamma", 3.0)).unwrap();
        let b = cache.get(&mut images, &src, key("Gamma", 3.0)).unwrap();
        assert_eq!(a.image, b.image);
        assert_eq!((a.width, a.height), (15, 30));
        assert_eq!(src.draws.load(std::sync::atomic::Ordering::Relaxed), 1);
        // Another scale is another string.
        cache.get(&mut images, &src, key("Gamma", 2.0)).unwrap();
        assert_eq!(src.draws.load(std::sync::atomic::Ordering::Relaxed), 2);
        // HD Text flipped: the same call draws again.
        src.revision.store(1, std::sync::atomic::Ordering::Relaxed);
        cache.get(&mut images, &src, key("Gamma", 3.0)).unwrap();
        assert_eq!(src.draws.load(std::sync::atomic::Ordering::Relaxed), 3);
    }

    #[test]
    fn without_a_font_the_string_falls_back_to_the_built_in_font() {
        let mut images = Assets::<Image>::default();
        let mut cache = TextCache::default();
        assert!(cache
            .get(&mut images, &BuiltInText, key("Gamma", 3.0))
            .is_none());
        let ctx = TextCtx {
            source: &BuiltInText,
            cache: &mut cache,
            images: &mut images,
            metrics: Metrics {
                scale: 3.0,
                scale_factor: 1.0,
            },
        };
        assert_eq!(
            ctx.measure(TextRole::Label, "abc"),
            estimate(TextRole::Label, "abc")
        );
    }

    #[test]
    fn a_malformed_drawing_is_no_image() {
        let t = TextImage {
            width: 2,
            height: 2,
            baseline: 2,
            rgba: vec![0; 15],
        };
        assert!(text_image(&t).is_none());
        let t = TextImage {
            rgba: vec![0; 16],
            ..t
        };
        assert!(text_image(&t).is_some());
    }
}
