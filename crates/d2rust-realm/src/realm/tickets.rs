//! The main server's join tickets (plan N3; contracts 11.5, B1).
//!
//! The signing keys are kept in the identity folder as `ticket-<id>.key`.
//! On its first start the main server makes key 1. With two key files it
//! signs with the newer (the higher id) and publishes both. That is how a
//! key is rotated: add the next key file and restart, so every game server
//! links again and holds both keys; after a ticket's lifetime remove the
//! old file and restart again. Game servers get the public keys in their
//! `NodeWelcome`, and an added game server's bundle lists them.
//!
//! [`RealmTickets::join_node`] makes the client's `JoinNode` for a seat
//! the main server has offered to a game server (the lobby sends it from
//! plan N4 on, when games run on game servers).

use std::path::Path;
use std::time::SystemTime;

use d2_net::msg::lobby::JoinNode;
use d2_net::msg::node::TicketKey as PublicKey;
use d2_tickets::{IssueError, SeatGrant, SignedIssuer, TicketKey, MAX_KEYS};

/// The main server's ticket issuer.
#[derive(Debug)]
pub struct RealmTickets {
    issuer: SignedIssuer,
}

/// How a client reaches the game server a seat is on (from the game
/// server's link).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeContact {
    /// Where it takes players: `host:port`.
    pub addr: String,
    /// The SHA-256 of its certificate (the client pins it).
    pub cert_sha256: [u8; 32],
    pub player_hosted: bool,
    /// `d2_net::msg::lobby::reach`.
    pub reach: u8,
}

impl RealmTickets {
    /// The ticket keys in `dir`: the newest signs, the one before it is
    /// published too. Makes key 1 when there is none and says so (`true`).
    pub fn load_or_make(dir: &Path) -> Result<(Self, bool), String> {
        let ids = key_ids(dir)?;
        let load = |id| {
            TicketKey::load(dir, id)
                .map_err(|e| format!("{}: {e}", TicketKey::file(dir, id).display()))
        };
        let (issuer, made) = match ids.as_slice() {
            [] => {
                let key = TicketKey::create(dir, 1)
                    .map_err(|e| format!("{}: {e}", TicketKey::file(dir, 1).display()))?;
                (SignedIssuer::new(key), true)
            }
            [.., newest] => {
                let issuer = SignedIssuer::new(load(*newest)?);
                match ids.len().checked_sub(MAX_KEYS).map(|i| ids[i]) {
                    Some(older) => (
                        issuer
                            .with_other(load(older)?.public())
                            .map_err(|e| e.to_string())?,
                        false,
                    ),
                    None => (issuer, false),
                }
            }
        };
        Ok((Self { issuer }, made))
    }

    /// The id of the key tickets are signed with.
    pub fn key_id(&self) -> u8 {
        self.issuer.key_id()
    }

    /// The public keys game servers check tickets with (for `NodeWelcome`
    /// and bundles), the signing key first.
    pub fn public_keys(&self) -> Vec<PublicKey> {
        d2_link::welcome_keys(&self.issuer.public_keys())
    }

    /// The `JoinNode` for the seat `g`, which the main server has offered
    /// to its game server, reached through `to`; the ticket is issued at
    /// `now`.
    pub fn join_node(
        &self,
        g: &SeatGrant,
        to: &NodeContact,
        now: SystemTime,
    ) -> Result<JoinNode, IssueError> {
        Ok(JoinNode {
            game_id: g.game,
            node: g.node.0.clone(),
            addr: to.addr.clone(),
            node_cert_sha256: to.cert_sha256,
            player_hosted: to.player_hosted,
            reach: to.reach,
            ticket: self.issuer.try_issue_at(g, now)?,
            difficulty: g.difficulty,
        })
    }
}

/// The ids of the ticket key files in `dir`, lowest first; none when the
/// folder doesn't exist yet.
fn key_ids(dir: &Path) -> Result<Vec<u8>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    let mut ids: Vec<u8> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let id = name.strip_prefix("ticket-")?.strip_suffix(".key")?;
            id.parse().ok()
        })
        .collect();
    ids.sort_unstable();
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use d2_link::{Lease, NodeId, PlayerId};
    use d2_net::msg::lobby::{reach, LobbyEvent};
    use d2_tickets::{
        OfferedSeat, SignedChecker, TicketError, TicketKeys, CLOCK_SLACK, TICKET_TTL,
    };

    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "d2rust-realm-tickets-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn grant() -> SeatGrant {
        SeatGrant {
            realm: "d2rust".into(),
            game: 7,
            node: NodeId("eu-1".into()),
            lease: Lease(42),
            player: PlayerId(9),
            character: "Kashya".into(),
            difficulty: 1,
            build: "0.1.2.0".into(),
        }
    }

    fn contact() -> NodeContact {
        NodeContact {
            addr: "203.0.113.5:6113".into(),
            cert_sha256: [3; 32],
            player_hosted: false,
            reach: reach::DIRECT,
        }
    }

    fn offer() -> OfferedSeat {
        OfferedSeat {
            game: 7,
            lease: Lease(42),
            player: PlayerId(9),
            character: "Kashya".into(),
        }
    }

    /// Game server `node`'s check, with the keys of a welcome from `t`.
    fn checker(t: &RealmTickets, node: &str) -> SignedChecker {
        let keys = d2_link::ticket_keys(&t.public_keys()).unwrap();
        SignedChecker::new("d2rust", NodeId(node.into()), keys)
    }

    #[test]
    fn key_1_is_made_once_and_kept() {
        let dir = temp("once");
        let (first, made) = RealmTickets::load_or_make(&dir).unwrap();
        assert!(made);
        assert_eq!(first.key_id(), 1);
        assert!(dir.join("ticket-1.key").is_file());
        let (again, made) = RealmTickets::load_or_make(&dir).unwrap();
        assert!(!made);
        assert_eq!(again.public_keys(), first.public_keys());
        assert_eq!(first.public_keys().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A rotation: the newer key signs, the older is still published, and
    /// a ticket of either passes.
    #[test]
    fn the_newer_key_signs_and_both_are_published() {
        let dir = temp("rotate");
        let (old, _) = RealmTickets::load_or_make(&dir).unwrap();
        TicketKey::create(&dir, 2).unwrap();
        std::fs::write(dir.join("ticket-x.key"), b"not a key").unwrap();
        let (new, made) = RealmTickets::load_or_make(&dir).unwrap();
        assert!(!made);
        assert_eq!(new.key_id(), 2);
        let ids: Vec<u8> = new.public_keys().iter().map(|k| k.key_id).collect();
        assert_eq!(ids, [2, 1]);
        let now = SystemTime::now();
        for t in [&old, &new] {
            let mut c = checker(&new, "eu-1");
            c.offer(offer(), now);
            let j = t.join_node(&grant(), &contact(), now).unwrap();
            assert!(c.verify(&j.ticket, now).is_ok());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The plan's ticket tests, from the main server's `JoinNode` to a game
    /// server's check: a round trip, a tampered byte, another game server,
    /// expired, reused, an unknown key, no offer.
    #[test]
    fn a_join_nodes_ticket_passes_its_game_server_once() {
        let dir = temp("join");
        let (t, _) = RealmTickets::load_or_make(&dir).unwrap();
        let now = SystemTime::now();
        let j = t.join_node(&grant(), &contact(), now).unwrap();
        assert_eq!(j.node, "eu-1");
        assert_eq!(j.addr, "203.0.113.5:6113");
        assert_eq!(j.difficulty, 1);
        assert!(j.ticket.len() <= d2_tickets::body::MAX_TICKET);
        // The client gets it whole.
        let ev = LobbyEvent::JoinNode(j.clone());
        assert_eq!(LobbyEvent::parse(&ev.body()), Ok(ev));

        let mut c = checker(&t, "eu-1");
        // No offer yet: the game server waits for it.
        assert_eq!(c.verify(&j.ticket, now), Err(TicketError::NoOffer));
        c.offer(offer(), now);
        let mut tampered = j.ticket.clone();
        tampered[10] ^= 1;
        assert_eq!(c.verify(&tampered, now), Err(TicketError::BadSignature));
        let late = now + TICKET_TTL + CLOCK_SLACK + Duration::from_secs(1);
        assert_eq!(c.verify(&j.ticket, late), Err(TicketError::Expired));
        let claims = c.verify(&j.ticket, now).unwrap();
        assert_eq!(claims.lease, Lease(42));
        assert_eq!(claims.character, "Kashya");
        c.offer(offer(), now);
        assert_eq!(c.verify(&j.ticket, now), Err(TicketError::Reused));

        let mut other = checker(&t, "eu-2");
        other.offer(offer(), now);
        assert_eq!(other.verify(&j.ticket, now), Err(TicketError::WrongNode));

        let stranger = TicketKey::from_pkcs8(1, &TicketKey::generate_pkcs8().unwrap()).unwrap();
        let keys = TicketKeys::new(vec![stranger.public()]).unwrap();
        let mut foreign = SignedChecker::new("d2rust", NodeId("eu-1".into()), keys);
        foreign.offer(offer(), now);
        assert_eq!(
            foreign.verify(&j.ticket, now),
            Err(TicketError::BadSignature)
        );
        let keys = TicketKeys::new(Vec::new()).unwrap();
        let mut keyless = SignedChecker::new("d2rust", NodeId("eu-1".into()), keys);
        keyless.offer(offer(), now);
        assert_eq!(keyless.verify(&j.ticket, now), Err(TicketError::UnknownKey));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
