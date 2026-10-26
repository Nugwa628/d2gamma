// D2RUST's own: UI art made for D2RUST and committed with the client.
//! D2RUST's own UI cels, beside the game's panel art: small images under
//! `crates/d2-client/art/`, made by the generators under `tools/` and
//! named in [`super::panels::UiFiles`] with [`OWN_PREFIX`]. Each value of
//! such an image is an act 1 palette index (0 transparent), so the cel is
//! drawn through the game's palette like a DC6 cel. The panel art loader
//! takes them from here instead of the archives
//! ([`crate::world_view::panel_art::PanelArtLoader`]); they are decoded
//! once, when the loader is made ([`load`]).

use std::sync::LazyLock;

use crate::frames::{FrameAnchor, FrameSet, IndexFrame};

/// The [`super::panels::UiFiles`] prefix of D2RUST's own cels.
pub const OWN_PREFIX: &str = "*d2rust\\";

/// The "Send stackables to stash" icon (`tools/stash-icon`): an
/// iron-banded chest under a gold down arrow, 32 × 32, drawn over the
/// game's 32 px button frame. Frames [`send_frame`].
pub const SEND_STASH: &str = "*d2rust\\sendstash";

/// The frames of [`SEND_STASH`].
pub mod send_frame {
    pub const NORMAL: u32 = 0;
    /// One palette step brighter: the game's buttons have no hover frame.
    pub const HOVER: u32 = 1;
    /// Darker, drawn 1 px down-right on the pressed button frame.
    pub const PRESSED: u32 = 2;
}

/// The side of the [`SEND_STASH`] frames.
pub const SEND_SIDE: u32 = 32;

const SEND_STASH_PNG: &[u8] = include_bytes!("../../art/send-stash.png");

/// The names of the own cels, for [`super::panels::UiFiles::extend`].
pub fn own_files() -> Vec<String> {
    vec![SEND_STASH.to_owned()]
}

/// The decoded own cels by name.
static OWN: LazyLock<Vec<(&'static str, Result<FrameSet, String>)>> = LazyLock::new(|| {
    vec![(
        SEND_STASH,
        frame_strip(SEND_STASH_PNG, SEND_SIDE).map(|frames| FrameSet { frames }),
    )]
});

/// Decodes the own cels (the panel art loader calls it when it is made); a
/// cel that does not decode is logged, and an error again when drawn.
pub fn load() {
    for (name, set) in OWN.iter() {
        if let Err(e) = set {
            bevy::log::error!("own UI art {name}: {e}");
        }
    }
}

/// The frame set of own cel `name`; `None`: not an own cel (read it from
/// the archives).
pub fn frame_set(name: &str) -> Option<Result<FrameSet, String>> {
    if !name.starts_with(OWN_PREFIX) {
        return None;
    }
    Some(OWN.iter().find(|(n, _)| *n == name).map_or_else(
        || Err(format!("{name}: no such own cel")),
        |(_, s)| s.clone(),
    ))
}

/// The frames of an 8-bit greyscale PNG strip of `side`-wide frames side by
/// side, each value a palette index. The frames are bottom-anchored like a
/// DC6 cel: a draw at (X, Y) covers rows Y − side + 1 … Y.
fn frame_strip(png_bytes: &[u8], side: u32) -> Result<Vec<IndexFrame>, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().map_err(|e| format!("PNG: {e}"))?;
    let info = reader.info();
    if info.color_type != png::ColorType::Grayscale || info.bit_depth != png::BitDepth::Eight {
        return Err(format!(
            "PNG is {:?} {:?}, an own cel is 8-bit greyscale",
            info.bit_depth, info.color_type
        ));
    }
    let (w, h) = (info.width, info.height);
    if h != side || w == 0 || w % side != 0 {
        return Err(format!("{w} × {h}, frames are {side} × {side}"));
    }
    let size = reader.output_buffer_size().ok_or("PNG: image too large")?;
    let mut buf = vec![0; size];
    let line = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG: {e}"))?
        .line_size;
    (0..w / side)
        .map(|f| {
            let x0 = (f * side) as usize;
            let pixels = (0..h as usize)
                .flat_map(|y| &buf[y * line + x0..y * line + x0 + side as usize])
                .copied()
                .collect();
            IndexFrame::new(side, side, 0, 0, pixels)
                .map(|fr| fr.with_anchor(FrameAnchor::Bottom))
                .map_err(|e| e.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_send_icon_decodes_to_three_32_px_frames() {
        let set = frame_set(SEND_STASH).unwrap().unwrap();
        assert_eq!(set.frames.len(), 3);
        for f in &set.frames {
            assert_eq!((f.width, f.height, f.anchor), (32, 32, FrameAnchor::Bottom));
            // The art sits in the button's content box; the corners stay
            // transparent so the game's frame shows.
            assert_eq!(f.pixels[0], 0);
            assert_eq!(f.pixels[32 * 32 - 1], 0);
            assert!(f.pixels.iter().filter(|&&p| p != 0).count() > 300);
            assert!(f.pixels.iter().all(|&p| p != 255));
        }
        // Hover and pressed recolour the same pixels.
        let solid = |f: &IndexFrame| f.pixels.iter().map(|&p| p != 0).collect::<Vec<_>>();
        assert_eq!(solid(&set.frames[0]), solid(&set.frames[1]));
        assert_eq!(solid(&set.frames[0]), solid(&set.frames[2]));
        assert_ne!(set.frames[0].pixels, set.frames[1].pixels);
        assert_ne!(set.frames[0].pixels, set.frames[2].pixels);
    }

    #[test]
    fn only_own_names_are_answered() {
        assert!(frame_set("panel\\buysellbtn").is_none());
        assert!(frame_set("*d2rust\\nothing").unwrap().is_err());
        assert_eq!(own_files(), [SEND_STASH]);
    }

    #[test]
    fn a_strip_must_be_8_bit_grey_frames_of_the_side() {
        let mut png_bytes = Vec::new();
        {
            let mut e = png::Encoder::new(&mut png_bytes, 10, 32);
            e.set_color(png::ColorType::Grayscale);
            e.set_depth(png::BitDepth::Eight);
            e.write_header()
                .unwrap()
                .write_image_data(&[0; 320])
                .unwrap();
        }
        assert!(frame_strip(&png_bytes, 32).is_err());
        assert!(frame_strip(b"not a png", 32).is_err());
    }
}
