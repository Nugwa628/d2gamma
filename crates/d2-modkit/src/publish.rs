//! Publishing metadata and author keys. The metadata record is excluded
//! from its own hash; the manifest (including the API pin) is covered.

use std::{fs, io::Write, path::Path};

use ring::{rand::SystemRandom, signature::Ed25519KeyPair};

use crate::{
    pack::{content_hash, hex, Hash, PackSource},
    sign::{self, PackSignature, Verified},
    MemoryPack, ModError,
};

pub const RECORD: &str = "files/d2gamma-pack-signatures.toml";

fn bad(message: impl Into<String>) -> ModError {
    ModError::BadPack(message.into())
}

/// Loads a persistent PKCS#8 author key, creating it privately on first use.
/// Callers pass a path in the author's user directory, never the mod folder.
pub fn author_key(path: &Path) -> Result<Ed25519KeyPair, ModError> {
    if !path.exists() {
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| bad("author key needs a parent folder"))?,
        )
        .map_err(|e| ModError::io(path.display(), e))?;
        let doc = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .map_err(|_| bad("could not generate an author signing key"))?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(path) {
            Ok(mut f) => {
                f.write_all(doc.as_ref())
                    .and_then(|_| f.sync_all())
                    .map_err(|e| ModError::io(path.display(), e))?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(ModError::io(path.display(), e)),
        }
    }
    let bytes = fs::read(path).map_err(|e| ModError::io(path.display(), e))?;
    Ed25519KeyPair::from_pkcs8(&bytes)
        .map_err(|_| bad("author key is invalid; restore it from your private backup"))
}

struct Payload<'a>(&'a dyn PackSource);
impl PackSource for Payload<'_> {
    fn paths(&self) -> Vec<String> {
        self.0.paths().into_iter().filter(|p| p != RECORD).collect()
    }
    fn read(&self, path: &str) -> Result<Vec<u8>, ModError> {
        self.0.read(path)
    }
}

/// Adds versioned metadata without changing the editable manifest. An existing
/// record is refused so a folder cannot smuggle stale signing metadata.
pub fn seal(pack: &mut MemoryPack, key: &Ed25519KeyPair) -> Result<Hash, ModError> {
    if pack.files.contains_key(RECORD) {
        return Err(bad(
            "the mod folder contains reserved pack signing metadata",
        ));
    }
    let manifest = crate::read_manifest(pack)?;
    match crate::check_api_now(manifest.api) {
        crate::ApiCheck::Ok => {}
        crate::ApiCheck::NeedsUpdate(m) | crate::ApiCheck::TooNew(m) => return Err(bad(m)),
    }
    let hash = content_hash(pack)?;
    let sig = PackSignature::sign(&hash, key);
    let record = format!("format = 1\ncontent_hash = \"{}\"\nauthor = \"{}\"\n# Optional reviewed_by_d2gamma = \"ed25519:public-key:signature\"\n", hex(&hash), sig.to_text());
    pack.files.insert(RECORD.into(), record.into_bytes());
    Ok(hash)
}

/// Verifies a published pack, including the optional review signature.
/// Public keys establish integrity; the mod list separately decides key trust.
pub fn verify(src: &dyn PackSource) -> Result<Verified, ModError> {
    let bytes = src.read(RECORD)?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| bad("pack signing metadata is not UTF-8"))?;
    let record: toml::Table = text
        .parse()
        .map_err(|e| bad(format!("pack signing metadata: {e}")))?;
    if record.get("format").and_then(toml::Value::as_integer) != Some(1) {
        return Err(bad("unknown pack signature format"));
    }
    for key in record.keys() {
        if !["format", "content_hash", "author", "reviewed_by_d2gamma"].contains(&key.as_str()) {
            return Err(bad(format!("unknown pack signature field `{key}`")));
        }
    }
    let field = |name| {
        record
            .get(name)
            .and_then(toml::Value::as_str)
            .ok_or_else(|| {
                bad(format!(
                    "pack signature field `{name}` is missing or not text"
                ))
            })
    };
    let h = field("content_hash")?;
    if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad("invalid pack content hash"));
    }
    let mut hash = [0; 32];
    for (i, b) in hash.iter_mut().enumerate() {
        *b = u8::from_str_radix(&h[i * 2..i * 2 + 2], 16)
            .map_err(|_| bad("invalid pack content hash"))?;
    }
    let author = PackSignature::from_text(field("author")?)?;
    let verified = sign::verify_pack(&Payload(src), &hash, Some(&author))?;
    if record.contains_key("reviewed_by_d2gamma") {
        sign::verify(
            &hash,
            &PackSignature::from_text(field("reviewed_by_d2gamma")?)?,
        )?;
    }
    Ok(verified)
}
