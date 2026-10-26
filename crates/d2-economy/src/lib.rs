//! The economy of one mod realm: the shared stash and the market
//! (multiplayer v2, plan section E; contracts 11.13).
//!
//! - **One database per mod realm.** Each mod (`d2rust`, `pd2`, ...) keeps
//!   its economy in its own SQLite file, `data/realms/<mod>/realm.sqlite`
//!   ([`realm_db_path`]), so nothing ever moves between mods: a request
//!   that names another mod is refused with
//!   [`Refusal::EconomyMismatch`]. Inside a mod, every row carries the
//!   split of its [`EconomyKey`] (softcore or hardcore, ladder season,
//!   classic or expansion), and goods never cross splits either.
//! - **Every change is one transaction.** A put, a take, a listing or a
//!   sale moves goods and writes its transfer-log row in one SQLite
//!   transaction ([`Economy`]); a crash at any point leaves either the
//!   whole move or none of it.
//! - **Every request is answered once.** Each changing call carries a
//!   [`Req`] (who asked, and their rising number). The answer is stored
//!   with the change; a repeat of the same `Req` gets the stored answer and
//!   changes nothing, so a lost answer never moves goods twice.
//! - **An item is in one place.** Items with a realm item ID live in one
//!   table keyed by that ID, so an item is in exactly one place of the
//!   economy: a stash, or one transfer in flight to a character (and,
//!   with the market, a listing's or an offer's escrow or a delivery).
//!   The realm's item registry (ID → place, across characters too) plugs
//!   in through [`ItemRegistry`]; character records stored in the same
//!   transaction plug in through [`CharacterRecords`].
//! - **Simple items are counts.** Runes, gems and the realm's other
//!   stackables ([`Stackable`]) are stored as `code → count`, never as
//!   item copies.
//!
//! The library holds no network code: the main server calls it for the
//! game's messages, and the web interface will call the same functions.
//! It never decodes items: the main server reads an item's facts (code,
//! quality, stats, ...) with its game data and hands them in as
//! [`ItemFacts`].

mod config;
mod db;
mod error;
mod key;
mod log;
mod market;
mod registry;
mod requests;
mod stash;

#[cfg(test)]
mod tests;

pub use config::{EconomyConfig, Stackable, MARKET_OFFER_ITEMS};
pub use db::{realm_db_path, Economy};
pub use error::{Done, Error, Refusal, RowDone, RowsDone};
pub use key::{AccountId, CharRef, EconomyKey, ItemId, Owner, Req, Scope};
pub use log::TransferRow;
pub use market::deliveries::{Claim, CLAIM_ROWS};
pub use market::facets::MAX_TYPES;
pub use market::history::{price_key, Bucket, History, Series, Shape, MAX_DAYS, RECENT};
pub use market::listings::{Post, PostOptions, NOTE_MAX};
pub use market::notices::{MarketCounts, Notice, NoticeKind};
pub use market::price::Price;
pub use market::shop::{
    DeliveriesPage, DeliveryRow, OfferRow, OffersPage, OffersWhich, Reason, ShopPage, ShopRow,
    ShopSort, ShopTab, Sold, EDIT_ROWS, SHOP_KEEP_MS,
};
pub use market::{
    Delivery, Facets, Group, Listing, ListingMode, ListingState, ListingsPage, Market, MarketQuery,
    MarketSort, Offer, OfferAnswer, OfferState, PriceFilter, PriceMode, ShopOp,
};
pub use registry::{CharacterRecords, ItemRegistry, Moved, NoRecords, NoRegistry, Place};
pub use stash::page::{ItemFilter, ItemsPage, StashItem, StashQuery, StashSort, StatRange, PAGE};
pub use stash::{Goods, InFlight, Item, ItemFacts, Placed, PutGoods, Save};

/// The SQLite binding the hooks receive their connection from.
pub use rusqlite;
