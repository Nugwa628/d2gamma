//! Adding an official game server (contracts 11.2): its entry in the list
//! (`data/nodes.toml`) with its certificate's fingerprint, and its bundle
//! (`node-ID.bundle`, [`d2_link::Bundle`]) for its folder.
//!
//! The game server's certificate is either one it made (`d2rust-node
//! --new-identity` prints the fingerprint, passed as `--cert-sha256`) or
//! one made here, into `node-ID.identity/` beside the bundle, to be copied
//! to the game server with it. Test setups (`bundle_dir`, `test_nodes`)
//! always take the second way, at each start, keeping a certificate made
//! before.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use d2_link::Bundle;
use d2_net::msg::node::{valid_node_id, TicketKey};
use d2_quic::{fingerprint, fingerprint_hex};
use d2_service::identity;

use super::nodes::{self, NodeEntry};

/// What `--add-node` is told.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddNode {
    pub id: String,
    pub name: String,
    /// The short country or region players see it as ("Finland", "US
    /// East"): the picker and the game list show it, not the name.
    pub location: String,
    /// The mod realm (the main server's own).
    pub game: String,
    pub max_games: u16,
    pub max_players_total: u16,
    /// The game server's own certificate, when it made one.
    pub cert_sha256: Option<[u8; 32]>,
    /// Where players reach it, when not where its link comes from.
    pub public_addr: Option<SocketAddr>,
}

/// What was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Added {
    pub bundle: PathBuf,
    /// The certificate made for it, when one was.
    pub identity: Option<PathBuf>,
    pub cert_sha256: String,
}

/// The identity folder made for game server `id` in `dir`.
pub fn identity_dir(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("node-{id}.identity"))
}

/// Adds game server `n` to the list `list` and writes its bundle into
/// `out`: it links to `realm_addr`, pinning `realm_cert`.
pub fn add_node(
    n: &AddNode,
    list: &Path,
    out: &Path,
    realm_addr: SocketAddr,
    realm_cert: [u8; 32],
    ticket_keys: Vec<TicketKey>,
) -> Result<Added, String> {
    if !valid_node_id(&n.id) {
        return Err(format!("game server id {:?}: 1-15 of a-z, 0-9 and -", n.id));
    }
    if n.name.is_empty() || n.name.len() > 64 || n.name.chars().any(char::is_control) {
        return Err("--name: 1-64 printable bytes".into());
    }
    if n.location.len() > 64 || n.location.chars().any(char::is_control) {
        return Err("--location: at most 64 printable bytes".into());
    }
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let (cert, made) = match n.cert_sha256 {
        Some(fp) => (fp, None),
        None => {
            let dir = identity_dir(out, &n.id);
            let (id, _) = identity::load_or_make(&dir)?;
            (fingerprint(&id.cert), Some(dir))
        }
    };
    let cert_sha256 = fingerprint_hex(&cert);
    nodes::upsert(
        list,
        NodeEntry {
            id: n.id.clone(),
            name: n.name.clone(),
            location: n.location.clone(),
            display_name: String::new(),
            game: n.game.clone(),
            cert_sha256: cert_sha256.clone(),
            public_addr: n.public_addr.map(|a| a.to_string()).unwrap_or_default(),
            max_games: n.max_games,
            max_players_total: n.max_players_total,
        },
    )?;
    let bundle = Bundle {
        id: n.id.clone(),
        realm_addr,
        realm_cert,
        game: n.game.clone(),
        ticket_keys,
    };
    let path = out.join(Bundle::file_name(&n.id));
    bundle
        .write(&path)
        .map_err(|e| format!("writing {}: {e}", path.display()))?;
    Ok(Added {
        bundle: path,
        identity: made,
        cert_sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(id: &str) -> AddNode {
        AddNode {
            id: id.into(),
            name: "Europe 1".into(),
            location: "Helsinki".into(),
            game: "d2rust".into(),
            max_games: 16,
            max_players_total: 128,
            cert_sha256: None,
            public_addr: None,
        }
    }

    #[test]
    fn adding_writes_the_entry_the_bundle_and_a_kept_certificate() {
        let dir = std::env::temp_dir().join(format!("d2rust-realm-add-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let list = dir.join("data/nodes.toml");
        let out = dir.join("bundles");
        let realm = "127.0.0.1:6112".parse().unwrap();
        let first = add_node(&add("eu-1"), &list, &out, realm, [7; 32], Vec::new()).unwrap();
        assert_eq!(first.bundle, out.join("node-eu-1.bundle"));
        let made = first.identity.clone().unwrap();
        let b = Bundle::load(&first.bundle).unwrap();
        assert_eq!(
            (b.id.as_str(), b.realm_addr, b.realm_cert),
            ("eu-1", realm, [7; 32])
        );
        let entries = nodes::load(&list).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].cert_sha256, first.cert_sha256);
        assert_eq!(
            identity::fingerprint_of(&identity::load(&made).unwrap()),
            first.cert_sha256
        );
        // Again (a test setup's next start): the same certificate.
        let again = add_node(&add("eu-1"), &list, &out, realm, [7; 32], Vec::new()).unwrap();
        assert_eq!(again.cert_sha256, first.cert_sha256);
        // A game server's own certificate: no identity made.
        let own = AddNode {
            cert_sha256: Some([5; 32]),
            ..add("eu-2")
        };
        let r = add_node(&own, &list, &out, realm, [7; 32], Vec::new()).unwrap();
        assert_eq!(r.identity, None);
        assert!(!identity_dir(&out, "eu-2").exists());
        assert_eq!(nodes::load(&list).unwrap().len(), 2);
        assert!(add_node(&add("EU"), &list, &out, realm, [7; 32], Vec::new()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
