//! The online shared stash: the panel's calls as requests to the main
//! server (contracts 11.13, kinds 0xA0-0xAF on the client's main-server
//! connection). The main server does every move with the character's game
//! server (`ItemWanted` / `ItemOut` / `ItemIn`, recorded in the game
//! journal as stash transfers); the client only asks and shows the
//! answer, so nothing here decides a move.
//!
//! The messages are the contracts' `StashOpen`, `StashPage`, `StashPut`,
//! `StashTake` and `StashDone`, with the item pages' cells and the
//! inventory's "Send stackables to stash" added (`StashPutStacks`,
//! `StashMove`, `StashSearch`; proposed, wired after N4). Every changing
//! request carries a request number rising per account; a repeat gets the
//! first answer.

use std::collections::BTreeMap;

use d2_economy::{ItemId, Refusal};

use crate::catalog::Catalog;
use crate::cells::{Cell, StashShape};
use crate::{PageItem, Put, PutDone, StashStore, StoreError, Take, TakeTo};

/// `StashOpen.tab`.
pub mod tab {
    pub const ITEMS: u8 = 0;
    pub const STACKS: u8 = 1;
}

/// `StashPut.to` and `StashTake.from`: what moves.
pub mod goods {
    pub const ITEM: u8 = 1;
    pub const STACK: u8 = 2;
    pub const GOLD: u8 = 3;
}

/// The main server's codes that are no economy refusal.
pub mod code {
    /// `NotSupported`: the main server doesn't keep these goods yet (the
    /// stackables).
    pub const NOT_SUPPORTED: u16 = 8;
    /// `NotInGame`: a move needs the character in a game.
    pub const NOT_IN_GAME: u16 = 81;
    /// `NoRoom`: no room on the character; the goods stay in the stash.
    pub const NO_ROOM: u16 = 93;
    /// `Other`: the server failed, or the game server didn't answer in
    /// time (the move may still end; the stash shows how).
    pub const OTHER: u16 = 99;
}

/// Client → main server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToRealm {
    /// 0xA0 `StashOpen`: an item page, or the stackables with the gold.
    Open { tab: u8, page: u16 },
    /// 0xA2 `StashPut`: one unit of the character in game (`to`:
    /// [`goods`]), with the item page cell it was dropped on.
    Put {
        req: u64,
        unit: u32,
        to: u8,
        gold: u64,
        cell: Option<Cell>,
    },
    /// 0xA5 `StashPutStacks` (proposed): every listed unit that is a
    /// stackable goes to its slot in one transfer; the rest stay.
    PutStacks { req: u64, units: Vec<u32> },
    /// 0xA3 `StashTake` with `place` (proposed: 0 inventory, 1 cursor).
    Take {
        req: u64,
        from: u8,
        item: Option<ItemId>,
        stack: Option<(String, u32)>,
        gold: u64,
        place: u8,
    },
    /// 0xA6 `StashMove` (proposed): an item to another free cell.
    Move { req: u64, item: ItemId, cell: Cell },
    /// 0xA7 `StashArrange` (proposed): pack one item page.
    Arrange { req: u64, page: u8 },
    /// 0xA8 `StashSearch` (proposed): items whose names contain `text`.
    Search { text: String },
    /// 0xAB `StashRename`: names item page `page` (a mod realm's
    /// renamable pages; empty: back to the default).
    Rename { req: u64, page: u8, name: String },
    /// 0xAC `StashLayout`: the item pages' shape and names.
    Layout,
}

/// An item of an item page as the main server sends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RealmItem {
    pub id: ItemId,
    pub cell: Cell,
    pub size: (u8, u8),
    /// The item's bytes.
    pub record: Vec<u8>,
}

/// Main server → client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FromRealm {
    /// 0xA1 `StashPage`.
    Page {
        tab: u8,
        page: u16,
        items: Vec<RealmItem>,
        stacks: Vec<(String, u32)>,
        gold: u64,
    },
    /// 0xA4 `StashDone`: `code` 0 done, else the refusal's error code;
    /// `count` what moved (a take's count or gold); `left` the units a
    /// `StashPutStacks` left on the character, with their codes.
    Done {
        req: u64,
        code: u16,
        count: u64,
        left: Vec<(u32, u16)>,
    },
    /// 0xA9 `StashFound` (proposed): the search's hits.
    Found { hits: Vec<(ItemId, Cell)> },
    /// 0xAD `StashShape`: the item pages' shape, the highest count of one
    /// slot and the pages' names, page I first.
    Shape {
        shape: StashShape,
        cap: u32,
        names: Vec<String>,
    },
}

/// The client's main-server connection, for the stash's messages: sends
/// one request and returns its answer.
pub trait RealmStash {
    fn call(&mut self, msg: ToRealm) -> Result<FromRealm, StoreError>;
}

/// The online shared stash.
pub struct OnlineStash<L: RealmStash> {
    link: L,
    catalog: Catalog,
    next_req: u64,
    /// The realm's item pages ([`OnlineStash::load_layout`]; D2Gamma's
    /// until then).
    shape: StashShape,
}

impl<L: RealmStash> OnlineStash<L> {
    /// `first_req`: where this session's request numbers start; it must be
    /// above every number the account used before (the client takes its
    /// login time in milliseconds times 1024).
    pub fn new(link: L, catalog: Catalog, first_req: u64) -> Self {
        OnlineStash {
            link,
            catalog,
            next_req: first_req,
            shape: StashShape::DEFAULT,
        }
    }

    /// Asks the realm for its item pages' shape and names, and keeps the
    /// shape ([`StashStore::shape`]). A realm that doesn't know the
    /// request (D2Gamma's before mod realms) leaves D2Gamma's shape.
    pub fn load_layout(&mut self) -> Result<Vec<String>, StoreError> {
        match self.link.call(ToRealm::Layout)? {
            FromRealm::Shape { shape, names, .. } => {
                self.shape = shape;
                Ok(names)
            }
            other => Err(StoreError::Link(format!("unexpected answer {other:?}"))),
        }
    }

    pub fn link(&mut self) -> &mut L {
        &mut self.link
    }

    /// The slots with a count and the gold, in one request
    /// ([`StashStore::stacks`] and [`StashStore::gold`] each ask).
    pub fn stacks_and_gold(&mut self) -> Result<(BTreeMap<String, u32>, u64), StoreError> {
        let FromRealm::Page { stacks, gold, .. } = self.open(tab::STACKS, 0)? else {
            unreachable!("open answers a page")
        };
        Ok((stacks.into_iter().filter(|(_, n)| *n > 0).collect(), gold))
    }

    fn req(&mut self) -> u64 {
        let r = self.next_req;
        self.next_req += 1;
        r
    }

    /// Sends a changing request and reads its `StashDone`.
    fn act(&mut self, msg: ToRealm) -> Result<(u64, Vec<(u32, u16)>), StoreError> {
        match self.link.call(msg)? {
            FromRealm::Done {
                code: 0,
                count,
                left,
                ..
            } => Ok((count, left)),
            FromRealm::Done { code, .. } => Err(refusal(code)),
            other => Err(StoreError::Link(format!("unexpected answer {other:?}"))),
        }
    }

    fn open(&mut self, tab: u8, page: u16) -> Result<FromRealm, StoreError> {
        match self.link.call(ToRealm::Open { tab, page })? {
            p @ FromRealm::Page { .. } => Ok(p),
            other => Err(StoreError::Link(format!("unexpected answer {other:?}"))),
        }
    }
}

/// A refused move's `StashDone` code. The economy's refusals read as
/// theirs; the main server's own codes come first (8 and 99 are no
/// market or request refusal there).
fn refusal(c: u16) -> StoreError {
    match c {
        code::NO_ROOM => StoreError::NoRoom,
        code::NOT_IN_GAME => StoreError::Game("the character is in no game".into()),
        code::NOT_SUPPORTED => StoreError::Link("not kept online yet".into()),
        code::OTHER => StoreError::Link("the move failed; the stash shows what moved".into()),
        c => match Refusal::from_code(c) {
            Some(r) => StoreError::Refused(r),
            None => StoreError::Link(format!("error code {c}")),
        },
    }
}

impl<L: RealmStash> StashStore for OnlineStash<L> {
    fn online(&self) -> bool {
        true
    }

    fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    fn page(&mut self, page: u8) -> Result<Vec<PageItem>, StoreError> {
        let FromRealm::Page { items, .. } = self.open(tab::ITEMS, page.into())? else {
            unreachable!("open answers a page")
        };
        Ok(items
            .into_iter()
            .map(|i| PageItem {
                id: i.id,
                cell: i.cell,
                size: i.size,
                record: i.record,
                facts: None,
                look: None,
            })
            .collect())
    }

    fn stacks(&mut self) -> Result<BTreeMap<String, u32>, StoreError> {
        let FromRealm::Page { stacks, .. } = self.open(tab::STACKS, 0)? else {
            unreachable!("open answers a page")
        };
        Ok(stacks.into_iter().filter(|(_, n)| *n > 0).collect())
    }

    fn gold(&mut self) -> Result<u64, StoreError> {
        let FromRealm::Page { gold, .. } = self.open(tab::STACKS, 0)? else {
            unreachable!("open answers a page")
        };
        Ok(gold)
    }

    fn search(&mut self, text: &str) -> Result<Vec<(ItemId, Cell)>, StoreError> {
        match self.link.call(ToRealm::Search { text: text.into() })? {
            FromRealm::Found { hits } => Ok(hits),
            other => Err(StoreError::Link(format!("unexpected answer {other:?}"))),
        }
    }

    fn put(&mut self, put: Put) -> Result<PutDone, StoreError> {
        let req = self.req();
        match put {
            Put::Item { unit, at } => {
                self.act(ToRealm::Put {
                    req,
                    unit,
                    to: goods::ITEM,
                    gold: 0,
                    cell: at,
                })?;
                Ok(PutDone {
                    moved: vec![unit],
                    left: Vec::new(),
                })
            }
            Put::Gold(gold) => {
                self.act(ToRealm::Put {
                    req,
                    unit: 0,
                    to: goods::GOLD,
                    gold,
                    cell: None,
                })?;
                Ok(PutDone::default())
            }
            Put::Stacks { units } => {
                let (_, left) = self.act(ToRealm::PutStacks {
                    req,
                    units: units.clone(),
                })?;
                let left: Vec<(u32, Refusal)> = left
                    .into_iter()
                    .map(|(u, c)| (u, Refusal::from_code(c).unwrap_or(Refusal::BadRequest)))
                    .collect();
                Ok(PutDone {
                    moved: units
                        .into_iter()
                        .filter(|u| !left.iter().any(|(l, _)| l == u))
                        .collect(),
                    left,
                })
            }
        }
    }

    fn take(&mut self, take: Take) -> Result<u64, StoreError> {
        let req = self.req();
        let msg = match take {
            Take::Item { id, to } => ToRealm::Take {
                req,
                from: goods::ITEM,
                item: Some(id),
                stack: None,
                gold: 0,
                place: u8::from(to == TakeTo::Cursor),
            },
            Take::Stack { code, count } => ToRealm::Take {
                req,
                from: goods::STACK,
                item: None,
                stack: Some((code, count)),
                gold: 0,
                place: 0,
            },
            Take::Gold(gold) => ToRealm::Take {
                req,
                from: goods::GOLD,
                item: None,
                stack: None,
                gold,
                place: 0,
            },
        };
        Ok(self.act(msg)?.0)
    }

    fn move_item(&mut self, id: ItemId, to: Cell) -> Result<(), StoreError> {
        let req = self.req();
        self.act(ToRealm::Move {
            req,
            item: id,
            cell: to,
        })
        .map(|_| ())
    }

    fn arrange(&mut self, page: u8) -> Result<(), StoreError> {
        let req = self.req();
        self.act(ToRealm::Arrange { req, page }).map(|_| ())
    }

    fn shape(&self) -> StashShape {
        self.shape
    }

    fn page_names(&mut self) -> Result<Vec<String>, StoreError> {
        self.load_layout()
    }

    fn rename_page(&mut self, page: u8, name: &str) -> Result<(), StoreError> {
        let name = name.trim();
        if !self.shape.renamable
            || page >= self.shape.pages
            || name.chars().count() > crate::PAGE_NAME_MAX
        {
            return Err(Refusal::BadRequest.into());
        }
        let req = self.req();
        self.act(ToRealm::Rename {
            req,
            page,
            name: name.to_owned(),
        })
        .map(|_| ())
    }
}
