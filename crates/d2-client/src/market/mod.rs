// D2RUST's own: the client's side of the realm's market.
//! The market's data side in the client: the seam the window talks to
//! ([`source::MarketSource`]) and the in-process fake market tests and
//! offscreen shots run against ([`fake::FakeSource`]); a price check's
//! answer is [`prices`]. The window is
//! [`crate::ui::market`]; the app pumps asks and answers between them
//! (`app/market.rs`). The wire messages and the rules are the market
//! contract's (`docs/plans/server/market-contract.md`).

pub mod fake;
pub mod holdings;
pub mod prices;
#[cfg(feature = "online")]
pub mod realm;
pub mod source;
pub mod trade;
pub mod wire;
