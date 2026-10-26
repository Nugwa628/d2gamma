//! The Ladder panel against the fake's ladder (`fake/ladder.rs`): the
//! chips' query, the pinned own row, find, My rank, the pages, the marks,
//! and the rank line's answers kept apart from the panel's pages.

use d2_formats::d2s::status;

use super::*;
use crate::ui::front_end::screens::online::fake::{character, FakeServer, SharedFake};
use crate::ui::front_end::screens::online::lobby::PANEL;
use crate::ui::front_end::screens::online::Online;

/// Every glyph 8 px wide in Font16; in the small font capitals 8 px and
/// the rest 6 px: wider than the game's fonts (its Font8 capitals and
/// digits are wider than its lower case), so what fits here fits there.
fn adv(font: u16, t: &[u16]) -> i32 {
    t.iter()
        .map(|&u| match font {
            FONT8 if (u16::from(b'A')..=u16::from(b'Z')).contains(&u) => 8,
            FONT8 => 6,
            _ => 8,
        })
        .sum()
}

fn w8(s: &str) -> i32 {
    adv(FONT8, &s.encode_utf16().collect::<Vec<u16>>())
}

/// "Werwer", a level 34 Barbarian of Season 1 (ranked 45th of 61 among
/// the fake's softcore expansion characters).
fn werwer() -> CharInfo {
    let mut c = character("Werwer", 4, 34, 5, status::EXPANSION);
    c.season = 1;
    c
}

/// Account "Nugwa" logged in with Werwer selected; the panel on the fake.
fn scene_with(edit: impl FnOnce(&mut FakeServer)) -> (LadderPanel, SharedFake) {
    let mut fake = FakeServer::default();
    fake.add_account("Nugwa", "password1", vec![werwer()]);
    edit(&mut fake);
    let shared = SharedFake::default();
    *shared.0.borrow_mut() = fake;
    let online = Online::new(Box::new(shared.clone()), Vec::new());
    for r in [
        Request::Connect { index: 0 },
        Request::Login {
            account: "Nugwa".into(),
            password: "password1".into(),
        },
        Request::CharSelect {
            name: "Werwer".into(),
        },
    ] {
        online.borrow_mut().send(r);
    }
    online.borrow_mut().take_events();
    (LadderPanel::new(online), shared)
}

fn scene() -> (LadderPanel, SharedFake) {
    scene_with(|_| {})
}

/// The link's events to the panel.
fn pump(p: &mut LadderPanel) {
    let evs = p.online.borrow_mut().take_events();
    for e in &evs {
        p.feed(e);
    }
}

fn opened() -> (LadderPanel, SharedFake) {
    let (mut p, f) = scene();
    p.open();
    pump(&mut p);
    (p, f)
}

fn last_query(f: &SharedFake) -> LadderQuery {
    f.0.borrow()
        .seen
        .iter()
        .rev()
        .find_map(|r| match r {
            Request::LadderQuery(q) => Some(q.clone()),
            _ => None,
        })
        .expect("a ladder query")
}

fn queries(f: &SharedFake) -> usize {
    f.0.borrow()
        .seen
        .iter()
        .filter(|r| matches!(r, Request::LadderQuery(_)))
        .count()
}

fn element(p: &LadderPanel, id: &str) -> Point {
    p.ui_elements()
        .into_iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no {id}"))
        .at
}

fn click(p: &mut LadderPanel, id: &str) -> Option<PanelOut> {
    let at = element(p, id);
    let out = p.mouse(at, 0);
    pump(p);
    out
}

/// The overlay's texts: (text, colour, x, baseline).
fn texts(p: &mut LadderPanel) -> Vec<(String, i32, i32, i32)> {
    p.overlay(PANEL, &adv, 0)
        .into_iter()
        .filter_map(|d| match d {
            DrawItem::Text {
                text, color, at, ..
            } => Some((text, color, at.x, at.y)),
            _ => None,
        })
        .collect()
}

fn ranks(p: &LadderPanel) -> Vec<u32> {
    p.page
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .map(|r| r.rank)
        .collect()
}

/// Rule (plan L11): opening asks for the seasons and the character's own
/// view; each chip changes one part of the query and asks for page 1.
#[test]
fn the_chips_make_the_query() {
    let (mut p, f) = opened();
    assert!(f.0.borrow().seen.contains(&Request::LadderSeasons));
    let exp = ladder_split::EXPANSION;
    let hc = ladder_split::HARDCORE;
    assert_eq!(
        last_query(&f),
        LadderQuery {
            season: 1,
            split: exp,
            class: 255,
            at: LadderAt::Rank(1),
            rows: ROWS,
        }
    );
    let view = |f: &SharedFake| {
        let q = last_query(f);
        (q.season, q.split, q.class, q.at)
    };
    click(&mut p, &Chip::Hardcore.id());
    assert_eq!(view(&f), (1, hc | exp, 255, LadderAt::Rank(1)));
    click(&mut p, &Chip::Classic.id());
    assert_eq!(view(&f), (1, hc, 255, LadderAt::Rank(1)));
    click(&mut p, &Chip::Softcore.id());
    click(&mut p, &Chip::Expansion.id());
    assert_eq!(view(&f), (1, exp, 255, LadderAt::Rank(1)));
    click(&mut p, &Chip::Class(0).id());
    assert_eq!(view(&f), (1, exp, 0, LadderAt::Rank(1)));
    assert!(p.page.as_ref().unwrap().rows.iter().all(|r| r.class == 0));
    click(&mut p, &Chip::Class(255).id());
    assert_eq!(view(&f), (1, exp, 255, LadderAt::Rank(1)));
    // The season chip steps through the realm's list: Season 1, then
    // non-ladder, then back.
    click(&mut p, &Chip::Season.id());
    assert_eq!(view(&f).0, 0);
    assert_eq!(season_label(p.seasons.as_deref(), p.season), "Non-ladder");
    click(&mut p, &Chip::Season.id());
    assert_eq!(view(&f).0, 1);
    assert!(texts(&mut p).iter().any(|t| t.0 == "SEASON 1"));
    // A page from another view turned to an earlier page: back to 1.
    p.wheel(-1);
    pump(&mut p);
    click(&mut p, &Chip::Class(4).id());
    assert_eq!(last_query(&f).at, LadderAt::Rank(1));
}

/// Rule (contract 6, `LadderHead`): the selected character ranked off the
/// page stands under it after `...`, yellow with "(you)".
#[test]
fn the_own_row_is_pinned_under_the_page() {
    let (mut p, _f) = opened();
    assert_eq!(ranks(&p), [1, 2, 3, 4, 5, 6]);
    let pinned = p.page.as_ref().unwrap().pinned.clone().expect("pinned");
    assert_eq!((pinned.char.as_str(), pinned.rank), ("Werwer", 45));
    let t = texts(&mut p);
    let me = t.iter().find(|t| t.0 == "Werwer (you)").expect("own row");
    assert_eq!((me.1, me.3), (YELLOW, PANEL.y + PINNED_BASE));
    assert!(t.iter().any(|t| t.0 == "..." && t.3 == PANEL.y + DOTS_BASE));
    assert!(t.iter().any(|t| t.0 == "1-6 of 61"));
    assert!(t.iter().any(|t| t.0 == LEGEND));
    // The character line's rank comes along with its own view.
    assert_eq!(p.rank, Some(45));
}

/// Rule (plan L11): My rank turns to the page with the character on it;
/// nothing is pinned then.
#[test]
fn my_rank_turns_to_the_own_page() {
    let (mut p, f) = opened();
    assert_eq!(p.action(MY_RANK), Some(PanelOut::Rebuild));
    pump(&mut p);
    assert_eq!(last_query(&f).at, LadderAt::Me);
    let page = p.page.as_ref().unwrap();
    assert_eq!(page.first_rank, 43);
    assert!(page.pinned.is_none());
    assert!(page
        .rows
        .iter()
        .any(|r| r.rank == 45 && r.flags & ladder_flag::ME != 0));
    // Not ranked in this view (the hardcore ladder): said under the rows.
    click(&mut p, &Chip::Hardcore.id());
    p.action(MY_RANK);
    pump(&mut p);
    assert_eq!(
        p.note,
        Some(("Your character is not on this ladder.".into(), GREY))
    );
}

/// Rule (contract 6, `find`): the whole name in any case; the page with it
/// is shown and its row framed; no such character is said in red and the
/// page stays.
#[test]
fn find_a_character() {
    let (mut p, f) = opened();
    assert!(
        !p.char('k'),
        "typing goes to the chat until the box is clicked"
    );
    assert_eq!(p.enter(), None, "Enter is JOIN's / the chat's");
    click(&mut p, "online_lobby.ladder_find");
    assert!(p.find_focus);
    for c in "kelmir!".chars() {
        assert!(p.char(c));
    }
    assert_eq!(p.find.text, "kelmir");
    assert_eq!(p.enter(), Some(PanelOut::Rebuild));
    pump(&mut p);
    assert_eq!(last_query(&f).at, LadderAt::Find("kelmir".into()));
    assert_eq!(p.found.as_deref(), Some("Kelmir"));
    assert_eq!(ranks(&p), [31, 32, 33, 34, 35, 36]);
    let row = element(&p, "online_lobby.ladder_row(Kelmir)");
    let framed = p.overlay(PANEL, &adv, 0).into_iter().any(|d| {
        matches!(d, DrawItem::Border { at, w, h }
            if at.y <= row.y && row.y < at.y + h && w > 200)
    });
    assert!(framed, "the found row is framed");

    for _ in 0..6 {
        p.key(vk::BACKSPACE);
    }
    for c in "Nobody".chars() {
        p.char(c);
    }
    p.enter();
    pump(&mut p);
    assert_eq!(
        p.note,
        Some(("No character named Nobody here.".into(), RED))
    );
    assert_eq!(ranks(&p), [31, 32, 33, 34, 35, 36], "the page stays");
    // A press outside the box (or the panel) ends the typing.
    p.blur();
    assert!(!p.char('x'));
}

/// Rule (contract 6): dead hardcore characters stay on the ladder, red and
/// marked.
#[test]
fn dead_hardcore_is_red_and_marked() {
    let (mut p, _f) = opened();
    click(&mut p, &Chip::Hardcore.id());
    let dead: Vec<String> = p
        .placed_rows()
        .into_iter()
        .filter(|(_, r)| r.flags & ladder_flag::DEAD != 0)
        .map(|(_, r)| r.char.clone())
        .collect();
    assert_eq!(dead, ["Sablethorn", "Brinehollow"]);
    let t = texts(&mut p);
    let row = t.iter().find(|t| t.0 == "Sablethorn +").expect("marked");
    assert_eq!(row.1, RED);
    let ironmaw = t.iter().find(|t| t.0 == "Ironmaw").expect("alive");
    assert_eq!(ironmaw.1, WHITE);
}

/// Ladder answers carry no request number on the link: the rank line's
/// question and the panel's pages are answered in the order asked, each
/// to its own (a one-row rank page never lands in the rows).
#[test]
fn the_rank_and_the_pages_answer_in_order() {
    let (mut p, _f) = scene();
    p.ask_rank(&werwer());
    p.open();
    pump(&mut p);
    assert_eq!(p.rank, Some(45));
    assert_eq!(ranks(&p), [1, 2, 3, 4, 5, 6]);
    assert!(p.asks.is_empty());
    // A page asked and then another before the first came: only the
    // latest is shown.
    p.wheel(-1);
    p.wheel(-1);
    assert_eq!(p.asks.len(), 2);
    pump(&mut p);
    assert_eq!(ranks(&p), [7, 8, 9, 10, 11, 12]);
    // The rank of a realm without the character on its ladder.
    let (mut p, _f) = scene();
    let mut other = werwer();
    other.status |= status::HARDCORE;
    p.ask_rank(&other);
    pump(&mut p);
    assert_eq!(p.rank, None);
}

/// Rule (plan L11): the wheel and Up / Down turn the pages, and stop at
/// the first and the last.
#[test]
fn the_wheel_and_keys_turn_pages() {
    let (mut p, f) = opened();
    let asked = queries(&f);
    assert_eq!(p.wheel(1), None, "the first page has none before it");
    assert!(!p.find_focus && p.key(38));
    assert_eq!(queries(&f), asked);
    assert_eq!(p.wheel(-1), Some(PanelOut::Rebuild));
    pump(&mut p);
    assert_eq!(last_query(&f).at, LadderAt::Rank(7));
    assert!(p.key(40));
    pump(&mut p);
    assert_eq!(ranks(&p), [13, 14, 15, 16, 17, 18]);
    assert!(p.key(38));
    pump(&mut p);
    assert_eq!(ranks(&p), [7, 8, 9, 10, 11, 12]);
    for _ in 0..20 {
        p.wheel(-1);
        pump(&mut p);
    }
    // 61 characters: the last page has the 61st alone.
    assert_eq!(ranks(&p), [61]);
    assert!(p.texts_have("61-61 of 61"));
    assert!(!p.key(13), "other keys stay the lobby's");
}

/// Rule (plan L11): a click on a name starts a whisper to it; not to one's
/// own characters.
#[test]
fn a_click_on_a_name_whispers() {
    let (mut p, _f) = opened();
    assert_eq!(
        click(&mut p, "online_lobby.ladder_row(Ashana)"),
        Some(PanelOut::Whisper("Ashana".into()))
    );
    assert_eq!(click(&mut p, "online_lobby.ladder_row(Werwer)"), None);
}

/// A realm without the ladder answers `NotSupported`: the panel says so.
#[test]
fn a_realm_without_a_ladder_says_so() {
    let (mut p, _f) = scene_with(|f| f.ladder_off = true);
    p.open();
    pump(&mut p);
    assert!(p.off);
    assert_eq!(p.empty_text(), Some("This realm has no ladder."));
    assert!(texts(&mut p)
        .iter()
        .any(|t| t.0 == "This realm has no ladder."));
    let (mut p, _f) = scene();
    assert_eq!(p.empty_text(), Some("Getting the ladder..."));
    p.open();
    pump(&mut p);
    assert_eq!(p.empty_text(), None);
}

/// Every cell stays in its column and every chip's label in its box at the
/// longest values: a 15-letter name, dead and the own, four-digit ranks,
/// the most experience, a four-letter tag, a long season name.
#[test]
fn every_cell_fits_its_column() {
    let (mut p, _f) = scene();
    p.seasons = Some(vec![LadderSeason {
        season: 12,
        name: "Season 12: The Long Winter of Ladders".into(),
        current: true,
        ..LadderSeason::default()
    }]);
    p.season = 12;
    let row = |rank: u32, flags: u8| LadderRow {
        rank,
        char: "Abcdefghijklmno".into(),
        class: 6,
        level: 99,
        experience: 3_837_233_737,
        tag: Some("WGWG".into()),
        flags,
    };
    p.page = Some(LadderPage {
        total: 9_999,
        first_rank: 9_990,
        rows: (0..u32::from(ROWS))
            .map(|i| row(9_990 + i, ladder_flag::DEAD))
            .collect(),
        pinned: Some(row(
            9_999,
            ladder_flag::ME | ladder_flag::PINNED | ladder_flag::MY_ACCOUNT,
        )),
        ..LadderPage::default()
    });
    let all = texts(&mut p);
    for (text, _, x, _) in &all {
        let (l, r) = (x - PANEL.x, x - PANEL.x + w8(text));
        assert!(
            l >= COL_RANK && r <= RIGHT,
            "{text:?} at {l}..{r} leaves the panel"
        );
    }
    // On each line under the chips (the header, the rows, the legend) the
    // cells don't run into each other.
    let mut lines: Vec<i32> = all.iter().map(|t| t.3).collect();
    lines.sort_unstable();
    lines.dedup();
    for y in lines.into_iter().filter(|&y| y > PANEL.y + 32) {
        let mut cells: Vec<(i32, i32, &str)> = all
            .iter()
            .filter(|t| t.3 == y && !t.0.is_empty())
            .map(|t| (t.2, t.2 + w8(&t.0), t.0.as_str()))
            .collect();
        cells.sort_unstable();
        for pair in cells.windows(2) {
            assert!(
                pair[0].1 + 2 <= pair[1].0,
                "{:?} runs into {:?} on line {y}",
                pair[0].2,
                pair[1].2
            );
        }
    }
    // The experience keeps a gutter from the guild's tag (the header's
    // too).
    for y in all.iter().map(|t| t.3) {
        let on_line = |s: &str| all.iter().find(|t| t.3 == y && t.0 == s);
        let tag = all
            .iter()
            .find(|t| t.3 == y && (t.0 == "WGWG" || t.0 == "GLD"));
        let exp = on_line("3,837,233,737").or_else(|| on_line("EXPERIENCE"));
        if let (Some(e), Some(t)) = (exp, tag) {
            assert!(
                e.2 + w8(&e.0) + 8 <= t.2,
                "{:?} crowds {:?} on line {y}",
                e.0,
                t.0
            );
        }
    }
    // Every chip's label inside its box.
    for (chip, x, w, row) in Chip::layout() {
        assert!(x >= COL_RANK && x + i32::from(w) <= RIGHT, "{chip:?}");
        let r = p.chip_rect(x, w, row);
        let label = all
            .iter()
            .find(|t| t.3 == r.y + i32::from(r.h) - 3 && r.contains(Point::new(t.2, r.y)))
            .unwrap_or_else(|| panic!("{chip:?} has a label"));
        assert!(
            label.2 + w8(&label.0) <= r.x + i32::from(r.w) - 2,
            "{chip:?}: {:?} leaves its box",
            label.0
        );
    }
    let t = texts(&mut p);
    assert!(t.iter().any(|t| t.0 == "3,837,233,737"));
    assert!(t
        .iter()
        .any(|t| t.0.starts_with("Abcde") && t.0.ends_with(" +")));
    assert!(t.iter().any(|t| t.0.ends_with(" (you)")));
}

#[test]
fn thousands_and_season_labels() {
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(1_000), "1,000");
    assert_eq!(thousands(3_151_000_000), "3,151,000,000");
    let test = LadderSeason {
        season: 3,
        test: true,
        ..LadderSeason::default()
    };
    assert_eq!(season_label(Some(&[test]), 3), "Test season");
    assert_eq!(season_label(None, 0), "Non-ladder");
    assert_eq!(season_label(None, 2), "Season 2");
}

impl LadderPanel {
    fn texts_have(&mut self, s: &str) -> bool {
        texts(self).iter().any(|t| t.0 == s)
    }
}
