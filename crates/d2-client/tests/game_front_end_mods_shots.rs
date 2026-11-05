//! The D2Gamma Mods screen and the main menu's door to it, on the user's
//! install, rendered without a window (`FrontHost::native_rgba`) over a fixed
//! list of made-up mods (their pictures are drawn here, not game art).
//! Ignored: needs `D2_GAME_DIR`; `D2_SHOTS` names the output folder (default:
//! the temp dir).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use d2_client::app::front_host::{add_front_end, FrontArt, FrontHost};
use d2_client::app::front_start::{front_host, Entry};
use d2_client::app::single_player::GameData;
use d2_client::gpu_compositor::ui_pass::UiPlacement;
use d2_client::mod_menu::{ModEntry, ModStatus, Switch};
use d2_client::ui::front_end::screens::main_menu;
use d2_client::ui::front_end::screens::mods::{MenuButton, ModsHost, ModsScreen};
use d2_client::ui::front_end::*;
use d2_client::ui::geom::Point;

struct Fixed {
    entries: Vec<ModEntry>,
    button: MenuButton,
    picks: RefCell<Vec<Switch>>,
}

impl ModsHost for Fixed {
    fn installed(&self) -> Vec<ModEntry> {
        self.entries.clone()
    }

    fn menu_button(&self) -> MenuButton {
        self.button
    }

    fn switch(&self, s: Switch) {
        self.picks.borrow_mut().push(s);
    }
}

pub(crate) fn app(host: FrontHost) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::asset::AssetPlugin::default(),
        bevy::input::InputPlugin,
    ))
    .init_asset::<Image>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        40,
    )));
    add_front_end(&mut app, host);
    app
}

pub(crate) fn host(a: &mut App) -> Mut<'_, FrontHost> {
    a.world_mut().non_send_mut::<FrontHost>()
}

pub(crate) fn ticks(a: &mut App, n: u32) {
    for _ in 0..n {
        a.update();
    }
}

fn click(a: &mut App, p: Point) {
    host(a).front.input(FrontInput::Down(p));
    ticks(a, 1);
    host(a).front.input(FrontInput::Up(p));
    ticks(a, 3);
}

fn click_id(a: &mut App, id: &str) {
    let els = host(a).front.ui_elements();
    let at = d2_client::ui::ui_ids::find(&els, id)
        .unwrap_or_else(|e| panic!("{e}"))
        .at;
    click(a, at);
}

pub(crate) fn write_png(path: &Path, w: u32, h: u32, color: png::ColorType, data: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(color);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(data).unwrap();
    println!("wrote {}", path.display());
}

/// A made-up mod picture: a sky-to-ground gradient with a disc in `tint`.
fn picture(path: &Path, tint: [u8; 3]) {
    let (w, h) = (320u32, 240u32);
    let mut px = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let t = y as f32 / h as f32;
            let mut c = [
                (30.0 + 90.0 * t) as u8,
                (40.0 + 50.0 * t) as u8,
                (90.0 - 60.0 * t) as u8,
            ];
            let (dx, dy) = (x as f32 - 160.0, y as f32 - 110.0);
            if dx * dx + dy * dy < 60.0 * 60.0 {
                c = tint;
            }
            px.extend(c);
        }
    }
    write_png(path, w, h, png::ColorType::Rgb, &px);
}

fn entry(dir: &Path, id: &str, name: &str, status: ModStatus, tint: Option<[u8; 3]>) -> ModEntry {
    let picture_path = tint.map(|t| {
        let p = dir.join(format!("{id}.png"));
        picture(&p, t);
        p
    });
    ModEntry {
        id: id.to_owned(),
        name: name.to_owned(),
        author: "Ada Modder".to_owned(),
        version: "1.2.0".to_owned(),
        accepts_d2gamma_characters: false,
        description: "New runes, harder fire and a second market.".to_owned(),
        status,
        picture: picture_path,
        dir: dir.join(id),
    }
}

pub(crate) fn shot(a: &mut App, out: &Path, name: &str) {
    let place = UiPlacement {
        frame_w: 800,
        frame_h: 600,
        out_w: 1200,
        out_h: 900,
    };
    let rgba = host(a).native_rgba(place).expect("native frame");
    let path = out.join(format!("{name}.png"));
    write_png(&path, 1200, 900, png::ColorType::Rgba, &rgba);
}

fn set(a: &mut App, entries: Vec<ModEntry>, button: MenuButton) -> Rc<Fixed> {
    let fixed = Rc::new(Fixed {
        entries,
        button,
        picks: RefCell::new(Vec::new()),
    });
    struct Shared(Rc<Fixed>);
    impl ModsHost for Shared {
        fn installed(&self) -> Vec<ModEntry> {
            self.0.installed()
        }
        fn menu_button(&self) -> MenuButton {
            self.0.menu_button()
        }
        fn switch(&self, s: Switch) {
            self.0.switch(s)
        }
    }
    let shared: Rc<dyn ModsHost> = Rc::new(Shared(fixed.clone()));
    {
        let mut h = host(a);
        h.front.register_screen(
            screens::ids::MODS,
            Box::new(ModsScreen::new(shared.clone())),
        );
        h.front
            .register_screen(MAIN_MENU, main_menu::screen_with(shared));
        // Back on the main menu, built again.
        h.front.goto(MAIN_MENU);
    }
    ticks(a, 30);
    fixed
}

#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn mods_screen_on_the_install() {
    let game = PathBuf::from(std::env::var("D2_GAME_DIR").expect("D2_GAME_DIR"));
    let out = std::env::var_os("D2_SHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out).unwrap();
    let work = std::env::temp_dir().join(format!("d2g-mods-shots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let data = GameData::select(Some(&game)).expect("live data");
    let GameData::Live(d) = &data;
    let mut art = FrontArt::new(d.archives.source());
    if let Ok(t) = d2_client::app::strings::TableStrings::load(
        d.archives.as_ref(),
        d2_client::app::strings::LANG,
    ) {
        art = art.with_strings(move |id| u16::try_from(id).map(|i| t.by_id(i)).unwrap_or_default());
    }
    use d2_data::bin::TableFiles;
    let (h, _handles) = front_host(&work, Some(art), d.archives.lod(), Entry::MainMenu);
    let mut a = app(h);
    ticks(&mut a, 30);

    let mods = vec![
        entry(
            &work,
            "ember-age",
            "Ember Age",
            ModStatus::Ready,
            Some([230, 120, 30]),
        ),
        entry(
            &work,
            "frost-road",
            "Frost Road",
            ModStatus::Ready,
            Some([120, 190, 235]),
        ),
        entry(&work, "plain-mod", "Plain Mod", ModStatus::Ready, None),
        entry(
            &work,
            "old-mod",
            "Old Mod",
            ModStatus::NeedsUpdate(
                "This mod needs updating: ask its author for a version made for this D2Gamma."
                    .to_owned(),
            ),
            Some([160, 160, 160]),
        ),
        entry(
            &work,
            "future-mod",
            "Future Mod",
            ModStatus::TooNew(
                "This mod needs a newer D2Gamma: update the game to play it.".to_owned(),
            ),
            None,
        ),
        {
            let mut broken = entry(
                &work,
                "broken-mod",
                "broken-mod",
                ModStatus::Unavailable("mod.toml: `version`: not a version".to_owned()),
                None,
            );
            broken.author.clear();
            broken.version.clear();
            broken.description.clear();
            broken
        },
    ];

    // The main menu with mods installed: the third button is D2GAMMA MODS.
    let _ = set(&mut a, mods.clone(), MenuButton::Mods);
    shot(&mut a, &out, "mods_01_main_menu_with_mods");
    click_id(&mut a, "menu.mods");
    assert_eq!(host(&mut a).front.current(), screens::ids::MODS);
    ticks(&mut a, 3);
    shot(&mut a, &out, "mods_02_list_first_page");
    // Select the second mod, then turn the page.
    click_id(&mut a, "mods.mod(frost-road)");
    ticks(&mut a, 3);
    shot(&mut a, &out, "mods_03_list_second_mod_selected");
    host(&mut a).front.input(FrontInput::Key(34));
    ticks(&mut a, 3);
    shot(&mut a, &out, "mods_04_list_last_page_unavailable_mods");
    click_id(&mut a, "mods.mod(old-mod)");
    ticks(&mut a, 3);
    shot(&mut a, &out, "mods_05_needs_update_selected_play_off");

    // Nothing installed: the list says where mods go.
    let _ = set(&mut a, Vec::new(), MenuButton::Mods);
    click_id(&mut a, "menu.mods");
    ticks(&mut a, 3);
    shot(&mut a, &out, "mods_06_empty_list");

    // Inside a mod: the third button is BACK TO D2GAMMA.
    let fixed = set(&mut a, mods, MenuButton::Back);
    shot(&mut a, &out, "mods_07_main_menu_inside_a_mod");
    click_id(&mut a, "menu.back_to_d2gamma");
    ticks(&mut a, 3);
    assert_eq!(*fixed.picks.borrow(), [Switch::Plain]);
    assert_eq!(host(&mut a).front.outcome(), Some(Outcome::Exit));
    let _ = std::fs::remove_dir_all(&work);
}
