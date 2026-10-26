//! The game server's join tickets (plan N3; contracts 11.5, B1).
//!
//! The main server pushes each seat first (`SeatOffer`), then hands the
//! client a ticket for it; the client shows the ticket here
//! (`Attach` mode 2). [`Tickets`] checks it with the main server's public
//! keys from the link's `NodeWelcome`, against this game server's realm
//! and id and the offers it holds, once. It never asks the main server.
//!
//! - A ticket whose offer hasn't arrived yet is tried again every
//!   [`RETRY`] for up to [`OFFER_WAIT`]; then it is refused.
//! - An offer nobody claimed within `OFFER_TTL` is given up: the main
//!   server hears `SeatRefused` with `SeatGone` ([`Tickets::expire`],
//!   every second).
//! - A closed game's offers go with it.
//! - Without a link no ticket passes; a new link starts with no offers.
//!
//! A ticket is only ever logged as its first two bytes
//! ([`d2_tickets::ticket_tag`]).

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use d2_link::{Lease, NodeId, PlayerId};
use d2_net::msg::node::{NodeToRealm, NodeWelcome, RealmToNode};
use d2_net::msg::ErrorCode;
use d2_tickets::{OfferedSeat, SignedChecker, TicketClaims, TicketError, OFFER_WAIT};

/// How often a ticket waiting for its offer is tried again.
pub const RETRY: Duration = Duration::from_millis(100);

/// The ticket check of one game server.
#[derive(Debug, Default)]
pub struct Tickets {
    /// `None` while unlinked.
    checker: Mutex<Option<SignedChecker>>,
}

impl Tickets {
    fn lock(&self) -> MutexGuard<'_, Option<SignedChecker>> {
        self.checker.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The link is up: tickets are checked with `w`'s keys, realm and id.
    /// The offers of an earlier link are dropped (the main server ended
    /// their leases when it lost the link). Returns the key ids.
    pub fn linked(&self, w: &NodeWelcome) -> Result<Vec<u8>, String> {
        let keys = d2_link::ticket_keys(&w.ticket_keys).map_err(|e| e.to_string())?;
        let ids = keys.iter().map(|k| k.id).collect();
        *self.lock() = Some(SignedChecker::new(
            w.realm.clone(),
            NodeId(w.node.clone()),
            keys,
        ));
        Ok(ids)
    }

    /// The link is gone: no ticket passes until it is back.
    pub fn unlinked(&self) {
        *self.lock() = None;
    }

    /// Takes the ticket part of a main server's message: a seat offer is
    /// held, a game's close drops its offers. `true` if `m` was one.
    pub fn on_message(&self, m: &RealmToNode, now: SystemTime) -> bool {
        let mut c = self.lock();
        match m {
            RealmToNode::SeatOffer {
                game,
                lease,
                player,
                character,
                ..
            } => {
                if let Some(c) = c.as_mut() {
                    c.offer(
                        OfferedSeat {
                            game: *game,
                            lease: Lease(*lease),
                            player: PlayerId(*player),
                            character: character.clone(),
                        },
                        now,
                    );
                }
                true
            }
            RealmToNode::CloseGame { game, .. } => {
                if let Some(c) = c.as_mut() {
                    c.game_closed(*game);
                }
                true
            }
            _ => false,
        }
    }

    /// Withdraws the offer of `lease` (the main server kicked its seat):
    /// its ticket no longer passes.
    pub fn withdraw(&self, lease: u64) {
        if let Some(c) = self.lock().as_mut() {
            c.withdraw(Lease(lease));
        }
    }

    /// A game closed: its offers go with it.
    pub fn game_closed(&self, game: u32) {
        if let Some(c) = self.lock().as_mut() {
            c.game_closed(game);
        }
    }

    /// Gives up the offers nobody claimed in time; the main server is told
    /// for each.
    pub fn expire(&self, now: SystemTime) -> Vec<NodeToRealm> {
        let gone = match self.lock().as_mut() {
            Some(c) => c.expire_offers(now),
            None => Vec::new(),
        };
        gone.into_iter()
            .map(|s| NodeToRealm::SeatRefused {
                game: s.game,
                lease: s.lease.0,
                code: ErrorCode::SEAT_GONE,
            })
            .collect()
    }

    /// Checks `ticket` once at `now`: the seat it is for, or why not.
    /// Without a link it is `UnknownKey` (no keys are held).
    pub fn verify(&self, ticket: &[u8], now: SystemTime) -> Result<TicketClaims, TicketError> {
        match self.lock().as_mut() {
            Some(c) => c.verify(ticket, now),
            None => Err(TicketError::UnknownKey),
        }
    }

    /// Checks `ticket`, waiting up to [`OFFER_WAIT`] for its offer.
    pub async fn check(&self, ticket: &[u8]) -> Result<TicketClaims, TicketError> {
        let until = Instant::now() + OFFER_WAIT;
        loop {
            match self.verify(ticket, SystemTime::now()) {
                Err(e) if e.retry() && Instant::now() + RETRY <= until => {
                    tokio::time::sleep(RETRY).await;
                }
                done => return done,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use d2_net::msg::node::NodeKind;
    use d2_tickets::{SeatGrant, SignedIssuer, TicketKey, OFFER_TTL};

    use super::*;

    fn issuer() -> SignedIssuer {
        SignedIssuer::new(TicketKey::from_pkcs8(1, &TicketKey::generate_pkcs8().unwrap()).unwrap())
    }

    fn welcome(i: &SignedIssuer, node: &str) -> NodeWelcome {
        NodeWelcome {
            realm: "d2rust".into(),
            node: node.into(),
            kind: NodeKind::Official,
            seen_addr: "127.0.0.1:1".into(),
            heartbeat_ms: 1000,
            orphan_s: 30,
            journal_chunk_s: 10,
            save_interval_s: 60,
            ticket_keys: d2_link::welcome_keys(&i.public_keys()),
        }
    }

    fn grant(lease: u64, character: &str) -> SeatGrant {
        SeatGrant {
            realm: "d2rust".into(),
            game: 7,
            node: NodeId("eu-1".into()),
            lease: Lease(lease),
            player: PlayerId(9),
            character: character.into(),
            difficulty: 0,
            build: "0.1.2.0".into(),
        }
    }

    fn offer(lease: u64, character: &str) -> RealmToNode {
        RealmToNode::SeatOffer {
            game: 7,
            lease,
            player: 9,
            character: character.into(),
            new: None,
            has_record: true,
            ticket: None,
        }
    }

    fn ticket(i: &SignedIssuer, lease: u64, character: &str) -> Vec<u8> {
        i.try_issue_at(&grant(lease, character), SystemTime::now())
            .unwrap()
    }

    #[test]
    fn an_offered_seats_ticket_passes_once() {
        let i = issuer();
        let t = Tickets::default();
        assert_eq!(t.linked(&welcome(&i, "eu-1")).unwrap(), [1]);
        let now = SystemTime::now();
        assert!(t.on_message(&offer(42, "Kashya"), now));
        let k = ticket(&i, 42, "Kashya");
        let claims = t.verify(&k, now).unwrap();
        assert_eq!(claims.character, "Kashya");
        assert_eq!(claims.lease, Lease(42));
        assert_eq!(claims.game, 7);
        assert_eq!(t.verify(&k, now), Err(TicketError::Reused));
    }

    #[test]
    fn without_a_link_or_for_another_server_no_ticket_passes() {
        let i = issuer();
        let t = Tickets::default();
        let now = SystemTime::now();
        let k = ticket(&i, 42, "Kashya");
        assert_eq!(t.verify(&k, now), Err(TicketError::UnknownKey));
        t.linked(&welcome(&i, "eu-2")).unwrap();
        t.on_message(&offer(42, "Kashya"), now);
        assert_eq!(t.verify(&k, now), Err(TicketError::WrongNode));
        // A new link drops the old link's offers; losing it drops the keys.
        t.linked(&welcome(&i, "eu-1")).unwrap();
        assert_eq!(t.verify(&k, now), Err(TicketError::NoOffer));
        t.on_message(&offer(42, "Kashya"), now);
        t.unlinked();
        assert_eq!(t.verify(&k, now), Err(TicketError::UnknownKey));
    }

    /// The offer of another character (the main server's offer and the
    /// ticket disagree) is refused.
    #[test]
    fn a_ticket_for_another_character_than_its_offer_is_refused() {
        let i = issuer();
        let t = Tickets::default();
        t.linked(&welcome(&i, "eu-1")).unwrap();
        let now = SystemTime::now();
        t.on_message(&offer(42, "Kashya"), now);
        assert_eq!(
            t.verify(&ticket(&i, 42, "Akara"), now),
            Err(TicketError::OfferMismatch)
        );
    }

    #[test]
    fn unclaimed_offers_expire_as_seat_gone_and_close_with_their_game() {
        let i = issuer();
        let t = Tickets::default();
        t.linked(&welcome(&i, "eu-1")).unwrap();
        let now = SystemTime::now();
        t.on_message(&offer(42, "Kashya"), now);
        t.on_message(&offer(43, "Akara"), now);
        assert!(t.expire(now).is_empty());
        let later = now + OFFER_TTL + Duration::from_secs(1);
        assert_eq!(
            t.expire(later),
            [
                NodeToRealm::SeatRefused {
                    game: 7,
                    lease: 42,
                    code: ErrorCode::SEAT_GONE
                },
                NodeToRealm::SeatRefused {
                    game: 7,
                    lease: 43,
                    code: ErrorCode::SEAT_GONE
                },
            ]
        );
        t.on_message(&offer(44, "Kashya"), now);
        assert!(t.on_message(&RealmToNode::CloseGame { game: 7, how: 0 }, now));
        assert_eq!(
            t.verify(&ticket(&i, 44, "Kashya"), now),
            Err(TicketError::NoOffer)
        );
    }

    /// A ticket that arrives before its offer waits for it.
    #[tokio::test]
    async fn a_ticket_waits_for_its_offer() {
        let i = issuer();
        let t = Arc::new(Tickets::default());
        t.linked(&welcome(&i, "eu-1")).unwrap();
        let k = ticket(&i, 42, "Kashya");
        let late = {
            let t = t.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                t.on_message(&offer(42, "Kashya"), SystemTime::now());
            })
        };
        let started = Instant::now();
        assert_eq!(t.check(&k).await.unwrap().character, "Kashya");
        assert!(started.elapsed() >= Duration::from_millis(250));
        late.await.unwrap();
    }
}
