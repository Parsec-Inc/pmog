//! Phone client: the server hands out a page and runs one TUI per phone, streamed over a WebSocket.

mod screen;

use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;
use serde_json::json;
use tungstenite::protocol::{Role, WebSocketConfig};
use tungstenite::{Message, WebSocket};

use crate::app::App;
use crate::constants::*;
use crate::input::handle_key;
use crate::server::Ports;
use crate::utils::clean_name;
use crate::{machine_id, ui};

const PAGE: &str = include_str!("index.html");

/// What a phone session needs from the server that serves it.
#[derive(Clone)]
pub struct WebCtx {
    pub port: u16,
    pub stop: Arc<AtomicBool>,
    sessions: Arc<AtomicUsize>,
}

impl WebCtx {
    pub fn new(port: u16, stop: Arc<AtomicBool>) -> WebCtx {
        WebCtx { port, stop, sessions: Arc::new(AtomicUsize::new(0)) }
    }
}

/// Address a phone types to open the game.
pub fn page_url(ip: std::net::IpAddr, port: u16) -> String {
    format!("http://{ip}:{port}")
}

/// True when a connection's first line is an HTTP request instead of a game message.
pub fn is_http(first: &[u8]) -> bool {
    first.starts_with(b"GET ")
}

/// Messages the page sends.
#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum PageMsg {
    Hello { id: String, name: String, cols: u16, rows: u16 },
    Key { k: String },
    Resize { cols: u16, rows: u16 },
}

/// Answers one HTTP request: the page itself or the WebSocket of a session.
pub fn serve(first: &[u8], mut reader: BufReader<TcpStream>, mut stream: TcpStream, ctx: &WebCtx) {
    let line = String::from_utf8_lossy(first);
    let path = line.split_whitespace().nth(1).unwrap_or("").split('?').next().unwrap_or("").to_string();
    let Ok(headers) = read_headers(&mut reader) else { return };
    let header = |name: &str| headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    let upgrade = header("upgrade").is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    match (path.as_str(), upgrade, header("sec-websocket-key")) {
        (WEB_PATH_SOCKET, true, Some(key)) => {
            if ctx.sessions.fetch_add(1, Ordering::SeqCst) >= WEB_MAX_SESSIONS {
                ctx.sessions.fetch_sub(1, Ordering::SeqCst);
                let _ = respond(&mut stream, "503 Service Unavailable", "text/plain; charset=utf-8", "Celulares demais conectados.".as_bytes());
                return;
            }
            let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
            let head = format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n");
            if stream.write_all(head.as_bytes()).is_ok() {
                let config = WebSocketConfig::default().max_message_size(Some(WEB_MAX_MESSAGE)).max_frame_size(Some(WEB_MAX_MESSAGE));
                let _ = stream.set_read_timeout(Some(TICK));
                session(WebSocket::from_raw_socket(stream, Role::Server, Some(config)), ctx);
            }
            ctx.sessions.fetch_sub(1, Ordering::SeqCst);
        }
        (WEB_PATH_PAGE, false, _) => {
            let _ = respond(&mut stream, "200 OK", "text/html; charset=utf-8", PAGE.as_bytes());
        }
        _ => {
            let _ = respond(&mut stream, "404 Not Found", "text/plain; charset=utf-8", b"404");
        }
    }
}

/// Lowercased header names with their values, up to the blank line.
fn read_headers(reader: &mut BufReader<TcpStream>) -> io::Result<Vec<(String, String)>> {
    let mut headers = Vec::new();
    let mut total = 0;
    loop {
        let mut line = String::new();
        let read = io::Read::take(&mut *reader, (WEB_MAX_HEAD - total) as u64).read_line(&mut line)?;
        total += read;
        if read == 0 || total >= WEB_MAX_HEAD {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "cabeçalho HTTP inválido"));
        }
        let line = line.trim_end();
        if line.is_empty() {
            return Ok(headers);
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

/// Machine id for a phone, derived from the random token its browser keeps.
pub fn phone_id(token: &str) -> Option<String> {
    let token = token.trim();
    let valid = (16..=64).contains(&token.len()) && token.chars().all(|c| c.is_ascii_hexdigit());
    valid.then(|| machine_id::derive(&format!("{WEB_ID_PREFIX}{}", token.to_ascii_lowercase()), "web").ok()).flatten()
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

/// Reads one page message; `Ok(None)` when nothing arrived this tick.
fn receive(ws: &mut WebSocket<TcpStream>) -> Result<Option<PageMsg>, ()> {
    match ws.read() {
        Ok(Message::Text(text)) => Ok(serde_json::from_str(text.as_str()).ok()),
        Ok(Message::Close(_)) => Err(()),
        Ok(_) => Ok(None),
        Err(tungstenite::Error::Io(e)) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => Ok(None),
        Err(_) => Err(()),
    }
}

/// Waits for the page to say who it is.
fn hello(ws: &mut WebSocket<TcpStream>) -> Option<(String, String, u16, u16)> {
    let started = Instant::now();
    while started.elapsed() < WEB_HELLO_TIMEOUT {
        match receive(ws) {
            Ok(Some(PageMsg::Hello { id, name, cols, rows })) => return Some((id, name, cols, rows)),
            Ok(_) => {}
            Err(()) => return None,
        }
    }
    None
}

/// Runs one phone: a full client connected to this server, drawn into the page.
fn session(mut ws: WebSocket<TcpStream>, ctx: &WebCtx) {
    let Some((token, name, cols, rows)) = hello(&mut ws) else { return };
    let Some(id) = phone_id(&token) else {
        send(&mut ws, json!({"t": "bye", "reason": "Identificação do celular inválida. Recarregue a página."}));
        return;
    };
    let name = match clean_name(name.trim()) {
        n if n.trim().is_empty() => "celular".to_string(),
        n => n,
    };
    let home = SocketAddr::from((Ipv4Addr::LOCALHOST, ctx.port));
    let mut app = App::new(name, id, None, None, Ports::default());
    app.home = Some(home);
    app.connect(home);

    let (cols, rows) = clamp_size(cols, rows);
    let Ok(mut terminal) = Terminal::new(TestBackend::new(cols, rows)) else { return };
    let mut sent: Vec<String> = Vec::new();
    let reason = loop {
        if ctx.stop.load(Ordering::SeqCst) {
            break "O servidor foi desligado.";
        }
        let now = Instant::now();
        match receive(&mut ws) {
            Ok(Some(PageMsg::Key { k })) => {
                if let Some(key) = key_event(&k)
                    && !handle_key(&mut app, key, now)
                {
                    break "Você saiu do jogo.";
                }
            }
            Ok(Some(PageMsg::Resize { cols, rows })) => {
                let (cols, rows) = clamp_size(cols, rows);
                terminal.backend_mut().resize(cols, rows);
                sent.clear();
            }
            Ok(_) => {}
            Err(()) => break "",
        }
        app.step(Instant::now());
        if app.quit {
            break "Você saiu do jogo.";
        }
        if std::mem::take(&mut app.name_changed) && !send(&mut ws, json!({"t": "name", "v": app.name})) {
            break "";
        }
        if terminal.draw(|f| ui::draw(f, &app)).is_err() || !send_changes(&mut ws, terminal.backend().buffer(), &mut sent) {
            break "";
        }
    };
    app.shutdown();
    if !reason.is_empty() {
        send(&mut ws, json!({"t": "bye", "reason": reason}));
        let _ = ws.close(None);
        let _ = ws.flush();
    }
}

/// Sends the rows that changed since the last frame; an empty `sent` resends all of them.
fn send_changes(ws: &mut WebSocket<TcpStream>, buffer: &ratatui::buffer::Buffer, sent: &mut Vec<String>) -> bool {
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
        return true;
    }
    let size = if full { format!(",\"cols\":{cols},\"rows\":{rows}") } else { String::new() };
    ws.send(Message::text(format!("{{\"t\":\"frame\"{size},\"lines\":[{}]}}", changed.join(",")))).is_ok()
}
