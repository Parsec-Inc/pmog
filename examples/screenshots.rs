//! Renders real game screens to SVG for the README, or every screen at phone sizes.
//!
//! cargo run --example screenshots
//! cargo run --example screenshots -- celular   (writes target/phone-layouts/<size>/*.svg)

use std::fmt::Write as _;
use std::fs;
use std::time::{Duration, Instant};

use pmog::app::{App, Screen};
use pmog::discovery::Discovery;
use pmog::duel::{Board, DraftPick, Duel, DuelView, HunterProgress, Minigame, Phase, Stance, TypingProgress};
use pmog::game::{Achievement, Overlay, Player, Tone};
use pmog::items::ItemId;
use pmog::protocol::{InviteView, PeerView, ServerMsg, StateView};
use pmog::server::{Ports, ServerHandle};
use pmog::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

const OUT: &str = "docs/screenshots";
const CELL_W: f64 = 8.4;
const CELL_H: f64 = 18.0;
const PAD: f64 = 16.0;
const BAR: f64 = 30.0;
const BG: &str = "#14121c";
const FG: &str = "#e4e2ec";

fn hex(color: Color, fallback: &str) -> String {
    let rgb = match color {
        Color::Reset => return fallback.to_string(),
        Color::Black => (24, 22, 32),
        Color::Red => (222, 76, 86),
        Color::Green => (92, 200, 120),
        Color::Yellow => (238, 196, 72),
        Color::Blue => (84, 132, 230),
        Color::Magenta => (210, 100, 220),
        Color::Cyan => (80, 196, 210),
        Color::Gray => (176, 174, 188),
        Color::DarkGray => (104, 100, 120),
        Color::LightRed => (255, 120, 120),
        Color::LightGreen => (130, 236, 150),
        Color::LightYellow => (255, 228, 120),
        Color::LightBlue => (130, 170, 255),
        Color::LightMagenta => (240, 140, 245),
        Color::LightCyan => (140, 230, 240),
        Color::White => (245, 245, 250),
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(i) => (i, i, i),
    };
    format!("#{:02x}{:02x}{:02x}", rgb.0, rgb.1, rgb.2)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// A terminal window with the buffer drawn cell by cell; block glyphs become rectangles so sprites stay crisp.
fn svg(buf: &Buffer, title: &str) -> String {
    let (w, h) = (buf.area.width, buf.area.height);
    let width = w as f64 * CELL_W + PAD * 2.0;
    let height = h as f64 * CELL_H + PAD * 2.0 + BAR;
    let mut out = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<rect width="{width}" height="{height}" rx="10" fill="{BG}"/>
<rect width="{width}" height="{BAR}" rx="10" fill="#24212f"/><rect y="{}" width="{width}" height="10" fill="#24212f"/>
<circle cx="18" cy="15" r="6" fill="#ff5f57"/><circle cx="38" cy="15" r="6" fill="#febc2e"/><circle cx="58" cy="15" r="6" fill="#28c840"/>
<text x="{}" y="20" fill="#9a96aa" font-family="sans-serif" font-size="13" text-anchor="middle">{}</text>
<g font-family="'JetBrains Mono','DejaVu Sans Mono',Menlo,Consolas,monospace" font-size="14">
"##,
        BAR - 10.0,
        width / 2.0,
        escape(title)
    );
    let x0 = PAD;
    let y0 = PAD + BAR;
    for y in 0..h {
        let top = y0 + y as f64 * CELL_H;
        // Backgrounds and block glyphs first.
        for x in 0..w {
            let cell = &buf[(x, y)];
            let left = x0 + x as f64 * CELL_W;
            let bg = hex(cell.bg, BG);
            let fg = hex(cell.fg, FG);
            let rect = |out: &mut String, dy: f64, hh: f64, color: &str| {
                let _ = writeln!(out, r#"<rect x="{left:.1}" y="{:.1}" width="{:.1}" height="{hh:.1}" fill="{color}"/>"#, top + dy, CELL_W + 0.3);
            };
            if bg != BG {
                rect(&mut out, 0.0, CELL_H + 0.3, &bg);
            }
            match cell.symbol() {
                "█" => rect(&mut out, 0.0, CELL_H + 0.3, &fg),
                "▀" => rect(&mut out, 0.0, CELL_H / 2.0 + 0.3, &fg),
                "▄" => rect(&mut out, CELL_H / 2.0, CELL_H / 2.0 + 0.3, &fg),
                _ => {}
            }
        }
        // Then text, in runs of the same style.
        let mut x = 0;
        while x < w {
            let cell = &buf[(x, y)];
            let style = (cell.fg, cell.modifier.contains(Modifier::BOLD));
            let start = x;
            let mut text = String::new();
            while x < w {
                let c = &buf[(x, y)];
                if (c.fg, c.modifier.contains(Modifier::BOLD)) != style {
                    break;
                }
                let s = c.symbol();
                text.push_str(if matches!(s, "█" | "▀" | "▄") { " " } else { s });
                x += 1;
            }
            if text.trim().is_empty() {
                continue;
            }
            let weight = if style.1 { r#" font-weight="bold""# } else { "" };
            let _ = writeln!(
                out,
                r#"<text x="{:.1}" y="{:.1}" fill="{}"{weight} textLength="{:.1}" lengthAdjust="spacingAndGlyphs" xml:space="preserve">{}</text>"#,
                x0 + start as f64 * CELL_W,
                top + CELL_H * 0.75,
                hex(style.0, FG),
                (x - start) as f64 * CELL_W,
                escape(&text)
            );
        }
    }
    out.push_str("</g>\n</svg>\n");
    out
}

/// Swaps this computer's LAN address for a made-up one of the same length; the images go public.
fn hide_lan_ip(svg: String) -> String {
    let Some(ip) = pmog::utils::lan_ip().map(|ip| ip.to_string()) else { return svg };
    let tail = ip.len().saturating_sub(7).clamp(1, 3);
    let fake = format!("10.0.0.{}", &"123"[..tail]);
    svg.replace(&ip, &fake)
}

fn shot(app: &App, name: &str, title: &str, w: u16, h: u16) {
    println!("{}", shot_to(OUT, app, name, title, w, h));
}

fn shot_to(dir: &str, app: &App, name: &str, title: &str, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let path = format!("{dir}/{name}.svg");
    fs::write(&path, hide_lan_ip(svg(terminal.backend().buffer(), title))).unwrap();
    path
}

fn peer(id: i64, name: &str, lifetime: f64, mp: f64, tag: Option<&str>) -> PeerView {
    let rank = (lifetime / 1000.0).sqrt();
    PeerView {
        id,
        name: name.into(),
        mp,
        lifetime,
        attack: 20.0 + rank * 12.0,
        defense: 18.0 + rank * 10.0,
        streak: (id % 4) as u32,
        in_duel: false,
        immune: false,
        immune_ms: 0,
        clan_tag: tag.map(Into::into),
    }
}

fn me() -> Player {
    let mut p = Player { name: "Chadoso".into(), mp: 18_420.0, lifetime: 41_300.0, ..Default::default() };
    for (item, level) in [
        (ItemId::Mewing, 14),
        (ItemId::GuaSha, 12),
        (ItemId::MidFade, 9),
        (ItemId::GymPlan, 8),
        (ItemId::Skincare, 6),
        (ItemId::Insoles, 5),
        (ItemId::RingLight, 3),
        (ItemId::Sunglasses, 2),
        (ItemId::Creatine, 7),
        (ItemId::SigmaShield, 2),
        (ItemId::EnergyDrink, 3),
    ] {
        p.items.insert(item, level);
    }
    p.mogs_won = 312;
    p.mogs_lost = 74;
    p.best_streak = 11;
    p.duels_won = 9;
    p.duels_lost = 4;
    p.achievements = vec![Achievement::FirstMog, Achievement::TenMogs, Achievement::HundredMogs, Achievement::Streak5, Achievement::Streak10];
    p.shield_armed = true;
    p
}

fn state(duel: Option<DuelView>) -> StateView {
    let mut immune = peer(4, "MogLord", 128_000.0, 52_300.0, Some("SIGMA"));
    immune.immune = true;
    immune.immune_ms = 23_000;
    let mut busy = peer(5, "Brenda", 2_400.0, 900.0, None);
    busy.in_duel = true;
    StateView {
        me: me(),
        streak: 6,
        cooldown_ms: 0,
        cooldown_total_ms: 0,
        immune_ms: 0,
        preworkout_ms: 0,
        cologne_ms: 42_000,
        aura_farm_ms: 0,
        statuses: vec![],
        meter_ms: 610,
        players: vec![
            peer(2, "jawline_99", 34_000.0, 6_800.0, Some("SIGMA")),
            peer(3, "Mewzinho", 9_100.0, 2_150.0, Some("MEW")),
            immune,
            busy,
            peer(6, "Neto", 420.0, 160.0, None),
            peer(7, "Ascendido", 360_000.0, 120_000.0, Some("GOD")),
        ],
        duel,
        invites: Vec::new(),
        outgoing: None,
        queued: None,
        rtt_ms: 3.0,
        clan_tag: Some("SIGMA".into()),
    }
}

fn playing(duel: Option<DuelView>) -> App {
    let mut app = App::new("Chadoso".into(), "f".repeat(64), None, None, Ports::default());
    app.screen = Screen::Playing;
    let now = Instant::now();
    app.game.apply(ServerMsg::State(Box::new(state(duel))), now);
    for (tone, text) in [
        (Tone::Info, "Conectado em Servidor da Galera. Aperte ? para ver as regras."),
        (Tone::Good, "Você moggou Neto! +38 MP (BOM x1, streak 4)"),
        (Tone::Bad, "Seu mog em MogLord falhou: a defesa deu 210 contra seu poder de 168. -120 MP, streak zerado"),
        (Tone::Good, "Usou Colônia de Feromônio. Mogs rendem +50% de MP por 60 s."),
        (Tone::Epic, "Você moggou jawline_99! +412 MP (PERFECT x2, streak 6)"),
        (Tone::Info, "MogLord ainda está se recuperando: imune por mais 23 s."),
    ] {
        app.game.log(tone, text);
    }
    app.game.selected = 1;
    app.game.flash(now, Tone::Epic, "PERFECT · MP x2");
    app
}

fn duel_view(game: Minigame, phase_ms: u64, board: Board) -> DuelView {
    DuelView {
        opponent: "jawline_99".into(),
        opponent_lifetime: 34_000.0,
        opponent_tag: Some("SIGMA".into()),
        stake: 1_250.0,
        my_wins: 1,
        their_wins: 1,
        round: 3,
        game,
        phase: Phase::Play,
        phase_ms,
        phase_len_ms: 30_000,
        result: None,
        i_won_round: None,
        i_won: None,
        board,
        seq: 0,
        lineup: Vec::new(),
    }
}

/// Terminal sizes the phone page ends up with, portrait and landscape.
const PHONE_SIZES: [(u16, u16); 10] = [(45, 28), (50, 38), (50, 46), (54, 40), (58, 56), (90, 20), (100, 22), (110, 24), (122, 24), (134, 25)];

/// A duel of `game` stepped into `phase`, as player one sees it.
fn minigame_view(game: Minigame, phase: Phase, after_ms: u64) -> DuelView {
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(7);
    let mut now = Instant::now();
    let mut duel = Duel::with_games([1, 2], ["Chadoso".into(), "jawline_99".into()], 1_250.0, &[game], now, &mut rng);
    for _ in 0..20_000 {
        if duel.phase == phase {
            break;
        }
        now += Duration::from_millis(20);
        duel.tick(now, &mut rng);
    }
    let end = now + Duration::from_millis(after_ms);
    while now < end {
        now += Duration::from_millis(20);
        duel.tick(now, &mut rng);
    }
    duel.view(0, 34_000.0, now)
}

/// Every screen at every phone size, for checking the layouts by eye.
fn phones() {
    let mut scenes: Vec<(String, App)> = Vec::new();
    let mut add = |name: &str, app: App| scenes.push((name.to_string(), app));
    let mut lobby = App::new("Chadoso".into(), "f".repeat(64), None, None, Ports::default());
    lobby.release = 202611161821;
    add("lobby", lobby);
    add("jogo", playing(None));
    for (name, overlay) in [("loja", Overlay::Shop), ("ranks", Overlay::Ranks), ("conquistas", Overlay::Achievements), ("ajuda", Overlay::Help), ("servidor", Overlay::Server), ("chat", Overlay::Chat), ("cla", Overlay::Clan)] {
        let mut app = playing(None);
        app.game.overlay = overlay;
        add(name, app);
    }
    let mut stake = playing(None);
    stake.game.overlay = Overlay::Stake;
    stake.game.stake_target = Some(2);
    add("aposta", stake);
    let mut invited = playing(None);
    let mut view = state(None);
    view.invites = vec![InviteView { peer: 2, name: "jawline_99".into(), fraction: 0.25, stake: 1_250.0, expires_ms: 20_000, draft: true }];
    invited.game.apply(ServerMsg::State(Box::new(view)), Instant::now());
    add("convite", invited);
    let mut naming = playing(None);
    naming.editing_name = Some("Chadoso".into());
    add("nome", naming);
    let mut confirm = playing(None);
    confirm.confirm = Some(pmog::app::Confirm::Disconnect);
    add("confirmar", confirm);
    for game in Minigame::ALL {
        let slug = format!("{game:?}").to_lowercase();
        add(&format!("duelo-{slug}-explica"), playing(Some(minigame_view(game, Phase::Countdown, 500))));
        add(&format!("duelo-{slug}"), playing(Some(minigame_view(game, Phase::Play, 1_500))));
    }
    add("duelo-fim-rodada", playing(Some(minigame_view(Minigame::Staredown, Phase::RoundOver, 300))));
    let mut draft = minigame_view(Minigame::Staredown, Phase::Play, 0);
    let pick = |mine, game, random| DraftPick { mine, game, random };
    draft.board = Board::Draft { picks: vec![pick(false, Minigame::FitCheck, false), pick(true, Minigame::Staredown, false)], my_turn: true };
    draft.phase = Phase::Draft;
    add("duelo-draft", playing(Some(draft)));
    for (w, h) in PHONE_SIZES {
        let dir = format!("target/phone-layouts/{w}x{h}");
        fs::create_dir_all(&dir).unwrap();
        for (name, app) in &scenes {
            shot_to(&dir, app, name, &format!("{name} · {w}x{h}"), w, h);
        }
    }
}

fn main() {
    if std::env::args().any(|a| a == "celular") {
        return phones();
    }
    fs::create_dir_all(OUT).unwrap();
    let (w, h) = (140, 40);

    // Lobby with a server hosted on this computer.
    let ports = Ports { discovery: 53_600, tcp_start: 53_601 };
    let host = ServerHandle::start("Servidor da Galera", None, ports).unwrap();
    let others = [("Casa do Neto", 53_620), ("LAN da Faculdade", 53_640)]
        .map(|(name, tcp)| ServerHandle::start(name, None, Ports { discovery: ports.discovery, tcp_start: tcp }).unwrap());
    let discovery = Discovery::start(ports.discovery).ok();
    let mut lobby = App::new("Chadoso".into(), "f".repeat(64), discovery, None, ports);
    lobby.hosts.push(host);
    lobby.release = 202611161821;
    let deadline = Instant::now() + Duration::from_secs(5);
    while lobby.lobby_rows().len() < 3 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        lobby.step(Instant::now());
    }
    lobby.confirm = None;
    shot(&lobby, "lobby", "pmog · tela inicial", w, h);
    drop(others);

    shot(&playing(None), "jogo", "pmog · arena", w, h);

    let mut shop = playing(None);
    shop.game.overlay = Overlay::Shop;
    shop.game.shop_cursor = pmog::items::CATALOG.iter().position(|d| d.id == ItemId::DiamondJaw).unwrap();
    shot(&shop, "loja", "pmog · loja", w, h);

    let hunter = Board::Hunter {
        schedule: vec![(1_500.0, false), (3_200.0, true), (5_000.0, true)],
        mine: HunterProgress { hits: 4, mistakes: 1, score: 3.5, avg_ms: Some(287.0), answered: vec![false, false, false] },
        theirs: HunterProgress { hits: 3, mistakes: 0, score: 3.0, avg_ms: Some(341.0), answered: vec![true, false, false] },
    };
    shot(&playing(Some(duel_view(Minigame::Hunter, 3_500, hunter))), "duelo", "pmog · duelo", w, h);

    let typing = |typed, wpm, accuracy| TypingProgress { typed, errors: 2, done: false, wpm, accuracy };
    let text = "o mewing diário venceu a genética e o queixo dele cortou o vidro do espelho";
    let board = Board::Typing { text: text.into(), mine: typing(41, 78.0, 0.96), theirs: typing(33, 64.0, 0.91) };
    let typing_duel = playing(Some(duel_view(Minigame::Typing, 9_000, board)));
    shot(&typing_duel, "digitacao", "pmog · batalha de digitação", w, h);

    let pick = |mine, game, random| DraftPick { mine, game, random };
    let picks = vec![
        pick(false, Minigame::FitCheck, false),
        pick(true, Minigame::Staredown, false),
        pick(false, Minigame::ReverseMog, true),
        pick(true, Minigame::Typing, false),
    ];
    let mut draft = duel_view(Minigame::Staredown, 4_000, Board::Draft { picks, my_turn: true });
    (draft.phase, draft.phase_len_ms, draft.round, draft.my_wins, draft.their_wins) = (Phase::Draft, 12_000, 0, 0, 0);
    let mut drafting = playing(Some(draft));
    drafting.game.draft_cursor = 7;
    shot(&drafting, "draft", "pmog · escolha dos minigames", w, h);

    let throws = |list: [Option<Stance>; 5]| list.to_vec();
    let rps = Board::ReverseMog {
        mine: throws([Some(Stance::Mog), Some(Stance::Defend), Some(Stance::Ignore), Some(Stance::Mog), None]),
        theirs: throws([Some(Stance::Ignore), Some(Stance::Mog), Some(Stance::Ignore), None, None]),
        their_ready: true,
    };
    let mut reverse = duel_view(Minigame::ReverseMog, 15_000, rps);
    (reverse.round, reverse.my_wins, reverse.their_wins) = (5, 3, 1);
    shot(&playing(Some(reverse)), "mog-reverso", "pmog · mog reverso", w, h);

    let mut stake = playing(None);
    stake.game.overlay = Overlay::Stake;
    stake.game.stake_target = Some(2);
    stake.game.stake_cursor = 2;
    stake.game.stake_row = 1;
    shot(&stake, "aposta", "pmog · desafio", w, h);

    let mut ranks = playing(None);
    ranks.game.player.lifetime = 1_240_000.0;
    ranks.game.overlay = Overlay::Ranks;
    ranks.game.list_scroll = u16::MAX;
    shot(&ranks, "ranks", "pmog · ranks", w, h);

    let mut achievements = playing(None);
    let p = &mut achievements.game.player;
    (p.mews, p.perfects, p.defends_won, p.defends_lost, p.purchases, p.items_used) = (4_210, 64, 38, 21, 47, 12);
    p.achievements = Achievement::ALL.into_iter().filter(|a| a.reached(p) || a.group() == pmog::achievements::Group::Mog && !a.secret()).collect();
    achievements.game.overlay = Overlay::Achievements;
    shot(&achievements, "conquistas", "pmog · conquistas", w, h);

    let mut exclusive = playing(None);
    exclusive.game.player.lifetime = 7_100_000.0;
    exclusive.game.player.mp = 9_400_000.0;
    exclusive.game.overlay = Overlay::Shop;
    exclusive.game.shop_cursor = pmog::items::CATALOG.iter().position(|d| d.id == ItemId::Trophy67).unwrap();
    shot(&exclusive, "loja-67", "pmog · loja: Brainrot e 67", w, h);
}
