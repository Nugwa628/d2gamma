//! The shared stash's control-stream messages (kinds 0xA0-0xAF, contracts
//! 11.13; plan N28): the stash's pages, the moves between the character
//! in game and the stash, and the item pages' own moves.
//!
//! The main server makes every move with the character's game server
//! (the link's `ItemWanted` / `ItemOut` / `ItemIn`, [`super::node`]); the
//! client only asks and shows the answer. Every changing request carries
//! `req`, a number rising per account: a repeat gets the first answer
//! and changes nothing. A failed request is answered with
//! [`StashEvent::Done`] and the refusal's code, never with `ERROR`, so the
//! client always knows which request ended.
//!
//! An item page is sent as [`StashEvent::Page`] (the header: gold,
//! stackables and how many items follow), then one [`StashEvent::Item`]
//! per item, so a full page never passes a control frame.

use std::ops::RangeInclusive;

use super::{Reader, Writer};
use crate::NetError;

/// The stash's kinds (the market's are 0xB0-0xCF).
pub const KINDS: RangeInclusive<u8> = 0xA0..=0xAF;

/// The stash's kinds.
pub mod kind {
    pub const STASH_OPEN: u8 = 0xA0;
    pub const STASH_PAGE: u8 = 0xA1;
    pub const STASH_PUT: u8 = 0xA2;
    pub const STASH_TAKE: u8 = 0xA3;
    pub const STASH_DONE: u8 = 0xA4;
    pub const STASH_PUT_STACKS: u8 = 0xA5;
    pub const STASH_MOVE: u8 = 0xA6;
    pub const STASH_ARRANGE: u8 = 0xA7;
    pub const STASH_SEARCH: u8 = 0xA8;
    pub const STASH_FOUND: u8 = 0xA9;
    pub const STASH_ITEM: u8 = 0xAA;
    /// C→S: names an item page (a mod realm's renamable pages).
    pub const STASH_RENAME: u8 = 0xAB;
    /// C→S: asks for the item pages' shape and names.
    pub const STASH_LAYOUT: u8 = 0xAC;
    /// S→C: the item pages' shape and names.
    pub const STASH_SHAPE: u8 = 0xAD;
}

/// `StashOpen.tab`.
pub mod tab {
    /// An item page.
    pub const ITEMS: u8 = 0;
    /// The stackables and the gold.
    pub const STACKS: u8 = 1;
}

/// What a move carries (`StashPut.to`, `StashTake.from`, and the link's
/// `ItemWanted.to`, `ItemIn.from`).
pub mod goods {
    pub const ITEM: u8 = 1;
    pub const STACK: u8 = 2;
    pub const GOLD: u8 = 3;
}

/// Where a taken item goes on the character (`StashTake.place`).
pub mod place {
    pub const INVENTORY: u8 = 0;
    pub const CURSOR: u8 = 1;
}

/// An item's bytes: at most 2048 (an item and its socketed items).
pub const MAX_ITEM: usize = 2048;
/// A stackable's code.
pub const MAX_CODE: usize = 8;
/// A search text.
pub const MAX_SEARCH: usize = 64;
/// The units of one `StashPutStacks`, and the units a `StashDone` lists.
pub const MAX_UNITS: usize = 200;
/// The stackables of one `StashPage`.
pub const MAX_STACKS: usize = 200;
/// The hits of one `StashFound`.
pub const MAX_HITS: usize = 400;
/// A page name's bytes (12 characters of up to 4 bytes).
pub const MAX_PAGE_NAME: usize = 48;
/// The page names of one `StashShape` (a realm has at most this many
/// item pages).
pub const MAX_PAGE_NAMES: usize = 64;

/// A cell of an item page: the page, then the column and row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StashCell {
    pub page: u8,
    pub x: u8,
    pub y: u8,
}

impl StashCell {
    fn encode(&self, w: &mut Writer) {
        w.u8(self.page).u8(self.x).u8(self.y);
    }

    fn decode(r: &mut Reader<'_>) -> Result<Self, NetError> {
        Ok(Self {
            page: r.u8()?,
            x: r.u8()?,
            y: r.u8()?,
        })
    }
}

/// S→C 0xAA: one item of an item page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StashItem {
    /// The realm item ID.
    pub id: u32,
    pub cell: StashCell,
    /// Width and height in cells.
    pub size: (u8, u8),
    /// The item's bytes (the save format's item, with its socketed items).
    pub record: Vec<u8>,
}

/// C→S stash requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StashRequest {
    /// 0xA0: an item page ([`tab::ITEMS`]), or the stackables and the gold
    /// ([`tab::STACKS`]). Answered with [`StashEvent::Page`].
    Open { tab: u8, page: u16 },
    /// 0xA2: goods of the character in game go to the stash: the unit
    /// `unit` ([`goods::ITEM`], dropped on `cell` when given; or a whole
    /// stack, [`goods::STACK`]) or `gold` ([`goods::GOLD`]).
    Put {
        req: u64,
        unit: u32,
        to: u8,
        gold: u64,
        cell: Option<StashCell>,
    },
    /// 0xA5: every listed unit that is a stackable goes to its count; the
    /// rest stay ([`StashEvent::Done`]'s `left`).
    PutStacks { req: u64, units: Vec<u32> },
    /// 0xA3: goods of the stash go to the character in game: the item
    /// `item` ([`goods::ITEM`], to `place`), `count` of the stackable
    /// `code` ([`goods::STACK`]) or `gold` ([`goods::GOLD`]).
    Take {
        req: u64,
        from: u8,
        item: Option<u32>,
        stack: Option<(String, u32)>,
        gold: u64,
        place: u8,
    },
    /// 0xA6: an item of the stash to another free cell.
    Move {
        req: u64,
        item: u32,
        cell: StashCell,
    },
    /// 0xA7: packs one item page.
    Arrange { req: u64, page: u8 },
    /// 0xA8: the items whose names contain `text` ([`StashEvent::Found`]).
    Search { text: String },
    /// 0xAB: names item page `page` (an empty name: back to the default).
    /// A realm whose pages are not renamable refuses it.
    Rename { req: u64, page: u8, name: String },
    /// 0xAC: the item pages' shape and names ([`StashEvent::Shape`]). A
    /// realm that doesn't know it answers `ERROR`: the client keeps
    /// D2Gamma's shape.
    Layout,
}

impl StashRequest {
    pub fn kind(&self) -> u8 {
        match self {
            Self::Open { .. } => kind::STASH_OPEN,
            Self::Put { .. } => kind::STASH_PUT,
            Self::PutStacks { .. } => kind::STASH_PUT_STACKS,
            Self::Take { .. } => kind::STASH_TAKE,
            Self::Move { .. } => kind::STASH_MOVE,
            Self::Arrange { .. } => kind::STASH_ARRANGE,
            Self::Search { .. } => kind::STASH_SEARCH,
            Self::Rename { .. } => kind::STASH_RENAME,
            Self::Layout => kind::STASH_LAYOUT,
        }
    }

    /// The request number of a changing request.
    pub fn req(&self) -> Option<u64> {
        match self {
            Self::Put { req, .. }
            | Self::PutStacks { req, .. }
            | Self::Take { req, .. }
            | Self::Move { req, .. }
            | Self::Arrange { req, .. }
            | Self::Rename { req, .. } => Some(*req),
            Self::Open { .. } | Self::Search { .. } | Self::Layout => None,
        }
    }

    pub fn body(&self) -> Vec<u8> {
        let mut w = Writer::new(self.kind());
        match self {
            Self::Open { tab, page } => {
                w.u8(*tab).u16(*page);
            }
            Self::Put {
                req,
                unit,
                to,
                gold,
                cell,
            } => {
                w.u64(*req).u32(*unit).u8(*to).u64(*gold);
                w.opt(cell.as_ref(), |w, c| c.encode(w));
            }
            Self::PutStacks { req, units } => {
                w.u64(*req).u16(units.len().min(MAX_UNITS) as u16);
                for u in units.iter().take(MAX_UNITS) {
                    w.u32(*u);
                }
            }
            Self::Take {
                req,
                from,
                item,
                stack,
                gold,
                place,
            } => {
                w.u64(*req).u8(*from);
                w.opt(item.as_ref(), |w, i| {
                    w.u32(*i);
                });
                w.opt(stack.as_ref(), |w, (code, count)| {
                    w.str8(code).u32(*count);
                });
                w.u64(*gold).u8(*place);
            }
            Self::Move { req, item, cell } => {
                w.u64(*req).u32(*item);
                cell.encode(&mut w);
            }
            Self::Arrange { req, page } => {
                w.u64(*req).u8(*page);
            }
            Self::Search { text } => {
                w.str8(text);
            }
            Self::Rename { req, page, name } => {
                w.u64(*req).u8(*page).str8(name);
            }
            Self::Layout => {}
        }
        w.finish()
    }

    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut r) = Reader::new(body)?;
        let m = match k {
            kind::STASH_OPEN => Self::Open {
                tab: r.u8()?,
                page: r.u16()?,
            },
            kind::STASH_PUT => Self::Put {
                req: r.u64()?,
                unit: r.u32()?,
                to: r.u8()?,
                gold: r.u64()?,
                cell: r.opt(StashCell::decode)?,
            },
            kind::STASH_PUT_STACKS => {
                let req = r.u64()?;
                let n = r.u16()? as usize;
                if n > MAX_UNITS {
                    return Err(NetError::Malformed);
                }
                let units = (0..n).map(|_| r.u32()).collect::<Result<_, _>>()?;
                Self::PutStacks { req, units }
            }
            kind::STASH_TAKE => Self::Take {
                req: r.u64()?,
                from: r.u8()?,
                item: r.opt(|r| r.u32())?,
                stack: r.opt(|r| Ok((r.str8(MAX_CODE)?, r.u32()?)))?,
                gold: r.u64()?,
                place: r.u8()?,
            },
            kind::STASH_MOVE => Self::Move {
                req: r.u64()?,
                item: r.u32()?,
                cell: StashCell::decode(&mut r)?,
            },
            kind::STASH_ARRANGE => Self::Arrange {
                req: r.u64()?,
                page: r.u8()?,
            },
            kind::STASH_SEARCH => Self::Search {
                text: r.str8(MAX_SEARCH)?,
            },
            kind::STASH_RENAME => Self::Rename {
                req: r.u64()?,
                page: r.u8()?,
                name: r.str8(MAX_PAGE_NAME)?,
            },
            kind::STASH_LAYOUT => Self::Layout,
            _ => return Err(NetError::Malformed),
        };
        r.finish()?;
        Ok(m)
    }
}

/// S→C stash messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StashEvent {
    /// 0xA1: the answer to `StashOpen`: the gold, the stackables with a
    /// count, and how many [`StashEvent::Item`]s of the page follow (0 for
    /// [`tab::STACKS`]).
    Page {
        tab: u8,
        page: u16,
        gold: u64,
        stacks: Vec<(String, u32)>,
        items: u16,
    },
    /// 0xAA: one item of the page just announced.
    Item(StashItem),
    /// 0xA4: a changing request ended: `code` 0 done, else the refusal's
    /// code; `count` what moved (a take's count, or gold); `left` the
    /// units a `StashPutStacks` left on the character, with their codes.
    Done {
        req: u64,
        code: u16,
        count: u64,
        left: Vec<(u32, u16)>,
    },
    /// 0xA9: the search's hits.
    Found { hits: Vec<(u32, StashCell)> },
    /// 0xAD: the answer to `StashLayout`: the item pages' shape, the
    /// highest count of one stackable slot, and the pages' names (page I
    /// first).
    Shape(StashShapeMsg),
}

/// The item pages of a realm's stash ([`StashEvent::Shape`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StashShapeMsg {
    pub pages: u8,
    pub cols: u8,
    pub rows: u8,
    /// The player may name the pages ([`StashRequest::Rename`]).
    pub renamable: bool,
    /// The highest count of one stackable slot.
    pub cap: u32,
    pub names: Vec<String>,
}

impl StashEvent {
    pub fn kind(&self) -> u8 {
        match self {
            Self::Page { .. } => kind::STASH_PAGE,
            Self::Item(_) => kind::STASH_ITEM,
            Self::Done { .. } => kind::STASH_DONE,
            Self::Found { .. } => kind::STASH_FOUND,
            Self::Shape(_) => kind::STASH_SHAPE,
        }
    }

    /// A refusal of request `req` with error code `code`.
    pub fn refused(req: u64, code: u16) -> Self {
        Self::Done {
            req,
            code,
            count: 0,
            left: Vec::new(),
        }
    }

    pub fn body(&self) -> Vec<u8> {
        let mut w = Writer::new(self.kind());
        match self {
            Self::Page {
                tab,
                page,
                gold,
                stacks,
                items,
            } => {
                w.u8(*tab).u16(*page).u64(*gold);
                w.u16(stacks.len().min(MAX_STACKS) as u16);
                for (code, n) in stacks.iter().take(MAX_STACKS) {
                    w.str8(code).u32(*n);
                }
                w.u16(*items);
            }
            Self::Item(i) => {
                w.u32(i.id);
                i.cell.encode(&mut w);
                w.u8(i.size.0).u8(i.size.1).bytes16(&i.record);
            }
            Self::Done {
                req,
                code,
                count,
                left,
            } => {
                w.u64(*req).u16(*code).u64(*count);
                w.u16(left.len().min(MAX_UNITS) as u16);
                for (unit, code) in left.iter().take(MAX_UNITS) {
                    w.u32(*unit).u16(*code);
                }
            }
            Self::Found { hits } => {
                w.u16(hits.len().min(MAX_HITS) as u16);
                for (id, cell) in hits.iter().take(MAX_HITS) {
                    w.u32(*id);
                    cell.encode(&mut w);
                }
            }
            Self::Shape(s) => {
                w.u8(s.pages)
                    .u8(s.cols)
                    .u8(s.rows)
                    .bool(s.renamable)
                    .u32(s.cap);
                w.u8(s.names.len().min(MAX_PAGE_NAMES) as u8);
                for n in s.names.iter().take(MAX_PAGE_NAMES) {
                    w.str8(n);
                }
            }
        }
        w.finish()
    }

    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut r) = Reader::new(body)?;
        let m = match k {
            kind::STASH_PAGE => {
                let tab = r.u8()?;
                let page = r.u16()?;
                let gold = r.u64()?;
                let n = r.u16()? as usize;
                if n > MAX_STACKS {
                    return Err(NetError::Malformed);
                }
                let stacks = (0..n)
                    .map(|_| Ok((r.str8(MAX_CODE)?, r.u32()?)))
                    .collect::<Result<_, NetError>>()?;
                Self::Page {
                    tab,
                    page,
                    gold,
                    stacks,
                    items: r.u16()?,
                }
            }
            kind::STASH_ITEM => Self::Item(StashItem {
                id: r.u32()?,
                cell: StashCell::decode(&mut r)?,
                size: (r.u8()?, r.u8()?),
                record: r.bytes16(MAX_ITEM)?,
            }),
            kind::STASH_DONE => {
                let req = r.u64()?;
                let code = r.u16()?;
                let count = r.u64()?;
                let n = r.u16()? as usize;
                if n > MAX_UNITS {
                    return Err(NetError::Malformed);
                }
                let left = (0..n)
                    .map(|_| Ok((r.u32()?, r.u16()?)))
                    .collect::<Result<_, NetError>>()?;
                Self::Done {
                    req,
                    code,
                    count,
                    left,
                }
            }
            kind::STASH_FOUND => {
                let n = r.u16()? as usize;
                if n > MAX_HITS {
                    return Err(NetError::Malformed);
                }
                let hits = (0..n)
                    .map(|_| Ok((r.u32()?, StashCell::decode(&mut r)?)))
                    .collect::<Result<_, NetError>>()?;
                Self::Found { hits }
            }
            kind::STASH_SHAPE => {
                let (pages, cols, rows) = (r.u8()?, r.u8()?, r.u8()?);
                let renamable = r.bool()?;
                let cap = r.u32()?;
                let n = r.u8()? as usize;
                if n > MAX_PAGE_NAMES {
                    return Err(NetError::Malformed);
                }
                let names = (0..n)
                    .map(|_| r.str8(MAX_PAGE_NAME))
                    .collect::<Result<_, _>>()?;
                Self::Shape(StashShapeMsg {
                    pages,
                    cols,
                    rows,
                    renamable,
                    cap,
                    names,
                })
            }
            _ => return Err(NetError::Malformed),
        };
        r.finish()?;
        Ok(m)
    }
}

#[cfg(test)]
#[path = "stash_tests.rs"]
mod tests;
