//! Phone access: this PMOG serves a page and streams its own screen to one phone, guarded by a password.

mod auth;
mod screen;

use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;
use serde_json::json;
use tungstenite::protocol::{Role, WebSocketConfig};
use tungstenite::{Message, WebSocket};

use crate::app::App;
use crate::constants::*;
use crate::input::handle_key;
use crate::ui;

pub use auth::PhoneSettings;

const PAGE: &str = include_str!("index.html");
const CLOSED: &str = "O PMOG foi fechado no PC.";

/// Address a phone types to open the game.
pub fn page_url(ip: std::net::IpAddr, port: u16) -> String {
    format!("http://{ip}:{port}")
}

/// Messages the page sends.
#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum PageMsg {
    Hello { id: String, cols: u16, rows: u16 },
    Login { password: String },
    Key { k: String },
    Resize { cols: u16, rows: u16 },
}

/// From a phone's socket thread to the game loop.
enum Event {
    Joined { cols: u16, rows: u16, out: Sender<Out> },
    Key(String),
    Resize(u16, u16),
    Left,
}

/// From the game loop to the phone's socket thread.
enum Out {
    Text(String),
    Close(String),
}

/// State the socket threads and the game loop share.
struct Shared {
    settings: PhoneSettings,
    connected: bool,
    /// Settings changed and should be saved.
    dirty: bool,
    failures: u32,
    locked_until: Option<Instant>,
}

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// The phone currently attached to this PMOG.
struct Phone {
    out: Sender<Out>,
    terminal: Terminal<TestBackend>,
    sent: Vec<String>,
}

/// Phone access for one running PMOG: listener, password and the connected phone.
pub struct Remote {
    /// Port the page is served on; 0 when no port was free.
    pub port: u16,
    shared: Arc<Mutex<Shared>>,
    events: Receiver<Event>,
    phone: Option<Phone>,
    /// The phone's terminal is out being drawn into; it still counts as connected.
    lent: bool,
    stop: Arc<AtomicBool>,
}

impl Remote {
    /// Listens on the first free port from `start`; without one, phone access stays off.
    pub fn start(start: u16, settings: PhoneSettings) -> Remote {
        let shared = Arc::new(Mutex::new(Shared { settings, connected: false, dirty: false, failures: 0, locked_until: None }));
        let (tx, events) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let listener = (start..start.saturating_add(WEB_PORT_COUNT)).find_map(|p| TcpListener::bind((Ipv4Addr::UNSPECIFIED, p)).ok());
        let port = listener.as_ref().and_then(|l| l.local_addr().ok()).map_or(0, |a| a.port());
        if let Some(listener) = listener {
            spawn_acceptor(listener, tx, shared.clone(), stop.clone());
        }
        Remote { port, shared, events, phone: None, lent: false, stop }
    }

    pub fn settings(&self) -> PhoneSettings {
        lock(&self.shared).settings.clone()
    }

    /// True once since the last call when settings changed and should be saved.
    pub fn take_dirty(&self) -> bool {
        std::mem::take(&mut lock(&self.shared).dirty)
    }

    pub fn has_password(&self) -> bool {
        lock(&self.shared).settings.has_password()
    }

    pub fn takeover(&self) -> bool {
        lock(&self.shared).settings.takeover
    }

    pub fn connected(&self) -> bool {
        self.phone.is_some() || self.lent
    }

    /// True while a phone runs the game and the PC only watches.
    pub fn controls(&self) -> bool {
        self.connected() && self.takeover()
    }

    /// Sets a new password, signing every browser out.
    pub fn set_password(&mut self, password: &str) {
        {
            let mut shared = lock(&self.shared);
            shared.settings.set_password(password);
            shared.dirty = true;
        }
        self.kick("A senha do celular mudou no PC. Entre de novo.");
    }

    pub fn toggle_takeover(&mut self) {
        let mut shared = lock(&self.shared);
        shared.settings.takeover = !shared.settings.takeover;
        shared.dirty = true;
    }

    /// Disconnects the phone, showing `reason` on it.
    pub fn kick(&mut self, reason: &str) {
        if let Some(phone) = self.phone.take() {
            let _ = phone.out.send(Out::Close(reason.into()));
        }
    }

    /// Takes in what the phone did since the last frame and returns its keys.
    pub fn poll(&mut self) -> Vec<KeyEvent> {
        let mut keys = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Joined { cols, rows, out } => {
                    let (cols, rows) = clamp_size(cols, rows);
                    if let Ok(terminal) = Terminal::new(TestBackend::new(cols, rows)) {
                        self.kick("Outro celular entrou.");
                        self.phone = Some(Phone { out, terminal, sent: Vec::new() });
                    }
                }
                Event::Left => self.phone = None,
                Event::Resize(cols, rows) => {
                    if let Some(phone) = self.phone.as_mut() {
                        let (cols, rows) = clamp_size(cols, rows);
                        phone.terminal.backend_mut().resize(cols, rows);
                        phone.sent.clear();
                    }
                }
                Event::Key(name) => keys.extend(key_event(&name).filter(|_| self.phone.is_some())),
            }
        }
        keys
    }
}

/// Applies the phone's keys to `app` and sends it the new screen; call once per frame.
pub fn pump(app: &mut App, now: Instant) {
    let keys = app.remote.as_mut().map(Remote::poll).unwrap_or_default();
    for key in keys {
        // Quitting from the phone only lets go of it: the game keeps running on the PC.
        if !handle_key(app, key, now) || app.quit {
            app.quit = false;
            if let Some(remote) = app.remote.as_mut() {
                remote.kick("Você saiu pelo celular. O PMOG continua aberto no PC.");
            }
        }
    }
    // The phone's terminal is lent out while drawing so the screen can read the rest of the app.
    let Some(mut phone) = app.remote.as_mut().and_then(|r| {
        r.lent = r.phone.is_some();
        r.phone.take()
    }) else {
        return;
    };
    let drawn = phone.terminal.draw(|f| ui::draw(f, app)).is_ok();
    let delivered = drawn && frame(phone.terminal.backend().buffer(), &mut phone.sent).is_none_or(|text| phone.out.send(Out::Text(text)).is_ok());
    if let Some(remote) = app.remote.as_mut() {
        remote.lent = false;
        if delivered {
            remote.phone = Some(phone);
        }
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        self.kick(CLOSED);
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn spawn_acceptor(listener: TcpListener, tx: Sender<Event>, shared: Arc<Mutex<Shared>>, stop: Arc<AtomicBool>) {
    let _ = listener.set_nonblocking(true);
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let (tx, shared, stop) = (tx.clone(), shared.clone(), stop.clone());
                    thread::spawn(move || serve(stream, &tx, &shared, &stop));
                }
                Err(_) => thread::sleep(ACCEPT_POLL),
            }
        }
    });
}

/// Answers one HTTP request: the page itself or the phone's WebSocket.
fn serve(mut stream: TcpStream, tx: &Sender<Event>, shared: &Mutex<Shared>, stop: &AtomicBool) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let Ok(read_half) = stream.try_clone() else { return };
    let Ok((path, headers)) = read_request(&mut BufReader::new(read_half)) else { return };
    let header = |name: &str| headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    let upgrade = header("upgrade").is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    match (path.as_str(), upgrade, header("sec-websocket-key")) {
        (WEB_PATH_SOCKET, true, Some(key)) => {
            let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
            let head = format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n");
            if stream.write_all(head.as_bytes()).is_ok() {
                let config = WebSocketConfig::default().max_message_size(Some(WEB_MAX_MESSAGE)).max_frame_size(Some(WEB_MAX_MESSAGE));
                let _ = stream.set_read_timeout(Some(WEB_POLL));
                session(WebSocket::from_raw_socket(stream, Role::Server, Some(config)), tx, shared, stop);
            }
        }
        (WEB_PATH_PAGE, false, _) => {
            let _ = respond(&mut stream, "200 OK", "text/html; charset=utf-8", PAGE.as_bytes());
        }
        _ => {
            let _ = respond(&mut stream, "404 Not Found", "text/plain; charset=utf-8", b"404");
        }
    }
}

/// Path of a GET request and its lowercased headers.
fn read_request(reader: &mut BufReader<TcpStream>) -> io::Result<(String, Vec<(String, String)>)> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "requisição HTTP inválida");
    let mut total = 0;
    let mut next = |reader: &mut BufReader<TcpStream>| -> io::Result<String> {
        let mut line = String::new();
        let read = io::Read::take(&mut *reader, (WEB_MAX_HEAD - total) as u64).read_line(&mut line)?;
        total += read;
        if read == 0 || total >= WEB_MAX_HEAD {
            return Err(invalid());
        }
        Ok(line.trim_end().to_string())
    };
    let first = next(reader)?;
    let mut parts = first.split_whitespace();
    let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else { return Err(invalid()) };
    let path = target.split('?').next().unwrap_or("").to_string();
    let mut headers = Vec::new();
    loop {
        let line = next(reader)?;
        if line.is_empty() {
            return Ok((path, headers));
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
}

fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) -> io::Result<()> {
    let head = format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len());
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// Maps a key name from the page (`KeyboardEvent.key` style, `Ctrl+` prefix allowed) to a terminal key.
pub fn key_event(name: &str) -> Option<KeyEvent> {
    if let Some(rest) = name.strip_prefix(WEB_CTRL_PREFIX).filter(|r| r.chars().count() == 1) {
        let mut key = key_event(&rest.to_lowercase())?;
        key.modifiers |= KeyModifiers::CONTROL;
        return Some(key);
    }
    let code = match name {
        "ArrowUp" => KeyCode::Up,
        "ArrowDown" => KeyCode::Down,
        "ArrowLeft" => KeyCode::Left,
        "ArrowRight" => KeyCode::Right,
        "Enter" => KeyCode::Enter,
        "Escape" => KeyCode::Esc,
        "Backspace" => KeyCode::Backspace,
        "Delete" => KeyCode::Delete,
        "Tab" => KeyCode::Tab,
        "ShiftTab" => KeyCode::BackTab,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        _ => {
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if !c.is_control() => KeyCode::Char(c),
                _ => return None,
            }
        }
    };
    let modifiers = if code == KeyCode::BackTab { KeyModifiers::SHIFT } else { KeyModifiers::NONE };
    Some(KeyEvent::new(code, modifiers))
}

fn clamp_size(cols: u16, rows: u16) -> (u16, u16) {
    (cols.clamp(WEB_MIN_COLS, WEB_MAX_COLS), rows.clamp(WEB_MIN_ROWS, WEB_MAX_ROWS))
}

fn send(ws: &mut WebSocket<TcpStream>, value: serde_json::Value) -> bool {
    ws.send(Message::text(value.to_string())).is_ok()
}

fn bye(ws: &mut WebSocket<TcpStream>, reason: &str) {
    send(ws, json!({"t": "bye", "reason": reason}));
    let _ = ws.close(None);
    let _ = ws.flush();
}

/// Reads one page message; `Ok(None)` when nothing arrived in time.
fn receive(ws: &mut WebSocket<TcpStream>) -> Result<Option<PageMsg>, ()> {
    match ws.read() {
        Ok(Message::Text(text)) => Ok(serde_json::from_str(text.as_str()).ok()),
        Ok(Message::Close(_)) => Err(()),
        Ok(_) => Ok(None),
        Err(tungstenite::Error::Io(e)) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => Ok(None),
        Err(_) => Err(()),
    }
}

/// Waits up to `limit` for a message `pick` accepts.
fn wait_for<T>(ws: &mut WebSocket<TcpStream>, limit: Duration, stop: &AtomicBool, pick: impl Fn(PageMsg) -> Option<T>) -> Option<T> {
    let started = Instant::now();
    while started.elapsed() < limit && !stop.load(Ordering::SeqCst) {
        match receive(ws) {
            Ok(Some(msg)) => {
                if let Some(found) = pick(msg) {
                    return Some(found);
                }
            }
            Ok(None) => {}
            Err(()) => return None,
        }
    }
    None
}

/// Accepts a remembered browser or asks for the password; `Err` carries the reason to show, empty when the page is gone.
fn authenticate(ws: &mut WebSocket<TcpStream>, token: &str, shared: &Mutex<Shared>, stop: &AtomicBool) -> Result<(), String> {
    if !lock(shared).settings.has_password() {
        return Err("O acesso pelo celular ainda não tem senha. No PC, aperte C na tela inicial ou no painel do servidor (S) para criar uma.".into());
    }
    if lock(shared).settings.trusts(token) {
        return Ok(());
    }
    let mut error: Option<String> = None;
    loop {
        if !send(ws, json!({"t": "auth", "error": error})) {
            return Err(String::new());
        }
        let password = wait_for(ws, WEB_LOGIN_TIMEOUT, stop, |m| if let PageMsg::Login { password } = m { Some(password) } else { None }).ok_or_else(String::new)?;
        let mut guard = lock(shared);
        let now = Instant::now();
        if let Some(until) = guard.locked_until.filter(|u| *u > now) {
            error = Some(format!("Muitas tentativas erradas. Espere {} s.", (until - now).as_secs() + 1));
            continue;
        }
        if guard.settings.check_password(&password) {
            guard.failures = 0;
            guard.settings.trust(token);
            guard.dirty = true;
            return Ok(());
        }
        guard.failures += 1;
        if guard.failures >= WEB_MAX_FAILURES {
            guard.failures = 0;
            guard.locked_until = Some(now + WEB_LOCKOUT);
        }
        drop(guard);
        thread::sleep(WEB_FAIL_DELAY);
        error = Some("Senha errada.".into());
    }
}

/// One phone: say who it is, prove it knows the password, then trade keys for screens.
fn session(mut ws: WebSocket<TcpStream>, tx: &Sender<Event>, shared: &Mutex<Shared>, stop: &AtomicBool) {
    let hello = wait_for(&mut ws, WEB_HELLO_TIMEOUT, stop, |m| if let PageMsg::Hello { id, cols, rows } = m { Some((id, cols, rows)) } else { None });
    let Some((token, cols, rows)) = hello else { return };
    let token_ok = (WEB_TOKEN_MIN..=WEB_TOKEN_MAX).contains(&token.len()) && token.chars().all(|c| c.is_ascii_hexdigit());
    if !token_ok {
        return bye(&mut ws, "Identificação do navegador inválida. Recarregue a página.");
    }
    if let Err(reason) = authenticate(&mut ws, &token.to_ascii_lowercase(), shared, stop) {
        if !reason.is_empty() {
            bye(&mut ws, &reason);
        }
        return;
    }
    if std::mem::replace(&mut lock(shared).connected, true) {
        return bye(&mut ws, "Já tem um celular conectado neste PMOG. Só um por vez.");
    }
    let (out_tx, out) = mpsc::channel();
    let joined = tx.send(Event::Joined { cols, rows, out: out_tx }).is_ok() && send(&mut ws, json!({"t": "ready"}));
    let reason = if joined { relay(&mut ws, tx, &out, stop) } else { String::new() };
    // Left goes out before the slot frees, so a new phone's Joined always arrives after it.
    let _ = tx.send(Event::Left);
    lock(shared).connected = false;
    if !reason.is_empty() {
        bye(&mut ws, &reason);
    }
}

/// Forwards page input to the game and screens back until either side stops; returns the reason to show.
fn relay(ws: &mut WebSocket<TcpStream>, tx: &Sender<Event>, out: &Receiver<Out>, stop: &AtomicBool) -> String {
    loop {
        if stop.load(Ordering::SeqCst) {
            return CLOSED.into();
        }
        let event = match receive(ws) {
            Ok(Some(PageMsg::Key { k })) => Some(Event::Key(k)),
            Ok(Some(PageMsg::Resize { cols, rows })) => Some(Event::Resize(cols, rows)),
            Ok(_) => None,
            Err(()) => return String::new(),
        };
        if event.is_some_and(|e| tx.send(e).is_err()) {
            return CLOSED.into();
        }
        loop {
            match out.try_recv() {
                Ok(Out::Text(text)) => {
                    if ws.send(Message::text(text)).is_err() {
                        return String::new();
                    }
                }
                Ok(Out::Close(reason)) => return reason,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return CLOSED.into(),
            }
        }
    }
}

/// Rows that changed since the last frame as one message; an empty `sent` resends all of them.
fn frame(buffer: &Buffer, sent: &mut Vec<String>) -> Option<String> {
    let (cols, rows) = (buffer.area.width, buffer.area.height);
    let full = sent.len() != rows as usize;
    if full {
        *sent = vec![String::new(); rows as usize];
    }
    let mut changed = Vec::new();
    for y in 0..rows {
        let encoded = screen::row(buffer, y).to_string();
        if encoded != sent[y as usize] {
            changed.push(format!("[{y},{encoded}]"));
            sent[y as usize] = encoded;
        }
    }
    if changed.is_empty() {
        return None;
    }
    let size = if full { format!(",\"cols\":{cols},\"rows\":{rows}") } else { String::new() };
    Some(format!("{{\"t\":\"frame\"{size},\"lines\":[{}]}}", changed.join(",")))
}
