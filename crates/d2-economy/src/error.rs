//! Answers: what a changing call did, or why it was refused.

/// Why the economy refused a request. Each has the wire error code the
/// main server answers with (contracts 3.8 and 11.7). A refusal is an
/// answer: it is stored with the request number like a success.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("the shared stash is full")]
    StashFull,
    #[error("the item, count or gold isn't there")]
    ItemNotFound,
    #[error("the listing was sold, cancelled or expired")]
    ListingGone,
    #[error("the listing's price changed")]
    PriceChanged,
    #[error("the stash doesn't hold enough to pay")]
    NotEnough,
    #[error("too many listings, offers or requests")]
    MarketLimit,
    #[error("the item is held")]
    ItemHeld,
    #[error("the offer was answered, withdrawn or expired")]
    OfferGone,
    #[error("the goods belong to another economy")]
    EconomyMismatch,
    #[error("the transfer failed its checks")]
    TransferInvalid,
    #[error("no such transfer is open")]
    TransferUnknown,
    #[error("the market is off")]
    MarketOff,
    #[error("the market's fees changed")]
    FeeChanged,
    #[error("that isn't yours")]
    NotYours,
    #[error("too many requests a minute")]
    RateLimited,
    #[error("the request doesn't make sense")]
    BadRequest,
}

impl Refusal {
    /// Every refusal, for code round trips.
    pub const ALL: [Refusal; 16] = [
        Refusal::StashFull,
        Refusal::ItemNotFound,
        Refusal::ListingGone,
        Refusal::PriceChanged,
        Refusal::NotEnough,
        Refusal::MarketLimit,
        Refusal::ItemHeld,
        Refusal::OfferGone,
        Refusal::EconomyMismatch,
        Refusal::TransferInvalid,
        Refusal::TransferUnknown,
        Refusal::MarketOff,
        Refusal::FeeChanged,
        Refusal::NotYours,
        Refusal::RateLimited,
        Refusal::BadRequest,
    ];

    /// The wire error code (`d2_net::msg::ErrorCode`).
    pub fn code(self) -> u16 {
        match self {
            Refusal::StashFull => 80,
            Refusal::ItemNotFound => 82,
            Refusal::ListingGone => 83,
            Refusal::PriceChanged => 84,
            Refusal::NotEnough => 85,
            Refusal::MarketLimit => 86,
            Refusal::ItemHeld => 87,
            Refusal::OfferGone => 88,
            Refusal::EconomyMismatch => 89,
            Refusal::TransferInvalid => 109,
            Refusal::TransferUnknown => 110,
            Refusal::MarketOff => 90,
            Refusal::FeeChanged => 91,
            Refusal::NotYours => 92,
            Refusal::RateLimited => 24,
            // `Other`, with the refusal's text.
            Refusal::BadRequest => 99,
        }
    }

    pub fn from_code(code: u16) -> Option<Refusal> {
        Refusal::ALL.into_iter().find(|r| r.code() == code)
    }
}

/// A changing call that failed: refused (an answer), or the database
/// failed (nothing changed, nothing stored; the caller may retry).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("refused: {0}")]
    Refused(#[from] Refusal),
    #[error("economy database: {0}")]
    Db(String),
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Db(e.to_string())
    }
}

impl Error {
    /// The refusal, if this is one.
    pub fn refusal(&self) -> Option<Refusal> {
        match self {
            Error::Refused(r) => Some(*r),
            Error::Db(_) => None,
        }
    }
}

/// One row of a request that acts on several rows (a shop edit on many
/// listings, a claim of many deliveries): each row is done or refused on
/// its own, in the request's one transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowDone {
    /// The listing or delivery the row named.
    pub id: i64,
    /// What the row made (a relist's new listing), or why it was refused.
    pub outcome: Result<Option<i64>, Refusal>,
}

/// A request on several rows that was taken (some rows may be refused).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowsDone {
    /// The request number was seen before: the stored answer, rows
    /// included; nothing changed now.
    pub repeated: bool,
    pub rows: Vec<RowDone>,
}

/// A changing call that was carried out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Done {
    /// What the call made, where it makes something: the transfer of a
    /// take, a listing, an offer.
    pub id: Option<i64>,
    /// The request number was seen before: this is the stored answer and
    /// nothing changed now.
    pub repeated: bool,
}
