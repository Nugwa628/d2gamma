//! The players' endpoint: the UDP port players connect to, and the socket
//! the link leaves from. A player's connection carries a game stream; its
//! first frame is `Attach` (contracts 11.4):
//!
//! - **mode 2**, the ticket of the main server's `JoinNode`: checked
//!   ([`crate::tickets`]); a ticket that doesn't pass is refused with
//!   `TicketInvalid`. One that passes takes its seat in its game
//!   ([`NodeGames::take_seat`]).
//! - **mode 1**, a resume: back to the game that holds its slot, else
//!   `UnknownResume`.
//! - **mode 0**, the dedicated server's ticket, or anything else:
//!   `Malformed`.
//!
//! A seated player's connection stays open with its game stream; a refused
//! one is closed after its answer.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use d2_net::msg::ErrorCode;
use d2_net::wire::{Attach, Refused};
use d2_quic::{FramedStream, QuicServer};
use d2_service::log::ServerLog;
use d2_tickets::ticket_tag;

use crate::seats::NodeGames;
use crate::tickets::Tickets;

/// How long a player has for its attach.
const ATTACH_WAIT: Duration = Duration::from_secs(10);

/// How long a connection stays open for its last frame.
const CLOSE_GRACE: Duration = Duration::from_secs(1);

/// Takes players until the endpoint closes.
pub async fn serve(
    quic: Arc<QuicServer>,
    tickets: Arc<Tickets>,
    games: Arc<NodeGames>,
    log: Arc<ServerLog>,
) {
    while let Some((conn, mut stream)) = quic.accept().await {
        let tickets = tickets.clone();
        let games = games.clone();
        let log = log.clone();
        tokio::spawn(async move {
            let peer = conn.peer();
            let Ok(Ok(body)) = tokio::time::timeout(ATTACH_WAIT, stream.recv_async()).await else {
                log.line(&format!("player from {peer}: no attach; closed"));
                conn.close(0, "no attach");
                return;
            };
            if attach(&body, stream, &tickets, &games, peer, &log)
                .await
                .is_err()
            {
                tokio::time::sleep(CLOSE_GRACE).await;
                conn.close(0, "refused");
            }
        });
    }
}

/// Answers a player's first frame `body` on its game stream: seats it, or
/// refuses it.
async fn attach(
    body: &[u8],
    stream: FramedStream,
    tickets: &Tickets,
    games: &NodeGames,
    peer: SocketAddr,
    log: &ServerLog,
) -> Result<(), ErrorCode> {
    match Attach::parse(body) {
        Ok(Attach::Node(ticket)) => match tickets.check(&ticket).await {
            Ok(claims) => {
                log.line(&format!(
                    "player from {peer}: {} for {} ({}, game {}) passed",
                    ticket_tag(&ticket),
                    claims.character,
                    claims.lease,
                    claims.game
                ));
                games.take_seat(&claims, stream, peer).await
            }
            Err(e) => {
                log.line(&format!(
                    "player from {peer}: {} refused: {e}",
                    ticket_tag(&ticket)
                ));
                refuse(stream, e.code())
            }
        },
        Ok(Attach::Resume(token)) => {
            let resumed = games.resume(token, stream).await;
            if let Err(code) = resumed {
                log.line(&format!("player from {peer}: resume refused: {code}"));
            }
            resumed
        }
        Ok(Attach::Ticket(_)) | Err(_) => {
            log.line(&format!("player from {peer}: not a game server's attach"));
            refuse(stream, ErrorCode::MALFORMED)
        }
    }
}

fn refuse(mut stream: FramedStream, code: ErrorCode) -> Result<(), ErrorCode> {
    let _ = stream.send(Refused::new(code).body());
    stream.close();
    Err(code)
}
