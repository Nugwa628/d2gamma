use super::lines::{segments, wrap};
use super::*;
use crate::chat::{ChatModel, FakeChat, Host, Room};

/// A paint that records text: every character 6 px wide, rows 16 / 10.
#[derive(Default)]
struct Rec {
    texts: Vec<(i32, i32, u8, String)>,
}

impl ChatPaint for Rec {
    fn width(&self, _f: ChatFont, t: &str) -> i32 {
        t.chars().count() as i32 * 6
    }
    fn row_height(&self, f: ChatFont) -> i32 {
        match f {
            ChatFont::Text => 16,
            ChatFont::Small => 10,
        }
    }
    fn text(&mut self, _f: ChatFont, x: i32, y: i32, c: u8, t: &str) {
        self.texts.push((x, y, c, t.to_string()));
    }
    fn panel(&mut self, _r: Rect) {}
    fn outline(&mut self, _r: Rect) {}
}

impl Rec {
    fn has(&self, s: &str) -> bool {
        self.texts.iter().any(|t| t.3 == s)
    }
}

/// The lobby pane of the preview, in 800 x 600.
const LAYOUT: ChatLayout = ChatLayout {
    tabs: Rect::new(50, 160, 350, 18),
    body: Rect::new(68, 186, 326, 264),
    column_w: 112,
    input: Rect::new(122, 482, 270, 20),
};

fn seeded() -> (ChatModel, FakeChat) {
    let mut fake = FakeChat::seeded();
    let mut m = ChatModel::new(Host::Lobby);
    m.pump(&mut fake, 0);
    (m, fake)
}

/// Rule (L9): a long line wraps to the pane at spaces, keeps its colours,
/// and a word wider than a row is cut.
#[test]
fn wraps_long_lines() {
    let w = |s: &str| s.chars().count() as i32 * 6;
    let segs = vec![
        ("Sparks: ".to_string(), colour::GOLD),
        (
            "need a key for the nightmare chaos game, it is moo".to_string(),
            colour::WHITE,
        ),
    ];
    let rows = wrap(&segs, 120, &w);
    assert!(rows.len() >= 3, "{rows:?}");
    for r in &rows {
        let text: String = r.iter().map(|s| s.0.as_str()).collect();
        assert!(w(&text) <= 120, "{text:?}");
    }
    assert_eq!(rows[0][0], ("Sparks: ".to_string(), colour::GOLD));
    let all: Vec<String> = rows
        .iter()
        .map(|r| r.iter().map(|s| s.0.as_str()).collect::<String>())
        .collect();
    assert_eq!(
        all.join(" "),
        "Sparks: need a key for the nightmare chaos game, it is moo"
    );
    // One long word: cut at the row's width.
    let rows = wrap(&[("x".repeat(50), colour::WHITE)], 120, &w);
    assert_eq!(
        rows.iter().map(|r| r[0].0.len()).collect::<Vec<_>>(),
        [20, 20, 10]
    );
    // Through the pane: the newest rows at the bottom, all inside it.
    let (mut m, mut fake) = seeded();
    fake.say_from(
        Room::Mod,
        &crate::chat::fake::player("Gravel", 3, 61, None),
        &"long line ".repeat(30),
    );
    m.pump(&mut fake, 0);
    let mut p = Rec::default();
    draw(&m, &LAYOUT, &mut p, false, 0, None);
    let (lines, _) = LAYOUT.split(&m);
    let in_lines: Vec<_> = p
        .texts
        .iter()
        .filter(|t| t.0 >= lines.x && t.0 < lines.x + i32::from(lines.w) && t.1 > lines.y)
        .filter(|t| t.1 <= lines.y + i32::from(lines.h))
        .collect();
    assert!(in_lines.iter().any(|t| t.3.starts_with("long line")));
    for t in &in_lines {
        assert!(t.0 + p.width(ChatFont::Text, &t.3) <= lines.x + i32::from(lines.w) + 1);
    }
}

/// The preview's left half: tabs with their unread badges, the Mod lines in
/// their colours, "In chat (38)" with "+N more", the input hint.
#[test]
fn draws_the_preview_pane() {
    let (m, _fake) = seeded();
    let mut p = Rec::default();
    draw(&m, &LAYOUT, &mut p, false, 0, None);
    for s in [
        "GLOBAL",
        "MOD: D2GAMMA",
        "GUILD: NUG",
        "DM",
        "3",
        "1",
        "In chat (38)",
    ] {
        assert!(p.has(s), "{s}: {:?}", p.texts);
    }
    assert!(p
        .texts
        .iter()
        .any(|t| t.3 == "Ironwolf: " && t.2 == colour::GOLD));
    assert!(p
        .texts
        .iter()
        .any(|t| t.3 == "[Market] " && t.2 == colour::BLUE));
    assert!(p.has("Necrolord joined D2Gamma chat."));
    assert!(p
        .texts
        .iter()
        .any(|t| t.3.starts_with('+') && t.3.ends_with(" more")));
    assert!(p.has("/w name   /g guild   /global"));
    // The open tab is gold, the others tan.
    assert!(p
        .texts
        .iter()
        .any(|t| t.3 == "MOD: D2GAMMA" && t.2 == colour::GOLD));
    assert!(p
        .texts
        .iter()
        .any(|t| t.3 == "GLOBAL" && t.2 == colour::TAN));
}

/// Clicks: a tab opens, a player gets a whisper line, a conversation opens.
#[test]
fn clicks_route_to_the_model() {
    let (mut m, mut fake) = seeded();
    let p = Rec::default();
    let guild = tabs::boxes(&m, LAYOUT.tabs, &p)
        .into_iter()
        .find(|b| b.tab == Tab::Room(Room::Guild))
        .unwrap();
    let h = hit(&m, &LAYOUT, &p, Point::new(guild.at.x + 2, guild.at.y + 2)).unwrap();
    assert_eq!(h, Hit::Tab(Tab::Room(Room::Guild)));
    click(&mut m, &h);
    m.pump(&mut fake, 0);
    assert_eq!(m.open_tab_now(), Tab::Room(Room::Guild));
    let (_, col) = LAYOUT.split(&m);
    let col = col.unwrap();
    // Row 1 is the first player (Gravel, Kaern, Werwer sorted).
    let h = hit(&m, &LAYOUT, &p, Point::new(col.x + 2, col.y + 12 + 2)).unwrap();
    assert_eq!(h, Hit::User("Gravel".into()));
    click(&mut m, &h);
    assert_eq!(m.input, "/w Gravel ");
    m.open_tab(Tab::Dm);
    m.pump(&mut fake, 0);
    let h = hit(&m, &LAYOUT, &p, Point::new(col.x + 2, col.y + 12 + 2)).unwrap();
    assert_eq!(h, Hit::Conversation(Some("Kaern".into())));
    click(&mut m, &h);
    assert_eq!(m.open_dm.as_deref(), Some("Kaern"));
    // The DM tab's lines, as the preview's DM tab.
    let mut d = Rec::default();
    draw(&m, &LAYOUT, &mut d, false, 0, None);
    assert!(d.has("Kaern -> You: "));
    assert!(d.has("You -> Kaern: "));
    let segs = segments(&m, Tab::Dm, m.lines(Tab::Dm)[0]);
    assert_eq!(segs[0].1, colour::PURPLE);
}

/// Polish: the strip always fits its width: too narrow for the full
/// labels, it takes the short ones ("MOD", "GUILD") with the full label as
/// the tab's tip; every box inside the strip.
#[test]
fn the_tab_strip_fits_its_width() {
    let (m, _fake) = seeded();
    let p = Rec::default();
    let full = tabs::boxes(&m, LAYOUT.tabs, &p);
    assert!(full.iter().all(|b| b.tip.is_none()), "{full:?}");
    for w in [260u16, 230, 200] {
        let strip = Rect::new(LAYOUT.tabs.x, LAYOUT.tabs.y, w, LAYOUT.tabs.h);
        let b = tabs::boxes(&m, strip, &p);
        let right = strip.x + i32::from(strip.w);
        assert!(
            b.iter().all(|b| b.at.x + i32::from(b.at.w) <= right),
            "{w}: {b:?}"
        );
        let label = |t: Tab| b.iter().find(|b| b.tab == t).unwrap().label.clone();
        assert_eq!(label(Tab::Room(Room::Mod)), "MOD", "{w}");
        assert_eq!(label(Tab::Room(Room::Guild)), "GUILD", "{w}");
        assert_eq!(label(Tab::Dm), "DM", "{w}");
        // The short tab's tip is its full label; a whole one has none.
        let mod_box = b.iter().find(|b| b.tab == Tab::Room(Room::Mod)).unwrap();
        let at = Point::new(mod_box.at.x + 2, mod_box.at.y + 2);
        assert_eq!(tabs::tip_at(&b, at).map(|t| t.1), Some("MOD: D2GAMMA"));
        let dm = b.iter().find(|b| b.tab == Tab::Dm).unwrap();
        assert_eq!(tabs::tip_at(&b, Point::new(dm.at.x + 2, dm.at.y + 2)), None);
    }
}

/// Polish: a long name in the column is cut with "..." before its class
/// and level, which every row shows whole, right-aligned at the column's
/// edge.
#[test]
fn long_names_never_reach_the_class_column() {
    let mut fake = FakeChat::seeded();
    fake.add_user(0, crate::chat::fake::player("Archbishopthorn", 2, 99, None));
    fake.add_user(1, crate::chat::fake::player("Andarielslayer", 1, 88, None));
    let mut m = ChatModel::new(Host::Lobby);
    m.pump(&mut fake, 0);
    let mut p = Rec::default();
    draw(&m, &LAYOUT, &mut p, false, 0, None);
    let (_, col) = LAYOUT.split(&m);
    let col = col.unwrap();
    let right = col.x + i32::from(col.w);
    let rows: Vec<&(i32, i32, u8, String)> = p
        .texts
        .iter()
        .filter(|t| col.contains(Point::new(t.0, t.1 - 1)))
        .collect();
    for name in ["Archbis", "Andarie"] {
        let n = rows
            .iter()
            .find(|t| t.3.starts_with(name))
            .unwrap_or_else(|| panic!("{name}: {rows:?}"));
        assert!(n.3.ends_with("..."), "{n:?}");
        let tail = rows
            .iter()
            .find(|t| t.1 == n.1 && t.0 > n.0)
            .unwrap_or_else(|| panic!("{name} has its class and level: {rows:?}"));
        assert!(
            n.0 + p.width(ChatFont::Small, &n.3) < tail.0,
            "{n:?} {tail:?}"
        );
        assert_eq!(
            tail.0 + p.width(ChatFont::Small, &tail.3),
            right,
            "{tail:?}"
        );
    }
}
