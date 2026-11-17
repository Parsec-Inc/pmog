//! Embedded server: TCP for players, UDP beacons for discovery, SQLite for storage.

use std::collections::HashMap;
use std::io::{self, BufReader};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, SocketAddrV4, TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crate::constants::*;
use crate::db::Db;
use crate::machine_id;
use crate::protocol::{Beacon, ClientMsg, PlayerId, ServerMsg, parse_msg, read_line, read_msg, write_msg};
use crate::update::{self, Shelf};
use crate::utils::{RttStats, clean_name};
use crate::chat::{ChatCtx, ChatHub};
use crate::clan::{ClanCtx, ClanHub};
use crate::web::{self, WebCtx};
use crate::world::World;

/// Ports a server uses; tests pick their own to stay isolated.
#[derive(Debug, Clone, Copy)]
pub struct Ports {
    pub discovery: u16,
    pub tcp_start: u16,
}

impl Default for Ports {
    fn default() -> Ports {
        Ports { discovery: DISCOVERY_PORT, tcp_start: SERVER_PORT_START }
    }
}

/// Live numbers for the server panel.
#[derive(Debug, Clone, Default)]
pub struct Status {
    pub online: Vec<String>,
    pub registered: i64,
    pub duels: i64,
    pub error: Option<String>,
}

pub struct ServerHandle {
    pub name: String,
    pub port: u16,
    pub db_path: Option<PathBuf>,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<Status>>,
    threads: Vec<JoinHandle<()>>,
}

type ConnId = u64;

enum Inbound {
    Connected(ConnId, TcpStream),
    /// The instant is when the reader thread received it.
    Msg(ConnId, ClientMsg, Instant),
    Closed(ConnId),
}

struct Conn {
    stream: TcpStream,
    player: Option<PlayerId>,
    opened: Instant,
    /// Ping still waiting for its pong.
    ping: Option<(u32, Instant)>,
    pings_sent: u32,
    rtt: RttStats,
}

/// Default database location for a hosted server.
pub fn default_db_path() -> Option<PathBuf> {
    crate::utils::data_dir().map(|d| d.join("server.db"))
}

impl ServerHandle {
    /// Starts every server thread; `db_path` of `None` keeps data in memory.
    pub fn start(name: &str, db_path: Option<PathBuf>, ports: Ports) -> io::Result<ServerHandle> {
        ServerHandle::start_with(name, db_path, ports, Shelf::beside_exe())
    }

    /// Like `start`, handing out the builds on `shelf` to players with older versions.
    pub fn start_with(name: &str, db_path: Option<PathBuf>, ports: Ports, shelf: Shelf) -> io::Result<ServerHandle> {
        let db = match &db_path {
            Some(path) => Db::open(path),
            None => Db::in_memory(),
        }
        .map_err(|e| io::Error::other(format!("banco de dados: {e}")))?;
        let listener = bind_listener(ports.tcp_start)?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;

        let name = clean_name(name);
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(Status::default()));
        let (tx, rx) = mpsc::channel();

        let offers = shelf.offers();
        let threads = vec![
            spawn_acceptor(listener, tx, WebCtx::new(port, stop.clone()), Arc::new(shelf)),
            spawn_world(db, rx, stop.clone(), status.clone(), name.clone()),
            spawn_beacon(ports.discovery, name.clone(), port, offers, stop.clone(), status.clone())?,
        ];
        Ok(ServerHandle { name, port, db_path, stop, status, threads })
    }

    pub fn local_addr(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))
    }

    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Saves everything, tells players goodbye and stops all threads.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for handle in self.threads.drain(..) {
            let _ = handle.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn bind_listener(start: u16) -> io::Result<TcpListener> {
    for port in start..start + SERVER_PORT_COUNT {
        if let Ok(listener) = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)) {
            return Ok(listener);
        }
    }
    Err(io::Error::new(io::ErrorKind::AddrInUse, "nenhuma porta livre para o servidor"))
}

fn spawn_acceptor(listener: TcpListener, tx: Sender<Inbound>, web: WebCtx, shelf: Arc<Shelf>) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut next_id: ConnId = 0;
        while !web.stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_nodelay(true);
                    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
                    next_id += 1;
                    spawn_reader(next_id, stream, tx.clone(), shelf.clone(), web.clone());
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => thread::sleep(ACCEPT_POLL),
                Err(_) => thread::sleep(ACCEPT_POLL),
            }
        }
    })
}

/// Reads one connection; update requests and phone pages are answered here and never reach the world.
fn spawn_reader(id: ConnId, stream: TcpStream, tx: Sender<Inbound>, shelf: Arc<Shelf>, web: WebCtx) {
    thread::spawn(move || {
        let Ok(reader_stream) = stream.try_clone() else { return };
        let mut reader = BufReader::new(reader_stream);
        let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
        let first = match read_line(&mut reader) {
            Ok(Some(line)) => line,
            _ => return,
        };
        if web::is_http(&first) {
            return web::serve(&first, reader, stream, &web);
        }
        let _ = stream.set_read_timeout(None);
        if let Some(query) = update::as_request(&first) {
            let mut out = stream;
            let _ = out.set_write_timeout(Some(UPDATE_TIMEOUT));
            let _ = shelf.serve(&query, &mut out);
            let _ = out.shutdown(Shutdown::Both);
            return;
        }
        let Ok(first) = parse_msg::<ClientMsg>(&first) else { return };
        if tx.send(Inbound::Connected(id, stream)).is_err() || tx.send(Inbound::Msg(id, first, Instant::now())).is_err() {
            return;
        }
        loop {
            match read_msg::<ClientMsg>(&mut reader) {
                Ok(Some(msg)) => {
                    if tx.send(Inbound::Msg(id, msg, Instant::now())).is_err() {
                        return;
                    }
                }
                Ok(None) | Err(_) => {
                    let _ = tx.send(Inbound::Closed(id));
                    return;
                }
            }
        }
    });
}

fn spawn_beacon(
    discovery_port: u16,
    name: String,
    port: u16,
    updates: Vec<(String, u64)>,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<Status>>,
) -> io::Result<JoinHandle<()>> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.set_broadcast(true)?;
    socket.set_multicast_loop_v4(true)?;
    Ok(thread::spawn(move || {
        let targets = [
            SocketAddrV4::new(MULTICAST_GROUP, discovery_port),
            SocketAddrV4::new(Ipv4Addr::BROADCAST, discovery_port),
        ];
        let mut last = Instant::now() - BEACON_INTERVAL;
        while !stop.load(Ordering::SeqCst) {
            if last.elapsed() >= BEACON_INTERVAL {
                let players = status.lock().map(|s| s.online.len() as u32).unwrap_or(0);
                let beacon = Beacon { v: PROTOCOL_VERSION, name: name.clone(), port, players, release: update::release(), updates: updates.clone() };
                if let Ok(bytes) = serde_json::to_vec(&beacon) {
                    for target in targets {
                        let _ = socket.send_to(&bytes, target);
                    }
                }
                last = Instant::now();
            }
            thread::sleep(ACCEPT_POLL);
        }
    }))
}

struct ServerLoop {
    db: Db,
    world: World,
    chat: ChatHub,
    clans: ClanHub,
    conns: HashMap<ConnId, Conn>,
    name: String,
}

fn spawn_world(
    db: Db,
    rx: Receiver<Inbound>,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<Status>>,
    name: String,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let (chat, clans) = (ChatHub::load(&db), ClanHub::load(&db));
        let mut server = ServerLoop { db, world: World::new(), chat, clans, conns: HashMap::new(), name };
        let mut last_tick = Instant::now();
        let mut last_state = last_tick;
        let mut last_flush = last_tick;
        let mut last_ping = last_tick;
        while !stop.load(Ordering::SeqCst) {
            match rx.recv_timeout(TICK) {
                Ok(inbound) => server.handle(inbound, Instant::now()),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            let now = Instant::now();
            if now - last_tick >= TICK {
                server.world.tick(now, (now - last_tick).as_secs_f64());
                server.drop_silent(now);
                last_tick = now;
            }
            if now - last_ping >= PING_INTERVAL {
                server.ping_all(now);
                last_ping = now;
            }
            if now - last_state >= STATE_INTERVAL {
                server.send_states(now);
                server.publish_online(&status);
                last_state = now;
            }
            server.deliver(now);
            if now - last_flush >= DB_FLUSH_INTERVAL {
                server.flush(&status);
                last_flush = now;
            }
        }
        server.shutdown(&status);
    })
}

impl ServerLoop {
    fn handle(&mut self, inbound: Inbound, now: Instant) {
        match inbound {
            Inbound::Connected(id, stream) => {
                self.conns.insert(id, Conn { stream, player: None, opened: now, ping: None, pings_sent: 0, rtt: RttStats::default() });
            }
            Inbound::Closed(id) => self.close(id),
            Inbound::Msg(id, ClientMsg::Hello { v, machine_id, name, release }, _) => self.hello(id, (v, release), machine_id, &name, now),
            Inbound::Msg(id, ClientMsg::Pong { n }, received) => self.pong(id, n, received),
            Inbound::Msg(id, msg, received) => {
                let Some(player) = self.conns.get(&id).and_then(|c| c.player) else {
                    self.close(id);
                    return;
                };
                match msg {
                    ClientMsg::Mog { target, meter } => self.world.mog(player, target, meter, now),
                    ClientMsg::Mew => self.world.mew(player, now),
                    ClientMsg::Buy { item } => self.world.buy(player, item),
                    ClientMsg::Use { item } => self.world.use_item(player, item, now),
                    ClientMsg::Rename { name } => self.world.rename(player, &name),
                    ClientMsg::Challenge { target, fraction, draft } => self.world.challenge(player, target, fraction, draft, now),
                    ClientMsg::Answer { from, accept } => self.world.answer(player, from, accept, now),
                    ClientMsg::CancelChallenge => self.world.cancel_challenge(player),
                    ClientMsg::Queue { fraction, draft } => self.world.queue(player, fraction, draft, now),
                    ClientMsg::Duel { key, at_ms } => {
                        let rtt_max = self.conns.get(&id).map_or(0.0, |c| c.rtt.max());
                        self.world.duel_input(player, key, at_ms, rtt_max, received);
                    }
                    ClientMsg::Forfeit => self.world.forfeit(player, now),
                    ClientMsg::Chat(cmd) => self.with_chat(now, |hub, ctx| hub.handle(ctx, player, cmd)),
                    ClientMsg::Clan(cmd) => {
                        let before = self.clan_map();
                        self.with_clans(now, |hub, ctx| hub.handle(ctx, player, cmd));
                        // Whoever entered or left a clan gets the clan channel added or removed right away.
                        let after = self.clan_map();
                        let changed: Vec<PlayerId> = before.keys().chain(after.keys()).filter(|p| before.get(p) != after.get(p)).copied().collect();
                        for p in changed {
                            self.with_chat(now, |hub, ctx| hub.handle(ctx, p, crate::chat::ChatCmd::Sync {}));
                        }
                    }
                    ClientMsg::Hello { .. } | ClientMsg::Pong { .. } => {}
                }
                self.send_state(player, now);
            }
        }
    }

    /// Clan id of every online player.
    fn clan_map(&self) -> HashMap<PlayerId, i64> {
        self.world.online_names().filter_map(|(p, _)| self.world.clan_of(p).map(|(c, _)| (p, c))).collect()
    }

    /// Runs one chat step; whatever it wants to say goes out with the world's messages.
    fn with_chat(&mut self, now: Instant, step: impl FnOnce(&mut ChatHub, &mut ChatCtx)) {
        let mut out = Vec::new();
        let mut ctx = ChatCtx { db: &self.db, world: &self.world, out: &mut out, now };
        step(&mut self.chat, &mut ctx);
        self.world.outbox.extend(out);
    }

    /// Runs one clan step, which may change clans in the world.
    fn with_clans(&mut self, now: Instant, step: impl FnOnce(&mut ClanHub, &mut ClanCtx)) {
        let mut out = Vec::new();
        let mut ctx = ClanCtx { db: &self.db, world: &mut self.world, out: &mut out, now };
        step(&mut self.clans, &mut ctx);
        self.world.outbox.extend(out);
    }

    /// Sends a refusal and closes the connection.
    fn reject(&mut self, id: ConnId, reason: &str) {
        if let Some(conn) = self.conns.get_mut(&id) {
            let _ = write_msg(&mut conn.stream, &ServerMsg::Rejected { reason: reason.into() });
        }
        self.close(id);
    }

    /// `version` is the client's (protocol, release).
    fn hello(&mut self, id: ConnId, version: (u32, u64), machine_id: String, name: &str, now: Instant) {
        if self.conns.get(&id).is_none_or(|c| c.player.is_some()) {
            return;
        }
        if let Some(reason) = update::version_refusal(version, (PROTOCOL_VERSION, update::release())) {
            return self.reject(id, reason);
        }
        if !machine_id::is_valid(&machine_id) {
            return self.reject(id, "Identificação do computador inválida.");
        }
        let name = match clean_name(name).trim() {
            "" => "anon".to_string(),
            n => n.to_string(),
        };
        // A new session from the same computer replaces the old one (dropped Wi-Fi, second window).
        let online = self.world.online().find(|(_, slot)| slot.machine_id == machine_id).map(|(pid, _)| pid);
        if let Some(old) = online.and_then(|pid| self.conn_of(pid).map(|(c, _)| c)) {
            self.reject(old, "Este computador entrou no servidor de novo em outra sessão.");
        }
        let (player_id, player) = match self.db.load_or_create(&machine_id, &name) {
            Ok(found) => found,
            Err(e) => return self.reject(id, &format!("Erro no banco do servidor: {e}")),
        };
        if let Err(reason) = self.world.join(player_id, machine_id, player, now) {
            return self.reject(id, &reason);
        }
        self.with_clans(now, |hub, ctx| hub.joined(ctx, player_id));
        self.with_chat(now, |hub, ctx| hub.joined(ctx, player_id));
        let Some(conn) = self.conns.get_mut(&id) else { return };
        conn.player = Some(player_id);
        let welcome = ServerMsg::Welcome { id: player_id, server: self.name.clone() };
        if write_msg(&mut conn.stream, &welcome).is_err() {
            self.close(id);
            return;
        }
        self.send_state(player_id, now);
    }

    /// Probes every player's round trip; an unanswered ping is simply replaced.
    fn ping_all(&mut self, now: Instant) {
        let mut dead = Vec::new();
        for (id, conn) in self.conns.iter_mut().filter(|(_, c)| c.player.is_some()) {
            conn.pings_sent = conn.pings_sent.wrapping_add(1);
            conn.ping = Some((conn.pings_sent, now));
            if write_msg(&mut conn.stream, &ServerMsg::Ping { n: conn.pings_sent }).is_err() {
                dead.push(*id);
            }
        }
        for id in dead {
            self.close(id);
        }
    }

    fn pong(&mut self, id: ConnId, n: u32, received: Instant) {
        let Some(conn) = self.conns.get_mut(&id) else { return };
        if let Some((sent_n, sent)) = conn.ping
            && sent_n == n
        {
            conn.rtt.add(received.saturating_duration_since(sent).as_secs_f64() * 1000.0);
            conn.ping = None;
        }
    }

    /// Drops connections that never said hello.
    fn drop_silent(&mut self, now: Instant) {
        let silent: Vec<ConnId> = self
            .conns
            .iter()
            .filter(|(_, c)| c.player.is_none() && now.saturating_duration_since(c.opened) > HANDSHAKE_TIMEOUT)
            .map(|(id, _)| *id)
            .collect();
        for id in silent {
            self.close(id);
        }
    }

    fn close(&mut self, id: ConnId) {
        let Some(conn) = self.conns.remove(&id) else { return };
        let _ = conn.stream.shutdown(Shutdown::Both);
        let now = Instant::now();
        if let Some(player) = conn.player {
            self.with_chat(now, |hub, ctx| hub.left(ctx, player));
            self.with_clans(now, |hub, ctx| hub.left(ctx, player));
        }
        if let Some(player) = conn.player
            && let Some(profile) = self.world.leave(player, now)
        {
            let _ = self.db.save(player, &profile);
        }
    }

    fn conn_of(&mut self, player: PlayerId) -> Option<(ConnId, &mut Conn)> {
        self.conns.iter_mut().find(|(_, c)| c.player == Some(player)).map(|(id, c)| (*id, c))
    }

    fn send(&mut self, player: PlayerId, msg: &ServerMsg) {
        let Some((id, conn)) = self.conn_of(player) else { return };
        if write_msg(&mut conn.stream, msg).is_err() {
            self.close(id);
        }
    }

    fn send_state(&mut self, player: PlayerId, now: Instant) {
        if let Some(mut view) = self.world.view(player, now) {
            view.rtt_ms = self.conn_of(player).map_or(0.0, |(_, c)| c.rtt.avg());
            self.send(player, &ServerMsg::State(Box::new(view)));
        }
    }

    fn send_states(&mut self, now: Instant) {
        let players: Vec<PlayerId> = self.world.online().map(|(id, _)| id).collect();
        for player in players {
            self.send_state(player, now);
        }
    }

    fn deliver(&mut self, now: Instant) {
        for (player, msg) in std::mem::take(&mut self.world.outbox) {
            self.send(player, &msg);
        }
        let mut push = std::mem::take(&mut self.world.push);
        push.sort_unstable();
        push.dedup();
        for player in push {
            self.send_state(player, now);
        }
    }

    fn publish_online(&self, status: &Mutex<Status>) {
        if let Ok(mut s) = status.lock() {
            s.online = self.world.online().map(|(_, slot)| slot.player.name.clone()).collect();
        }
    }

    /// Persists changed profiles and history in one transaction.
    fn flush(&mut self, status: &Mutex<Status>) {
        let dirty = self.world.take_dirty();
        let duels = std::mem::take(&mut self.world.duels);
        let purchases = std::mem::take(&mut self.world.purchases);
        let matches = std::mem::take(&mut self.world.matches);
        let result = self.db.transaction(|db| {
            for (id, player) in &dirty {
                db.save(*id, player)?;
            }
            for duel in &duels {
                db.log_duel(duel)?;
            }
            for purchase in &purchases {
                db.log_purchase(purchase)?;
            }
            for m in &matches {
                db.log_match(m)?;
            }
            Ok(())
        });
        self.publish_online(status);
        if let Ok(mut s) = status.lock() {
            s.registered = self.db.player_count().unwrap_or(s.registered);
            s.duels = self.db.duel_count(None).unwrap_or(s.duels);
            s.error = result.err().map(|e| e.to_string());
        }
    }

    fn shutdown(&mut self, status: &Mutex<Status>) {
        let ids: Vec<ConnId> = self.conns.keys().copied().collect();
        for id in &ids {
            if let Some(conn) = self.conns.get_mut(id) {
                let _ = write_msg(&mut conn.stream, &ServerMsg::Shutdown);
            }
        }
        self.world.abort_duels();
        self.flush(status);
        for (id, player) in self.world.all_profiles() {
            let _ = self.db.save(id, &player);
        }
        for (_, conn) in self.conns.drain() {
            let _ = conn.stream.shutdown(Shutdown::Both);
        }
    }
}
