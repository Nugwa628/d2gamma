//! The program's identity: its self-signed certificate and key, kept in
//! its identity folder across versions, and the certificate's SHA-256
//! fingerprint that clients (and, for a game server, the main server) pin.

use std::path::Path;

use d2_quic::{fingerprint, fingerprint_hex, Identity};

/// The identity in `dir`, made and written there first when the folder
/// holds none (`--new-identity`, and the first start). Returns it and
/// whether it was made now.
pub fn load_or_make(dir: &Path) -> Result<(Identity, bool), String> {
    Identity::load_or_create(dir).map_err(|e| e.to_string())
}

/// The identity in `dir`; an error when there is none yet.
pub fn load(dir: &Path) -> Result<Identity, String> {
    Identity::load(dir).map_err(|e| format!("no identity in {}: {e}", dir.display()))
}

/// The certificate's fingerprint as 64 lowercase hex digits.
pub fn fingerprint_of(id: &Identity) -> String {
    fingerprint_hex(&fingerprint(&id.cert))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identity_is_made_once_and_kept() {
        let dir = std::env::temp_dir().join(format!("d2-service-id-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(load(&dir).is_err(), "none yet");
        let (made, was_made) = load_or_make(&dir).unwrap();
        assert!(was_made);
        let (kept, was_made) = load_or_make(&dir).unwrap();
        assert!(!was_made);
        assert_eq!(fingerprint_of(&made), fingerprint_of(&kept));
        assert_eq!(fingerprint_of(&load(&dir).unwrap()), fingerprint_of(&made));
        assert_eq!(fingerprint_of(&made).len(), 64);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
