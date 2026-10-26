//! The market (plan E2 and E3; modules N29a, N30 and N31): listings from
//! the shared stash, their goods held in escrow, buying for gold and
//! stackables, offers and counter-offers ([`offers`]), the Deliveries tab,
//! expiry, cancel, limits and the search ([`search`]).
//!
//! - **Post:** an item or a count of a stackable leaves the seller's stash
//!   into the listing's escrow, in one transaction with the listing. The
//!   listing has a price, takes offers, or both ([`ListingMode`]).
//! - **Buy:** one transaction takes the price from the buyer's stash, puts
//!   the proceeds (less the fee, if the realm takes one) into the seller's
//!   Deliveries and the goods into the buyer's, and closes the listing. Of
//!   two buyers racing for one listing, the first transaction wins and the
//!   other is refused with [`Refusal::ListingGone`].
//! - **Cancel and expiry** send the goods back to the seller's Deliveries.
//! - **Deliveries** hold what the market gives an account (purchases,
//!   proceeds, returns), each with its source, until it is claimed into
//!   the stash ([`Economy::claim`]) or taken to a character in a game
//!   ([`Economy::take_delivery`], through a transfer in flight like a
//!   stash take).
//! - The market answers only when the realm's `market` switch is on; the
//!   owner turns it on after the audit tools are in.

pub(crate) mod deliveries;
pub(crate) mod facets;
pub(crate) mod history;
pub(crate) mod listings;
pub(crate) mod notices;
pub(crate) mod offers;
pub mod price;
pub(crate) mod search;
pub(crate) mod shop;

use rusqlite::{params, OptionalExtension, Row};

use crate::config::EconomyConfig;
use crate::db::{Act, Economy};
use crate::error::{Done, Error, Refusal, RowsDone};
use crate::key::{EconomyKey, ItemId, Owner, Req};
use crate::stash::page::{fill_stats, item_of, PAGE};
use crate::stash::{Goods, Item};
pub use deliveries::Delivery;
pub use facets::{Facets, Group};
use listings::Post;
pub use offers::{Offer, OfferAnswer, OfferState};
use price::Price;
pub use search::{MarketQuery, MarketSort, PriceFilter, PriceMode};
pub use shop::ShopOp;

/// Migration 2: listings, escrow and deliveries.
pub(crate) const SCHEMA: &str = "
CREATE TABLE listings(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account INTEGER NOT NULL,
    economy TEXT NOT NULL,
    seller TEXT NOT NULL,
    item_id INTEGER,
    code TEXT,
    count INTEGER,
    mode INTEGER NOT NULL,
    price BLOB,
    price_gold INTEGER,
    state TEXT NOT NULL,
    posted INTEGER NOT NULL,
    expires INTEGER NOT NULL,
    buyer INTEGER,
    closed INTEGER
);
CREATE INDEX listings_open ON listings(state, expires);
CREATE INDEX listings_owner ON listings(account, economy, state);
CREATE INDEX listings_posted ON listings(economy, state, posted);
CREATE TABLE escrow(
    holder TEXT NOT NULL,
    holder_id INTEGER NOT NULL,
    gold INTEGER NOT NULL CHECK(gold > 0),
    PRIMARY KEY(holder, holder_id)
) WITHOUT ROWID;
CREATE TABLE escrow_stacks(
    holder TEXT NOT NULL,
    holder_id INTEGER NOT NULL,
    code TEXT NOT NULL,
    count INTEGER NOT NULL CHECK(count > 0),
    PRIMARY KEY(holder, holder_id, code)
) WITHOUT ROWID;
CREATE TABLE deliveries(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account INTEGER NOT NULL,
    economy TEXT NOT NULL,
    source TEXT NOT NULL,
    listing INTEGER,
    offer INTEGER,
    item_id INTEGER,
    code TEXT,
    count INTEGER,
    gold INTEGER,
    at INTEGER NOT NULL
);
CREATE INDEX deliveries_owner ON deliveries(account, economy);
ALTER TABLE in_flight ADD COLUMN from_source TEXT;
";

/// Migration 5: what the market window needs (market contract 8). Item
/// facts gain the runeword flag, identified and the type's ancestors;
/// listings their shop fields (hidden, note, the sale); stored answers
/// their per-row results; the search index the group, names, price flags
/// and price key, and the type tree; and the sales history. Rows from
/// before are filled from what they hold: their items count as not
/// runewords, identified, of their item type alone.
pub(crate) const SCHEMA_5: &str = "
ALTER TABLE econ_items ADD COLUMN runeword INTEGER NOT NULL DEFAULT 0;
ALTER TABLE econ_items ADD COLUMN identified INTEGER NOT NULL DEFAULT 1;
ALTER TABLE econ_items ADD COLUMN types TEXT NOT NULL DEFAULT '';
ALTER TABLE listings ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;
ALTER TABLE listings ADD COLUMN note TEXT NOT NULL DEFAULT '';
ALTER TABLE listings ADD COLUMN buyer_name TEXT;
ALTER TABLE listings ADD COLUMN sold_price BLOB;
ALTER TABLE listings ADD COLUMN sale_fee INTEGER;
ALTER TABLE listings ADD COLUMN relisted_from INTEGER;
CREATE INDEX listings_closed ON listings(account, economy, state, closed);
ALTER TABLE requests ADD COLUMN rows BLOB;
ALTER TABLE market_items ADD COLUMN grp INTEGER NOT NULL DEFAULT 0;
ALTER TABLE market_items ADD COLUMN gorder INTEGER NOT NULL DEFAULT 6;
ALTER TABLE market_items ADD COLUMN name TEXT NOT NULL DEFAULT '';
ALTER TABLE market_items ADD COLUMN seller TEXT NOT NULL DEFAULT '';
ALTER TABLE market_items ADD COLUMN price_flags INTEGER NOT NULL DEFAULT 0;
ALTER TABLE market_items ADD COLUMN identified INTEGER NOT NULL DEFAULT 1;
ALTER TABLE market_items ADD COLUMN price_key TEXT NOT NULL DEFAULT '';
ALTER TABLE market_items ADD COLUMN price_cur TEXT;
ALTER TABLE market_items ADD COLUMN price_amt INTEGER;
UPDATE market_items SET grp = CASE
    WHEN item_id IS NULL THEN 7
    ELSE CASE quality WHEN 4 THEN 1 WHEN 6 THEN 2 WHEN 8 THEN 3 WHEN 5 THEN 4 WHEN 7 THEN 5
        ELSE 0 END
    END;
UPDATE market_items SET
    gorder = CASE grp WHEN 5 THEN 0 WHEN 4 THEN 1 WHEN 6 THEN 2 WHEN 2 THEN 3 WHEN 3 THEN 4
        WHEN 1 THEN 5 WHEN 0 THEN 6 ELSE 7 END,
    name = COALESCE((SELECT i.name FROM econ_items i WHERE i.item_id = market_items.item_id),
        code),
    seller = COALESCE((SELECT l.seller FROM listings l WHERE l.id = market_items.listing), ''),
    price_flags = (CASE WHEN mode != 1 THEN 1 ELSE 0 END)
        | (CASE WHEN mode != 0 THEN 2 ELSE 0 END)
        | (CASE WHEN price_gold IS NOT NULL THEN 4 ELSE 0 END)
        | (CASE WHEN EXISTS (SELECT 1 FROM market_prices p WHERE p.listing = market_items.listing)
            THEN 8 ELSE 0 END);
UPDATE market_items SET price_key = CASE
    WHEN item_id IS NULL THEN 's:' || code
    WHEN grp IN (4, 5) THEN 'u:' || name
    ELSE 'b:' || code || ':' || grp
    END;
UPDATE market_items SET price_cur = '', price_amt = price_gold WHERE price_flags & 13 = 5;
UPDATE market_items SET (price_cur, price_amt) =
    (SELECT code, count FROM market_prices p WHERE p.listing = market_items.listing)
    WHERE price_flags & 13 = 9
    AND (SELECT COUNT(*) FROM market_prices p WHERE p.listing = market_items.listing) = 1;
DROP INDEX market_items_posted;
CREATE INDEX market_items_posted ON market_items(economy, posted, expires);
CREATE INDEX market_items_grid ON market_items(economy, gorder, posted, expires);
CREATE INDEX market_items_grid_desc ON market_items(economy, gorder DESC, posted, expires);
CREATE INDEX market_items_cur ON market_items(economy, price_cur, price_amt);
CREATE INDEX market_items_key ON market_items(economy, price_key);
CREATE TABLE market_type_tree(
    item_type TEXT NOT NULL,
    type TEXT NOT NULL,
    PRIMARY KEY(type, item_type)
) WITHOUT ROWID;
INSERT OR IGNORE INTO market_type_tree(item_type, type)
    SELECT item_type, item_type FROM market_items WHERE item_id IS NOT NULL;
CREATE TABLE market_sales(
    id INTEGER PRIMARY KEY,
    economy TEXT NOT NULL,
    price_key TEXT NOT NULL,
    listing INTEGER NOT NULL,
    closed INTEGER NOT NULL,
    price BLOB NOT NULL,
    shape INTEGER NOT NULL
);
CREATE INDEX market_sales_key ON market_sales(economy, price_key, closed);
";

/// What a listing takes (`MarketPost.mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListingMode {
    /// Sold at its price.
    Price = 0,
    /// No price: buyers make offers.
    OffersOnly = 1,
    /// Sold at its price, or to an accepted offer.
    PriceAndOffers = 2,
}

impl ListingMode {
    pub fn from_u8(m: u8) -> Option<ListingMode> {
        match m {
            0 => Some(ListingMode::Price),
            1 => Some(ListingMode::OffersOnly),
            2 => Some(ListingMode::PriceAndOffers),
            _ => None,
        }
    }

    pub fn has_price(self) -> bool {
        self != ListingMode::OffersOnly
    }

    pub fn takes_offers(self) -> bool {
        self != ListingMode::Price
    }
}

/// Where a listing is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListingState {
    Active,
    Sold,
    Cancelled,
    Expired,
}

impl ListingState {
    pub fn as_str(self) -> &'static str {
        match self {
            ListingState::Active => "active",
            ListingState::Sold => "sold",
            ListingState::Cancelled => "cancelled",
            ListingState::Expired => "expired",
        }
    }

    fn parse(s: &str) -> ListingState {
        match s {
            "active" => ListingState::Active,
            "sold" => ListingState::Sold,
            "cancelled" => ListingState::Cancelled,
            _ => ListingState::Expired,
        }
    }
}

/// A listing as the market shows it (`ListingInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub id: i64,
    pub account: i64,
    /// The split of the economy it is in.
    pub economy: String,
    /// The seller's character name, as shown.
    pub seller: String,
    pub goods: Goods,
    /// The item's bytes and facts, for an item.
    pub item: Option<Item>,
    pub mode: ListingMode,
    pub price: Option<Price>,
    pub state: ListingState,
    pub posted: i64,
    pub expires: i64,
    /// When it was sold, cancelled or expired.
    pub closed: Option<i64>,
    /// Out of the search, still in escrow (My shop's Hidden tab).
    pub hidden: bool,
    /// The seller's public note, 0 to 60 characters.
    pub note: String,
    /// The quality group the browser lists it under.
    pub group: Group,
    /// Live offers on it.
    pub offers: u16,
    /// The seller account's sold listings in this economy.
    pub seller_sales: u32,
}

impl Listing {
    /// `ListingRow.price_hash`: the hash of its price (of no price for an
    /// offers-only listing), which a buyer echoes back.
    pub fn price_hash(&self) -> [u8; 32] {
        self.price.clone().unwrap_or_default().hash()
    }
}

/// The listing columns as [`listing_of`] reads them, under alias `l`.
pub(crate) const LISTING_COLUMNS: &str =
    "l.id, l.account, l.economy, l.seller, l.item_id, l.code, l.count, l.mode, l.price, l.state, \
     l.posted, l.expires, l.closed, l.hidden, l.note";

pub(crate) fn listing_of(r: &Row) -> rusqlite::Result<Listing> {
    let item: Option<u32> = r.get(4)?;
    let (goods, group) = match item {
        Some(id) => (Goods::Item(ItemId(id)), Group::Other),
        None => (
            Goods::Stack {
                code: r.get(5)?,
                count: r.get(6)?,
            },
            Group::Stackable,
        ),
    };
    let price: Option<Vec<u8>> = r.get(8)?;
    let state: String = r.get(9)?;
    Ok(Listing {
        id: r.get(0)?,
        account: r.get(1)?,
        economy: r.get(2)?,
        seller: r.get(3)?,
        goods,
        item: None,
        mode: ListingMode::from_u8(r.get(7)?).unwrap_or(ListingMode::Price),
        price: price.as_deref().and_then(Price::decode),
        state: ListingState::parse(&state),
        posted: r.get(10)?,
        expires: r.get(11)?,
        closed: r.get(12)?,
        hidden: r.get(13)?,
        note: r.get(14)?,
        group,
        offers: 0,
        seller_sales: 0,
    })
}

/// The name of goods as the market shows it: the item's name, a count and
/// the stackable's name, or gold.
pub(crate) fn goods_name(
    conn: &rusqlite::Connection,
    config: &EconomyConfig,
    goods: &Goods,
) -> Result<String, Error> {
    Ok(match goods {
        Goods::Item(id) => conn
            .query_row(
                "SELECT name FROM econ_items WHERE item_id = ?1",
                [id.0],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| format!("item {}", id.0)),
        Goods::Stack { code, count } => {
            let name = config.stackable(code).map_or(code.as_str(), |s| &s.name);
            if *count == 1 {
                name.to_owned()
            } else {
                format!("{count} {name}")
            }
        }
        Goods::Gold(g) => format!("{g} gold"),
    })
}

/// Fills a listing's item bytes and facts from the economy's items, its
/// group, and its counts (live offers, the seller's sales).
pub(crate) fn fill_item(conn: &rusqlite::Connection, l: &mut Listing) -> Result<(), Error> {
    let (offers, sales): (u32, u32) = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM offers WHERE listing = ?1 AND state IN ('open', 'countered')), \
         (SELECT COUNT(*) FROM listings WHERE account = ?2 AND economy = ?3 AND state = 'sold')",
        params![l.id, l.account, l.economy],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    l.offers = u16::try_from(offers).unwrap_or(u16::MAX);
    l.seller_sales = sales;
    if let Goods::Item(id) = l.goods {
        let item = conn
            .query_row(
                &format!(
                    "SELECT {} FROM econ_items i WHERE i.item_id = ?1",
                    crate::stash::page::fact_columns("i")
                ),
                [id.0],
                |r| item_of(r, 0),
            )
            .optional()?;
        if let Some(mut item) = item {
            fill_stats(conn, &mut item)?;
            l.group = Group::of(&item.facts);
            l.item = Some(item);
        }
    }
    Ok(())
}

/// A page of listings (`MarketResults`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListingsPage {
    /// Listings matching, on every page.
    pub total: u32,
    pub page: u16,
    pub listings: Vec<Listing>,
}

/// The market's moves, as the game's requests and the web interface (N32)
/// make them.
pub trait Market {
    fn post(&mut self, owner: &Owner, req: &Req, post: &Post, now: i64) -> Result<Done, Error>;
    fn buy(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        price_hash: &[u8; 32],
        now: i64,
    ) -> Result<Done, Error>;
    fn cancel(&mut self, owner: &Owner, req: &Req, listing: i64, now: i64) -> Result<Done, Error>;
    fn offer(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        buyer: &str,
        price: &Price,
        now: i64,
    ) -> Result<Done, Error>;
    fn answer(
        &mut self,
        owner: &Owner,
        req: &Req,
        offer: i64,
        how: &OfferAnswer,
        now: i64,
    ) -> Result<Done, Error>;
    /// Takes a listing out of the search; it stays in escrow.
    fn hide(&mut self, owner: &Owner, req: &Req, listing: i64, now: i64) -> Result<Done, Error>;
    /// Puts a hidden listing back in the search.
    fn show(&mut self, owner: &Owner, req: &Req, listing: i64, now: i64) -> Result<Done, Error>;
    fn set_note(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        note: &str,
        now: i64,
    ) -> Result<Done, Error>;
    fn reprice(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        mode: ListingMode,
        price: Option<&Price>,
        now: i64,
    ) -> Result<Done, Error>;
    /// Extends an active listing, or posts a closed one's goods again (the
    /// answer's `id` is then the new listing).
    fn relist(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        days: u8,
        now: i64,
    ) -> Result<Done, Error>;
    /// One shop edit on up to 50 listings, each row answered on its own
    /// (`MarketShopEdit`), under the fees the player saw.
    fn shop_edit(
        &mut self,
        owner: &Owner,
        req: &Req,
        op: &ShopOp,
        listings: &[i64],
        fee_rev: u32,
        now: i64,
    ) -> Result<RowsDone, Error>;
}

impl Market for Economy {
    fn post(&mut self, owner: &Owner, req: &Req, post: &Post, now: i64) -> Result<Done, Error> {
        Economy::post(self, owner, req, post, now)
    }

    fn buy(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        price_hash: &[u8; 32],
        now: i64,
    ) -> Result<Done, Error> {
        Economy::buy(self, owner, req, listing, price_hash, now)
    }

    fn cancel(&mut self, owner: &Owner, req: &Req, listing: i64, now: i64) -> Result<Done, Error> {
        Economy::cancel(self, owner, req, listing, now)
    }

    fn offer(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        buyer: &str,
        price: &Price,
        now: i64,
    ) -> Result<Done, Error> {
        Economy::offer(self, owner, req, listing, buyer, price, now)
    }

    fn answer(
        &mut self,
        owner: &Owner,
        req: &Req,
        offer: i64,
        how: &OfferAnswer,
        now: i64,
    ) -> Result<Done, Error> {
        Economy::answer(self, owner, req, offer, how, now)
    }

    fn hide(&mut self, owner: &Owner, req: &Req, listing: i64, now: i64) -> Result<Done, Error> {
        self.shop_one(owner, req, &ShopOp::Hide, listing, now)
    }

    fn show(&mut self, owner: &Owner, req: &Req, listing: i64, now: i64) -> Result<Done, Error> {
        self.shop_one(owner, req, &ShopOp::Show, listing, now)
    }

    fn set_note(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        note: &str,
        now: i64,
    ) -> Result<Done, Error> {
        self.shop_one(owner, req, &ShopOp::Note(note.to_owned()), listing, now)
    }

    fn reprice(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        mode: ListingMode,
        price: Option<&Price>,
        now: i64,
    ) -> Result<Done, Error> {
        let op = ShopOp::Reprice {
            mode,
            price: price.cloned(),
        };
        self.shop_one(owner, req, &op, listing, now)
    }

    fn relist(
        &mut self,
        owner: &Owner,
        req: &Req,
        listing: i64,
        days: u8,
        now: i64,
    ) -> Result<Done, Error> {
        self.shop_one(owner, req, &ShopOp::Relist { days }, listing, now)
    }

    fn shop_edit(
        &mut self,
        owner: &Owner,
        req: &Req,
        op: &ShopOp,
        listings: &[i64],
        fee_rev: u32,
        now: i64,
    ) -> Result<RowsDone, Error> {
        Economy::shop_edit(self, owner, req, op, listings, fee_rev, now)
    }
}

impl Economy {
    /// The active listings of an economy, newest first, 50 a page: every
    /// listing, or those of one item type or stackable code. The simple
    /// list until the search (N30).
    pub fn browse(
        &self,
        economy: &EconomyKey,
        kind: Option<&str>,
        page: u16,
    ) -> Result<ListingsPage, Error> {
        self.check_key(economy)?;
        let conn = self.conn();
        let from = "FROM listings l LEFT JOIN econ_items i ON i.item_id = l.item_id \
                    WHERE l.economy = ?1 AND l.state = 'active' AND l.hidden = 0 \
                    AND (?2 IS NULL OR i.item_type = ?2 OR l.code = ?2)";
        let split = economy.split();
        let total: u32 = conn.query_row(
            &format!("SELECT COUNT(*) {from}"),
            params![split, kind],
            |r| r.get(0),
        )?;
        let mut st = conn.prepare(&format!(
            "SELECT {LISTING_COLUMNS} {from} ORDER BY l.id DESC LIMIT ?3 OFFSET ?4"
        ))?;
        let rows = st.query_map(
            params![split, kind, PAGE, u32::from(page) * PAGE],
            listing_of,
        )?;
        let mut listings: Vec<Listing> = rows.collect::<Result<_, _>>()?;
        for l in &mut listings {
            fill_item(conn, l)?;
        }
        Ok(ListingsPage {
            total,
            page,
            listings,
        })
    }

    /// A listing by its ID, in any state.
    pub fn listing(&self, id: i64) -> Result<Option<Listing>, Error> {
        let conn = self.conn();
        let l = conn
            .query_row(
                &format!("SELECT {LISTING_COLUMNS} FROM listings l WHERE l.id = ?1"),
                [id],
                listing_of,
            )
            .optional()?;
        match l {
            Some(mut l) => {
                fill_item(conn, &mut l)?;
                Ok(Some(l))
            }
            None => Ok(None),
        }
    }
}

impl Act<'_> {
    pub fn check_market(&self) -> Result<(), Error> {
        if self.config.market {
            Ok(())
        } else {
            Err(Refusal::MarketOff.into())
        }
    }

    /// A listing in any state; gone if there is none.
    pub fn any_listing(&self, id: i64) -> Result<Listing, Error> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {LISTING_COLUMNS} FROM listings l WHERE l.id = ?1"),
                [id],
                listing_of,
            )
            .optional()?
            .ok_or(Refusal::ListingGone)?)
    }

    /// An active listing, for changing; gone if sold, cancelled, expired
    /// or past its expiry.
    pub fn active_listing(&self, id: i64) -> Result<Listing, Error> {
        let l = self
            .conn
            .query_row(
                &format!("SELECT {LISTING_COLUMNS} FROM listings l WHERE l.id = ?1"),
                [id],
                listing_of,
            )
            .optional()?
            .ok_or(Refusal::ListingGone)?;
        if l.state != ListingState::Active || l.expires <= self.now {
            return Err(Refusal::ListingGone.into());
        }
        Ok(l)
    }

    pub fn escrow_stack(&self, holder: &str, id: i64, code: &str, count: u32) -> Result<(), Error> {
        self.exec(
            "INSERT INTO escrow_stacks(holder, holder_id, code, count) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(holder, holder_id, code) DO UPDATE SET count = count + excluded.count",
            params![holder, id, code, count],
        )?;
        Ok(())
    }

    /// Everything a holder's escrow holds, emptying it: its gold, counts
    /// and items, as goods.
    pub fn drain_escrow(&self, holder: &str, id: i64) -> Result<Vec<Goods>, Error> {
        let mut out = Vec::new();
        let gold: Option<i64> = self
            .conn
            .query_row(
                "SELECT gold FROM escrow WHERE holder = ?1 AND holder_id = ?2",
                params![holder, id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(g) = gold {
            out.push(Goods::Gold(g as u64));
            self.exec(
                "DELETE FROM escrow WHERE holder = ?1 AND holder_id = ?2",
                params![holder, id],
            )?;
        }
        let stacks: Vec<(String, u32)> = {
            let mut st = self.conn.prepare(
                "SELECT code, count FROM escrow_stacks WHERE holder = ?1 AND holder_id = ?2 \
                 ORDER BY code",
            )?;
            let rows = st.query_map(params![holder, id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        if !stacks.is_empty() {
            self.exec(
                "DELETE FROM escrow_stacks WHERE holder = ?1 AND holder_id = ?2",
                params![holder, id],
            )?;
        }
        out.extend(
            stacks
                .into_iter()
                .map(|(code, count)| Goods::Stack { code, count }),
        );
        let items: Vec<u32> = {
            let mut st = self.conn.prepare(
                "SELECT item_id FROM econ_items WHERE place = ?1 AND place_id = ?2 ORDER BY item_id",
            )?;
            let rows = st.query_map(params![holder, id], |r| r.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        out.extend(items.into_iter().map(|i| Goods::Item(ItemId(i))));
        Ok(out)
    }
}
