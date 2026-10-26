//! Every image the render world takes has pixels of its own size.
//!
//! The render world uploads an image with its pixels through `wgpu`'s
//! `create_texture_with_data`, which reads as many bytes as the image's
//! size asks for and panics (on a render thread, so the app dies) when the
//! pixels are fewer: a picture of 800 × 600 put in an image made 16:9, say.
//! The writers keep the two together; this is the net under them. Each
//! image added or changed this frame is checked before it is extracted,
//! and one whose pixels do not fit its size gets blank pixels of its size
//! (it shows nothing for a frame) and a warning, never a panic.

use bevy::image::TextureFormatPixelInfo;
use bevy::prelude::*;

/// The pixel bytes `image`'s size asks for, when they are known: one mip
/// level of an uncompressed format (the app's pictures; others are left
/// alone).
pub fn want_len(image: &Image) -> Option<usize> {
    let d = &image.texture_descriptor;
    if d.mip_level_count != 1 || d.format.block_dimensions() != (1, 1) {
        return None;
    }
    let px = d.format.pixel_size().ok()?;
    let s = d.size;
    Some(s.width as usize * s.height as usize * s.depth_or_array_layers as usize * px)
}

/// `(have, want)` bytes when `image`'s pixels do not fit its size; `None`
/// when they do, or it has none (a render target the GPU fills).
pub fn mismatch(image: &Image) -> Option<(usize, usize)> {
    let have = image.data.as_ref()?.len();
    let want = want_len(image)?;
    (have != want).then_some((have, want))
}

/// Blanks the pixels of each image added or changed this frame whose
/// pixels do not fit its size (module docs). Runs after the frame's
/// writers, before extraction.
pub fn keep_image_sizes(
    mut events: MessageReader<AssetEvent<Image>>,
    mut images: ResMut<Assets<Image>>,
) {
    for ev in events.read() {
        let (AssetEvent::Added { id } | AssetEvent::Modified { id }) = *ev else {
            continue;
        };
        let Some((have, want)) = images.get(id).and_then(mismatch) else {
            continue;
        };
        if let Some(mut image) = images.get_mut(id) {
            warn!(
                "image {id:?}: {have} pixel bytes for a {} x {} image; blanked",
                image.width(),
                image.height()
            );
            image.data = Some(vec![0; want]);
        }
    }
}

/// Adds [`keep_image_sizes`] (once).
pub fn add(app: &mut App) {
    #[derive(Resource)]
    struct Added;
    if app.world().contains_resource::<Added>() {
        return;
    }
    app.insert_resource(Added)
        .add_systems(Last, keep_image_sizes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    fn rgba(w: u32, h: u32) -> Image {
        Image::new(
            Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![7; (w * h * 4) as usize],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        )
    }

    /// The owner's crash: the front end's picture image made 16:9 and given
    /// an 800 × 600 picture. The net blanks it to its size; a fitting
    /// image and a GPU-filled target are left alone.
    #[test]
    fn an_image_whose_pixels_do_not_fit_its_size_is_blanked_to_it() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<Image>();
        add(&mut app);
        add(&mut app);
        let (bad, good, target) = {
            let mut images = app.world_mut().resource_mut::<Assets<Image>>();
            let mut target = Image::new_target_texture(64, 32, TextureFormat::Rgba8Unorm, None);
            target.data = None;
            (
                images.add(rgba(800, 600)),
                images.add(rgba(1068, 600)),
                images.add(target),
            )
        };
        app.update();
        {
            let mut images = app.world_mut().resource_mut::<Assets<Image>>();
            let mut image = images.get_mut(&bad).unwrap();
            image.texture_descriptor.size.width = 1068;
            assert_eq!(mismatch(&image), Some((1_920_000, 2_563_200)));
        }
        app.update();
        let images = app.world().resource::<Assets<Image>>();
        let bad = images.get(&bad).unwrap();
        assert_eq!(mismatch(bad), None);
        assert_eq!(bad.data.as_ref().map(Vec::len), Some(2_563_200));
        assert!(bad.data.as_ref().unwrap().iter().all(|&b| b == 0));
        let good = images.get(&good).unwrap();
        assert!(good.data.as_ref().unwrap().iter().all(|&b| b == 7));
        assert!(images.get(&target).unwrap().data.is_none());
    }
}
