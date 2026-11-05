//! Client side of the versioned, declared mod network contract.
use crate::bridge::world::ClientWorld;
use d2_modkit::net::Definition;

#[derive(Default)]
pub struct Network {
    pub definition: Definition,
    identity: [u8; 32],
    ready: bool,
    pending_hello: bool,
}
impl Network {
    pub fn new(definition: Definition, identity: [u8; 32]) -> Self {
        Self {
            definition,
            identity,
            ready: false,
            pending_hello: true,
        }
    }
    pub fn hello(&mut self) -> Option<Vec<u8>> {
        self.ready = false;
        self.pending_hello = false;
        (!self.definition.empty()).then(|| d2_proto::mod_ui::net_identity(false, &self.identity))
    }
    pub fn restart(&mut self) {
        self.ready = false;
        self.pending_hello = true;
    }
    pub fn after_join(&mut self, joined: bool) -> Option<Vec<u8>> {
        if joined && self.pending_hello {
            self.hello()
        } else {
            None
        }
    }
    pub fn receive(&mut self, payload: &[u8], world: &mut ClientWorld) -> Result<(), String> {
        if let Some(reason) = d2_proto::mod_ui::read_net_rejection(payload) {
            return Err(reason.into());
        }
        if let Some(hash) = d2_proto::mod_ui::read_net_identity(payload, true) {
            if self.definition.empty() {
                return Ok(());
            }
            if hash != self.identity {
                return Err("This mod needs updating: server network contract differs. Install the server's mod version and rejoin.".into());
            }
            self.ready = true;
            return Ok(());
        }
        if self.ready {
            if let Some((id, value)) = d2_proto::mod_ui::read_synced(payload) {
                if let Some(d) = self.definition.values.iter().find(|v| v.id == id) {
                    world.synced_values.insert(d.name.clone(), value);
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_values_require_handshake_and_accept_only_declared_ids() {
        let mut w = ClientWorld::default();
        let b = d2_proto::mod_ui::synced(1, 42).unwrap();
        Network::default().receive(&b, &mut w).unwrap();
        assert!(w.synced_values.is_empty());
        let mut n = Network {
            definition: Definition::parse(
                "version=1\n[[values]]\nid=1\nname='rage'\nsource='stat.rage'",
            )
            .unwrap(),
            identity: [5; 32],
            ready: false,
            pending_hello: true,
        };
        n.receive(&b, &mut w).unwrap();
        assert!(w.synced_values.is_empty());
        n.receive(&d2_proto::mod_ui::net_identity(true, &[5; 32]), &mut w)
            .unwrap();
        n.receive(&b, &mut w).unwrap();
        assert_eq!(w.synced_values["rage"], 42);
        n.receive(&d2_proto::mod_ui::synced(2, 7).unwrap(), &mut w)
            .unwrap();
        assert_eq!(w.synced_values.len(), 1);
        assert!(n.hello().is_some());
        w.synced_values.clear();
        n.receive(&b, &mut w).unwrap();
        assert!(w.synced_values.is_empty());
    }
    #[test]
    fn network_mismatch_has_a_clear_upgrade_reason_and_default_emits_no_hello() {
        let mut n = Network::default();
        assert!(n.hello().is_none());
        let mut w = ClientWorld::default();
        let reason = "This mod needs updating: server alpha 2.0.0";
        assert_eq!(
            n.receive(&d2_proto::mod_ui::net_rejection(reason).unwrap(), &mut w),
            Err(reason.into())
        );
    }

    #[test]
    fn hello_waits_for_join_and_restarts_once_after_resume() {
        let mut n = Network::new(
            Definition::parse("version=1\n[[values]]\nid=1\nname='rage'\nsource='stat.rage'")
                .unwrap(),
            [5; 32],
        );
        assert!(n.after_join(false).is_none());
        assert!(n.after_join(true).is_some());
        assert!(n.after_join(true).is_none());
        n.restart();
        assert!(n.after_join(false).is_none());
        assert!(n.after_join(true).is_some());
        assert!(n.after_join(true).is_none());
        let mut plain = Network::default();
        assert!(plain.after_join(true).is_none());
        plain.restart();
        assert!(plain.after_join(true).is_none());
    }
}
