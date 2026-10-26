//! My shop (market contract 3.3, example 8 of the plan): the seller's
//! edits on their listings (cancel, relist, hide, show, note, reprice),
//! one at a time or on up to 50 listings in one transaction with an
//! answer per row; and the pages of the market window: the shop's tabs,
//! the offers both ways and the Deliveries tab.
//!
//! - **Hide / show:** a hidden listing keeps its goods in escrow and its
//!   expiry, leaves the search index and can't be bought or offered on
//!   (others get [`Refusal::ListingGone`]); its live offers stay.
//! - **Relist:** an active listing's expiry becomes now + days; a
//!   cancelled or expired one becomes a new listing (same goods, mode,
//!   price and note) if its goods are still in the seller's Deliveries
//!   (taken from there) or stash, else [`Refusal::ItemNotFound`]. A relist
//!   takes the listing fee.
//! - **Reprice:** a new mode and price; live offers stay, and a buyer who
//!   saw the old price gets [`Refusal::PriceChanged`].

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, OptionalExtension};

use crate::db::{Act, Economy};
use crate::error::{Done, Error, Refusal, RowDone, RowsDone};
use crate::key::{Owner, Req};
use crate::market::deliveries::{goods_of, Delivery};
use crate::market::listings::{check_note, NewListing, DAY_MS};
use crate::market::offers::{offer_of, Offer, OFFER_COLUMNS};
use crate::market::price::Price;
use crate::market::{
    fill_item, goods_name, listing_of, Listing, ListingMode, ListingState, LISTING_COLUMNS,
};
use crate::stash::page::{load_item, PAGE};
use crate::stash::{Goods, Item};

/// The most listings one shop edit names.
pub const EDIT_ROWS: usize = 50;
/// How long sold, cancelled and expired listings stay in the shop.
pub const SHOP_KEEP_MS: i64 = 30 * DAY_MS;

/// One shop edit (`MarketShopEdit.op`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShopOp {
    /// The goods go to the seller's Deliveries; live offers back to their
    /// buyers.
    Cancel,
    /// Extend an active listing to now + `days`, or post a closed one's
    /// goods again for `days`.
    Relist {
        days: u8,
    },
    Hide,
    Show,
    /// Replace the note.
    Note(String),
    /// A new mode and price (none for offers only).
    Reprice {
        mode: ListingMode,
        price: Option<Price>,
    },
}

/// My shop's tabs (`MarketShopOpen.tab`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShopTab {
    /// Active, in the search.
    Shown = 0,
    /// Active, out of the search.
    Hidden = 1,
    /// Sold in the last 30 days.
    Sold = 2,
    /// Expired or cancelled in the last 30 days.
    Expired = 3,
}

impl ShopTab {
    pub const ALL: [ShopTab; 4] = [
        ShopTab::Shown,
        ShopTab::Hidden,
        ShopTab::Sold,
        ShopTab::Expired,
    ];

    pub fn from_u8(t: u8) -> Option<ShopTab> {
        ShopTab::ALL.get(usize::from(t)).copied()
    }

    /// Its rows, over `listings l` (`?1` account, `?2` split, `?3` the
    /// oldest close kept).
    fn sql(self) -> &'static str {
        match self {
            ShopTab::Shown => "l.state = 'active' AND l.hidden = 0",
            ShopTab::Hidden => "l.state = 'active' AND l.hidden = 1",
            ShopTab::Sold => "l.state = 'sold' AND l.closed > ?3",
            ShopTab::Expired => "l.state IN ('expired', 'cancelled') AND l.closed > ?3",
        }
    }
}

/// What a shop page is sorted by (`MarketShopOpen.sort`); ties by listing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShopSort {
    /// Date posted.
    #[default]
    Posted,
    /// When it ends (or ended).
    Ends,
    /// The gold of its price; listings without gold last.
    Gold,
    /// The goods' name, any case.
    Name,
}

/// A sale, on a Sold row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sold {
    /// The buyer's character name (empty when not given).
    pub buyer: String,
    /// The price, or the accepted offer or counter.
    pub paid: Price,
    pub sale_fee: u64,
    pub closed: i64,
}

/// One row of My shop (`ShopRow`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShopRow {
    pub listing: Listing,
    /// When the newest offer on it came, if any.
    pub newest_offer: Option<i64>,
    pub sold: Option<Sold>,
    /// Cancelled or expired, and its goods are still in the seller's
    /// Deliveries or stash.
    pub relistable: bool,
}

/// A page of My shop (`MarketShopBegin` and its rows).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShopPage {
    pub tab: ShopTab,
    pub page: u16,
    pub rows: Vec<ShopRow>,
    /// Rows of each tab, in [`ShopTab`] order.
    pub counts: [u32; 4],
    /// Active listings, shown and hidden.
    pub slots_used: u32,
    pub slots_max: u32,
}

/// Which offers a page lists (`MarketOffersOpen.which`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffersWhich {
    /// Offers on the account's listings.
    OnMine = 0,
    /// Offers the account made.
    MadeByMe = 1,
}

/// One offer with what it is for (`OfferRow`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfferRow {
    pub offer: Offer,
    /// The listing it is on, with its goods (the seller's name is its
    /// `seller`).
    pub listing: Listing,
    /// The offered items' bytes and facts, as far as the economy still
    /// holds them.
    pub items: Vec<Item>,
}

/// A page of offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OffersPage {
    pub total: u32,
    pub page: u16,
    pub rows: Vec<OfferRow>,
}

/// One row of the Deliveries tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryRow {
    pub delivery: Delivery,
    /// The name of the listing's goods it came from ("Grim Crown"), else
    /// of its own goods.
    pub about: String,
    /// Why goods came back, for a refund line.
    pub reason: Reason,
}

/// Why a delivery's goods came back (`MarketDeliveryRow.reason`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reason {
    /// Not a refund: bought goods or a sale's proceeds.
    #[default]
    None = 0,
    /// The seller declined the offer.
    Declined = 1,
    /// The buyer took the offer back.
    Withdrawn = 2,
    /// The listing was sold to someone else.
    Outbid = 3,
    /// The listing expired.
    Expired = 4,
    /// The listing was cancelled.
    Cancelled = 5,
    /// What an accepted offer or counter held beyond the price.
    Change = 6,
}

impl Reason {
    /// The reason of a delivery from its source, and the states of the
    /// offer and the listing it names.
    pub fn of(source: &str, offer: Option<&str>, listing: Option<&str>) -> Reason {
        match (source, offer, listing) {
            ("cancelled", _, _) => Reason::Cancelled,
            ("expired", _, _) => Reason::Expired,
            ("declined", _, _) | (_, Some("declined"), _) => Reason::Declined,
            ("withdrawn", _, _) | (_, Some("withdrawn"), _) => Reason::Withdrawn,
            ("returned", Some("accepted"), _) => Reason::Change,
            ("returned", _, Some("sold")) => Reason::Outbid,
            ("returned", _, Some("expired")) => Reason::Expired,
            ("returned", _, Some("cancelled")) => Reason::Cancelled,
            _ => Reason::None,
        }
    }
}

/// A page of the Deliveries tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveriesPage {
    pub total: u32,
    pub page: u16,
    pub rows: Vec<DeliveryRow>,
}

impl Economy {
    /// One shop edit on up to [`EDIT_ROWS`] of the asker's listings, in
    /// one transaction: each listing is done or refused on its own (rows
    /// of other accounts: [`Refusal::NotYours`]). The whole request is
    /// refused when the market is off, the fees changed, the edit is bad
    /// (no rows, too many, a listing twice, a bad note, days or price).
    /// A relist row's `outcome` names the new listing when it made one.
    pub fn shop_edit(
        &mut self,
        owner: &Owner,
        req: &Req,
        op: &ShopOp,
        listings: &[i64],
        fee_rev: u32,
        now: i64,
    ) -> Result<RowsDone, Error> {
        self.act_rows(req, now, |a| {
            a.check_market()?;
            a.check_key(&owner.economy)?;
            a.check_fee_rev(fee_rev)?;
            let mut seen = listings.to_vec();
            seen.sort_unstable();
            seen.dedup();
            if listings.is_empty() || listings.len() > EDIT_ROWS || seen.len() != listings.len() {
                return Err(Refusal::BadRequest.into());
            }
            let op = a.checked_op(op)?;
            let mut rows = Vec::with_capacity(listings.len());
            for &id in listings {
                let outcome = a.row(|| a.shop_row(owner, &op, id))?;
                rows.push(RowDone { id, outcome });
            }
            Ok((None, rows))
        })
        .map(|(done, rows)| RowsDone {
            repeated: done.repeated,
            rows,
        })
    }

    /// One shop edit on one listing; its refusal is the request's. The
    /// answer's `id` is the listing, or the new one of a relist.
    pub(crate) fn shop_one(
        &mut self,
        owner: &Owner,
        req: &Req,
        op: &ShopOp,
        listing: i64,
        now: i64,
    ) -> Result<Done, Error> {
        self.act(req, now, |a| {
            a.check_market()?;
            a.check_key(&owner.economy)?;
            let op = a.checked_op(op)?;
            Ok(Some(a.shop_row(owner, &op, listing)?.unwrap_or(listing)))
        })
    }

    /// A page of My shop: one tab, 50 rows a page, with every tab's count
    /// and the slots used.
    pub fn shop_page(
        &self,
        owner: &Owner,
        tab: ShopTab,
        page: u16,
        sort: ShopSort,
        desc: bool,
        now: i64,
    ) -> Result<ShopPage, Error> {
        self.check_key(&owner.economy)?;
        let conn = self.conn();
        let split = owner.economy.split();
        let kept = now - SHOP_KEEP_MS;
        let sums = ShopTab::ALL
            .iter()
            .map(|t| format!("COALESCE(SUM({}), 0)", t.sql()))
            .collect::<Vec<_>>()
            .join(", ");
        let counts: [u32; 4] = conn.query_row(
            &format!("SELECT {sums} FROM listings l WHERE l.account = ?1 AND l.economy = ?2"),
            params![owner.account.0, split, kept],
            |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]),
        )?;
        let dir = if desc { "DESC" } else { "ASC" };
        let order = match sort {
            ShopSort::Posted => format!("l.posted {dir}"),
            ShopSort::Ends => format!("COALESCE(l.closed, l.expires) {dir}"),
            ShopSort::Gold => format!("l.price_gold IS NULL, l.price_gold {dir}"),
            ShopSort::Name => format!("COALESCE(i.name, l.code) COLLATE NOCASE {dir}"),
        };
        let mut st = conn.prepare(&format!(
            "SELECT {LISTING_COLUMNS}, l.buyer_name, l.sold_price, l.sale_fee FROM listings l \
             LEFT JOIN econ_items i ON i.item_id = l.item_id \
             WHERE l.account = ?1 AND l.economy = ?2 AND {} \
             ORDER BY {order}, l.id {dir} LIMIT ?4 OFFSET ?5",
            tab.sql()
        ))?;
        let found = st.query_map(
            params![
                owner.account.0,
                split,
                kept,
                PAGE,
                i64::from(page) * i64::from(PAGE)
            ],
            |r| {
                let l = listing_of(r)?;
                let buyer: Option<String> = r.get(15)?;
                let paid: Option<Vec<u8>> = r.get(16)?;
                let fee: Option<i64> = r.get(17)?;
                let sold = paid.as_deref().and_then(Price::decode).map(|paid| Sold {
                    buyer: buyer.unwrap_or_default(),
                    paid,
                    sale_fee: fee.unwrap_or(0) as u64,
                    closed: l.closed.unwrap_or(0),
                });
                Ok((l, sold))
            },
        )?;
        let found: Vec<(Listing, Option<Sold>)> = found.collect::<Result<_, _>>()?;
        let mut rows = Vec::with_capacity(found.len());
        for (mut listing, sold) in found {
            fill_item(conn, &mut listing)?;
            let newest_offer: Option<i64> = conn.query_row(
                "SELECT MAX(at) FROM offers WHERE listing = ?1",
                [listing.id],
                |r| r.get(0),
            )?;
            let relistable = matches!(
                listing.state,
                ListingState::Cancelled | ListingState::Expired
            ) && relist_source(conn, owner, &listing)?.is_some();
            rows.push(ShopRow {
                listing,
                newest_offer,
                sold,
                relistable,
            });
        }
        Ok(ShopPage {
            tab,
            page,
            rows,
            counts,
            slots_used: counts[0] + counts[1],
            slots_max: self.config().market_max_listings,
        })
    }

    /// A page of offers, newest first, 50 a page: on the account's
    /// listings or made by it; `live_only` for open and countered ones.
    pub fn offers_page(
        &self,
        owner: &Owner,
        which: OffersWhich,
        live_only: bool,
        page: u16,
    ) -> Result<OffersPage, Error> {
        self.check_key(&owner.economy)?;
        let conn = self.conn();
        let mut wh = match which {
            OffersWhich::OnMine => {
                "o.listing IN (SELECT id FROM listings WHERE account = ? AND economy = ?)"
            }
            OffersWhich::MadeByMe => "o.account = ? AND o.economy = ?",
        }
        .to_owned();
        if live_only {
            wh.push_str(" AND o.state IN ('open', 'countered')");
        }
        let mut values = vec![
            Value::Integer(owner.account.0),
            Value::Text(owner.economy.split()),
        ];
        let total: u32 = conn.query_row(
            &format!("SELECT COUNT(*) FROM offers o WHERE {wh}"),
            params_from_iter(values.iter()),
            |r| r.get(0),
        )?;
        values.push(Value::Integer(PAGE.into()));
        values.push(Value::Integer(i64::from(page) * i64::from(PAGE)));
        let offers: Vec<Offer> = {
            let mut st = conn.prepare(&format!(
                "SELECT {OFFER_COLUMNS} FROM offers o WHERE {wh} ORDER BY o.id DESC LIMIT ? OFFSET ?"
            ))?;
            let rows = st.query_map(params_from_iter(values.iter()), offer_of)?;
            rows.collect::<Result<_, _>>()?
        };
        let mut rows = Vec::with_capacity(offers.len());
        for offer in offers {
            let Some(listing) = self.listing(offer.listing)? else {
                continue;
            };
            let mut items = Vec::new();
            for id in &offer.price.items {
                items.extend(load_item(conn, *id)?);
            }
            rows.push(OfferRow {
                offer,
                listing,
                items,
            });
        }
        Ok(OffersPage { total, page, rows })
    }

    /// A page of the Deliveries tab, newest first, 50 a page.
    pub fn deliveries_page(&self, owner: &Owner, page: u16) -> Result<DeliveriesPage, Error> {
        self.check_key(&owner.economy)?;
        let conn = self.conn();
        let split = owner.economy.split();
        let total: u32 = conn.query_row(
            "SELECT COUNT(*) FROM deliveries WHERE account = ?1 AND economy = ?2",
            params![owner.account.0, split],
            |r| r.get(0),
        )?;
        let found: Vec<Delivery> = {
            let mut st = conn.prepare(
                "SELECT id, source, listing, offer, item_id, code, count, gold, at FROM deliveries \
                 WHERE account = ?1 AND economy = ?2 ORDER BY id DESC LIMIT ?3 OFFSET ?4",
            )?;
            let rows = st.query_map(
                params![
                    owner.account.0,
                    split,
                    PAGE,
                    i64::from(page) * i64::from(PAGE)
                ],
                |r| {
                    Ok(Delivery {
                        id: r.get(0)?,
                        source: r.get(1)?,
                        listing: r.get(2)?,
                        offer: r.get(3)?,
                        goods: goods_of(r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?),
                        item: None,
                        at: r.get(8)?,
                    })
                },
            )?;
            rows.collect::<Result<_, _>>()?
        };
        let mut rows = Vec::with_capacity(found.len());
        for mut delivery in found {
            if let Goods::Item(id) = delivery.goods {
                delivery.item = load_item(conn, id)?;
            }
            let of_listing: Option<Goods> = match delivery.listing {
                Some(l) => conn
                    .query_row(
                        "SELECT item_id, code, count FROM listings WHERE id = ?1",
                        [l],
                        |r| Ok(goods_of(r.get(0)?, r.get(1)?, r.get(2)?, None)),
                    )
                    .optional()?,
                None => None,
            };
            let about = goods_name(
                conn,
                self.config(),
                of_listing.as_ref().unwrap_or(&delivery.goods),
            )?;
            let state = |table: &str, id: Option<i64>| -> Result<Option<String>, Error> {
                let Some(id) = id else {
                    return Ok(None);
                };
                Ok(conn
                    .query_row(
                        &format!("SELECT state FROM {table} WHERE id = ?1"),
                        [id],
                        |r| r.get(0),
                    )
                    .optional()?)
            };
            let reason = Reason::of(
                &delivery.source,
                state("offers", delivery.offer)?.as_deref(),
                state("listings", delivery.listing)?.as_deref(),
            );
            rows.push(DeliveryRow {
                delivery,
                about,
                reason,
            });
        }
        Ok(DeliveriesPage { total, page, rows })
    }
}

/// Where a closed listing's goods are for a relist: a delivery of the
/// seller's that it returned (`Some(Some(id))`), or the stash
/// (`Some(None)`); `None` if neither holds them.
fn relist_source(
    conn: &rusqlite::Connection,
    owner: &Owner,
    l: &Listing,
) -> Result<Option<Option<i64>>, Error> {
    let split = owner.economy.split();
    let delivery: Option<i64> = conn
        .query_row(
            "SELECT id FROM deliveries WHERE account = ?1 AND economy = ?2 AND listing = ?3 \
             AND source IN ('cancelled', 'expired') ORDER BY id LIMIT 1",
            params![owner.account.0, split, l.id],
            |r| r.get(0),
        )
        .optional()?;
    if delivery.is_some() {
        return Ok(Some(delivery));
    }
    let in_stash = match &l.goods {
        Goods::Item(id) => conn
            .query_row(
                "SELECT 1 FROM econ_items WHERE item_id = ?1 AND account = ?2 AND economy = ?3 \
                 AND place = 'stash'",
                params![id.0, owner.account.0, split],
                |_| Ok(()),
            )
            .optional()?
            .is_some(),
        Goods::Stack { code, count } => {
            let have: Option<i64> = conn
                .query_row(
                    "SELECT count FROM stash_stacks WHERE account = ?1 AND economy = ?2 \
                     AND code = ?3",
                    params![owner.account.0, split, code],
                    |r| r.get(0),
                )
                .optional()?;
            have.unwrap_or(0) >= i64::from(*count)
        }
        Goods::Gold(_) => false,
    };
    Ok(in_stash.then_some(None))
}

impl Act<'_> {
    /// An edit's own arguments checked once for all its rows: a note's
    /// length and text, relist days, a reprice's price (normalized).
    fn checked_op(&self, op: &ShopOp) -> Result<ShopOp, Error> {
        Ok(match op {
            ShopOp::Note(n) => {
                check_note(n)?;
                op.clone()
            }
            ShopOp::Relist { days } => {
                self.check_days(*days)?;
                op.clone()
            }
            ShopOp::Reprice { mode, price } => ShopOp::Reprice {
                mode: *mode,
                price: self.checked_listing_price(*mode, price.as_ref())?,
            },
            ShopOp::Cancel | ShopOp::Hide | ShopOp::Show => op.clone(),
        })
    }

    /// The asker's listing `id`: gone if there is none, not theirs if it
    /// is another account's or in another split.
    fn my_listing(&self, owner: &Owner, id: i64) -> Result<Listing, Error> {
        let l = self.any_listing(id)?;
        if l.account != owner.account.0 || l.economy != owner.economy.split() {
            return Err(Refusal::NotYours.into());
        }
        Ok(l)
    }

    /// The asker's active listing `id` (shown or hidden).
    fn my_active(&self, owner: &Owner, id: i64) -> Result<Listing, Error> {
        self.my_listing(owner, id)?;
        self.active_listing(id)
    }

    /// One row of a shop edit; what it made (a relist's new listing).
    fn shop_row(&self, owner: &Owner, op: &ShopOp, id: i64) -> Result<Option<i64>, Error> {
        match op {
            ShopOp::Cancel => {
                let l = self.my_active(owner, id)?;
                self.close_listing(&l, ListingState::Cancelled, None)?;
                self.return_listing_goods(&l, "cancelled")?;
            }
            ShopOp::Hide | ShopOp::Show => {
                let hide = *op == ShopOp::Hide;
                let l = self.my_active(owner, id)?;
                if l.hidden != hide {
                    self.exec(
                        "UPDATE listings SET hidden = ?2 WHERE id = ?1",
                        params![id, hide],
                    )?;
                    if hide {
                        self.unindex_listing(id)?;
                    } else {
                        self.index_listing(id)?;
                    }
                }
            }
            ShopOp::Note(note) => {
                self.my_active(owner, id)?;
                self.exec(
                    "UPDATE listings SET note = ?2 WHERE id = ?1",
                    params![id, note],
                )?;
            }
            ShopOp::Reprice { mode, price } => {
                let l = self.my_active(owner, id)?;
                self.exec(
                    "UPDATE listings SET mode = ?2, price = ?3, price_gold = ?4 WHERE id = ?1",
                    params![
                        id,
                        *mode as u8,
                        price.as_ref().map(Price::encode),
                        price.as_ref().map(|p| p.gold as i64)
                    ],
                )?;
                if !l.hidden {
                    self.unindex_listing(id)?;
                    self.index_listing(id)?;
                }
            }
            ShopOp::Relist { days } => return self.relist_row(owner, id, *days),
        }
        Ok(None)
    }

    fn relist_row(&self, owner: &Owner, id: i64, days: u8) -> Result<Option<i64>, Error> {
        let l = self.my_listing(owner, id)?;
        let split = owner.economy.split();
        let account = owner.account.0;
        let until = self.now + i64::from(days) * DAY_MS;
        match l.state {
            ListingState::Active if l.expires > self.now => {
                self.take_listing_fee(account, &split, l.id)?;
                self.exec(
                    "UPDATE listings SET expires = ?2 WHERE id = ?1",
                    params![id, until],
                )?;
                self.exec(
                    "UPDATE market_items SET expires = ?2 WHERE listing = ?1",
                    params![id, until],
                )?;
                Ok(None)
            }
            ListingState::Cancelled | ListingState::Expired => {
                let source = relist_source(self.conn, owner, &l)?.ok_or(Refusal::ItemNotFound)?;
                let from = match source {
                    Some(delivery) => {
                        let goods = self.take_delivery_goods(owner, delivery)?;
                        if goods != l.goods {
                            return Err(Refusal::ItemNotFound.into());
                        }
                        ("delivery", Some(delivery))
                    }
                    None => {
                        match &l.goods {
                            Goods::Item(item) => {
                                self.check_in_stash(*item, account, &split)?;
                                self.registry.listable(self.conn, *item, self.now)?;
                            }
                            Goods::Stack { code, count } => self.remove_stack(
                                account,
                                &split,
                                code,
                                *count,
                                Refusal::ItemNotFound,
                            )?,
                            Goods::Gold(_) => return Err(Refusal::ItemNotFound.into()),
                        }
                        ("stash", Some(account))
                    }
                };
                self.check_listing_slots(account)?;
                let new = self.insert_listing(&NewListing {
                    account,
                    split: &split,
                    seller: &l.seller,
                    goods: &l.goods,
                    mode: l.mode,
                    price: l.price.as_ref(),
                    days,
                    note: &l.note,
                    hidden: false,
                    relisted_from: Some(l.id),
                    from,
                })?;
                self.take_listing_fee(account, &split, new)?;
                Ok(Some(new))
            }
            // Sold, or past its expiry and waiting for the sweep.
            _ => Err(Refusal::ListingGone.into()),
        }
    }
}
