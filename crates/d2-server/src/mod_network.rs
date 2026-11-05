//! Declared mod values and server actions on the existing mod envelope.
use crate::{file_source::FileSource, ui_actions::UiActions};
use d2_modkit::net::Definition;
use d2_proto::{
    envelope::{self, Direction},
    mod_ui,
};
use d2_sim::{rules::MessageHost, units::UnitId};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct Network {
    identity: [u8; 32],
    label: String,
    sessions: BTreeMap<UnitId, Session>,
    budgets: limits::Budgets,
    cursors: BTreeMap<UnitId, usize>,
    kicks: BTreeMap<UnitId, String>,

    pub definition: Definition,
    actions: UiActions,
    legacy: UiActions,
    sources: Vec<(u16, Source)>,
    variables: BTreeMap<(UnitId, String), i32>,
    last: BTreeMap<(UnitId, u16), i32>,
}
#[derive(Default)]
struct Session {
    ready: bool,
    pending_ack: bool,
    started: i32,
}
pub fn identity(source: &dyn FileSource, definition: &Definition) -> [u8; 32] {
    d2_modkit::net::fingerprint(definition, &source.mod_versions())
}
enum Source {
    Stat(u16),
    State(u16),
    Variable(String),
}
pub fn definition(source: &dyn FileSource) -> Result<Definition, String> {
    source
        .net_document()?
        .map(|b| Definition::parse(std::str::from_utf8(&b).map_err(|e| e.to_string())?))
        .transpose()
        .map(|d| d.unwrap_or_default())
}
impl Network {
    pub fn load(source: &dyn FileSource) -> Result<Self, String> {
        let definition = definition(source)?;
        let actions = UiActions::declared(
            source,
            definition
                .actions
                .iter()
                .map(|a| (a.rule.name.clone(), a.rule.clone()))
                .collect(),
        )?;
        let mut sources = Vec::new();
        let (mut stats, mut states) = (None, None);
        for value in &definition.values {
            let (kind, name) = value
                .source
                .split_once('.')
                .ok_or("invalid network source")?;
            let source = match kind {
                "stat" => Source::Stat(resolve(source, &mut stats, "itemstatcost", "stat", name)?),
                "state" => Source::State(resolve(source, &mut states, "states", "state", name)?),
                "var" => Source::Variable(name.into()),
                _ => return Err("invalid network source".into()),
            };
            sources.push((value.id, source));
        }
        let identity = identity(source, &definition);
        let label = source
            .mod_versions()
            .into_iter()
            .map(|(id, v)| format!("{id} {v}"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut safe = String::new();
        for c in label.chars().filter(|c| !c.is_control()) {
            if safe.len() + c.len_utf8() > 180 {
                break;
            }
            safe.push(c);
        }
        let label = if safe.is_empty() {
            "network schema".into()
        } else {
            safe
        };
        Ok(Self {
            identity,
            label,
            definition,
            actions,
            sources,
            ..Default::default()
        })
    }

    pub fn ready(&self, p: UnitId) -> bool {
        !self.kicks.contains_key(&p)
            && (self.definition.empty() || self.sessions.get(&p).is_some_and(|s| s.ready))
    }
    pub fn incoming(&mut self, p: UnitId, frame: i32, hz: u32, payload: &[u8]) -> bool {
        let id = if mod_ui::read_net_identity(payload, false).is_some() {
            Some(0)
        } else if mod_ui::read_action(payload).is_some() {
            Some(u16::MAX)
        } else {
            mod_ui::read_server_action(payload)
                .filter(|id| self.definition.actions.iter().any(|a| a.id == *id))
        };
        self.budgets.incoming(p, frame, hz, payload.len(), id)
    }
    pub fn hello(&mut self, p: UnitId, frame: i32, hz: u32, payload: &[u8]) -> Option<Vec<u8>> {
        let received = mod_ui::read_net_identity(payload, false)?;
        if self.definition.empty() {
            self.kicks.insert(p,format!("This mod needs updating: server {} has no declared network contract. Install the server's mod version and rejoin.",self.label));
            return None;
        }
        let session = self.sessions.entry(p).or_insert(Session {
            started: frame,
            ..Default::default()
        });
        if received != self.identity {
            session.ready = false;
            self.kicks.insert(p,format!("This mod needs updating: server requires {} and the same network schema. Rejoin with the server's mod version.",self.label));
            return None;
        }
        if !self.kicks.contains_key(&p) {
            session.ready = true;
            session.pending_ack = true;
            self.budgets
                .outgoing(p, frame, hz, 41, 0)
                .then(|| mod_ui::net_identity(true, &self.identity))
        } else {
            None
        }
    }
    pub fn take_kick(&mut self, p: UnitId, frame: i32, hz: u32) -> Option<String> {
        if let Some(reason) = self.kicks.remove(&p) {
            return Some(reason);
        }
        if self.definition.empty() {
            return None;
        }
        let session = self.sessions.entry(p).or_insert(Session {
            started: frame,
            ..Default::default()
        });
        if !session.ready && frame.wrapping_sub(session.started) > 5 * hz as i32 {
            self.kicks.entry(p).or_insert_with(||"This mod needs updating: network version handshake missing. Install the server's mod version and rejoin.".into());
        }
        self.kicks.remove(&p)
    }
    pub fn restart(&mut self, p: UnitId) {
        self.last.retain(|(u, _), _| *u != p);
        self.sessions.remove(&p);
        self.budgets.forget(p);
        self.cursors.remove(&p);
        self.kicks.remove(&p);
    }

    pub fn set_legacy(&mut self, actions: UiActions) {
        self.legacy = actions;
    }
    /// Claims malformed service messages, so they cannot reach a custom hook.
    pub fn dispatch(
        &mut self,
        payload: &[u8],
        player: UnitId,
        frame: i32,
        h: &mut dyn MessageHost,
        scripts: Option<&crate::scripts::ScriptHost>,
    ) -> bool {
        if !mod_ui::claims(payload) {
            return false;
        }
        if self.ready(player) {
            match mod_ui::read_action_request(payload) {
                Some(mod_ui::ActionRequest::Declared(id)) => {
                    self.execute(id, player, frame, h, scripts);
                }
                Some(mod_ui::ActionRequest::Legacy(key)) => {
                    self.legacy.execute(key, player, frame, h, scripts);
                }
                None => {}
            }
        }
        true
    }
    pub fn validate_scripts(
        &self,
        scripts: Option<&crate::scripts::ScriptHost>,
    ) -> Result<(), String> {
        self.actions.validate_scripts(scripts)?;
        self.legacy.validate_scripts(scripts)
    }
    pub fn observe_variable(&mut self, player: UnitId, name: &str, value: i32) {
        if self
            .sources
            .iter()
            .any(|(_, s)| matches!(s,Source::Variable(n)if n==name))
        {
            self.variables.insert((player, name.into()), value);
        }
    }
    pub fn values(
        &mut self,
        player: UnitId,
        frame: i32,
        hz: u32,
        h: &dyn MessageHost,
    ) -> Vec<Vec<u8>> {
        let mut messages = Vec::new();
        if !self.ready(player) {
            return messages;
        }
        // ACK must reach the same reliable queue before any value is cached.
        if self.sessions.get(&player).is_some_and(|s| s.pending_ack) {
            if self.budgets.outgoing(player, frame, hz, 41, 0) {
                messages.push(
                    envelope::encode(
                        Direction::ToClient,
                        &mod_ui::net_identity(true, &self.identity),
                    )
                    .expect("bounded identity"),
                );
            }
            return messages;
        }
        let count = self.sources.len();
        let start = self.cursors.get(&player).copied().unwrap_or(0);
        let mut next = start;
        for offset in 0..count {
            let index = (start + offset) % count;
            let (id, source) = &self.sources[index];
            let value = match source {
                Source::Stat(id) => h.stat(player, *id, 0),
                Source::State(id) => i32::from(h.has_state(player, *id)),
                Source::Variable(name) => self
                    .variables
                    .get(&(player, name.clone()))
                    .copied()
                    .unwrap_or(0),
            };
            if self.last.get(&(player, *id)) == Some(&value) {
                continue;
            }
            if !self.budgets.outgoing(player, frame, hz, 15, *id) {
                continue;
            }
            next = (index + 1) % count;
            let payload = mod_ui::synced(*id, value).expect("validated schema id");
            messages.push(
                envelope::encode(Direction::ToClient, &payload).expect("fixed bounded payload"),
            );
        }
        self.cursors.insert(player, next);
        messages
    }
    /// Cache only messages successfully queued, so a full client buffer retries.
    pub fn queued(&mut self, player: UnitId, bytes: &[u8]) {
        if envelope::payload(Direction::ToClient, bytes)
            .and_then(|p| mod_ui::read_net_identity(p, true))
            == Some(self.identity)
        {
            if let Some(session) = self.sessions.get_mut(&player) {
                session.pending_ack = false;
            }
        }

        if let Some((id, value)) =
            envelope::payload(Direction::ToClient, bytes).and_then(mod_ui::read_synced)
        {
            self.last.insert((player, id), value);
        }
    }
    pub fn execute(
        &mut self,
        id: u16,
        player: UnitId,
        frame: i32,
        h: &mut dyn MessageHost,
        scripts: Option<&crate::scripts::ScriptHost>,
    ) -> bool {
        if !self.ready(player) {
            return false;
        }
        let Some(a) = self.definition.actions.iter().find(|a| a.id == id) else {
            return false;
        };
        self.actions
            .execute(&a.rule.name, player, frame, h, scripts)
    }
    pub fn forget(&mut self, player: UnitId) {
        self.restart(player);
        self.variables.retain(|(p, _), _| *p != player);
        self.actions.forget(player);
        self.legacy.forget(player);
    }
}
fn resolve(
    source: &dyn FileSource,
    cache: &mut Option<BTreeMap<String, u16>>,
    table: &str,
    column: &str,
    name: &str,
) -> Result<u16, String> {
    if table == "itemstatcost" {
        if cache.is_none() {
            *cache = Some(crate::ui_actions::stat_names(source)?);
        }
        return crate::ui_actions::resolve_name(cache.as_ref().unwrap(), name)
            .ok_or_else(|| format!("net: unknown {table} name {name}"));
    }
    if cache.is_none() {
        let path = format!("data\\global\\excel\\{table}.txt");
        let bytes = source
            .read_file(&path)
            .ok_or_else(|| format!("net needs {path}"))??;
        let t = d2_data::txt::TxtTable::parse(&path, &bytes).map_err(|e| e.to_string())?;
        let col = t
            .header
            .iter()
            .position(|h| h.eq_ignore_ascii_case(column.as_bytes()))
            .ok_or_else(|| format!("{path}: missing {column}"))?;
        let mut linker = d2_data::compile::NameLinker::default();
        let mut names = BTreeMap::new();
        for row in t.records {
            let key = d2_data::compile::name_key(&row.cells[col]).ok_or("non-ASCII table name")?;
            let id = linker.find_or_add(&key);
            if key.is_empty() {
                continue;
            }
            let id = u16::try_from(id)
                .ok()
                .filter(|id| *id != u16::MAX)
                .ok_or("table id space exhausted")?;
            if names
                .insert(String::from_utf8(key).map_err(|e| e.to_string())?, id)
                .is_some()
            {
                return Err("duplicate compiled name".into());
            }
        }
        *cache = Some(names);
    }
    cache
        .as_ref()
        .unwrap()
        .get(
            &name
                .to_ascii_lowercase()
                .chars()
                .take(31)
                .collect::<String>(),
        )
        .copied()
        .ok_or_else(|| format!("net: unknown {table} name {name}"))
}

mod limits;

#[cfg(test)]
mod tests {
    use super::*;
    struct Files(String);
    impl FileSource for Files {
        fn read_file(&self, _: &str) -> Option<Result<Vec<u8>, String>> {
            Some(Ok(b"Stat\tID\r\nstrength\t99\r\n".to_vec()))
        }
        fn net_document(&self) -> Result<Option<Vec<u8>>, String> {
            Ok(Some(self.0.as_bytes().to_vec()))
        }
    }
    #[test]
    fn network_rejects_unresolved_effective_names_and_missing_script_exports() {
        assert!(Network::load(&Files(
            "version=1\n[[values]]\nid=1\nname='a'\nsource='stat.missing'".into()
        ))
        .is_err());
        let n=Network::load(&Files("version=1\n[[actions]]\nid=1\nname='a'\nstat='strength'\ndelta=1\n[actions.validator]\nmodule='rules.luau'\nfunction='approve'".into())).unwrap();
        assert!(n
            .validate_scripts(None)
            .unwrap_err()
            .contains("unknown validator"));
        assert!(Network::default().validate_scripts(None).is_ok());
    }

    #[test]
    fn network_legacy_action_names_share_one_bounded_rate_bucket() {
        let mut n = Network::default();
        let p = UnitId(1);
        for id in 0..8 {
            assert!(n.incoming(p, 0, 25, &mod_ui::action(&format!("hud/a{id}")).unwrap()));
        }
        assert!(!n.incoming(p, 0, 25, &mod_ui::action("hud/another").unwrap()));
        assert!(n.incoming(UnitId(2), 0, 25, &mod_ui::action("hud/another").unwrap()));
        assert!(n.incoming(p, 25, 25, &mod_ui::action("hud/another").unwrap()));
    }

    #[test]
    fn network_queued_refusal_blocks_legacy_actions_before_the_host_disconnects() {
        let mut n = Network::default();
        let p = UnitId(1);
        assert!(n.ready(p));
        assert!(n
            .hello(p, 0, 25, &mod_ui::net_identity(false, &[1; 32]))
            .is_none());
        assert!(!n.ready(p));
        assert!(n.ready(UnitId(2)));
        assert!(n
            .take_kick(p, 0, 25)
            .unwrap()
            .contains("no declared network contract"));
    }
}
