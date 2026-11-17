//! Phone access: the page, the password, one phone at a time, mirror and takeover.

mod common;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use common::{DEADLINE, ports};
use pmog::app::App;
use pmog::input::handle_local_key;
use pmog::web::{PhoneSettings, Remote, key_event};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket};

const PHONE_A: &str = "0123456789abcdef0123456789abcdef";
const PHONE_B: &str = "fedcba9876543210fedcba9876543210";
const PASSWORD: &str = "mog67";

/// An offline PMOG with phone access on its own port.
fn pmog(slot: u16, password: Option<&str>) -> App {
    let mut settings = PhoneSettings::default();
    if let Some(p) = password {
        settings.set_password(p);
    }
    let mut app = App::new("Igor".into(), pmog::machine_id::for_bot("web"), None, None, ports(slot));
    app.remote = Some(Remote::start(ports(slot).tcp_start, settings));
    app
}

fn addr(app: &App) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], app.remote.as_ref().unwrap().port))
}

fn get(addr: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(stream, "GET {path} HTTP/1.1\r\nHost: pmog\r\n\r\n").unwrap();
    let mut body = String::new();
    stream.read_to_string(&mut body).unwrap();
    body
}

fn screen_of(app: &App) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| pmog::ui::draw_local(f, app)).unwrap();
    terminal.backend().buffer().content().iter().map(|c| c.symbol()).collect()
}

/// A phone browser: its socket and the screen it shows.
struct Phone {
    ws: WebSocket<TcpStream>,
    screen: Vec<String>,
    seen: Vec<Value>,
}

impl Phone {
    fn open(app: &App, token: &str) -> Phone {
        let stream = TcpStream::connect(addr(app)).unwrap();
        let (ws, _) = tungstenite::client(format!("ws://{}/ws", addr(app)), stream).unwrap();
        // timeout curto só depois do handshake: antes, um runner lento interrompe o aperto de mão
        ws.get_ref().set_read_timeout(Some(Duration::from_millis(20))).unwrap();
        let mut phone = Phone { ws, screen: Vec::new(), seen: Vec::new() };
        phone.send(json!({"t": "hello", "id": token, "cols": 100, "rows": 30}));
        phone
    }

    fn send(&mut self, value: Value) {
        self.ws.send(Message::text(value.to_string())).unwrap();
    }

    fn key(&mut self, k: &str) {
        self.send(json!({"t": "key", "k": k}));
    }

    fn text(&self) -> String {
        self.screen.join("\n")
    }

    /// Steps the PMOG and reads the socket until `done` holds.
    fn until(&mut self, app: &mut App, done: impl Fn(&Phone) -> bool) {
        let started = Instant::now();
        while !done(self) {
            assert!(started.elapsed() < DEADLINE, "o celular não chegou no esperado: {:?}\n{}", self.seen, self.text());
            app.step(Instant::now());
            match self.ws.read() {
                Ok(Message::Text(text)) => {
                    let msg: Value = serde_json::from_str(text.as_str()).unwrap();
                    if msg["t"] == "frame" {
                        if let Some(rows) = msg["rows"].as_u64() {
                            self.screen = vec![String::new(); rows as usize];
                        }
                        for line in msg["lines"].as_array().unwrap() {
                            let runs = line[1].as_array().unwrap();
                            self.screen[line[0].as_u64().unwrap() as usize] = runs.iter().map(|r| r[0].as_str().unwrap()).collect();
                        }
                    } else {
                        self.seen.push(msg);
                    }
                }
                Ok(_) | Err(tungstenite::Error::Io(_)) => {}
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }

    fn got(&self, kind: &str) -> Option<&Value> {
        self.seen.iter().rev().find(|m| m["t"] == kind)
    }

    fn bye(&self) -> Option<String> {
        self.got("bye").map(|m| m["reason"].as_str().unwrap().to_string())
    }

    fn login(&mut self, app: &mut App, password: &str) {
        self.until(app, |p| p.got("auth").is_some());
        self.seen.clear();
        self.send(json!({"t": "login", "password": password}));
    }

    /// Opens the page, types the password if asked and waits until the PMOG has the phone.
    fn enter(app: &mut App, token: &str) -> Phone {
        let mut phone = Phone::open(app, token);
        phone.until(app, |p| p.got("auth").is_some() || p.got("ready").is_some());
        if phone.got("auth").is_some() {
            phone.seen.clear();
            phone.send(json!({"t": "login", "password": PASSWORD}));
        }
        phone.until(app, |p| p.got("ready").is_some());
        let started = Instant::now();
        while !app.remote.as_ref().unwrap().connected() {
            assert!(started.elapsed() < DEADLINE, "o PMOG não registrou o celular");
            app.step(Instant::now());
        }
        phone
    }
}

fn local(app: &mut App, code: KeyCode) {
    handle_local_key(app, KeyEvent::new(code, KeyModifiers::NONE), Instant::now());
}

fn local_text(app: &mut App, text: &str) {
    text.chars().for_each(|c| local(app, KeyCode::Char(c)));
}

#[test]
fn this_pmog_serves_the_phone_page() {
    let app = pmog(40, None);
    let page = get(addr(&app), "/");
    assert!(page.starts_with("HTTP/1.1 200 OK") && page.contains("<title>PMOG</title>"), "{page}");
    assert!(get(addr(&app), "/nada").starts_with("HTTP/1.1 404"));
}

#[test]
fn without_a_password_the_phone_is_told_to_create_one_on_the_pc() {
    let mut app = pmog(41, None);
    let mut phone = Phone::open(&app, PHONE_A);
    phone.until(&mut app, |p| p.bye().is_some());
    assert!(phone.bye().unwrap().contains("ainda não tem senha"));
    assert!(!app.remote.as_ref().unwrap().connected());
}

#[test]
fn the_password_is_asked_once_per_browser_and_again_after_it_changes() {
    let mut app = pmog(42, Some(PASSWORD));
    let mut phone = Phone::open(&app, PHONE_A);
    phone.login(&mut app, "errada");
    phone.until(&mut app, |p| p.got("auth").is_some());
    assert_eq!(phone.got("auth").unwrap()["error"], "Senha errada.");
    phone.seen.clear();
    phone.send(json!({"t": "login", "password": PASSWORD}));
    phone.until(&mut app, |p| p.got("ready").is_some() && p.text().contains("Igor"));
    assert!(app.remote.as_ref().unwrap().connected());
    assert!(app.remote.as_ref().unwrap().take_dirty(), "o navegador lembrado precisa ser salvo");
    drop(phone);

    let started = Instant::now();
    while app.remote.as_ref().unwrap().connected() {
        assert!(started.elapsed() < DEADLINE, "o celular fechado continuou conectado");
        app.step(Instant::now());
    }
    let mut again = Phone::open(&app, PHONE_A);
    again.until(&mut app, |p| p.got("ready").is_some() || p.got("auth").is_some());
    assert!(again.got("auth").is_none(), "o mesmo navegador não digita a senha de novo");
    while !app.remote.as_ref().unwrap().connected() {
        app.step(Instant::now());
    }

    app.remote.as_mut().unwrap().set_password("nova-senha");
    again.until(&mut app, |p| p.bye().is_some());
    assert!(again.bye().unwrap().contains("senha do celular mudou"));
    let mut after = Phone::open(&app, PHONE_A);
    after.until(&mut app, |p| p.got("auth").is_some());
}

#[test]
fn only_one_phone_at_a_time() {
    let mut app = pmog(43, Some(PASSWORD));
    let mut first = Phone::enter(&mut app, PHONE_A);
    let mut second = Phone::open(&app, PHONE_B);
    second.login(&mut app, PASSWORD);
    second.until(&mut app, |p| p.bye().is_some());
    assert!(second.bye().unwrap().contains("Só um por vez"));
    assert!(first.bye().is_none());

    app.drop_phone("O PC desconectou o celular.");
    first.until(&mut app, |p| p.bye().is_some());
    Phone::enter(&mut app, PHONE_B);
}

#[test]
fn in_mirror_mode_phone_and_pc_drive_the_same_game() {
    let mut app = pmog(44, Some(PASSWORD));
    let mut phone = Phone::enter(&mut app, PHONE_A);
    phone.key("n");
    phone.until(&mut app, |p| p.text().contains("Seu nome"));
    assert!(app.editing_name.is_some(), "a tecla do celular chega na mesma sessão");
    local_text(&mut app, "x");
    phone.until(&mut app, |p| p.text().contains("Igorx"));
    assert!(!app.phone_controls());
    phone.key("Enter");
    phone.until(&mut app, |p| p.text().contains("Celular: conectado"));
    phone.key("e");
    phone.until(&mut app, |p| p.text().contains("modo assume"));
    assert!(app.remote.as_ref().unwrap().takeover(), "o celular também troca o modo");
}

#[test]
fn in_takeover_mode_the_pc_only_watches_until_it_takes_back() {
    let mut app = pmog(45, Some(PASSWORD));
    app.toggle_phone_mode();
    let mut phone = Phone::enter(&mut app, PHONE_A);
    app.step(Instant::now());
    assert!(app.phone_controls());
    local_text(&mut app, "n");
    assert!(app.editing_name.is_none(), "o PC não joga enquanto o celular assume");
    assert!(screen_of(&app).contains("Celular no controle"));
    assert!(!phone.text().contains("Celular no controle"), "o aviso é só da tela do PC");

    local_text(&mut app, "r");
    phone.until(&mut app, |p| p.bye().is_some());
    assert_eq!(phone.bye().unwrap(), "O PC retomou o controle.");
    assert!(!app.phone_controls());
}

#[test]
fn quitting_on_the_phone_keeps_the_pmog_running() {
    let mut app = pmog(46, Some(PASSWORD));
    let mut phone = Phone::enter(&mut app, PHONE_A);
    phone.key("q");
    phone.until(&mut app, |p| p.bye().is_some());
    assert!(phone.bye().unwrap().contains("continua aberto no PC"));
    assert!(!app.quit);
}

#[test]
fn wrong_passwords_lock_the_door_for_a_while() {
    let mut app = pmog(47, Some(PASSWORD));
    let mut phone = Phone::open(&app, PHONE_A);
    for _ in 0..pmog::constants::WEB_MAX_FAILURES {
        phone.login(&mut app, "chute");
    }
    phone.login(&mut app, PASSWORD);
    phone.until(&mut app, |p| p.got("auth").is_some());
    assert!(phone.got("auth").unwrap()["error"].as_str().unwrap().contains("Muitas tentativas"));
}

#[test]
fn the_pc_sets_the_password_with_a_masked_editor() {
    let mut app = pmog(48, None);
    local_text(&mut app, "cabc");
    local(&mut app, KeyCode::Enter);
    assert!(!app.remote.as_ref().unwrap().has_password(), "curta demais");
    local_text(&mut app, "c");
    local_text(&mut app, PASSWORD);
    let screen = screen_of(&app);
    assert!(screen.contains("•••••") && !screen.contains(PASSWORD), "a senha não aparece na tela");
    local(&mut app, KeyCode::Enter);
    let settings = app.remote.as_ref().unwrap().settings();
    assert!(settings.check_password(PASSWORD) && !settings.password_hash.contains(PASSWORD));
}

#[test]
fn the_lobby_shows_the_phone_address_once_there_is_a_password() {
    let mut app = pmog(49, None);
    assert!(screen_of(&app).contains("criar a senha"));
    app.remote.as_mut().unwrap().set_password(PASSWORD);
    let Some(ip) = pmog::utils::lan_ip() else { return };
    let url = pmog::web::page_url(ip, addr(&app).port());
    let screen = screen_of(&app);
    assert!(screen.contains(&url) && screen.contains("modo espelho"), "{screen}");
    app.toggle_phone_mode();
    assert!(screen_of(&app).contains("modo assume"));
}

#[test]
fn settings_remember_a_few_browsers_and_forget_them_on_a_new_password() {
    let mut settings = PhoneSettings::default();
    assert!(!settings.trusts(PHONE_A), "sem senha ninguém entra");
    settings.set_password(PASSWORD);
    assert!(!settings.check_password("outra"));
    for i in 0..20 {
        settings.trust(&format!("{i:032x}"));
    }
    assert_eq!(settings.trusted.len(), pmog::constants::WEB_MAX_TRUSTED);
    assert!(settings.trusts(&format!("{:032x}", 19)) && !settings.trusts(&format!("{:032x}", 0)));
    settings.set_password("nova");
    assert!(settings.trusted.is_empty());
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
