//! Helpers shared by the network tests: isolated ports, temp databases and a scripted client.
#![allow(dead_code)]

use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use pmog::connection::{Connection, Incoming};
use pmog::constants::TICK;
use pmog::game::{Game, Player};
use pmog::machine_id;
use pmog::protocol::ServerMsg;
use pmog::server::{Ports, ServerHandle};

pub const DEADLINE: Duration = Duration::from_secs(10);

/// Ports no other test (or a real game) uses.
pub fn ports(slot: u16) -> Ports {
    let base = 51_000 + slot * 20;
    Ports { discovery: base, tcp_start: base + 1 }
}

/// Fresh database file path, removed on drop.
pub struct TempDb(pub PathBuf);

impl TempDb {
    pub fn new(tag: &str) -> TempDb {
        let path = std::env::temp_dir().join(format!("pmog-test-{tag}-{}-{}.db", std::process::id(), rand::random::<u32>()));
        TempDb(path)
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

pub fn start_server(slot: u16, db: Option<&TempDb>) -> ServerHandle {
    ServerHandle::start("Servidor de Teste", db.map(|d| d.0.clone()), ports(slot)).expect("server starts")
}

/// A scripted player speaking the real protocol.
pub struct Client {
    pub conn: Connection,
    pub game: Game,
    pub closed: bool,
    pub shutdown: bool,
    pub rejected: Option<String>,
    /// State snapshots received so far.
    pub states: usize,
}

impl Client {
    pub fn connect(server: &ServerHandle, name: &str) -> Client {
        Client::connect_as(server, name, &machine_id::for_bot(name))
    }

    pub fn connect_as(server: &ServerHandle, name: &str, machine: &str) -> Client {
        let conn = Connection::open(server.local_addr(), machine, name).expect("connects");
        let game = Game::new(Player { name: name.into(), ..Default::default() }, Instant::now());
        let mut client = Client { conn, game, closed: false, shutdown: false, rejected: None, states: 0 };
        assert!(client.wait(|c| c.states > 0), "first state");
        client
    }

    /// Sends queued actions and applies whatever arrived.
    pub fn pump(&mut self) {
        let now = Instant::now();
        for msg in std::mem::take(&mut self.game.outbox) {
            if !self.conn.send(&msg) {
                self.closed = true;
            }
        }
        for incoming in self.conn.drain() {
            match incoming {
                Incoming::Msg(ServerMsg::Shutdown) => self.shutdown = true,
                Incoming::Msg(ServerMsg::Rejected { reason }) => self.rejected = Some(reason),
                Incoming::Msg(msg) => {
                    self.states += matches!(msg, ServerMsg::State(_)) as usize;
                    self.game.apply(msg, now);
                }
                Incoming::Closed => self.closed = true,
            }
        }
    }

    pub fn wait(&mut self, done: impl Fn(&Client) -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < DEADLINE {
            self.pump();
            if done(self) {
                return true;
            }
            thread::sleep(TICK);
        }
        false
    }

    pub fn feed_has(&self, needle: &str) -> bool {
        self.game.feed.iter().any(|l| l.text.contains(needle))
    }

    pub fn select(&mut self, name: &str) {
        self.game.selected = self.game.peers.iter().position(|p| p.name == name).expect("peer listed");
    }
}

/// Pumps every client until `done` holds.
pub fn wait_all(clients: &mut [Client], done: impl Fn(&[Client]) -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < DEADLINE {
        for c in clients.iter_mut() {
            c.pump();
        }
        if done(clients) {
            return true;
        }
        thread::sleep(TICK);
    }
    false
}
