//! Client side of the TCP link to a server.

use std::io::BufReader;
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crate::constants::{CONNECT_TIMEOUT, HANDSHAKE_TIMEOUT, PROTOCOL_VERSION, SELF_PAUSE, SERVER_SILENCE, WRITE_TIMEOUT};
use crate::protocol::{ClientMsg, PlayerId, ServerMsg, read_msg, write_msg};

pub enum Incoming {
    Msg(ServerMsg),
    Closed,
}

pub struct Connection {
    stream: Arc<Mutex<TcpStream>>,
    rx: Receiver<Incoming>,
    pub my_id: PlayerId,
    pub server_name: String,
    pub addr: SocketAddr,
}

impl Connection {
    /// Connects and identifies this computer; errors are ready to show to the player.
    pub fn open(addr: SocketAddr, machine_id: &str, name: &str) -> Result<Connection, String> {
        let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
            .map_err(|e| format!("Não consegui conectar em {addr}: {e}"))?;
        let _ = stream.set_nodelay(true);
        let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
        let hello = ClientMsg::Hello { v: PROTOCOL_VERSION, machine_id: machine_id.into(), name: name.into() };
        write_msg(&mut stream, &hello).map_err(|e| format!("Falha ao falar com o servidor: {e}"))?;

        let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
        let reader_stream = stream.try_clone().map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(reader_stream);
        let (my_id, server_name) = match read_msg::<ServerMsg>(&mut reader) {
            Ok(Some(ServerMsg::Welcome { id, server })) => (id, server),
            Ok(Some(ServerMsg::Rejected { reason })) => return Err(reason),
            Ok(_) => return Err("O servidor fechou a conexão.".into()),
            Err(e) => return Err(format!("O servidor não respondeu: {e}")),
        };
        let _ = stream.set_read_timeout(None);

        let stream = Arc::new(Mutex::new(stream));
        let ponger = stream.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            loop {
                match read_msg::<ServerMsg>(&mut reader) {
                    Ok(Some(ServerMsg::Ping { n })) => {
                        // Answered here so a busy UI thread never inflates the measured latency.
                        if let Ok(mut out) = ponger.lock() {
                            let _ = write_msg(&mut *out, &ClientMsg::Pong { n });
                        }
                    }
                    Ok(Some(msg)) => {
                        if tx.send(Incoming::Msg(msg)).is_err() {
                            return;
                        }
                    }
                    Ok(None) | Err(_) => {
                        let _ = tx.send(Incoming::Closed);
                        return;
                    }
                }
            }
        });
        Ok(Connection { stream, rx, my_id, server_name, addr })
    }

    /// Sends one action; false means the link is gone.
    pub fn send(&mut self, msg: &ClientMsg) -> bool {
        self.stream.lock().is_ok_and(|mut out| write_msg(&mut *out, msg).is_ok())
    }

    pub fn drain(&self) -> Vec<Incoming> {
        self.rx.try_iter().collect()
    }

    pub fn close(&self) {
        if let Ok(out) = self.stream.lock() {
            let _ = out.shutdown(Shutdown::Both);
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.close();
    }
}

/// Notices a silent server, without blaming it for time this process spent paused.
#[derive(Debug, Clone, Copy)]
pub struct Liveness {
    last_heard: Instant,
    last_check: Instant,
}

impl Liveness {
    pub fn new(now: Instant) -> Liveness {
        Liveness { last_heard: now, last_check: now }
    }

    /// Call once per loop with whether anything arrived; true once the server went quiet.
    pub fn is_dead(&mut self, heard: bool, now: Instant) -> bool {
        if heard || now.saturating_duration_since(self.last_check) > SELF_PAUSE {
            self.last_heard = now;
        }
        self.last_check = now;
        now.saturating_duration_since(self.last_heard) > SERVER_SILENCE
    }
}
