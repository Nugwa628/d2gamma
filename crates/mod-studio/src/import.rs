//! Offline conversions into the native formats the engine already reads.
use anyhow::{bail, Context, Result};
use d2_formats::dc6::{Dc6, Dc6Frame, Dc6Header};
use d2_formats::wav::Wav;
use d2_native::kind::{MemStore, NativeFile, NativeKind, ViewPalette};
use std::collections::BTreeMap;
use std::path::Path;

pub struct ArtImport {
    pub files: Vec<NativeFile>,
    pub sprite: Dc6,
}
pub fn sprite(
    bytes: &[u8],
    destination: &str,
    frame: (u32, u32),
    offset: (i32, i32),
    palette: &ViewPalette,
) -> Result<ArtImport> {
    let path = destination_path(destination, "dc6")?;
    if bytes.len() > 64 * 1024 * 1024 {
        bail!("PNG exceeds 64 MiB");
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode()?.to_rgba8();
    let (w, h) = image.dimensions();
    let (fw, fh) = frame;
    if fw == 0
        || fh == 0
        || w % fw != 0
        || h % fh != 0
        || w / fw > 256
        || h / fh > 32
        || u64::from(w) * u64::from(h) > 16 * 1024 * 1024
    {
        bail!(
            "Use a grid of 1..256 frames across, 1..32 directions down, at most 16 million pixels"
        );
    }
    let mut cache = BTreeMap::new();
    let mut frames = Vec::new();
    for direction in 0..h / fh {
        for frame in 0..w / fw {
            let mut pixels = Vec::with_capacity((fw * fh) as usize);
            for y in 0..fh {
                for x in 0..fw {
                    let p = image.get_pixel(frame * fw + x, direction * fh + y).0;
                    if p[3] < 128 {
                        pixels.push(0);
                        continue;
                    }
                    let rgb = [p[0], p[1], p[2]];
                    let index = *cache.entry(rgb).or_insert_with(|| {
                        (1..=255usize)
                            .min_by_key(|&i| {
                                let c = palette[i];
                                let d = |a: u8, b: u8| {
                                    let d = i32::from(a) - i32::from(b);
                                    d * d
                                };
                                d(c.r, p[0]) + d(c.g, p[1]) + d(c.b, p[2])
                            })
                            .unwrap() as u8
                    });
                    pixels.push(index);
                    if cache.len() > 65536 {
                        cache.clear();
                    }
                }
            }
            frames.push(Dc6Frame {
                flip: 0,
                width: fw,
                height: fh,
                offset_x: offset.0,
                offset_y: offset.1,
                unknown: 0,
                next_block: 0,
                pixels,
            });
        }
    }
    let sprite = Dc6 {
        header: Dc6Header {
            version: 6,
            flags: 1,
            encoding: 0,
            termination: [0xee; 4],
            directions: h / fh,
            frames_per_direction: w / fw,
        },
        frames,
    };
    let files = sprite.write(&path, palette)?;
    let read = Dc6::read(&path, &MemStore::from_files(&files))?;
    if let Some(diff) = sprite.first_difference(&read, &path) {
        bail!("Native sprite read-back: {diff}");
    }
    Ok(ArtImport { files, sprite })
}

pub fn palette(bytes: Vec<u8>) -> Result<ViewPalette> {
    let store = MemStore(BTreeMap::from([("import.pal".into(), bytes)]));
    Ok(d2_formats::palette::Palette::read("import", &store)?.colors)
}

/// PCM 8/16/24/32 and IEEE float32 RIFF input; pad bytes are consumed here.
/// The output is always canonical 16-bit PCM at the engine's 22,050 Hz.
pub fn sound(bytes: &[u8], destination: &str) -> Result<(NativeFile, Wav)> {
    let path = destination_path(destination, "wav")?;
    if bytes.len() > 64 * 1024 * 1024
        || bytes.len() < 12
        || &bytes[..4] != b"RIFF"
        || &bytes[8..12] != b"WAVE"
    {
        bail!("Expected RIFF/WAVE up to 64 MiB");
    }
    let u16_at = |b: &[u8], i: usize| -> Result<u16> {
        Ok(u16::from_le_bytes(
            b.get(i..i + 2).context("Truncated WAV field")?.try_into()?,
        ))
    };
    let u32_at = |b: &[u8], i: usize| -> Result<u32> {
        Ok(u32::from_le_bytes(
            b.get(i..i + 4).context("Truncated WAV field")?.try_into()?,
        ))
    };
    let end = usize::try_from(u32_at(bytes, 4)?)?
        .checked_add(8)
        .context("WAV size overflow")?;
    if end > bytes.len() || end < 12 {
        bail!("Truncated RIFF size");
    }
    let (mut fmt, mut data) = (None, None);
    let mut at = 12usize;
    while at < end {
        let head = bytes
            .get(at..at + 8)
            .filter(|_| at + 8 <= end)
            .context("Truncated WAV chunk")?;
        let len = usize::try_from(u32_at(head, 4)?)?;
        let start = at + 8;
        let stop = start
            .checked_add(len)
            .filter(|n| *n <= end)
            .context("Truncated WAV chunk body")?;
        match &head[..4] {
            b"fmt " if fmt.replace(&bytes[start..stop]).is_some() => bail!("Repeated fmt chunk"),
            b"data" if data.replace(&bytes[start..stop]).is_some() => bail!("Repeated data chunk"),
            _ => {}
        }
        at = stop
            .checked_add(len & 1)
            .filter(|n| *n <= end)
            .context("Missing RIFF pad byte")?;
    }
    let fmt = fmt.context("Missing fmt chunk")?;
    let data = data.context("Missing data chunk")?;
    let tag = u16_at(fmt, 0)?;
    let channels = u16_at(fmt, 2)?;
    let rate = u32_at(fmt, 4)?;
    let align = u16_at(fmt, 12)?;
    let bits = u16_at(fmt, 14)?;
    if !(1..=2).contains(&channels)
        || !(8000..=192000).contains(&rate)
        || !((tag == 1 && [8, 16, 24, 32].contains(&bits)) || (tag == 3 && bits == 32))
        || align != channels * (bits / 8)
        || data.len() % usize::from(align) != 0
    {
        bail!("Use mono/stereo PCM 8/16/24/32 or float32 WAV at 8..192 kHz");
    }
    let mut input = Vec::new();
    for b in data.chunks_exact(usize::from(bits / 8)) {
        let value = match (tag, bits) {
            (3, 32) => {
                let v = f32::from_le_bytes(b.try_into()?);
                if !v.is_finite() {
                    bail!("WAV contains a non-finite sample");
                }
                (v.clamp(-1., 1.) * 32767.).round() as i16
            }
            (_, 8) => (i16::from(b[0]) - 128) * 256,
            (_, 16) => i16::from_le_bytes(b.try_into()?),
            (_, 24) => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 16) as i16,
            (_, 32) => (i32::from_le_bytes(b.try_into()?) >> 16) as i16,
            _ => unreachable!(),
        };
        input.push(value);
    }
    let count = input.len() / usize::from(channels);
    if count == 0 {
        bail!("WAV has no samples");
    }
    let frames = (count as u64 * 22050).div_ceil(u64::from(rate));
    if frames > 22050 * 600 {
        bail!("WAV exceeds ten minutes");
    }
    let mut samples = Vec::with_capacity(frames as usize * usize::from(channels));
    for f in 0..frames {
        let position = f * u64::from(rate);
        let i = (position / 22050) as usize;
        let fraction = (position % 22050) as i64;
        for channel in 0..usize::from(channels) {
            let a = i64::from(input[i.min(count - 1) * usize::from(channels) + channel]);
            let b = i64::from(input[(i + 1).min(count - 1) * usize::from(channels) + channel]);
            samples.push((a + (b - a) * fraction / 22050) as i16);
        }
    }
    let wav = Wav {
        format_tag: 1,
        channels,
        rate: 22050,
        byte_rate: 22050 * u32::from(channels) * 2,
        bits: 16,
        block_align: channels * 2,
        samples,
    };
    let native = d2_native::wav::write_wav(&path, &wav)?;
    let reread = d2_native::wav::read_wav(&path, &native)?;
    if wav != reread {
        bail!("Native WAV read-back differs");
    }
    Ok((
        NativeFile {
            path,
            bytes: native,
        },
        wav,
    ))
}

pub fn destination_path(path: &str, extension: &str) -> Result<String> {
    let p = path.replace('\\', "/").to_ascii_lowercase();
    validate_path(&p)?;
    if !p.ends_with(&format!(".{extension}")) {
        bail!("Use a data/... .{extension} engine path");
    }
    Ok(p)
}
fn validate_path(p: &str) -> Result<()> {
    if p.len() > 240
        || !p.starts_with("data/")
        || !p
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_./- ".contains(&b))
        || p.split('/')
            .any(|c| c.is_empty() || c == "." || c == ".." || c.ends_with(['.', ' ']))
    {
        bail!("Use a relative lowercase data/... engine path");
    }
    Ok(())
}
pub fn write(root: &Path, files: &[NativeFile], replace: bool) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for file in files {
        validate_path(&file.path)?;
        if !seen.insert(&file.path) {
            bail!("Duplicate import destination");
        }
        let mut path = root.to_path_buf();
        for component in std::iter::once("files").chain(file.path.split('/')) {
            path.push(component);
            if let Ok(meta) = std::fs::symlink_metadata(&path) {
                if meta.file_type().is_symlink() {
                    bail!("Import destination contains a symlink: {}", path.display());
                }
            }
        }
        if !replace && path.exists() {
            bail!("Already exists: {}", file.path);
        }
    }
    for file in files {
        let path = root.join("files").join(&file.path);
        std::fs::create_dir_all(path.parent().context("No asset parent")?)?;
        std::fs::write(path, &file.bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png() -> Vec<u8> {
        let mut i = image::RgbaImage::new(4, 2);
        for (y, row) in i.rows_mut().enumerate() {
            for (x, p) in row.enumerate() {
                *p = image::Rgba([
                    if x % 2 == 0 { 255 } else { 0 },
                    0,
                    0,
                    if y == 0 { 255 } else { 0 },
                ]);
            }
        }
        let mut b = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(i)
            .write_to(&mut b, image::ImageFormat::Png)
            .unwrap();
        b.into_inner()
    }
    #[test]
    fn rgba_grid_bakes_transparency_offsets_and_native_sidecars() {
        let mut p = [d2_formats::palette::Rgb::default(); 256];
        p[7] = d2_formats::palette::Rgb { r: 255, g: 0, b: 0 };
        let imported = sprite(
            &png(),
            "data/global/missiles/ember.dc6",
            (2, 1),
            (-2, 3),
            &p,
        )
        .unwrap();
        assert_eq!(
            (
                imported.sprite.header.directions,
                imported.sprite.header.frames_per_direction
            ),
            (2, 2)
        );
        assert_eq!(imported.sprite.frames[0].pixels, [7, 1]);
        assert_eq!(imported.sprite.frames[2].pixels, [0, 0]);
        assert_eq!(imported.sprite.frames[0].offset_x, -2);
        assert_eq!(imported.files.len(), 2);
        let read = Dc6::read(
            "data/global/missiles/ember.dc6",
            &MemStore::from_files(&imported.files),
        )
        .unwrap();
        assert_eq!(read.frames[0].pixels, [7, 1]);
        assert!(sprite(&png(), "../bad.dc6", (2, 1), (0, 0), &p).is_err());
        assert!(sprite(&png(), "data/a.dc6", (3, 1), (0, 0), &p).is_err());
    }
    fn wave(tag: u16, bits: u16, rate: u32, data: &[u8]) -> Vec<u8> {
        let mut b = b"RIFF\0\0\0\0WAVEJUNK\x01\0\0\0\0\0fmt \x10\0\0\0".to_vec();
        b.extend(tag.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(rate.to_le_bytes());
        b.extend((rate * u32::from(bits / 8)).to_le_bytes());
        b.extend((bits / 8).to_le_bytes());
        b.extend(bits.to_le_bytes());
        b.extend(b"data");
        b.extend((data.len() as u32).to_le_bytes());
        b.extend(data);
        if data.len() % 2 == 1 {
            b.push(0);
        }
        let n = (b.len() - 8) as u32;
        b[4..8].copy_from_slice(&n.to_le_bytes());
        b
    }
    #[test]
    fn wave_conversion_bakes_pcm24_float_and_sample_rate() {
        let b = wave(
            1,
            24,
            44100,
            &[0, 0, 0x80, 0xff, 0xff, 0x7f, 0, 0, 0, 0, 0, 0],
        );
        let (f, w) = sound(&b, "data/global/sfx/ember.wav").unwrap();
        assert_eq!((w.rate, w.bits, w.samples.len()), (22050, 16, 2));
        assert_eq!(w.samples[0], i16::MIN);
        assert!(w.is_playable());
        assert_eq!(f.path, "data/global/sfx/ember.wav");
        let data = [0.5f32.to_le_bytes(), (-0.5f32).to_le_bytes()].concat();
        let (_, w) = sound(&wave(3, 32, 22050, &data), "data/a.wav").unwrap();
        assert_eq!(w.samples, [16384, -16384]);
        assert!(sound(&wave(3, 32, 22050, &f32::NAN.to_le_bytes()), "data/a.wav").is_err());
        assert!(sound(&b[..b.len() - 1], "data/a.wav").is_err());
    }
    #[test]
    fn existing_imports_require_explicit_replacement() {
        let root = std::env::temp_dir().join(format!("studio-import-{}", std::process::id()));
        let f = NativeFile {
            path: "data/test.wav".into(),
            bytes: vec![1, 2],
        };
        write(&root, std::slice::from_ref(&f), false).unwrap();
        assert!(write(&root, std::slice::from_ref(&f), false).is_err());
        write(&root, &[f], true).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        for p in ["data/../a.wav", "C:/a.wav", "/data/a.wav", "data//a.wav"] {
            assert!(destination_path(p, "wav").is_err());
        }
    }
    #[test]
    fn imported_sprite_is_readable_by_studio_native_preview() {
        let root =
            std::env::temp_dir().join(format!("sprite-preview-import-{}", std::process::id()));
        let p = d2_native::kind::grey_palette();
        let art = sprite(&png(), "data/global/missiles/test.dc6", (2, 1), (-2, 3), &p).unwrap();
        write(&root, &art.files, false).unwrap();
        let f =
            crate::sprite_preview::frames(&root, &root, "data/global/missiles/test.dc6").unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!((f[0].x, f[0].y), (-2, 3));
        assert_eq!(f[0].pixels, art.sprite.frames[0].pixels);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn import_write_rejects_unsafe_paths_before_any_output() {
        let root = std::env::temp_dir().join(format!("unsafe-import-{}", std::process::id()));
        let files = [
            NativeFile {
                path: "data/good.wav".into(),
                bytes: vec![1],
            },
            NativeFile {
                path: "../escape".into(),
                bytes: vec![2],
            },
        ];
        assert!(write(&root, &files, true).is_err());
        assert!(!root.exists());
    }
    #[test]
    fn baked_art_and_sound_load_through_production_native_source() {
        use d2_native::{
            layers::{Layer, LayerStack},
            manifest::Manifest,
            source::{NativeAsset, NativeSource},
        };
        let root =
            std::env::temp_dir().join(format!("import-runtime-native-{}", std::process::id()));
        let art = sprite(
            &png(),
            "data/global/missiles/test.dc6",
            (2, 1),
            (0, 0),
            &d2_native::kind::grey_palette(),
        )
        .unwrap();
        write(&root, &art.files, false).unwrap();
        let (sound, expected) = sound(
            &wave(1, 8, 11025, &[0, 128, 255]),
            "data/global/sfx/test.wav",
        )
        .unwrap();
        write(&root, &[sound], false).unwrap();
        let source = NativeSource {
            root: root.clone(),
            manifest: Manifest {
                converter_version: "0.0.0".into(),
                converter_commit: String::new(),
                kinds: BTreeMap::new(),
                language: "ENG".into(),
                lod: true,
                archives: Vec::new(),
                counts: BTreeMap::new(),
                unnamed_blocks: BTreeMap::new(),
                complete: true,
                files_sha256: String::new(),
            },
            layers: LayerStack {
                layers: vec![Layer::dir("import", root.join("files"), None)],
            },
        };
        let NativeAsset::Dc6(read) = source
            .read_native("data/global/missiles/test.dc6")
            .unwrap()
            .unwrap()
        else {
            panic!("DC6");
        };
        assert_eq!(read.frames[0].pixels, art.sprite.frames[0].pixels);
        let NativeAsset::Wav(read) = source
            .read_native(&d2_native::source::fold(&d2_data::sounds::sound_path(
                1,
                b"test.wav",
            )))
            .unwrap()
            .unwrap()
        else {
            panic!("WAV");
        };
        assert_eq!(read, expected);
        std::fs::remove_dir_all(root).unwrap();
    }
}
