use super::*;
use crate::ui::front_end::screens::online::fake::social::session;
use crate::ui::front_end::screens::online::fake::{SharedFake, FAKE_NOW_MS};
use crate::ui::front_end::screens::online::link::{lobby_code, FriendGame};
use crate::ui::front_end::screens::online::lobby::PANEL;

/// Every glyph 8 px wide (Font16), 5 px in the small font, as the lobby's
/// tests measure.
fn adv(font: u16, t: &[u16]) -> i32 {
    let per = if font == FONT8 { 5 } else { 8 };
    t.len() as i32 * per
}

/// The realm's answers fed and the panel rebuilt (which asks again for a
/// stale list) until nothing more comes.
fn settle(online: &OnlineHandle, p: &mut FriendsPanel) -> Vec<Control> {
    let mut v = Vec::new();
    for _ in 0..4 {
        // Taken first, as the lobby does: the panel reads the link too.
        let events = online.borrow_mut().take_events();
        for ev in &events {
            p.feed(ev);
        }
        v.clear();
        p.controls(PANEL, &mut v);
    }
    v
}

/// The panel opened on the seeded realm.
fn opened(
    set: impl FnOnce(&mut crate::ui::front_end::screens::online::fake::FakeServer),
) -> (SharedFake, OnlineHandle, FriendsPanel) {
    let (fake, online) = session(set);
    let mut p = FriendsPanel::new(online.clone());
    p.open();
    settle(&online, &mut p);
    (fake, online, p)
}

fn sent(fake: &SharedFake) -> Vec<Request> {
    std::mem::take(&mut fake.0.borrow_mut().seen)
}

fn names(rows: &[FriendRow]) -> Vec<&str> {
    rows.iter().map(|r| shown(&r.who)).collect()
}

/// A click on a UI element (its centre).
fn click(p: &mut FriendsPanel, id: &str) -> Option<PanelOut> {
    let at = p
        .ui_elements()
        .into_iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no {id}"))
        .at;
    p.mouse(at, 10_000)
}

fn type_in(p: &mut FriendsPanel, text: &str) {
    p.action(act::FIELD);
    for c in text.chars() {
        assert!(p.char(c));
    }
}

fn texts(items: &[DrawItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|i| match i {
            DrawItem::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// The list: online friends first, the mutual mark, the guild tag, and
/// where each is as far as the friend lets the player see.
#[test]
fn the_list_says_who_is_where() {
    let (_fake, _online, mut p) = opened(|_| {});
    assert_eq!(
        names(&p.friends),
        ["Kaern", "Mirelle", "Rhaego", "Vexa", "Tobben"]
    );
    let place = |n: &str| {
        let r = p.friends.iter().find(|r| shown(&r.who) == n).unwrap();
        place_text(r, FAKE_NOW_MS).0
    };
    assert_eq!(place("Kaern"), "in the lobby");
    assert_eq!(place("Rhaego"), "in baal run");
    // One-sided: online only. Mutual but hiding: hidden.
    assert_eq!(place("Mirelle"), "online");
    assert_eq!(place("Vexa"), "hidden");
    assert_eq!(place("Tobben"), "last seen 2 h ago");
    let mutual: Vec<bool> = p.friends.iter().map(|r| r.mutual).collect();
    assert_eq!(mutual, [true, false, true, true, true]);
    let shown_items = texts(&p.overlay(PANEL, &adv, 10_000));
    for t in [
        "Kaern",
        "*",
        " <NUG>",
        "Bar 82",
        "in the lobby",
        "last seen 2 h ago",
    ] {
        assert!(shown_items.iter().any(|s| s == t), "{t} in {shown_items:?}");
    }
    // The legend under the panel names the mark.
    assert!(shown_items
        .iter()
        .any(|s| s.starts_with("* a mutual friend")));
}

/// ADD sends the typed name; the realm's answer is the note and the list
/// follows. Enter in the box adds too.
#[test]
fn add_puts_a_friend_on_the_list() {
    let (fake, online, mut p) = opened(|_| {});
    sent(&fake);
    type_in(&mut p, "Grim");
    assert_eq!(p.action(act::ADD), Some(PanelOut::Rebuild));
    assert_eq!(
        sent(&fake),
        [Request::FriendEdit(FriendOp::Add("Grim".into()))]
    );
    assert!(p.name.text.is_empty());
    settle(&online, &mut p);
    assert_eq!(
        p.note,
        Some(("Grim is on your friends list.".into(), GREEN))
    );
    assert!(names(&p.friends).contains(&"Grim"));
    // Enter in the box.
    type_in(&mut p, "Kestrel");
    assert_eq!(p.enter(), Some(PanelOut::Rebuild));
    settle(&online, &mut p);
    assert!(names(&p.friends).contains(&"Kestrel"));
    // An empty box asks for a name and sends nothing.
    sent(&fake);
    p.action(act::ADD);
    assert!(sent(&fake).is_empty());
    // A press outside the panel leaves the box: typing and Enter go to
    // the chat.
    p.action(act::FIELD);
    p.blur();
    assert!(!p.char('x'));
    assert_eq!(p.enter(), None);
}

/// A refusal says why in red.
#[test]
fn a_refused_add_says_why() {
    let (_fake, online, mut p) = opened(|_| {});
    type_in(&mut p, "Nobody");
    p.action(act::ADD);
    settle(&online, &mut p);
    let want = failure_text(&Failure {
        code: lobby_code::UNKNOWN_PLAYER,
        wait_s: 0,
        text: String::new(),
    });
    assert_eq!(p.note, Some((want, RED)));
}

/// REMOVE takes the selected friend off by account.
#[test]
fn remove_takes_the_selected_friend_off() {
    let (fake, online, mut p) = opened(|_| {});
    assert_eq!(p.action(act::REMOVE), Some(PanelOut::Rebuild));
    assert!(sent(&fake)
        .iter()
        .all(|r| !matches!(r, Request::FriendEdit(_))));
    click(&mut p, "online_lobby.friend(Mirelle)");
    assert_eq!(p.selected.as_deref(), Some("*mirelle"));
    p.action(act::REMOVE);
    assert_eq!(
        sent(&fake),
        [Request::FriendEdit(FriendOp::Remove("*mirelle".into()))]
    );
    settle(&online, &mut p);
    assert!(!names(&p.friends).contains(&"Mirelle"));
    assert_eq!(p.selected, None);
}

/// WHISPER (and a double click) start a whisper to the selected friend.
#[test]
fn whisper_starts_a_whisper() {
    let (_fake, _online, mut p) = opened(|_| {});
    assert_eq!(p.action(act::WHISPER), Some(PanelOut::Rebuild));
    click(&mut p, "online_lobby.friend(Kaern)");
    assert_eq!(
        p.action(act::WHISPER),
        Some(PanelOut::Whisper("Kaern".into()))
    );
    assert_eq!(
        click(&mut p, "online_lobby.friend(Kaern)"),
        Some(PanelOut::Whisper("Kaern".into()))
    );
}

/// JOIN joins a mutual friend's game the character may join; Enter does
/// too when the chat line is empty. Otherwise the line says why not.
#[test]
fn join_goes_to_a_mutual_friends_game() {
    let (_fake, _online, mut p) = opened(|_| {});
    click(&mut p, "online_lobby.friend(Rhaego)");
    assert_eq!(p.action(act::JOIN), Some(PanelOut::Join(101)));
    assert_eq!(p.primary(false), Some(PanelOut::Join(101)));
    // Typed chat goes to the chat.
    assert_eq!(p.primary(true), None);
    click(&mut p, "online_lobby.friend(Mirelle)");
    assert_eq!(p.action(act::JOIN), Some(PanelOut::Rebuild));
    assert!(p
        .note
        .as_ref()
        .unwrap()
        .0
        .contains("ask Mirelle to add you"));
    click(&mut p, "online_lobby.friend(Kaern)");
    p.action(act::JOIN);
    assert_eq!(
        p.note.as_ref().unwrap().0,
        "Kaern is not in a game you can see."
    );
    // A full game: the reason, no join.
    let mut rows = p.friends.clone();
    rows[2].game = Some(FriendGame {
        game_id: 101,
        game_name: "baal run".into(),
        joinable: ErrorCode::GAME_FULL.0,
    });
    p.feed(&Event::Friends {
        ignored: false,
        rows,
    });
    click(&mut p, "online_lobby.friend(Rhaego)");
    assert_eq!(p.action(act::JOIN), Some(PanelOut::Rebuild));
    assert_eq!(p.note.as_ref().map(|n| n.1), Some(RED));
}

/// The ignore list: IGNORE adds the typed name, UNIGNORE the selected one.
#[test]
fn the_ignore_list() {
    let (fake, online, mut p) = opened(|_| {});
    p.action(act::TAB + 1);
    assert_eq!(p.tab, Sub::Ignored);
    assert_eq!(names(&p.ignored), ["Goldseller"]);
    sent(&fake);
    type_in(&mut p, "Kestrel");
    p.action(act::ADD);
    assert_eq!(
        sent(&fake),
        [Request::FriendEdit(FriendOp::Ignore("Kestrel".into()))]
    );
    settle(&online, &mut p);
    assert_eq!(names(&p.ignored), ["Goldseller", "Kestrel"]);
    assert_eq!(p.note, Some(("You ignore Kestrel.".into(), WHITE)));
    sent(&fake);
    click(&mut p, "online_lobby.ignored(Goldseller)");
    p.action(act::REMOVE);
    assert_eq!(
        sent(&fake),
        [Request::FriendEdit(FriendOp::Unignore("Goldseller".into()))]
    );
    settle(&online, &mut p);
    assert_eq!(names(&p.ignored), ["Kestrel"]);
    let ids: Vec<String> = p.ui_elements().into_iter().map(|e| e.id).collect();
    assert!(ids.contains(&"online_lobby.friends.ignore".to_string()));
    assert!(ids.contains(&"online_lobby.friends.unignore".to_string()));
    assert!(!ids.contains(&"online_lobby.friends.join".to_string()));
}

/// The two privacy choices go to the realm and show once it took them.
#[test]
fn the_privacy_choices() {
    let (fake, online, mut p) = opened(|_| {});
    p.action(act::TAB + 2);
    sent(&fake);
    click(&mut p, "online_lobby.privacy.whispers(2)");
    p.action(act::WHISPERS + 2);
    let want = Privacy {
        whispers: 2,
        show_where: 0,
    };
    assert_eq!(sent(&fake), [Request::FriendEdit(FriendOp::Privacy(want))]);
    // Not shown as chosen until the realm says done.
    assert_eq!(p.privacy, Privacy::default());
    settle(&online, &mut p);
    assert_eq!(p.privacy, want);
    p.action(act::SHOW_WHERE + 1);
    settle(&online, &mut p);
    let want = Privacy {
        whispers: 2,
        show_where: 1,
    };
    assert_eq!(p.privacy, want);
    assert_eq!(fake.0.borrow().social.privacy, want);
    // The chosen radios are checked.
    let v = settle(&online, &mut p);
    let checked = v
        .iter()
        .filter(|c| c.art == Some(RADIO) && c.checked)
        .count();
    assert_eq!(checked, 2);
}

/// A friend's notice moves their row at once.
#[test]
fn a_notice_moves_the_row() {
    let (fake, online, mut p) = opened(|_| {});
    fake.0
        .borrow_mut()
        .player_moves("Tobben", Where::Lobby, None);
    settle(&online, &mut p);
    let r = p
        .friends
        .iter()
        .find(|r| shown(&r.who) == "Tobben")
        .unwrap();
    assert_eq!(r.place, Where::Lobby);
    assert_eq!(p.note, Some(("Tobben came online.".into(), WHITE)));
    // Online friends first.
    assert_eq!(
        names(&p.friends),
        ["Kaern", "Mirelle", "Rhaego", "Tobben", "Vexa"]
    );
}

/// The chat's own `/f add` leaves the panel's line alone, but the list
/// follows.
#[test]
fn the_chats_change_is_not_the_panels() {
    let (_fake, online, mut p) = opened(|_| {});
    online
        .borrow_mut()
        .send(Request::FriendEdit(FriendOp::Add("Brisa".into())));
    settle(&online, &mut p);
    assert_eq!(p.note, None);
    assert!(names(&p.friends).contains(&"Brisa"));
}

/// A realm without friends says so and shows no buttons.
#[test]
fn a_realm_without_friends() {
    let (_fake, _online, mut p) = opened(|f| f.social_off = true);
    assert!(p.unsupported);
    let ids: Vec<String> = p.ui_elements().into_iter().map(|e| e.id).collect();
    assert!(!ids.contains(&"online_lobby.friends.add".to_string()));
    let t = texts(&p.overlay(PANEL, &adv, 0));
    assert!(t.contains(&"This realm has no friends lists yet.".to_string()));
}

/// The fourth tab is the guild's; its buttons are the panel's.
#[test]
fn the_guild_tab() {
    let (fake, online, mut p) = opened(|_| {});
    sent(&fake);
    p.action(act::TAB + 3);
    assert_eq!(
        sent(&fake),
        [
            Request::GuildGet(super::super::link::GuildQuery::Mine),
            Request::GuildGet(super::super::link::GuildQuery::Invites),
        ]
    );
    settle(&online, &mut p);
    let ids: Vec<String> = p.ui_elements().into_iter().map(|e| e.id).collect();
    for id in [
        "online_lobby.friends_tab(guild)",
        "online_lobby.guild.disband",
        "online_lobby.guild_member(Kaern)",
    ] {
        assert!(ids.contains(&id.to_string()), "{id} in {ids:?}");
    }
    // The guild's actions reach it through the panel.
    click(&mut p, "online_lobby.guild_member(Kaern)");
    assert_eq!(
        p.action(super::super::guild_panel::act::WHISPER),
        Some(PanelOut::Whisper("Kaern".into()))
    );
}

/// Everything the panel draws stays inside it (the two lines under it
/// excepted): rows, chips and the box don't overlap.
#[test]
fn the_panel_fits() {
    for tab in 0..4 {
        let (_fake, online, mut p) = opened(|_| {});
        p.action(act::TAB + tab);
        let v = settle(&online, &mut p);
        let bottom = PANEL.y + i32::from(PANEL.h);
        for c in &v {
            let (top, low) = (c.y - i32::from(c.h), c.y);
            assert!(
                c.x >= PANEL.x
                    && c.x + i32::from(c.w) <= PANEL.x + i32::from(PANEL.w)
                    && top >= PANEL.y
                    && low <= bottom,
                "tab {tab}: {c:?}"
            );
        }
        let chips: Vec<Rect> = p
            .chips
            .iter()
            .map(|c| c.at)
            .chain(p.guild.chips.iter().map(|c| c.at))
            .collect();
        for (i, a) in chips.iter().enumerate() {
            for b in &chips[i + 1..] {
                let apart = a.right() <= i64::from(b.x)
                    || b.right() <= i64::from(a.x)
                    || a.bottom() <= i64::from(b.y)
                    || b.bottom() <= i64::from(a.y);
                assert!(apart, "tab {tab}: {a:?} {b:?}");
            }
        }
        for item in p.overlay(PANEL, &adv, 0) {
            if let DrawItem::Text { at, text, font, .. } = item {
                let w = adv(font, &text.encode_utf16().collect::<Vec<u16>>());
                assert!(
                    at.x >= PANEL.x && at.x + w <= PANEL.x + i32::from(PANEL.w),
                    "tab {tab}: {text:?} at {at:?}"
                );
                assert!(at.y <= bottom + UNDER[1], "tab {tab}: {text:?} at {at:?}");
            }
        }
    }
}

/// "Last seen" in minutes, hours and days.
#[test]
fn ago_reads_like_a_person() {
    let now = FAKE_NOW_MS;
    assert_eq!(ago(now, now - 20_000), "just now");
    assert_eq!(ago(now, now - 5 * 60_000), "5 min ago");
    assert_eq!(ago(now, now - 3 * 3_600_000), "3 h ago");
    assert_eq!(ago(now, now - 72 * 3_600_000), "3 days ago");
}

/// The class and level end before the place column at the game's small
/// font width (about 7 px a glyph), so "Asn 74" never runs into "hidden".
#[test]
fn the_class_column_ends_before_the_place() {
    let (_fake, _online, mut p) = opened(|_| {});
    let classes: Vec<String> = p
        .friends
        .iter()
        .map(|r| format!("{} {}", class_short(r.who.class), r.who.level))
        .collect();
    let wide = |font: u16, t: &[u16]| t.len() as i32 * if font == FONT8 { 7 } else { 9 };
    let mut seen = 0;
    for item in p.overlay(PANEL, &wide, 10_000) {
        if let DrawItem::Text { at, text, font, .. } = item {
            if classes.contains(&text) {
                let end = at.x + wide(font, &text.encode_utf16().collect::<Vec<u16>>());
                assert!(end <= PANEL.x + COL_PLACE - 4, "{text:?} ends at {end}");
                seen += 1;
            }
        }
    }
    assert_eq!(seen, classes.len());
}

/// Polish: a chip's label keeps its padding from the box's sides, a row's
/// labels in one font: a row Font16 would fill edge to edge takes Font8,
/// and one too wide for that (its capitals near Font16's) the smallest
/// capitals.
#[test]
fn chip_labels_keep_clear_of_their_boxes() {
    // As the ladder's tests measure: Font8 capitals 8 px, FontRidiculous 6.
    let adv = |font: u16, t: &[u16]| {
        t.len() as i32
            * match font {
                FONT_TINY => 6,
                _ => 8,
            }
    };
    let items: Vec<(&'static str, u32, String)> =
        ["WHISPER", "KICK", "PROMOTE", "DEMOTE", "LEADER"]
            .iter()
            .map(|l| (*l, 0, l.to_lowercase()))
            .collect();
    // Chips 54 px wide: DEMOTE and LEADER (48 px) would fit but for 3 px a
    // side, WHISPER and PROMOTE (56) not at all.
    let chips = chip_row(Rect::new(0, 0, 282, 100), 0, &items);
    assert_eq!(chips[0].at.w, 54);
    let out = chip_items(&chips, &adv);
    let mut fonts = Vec::new();
    for c in &chips {
        let (x, font) = out
            .iter()
            .find_map(|i| match i {
                DrawItem::Text { text, at, font, .. } if text == c.label => Some((at.x, *font)),
                _ => None,
            })
            .unwrap();
        fonts.push(font);
        let w = adv(font, &c.label.encode_utf16().collect::<Vec<u16>>());
        let r = c.at;
        assert!(
            x >= r.x + CHIP_PAD && x + w <= r.x + i32::from(r.w) - CHIP_PAD,
            "{} at {x}..{} in {r:?}",
            c.label,
            x + w
        );
    }
    assert!(
        fonts.iter().all(|f| *f == fonts[0]),
        "one font a row: {fonts:?}"
    );
}
