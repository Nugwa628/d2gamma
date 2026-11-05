//! Native sprite preview. Only decoded project/native files are read.
use anyhow::{Context, Result};
use d2_native::kind::{FileStore, NativeKind, ViewPalette};
use std::path::{Path, PathBuf};
pub struct Store {
    pub roots: Vec<PathBuf>,
}
impl FileStore for Store {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        for root in &self.roots {
            let p = root.join(path.replace('\\', "/"));
            if let Ok(m) = p.metadata() {
                if m.len() > 64 * 1024 * 1024 {
                    return None;
                }
                if let Ok(b) = std::fs::read(p) {
                    return Some(b);
                }
            }
        }
        None
    }
}
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub pixels: Vec<u8>,
}
pub fn frames(project: &Path, native: &Path, path: &str) -> Result<Vec<Frame>> {
    let path = path.replace('\\', "/").to_ascii_lowercase();
    if path.contains(':')
        || !path.is_ascii()
        || !path.starts_with("data/")
        || path
            .split('/')
            .any(|p| p.is_empty() || p == ".." || p == ".")
    {
        anyhow::bail!("Invalid sprite path");
    }
    let store = Store {
        roots: vec![project.join("files"), native.join("base")],
    };
    if path.ends_with(".dcc") {
        let d = d2_formats::dcc::Dcc::read(&path, &store)?;
        let d = d.directions.first().context("No directions")?;
        Ok(d.frames
            .iter()
            .take(256)
            .map(|f| Frame {
                width: f.width,
                height: f.height,
                x: f.x_min,
                y: f.y_min,
                pixels: f.pixels.clone(),
            })
            .collect())
    } else {
        let d = d2_formats::dc6::Dc6::read(&path, &store)?;
        Ok(d.frames
            .into_iter()
            .take(d.header.frames_per_direction.min(256) as usize)
            .map(|f| Frame {
                width: f.width,
                height: f.height,
                x: f.offset_x,
                y: f.offset_y,
                pixels: f.pixels,
            })
            .collect())
    }
}
pub fn color(frame: &Frame, palette: &ViewPalette) -> eframe::egui::ColorImage {
    let rgba: Vec<u8> = frame
        .pixels
        .iter()
        .flat_map(|&i| {
            let c = palette[usize::from(i)];
            [c.r, c.g, c.b, if i == 0 { 0 } else { 255 }]
        })
        .collect();
    eframe::egui::ColorImage::from_rgba_unmultiplied(
        [frame.width as usize, frame.height as usize],
        &rgba,
    )
}
