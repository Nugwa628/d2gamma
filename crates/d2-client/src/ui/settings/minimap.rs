// D2RUST's own: the minimap widget.
//! The minimap (Interface, `[hud] minimap`, `minimap_size`,
//! `minimap_opacity`): the automap around the local player, always on in
//! the top-right corner of the world, drawn at the window's resolution over
//! the game picture (`ui::automap::minimap` draws the map and places it).
//!
//! - A dark panel with a thin gold edge, the menus' style; the map inside
//!   it, each map pixel a whole number of output pixels (sharp, never
//!   blurred).
//! - It hides while the full automap is open, while both side panels are
//!   open, and while any of the game's own UI is drawn where it sits (a
//!   tooltip, the skill list, a menu): it never covers the HUD or a panel.
//!   An open right panel moves it to the left of the panel. Hiding and
//!   showing take a short fade, so a passing tooltip doesn't flicker it.
//! - It never takes the mouse: a click on it goes to the world under it.
//!
//! It only reads the world view's facts of the last drawn frame
//! (`WorldViewState::minimap`) and the automap's cells: the game frame,
//! its hashes and the mouse mapping are untouched.

use std::collections::BTreeMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;

use super::z;
use crate::app::config::ConfigRes;
use crate::frames::{FrameSet, IndexFrame};
use crate::ui::automap::markers::MarkerUnit;
use crate::ui::automap::minimap::{
    origin, picture, picture_side, place, zoom, MinimapCels, PictureInput,
};
use crate::ui::automap::options::{cel_paths, CelFile};
use crate::ui::automap::{TownKind, TreeKind};
use crate::world_view::WorldViewState;

/// The panel's fill and edge (the settings menu's colours).
pub const PANEL: [u8; 3] = [14, 12, 10];
pub const EDGE: [u8; 3] = [107, 87, 51];
/// How opaque the panel's fill is at full opacity: the world stays faintly
/// visible behind the map.
const FILL_ALPHA: f32 = 0.9;
/// Seconds a whole fade in or out takes.
const FADE_S: f32 = 0.15;
/// The panel's corner radius and edge width, view units.
const RADIUS: f32 = 4.0;
const EDGE_W: f32 = 1.0;

/// The automap's full-size cel files, read once each from the game's
/// files.
#[derive(Default)]
struct CelFiles {
    files: BTreeMap<CelFile, Option<FrameSet>>,
}

impl CelFiles {
    fn load(&mut self, file: CelFile, state: &WorldViewState) {
        if self.files.contains_key(&file) {
            return;
        }
        let set = state.automap_view.as_ref().and_then(|v| {
            let (source, expansion) = v.cel_source();
            let path = cel_paths(false, expansion)[file as usize].clone()?;
            let dc6 = crate::assets::path::read_dc6(source.as_ref(), &path)?.ok()?;
            FrameSet::from_dc6(&dc6, 0).ok()
        });
        if set.is_none() {
            warn!("minimap: the automap cel file {file:?} could not be read");
        }
        self.files.insert(file, set);
    }
}

impl MinimapCels for CelFiles {
    fn frame(&self, file: CelFile, cel: i16) -> Option<&IndexFrame> {
        let set = self.files.get(&file)?.as_ref()?;
        set.frames.get(usize::try_from(cel).ok()?)
    }
}

/// What a drawn picture was drawn from: a new picture is drawn only when
/// one of these changes.
#[derive(Clone, Debug, PartialEq)]
struct Drawn {
    origin: (i32, i32),
    side: u32,
    layer: Option<u32>,
    town: TownKind,
    cells: [usize; 4],
    markers: Vec<MarkerUnit>,
    palette: u64,
}

/// The widget's state: the cel files, the picture and how far it is faded
/// in.
#[derive(Resource, Default)]
pub struct MinimapWidget {
    cels: CelFiles,
    image: Option<Handle<Image>>,
    drawn: Option<Drawn>,
    /// 0 hidden to 1 fully shown.
    pub shown: f32,
    /// The nodes' place and alpha as last built.
    built: Option<(Screen, f32)>,
}

/// The widget's root node (the panel).
#[derive(Component)]
pub struct MinimapRoot;

/// The map image inside the panel.
#[derive(Component)]
pub struct MinimapMap;

pub(super) fn register(app: &mut App) {
    app.init_resource::<MinimapWidget>()
        .add_systems(PostUpdate, minimap.before(bevy::ui::UiSystems::Layout));
}

/// Where the widget is on the window, physical pixels: its panel's
/// top-left and side, the map's output pixels per map pixel and the
/// picture's side in map pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    /// Output pixels per view unit.
    pub scale: f32,
    pub left: f32,
    pub top: f32,
    pub side: f32,
    pub zoom: u32,
    pub picture: u32,
}

/// The widget at view place `p` on presentation `pr`: the view's units
/// follow the presented picture (so the widget stays on the world, never on
/// the bars beside it).
pub fn screen(p: &crate::ui::automap::minimap::Place, pr: &crate::ui::Presentation) -> Screen {
    let scale = pr.sx.min(pr.sy);
    let side = (p.side as f32 * scale).round().max(1.0);
    let right = pr.left as f32 + (p.right() as f32 * pr.sx).round();
    let top = pr.top as f32 + (p.top as f32 * pr.sy).round();
    let zoom = zoom(scale);
    Screen {
        scale,
        left: right - side,
        top,
        side,
        zoom,
        picture: picture_side(side as u32, zoom),
    }
}

/// A cheap fingerprint of the act palette: a new act redraws the picture.
fn palette_sum(p: &d2_formats::palette::Palette) -> u64 {
    p.colors.iter().enumerate().fold(0u64, |s, (i, c)| {
        s.wrapping_mul(31)
            .wrapping_add(i as u64 ^ (u64::from(c.r) << 16 | u64::from(c.g) << 8 | u64::from(c.b)))
    })
}

/// Keeps the widget in step with the settings, the world view's last
/// frame and the window.
#[allow(clippy::too_many_arguments)]
fn minimap(
    mut commands: Commands,
    time: Res<Time<Real>>,
    cfg: Option<Res<ConfigRes>>,
    state: Option<Res<WorldViewState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut widget: ResMut<MinimapWidget>,
    roots: Query<Entity, With<MinimapRoot>>,
    mut colours: Query<(&mut BackgroundColor, &mut BorderColor), With<MinimapRoot>>,
    mut maps: Query<&mut ImageNode, With<MinimapMap>>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let settings = cfg.as_ref().map(|c| &c.settings);
    let on = settings.is_some_and(|s| s.minimap != 0);
    let window = windows.single().ok();
    let pr = window.and_then(|w| crate::ui::edge::presentation(w).ok());
    let sf = window.map_or(1.0, |w| w.scale_factor()).max(f32::EPSILON);
    // Where it would sit now, when nothing hides it.
    let target = match (on, state.as_deref(), settings, pr) {
        (true, Some(state), Some(s), Some(pr)) => visible_place(state, s.minimap_size)
            .map(|p| (p, screen(&p, &pr), f32::from(s.minimap_opacity) / 100.0)),
        _ => None,
    };
    let dt = time.delta_secs();
    widget.shown = if target.is_some() {
        (widget.shown + dt / FADE_S).min(1.0)
    } else {
        (widget.shown - dt / FADE_S).max(0.0)
    };
    let (Some((_, at, opacity)), Some(state), Some(mut images)) = (target, state, images) else {
        // Fading out keeps the last picture where it was.
        if widget.shown <= 0.0 || !on {
            widget.shown = 0.0;
            widget.built = None;
            for e in &roots {
                commands.entity(e).despawn();
            }
        } else if let Some((at, alpha)) = widget.built {
            // The opacity it was built with, fading.
            fade(&mut colours, &mut maps, widget.shown * alpha);
            widget.built = Some((at, alpha));
        }
        return;
    };
    let Some(facts) = state.minimap.as_ref() else {
        return;
    };
    let Some(session) = state.automap.as_ref() else {
        return;
    };
    let map = &session.map;
    let town = map.town();
    for file in [CelFile::MaxiMap, CelFile::of_town(town)] {
        widget.cels.load(file, &state);
    }
    let drawn = Drawn {
        origin: origin(facts.player, at.picture, at.picture),
        side: at.picture,
        layer: map.current(),
        town,
        cells: TreeKind::ALL.map(|k| map.cells.tree(k).len()),
        markers: facts.markers.clone(),
        palette: palette_sum(&state.assets.palette),
    };
    if widget.drawn.as_ref() != Some(&drawn) || widget.image.is_none() {
        let pic = picture(&PictureInput {
            cells: &map.cells,
            town,
            player: facts.player,
            markers: &facts.markers,
            palette: &state.assets.palette,
            cels: &widget.cels,
            width: at.picture,
            height: at.picture,
        });
        let mut image = Image::new(
            Extent3d {
                width: pic.width,
                height: pic.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pic.rgba,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = bevy::image::ImageSampler::nearest();
        match widget.image.clone().filter(|h| images.contains(h)) {
            Some(h) => {
                if let Some(mut old) = images.get_mut(&h) {
                    *old = image;
                }
            }
            None => widget.image = Some(images.add(image)),
        }
        widget.drawn = Some(drawn);
    }
    let Some(handle) = widget.image.clone() else {
        return;
    };
    let alpha = widget.shown * opacity;
    let root = roots.iter().next();
    if root.is_some() && widget.built.is_some_and(|(b, o)| b == at && o == opacity) {
        fade(&mut colours, &mut maps, alpha);
        return;
    }
    widget.built = Some((at, opacity));
    let px = |v: f32| Val::Px(v / sf);
    let edge = (EDGE_W * at.scale).round().max(1.0);
    // The map at whole output pixels per map pixel, its middle (the
    // player) on the panel's middle; the panel cuts what reaches past it.
    let inner = at.side - 2.0 * edge;
    let map_px = (at.picture * at.zoom) as f32;
    let off = ((inner - map_px) / 2.0).round();
    let root = root.unwrap_or_else(|| commands.spawn(MinimapRoot).id());
    commands
        .entity(root)
        .despawn_children()
        .insert((
            Node {
                position_type: PositionType::Absolute,
                left: px(at.left),
                top: px(at.top),
                width: px(at.side),
                height: px(at.side),
                border: UiRect::all(px(edge)),
                border_radius: BorderRadius::all(px((RADIUS * at.scale).round())),
                overflow: Overflow::clip(),
                ..default()
            },
            GlobalZIndex(z::MINIMAP),
            panel_colours(alpha),
        ))
        .with_children(|p| {
            p.spawn((
                MinimapMap,
                ImageNode::new(handle).with_color(map_colour(alpha)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(off),
                    top: px(off),
                    width: px(map_px),
                    height: px(map_px),
                    ..default()
                },
            ));
        });
}

/// The panel's fill and edge at `alpha`.
fn panel_colours(alpha: f32) -> (BackgroundColor, BorderColor) {
    let c = |rgb: [u8; 3], a: f32| {
        Color::srgba_u8(rgb[0], rgb[1], rgb[2], (a.clamp(0.0, 1.0) * 255.0) as u8)
    };
    (
        BackgroundColor(c(PANEL, alpha * FILL_ALPHA)),
        BorderColor::all(c(EDGE, alpha)),
    )
}

/// The map image's tint at `alpha`.
fn map_colour(alpha: f32) -> Color {
    Color::srgba(1.0, 1.0, 1.0, alpha.clamp(0.0, 1.0))
}

/// Sets the built nodes' colours to `alpha`.
fn fade(
    colours: &mut Query<(&mut BackgroundColor, &mut BorderColor), With<MinimapRoot>>,
    maps: &mut Query<&mut ImageNode, With<MinimapMap>>,
    alpha: f32,
) {
    let (fill, edge) = panel_colours(alpha);
    for (mut f, mut e) in colours.iter_mut() {
        *f = fill;
        *e = edge;
    }
    for mut m in maps.iter_mut() {
        m.color = map_colour(alpha);
    }
}

/// The widget's place in the view, when the game shows it now: a world is
/// drawn around a local player, the full automap is closed, the side
/// panels leave a corner and no game UI is drawn there.
pub fn visible_place(
    state: &WorldViewState,
    size: u8,
) -> Option<crate::ui::automap::minimap::Place> {
    let facts = state.minimap.as_ref()?;
    let session = state.automap.as_ref()?;
    if session.open || session.map.current().is_none() {
        return None;
    }
    let p = place(crate::ui::ui_space::current(), facts.open_mode, size)?;
    (!facts.ui.covers(&p)).then_some(p)
}
