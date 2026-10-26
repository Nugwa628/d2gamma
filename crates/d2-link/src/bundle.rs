//! An official game server's bundle (contracts 11.2): the small file
//! `d2rust-realm --add-node` writes for it, `node-ID.bundle`. It holds no
//! secret: the game server's id, the main server's address and the
//! fingerprint the game server pins, the mod realm, and the main server's
//! public ticket keys (B1). It is read with the config reader:
//!
//! ```text
//! # d2rust-node bundle, written by d2rust-realm --add-node
//! id                = eu-1
//! realm_addr        = 203.0.113.10:6112
//! realm_cert_sha256 = <64 hex>
//! game              = d2rust
//! ticket_keys       = 1:<64 hex>,2:<64 hex>
//! ```

use std::net::SocketAddr;
use std::path::Path;

use d2_net::msg::node::{valid_node_id, TicketKey};
use d2_quic::{fingerprint_hex, parse_fingerprint};
use d2_service::config::{self, set_once, ConfigError};

/// What a bundle says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bundle {
    /// The game server's id.
    pub id: String,
    /// Where the main server listens.
    pub realm_addr: SocketAddr,
    /// The main server's certificate fingerprint (pinned).
    pub realm_cert: [u8; 32],
    /// The mod realm.
    pub game: String,
    /// The main server's ticket keys (B1; none before it).
    pub ticket_keys: Vec<TicketKey>,
}

impl Bundle {
    /// The bundle's file name for game server `id`.
    pub fn file_name(id: &str) -> String {
        format!("node-{id}.bundle")
    }

    /// Reads the bundle at `path`.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        Self::parse(&config::read(path)?)
    }

    /// Parses bundle text.
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let (mut id, mut addr, mut cert, mut game, mut keys) = (None, None, None, None, None);
        for entry in config::entries(text) {
            let e = entry?;
            match e.key {
                "id" => {
                    if !valid_node_id(e.value) {
                        return Err(e.bad("1-15 of a-z, 0-9 and -"));
                    }
                    set_once(&mut id, e.value.to_owned(), &e)?;
                }
                "realm_addr" => set_once(&mut addr, e.parse::<SocketAddr>()?, &e)?,
                "realm_cert_sha256" => {
                    let fp = parse_fingerprint(e.value).ok_or_else(|| e.bad("64 hex digits"))?;
                    set_once(&mut cert, fp, &e)?;
                }
                "game" => set_once(&mut game, e.value.to_owned(), &e)?,
                "ticket_keys" => {
                    let parsed = e
                        .value
                        .split(',')
                        .map(|k| {
                            let (id, public) = k.trim().split_once(':')?;
                            Some(TicketKey {
                                key_id: id.parse().ok()?,
                                public: parse_fingerprint(public)?,
                            })
                        })
                        .collect::<Option<Vec<_>>>()
                        .ok_or_else(|| e.bad("ID:HEX64 keys, comma separated"))?;
                    set_once(&mut keys, parsed, &e)?;
                }
                _ => return Err(e.unknown()),
            }
        }
        let missing = |key| ConfigError::Missing {
            key,
            why: "the bundle d2rust-realm --add-node wrote is incomplete",
        };
        Ok(Bundle {
            id: id.ok_or_else(|| missing("id"))?,
            realm_addr: addr.ok_or_else(|| missing("realm_addr"))?,
            realm_cert: cert.ok_or_else(|| missing("realm_cert_sha256"))?,
            game: game.ok_or_else(|| missing("game"))?,
            ticket_keys: keys.unwrap_or_default(),
        })
    }

    /// The file's text.
    pub fn text(&self) -> String {
        let mut t = format!(
            "# d2rust-node bundle, written by d2rust-realm\n\
             id = {}\nrealm_addr = {}\nrealm_cert_sha256 = {}\ngame = {}\n",
            self.id,
            self.realm_addr,
            fingerprint_hex(&self.realm_cert),
            self.game
        );
        if !self.ticket_keys.is_empty() {
            let keys: Vec<String> = self
                .ticket_keys
                .iter()
                .map(|k| format!("{}:{}", k.key_id, fingerprint_hex(&k.public)))
                .collect();
            t.push_str(&format!("ticket_keys = {}\n", keys.join(",")));
        }
        t
    }

    /// Writes the bundle to `path` (a temporary file renamed over it).
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("bundle.tmp");
        std::fs::write(&tmp, self.text())?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle() -> Bundle {
        Bundle {
            id: "eu-1".into(),
            realm_addr: "203.0.113.10:6112".parse().unwrap(),
            realm_cert: [0xab; 32],
            game: "d2rust".into(),
            ticket_keys: vec![],
        }
    }

    #[test]
    fn a_bundle_round_trips_through_its_text() {
        let b = bundle();
        assert_eq!(Bundle::parse(&b.text()).unwrap(), b);
        let keyed = Bundle {
            ticket_keys: vec![
                TicketKey {
                    key_id: 1,
                    public: [1; 32],
                },
                TicketKey {
                    key_id: 2,
                    public: [2; 32],
                },
            ],
            ..bundle()
        };
        assert_eq!(Bundle::parse(&keyed.text()).unwrap(), keyed);
        assert_eq!(Bundle::file_name("eu-1"), "node-eu-1.bundle");
    }

    #[test]
    fn a_bad_bundle_is_refused() {
        let text = bundle().text();
        let without_cert: String = text
            .lines()
            .filter(|l| !l.starts_with("realm_cert"))
            .map(|l| format!("{l}\n"))
            .collect();
        assert!(matches!(
            Bundle::parse(&without_cert),
            Err(ConfigError::Missing {
                key: "realm_cert_sha256",
                ..
            })
        ));
        let bad_id = text.replace("id = eu-1", "id = EU 1");
        assert!(matches!(
            Bundle::parse(&bad_id),
            Err(ConfigError::Value { .. })
        ));
        let extra = format!("{text}listen = 0.0.0.0:6113\n");
        assert!(matches!(
            Bundle::parse(&extra),
            Err(ConfigError::UnknownKey { .. })
        ));
        let bad_keys = format!("{text}ticket_keys = 1:00\n");
        assert!(matches!(
            Bundle::parse(&bad_keys),
            Err(ConfigError::Value { .. })
        ));
    }
}
