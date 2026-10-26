//! The game-server link's item transfers (kinds 0xD0-0xD5, contracts
//! 11.13; plan N28): goods between a character in a game and its
//! account's shared stash.
//!
//! - **Put:** the main server asks for goods ([`ItemWanted`]); the game
//!   server takes them off the character, saves it at once and sends both
//!   ([`ItemOut`]: the goods inline, the record on its stream, as a
//!   `Save`'s). The main server stores the goods and the record in one
//!   transaction and answers `ItemOutAck`; a refusal gives the goods back
//!   to the character. A game that can't take the goods (the item isn't
//!   held, not that much gold) answers `ItemOutRefused` instead.
//! - **Take:** the main server moves the goods out of the stash into a
//!   transfer in flight and sends them ([`ItemIn`]); the game server
//!   places them, saves at once and answers [`ItemInDone`] with that save
//!   (or that there was no room). The main server closes the transfer
//!   with the save and answers `ItemOutAck`; if the seat ends first, it
//!   puts the goods back in the stash.
//!
//! `req` numbers the transfer: the main server's, rising per character.
//! A repeat (after the link came back) gets the first answer.

use super::node::{ItemIdBlock, SaveInfo, SAVE_REASON_TRANSFER};
use super::stash::{MAX_CODE, MAX_ITEM};
use super::{Reader, Writer};
use crate::NetError;

/// What a transfer carries (`to`, `from`): [`super::stash::goods`].
pub use super::stash::goods;

/// Results of an [`ItemInDone`].
pub mod placed {
    pub const PLACED: u8 = 0;
    pub const NO_ROOM: u8 = 1;
    pub const REFUSED: u8 = 2;
}

/// The highest `ItemIn.from` (5, a refund).
pub const FROM_MAX: u8 = 5;
/// An item type's code.
pub const MAX_TYPE: usize = 8;

/// R→N 0xD0: the goods the player asked to put in the stash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemWanted {
    pub game: u32,
    pub lease: u64,
    pub req: u64,
    /// The item's unit id in the game (0 for gold).
    pub unit: u32,
    /// [`goods`].
    pub to: u8,
    /// A stackable's count (0: the whole stack).
    pub count: u32,
    pub gold: u64,
}

impl ItemWanted {
    pub(super) fn encode(&self, w: &mut Writer) {
        w.u32(self.game)
            .u64(self.lease)
            .u64(self.req)
            .u32(self.unit)
            .u8(self.to)
            .u32(self.count)
            .u64(self.gold);
    }

    pub(super) fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            game: r.u32()?,
            lease: r.u64()?,
            req: r.u64()?,
            unit: r.u32()?,
            to: goods_kind(r)?,
            count: r.u32()?,
            gold: r.u64()?,
        })
    }
}

/// What the game knows of an item that leaves a character: what the
/// stash lists and searches by.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemSketch {
    pub code: String,
    /// The item type's code.
    pub item_type: String,
    /// The save format's quality number.
    pub quality: u8,
    pub level_req: u8,
    pub ilvl: u8,
    pub sockets: u8,
    pub ethereal: bool,
    /// The class the item is restricted to.
    pub class: Option<u8>,
    /// Width and height in cells.
    pub size: (u8, u8),
}

impl ItemSketch {
    fn encode(&self, w: &mut Writer) {
        w.str8(&self.code)
            .str8(&self.item_type)
            .u8(self.quality)
            .u8(self.level_req)
            .u8(self.ilvl)
            .u8(self.sockets)
            .bool(self.ethereal)
            .opt(self.class.as_ref(), |w, c| {
                w.u8(*c);
            })
            .u8(self.size.0)
            .u8(self.size.1);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let s = Self {
            code: r.str8(MAX_CODE)?,
            item_type: r.str8(MAX_TYPE)?,
            quality: r.u8()?,
            level_req: r.u8()?,
            ilvl: r.u8()?,
            sockets: r.u8()?,
            ethereal: r.bool()?,
            class: r.opt(|r| r.u8())?,
            size: (r.u8()?, r.u8()?),
        };
        if s.code.is_empty() || !(1..=4).contains(&s.size.0) || !(1..=4).contains(&s.size.1) {
            return Err(NetError::Malformed);
        }
        Ok(s)
    }
}

/// N→R 0xD1: goods left the character for the stash. `save` is the save
/// that goes with them (reason 5; the record without the goods on its
/// stream, as a `Save`'s).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemOut {
    pub save: SaveInfo,
    pub req: u64,
    /// [`goods`].
    pub to: u8,
    /// The item's unit id in the game (0 for gold).
    pub unit: u32,
    /// The item's realm item ID.
    pub item_id: Option<u32>,
    pub stack: Option<(String, u32)>,
    pub gold: u64,
    /// The item, for [`goods::ITEM`].
    pub sketch: Option<ItemSketch>,
    /// The item's bytes (the save format's item and its socketed items);
    /// empty for gold.
    pub item: Vec<u8>,
    /// A mod game's item that becomes counts (a container folded into the
    /// stackables it holds): every `(code, count)` credited at once, all
    /// or none. Empty for every other put; then nothing of it is sent, so
    /// D2Gamma's `ItemOut` bytes are unchanged.
    pub credits: Vec<(String, u32)>,
}

/// The credits of one `ItemOut`.
pub const MAX_CREDITS: usize = 32;

impl ItemOut {
    pub(super) fn encode(&self, w: &mut Writer) {
        self.save.encode(w);
        w.u64(self.req).u8(self.to).u32(self.unit);
        w.opt(self.item_id.as_ref(), |w, i| {
            w.u32(*i);
        });
        stack(w, self.stack.as_ref());
        w.u64(self.gold);
        w.opt(self.sketch.as_ref(), |w, s| s.encode(w));
        w.bytes16(&self.item);
        if !self.credits.is_empty() {
            w.u8(self.credits.len().min(MAX_CREDITS) as u8);
            for (code, count) in self.credits.iter().take(MAX_CREDITS) {
                w.str8(code).u32(*count);
            }
        }
    }

    pub(super) fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let m = Self {
            save: SaveInfo::decode(r)?,
            req: r.u64()?,
            to: goods_kind(r)?,
            unit: r.u32()?,
            item_id: r.opt(|r| r.u32())?,
            stack: read_stack(r)?,
            gold: r.u64()?,
            sketch: r.opt(ItemSketch::decode)?,
            item: r.bytes16(MAX_ITEM)?,
            credits: read_credits(r)?,
        };
        if m.save.reason != SAVE_REASON_TRANSFER {
            return Err(NetError::Malformed);
        }
        Ok(m)
    }
}

/// R→N 0xD3: goods from the stash for the character of `lease`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemIn {
    pub game: u32,
    pub lease: u64,
    pub req: u64,
    /// 1 stash item, 2 stackables, 3 gold, 4 market delivery, 5 refund.
    pub from: u8,
    pub item_id: Option<u32>,
    pub stack: Option<(String, u32)>,
    pub gold: u64,
    /// IDs for the fresh simple items of a stack.
    pub ids: Option<ItemIdBlock>,
    /// Where an item goes: [`super::stash::place`].
    pub place: u8,
    /// The item's bytes; empty without an item.
    pub item: Vec<u8>,
}

impl ItemIn {
    pub(super) fn encode(&self, w: &mut Writer) {
        w.u32(self.game).u64(self.lease).u64(self.req).u8(self.from);
        w.opt(self.item_id.as_ref(), |w, i| {
            w.u32(*i);
        });
        stack(w, self.stack.as_ref());
        w.u64(self.gold);
        w.opt(self.ids.as_ref(), |w, b| b.encode(w));
        w.u8(self.place).bytes16(&self.item);
    }

    pub(super) fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let m = Self {
            game: r.u32()?,
            lease: r.u64()?,
            req: r.u64()?,
            from: r.u8()?,
            item_id: r.opt(|r| r.u32())?,
            stack: read_stack(r)?,
            gold: r.u64()?,
            ids: r.opt(ItemIdBlock::decode)?,
            place: r.u8()?,
            item: r.bytes16(MAX_ITEM)?,
        };
        if !(1..=FROM_MAX).contains(&m.from) {
            return Err(NetError::Malformed);
        }
        Ok(m)
    }
}

/// N→R 0xD4: how an `ItemIn` went ([`placed`]). Placed: `save` is the
/// save that holds the goods (reason 5; the record on its stream);
/// otherwise there is none and nothing changed on the character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemInDone {
    pub lease: u64,
    pub req: u64,
    pub result: u8,
    pub save: Option<SaveInfo>,
}

impl ItemInDone {
    pub(super) fn encode(&self, w: &mut Writer) {
        w.u64(self.lease).u64(self.req).u8(self.result);
        w.opt(self.save.as_ref(), |w, s| s.encode(w));
    }

    pub(super) fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let m = Self {
            lease: r.u64()?,
            req: r.u64()?,
            result: r.u8()?,
            save: r.opt(SaveInfo::decode)?,
        };
        let placed = m.result == placed::PLACED;
        if m.result > placed::REFUSED
            || placed != m.save.is_some()
            || m.save
                .as_ref()
                .is_some_and(|s| s.reason != SAVE_REASON_TRANSFER || s.lease != m.lease)
        {
            return Err(NetError::Malformed);
        }
        Ok(m)
    }
}

/// `ItemOut`'s trailing credits: none when the message ends here.
fn read_credits(r: &mut Reader) -> Result<Vec<(String, u32)>, NetError> {
    if r.is_empty() {
        return Ok(Vec::new());
    }
    let n = r.u8()? as usize;
    if n == 0 || n > MAX_CREDITS {
        return Err(NetError::Malformed);
    }
    (0..n)
        .map(|_| {
            let code = r.str8(MAX_CODE)?;
            let count = r.u32()?;
            if code.is_empty() || count == 0 {
                return Err(NetError::Malformed);
            }
            Ok((code, count))
        })
        .collect()
}

fn goods_kind(r: &mut Reader) -> Result<u8, NetError> {
    let v = r.u8()?;
    if !(goods::ITEM..=goods::GOLD).contains(&v) {
        return Err(NetError::Malformed);
    }
    Ok(v)
}

fn stack(w: &mut Writer, s: Option<&(String, u32)>) {
    w.opt(s, |w, (code, count)| {
        w.str8(code).u32(*count);
    });
}

fn read_stack(r: &mut Reader) -> Result<Option<(String, u32)>, NetError> {
    r.opt(|r| Ok((r.str8(MAX_CODE)?, r.u32()?)))
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
