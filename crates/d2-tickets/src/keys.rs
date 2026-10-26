//! Ticket keys: the main server's Ed25519 signing keys and the public
//! halves game servers check tickets with.
//!
//! A signing key is kept in `<dir>/ticket-<key_id>.key` (PKCS#8, readable
//! only by its owner on Unix). No type here shows key bytes in its
//! `Debug` output, so a key can't end up in a log by accident.

use std::io;
use std::path::{Path, PathBuf};

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};

use crate::MAX_KEYS;

/// Why a ticket key couldn't be made, read or used.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("ticket key file: {0}")]
    Io(#[from] io::Error),
    #[error("the ticket key file doesn't hold an Ed25519 key")]
    BadKey,
    #[error("no random numbers for a new ticket key")]
    Random,
    #[error("more than {MAX_KEYS} ticket keys, or two with one id")]
    BadSet,
}

/// One of the main server's signing keys and its id.
pub struct TicketKey {
    id: u8,
    pair: Ed25519KeyPair,
}

impl TicketKey {
    /// A new random key's PKCS#8 bytes (what its file holds).
    pub fn generate_pkcs8() -> Result<Vec<u8>, KeyError> {
        let doc =
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).map_err(|_| KeyError::Random)?;
        Ok(doc.as_ref().to_vec())
    }

    /// The key in `pkcs8` (as [`generate_pkcs8`](Self::generate_pkcs8)
    /// makes it), under id `id`.
    pub fn from_pkcs8(id: u8, pkcs8: &[u8]) -> Result<Self, KeyError> {
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8).map_err(|_| KeyError::BadKey)?;
        Ok(Self { id, pair })
    }

    /// Where key `id` is kept in `dir`.
    pub fn file(dir: &Path, id: u8) -> PathBuf {
        dir.join(format!("ticket-{id}.key"))
    }

    /// Makes key `id` and writes its file in `dir`. Fails if the file
    /// exists: a key is never replaced in place (rotate to a new id).
    pub fn create(dir: &Path, id: u8) -> Result<Self, KeyError> {
        let pkcs8 = Self::generate_pkcs8()?;
        let key = Self::from_pkcs8(id, &pkcs8)?;
        std::fs::create_dir_all(dir)?;
        write_private(&Self::file(dir, id), &pkcs8)?;
        Ok(key)
    }

    /// Reads key `id` from its file in `dir`.
    pub fn load(dir: &Path, id: u8) -> Result<Self, KeyError> {
        Self::from_pkcs8(id, &std::fs::read(Self::file(dir, id))?)
    }

    pub fn id(&self) -> u8 {
        self.id
    }

    /// The public half, for game servers.
    pub fn public(&self) -> TicketPublicKey {
        let mut bytes = [0; 32];
        bytes.copy_from_slice(self.pair.public_key().as_ref());
        TicketPublicKey { id: self.id, bytes }
    }

    /// The 64-byte signature over `msg`.
    pub(crate) fn sign(&self, msg: &[u8]) -> ring::signature::Signature {
        self.pair.sign(msg)
    }
}

impl std::fmt::Debug for TicketKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TicketKey")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// A ticket key's public half and its id, as `NodeWelcome` carries it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TicketPublicKey {
    pub id: u8,
    pub bytes: [u8; 32],
}

impl TicketPublicKey {
    /// Whether `sig` is this key's signature over `msg`.
    pub(crate) fn verifies(&self, msg: &[u8], sig: &[u8]) -> bool {
        UnparsedPublicKey::new(&ED25519, &self.bytes)
            .verify(msg, sig)
            .is_ok()
    }
}

impl std::fmt::Debug for TicketPublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TicketPublicKey")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// The keys a game server accepts tickets from: at most [`MAX_KEYS`], each
/// id once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TicketKeys(Vec<TicketPublicKey>);

impl TicketKeys {
    pub fn new(keys: Vec<TicketPublicKey>) -> Result<Self, KeyError> {
        let unique = keys
            .iter()
            .enumerate()
            .all(|(i, k)| keys[..i].iter().all(|o| o.id != k.id));
        if keys.len() > MAX_KEYS || !unique {
            return Err(KeyError::BadSet);
        }
        Ok(Self(keys))
    }

    /// The key with id `id`.
    pub fn get(&self, id: u8) -> Option<&TicketPublicKey> {
        self.0.iter().find(|k| k.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &TicketPublicKey> {
        self.0.iter()
    }
}

/// Writes `bytes` to a new file only its owner can read (on Unix).
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}
