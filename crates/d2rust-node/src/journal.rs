//! The games' journals on their way to the main server (plan N7; contracts
//! 11.2, 11.11). Each game's recorder hands its chunks to a [`ChunkSink`];
//! one task sends them up the link in the order they were cut, each on a
//! journal stream of its own, then announces it with `JournalChunk`.
//!
//! A chunk cut while there is no link is dropped: the games of a lost link
//! are closed without saves when it is back, and their journals end there.

use std::sync::Arc;

use d2_games::journal::{JournalChunk, JournalSink};
use d2_link::streams::{self, What};
use d2_link::LinkTx;
use d2_net::msg::node::NodeToRealm;
use d2_quic::QuicConn;
use d2_service::log::ServerLog;
use tokio::sync::mpsc;

/// Where a game's recorder puts its chunks: the queue of [`send`].
pub struct ChunkSink(pub mpsc::UnboundedSender<JournalChunk>);

impl JournalSink for ChunkSink {
    fn chunk(&self, c: JournalChunk) {
        // The game thread never waits for the link.
        let _ = self.0.send(c);
    }
}

/// Sends the queued chunks up the link `link` gives at the time, one after
/// another.
pub async fn send(
    mut rx: mpsc::UnboundedReceiver<JournalChunk>,
    link: impl Fn() -> Option<(LinkTx<NodeToRealm>, QuicConn)>,
    log: Arc<ServerLog>,
) {
    while let Some(c) = rx.recv().await {
        let Some((tx, conn)) = link() else {
            log.line(&format!(
                "game {}: journal chunk {} dropped: no link to the main server",
                c.game, c.seq
            ));
            continue;
        };
        let sent = streams::send(&conn, What::Journal { game: c.game }, c.seq, &c.gz)
            .await
            .and_then(|()| {
                tx.send_msg(&NodeToRealm::JournalChunk {
                    game: c.game,
                    seq: c.seq,
                    first_tick: c.first_tick,
                    ticks: c.ticks,
                    last: c.last,
                })
            });
        if let Err(e) = sent {
            log.line(&format!(
                "game {}: sending journal chunk {} ({} bytes): {e}",
                c.game,
                c.seq,
                c.gz.len()
            ));
        }
    }
}
