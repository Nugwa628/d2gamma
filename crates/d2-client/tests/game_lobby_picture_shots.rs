//! The realm lobby on its still picture (`app::lobby_picture`), on the
//! user's install, without a window: the main menu's art on its blurred
//! cover, the same whatever `[video] menu_sides` says, under the lobby's
//! UI as the native UI pass lays it on (`FrontHost::native_rgba_over`), against the
//! in-memory fake server. No game starts behind the lobby. One PNG at
//! 1080p and one at 2160p in `D2_SHOTS` (default: the temp dir).
//! Ignored: needs `D2_GAME_DIR`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use d2_client::app::backdrop_bake::{missing, StillsBake};
use d2_client::app::config::{ConfigRes, MenuSides, Settings};
use d2_client::app::front_host::{add_front_end, FrontArt, FrontHost};
use d2_client::app::front_ui::Under;
use d2_client::app::lobby_picture::{LobbyPicture, Shows};
use d2_client::app::single_player::GameData;
use d2_client::controls::Bindings;
use d2_client::gpu_compositor::ui_pass::UiPlacement;
use d2_client::ui::front_end::screens::online::backdrop::Backdrop;
use d2_client::ui::front_end::screens::online::fake::{character, FakeServer, SharedFake};
use d2_client::ui::front_end::screens::online::{self, Online, ServerEntry};
use d2_client::ui::front_end::*;
use d2_client::ui::geom::Point;
use d2_formats::d2s::status;

struct Rig {
    app: App,
    ms: Arc<AtomicU32>,
    cfg: PathBuf,
}

impl Rig {
    fn host(&mut self) -> Mut<'_, FrontHost> {
        self.app.world_mut().non_send_mut::<FrontHost>()
    }

    fn ticks(&mut self, n: u32) {
        for _ in 0..n {
            self.app.update();
            self.ms.fetch_add(40, Ordering::SeqCst);
        }
    }

    fn input(&mut self, ev: FrontInput) {
        self.host().front.input(ev);
        self.ticks(2);
    }

    fn typed(&mut self, s: &str) {
        for u in s.encode_utf16() {
            self.host().front.input(FrontInput::Char(u));
        }
        self.ticks(2);
    }

    fn click_id(&mut self, id: &str) {
        let e = self
            .host()
            .front
            .ui_elements()
            .into_iter()
            .find(|e| e.id == id)
            .unwrap_or_else(|| panic!("no {id}"));
        self.host().front.input(FrontInput::Move(e.at));
        self.host().front.input(FrontInput::Down(e.at));
        self.ticks(1);
        self.host().front.input(FrontInput::Up(e.at));
        self.ticks(4);
    }

    fn picture(&self) -> Option<LobbyPicture> {
        self.app.world().get_resource::<LobbyPicture>().cloned()
    }

    /// Nothing of a game runs: no bridge to a server, no world view, no
    /// scene.
    fn no_game(&self, what: &str) {
        let w = self.app.world();
        assert!(
            !w.contains_resource::<d2_client::bridge::mirror::BridgeResource>(),
            "{what}: no game's bridge"
        );
        assert!(
            !w.contains_resource::<d2_client::world_view::present::WorldViewTarget>(),
            "{what}: no world view"
        );
        assert!(
            !w.contains_resource::<d2_client::world_view::scene_view::SceneView>(),
            "{what}: no scene"
        );
    }

    /// The frame as the window shows it at `out` (the pointer out of the
    /// way), to `dir/name.png`.
    fn shot(&mut self, dir: &Path, name: &str, out: (u32, u32)) {
        self.host()
            .front
            .input(FrontInput::Move(Point::new(1060, 590)));
        self.ticks(2);
        let picture = self.picture().expect("the lobby's picture");
        let (rgba, size) = {
            let images = self.app.world().resource::<Assets<Image>>();
            let i = images.get(&picture.image).expect("its image");
            (i.data.clone().expect("its pixels"), (i.width(), i.height()))
        };
        assert_eq!(rgba.len(), (size.0 * size.1 * 4) as usize);
        let under = Under {
            rgba: &rgba,
            size,
            exponent: picture.exponent,
        };
        let canvas = self.host().front.canvas();
        let place = UiPlacement {
            frame_w: canvas.width as u32,
            frame_h: canvas.height as u32,
            out_w: out.0,
            out_h: out.1,
        };
        let shot = self
            .host()
            .native_rgba_over(place, Some(&under))
            .expect("native frame");
        let path = dir.join(format!("{name}.png"));
        let file = std::fs::File::create(&path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), out.0, out.1);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&shot).unwrap();
        println!("wrote {}", path.display());
    }
}

fn shots_dir() -> PathBuf {
    let out = std::env::var_os("D2_SHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("lobby-picture");
    std::fs::create_dir_all(&out).unwrap();
    out
}

/// The front end on the install's art and the online screens on a fake
/// server, `sides` as the saved side fill and the stills in `stills` (none
/// baked by the rig); logged in and in the lobby.
fn rig(sides: MenuSides, stills: PathBuf) -> Rig {
    use d2_data::bin::TableFiles;
    let game_dir = PathBuf::from(std::env::var("D2_GAME_DIR").expect("D2_GAME_DIR"));
    let data = GameData::select(Some(&game_dir)).expect("live data");
    let GameData::Live(live) = &data;
    let mut art = FrontArt::new(live.archives.source());
    if let Ok(t) = d2_client::app::strings::TableStrings::load(
        live.archives.as_ref(),
        d2_client::app::strings::LANG,
    ) {
        art = art.with_strings(move |id| u16::try_from(id).map(|i| t.by_id(i)).unwrap_or_default());
    }
    let mut fake = FakeServer::default();
    fake.server_name = "D2Gamma".into();
    fake.add_account(
        "Nugwa",
        "password1",
        vec![character("Werwer", 4, 34, 5, status::EXPANSION)],
    );
    let server = SharedFake(std::rc::Rc::new(std::cell::RefCell::new(fake)));
    let servers = vec![ServerEntry {
        id: "eu1".into(),
        name: "D2Gamma".into(),
        address: "127.0.0.1:6112".into(),
    }];
    let handle = Online::new(Box::new(server.clone()), servers);
    let mut reg = Registry::default();
    screens::register_all(&mut reg);
    online::register_with(&mut reg, handle);
    let front = FrontEnd::new(live.archives.lod(), Box::new(false), reg);
    let host = FrontHost::with_front(front, Some(art), false);

    let ms = Arc::new(AtomicU32::new(1000));
    let cfg = std::env::temp_dir().join(format!(
        "lobby-picture-cfg-{}-{}-{}",
        std::process::id(),
        sides.key(),
        stills
            .file_name()
            .map_or("".into(), |n| n.to_string_lossy())
    ));
    std::fs::create_dir_all(&cfg).unwrap();
    let mut app = App::new();
    app.insert_resource(d2_client::bridge::mirror::ScriptedClock(ms.clone()));
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        StatesPlugin,
        bevy::input::InputPlugin,
    ))
    .init_asset::<Image>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        40,
    )));
    d2_client::app::play::register_live_client(&mut app, false);
    d2_client::app::session::add_session(&mut app);
    app.insert_resource(ConfigRes {
        dir: cfg.clone(),
        settings: Settings {
            menu_sides: sides,
            lobby_background: Backdrop::Tristram,
            ..Settings::default()
        },
        bindings: Bindings::empty(),
        sound_device: false,
    });
    app.insert_resource(StillsBake::at(Some(stills), None));
    add_front_end(&mut app, host);
    let mut r = Rig { app, ms, cfg };
    r.ticks(30);
    assert!(r.picture().is_none(), "no picture before the lobby");
    // Log in and into the lobby.
    r.host().front.trigger(Trigger::Online);
    r.ticks(10);
    r.typed("nugwa");
    r.input(FrontInput::Key(9));
    r.typed("password1");
    r.input(FrontInput::Key(13));
    r.ticks(20);
    r.click_id("online_chars.char(Werwer)");
    r.click_id("online_chars.ok");
    r.ticks(6);
    assert_eq!(r.host().front.current(), online::ONLINE_LOBBY);
    r
}

#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn without_its_still_the_lobby_shows_the_menu_art_and_starts_no_game() {
    let dir = shots_dir();
    // No still baked.
    let none = std::env::temp_dir().join(format!("lobby-no-stills-{}", std::process::id()));
    std::fs::create_dir_all(&none).unwrap();
    let mut pictures = Vec::new();
    for sides in MenuSides::ALL {
        let mut r = rig(sides, none.clone());
        assert!(r.host().front.wide(), "the lobby is 16:9");
        let p = r.picture().expect("the lobby shows its picture");
        assert_eq!(p.shows, Shows::MenuArt);
        let canvas = r.host().front.canvas();
        assert_eq!(p.size, (canvas.width as u32, canvas.height as u32));
        let data = {
            let images = r.app.world().resource::<Assets<Image>>();
            images
                .get(&p.image)
                .and_then(|i| i.data.clone())
                .expect("its pixels")
        };
        // The art and its cover: no black column at the edges.
        let w = p.size.0 as usize;
        let row =
            |x: usize| (0..p.size.1 as usize).any(|y| data[(y * w + x) * 4..][..3] != [0, 0, 0]);
        assert!(row(0) && row(w - 1), "{}: the width is filled", sides.key());
        pictures.push(data);
        // A while in the lobby, a background picked: still no game.
        r.ticks(100);
        r.click_id("online_lobby.background");
        r.click_id("online_lobby.background(arreat-summit)");
        r.ticks(50);
        r.no_game(sides.key());
        assert_eq!(r.picture().map(|p| p.shows), Some(Shows::MenuArt));
        if sides == MenuSides::Blur {
            r.shot(&dir, "lobby-fallback-1080p", (1920, 1080));
            r.shot(&dir, "lobby-fallback-2160p", (3840, 2160));
        }
        // Back to the characters (4:3): the picture goes.
        r.click_id("online_lobby.back");
        r.ticks(10);
        if !r.host().front.wide() {
            assert!(r.picture().is_none(), "no picture on a 4:3 screen");
        }
        r.no_game("after the lobby");
        std::fs::remove_dir_all(&r.cfg).ok();
    }
    assert!(
        pictures.windows(2).all(|w| w[0] == w[1]),
        "the same picture whatever menu_sides says"
    );
}

/// The stills `bake-backdrops` wrote to `D2_STILLS` (`game_backdrop_bake`),
/// else baked here by the game's exe into a temp folder.
fn baked_stills() -> PathBuf {
    if let Some(d) = std::env::var_os("D2_STILLS").map(PathBuf::from) {
        if missing(&d).is_empty() {
            return d;
        }
    }
    let out = std::env::temp_dir().join(format!("lobby-stills-{}", std::process::id()));
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_D2Gamma-Game"))
        .args(["--mute", "bake-backdrops", "--out"])
        .arg(&out)
        .output()
        .expect("the game's exe runs");
    assert!(
        run.status.success(),
        "bake-backdrops: {}",
        String::from_utf8_lossy(&run.stdout)
    );
    out
}

#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn the_lobby_shows_each_baked_still() {
    let dir = shots_dir();
    let stills = baked_stills();
    let mut r = rig(MenuSides::Black, stills);
    assert_eq!(
        r.picture().map(|p| p.shows),
        Some(Shows::Still(Backdrop::Tristram)),
        "the saved background's still"
    );
    for b in Backdrop::ALL {
        if r.picture().map(|p| p.shows) != Some(Shows::Still(b)) {
            r.click_id("online_lobby.background");
            r.click_id(&format!("online_lobby.background({})", b.key()));
            r.ticks(2);
        }
        let p = r.picture().expect("the lobby's picture");
        assert_eq!(p.shows, Shows::Still(b), "the picker swaps the still");
        let size = {
            let images = r.app.world().resource::<Assets<Image>>();
            let i = images.get(&p.image).expect("its image");
            (i.width(), i.height())
        };
        assert_eq!(size.0, 1280, "{}: the still's own pixels", b.key());
        assert!((719..=720).contains(&size.1), "{}: {size:?}", b.key());
        r.no_game(b.key());
        r.shot(
            &dir,
            &format!("lobby-still-{}-1080p", b.key()),
            (1920, 1080),
        );
        r.shot(
            &dir,
            &format!("lobby-still-{}-2160p", b.key()),
            (3840, 2160),
        );
    }
    std::fs::remove_dir_all(&r.cfg).ok();
}
