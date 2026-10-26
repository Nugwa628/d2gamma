//! PCP (RFC 6887) and NAT-PMP (RFC 6886) through `crab_nat`, sent to each
//! default gateway in turn. PCP goes first; NAT-PMP is tried when PCP got
//! any answer other than a mapping (a router that only speaks NAT-PMP may
//! answer PCP with garbage), but not when nothing answered at all.

use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU16;

use crab_nat::{
    natpmp, pcp, InternetProtocol, MappingFailure, PortMapping, PortMappingOptions,
    PortMappingType, TimeoutConfig, RECOMMENDED_MAPPING_LIFETIME_SECONDS,
};

use super::{Attempt, Mapping, PmpConfig, PortMapConfig};
use crate::gateway::{default_gateways, local_ip_toward};
use crate::ReachMethod;

pub(super) struct Held {
    pub(super) mapping: Mapping,
    port_mapping: PortMapping,
    timeouts: TimeoutConfig,
}

fn timeouts(pmp: &PmpConfig) -> TimeoutConfig {
    TimeoutConfig {
        initial_timeout: pmp.initial_timeout,
        max_retries: pmp.retries,
        max_retry_timeout: None,
    }
}

pub(super) async fn map(config: &PortMapConfig, pmp: &PmpConfig) -> Result<Held, Vec<Attempt>> {
    let gateways: Vec<IpAddr> = match &pmp.gateways {
        Some(g) => g.clone(),
        None => default_gateways().into_iter().map(IpAddr::V4).collect(),
    };
    if gateways.is_empty() {
        return Err(vec![Attempt::new(
            ReachMethod::NatPmp,
            None,
            "no default gateway",
        )]);
    }
    let Some(port) = NonZeroU16::new(config.internal_port) else {
        return Err(vec![Attempt::new(ReachMethod::NatPmp, None, "port 0")]);
    };
    let timeouts = timeouts(pmp);
    // A NAT-PMP lease of 0 deletes: a permanent request asks the default.
    let lifetime = match config.lease_s {
        0 => RECOMMENDED_MAPPING_LIFETIME_SECONDS,
        s => s,
    };
    let options = PortMappingOptions {
        external_port: Some(port),
        lifetime_seconds: Some(lifetime),
        timeout_config: Some(timeouts),
    };

    let mut attempts = Vec::new();
    for gw in gateways {
        let client = match config.local_ip {
            Some(ip) => ip,
            None => match local_ip_toward(gw) {
                Ok(ip) => ip,
                Err(e) => {
                    attempts.push(Attempt::new(ReachMethod::Pcp, Some(gw), e));
                    continue;
                }
            },
        };
        let made = match PortMapping::new(gw.into(), client, InternetProtocol::Udp, port, options)
            .await
        {
            Ok(m) => Some(m),
            Err(MappingFailure::Pcp(pcp::Failure::Timeout)) => {
                attempts.push(Attempt::new(ReachMethod::Pcp, Some(gw), "no answer"));
                None
            }
            Err(MappingFailure::Pcp(e)) => {
                attempts.push(Attempt::new(ReachMethod::Pcp, Some(gw), e));
                match natpmp::port_mapping(gw.into(), InternetProtocol::Udp, port, options).await {
                    Ok(m) => Some(m),
                    Err(e) => {
                        attempts.push(Attempt::new(ReachMethod::NatPmp, Some(gw), e));
                        None
                    }
                }
            }
            Err(MappingFailure::NatPmp(e)) => {
                attempts.push(Attempt::new(ReachMethod::NatPmp, Some(gw), e));
                None
            }
        };
        let Some(port_mapping) = made else {
            continue;
        };
        match held(port_mapping, client, gw, timeouts).await {
            Ok(h) => return Ok(h),
            Err(a) => attempts.push(a),
        }
    }
    Err(attempts)
}

/// The mapping with its external address: PCP's answer carries it, NAT-PMP
/// asks for it separately. Removed again when that fails.
async fn held(
    port_mapping: PortMapping,
    client: IpAddr,
    gw: IpAddr,
    timeouts: TimeoutConfig,
) -> Result<Held, Attempt> {
    let (method, external_ip) = match port_mapping.mapping_type() {
        PortMappingType::Pcp { external_ip, .. } => (ReachMethod::Pcp, external_ip),
        PortMappingType::NatPmp => {
            match natpmp::external_address(gw.into(), Some(timeouts)).await {
                Ok(ip) => (ReachMethod::NatPmp, IpAddr::V4(ip)),
                Err(e) => {
                    let _ = port_mapping.try_drop().await;
                    return Err(Attempt::new(
                        ReachMethod::NatPmp,
                        Some(gw),
                        format_args!("no external address ({e})"),
                    ));
                }
            }
        }
    };
    let mapping = Mapping {
        method,
        external: SocketAddr::new(external_ip, port_mapping.external_port().get()),
        internal: SocketAddr::new(client, port_mapping.internal_port().get()),
        lease_s: port_mapping.lifetime(),
        gateway: gw,
    };
    Ok(Held {
        mapping,
        port_mapping,
        timeouts,
    })
}

impl Held {
    pub(super) async fn renew(&mut self) -> Result<(), Attempt> {
        let m = self.mapping;
        self.port_mapping.timeout_config = self.timeouts;
        self.port_mapping.renew().await.map_err(|e| {
            Attempt::new(
                m.method,
                Some(m.gateway),
                format_args!("renewal refused: {e}"),
            )
        })?;
        let pm = &self.port_mapping;
        self.mapping.external.set_port(pm.external_port().get());
        if let PortMappingType::Pcp { external_ip, .. } = pm.mapping_type() {
            self.mapping.external.set_ip(external_ip);
        }
        self.mapping.lease_s = pm.lifetime();
        Ok(())
    }

    pub(super) async fn remove(self) -> bool {
        self.port_mapping.try_drop().await.is_ok()
    }
}
