//! The game servers' side of the stash's moves: `ItemOut`, `ItemInDone`
//! and `ItemOutRefused` on the link, each answered (`ItemOutAck`) once the
//! economy stored the move or refused it.

use d2_accounts::rules::name_key;
use d2_accounts::AccountId;
use d2_economy::{CharRef, Placed, PutGoods, Save};
use d2_link::LinkTx;
use d2_net::msg::node::RealmToNode;
use d2_net::msg::stash::goods;
use d2_net::msg::transfer::{placed, ItemInDone, ItemOut};
use d2_net::msg::ErrorCode;

use super::{code, econ_req, now_ms, now_s, Answer, Claimed, Stash, What};
use crate::realm::games::GameNews;
use crate::realm::saves::{NodeSave, TransferHalf};
use crate::realm::Realm;

impl Realm {
    /// A transfer's message from game server `node`, with the record of
    /// its save (`None`: an `ItemInDone` without one).
    pub(crate) async fn transfer(
        &self,
        node: &str,
        tx: &LinkTx<RealmToNode>,
        half: TransferHalf,
        record: Option<Vec<u8>>,
    ) {
        match (half, record) {
            (TransferHalf::Out(o), Some(record)) => self.item_out(node, tx, *o, record).await,
            (TransferHalf::In(d), record) => self.item_in_done(node, tx, d, record).await,
            (TransferHalf::Out(o), None) => {
                ack(tx, o.save.lease, o.req, ErrorCode::SAVE_INVALID.0);
            }
        }
    }

    /// `ItemOutRefused`: the game couldn't take the goods; nothing moved.
    pub(crate) fn transfer_refused(&self, lease: u64, req: u64, code: ErrorCode) {
        if let Some(stash) = &self.stash {
            stash.end(lease, req, Answer::of(code));
        }
    }

    /// A transfer's message whose record never came: refused, and a take's
    /// goods go back to the stash (the game takes them off again).
    pub(crate) async fn transfer_expired(
        &self,
        node: &str,
        tx: &LinkTx<RealmToNode>,
        half: TransferHalf,
    ) {
        let (lease, req) = half.transfer();
        self.log_line(&format!(
            "node {node}: transfer {req} of lease {lease} came without its record; refused"
        ));
        ack(tx, lease, req, ErrorCode::SAVE_INVALID.0);
        let Some(stash) = self.stash.clone() else {
            return;
        };
        if let Some(c) = stash.claim(lease, req) {
            if let What::Take { transfer, .. } = c.mv.what {
                self.take_back(&stash, &c, transfer).await;
            }
            stash.finish(c, Answer::of(ErrorCode::SAVE_INVALID));
        }
    }

    /// `ItemOut` and its record: the goods and the record without them go
    /// into the economy in one transaction.
    async fn item_out(&self, node: &str, tx: &LinkTx<RealmToNode>, o: ItemOut, record: Vec<u8>) {
        let (lease, req) = (o.save.lease, o.req);
        let claimed = self
            .stash
            .clone()
            .and_then(|s| s.claim(lease, req).map(|c| (s, c)));
        let Some((stash, c)) = claimed else {
            self.log_line(&format!(
                "node {node}: ItemOut {req} of lease {lease}: no such move; refused"
            ));
            ack(tx, lease, req, code::TRANSFER_UNKNOWN);
            return;
        };
        let a = match c.mv.what {
            What::Put { .. } => self.store_put(node, &stash, &c, &o, record).await,
            What::Take { .. } => Answer::refused(code::TRANSFER_UNKNOWN),
        };
        ack(tx, lease, req, a.code);
        stash.finish(c, a);
    }

    async fn store_put(
        &self,
        node: &str,
        stash: &Stash,
        c: &Claimed,
        o: &ItemOut,
        record: Vec<u8>,
    ) -> Answer {
        let save = NodeSave {
            info: o.save.clone(),
            record,
        };
        let grant = match self.leased(node, c, &save).await {
            Ok(g) => g,
            Err(a) => return a,
        };
        let (account, split) = (c.mv.owner.account.0, c.mv.owner.economy.split());
        if !o.credits.is_empty() {
            return self.store_credits(stash, c, o, save, &grant).await;
        }
        // A plain item of a listed kind is kept as a count, put to its
        // slot or to an item page alike.
        let as_stack = o.to == goods::STACK
            || (o.to == goods::ITEM
                && o.stack
                    .as_ref()
                    .is_some_and(|(code, _)| stash.config.stackable(code).is_some()));
        let (goods, cell) = match o.to {
            _ if as_stack => {
                let (code, count, id) = match stash.stack_of(o) {
                    Ok(x) => x,
                    Err(e) => {
                        self.log_line(&format!("stash: `{}`'s stackable: {e}", grant.character));
                        return Answer::refused(code::TRANSFER_INVALID);
                    }
                };
                if id != 0 {
                    if let Some(a) = self.unknown_item(&grant, id).await {
                        return a;
                    }
                }
                let held = match stash.store.amounts(account, &split) {
                    Ok(a) => a
                        .stacks
                        .into_iter()
                        .find(|(c, _)| *c == code)
                        .map_or(0, |(_, n)| n),
                    Err(e) => {
                        self.log_line(&format!("stash: {e}"));
                        return Answer::refused(code::OTHER);
                    }
                };
                if u64::from(held) + u64::from(count) > u64::from(stash.settings.cap) {
                    return Answer::refused(code::STASH_FULL);
                }
                (PutGoods::Stack { code, count }, None)
            }
            goods::ITEM => {
                let (item, size) = match stash.item_of(o) {
                    Ok(x) => x,
                    Err(e) => {
                        self.log_line(&format!("stash: `{}`'s item: {e}", grant.character));
                        return Answer::refused(code::TRANSFER_INVALID);
                    }
                };
                let wanted = match c.mv.what {
                    What::Put { cell } => cell,
                    What::Take { .. } => None,
                };
                let layout = match stash.store.layout(account, &split, &stash.grid) {
                    Ok(l) => l,
                    Err(e) => {
                        self.log_line(&format!("stash: {e}"));
                        return Answer::refused(code::OTHER);
                    }
                };
                let Some(at) = layout.place(size, wanted) else {
                    return Answer::refused(code::STASH_FULL);
                };
                if let Some(a) = self.unknown_item(&grant, item.id.0).await {
                    return a;
                }
                (PutGoods::Item(item), Some(at))
            }
            goods::GOLD if o.gold > 0 => (PutGoods::Gold(o.gold), None),
            _ => return Answer::refused(code::TRANSFER_INVALID),
        };
        let count = match &goods {
            PutGoods::Gold(g) => *g,
            PutGoods::Stack { count, .. } => u64::from(*count),
            PutGoods::Item(_) => 1,
        };
        let id = match &goods {
            PutGoods::Item(i) => Some(i.id.0),
            _ => None,
        };
        let from = CharRef {
            name: grant.character.clone(),
            game: grant.game,
            lease: c.lease,
        };
        let owner = c.mv.owner.clone();
        let s = Save {
            record: save.record,
            seq: save.info.seq,
        };
        let req = econ_req(c.lease, c.req, 0);
        let done = stash
            .bank
            .run(move |e| e.put(&owner, &req, &from, goods, &s, now_ms()))
            .await;
        if let Some(a) = self.refusal(done) {
            return a;
        }
        if let (Some(id), Some(at)) = (id, cell) {
            if let Err(e) = stash.store.set_cell(id, account, &split, &at) {
                // The next page read places it.
                self.log_line(&format!("stash: {e}"));
            }
        }
        self.write_record(stash, &grant.character).await;
        Answer { code: 0, count }
    }

    /// A mod game's item that becomes counts (`ItemOut.credits`, a
    /// container folded into what it holds): every listed stackable is
    /// credited with the record in one transaction, or none, each slot
    /// within the realm's cap. The game server is the one that knows what
    /// the item holds; the realm checks the codes, the counts and the
    /// item's ID.
    async fn store_credits(
        &self,
        stash: &Stash,
        c: &Claimed,
        o: &ItemOut,
        save: NodeSave,
        grant: &crate::realm::saves::LeaseGrant,
    ) -> Answer {
        if o.to != goods::STACK {
            return Answer::refused(code::TRANSFER_INVALID);
        }
        if let Some((bad, _)) = o
            .credits
            .iter()
            .find(|(code, n)| *n == 0 || stash.config.stackable(code).is_none())
        {
            self.log_line(&format!(
                "stash: `{}`'s credit of `{bad}`: no stackable of this realm",
                grant.character
            ));
            return Answer::refused(code::TRANSFER_INVALID);
        }
        if let Some(id) = o.item_id.filter(|&id| id != 0) {
            if let Some(a) = self.unknown_item(grant, id).await {
                return a;
            }
        }
        let count = o.credits.iter().map(|(_, n)| u64::from(*n)).sum();
        let credits = o.credits.clone();
        let cap = stash.settings.cap;
        let from = CharRef {
            name: grant.character.clone(),
            game: grant.game,
            lease: c.lease,
        };
        let owner = c.mv.owner.clone();
        let s = Save {
            record: save.record,
            seq: save.info.seq,
        };
        let req = econ_req(c.lease, c.req, 0);
        let done = stash
            .bank
            .run(move |e| e.put_stacks(&owner, &req, &from, &credits, cap, &s, now_ms()))
            .await;
        if let Some(a) = self.refusal(done) {
            return a;
        }
        self.write_record(stash, &grant.character).await;
        Answer { code: 0, count }
    }

    /// `ItemInDone`: placed, the record holding the goods closes the
    /// transfer; not placed, the goods go back to the stash.
    async fn item_in_done(
        &self,
        node: &str,
        tx: &LinkTx<RealmToNode>,
        d: ItemInDone,
        record: Option<Vec<u8>>,
    ) {
        let (lease, req) = (d.lease, d.req);
        let claimed = self
            .stash
            .clone()
            .and_then(|s| s.claim(lease, req).map(|c| (s, c)));
        let Some((stash, c)) = claimed else {
            self.log_line(&format!(
                "node {node}: ItemInDone {req} of lease {lease}: no such move"
            ));
            if record.is_some() {
                ack(tx, lease, req, code::TRANSFER_UNKNOWN);
            }
            return;
        };
        let What::Take { transfer, count } = c.mv.what else {
            if record.is_some() {
                ack(tx, lease, req, code::TRANSFER_UNKNOWN);
            }
            stash.finish(c, Answer::refused(code::TRANSFER_UNKNOWN));
            return;
        };
        let Some(record) = record.filter(|_| d.result == placed::PLACED) else {
            let why = if d.result == placed::NO_ROOM {
                Placed::NoRoom
            } else {
                Placed::Refused
            };
            let req_ = econ_req(lease, req, 1);
            let undone = stash
                .bank
                .run(move |e| e.take_done(&req_, lease, transfer, why, None, now_ms()))
                .await;
            if let Some(a) = self.refusal(undone) {
                self.log_line(&format!(
                    "stash: transfer {transfer} not undone: {}",
                    a.code
                ));
            }
            let a = if why == Placed::NoRoom {
                Answer::refused(code::NO_ROOM)
            } else {
                Answer::refused(code::TRANSFER_INVALID)
            };
            stash.finish(c, a);
            return;
        };
        let Some(info) = d.save.clone() else {
            return;
        };
        let save = NodeSave { info, record };
        let a = match self.leased(node, &c, &save).await {
            Ok(grant) => {
                let s = Save {
                    record: save.record,
                    seq: save.info.seq,
                };
                let req_ = econ_req(lease, req, 1);
                let done = stash
                    .bank
                    .run(move |e| {
                        e.take_done(&req_, lease, transfer, Placed::Placed, Some(&s), now_ms())
                    })
                    .await;
                match self.refusal(done) {
                    Some(a) => a,
                    None => {
                        self.write_record(&stash, &grant.character).await;
                        Answer { code: 0, count }
                    }
                }
            }
            Err(a) => a,
        };
        ack(tx, lease, req, a.code);
        if a.code != 0 {
            // The game takes the goods off again: they go back.
            self.take_back(&stash, &c, transfer).await;
        }
        stash.finish(c, a);
    }

    /// A put's item ID that the item registry never saw nor granted to
    /// the item's game: a game server's invention, refused (and logged)
    /// before anything is stored.
    async fn unknown_item(
        &self,
        grant: &crate::realm::saves::LeaseGrant,
        id: u32,
    ) -> Option<Answer> {
        match self.saves.knows_item(grant.game, id).await {
            Ok(true) => None,
            Ok(false) => {
                self.log_line(&format!(
                    "stash: `{}` put item ID {id:#010x} from game {}, which the item \
                     registry never granted: an invented item; refused",
                    grant.character, grant.game
                ));
                Some(Answer::refused(code::TRANSFER_INVALID))
            }
            Err(e) => {
                self.log_line(&format!("stash: item registry: {e}"));
                Some(Answer::refused(code::OTHER))
            }
        }
    }

    /// Puts a take's goods back in the stash (the game doesn't keep them).
    pub(super) async fn take_back(&self, stash: &Stash, c: &Claimed, transfer: i64) {
        let (lease, req) = (c.lease, econ_req(c.lease, c.req, 2));
        let undone = stash
            .bank
            .run(move |e| e.take_done(&req, lease, transfer, Placed::Refused, None, now_ms()))
            .await;
        if let Some(a) = self.refusal(undone) {
            // Already closed (stored, or undone with its lease).
            if a.code != code::TRANSFER_UNKNOWN {
                self.log_line(&format!(
                    "stash: transfer {transfer} not undone: {}",
                    a.code
                ));
            }
        }
    }

    /// The save that goes with a move: its lease must be live and the
    /// move's character's, and it passes the save checks and the item
    /// registry like any save.
    async fn leased(
        &self,
        node: &str,
        c: &Claimed,
        save: &NodeSave,
    ) -> Result<crate::realm::saves::LeaseGrant, Answer> {
        let grant = self
            .saves
            .take_lease(node, &save.info)
            .map_err(|d| Answer::of(d.code))?;
        if grant.account != AccountId(c.mv.account)
            || name_key(&grant.character) != name_key(&c.mv.character)
        {
            return Err(Answer::refused(code::TRANSFER_INVALID));
        }
        self.saves
            .guard(&grant, save)
            .await
            .map_err(|d| Answer::of(d.code))?;
        Ok(grant)
    }

    /// An economy call's refusal, if any, as the answer.
    fn refusal(
        &self,
        r: Result<Result<d2_economy::Done, d2_economy::Error>, String>,
    ) -> Option<Answer> {
        match r {
            Ok(Ok(_)) => None,
            Ok(Err(e)) => Some(match e.refusal() {
                Some(r) => Answer::refused(r.code()),
                None => {
                    self.log_line(&format!("stash: {e}"));
                    Answer::refused(code::OTHER)
                }
            }),
            Err(e) => {
                self.log_line(&format!("stash: {e}"));
                Some(Answer::refused(code::OTHER))
            }
        }
    }

    /// Writes the record a move stored for `character` to its file. One
    /// that can't be written stays for the main server's next start, unless
    /// a later save of the character is written first.
    async fn write_record(&self, stash: &Stash, character: &str) {
        let key = name_key(character);
        let r = match stash.store.record(&key) {
            Ok(Some(r)) => r,
            Ok(None) => return,
            Err(e) => {
                self.log_line(&format!("stash: {e}"));
                return;
            }
        };
        let account = AccountId(r.account);
        match self
            .saves
            .accounts()
            .store_record(account, &r.name, r.record.clone(), now_s())
            .await
        {
            Ok(summary) => {
                if let Err(e) = stash.store.record_written(&r) {
                    self.log_line(&format!("stash: {e}"));
                }
                self.news.send(GameNews::Saved { account, summary });
            }
            Err(d) => {
                self.log_line(&format!(
                    "stash: `{}`'s record: {}; kept for the next start",
                    r.name, d.text
                ));
                stash
                    .unwritten
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(key);
            }
        }
    }
}

fn ack(tx: &LinkTx<RealmToNode>, lease: u64, req: u64, code: u16) {
    let _ = tx.send_msg(&RealmToNode::ItemOutAck {
        lease,
        req,
        code: ErrorCode(code),
    });
}
