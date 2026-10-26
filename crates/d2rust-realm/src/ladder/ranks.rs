//! The ladder's rank tables (lobby contract section 6): one ladder of a
//! season (hardcore or softcore, expansion or classic) in rank order, kept
//! in memory and computed again at most every `ladder_refresh_s`, and the
//! pages a [`LadderQuery`] asks of it.
//!
//! Rank order: experience (most first), then the time the experience was
//! reached (earlier first), then the name in any case. Every row has its
//! own rank, so a tie on experience is decided by the time. A class view
//! ranks that class's rows alone, from 1.
//!
//! Pages are aligned to the page size: the page with a found character (or
//! the asker's own) starts at a multiple of `rows` plus 1, as paging up and
//! down from the first page lands on it.

use d2_accounts::ladder::LadderEntry;
use d2_net::msg::ladder::{mode, row_flags, LadderQuery, ALL_CLASSES, MAX_ROWS};
use d2_net::msg::ErrorCode;

/// One ladder of a season, in rank order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    rows: Vec<LadderEntry>,
    /// When it was computed (Unix ms): the head's `updated_ms`.
    pub updated_ms: u64,
}

/// A row with its rank in the view it was read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ranked<'a> {
    pub rank: u32,
    pub entry: &'a LadderEntry,
}

/// The answer to a query: the page's rows and the asker's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page<'a> {
    /// Characters in the view.
    pub total: u32,
    pub first_rank: u32,
    pub rows: Vec<Ranked<'a>>,
    /// The asker's selected character's rank in the view.
    pub my_rank: Option<u32>,
    /// The asker's row when it is ranked and not on the page.
    pub pinned: Option<Ranked<'a>>,
}

/// The rank order's key: experience (most first), reached, the name in
/// any case.
fn order(a: &LadderEntry, b: &LadderEntry) -> std::cmp::Ordering {
    b.experience
        .cmp(&a.experience)
        .then(a.reached_ms.cmp(&b.reached_ms))
        .then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
}

impl Table {
    /// The table of `rows` (one ladder's), computed at `updated_ms`.
    pub fn new(mut rows: Vec<LadderEntry>, updated_ms: u64) -> Self {
        rows.sort_by(order);
        Self { rows, updated_ms }
    }

    /// Characters in the whole ladder.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The rows of `class` ([`ALL_CLASSES`]: every row), in rank order.
    fn view(&self, class: u8) -> Vec<&LadderEntry> {
        self.rows
            .iter()
            .filter(|e| class == ALL_CLASSES || e.class == class)
            .collect()
    }

    /// The page `q` asks for; `me` is the asker's selected character.
    /// Refused with `UnknownPlayer` when the character to find (or the
    /// asker's own, for the page around it) isn't in the view.
    pub fn page(&self, q: &LadderQuery, me: Option<&str>) -> Result<Page<'_>, ErrorCode> {
        let view = self.view(q.class);
        let total = u32::try_from(view.len()).unwrap_or(u32::MAX);
        let size = u32::from(q.rows.clamp(1, MAX_ROWS));
        let rank_of = |name: &str| {
            view.iter()
                .position(|e| e.name.eq_ignore_ascii_case(name))
                .map(|i| u32::try_from(i + 1).unwrap_or(u32::MAX))
        };
        let my_rank = me.and_then(rank_of);
        let aligned = |rank: u32| (rank - 1) / size * size + 1;
        let first_rank = match q.mode {
            mode::FIND => aligned(rank_of(&q.find).ok_or(ErrorCode::UNKNOWN_PLAYER)?),
            mode::AROUND_ME => aligned(my_rank.ok_or(ErrorCode::UNKNOWN_PLAYER)?),
            _ => q.from_rank.max(1),
        };
        let ranked = |rank: u32| Ranked {
            rank,
            entry: view[usize::try_from(rank - 1).unwrap_or(usize::MAX)],
        };
        let last = first_rank.saturating_add(size - 1).min(total);
        let rows: Vec<Ranked> = (first_rank..=last).map(ranked).collect();
        let pinned = my_rank.filter(|&m| m < first_rank || m > last).map(ranked);
        Ok(Page {
            total,
            first_rank,
            rows,
            my_rank,
            pinned,
        })
    }
}

/// A row's flags for the asker: its account, its selected character `me`,
/// and whether the row's character is `online`.
pub fn flags(e: &LadderEntry, account: i64, me: Option<&str>, online: bool) -> u8 {
    let mut f = 0;
    if e.dead && e.split.hardcore {
        f |= row_flags::DEAD_HARDCORE;
    }
    if e.account == account {
        f |= row_flags::MY_ACCOUNT;
        if me.is_some_and(|m| m.eq_ignore_ascii_case(&e.name)) {
            f |= row_flags::MY_CHARACTER;
        }
    }
    if online {
        f |= row_flags::ONLINE;
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2_accounts::ladder::Split;

    fn entry(name: &str, class: u8, experience: u64, reached_ms: i64) -> LadderEntry {
        LadderEntry {
            season: 1,
            name: name.into(),
            account: 1,
            class,
            level: 1,
            experience,
            split: Split {
                hardcore: false,
                expansion: true,
            },
            dead: false,
            reached_ms,
        }
    }

    fn query(mode: u8, from_rank: u32, find: &str, rows: u8) -> LadderQuery {
        LadderQuery {
            season: 1,
            mode,
            from_rank,
            find: find.into(),
            rows,
            ..LadderQuery::default()
        }
    }

    fn names(p: &Page) -> Vec<String> {
        p.rows.iter().map(|r| r.entry.name.clone()).collect()
    }

    /// `n` characters, `c0` the best, classes in turn.
    fn many(n: u32) -> Table {
        let rows = (0..n)
            .map(|i| {
                let class = u8::try_from(i % 7).unwrap();
                entry(&format!("c{i}"), class, u64::from(10_000 - i), 0)
            })
            .collect();
        Table::new(rows, 5)
    }

    #[test]
    fn ranks_ties() {
        let t = Table::new(
            vec![
                entry("Late", 0, 500, 20),
                entry("bravo", 0, 500, 10),
                entry("Alpha", 0, 500, 10),
                entry("Top", 0, 900, 99),
                entry("Low", 0, 1, 0),
            ],
            0,
        );
        let p = t.page(&query(mode::FROM_RANK, 1, "", 50), None).unwrap();
        assert_eq!(names(&p), ["Top", "Alpha", "bravo", "Late", "Low"]);
        let ranks: Vec<u32> = p.rows.iter().map(|r| r.rank).collect();
        assert_eq!(ranks, [1, 2, 3, 4, 5], "every row has a rank of its own");
        assert_eq!((p.total, p.first_rank), (5, 1));
    }

    #[test]
    fn dead_marked() {
        let mut dead = entry("Gone", 0, 5, 0);
        dead.dead = true;
        dead.split.hardcore = true;
        assert_eq!(flags(&dead, 2, None, false), row_flags::DEAD_HARDCORE);
        // Softcore deaths aren't marked: the character plays on.
        dead.split.hardcore = false;
        assert_eq!(flags(&dead, 2, None, false), 0);
        let mine = entry("Mine", 0, 5, 0);
        assert_eq!(
            flags(&mine, 1, Some("MINE"), true),
            row_flags::MY_ACCOUNT | row_flags::MY_CHARACTER | row_flags::ONLINE
        );
        assert_eq!(flags(&mine, 1, Some("Other"), false), row_flags::MY_ACCOUNT);
        assert_eq!(flags(&mine, 2, Some("Mine"), false), 0, "another account's");
    }

    #[test]
    fn find_whole_name_any_case() {
        let t = many(120);
        let p = t.page(&query(mode::FIND, 1, "C73", 50), None).unwrap();
        assert_eq!(p.first_rank, 51, "the page that holds rank 74");
        assert_eq!(p.rows.len(), 50);
        assert!(p.rows.iter().any(|r| r.rank == 74 && r.entry.name == "c73"));
        for part in ["c", "73", "c73x", ""] {
            assert_eq!(
                t.page(&query(mode::FIND, 1, part, 50), None),
                Err(ErrorCode::UNKNOWN_PLAYER),
                "{part:?} is not a whole name"
            );
        }
        // The class view ranks its own rows: c73 is class 3, the 11th of it.
        let mut q = query(mode::FIND, 1, "c73", 10);
        q.class = 3;
        let p = t.page(&q, None).unwrap();
        assert_eq!((p.total, p.first_rank), (17, 11));
        assert_eq!(p.rows[0].entry.name, "c73");
        // Not in this class's view.
        q.class = 4;
        assert_eq!(t.page(&q, None), Err(ErrorCode::UNKNOWN_PLAYER));
    }

    #[test]
    fn pinned_row_off_page() {
        let t = many(60);
        let p = t
            .page(&query(mode::FROM_RANK, 1, "", 50), Some("c55"))
            .unwrap();
        assert_eq!(p.my_rank, Some(56));
        assert_eq!(p.rows.len(), 50);
        let pinned = p.pinned.expect("off the page: pinned");
        assert_eq!((pinned.rank, pinned.entry.name.as_str()), (56, "c55"));
        // On the page: not pinned.
        let p = t
            .page(&query(mode::FROM_RANK, 51, "", 50), Some("c55"))
            .unwrap();
        assert_eq!((p.first_rank, p.rows.len(), p.pinned), (51, 10, None));
        // Around me: the page that holds it.
        let p = t
            .page(&query(mode::AROUND_ME, 1, "", 50), Some("C55"))
            .unwrap();
        assert_eq!((p.first_rank, p.my_rank, p.pinned), (51, Some(56), None));
        // No ranked character: no own rank, and no page around it.
        let p = t
            .page(&query(mode::FROM_RANK, 1, "", 50), Some("nobody"))
            .unwrap();
        assert_eq!((p.my_rank, p.pinned), (None, None));
        assert_eq!(
            t.page(&query(mode::AROUND_ME, 1, "", 50), None),
            Err(ErrorCode::UNKNOWN_PLAYER)
        );
    }

    #[test]
    fn pages_past_the_end_are_empty() {
        let t = many(60);
        let p = t.page(&query(mode::FROM_RANK, 61, "", 50), None).unwrap();
        assert_eq!((p.total, p.first_rank, p.rows.len()), (60, 61, 0));
        let p = t.page(&query(mode::FROM_RANK, 0, "", 0), None).unwrap();
        assert_eq!(
            (p.first_rank, p.rows.len()),
            (1, 1),
            "rank 1 and one row at least"
        );
        let empty = Table::default();
        let p = empty
            .page(&query(mode::FROM_RANK, 1, "", 50), None)
            .unwrap();
        assert_eq!((p.total, p.rows.len()), (0, 0));
    }
}
