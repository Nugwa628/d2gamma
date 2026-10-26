//! Password hashing: argon2id, stored as a PHC string (contracts section
//! 4). The PHC string carries its own parameters, so a hash made at one
//! cost still verifies after the server's cost changes.
//!
//! A hash at the live cost works through 19 MiB of memory. The hashes
//! share [`HASHERS`] such blocks, made once and used again; a hash waits
//! for a free one. Memory allocated afresh for every login would be kept
//! by the allocator for each worker thread that hashed (the main server's
//! memory grew by 19 MiB steps as players logged in), and a burst of
//! logins would take 19 MiB each at once.

use std::sync::{Condvar, Mutex, PoisonError};

use argon2::password_hash::{Output, ParamsString, PasswordHash, SaltString};
use argon2::{Algorithm, Argon2, Block, Params, Version};

/// How many hashes run at once (and blocks of memory are kept).
pub const HASHERS: usize = 4;

/// The hashes' memory: the free blocks and how many were made.
struct Memory {
    free: Mutex<(Vec<Vec<Block>>, usize)>,
    returned: Condvar,
}

static MEMORY: Memory = Memory {
    free: Mutex::new((Vec::new(), 0)),
    returned: Condvar::new(),
};

/// Runs `f` with `blocks` blocks of argon2 memory from [`MEMORY`],
/// waiting while all [`HASHERS`] are in use.
fn with_memory<R>(blocks: usize, f: impl FnOnce(&mut [Block]) -> R) -> R {
    let mut mem = {
        let mut g = MEMORY.free.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(m) = g.0.pop() {
                break m;
            }
            if g.1 < HASHERS {
                g.1 += 1;
                break Vec::new();
            }
            g = MEMORY
                .returned
                .wait(g)
                .unwrap_or_else(PoisonError::into_inner);
        }
    };
    if mem.len() < blocks {
        mem.resize(blocks, Block::default());
    }
    let r = f(&mut mem[..blocks]);
    MEMORY
        .free
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .0
        .push(mem);
    MEMORY.returned.notify_one();
    r
}

/// The argon2 output of `password` and `salt` with `hasher`, `len` bytes.
fn output(
    hasher: &Argon2<'_>,
    params: &Params,
    password: &str,
    salt: &[u8],
    len: usize,
) -> Option<Output> {
    Output::init_with(len, |out| {
        with_memory(params.block_count(), |mem| {
            hasher.hash_password_into_with_memory(password.as_bytes(), salt, out, mem)
        })
        .map_err(|_| argon2::password_hash::Error::Password)
    })
    .ok()
}

/// How expensive a new hash is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashCost {
    /// The live cost: m = 19456 KiB, t = 2, p = 1.
    Normal,
    /// For test servers only: m = 256 KiB, t = 1, p = 1. The server logs a
    /// warning when it is set; a deployed server never sets it.
    Test,
}

impl HashCost {
    fn params(self) -> Params {
        let (m, t) = match self {
            HashCost::Normal => (19_456, 2),
            HashCost::Test => (256, 1),
        };
        Params::new(m, t, 1, None).expect("valid argon2 parameters")
    }
}

/// Hashes `password` with a fresh random salt.
pub fn hash(cost: HashCost, password: &str) -> String {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).expect("the operating system has a random source");
    hash_salted(cost, password, &bytes)
}

/// Hashes `password` with the salt `bytes`.
fn hash_salted(cost: HashCost, password: &str, bytes: &[u8]) -> String {
    let salt = SaltString::encode_b64(bytes).expect("16 bytes make a valid salt");
    let params = cost.params();
    let hasher = Argon2::new(Algorithm::Argon2id, Version::V0x13, params.clone());
    let hash = output(
        &hasher,
        &params,
        password,
        bytes,
        Params::DEFAULT_OUTPUT_LEN,
    )
    .expect("argon2 hashes any password of the allowed length");
    PasswordHash {
        algorithm: Algorithm::Argon2id.ident(),
        version: Some(Version::V0x13.into()),
        params: ParamsString::try_from(&params).expect("the costs make a valid string"),
        salt: Some(salt.as_salt()),
        hash: Some(hash),
    }
    .to_string()
}

/// Whether `password` matches the stored `phc` string. A damaged string
/// matches nothing.
pub fn verify(phc: &str, password: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(phc) else {
        return false;
    };
    let (Some(salt), Some(expected)) = (parsed.salt, parsed.hash) else {
        return false;
    };
    let Ok(algorithm) = Algorithm::try_from(parsed.algorithm) else {
        return false;
    };
    let version = match parsed.version.map(Version::try_from) {
        None => Version::default(),
        Some(Ok(v)) => v,
        Some(Err(_)) => return false,
    };
    let Ok(params) = Params::try_from(&parsed) else {
        return false;
    };
    let mut salt_bytes = [0; 64];
    let Ok(salt) = salt.decode_b64(&mut salt_bytes) else {
        return false;
    };
    let hasher = Argon2::new(algorithm, version, params.clone());
    // `Output`'s comparison takes the same time whatever differs.
    output(&hasher, &params, password, salt, expected.len()) == Some(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_verifies_its_password_only() {
        let h = hash(HashCost::Test, "correct horse");
        assert!(h.starts_with("$argon2id$v=19$m=256,t=1,p=1$"), "{h}");
        assert!(verify(&h, "correct horse"));
        assert!(!verify(&h, "correct horsf"));
        assert!(!verify("not a hash", "correct horse"));
    }

    #[test]
    fn salts_differ() {
        assert_ne!(
            hash(HashCost::Test, "same pass"),
            hash(HashCost::Test, "same pass")
        );
    }

    #[test]
    fn the_hashes_are_the_argon2_crates_own() {
        use argon2::password_hash::PasswordHasher;
        let salt = b"somesaltsomesalt";
        for cost in [HashCost::Test, HashCost::Normal] {
            let theirs = Argon2::new(Algorithm::Argon2id, Version::V0x13, cost.params())
                .hash_password(b"correct horse", &SaltString::encode_b64(salt).unwrap())
                .unwrap()
                .to_string();
            assert_eq!(hash_salted(cost, "correct horse", salt), theirs);
            assert!(verify(&theirs, "correct horse"));
            assert!(!verify(&theirs, "correct horsf"));
        }
    }

    #[test]
    fn many_hashes_at_once_share_the_memory() {
        let hashes: Vec<String> = std::thread::scope(|s| {
            let workers: Vec<_> = (0..HASHERS * 3)
                .map(|i| s.spawn(move || hash(HashCost::Test, &format!("pass {i}"))))
                .collect();
            workers.into_iter().map(|w| w.join().unwrap()).collect()
        });
        for (i, h) in hashes.iter().enumerate() {
            assert!(verify(h, &format!("pass {i}")));
        }
        let made = MEMORY.free.lock().unwrap().1;
        assert!((1..=HASHERS).contains(&made), "{made} blocks made");
    }

    #[test]
    fn the_normal_cost_is_the_contract_one() {
        let h = hash(HashCost::Normal, "correct horse");
        assert!(h.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"), "{h}");
        assert!(verify(&h, "correct horse"));
    }
}
