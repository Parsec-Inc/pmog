//! Finds servers on the LAN by listening to their UDP beacons.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};

use crate::constants::{MULTICAST_GROUP, PROTOCOL_VERSION, RECV_BUFFER, RECV_ERROR_BACKOFF, SERVER_STALE};
use crate::protocol::Beacon;
use crate::update::{platform, release};
use crate::utils::clean_name;

/// A server seen on the network; `addr` is where to open the TCP connection.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerEntry {
    pub addr: SocketAddr,
    pub name: String,
    pub players: u32,
    pub last_seen: Instant,
    /// False when the server runs another game version; it can still hand out an update.
    pub compatible: bool,
    /// Build the server runs.
    pub release: u64,
    /// Newest signed build it offers for this computer's platform.
    pub update: Option<u64>,
}

/// Binds a UDP socket that several processes on one machine can share.
pub fn bind_shared(port: u16) -> io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(all(unix, not(target_os = "linux")))]
    socket.set_reuse_port(true)?;
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into())?;
    // Without a multicast route the broadcast beacon still works.
    let _ = socket.join_multicast_v4(&MULTICAST_GROUP, &Ipv4Addr::UNSPECIFIED);
    Ok(socket.into())
}

pub struct Discovery {
    rx: Receiver<(Beacon, SocketAddr)>,
    servers: Vec<ServerEntry>,
}

impl Discovery {
    pub fn start(port: u16) -> io::Result<Discovery> {
        let socket = bind_shared(port)?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; RECV_BUFFER];
            loop {
                match socket.recv_from(&mut buf) {
                    Ok((len, src)) => {
                        let Ok(beacon) = serde_json::from_slice::<Beacon>(&buf[..len]) else { continue };
                        if tx.send((beacon, src)).is_err() {
                            return;
                        }
                    }
                    Err(_) => thread::sleep(RECV_ERROR_BACKOFF),
                }
            }
        });
        Ok(Discovery { rx, servers: Vec::new() })
    }

    /// Folds new beacons in, drops silent servers and returns the current list.
    pub fn refresh(&mut self, now: Instant) -> &[ServerEntry] {
        for (beacon, src) in self.rx.try_iter() {
            let addr = SocketAddr::new(src.ip(), beacon.port);
            let update = beacon.updates.iter().filter(|(p, _)| p == platform()).map(|(_, r)| *r).max();
            let entry = ServerEntry {
                addr,
                name: clean_name(&beacon.name),
                players: beacon.players,
                last_seen: now,
                compatible: beacon.v == PROTOCOL_VERSION && beacon.release == release(),
                release: beacon.release,
                update,
            };
            // The same server heard over two networks (LAN and VPN) keeps the first address.
            match self.servers.iter_mut().find(|s| s.addr == addr || (s.addr.port() == addr.port() && s.name == entry.name)) {
                Some(existing) => *existing = ServerEntry { addr: existing.addr, ..entry },
                None => self.servers.push(entry),
            }
        }
        self.servers.retain(|s| now.saturating_duration_since(s.last_seen) < SERVER_STALE);
        self.servers.sort_by(|a, b| a.name.cmp(&b.name).then(a.addr.cmp(&b.addr)));
        &self.servers
    }

    pub fn servers(&self) -> &[ServerEntry] {
        &self.servers
    }

    /// Waits until at least one server shows up.
    pub fn wait_for_server(&mut self, timeout: Duration) -> Option<ServerEntry> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Some(first) = self.refresh(Instant::now()).iter().find(|s| s.compatible) {
                return Some(first.clone());
            }
            thread::sleep(Duration::from_millis(50));
        }
        None
    }
}
