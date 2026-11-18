//! Terminal rendering.

pub mod chat;
pub mod clan;
mod burst;
mod consume;
mod duel;
mod duelfx;
mod fx;
mod minigames;
pub mod flair;

use std::time::{Duration, Instant};

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Cell, Clear, Gauge, LineGauge, List, ListItem, ListState, Paragraph, Row,
    Table, TableState, Tabs, Wrap,
};

use crate::achievements::Group;
use crate::constants::{LOBBY_TALL, NARROW_FULL_HEADER_FROM, SHOP_DETAILS_ROWS, SHOP_FULL_TABLE_WIDTH, NARROW_DUEL_FEED_FROM, NARROW_FEED_ROWS, NARROW_ROOMY_BODY, NARROW_WIDTH, SHORT_HEIGHT, PULSE_MS, QR_PANEL_MIN_WIDTH, QR_QUIET_ZONE, RANK_NAME_COLUMN, SHOP_PREVIEW_MIN_WIDTH, SHOP_PREVIEW_WIDTH, SHOP_RESULT_SHOW};
use crate::duel::Minigame;
use crate::game::{Achievement, Buff, CATALOG, Category, Game, ItemDef, Overlay, Rank, Stats, TimingTier, Tone};
use crate::app::{App, Confirm, LobbyRow, Screen};
use crate::help;
use crate::sprites;
use crate::utils::{decimal, format_count, format_mp, format_wait};

const BIG_BUTTON: [&str; 6] = [
    "███╗   ███╗ ██████╗  ██████╗  █████╗ ██████╗ ",
    "████╗ ████║██╔═══██╗██╔════╝ ██╔══██╗██╔══██╗",
    "██╔████╔██║██║   ██║██║  ███╗███████║██████╔╝",
    "██║╚██╔╝██║██║   ██║██║   ██║██╔══██║██╔══██╗",
    "██║ ╚═╝ ██║╚██████╔╝╚██████╔╝██║  ██║██║  ██║",
    "╚═╝     ╚═╝ ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝",
];

fn rank_color(rank: Rank) -> Color {
    match rank {
        Rank::Sub5 => Color::Gray,
        Rank::Ltn => Color::White,
        Rank::Mtn => Color::Green,
        Rank::Htn => Color::Cyan,
        Rank::Chadlite => Color::Blue,
        Rank::Chad => Color::Magenta,
        Rank::Gigachad => Color::Yellow,
        Rank::Ascended => Color::Red,
        Rank::Sigma => Color::Rgb(200, 215, 235),
        Rank::AuraLord => Color::Rgb(190, 90, 255),
        Rank::SixSeven => Color::Rgb(120, 255, 120),
        Rank::Mythic => Color::Rgb(0, 210, 190),
        Rank::GreekGod => Color::Rgb(255, 215, 90),
        Rank::Cosmic => Color::Rgb(110, 110, 255),
        Rank::Transcendent => Color::White,
    }
}

fn tone_color(tone: Tone) -> Color {
    match tone {
        Tone::Info => Color::Gray,
        Tone::Good => Color::Green,
        Tone::Bad => Color::Red,
        Tone::Epic => Color::Yellow,
    }
}

fn fg(color: Color) -> Style {
    Style::new().fg(color)
}

pub fn draw(f: &mut Frame, app: &App) {
    match app.screen {
        Screen::Lobby => draw_lobby(f, app),
        Screen::Playing => draw_game(f, app),
    }
    if app.screen == Screen::Playing {
        let now = Instant::now();
        let age = |at: Instant| now.saturating_duration_since(at).as_millis() as u64;
        if let Some(h) = &app.game.last_taken {
            let area = f.area();
            burst::vignette(f.buffer_mut(), area, h, age(h.at));
            burst::shake(f.buffer_mut(), h.intensity() * 1.3, age(h.at));
        }
        if let Some(h) = &app.game.last_mog {
            burst::shake(f.buffer_mut(), h.intensity(), age(h.at));
        }
        if let Some(d) = &app.game.duel {
            let area = f.area();
            duelfx::screen(f.buffer_mut(), area, d, (app.game.duel_elapsed(now) * 1000.0) as u64);
        }
        if let Some((at, fx)) = app.game.last_fx {
            let area = f.area();
            fx::render(f.buffer_mut(), area, fx, age(at));
            burst::shake(f.buffer_mut(), fx::shake_power(fx), age(at));
        }
        if let Some((at, item)) = app.game.last_used {
            let area = f.area();
            consume::render(f.buffer_mut(), area, item, age(at));
            burst::shake(f.buffer_mut(), consume::shake_power(item), age(at));
        }
    }
    if let Some(typed) = &app.editing_name {
        draw_text_editor(f, " Seu nome · Enter salva · Esc cancela ", typed);
    }
    if let Some(edit) = &app.editing_world {
        let title = if edit.id.is_some() { " Novo nome do mundo · Enter salva · Esc cancela " } else { " Nome do mundo novo · Enter cria · Esc cancela " };
        draw_text_editor(f, title, &edit.typed);
    }
    if let Some(typed) = &app.editing_phone_password {
        draw_text_editor(f, " Senha do celular · Enter salva · Esc cancela ", &"•".repeat(typed.chars().count()));
    }
    if let Some(question) = app.confirm {
        draw_confirm(f, app, question);
    }
}

/// The PC's own screen: the game plus a notice while a phone has taken over.
pub fn draw_local(f: &mut Frame, app: &App) {
    draw(f, app);
    if app.phone_controls() {
        let area = small_popup(f.area(), 46, 5);
        f.render_widget(Clear, area);
        let block = Block::bordered().border_type(BorderType::Double).border_style(fg(Color::LightGreen)).title(" Celular no controle ");
        let lines = vec![Line::raw(""), Line::from(vec![Span::raw("O jogo está no celular. "), Span::styled("R", Style::new().bold()), Span::raw(" retoma aqui.")])];
        f.render_widget(Paragraph::new(lines).centered().block(block), area);
    }
}

fn draw_game(f: &mut Frame, app: &App) {
    let g = &app.game;
    let now = Instant::now();
    let full = f.area();
    let narrow = full.width < NARROW_WIDTH;
    // Upright phones: a duel's score box already names both sides, and short screens keep only the essentials.
    let header_lines = header_lines(app, now, narrow, narrow && full.height < NARROW_FULL_HEADER_FROM);
    let header_h = if (narrow || full.height < SHORT_HEIGHT) && g.duel.is_some() { 0 } else { header_height(&header_lines, full.width) };
    let hints = match &g.duel {
        Some(d) => duel::duel_hints(d),
        None => game_hints(g),
    };
    let footer = hint_lines(&hints, full.width);
    let footer_h = (footer.len() as u16).min(if narrow { 3 } else if full.height >= SHORT_HEIGHT - 4 { 2 } else { 1 });
    let [header, body, footer_area] =
        Layout::vertical([Constraint::Length(header_h), Constraint::Min(8), Constraint::Length(footer_h)]).areas(full);
    draw_header(f, header, app, header_lines, now);
    f.render_widget(Paragraph::new(footer), footer_area);

    if g.duel.is_some() {
        let feed_h = if body.height >= NARROW_DUEL_FEED_FROM { NARROW_FEED_ROWS } else { 0 };
        let split = if narrow {
            Layout::vertical([Constraint::Min(0), Constraint::Length(feed_h)])
        } else {
            Layout::horizontal([Constraint::Percentage(72), Constraint::Percentage(28)])
        };
        let [arena, feed] = split.areas(body);
        duel::draw_duel(f, arena, g, now);
        if feed.height > 0 {
            draw_feed(f, feed, g);
        }
        return;
    }
    if narrow {
        let compact = body.height < NARROW_ROOMY_BODY;
        let target_extra = target_paragraph(g, now).line_count(body.width.saturating_sub(2)).saturating_sub(3) as u16;
        let center_h = if compact { 12 } else { 15 } + target_extra;
        let peers_h = ((g.peers.len() as u16) * 2 + 2).min(body.height.saturating_sub(center_h + NARROW_FEED_ROWS).max(4));
        let [center, peers, feed] =
            Layout::vertical([Constraint::Length(center_h), Constraint::Length(peers_h), Constraint::Min(0)]).areas(body);
        draw_center(f, center, g, now, true);
        draw_peers(f, peers, g, now);
        if feed.height >= 3 {
            draw_feed(f, feed, g);
        }
    } else {
        let [left, center, right] =
            Layout::horizontal([Constraint::Percentage(26), Constraint::Percentage(48), Constraint::Percentage(26)]).areas(body);
        draw_peers(f, left, g, now);
        draw_center(f, center, g, now, full.height < SHORT_HEIGHT);
        draw_feed(f, right, g);
    }

    match g.overlay {
        Overlay::Shop => {
            draw_shop(f, g);
            draw_shop_card(f, g, now);
        }
        Overlay::Achievements => draw_achievements(f, g),
        Overlay::Ranks => draw_ranks(f, g, now),
        Overlay::Help => draw_help(f, g),
        Overlay::Server => draw_server_panel(f, app),
        Overlay::Stake => duel::draw_stake_picker(f, g),
        Overlay::Chat => chat::draw(f, app, now),
        Overlay::Clan => clan::draw(f, app, now),
        Overlay::None => duel::draw_invite(f, g, now),
    }
}

/// Text lines of the header (the rank bar comes after them); `narrow` stacks them for phones.
fn header_lines(app: &App, now: Instant, narrow: bool, compact: bool) -> Vec<Line<'static>> {
    let g = &app.game;
    let p = &g.player;
    let stats = p.stats();
    let rank = p.rank();
    let gap = if narrow { "  " } else { "   " };
    let pair = |label: &str, value: String| format!("{gap}{label}\u{a0}{value}");
    let mut name = flair::tagged_spans(g.clan_tag.as_deref(), &p.name, rank, now);
    name.push(Span::raw(" "));
    name.push(Span::styled(format!("[{}]", rank.label()), fg(rank_color(rank)).bold()));
    let mut numbers = vec![
        Span::styled(pair("MP", String::new()), Style::new()),
        Span::styled(format_mp(p.mp), fg(Color::Yellow).bold()),
        Span::raw(pair("Total", format_mp(p.lifetime))),
        Span::raw(pair("Streak", String::new())),
        Span::styled(format!("{}\u{a0}(x{:.2})", g.streak, g.streak_mult(&stats)), fg(if g.streak > 0 { Color::LightRed } else { Color::Gray }).bold()),
        Span::styled(pair("Duelos", format!("{}V\u{a0}{}D", p.duels_won, p.duels_lost)), fg(Color::Gray)),
    ];
    let combat = Span::styled(
        format!(
            "{}{}{}{}",
            pair("Poder", format_mp(stats.attack)),
            pair("Defesa", format_mp(stats.defense)),
            pair("Passivo", format!("{}/s", format_mp(stats.passive))),
            pair("CD", format!("{:.1}s", stats.cooldown)),
        ),
        fg(Color::Gray),
    );
    let mut badges = Vec::new();
    for (buff, left) in g.buffs(now) {
        badges.push(Span::styled(format!(" {} {}{} ", buff.label(), buff.effect(), buff_left(left)).replace(' ', "\u{a0}"), buff_style(buff)));
        badges.push(" ".into());
    }
    if now < g.immune_until {
        badges.push(Span::styled("\u{a0}IMUNE\u{a0}", Style::new().bg(Color::Red).fg(Color::Black).bold()));
    }
    if narrow {
        numbers[0].content = "MP\u{a0}".into();
        if compact {
            numbers.truncate(4);
            numbers.pop();
        }
        let mut lines = vec![Line::from(name), Line::from(numbers)];
        if !compact {
            lines.push(Line::from(combat));
        }
        if !badges.is_empty() {
            lines.push(Line::from(badges));
        }
        return lines;
    }
    let mut first = vec![Span::raw(" ")];
    first.extend(name);
    first.extend(numbers);
    let mut second = vec![combat, Span::raw("  ")];
    second.extend(badges);
    vec![Line::from(first), Line::from(second)]
}

/// Rows the header needs: its wrapped text, the rank bar and the border.
fn header_height(lines: &[Line<'static>], width: u16) -> u16 {
    Paragraph::new(lines.to_vec()).wrap(Wrap { trim: true }).line_count(width.saturating_sub(2)) as u16 + 3
}

fn draw_header(f: &mut Frame, area: Rect, app: &App, lines: Vec<Line<'static>>, now: Instant) {
    let g = &app.game;
    let p = &g.player;
    let rank = p.rank();
    let border = if now < g.hit_flash_until { Color::Red } else { Color::Magenta };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(fg(border))
        .title(Span::styled(" PMOG ", fg(Color::Magenta).bold()))
        .title(Line::styled(server_label(app), fg(Color::Gray)).right_aligned());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [text, bar] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), text);

    let (ratio, label) = match rank.next() {
        Some(next) => {
            let span = next.threshold() - rank.threshold();
            let ratio = ((p.lifetime - rank.threshold()) / span).clamp(0.0, 1.0);
            (ratio, format!(" Próximo: {} ({}/{}) ", next.label(), format_mp(p.lifetime), format_mp(next.threshold())))
        }
        None => (1.0, " Rank máximo ".to_string()),
    };
    let gauge = LineGauge::default()
        .filled_style(fg(rank.next().map_or(rank_color(rank), rank_color)))
        .unfilled_style(fg(Color::DarkGray))
        .ratio(ratio)
        .label(label);
    f.render_widget(gauge, bar);
}

fn draw_peers(f: &mut Frame, area: Rect, g: &Game, now: Instant) {
    let items: Vec<ListItem> = g
        .peers
        .iter()
        .map(|peer| {
            if peer.is_npc() {
                return ListItem::new(vec![
                    Line::styled(peer.name.clone(), fg(Color::DarkGray).bold()),
                    Line::styled("   bot local, só pra farmar", fg(Color::DarkGray)),
                ]);
            }
            let rank = Rank::from_lifetime(peer.lifetime);
            let mut title = flair::tagged_spans(peer.clan_tag.as_deref(), &peer.name, rank, now);
            title.push("  ".into());
            title.push(Span::styled(rank.label(), fg(rank_color(rank))));
            if peer.in_duel {
                title.push(Span::styled("  ⚔ em duelo", fg(Color::Yellow)));
            } else if let Some(left) = peer.immune_left(now) {
                title.push(Span::styled(format!("  imune {}", format_wait(left)), fg(Color::Red)));
            }
            ListItem::new(vec![
                Line::from(title),
                Line::styled(
                    format!(
                        "   MP {}  Pod {}  Def {}  St {}",
                        format_mp(peer.mp),
                        format_mp(peer.attack),
                        format_mp(peer.defense),
                        peer.streak
                    ),
                    fg(Color::DarkGray),
                ),
            ])
        })
        .collect();
    let list = List::new(items)
        .block(Block::bordered().title(format!(" No servidor ({}) ", g.peers.len() - 1)))
        .highlight_symbol("▶ ")
        .highlight_style(Style::new().bg(Color::Rgb(45, 20, 55)));
    let mut state = ListState::default().with_selected(Some(g.selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn odds_color(chance: f64) -> Color {
    if chance < 0.25 {
        Color::Red
    } else if chance < 0.6 {
        Color::Yellow
    } else {
        Color::Green
    }
}

/// "· chance BOM 40% · PERFECT 85%"
fn odds_spans((good, perfect): (f64, f64)) -> Vec<Span<'static>> {
    let pct = |c: f64| format!("{:.0}%", c * 100.0);
    vec![
        Span::styled("  ·\u{a0}chance\u{a0}BOM\u{a0}", fg(Color::Gray)),
        Span::styled(pct(good), fg(odds_color(good)).bold()),
        Span::styled(" ·\u{a0}PERFECT\u{a0}", fg(Color::Gray)),
        Span::styled(pct(perfect), fg(odds_color(perfect)).bold()),
    ]
}

/// Bordered target box: who is selected and the odds against them.
fn target_paragraph(g: &Game, now: Instant) -> Paragraph<'static> {
    let line = match g.peers.get(g.selected) {
        Some(peer) => {
            let mut spans = vec![Span::raw("Alvo: ")];
            if peer.is_npc() {
                spans.push(Span::styled(peer.name.clone(), fg(Color::DarkGray).bold()));
            } else {
                spans.extend(flair::tagged_spans(peer.clan_tag.as_deref(), &peer.name, Rank::from_lifetime(peer.lifetime), now));
            }
            match peer.immune_left(now) {
                Some(left) => spans.push(Span::styled(format!("  · se recuperando, volta em {}", format_wait(left)), fg(Color::Red).bold())),
                None => spans.extend(odds_spans(g.mog_odds(peer))),
            }
            Line::from(spans)
        }
        None => Line::raw("Sem alvo"),
    };
    let mut block = Block::bordered();
    if let Some(status) = duel::pending_status(g) {
        block = block.title_bottom(status.centered());
    }
    Paragraph::new(line).centered().wrap(Wrap { trim: true }).block(block)
}

/// Target, MOGAR button, meter, cooldown and banner; `phone` drops the banner box and shows buffs in the header only.
fn draw_center(f: &mut Frame, area: Rect, g: &Game, now: Instant, phone: bool) {
    let target_text = target_paragraph(g, now);
    let target_h = target_text.line_count(area.width.saturating_sub(2)) as u16;
    let banner_h = if phone { 0 } else { 3 };
    let [target, button, meter, cooldown, banner] = Layout::vertical([
        Constraint::Length(target_h),
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(banner_h),
    ])
    .areas(area);
    let stats = g.player.stats();
    let ready = g.is_ready(now);

    f.render_widget(target_text, target);

    let pulse = ((now - g.started).as_millis() / PULSE_MS).is_multiple_of(2);
    let buffs = g.buffs(now);
    let burst = g.last_mog.filter(|h| (now - h.at).as_millis() < burst::duration_ms(h) as u128);
    let color = match (ready, pulse, buffs.is_empty()) {
        (false, _, _) => Color::DarkGray,
        (true, true, true) => Color::Magenta,
        (true, false, true) => Color::LightMagenta,
        (true, true, false) => BUFF_GOLD,
        (true, false, false) => Color::LightYellow,
    };
    let mut block = Block::bordered()
        .border_type(if ready { BorderType::Thick } else { BorderType::Plain })
        .border_style(fg(match burst {
            Some(h) => tier_color(h.tier),
            None if buffs.is_empty() => color,
            None => BUFF_GOLD,
        }))
        .title(Line::from(" [ESPAÇO] ").centered());
    if !buffs.is_empty() && !phone {
        let room = button.width.saturating_sub(4) as usize;
        let mut spans = vec![Span::styled(" ⚡ BUFFADO ", Style::new().bg(BUFF_GOLD).fg(Color::Black).bold()), " ".into()];
        for (buff, left) in &buffs {
            spans.push(Span::styled(format!(" {} {}{} ", buff.label(), buff.effect(), buff_left(*left)), buff_style(*buff)));
            spans.push(" ".into());
        }
        // Too many to describe on one border: keep only the names.
        let width = |spans: &[Span]| spans.iter().map(Span::width).sum::<usize>();
        if width(&spans) > room {
            spans.truncate(2);
            for (buff, _) in &buffs {
                spans.push(Span::styled(format!(" {} ", buff.label()), buff_style(*buff)));
                spans.push(" ".into());
            }
        }
        if width(&spans) > room {
            spans = vec![Span::styled(format!(" ⚡ BUFFADO x{} ", buffs.len()), Style::new().bg(BUFF_GOLD).fg(Color::Black).bold())];
        }
        block = block.title_bottom(Line::from(spans).centered());
    }
    if phone && let Some(b) = g.banner.as_ref().filter(|b| now < b.until) {
        block = block.title_bottom(Line::styled(format!(" {} ", b.text), fg(tone_color(b.tone)).bold()).centered());
    }
    let inner = block.inner(button);
    let art: Vec<&str> = if inner.width as usize > BIG_BUTTON[0].chars().count() && inner.height as usize >= BIG_BUTTON.len() {
        BIG_BUTTON.to_vec()
    } else {
        vec!["M O G A R"]
    };
    let pad = (inner.height as usize).saturating_sub(art.len()) / 2;
    let mut lines = vec![Line::raw(""); pad];
    lines.extend(art.into_iter().map(|l| Line::styled(l, fg(color).bold())));
    f.render_widget(Paragraph::new(lines).centered().block(block), button);
    if let Some(h) = burst {
        burst::render(f.buffer_mut(), inner, &h, (now - h.at).as_millis() as u64);
    }
    if let Some(h) = g.last_taken.filter(|h| (now - h.at).as_millis() < burst::hit_duration_ms(h) as u128) {
        burst::impact(f.buffer_mut(), inner, &h, (now - h.at).as_millis() as u64);
    }

    let legend: Vec<Span> = TimingTier::ALL
        .iter()
        .flat_map(|t| {
            [
                Span::styled("■ ", fg(tier_color(*t))),
                Span::raw(format!("{} x{}  ", t.label(), decimal(t.reward()))),
            ]
        })
        .collect();
    let legend_line = Line::from([vec![Span::raw(" MP: ")], legend].concat());
    let meter_block = if legend_line.width() + 2 > meter.width as usize {
        let short: Vec<Span> = TimingTier::ALL.iter().flat_map(|t| [Span::styled("■", fg(tier_color(*t))), Span::raw(format!("x{} ", decimal(t.reward())))]).collect();
        Block::bordered().title(Line::from([vec![Span::raw(" ")], short].concat()))
    } else {
        Block::bordered().title(legend_line)
    };
    let width = meter_block.inner(meter).width as usize;
    f.render_widget(
        Paragraph::new(meter_line(width, g.meter(now), stats.perfect_threshold)).block(meter_block),
        meter,
    );

    let remaining = g.cooldown_until.saturating_duration_since(now).as_secs_f64();
    let ratio = if g.cooldown_total > 0.0 {
        1.0 - (remaining / g.cooldown_total).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let label = if ready { "PRONTO".to_string() } else { format!("{remaining:.1}s") };
    let gauge = Gauge::default()
        .block(Block::bordered().title(" Cooldown "))
        .gauge_style(fg(if ready { Color::Green } else { Color::Blue }))
        .ratio(ratio)
        .label(label);
    f.render_widget(gauge, cooldown);

    if banner.height > 0 {
        let text = match &g.banner {
            Some(b) if now < b.until => Line::styled(b.text.clone(), fg(tone_color(b.tone)).bold()),
            _ => Line::raw(""),
        };
        f.render_widget(Paragraph::new(text).centered().block(Block::bordered()), banner);
    }
}

const BUFF_GOLD: Color = Color::Rgb(255, 200, 40);

/// Badge style for a buff.
fn buff_style(buff: Buff) -> Style {
    let color = match buff {
        Buff::Shield => Color::Cyan,
        Buff::Angle => Color::Yellow,
        Buff::Mirror => Color::LightBlue,
        Buff::Tape => Color::LightCyan,
        Buff::PreWorkout => Color::LightMagenta,
        Buff::Cologne => Color::LightGreen,
        Buff::Combo => Color::LightRed,
        Buff::AuraFarm => Color::Rgb(180, 140, 255),
    };
    Style::new().bg(color).fg(Color::Black).bold()
}

/// Remaining time of a timed buff, or nothing for one that waits to be used.
fn buff_left(left: Option<Duration>) -> String {
    left.map_or(String::new(), |l| format!(" {}s", l.as_secs_f64().ceil() as u64))
}

/// Fixed RGB so terminal themes can't remap the tier colors.
fn tier_color(tier: TimingTier) -> Color {
    match tier {
        TimingTier::Bad => Color::Rgb(220, 60, 60),
        TimingTier::Ok => Color::Rgb(235, 200, 60),
        TimingTier::Good => Color::Rgb(70, 150, 240),
        TimingTier::Perfect => Color::Rgb(60, 210, 90),
    }
}

fn meter_line(width: usize, value: f64, threshold: f64) -> Line<'static> {
    let last = width.saturating_sub(1).max(1) as f64;
    let cursor = (value * last).round() as usize;
    let spans: Vec<Span> = (0..width)
        .map(|i| {
            let frac = i as f64 / last;
            if i == cursor {
                return Span::styled("█", fg(Color::White).bold());
            }
            Span::styled("▬", fg(tier_color(TimingTier::from_meter(frac, threshold))))
        })
        .collect();
    Line::from(spans)
}

fn draw_feed(f: &mut Frame, area: Rect, g: &Game) {
    let lines: Vec<Line> = g
        .feed
        .iter()
        .take(area.height as usize)
        .map(|l| Line::styled(l.text.clone(), fg(tone_color(l.tone))))
        .collect();
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title(" Feed "));
    f.render_widget(paragraph, area);
}

/// One footer hint: the key and what it does; an empty key is plain text.
pub(crate) type KeyHint = (String, String);

pub(crate) fn hint(key: &str, label: &str) -> KeyHint {
    (key.to_string(), label.to_string())
}

fn game_hints(g: &Game) -> Vec<KeyHint> {
    let count = |k: &str, n: usize| if n == 0 { k.to_string() } else { format!("{k} ({n})") };
    let mut hints = vec![
        hint("ESPAÇO", "mogar"),
        hint("↑↓", "alvo"),
        hint("M", "mewar"),
        hint("L", "loja"),
        hint("C", "conquistas"),
        hint("P", "ranks"),
        hint("S", "servidor"),
        hint("D", "duelar"),
        hint("F", "fila"),
        (count("T", g.chat.unread()), "chat".into()),
        (count("G", g.clan.pending()), "clã".into()),
        hint("?", "ajuda"),
        hint("Q", "sair"),
    ];
    for def in CATALOG.iter().filter(|d| g.player.level(d.id) > 0) {
        if let Some((k, _)) = def.usage {
            hints.push((k.to_string(), format!("{} x{}", def.name, g.player.level(def.id))));
        }
    }
    hints
}

/// Packs hints into as few lines as fit `width`, never splitting one.
pub(crate) fn hint_lines(hints: &[KeyHint], width: u16) -> Vec<Line<'static>> {
    let key_style = Style::new().bg(Color::DarkGray).fg(Color::White).bold();
    let mut lines = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for (key, label) in hints {
        let key_w = if key.is_empty() { 0 } else { key.chars().count() + 2 };
        let size = key_w + label.chars().count() + 2;
        if used > 0 && used + size > width as usize {
            lines.push(Line::from(std::mem::take(&mut current)));
            used = 0;
        }
        if !key.is_empty() {
            current.push(Span::styled(format!(" {key} "), key_style));
        }
        current.push(Span::raw(format!(" {label} ")));
        used += size;
    }
    if !current.is_empty() {
        lines.push(Line::from(current));
    }
    lines
}

/// Centered box taking a share of the screen; phones get all of the width, short screens all of the height.
fn popup_area(area: Rect, pct_x: u16, pct_y: u16) -> Rect {
    let pct_x = if area.width < NARROW_WIDTH { 100 } else { pct_x };
    let pct_y = if area.height < SHORT_HEIGHT || area.width < NARROW_WIDTH { 100 } else { pct_y };
    let [_, mid, _] = Layout::vertical([
        Constraint::Percentage((100 - pct_y) / 2),
        Constraint::Percentage(pct_y),
        Constraint::Percentage((100 - pct_y) / 2),
    ])
    .areas(area);
    let [_, center, _] = Layout::horizontal([
        Constraint::Percentage((100 - pct_x) / 2),
        Constraint::Percentage(pct_x),
        Constraint::Percentage((100 - pct_x) / 2),
    ])
    .areas(mid);
    center
}

/// Popup with a scrolling list; the renderer stores how far it can scroll.
fn draw_list_popup(f: &mut Frame, g: &Game, title: &str, lines: Vec<Line<'static>>) {
    let area = popup_area(f.area(), 80, 85);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(fg(Color::Magenta))
        .title(Span::styled(title.to_string(), fg(Color::Magenta).bold()));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [body, hint_area] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let max_scroll = (paragraph.line_count(body.width) as u16).saturating_sub(body.height);
    g.list_max_scroll.set(max_scroll);
    let scroll = g.list_scroll.min(max_scroll);
    f.render_widget(paragraph.scroll((scroll, 0)), body);
    let more = if scroll < max_scroll { "  ↓ tem mais abaixo" } else { "" };
    f.render_widget(
        Line::from(vec![
            Span::styled(if hint_area.width < NARROW_WIDTH { "↑↓ rolar  Esc fecha" } else { "↑ ↓ rolar   PgUp PgDn página   Esc fecha" }, fg(Color::DarkGray)),
            Span::styled(more, fg(Color::Yellow)),
        ]),
        hint_area,
    );
}

fn draw_achievements(f: &mut Frame, g: &Game) {
    let p = &g.player;
    let mut lines = vec![
        Line::styled(
            format!(
                "Mogs\u{a0}{}/{}   Defesas\u{a0}{}/{}   PERFECTs\u{a0}{}   Melhor\u{a0}streak\u{a0}{}   Mews\u{a0}{}",
                p.mogs_won,
                p.mogs_won + p.mogs_lost,
                p.defends_won,
                p.defends_won + p.defends_lost,
                p.perfects,
                p.best_streak,
                p.mews
            ),
            fg(Color::Cyan),
        ),
        Line::styled(
            format!(
                "Duelos\u{a0}{}V\u{a0}{}D   Rodadas\u{a0}vencidas\u{a0}{}   Minigames\u{a0}vencidos\u{a0}{}/{}   Consumíveis\u{a0}usados\u{a0}{}",
                p.duels_won,
                p.duels_lost,
                p.duel_rounds_won,
                p.minigames_won.len(),
                Minigame::ALL.len(),
                p.items_used
            ),
            fg(Color::Cyan),
        ),
    ];
    for group in Group::ALL {
        let members: Vec<Achievement> = Achievement::ALL.into_iter().filter(|a| a.group() == group).collect();
        let done = members.iter().filter(|a| p.achievements.contains(a)).count();
        lines.push(Line::raw(""));
        lines.push(Line::styled(format!("── {} ({done}/{}) ──", group.label(), members.len()), fg(Color::Magenta).bold()));
        for a in members {
            lines.push(if p.achievements.contains(&a) {
                Line::styled(format!("[x] {} · {}", a.name(), a.desc()), fg(Color::Yellow).bold())
            } else if a.secret() {
                Line::styled("[ ] ??? · Conquista secreta", fg(Color::DarkGray))
            } else {
                let mut spans = vec![Span::styled(format!("[ ] {} · {}", a.name(), a.desc()), fg(Color::DarkGray))];
                if let Some((have, need)) = a.progress(p).filter(|(have, need)| *need > 1 && *have > 0) {
                    spans.push(Span::styled(format!("  {}/{}", format_count(have), format_count(need)), fg(Color::Gray)));
                }
                Line::from(spans)
            });
        }
    }
    let title = format!(" Conquistas {}/{} ", p.achievements.len(), Achievement::ALL.len());
    draw_list_popup(f, g, &title, lines);
}

/// Text bar filled to `ratio`.
fn progress_bar(ratio: f64, width: usize) -> String {
    let filled = ((ratio.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

fn draw_ranks(f: &mut Frame, g: &Game, now: Instant) {
    let p = &g.player;
    let current = p.rank();
    let mut head = vec![Span::raw("Seu rank: ")];
    head.extend(flair::name_spans(current.label(), current, now));
    head.push(Span::styled(format!("   MP Total {}", format_mp(p.lifetime)), fg(Color::Cyan)));
    let mut lines = vec![Line::from(head)];
    lines.push(match current.next() {
        Some(next) => {
            let (from, to) = (current.threshold(), next.threshold());
            let ratio = (p.lifetime - from) / (to - from);
            Line::from(vec![
                Span::styled(progress_bar(ratio, 24), fg(rank_color(next))),
                Span::styled(
                    format!(" {:.0}% até {} · faltam {} MP", ratio.clamp(0.0, 1.0) * 100.0, next.label(), format_mp(to - p.lifetime)),
                    fg(Color::Gray),
                ),
            ])
        }
        None => Line::styled("Rank máximo. Não sobrou ninguém para moggar.", fg(Color::Yellow).bold()),
    });
    lines.push(Line::styled("O rank sobe com o MP Total (tudo o que você já ganhou, mesmo gastando).", fg(Color::DarkGray)));
    for rank in Rank::ALL {
        let reached = rank <= current;
        let (marker, marker_style) = match rank.cmp(&current) {
            std::cmp::Ordering::Equal => ("▶ ", fg(Color::Magenta).bold()),
            std::cmp::Ordering::Less => ("✓ ", fg(Color::Green)),
            std::cmp::Ordering::Greater => ("  ", fg(Color::DarkGray)),
        };
        let name = flair::name_spans(rank.label(), rank, now);
        let width: usize = name.iter().map(|s| s.content.chars().count()).sum();
        let mut first = vec![Span::styled(marker, marker_style)];
        first.extend(name);
        first.push(Span::raw(" ".repeat(RANK_NAME_COLUMN.saturating_sub(width))));
        first.push(Span::styled(format!("{:>8} MP", format_mp(rank.threshold())), fg(Color::Cyan)));
        if rank == current {
            first.push(Span::styled("  ◀ você", fg(Color::Magenta).bold()));
        } else if !reached {
            first.push(Span::styled(format!("  faltam {}", format_mp(rank.threshold() - p.lifetime)), fg(Color::DarkGray)));
        }
        let mut details = vec![rank.desc().to_string()];
        let unlocks = CATALOG.iter().filter(|d| d.min_rank == rank).count();
        if unlocks > 0 {
            details.push(if unlocks == 1 { "libera 1 item".to_string() } else { format!("libera {unlocks} itens") });
        }
        let here: Vec<&str> =
            g.peers.iter().filter(|peer| !peer.is_npc() && Rank::from_lifetime(peer.lifetime) == rank).map(|peer| peer.name.as_str()).collect();
        if !here.is_empty() {
            details.push(format!("na rede: {}", here.join(", ")));
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(first));
        lines.push(Line::styled(format!("    {}", details.join(" · ")), fg(if reached { Color::Gray } else { Color::DarkGray })));
    }
    let title = format!(" Ranks {}/{} ", current as usize + 1, Rank::ALL.len());
    draw_list_popup(f, g, &title, lines);
}

fn draw_shop(f: &mut Frame, g: &Game) {
    let popup = popup_area(f.area(), 94, 88);
    f.render_widget(Clear, popup);
    let area = if popup.width >= SHOP_PREVIEW_MIN_WIDTH {
        let [table, preview] =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(SHOP_PREVIEW_WIDTH)]).areas(popup);
        draw_item_preview(f, preview, g, &CATALOG[g.shop_cursor]);
        table
    } else {
        popup
    };
    let p = &g.player;
    let rank = p.rank();
    let category = g.shop_category();
    let compact = area.width < SHOP_FULL_TABLE_WIDTH;
    let title = if compact { format!(" Loja · {} MP ", format_mp(p.mp)) } else { format!(" Loja · {} MP · ←→ categoria · Enter compra · Esc fecha ", format_mp(p.mp)) };
    let block = Block::bordered().border_type(BorderType::Double).border_style(fg(Color::Magenta)).title(title).title_bottom(shop_status(g));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let details_h = if compact { SHOP_DETAILS_ROWS } else { 0 };
    let [tabs, _, area, details] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0), Constraint::Length(details_h)]).areas(inner);
    let tabs_line = shop_tabs(g, category);
    if tabs_line.width() > tabs.width as usize {
        let at = Category::ALL.iter().position(|c| *c == category).unwrap_or(0);
        f.render_widget(pager_line(category.label(), at, Category::ALL.len()), tabs);
    } else {
        f.render_widget(tabs_line, tabs);
    }
    if compact {
        let def = &CATALOG[g.shop_cursor];
        let lines = vec![
            Line::styled("─".repeat(details.width as usize), fg(Color::DarkGray)),
            Line::styled(def.name, fg(Color::Yellow).bold()),
            Line::from(vec![Span::raw("Requer "), Span::styled(def.min_rank.label(), fg(rank_color(def.min_rank))), Span::raw(" · "), Span::raw(def.summary())]),
            Line::styled("←→ categoria · Enter compra · Esc fecha", fg(Color::DarkGray)),
        ];
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), details);
    }
    let shown: Vec<usize> = category.items().collect();
    let rows = shown.iter().map(|i| &CATALOG[*i]).map(|def| {
        let level = p.level(def.id);
        let locked = rank < def.min_rank;
        let maxed = level >= def.max_level;
        let cost = def.cost(level);
        let (cost_text, cost_color) = if locked {
            ("bloqueado".to_string(), Color::DarkGray)
        } else if maxed {
            ("MÁX".to_string(), Color::DarkGray)
        } else {
            (format_mp(cost), if p.mp >= cost { Color::Green } else { Color::Red })
        };
        let level_text = if def.consumable() { format!("x{level}") } else { format!("{level}/{}", def.max_level) };
        let mut cells = vec![Cell::from(def.name), Cell::from(level_text), Cell::from(cost_text).style(fg(cost_color))];
        if !compact {
            cells.push(Cell::from(def.min_rank.label()).style(fg(rank_color(def.min_rank))));
            cells.push(Cell::from(def.summary()));
        }
        Row::new(cells)
        .style(if locked { fg(Color::DarkGray) } else { Style::new() })
    });
    let widths = if compact {
        vec![Constraint::Min(10), Constraint::Length(6), Constraint::Length(9)]
    } else {
        vec![
            Constraint::Length(CATALOG.iter().map(|d| d.name.chars().count()).max().unwrap_or(21) as u16 + 1),
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Min(20),
        ]
    };
    let header = if compact { Row::new(vec!["Item", "Nível", "Custo"]) } else { Row::new(vec!["Item", "Nível", "Custo", "Requer", "Efeito"]) };
    let table = Table::new(rows, widths)
    .header(header.style(fg(Color::Yellow).bold()))
    .row_highlight_style(Style::new().bg(Color::Rgb(45, 20, 55)))
    .highlight_symbol("▶ ");
    let mut state = TableState::default().with_selected(shown.iter().position(|i| *i == g.shop_cursor));
    f.render_stateful_widget(table, area, &mut state);
}

/// Category tabs of the shop; each shows how many of its items can be bought now.
fn shop_tabs(g: &Game, current: Category) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for category in Category::ALL {
        let ready = category.items().filter(|i| CATALOG[*i].purchase_block(&g.player).is_none()).count();
        let label = if ready > 0 { format!(" {} ·{ready} ", category.label()) } else { format!(" {} ", category.label()) };
        let style = if category == current {
            Style::new().bold().fg(Color::Black).bg(Color::Magenta)
        } else if ready > 0 {
            fg(Color::White)
        } else {
            fg(Color::DarkGray)
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

/// Bottom line of the shop: a purchase on its way.
fn shop_status(g: &Game) -> Line<'static> {
    match g.shop_pending {
        Some(item) => Line::styled(format!(" Comprando {}... ", crate::items::def(item).name), fg(Color::Yellow).bold()).centered(),
        None => Line::raw(""),
    }
}

/// Confirmation before buying, then the server's answer.
fn draw_shop_card(f: &mut Frame, g: &Game, now: Instant) {
    let fresh = g.shop_result.as_ref().filter(|(_, _, at)| now.saturating_duration_since(*at) < SHOP_RESULT_SHOW);
    let (lines, color, title) = if let Some((ok, text, _)) = fresh {
        let (mark, color) = if *ok { ("✓ COMPRADO", Color::Green) } else { ("✗ NÃO COMPROU", Color::Red) };
        let lines = vec![
            Line::raw(""),
            Line::styled(mark, fg(color).bold()).centered(),
            Line::raw(""),
            Line::raw(text.clone()).centered(),
            Line::styled(format!("Seu MP agora: {}", format_mp(g.player.mp)), fg(Color::Gray)).centered(),
            Line::raw(""),
            Line::styled("qualquer seta continua comprando · Esc fecha", fg(Color::DarkGray)).centered(),
        ];
        (lines, color, if *ok { " Compra feita " } else { " Compra recusada " })
    } else if let Some(def) = g.shop_confirm.and_then(|i| CATALOG.get(i)) {
        (confirm_lines(g, def), Color::Yellow, " Confirmar compra ")
    } else {
        return;
    };
    let area = small_popup(f.area(), 64, lines.len() as u16 + 2);
    f.render_widget(Clear, area);
    let block = Block::bordered().border_type(BorderType::Double).border_style(fg(color)).title(title);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }).block(block), area);
}

fn confirm_lines(g: &Game, def: &ItemDef) -> Vec<Line<'static>> {
    let p = &g.player;
    let level = p.level(def.id);
    let cost = def.cost(level);
    let mut lines = vec![Line::raw(""), Line::styled(format!("Comprar {}?", def.name), fg(Color::Yellow).bold()).centered()];
    let have = if def.consumable() { format!("Você tem x{level} → x{}", level + 1) } else { format!("Nível {level} → {} de {}", level + 1, def.max_level) };
    lines.push(Line::styled(have, fg(Color::Gray)).centered());
    lines.push(Line::raw(def.summary()).centered());
    lines.push(Line::raw(""));
    if let Some(reason) = def.purchase_block(p) {
        lines.push(Line::styled(format!("Não dá para comprar: {reason}"), fg(Color::Red).bold()).centered());
        lines.push(Line::raw(""));
        lines.push(Line::styled("Esc volta", fg(Color::DarkGray)).centered());
        return lines;
    }
    lines.push(
        Line::from(vec![
            Span::raw("Preço "),
            Span::styled(format!("{} MP", format_mp(cost)), fg(Color::Yellow).bold()),
            Span::raw(format!("   ·   seu MP {} → {}", format_mp(p.mp), format_mp(p.mp - cost))),
        ])
        .centered(),
    );
    if !def.consumable() {
        let diffs = stat_diffs(&p.stats(), &p.stats_after_buying(def.id));
        if !diffs.is_empty() {
            lines.push(Line::styled(diffs.join("   "), fg(Color::Cyan)).centered());
        }
    }
    lines.push(Line::raw(""));
    lines.push(
        Line::from(vec![
            Span::styled(" Enter ", Style::new().bold().fg(Color::Black).bg(Color::Green)),
            Span::raw(" comprar     "),
            Span::styled(" Esc ", Style::new().bold().fg(Color::Black).bg(Color::Red)),
            Span::raw(" cancelar"),
        ])
        .centered(),
    );
    lines
}

/// Milliseconds since the first call; drives sprite animation.
fn anim_ms() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Renders a sprite frame with half blocks: two pixels per terminal cell.
fn sprite_lines(frame: &sprites::Frame, dim: bool) -> Vec<Line<'static>> {
    let pixel = |p: Option<sprites::Rgb>| {
        p.map(|(r, g, b)| {
            if dim {
                let gray = ((r as u16 + g as u16 + b as u16) / 6) as u8;
                Color::Rgb(gray, gray, gray)
            } else {
                Color::Rgb(r, g, b)
            }
        })
    };
    frame
        .rows
        .chunks(2)
        .map(|pair| {
            let spans: Vec<Span> = pair[0]
                .iter()
                .zip(&pair[1])
                .map(|(&top, &bottom)| match (pixel(top), pixel(bottom)) {
                    (None, None) => Span::raw(" "),
                    (Some(t), None) => Span::styled("▀", fg(t)),
                    (None, Some(b)) => Span::styled("▄", fg(b)),
                    (Some(t), Some(b)) => Span::styled("▀", fg(t).bg(b)),
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

fn draw_item_preview(f: &mut Frame, area: Rect, g: &Game, def: &ItemDef) {
    let p = &g.player;
    let level = p.level(def.id);
    let locked = p.rank() < def.min_rank;
    let maxed = level >= def.max_level;
    let cost = def.cost(level);
    let heading = |t: &str| Line::styled(t.to_string(), fg(Color::Magenta).bold());

    let sprite = sprite_lines(sprites::sprite_at(def.id, anim_ms()), locked);
    let mut lines: Vec<Line> = (0..sprite.len() + 2).map(|_| Line::raw("")).collect();
    lines.push(Line::styled(def.name, fg(Color::Yellow).bold()).centered());
    lines.push(Line::styled(def.flavor, fg(Color::Gray).italic()).centered());
    lines.push(Line::raw(""));

    lines.push(heading("O QUE FAZ"));
    match def.usage {
        Some((key, text)) => {
            lines.push(Line::raw(format!(" Tecla {key}: {text}.")));
            lines.push(Line::styled(" Fica no inventário até você usar.", fg(Color::Gray)));
        }
        None => {
            lines.push(Line::raw(" Cada nível comprado dá:"));
            lines.extend(def.effects.iter().map(|e| Line::raw(format!("  • {}", e.describe()))));
        }
    }
    lines.push(Line::raw(""));

    lines.push(heading("SITUAÇÃO"));
    if locked {
        let missing = def.min_rank.threshold() - p.lifetime;
        lines.push(Line::styled(format!(" Bloqueado: precisa do rank {}.", def.min_rank.label()), fg(rank_color(def.min_rank))));
        lines.push(Line::styled(format!(" Faltam {} MP no seu Total.", format_mp(missing)), fg(Color::Gray)));
    } else if maxed {
        lines.push(Line::styled(" Nível máximo atingido.", fg(Color::Yellow)));
    } else {
        let owned = if def.consumable() { format!(" Você tem {level}.") } else { format!(" Nível {level} de {}.", def.max_level) };
        lines.push(Line::raw(owned));
        let price = if p.mp >= cost {
            Line::styled(format!(" Preço: {} MP. Enter para comprar.", format_mp(cost)), fg(Color::Green))
        } else {
            Line::styled(format!(" Preço: {} MP (faltam {}).", format_mp(cost), format_mp(cost - p.mp)), fg(Color::Red))
        };
        lines.push(price);
    }

    if !def.consumable() && !maxed {
        let diffs = stat_diffs(&p.stats(), &p.stats_after_buying(def.id));
        if !diffs.is_empty() {
            lines.push(Line::raw(""));
            lines.push(heading("COM A COMPRA"));
            lines.extend(diffs.into_iter().map(|d| Line::styled(format!("  {d}"), fg(Color::Cyan))));
        }
    }

    let block = Block::bordered().border_type(BorderType::Double).border_style(fg(Color::Magenta));
    let inner = block.inner(area);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), area);
    f.render_widget(Paragraph::new(sprite.into_iter().map(Line::centered).collect::<Vec<_>>()), Rect { y: inner.y + 1, height: inner.height.saturating_sub(1), ..inner });
}

type StatRow<'a> = (&'a str, f64, f64, &'a dyn Fn(f64) -> String);

/// Human-readable "before → after" for every stat that changes.
fn stat_diffs(before: &Stats, after: &Stats) -> Vec<String> {
    let dec = decimal;
    let pct = |v: f64| format!("{}%", decimal(v * 100.0));
    let rows: [StatRow; 13] = [
        ("Poder", before.attack, after.attack, &|v| format_mp(v)),
        ("Defesa", before.defense, after.defense, &|v| format_mp(v)),
        ("MP/s passivo", before.passive, after.passive, &|v| dec(v)),
        ("MP por mewada", before.mew_gain, after.mew_gain, &|v| dec(v)),
        ("Mewadas por segundo", before.mew_cap as f64, after.mew_cap as f64, &|v| dec(v)),
        ("Cooldown", before.cooldown, after.cooldown, &|v| format!("{} s", dec(v))),
        ("Zona PERFECT começa em", before.perfect_threshold, after.perfect_threshold, &|v| pct(v)),
        ("Bônus por streak", before.streak_bonus, after.streak_bonus, &|v| pct(v)),
        ("Bônus nas vitórias", before.win_bonus, after.win_bonus, &|v| pct(v)),
        ("Proteção nas perdas", before.loss_shield, after.loss_shield, &|v| pct(v)),
        ("Imunidade", before.immunity.as_secs_f64(), after.immunity.as_secs_f64(), &|v| format!("{} s", dec(v))),
        ("MP ao segurar mog", before.counter_bonus, after.counter_bonus, &|v| dec(v)),
        ("Velocidade do cursor", before.meter_factor, after.meter_factor, &|v| pct(v)),
    ];
    rows.iter()
        .filter(|(_, a, b, _)| (a - b).abs() > 1e-9)
        .map(|(name, a, b, show)| format!("{name}: {} → {}", show(*a), show(*b)))
        .collect()
}

fn draw_help(f: &mut Frame, g: &Game) {
    let area = popup_area(f.area(), 86, 88);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(fg(Color::Magenta))
        .title(Span::styled(" Como jogar ", fg(Color::Magenta).bold()));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [tabs_area, _, body, hint_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
            .areas(inner);

    let titles: Vec<String> = help::PAGES.iter().enumerate().map(|(i, t)| format!("{} {t}", i + 1)).collect();
    let all_width: usize = titles.iter().map(|t| t.chars().count() + 3).sum();
    if all_width > tabs_area.width as usize {
        f.render_widget(pager_line(&titles[g.help_page], g.help_page, titles.len()), tabs_area);
    } else {
        let tabs = Tabs::new(titles)
            .select(g.help_page)
            .style(fg(Color::Gray))
            .highlight_style(Style::new().fg(Color::Black).bg(Color::Magenta).bold())
            .divider("│");
        f.render_widget(tabs, tabs_area);
    }

    let paragraph = Paragraph::new(help::page(g.help_page, g)).wrap(Wrap { trim: false });
    let max_scroll = (paragraph.line_count(body.width) as u16).saturating_sub(body.height);
    g.help_max_scroll.set(max_scroll);
    let scroll = g.help_scroll.min(max_scroll);
    f.render_widget(paragraph.scroll((scroll, 0)), body);

    let more = if scroll < max_scroll { "  ↓ tem mais abaixo" } else { "" };
    let hint_line = Line::from(vec![
        Span::styled(if hint_area.width < NARROW_WIDTH { "←→ aba  ↑↓ rolar  Esc fecha" } else { "← → aba   ↑ ↓ rolar   1-8 pular   Esc fecha" }, fg(Color::DarkGray)),
        Span::styled(more, fg(Color::Yellow)),
    ]);
    f.render_widget(hint_line, hint_area);
}

/// "◀ label ▶ n/N": one tab at a time when the full strip does not fit.
fn pager_line(label: &str, at: usize, total: usize) -> Line<'static> {
    Line::from(vec![
        Span::styled(" ◀ ", fg(Color::Gray)),
        Span::styled(format!(" {label} "), Style::new().bold().fg(Color::Black).bg(Color::Magenta)),
        Span::styled(format!(" ▶  {}/{total}", at + 1), fg(Color::Gray)),
    ])
}

fn server_label(app: &App) -> String {
    match (&app.conn, app.current_host()) {
        (Some(conn), Some(_)) => format!(" {} · você hospeda ", conn.server_name),
        (Some(conn), None) => format!(" {} ", conn.server_name),
        _ => String::new(),
    }
}

const TITLE: [&str; 3] = [
    "█▀█ █▀▄▀█ █▀█ █▀▀",
    "█▀▀ █ ▀ █ █▄█ █▄█",
    "parses mogging · a arena de mog da sua rede",
];

fn draw_lobby(f: &mut Frame, app: &App) {
    let full = f.area();
    let narrow = full.width < NARROW_WIDTH;
    let mut hints = vec![hint("↑↓", "escolher"), hint("Enter", "entrar")];
    match app.selected_row() {
        Some(LobbyRow::Own(_)) => hints.push(hint("P", "parar mundo")),
        Some(LobbyRow::World(_)) => hints.extend([hint("R", "renomear"), hint("A", "apagar")]),
        _ => {}
    }
    hints.extend([hint("N", "trocar nome"), hint("C", "senha do celular")]);
    hints.push(hint("Q", "sair"));
    let footer_lines = hint_lines(&hints, full.width);
    let notice_lines = app.notice.as_ref().map(|(tone, text)| Line::styled(text.clone(), fg(tone_color(*tone)).bold()));
    let notice_h = notice_lines.as_ref().map_or(0, |l| Paragraph::new(l.clone()).wrap(Wrap { trim: true }).line_count(full.width.saturating_sub(2)) as u16 + 2);
    let title_h = match full.height {
        h if h >= LOBBY_TALL && !narrow => 9,
        h if h >= LOBBY_TALL => 5,
        _ => 3,
    };
    let [title, body, notice, footer] = Layout::vertical([
        Constraint::Length(title_h),
        Constraint::Min(6),
        Constraint::Length(notice_h),
        Constraint::Length(footer_lines.len().min(3) as u16),
    ])
    .areas(full);

    let mut title_lines = vec![Line::raw(""); title_h.saturating_sub(3) as usize / 2 + usize::from(title_h == 9)];
    title_lines.extend(TITLE[..2].iter().map(|l| Line::styled(*l, fg(Color::Magenta).bold())));
    title_lines.push(Line::styled(TITLE[2], fg(Color::Gray)));
    f.render_widget(Paragraph::new(title_lines).centered(), title);
    // Mascot to the left of the title; the 43-col title is centered, so it needs 16 + 43 + margin cols.
    let mascot_x = (title.width / 2).saturating_sub(22 + 17);
    if title.width >= 80 {
        let area = Rect::new(title.x + mascot_x, title.y, 16, title.height);
        f.render_widget(Paragraph::new(sprite_lines(sprites::mascot_at(anim_ms()), false)), area);
    }

    let [list_area, info_area] = if narrow {
        Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(body)
    } else {
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(body)
    };
    let rows = app.lobby_rows();
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            LobbyRow::Own(port) => {
                let status = app.host_on(*port).map(|h| h.status()).unwrap_or_default();
                let name = app.host_on(*port).map_or(String::new(), |h| h.name.clone());
                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(name, Style::new().bold()),
                        Span::styled("  hospedado aqui", fg(Color::LightGreen)),
                    ]),
                    Line::styled(
                        format!("   {} online · Enter para voltar a jogar · P para parar", status.online.len()),
                        fg(Color::DarkGray),
                    ),
                ])
            }
            LobbyRow::Server(entry) => {
                let mut title = vec![Span::styled(entry.name.clone(), Style::new().bold())];
                if entry.update.is_some_and(|r| app.release > 0 && r > app.release) {
                    title.push(Span::styled("  versão nova! Enter atualiza", fg(Color::LightGreen).bold()));
                } else if !entry.compatible && entry.release < app.release {
                    title.push(Span::styled("  versão mais velha · quem hospeda precisa atualizar", fg(Color::DarkGray)));
                } else if !entry.compatible {
                    title.push(Span::styled("  outra versão do jogo", fg(Color::DarkGray)));
                }
                ListItem::new(vec![
                    Line::from(title),
                    Line::styled(format!("   {} · {} jogador(es) online", entry.addr, entry.players), fg(Color::DarkGray)),
                ])
            }
            LobbyRow::World(id) => ListItem::new(vec![
                Line::from(vec![
                    Span::styled(app.world(*id).map_or(String::new(), |w| w.name.clone()), Style::new().bold()),
                    Span::styled("  salvo aqui", fg(Color::Gray)),
                ]),
                Line::styled("   Enter hospeda · R renomeia · A apaga", fg(Color::DarkGray)),
            ]),
            LobbyRow::Host => ListItem::new(vec![
                Line::styled("+ Criar mundo novo", fg(Color::LightMagenta).bold()),
                Line::styled("   um save novo, hospedado aqui e já com você dentro", fg(Color::DarkGray)),
            ]),
        })
        .collect();
    let searching = app.discovery.is_some() && rows.iter().all(|r| !matches!(r, LobbyRow::Server(_)));
    let title = if searching { " Mundos e servidores (procurando na rede...) " } else { " Mundos e servidores " };
    let list = List::new(items)
        .block(Block::bordered().border_type(BorderType::Rounded).title(title))
        .highlight_symbol("▶ ")
        .highlight_style(Style::new().bg(Color::Rgb(45, 20, 55)));
    let mut state = ListState::default().with_selected(Some(app.lobby_cursor.min(rows.len().saturating_sub(1))));
    f.render_stateful_widget(list, list_area, &mut state);

    let mut info = firewall_lines(app);
    info.extend([
        Line::styled("VOCÊ", fg(Color::Magenta).bold()),
        Line::from(vec![Span::raw("Nome: "), Span::styled(app.name.clone(), Style::new().bold())]),
        Line::styled(format!("Computador: {}", &app.machine_id[..8.min(app.machine_id.len())]), fg(Color::Gray)),
        Line::styled(
            if app.release > 0 { format!("Versão: {}", crate::update::release_label(app.release)) } else { "Versão: desenvolvimento".into() },
            fg(Color::Gray),
        ),
        Line::raw(""),
    ]);
    let phone_info = phone_lines(app, false);
    if !phone_info.is_empty() {
        info.push(Line::styled("CELULAR", fg(Color::Magenta).bold()));
        info.extend(phone_info);
        info.push(Line::raw(""));
    }
    info.extend([
        Line::styled("COMO FUNCIONA", fg(Color::Magenta).bold()),
        Line::raw("Seu progresso fica salvo no servidor em que você joga, ligado a este computador."),
        Line::raw(""),
        Line::raw("Para jogar com amigos, um de vocês hospeda e os outros entram pela lista, na mesma rede."),
    ]);
    let info_block = Block::bordered().border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(info).wrap(Wrap { trim: true }).block(info_block), info_area);

    if let Some(line) = notice_lines {
        f.render_widget(Paragraph::new(line).centered().wrap(Wrap { trim: true }).block(Block::bordered()), notice);
    }
    f.render_widget(Paragraph::new(footer_lines), footer);
}

/// Firewall warning and the command that opens it, for the lobby.
fn firewall_lines(app: &App) -> Vec<Line<'static>> {
    let Some(warning) = &app.firewall else { return Vec::new() };
    let color = if warning.sure { Color::Red } else { Color::Yellow };
    vec![
        Line::styled("FIREWALL", fg(color).bold()),
        Line::styled(warning.what.clone(), fg(color)),
        Line::raw("Os outros computadores podem não te achar. Para liberar, rode:"),
        Line::styled(warning.fix.clone(), fg(Color::LightCyan).bold()),
        Line::raw(""),
    ]
}

/// Page a phone opens to reach this PMOG, once a password exists.
fn phone_url(app: &App) -> Option<String> {
    let remote = app.remote.as_ref().filter(|r| r.port > 0 && r.has_password())?;
    Some(crate::web::page_url(crate::utils::lan_ip()?, remote.port))
}

/// Phone access status and its keys, for the server panel and the lobby.
fn phone_lines(app: &App, beside_qr: bool) -> Vec<Line<'static>> {
    let Some(remote) = &app.remote else { return Vec::new() };
    if remote.port == 0 {
        return vec![Line::styled(" Sem porta livre para o celular (47800 a 47809).", fg(Color::Red))];
    }
    if !remote.has_password() {
        return vec![Line::raw(" C  criar a senha e liberar o acesso pelo celular")];
    }
    let mut lines = Vec::new();
    match phone_url(app) {
        Some(url) => {
            lines.push(Line::raw(" Na mesma rede, abra no navegador do celular:"));
            lines.push(Line::styled(format!(" {url}"), fg(Color::LightGreen).bold()));
            if beside_qr {
                lines.push(Line::styled(" ou aponte a câmera para o QR code ao lado.", fg(Color::Gray)));
            }
        }
        None => lines.push(Line::raw(format!(" Porta {} (sem rede local encontrada)", remote.port))),
    }
    let mode = if remote.takeover() { "assume (o PC só assiste)" } else { "espelho (os dois controlam)" };
    let state = if remote.connected() { "conectado" } else { "nenhum" };
    lines.push(Line::raw(format!(" Celular: {state} · modo {mode}")));
    let mut keys = " E  trocar o modo · C  trocar a senha".to_string();
    if remote.connected() {
        keys.push_str(" · X  desconectar o celular");
    }
    lines.push(Line::raw(keys));
    lines
}

/// QR code of `text` drawn with half blocks, two modules per line, quiet zone included.
fn qr_lines(text: &str) -> Vec<Line<'static>> {
    let Ok(code) = qrcode::QrCode::new(text.as_bytes()) else { return Vec::new() };
    let width = code.width();
    let colors = code.to_colors();
    let quiet = QR_QUIET_ZONE;
    let side = width + 2 * quiet;
    let dark = |x: usize, y: usize| {
        x >= quiet && y >= quiet && x < quiet + width && y < quiet + width && colors[(y - quiet) * width + (x - quiet)] == qrcode::Color::Dark
    };
    let shade = |on: bool| if on { Color::Rgb(0, 0, 0) } else { Color::Rgb(255, 255, 255) };
    (0..side.div_ceil(2))
        .map(|row| {
            let spans: Vec<Span> = (0..side)
                .map(|x| Span::styled("▀", Style::new().fg(shade(dark(x, row * 2))).bg(shade(dark(x, row * 2 + 1)))))
                .collect();
            Line::from(spans)
        })
        .collect()
}

fn draw_server_panel(f: &mut Frame, app: &App) {
    let phone = phone_url(app);
    let qr = phone.as_deref().map(qr_lines).unwrap_or_default();
    let qr_width = qr.first().map_or(0, |l| l.width() as u16);
    let wide = !qr.is_empty() && f.area().width >= QR_PANEL_MIN_WIDTH && f.area().height >= qr.len() as u16 + 4;
    let area = if wide { popup_area(f.area(), 85, 80) } else { popup_area(f.area(), 60, 70) };
    f.render_widget(Clear, area);
    let heading = |t: &str| Line::styled(t.to_string(), fg(Color::Magenta).bold());
    let mut lines = Vec::new();
    if let Some(conn) = &app.conn {
        lines.push(heading("CONEXÃO"));
        lines.push(Line::raw(format!(" Servidor: {}", conn.server_name)));
        lines.push(Line::raw(format!(" Endereço: {}", conn.addr)));
        lines.push(Line::raw(format!(" Você: {} (jogador #{})", app.name, conn.my_id)));
        lines.push(Line::raw(""));
    }
    if let Some(host) = app.current_host() {
        let status = host.status();
        lines.push(heading("VOCÊ HOSPEDA ESTE SERVIDOR"));
        let ip = crate::utils::lan_ip().map_or("desconhecido".to_string(), |ip| ip.to_string());
        lines.push(Line::raw(format!(" Endereço na rede: {ip}:{}", host.port)));
        let db = host.db_path.as_ref().map_or("em memória".to_string(), |p| p.display().to_string());
        lines.push(Line::raw(format!(" Banco: {db}")));
        lines.push(Line::raw(format!(" Jogadores cadastrados: {}   Duelos registrados: {}", status.registered, status.duels)));
        lines.push(Line::raw(format!(" Online agora ({}): {}", status.online.len(), status.online.join(", "))));
        if let Some(err) = status.error {
            lines.push(Line::styled(format!(" Erro ao salvar: {err}"), fg(Color::Red)));
        }
        lines.push(Line::raw(""));
    }
    let phone_info = phone_lines(app, wide);
    if !phone_info.is_empty() {
        lines.push(heading("JOGAR NO CELULAR"));
        lines.extend(phone_info);
        lines.push(Line::raw(""));
    }
    lines.push(heading("AÇÕES"));
    lines.push(Line::raw(" D  desconectar e voltar para a tela inicial"));
    if app.current_host().is_some() {
        lines.push(Line::raw(" P  parar este mundo (todos são desconectados, o progresso fica salvo)"));
    }
    lines.push(Line::raw(" N  trocar seu nome (o progresso continua o mesmo)"));
    let state = if app.notifier.enabled { "ligadas" } else { "desligadas" };
    lines.push(Line::raw(format!(" Notificações do sistema: {state} (B)")));
    lines.push(Line::raw(" Esc  fechar"));
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(fg(Color::Magenta))
        .title(" Servidor ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let text_area = if wide {
        let [text, code] = Layout::horizontal([Constraint::Min(0), Constraint::Length(qr_width + 1)]).areas(inner);
        f.render_widget(Paragraph::new(qr), code);
        text
    } else {
        inner
    };
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), text_area);
}

fn small_popup(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

fn draw_text_editor(f: &mut Frame, title: &str, typed: &str) {
    let area = small_popup(f.area(), 50, 5);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(fg(Color::Magenta))
        .title(title.to_string());
    let line = Line::from(vec![Span::raw(" "), Span::styled(typed.to_string(), Style::new().bold()), Span::styled("█", fg(Color::Magenta))]);
    f.render_widget(Paragraph::new(vec![Line::raw(""), line]).block(block), area);
}

fn draw_confirm(f: &mut Frame, app: &App, question: Confirm) {
    let text = match question {
        Confirm::QuitWhileHosting => {
            format!("Você está hospedando. Sair desliga os mundos para {} jogador(es). Sair mesmo?", app.hosted_others())
        }
        Confirm::StopServer(port) => {
            let host = app.host_on(port);
            let mine = usize::from(app.current_host().is_some_and(|h| h.port == port));
            let others = host.map_or(0, |h| h.status().online.len().saturating_sub(mine));
            let name = host.map_or(String::new(), |h| h.name.clone());
            format!("Parar {name}? {others} jogador(es) serão desconectados. O progresso de todos fica salvo.")
        }
        Confirm::DeleteWorld(id) => {
            let name = app.world(id).map_or(String::new(), |w| w.name.clone());
            format!("Apagar {name}? O save vai para a lixeira do PMOG e ainda dá para recuperar de lá.")
        }
        Confirm::Disconnect => "Desconectar e voltar para a tela inicial?".to_string(),
        Confirm::Forfeit => {
            let stake = app.game.duel.as_ref().map_or(0.0, |d| d.stake);
            format!("Desistir do duelo? Você perde a aposta ({} MP).", format_mp(stake))
        }
        Confirm::QuitWhileDueling => "Sair agora conta como desistência do duelo. Sair mesmo?".to_string(),
        Confirm::Update => match app.update_offer() {
            Some((source, release)) => format!(
                "Versão nova do PMOG ({}) disponível em {}. Atualizar agora? O jogo baixa, confere a assinatura e reabre sozinho.",
                crate::update::release_label(release),
                source.label()
            ),
            None => "A versão nova sumiu. Tente de novo mais tarde.".to_string(),
        },
    };
    let area = small_popup(f.area(), 60, 7);
    f.render_widget(Clear, area);
    let block = Block::bordered().border_type(BorderType::Double).border_style(fg(Color::Yellow)).title(" Confirmar ");
    let lines = vec![
        Line::raw(""),
        Line::raw(text),
        Line::raw(""),
        Line::styled("S = sim    N = não", fg(Color::Yellow).bold()).centered(),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }).block(block), area);
}
