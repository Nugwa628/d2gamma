//! Shared-stash transfers on the game thread (contracts 11.13; plan N28):
//! goods between a seated character and its account's stash on the main
//! server.
//!
//! - **Out** ([`crate::GameHandle::item_out`]): the goods leave the
//!   character (the journal's `ItemOut`), the character is saved at once,
//!   and that save carries the goods up ([`Transfer`] on its
//!   [`SaveRecord`](crate::SaveRecord)): the main server stores both in one
//!   transaction.
//! - **In** ([`crate::GameHandle::item_in`]): the goods are placed (the
//!   journal's `ItemIn`); when they were, the character is saved at once
//!   and that save closes the transfer on the main server.
//!
//! A mod game's stash may fold an item into counts (a container into the
//! stackables it holds): a stackable put of such an item
//! ([`d2_sim::rules::StashHooks::credits`]) takes the item out as itself
//! and sends its credits up with it ([`TransferGoods::Out::credits`]); the
//! main server credits them all or none. The journal records them.
//!
//! While a transfer's save waits for its answer, its seat makes no other
//! save: a save made then would carry the character without goods the main
//! server may still refuse, or with goods it may still take back. The sink
//! tells the answer to the game thread ([`Settled`]):
//!
//! - stored: the seat saves again as usual;
//! - refused: the transfer is undone in the game (goods that left come
//!   back, as a refund; placed goods leave again), then the character is
//!   saved;
//! - no answer (the link was lost): the seat saves nothing more. What the
//!   main server holds is whole either way: the record before the
//!   transfer, or the transfer with its save.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use d2_journal::format::{Goods, Record};
use d2_journal::replay::{TransferIn, TransferOut};
use d2_journal::server::{bring_in_item, save_after_transfer, take_out_item};
use d2_net::msg::stash::goods;
use d2_net::msg::transfer::{placed, ItemSketch, MAX_CREDITS};
use d2_net::msg::ErrorCode;
use d2_server::game::Sim;
use d2_server::mod_items::with_items;
use d2_server::seams::ClientId;
use d2_server::stash_items::{self, PlaceTo};
use d2_sim::rules::stash::{credits as folded, Credit};
use d2_sim::rules::Ruleset;
use d2_sim::units::UnitId;

use crate::journal::Recorder;

/// The refusals a game gives a transfer (the economy's codes, contracts
/// 11.13).
pub mod code {
    use d2_net::msg::ErrorCode;

    /// The character isn't in this game (or hasn't joined it yet).
    pub const NOT_IN_GAME: ErrorCode = ErrorCode::NOT_IN_GAME;
    /// The character doesn't hold that item, or that much gold.
    pub const ITEM_NOT_FOUND: ErrorCode = ErrorCode(82);
    /// Another transfer of the character is under way.
    pub const ITEM_HELD: ErrorCode = ErrorCode(87);
    /// Goods the game doesn't move that way (a stackable put of an item
    /// that isn't a plain item, a stackable with item bytes).
    pub const BAD_REQUEST: ErrorCode = ErrorCode(99);
}

/// The transfer a save goes with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transfer {
    /// The main server's request number.
    pub req: u64,
    pub goods: TransferGoods,
    /// Where the sink tells a refusal.
    pub answer: TransferAnswer,
}

/// What a transfer's save carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferGoods {
    /// Goods that left the character (`ItemOut`).
    Out {
        /// [`goods`].
        to: u8,
        /// The item's unit id (0 for gold).
        unit: u32,
        /// The item's realm item ID.
        item_id: Option<u32>,
        /// The item's code and count when it is a plain item (a rune, a
        /// gem, keys): the main server keeps its stackables as counts.
        stack: Option<(String, u32)>,
        gold: u64,
        sketch: Option<ItemSketch>,
        /// The item's save entry; empty for gold.
        item: Vec<u8>,
        /// A mod game's item that becomes counts (a container folded into
        /// what it holds): the `(code, count)` credits, all or none; the
        /// item bytes go with them as the record of what was folded. Empty
        /// for every other put.
        credits: Vec<(String, u32)>,
    },
    /// Goods from the stash were placed (`ItemInDone`).
    In,
}

/// Where a save sink tells that the main server refused a transfer, with
/// its error code (the sink's `Err` alone can't say whether the main
/// server answered).
#[derive(Clone, Debug, Default)]
pub struct TransferAnswer(Arc<Mutex<Option<ErrorCode>>>);

impl TransferAnswer {
    /// The main server refused the transfer: nothing of it was stored.
    pub fn refuse(&self, code: ErrorCode) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(code);
    }

    /// The refusal, if the main server refused.
    pub fn refused(&self) -> Option<ErrorCode> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl PartialEq for TransferAnswer {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for TransferAnswer {}

/// A seat's transfer whose save waits for its answer: what undoing it
/// takes.
#[derive(Clone, Debug)]
pub(crate) struct Pending {
    pub req: u64,
    pub undo: Undo,
}

#[derive(Clone, Debug)]
pub(crate) enum Undo {
    /// Goods that left: the item's entry (from the cursor or not) and the
    /// gold.
    Out {
        item: Option<Vec<u8>>,
        cursor: bool,
        gold: u64,
    },
    /// Goods placed: the items' unit ids (one item, or a stackable's
    /// fresh items) and the gold.
    In { units: Vec<u32>, gold: u64 },
}

/// How a transfer's save went, from the sink to the game thread.
#[derive(Clone, Debug)]
pub(crate) struct Settled {
    pub client: ClientId,
    pub req: u64,
    pub outcome: Outcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Stored,
    Refused(ErrorCode),
    /// No answer: why.
    Unanswered(String),
}

impl Outcome {
    /// The outcome of a store that returned `stored`, with `answer`.
    pub fn of(stored: &Result<(), String>, answer: &TransferAnswer) -> Self {
        match (stored, answer.refused()) {
            (Ok(()), _) => Outcome::Stored,
            (Err(_), Some(code)) => Outcome::Refused(code),
            (Err(e), None) => Outcome::Unanswered(e.clone()),
        }
    }
}

/// Where a transfer's save is tagged: the game thread sets it just before
/// it saves the client, the character writer takes it.
pub(crate) type Tag = std::rc::Rc<std::cell::RefCell<Option<(ClientId, Transfer)>>>;

/// The realm item ID a hostile test build gives the items it invents
/// ([`crate::GamesConfig::invent_stash_items`]): above every block the main
/// server grants.
pub const INVENTED_ID: u32 = 0x7FFF_FFF0;

/// What a main server's `ItemWanted` asks of a character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutAsk {
    pub req: u64,
    /// The item's unit id (0 for gold).
    pub unit: u32,
    /// [`goods`].
    pub to: u8,
    pub gold: u64,
    /// [`crate::GamesConfig::invent_stash_items`].
    pub invent: bool,
}

/// Takes goods out of `client`'s character for the stash and saves it with
/// them (see the module): an item ([`goods::ITEM`]), a plain item the
/// stash keeps as a count ([`goods::STACK`]: a rune, a gem, keys; any other
/// item is refused before it moves) or gold. `Err`: nothing changed.
pub(crate) fn out(
    sim: &mut Sim,
    client: ClientId,
    journal: Option<&Recorder>,
    tag: &Tag,
    ask: OutAsk,
) -> Result<Pending, ErrorCode> {
    let OutAsk {
        req,
        unit,
        to,
        gold,
        invent,
    } = ask;
    let p = sim.player_of(client).ok_or(code::NOT_IN_GAME)?;
    // A mod game's stash may fold the item into counts: it goes up as
    // itself, with the credits.
    let credits = if to == goods::STACK && unit != 0 && gold == 0 {
        fold(sim, p, unit)?
    } else {
        Vec::new()
    };
    let held = match to {
        goods::ITEM | goods::STACK if unit != 0 && gold == 0 => {
            let h = stash_items::held_item(sim, p, unit).ok_or(code::ITEM_NOT_FOUND)?;
            if to == goods::STACK && h.plain_count.is_none() && credits.is_empty() {
                return Err(code::BAD_REQUEST);
            }
            Some(h)
        }
        goods::GOLD if unit == 0 && gold > 0 => {
            if gold > stash_items::gold(sim, p) {
                return Err(code::ITEM_NOT_FOUND);
            }
            None
        }
        _ => return Err(code::BAD_REQUEST),
    };
    let unit = if held.is_some() { unit } else { 0 };
    // Folded items go up as credits; no count of their own.
    let plain = held
        .as_ref()
        .filter(|_| credits.is_empty())
        .and_then(|h| h.plain_count.map(|n| (h.code.clone(), n)));
    // The journal names a stackable put's stack: a re-run checks the item
    // is that plain stack.
    let journal_stack = plain.clone().filter(|_| to == goods::STACK);
    let t = TransferOut {
        req,
        unit,
        item_id: None,
        stack: journal_stack.as_ref().map(|(c, n)| (c.as_str(), *n)),
        gold,
    };
    let lifted = take_out_item(sim, client, &t).map_err(|_| code::ITEM_NOT_FOUND)?;
    let item_id = lifted.as_ref().and_then(|l| l.id);
    if let Some(r) = journal {
        r.record(Record::ItemOut {
            client: client as u16,
            req,
            unit,
            item_id,
            stack: journal_stack,
            gold,
        });
        if !credits.is_empty() {
            r.record(Record::ItemCredits {
                client: client as u16,
                req,
                credits: credits.clone(),
            });
        }
    }
    let sketch = lifted.as_ref().map(|l| {
        let h = &l.held;
        ItemSketch {
            code: h.code.clone(),
            item_type: h.item_type.clone(),
            quality: h.quality,
            level_req: h.level_req,
            ilvl: h.ilvl,
            sockets: h.sockets,
            ethereal: h.ethereal,
            class: h.class,
            size: h.size,
        }
    });
    let item = lifted.as_ref().map(|l| l.bytes.clone());
    // A hostile test build sends an invented item up (the same item with
    // an ID never granted); the undo still has the character's own.
    let (item_id, sent) = match (&item, item_id) {
        (Some(bytes), Some(_)) if invent => {
            match stash_items::with_realm_id(sim, bytes, INVENTED_ID) {
                Ok(b) => (Some(INVENTED_ID), Some(b)),
                Err(_) => (item_id, item.clone()),
            }
        }
        _ => (item_id, item.clone()),
    };
    let pending = Pending {
        req,
        undo: Undo::Out {
            item: item.clone(),
            cursor: held.is_some_and(|h| h.cursor),
            gold,
        },
    };
    *tag.borrow_mut() = Some((
        client,
        Transfer {
            req,
            goods: TransferGoods::Out {
                to,
                unit,
                item_id,
                stack: plain,
                gold,
                sketch,
                item: sent.unwrap_or_default(),
                credits,
            },
            answer: TransferAnswer::default(),
        },
    ));
    Ok(pending)
}

/// What the mod's stash says item `unit` of `player` is worth in counts
/// (see the module); empty without a mod stash or when it folds nothing.
/// `Err`: the credits can't go up as one put.
fn fold(sim: &mut Sim, player: UnitId, unit: u32) -> Result<Vec<Credit>, ErrorCode> {
    let Ruleset::Mod(rules) = &sim.rules else {
        return Ok(Vec::new());
    };
    let rules = rules.clone();
    let Some(hooks) = rules.stash() else {
        return Ok(Vec::new());
    };
    let credits = with_items(sim, |items| folded(items, hooks, player, unit)).unwrap_or_default();
    if credits.len() > MAX_CREDITS {
        return Err(code::BAD_REQUEST);
    }
    Ok(credits)
}

/// Goods from the stash for a character: an item's save entry (empty for
/// none), a stackable's code and count (placed as fresh plain items), the
/// gold, and whether the item goes on the cursor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Arriving {
    pub item: Vec<u8>,
    pub stack: Option<(String, u32)>,
    pub gold: u64,
    pub cursor: bool,
}

/// Places goods from the stash on `client`'s character: the result
/// ([`placed`]) and, when placed, what undoing it takes. The caller saves
/// a placed transfer with its tag set.
pub(crate) fn place(
    sim: &mut Sim,
    client: ClientId,
    journal: Option<&Recorder>,
    tag: &Tag,
    req: u64,
    arriving: Arriving,
) -> Result<(u8, Option<Pending>), ErrorCode> {
    sim.player_of(client).ok_or(code::NOT_IN_GAME)?;
    let Arriving {
        item,
        stack,
        gold,
        cursor,
    } = arriving;
    let goods = match stack {
        Some((code, count)) if item.is_empty() => Goods::Stack { code, count },
        Some(_) => return Err(code::BAD_REQUEST),
        None => goods_of(item),
    };
    // Only an item goes on the cursor; a stackable's items fill the
    // inventory.
    let cursor = cursor && matches!(goods, Goods::Item(_));
    let t = TransferIn {
        req,
        goods: &goods,
        gold,
        cursor,
    };
    let (result, units) = bring_in_item(sim, client, &t).map_err(|_| code::BAD_REQUEST)?;
    if let Some(r) = journal {
        r.record(Record::ItemIn {
            client: client as u16,
            req,
            goods,
            gold,
            cursor,
            result,
        });
    }
    if result != placed::PLACED {
        return Ok((result, None));
    }
    *tag.borrow_mut() = Some((
        client,
        Transfer {
            req,
            goods: TransferGoods::In,
            answer: TransferAnswer::default(),
        },
    ));
    Ok((
        result,
        Some(Pending {
            req,
            undo: Undo::In { units, gold },
        }),
    ))
}

/// Saves `client` for its transfer (its tag set). `Err`: no save was made.
pub(crate) fn save(sim: &mut Sim, client: ClientId, tag: &Tag) -> Result<(), String> {
    let r = save_after_transfer(sim, client);
    // A save that didn't reach the writer leaves no tag behind.
    tag.borrow_mut().take();
    r
}

/// Undoes a refused transfer in the game (journalled as the opposite
/// transfer). Returns what was lost, if anything: an item that found no
/// room to come back.
pub(crate) fn undo(
    sim: &mut Sim,
    client: ClientId,
    journal: Option<&Recorder>,
    pending: &Pending,
) -> Option<String> {
    match &pending.undo {
        Undo::Out { item, cursor, gold } => {
            let item_bytes = item.clone().unwrap_or_default();
            let goods = goods_of(item_bytes.clone());
            // Back where it came from: the cursor, when it was there.
            let back_to_cursor = *cursor && item.is_some();
            let t = TransferIn {
                req: pending.req,
                goods: &goods,
                gold: *gold,
                cursor: back_to_cursor,
            };
            let result = bring_in_item(sim, client, &t).map_or(placed::REFUSED, |(r, _)| r);
            if let Some(r) = journal {
                r.record(Record::ItemIn {
                    client: client as u16,
                    req: pending.req,
                    goods,
                    gold: *gold,
                    cursor: back_to_cursor,
                    result,
                });
            }
            if result == placed::PLACED {
                return None;
            }
            if item.is_none() {
                return Some(format!(
                    "{gold} gold of a refused transfer found no room to come back"
                ));
            }
            // No room in the inventory: the cursor it came from, if free.
            let p = sim.player_of(client)?;
            let to = if *cursor {
                PlaceTo::Cursor
            } else {
                PlaceTo::Inventory(None)
            };
            match stash_items::place_item(sim, p, &item_bytes, to)
                .or_else(|_| stash_items::place_item(sim, p, &item_bytes, PlaceTo::Cursor))
            {
                Ok(_) => None,
                Err(e) => Some(format!("a refused item found no room to come back: {e}")),
            }
        }
        Undo::In { units, gold } => {
            // Each placed item leaves again, the gold with the first.
            let mut leave: Vec<(u32, u64)> = units.iter().map(|&u| (u, 0)).collect();
            match leave.first_mut() {
                Some(first) => first.1 = *gold,
                None if *gold > 0 => leave.push((0, *gold)),
                None => {}
            }
            let mut stay = Vec::new();
            for (unit, gold) in leave {
                let t = TransferOut {
                    req: pending.req,
                    unit,
                    item_id: None,
                    stack: None,
                    gold,
                };
                match take_out_item(sim, client, &t) {
                    Ok(_) => {
                        if let Some(r) = journal {
                            r.record(Record::ItemOut {
                                client: client as u16,
                                req: pending.req,
                                unit,
                                item_id: None,
                                stack: None,
                                gold,
                            });
                        }
                    }
                    Err(e) => stay.push(e),
                }
            }
            (!stay.is_empty()).then(|| {
                format!(
                    "placed goods of a refused transfer stay: {}",
                    stay.join("; ")
                )
            })
        }
    }
}

/// The journal's goods for item bytes: none (gold only) is the empty
/// stack.
fn goods_of(item: Vec<u8>) -> Goods {
    if item.is_empty() {
        Goods::Stack {
            code: String::new(),
            count: 0,
        }
    } else {
        Goods::Item(item)
    }
}

/// Sends a transfer's outcome to the game thread.
pub(crate) fn settle(
    tx: &Sender<Settled>,
    client: ClientId,
    req: u64,
    stored: &Result<(), String>,
    answer: &TransferAnswer,
) {
    let _ = tx.send(Settled {
        client,
        req,
        outcome: Outcome::of(stored, answer),
    });
}
