//! The ladder on the control stream (kinds 0x50-0x57, inside the lobby's
//! range; `docs/plans/server/lobby-contract.md` section 6): the seasons
//! and the pages of a ladder.
//!
//! Every C→S message is a [`ToRealm`] with a request number (`req`, the
//! session's counter the social messages share). Every request is answered
//! with its rows, then [`LadderEnd`] (code 0, or the refusal): seasons are
//! [`FromRealm::SeasonRow`]s; a page is [`FromRealm::Head`], its rows,
//! the pinned own row when it is off the page, then the end.

use super::lobby::MAX_FROM;
use super::social::{valid_class, MAX_TAG, NO_CLASS};
use super::{ErrorCode, Reader, Writer};
use crate::NetError;

/// The ladder's kinds.
pub mod kind {
    pub const SEASONS: u8 = 0x50;
    pub const SEASON_ROW: u8 = 0x51;
    pub const QUERY: u8 = 0x52;
    pub const HEAD: u8 = 0x53;
    pub const ROW: u8 = 0x54;
    pub const END: u8 = 0x55;
}

/// A season's name ("Season 1", "Test season").
pub const MAX_SEASON_NAME: usize = 32;
/// Most rows one page asks for.
pub const MAX_ROWS: u8 = 50;
/// All classes, in [`LadderQuery::class`].
pub const ALL_CLASSES: u8 = NO_CLASS;

/// The bits of a ladder's split (`u8`).
pub mod split {
    pub const HARDCORE: u8 = 1 << 0;
    pub const EXPANSION: u8 = 1 << 1;
}

/// How a [`LadderQuery`] picks its page (`u8`).
pub mod mode {
    /// The page from `from_rank`.
    pub const FROM_RANK: u8 = 0;
    /// The page with the character named by `find`.
    pub const FIND: u8 = 1;
    /// The page around the asker's selected character.
    pub const AROUND_ME: u8 = 2;
}

/// The bits of [`LadderRow::flags`].
pub mod row_flags {
    pub const DEAD_HARDCORE: u8 = 1 << 0;
    /// One of the asker's account's characters.
    pub const MY_ACCOUNT: u8 = 1 << 1;
    /// The asker's selected character.
    pub const MY_CHARACTER: u8 = 1 << 2;
    pub const ONLINE: u8 = 1 << 3;
    /// The own row pinned after a page it isn't on.
    pub const PINNED: u8 = 1 << 4;
}

/// A season of the realm (S→C 0x51, after its `req`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeasonRow {
    /// 0: the non-ladder characters, always listed.
    pub season: u16,
    pub name: String,
    pub started_ms: u64,
    /// 0 while open.
    pub ended_ms: u64,
    pub test: bool,
    pub current: bool,
}

impl SeasonRow {
    fn encode(&self, w: &mut Writer) {
        w.u16(self.season)
            .str8(&self.name)
            .u64(self.started_ms)
            .u64(self.ended_ms)
            .bool(self.test)
            .bool(self.current);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            season: r.u16()?,
            name: r.str8(MAX_SEASON_NAME)?,
            started_ms: r.u64()?,
            ended_ms: r.u64()?,
            test: r.bool()?,
            current: r.bool()?,
        })
    }
}

/// What a ladder page asks (C→S 0x52, after its `req`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LadderQuery {
    pub season: u16,
    /// [`split`].
    pub split: u8,
    /// 0-6, [`ALL_CLASSES`].
    pub class: u8,
    /// [`mode`].
    pub mode: u8,
    /// 1-based, for [`mode::FROM_RANK`].
    pub from_rank: u32,
    /// A whole character name, any case, for [`mode::FIND`].
    pub find: String,
    /// 1-[`MAX_ROWS`].
    pub rows: u8,
}

impl Default for LadderQuery {
    fn default() -> Self {
        Self {
            season: 0,
            split: split::EXPANSION,
            class: ALL_CLASSES,
            mode: mode::FROM_RANK,
            from_rank: 1,
            find: String::new(),
            rows: MAX_ROWS,
        }
    }
}

impl LadderQuery {
    fn encode(&self, w: &mut Writer) {
        w.u16(self.season)
            .u8(self.split)
            .u8(self.class)
            .u8(self.mode)
            .u32(self.from_rank)
            .str8(&self.find)
            .u8(self.rows);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let q = Self {
            season: r.u16()?,
            split: r.u8()?,
            class: r.u8()?,
            mode: r.u8()?,
            from_rank: r.u32()?,
            find: r.str8(MAX_FROM)?,
            rows: r.u8()?,
        };
        if q.split & !(split::HARDCORE | split::EXPANSION) != 0
            || !valid_class(q.class)
            || q.mode > mode::AROUND_ME
        {
            return Err(NetError::Malformed);
        }
        Ok(q)
    }
}

/// S→C 0x53: a page follows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LadderHead {
    pub req: u32,
    pub season: u16,
    pub split: u8,
    pub class: u8,
    /// Characters in this view.
    pub total: u32,
    pub first_rank: u32,
    /// Rows that follow (the pinned row not counted).
    pub rows: u8,
    /// The selected character's rank in this view.
    pub my_rank: Option<u32>,
    /// When the ranks were computed.
    pub updated_ms: u64,
    pub now_ms: u64,
}

impl LadderHead {
    fn encode(&self, w: &mut Writer) {
        w.u32(self.req)
            .u16(self.season)
            .u8(self.split)
            .u8(self.class)
            .u32(self.total)
            .u32(self.first_rank)
            .u8(self.rows);
        w.opt(self.my_rank.as_ref(), |w, v| {
            w.u32(*v);
        });
        w.u64(self.updated_ms).u64(self.now_ms);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let h = Self {
            req: r.u32()?,
            season: r.u16()?,
            split: r.u8()?,
            class: r.u8()?,
            total: r.u32()?,
            first_rank: r.u32()?,
            rows: r.u8()?,
            my_rank: r.opt(|r| r.u32())?,
            updated_ms: r.u64()?,
            now_ms: r.u64()?,
        };
        if h.rows > MAX_ROWS || !valid_class(h.class) {
            return Err(NetError::Malformed);
        }
        Ok(h)
    }

    /// Whether the own row comes pinned after the page's rows: the rank is
    /// set and off this page.
    pub fn pinned(&self) -> bool {
        self.my_rank.is_some_and(|m| {
            m < self.first_rank || m >= self.first_rank.saturating_add(u32::from(self.rows))
        })
    }
}

/// A character's row (S→C 0x54, after its `req`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LadderRow {
    pub rank: u32,
    pub char: String,
    pub class: u8,
    pub level: u8,
    pub experience: u64,
    pub tag: Option<String>,
    /// [`row_flags`].
    pub flags: u8,
}

impl LadderRow {
    fn encode(&self, w: &mut Writer) {
        w.u32(self.rank)
            .str8(&self.char)
            .u8(self.class)
            .u8(self.level)
            .u64(self.experience);
        w.opt(self.tag.as_ref(), |w, t| {
            w.str8(t);
        });
        w.u8(self.flags);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let row = Self {
            rank: r.u32()?,
            char: r.str8(MAX_FROM)?,
            class: r.u8()?,
            level: r.u8()?,
            experience: r.u64()?,
            tag: r.opt(|r| r.str8(MAX_TAG))?,
            flags: r.u8()?,
        };
        if row.class >= super::account::CLASSES || row.flags & !0x1F != 0 {
            return Err(NetError::Malformed);
        }
        Ok(row)
    }
}

/// S→C 0x55: the end of every ladder answer: 0, or the refusal
/// (`UnknownPlayer` for a find with no such character in the view,
/// `NoSuchSeason`, `RateLimited` with `wait_s`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LadderEnd {
    pub req: u32,
    pub code: ErrorCode,
    pub wait_s: u32,
}

impl LadderEnd {
    pub fn ok(req: u32) -> Self {
        Self {
            req,
            code: ErrorCode(0),
            wait_s: 0,
        }
    }

    pub fn is_ok(&self) -> bool {
        self.code.0 == 0
    }
}

/// What a C→S ladder message asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// The seasons, newest first, season 0 last.
    Seasons,
    Query(LadderQuery),
}

impl Ask {
    pub fn kind(&self) -> u8 {
        match self {
            Self::Seasons => kind::SEASONS,
            Self::Query(_) => kind::QUERY,
        }
    }
}

/// A C→S ladder message: its request number and what it asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToRealm {
    pub req: u32,
    pub ask: Ask,
}

impl ToRealm {
    pub fn kind(&self) -> u8 {
        self.ask.kind()
    }

    pub fn body(&self) -> Vec<u8> {
        let mut w = Writer::new(self.kind());
        w.u32(self.req);
        match &self.ask {
            Ask::Seasons => {}
            Ask::Query(q) => q.encode(&mut w),
        }
        w.finish()
    }

    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut rest) = Reader::new(body)?;
        let r = &mut rest;
        let req = r.u32()?;
        let ask = match k {
            kind::SEASONS => Ask::Seasons,
            kind::QUERY => Ask::Query(LadderQuery::decode(r)?),
            _ => return Err(NetError::Malformed),
        };
        rest.finish()?;
        Ok(Self { req, ask })
    }
}

/// An S→C ladder message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FromRealm {
    SeasonRow { req: u32, row: SeasonRow },
    Head(LadderHead),
    Row { req: u32, row: LadderRow },
    End(LadderEnd),
}

impl FromRealm {
    pub fn kind(&self) -> u8 {
        match self {
            Self::SeasonRow { .. } => kind::SEASON_ROW,
            Self::Head(_) => kind::HEAD,
            Self::Row { .. } => kind::ROW,
            Self::End(_) => kind::END,
        }
    }

    pub fn req(&self) -> u32 {
        match self {
            Self::SeasonRow { req, .. } | Self::Row { req, .. } => *req,
            Self::Head(h) => h.req,
            Self::End(e) => e.req,
        }
    }

    pub fn body(&self) -> Vec<u8> {
        let mut w = Writer::new(self.kind());
        let w = &mut w;
        match self {
            Self::SeasonRow { req, row } => {
                w.u32(*req);
                row.encode(w);
            }
            Self::Head(h) => h.encode(w),
            Self::Row { req, row } => {
                w.u32(*req);
                row.encode(w);
            }
            Self::End(e) => {
                w.u32(e.req).u16(e.code.0).u32(e.wait_s);
            }
        }
        w.finish()
    }

    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut rest) = Reader::new(body)?;
        let r = &mut rest;
        let m = match k {
            kind::SEASON_ROW => Self::SeasonRow {
                req: r.u32()?,
                row: SeasonRow::decode(r)?,
            },
            kind::HEAD => Self::Head(LadderHead::decode(r)?),
            kind::ROW => Self::Row {
                req: r.u32()?,
                row: LadderRow::decode(r)?,
            },
            kind::END => Self::End(LadderEnd {
                req: r.u32()?,
                code: ErrorCode(r.u16()?),
                wait_s: r.u32()?,
            }),
            _ => return Err(NetError::Malformed),
        };
        rest.finish()?;
        Ok(m)
    }
}

#[cfg(test)]
#[path = "ladder_tests.rs"]
mod tests;
