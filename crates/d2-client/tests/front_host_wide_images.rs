//! The front end's real Bevy systems on the 16:9 realm lobby, headless:
//! every image's pixels fit its size once the lobby is laid out, through
//! a window resize and an HD Text toggle, side fill or not. The owner's
//! release crashed on its first lobby frame on a real GPU: the picture
//! image was made 16:9 while the frame put in it stayed 800 × 600, and the
//! upload (`create_texture_with_data`) read past its end. No game files.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use d2_client::app::config::{MenuSides, Settings, WindowChoice};
use d2_client::app::front_host::{add_front_end, FrontHost};
use d2_client::app::image_sizes::{mismatch, want_len};
use d2_client::rules::camera::FrameSize;
use d2_client::ui::front_end::screens::online::fake::{character, FakeServer, SharedFake};
use d2_client::ui::front_end::screens::online::{self, Online, ServerEntry};
use d2_client::ui::front_end::*;
use d2_formats::d2s::status;

const WINDOWS: [(u32, u32); 3] = [(1280, 720), (1920, 1080), (3840, 2160)];

fn lobby_app(sides: MenuSides, (w, h): (u32, u32)) -> (App, Entity) {
    let mut fake = FakeServer::default();
    fake.server_name = "D2Gamma".into();
    fake.add_account(
        "Nugwa",
        "password1",
        vec![character("Werwer", 4, 34, 5, status::EXPANSION)],
    );
    let online = Online::new(
        Box::new(SharedFake(std::rc::Rc::new(std::cell::RefCell::new(fake)))),
        vec![ServerEntry {
            id: "eu1".into(),
            name: "D2Gamma".into(),
            address: "127.0.0.1:6112".into(),
        }],
    );
    let mut reg = Registry::default();
    screens::register_all(&mut reg);
    online::register_with(&mut reg, online);
    let host = FrontHost::with_front(FrontEnd::new(true, Box::new(false), reg), None, false);
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::asset::AssetPlugin::default(),
        bevy::input::InputPlugin,
    ))
    .init_asset::<Image>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        40,
    )))
    .insert_resource(WindowChoice::of(&Settings {
        menu_sides: sides,
        ..Settings::default()
    }));
    add_front_end(&mut app, host);
    let mut win = Window {
        resolution: bevy::window::WindowResolution::new(w, h).with_scale_factor_override(1.0),
        ..Default::default()
    };
    win.set_physical_cursor_position(None);
    let win = app
        .world_mut()
        .spawn((win, bevy::window::PrimaryWindow))
        .id();
    app.update();
    // Log in and into the lobby.
    front(&mut app).trigger(Trigger::Online);
    ticks(&mut app, 4);
    typed(&mut app, "nugwa");
    front(&mut app).input(FrontInput::Key(9));
    typed(&mut app, "password1");
    front(&mut app).input(FrontInput::Key(13));
    ticks(&mut app, 6);
    click(&mut app, "online_chars.char(Werwer)");
    click(&mut app, "online_chars.ok");
    ticks(&mut app, 6);
    assert_eq!(front(&mut app).current(), online::ONLINE_LOBBY);
    (app, win)
}

fn front(app: &mut App) -> &mut FrontEnd {
    &mut app
        .world_mut()
        .non_send_mut::<FrontHost>()
        .into_inner()
        .front
}

fn ticks(app: &mut App, n: u32) {
    for _ in 0..n {
        app.update();
    }
}

fn typed(app: &mut App, s: &str) {
    for u in s.encode_utf16() {
        front(app).input(FrontInput::Char(u));
    }
    ticks(app, 2);
}

fn click(app: &mut App, id: &str) {
    let at = front(app)
        .ui_elements()
        .into_iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no {id}"))
        .at;
    front(app).input(FrontInput::Move(at));
    front(app).input(FrontInput::Down(at));
    ticks(app, 1);
    front(app).input(FrontInput::Up(at));
    ticks(app, 3);
}

/// Every image with pixels has as many as its size asks for, and the
/// front end's picture is the wide canvas's.
fn all_fit(app: &App, what: &str) {
    let images = app.world().resource::<Assets<Image>>();
    let mut checked = 0;
    for (id, image) in images.iter() {
        let (w, h) = (image.width(), image.height());
        assert_eq!(mismatch(image), None, "{what}: image {id:?} {w} x {h}");
        if image.data.is_some() && want_len(image).is_some() {
            checked += 1;
        }
    }
    let wide = FrameSize::WIDE;
    assert!(
        images.iter().any(|(_, i)| (i.width(), i.height())
            == (wide.width as u32, wide.height as u32)
            && i.data.as_ref().is_some_and(|d| d.iter().any(|&b| b != 0))),
        "{what}: no drawn {} x {} picture",
        wide.width,
        wide.height
    );
    assert!(checked >= 2, "{what}: {checked} images checked");
}

#[test]
fn every_image_fits_its_size_on_the_wide_lobby() {
    for sides in MenuSides::ALL {
        for (w, h) in WINDOWS {
            let what = format!("{w}x{h} {sides:?}");
            let (mut app, win) = lobby_app(sides, (w, h));
            assert!(front(&mut app).wide(), "{what}");
            all_fit(&app, &what);
            // HD Text on and off again.
            for n in 0..2 {
                click(&mut app, "online_lobby.hd_text");
                all_fit(&app, &format!("{what} hd toggle {n}"));
            }
            // Through every other window size, and back.
            for (w2, h2) in WINDOWS.into_iter().chain([(w, h)]) {
                app.world_mut()
                    .get_mut::<Window>(win)
                    .unwrap()
                    .resolution
                    .set_physical_resolution(w2, h2);
                ticks(&mut app, 2);
                all_fit(&app, &format!("{what} resized to {w2}x{h2}"));
            }
            // Out of the lobby (4:3 again) and back in.
            front(&mut app).input(FrontInput::Key(27));
            ticks(&mut app, 4);
            all_fit_any(&app, &format!("{what} left"));
        }
    }
}

/// [`all_fit`] without the wide picture: any screen.
fn all_fit_any(app: &App, what: &str) {
    for (id, image) in app.world().resource::<Assets<Image>>().iter() {
        assert_eq!(mismatch(image), None, "{what}: image {id:?}");
    }
}
