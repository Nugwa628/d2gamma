//! The fake's ladder (lobby contract section 6): the seasons, 60 ranked
//! softcore expansion characters of Season 1 plus a few hardcore (one
//! dead) and classic ones, and the session's own characters of the season
//! asked. Ranks: experience, then when it was reached, then the name.

use d2_formats::d2s::status;

use super::super::link::{
    ladder_flag, ladder_split, lobby_code, Asked, ErrorCode, Event, LadderAt, LadderPage,
    LadderQuery, LadderRow, LadderSeason, Request,
};
use super::{refused, FakeServer};

/// A ranked character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LadderChar {
    pub season: u16,
    pub name: String,
    pub account: String,
    pub class: u8,
    pub level: u8,
    pub experience: u64,
    pub hardcore: bool,
    pub expansion: bool,
    pub dead: bool,
    /// When the experience was reached (Unix ms).
    pub reached_ms: u64,
    pub tag: Option<String>,
    pub online: bool,
}

/// The fake's ladder.
#[derive(Clone, Debug)]
pub struct FakeLadder {
    /// In any order; answered newest first, season 0 last.
    pub seasons: Vec<LadderSeason>,
    pub chars: Vec<LadderChar>,
    /// When the ranks were "computed".
    pub updated_ms: u64,
}

/// Name parts for the seeded characters (60 unique names).
const FIRST: [&str; 12] = [
    "Ash", "Bel", "Cor", "Dra", "Eld", "Fen", "Gal", "Hro", "Ith", "Jor", "Kel", "Lyr",
];
const LAST: [&str; 5] = ["ana", "dor", "mir", "wyn", "rak"];

/// The experience the fake gives a level (rising, not the game's table).
pub fn experience_of(level: u8) -> u64 {
    let l = u64::from(level);
    l * l * l * 3_500
}

impl FakeLadder {
    /// The seeded ladder at clock `now`.
    pub fn seeded(now: u64) -> Self {
        let day = 86_400_000;
        let seasons = vec![
            LadderSeason {
                season: 1,
                name: "Season 1".into(),
                started_ms: now - 60 * day,
                ended_ms: 0,
                test: false,
                current: true,
            },
            LadderSeason {
                season: 0,
                name: "Non-ladder".into(),
                ..LadderSeason::default()
            },
        ];
        let mut chars = Vec::new();
        for i in 0..60usize {
            let name = format!("{}{}", FIRST[i % FIRST.len()], LAST[i / FIRST.len()]);
            // Every tenth character ties with the one before on
            // experience: the one that reached it first ranks first.
            let level = 99 - (i * 3 / 2) as u8 + u8::from(i % 10 == 1);
            let experience = experience_of(level);
            chars.push(LadderChar {
                season: 1,
                account: name.to_ascii_lowercase(),
                name,
                class: (i % 7) as u8,
                level,
                experience,
                hardcore: false,
                expansion: true,
                dead: false,
                reached_ms: now - day + (i as u64) * 60_000,
                tag: (i % 4 == 0).then(|| "NUG".to_string()),
                online: i % 3 == 0,
            });
        }
        for (i, (name, level, dead)) in [
            ("Ironmaw", 88, false),
            ("Sablethorn", 81, true),
            ("Kettlebrand", 77, false),
            ("Duskwalker", 63, false),
            ("Brinehollow", 52, true),
            ("Oathkeeper", 40, false),
        ]
        .into_iter()
        .enumerate()
        {
            chars.push(LadderChar {
                season: 1,
                name: name.into(),
                account: name.to_ascii_lowercase(),
                class: (i % 7) as u8,
                level,
                experience: experience_of(level),
                hardcore: true,
                expansion: true,
                dead,
                reached_ms: now - 2 * day,
                tag: None,
                online: false,
            });
        }
        for (i, (name, level)) in [("Oldschool", 75), ("Pureclassic", 61), ("Lodbrok", 44)]
            .into_iter()
            .enumerate()
        {
            chars.push(LadderChar {
                season: 1,
                name: name.into(),
                account: name.to_ascii_lowercase(),
                class: (i % 5) as u8,
                level,
                experience: experience_of(level),
                hardcore: false,
                expansion: false,
                dead: false,
                reached_ms: now - 3 * day,
                tag: None,
                online: false,
            });
        }
        FakeLadder {
            seasons,
            chars,
            updated_ms: now - 30_000,
        }
    }
}

impl FakeServer {
    /// One view's characters in rank order: the seeded ones and the
    /// session's own of that season.
    fn ladder_view(&self, q: &LadderQuery) -> Vec<LadderChar> {
        let hc = q.split & ladder_split::HARDCORE != 0;
        let exp = q.split & ladder_split::EXPANSION != 0;
        let account = self.my_account().unwrap_or_default().to_string();
        let tag = self.social.my_guild().map(|g| g.tag.clone());
        let selected = self.selected.as_ref().map(|c| c.name.clone());
        let own = self.my_chars().iter().map(|c| LadderChar {
            season: c.season,
            name: c.name.clone(),
            account: account.clone(),
            class: c.class,
            level: c.level,
            experience: experience_of(c.level),
            hardcore: c.status & status::HARDCORE != 0,
            expansion: c.status & status::EXPANSION != 0,
            dead: c.status & status::DEAD != 0,
            reached_ms: self.now_ms,
            tag: tag.clone(),
            online: selected
                .as_ref()
                .is_some_and(|s| s.eq_ignore_ascii_case(&c.name)),
        });
        let mut view: Vec<LadderChar> = self
            .ladder
            .chars
            .iter()
            .cloned()
            .chain(own)
            .filter(|c| c.season == q.season && c.hardcore == hc && c.expansion == exp)
            .filter(|c| q.class == 255 || c.class == q.class)
            .collect();
        view.sort_by(|a, b| {
            b.experience
                .cmp(&a.experience)
                .then(a.reached_ms.cmp(&b.reached_ms))
                .then(
                    a.name
                        .to_ascii_lowercase()
                        .cmp(&b.name.to_ascii_lowercase()),
                )
        });
        view
    }

    fn ladder_row(&self, c: &LadderChar, rank: u32) -> LadderRow {
        let mine = self
            .my_account()
            .is_some_and(|a| a.eq_ignore_ascii_case(&c.account));
        let me = mine
            && self
                .selected
                .as_ref()
                .is_some_and(|s| s.name.eq_ignore_ascii_case(&c.name));
        let mut flags = 0;
        for (bit, on) in [
            (ladder_flag::DEAD, c.hardcore && c.dead),
            (ladder_flag::MY_ACCOUNT, mine),
            (ladder_flag::ME, me),
            (ladder_flag::ONLINE, c.online),
        ] {
            if on {
                flags |= bit;
            }
        }
        LadderRow {
            rank,
            char: c.name.clone(),
            class: c.class,
            level: c.level,
            experience: c.experience,
            tag: c.tag.clone(),
            flags,
        }
    }

    /// The seasons and the ladder pages.
    pub(super) fn answer_ladder(&mut self, asked: Asked, r: Request) {
        match r {
            Request::LadderSeasons => {
                let mut list = self.ladder.seasons.clone();
                list.sort_by_key(|s| (s.season == 0, std::cmp::Reverse(s.season)));
                self.out.push_back(Event::LadderSeasons(list));
            }
            Request::LadderQuery(q) => {
                if !self.ladder.seasons.iter().any(|s| s.season == q.season) {
                    return self
                        .out
                        .push_back(refused(asked, lobby_code::NO_SUCH_SEASON, ""));
                }
                let view = self.ladder_view(&q);
                let rows = usize::from(q.rows.clamp(1, 50));
                let mine = self.selected.as_ref().and_then(|s| {
                    let account = self.my_account()?;
                    view.iter().position(|c| {
                        c.name.eq_ignore_ascii_case(&s.name)
                            && c.account.eq_ignore_ascii_case(account)
                    })
                });
                let page_of = |i: usize| i / rows * rows;
                let first = match &q.at {
                    LadderAt::Rank(r) => (*r).max(1) as usize - 1,
                    LadderAt::Find(name) => {
                        match view.iter().position(|c| c.name.eq_ignore_ascii_case(name)) {
                            Some(i) => page_of(i),
                            None => {
                                return self.out.push_back(refused(
                                    asked,
                                    lobby_code::UNKNOWN_PLAYER,
                                    "",
                                ))
                            }
                        }
                    }
                    LadderAt::Me => mine.map_or(0, page_of),
                };
                let page: Vec<LadderRow> = view
                    .iter()
                    .enumerate()
                    .skip(first)
                    .take(rows)
                    .map(|(i, c)| self.ladder_row(c, i as u32 + 1))
                    .collect();
                let pinned = mine
                    .filter(|i| !(first..first + rows).contains(i))
                    .map(|i| {
                        let mut row = self.ladder_row(&view[i], i as u32 + 1);
                        row.flags |= ladder_flag::PINNED;
                        row
                    });
                self.out.push_back(Event::Ladder(LadderPage {
                    season: q.season,
                    split: q.split,
                    class: q.class,
                    total: view.len() as u32,
                    first_rank: first as u32 + 1,
                    my_rank: mine.map(|i| i as u32 + 1),
                    updated_ms: self.ladder.updated_ms,
                    now_ms: self.now_ms,
                    rows: page,
                    pinned,
                }));
            }
            _ => self
                .out
                .push_back(refused(asked, ErrorCode::NOT_SUPPORTED, "")),
        }
    }
}
