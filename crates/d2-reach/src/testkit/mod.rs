//! Test helpers, also for the multiplayer test tool (feature `testkit`):
//! an in-process simulated network with NAT routers ([`sim`]) and a fake
//! home router that answers UPnP-IGD, NAT-PMP and PCP on loopback
//! ([`fake_gateway`]). Nothing here touches the real network.

pub mod fake_gateway;
pub mod sim;
