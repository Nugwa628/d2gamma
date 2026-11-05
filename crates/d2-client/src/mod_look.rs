//! What a kit mod changes in how the game looks when it starts: its window
//! icon and its intro.
//!
//! - **Icon** (`files/mod/icon.png`): the picture the window and the taskbar
//!   show for the mod ([`icon_of`]; `crate::app::window_icon` puts it on the
//!   window). The mod's own `files/mod/icon.ico` is for the launcher's
//!   shortcut and is not read here. A derived mod without an icon of its own
//!   shows the icon of the mod it builds on.
//! - **Intro music** (`files/mod/intro.wav`): plays in the place of the
//!   menu playlist's first track ([`intro_music_of`]; the rest of the
//!   playlist is the game's).
//! - **Intro** (`files/mod/intro.toml`): what the trademark screen at the
//!   start does ([`Intro`]): leave it out, shorten or lengthen it, drop the
//!   logo animation from it, or say its own lines in place of the legal text.
//!   Its pictures (the trademark screen's backdrop, the logo animation, the
//!   main menu's backdrop and every other front-end picture) are replaced
//!   the way every other picture is: the mod ships a file at the same path
//!   in its layer. There are no videos.
//!
//! A mod with neither file starts exactly as D2Gamma does: [`Intro::default`]
//! is today's trademark screen and no window icon is set.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use d2_modkit::ModStack;

use crate::assets::path::FileSource;
use crate::audio::environment::frontend_stream::IntroTrack;
use crate::kit_mod;
use crate::mod_menu::{decode_png, Rgba, MAX_IMAGE_BYTES};

/// The icon file, under the mod folder.
pub const ICON_PATH: &str = "files/mod/icon.png";
/// The intro file, under the mod folder.
pub const INTRO_PATH: &str = "files/mod/intro.toml";

/// The intro music file, under the mod folder: a 16-bit PCM WAV at 22,050 Hz,
/// mono or stereo, as the game's own music is.
pub const INTRO_MUSIC_PATH: &str = "files/mod/intro.wav";
/// The biggest intro music file.
pub const MAX_INTRO_MUSIC_BYTES: u64 = 128 << 20;

/// The trademark screen's length, in seconds, when the mod does not say.
pub const DEFAULT_SECONDS: u32 = crate::ui::front_end::screens::trademark::TIMER_SECONDS;
/// The longest an intro may stay.
pub const MAX_SECONDS: u32 = 60;
/// The most lines of its own text an intro may show, and the longest line.
pub const MAX_TEXT_LINES: usize = 4;
pub const MAX_TEXT_CHARS: usize = 120;

/// What the trademark screen at the start does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intro {
    /// The screen is left out: the game opens on the main menu.
    pub skip: bool,
    /// Whole seconds the screen stays when nobody clicks or presses a key.
    pub seconds: u32,
    /// The logo animation on the screen.
    pub logo: bool,
    /// Lines of the mod's own, in place of the legal text; an empty list
    /// (`text = []`) shows no text. `None`: the legal text of the game.
    pub text: Option<Vec<String>>,
}

impl Default for Intro {
    fn default() -> Intro {
        Intro {
            skip: false,
            seconds: DEFAULT_SECONDS,
            logo: true,
            text: None,
        }
    }
}

impl Intro {
    /// Reads an `intro.toml`. Every problem names its key; a key this build
    /// does not know is a problem too, so a typo is not silently ignored.
    pub fn parse(text: &str) -> Result<Intro, String> {
        let doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("not toml: {e}"))?;
        let mut intro = Intro::default();
        for (key, item) in doc.iter() {
            match key {
                "skip" => intro.skip = flag(key, item)?,
                "logo" => intro.logo = flag(key, item)?,
                "seconds" => {
                    let n = item
                        .as_integer()
                        .ok_or_else(|| format!("`{key}` must be a whole number"))?;
                    intro.seconds = u32::try_from(n)
                        .ok()
                        .filter(|s| (1..=MAX_SECONDS).contains(s))
                        .ok_or_else(|| format!("`{key}` must be from 1 to {MAX_SECONDS}"))?;
                }
                "text" => {
                    let lines = item
                        .as_array()
                        .ok_or_else(|| format!("`{key}` must be a list of lines"))?;
                    if lines.len() > MAX_TEXT_LINES {
                        return Err(format!("`{key}` may have at most {MAX_TEXT_LINES} lines"));
                    }
                    let mut out = Vec::new();
                    for line in lines.iter() {
                        let s = line
                            .as_str()
                            .ok_or_else(|| format!("`{key}` must hold only text"))?;
                        if s.chars().count() > MAX_TEXT_CHARS {
                            return Err(format!(
                                "a line of `{key}` may have at most {MAX_TEXT_CHARS} characters"
                            ));
                        }
                        out.push(s.to_owned());
                    }
                    intro.text = Some(out);
                }
                other => {
                    return Err(format!(
                        "`{other}` is not an intro setting (skip, seconds, logo, text)"
                    ))
                }
            }
        }
        Ok(intro)
    }
}

fn flag(key: &str, item: &toml_edit::Item) -> Result<bool, String> {
    item.as_bool()
        .ok_or_else(|| format!("`{key}` must be true or false"))
}

/// A mod's icon file decoded, when it is a readable PNG within the size
/// limits of mod pictures.
pub fn load_icon(path: &Path) -> Option<Rgba> {
    if std::fs::metadata(path).ok()?.len() > MAX_IMAGE_BYTES {
        return None;
    }
    decode_png(&std::fs::read(path).ok()?)
}

/// The icon of the mod the player picked, or of the nearest mod it builds on
/// that has one.
pub fn icon_of(stack: &ModStack) -> Option<Rgba> {
    stack
        .mods
        .iter()
        .rev()
        .find_map(|m| load_icon(&m.dir.join(ICON_PATH)))
}

/// The intro of the picked mod, or of the nearest mod it builds on that has
/// an `intro.toml`; the default intro when none does. An `intro.toml` that
/// cannot be read is `Err` with the reason (the caller starts with the
/// default intro and says why).
pub fn intro_of(stack: &ModStack) -> Result<Intro, String> {
    for m in stack.mods.iter().rev() {
        let path = m.dir.join(INTRO_PATH);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                return Intro::parse(&text)
                    .map_err(|e| format!("{INTRO_PATH} ({}): {e}", m.manifest.id))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{INTRO_PATH} ({}): {e}", m.manifest.id)),
        }
    }
    Ok(Intro::default())
}

/// The intro music of the picked mod, or of the nearest mod it builds on
/// that has an `intro.wav`; `None` when none does. A file the game cannot
/// play is `Err` with the reason (the caller keeps the game's music).
pub fn intro_music_of(stack: &ModStack) -> Result<Option<PathBuf>, String> {
    for m in stack.mods.iter().rev() {
        let path = m.dir.join(INTRO_MUSIC_PATH);
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        let id = &m.manifest.id;
        if meta.len() > MAX_INTRO_MUSIC_BYTES {
            return Err(format!(
                "{INTRO_MUSIC_PATH} ({id}): more than {} MB",
                MAX_INTRO_MUSIC_BYTES >> 20
            ));
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("{INTRO_MUSIC_PATH} ({id}): {e}"))?;
        let wav = d2_formats::wav::Wav::parse(&bytes)
            .map_err(|e| format!("{INTRO_MUSIC_PATH} ({id}): {e}"))?;
        if !wav.is_playable() {
            return Err(format!(
                "{INTRO_MUSIC_PATH} ({id}): must be 16-bit PCM at 22050 Hz, mono or stereo"
            ));
        }
        return Ok(Some(path));
    }
    Ok(None)
}

/// `source` for the menu music: with this run's mod's intro music in the
/// place of the first track, else `source` itself. An `intro.wav` that
/// cannot be played is noted for the crash log and the game's music plays.
pub fn music_source(source: Arc<dyn FileSource>) -> Arc<dyn FileSource> {
    let Some(stack) = kit_mod::active() else {
        return source;
    };
    match intro_music_of(stack) {
        Ok(Some(wav)) => Arc::new(IntroTrack::new(source, wav)),
        Ok(None) => source,
        Err(e) => {
            crate::launch::note(format!("intro music not used: {e}"));
            source
        }
    }
}

/// This run's intro: the active mod's, else D2Gamma's own. A broken
/// `intro.toml` is noted for the crash log and the default intro is used.
pub fn intro() -> Intro {
    let Some(stack) = kit_mod::active() else {
        return Intro::default();
    };
    intro_of(stack).unwrap_or_else(|e| {
        crate::launch::note(format!("intro not used: {e}"));
        Intro::default()
    })
}

/// This run's window icon: the active mod's, when it has one.
pub fn icon() -> Option<&'static Rgba> {
    static ICON: std::sync::OnceLock<Option<Rgba>> = std::sync::OnceLock::new();
    ICON.get_or_init(|| kit_mod::active().and_then(icon_of))
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("d2g-look-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let px: Vec<u8> = (0..w * h).flat_map(|_| rgba).collect();
        enc.write_header().unwrap().write_image_data(&px).unwrap();
        out
    }

    /// A resolved stack of `ids` in `mods`, the last one picked; each mod
    /// builds on the one before it.
    fn stack(mods: &Path, ids: &[&str]) -> ModStack {
        let mut prev: Option<&str> = None;
        for id in ids {
            let dir = mods.join(id);
            fs::create_dir_all(dir.join("files")).unwrap();
            let base = prev
                .map(|p| format!("[base]\nid = \"{p}\"\nversion = \">=1.0.0\"\n"))
                .unwrap_or_default();
            fs::write(
                dir.join("mod.toml"),
                format!(
                    "id = \"{id}\"\nname = \"{id}\"\nversion = \"1.0.0\"\nauthor = \"A\"\n\
                     description = \"\"\nkind = \"full\"\napi = {}\nallow_derived = true\n{base}",
                    d2_modkit::CURRENT_API
                ),
            )
            .unwrap();
            prev = Some(id);
        }
        d2_modkit::resolve(&d2_modkit::ModSpec::Id(prev.unwrap().to_owned()), mods).unwrap()
    }

    fn put(stack: &ModStack, id: &str, rel: &str, bytes: &[u8]) {
        let m = stack.mods.iter().find(|m| m.manifest.id == id).unwrap();
        let p = m.dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    #[test]
    fn no_intro_file_is_todays_trademark_screen() {
        let intro = Intro::parse("").unwrap();
        assert_eq!(intro, Intro::default());
        assert!(!intro.skip && intro.logo && intro.text.is_none());
        assert_eq!(intro.seconds, 9);
    }

    #[test]
    fn an_intro_says_what_it_changes() {
        let intro = Intro::parse(
            "skip = false\nlogo = false\nseconds = 4\ntext = [\"Ember Age\", \"by Ada\"]\n",
        )
        .unwrap();
        assert_eq!(
            intro,
            Intro {
                skip: false,
                seconds: 4,
                logo: false,
                text: Some(vec!["Ember Age".into(), "by Ada".into()]),
            }
        );
        assert!(Intro::parse("skip = true").unwrap().skip);
        // An empty list is no text at all, not the legal text.
        assert_eq!(Intro::parse("text = []").unwrap().text, Some(Vec::new()));
    }

    #[test]
    fn a_bad_intro_names_the_key() {
        for (text, key) in [
            ("skip = 1", "`skip`"),
            ("seconds = 0", "`seconds`"),
            ("seconds = 61", "`seconds`"),
            ("seconds = \"4\"", "`seconds`"),
            ("logo = \"no\"", "`logo`"),
            ("text = \"one\"", "`text`"),
            ("text = [1]", "`text`"),
            ("text = [\"1\",\"2\",\"3\",\"4\",\"5\"]", "`text`"),
            ("videos = true", "`videos`"),
        ] {
            let e = Intro::parse(text).unwrap_err();
            assert!(e.contains(key), "{text}: {e}");
        }
        let long = format!("text = [\"{}\"]", "x".repeat(MAX_TEXT_CHARS + 1));
        assert!(Intro::parse(&long).unwrap_err().contains("characters"));
        assert!(Intro::parse("seconds = ").unwrap_err().contains("not toml"));
    }

    #[test]
    fn the_icon_is_the_picked_mods_or_the_one_it_builds_on() {
        let mods = temp("icon");
        let s = stack(&mods, &["base-mod", "derived"]);
        assert!(icon_of(&s).is_none());
        put(&s, "base-mod", ICON_PATH, &png(2, 2, [255, 0, 0, 255]));
        let got = icon_of(&s).unwrap();
        assert_eq!((got.w, got.h), (2, 2));
        assert_eq!(&got.rgba[..4], &[255, 0, 0, 255]);
        // The derived mod's own icon wins.
        put(&s, "derived", ICON_PATH, &png(4, 4, [0, 255, 0, 255]));
        let got = icon_of(&s).unwrap();
        assert_eq!((got.w, got.h), (4, 4));
        assert_eq!(&got.rgba[..4], &[0, 255, 0, 255]);
        // A file that is not a PNG is no icon (the base's icon shows).
        put(&s, "derived", ICON_PATH, b"not a png");
        assert_eq!(icon_of(&s).unwrap().w, 2);
        let _ = fs::remove_dir_all(&mods);
    }

    #[test]
    fn an_oversized_icon_is_not_used() {
        let mods = temp("big");
        let s = stack(&mods, &["big-icon"]);
        put(
            &s,
            "big-icon",
            ICON_PATH,
            &png(crate::mod_menu::MAX_IMAGE_SIDE + 1, 1, [1, 2, 3, 255]),
        );
        assert!(icon_of(&s).is_none());
        let _ = fs::remove_dir_all(&mods);
    }

    /// A silent WAV: `frames` frames of 16-bit PCM at `rate`.
    fn wav(rate: u32, frames: usize) -> Vec<u8> {
        let data = vec![0u8; frames * 4];
        let mut b = b"RIFF".to_vec();
        b.extend((36 + data.len() as u32).to_le_bytes());
        b.extend(*b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        for v in [1u16, 2] {
            b.extend(v.to_le_bytes());
        }
        b.extend(rate.to_le_bytes());
        b.extend((rate * 4).to_le_bytes());
        b.extend(4u16.to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(*b"data");
        b.extend((data.len() as u32).to_le_bytes());
        b.extend(data);
        b
    }

    #[test]
    fn the_intro_music_is_the_picked_mods_or_the_one_it_builds_on() {
        let mods = temp("music");
        let s = stack(&mods, &["base-mod", "derived"]);
        assert_eq!(intro_music_of(&s), Ok(None));
        put(&s, "base-mod", INTRO_MUSIC_PATH, &wav(22_050, 100));
        let base_file = s.mods[0].dir.join(INTRO_MUSIC_PATH);
        assert_eq!(intro_music_of(&s), Ok(Some(base_file)));
        put(&s, "derived", INTRO_MUSIC_PATH, &wav(22_050, 50));
        let own_file = s.mods[1].dir.join(INTRO_MUSIC_PATH);
        assert_eq!(intro_music_of(&s), Ok(Some(own_file)));
        let _ = fs::remove_dir_all(&mods);
    }

    #[test]
    fn intro_music_the_game_cannot_play_is_refused_with_its_file_and_mod() {
        let mods = temp("music-bad");
        let s = stack(&mods, &["base-mod", "derived"]);
        put(&s, "base-mod", INTRO_MUSIC_PATH, &wav(22_050, 100));
        for bad in [wav(44_100, 100), b"not a wav".to_vec()] {
            put(&s, "derived", INTRO_MUSIC_PATH, &bad);
            // It does not fall through to the base's file.
            let e = intro_music_of(&s).unwrap_err();
            assert!(
                e.contains("files/mod/intro.wav") && e.contains("derived"),
                "{e}"
            );
        }
        let _ = fs::remove_dir_all(&mods);
    }

    #[test]
    fn the_intro_is_the_picked_mods_or_the_one_it_builds_on() {
        let mods = temp("intro");
        let s = stack(&mods, &["base-mod", "derived"]);
        assert_eq!(intro_of(&s).unwrap(), Intro::default());
        put(&s, "base-mod", INTRO_PATH, b"seconds = 3\n");
        assert_eq!(intro_of(&s).unwrap().seconds, 3);
        put(&s, "derived", INTRO_PATH, b"skip = true\n");
        let got = intro_of(&s).unwrap();
        assert!(got.skip);
        assert_eq!(got.seconds, DEFAULT_SECONDS);
        // A broken file is an error naming the file and the mod, and does not
        // fall through to the base's.
        put(&s, "derived", INTRO_PATH, b"seconds = 0\n");
        let e = intro_of(&s).unwrap_err();
        assert!(
            e.contains("files/mod/intro.toml") && e.contains("derived"),
            "{e}"
        );
        let _ = fs::remove_dir_all(&mods);
    }
}
