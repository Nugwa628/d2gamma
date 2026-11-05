//! The D2Gamma Mods menu's model: which mods are installed and whether each
//! can be started, which mod was played last, and the request to start
//! another one.
//!
//! - **The list** ([`list`]): every folder of `Documents\D2Gamma\mods` with
//!   what the player is shown: name, author, version, a picture when the mod
//!   has one (`files/mod/picture.png`), and a [`ModStatus`]. A mod made for
//!   an API version that is too old or too new says so in the words of
//!   `d2-modkit` ("this mod needs updating", "needs a newer D2Gamma"); a
//!   folder with a broken `mod.toml` is listed with the reason instead of
//!   being hidden.
//! - **Last played** ([`read_last`], [`write_last`]): `mods\active.toml`
//!   holds the id of the mod D2Gamma opens into on the next plain start (or
//!   `none` for plain D2Gamma). It is written by the run that starts, so a
//!   launcher's `--mod <id>` counts as playing it. It is a file of its own
//!   because `settings.toml` is versioned and migrated.
//! - **Switching** ([`Switch`], [`request_switch`]): picking a mod, or going
//!   back to plain D2Gamma, ends the front end; `main` then starts the exe
//!   again with the picked mod and waits for it ([`crate::kit_mod::relaunch`]).
//!   The mod is fixed for a process (identity, user folder and data), so
//!   this is the only way to change it.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use d2_modkit::api::{check_api, ApiCheck, Date};
use d2_modkit::{Kind, ModSpec};

use crate::kit_mod::{self, PLAIN_ID};

/// The last-played file, in the mods folder next to the mods' own folders.
pub const ACTIVE_FILE: &str = "active.toml";

/// A mod's picture in the mod folder: shown in the mods list.
pub const PICTURE_PATH: &str = "files/mod/picture.png";

/// Whether a listed mod can be started, and if not why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModStatus {
    /// It can be started.
    Ready,
    /// Made for an older API than D2Gamma still runs: the message tells the
    /// player to get an updated mod.
    NeedsUpdate(String),
    /// Made for a newer API than this D2Gamma: the message tells the player
    /// to update the game.
    TooNew(String),
    /// Cannot be started for another reason (a broken `mod.toml`, an add-on,
    /// a mod it builds on that is missing); the message says what.
    Unavailable(String),
}

impl ModStatus {
    pub fn is_ready(&self) -> bool {
        matches!(self, ModStatus::Ready)
    }

    /// The line shown under a mod that cannot be started (`None` when it
    /// can).
    pub fn problem(&self) -> Option<&str> {
        match self {
            ModStatus::Ready => None,
            ModStatus::NeedsUpdate(m) | ModStatus::TooNew(m) | ModStatus::Unavailable(m) => Some(m),
        }
    }
}

/// One line of the mods list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModEntry {
    /// The mod's id (its folder name when the manifest cannot be read).
    pub id: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub status: ModStatus,
    /// The mod's picture file, when it has one.
    pub picture: Option<PathBuf>,
    pub dir: PathBuf,
    pub accepts_d2gamma_characters: bool,
}

/// Every installed mod, by name. `today` decides which API versions are
/// still supported.
pub fn list_at(mods_dir: &Path, today: Date) -> Vec<ModEntry> {
    list_checked(mods_dir, today, &|api| check_api(api, today))
}

/// [`list_at`] with the API version check given (the kit's release history
/// has one entry today, so a test supplies its own to see an old mod).
fn list_checked(mods_dir: &Path, today: Date, api: &dyn Fn(u32) -> ApiCheck) -> Vec<ModEntry> {
    let installed = d2_modkit::resolve::list_installed(mods_dir);
    let manifests = installed
        .iter()
        .filter_map(|i| i.manifest.as_ref().ok())
        .map(|m| (m.id.clone(), m.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let target_of = |id: &str| manifests.get(id).and_then(|m| m.attaches_to.as_deref());
    let group_name = |id: &str| {
        if id == "d2gamma" {
            "D2Gamma".to_owned()
        } else {
            manifests
                .get(id)
                .map(|m| m.name.clone())
                .unwrap_or_else(|| id.to_owned())
        }
    };
    let mut out: Vec<ModEntry> = installed
        .into_iter()
        .map(|inst| match inst.manifest {
            Err(e) => ModEntry {
                id: inst.folder.clone(),
                name: inst.folder,
                author: String::new(),
                version: String::new(),
                description: String::new(),
                status: ModStatus::Unavailable(first_line(&e.to_string())),
                picture: None,
                dir: inst.dir,
                accepts_d2gamma_characters: false,
            },
            Ok(m) => {
                let mut status = status_of(&m, &inst.dir, mods_dir, today, api);
                if let Some(target) = m.attaches_to.as_deref() {
                    let enabled = d2_modkit::addons::enabled(mods_dir, target);
                    let selected = enabled.as_ref().is_ok_and(|ids| ids.contains(&m.id));
                    let approval = if target == "d2gamma" {
                        "owner approval checked at load".to_owned()
                    } else if manifests
                        .get(target)
                        .is_some_and(|base| base.approved_addons.contains(&m.id))
                    {
                        "maker-approved".to_owned()
                    } else {
                        "maker approval missing".to_owned()
                    };
                    if matches!(status, ModStatus::Unavailable(_)) {
                        status = ModStatus::Unavailable(format!(
                            "Add-on for {} · {} · {approval}{}",
                            group_name(target),
                            if selected { "enabled" } else { "disabled" },
                            enabled.err().map(|e| format!(" · {e}")).unwrap_or_default()
                        ));
                    }
                }
                let picture = Some(inst.dir.join(PICTURE_PATH)).filter(|p| p.is_file());
                ModEntry {
                    id: m.id,
                    name: m.name,
                    author: m.author,
                    version: m.version.to_string(),
                    description: m.description,
                    status,
                    picture,
                    dir: inst.dir,
                    accepts_d2gamma_characters: m.accepts_d2gamma_characters,
                }
            }
        })
        .collect();
    if manifests
        .values()
        .any(|m| m.attaches_to.as_deref() == Some("d2gamma"))
    {
        out.push(ModEntry {
            id: PLAIN_ID.into(),
            name: "D2Gamma".into(),
            author: String::new(),
            version: String::new(),
            description: "Plain D2Gamma; approved screen add-ons also work online.".into(),
            status: ModStatus::Ready,
            picture: None,
            dir: mods_dir.to_path_buf(),
            accepts_d2gamma_characters: false,
        });
    }
    let group = |e: &ModEntry| {
        target_of(&e.id)
            .map(group_name)
            .unwrap_or_else(|| {
                if e.id == PLAIN_ID {
                    "D2Gamma".into()
                } else {
                    e.name.clone()
                }
            })
            .to_lowercase()
    };
    out.sort_by(|a, b| {
        group(a)
            .cmp(&group(b))
            .then_with(|| target_of(&a.id).is_some().cmp(&target_of(&b.id).is_some()))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
    for entry in &mut out {
        if target_of(&entry.id).is_some() {
            entry.name = format!("  ↳ {}", entry.name);
        }
    }
    out
}

/// [`list_at`] today, in D2Gamma's mods folder.
pub fn list() -> Vec<ModEntry> {
    list_at(&crate::user_dir::mods_dir(), Date::today())
}

fn status_of(
    m: &d2_modkit::Manifest,
    dir: &Path,
    mods_dir: &Path,
    today: Date,
    api: &dyn Fn(u32) -> ApiCheck,
) -> ModStatus {
    match api(m.api) {
        ApiCheck::NeedsUpdate(msg) => return ModStatus::NeedsUpdate(msg),
        ApiCheck::TooNew(msg) => return ModStatus::TooNew(msg),
        ApiCheck::Ok => {}
    }
    if m.kind != Kind::Full {
        return ModStatus::Unavailable(
            "An add-on: it runs with the mod it is made for, not by itself.".to_owned(),
        );
    }
    // The folder is named by the id for every installed mod; one that is not
    // is not startable by id.
    if dir.file_name().and_then(|n| n.to_str()) != Some(m.id.as_str()) {
        return ModStatus::Unavailable(format!(
            "The folder must be named {}, after the mod's id.",
            m.id
        ));
    }
    match d2_modkit::resolve::resolve_at(&ModSpec::Id(m.id.clone()), mods_dir, today) {
        Ok(_) => ModStatus::Ready,
        Err(e) => ModStatus::Unavailable(first_line(&e.to_string())),
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().trim().to_owned()
}

/// The mod id in `active.toml` of `mods_dir`; `None` for plain D2Gamma
/// (`none`), a missing file, or anything that is not a valid mod id.
pub fn read_last(mods_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(mods_dir.join(ACTIVE_FILE)).ok()?;
    let doc: toml_edit::DocumentMut = text.parse().ok()?;
    let id = doc.get("last_played")?.as_str()?;
    (id != PLAIN_ID && d2_modkit::manifest::check_id(id).is_ok()).then(|| id.to_owned())
}

/// Remembers `id` as the mod played last, or plain D2Gamma for `None`.
pub fn write_last(mods_dir: &Path, id: Option<&str>) -> std::io::Result<()> {
    std::fs::create_dir_all(mods_dir)?;
    let text = format!(
        "# The mod D2Gamma opens into. `none` is plain D2Gamma. Written by the game.\nlast_played = \"{}\"\n",
        id.unwrap_or(PLAIN_ID)
    );
    let path = mods_dir.join(ACTIVE_FILE);
    let tmp = mods_dir.join(format!("{ACTIVE_FILE}.tmp"));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path).or_else(|_| {
        // A rename onto an existing file can fail on some platforms.
        let _ = std::fs::remove_file(&path);
        std::fs::rename(&tmp, &path)
    })
}

/// What a plain `play` start (no `--mod`, no `--no-mod`) opens: the mod
/// played last, when there is one and `args` start the front-end session.
pub fn start_choice(args: &[String], mods_dir: &Path) -> Option<ModSpec> {
    if !kit_mod::is_front_session(args) {
        return None;
    }
    read_last(mods_dir).map(ModSpec::Id)
}

/// The most pixels a mod's picture or icon may have per side, and the most
/// bytes of its file: a bigger file is ignored rather than slowing the menu.
pub const MAX_IMAGE_SIDE: u32 = 1024;
pub const MAX_IMAGE_BYTES: u64 = 4 << 20;

/// A decoded image: `rgba` is four bytes a pixel, rows top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// Decodes a PNG of any colour type to RGBA. `None` for anything that is not
/// a readable PNG within [`MAX_IMAGE_SIDE`].
pub fn decode_png(bytes: &[u8]) -> Option<Rgba> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().ok()?;
    let info = reader.info();
    if info.width == 0
        || info.height == 0
        || info.width > MAX_IMAGE_SIDE
        || info.height > MAX_IMAGE_SIDE
    {
        return None;
    }
    let mut buf = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..frame.buffer_size()];
    let rgba: Vec<u8> = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(Rgba {
        w: frame.width,
        h: frame.height,
        rgba,
    })
}

/// A mod's picture file, scaled down to fit `max_w` x `max_h` (never up;
/// nearest-pixel) and flattened onto black: three bytes a pixel and its
/// size. `None` when the file is missing, too big or not a PNG.
pub fn load_picture(path: &Path, max_w: u32, max_h: u32) -> Option<(u32, u32, Vec<u8>)> {
    if std::fs::metadata(path).ok()?.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let img = decode_png(&std::fs::read(path).ok()?)?;
    let scale = (f64::from(max_w) / f64::from(img.w))
        .min(f64::from(max_h) / f64::from(img.h))
        .min(1.0);
    let w = ((f64::from(img.w) * scale).round() as u32).max(1);
    let h = ((f64::from(img.h) * scale).round() as u32).max(1);
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        let sy = (y * img.h / h).min(img.h - 1);
        for x in 0..w {
            let sx = (x * img.w / w).min(img.w - 1);
            let i = ((sy * img.w + sx) * 4) as usize;
            let a = u32::from(img.rgba[i + 3]);
            rgb.extend((0..3).map(|c| (u32::from(img.rgba[i + c]) * a / 255) as u8));
        }
    }
    Some((w, h, rgb))
}

/// What the player picked in the menus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Switch {
    /// Plain D2Gamma ("Back to D2Gamma").
    Plain,
    /// The mod with this id.
    Mod(String),
}

impl Switch {
    /// The command line that starts the picked game: `original` (this run's
    /// arguments) with the old mod choice replaced.
    pub fn args(&self, original: &[String]) -> Vec<String> {
        kit_mod::child_args(
            original,
            match self {
                Switch::Plain => PLAIN_ID,
                Switch::Mod(id) => id,
            },
        )
    }
}

static SWITCH: Mutex<Option<Switch>> = Mutex::new(None);

/// Asks for the game to start `switch` once the front end has closed.
pub fn request_switch(switch: Switch) {
    if let Ok(mut s) = SWITCH.lock() {
        *s = Some(switch);
    }
}

/// The switch asked for, once.
pub fn take_switch() -> Option<Switch> {
    SWITCH.lock().ok().and_then(|mut s| s.take())
}

/// Starts the game the player picked (if they picked one) and waits for it:
/// the exit code to end with. `None` when no switch was asked for.
pub fn restart_if_asked(original: &[String]) -> Option<std::io::Result<u8>> {
    let switch = take_switch()?;
    let args = switch.args(original);
    crate::launch::note(format!("switching game: {}", args.join(" ")));
    Some(kit_mod::relaunch_self(&args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn today() -> Date {
        Date::new(2026, 11, 1)
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("d2g-mod-menu-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn manifest(id: &str, name: &str, api: u32) -> String {
        format!(
            "id = \"{id}\"\nname = \"{name}\"\nversion = \"1.2.0\"\nauthor = \"Ada\"\n\
             description = \"Harder fire.\"\napi = {api}\nkind = \"full\"\n"
        )
    }

    fn install(mods: &Path, folder: &str, toml: &str) -> PathBuf {
        let dir = mods.join(folder);
        fs::create_dir_all(dir.join("files")).unwrap();
        fs::write(dir.join("mod.toml"), toml).unwrap();
        dir
    }

    #[test]
    fn the_list_has_the_shown_fields_of_a_good_mod_and_its_picture() {
        let mods = temp("good");
        let dir = install(&mods, "ember-age", &manifest("ember-age", "Ember Age", 1));
        fs::create_dir_all(dir.join("files/mod")).unwrap();
        fs::write(dir.join(PICTURE_PATH), b"not read here").unwrap();
        let l = list_at(&mods, today());
        assert_eq!(l.len(), 1);
        let e = &l[0];
        assert_eq!(
            (
                e.id.as_str(),
                e.name.as_str(),
                e.author.as_str(),
                e.version.as_str()
            ),
            ("ember-age", "Ember Age", "Ada", "1.2.0")
        );
        assert_eq!(e.description, "Harder fire.");
        assert_eq!(e.status, ModStatus::Ready);
        assert_eq!(e.picture.as_deref(), Some(dir.join(PICTURE_PATH).as_path()));
        // No picture file: none listed.
        install(&mods, "plain-mod", &manifest("plain-mod", "Plain Mod", 1));
        let l = list_at(&mods, today());
        assert!(l
            .iter()
            .find(|e| e.id == "plain-mod")
            .unwrap()
            .picture
            .is_none());
        let _ = fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_broken_manifest_is_listed_with_its_reason() {
        let mods = temp("broken");
        install(&mods, "oops", "id = \"oops\"\nname = [1\n");
        install(&mods, "empty-one", "");
        let l = list_at(&mods, today());
        assert_eq!(l.len(), 2);
        for e in &l {
            let ModStatus::Unavailable(why) = &e.status else {
                panic!("{e:?}")
            };
            assert!(!why.is_empty() && !why.contains('\n'), "{why:?}");
            // The folder name stands in for the name.
            assert_eq!(e.name, e.id);
        }
        let _ = fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_mod_for_a_newer_api_says_it_needs_a_newer_game() {
        let mods = temp("api-new");
        install(&mods, "too-new", &manifest("too-new", "Too New", 2));
        install(&mods, "current", &manifest("current", "Current", 1));
        let l = list_at(&mods, today());
        let by = |id: &str| l.iter().find(|e| e.id == id).unwrap();
        assert_eq!(by("current").status, ModStatus::Ready);
        // The message is the kit's own, shown as it is.
        assert_eq!(
            check_api(2, today()),
            ApiCheck::TooNew(by("too-new").status.problem().unwrap().to_owned())
        );
        assert!(matches!(by("too-new").status, ModStatus::TooNew(_)));
        let _ = fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_mod_left_behind_says_it_needs_updating() {
        let mods = temp("api-old");
        install(
            &mods,
            "left-behind",
            &manifest("left-behind", "Left Behind", 1),
        );
        // A history where version 2 came out and version 1 has left the
        // window: the old mod is `NeedsUpdate` with the kit's message.
        let old = |api: u32| match api {
            1 => ApiCheck::NeedsUpdate("This mod needs updating.".to_owned()),
            _ => ApiCheck::Ok,
        };
        let l = list_checked(&mods, today(), &old);
        let by = |id: &str| l.iter().find(|e| e.id == id).unwrap();
        assert_eq!(
            by("left-behind").status,
            ModStatus::NeedsUpdate("This mod needs updating.".to_owned())
        );
        assert!(!by("left-behind").status.is_ready());
        let _ = fs::remove_dir_all(&mods);
    }

    #[test]
    fn an_add_on_and_a_missing_base_cannot_be_picked() {
        let mods = temp("unavailable");
        install(
            &mods,
            "ember-ui",
            "id = \"ember-ui\"\nname = \"Ember UI\"\nversion = \"1.0.0\"\nauthor = \"Ada\"\n\
             description = \"\"\napi = 1\nkind = \"addon\"\nfor = \"ember-age\"\n",
        );
        install(
            &mods,
            "child-mod",
            &format!(
                "{}[base]\nid = \"ember-age\"\nversion = \">=1.0.0\"\n",
                manifest("child-mod", "Child Mod", 1)
            ),
        );
        let l = list_at(&mods, today());
        for e in &l {
            assert!(
                matches!(e.status, ModStatus::Unavailable(_)),
                "{}: {:?}",
                e.id,
                e.status
            );
        }
        let child = l.iter().find(|e| e.id == "child-mod").unwrap();
        assert!(child.status.problem().unwrap().contains("not installed"));
        let _ = fs::remove_dir_all(&mods);
    }

    #[test]
    fn addons_are_grouped_below_targets_with_approval_and_enablement() {
        let mods = temp("addon-groups");
        install(
            &mods,
            "test-mod",
            &format!(
                "{}approved_addons=['test-hud']",
                manifest("test-mod", "Test Mod", 1)
            ),
        );
        install(&mods, "other-mod", &manifest("other-mod", "Other Mod", 1));
        install(&mods, "test-hud", "id='test-hud'\nname='A HUD'\nversion='1.0.0'\nauthor='Test'\ndescription=''\napi=1\nkind='addon'\nfor='test-mod'");
        install(&mods, "plain-hud", "id='plain-hud'\nname='Plain HUD'\nversion='1.0.0'\nauthor='Test'\ndescription=''\napi=1\nkind='addon'\nfor='d2gamma'\nscreen_only=true");
        fs::write(mods.join("addons.toml"), "[enabled]\ntest-mod=['test-hud']").unwrap();
        let entries = list_at(&mods, today());
        assert_eq!(
            entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["none", "plain-hud", "other-mod", "test-mod", "test-hud"]
        );
        let hud = entries.last().unwrap();
        assert!(hud.name.contains('↳'));
        assert!(hud
            .status
            .problem()
            .unwrap()
            .contains("enabled · maker-approved"));
        assert!(!hud.status.is_ready());
        fs::remove_dir_all(mods).unwrap();
    }

    #[test]
    fn files_and_dot_folders_are_not_mods_and_the_list_is_by_name() {
        let mods = temp("sorted");
        install(&mods, "zeta-mod", &manifest("zeta-mod", "alpha", 1));
        install(&mods, "beta-mod", &manifest("beta-mod", "Zulu", 1));
        install(&mods, ".hidden", &manifest("hidden-mod", "Hidden", 1));
        write_last(&mods, Some("beta-mod")).unwrap();
        let names: Vec<_> = list_at(&mods, today())
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["alpha", "Zulu"]);
        // No mods folder at all: an empty list.
        assert!(list_at(&mods.join("nope"), today()).is_empty());
        let _ = fs::remove_dir_all(&mods);
    }

    #[test]
    fn the_last_played_mod_is_remembered_and_none_is_plain() {
        let mods = temp("last");
        assert_eq!(read_last(&mods), None);
        write_last(&mods, Some("ember-age")).unwrap();
        assert_eq!(read_last(&mods).as_deref(), Some("ember-age"));
        // The file is readable text of its own.
        let text = fs::read_to_string(mods.join(ACTIVE_FILE)).unwrap();
        assert!(text.contains("last_played = \"ember-age\""), "{text}");
        write_last(&mods, Some("second-mod")).unwrap();
        assert_eq!(read_last(&mods).as_deref(), Some("second-mod"));
        write_last(&mods, None).unwrap();
        assert_eq!(read_last(&mods), None);
        assert!(fs::read_to_string(mods.join(ACTIVE_FILE))
            .unwrap()
            .contains("last_played = \"none\""));
        // A file that is damaged or names no valid id opens plain D2Gamma.
        for bad in [
            "last_played = [",
            "last_played = \"Bad Id!\"",
            "other = 1\n",
            "",
        ] {
            fs::write(mods.join(ACTIVE_FILE), bad).unwrap();
            assert_eq!(read_last(&mods), None, "{bad:?}");
        }
        let _ = fs::remove_dir_all(&mods);
    }

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn only_the_front_end_session_opens_the_last_played_mod() {
        let mods = temp("start");
        write_last(&mods, Some("ember-age")).unwrap();
        let want = Some(ModSpec::Id("ember-age".into()));
        assert_eq!(start_choice(&v(&[]), &mods), want);
        assert_eq!(start_choice(&v(&["play", "--res", "800x600"]), &mods), want);
        for tool in [
            &["play", "--new", "amazon", "Amy"][..],
            &["play", "--save", "x.d2s"],
            &["play", "--frames", "3"],
            &["--offscreen"],
            &["play", "--dump-draws", "d"],
            &["play", "--save=x.d2s"],
        ] {
            assert_eq!(start_choice(&v(tool), &mods), None, "{tool:?}");
        }
        // `--save-dir` only names a folder; the menus still run.
        assert_eq!(start_choice(&v(&["play", "--save-dir", "s"]), &mods), want);
        write_last(&mods, None).unwrap();
        assert_eq!(start_choice(&v(&[]), &mods), None);
        let _ = fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_pick_restarts_with_its_mod_and_back_with_none() {
        let original = v(&[
            "play",
            "--mod",
            "old-mod",
            "--res",
            "800x600",
            "--game-dir",
            "G",
        ]);
        assert_eq!(
            Switch::Mod("new-mod".into()).args(&original),
            v(&[
                "play",
                "--mod",
                "new-mod",
                "--res",
                "800x600",
                "--game-dir",
                "G"
            ])
        );
        assert_eq!(
            Switch::Plain.args(&original),
            v(&[
                "play",
                "--mod",
                "none",
                "--res",
                "800x600",
                "--game-dir",
                "G"
            ])
        );
        // From plain D2Gamma, with a safety flag: it is replaced, not kept.
        assert_eq!(
            Switch::Mod("ember-age".into()).args(&v(&["--no-mod", "--res", "800x600"])),
            v(&["--mod", "ember-age", "--res", "800x600"])
        );
        // The child's own command line is read back as the choice it made.
        let mut child = Switch::Plain.args(&original);
        assert_eq!(
            kit_mod::take_plain(&mut child),
            Ok(Some(kit_mod::Plain::Chosen))
        );
    }

    fn write_png(path: &Path, w: u32, h: u32, color: png::ColorType, data: &[u8]) {
        let file = fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(color);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(data).unwrap();
    }

    #[test]
    fn a_mod_picture_is_fitted_and_flattened() {
        let dir = temp("picture");
        // 8 x 4, opaque red on the left half, transparent on the right.
        let data: Vec<u8> = (0..4)
            .flat_map(|_| {
                (0..8).flat_map(|x| {
                    if x < 4 {
                        [255, 0, 0, 255]
                    } else {
                        [0, 255, 0, 0]
                    }
                })
            })
            .collect();
        let path = dir.join("p.png");
        write_png(&path, 8, 4, png::ColorType::Rgba, &data);
        // Fits as it is: never scaled up.
        let (w, h, rgb) = load_picture(&path, 100, 100).unwrap();
        assert_eq!((w, h, rgb.len()), (8, 4, 8 * 4 * 3));
        assert_eq!(&rgb[0..3], &[255, 0, 0]);
        // The transparent half is black, not green.
        assert_eq!(&rgb[7 * 3..8 * 3], &[0, 0, 0]);
        // Scaled down to the box, aspect kept.
        let (w, h, _) = load_picture(&path, 4, 4).unwrap();
        assert_eq!((w, h), (4, 2));
        // Other colour types read too.
        write_png(
            &dir.join("g.png"),
            2,
            1,
            png::ColorType::Grayscale,
            &[10, 200],
        );
        let g = decode_png(&fs::read(dir.join("g.png")).unwrap()).unwrap();
        assert_eq!(g.rgba, [10, 10, 10, 255, 200, 200, 200, 255]);
        // Not a PNG, or missing: no picture.
        fs::write(dir.join("bad.png"), b"nope").unwrap();
        assert!(load_picture(&dir.join("bad.png"), 10, 10).is_none());
        assert!(load_picture(&dir.join("missing.png"), 10, 10).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_switch_is_asked_for_once() {
        // The one test that uses the process-wide request.
        assert_eq!(take_switch(), None);
        request_switch(Switch::Mod("ember-age".into()));
        request_switch(Switch::Plain);
        assert_eq!(take_switch(), Some(Switch::Plain));
        assert_eq!(take_switch(), None);
    }
}
