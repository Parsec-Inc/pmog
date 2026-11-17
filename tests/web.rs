//! Phone client: the page, the WebSocket session and the key mapping.

mod common;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use common::{DEADLINE, ports};
use pmog::server::ServerHandle;
use pmog::web::{key_event, phone_id};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket};

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

fn get(addr: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(stream, "GET {path} HTTP/1.1\r\nHost: pmog\r\n\r\n").unwrap();
    let mut body = String::new();
    stream.read_to_string(&mut body).unwrap();
    body
}

fn open(addr: SocketAddr) -> WebSocket<TcpStream> {
    let stream = TcpStream::connect(addr).unwrap();
    stream.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
    let (ws, _) = tungstenite::client(format!("ws://{addr}/ws"), stream).unwrap();
    ws
}

fn send(ws: &mut WebSocket<TcpStream>, value: Value) {
    ws.send(Message::text(value.to_string())).unwrap();
}

/// The screen as plain text, kept up to date from frame messages.
struct Screen(Vec<String>);

impl Screen {
    fn apply(&mut self, msg: &Value) {
        if let Some(rows) = msg["rows"].as_u64() {
            self.0 = vec![String::new(); rows as usize];
        }
        for line in msg["lines"].as_array().into_iter().flatten() {
            let y = line[0].as_u64().unwrap() as usize;
            self.0[y] = line[1].as_array().unwrap().iter().map(|run| run[0].as_str().unwrap()).collect();
        }
    }

    fn text(&self) -> String {
        self.0.join("\n")
    }
}

/// Reads messages until `done` holds for the screen, returning every non-frame message seen.
fn until(ws: &mut WebSocket<TcpStream>, screen: &mut Screen, done: impl Fn(&Screen) -> bool) -> Vec<Value> {
    let started = Instant::now();
    let mut others = Vec::new();
    while !done(screen) {
        assert!(started.elapsed() < DEADLINE, "a tela não chegou no esperado:\n{}", screen.text());
        match ws.read() {
            Ok(Message::Text(text)) => {
                let msg: Value = serde_json::from_str(text.as_str()).unwrap();
                if msg["t"] == "frame" { screen.apply(&msg) } else { others.push(msg) }
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(_)) => {}
            Err(e) => panic!("WebSocket caiu: {e}"),
        }
    }
    others
}

fn wait_for(done: impl Fn() -> bool, why: &str) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < DEADLINE, "{why}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn the_game_port_serves_the_phone_page() {
    let server = ServerHandle::start("Host", None, ports(40)).unwrap();
    let page = get(server.local_addr(), "/");
    assert!(page.starts_with("HTTP/1.1 200 OK"), "{page}");
    assert!(page.contains("<title>PMOG</title>"));
    assert!(get(server.local_addr(), "/nada").starts_with("HTTP/1.1 404"));
}

#[test]
fn a_phone_plays_through_the_websocket() {
    let server = ServerHandle::start("Casa", None, ports(41)).unwrap();
    let mut ws = open(server.local_addr());
    send(&mut ws, json!({"t": "hello", "id": TOKEN, "name": "Zap", "cols": 100, "rows": 30}));
    let mut screen = Screen(Vec::new());
    until(&mut ws, &mut screen, |s| s.text().contains("Casa") && s.text().contains("Zap"));
    assert_eq!(screen.0.len(), 30);
    assert!(screen.0.iter().all(|row| row.chars().count() == 100), "cada linha tem todas as colunas");
    wait_for(|| server.status().online.contains(&"Zap".to_string()), "o celular não apareceu online");

    send(&mut ws, json!({"t": "key", "k": "?"}));
    until(&mut ws, &mut screen, |s| s.text().contains("Como jogar"));

    send(&mut ws, json!({"t": "resize", "cols": 60, "rows": 20}));
    until(&mut ws, &mut screen, |s| s.0.len() == 20);
    assert!(screen.0.iter().all(|row| row.chars().count() == 60));
}

#[test]
fn quitting_on_the_phone_ends_the_session_and_frees_the_player() {
    let server = ServerHandle::start("Casa", None, ports(42)).unwrap();
    let mut ws = open(server.local_addr());
    send(&mut ws, json!({"t": "hello", "id": TOKEN, "name": "Zap", "cols": 100, "rows": 30}));
    let mut screen = Screen(Vec::new());
    until(&mut ws, &mut screen, |s| s.text().contains("Casa"));
    send(&mut ws, json!({"t": "key", "k": "q"}));
    let started = Instant::now();
    let bye = loop {
        assert!(started.elapsed() < DEADLINE, "a sessão não terminou");
        if let Ok(Message::Text(text)) = ws.read() {
            let msg: Value = serde_json::from_str(text.as_str()).unwrap();
            if msg["t"] == "bye" {
                break msg;
            }
        }
    };
    assert_eq!(bye["reason"], "Você saiu do jogo.");
    wait_for(|| server.status().online.is_empty(), "o jogador continuou online");
}

#[test]
fn a_bad_phone_token_is_refused() {
    let server = ServerHandle::start("Casa", None, ports(43)).unwrap();
    let mut ws = open(server.local_addr());
    send(&mut ws, json!({"t": "hello", "id": "não é hex", "name": "Zap", "cols": 100, "rows": 30}));
    let mut screen = Screen(Vec::new());
    let others = until(&mut ws, &mut screen, |_| true);
    let started = Instant::now();
    let mut seen = others;
    while !seen.iter().any(|m| m["t"] == "bye") {
        assert!(started.elapsed() < DEADLINE);
        if let Ok(Message::Text(text)) = ws.read() {
            seen.push(serde_json::from_str(text.as_str()).unwrap());
        }
    }
    assert!(server.status().online.is_empty());
}

#[test]
fn phone_ids_are_stable_and_validated() {
    let id = phone_id(TOKEN).unwrap();
    assert_eq!(phone_id(&TOKEN.to_uppercase()), Some(id.clone()));
    assert!(pmog::machine_id::is_valid(&id));
    assert_ne!(phone_id("fedcba9876543210fedcba9876543210"), Some(id));
    assert_eq!(phone_id("curto"), None);
    assert_eq!(phone_id("zz23456789abcdef0123456789abcdef"), None);
}

#[test]
fn page_keys_map_to_terminal_keys() {
    assert_eq!(key_event("ArrowUp").unwrap().code, KeyCode::Up);
    assert_eq!(key_event("Escape").unwrap().code, KeyCode::Esc);
    assert_eq!(key_event("ShiftTab").unwrap().code, KeyCode::BackTab);
    assert_eq!(key_event(" ").unwrap().code, KeyCode::Char(' '));
    assert_eq!(key_event("ç").unwrap().code, KeyCode::Char('ç'));
    let ctrl = key_event("Ctrl+n").unwrap();
    assert_eq!((ctrl.code, ctrl.modifiers), (KeyCode::Char('n'), KeyModifiers::CONTROL));
    assert!(key_event("Ctrl+").is_none());
    assert!(key_event("Unidentified").is_none());
    assert!(key_event("\u{7}").is_none());
}

fn render(app: &pmog::app::App, w: u16, h: u16) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| pmog::ui::draw(f, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..h).map(|y| (0..w).map(|x| buffer[(x, y)].symbol()).collect::<String>()).collect::<Vec<_>>().join("\n")
}

fn press(app: &mut pmog::app::App, code: KeyCode) {
    pmog::input::handle_key(app, ratatui::crossterm::event::KeyEvent::new(code, KeyModifiers::NONE), Instant::now());
}

#[test]
fn the_host_sees_the_phone_address_and_qr_code() {
    let p = ports(44);
    let mut host = pmog::app::App::new("Host".into(), pmog::machine_id::for_bot("web-host"), None, None, p);
    press(&mut host, KeyCode::Enter);
    assert!(host.host.is_some(), "{:?}", host.notice);
    press(&mut host, KeyCode::Char('s'));
    let Some(ip) = pmog::utils::lan_ip() else { return };
    let url = pmog::web::page_url(ip, host.host.as_ref().unwrap().port);
    let wide = render(&host, 140, 45);
    assert!(wide.contains("JOGAR NO CELULAR") && wide.contains(&url), "{wide}");
    assert!(wide.contains("▀▀▀▀▀▀▀▀"), "o QR code aparece ao lado: {wide}");
    let narrow = render(&host, 80, 30);
    assert!(narrow.contains(&url) && !narrow.contains("▀▀▀▀▀▀▀▀"), "{narrow}");
    host.shutdown();
}

#[test]
fn a_phone_lobby_only_offers_its_server() {
    let mut phone = pmog::app::App::new("Zap".into(), phone_id(TOKEN).unwrap(), None, None, ports(46));
    phone.home = Some(SocketAddr::from(([127, 0, 0, 1], ports(46).tcp_start)));
    assert_eq!(phone.lobby_rows(), vec![pmog::app::LobbyRow::Home]);
    let screen = render(&phone, 120, 30);
    assert!(screen.contains("Servidor deste computador") && !screen.contains("Hospedar"), "{screen}");
    press(&mut phone, KeyCode::Enter);
    assert!(phone.notice.is_some(), "sem servidor, Enter avisa que não conectou");
}
