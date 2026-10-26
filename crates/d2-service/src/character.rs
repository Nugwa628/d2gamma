//! The account and new-character types a seat carries from the accounts
//! (`d2-accounts`) to the games (`d2-games`), which don't depend on each
//! other.

/// An account's row id in the accounts database.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountId(pub i64);

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "account {}", self.0)
    }
}

/// A character the account created that has never been played: the store
/// holds only its summary until its first game makes and saves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewChar {
    pub name: String,
    /// Class id 0-6.
    pub class: u8,
    pub hardcore: bool,
    pub expansion: bool,
    /// The progression bits (0-31, acts completed across the difficulties)
    /// the character starts with: what `--set-progression` stored before
    /// its first game. The game writes them into the new record, so its
    /// first save keeps them.
    pub progression: u8,
}
