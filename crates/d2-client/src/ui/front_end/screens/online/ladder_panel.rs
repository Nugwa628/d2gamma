//! The lobby's Ladder panel (plan `lobby-ui.md` L11, lobby contract
//! section 6), in the lobby's right panel ([`super::lobby::PANEL`]) through
//! [`RightPanel`], as the preview `preview-lobby-ladder.png` has it:
//!
//! - **Chips** (two rows, in the chat tabs' boxes): the season (a click
//!   steps through the realm's seasons, newest first, then non-ladder),
//!   HC / SC, EXP / CLASSIC; ALL and the seven classes. A chip asks the
//!   realm for that view's first page.
//! - **Rows**: # / CHARACTER / CLS / LVL / EXPERIENCE / GLD, [`ROWS`] a
//!   page. A dead hardcore character is red with a `+` after the name; the
//!   selected character is yellow with "(you)", and when it is ranked off
//!   the page the realm's pinned row stands under it after a `...` line.
//!   The wheel and Up / Down turn the pages; a click on a name starts a
//!   whisper to it in the chat's input line.
//! - **Find** (the box at the bottom, Enter): the page with that character
//!   (whole name, any case), its row framed; **MY RANK**: the page with the
//!   selected character.
//!
//! The panel also asks for the character's own rank (its season and kind,
//! all classes) that the lobby's character line shows
//! (`Ladder rank #37`, [`LadderPanel::rank`]). Ladder answers carry no
//! request number on the link, so the panel keeps its questions in order
//! ([`LadderPanel::asks`]) and gives each answer to the one it belongs to.

use std::collections::VecDeque;

use d2_formats::d2s::status;

use crate::ui::front_end::control::vk;
use crate::ui::front_end::{Control, DrawItem};
use crate::ui::geom::{Point, Rect};
use crate::ui::ui_ids::UiElement;

use super::link::{
    failure_text, ladder_flag, ladder_split, lobby_code, Asked, CharInfo, ErrorCode, Event,
    Failure, LadderAt, LadderPage, LadderQuery, LadderRow, LadderSeason, Request,
};
use super::lobby::{act, rank_query, PanelOut, RightPanel};
use super::widgets::*;
use super::OnlineHandle;

/// Rows of a page.
pub const ROWS: u8 = 6;
/// The panel's own actions: the find box's click area, MY RANK.
pub const FIND: u32 = act::PANEL;
pub const MY_RANK: u32 = act::PANEL + 1;
/// Most characters of a character name.
const NAME_MAX: usize = 15;

/// Text colours the widgets don't name: the chips' unselected tan and the
/// own row's yellow.
const TAN: i32 = 7;
const YELLOW: i32 = 9;

/// The layout, from the panel's top left.
const CHIP_H: u16 = 13;
const CHIP_ROWS: [i32; 2] = [1, 16];
const HEAD_BASE: i32 = 42;
const ROW_BASE: i32 = 54;
const ROW_STEP: i32 = 11;
/// The `...` line and the pinned row under the page.
const DOTS_BASE: i32 = ROW_BASE + ROW_STEP * ROWS as i32;
const PINNED_BASE: i32 = DOTS_BASE + ROW_STEP;
/// The legend (or a note) and the page's ranks.
const LEGEND_BASE: i32 = PINNED_BASE + 12;
/// The find box's cel bottom, MY RANK's bottom and x.
const FIND_BOTTOM: i32 = 182;
const MY_RANK_X: i32 = 182;
const MY_RANK_W: u16 = 124;
/// The columns: rank, name, class, level, experience (its right edge), the
/// guild's tag; the rows' right edge. The experience keeps a gutter from
/// the tag (ten digits and their commas fill its column).
const COL_RANK: i32 = 4;
const COL_NAME: i32 = 36;
const COL_CLASS: i32 = 126;
const COL_LEVEL: i32 = 152;
const COL_EXP_RIGHT: i32 = 258;
const COL_GUILD: i32 = 268;
const RIGHT: i32 = 306;

/// The legend under the rows.
pub const LEGEND: &str = "+ dead HC - click a name to whisper";
/// The find box's text while it is empty.
pub const FIND_HINT: &str = "find a character";

/// The classes' short names (class ids 0-6).
pub const CLASSES: [&str; 7] = ["AMA", "SOR", "NEC", "PAL", "BAR", "DRU", "ASN"];

/// A chip of the two rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chip {
    Season,
    Hardcore,
    Softcore,
    Expansion,
    Classic,
    /// A class (0-6), or all (255).
    Class(u8),
}

impl Chip {
    /// Every chip with its box (x, width) and row.
    fn layout() -> Vec<(Chip, i32, u16, usize)> {
        let mut v = vec![
            (Chip::Season, 4, 88, 0),
            (Chip::Hardcore, 98, 26, 0),
            (Chip::Softcore, 126, 26, 0),
            (Chip::Expansion, 158, 34, 0),
            (Chip::Classic, 194, 66, 0),
            (Chip::Class(255), 4, 30, 1),
        ];
        for c in 0..7u8 {
            v.push((Chip::Class(c), 38 + 38 * i32::from(c), 36, 1));
        }
        v
    }

    /// Its UI element name (`online_lobby.ladder_chip(hc)`).
    pub fn id(self) -> String {
        let name = match self {
            Chip::Season => "season".to_string(),
            Chip::Hardcore => "hc".into(),
            Chip::Softcore => "sc".into(),
            Chip::Expansion => "exp".into(),
            Chip::Classic => "classic".into(),
            Chip::Class(255) => "all".into(),
            Chip::Class(c) => CLASSES[usize::from(c.min(6))].to_ascii_lowercase(),
        };
        format!("online_lobby.ladder_chip({name})")
    }
}

/// What a ladder answer the panel waits for is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// The character's own rank (the lobby's character line).
    Rank,
    /// A page for the rows, by number (only the latest is shown).
    Page(u32),
}

pub struct LadderPanel {
    pub online: OnlineHandle,
    /// The realm's seasons once it listed them.
    pub seasons: Option<Vec<LadderSeason>>,
    /// The view: season, hardcore, expansion, class (255 all).
    pub season: u16,
    pub hardcore: bool,
    pub expansion: bool,
    pub class: u8,
    /// The page shown.
    pub page: Option<LadderPage>,
    /// The character's rank in its own view (all classes).
    pub rank: Option<u32>,
    /// The realm has no ladder (`NotSupported`).
    pub off: bool,
    /// A line in the legend's place (a find without a match, a refusal).
    pub note: Option<(String, i32)>,
    pub find: Field,
    pub find_focus: bool,
    /// The character the last find went to: its row is framed.
    pub found: Option<String>,
    /// The ladder answers to come, in the order they were asked.
    pub asks: VecDeque<Ask>,
    /// The latest page asked, and where it starts.
    seq: u32,
    at: LadderAt,
    /// The character the view was set for.
    for_char: Option<String>,
    area: Rect,
    /// The find box's text control in the built list.
    find_ix: Option<usize>,
}

impl LadderPanel {
    pub fn new(online: OnlineHandle) -> Self {
        LadderPanel {
            online,
            seasons: None,
            season: 0,
            hardcore: false,
            expansion: true,
            class: 255,
            page: None,
            rank: None,
            off: false,
            note: None,
            find: Field::new(NAME_MAX),
            find_focus: false,
            found: None,
            asks: VecDeque::new(),
            seq: 0,
            at: LadderAt::Rank(1),
            for_char: None,
            area: super::lobby::PANEL,
            find_ix: None,
        }
    }

    fn character(&self) -> Option<CharInfo> {
        self.online.borrow().state.selected.clone()
    }

    /// Asks for character `c`'s rank in its own view (the lobby's
    /// character line); [`Self::rank`] holds it once the answer is in.
    pub fn ask_rank(&mut self, c: &CharInfo) {
        self.rank = None;
        self.asks.push_back(Ask::Rank);
        self.online
            .borrow_mut()
            .send(Request::LadderQuery(rank_query(c)));
    }

    /// The view's split bits.
    pub fn split(&self) -> u8 {
        let mut s = 0;
        if self.hardcore {
            s |= ladder_split::HARDCORE;
        }
        if self.expansion {
            s |= ladder_split::EXPANSION;
        }
        s
    }

    /// The query for the view's page at `at`.
    pub fn query_at(&self, at: LadderAt) -> LadderQuery {
        LadderQuery {
            season: self.season,
            split: self.split(),
            class: self.class,
            at,
            rows: ROWS,
        }
    }

    /// Asks for the view's page at `at`.
    fn ask(&mut self, at: LadderAt) {
        self.seq += 1;
        self.asks.push_back(Ask::Page(self.seq));
        self.at = at.clone();
        self.note = None;
        let q = self.query_at(at);
        self.online.borrow_mut().send(Request::LadderQuery(q));
    }

    /// The view of character `c`: its season and kind, all classes.
    fn view_of(&mut self, c: Option<&CharInfo>) {
        let st = c.map_or(status::EXPANSION, |c| c.status);
        self.season = c.map_or(0, |c| c.season);
        self.hardcore = st & status::HARDCORE != 0;
        self.expansion = st & status::EXPANSION != 0;
        self.class = 255;
        self.page = None;
        self.found = None;
        self.note = None;
        self.find.text.clear();
        self.find_focus = false;
    }

    /// The page `d` pages on from the one shown; `false` at an end.
    fn turn(&mut self, d: i32) -> bool {
        let Some(p) = &self.page else {
            return false;
        };
        let rows = i64::from(ROWS);
        let last = (i64::from(p.total) - 1).max(0) / rows * rows + 1;
        let to = (i64::from(p.first_rank) + i64::from(d) * rows).clamp(1, last);
        if to == i64::from(p.first_rank) {
            return false;
        }
        self.found = None;
        self.ask(LadderAt::Rank(to as u32));
        true
    }

    /// A chip pressed: the view changes and its first page is asked.
    pub fn press(&mut self, chip: Chip) {
        match chip {
            Chip::Season => {
                let Some(list) = self.seasons.as_ref().filter(|l| !l.is_empty()) else {
                    return;
                };
                let i = list.iter().position(|s| s.season == self.season);
                self.season = list[i.map_or(0, |i| (i + 1) % list.len())].season;
            }
            Chip::Hardcore => self.hardcore = true,
            Chip::Softcore => self.hardcore = false,
            Chip::Expansion => self.expansion = true,
            Chip::Classic => self.expansion = false,
            Chip::Class(c) => self.class = c,
        }
        self.found = None;
        self.ask(LadderAt::Rank(1));
    }

    /// Whether `chip` shows as chosen.
    pub fn chosen(&self, chip: Chip) -> bool {
        match chip {
            Chip::Season => true,
            Chip::Hardcore => self.hardcore,
            Chip::Softcore => !self.hardcore,
            Chip::Expansion => self.expansion,
            Chip::Classic => !self.expansion,
            Chip::Class(c) => self.class == c,
        }
    }

    fn chip_rect(&self, x: i32, w: u16, row: usize) -> Rect {
        Rect::new(self.area.x + x, self.area.y + CHIP_ROWS[row], w, CHIP_H)
    }

    fn find_rect(&self) -> Rect {
        Rect::new(
            self.area.x + 4,
            self.area.y + FIND_BOTTOM - i32::from(FIELD_H),
            FIELD_W,
            FIELD_H,
        )
    }

    /// The shown rows with their baselines: the page, then the pinned row.
    pub fn placed_rows(&self) -> Vec<(i32, &LadderRow)> {
        let Some(p) = &self.page else {
            return Vec::new();
        };
        let top = self.area.y;
        let mut v: Vec<(i32, &LadderRow)> = p
            .rows
            .iter()
            .take(usize::from(ROWS))
            .enumerate()
            .map(|(i, r)| (top + ROW_BASE + ROW_STEP * i as i32, r))
            .collect();
        if let Some(r) = &p.pinned {
            v.push((top + PINNED_BASE, r));
        }
        v
    }

    /// A row's click area from its baseline.
    fn row_rect(&self, base: i32) -> Rect {
        Rect::new(
            self.area.x,
            base - ROW_STEP + 1,
            (RIGHT + 2) as u16,
            ROW_STEP as u16,
        )
    }

    /// The text under the rows: a note, or the legend.
    fn legend(&self) -> (String, i32) {
        self.note
            .clone()
            .unwrap_or_else(|| (LEGEND.to_string(), GREY))
    }

    /// What the rows' place says when there are no rows.
    pub fn empty_text(&self) -> Option<&'static str> {
        if self.off {
            return Some("This realm has no ladder.");
        }
        match &self.page {
            None => Some("Getting the ladder..."),
            Some(p) if p.rows.is_empty() => Some("No characters on this ladder yet."),
            Some(_) => None,
        }
    }

    /// A ladder answer: the rank's or a page's, in the order asked.
    fn answer(&mut self, got: Result<&LadderPage, &Failure>) {
        let Some(ask) = self.asks.pop_front() else {
            return;
        };
        if let Err(f) = got {
            if f.code == ErrorCode::NOT_SUPPORTED {
                self.off = true;
            }
        }
        match (ask, got) {
            (Ask::Rank, Ok(p)) => self.rank = p.my_rank,
            (Ask::Rank, Err(_)) => self.rank = None,
            (Ask::Page(n), _) if n != self.seq => {}
            (Ask::Page(_), Ok(p)) => {
                self.found = match &self.at {
                    LadderAt::Find(name) => p
                        .rows
                        .iter()
                        .find(|r| r.char.eq_ignore_ascii_case(name))
                        .map(|r| r.char.clone()),
                    _ => None,
                };
                if self.at == LadderAt::Me && p.my_rank.is_none() {
                    self.note = Some(("Your character is not on this ladder.".into(), GREY));
                }
                // The character's own view also tells its rank.
                if let Some(c) = self.character() {
                    let q = rank_query(&c);
                    if (q.season, q.split, q.class) == (p.season, p.split, p.class) {
                        self.rank = p.my_rank;
                    }
                }
                self.page = Some(p.clone());
            }
            (Ask::Page(_), Err(f)) => {
                let text = match (&self.at, f.code) {
                    (_, ErrorCode::NOT_SUPPORTED) => return,
                    (LadderAt::Find(name), lobby_code::UNKNOWN_PLAYER) => {
                        format!("No character named {name} here.")
                    }
                    _ => failure_text(f),
                };
                self.note = Some((text, RED));
            }
        }
    }
}

/// `n` with thousands separators (`3,151,000,000`).
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The season chip's text: the realm's name for it, else "Test season",
/// "Non-ladder" or "Season N".
pub fn season_label(seasons: Option<&[LadderSeason]>, season: u16) -> String {
    let s = seasons.and_then(|l| l.iter().find(|s| s.season == season));
    match s {
        Some(s) if !s.name.is_empty() => s.name.clone(),
        Some(s) if s.test => "Test season".into(),
        _ if season == 0 => "Non-ladder".into(),
        _ => format!("Season {season}"),
    }
}

/// A row's cells: rank, name (with its mark), class, level, experience,
/// tag; and its colour.
pub fn row_cells(r: &LadderRow) -> ([String; 6], i32) {
    let dead = r.flags & ladder_flag::DEAD != 0;
    let me = r.flags & ladder_flag::ME != 0;
    let mut name = r.char.clone();
    if dead {
        name.push_str(" +");
    }
    if me {
        name.push_str(" (you)");
    }
    let colour = if dead {
        RED
    } else if me {
        YELLOW
    } else {
        WHITE
    };
    let class = CLASSES
        .get(usize::from(r.class))
        .copied()
        .unwrap_or("?")
        .to_string();
    (
        [
            r.rank.to_string(),
            name,
            class,
            r.level.to_string(),
            thousands(r.experience),
            r.tag.clone().unwrap_or_default(),
        ],
        colour,
    )
}

fn text_item(font: u16, x: i32, y: i32, color: i32, text: &str) -> DrawItem {
    DrawItem::Text {
        string_id: 0,
        text: text.to_string(),
        font,
        at: Point::new(x, y),
        label: None,
        color,
        boxed: None,
    }
}

impl RightPanel for LadderPanel {
    fn open(&mut self) {
        let c = self.character();
        let name = c.as_ref().map(|c| c.name.clone());
        if name != self.for_char {
            self.view_of(c.as_ref());
            self.for_char = name;
        }
        self.off = false;
        if self.seasons.is_none() {
            self.online.borrow_mut().send(Request::LadderSeasons);
        }
        self.found = None;
        self.ask(LadderAt::Rank(1));
    }

    fn feed(&mut self, ev: &Event) {
        match ev {
            Event::LadderSeasons(list) => self.seasons = Some(list.clone()),
            Event::Ladder(p) => self.answer(Ok(p)),
            Event::Refused {
                asked: Asked::LadderQuery,
                failure,
            } => self.answer(Err(failure)),
            Event::Refused {
                asked: Asked::LadderSeasons,
                failure,
            } if failure.code == ErrorCode::NOT_SUPPORTED => self.off = true,
            Event::Disconnected(_) => {
                self.asks.clear();
                self.seasons = None;
                self.for_char = None;
            }
            _ => {}
        }
    }

    fn controls(&mut self, area: Rect, v: &mut Vec<Control>) {
        self.area = area;
        v.extend(field_controls(area.x + 4, area.y + FIND_BOTTOM, FIND));
        self.find_ix = Some(v.len() - 1);
        v.push(narrow_button(
            area.x + MY_RANK_X,
            area.y + FIND_BOTTOM + 1,
            MY_RANK_W,
            "MY RANK",
            MY_RANK,
        ));
    }

    fn overlay(
        &mut self,
        area: Rect,
        adv: &dyn Fn(u16, &[u16]) -> i32,
        _now_ms: u64,
    ) -> Vec<DrawItem> {
        self.area = area;
        let width = |font: u16| move |s: &str| adv(font, &s.encode_utf16().collect::<Vec<u16>>());
        let w8 = width(FONT8);
        let w16 = width(FONT16);
        let (x0, y0) = (area.x, area.y);
        let mut out = Vec::new();
        // The chips: a dark box, its outline (twice when chosen), the
        // label centred, gold when chosen.
        for (chip, x, w, row) in Chip::layout() {
            let r = self.chip_rect(x, w, row);
            let at = r.origin();
            let (bw, bh) = (i32::from(r.w), i32::from(r.h));
            out.push(DrawItem::Rect { at, w: bw, h: bh });
            out.push(DrawItem::Border { at, w: bw, h: bh });
            let on = self.chosen(chip);
            if on && chip != Chip::Season {
                out.push(DrawItem::Border {
                    at: Point::new(r.x + 1, r.y + 1),
                    w: bw - 2,
                    h: bh - 2,
                });
            }
            let text = match chip {
                Chip::Season => season_label(self.seasons.as_deref(), self.season),
                Chip::Hardcore => "HC".into(),
                Chip::Softcore => "SC".into(),
                Chip::Expansion => "EXP".into(),
                Chip::Classic => "CLASSIC".into(),
                Chip::Class(255) => "ALL".into(),
                Chip::Class(c) => CLASSES[usize::from(c.min(6))].into(),
            }
            .to_uppercase();
            let text = fit(&text, bw - 6, &w8);
            let tx = r.x + (bw - w8(&text)) / 2;
            let colour = if on { GOLD } else { TAN };
            out.push(text_item(FONT8, tx, r.y + bh - 3, colour, &text));
        }
        // The header.
        let head = y0 + HEAD_BASE;
        for (x, h) in [
            (COL_RANK, "#"),
            (COL_NAME, "CHARACTER"),
            (COL_CLASS, "CLS"),
            (COL_LEVEL, "LVL"),
            (COL_GUILD, "GLD"),
        ] {
            out.push(text_item(FONT8, x0 + x, head, GOLD, h));
        }
        let exp = "EXPERIENCE";
        out.push(text_item(
            FONT8,
            x0 + COL_EXP_RIGHT - w8(exp),
            head,
            GOLD,
            exp,
        ));
        if let Some(text) = self.empty_text() {
            let tx = x0 + (i32::from(area.w) - w16(text)) / 2;
            out.push(text_item(FONT16, tx, y0 + 100, WHITE, text));
        } else {
            // The found character's row, framed.
            for (base, r) in self.placed_rows() {
                if self
                    .found
                    .as_ref()
                    .is_some_and(|f| f.eq_ignore_ascii_case(&r.char))
                {
                    let rr = self.row_rect(base);
                    out.push(DrawItem::Border {
                        at: rr.origin(),
                        w: i32::from(rr.w),
                        h: i32::from(rr.h) + 1,
                    });
                }
            }
            if self.page.as_ref().is_some_and(|p| p.pinned.is_some()) {
                out.push(text_item(FONT8, x0 + COL_RANK, y0 + DOTS_BASE, GREY, "..."));
            }
            for (base, r) in self.placed_rows() {
                let (cells, colour) = row_cells(r);
                let [rank, name, class, level, experience, tag] = cells;
                let rank = fit(&rank, COL_NAME - COL_RANK - 4, &w8);
                out.push(text_item(FONT8, x0 + COL_RANK, base, colour, &rank));
                // The name cut before its mark, the mark kept whole.
                let mark = &name[r.char.len()..];
                let room = COL_CLASS - COL_NAME - 4 - w8(mark);
                let shown = format!("{}{mark}", fit(&r.char, room, &w8));
                out.push(text_item(FONT8, x0 + COL_NAME, base, colour, &shown));
                out.push(text_item(FONT8, x0 + COL_CLASS, base, colour, &class));
                out.push(text_item(FONT8, x0 + COL_LEVEL, base, colour, &level));
                out.push(text_item(
                    FONT8,
                    x0 + COL_EXP_RIGHT - w8(&experience),
                    base,
                    colour,
                    &experience,
                ));
                let tag = fit(&tag, RIGHT - COL_GUILD, &w8);
                out.push(text_item(FONT8, x0 + COL_GUILD, base, GREEN, &tag));
            }
        }
        // The legend and the page's ranks right-aligned, or a note.
        let base = y0 + LEGEND_BASE;
        let mut room = RIGHT - COL_RANK;
        let rows = self.page.as_ref().filter(|p| !p.rows.is_empty());
        if let Some(p) = rows.filter(|_| self.note.is_none()) {
            let last = p.first_rank + p.rows.len() as u32 - 1;
            let ranks = format!("{}-{} of {}", p.first_rank, last, p.total);
            let rw = w8(&ranks);
            out.push(text_item(FONT8, x0 + RIGHT - rw, base, GREY, &ranks));
            room -= rw + 8;
        }
        let (legend, colour) = self.legend();
        let legend = fit(&legend, room, &w8);
        out.push(text_item(FONT8, x0 + COL_RANK, base, colour, &legend));
        out
    }

    fn sync(&mut self, controls: &mut [Control], now_ms: u64) {
        let Some(c) = self.find_ix.and_then(|i| controls.get_mut(i)) else {
            return;
        };
        if !self.find_focus && self.find.text.is_empty() {
            c.text = Some(FIND_HINT.into());
            c.color = GREY;
        } else {
            show_field(c, &self.find, self.find_focus, now_ms, NAME_MAX);
            c.color = WHITE;
        }
    }

    fn action(&mut self, id: u32) -> Option<PanelOut> {
        match id {
            FIND => self.find_focus = true,
            MY_RANK => {
                self.found = None;
                self.ask(LadderAt::Me);
            }
            _ => return None,
        }
        Some(PanelOut::Rebuild)
    }

    fn mouse(&mut self, p: Point, _now_ms: u64) -> Option<PanelOut> {
        let focus = self.find_rect().contains(p);
        let changed = focus != self.find_focus;
        self.find_focus = focus;
        if focus || self.off {
            return changed.then_some(PanelOut::Rebuild);
        }
        let chip = Chip::layout()
            .into_iter()
            .find(|&(_, x, w, row)| self.chip_rect(x, w, row).contains(p));
        if let Some((chip, ..)) = chip {
            self.press(chip);
            return Some(PanelOut::Rebuild);
        }
        let row = self
            .placed_rows()
            .into_iter()
            .find(|&(base, _)| self.row_rect(base).contains(p))
            .map(|(_, r)| r.clone());
        match row {
            // Not to one's own characters.
            Some(r) if r.flags & ladder_flag::MY_ACCOUNT == 0 => Some(PanelOut::Whisper(r.char)),
            _ => changed.then_some(PanelOut::Rebuild),
        }
    }

    fn wheel(&mut self, delta: i32) -> Option<PanelOut> {
        self.turn(-delta.signum()).then_some(PanelOut::Rebuild)
    }

    fn char(&mut self, c: char) -> bool {
        if !self.find_focus {
            return false;
        }
        self.find
            .type_char(c, |c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        true
    }

    fn key(&mut self, k: u16) -> bool {
        const UP: u16 = 38;
        const DOWN: u16 = 40;
        match k {
            vk::BACKSPACE if self.find_focus => self.find.backspace(),
            UP => {
                self.turn(-1);
            }
            DOWN => {
                self.turn(1);
            }
            _ => return false,
        }
        true
    }

    fn enter(&mut self) -> Option<PanelOut> {
        if !self.find_focus {
            return None;
        }
        let name = self.find.text.trim().to_string();
        if !name.is_empty() {
            self.ask(LadderAt::Find(name));
        }
        Some(PanelOut::Rebuild)
    }

    fn blur(&mut self) {
        self.find_focus = false;
    }

    fn ui_elements(&self) -> Vec<UiElement> {
        let mut v: Vec<UiElement> = Chip::layout()
            .into_iter()
            .map(|(chip, x, w, row)| UiElement::new(chip.id(), self.chip_rect(x, w, row)))
            .collect();
        v.extend(self.placed_rows().into_iter().map(|(base, r)| {
            UiElement::new(
                format!("online_lobby.ladder_row({})", r.char),
                self.row_rect(base),
            )
        }));
        v.push(UiElement::new("online_lobby.ladder_find", self.find_rect()));
        v
    }
}

#[cfg(test)]
mod tests;
