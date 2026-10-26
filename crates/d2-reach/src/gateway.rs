//! The default gateway (the home router) and the local address facing it.
//!
//! NAT-PMP and PCP talk to the default gateway directly, so the port mapper
//! needs its address; UPnP finds the router by multicast instead. The
//! internal client of a mapping is the local address the system would use
//! toward the router.
//!
//! On Linux the gateways come from `/proc/net/route`; on Windows from the
//! output of `route print -4 0.0.0.0` (run without a console window). The
//! parsers are plain functions over text so both are tested on any system.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

/// The IPv4 default gateways of this machine, best first; empty when there
/// is none or the system can't tell.
pub fn default_gateways() -> Vec<Ipv4Addr> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/net/route")
            .map(|text| parse_proc_net_route(&text))
            .unwrap_or_default()
    }
    #[cfg(windows)]
    {
        route_print()
            .map(|text| parse_route_print(&text))
            .unwrap_or_default()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
fn route_print() -> io::Result<String> {
    use std::os::windows::process::CommandExt;
    /// `CREATE_NO_WINDOW`: no console flashes up for the game server.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let exe = std::env::var_os("SystemRoot")
        .map(|root| {
            std::path::PathBuf::from(root)
                .join("System32")
                .join("route.exe")
        })
        .unwrap_or_else(|| "route.exe".into());
    let out = std::process::Command::new(exe)
        .args(["print", "-4", "0.0.0.0"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The gateways of the default routes in Linux's `/proc/net/route`, lowest
/// metric first. Addresses are printed as the kernel's native-endian
/// 32-bit value in hex.
pub fn parse_proc_net_route(text: &str) -> Vec<Ipv4Addr> {
    /// `RTF_UP | RTF_GATEWAY`.
    const UP_GATEWAY: u32 = 0x0003;
    let mut found: Vec<(u32, Ipv4Addr)> = Vec::new();
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 8 {
            continue;
        }
        let hex = |s: &str| u32::from_str_radix(s, 16).ok();
        let (Some(dest), Some(gw), Some(flags), Some(mask)) =
            (hex(cols[1]), hex(cols[2]), hex(cols[3]), hex(cols[7]))
        else {
            continue;
        };
        let metric = cols[6].parse().unwrap_or(u32::MAX);
        if dest == 0 && mask == 0 && gw != 0 && flags & UP_GATEWAY == UP_GATEWAY {
            found.push((metric, Ipv4Addr::from(gw.to_ne_bytes())));
        }
    }
    sorted_unique(found)
}

/// The gateways of the default routes in the active route table printed by
/// Windows' `route print -4 0.0.0.0`, lowest metric first. Rows read
/// `0.0.0.0  0.0.0.0  <gateway>  <interface>  <metric>`; on-link rows and
/// the persistent routes (no interface column) are skipped.
pub fn parse_route_print(text: &str) -> Vec<Ipv4Addr> {
    let mut found: Vec<(u32, Ipv4Addr)> = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() != 5 || cols[0] != "0.0.0.0" || cols[1] != "0.0.0.0" {
            continue;
        }
        let (Ok(gw), Ok(_iface), Ok(metric)) = (
            cols[2].parse::<Ipv4Addr>(),
            cols[3].parse::<Ipv4Addr>(),
            cols[4].parse::<u32>(),
        ) else {
            continue;
        };
        if !gw.is_unspecified() {
            found.push((metric, gw));
        }
    }
    sorted_unique(found)
}

fn sorted_unique(mut found: Vec<(u32, Ipv4Addr)>) -> Vec<Ipv4Addr> {
    found.sort_by_key(|(metric, _)| *metric);
    let mut out = Vec::new();
    for (_, gw) in found {
        if !out.contains(&gw) {
            out.push(gw);
        }
    }
    out
}

/// The local address this machine sends from toward `gateway`. A UDP
/// socket is connected (no packet is sent) and asked its address.
pub fn local_ip_toward(gateway: IpAddr) -> io::Result<IpAddr> {
    let any: SocketAddr = match gateway {
        IpAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        IpAddr::V6(_) => (std::net::Ipv6Addr::UNSPECIFIED, 0).into(),
    };
    let socket = UdpSocket::bind(any)?;
    socket.connect((gateway, 9))?;
    Ok(socket.local_addr()?.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table as a little-endian kernel prints it.
    #[test]
    #[cfg(target_endian = "little")]
    fn linux_route_table() {
        let text = "\
Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT
wlan0\t00000000\t0102A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0
eth0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0
eth0\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0
tun0\t00000000\t0101000A\t0001\t0\t0\t50\t00000000\t0\t0\t0
";
        assert_eq!(
            parse_proc_net_route(text),
            vec![Ipv4Addr::new(192, 168, 1, 1), Ipv4Addr::new(192, 168, 2, 1)],
            "default routes with a gateway, lowest metric first"
        );
        assert!(parse_proc_net_route("").is_empty());
        assert!(parse_proc_net_route("header\ngarbage line\n").is_empty());
    }

    #[test]
    fn windows_route_print() {
        let text = "\
===========================================================================
Interface List
 12...00 11 22 33 44 55 ......Ethernet
===========================================================================

IPv4 Route Table
===========================================================================
Active Routes:
Network Destination        Netmask          Gateway       Interface  Metric
          0.0.0.0          0.0.0.0     192.168.0.1    192.168.0.20     35
          0.0.0.0          0.0.0.0      10.20.0.1      10.20.0.7       25
          0.0.0.0          0.0.0.0         On-link      10.99.0.2       5
===========================================================================
Persistent Routes:
  Network Address          Netmask  Gateway Address  Metric
          0.0.0.0          0.0.0.0      172.16.0.1  Default
===========================================================================
";
        assert_eq!(
            parse_route_print(text),
            vec![Ipv4Addr::new(10, 20, 0, 1), Ipv4Addr::new(192, 168, 0, 1)]
        );
        assert!(parse_route_print("no routes here").is_empty());
    }

    #[test]
    fn local_address_toward_loopback() {
        let ip = local_ip_toward(IpAddr::V4(Ipv4Addr::LOCALHOST)).unwrap();
        assert!(ip.is_loopback());
    }
}
