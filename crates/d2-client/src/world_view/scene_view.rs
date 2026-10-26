//! The scene view: a game drawn as a backdrop, not played. While [`SceneView`] is
//! present the world view draws the world alone: no UI (the HUD, labels,
//! overhead text and markers), no players or monsters
//! ([`crate::bridge::world::ClientWorld::actors_hidden`]), and the world
//! under the HUD rows too. It takes no input, sets no window cursor and
//! presents nothing itself (its camera is off): the front end draws
//! [`super::present::WorldViewTarget`]'s picture under its own screen.
//! It draws one frame per server tick, and none while paused.

use bevy::prelude::*;

use crate::bridge::BridgeResource;
use crate::scene::DrawItem;

/// A game shown as a scene ([`self`]).
#[derive(Resource, Debug, Default)]
pub struct SceneView {
    /// The server tick of the last frame drawn.
    pub drawn_tick: u64,
    /// No frames and no ticks: the window is not focused, or the screen
    /// that shows the scene is not up.
    pub paused: bool,
    /// No frames, the ticks go on: the scene is being set up (the warp,
    /// the camera's moves), and the picture drawn last stays.
    pub hold: bool,
    /// Frames drawn, for the frame cost's log.
    pub frames: u64,
}

/// A run condition: no scene view.
pub fn not_scene(scene: Option<Res<SceneView>>) -> bool {
    scene.is_none()
}

/// A run condition for the world view's frame: always without a scene
/// view; with one, only when it is neither paused nor held and a server
/// tick came since the last frame drawn (the scene's frame rate is the
/// tick rate).
pub fn frame_due(scene: Option<Res<SceneView>>, bridge: Option<Res<BridgeResource>>) -> bool {
    let Some(scene) = scene else {
        return true;
    };
    let tick = bridge.map_or(0, |b| b.0.world().server_ticks);
    !scene.paused && !scene.hold && (tick == 0 || tick != scene.drawn_tick)
}

/// What a scene shows of a frame's `items`: every item but the UI's.
pub fn keep_world(items: &mut Vec<DrawItem>) {
    items.retain(|i| !super::native_ui::is_ui(i));
}

/// The game's present camera is off while a scene view is up (the front
/// end shows the picture), on otherwise.
pub fn camera(
    scene: Option<Res<SceneView>>,
    mut cameras: Query<&mut Camera, With<super::post::WorldViewCamera>>,
) {
    let on = scene.is_none();
    for mut c in &mut cameras {
        if c.is_active != on {
            c.is_active = on;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{DrawKey, FrameId, ItemTag};

    fn item(pass: u32, tag: ItemTag) -> DrawItem {
        let mut i = DrawItem::new(FrameId(0), 0, 0);
        i.key = DrawKey::new(pass, 0, 0, 0).unwrap();
        i.tag = tag;
        i
    }

    #[test]
    fn a_scene_keeps_the_world_and_drops_the_ui() {
        let ui = crate::scene::order::pass::UI;
        let mut items = vec![
            item(0, ItemTag::Tile { x: 1, y: 2 }),
            item(1, ItemTag::Unit(7)),
            item(ui, ItemTag::None),
            item(1, ItemTag::Ui(3)),
        ];
        keep_world(&mut items);
        assert_eq!(
            items.iter().map(|i| i.tag).collect::<Vec<_>>(),
            vec![ItemTag::Tile { x: 1, y: 2 }, ItemTag::Unit(7)]
        );
    }

    #[test]
    fn a_scene_draws_once_per_tick_and_not_while_paused_or_held() {
        let mut app = App::new();
        app.insert_resource(SceneView {
            drawn_tick: 5,
            ..default()
        });
        let due = |app: &mut App| app.world_mut().run_system_cached(frame_due).unwrap();
        // No bridge: tick 0, the start.
        assert!(due(&mut app));
        app.world_mut().resource_mut::<SceneView>().paused = true;
        assert!(!due(&mut app));
        app.world_mut().resource_mut::<SceneView>().paused = false;
        app.world_mut().resource_mut::<SceneView>().hold = true;
        assert!(!due(&mut app), "held while the scene is set up");
        app.world_mut().remove_resource::<SceneView>();
        assert!(due(&mut app), "a game draws every pass");
    }
}
