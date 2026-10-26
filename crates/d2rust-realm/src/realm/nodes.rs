//! The game-server list (contracts 11.2).
//!
//! - [`NodeEntry`] and the list file, `data/nodes.toml`: the official game
//!   servers the owner added (`d2rust-realm --add-node`), each with the
//!   fingerprint of the certificate it must link with, and the name players
//!   see it by when the admin set one ([`rename`], `--node-name`).
//! - [`NodeRegistry`]: the game servers linked now, with their last load
//!   and upgrade state, and the ones refused lately for their version.
//!   It reads the list file when a certificate links, so a server added
//!   while the main server runs can link at once.
//! - [`NodeRow`] and `logs/nodes.json` ([`nodes_json`]): every official
//!   game server, linked or not, for `server-ctl nodes`.
//!
//! ```toml
//! [[node]]
//! id = "eu-1"
//! name = "Europe 1"
//! location = "Helsinki"
//! display_name = "Finland"
//! game = "d2rust"
//! cert_sha256 = "<64 hex>"
//! public_addr = "203.0.113.20:6113"
//! max_games = 16
//! max_players_total = 128
//! ```

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use d2_link::LinkTx;
use d2_net::msg::node::{update_state, valid_node_id, Load, RealmToNode};
use d2_quic::{fingerprint_hex, parse_fingerprint, QuicConn};
use serde::{Deserialize, Serialize};

use crate::log::ServerLog;

/// One official game server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeEntry {
    pub id: String,
    pub name: String,
    /// Where it is (a short country or region, "Finland", "US East": the
    /// name players see when it has no `display_name`).
    pub location: String,
    /// The name players see it by in the picker and the game list
    /// (`--node-name`; empty: its location, `listing::display_name`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    /// The mod realm it serves.
    pub game: String,
    /// Its certificate's fingerprint, 64 hex digits.
    pub cert_sha256: String,
    /// The address players are sent to, when not the one the link comes
    /// from.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub public_addr: String,
    pub max_games: u16,
    pub max_players_total: u16,
}

#[derive(Default, Serialize, Deserialize)]
struct ListFile {
    #[serde(default, rename = "node")]
    nodes: Vec<NodeEntry>,
}

/// The list in `path`; empty when there is no file yet. Refused: a bad id
/// or fingerprint, or an id or certificate listed twice.
pub fn load(path: &Path) -> Result<Vec<NodeEntry>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("reading {}: {e}", path.display())),
    };
    let file: ListFile = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    for (i, n) in file.nodes.iter().enumerate() {
        let bad = |why: &str| format!("{}: game server {:?}: {why}", path.display(), n.id);
        if !valid_node_id(&n.id) {
            return Err(bad("the id must be 1-15 of a-z, 0-9 and -"));
        }
        if parse_fingerprint(&n.cert_sha256).is_none() {
            return Err(bad("cert_sha256 must be 64 hex digits"));
        }
        let earlier = &file.nodes[..i];
        if earlier.iter().any(|m| m.id == n.id) {
            return Err(bad("listed twice"));
        }
        if earlier
            .iter()
            .any(|m| m.cert_sha256.eq_ignore_ascii_case(&n.cert_sha256))
        {
            return Err(bad("its certificate is another game server's"));
        }
    }
    Ok(file.nodes)
}

/// Writes the list to `path` (a temporary file renamed over it).
pub fn save(path: &Path, nodes: &[NodeEntry]) -> Result<(), String> {
    let file = ListFile {
        nodes: nodes.to_vec(),
    };
    let body = toml::to_string(&file).map_err(|e| e.to_string())?;
    let text = format!(
        "# The official game servers (d2rust-realm --add-node). Each links with the\n\
         # certificate cert_sha256 names; another certificate is refused.\n\n{body}"
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("writing {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("writing {}: {e}", path.display()))
}

/// The longest display name ([`rename`]): what the lobby's list carries.
pub const MAX_DISPLAY_NAME: usize = d2_net::msg::nodelist::MAX_NODE_NAME;

/// Sets the name players see game server `id` by (`--node-name`), in the
/// list in `path` (written whole to a temporary file, then renamed over
/// it). Refused: an unknown id, or a name that is empty, longer than
/// [`MAX_DISPLAY_NAME`] bytes or holds control characters. Returns the
/// changed entry.
pub fn rename(path: &Path, id: &str, name: &str) -> Result<NodeEntry, String> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_DISPLAY_NAME || name.chars().any(char::is_control) {
        return Err(format!(
            "the display name must be 1-{MAX_DISPLAY_NAME} printable bytes"
        ));
    }
    let mut nodes = load(path)?;
    let entry = nodes
        .iter_mut()
        .find(|n| n.id == id)
        .ok_or_else(|| format!("{}: no game server {id:?}", path.display()))?;
    entry.display_name = name.to_string();
    let changed = entry.clone();
    save(path, &nodes)?;
    Ok(changed)
}

/// Adds `entry` to the list in `path`, or replaces the entry with its id.
pub fn upsert(path: &Path, entry: NodeEntry) -> Result<(), String> {
    let mut nodes = load(path)?;
    nodes.retain(|n| n.id != entry.id);
    nodes.push(entry);
    save(path, &nodes)
}

impl Linked {
    /// The link's connection (for its record streams).
    pub fn conn(&self) -> &QuicConn {
        &self.conn
    }
}

/// A game server linked now.
#[derive(Clone, Debug)]
pub struct Linked {
    /// This link's number (a game server that links again gets a new one).
    pub link: u64,
    pub entry: NodeEntry,
    /// Where its link comes from: the address its players reach.
    pub addr: SocketAddr,
    pub load: Load,
    pub since: Instant,
    /// Its last `UpdateState`: the version it updates to and its state.
    pub update: Option<(String, u8)>,
    pub tx: LinkTx<RealmToNode>,
    conn: QuicConn,
}

/// How long a game server refused for its version stays in `nodes.json`
/// as refused (its updater fetches the main server's version and links
/// again well before).
pub const REFUSED_KEEP: Duration = Duration::from_secs(600);

/// A game server refused for its version (`NodeBuild`).
#[derive(Clone, Debug)]
struct Refused {
    version: String,
    addr: SocketAddr,
    at: Instant,
}

/// One official game server in `logs/nodes.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeRow {
    pub id: String,
    pub name: String,
    /// `linked`, `downloading`, `ready`, `failed`, `restarting` (an
    /// upgrade's states, linked), `refused` (another version) or `offline`.
    pub state: &'static str,
    /// The version it runs: the main server's when linked, its own when
    /// refused, empty when offline.
    pub version: String,
    /// The version an upgrade takes it to, while one does.
    pub update: String,
    pub games: u32,
    pub players: u32,
    /// Where its link comes from (linked or refused).
    pub addr: String,
    /// Seconds since it linked, or since it was refused.
    pub since_s: u64,
}

/// The official game servers and which of them are linked.
#[derive(Debug)]
pub struct NodeRegistry {
    list: PathBuf,
    log: Arc<ServerLog>,
    linked: Mutex<BTreeMap<String, Linked>>,
    refused: Mutex<BTreeMap<String, Refused>>,
    next: AtomicU64,
}

impl NodeRegistry {
    pub fn new(list: PathBuf, log: Arc<ServerLog>) -> Self {
        Self {
            list,
            log,
            linked: Mutex::new(BTreeMap::new()),
            refused: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
        }
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, Linked>> {
        self.linked.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The official game server whose certificate has fingerprint `fp`.
    pub fn official(&self, fp: &[u8; 32]) -> Result<Option<NodeEntry>, String> {
        let hex = fingerprint_hex(fp);
        Ok(load(&self.list)?
            .into_iter()
            .find(|n| n.cert_sha256.eq_ignore_ascii_case(&hex)))
    }

    /// `entry` linked from `addr`. A link it had before is closed. Returns
    /// this link's number.
    pub fn linked(
        &self,
        entry: NodeEntry,
        addr: SocketAddr,
        tx: LinkTx<RealmToNode>,
        conn: QuicConn,
    ) -> u64 {
        let link = self.next.fetch_add(1, Ordering::Relaxed);
        let id = entry.id.clone();
        let (max_games, max_players) = (entry.max_games, entry.max_players_total);
        let old = self.lock().insert(
            id.clone(),
            Linked {
                link,
                entry,
                addr,
                load: Load {
                    games: 0,
                    players: 0,
                    cpu_permille: 0,
                    tick_overruns: 0,
                    draining: false,
                },
                since: Instant::now(),
                update: None,
                tx,
                conn,
            },
        );
        self.refused_list().remove(&id);
        if let Some(old) = old {
            old.tx.close();
            old.conn.close(0, "linked again");
            self.log.line(&format!(
                "node {id}: its link from {} is replaced",
                old.addr
            ));
        }
        self.log.line(&format!(
            "node {id} linked from {addr} ({max_games} games, {max_players} players at most)"
        ));
        link
    }

    /// A `Load` of link `link` of game server `id`.
    pub fn load(&self, id: &str, link: u64, load: Load) {
        if let Some(l) = self.lock().get_mut(id).filter(|l| l.link == link) {
            l.load = load;
        }
    }

    /// An `UpdateState` of link `link` of game server `id`.
    pub fn update(&self, id: &str, link: u64, version: &str, state: u8) {
        if let Some(l) = self.lock().get_mut(id).filter(|l| l.link == link) {
            l.update = Some((version.to_owned(), state));
        }
    }

    fn refused_list(&self) -> MutexGuard<'_, BTreeMap<String, Refused>> {
        self.refused.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Official game server `id`, running `version`, was refused from
    /// `addr` for its version (`NodeBuild`).
    pub fn refused_version(&self, id: &str, version: &str, addr: SocketAddr) {
        self.refused_list().insert(
            id.to_owned(),
            Refused {
                version: version.to_owned(),
                addr,
                at: Instant::now(),
            },
        );
    }

    /// Every official game server (the list file) and every linked one,
    /// by id; `version` is the one the main server runs.
    pub fn rows(&self, version: &str, now: Instant) -> Vec<NodeRow> {
        let mut refused = self.refused_list();
        refused.retain(|_, r| now.saturating_duration_since(r.at) < REFUSED_KEEP);
        // An unreadable list still shows the linked ones.
        let mut rows = listed_rows(load(&self.list).unwrap_or_default(), &refused, now);
        drop(refused);
        for l in self.lock().values() {
            let row = linked_row(
                &l.entry,
                l.addr,
                &l.load,
                l.since,
                l.update.as_ref(),
                version,
                now,
            );
            rows.insert(row.id.clone(), row);
        }
        rows.into_values().collect()
    }

    /// Link `link` of game server `id` is over, because of `why`. True
    /// when it was the server's current link (not one replaced since).
    pub fn lost(&self, id: &str, link: u64, why: &str) -> bool {
        let mut linked = self.lock();
        if linked.get(id).is_some_and(|l| l.link == link) {
            if let Some(l) = linked.remove(id) {
                drop(linked);
                l.conn.close(0, "lost");
                self.log.line(&format!("node {id} lost: {why}"));
                return true;
            }
        }
        false
    }

    /// Game server `id`'s link, while it is linked.
    pub fn get(&self, id: &str) -> Option<Linked> {
        self.lock().get(id).cloned()
    }

    /// The linked game servers.
    pub fn linked_now(&self) -> Vec<Linked> {
        self.lock().values().cloned().collect()
    }
}

/// The listed game servers' rows: `offline`, or `refused` (with the
/// version it runs) when it was refused for its version lately.
fn listed_rows(
    listed: Vec<NodeEntry>,
    refused: &BTreeMap<String, Refused>,
    now: Instant,
) -> BTreeMap<String, NodeRow> {
    let mut rows = BTreeMap::new();
    for e in listed {
        let mut row = NodeRow {
            name: super::listing::display_name(&e),
            id: e.id,
            state: "offline",
            version: String::new(),
            update: String::new(),
            games: 0,
            players: 0,
            addr: String::new(),
            since_s: 0,
        };
        if let Some(r) = refused.get(&row.id) {
            row.state = "refused";
            row.version = r.version.clone();
            row.addr = r.addr.to_string();
            row.since_s = now.saturating_duration_since(r.at).as_secs();
        }
        rows.insert(row.id.clone(), row);
    }
    rows
}

/// A linked game server's row: `linked`, or its upgrade state and the
/// version it updates to. It runs `version`, the main server's (it was
/// let in).
fn linked_row(
    entry: &NodeEntry,
    addr: SocketAddr,
    load: &Load,
    since: Instant,
    update: Option<&(String, u8)>,
    version: &str,
    now: Instant,
) -> NodeRow {
    let (state, to) = match update {
        Some((v, s)) => {
            let state = match *s {
                update_state::DOWNLOADING => "downloading",
                update_state::READY => "ready",
                update_state::FAILED => "failed",
                update_state::RESTARTING => "restarting",
                _ => "linked",
            };
            (
                state,
                if *s == update_state::IDLE {
                    String::new()
                } else {
                    v.clone()
                },
            )
        }
        None => ("linked", String::new()),
    };
    NodeRow {
        id: entry.id.clone(),
        name: super::listing::display_name(entry),
        state,
        version: version.to_owned(),
        update: to,
        games: u32::from(load.games),
        players: u32::from(load.players),
        addr: addr.to_string(),
        since_s: now.saturating_duration_since(since).as_secs(),
    }
}

/// The file in the log folder that [`nodes_json`] is written to.
pub const NODES_FILE: &str = "nodes.json";

/// `logs/nodes.json`: the main server's version and one game server per
/// line (so `server-ctl nodes` reads it without a JSON tool).
///
/// ```text
/// {"version": "0.1.1.5", "nodes": [
/// {"id": "eu-1", "name": "Europe 1", "state": "linked", "version": "0.1.1.5", "update": "", "games": 2, "players": 5, "addr": "203.0.113.20:6113", "since_s": 3600},
/// {"id": "eu-2", "name": "Europe 2", "state": "offline", "version": "", "update": "", "games": 0, "players": 0, "addr": "", "since_s": 0}
/// ]}
/// ```
pub fn nodes_json(version: &str, rows: &[NodeRow]) -> String {
    let mut s = format!("{{\"version\": {}, \"nodes\": [\n", json_str(version));
    for (i, r) in rows.iter().enumerate() {
        s.push_str(&format!(
            "{{\"id\": {}, \"name\": {}, \"state\": \"{}\", \"version\": {}, \"update\": {}, \"games\": {}, \"players\": {}, \"addr\": {}, \"since_s\": {}}}{}\n",
            json_str(&r.id),
            json_str(&r.name),
            r.state,
            json_str(&r.version),
            json_str(&r.update),
            r.games,
            r.players,
            json_str(&r.addr),
            r.since_s,
            if i + 1 < rows.len() { "," } else { "" }
        ));
    }
    s.push_str("]}\n");
    s
}

/// A JSON string of `s`.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Writes `logs/nodes.json` (a temporary file renamed over it).
pub fn write_nodes_json(logs: &Path, json: &str) -> std::io::Result<()> {
    let tmp = logs.join(format!("{NODES_FILE}.tmp"));
    std::fs::write(&tmp, json)?;
    std::fs::rename(tmp, logs.join(NODES_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, cert: u8) -> NodeEntry {
        NodeEntry {
            id: id.into(),
            name: format!("Server {id}"),
            location: "Helsinki".into(),
            display_name: String::new(),
            game: "d2rust".into(),
            cert_sha256: fingerprint_hex(&[cert; 32]),
            public_addr: String::new(),
            max_games: 16,
            max_players_total: 128,
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("d2rust-realm-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("data").join("nodes.toml")
    }

    #[test]
    fn the_list_round_trips_and_an_id_is_replaced() {
        let path = temp("list");
        assert_eq!(load(&path).unwrap(), vec![], "no file: no servers");
        upsert(&path, entry("eu-1", 1)).unwrap();
        upsert(&path, entry("eu-2", 2)).unwrap();
        let mut moved = entry("eu-1", 3);
        moved.public_addr = "203.0.113.20:6113".into();
        upsert(&path, moved.clone()).unwrap();
        assert_eq!(load(&path).unwrap(), vec![entry("eu-2", 2), moved]);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# The official game servers"), "{text}");
        assert!(text.contains("[[node]]"), "{text}");
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn a_bad_list_is_refused() {
        let path = temp("bad");
        save(&path, &[entry("eu-1", 1), entry("eu-1", 2)]).unwrap();
        assert!(load(&path).unwrap_err().contains("listed twice"));
        save(&path, &[entry("eu-1", 1), entry("eu-2", 1)]).unwrap();
        assert!(load(&path).unwrap_err().contains("another game server"));
        let mut bad = entry("EU 1", 1);
        save(&path, std::slice::from_ref(&bad)).unwrap();
        assert!(load(&path).unwrap_err().contains("id"));
        bad = entry("eu-1", 1);
        bad.cert_sha256 = "00".into();
        save(&path, &[bad]).unwrap();
        assert!(load(&path).unwrap_err().contains("cert_sha256"));
        std::fs::write(&path, "[[node]]\nid = 1\n").unwrap();
        assert!(load(&path).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn a_display_name_is_set_without_touching_the_rest() {
        let path = temp("rename");
        upsert(&path, entry("n1", 1)).unwrap();
        upsert(&path, entry("n2", 2)).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        assert!(!before.contains("display_name"), "{before}");
        let n2 = rename(&path, "n2", "  JP South ").unwrap();
        assert_eq!(n2.display_name, "JP South");
        let mut want = entry("n2", 2);
        want.display_name = "JP South".into();
        assert_eq!(load(&path).unwrap(), vec![entry("n1", 1), want]);
        // Again: replaced.
        rename(&path, "n2", "Japan").unwrap();
        assert_eq!(load(&path).unwrap()[1].display_name, "Japan");
        // Refused, and the file stays as it was.
        let kept = std::fs::read_to_string(&path).unwrap();
        assert!(rename(&path, "n9", "Finland")
            .unwrap_err()
            .contains("no game server"));
        let long = "x".repeat(MAX_DISPLAY_NAME + 1);
        for bad in ["", "   ", "Fin\tland", long.as_str()] {
            assert!(rename(&path, "n1", bad).is_err(), "{bad:?}");
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), kept);
        assert!(!path.with_extension("toml.tmp").exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn a_certificate_finds_its_server() {
        let path = temp("find");
        upsert(&path, entry("eu-1", 1)).unwrap();
        let log = Arc::new(ServerLog::open(&path.parent().unwrap().join("logs")).unwrap());
        let reg = NodeRegistry::new(path.clone(), log);
        assert_eq!(reg.official(&[1; 32]).unwrap(), Some(entry("eu-1", 1)));
        assert_eq!(reg.official(&[9; 32]).unwrap(), None);
        // Added while running: found at once.
        upsert(&path, entry("eu-2", 9)).unwrap();
        assert_eq!(
            reg.official(&[9; 32]).unwrap().map(|e| e.id),
            Some("eu-2".into())
        );
        assert!(reg.linked_now().is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    fn load_of(games: u16, players: u16) -> Load {
        Load {
            games,
            players,
            cpu_permille: 0,
            tick_overruns: 0,
            draining: false,
        }
    }

    #[test]
    fn listed_servers_are_offline_or_refused_for_their_version() {
        let t0 = Instant::now();
        let now = t0 + Duration::from_secs(5);
        let mut refused = BTreeMap::new();
        let addr: SocketAddr = "203.0.113.20:6113".parse().unwrap();
        refused.insert(
            "eu-2".to_owned(),
            Refused {
                version: "0.0.0.1".into(),
                addr,
                at: t0,
            },
        );
        let rows = listed_rows(vec![entry("eu-2", 2), entry("eu-1", 1)], &refused, now);
        let rows: Vec<NodeRow> = rows.into_values().collect();
        assert_eq!(rows[0].id, "eu-1", "by id");
        assert_eq!(rows[0].state, "offline");
        assert_eq!(rows[0].version, "");
        assert_eq!(rows[0].name, "Helsinki", "the name players see");
        assert_eq!(rows[1].state, "refused");
        assert_eq!(rows[1].version, "0.0.0.1", "the version it runs");
        assert_eq!(rows[1].addr, "203.0.113.20:6113");
        assert_eq!(rows[1].since_s, 5);
    }

    #[test]
    fn a_linked_row_shows_its_upgrade_state() {
        let t0 = Instant::now();
        let now = t0 + Duration::from_secs(90);
        let addr: SocketAddr = "198.51.100.7:6113".parse().unwrap();
        let e = entry("eu-1", 1);
        let row = |u: Option<(String, u8)>| {
            linked_row(&e, addr, &load_of(2, 5), t0, u.as_ref(), "0.1.1.5", now)
        };
        let r = row(None);
        assert_eq!(
            (r.state, r.version.as_str(), r.update.as_str()),
            ("linked", "0.1.1.5", "")
        );
        assert_eq!((r.games, r.players, r.since_s), (2, 5, 90));
        assert_eq!(r.addr, "198.51.100.7:6113");
        let to = |s: u8| {
            let r = row(Some(("0.1.2.0".into(), s)));
            (r.state, r.update)
        };
        assert_eq!(to(update_state::IDLE), ("linked", String::new()));
        assert_eq!(
            to(update_state::DOWNLOADING),
            ("downloading", "0.1.2.0".into())
        );
        assert_eq!(to(update_state::READY), ("ready", "0.1.2.0".into()));
        assert_eq!(to(update_state::FAILED), ("failed", "0.1.2.0".into()));
        assert_eq!(
            to(update_state::RESTARTING),
            ("restarting", "0.1.2.0".into())
        );
    }

    #[test]
    fn a_refusal_is_shown_until_it_is_old() {
        let path = temp("refused");
        upsert(&path, entry("eu-1", 1)).unwrap();
        upsert(&path, entry("eu-2", 2)).unwrap();
        let log = Arc::new(ServerLog::open(&path.parent().unwrap().join("logs")).unwrap());
        let reg = NodeRegistry::new(path.clone(), log);
        let addr: SocketAddr = "127.0.0.1:6113".parse().unwrap();
        reg.refused_version("eu-2", "0.0.0.1", addr);
        let now = Instant::now();
        let states = |at: Instant| {
            reg.rows("0.1.1.5", at)
                .into_iter()
                .map(|r| (r.id, r.state, r.version))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            states(now),
            vec![
                ("eu-1".into(), "offline", String::new()),
                ("eu-2".into(), "refused", "0.0.0.1".into())
            ]
        );
        assert_eq!(
            states(now + REFUSED_KEEP + Duration::from_secs(1))[1],
            ("eu-2".into(), "offline", String::new())
        );
        assert!(reg.refused_list().is_empty(), "an old refusal is dropped");
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn nodes_json_has_one_server_per_line() {
        let mut a = NodeRow {
            id: "eu-1".into(),
            name: "Europe \"1\" \\ Helsinki".into(),
            state: "downloading",
            version: "0.1.1.5".into(),
            update: "0.1.2.0".into(),
            games: 2,
            players: 5,
            addr: "203.0.113.20:6113".into(),
            since_s: 3600,
        };
        let b = NodeRow {
            id: "eu-2".into(),
            name: "Europe 2\tnew".into(),
            state: "offline",
            version: String::new(),
            update: String::new(),
            games: 0,
            players: 0,
            addr: String::new(),
            since_s: 0,
        };
        let json = nodes_json("0.1.1.5", &[a.clone(), b]);
        let lines: Vec<&str> = json.lines().collect();
        assert_eq!(lines.len(), 4, "{json}");
        assert_eq!(lines[0], r#"{"version": "0.1.1.5", "nodes": ["#);
        assert_eq!(
            lines[1],
            r#"{"id": "eu-1", "name": "Europe \"1\" \\ Helsinki", "state": "downloading", "version": "0.1.1.5", "update": "0.1.2.0", "games": 2, "players": 5, "addr": "203.0.113.20:6113", "since_s": 3600},"#
        );
        assert_eq!(
            lines[2],
            r#"{"id": "eu-2", "name": "Europe 2\u0009new", "state": "offline", "version": "", "update": "", "games": 0, "players": 0, "addr": "", "since_s": 0}"#
        );
        assert_eq!(lines[3], "]}");
        a.name = "Ünïcode".into();
        assert!(
            nodes_json("v", &[a]).contains(r#""name": "Ünïcode""#),
            "UTF-8 as is"
        );
        assert_eq!(
            nodes_json("v", &[]),
            "{\"version\": \"v\", \"nodes\": [\n]}\n"
        );
    }
}
