//! Signatures over a mod's content hash.
//!
//! The format only: a signature covers a fixed prefix and the 32-byte
//! content hash ([`crate::pack::content_hash`]), so it can be made once the
//! pack is built and checked wherever the pack is read, whatever the
//! container. A signature carries the public key beside the signature
//! bytes; [`verify`] only proves the two belong together and to this
//! content. Which keys to trust, how keys are made and stored, and where
//! the signature travels with a pack are the packager's and the mod list's
//! decisions.
//!
//! Ed25519 is the one algorithm; the text form starts with its name so
//! another can be added without breaking the readers of this one.

use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};

use crate::error::ModError;
use crate::pack::{content_hash, hex, Hash, PackSource};

/// The signed bytes are this prefix followed by the content hash, so a
/// signature made here can never be valid for anything else.
pub const SIGN_PREFIX: &[u8] = b"d2gamma-mod-pack-v1\0";

const ALGORITHM: &str = "ed25519";

/// An Ed25519 signature and the public key that made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackSignature {
    pub public_key: [u8; 32],
    pub signature: [u8; 64],
}

fn signed_bytes(hash: &Hash) -> Vec<u8> {
    let mut msg = SIGN_PREFIX.to_vec();
    msg.extend_from_slice(hash);
    msg
}

impl PackSignature {
    /// Signs `hash` with `key`.
    pub fn sign(hash: &Hash, key: &Ed25519KeyPair) -> PackSignature {
        let mut public_key = [0u8; 32];
        public_key.copy_from_slice(key.public_key().as_ref());
        let mut signature = [0u8; 64];
        signature.copy_from_slice(key.sign(&signed_bytes(hash)).as_ref());
        PackSignature {
            public_key,
            signature,
        }
    }

    /// The text form: `ed25519:<public key hex>:<signature hex>`.
    pub fn to_text(&self) -> String {
        format!(
            "{ALGORITHM}:{}:{}",
            hex(&self.public_key),
            hex(&self.signature)
        )
    }

    /// Reads the text form.
    pub fn from_text(text: &str) -> Result<PackSignature, ModError> {
        let mut parts = text.trim().split(':');
        let alg = parts.next().unwrap_or("");
        if alg != ALGORITHM {
            return Err(ModError::UnknownSignature(alg.to_owned()));
        }
        let (Some(key), Some(sig), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(ModError::BadSignature);
        };
        Ok(PackSignature {
            public_key: unhex(key).ok_or(ModError::BadSignature)?,
            signature: unhex(sig).ok_or(ModError::BadSignature)?,
        })
    }
}

fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// What a successful check found out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    pub content_hash: Hash,
    /// The public key of a valid signature, or `None` for an unsigned pack.
    pub signer: Option<[u8; 32]>,
}

impl Verified {
    /// Whether the pack carries a valid signature by one of `trusted`.
    pub fn signed_by(&self, trusted: &[[u8; 32]]) -> bool {
        self.signer.is_some_and(|k| trusted.contains(&k))
    }
}

/// Checks `signature` against `hash`: the signature must be valid for this
/// hash under the key stored with it.
pub fn verify(hash: &Hash, signature: &PackSignature) -> Result<(), ModError> {
    UnparsedPublicKey::new(&ED25519, &signature.public_key)
        .verify(&signed_bytes(hash), &signature.signature)
        .map_err(|_| ModError::BadSignature)
}

/// Checks a whole pack: its content against the hash recorded when it was
/// made, then the signature if there is one.
pub fn verify_pack(
    src: &dyn PackSource,
    recorded_hash: &Hash,
    signature: Option<&PackSignature>,
) -> Result<Verified, ModError> {
    crate::pack::verify_content(src, recorded_hash)?;
    let signer = match signature {
        Some(s) => {
            verify(recorded_hash, s)?;
            Some(s.public_key)
        }
        None => None,
    };
    Ok(Verified {
        content_hash: *recorded_hash,
        signer,
    })
}

/// Hashes `src` and signs the result in one step (what a packager does).
pub fn sign_pack(
    src: &dyn PackSource,
    key: &Ed25519KeyPair,
) -> Result<(Hash, PackSignature), ModError> {
    let hash = content_hash(src)?;
    Ok((hash, PackSignature::sign(&hash, key)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::ModFolder;
    use crate::pack::MemoryPack;
    use crate::testutil::sample;

    fn key(seed: u8) -> Ed25519KeyPair {
        // Fixed test seeds; real keys are the packager's business.
        Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap()
    }

    fn pack(name: &str) -> MemoryPack {
        MemoryPack::from_folder(&ModFolder::scan(&sample(name)).unwrap()).unwrap()
    }

    #[test]
    fn unsigned_and_signed_packs_verify() {
        let p = pack("sign-ok");
        let (hash, sig) = sign_pack(&p, &key(7)).unwrap();
        let unsigned = verify_pack(&p, &hash, None).unwrap();
        assert_eq!(unsigned.signer, None);
        assert!(!unsigned.signed_by(&[[0; 32]]));

        let v = verify_pack(&p, &hash, Some(&sig)).unwrap();
        assert_eq!(v.content_hash, hash);
        assert_eq!(v.signer, Some(sig.public_key));
        assert!(v.signed_by(&[[1; 32], sig.public_key]));
        assert!(!v.signed_by(&[[1; 32]]));
    }

    #[test]
    fn a_changed_pack_fails_the_hash_before_the_signature() {
        let mut p = pack("sign-changed");
        let (hash, sig) = sign_pack(&p, &key(7)).unwrap();
        p.files.get_mut("files/art/pic.bin").unwrap()[0] ^= 1;
        let e = verify_pack(&p, &hash, Some(&sig)).unwrap_err();
        assert!(matches!(e, ModError::HashMismatch { .. }), "{e}");
    }

    #[test]
    fn tampered_signatures_fail() {
        let p = pack("sign-bad");
        let (hash, sig) = sign_pack(&p, &key(7)).unwrap();
        let mut flipped = sig.clone();
        flipped.signature[0] ^= 1;
        assert!(matches!(
            verify_pack(&p, &hash, Some(&flipped)),
            Err(ModError::BadSignature)
        ));
        // A signature checked under another key.
        let mut swapped = sig.clone();
        swapped.public_key = PackSignature::sign(&hash, &key(8)).public_key;
        assert!(verify(&hash, &swapped).is_err());
        // A signature of other content does not move onto this one.
        let mut other = pack("sign-other");
        other.files.insert("files/more.txt".into(), vec![1]);
        let (other_hash, other_sig) = sign_pack(&other, &key(7)).unwrap();
        assert_ne!(other_hash, hash);
        assert!(verify(&hash, &other_sig).is_err());
        assert!(verify(&other_hash, &other_sig).is_ok());
    }

    #[test]
    fn the_text_form_round_trips_and_rejects_junk() {
        let hash = [9u8; 32];
        let sig = PackSignature::sign(&hash, &key(3));
        let text = sig.to_text();
        assert!(text.starts_with("ed25519:"));
        assert_eq!(PackSignature::from_text(&text).unwrap(), sig);
        assert_eq!(
            PackSignature::from_text(&format!(" {text}\n")).unwrap(),
            sig
        );
        assert!(matches!(
            PackSignature::from_text("rsa:00:00"),
            Err(ModError::UnknownSignature(a)) if a == "rsa"
        ));
        for junk in [
            "ed25519",
            "ed25519:zz:zz",
            "ed25519:00:00",
            &format!("{text}:extra"),
        ] {
            assert!(
                matches!(PackSignature::from_text(junk), Err(ModError::BadSignature)),
                "{junk}"
            );
        }
    }
}
