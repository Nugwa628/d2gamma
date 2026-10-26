//! The Deliveries tab (plan E1): what the market gives an account, each
//! with its source, until it is claimed into the stash or taken to a
//! character.

use rusqlite::params;

use crate::db::{Act, Economy};
use crate::error::{Done, Error, Refusal, RowDone, RowsDone};
use crate::key::{CharRef, ItemId, Owner, Req};
use crate::log::{End, Transfer};
use crate::registry::Place;
use crate::stash::page::load_item;
use crate::stash::{goods_columns, Goods, InFlight, Item};

/// One delivery: one item, one count of a stackable, or gold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivery {
    pub id: i64,
    /// Why it is here: `bought`, `sold` (proceeds), `cancelled`, `expired`,
    /// `returned` (an offer's goods), ...
    pub source: String,
    pub listing: Option<i64>,
    pub offer: Option<i64>,
    pub goods: Goods,
    pub item: Option<Item>,
    pub at: i64,
}

impl Delivery {
    /// `MarketDeliveryRow.source`: 1 bought, 2 sold (proceeds), 3
    /// cancelled, 4 expired, 5 returned (an offer's goods back: declined,
    /// withdrawn, outbid, or what an accepted offer held beyond the
    /// price), 255 other.
    pub fn source_code(&self) -> u8 {
        match self.source.as_str() {
            "bought" => 1,
            "sold" => 2,
            "cancelled" => 3,
            "expired" => 4,
            "returned" | "declined" | "withdrawn" => 5,
            _ => 255,
        }
    }
}

/// Which deliveries a claim takes (`MarketClaim`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Claim {
    /// The oldest [`CLAIM_ROWS`].
    All,
    /// These, 1 to [`CLAIM_ROWS`].
    Ids(Vec<i64>),
}

/// The most deliveries one claim takes.
pub const CLAIM_ROWS: usize = 50;

pub(crate) fn goods_of(
    item: Option<u32>,
    code: Option<String>,
    count: Option<u32>,
    gold: Option<i64>,
) -> Goods {
    match (item, code, count) {
        (Some(id), _, _) => Goods::Item(ItemId(id)),
        (None, Some(code), Some(count)) => Goods::Stack { code, count },
        _ => Goods::Gold(gold.unwrap_or(0) as u64),
    }
}

/// Where a delivery comes from, for its row and the transfer log.
/// A delivery taken out of the table: its goods, source, listing and offer.
struct TakenDelivery {
    goods: Goods,
    source: String,
    listing: Option<i64>,
    offer: Option<i64>,
}

pub(crate) struct Source<'a> {
    /// The delivery's source and the log row's kind.
    pub kind: &'a str,
    pub listing: Option<i64>,
    pub offer: Option<i64>,
    pub from: End<'a>,
}

impl Act<'_> {
    /// Puts goods into an account's Deliveries. An item moves there from
    /// wherever it is in the economy; counts and gold must already have
    /// left their source. Deliveries have no limit.
    pub fn deliver(
        &self,
        account: i64,
        split: &str,
        goods: &Goods,
        src: &Source<'_>,
    ) -> Result<i64, Error> {
        self.deliver_as(None, account, split, goods, src, true)
    }

    /// [`Act::deliver`] under a given ID (a delivery coming back from a
    /// transfer that didn't finish).
    fn deliver_as(
        &self,
        id: Option<i64>,
        account: i64,
        split: &str,
        goods: &Goods,
        src: &Source<'_>,
        log: bool,
    ) -> Result<i64, Error> {
        let (item, code, count, gold) = goods_columns(goods);
        self.exec(
            "INSERT INTO deliveries(id, account, economy, source, listing, offer, item_id, code, \
             count, gold, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id,
                account,
                split,
                src.kind,
                src.listing,
                src.offer,
                item,
                code,
                count,
                gold,
                self.now
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        if let Goods::Item(item) = goods {
            self.exec(
                "UPDATE econ_items SET place = 'delivery', place_id = ?2, account = ?3 \
                 WHERE item_id = ?1",
                params![item.0, id, account],
            )?;
            self.moved(*item, Place::Delivery, Some(account), None, src.kind)?;
        }
        if !log {
            return Ok(id);
        }
        self.log(Transfer {
            kind: src.kind,
            economy: split,
            goods,
            from: src.from,
            to: ("delivery", Some(id)),
            character: None,
            game: None,
            lease: None,
            listing: src.listing,
            offer: src.offer,
        })?;
        Ok(id)
    }

    /// An account's delivery, removed from the tab: its goods (an item
    /// stays where it is until the caller moves it) and source.
    fn take_out_delivery(&self, owner: &Owner, id: i64) -> Result<TakenDelivery, Error> {
        let row = self.conn.query_row(
            "SELECT account, economy, item_id, code, count, gold, source, listing, offer \
             FROM deliveries WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    TakenDelivery {
                        goods: goods_of(r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?),
                        source: r.get(6)?,
                        listing: r.get(7)?,
                        offer: r.get(8)?,
                    },
                ))
            },
        );
        let (account, split, taken) = match row {
            Ok(row) => row,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Err(Refusal::ItemNotFound.into()),
            Err(e) => return Err(e.into()),
        };
        if account != owner.account.0 {
            return Err(Refusal::ItemNotFound.into());
        }
        if split != owner.economy.split() {
            return Err(Refusal::EconomyMismatch.into());
        }
        self.exec("DELETE FROM deliveries WHERE id = ?1", [id])?;
        Ok(taken)
    }

    /// A take from a delivery that didn't finish: the delivery comes back
    /// under its ID (the caller logs the undo).
    pub fn undo_delivery_flight(&self, flight: &InFlight, split: &str) -> Result<(), Error> {
        let (from_id, source): (Option<i64>, Option<String>) = self.conn.query_row(
            "SELECT from_id, from_source FROM in_flight WHERE id = ?1",
            [flight.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let source = source.unwrap_or_else(|| "returned".into());
        self.deliver_as(
            from_id,
            flight.account,
            split,
            &flight.goods,
            &Source {
                kind: &source,
                listing: None,
                offer: None,
                from: ("in_flight", Some(flight.id)),
            },
            false,
        )?;
        Ok(())
    }
}

impl Act<'_> {
    /// One delivery into the account's stash, within the stash's limits.
    fn claim_one(&self, owner: &Owner, delivery: i64) -> Result<(), Error> {
        let split = owner.economy.split();
        let TakenDelivery {
            goods,
            listing,
            offer,
            ..
        } = self.take_out_delivery(owner, delivery)?;
        self.give_to_stash(owner.account.0, &split, &goods, true, "claim")?;
        self.log(Transfer {
            kind: "claim",
            economy: &split,
            goods: &goods,
            from: ("delivery", Some(delivery)),
            to: ("stash", Some(owner.account.0)),
            character: None,
            game: None,
            lease: None,
            listing,
            offer,
        })
    }

    /// Takes a delivery out of the tab for a relist: its goods (an item
    /// stays where it is until the caller moves it).
    pub fn take_delivery_goods(&self, owner: &Owner, id: i64) -> Result<Goods, Error> {
        self.take_out_delivery(owner, id).map(|t| t.goods)
    }
}

impl Economy {
    /// An account's Deliveries tab, newest first.
    pub fn deliveries(&self, owner: &Owner) -> Result<Vec<Delivery>, Error> {
        self.check_key(&owner.economy)?;
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT id, source, listing, offer, item_id, code, count, gold, at FROM deliveries \
             WHERE account = ?1 AND economy = ?2 ORDER BY id DESC",
        )?;
        let rows = st.query_map(params![owner.account.0, owner.economy.split()], |r| {
            Ok(Delivery {
                id: r.get(0)?,
                source: r.get(1)?,
                listing: r.get(2)?,
                offer: r.get(3)?,
                goods: goods_of(r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?),
                item: None,
                at: r.get(8)?,
            })
        })?;
        let mut out: Vec<Delivery> = rows.collect::<Result<_, _>>()?;
        for d in &mut out {
            if let Goods::Item(id) = d.goods {
                d.item = load_item(conn, id)?;
            }
        }
        Ok(out)
    }

    /// Delivery → stash, within the stash's limits.
    pub fn claim(
        &mut self,
        owner: &Owner,
        req: &Req,
        delivery: i64,
        now: i64,
    ) -> Result<Done, Error> {
        self.act(req, now, |a| {
            a.check_key(&owner.economy)?;
            a.claim_one(owner, delivery)?;
            Ok(None)
        })
    }

    /// Deliveries → stash, several in one transaction (`MarketClaim`):
    /// each row is claimed or refused on its own (a full stash refuses the
    /// rows that don't fit, [`Refusal::StashFull`]), and the rows are
    /// stored with the answer.
    pub fn claim_many(
        &mut self,
        owner: &Owner,
        req: &Req,
        which: &Claim,
        now: i64,
    ) -> Result<RowsDone, Error> {
        self.act_rows(req, now, |a| {
            a.check_key(&owner.economy)?;
            let ids: Vec<i64> = match which {
                Claim::All => {
                    let mut st = a.conn.prepare(
                        "SELECT id FROM deliveries WHERE account = ?1 AND economy = ?2 \
                         ORDER BY id LIMIT ?3",
                    )?;
                    let rows = st.query_map(
                        params![owner.account.0, owner.economy.split(), CLAIM_ROWS as i64],
                        |r| r.get(0),
                    )?;
                    rows.collect::<Result<_, _>>()?
                }
                Claim::Ids(ids) => {
                    let mut seen = ids.clone();
                    seen.sort_unstable();
                    seen.dedup();
                    if ids.is_empty() || ids.len() > CLAIM_ROWS || seen.len() != ids.len() {
                        return Err(Refusal::BadRequest.into());
                    }
                    ids.clone()
                }
            };
            let mut rows = Vec::with_capacity(ids.len());
            for id in ids {
                let outcome = a.row(|| a.claim_one(owner, id).map(|()| None))?;
                rows.push(RowDone { id, outcome });
            }
            Ok((None, rows))
        })
        .map(|(done, rows)| RowsDone {
            repeated: done.repeated,
            rows,
        })
    }

    /// Delivery → character, step 1: like [`Economy::take`] from the
    /// stash; [`Economy::take_done`] finishes it, and an unfinished one
    /// comes back to the Deliveries tab.
    pub fn take_delivery(
        &mut self,
        owner: &Owner,
        req: &Req,
        to: &CharRef,
        delivery: i64,
        now: i64,
    ) -> Result<Done, Error> {
        self.act(req, now, |a| {
            a.check_key(&owner.economy)?;
            let split = owner.economy.split();
            let TakenDelivery {
                goods,
                source,
                listing,
                offer,
            } = a.take_out_delivery(owner, delivery)?;
            let transfer = a.open_flight(
                owner.account.0,
                &split,
                to,
                ("delivery", Some(delivery)),
                &goods,
            )?;
            a.exec(
                "UPDATE in_flight SET from_source = ?2 WHERE id = ?1",
                params![transfer, source],
            )?;
            a.log(Transfer {
                kind: "take",
                economy: &split,
                goods: &goods,
                from: ("delivery", Some(delivery)),
                to: ("in_flight", Some(transfer)),
                character: Some(&to.name),
                game: Some(to.game),
                lease: Some(to.lease),
                listing,
                offer,
            })?;
            Ok(Some(transfer))
        })
    }
}
