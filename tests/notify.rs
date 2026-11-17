//! Desktop notifications: event mapping, focus policy, throttling, toggle and sanitizing.
//! Every test delivers into a fake sink; nothing reaches the desktop or the terminal.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pmog::app::{App, Screen};
use pmog::constants::*;
use pmog::game::{Game, Overlay, Player, Tone};
use pmog::input::handle_key;
use pmog::machine_id;
use pmog::notify::*;
use pmog::protocol::{InviteView, ServerMsg, StateView};
use pmog::save;
use pmog::server::Ports;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

type Log = Rc<RefCell<Vec<(Notice, Urgency)>>>;

struct FakeSink(Log);

impl Sink for FakeSink {
    fn deliver(&mut self, notice: &Notice, urgency: Urgency) {
        self.0.borrow_mut().push((notice.clone(), urgency));
    }
}

fn notifier(now: Instant) -> (Notifier, Log) {
    let log = Log::default();
    (Notifier::new(Box::new(FakeSink(log.clone())), now), log)
}

fn away(now: Instant) -> (Notifier, Log) {
    let (mut n, log) = notifier(now);
    n.focus.lost();
    (n, log)
}

fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

fn notice(title: &str) -> Notice {
    Notice { title: title.into(), body: "corpo".into() }
}

fn event(game: &mut Game, text: &str, banner: Option<&str>, hit: bool) {
    let msg = ServerMsg::Event { tone: Tone::Bad, text: text.into(), banner: banner.map(String::from), hit };
    game.apply(msg, Instant::now());
}

fn state(invites: Vec<InviteView>) -> ServerMsg {
    ServerMsg::State(Box::new(StateView {
        me: Player { name: "Igor".into(), ..Default::default() },
        streak: 0,
        cooldown_ms: 0,
        cooldown_total_ms: 0,
        immune_ms: 0,
        preworkout_ms: 0,
        cologne_ms: 0,
        aura_farm_ms: 0,
        meter_ms: 0,
        players: Vec::new(),
        duel: None,
        invites,
        outgoing: None,
        queued: None,
        rtt_ms: 0.0,
        clan_tag: None,
    }))
}

fn invite(peer: i64, name: &str) -> InviteView {
    InviteView { peer, name: name.into(), fraction: 0.5, stake: 120.0, expires_ms: 10_000, draft: false }
}

fn offline_app() -> App {
    App::new("Igor".into(), machine_id::for_bot("notify"), None, None, Ports::default())
}

// Event mapping

#[test]
fn being_mogged_makes_a_notice() {
    let mut game = Game::new(Player::default(), Instant::now());
    event(&mut game, "Rival te moggou! -50 MP (imune por 5 s)", Some("MOGGADO POR RIVAL"), true);
    assert_eq!(game.notices, vec![Notice::new(NOTIFY_TITLE_MOGGED, "Rival te moggou! -50 MP (imune por 5 s)")]);
}

#[test]
fn own_reverse_mog_and_plain_events_do_not_notify() {
    let mut game = Game::new(Player::default(), Instant::now());
    event(&mut game, "Rival te frame-moggou de volta.", Some("REVERSE MOG"), true);
    event(&mut game, "Você moggou Rival!", None, false);
    event(&mut game, "Cooldown!", Some("Cooldown! Respira."), false);
    assert!(game.notices.is_empty());
}

#[test]
fn duel_results_make_notices() {
    let mut game = Game::new(Player::default(), Instant::now());
    event(&mut game, "Você venceu o duelo contra Rival!", Some("VITÓRIA NO DUELO"), false);
    event(&mut game, "Rival venceu o duelo.", Some("DERROTA NO DUELO"), true);
    event(&mut game, "Duelo empatado.", Some("EMPATE"), false);
    let titles: Vec<&str> = game.notices.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(titles, [NOTIFY_TITLE_DUEL_WON, NOTIFY_TITLE_DUEL_LOST, NOTIFY_TITLE_DUEL_DRAW]);
}

#[test]
fn rank_up_and_achievement_make_notices() {
    let mut game = Game::new(Player::default(), Instant::now());
    event(&mut game, "Você subiu para LTN! Novos itens na loja.", Some("RANK UP: LTN"), false);
    event(&mut game, "Conquista: Primeiro mog (moggue alguém)", None, false);
    assert_eq!(game.notices[0].title, NOTIFY_TITLE_RANK);
    assert_eq!(game.notices[1], Notice::new(NOTIFY_TITLE_ACHIEVEMENT, "Primeiro mog (moggue alguém)"));
}

#[test]
fn invite_notifies_once_until_it_goes_away() {
    let mut game = Game::new(Player::default(), Instant::now());
    let now = Instant::now();
    game.apply(state(vec![invite(2, "Rival")]), now);
    game.apply(state(vec![invite(2, "Rival")]), now);
    assert_eq!(game.notices.len(), 1);
    assert_eq!(game.notices[0].title, NOTIFY_TITLE_INVITE);
    assert!(game.notices[0].body.contains("Rival"));
    game.apply(state(vec![invite(2, "Rival"), invite(3, "Outro")]), now);
    assert_eq!(game.notices.len(), 2);
    game.apply(state(Vec::new()), now);
    game.apply(state(vec![invite(2, "Rival")]), now);
    assert_eq!(game.notices.len(), 3);
}

#[test]
fn undrained_notices_stay_bounded() {
    let mut game = Game::new(Player::default(), Instant::now());
    for _ in 0..NOTIFY_QUEUE_MAX * 2 {
        event(&mut game, "R te moggou!", Some("MOGGADO POR R"), true);
    }
    assert_eq!(game.notices.len(), NOTIFY_QUEUE_MAX);
}

#[test]
fn losing_the_connection_notifies_through_the_app() {
    let mut app = offline_app();
    let now = Instant::now();
    let (n, log) = away(now);
    app.notifier = n;
    app.game.notices.push(Notice::disconnected("O servidor foi desligado."));
    app.step(now);
    let log = log.borrow();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].0.title, NOTIFY_TITLE_DISCONNECTED);
    assert_eq!(log[0].1, Urgency::Critical);
}

// Focus policy

#[test]
fn focused_window_never_notifies() {
    let now = Instant::now();
    let (mut n, log) = notifier(now);
    n.focus.gained();
    n.pump(vec![notice("a")], now + secs(600));
    assert!(log.borrow().is_empty());
}

#[test]
fn focus_lost_delivers_and_focus_gained_stops() {
    let now = Instant::now();
    let (mut n, log) = away(now);
    n.pump(vec![notice("a")], now);
    assert_eq!(log.borrow().len(), 1);
    n.focus.gained();
    n.pump(vec![notice("b")], now + secs(1));
    assert_eq!(log.borrow().len(), 1);
}

#[test]
fn unknown_focus_waits_for_idle_threshold() {
    let now = Instant::now();
    let (mut n, log) = notifier(now);
    n.pump(vec![notice("a")], now + NOTIFY_IDLE_AWAY - secs(1));
    assert!(log.borrow().is_empty());
    n.pump(vec![notice("b")], now + NOTIFY_IDLE_AWAY);
    assert_eq!(log.borrow().len(), 1);
    n.focus.key_pressed(now + NOTIFY_IDLE_AWAY + secs(1));
    n.pump(vec![notice("c")], now + NOTIFY_IDLE_AWAY + secs(2));
    assert_eq!(log.borrow().len(), 1);
}

#[test]
fn key_press_after_lost_focus_means_focused_again() {
    let now = Instant::now();
    let (mut n, log) = away(now);
    n.focus.key_pressed(now);
    n.pump(vec![notice("a")], now);
    assert!(log.borrow().is_empty());
}

// Throttle and coalescing

#[test]
fn burst_becomes_one_summary() {
    let now = Instant::now();
    let (mut n, log) = away(now);
    let mog = |who: &str| Notice::new(NOTIFY_TITLE_MOGGED, format!("{who} te moggou!"));
    n.pump(vec![mog("A")], now);
    for i in 0..9 {
        n.pump(vec![mog("B")], now + Duration::from_millis(100 * i));
    }
    assert_eq!(log.borrow().len(), 1);
    n.pump(Vec::new(), now + NOTIFY_THROTTLE);
    let log = log.borrow();
    assert_eq!(log.len(), 2);
    assert_eq!(log[1].0.title, NOTIFY_TITLE_MOGGED);
    assert!(log[1].0.body.starts_with("Você foi moggado 9 vezes."), "{}", log[1].0.body);
}

#[test]
fn different_titles_do_not_throttle_each_other() {
    let now = Instant::now();
    let (mut n, log) = away(now);
    n.pump(vec![notice("a"), notice("b"), notice("a")], now);
    let titles: Vec<String> = log.borrow().iter().map(|(n, _)| n.title.clone()).collect();
    assert_eq!(titles, ["a", "b"]);
}

#[test]
fn pending_queue_is_bounded() {
    let now = Instant::now();
    let (mut n, log) = away(now);
    n.pump(vec![notice("first")], now);
    n.pump((0..NOTIFY_MAX_PENDING * 3).map(|i| notice(&format!("t{i}"))).collect(), now);
    assert_eq!(log.borrow().len(), 1 + NOTIFY_MAX_PENDING);
}

#[test]
fn regaining_focus_drops_waiting_notices() {
    let now = Instant::now();
    let (mut n, log) = away(now);
    n.pump(vec![notice("a")], now);
    n.pump(vec![notice("a")], now + secs(1));
    n.focus.gained();
    n.pump(Vec::new(), now + secs(2));
    n.focus.lost();
    n.pump(Vec::new(), now + NOTIFY_THROTTLE + secs(1));
    assert_eq!(log.borrow().len(), 1);
}

#[test]
fn urgency_follows_kind() {
    assert_eq!(Kind::of(&Notice::new(NOTIFY_TITLE_RANK, "")).urgency(), Urgency::Low);
    assert_eq!(Kind::of(&Notice::new(NOTIFY_TITLE_MOGGED, "")).urgency(), Urgency::Normal);
    assert_eq!(Kind::of(&Notice::new("Mensagem de Fulano", "")).urgency(), Urgency::Normal);
}

// Toggle and config

#[test]
fn disabled_delivers_nothing() {
    let now = Instant::now();
    let (mut n, log) = away(now);
    n.enabled = false;
    n.pump(vec![notice("a")], now);
    assert!(log.borrow().is_empty());
}

#[test]
fn chat_style_notice_is_delivered_by_the_app() {
    let mut app = offline_app();
    let now = Instant::now();
    let (n, log) = away(now);
    app.notifier = n;
    app.game.notices.push(Notice { title: "Fulano".into(), body: "oi".into() });
    app.step(now);
    assert!(app.game.notices.is_empty());
    assert_eq!(log.borrow()[0].0, Notice::new("Fulano", "oi"));
}

#[test]
fn default_app_never_delivers_anywhere() {
    let mut app = offline_app();
    app.notifier.focus.lost();
    app.game.notices.push(notice("a"));
    app.step(Instant::now());
    assert!(app.game.notices.is_empty());
}

#[test]
fn b_key_in_server_panel_toggles_and_flags_the_config() {
    let mut app = offline_app();
    app.screen = Screen::Playing;
    app.game.overlay = Overlay::Server;
    assert!(app.notifier.enabled);
    handle_key(&mut app, KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE), Instant::now());
    assert!(!app.notifier.enabled);
    assert!(app.notifications_changed);
    assert_eq!(app.game.overlay, Overlay::Server);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| pmog::ui::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let screen: String = (0..40).map(|y| (0..120).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>()).collect();
    assert!(screen.contains("Notificações do sistema: desligadas (B)"), "{screen}");
    handle_key(&mut app, KeyEvent::new(KeyCode::Char('B'), KeyModifiers::NONE), Instant::now());
    assert!(app.notifier.enabled);
}

#[test]
fn config_defaults_on_and_persists_the_toggle() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("notify-{}", std::process::id()));
    let path = dir.join("config.json");
    assert!(save::load_from(&path).notifications);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&path, br#"{"name":"Igor"}"#).unwrap();
    let old = save::load_from(&path);
    assert!(old.notifications);
    assert_eq!(old.name, "Igor");
    save::store_to(&path, &save::Config { name: "Igor".into(), notifications: false, ..Default::default() });
    let back = save::load_from(&path);
    assert!(!back.notifications);
    assert_eq!(back.name, "Igor");
    let _ = std::fs::remove_dir_all(&dir);
}

// Sanitizing

#[test]
fn sanitize_strips_escapes_and_controls() {
    let text = sanitize("a\x1b]777;notify;x;y\x07b\nc\u{202e}d\0", 100);
    assert!(!text.chars().any(|c| c.is_control() || c == '\u{202e}'), "{text:?}");
    assert_eq!(text, "a]777;notify;x;yb cd");
}

#[test]
fn sanitize_caps_length_in_chars() {
    let text = sanitize(&"é".repeat(50), 10);
    assert_eq!(text.chars().count(), 10);
    assert!(text.ends_with('…'));
    assert_eq!(sanitize("curto", 10), "curto");
}

#[test]
fn leading_dash_stays_after_the_separator() {
    let args = notify_send_args(&Notice::new("-t 1", "--help"), Urgency::Normal);
    let sep = args.iter().position(|a| a == "--").unwrap();
    assert_eq!(&args[sep + 1..], ["-t 1", "--help"]);
    assert_eq!(&args[..sep], ["-a", "PMOG", "-u", "normal"]);
}

#[test]
fn delivered_text_is_sanitized_before_the_sink() {
    let now = Instant::now();
    let (mut n, log) = away(now);
    n.pump(vec![Notice::new("T\x1b[2J", "x".repeat(1000))], now);
    let log = log.borrow();
    assert_eq!(log[0].0.title, "T[2J");
    assert_eq!(log[0].0.body.chars().count(), NOTIFY_BODY_MAX);
}

#[test]
fn mac_notifications_pass_the_text_as_arguments() {
    let notice = pmog::notify::Notice { title: "PMOG \"aspas\"".into(), body: "end run; do shell script \"x\"".into() };
    let args = pmog::notify::macos_notify_args(&notice);
    let script: Vec<&String> = args.iter().skip(1).step_by(2).take(3).collect();
    assert!(script.iter().all(|line| !line.contains("aspas") && !line.contains("do shell")), "text never enters the script");
    assert_eq!(&args[args.len() - 2..], [notice.title.clone(), notice.body.clone()]);
}
