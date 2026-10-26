use super::*;

fn menu() -> OptionsMenu {
    let mut o = OptionsMenu::default();
    o.set_settings(Settings::default());
    o.open();
    o.take_events();
    o.take_changed();
    o
}

fn row_of(r: Row) -> usize {
    ROWS.iter().position(|d| d.row == r).unwrap()
}

fn pt(x: i32, y: i32) -> Point {
    Point::new(x, y)
}

/// The Game page opens on Return to Game; the arrows wrap; Options asks
/// for the settings menu and stays on the page.
#[test]
fn opens_on_return_to_game_and_options_asks_for_the_settings_menu() {
    let mut o = OptionsMenu::default();
    o.open();
    assert_eq!(o.selected, 2);
    assert_eq!(o.pentagram_y(), 285 + 51);
    // Down wraps to Options.
    o.key_down();
    assert_eq!(o.selected, 0);
    assert_eq!(o.take_events(), vec![MenuEvent::CursorPass]);
    o.key_up();
    assert_eq!(o.selected, 2);
    o.take_events();
    o.selected = row_of(Row::Options);
    o.key_enter();
    assert_eq!(o.selected, row_of(Row::Options));
    assert_eq!(
        o.take_events(),
        vec![MenuEvent::CursorSelect, MenuEvent::OpenSettings]
    );
    // Back from Configure Controls: the page, Return to Game selected.
    o.return_from_controls();
    assert_eq!(o.selected, row_of(Row::ReturnToGame));
}

#[test]
fn save_and_exit_and_return_are_events() {
    let mut o = menu();
    o.selected = 1;
    o.key_enter();
    assert!(o.take_events().contains(&MenuEvent::SaveAndExit));
    o.selected = 2;
    o.key_enter();
    assert!(o.take_events().contains(&MenuEvent::Close));
}

#[test]
fn row_tops_follow_the_position_table() {
    let o = menu();
    let tops = (0..o.rows().len()).map(|i| o.y_top(i)).collect::<Vec<_>>();
    assert_eq!(tops, [185, 235, 285]);
    assert_eq!(o.baseline(0), 185 + 39);
}

#[test]
fn row_tops_at_640_follow_the_position_table() {
    let mut o = menu();
    assert_eq!(o.half(), 400);
    o.screen = Screen::R640;
    assert_eq!(o.half(), 320);
    // The 640 × 480 column of the table (h = 320).
    let tops = (0..o.rows().len()).map(|i| o.y_top(i)).collect::<Vec<_>>();
    assert_eq!(tops, [125, 175, 225]);
    // Row hit-test follows the same y: the second row's top at 640.
    assert_eq!(o.row_at(o.y_top(1) + 3 + 3), Some(1));
}

#[test]
fn pointer_rows_follow_the_row_bands() {
    let o = menu();
    assert_eq!(o.row_at(185), None, "the span's top edge");
    assert_eq!(o.row_at(200), Some(0));
    assert_eq!(o.row_at(240), Some(1));
    assert_eq!(o.row_at(300), Some(2));
    assert_eq!(o.row_at(335), None, "the span's bottom edge");
}

#[test]
fn click_activates_on_release_and_hover_selects() {
    let mut o = menu();
    o.moved(pt(400, 200));
    assert_eq!(o.selected, 0);
    o.press(pt(400, 200));
    assert!(
        !o.take_events().contains(&MenuEvent::OpenSettings),
        "not yet"
    );
    o.release(pt(400, 200));
    assert!(o.take_events().contains(&MenuEvent::OpenSettings));
    // Released elsewhere: nothing.
    o.press(pt(400, 240));
    o.release(pt(400, 300));
    assert!(o.take_events().is_empty());
}

#[test]
fn each_row_has_one_click_element_on_its_band() {
    let o = menu();
    let ids = o.ui_elements();
    let names: Vec<_> = ids.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        names,
        [
            "options.game.options",
            "options.game.exit_game",
            "options.game.return_to_game"
        ]
    );
    for (i, e) in ids.iter().enumerate() {
        let mid = e.rect.y + i32::from(e.rect.h) / 2;
        assert_eq!(o.row_at(mid), Some(i), "{}", e.id);
    }
}

/// The settings menu's changes come back as one change for the host.
#[test]
fn a_change_is_handed_over_once() {
    let mut o = menu();
    assert_eq!(o.take_changed(), None);
    o.settings.gamma = 205;
    o.changed = true;
    assert_eq!(o.take_changed().map(|s| s.gamma), Some(205));
    assert_eq!(o.take_changed(), None);
}
