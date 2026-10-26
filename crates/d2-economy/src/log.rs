//! The transfer log: one append-only row per move of goods, written in the
//! move's own transaction. Audits and rollbacks follow an item ID through
//! it (plan E2).

use rusqlite::params;

use crate::db::{Act, Economy};
use crate::error::Error;
use crate::key::ItemId;
use crate::stash::Goods;

/// One end of a move: a kind (`character`, `stash`, `in_flight`, ...) and
/// the row or account it names.
pub(crate) type End<'a> = (&'a str, Option<i64>);

/// A move to log.
pub(crate) struct Transfer<'a> {
    pub kind: &'a str,
    pub economy: &'a str,
    pub goods: &'a Goods,
    pub from: End<'a>,
    pub to: End<'a>,
    pub character: Option<&'a str>,
    pub game: Option<u32>,
    pub lease: Option<u64>,
    /// The listing and offer the move belongs to, for the audit tools.
    pub listing: Option<i64>,
    pub offer: Option<i64>,
}

impl Act<'_> {
    pub fn log(&self, t: Transfer<'_>) -> Result<(), Error> {
        let (item, code, count, gold) = match t.goods {
            Goods::Item(id) => (Some(id.0), None, None, None),
            Goods::Stack { code, count } => (None, Some(code.as_str()), Some(*count), None),
            Goods::Gold(g) => (None, None, None, Some(*g as i64)),
        };
        self.exec(
            "INSERT INTO transfers(at, kind, economy, item_id, code, count, gold, from_kind, \
             from_id, to_kind, to_id, character, game, lease, listing, offer, req) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                self.now,
                t.kind,
                t.economy,
                item,
                code,
                count,
                gold,
                t.from.0,
                t.from.1,
                t.to.0,
                t.to.1,
                t.character,
                t.game,
                t.lease.map(|l| l as i64),
                t.listing,
                t.offer,
                self.req,
            ],
        )?;
        Ok(())
    }
}

/// One row of the transfer log, as audits read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferRow {
    pub id: i64,
    pub at: i64,
    pub kind: String,
    pub from_kind: String,
    pub from_id: Option<i64>,
    pub to_kind: String,
    pub to_id: Option<i64>,
    pub character: Option<String>,
}

impl Economy {
    /// Every logged move of an item, oldest first.
    pub fn item_history(&self, id: ItemId) -> Result<Vec<TransferRow>, Error> {
        let mut st = self.conn().prepare(
            "SELECT id, at, kind, from_kind, from_id, to_kind, to_id, character \
             FROM transfers WHERE item_id = ?1 ORDER BY id",
        )?;
        let rows = st.query_map([id.0], |r| {
            Ok(TransferRow {
                id: r.get(0)?,
                at: r.get(1)?,
                kind: r.get(2)?,
                from_kind: r.get(3)?,
                from_id: r.get(4)?,
                to_kind: r.get(5)?,
                to_id: r.get(6)?,
                character: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
