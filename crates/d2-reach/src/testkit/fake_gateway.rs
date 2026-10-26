//! A fake home router on loopback, for the port mapper's tests: a UPnP-IGD
//! device ([`FakeUpnp`]: SSDP answers, the device and service descriptions
//! and SOAP control over HTTP) and a NAT-PMP/PCP server ([`FakePmp`]).
//! Plain threads and sockets on 127.0.0.0/8; it records every request so
//! tests can check what the mapper asked for.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const SERVICE: &str = "urn:schemas-upnp-org:service:WANIPConnection:1";
const POLL: Duration = Duration::from_millis(50);
/// NAT-PMP's and PCP's fixed server port.
const PMP_PORT: u16 = 5351;

/// A port forward the fake router holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FakeMapping {
    /// The external port.
    pub external_port: u16,
    /// Where it forwards to.
    pub internal: SocketAddr,
    /// The lease granted, in seconds; 0 is permanent.
    pub lease_s: u32,
}

/// How the fake UPnP router behaves.
#[derive(Clone, Debug)]
pub struct UpnpBehaviour {
    /// The external address it reports.
    pub external_ip: Ipv4Addr,
    /// Refuses every mapping (606, not authorized).
    pub refuse: bool,
    /// Only grants lease 0 (725 for any other lease).
    pub permanent_only: bool,
    /// External ports taken by another machine (718); `AddAnyPortMapping`
    /// then hands out the next free port from 40000.
    pub taken: Vec<u16>,
}

impl Default for UpnpBehaviour {
    fn default() -> Self {
        Self {
            external_ip: Ipv4Addr::new(203, 0, 113, 7),
            refuse: false,
            permanent_only: false,
            taken: Vec::new(),
        }
    }
}

/// A SOAP request the fake UPnP router got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpnpRequest {
    /// The action (`AddPortMapping`, ...).
    pub action: String,
    /// Its arguments by name.
    pub args: BTreeMap<String, String>,
}

#[derive(Default)]
struct UpnpState {
    requests: Vec<UpnpRequest>,
    mappings: BTreeMap<u16, FakeMapping>,
}

/// A UPnP-IGD router on loopback.
pub struct FakeUpnp {
    ssdp: SocketAddr,
    state: Arc<Mutex<UpnpState>>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl FakeUpnp {
    /// Starts the router's SSDP and HTTP servers on free loopback ports.
    pub fn start(behaviour: UpnpBehaviour) -> io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(UpnpState::default()));

        let http = TcpListener::bind("127.0.0.1:0")?;
        http.set_nonblocking(true)?;
        let http_addr = http.local_addr()?;
        let ssdp = UdpSocket::bind("127.0.0.1:0")?;
        ssdp.set_read_timeout(Some(POLL))?;
        let ssdp_addr = ssdp.local_addr()?;

        let mut threads = Vec::new();
        let s = stop.clone();
        threads.push(std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while !s.load(Ordering::Relaxed) {
                let Ok((n, from)) = ssdp.recv_from(&mut buf) else {
                    continue;
                };
                if buf[..n].starts_with(b"M-SEARCH") {
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=120\r\n\
                         ST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\
                         USN: uuid:fake-router::urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\
                         EXT:\r\nSERVER: fake/1.0 UPnP/1.0\r\n\
                         LOCATION: http://{http_addr}/root.xml\r\n\r\n"
                    );
                    let _ = ssdp.send_to(reply.as_bytes(), from);
                }
            }
        }));
        let (s, st) = (stop.clone(), state.clone());
        threads.push(std::thread::spawn(move || {
            while !s.load(Ordering::Relaxed) {
                match http.accept() {
                    Ok((conn, _)) => {
                        let _ = serve_http(conn, &behaviour, &st);
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        }));
        Ok(Self {
            ssdp: ssdp_addr,
            state,
            stop,
            threads,
        })
    }

    /// Where the SSDP search goes (`UpnpConfig::search_addr`).
    pub fn search_addr(&self) -> SocketAddr {
        self.ssdp
    }

    /// The SOAP requests so far.
    pub fn requests(&self) -> Vec<UpnpRequest> {
        self.state.lock().expect("state").requests.clone()
    }

    /// The SOAP actions so far, in order.
    pub fn actions(&self) -> Vec<String> {
        self.requests().into_iter().map(|r| r.action).collect()
    }

    /// The forwards held now.
    pub fn mappings(&self) -> Vec<FakeMapping> {
        let st = self.state.lock().expect("state");
        st.mappings.values().copied().collect()
    }
}

impl Drop for FakeUpnp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

fn serve_http(mut conn: TcpStream, b: &UpnpBehaviour, st: &Mutex<UpnpState>) -> io::Result<()> {
    conn.set_nonblocking(false)?;
    conn.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    let head_end = loop {
        let n = conn.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(i) = find(&data, b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
    let header = |name: &str| {
        head.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case(name)
                .then(|| v.trim().to_string())
        })
    };
    let length: usize = header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    while data.len() < head_end + length {
        let n = conn.read(&mut buf)?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
    }
    let body = String::from_utf8_lossy(&data[head_end..]).into_owned();
    let path = head.split_whitespace().nth(1).unwrap_or("");

    let (status, reply) = if head.starts_with("GET") && path == "/root.xml" {
        ("200 OK", ROOT_XML.to_string())
    } else if head.starts_with("GET") && path == "/scpd.xml" {
        ("200 OK", scpd_xml())
    } else if head.starts_with("POST") && path == "/ctl" {
        let action = header("soapaction")
            .and_then(|v| {
                let v = v.trim_matches('"');
                v.split_once('#').map(|(_, a)| a.to_string())
            })
            .unwrap_or_default();
        soap(&action, &body, b, st)
    } else {
        ("404 Not Found", String::new())
    };
    let out = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/xml; charset=\"utf-8\"\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    );
    conn.write_all(out.as_bytes())?;
    conn.flush()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The text of `<name>...</name>` in `body`.
fn arg(body: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = body.find(&open)? + open.len();
    let end = body[start..].find(&format!("</{name}>"))? + start;
    Some(body[start..end].to_string())
}

const ARGS: [&str; 9] = [
    "NewRemoteHost",
    "NewExternalPort",
    "NewProtocol",
    "NewInternalPort",
    "NewInternalClient",
    "NewEnabled",
    "NewPortMappingDescription",
    "NewLeaseDuration",
    "NewReservedPort",
];

fn soap(
    action: &str,
    body: &str,
    b: &UpnpBehaviour,
    st: &Mutex<UpnpState>,
) -> (&'static str, String) {
    let args: BTreeMap<String, String> = ARGS
        .iter()
        .filter_map(|n| arg(body, n).map(|v| (n.to_string(), v)))
        .collect();
    let mut st = st.lock().expect("state");
    st.requests.push(UpnpRequest {
        action: action.to_string(),
        args: args.clone(),
    });
    let num = |n: &str| args.get(n).and_then(|v| v.parse::<u32>().ok());
    let mapping = |port: u16| -> Option<FakeMapping> {
        let ip: IpAddr = args.get("NewInternalClient")?.parse().ok()?;
        let internal = u16::try_from(num("NewInternalPort")?).ok()?;
        Some(FakeMapping {
            external_port: port,
            internal: SocketAddr::new(ip, internal),
            lease_s: num("NewLeaseDuration")?,
        })
    };
    match action {
        "GetExternalIPAddress" => ok(
            action,
            &format!(
                "<NewExternalIPAddress>{}</NewExternalIPAddress>",
                b.external_ip
            ),
        ),
        "AddPortMapping" | "AddAnyPortMapping" => {
            if b.refuse {
                return fault(606, "Action not authorized");
            }
            if b.permanent_only && num("NewLeaseDuration") != Some(0) {
                return fault(725, "OnlyPermanentLeasesSupported");
            }
            let wanted = num("NewExternalPort").and_then(|p| u16::try_from(p).ok());
            let port = if action == "AddPortMapping" {
                match wanted {
                    Some(p) if b.taken.contains(&p) => return fault(718, "ConflictInMappingEntry"),
                    Some(p) => p,
                    None => return fault(402, "Invalid Args"),
                }
            } else {
                (40000..u16::MAX)
                    .find(|p| !b.taken.contains(p) && !st.mappings.contains_key(p))
                    .unwrap_or(40000)
            };
            let Some(m) = mapping(port) else {
                return fault(402, "Invalid Args");
            };
            st.mappings.insert(port, m);
            if action == "AddPortMapping" {
                ok(action, "")
            } else {
                ok(
                    action,
                    &format!("<NewReservedPort>{port}</NewReservedPort>"),
                )
            }
        }
        "DeletePortMapping" => {
            let port = num("NewExternalPort").and_then(|p| u16::try_from(p).ok());
            match port.and_then(|p| st.mappings.remove(&p)) {
                Some(_) => ok(action, ""),
                None => fault(714, "NoSuchEntryInArray"),
            }
        }
        _ => fault(401, "Invalid Action"),
    }
}

fn ok(action: &str, out: &str) -> (&'static str, String) {
    (
        "200 OK",
        format!(
            "<?xml version=\"1.0\"?>\n<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
             s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body>\
             <u:{action}Response xmlns:u=\"{SERVICE}\">{out}</u:{action}Response>\
             </s:Body></s:Envelope>"
        ),
    )
}

fn fault(code: u16, what: &str) -> (&'static str, String) {
    (
        "500 Internal Server Error",
        format!(
            "<?xml version=\"1.0\"?>\n<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
             s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><s:Fault>\
             <faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring><detail>\
             <UPnPError xmlns=\"urn:schemas-upnp-org:control-1-0\"><errorCode>{code}</errorCode>\
             <errorDescription>{what}</errorDescription></UPnPError></detail></s:Fault>\
             </s:Body></s:Envelope>"
        ),
    )
}

const ROOT_XML: &str = "<?xml version=\"1.0\"?>
<root xmlns=\"urn:schemas-upnp-org:device-1-0\">
<specVersion><major>1</major><minor>0</minor></specVersion>
<device>
<deviceType>urn:schemas-upnp-org:device:InternetGatewayDevice:1</deviceType>
<friendlyName>fake router</friendlyName>
<deviceList><device>
<deviceType>urn:schemas-upnp-org:device:WANDevice:1</deviceType>
<deviceList><device>
<deviceType>urn:schemas-upnp-org:device:WANConnectionDevice:1</deviceType>
<serviceList><service>
<serviceType>urn:schemas-upnp-org:service:WANIPConnection:1</serviceType>
<serviceId>urn:upnp-org:serviceId:WANIPConn1</serviceId>
<controlURL>/ctl</controlURL>
<eventSubURL>/evt</eventSubURL>
<SCPDURL>/scpd.xml</SCPDURL>
</service></serviceList>
</device></deviceList>
</device></deviceList>
</device>
</root>
";

fn scpd_xml() -> String {
    let arg = |name: &str, dir: &str| {
        format!(
            "<argument><name>{name}</name><direction>{dir}</direction>\
             <relatedStateVariable>{name}</relatedStateVariable></argument>"
        )
    };
    let add_in: String = ARGS[..8].iter().map(|n| arg(n, "in")).collect();
    let action = |name: &str, args: String| {
        format!("<action><name>{name}</name><argumentList>{args}</argumentList></action>")
    };
    format!(
        "<?xml version=\"1.0\"?>\n<scpd xmlns=\"urn:schemas-upnp-org:service-1-0\">\
         <specVersion><major>1</major><minor>0</minor></specVersion><actionList>{}{}{}{}</actionList>\
         <serviceStateTable></serviceStateTable></scpd>",
        action("AddPortMapping", add_in.clone()),
        action(
            "AddAnyPortMapping",
            add_in + &arg("NewReservedPort", "out")
        ),
        action(
            "DeletePortMapping",
            ["NewRemoteHost", "NewExternalPort", "NewProtocol"]
                .iter()
                .map(|n| arg(n, "in"))
                .collect()
        ),
        action(
            "GetExternalIPAddress",
            arg("NewExternalIPAddress", "out")
        ),
    )
}

/// Which protocols the fake NAT-PMP/PCP router speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PmpMode {
    /// NAT-PMP only: PCP requests get NAT-PMP's unsupported-version answer.
    NatPmpOnly,
    /// PCP (and NAT-PMP).
    Pcp,
    /// Answers both, refusing every mapping (not authorized).
    Refuse,
}

/// A NAT-PMP or PCP request the fake router got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PmpRequest {
    /// 0 for NAT-PMP, 2 for PCP.
    pub version: u8,
    /// The internal port asked for.
    pub internal_port: u16,
    /// The lifetime asked for; 0 deletes.
    pub lifetime: u32,
}

#[derive(Default)]
struct PmpState {
    requests: Vec<PmpRequest>,
    mappings: BTreeMap<u16, FakeMapping>,
}

/// A NAT-PMP/PCP router on its own loopback address, port 5351.
pub struct FakePmp {
    addr: SocketAddr,
    state: Arc<Mutex<PmpState>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

static NEXT_IP: AtomicU32 = AtomicU32::new(0);

impl FakePmp {
    /// Starts the router on a loopback address of its own (127.x.y.z), as
    /// the protocols use the fixed port 5351.
    pub fn start(mode: PmpMode, external_ip: Ipv4Addr) -> io::Result<Self> {
        let socket = bind_5351()?;
        socket.set_read_timeout(Some(POLL))?;
        let addr = socket.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(PmpState::default()));
        let (s, st) = (stop.clone(), state.clone());
        let thread = std::thread::spawn(move || {
            let mut buf = [0u8; 1100];
            while !s.load(Ordering::Relaxed) {
                let Ok((n, from)) = socket.recv_from(&mut buf) else {
                    continue;
                };
                let mut st = st.lock().expect("state");
                if let Some(reply) = answer(mode, external_ip, from, &buf[..n], &mut st) {
                    let _ = socket.send_to(&reply, from);
                }
            }
        });
        Ok(Self {
            addr,
            state,
            stop,
            thread: Some(thread),
        })
    }

    /// The router's address (`PmpConfig::gateways`).
    pub fn gateway(&self) -> IpAddr {
        self.addr.ip()
    }

    /// The requests so far.
    pub fn requests(&self) -> Vec<PmpRequest> {
        self.state.lock().expect("state").requests.clone()
    }

    /// The forwards held now.
    pub fn mappings(&self) -> Vec<FakeMapping> {
        let st = self.state.lock().expect("state");
        st.mappings.values().copied().collect()
    }
}

impl Drop for FakePmp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A loopback address no other fake (in any test process) holds, port 5351.
fn bind_5351() -> io::Result<UdpSocket> {
    let base = std::process::id().wrapping_mul(2_654_435_761);
    let mut last = io::Error::other("no free loopback address");
    for _ in 0..64 {
        let n = base.wrapping_add(NEXT_IP.fetch_add(7919, Ordering::Relaxed));
        let [_, a, b, c] = n.to_be_bytes();
        // 127.0.0.1 is everyone's, and .0 / .255 are odd; skip those.
        if (a, b) == (0, 0) || c == 0 || c == 255 {
            continue;
        }
        match UdpSocket::bind((Ipv4Addr::new(127, a, b, c), PMP_PORT)) {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn epoch() -> [u8; 4] {
    1000u32.to_be_bytes()
}

fn answer(
    mode: PmpMode,
    external_ip: Ipv4Addr,
    from: SocketAddr,
    req: &[u8],
    st: &mut PmpState,
) -> Option<Vec<u8>> {
    let (&version, &op) = (req.first()?, req.get(1)?);
    match (version, op) {
        // NAT-PMP external address.
        (0, 0) => {
            let mut r = vec![0, 128, 0, 0];
            r.extend_from_slice(&epoch());
            r.extend_from_slice(&external_ip.octets());
            Some(r)
        }
        // NAT-PMP UDP mapping (or deletion with lifetime 0).
        (0, 1) if req.len() >= 12 => {
            let internal = u16::from_be_bytes([req[4], req[5]]);
            let suggested = u16::from_be_bytes([req[6], req[7]]);
            let lifetime = u32::from_be_bytes([req[8], req[9], req[10], req[11]]);
            st.requests.push(PmpRequest {
                version: 0,
                internal_port: internal,
                lifetime,
            });
            let mut r = vec![0, 129];
            if mode == PmpMode::Refuse {
                r.extend_from_slice(&2u16.to_be_bytes());
                r.extend_from_slice(&epoch());
                r.extend_from_slice(&internal.to_be_bytes());
                r.extend_from_slice(&[0; 6]);
                return Some(r);
            }
            let external = if lifetime == 0 {
                st.mappings.retain(|_, m| m.internal.port() != internal);
                0
            } else {
                let port = if suggested != 0 { suggested } else { internal };
                st.mappings.insert(
                    port,
                    FakeMapping {
                        external_port: port,
                        internal: SocketAddr::new(from.ip(), internal),
                        lease_s: lifetime,
                    },
                );
                port
            };
            r.extend_from_slice(&0u16.to_be_bytes());
            r.extend_from_slice(&epoch());
            r.extend_from_slice(&internal.to_be_bytes());
            r.extend_from_slice(&external.to_be_bytes());
            r.extend_from_slice(&lifetime.to_be_bytes());
            Some(r)
        }
        // PCP MAP.
        (2, 1) if req.len() >= 60 => {
            let lifetime = u32::from_be_bytes([req[4], req[5], req[6], req[7]]);
            let client = std::net::Ipv6Addr::from(<[u8; 16]>::try_from(&req[8..24]).ok()?);
            let internal = u16::from_be_bytes([req[40], req[41]]);
            let suggested = u16::from_be_bytes([req[42], req[43]]);
            st.requests.push(PmpRequest {
                version: 2,
                internal_port: internal,
                lifetime,
            });
            if mode == PmpMode::NatPmpOnly {
                // RFC 6887 section 9: a NAT-PMP server answers a PCP
                // request with its own header and UNSUPP_VERSION.
                let mut r = vec![0, 0x80 | op, 0, 1];
                r.extend_from_slice(&epoch());
                return Some(r);
            }
            let refused = mode == PmpMode::Refuse;
            let external = if refused {
                0
            } else if lifetime == 0 {
                st.mappings.retain(|_, m| m.internal.port() != internal);
                0
            } else {
                let port = if suggested != 0 { suggested } else { internal };
                let ip = client
                    .to_ipv4_mapped()
                    .map(IpAddr::V4)
                    .unwrap_or(IpAddr::V6(client));
                st.mappings.insert(
                    port,
                    FakeMapping {
                        external_port: port,
                        internal: SocketAddr::new(ip, internal),
                        lease_s: lifetime,
                    },
                );
                port
            };
            let mut r = vec![2, 0x81, 0, if refused { 2 } else { 0 }];
            r.extend_from_slice(&(if refused { 0 } else { lifetime }).to_be_bytes());
            r.extend_from_slice(&epoch());
            r.extend_from_slice(&[0; 12]);
            r.extend_from_slice(&req[24..36]); // nonce
            r.push(req[36]); // protocol
            r.extend_from_slice(&[0; 3]);
            r.extend_from_slice(&internal.to_be_bytes());
            r.extend_from_slice(&external.to_be_bytes());
            r.extend_from_slice(&external_ip.to_ipv6_mapped().octets());
            Some(r)
        }
        _ => None,
    }
}
