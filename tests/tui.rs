//! Renders the TUI headlessly and drives it with key events.

mod common;

use std::thread;
use std::time::{Duration, Instant};

use common::*;
use pmog::app::{App, Confirm, LobbyRow, Screen};
use pmog::discovery::Discovery;
use pmog::game::{Achievement, Overlay, Player};
use pmog::input::handle_key;
use pmog::items::ItemId;
use pmog::machine_id;
use pmog::protocol::{ClientMsg, PeerView, ServerMsg, StateView};
use pmog::server::Ports;
use pmog::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn offline_app() -> App {
    App::new("Igor".into(), machine_id::for_bot("tui"), None, None, Ports::default())
}

/// App on the game screen with one rival, without any server.
fn playing() -> App {
    let mut app = offline_app();
    app.screen = Screen::Playing;
    let view = StateView {
        me: Player { name: "Igor".into(), ..Default::default() },
        streak: 0,
        cooldown_ms: 0,
        cooldown_total_ms: 0,
        immune_ms: 0,
        preworkout_ms: 0,
        cologne_ms: 0,
        aura_farm_ms: 0,
        statuses: vec![],
        meter_ms: 0,
        players: vec![PeerView { id: 2, name: "Rival".into(), mp: 0.0, lifetime: 0.0, attack: 10.0, defense: 10.0, streak: 0, in_duel: false, immune: false, immune_ms: 0, clan_tag: None, statuses: Vec::new() }],
        duel: None,
        invites: Vec::new(),
        outgoing: None,
        queued: None,
        rtt_ms: 0.0,
        clan_tag: None,
    };
    app.game.apply(ServerMsg::State(Box::new(view)), Instant::now());
    app
}

fn render(app: &App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..h)
        .map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
        .replace('\u{a0}', " ")
}

fn press(app: &mut App, code: KeyCode) -> bool {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE), Instant::now())
}

#[test]
fn main_screen_shows_everything() {
    let a = playing();
    let screen = render(&a, 170, 42);
    for needle in ["PMOG", "Igor", "Sub-5", "NPC de Treino", "Rival", "No servidor (1)", "███╗", "PERFECT x2", "Cooldown", "PRONTO", "Feed", "mogar"] {
        assert!(screen.contains(needle), "missing {needle:?}\n{screen}");
    }
}

#[test]
fn the_inventory_lists_owned_items_under_the_feed() {
    let mut a = playing();
    assert!(render(&a, 170, 42).contains("Nada ainda. L abre a loja."));
    a.game.player.items.insert(ItemId::Mewing, 3);
    a.game.player.items.insert(ItemId::EnergyDrink, 2);
    let screen = render(&a, 170, 42);
    let row = |needle: &str| screen.lines().position(|l| l.contains(needle)).unwrap_or_else(|| panic!("missing {needle:?}\n{screen}"));
    assert!(row("Feed") < row("Inventário"));
    assert!(row("Inventário") < row("1  Energético") && row("1  Energético") < row("Tutorial de Mewing"));
    assert!(screen.lines().nth(row("Tutorial de Mewing")).unwrap().contains("3/50"));
    for i in 0..40 {
        a.game.player.items.insert(pmog::game::CATALOG[i].id, 1);
    }
    assert!(render(&a, 170, 42).contains("itens"));
}

#[test]
fn o_hides_the_inventory_panel_and_i_opens_it_by_tab() {
    let mut a = playing();
    a.game.player.items.insert(ItemId::Mewing, 3);
    a.game.player.items.insert(ItemId::EnergyDrink, 2);
    press(&mut a, KeyCode::Char('o'));
    assert!(!a.inventory_panel && a.settings_changed);
    assert!(!render(&a, 170, 42).contains("Inventário"));
    press(&mut a, KeyCode::Char('o'));
    assert!(render(&a, 170, 42).contains("Inventário"));

    press(&mut a, KeyCode::Char('i'));
    assert_eq!(a.game.overlay, Overlay::Inventory);
    let screen = render(&a, 170, 42);
    for needle in ["Rosto ·1", "Consumíveis ·1", "Tutorial de Mewing", "Cada nível comprado dá", "Nível 3 de 50."] {
        assert!(screen.contains(needle), "missing {needle:?}\n{screen}");
    }
    assert!(!screen.contains("Energético  "), "other tabs stay out of the list");
    press(&mut a, KeyCode::Left);
    assert_eq!(a.game.inventory_tab, pmog::items::Category::Consumable);
    assert!(render(&a, 170, 42).contains("Enter usa agora."));
    press(&mut a, KeyCode::Left);
    assert!(render(&a, 170, 42).contains("Nenhum item de Brainrot e 67 ainda."));
    for (w, h) in [(50, 45), (80, 24), (20, 8)] {
        render(&a, w, h);
    }
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.game.overlay, Overlay::None);
}

#[test]
fn every_overlay_renders() {
    let mut a = playing();
    a.game.player.achievements.push(Achievement::FirstMog);
    for (overlay, needle) in [
        (Overlay::Shop, "Tutorial de Mewing"),
        (Overlay::Achievements, "[x] Primeiro Mog"),
        (Overlay::Help, "OBJETIVO"),
    ] {
        a.game.overlay = overlay;
        let screen = render(&a, 170, 42);
        assert!(screen.contains(needle), "{overlay:?} missing {needle:?}");
    }
}

#[test]
fn the_lobby_shows_a_blocking_firewall_and_its_fix() {
    let mut app = offline_app();
    app.firewall = pmog::firewall::firewalld_warning("ssh", "", true);
    let text = render(&app, 160, 40);
    for needle in ["FIREWALL", "O firewalld está bloqueando o PMOG", "sudo firewall-cmd --permanent"] {
        assert!(text.contains(needle), "missing {needle:?}:\n{text}");
    }
    for (w, h) in [(50, 45), (100, 22), (80, 24), (20, 8)] {
        render(&app, w, h);
    }
}

#[test]
fn renders_at_any_size_without_panicking() {
    let mut a = playing();
    a.game.player.lifetime = 1e12;
    a.game.player.mp = 1e12;
    for overlay in [Overlay::None, Overlay::Shop, Overlay::Achievements, Overlay::Help] {
        a.game.overlay = overlay;
        for (w, h) in [(1, 1), (10, 5), (40, 12), (80, 24), (120, 30), (300, 80)] {
            render(&a, w, h);
        }
    }
}

#[test]
fn small_terminal_uses_compact_button() {
    let a = playing();
    let screen = render(&a, 80, 24);
    assert!(screen.contains("M O G A R"));
}

#[test]
fn shop_preview_shows_selected_item_sprite() {
    let mut a = playing();
    a.game.overlay = Overlay::Shop;
    for cursor in 0..pmog::game::CATALOG.len() {
        a.game.shop_cursor = cursor;
        let def = &pmog::game::CATALOG[cursor];
        let screen = render(&a, 170, 42);
        assert!(screen.contains(def.name), "preview missing {}", def.name);
        assert!(screen.contains('▀') || screen.contains('▄'), "no sprite for {}", def.name);
    }
    let narrow = render(&a, 60, 30);
    assert!(!narrow.contains('▀'), "preview hidden on narrow terminals");
}

#[test]
fn locked_item_sprite_is_grayscale() {
    use ratatui::style::Color;
    let mut a = playing();
    a.game.overlay = Overlay::Shop;
    let locked = pmog::game::CATALOG.iter().position(|d| d.id == ItemId::DiamondJaw).unwrap();
    let open = pmog::game::CATALOG.iter().position(|d| d.id == ItemId::GuaSha).unwrap();
    let colors = |a: &App| {
        let mut terminal = Terminal::new(TestBackend::new(170, 42)).unwrap();
        terminal.draw(|f| ui::draw(f, a)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .filter(|c| c.symbol() == "▀" || c.symbol() == "▄")
            .map(|c| c.fg)
            .collect::<Vec<_>>()
    };
    a.game.shop_cursor = locked;
    assert!(colors(&a).iter().all(|c| matches!(c, Color::Rgb(r, gg, b) if r == gg && gg == b)));
    a.game.shop_cursor = open;
    assert!(colors(&a).iter().any(|c| matches!(c, Color::Rgb(r, gg, b) if !(r == gg && gg == b))));
}

#[test]
fn help_has_every_page_and_scrolls() {
    let mut a = playing();
    press(&mut a, KeyCode::Char('?'));
    let expected = ["OBJETIVO", "UM MOG, PASSO A PASSO", "MOGGING POINTS", "RANKS", "COMO A LOJA FUNCIONA", "DUELO DIRETO", "CHAT (T)", "GÍRIAS DO JOGO"];
    for (i, needle) in expected.iter().enumerate() {
        assert_eq!(a.game.help_page, i);
        let screen = render(&a, 120, 40);
        assert!(screen.contains(needle), "page {i} missing {needle}");
        press(&mut a, KeyCode::Right);
    }
    assert_eq!(a.game.help_page, 0, "wraps around");
    press(&mut a, KeyCode::Left);
    assert_eq!(a.game.help_page, pmog::help::PAGES.len() - 1);

    // Small terminal: page 2 (combat) must scroll, and scrolling stops at the end.
    press(&mut a, KeyCode::Char('2'));
    render(&a, 80, 20);
    let max = a.game.help_max_scroll.get();
    assert!(max > 0);
    for _ in 0..500 {
        press(&mut a, KeyCode::Down);
    }
    assert_eq!(a.game.help_scroll, max);
    let bottom = render(&a, 80, 20);
    assert!(bottom.contains("SEUS NÚMEROS AGORA"));
    assert!(!bottom.contains("tem mais abaixo"));
    press(&mut a, KeyCode::Right);
    assert_eq!(a.game.help_scroll, 0, "new page starts at the top");
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.game.overlay, Overlay::None);
}

#[test]
fn help_numbers_follow_constants_and_player() {
    let mut a = playing();
    a.game.overlay = Overlay::Help;
    a.game.help_page = 1;
    a.game.player.items.insert(ItemId::MidFade, 2);
    let screen = render(&a, 200, 80);
    assert!(screen.contains("x1,6"), "perfect multiplier");
    assert!(screen.contains("até x1,18"), "best non-perfect multiplier");
    assert!(screen.contains("máximo 5,0k"), "pt-BR decimal comma");
    assert!(screen.contains("Poder 20"), "player attack with 2 Mid-Fades");
}

#[test]
fn shop_preview_explains_purchase_impact() {
    let mut a = playing();
    a.game.overlay = Overlay::Shop;
    a.game.player.lifetime = 1e9;
    a.game.player.mp = 1e9;
    let at = |id| pmog::game::CATALOG.iter().position(|d| d.id == id).unwrap();

    a.game.shop_cursor = at(ItemId::MidFade);
    let screen = render(&a, 170, 50);
    assert!(screen.contains("O QUE FAZ") && screen.contains("+5 de poder"));
    assert!(screen.contains("Poder: 10 → 15"), "{screen}");

    a.game.shop_cursor = at(ItemId::Therapy);
    assert!(render(&a, 170, 50).contains("Proteção nas perdas: 0% → 5%"));

    a.game.shop_cursor = at(ItemId::PocketMirror);
    let screen = render(&a, 170, 50);
    assert!(screen.contains("Tecla 5") && !screen.contains("COM A COMPRA"));

    a.game.player.lifetime = 0.0;
    a.game.shop_cursor = at(ItemId::MogCrown);
    assert!(render(&a, 170, 50).contains("Bloqueado: precisa do rank Ascendido"));
}

#[test]
fn meter_uses_fixed_rgb_tier_colors_and_legend() {
    use ratatui::style::Color;
    let a = playing();
    let mut terminal = Terminal::new(TestBackend::new(170, 42)).unwrap();
    terminal.draw(|f| ui::draw(f, &a)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let bar: Vec<Color> = buf.content().iter().filter(|c| c.symbol() == "▬").map(|c| c.fg).collect();
    assert!(!bar.is_empty());
    assert!(bar.iter().all(|c| matches!(c, Color::Rgb(..))), "theme-proof colors");
    let distinct: std::collections::HashSet<_> = bar.iter().map(|c| format!("{c:?}")).collect();
    assert_eq!(distinct.len(), 4, "four tiers visible");
    let screen = render(&a, 170, 42);
    assert!(screen.contains("RUIM x0,5") && screen.contains("PERFECT x2"));
}

#[test]
fn status_flags_and_banner_render() {
    let mut a = playing();
    let now = Instant::now();
    a.game.player.shield_armed = true;
    a.game.select_move(1);
    a.game.try_mog(now);
    let screen = render(&a, 170, 42);
    for needle in ["ESCUDO", "Alvo: Rival"] {
        assert!(screen.contains(needle), "missing {needle:?}\n{screen}");
    }
    assert!(!screen.contains("PRONTO"), "local cooldown guess runs until the server answers");
}

#[test]
fn keys_send_actions_to_the_server() {
    let mut a = playing();
    assert!(press(&mut a, KeyCode::Char('m')));
    assert_eq!(a.game.outbox.last(), Some(&ClientMsg::Mew));

    press(&mut a, KeyCode::Down);
    assert_eq!(a.game.peers[a.game.selected].name, "Rival");
    press(&mut a, KeyCode::Char(' '));
    assert!(matches!(a.game.outbox.last(), Some(ClientMsg::Mog { target: 2, .. })));
    let sent = a.game.outbox.len();
    press(&mut a, KeyCode::Char(' '));
    assert_eq!(a.game.outbox.len(), sent, "cooldown blocks spamming the server");

    press(&mut a, KeyCode::Char('l'));
    assert_eq!(a.game.overlay, Overlay::Shop);
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.game.shop_confirm, Some(0), "Enter asks first");
    press(&mut a, KeyCode::Esc);
    assert_eq!((a.game.shop_confirm, a.game.overlay), (None, Overlay::Shop), "Esc closes the card, not the shop");
    press(&mut a, KeyCode::Down);
    assert_eq!(a.game.shop_cursor, 1);
    assert!(press(&mut a, KeyCode::Char('q')), "q in shop closes it, does not quit");
    assert_eq!(a.game.overlay, Overlay::None);

    press(&mut a, KeyCode::Char('?'));
    assert_eq!(a.game.overlay, Overlay::Help);
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.game.overlay, Overlay::None);
}

#[test]
fn consumable_keys_only_send_what_you_own() {
    let mut a = playing();
    press(&mut a, KeyCode::Char('1'));
    assert!(a.game.outbox.is_empty());
    assert!(a.game.feed.iter().any(|l| l.text.contains("Você não tem")));
    a.game.player.items.insert(ItemId::SigmaShield, 1);
    press(&mut a, KeyCode::Char('2'));
    assert_eq!(a.game.outbox.last(), Some(&ClientMsg::Use { item: ItemId::SigmaShield }));
}

#[test]
fn esc_never_quits_and_q_does() {
    let mut a = playing();
    assert!(press(&mut a, KeyCode::Esc));
    assert!(!press(&mut a, KeyCode::Char('Q')));
    let mut a = offline_app();
    assert!(press(&mut a, KeyCode::Down));
    assert!(!handle_key(&mut a, KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL), Instant::now()));
}

#[test]
fn fast_esc_then_key_closes_overlay_first() {
    let mut a = playing();
    a.game.overlay = Overlay::Shop;
    assert!(handle_key(&mut a, KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT), Instant::now()));
    assert_eq!(a.game.overlay, Overlay::Achievements);
}

#[test]
fn lobby_renders_and_edits_the_name() {
    let mut a = offline_app();
    let screen = render(&a, 120, 30);
    for needle in ["Mundos e servidores", "Criar mundo novo", "Igor", "Computador:", "Não consegui escutar"] {
        assert!(screen.contains(needle), "missing {needle:?}\n{screen}");
    }
    press(&mut a, KeyCode::Char('n'));
    assert!(render(&a, 120, 30).contains("Seu nome"));
    for _ in 0..10 {
        press(&mut a, KeyCode::Backspace);
    }
    for c in "Novo Nome".chars() {
        press(&mut a, KeyCode::Char(c));
    }
    assert!(press(&mut a, KeyCode::Char('q')), "typing q in the editor doesn't quit");
    press(&mut a, KeyCode::Backspace);
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.name, "Novo Nome");
    assert!(a.name_changed);
    press(&mut a, KeyCode::Char('n'));
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.name, "Novo Nome", "Esc cancels");
    for (w, h) in [(1, 1), (20, 6), (60, 20), (300, 80)] {
        render(&a, w, h);
    }
}

#[test]
#[cfg_attr(target_os = "macos", ignore = "o runner do macOS no CI não entrega o beacon UDP para a própria máquina")]
fn hosting_and_joining_through_the_interface() {
    let p = ports(12);
    let mut host = App::new("Host".into(), machine_id::for_bot("host"), Discovery::start(p.discovery).ok(), None, p);
    assert_eq!(host.lobby_rows(), vec![LobbyRow::Host]);
    press(&mut host, KeyCode::Enter);
    assert!(render(&host, 120, 30).contains("Nome do mundo novo"), "Enter asks for the world name");
    press(&mut host, KeyCode::Enter);
    assert_eq!(host.screen, Screen::Playing, "{:?}", host.notice);
    assert_eq!(host.hosts.len(), 1);

    let mut guest = App::new("Convidado".into(), machine_id::for_bot("guest"), Discovery::start(p.discovery).ok(), None, p);
    let start = Instant::now();
    while start.elapsed() < DEADLINE && !guest.lobby_rows().iter().any(|r| matches!(r, LobbyRow::Server(_))) {
        guest.step(Instant::now());
        host.step(Instant::now());
        thread::sleep(Duration::from_millis(50));
    }
    let rows = guest.lobby_rows();
    assert!(matches!(&rows[0], LobbyRow::Server(e) if e.name == "Servidor de Host"), "{rows:?}");
    assert!(render(&guest, 120, 30).contains("Servidor de Host"));
    press(&mut guest, KeyCode::Enter);
    assert_eq!(guest.screen, Screen::Playing, "{:?}", guest.notice);

    let both = |host: &mut App, guest: &mut App, done: &dyn Fn(&App, &App) -> bool| {
        let start = Instant::now();
        while start.elapsed() < DEADLINE && !done(host, guest) {
            host.step(Instant::now());
            guest.step(Instant::now());
            thread::sleep(Duration::from_millis(30));
        }
        done(host, guest)
    };
    let state = |h: &App, g: &App| {
        let feed = |a: &App| a.game.feed.iter().take(5).map(|l| l.text.clone()).collect::<Vec<_>>();
        format!(
            "host: screen {:?} notice {:?} peers {} feed {:?}\nguest: screen {:?} notice {:?} peers {} feed {:?}\nonline {:?}",
            h.screen, h.notice, h.game.peers.len(), feed(h), g.screen, g.notice, g.game.peers.len(), feed(g),
            h.hosts.first().map(|s| s.status().online)
        )
    };
    assert!(both(&mut host, &mut guest, &|h, g| h.game.peers.len() == 2 && g.game.peers.len() == 2), "{}", state(&host, &guest));
    assert!(both(&mut host, &mut guest, &|h, _| h.hosts[0].status().online.len() == 2), "{}", state(&host, &guest));
    assert!(matches!(host.lobby_rows().first(), Some(LobbyRow::Own(_))), "own server first, not duplicated");

    // Server panel shows hosting details.
    press(&mut host, KeyCode::Char('s'));
    let panel = render(&host, 150, 45);
    assert!(panel.contains("VOCÊ HOSPEDA") && panel.contains("Convidado"), "{panel}");
    press(&mut host, KeyCode::Esc);

    // Renaming in game reaches the other player.
    press(&mut guest, KeyCode::Char('s'));
    press(&mut guest, KeyCode::Char('n'));
    for _ in 0..20 {
        press(&mut guest, KeyCode::Backspace);
    }
    for c in "Visita".chars() {
        press(&mut guest, KeyCode::Char(c));
    }
    press(&mut guest, KeyCode::Enter);
    assert!(both(&mut host, &mut guest, &|h, _| h.game.peers.iter().any(|p| p.name == "Visita")));

    // Quitting while others play asks first; stopping kicks the guest back to the lobby.
    press(&mut host, KeyCode::Char('q'));
    assert_eq!(host.confirm, Some(Confirm::QuitWhileHosting));
    assert!(render(&host, 120, 30).contains("Sair mesmo?"));
    press(&mut host, KeyCode::Char('n'));
    assert!(!host.quit);
    press(&mut host, KeyCode::Char('s'));
    press(&mut host, KeyCode::Char('p'));
    assert_eq!(host.confirm, Some(Confirm::StopServer(host.hosts[0].port)));
    press(&mut host, KeyCode::Char('s'));
    assert!(host.hosts.is_empty() && host.screen == Screen::Lobby);
    assert!(both(&mut host, &mut guest, &|_, g| g.screen == Screen::Lobby));
    assert!(guest.notice.as_ref().is_some_and(|(_, t)| t.contains("desligado")), "{:?}", guest.notice);
}

#[test]
fn disconnect_keeps_hosting() {
    let p = ports(13);
    let mut host = App::new("Solo".into(), machine_id::for_bot("solo"), None, None, p);
    host.host_new_world("Servidor de Solo");
    assert_eq!(host.screen, Screen::Playing);
    press(&mut host, KeyCode::Char('s'));
    press(&mut host, KeyCode::Char('d'));
    press(&mut host, KeyCode::Enter);
    assert_eq!(host.screen, Screen::Lobby);
    assert_eq!(host.hosts.len(), 1, "server keeps running for the others");
    assert!(matches!(host.lobby_rows()[0], LobbyRow::Own(_)));
    press(&mut host, KeyCode::Enter);
    assert_eq!(host.screen, Screen::Playing, "rejoins its own server");
    assert!(!press(&mut host, KeyCode::Char('q')), "alone on the server: quits without asking");
    host.shutdown();
}

#[test]
fn unreachable_server_shows_a_notice() {
    let mut a = offline_app();
    a.connect("127.0.0.1:9".parse().unwrap());
    assert_eq!(a.screen, Screen::Lobby);
    assert!(a.notice.as_ref().is_some_and(|(_, t)| t.contains("Não consegui conectar")));
}

#[test]
fn achievements_screen_lists_unlocked_ones() {
    let mut a = playing();
    a.game.player.achievements.push(Achievement::FirstMog);
    a.game.overlay = Overlay::Achievements;
    assert!(render(&a, 170, 42).contains("[x] Primeiro Mog"));
}

// ---------- Duels ----------

use pmog::duel::{Board, DuelKey, DuelView, Minigame, Phase, Progress};
use pmog::protocol::InviteView;

fn state_with(app: &mut App, edit: impl FnOnce(&mut StateView)) {
    let mut view = StateView {
        me: app.game.player.clone(),
        streak: 0,
        cooldown_ms: 0,
        cooldown_total_ms: 0,
        immune_ms: 0,
        preworkout_ms: 0,
        cologne_ms: 0,
        aura_farm_ms: 0,
        statuses: vec![],
        meter_ms: 0,
        players: vec![PeerView { id: 2, name: "Rival".into(), mp: 50.0, lifetime: 0.0, attack: 10.0, defense: 10.0, streak: 0, in_duel: false, immune: false, immune_ms: 0, clan_tag: None, statuses: Vec::new() }],
        duel: None,
        invites: Vec::new(),
        outgoing: None,
        queued: None,
        rtt_ms: 0.0,
        clan_tag: None,
    };
    edit(&mut view);
    app.game.apply(ServerMsg::State(Box::new(view)), Instant::now());
}

fn duel_view(game: Minigame, phase: Phase, board: Board) -> DuelView {
    DuelView {
        opponent: "Rival".into(),
        opponent_lifetime: 1e9,
        opponent_tag: None,
        stake: 40.0,
        my_wins: 1,
        their_wins: 0,
        round: 2,
        game,
        phase,
        phase_ms: 500,
        phase_len_ms: 5_000,
        result: Some("Rival piscou antes da hora!".into()),
        i_won_round: Some(true),
        i_won: None,
        board,
        seq: 0,
        lineup: Vec::new(),
    }
}

fn dueling(view: DuelView) -> App {
    let mut a = playing();
    state_with(&mut a, |s| s.duel = Some(view));
    a
}

#[test]
fn every_duel_screen_renders() {
    let p = Progress { steps: 1, failed: false, done: false };
    let cases = [
        (duel_view(Minigame::Staredown, Phase::Countdown, Board::Staredown { go: false, i_pressed: false, go_ms: 3_000 }), "PRÓXIMO: ENCARADA"),
        (duel_view(Minigame::Staredown, Phase::Play, Board::Staredown { go: false, i_pressed: false, go_ms: 3_000 }), "não pisque"),
        (duel_view(Minigame::Staredown, Phase::Play, Board::Staredown { go: true, i_pressed: false, go_ms: 100 }), "A G O R A"),
        (duel_view(Minigame::Staredown, Phase::Play, Board::Staredown { go: true, i_pressed: true, go_ms: 100 }), "Esperando o rival"),
        (
            duel_view(Minigame::Skincare, Phase::Show, Board::Skincare { level: 3, sequence: vec![DuelKey::Up, DuelKey::Left, DuelKey::Down], mine: p, theirs: p }),
            "Tônico → Hidratante → Sérum",
        ),
        (duel_view(Minigame::Skincare, Phase::Play, Board::Skincare { level: 3, sequence: vec![], mine: p, theirs: Progress { failed: true, ..p } }), "errou no passo 2"),
        (duel_view(Minigame::Mewing, Phase::Play, Board::Mewing { mine: 17, theirs: 9 }), "ALTERNE"),
        (duel_view(Minigame::Angle, Phase::Play, Board::Angle { target: 0.5, mine: vec![0.042], theirs: vec![] }), "4.2%"),
        (duel_view(Minigame::Angle, Phase::RoundOver, Board::Angle { target: 0.5, mine: vec![], theirs: vec![] }), "RODADA SUA!"),
    ];
    for (view, needle) in cases {
        let a = dueling(view);
        let screen = render(&a, 150, 42);
        assert!(screen.contains(needle), "missing {needle:?}\n{screen}");
        assert!(screen.contains("DUELO") && screen.contains("Rodada 2") && screen.contains("●○"), "{screen}");
        for (w, h) in [(1, 1), (30, 10), (80, 24), (300, 80)] {
            render(&a, w, h);
        }
    }
    let mut over = duel_view(Minigame::Mewing, Phase::Finished, Board::Mewing { mine: 3, theirs: 4 });
    over.i_won = Some(Some(false));
    let screen = render(&dueling(over), 150, 42);
    assert!(screen.contains("DERROTA") && screen.contains("-40 MP"), "{screen}");
}

#[test]
fn duel_keys_go_to_the_server() {
    let mut a = dueling(duel_view(Minigame::Mewing, Phase::Play, Board::Mewing { mine: 0, theirs: 0 }));
    for (code, key) in [
        (KeyCode::Left, DuelKey::Left),
        (KeyCode::Char('d'), DuelKey::Right),
        (KeyCode::Char('w'), DuelKey::Up),
        (KeyCode::Down, DuelKey::Down),
        (KeyCode::Char(' '), DuelKey::Action),
    ] {
        press(&mut a, code);
        assert!(matches!(a.game.outbox.last(), Some(ClientMsg::Duel { key: k, at_ms: Some(_) }) if *k == key));
    }
    let sent = a.game.outbox.len();
    press(&mut a, KeyCode::Char('l'));
    press(&mut a, KeyCode::Char('m'));
    assert_eq!(a.game.outbox.len(), sent, "shop and mewing are off during a duel");
    assert_eq!(a.game.overlay, Overlay::None);

    assert!(press(&mut a, KeyCode::Esc));
    assert_eq!(a.confirm, Some(Confirm::Forfeit));
    assert!(render(&a, 120, 30).contains("Desistir do duelo"));
    press(&mut a, KeyCode::Char('s'));
    assert_eq!(a.game.outbox.last(), Some(&ClientMsg::Forfeit));

    assert!(press(&mut a, KeyCode::Char('q')));
    assert_eq!(a.confirm, Some(Confirm::Forfeit), "q also asks before giving up");
    press(&mut a, KeyCode::Char('n'));
    assert!(handle_key(&mut a, KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL), Instant::now()));
    assert_eq!(a.confirm, Some(Confirm::QuitWhileDueling));
}

#[test]
fn challenging_through_the_bet_picker() {
    let mut a = playing();
    press(&mut a, KeyCode::Char('d'));
    assert_eq!(a.game.overlay, Overlay::None, "can't duel the NPC");
    assert!(a.game.feed.iter().any(|l| l.text.contains("jogador de verdade")));

    state_with(&mut a, |s| s.me.mp = 100.0);
    press(&mut a, KeyCode::Down);
    press(&mut a, KeyCode::Char('d'));
    assert_eq!(a.game.overlay, Overlay::Stake);
    press(&mut a, KeyCode::Right);
    let screen = render(&a, 120, 30);
    assert!(screen.contains("Desafiar Rival") && screen.contains("Cada um aposta 12 MP"), "{screen}");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.game.outbox.last(), Some(&ClientMsg::Challenge { target: 2, fraction: 0.25, draft: false }));
    assert_eq!(a.game.overlay, Overlay::None);

    state_with(&mut a, |s| {
        s.outgoing = Some(InviteView { peer: 2, name: "Rival".into(), fraction: 0.25, stake: 12.0, expires_ms: 9_500, draft: false })
    });
    assert!(render(&a, 150, 42).contains("Desafio para Rival · 12 MP · expira em 10 s"));
    press(&mut a, KeyCode::Char('x'));
    assert_eq!(a.game.outbox.last(), Some(&ClientMsg::CancelChallenge));

    press(&mut a, KeyCode::Char('f'));
    assert_eq!(a.game.overlay, Overlay::Stake);
    let screen = render(&a, 120, 30);
    assert!(screen.contains("Fila de duelo") && screen.contains("Aleatório") && screen.contains("sorteados"), "{screen}");
    press(&mut a, KeyCode::Left);
    press(&mut a, KeyCode::Left);
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.game.outbox.last(), Some(&ClientMsg::Queue { fraction: Some(0.0), draft: false }), "random is the default");

    press(&mut a, KeyCode::Char('d'));
    press(&mut a, KeyCode::Down);
    press(&mut a, KeyCode::Right);
    assert!(render(&a, 120, 30).contains("Cada um escolhe 3 minigames"));
    press(&mut a, KeyCode::Up);
    press(&mut a, KeyCode::Right);
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.game.outbox.last(), Some(&ClientMsg::Challenge { target: 2, fraction: 0.1, draft: true }), "second row picks the draft");
    press(&mut a, KeyCode::Char('d'));
    assert!(!a.game.stake_draft, "reopening goes back to random");
    press(&mut a, KeyCode::Esc);
    state_with(&mut a, |s| s.queued = Some(0.0));
    assert!(render(&a, 150, 42).contains("Na fila de duelo"));
    press(&mut a, KeyCode::Char('f'));
    assert_eq!(a.game.outbox.last(), Some(&ClientMsg::Queue { fraction: None, draft: false }));
}

#[test]
fn invitations_can_be_answered() {
    let mut a = playing();
    let invite = InviteView { peer: 2, name: "Rival".into(), fraction: 0.5, stake: 25.0, expires_ms: 14_000, draft: false };
    state_with(&mut a, |s| s.invites = vec![invite.clone(), InviteView { peer: 3, ..invite.clone() }]);
    let screen = render(&a, 150, 42);
    assert!(screen.contains("te desafiou para um duelo") && screen.contains("25 MP"), "{screen}");
    press(&mut a, KeyCode::Char('a'));
    assert_eq!(a.game.outbox.last(), Some(&ClientMsg::Answer { from: 2, accept: true }));
    press(&mut a, KeyCode::Char('r'));
    assert_eq!(a.game.outbox.last(), Some(&ClientMsg::Answer { from: 3, accept: false }));
    assert!(a.game.invites.is_empty());
}

#[test]
fn names_wear_their_rank() {
    let mut a = playing();
    a.game.player.lifetime = 6e5;
    state_with(&mut a, |s| {
        s.me.lifetime = 6e5;
        s.players[0].lifetime = 6e5;
        s.players[0].in_duel = true;
    });
    let screen = render(&a, 170, 42);
    assert!(screen.contains("♛ Igor"), "crown in the header\n{screen}");
    assert!(screen.contains("♛ Rival") && screen.contains("em duelo"), "{screen}");
    assert!(screen.contains("Duelos 0V 0D"));
}

#[test]
fn staredown_go_is_timed_on_the_client_clock() {
    let mut view = duel_view(Minigame::Staredown, Phase::Play, Board::Staredown { go: false, i_pressed: false, go_ms: 600 });
    view.phase_ms = 500;
    let mut a = playing();
    let now = Instant::now();
    state_with(&mut a, |s| {
        s.duel = Some(view);
        s.rtt_ms = 40.0;
    });
    // 500 ms elapsed plus 20 ms of travel: GO is 80 ms away.
    assert!(!a.game.staredown_go(now));
    let wake = a.game.next_wake(now).expect("wakes for the GO");
    assert!(wake <= Duration::from_millis(81), "{wake:?}");
    assert!(a.game.staredown_go(now + Duration::from_millis(85)));
    a.game.compensate = false;
    assert!(!a.game.staredown_go(now + Duration::from_secs(5)), "legacy mode trusts the server flag");
}

#[test]
fn mewing_starts_on_time_even_before_the_server_says_so() {
    let mut view = duel_view(Minigame::Mewing, Phase::Countdown, Board::Angle { target: 0.5, mine: vec![], theirs: vec![] });
    view.phase_ms = 2_900;
    view.phase_len_ms = 3_000;
    let mut a = playing();
    let now = Instant::now();
    state_with(&mut a, |s| s.duel = Some(view));
    a.game.predict_duel(now);
    assert_eq!(a.game.duel.as_ref().unwrap().phase, Phase::Countdown);
    let later = now + Duration::from_millis(150);
    a.game.duel_key(DuelKey::Left, later);
    let d = a.game.duel.as_ref().unwrap();
    assert_eq!(d.phase, Phase::Play);
    assert!(matches!(d.board, Board::Mewing { mine: 0, theirs: 0 }), "stale board replaced");
    let Some(ClientMsg::Duel { at_ms: Some(at), .. }) = a.game.outbox.last() else { panic!("key sent") };
    assert!((40.0..=60.0).contains(at), "pressed ~50 ms into play: {at}");
}

fn typing_board(typed: usize) -> Board {
    use pmog::duel::TypingProgress;
    let p = |typed| TypingProgress { typed, errors: 0, done: false, wpm: 0.0, accuracy: 1.0 };
    Board::Typing { text: "mog queixo aura".into(), mine: p(typed), theirs: p(4) }
}

#[test]
fn typing_battle_renders_and_every_letter_types() {
    let show = dueling(duel_view(Minigame::Typing, Phase::Show, typing_board(0)));
    let screen = render(&show, 150, 42);
    assert!(screen.contains("LEIA A FRASE") && screen.contains("mog queixo aura"), "{screen}");

    let mut a = dueling(duel_view(Minigame::Typing, Phase::Play, typing_board(0)));
    let screen = render(&a, 150, 42);
    assert!(screen.contains("DIGITE!") && screen.contains("Rival") && screen.contains("ppm"), "{screen}");
    // Letters that are shortcuts elsewhere (q quits, a/d/w/s move) all type here.
    for c in ['m', 'o', 'g', 'q', 'a', 'w'] {
        assert!(press(&mut a, KeyCode::Char(c)));
        assert!(matches!(a.game.outbox.last(), Some(ClientMsg::Duel { key: DuelKey::Char(k), .. }) if *k == c));
    }
    assert_eq!(a.confirm, None, "q types instead of asking to give up");
    let mine = a.game.typing_mine(Instant::now()).unwrap();
    assert_eq!((mine.typed, mine.errors), (3, 3), "shown at once, before the server confirms");
    for (w, h) in [(1, 1), (30, 10), (80, 24)] {
        render(&a, w, h);
    }
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.confirm, Some(Confirm::Forfeit));
}

#[test]
fn local_typing_never_leaks_into_the_next_typing_round() {
    let mut a = dueling(duel_view(Minigame::Typing, Phase::Play, typing_board(0)));
    for c in "mog q".chars() {
        press(&mut a, KeyCode::Char(c));
    }
    assert_eq!(a.game.typing_mine(Instant::now()).unwrap().typed, 5);
    // Next duel, same round number, another typing phrase.
    state_with(&mut a, |s| s.duel = Some(duel_view(Minigame::Mewing, Phase::Countdown, Board::Mewing { mine: 0, theirs: 0 })));
    state_with(&mut a, |s| s.duel = Some(duel_view(Minigame::Typing, Phase::Play, typing_board(0))));
    assert_eq!(a.game.typing_mine(Instant::now()).unwrap().typed, 0);
}

#[test]
fn new_minigames_render() {
    use pmog::duel::{HunterProgress, MacroProblem, MacrosProgress, RhythmProgress};
    let r = RhythmProgress { hits: 2, perfects: 1, misses: 1, score: 1.2, judged: vec![Some(10.0), Some(-90.0), None, None, None, None, None, None, None, None, None, None] };
    let h = HunterProgress { hits: 1, mistakes: 0, score: 1.0, avg_ms: Some(312.0), answered: vec![true, false] };
    let m = MacrosProgress { solved: 0, errors: 1, done: false };
    let problems = vec![MacroProblem { a: 40, b: 25, plus: true }];
    let cases = [
        (duel_view(Minigame::Rhythm, Phase::Countdown, Board::Mewing { mine: 0, theirs: 0 }), "TREINO NO RITMO"),
        (duel_view(Minigame::Rhythm, Phase::Play, Board::Rhythm { mine: r.clone(), theirs: r }), "perfeitas"),
        (duel_view(Minigame::Hunter, Phase::Play, Board::Hunter { schedule: vec![(0.0, true), (2_000.0, false)], mine: h.clone(), theirs: h }), "312 ms"),
        (duel_view(Minigame::Macros, Phase::Show, Board::Macros { problems: problems.clone(), mine: m, theirs: m }), "CALCULADORA"),
        (duel_view(Minigame::Macros, Phase::Play, Board::Macros { problems, mine: m, theirs: m }), "40g + 25g"),
    ];
    for (view, needle) in cases {
        let a = dueling(view);
        let screen = render(&a, 150, 42);
        assert!(screen.contains(needle), "missing {needle:?}\n{screen}");
        for (w, h) in [(1, 1), (30, 10), (80, 24), (300, 80)] {
            render(&a, w, h);
        }
    }
}

#[test]
fn macros_takes_digits_and_enter_sends_the_number() {
    use pmog::duel::{MacroProblem, MacrosProgress};
    let m = MacrosProgress { solved: 0, errors: 0, done: false };
    let problems = vec![MacroProblem { a: 40, b: 25, plus: false }, MacroProblem { a: 10, b: 10, plus: true }];
    let mut a = dueling(duel_view(Minigame::Macros, Phase::Play, Board::Macros { problems, mine: m, theirs: m }));
    for c in ['1', 'x', 'q', '5'] {
        press(&mut a, KeyCode::Char(c));
    }
    assert_eq!(a.game.macros_input, "15", "only digits go in");
    assert_eq!(a.confirm, None, "q does not give up here");
    press(&mut a, KeyCode::Enter);
    assert!(matches!(a.game.outbox.last(), Some(ClientMsg::Duel { key: DuelKey::Answer(15), .. })));
    assert!(a.game.macros_input.is_empty());
    assert_eq!(a.game.macros_mine().unwrap().solved, 1, "right answer shows at once");
    let screen = render(&a, 150, 42);
    assert!(screen.contains("Conta 2/2") && screen.contains("10g + 10g"), "{screen}");
    press(&mut a, KeyCode::Char('3'));
    press(&mut a, KeyCode::Backspace);
    press(&mut a, KeyCode::Char('9'));
    press(&mut a, KeyCode::Enter);
    let mine = a.game.macros_mine().unwrap();
    assert_eq!((mine.solved, mine.errors), (1, 1), "wrong answer is an error");
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.confirm, Some(Confirm::Forfeit));
}

#[test]
fn rhythm_and_hunter_use_space() {
    use pmog::duel::rhythm_fresh;
    let mut a = dueling(duel_view(Minigame::Rhythm, Phase::Play, Board::Rhythm { mine: rhythm_fresh(), theirs: rhythm_fresh() }));
    press(&mut a, KeyCode::Char(' '));
    assert!(matches!(a.game.outbox.last(), Some(ClientMsg::Duel { key: DuelKey::Action, .. })));
    let screen = render(&a, 150, 42);
    assert!(screen.contains("ESPAÇO"), "{screen}");
}

#[test]
fn immune_target_shows_recovery_time_and_is_not_attacked() {
    let mut a = playing();
    state_with(&mut a, |s| {
        s.players[0].immune = true;
        s.players[0].immune_ms = 42_000;
    });
    a.game.selected = a.game.peers.iter().position(|p| p.name == "Rival").unwrap();
    let screen = render(&a, 150, 42);
    assert!(screen.contains("imune 42 s"), "list shows the countdown\n{screen}");
    assert!(screen.contains("volta em 42 s"), "target shows the countdown\n{screen}");
    a.game.outbox.clear();
    press(&mut a, KeyCode::Char(' '));
    assert!(!a.game.outbox.iter().any(|m| matches!(m, ClientMsg::Mog { .. })), "no wasted attempt");
    assert!(a.game.is_ready(Instant::now()), "cooldown not spent");
    for (w, h) in [(30, 10), (80, 24)] {
        render(&a, w, h);
    }
}

#[test]
fn recovery_time_format() {
    use pmog::utils::format_wait;
    assert_eq!(format_wait(Duration::from_millis(41_200)), "42 s");
    assert_eq!(format_wait(Duration::from_secs(125)), "2:05");
}

fn shop_with_mp(mp: f64) -> App {
    let mut a = playing();
    state_with(&mut a, |s| s.me.mp = mp);
    a.game.overlay = Overlay::Shop;
    a.game.shop_cursor = pmog::game::CATALOG.iter().position(|d| d.id == ItemId::GuaSha).unwrap();
    a.game.outbox.clear();
    a
}

#[test]
fn buying_asks_then_shows_the_result() {
    let mut a = shop_with_mp(500.0);
    press(&mut a, KeyCode::Enter);
    let screen = render(&a, 150, 42);
    assert!(screen.contains("Comprar Gua Sha?") && screen.contains("Nível 0 → 1 de 50"), "{screen}");
    assert!(screen.contains("seu MP 500 → 470") && screen.contains("Defesa"), "{screen}");
    assert!(a.game.outbox.is_empty(), "nothing bought before confirming");

    press(&mut a, KeyCode::Enter);
    assert_eq!(a.game.outbox.last(), Some(&ClientMsg::Buy { item: ItemId::GuaSha }));
    assert_eq!(a.game.shop_pending, Some(ItemId::GuaSha));
    assert!(render(&a, 150, 42).contains("Comprando Gua Sha..."));

    a.game.apply(ServerMsg::Purchase { item: ItemId::GuaSha, ok: true, text: "Comprou Gua Sha (-30 MP)".into() }, Instant::now());
    assert_eq!(a.game.shop_pending, None);
    let screen = render(&a, 150, 42);
    assert!(screen.contains("✓ COMPRADO") && screen.contains("Comprou Gua Sha (-30 MP)"), "{screen}");
    press(&mut a, KeyCode::Down);
    assert!(!render(&a, 150, 42).contains("COMPRADO"), "moving on hides it");
}

#[test]
fn blocked_purchase_is_explained_and_never_sent() {
    let mut a = shop_with_mp(10.0);
    press(&mut a, KeyCode::Enter);
    let screen = render(&a, 150, 42);
    assert!(screen.contains("Não dá para comprar: Faltam 20 MP para Gua Sha."), "{screen}");
    press(&mut a, KeyCode::Enter);
    assert!(a.game.outbox.is_empty(), "no request when it can't be bought");
    assert_eq!(a.game.shop_confirm, None);

    a.game.apply(ServerMsg::Purchase { item: ItemId::GuaSha, ok: false, text: "Faltam 20 MP para Gua Sha.".into() }, Instant::now());
    let screen = render(&a, 150, 42);
    assert!(screen.contains("✗ NÃO COMPROU") && screen.contains("Faltam 20 MP"), "{screen}");
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.game.overlay, Overlay::Shop, "first Esc only dismisses the result");
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.game.overlay, Overlay::None);
    for (w, h) in [(30, 10), (80, 24)] {
        render(&a, w, h);
    }
}

#[test]
fn active_buffs_stand_out_and_their_end_is_announced() {
    let mut a = playing();
    assert!(!render(&a, 150, 42).contains("BUFFADO"));
    state_with(&mut a, |s| {
        s.me.angle_armed = true;
        s.cologne_ms = 3_000;
    });
    let screen = render(&a, 150, 42);
    assert!(screen.contains("BUFFADO"), "{screen}");
    assert!(screen.contains("ÂNGULO poder x2") && screen.contains("COLÔNIA +50% MP 3s"), "{screen}");
    let now = Instant::now();
    a.game.note_expired_buffs(now);
    a.game.note_expired_buffs(now + Duration::from_secs(4));
    assert!(a.game.feed.iter().any(|l| l.text.contains("O efeito de Colônia de Feromônio acabou")), "expiry is logged");
    for (w, h) in [(40, 12), (80, 24)] {
        render(&a, w, h);
    }
}

/// What side 0 sees of `game`, `ms` into play.
fn live_view(game: Minigame, ms: u64) -> DuelView {
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(5);
    let now = Instant::now();
    let mut d = pmog::duel::Duel::with_games([1, 2], ["Igor".into(), "Rival".into()], 10.0, &[game], now, &mut rng);
    let mut t = now;
    while d.phase != Phase::Play {
        t += Duration::from_millis(10);
        d.tick(t, &mut rng);
    }
    d.view(0, 0.0, t + Duration::from_millis(ms))
}

#[test]
fn every_new_minigame_screen_renders() {
    let cases = [
        (Minigame::Jawline, "ESCULPA O MAXILAR"),
        (Minigame::Posture, "faixa reta"),
        (Minigame::FitCheck, "FIT DO DIA"),
        (Minigame::Memory, "Nível 4"),
        (Minigame::Hydration, "FECHE COM ESPAÇO"),
        (Minigame::Photo, "FOTO 1/4"),
        (Minigame::Status, "MAIS CARO"),
        (Minigame::Hair, "SEGURA O CABELO"),
        (Minigame::Brow, "SOBRANCELHA"),
        (Minigame::ReverseMog, "JOGADA 1/5"),
    ];
    for (game, text) in cases {
        let a = dueling(live_view(game, 1_500));
        let screen = render(&a, 150, 42);
        assert!(screen.contains(text), "{game:?}:\n{screen}");
        assert!(screen.contains("Rodada 1 de 6"), "{game:?}");
        for (w, h) in [(30, 10), (80, 24), (200, 60)] {
            render(&a, w, h);
        }
    }
}

#[test]
fn draft_screen_moves_and_picks() {
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(1);
    let now = Instant::now();
    let d = pmog::duel::Duel::new([1, 2], ["Igor".into(), "Rival".into()], 0.0, true, now, &mut rng);
    let side = (0..2).find(|s| matches!(d.view(*s, 0.0, now).board, Board::Draft { my_turn: true, .. })).unwrap();
    let mut a = dueling(d.view(side, 0.0, now));
    let screen = render(&a, 150, 42);
    assert!(screen.contains("SUA VEZ DE ESCOLHER") && screen.contains("Escolha dos minigames"), "{screen}");
    assert!(screen.contains("Encarada: Encare sem piscar"), "rules of the highlighted one: {screen}");
    press(&mut a, KeyCode::Right);
    press(&mut a, KeyCode::Down);
    assert_eq!(a.game.draft_cursor, 1 + pmog::constants::DRAFT_COLUMNS);
    a.game.outbox.clear();
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.game.outbox.last(), Some(&ClientMsg::Duel { key: DuelKey::Pick(Some(Minigame::ALL[a.game.draft_cursor])), at_ms: None }));
    for _ in 0..20 {
        press(&mut a, KeyCode::Down);
    }
    assert_eq!(a.game.draft_cursor, Minigame::ALL.len(), "the last option is the random pick");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.game.outbox.last(), Some(&ClientMsg::Duel { key: DuelKey::Pick(None), at_ms: None }));
    assert!(render(&a, 150, 42).contains("Aleatório: o servidor sorteia"));
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.confirm, Some(Confirm::Forfeit), "Esc still offers to give up");
    for (w, h) in [(30, 10), (80, 24)] {
        render(&a, w, h);
    }
}

#[test]
fn digits_reach_the_duel() {
    let mut a = dueling(live_view(Minigame::Status, 100));
    a.game.outbox.clear();
    press(&mut a, KeyCode::Char('3'));
    assert!(matches!(a.game.outbox.last(), Some(ClientMsg::Duel { key: DuelKey::Char('3'), .. })));
}

#[test]
fn shop_tabs_group_items_and_move_between_them() {
    use pmog::items::Category;
    let mut a = playing();
    press(&mut a, KeyCode::Char('l'));
    let screen = render(&a, 150, 42);
    for c in Category::ALL {
        assert!(screen.contains(c.label()), "tab {} missing:\n{screen}", c.label());
    }
    assert!(screen.contains("Tutorial de Mewing") && !screen.contains("Energético"), "only the current tab is listed");
    let first_face: Vec<usize> = Category::Face.items().collect();
    for _ in 0..first_face.len() {
        press(&mut a, KeyCode::Down);
    }
    assert_eq!(a.game.shop_cursor, first_face[0], "↑↓ wrap inside the tab");
    press(&mut a, KeyCode::Left);
    assert_eq!(a.game.shop_category(), Category::Consumable, "← wraps to the last tab");
    let screen = render(&a, 150, 42);
    assert!(screen.contains("Combo 6-7") && screen.contains("Farmar Aura"), "{screen}");
    press(&mut a, KeyCode::Right);
    press(&mut a, KeyCode::Tab);
    assert_eq!(a.game.shop_category(), Category::Body);
    for _ in 0..3 {
        press(&mut a, KeyCode::Right);
    }
    assert_eq!(a.game.shop_category(), Category::Brainrot);
    let screen = render(&a, 150, 42);
    assert!(screen.contains("Gesto do 6-7") && screen.contains("Capoeira Proibida") && screen.contains("Tung Tung Tung Sahur"), "{screen}");
    press(&mut a, KeyCode::Enter);
    assert!(render(&a, 150, 42).contains("Confirmar compra"), "buying still asks first");
    for (w, h) in [(40, 12), (80, 24)] {
        render(&a, w, h);
    }
}

#[test]
fn new_buffs_show_up() {
    let mut a = playing();
    state_with(&mut a, |s| {
        s.me.combo_armed = true;
        s.aura_farm_ms = 30_000;
    });
    let screen = render(&a, 150, 42);
    assert!(screen.contains("COMBO 6-7") && screen.contains("FARMANDO AURA"), "{screen}");
}

#[test]
fn rank_list_shows_every_rank_and_the_way_up() {
    let mut a = playing();
    state_with(&mut a, |s| {
        s.me.lifetime = 300_000.0;
        s.players[0].lifetime = 2e6;
    });
    press(&mut a, KeyCode::Char('p'));
    assert_eq!(a.game.overlay, Overlay::Ranks);
    let screen = render(&a, 150, 60);
    assert!(screen.contains("Ranks 7/15") && screen.contains("até Ascendido"), "{screen}");
    assert!(screen.contains("◀ você"), "{screen}");
    press(&mut a, KeyCode::PageDown);
    press(&mut a, KeyCode::PageDown);
    press(&mut a, KeyCode::PageDown);
    let screen = render(&a, 150, 60);
    assert!(screen.contains("Transcendente") && screen.contains("na rede: Rival"), "{screen}");
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.game.overlay, Overlay::None);
    press(&mut a, KeyCode::Char('p'));
    assert_eq!(a.game.list_scroll, 0, "reopening starts at the top");
}

#[test]
fn achievements_are_grouped_with_progress_and_secrets() {
    let mut a = playing();
    state_with(&mut a, |s| s.me.mogs_won = 37);
    press(&mut a, KeyCode::Char('c'));
    let screen = render(&a, 150, 80);
    assert!(screen.contains("── Mog (") && screen.contains("37/100") && screen.contains("37/1.000"), "{screen}");
    assert!(screen.contains("??? · Conquista secreta"), "{screen}");
    assert!(!screen.contains("Sessenta e Sete"), "secrets stay hidden");
}
