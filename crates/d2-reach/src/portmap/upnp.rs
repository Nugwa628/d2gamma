//! UPnP-IGD through `igd-next`: an SSDP search, then `AddPortMapping` for
//! the same external port. A router that only grants permanent mappings
//! gets lease 0; when the port is taken by another machine the router
//! picks one (`AddAnyPortMapping`, or random ports where it lacks that).

use std::fmt;
use std::net::SocketAddr;

use igd_next::aio::tokio::{search_gateway, Tokio};
use igd_next::aio::Gateway;
use igd_next::{AddAnyPortError, AddPortError, PortMappingProtocol, SearchOptions};

use super::{Attempt, Mapping, PortMapConfig, UpnpConfig};
use crate::gateway::local_ip_toward;
use crate::ReachMethod;

pub(super) struct Held {
    pub(super) mapping: Mapping,
    gateway: Gateway<Tokio>,
}

/// Why the router said no, as far as the next try cares.
enum Refusal {
    PermanentOnly,
    InUse,
    Other(String),
}

impl From<AddPortError> for Refusal {
    fn from(e: AddPortError) -> Self {
        match e {
            AddPortError::OnlyPermanentLeasesSupported => Refusal::PermanentOnly,
            AddPortError::PortInUse => Refusal::InUse,
            e => Refusal::Other(e.to_string()),
        }
    }
}

impl From<AddAnyPortError> for Refusal {
    fn from(e: AddAnyPortError) -> Self {
        match e {
            AddAnyPortError::OnlyPermanentLeasesSupported => Refusal::PermanentOnly,
            e => Refusal::Other(e.to_string()),
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::PermanentOnly => f.write_str("only permanent mappings"),
            Refusal::InUse => f.write_str("port in use"),
            Refusal::Other(s) => f.write_str(s),
        }
    }
}

pub(super) async fn map(config: &PortMapConfig, upnp: &UpnpConfig) -> Result<Held, Vec<Attempt>> {
    let fail = |gw, why: &dyn fmt::Display| vec![Attempt::new(ReachMethod::UpnpIgd, gw, why)];
    let mut options = SearchOptions::default();
    options.bind_addr = upnp.bind_addr;
    options.broadcast_address = upnp.search_addr;
    options.timeout = Some(upnp.timeout);
    options.single_search_timeout = Some(upnp.timeout);
    options.allowed_gateway_ips = upnp.trusted.clone();
    let gateway = search_gateway(options)
        .await
        .map_err(|e| fail(None, &format_args!("no router answered ({e})")))?;
    let gw_ip = gateway.addr.ip();
    let local_ip = match config.local_ip {
        Some(ip) => ip,
        None => local_ip_toward(gw_ip).map_err(|e| fail(Some(gw_ip), &e))?,
    };
    let internal = SocketAddr::new(local_ip, config.internal_port);

    let mut lease = config.lease_s;
    let mut any_port = false;
    let external_port = loop {
        let got: Result<u16, Refusal> = if any_port {
            gateway
                .add_any_port(
                    PortMappingProtocol::UDP,
                    internal,
                    lease,
                    &config.description,
                )
                .await
                .map_err(Refusal::from)
        } else {
            gateway
                .add_port(
                    PortMappingProtocol::UDP,
                    config.internal_port,
                    internal,
                    lease,
                    &config.description,
                )
                .await
                .map(|()| config.internal_port)
                .map_err(Refusal::from)
        };
        match got {
            Ok(port) => break port,
            Err(Refusal::PermanentOnly) if lease != 0 => lease = 0,
            Err(Refusal::InUse) if !any_port => any_port = true,
            Err(why) => return Err(fail(Some(gw_ip), &format_args!("refused: {why}"))),
        }
    };

    let external_ip = match gateway.get_external_ip().await {
        Ok(ip) => ip,
        Err(e) => {
            let _ = gateway
                .remove_port(PortMappingProtocol::UDP, external_port)
                .await;
            return Err(fail(
                Some(gw_ip),
                &format_args!("no external address ({e})"),
            ));
        }
    };
    Ok(Held {
        mapping: Mapping {
            method: ReachMethod::UpnpIgd,
            external: SocketAddr::new(external_ip, external_port),
            internal,
            lease_s: lease,
            gateway: gw_ip,
        },
        gateway,
    })
}

impl Held {
    /// UPnP has no renewal: the same mapping is added again.
    pub(super) async fn renew(&mut self, config: &PortMapConfig) -> Result<(), Attempt> {
        let m = self.mapping;
        self.gateway
            .add_port(
                PortMappingProtocol::UDP,
                m.external.port(),
                m.internal,
                m.lease_s,
                &config.description,
            )
            .await
            .map_err(|e| {
                Attempt::new(
                    ReachMethod::UpnpIgd,
                    Some(m.gateway),
                    format_args!("renewal refused: {e}"),
                )
            })
    }

    pub(super) async fn remove(self) -> bool {
        self.gateway
            .remove_port(PortMappingProtocol::UDP, self.mapping.external.port())
            .await
            .is_ok()
    }
}
