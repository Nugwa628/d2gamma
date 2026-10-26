//! The DMs (lobby contract 3.2): a conversation per account pair, stored
//! for `dm_keep_days` (the store's `dms`), pushed as `DmLine` to every
//! session with a hello of both accounts.
//!
//! - A line to an account with no session that sent a hello waits
//!   (`SocialDone.flags` [`done_flags::STORED`]); it is pushed at the
//!   account's next hello and marked delivered then.
//! - A recipient who ignores the sender or whose `whispers` privacy
//!   refuses them answers `WhisperBlocked` (the store's rule).
//! - Self-targets and names nobody has answer `UnknownPlayer`.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::time::Instant;

use d2_accounts::social::{DmLimits, DmRow, Refusal, ShowWhere};
use d2_net::msg::social::{
    done_flags, kind, place, DmConversationRow, DmLine, Done, FromRealm, ListHead,
};

use super::{line_text, lock, wire_ms, Member, Social};
use crate::accounts::AccountId;
use crate::denied::Denied;

/// A stored line as `DmLine` seen from `account`, whose peer's account is
/// named `peer_account`.
fn line_of(req: u32, row: &DmRow, account: i64, peer_account: &str) -> FromRealm {
    let outgoing = row.from == account;
    FromRealm::DmLine(DmLine {
        req,
        dm: u64::try_from(row.id).unwrap_or(0),
        peer_account: peer_account.to_owned(),
        peer_char: if outgoing {
            row.to_char.clone()
        } else {
            row.from_char.clone()
        },
        outgoing,
        stored: !row.delivered,
        text: row.text.clone(),
        at_ms: wire_ms(row.at_ms),
    })
}

impl Social {
    fn dm_limits(&self) -> DmLimits {
        DmLimits {
            waiting_max: self.cfg.dm_waiting_max,
            keep_days: self.cfg.dm_keep_days,
            ..DmLimits::default()
        }
    }

    /// The character an online account plays (its first session with
    /// one).
    fn online_char(&self, account: AccountId) -> Option<String> {
        self.presence
            .sessions_of(account)
            .into_iter()
            .find_map(|h| h.character.map(|c| c.name))
    }

    /// Sends `body` to the account's sessions with a hello.
    fn push(&self, account: AccountId, body: &[u8]) {
        for h in self.hello_sessions(account) {
            h.out.send(body.to_vec());
        }
    }

    /// `Whisper` to `to` (a character name or `*account`).
    pub(super) async fn whisper(
        &self,
        who: &Member,
        req: u32,
        to: &str,
        text: &str,
    ) -> Result<Done, Denied> {
        let now = self.clock.now();
        self.check_mute(who.account, now).await?;
        let text = line_text(text)?;
        let from = who.account.0;
        let to_name = to.to_owned();
        let (peer, peer_last, my_last, new_peer) = self
            .accounts
            .store(move |db| {
                let peer = db
                    .find_player(&to_name)?
                    .filter(|p| p.account != from)
                    .ok_or(Refusal::UnknownPlayer)?;
                let new_peer = !db.dms().has_conversation(from, peer.account)?;
                let peer_last = db.last_character(peer.account)?;
                Ok((peer, peer_last, db.last_character(from)?, new_peer))
            })
            .await?;
        lock(&self.limits).whisper(from, new_peer, Instant::now())?;
        let peer_id = AccountId(peer.account);
        let to_char = peer
            .char_name
            .clone()
            .or_else(|| self.online_char(peer_id))
            .or(peer_last)
            .unwrap_or_default();
        let from_char = who
            .character
            .as_ref()
            .map(|c| c.name.clone())
            .or(my_last)
            .unwrap_or_default();
        let delivered = !self.hello_sessions(peer_id).is_empty();
        let limits = self.dm_limits();
        let (fc, tc, t) = (from_char.clone(), to_char.clone(), text.clone());
        let sent = self
            .accounts
            .store(move |db| {
                db.dms()
                    .send(from, peer.account, &fc, &tc, &t, now, delivered, limits)
            })
            .await?;
        let row = DmRow {
            id: sent.id,
            from,
            to: peer_id.0,
            from_char,
            to_char,
            text,
            at_ms: now,
            delivered,
        };
        self.push(
            who.account,
            &line_of(0, &row, from, &peer.account_name).body(),
        );
        if delivered {
            let mut live = row.clone();
            live.delivered = true;
            self.push(
                peer_id,
                &line_of(0, &live, peer_id.0, &who.account_name).body(),
            );
            self.reload(peer_id).await;
        }
        Ok(Done {
            id: Some(u64::try_from(sent.id).unwrap_or(0)),
            flags: if sent.stored { done_flags::STORED } else { 0 },
            ..Done::ok(req, kind::WHISPER)
        })
    }

    /// `DmConversations`: page `page`, newest first.
    pub(super) async fn conversations(
        &self,
        who: &Member,
        req: u32,
        page: u16,
    ) -> Result<(), Denied> {
        self.list_limit(who.account)?;
        let a = who.account.0;
        let (total, rows, hidden) = self
            .accounts
            .store(move |db| {
                let (total, rows) = db.dms().conversations(a, page)?;
                let mut hidden = Vec::with_capacity(rows.len());
                for c in &rows {
                    hidden.push(db.prefs().get(c.peer)?.show_where == ShowWhere::Nobody);
                }
                Ok((total, rows, hidden))
            })
            .await?;
        who.out.send(
            FromRealm::ListBegin(ListHead {
                req,
                what: kind::DM_CONVERSATIONS,
                total,
                page,
                rows: u16::try_from(rows.len()).unwrap_or(u16::MAX),
                now_ms: wire_ms(self.clock.now()),
            })
            .body(),
        );
        for (c, hidden) in rows.into_iter().zip(hidden) {
            let peer = AccountId(c.peer);
            let at = match self.presence.place_of(peer).0 {
                place::OFFLINE => place::OFFLINE,
                _ if hidden => place::HIDDEN,
                p => p,
            };
            let row = DmConversationRow {
                peer_account: c.peer_account,
                peer_char: self.online_char(peer).unwrap_or(c.peer_char),
                place: at,
                unread: c.unread,
                last_ms: wire_ms(c.last_ms),
                last_text: c.last_text,
            };
            who.out
                .send(FromRealm::DmConversationRow { req, row }.body());
        }
        who.out.send(FromRealm::ListEnd { req }.body());
        Ok(())
    }

    /// `DmHistory` with `peer`: `rows` lines before `before_dm`, oldest
    /// first.
    pub(super) async fn history(
        &self,
        who: &Member,
        req: u32,
        peer: &str,
        before_dm: u64,
        rows: u8,
    ) -> Result<(), Denied> {
        self.list_limit(who.account)?;
        let a = who.account.0;
        let peer = peer.to_owned();
        let before = i64::try_from(before_dm).unwrap_or(i64::MAX);
        let (peer, lines) = self
            .accounts
            .store(move |db| {
                let p = db
                    .find_player(&peer)?
                    .filter(|p| p.account != a)
                    .ok_or(Refusal::UnknownPlayer)?;
                let lines = db.dms().history(a, p.account, before, rows)?;
                Ok((p, lines))
            })
            .await?;
        let n = u16::try_from(lines.len()).unwrap_or(u16::MAX);
        who.out.send(
            FromRealm::ListBegin(ListHead {
                req,
                what: kind::DM_HISTORY,
                total: u32::from(n),
                page: 0,
                rows: n,
                now_ms: wire_ms(self.clock.now()),
            })
            .body(),
        );
        for l in &lines {
            who.out.send(line_of(req, l, a, &peer.account_name).body());
        }
        who.out.send(FromRealm::ListEnd { req }.body());
        Ok(())
    }

    /// `DmRead`: the conversation with `peer_account` (with or without
    /// its `*`) read up to `up_to_dm`; every session's counts follow.
    pub(super) async fn dm_read(
        &self,
        who: &Member,
        peer_account: &str,
        up_to_dm: u64,
    ) -> Result<(), Denied> {
        let a = who.account.0;
        let name = format!(
            "*{}",
            peer_account.strip_prefix('*').unwrap_or(peer_account)
        );
        let up_to = i64::try_from(up_to_dm).unwrap_or(i64::MAX);
        self.accounts
            .store(move |db| {
                let p = db
                    .find_player(&name)?
                    .filter(|p| p.account != a)
                    .ok_or(Refusal::UnknownPlayer)?;
                Ok(db.dms().mark_read(a, p.account, up_to)?)
            })
            .await?;
        self.reload(who.account).await;
        Ok(())
    }

    /// The lines that waited for the account: pushed to its sessions with
    /// a hello (`stored`), then marked delivered.
    pub(super) async fn deliver_waiting(&self, account: AccountId) {
        let a = account.0;
        let waiting = self
            .accounts
            .store(move |db| {
                let rows = db.dms().undelivered(a)?;
                let mut names = BTreeMap::new();
                for r in &rows {
                    if let Entry::Vacant(e) = names.entry(r.from) {
                        let name = db.account_by_id(r.from)?.map(|x| x.name);
                        e.insert(name.unwrap_or_default());
                    }
                }
                Ok((rows, names))
            })
            .await;
        let (rows, names) = match waiting {
            Ok(w) => w,
            Err(d) => return self.log_line(&format!("{account}: waiting DMs: {}", d.text)),
        };
        let Some(last) = rows.last().map(|r| r.id) else {
            return;
        };
        for r in &rows {
            let name = names.get(&r.from).map_or("", String::as_str);
            self.push(account, &line_of(0, r, a, name).body());
        }
        let marked = self
            .accounts
            .store(move |db| Ok(db.dms().mark_delivered(a, last)?))
            .await;
        if let Err(d) = marked {
            self.log_line(&format!("{account}: marking DMs delivered: {}", d.text));
        }
    }
}
