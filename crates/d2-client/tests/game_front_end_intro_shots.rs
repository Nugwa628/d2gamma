//! A mod's intro and window icon on the user's install, rendered without a
//! window (`FrontHost::native_rgba`): the trademark screen as the game has
//! always shown it, the same screen with a mod's own lines and no logo, and
//! the icon a mod ships. Ignored: needs `D2_GAME_DIR`; `D2_SHOTS` names the
//! output folder (default: the temp dir).

use std::path::PathBuf;

use d2_client::app::front_host::FrontArt;
use d2_client::app::front_start::{front_host, Entry};
use d2_client::app::single_player::GameData;
use d2_client::mod_look::{load_icon, Intro, ICON_PATH};
use d2_client::ui::front_end::screens::trademark;
use d2_client::ui::front_end::TRADEMARK;

use super::game_front_end_mods_shots::{app, host, shot, ticks, write_png};

/// A made-up mod icon: a rounded square with a flame-like triangle.
fn icon_pixels(side: u32) -> Vec<u8> {
    let mut px = Vec::with_capacity((side * side * 4) as usize);
    let s = side as f32;
    for y in 0..side {
        for x in 0..side {
            let (fx, fy) = (x as f32 / s, y as f32 / s);
            let (cx, cy) = ((fx - 0.5).abs(), (fy - 0.5).abs());
            let inside = cx < 0.42 && cy < 0.42 && !(cx > 0.34 && cy > 0.34);
            // A triangle pointing up.
            let flame = fy > 0.22 && fy < 0.78 && (fx - 0.5).abs() < (fy - 0.22) * 0.5;
            let c = if flame {
                [240, 150, 40, 255]
            } else if inside {
                [40, 50, 90, 255]
            } else {
                [0, 0, 0, 0]
            };
            px.extend(c);
        }
    }
    px
}

#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn intro_and_icon_on_the_install() {
    let game = PathBuf::from(std::env::var("D2_GAME_DIR").expect("D2_GAME_DIR"));
    let out = std::env::var_os("D2_SHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out).unwrap();
    let work = std::env::temp_dir().join(format!("d2g-intro-shots-{}", std::process::id()));
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
    let (h, _handles) = front_host(&work, Some(art), d.archives.lod(), Entry::First);
    let mut a = app(h);
    assert_eq!(host(&mut a).front.current(), TRADEMARK);

    // The trademark screen as it always was (no mod).
    ticks(&mut a, 40);
    shot(&mut a, &out, "intro_01_default_trademark");

    // The same screen with a mod's intro: its own lines, no logo animation.
    let intro = Intro {
        skip: false,
        seconds: 5,
        logo: false,
        text: Some(vec![
            "Ember Age".to_owned(),
            "A mod by Ada Modder".to_owned(),
            "Press any key".to_owned(),
        ]),
    };
    host(&mut a)
        .front
        .register_screen(TRADEMARK, trademark::screen(intro));
    ticks(&mut a, 40);
    shot(&mut a, &out, "intro_02_mod_text_no_logo");

    // The icon a mod ships, read the way the game reads it.
    let dir = work.join("files/mod");
    std::fs::create_dir_all(&dir).unwrap();
    let side = 128;
    write_png(
        &work.join(ICON_PATH),
        side,
        side,
        png::ColorType::Rgba,
        &icon_pixels(side),
    );
    let icon = load_icon(&work.join(ICON_PATH)).expect("the game reads the icon");
    assert_eq!((icon.w, icon.h), (side, side));
    assert!(d2_client::app::window_icon::window_icon(&icon).is_some());
    write_png(
        &out.join("intro_03_mod_window_icon.png"),
        side,
        side,
        png::ColorType::Rgba,
        &icon.rgba,
    );
    let _ = std::fs::remove_dir_all(&work);
}
