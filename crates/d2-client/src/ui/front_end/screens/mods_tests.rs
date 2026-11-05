//! The D2Gamma Mods list and the main menu's door to it, on a fixed list of
//! mods. No game files.

use std::cell::RefCell;
use std::path::PathBuf;

use super::*;
use crate::ui::front_end::screens::ids::MAIN_MENU;
use crate::ui::front_end::{FrontEnd, FrontInput, Outcome};
use crate::ui::ui_ids;

struct Fixed {
    entries: Vec<ModEntry>,
    button: MenuButton,
    picks: RefCell<Vec<Switch>>,
    offer: Option<crate::mod_download::Prompt>,
    downloads: RefCell<Vec<crate::mod_download::Confirmed>>,
    copies: RefCell<Vec<(String, String)>>,
}

impl ModsHost for Rc<Fixed> {
    fn characters_to_copy(&self) -> Vec<String> {
        vec!["Traveller".into(), "Walker".into()]
    }
    fn copy_character(&self, id: &str, name: &str) -> Result<(), String> {
        self.copies.borrow_mut().push((id.into(), name.into()));
        Ok(())
    }
    fn installed(&self) -> Vec<ModEntry> {
        self.entries.clone()
    }

    fn menu_button(&self) -> MenuButton {
        self.button
    }

    fn switch(&self, s: Switch) {
        self.picks.borrow_mut().push(s);
    }
    fn download_offer(&self) -> Option<crate::mod_download::Prompt> {
        self.offer.clone()
    }
    fn request_download(&self, request: crate::mod_download::Confirmed) -> Result<(), String> {
        self.downloads.borrow_mut().push(request);
        Ok(())
    }
}

fn entry(id: &str, status: ModStatus) -> ModEntry {
    ModEntry {
        id: id.to_owned(),
        name: format!("Mod {id}"),
        author: "Ada".to_owned(),
        version: "1.2.0".to_owned(),
        accepts_d2gamma_characters: false,
        description: "Harder fire.".to_owned(),
        status,
        picture: None,
        dir: PathBuf::from(id),
    }
}

fn ready(id: &str) -> ModEntry {
    entry(id, ModStatus::Ready)
}

fn front(entries: Vec<ModEntry>, button: MenuButton) -> (FrontEnd, Rc<Fixed>) {
    front_with_offer(entries, button, None)
}
fn front_with_offer(
    entries: Vec<ModEntry>,
    button: MenuButton,
    offer: Option<crate::mod_download::Prompt>,
) -> (FrontEnd, Rc<Fixed>) {
    let host = Rc::new(Fixed {
        entries,
        button,
        picks: RefCell::new(Vec::new()),
        offer,
        downloads: RefCell::new(Vec::new()),
        copies: RefCell::new(Vec::new()),
    });
    let mut reg = Registry::default();
    crate::ui::front_end::screens::register_all(&mut reg);
    let dynhost: Rc<dyn ModsHost> = Rc::new(host.clone());
    register_with(&mut reg, dynhost.clone());
    main_menu::register_with(&mut reg, dynhost);
    let mut f = FrontEnd::new(true, Box::new(false), reg);
    f.start(false);
    assert_eq!(f.current(), MAIN_MENU);
    (f, host)
}

fn ids(f: &FrontEnd) -> Vec<String> {
    f.ui_elements().into_iter().map(|e| e.id).collect()
}

fn click_at(f: &mut FrontEnd, at: Point) {
    f.input(FrontInput::Move(at));
    f.input(FrontInput::Down(at));
    f.input(FrontInput::Up(at));
    f.tick();
}

fn click(f: &mut FrontEnd, id: &str) {
    let els = f.ui_elements();
    let at = ui_ids::find(&els, id).unwrap_or_else(|e| panic!("{e}")).at;
    click_at(f, at);
}

fn key(f: &mut FrontEnd, k: u16) {
    f.input(FrontInput::Key(k));
    f.tick();
}

fn open(entries: Vec<ModEntry>) -> (FrontEnd, Rc<Fixed>) {
    let (mut f, host) = front(entries, MenuButton::Mods);
    click(&mut f, "menu.mods");
    assert_eq!(f.current(), MODS);
    (f, host)
}

fn texts(f: &mut FrontEnd) -> Vec<(i32, String)> {
    f.overlay(&|_, t| t.len() as i32 * 6)
        .into_iter()
        .filter_map(|d| match d {
            DrawItem::Text { text, color, .. } => Some((color, text)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_main_menu_is_unchanged_without_mods() {
    // Stock screens, a host with nothing installed, and a host that says
    // Stock all give the same controls.
    let mut stock = FrontEnd::with_screens(true, Box::new(false));
    stock.start(false);
    let (none, _) = front(Vec::new(), MenuButton::Stock);
    assert_eq!(none.controls(), stock.controls());
    assert!(!ids(&none)
        .iter()
        .any(|i| i.contains("mods") || i.contains("back")));
}

#[test]
fn the_third_button_opens_the_mods_and_back_returns() {
    let (mut f, _) = front(vec![ready("ember-age")], MenuButton::Mods);
    let label = f
        .controls()
        .iter()
        .find(|c| c.text.as_deref() == Some("D2GAMMA MODS"))
        .expect("the button");
    // It sits where Other Multiplayer sat, as a wide button.
    assert_eq!((label.x, label.y, label.w, label.h), (264, 433, 272, 35));
    assert!(ids(&f).contains(&"menu.mods".to_owned()));
    click(&mut f, "menu.mods");
    assert_eq!(f.current(), MODS);
    assert_eq!(f.outcome(), None);
    click(&mut f, "mods.back");
    assert_eq!(f.current(), MAIN_MENU);
    // Esc goes back too.
    click(&mut f, "menu.mods");
    key(&mut f, 27);
    assert_eq!(f.current(), MAIN_MENU);
}

#[test]
fn back_to_d2gamma_restarts_plain() {
    let (mut f, host) = front(Vec::new(), MenuButton::Back);
    assert!(f
        .controls()
        .iter()
        .any(|c| c.text.as_deref() == Some("BACK TO D2GAMMA")));
    click(&mut f, "menu.back_to_d2gamma");
    assert_eq!(f.outcome(), Some(Outcome::Exit));
    assert_eq!(*host.picks.borrow(), [Switch::Plain]);
}

#[test]
fn picking_a_mod_hands_it_to_the_host_and_ends_the_front_end() {
    let (mut f, host) = open(vec![ready("aaa-mod"), ready("bbb-mod")]);
    assert_eq!(
        ids(&f)
            .into_iter()
            .filter(|i| i.starts_with("mods.mod"))
            .collect::<Vec<_>>(),
        ["mods.mod(aaa-mod)", "mods.mod(bbb-mod)"]
    );
    // One click selects; PLAY starts the selected one.
    click(&mut f, "mods.mod(bbb-mod)");
    assert!(host.picks.borrow().is_empty());
    click(&mut f, "mods.play");
    assert_eq!(f.outcome(), Some(Outcome::Exit));
    assert_eq!(*host.picks.borrow(), [Switch::Mod("bbb-mod".into())]);
}

#[test]
fn enter_and_a_double_click_start_the_selected_mod() {
    let (mut f, host) = open(vec![ready("aaa-mod"), ready("bbb-mod")]);
    // The first mod that can be started is selected on opening.
    key(&mut f, 13);
    assert_eq!(*host.picks.borrow(), [Switch::Mod("aaa-mod".into())]);

    let (mut f, host) = open(vec![ready("aaa-mod"), ready("bbb-mod")]);
    let els = f.ui_elements();
    let at = ui_ids::find(&els, "mods.mod(bbb-mod)").unwrap().at;
    click_at(&mut f, at);
    assert!(host.picks.borrow().is_empty());
    click_at(&mut f, at);
    assert_eq!(*host.picks.borrow(), [Switch::Mod("bbb-mod".into())]);
    assert_eq!(f.outcome(), Some(Outcome::Exit));
}

#[test]
fn a_mod_that_cannot_start_cannot_be_picked() {
    let old = entry(
        "old-mod",
        ModStatus::NeedsUpdate("This mod needs updating.".into()),
    );
    let new = entry(
        "new-mod",
        ModStatus::TooNew("This mod needs a newer D2Gamma.".into()),
    );
    let (mut f, host) = open(vec![old, new, ready("fine-mod")]);
    // Opens on the first one that can start.
    key(&mut f, 13);
    assert_eq!(*host.picks.borrow(), [Switch::Mod("fine-mod".into())]);

    let (mut f, host) = open(vec![
        entry(
            "old-mod",
            ModStatus::NeedsUpdate("This mod needs updating.".into()),
        ),
        entry("new-mod", ModStatus::TooNew("needs a newer D2Gamma".into())),
    ]);
    // Nothing can start: PLAY is off, Enter and a double click do nothing.
    assert!(
        !f.controls()
            .iter()
            .find(|c| c.text.as_deref() == Some("PLAY"))
            .unwrap()
            .enabled
    );
    key(&mut f, 13);
    let els = f.ui_elements();
    let at = ui_ids::find(&els, "mods.mod(old-mod)").unwrap().at;
    click_at(&mut f, at);
    click_at(&mut f, at);
    assert!(host.picks.borrow().is_empty());
    assert_eq!(f.outcome(), None);
    // The list says why, in red, in the kit's words.
    let t = texts(&mut f);
    assert!(
        t.contains(&(RED, "This mod needs updating.".to_owned())),
        "{t:?}"
    );
    assert!(
        t.contains(&(RED, "needs a newer D2Gamma".to_owned())),
        "{t:?}"
    );
}

#[test]
fn a_row_shows_name_author_version_and_description() {
    let (mut f, _) = open(vec![ready("ember-age")]);
    let t = texts(&mut f);
    for want in [
        "Mod ember-age",
        "by Ada    v1.2.0",
        "Harder fire.",
        "Ready to play",
    ] {
        assert!(t.iter().any(|(_, s)| s == want), "{want}: {t:?}");
    }
}

#[test]
fn the_selection_and_pages_move_with_the_keys_and_the_wheel() {
    let many: Vec<_> = (0..10).map(|i| ready(&format!("mod-{i:02}"))).collect();
    let (mut f, host) = open(many);
    let visible = |f: &FrontEnd| {
        ids(f)
            .into_iter()
            .filter(|i| i.starts_with("mods.mod"))
            .collect::<Vec<_>>()
    };
    assert_eq!(visible(&f).len(), ROWS);
    assert_eq!(visible(&f)[0], "mods.mod(mod-00)");
    // Down past the page scrolls with the selection.
    for _ in 0..5 {
        key(&mut f, 40);
    }
    assert_eq!(visible(&f)[0], "mods.mod(mod-02)");
    key(&mut f, 13);
    assert_eq!(*host.picks.borrow(), [Switch::Mod("mod-05".into())]);

    let (mut f, _) = open((0..10).map(|i| ready(&format!("mod-{i:02}"))).collect());
    key(&mut f, 34);
    assert_eq!(visible(&f)[0], "mods.mod(mod-04)");
    key(&mut f, 34);
    key(&mut f, 34);
    // The last page holds the last rows and stops there.
    assert_eq!(visible(&f)[0], "mods.mod(mod-06)");
    key(&mut f, 33);
    assert_eq!(visible(&f)[0], "mods.mod(mod-02)");
    key(&mut f, 36);
    assert_eq!(visible(&f)[0], "mods.mod(mod-00)");
    f.input(FrontInput::Wheel(-120));
    f.tick();
    assert_eq!(visible(&f)[0], "mods.mod(mod-01)");
    f.input(FrontInput::Wheel(120));
    f.input(FrontInput::Wheel(120));
    f.tick();
    assert_eq!(visible(&f)[0], "mods.mod(mod-00)");
    let t = texts(&mut f);
    assert!(t.iter().any(|(_, s)| s == "1-4 of 10"), "{t:?}");
}

#[test]
fn the_page_count_is_in_a_font_that_draws_every_digit() {
    // The small font's 6 looks like its 0 ("1-4 of 6" read "1-4 of 0").
    let (mut f, _) = open((0..6).map(|i| ready(&format!("mod-{i:02}"))).collect());
    let count = f
        .overlay(&|_, t| t.len() as i32 * 6)
        .into_iter()
        .find_map(|d| match d {
            DrawItem::Text { text, font, .. } if text == "1-4 of 6" => Some(font),
            _ => None,
        });
    assert_eq!(count, Some(FONT16));
}

#[test]
fn an_empty_list_says_where_mods_go() {
    let (mut f, host) = open(Vec::new());
    let t = texts(&mut f);
    assert!(
        t.iter().any(|(_, s)| s == "No mods are installed."),
        "{t:?}"
    );
    key(&mut f, 13);
    assert!(host.picks.borrow().is_empty());
    assert_eq!(f.current(), MODS);
}

#[test]
fn a_mod_picture_is_drawn_in_its_row_and_a_missing_one_is_not() {
    let dir = std::env::temp_dir().join(format!("d2g-mods-pic-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("picture.png");
    {
        let file = std::fs::File::create(&path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), 16, 8);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let data: Vec<u8> = (0..16 * 8)
            .flat_map(|i| [(i % 16 * 16) as u8, 90, 40])
            .collect();
        enc.write_header().unwrap().write_image_data(&data).unwrap();
    }
    let mut with = ready("with-pic");
    with.picture = Some(path);
    let mut missing = ready("no-file");
    missing.picture = Some(dir.join("gone.png"));
    let (mut f, _) = open(vec![with, missing, ready("no-pic")]);
    let pictures: Vec<_> = f
        .overlay(&|_, t| t.len() as i32 * 6)
        .into_iter()
        .filter_map(|d| match d {
            DrawItem::Picture { w, h, rgb, .. } => Some((w, h, rgb.len())),
            _ => None,
        })
        .collect();
    // Small pictures are not scaled up.
    assert_eq!(pictures, [(16, 8, 16 * 8 * 3)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_rows_are_ui_elements_inside_the_list() {
    let (f, _) = open(vec![ready("aaa-mod")]);
    for e in f
        .ui_elements()
        .iter()
        .filter(|e| e.id.starts_with("mods.mod"))
    {
        assert_eq!(e.rect.x, LIST_X);
        assert!(e.rect.contains(e.at));
    }
}

#[test]
fn official_download_requires_click_with_author_size_visible_and_cancel_is_inert() {
    use crate::mod_download::{Offer, Prompt, Required, Settings};
    let offer = Offer {
        id: "ghost-world".into(),
        version: "1.0.0".into(),
        author: "Nugwa".into(),
        bytes: 123456,
        package_url: "https://official.example/mod.d2pack".into(),
        sha256: "a".repeat(64),
        author_key: "b".repeat(64),
    };
    let url = "https://official.example/mods";
    let prompt = Prompt::from_official(
        &Settings {
            official_list_url: url.into(),
        },
        url,
        &Required {
            id: offer.id.clone(),
            version: offer.version.clone(),
        },
        &serde_json::to_vec(&offer).unwrap(),
    )
    .unwrap();
    for confirm in [false, true] {
        let (mut f, host) = front_with_offer(Vec::new(), MenuButton::Mods, Some(prompt.clone()));
        click(&mut f, "menu.mods");
        assert!(f.controls().iter().any(|c| c
            .text
            .as_ref()
            .is_some_and(|t| t.contains("Nugwa") && t.contains("123456 bytes"))));
        key(&mut f, vk::ENTER);
        assert!(host.downloads.borrow().is_empty());
        assert!(host.picks.borrow().is_empty());
        if confirm {
            click(&mut f, "mods.download");
            assert_eq!(host.downloads.borrow().len(), 1);
        } else {
            key(&mut f, vk::ESC);
            assert!(host.downloads.borrow().is_empty());
            assert_eq!(f.current(), MODS);
        }
    }
}

#[test]
fn character_copy_offer_is_opt_in_and_requires_an_explicit_confirmation_click() {
    let (f, _) = open(vec![ready("closed-world")]);
    assert!(!ids(&f).contains(&"mods.copy_character".into()));
    for confirm in [false, true] {
        let mut mod_entry = ready("new-world");
        mod_entry.accepts_d2gamma_characters = true;
        let (mut f, host) = open(vec![mod_entry]);
        click(&mut f, "mods.copy_character");
        assert!(f
            .controls()
            .iter()
            .any(|c| c.text.as_ref().is_some_and(
                |t| t.contains("Traveller") && t.contains("original stays unchanged")
            )));
        key(&mut f, vk::ENTER);
        assert!(host.copies.borrow().is_empty());
        assert!(host.picks.borrow().is_empty());
        if confirm {
            click(&mut f, "mods.copy_next");
            click(&mut f, "mods.copy_confirm");
            assert_eq!(
                *host.copies.borrow(),
                [("new-world".into(), "Walker".into())]
            );
        } else {
            key(&mut f, vk::ESC);
            assert!(host.copies.borrow().is_empty());
        }
        assert_eq!(f.current(), MODS);
    }
}
