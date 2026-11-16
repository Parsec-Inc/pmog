//! Terminal rendering.

pub mod chat;
pub mod clan;
mod duel;
mod minigames;
pub mod flair;

use std::time::{Duration, Instant};

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Cell, Clear, Gauge, LineGauge, List, ListItem, ListState, Paragraph, Row,
    Table, TableState, Tabs, Wrap,
};

use crate::constants::{PULSE_MS, SHOP_PREVIEW_MIN_WIDTH, SHOP_PREVIEW_WIDTH, SHOP_RESULT_SHOW};
use crate::game::{Achievement, Buff, CATALOG, Game, ItemDef, Overlay, Rank, Stats, TimingTier, Tone};
use crate::app::{App, Confirm, LobbyRow, Screen};
use crate::help;
use crate::sprites::{self, Sprite};
use crate::utils::{decimal, format_mp, format_wait};

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
    if let Some(typed) = &app.editing_name {
        draw_name_editor(f, typed);
    }
    if let Some(question) = app.confirm {
        draw_confirm(f, app, question);
    }
}

fn draw_game(f: &mut Frame, app: &App) {
    let g = &app.game;
    let now = Instant::now();
    let [header, body, footer] =
        Layout::vertical([Constraint::Length(5), Constraint::Min(12), Constraint::Length(1)])
            .areas(f.area());
    let [left, center, right] = Layout::horizontal([
        Constraint::Percentage(26),
        Constraint::Percentage(48),
        Constraint::Percentage(26),
    ])
    .areas(body);

    draw_header(f, header, app, now);
    if let Some(d) = &g.duel {
        let [arena, feed] = Layout::horizontal([Constraint::Percentage(72), Constraint::Percentage(28)]).areas(body);
        duel::draw_duel(f, arena, g, now);
        draw_feed(f, feed, g);
        f.render_widget(duel::duel_footer(d), footer);
        return;
    }
    draw_peers(f, left, g, now);
    draw_center(f, center, g, now);
    draw_feed(f, right, g);
    draw_footer(f, footer, g);

    match g.overlay {
        Overlay::Shop => {
            draw_shop(f, g);
            draw_shop_card(f, g, now);
        }
        Overlay::Achievements => draw_achievements(f, g),
        Overlay::Help => draw_help(f, g),
        Overlay::Server => draw_server_panel(f, app),
        Overlay::Stake => duel::draw_stake_picker(f, g),
        Overlay::Chat => chat::draw(f, app, now),
        Overlay::Clan => clan::draw(f, app, now),
        Overlay::None => duel::draw_invite(f, g, now),
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App, now: Instant) {
    let g = &app.game;
    let p = &g.player;
    let stats = p.stats();
    let rank = p.rank();
    let border = if now < g.hit_flash_until { Color::Red } else { Color::Magenta };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(fg(border))
        .title(Span::styled(" PMOG ", fg(Color::Magenta).bold()))
        .title(Line::styled(server_label(app), fg(Color::Gray)).right_aligned());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [l1, l2, l3] = Layout::vertical([Constraint::Length(1); 3]).areas(inner);

    let mut name = vec![Span::raw(" ")];
    name.extend(flair::tagged_spans(g.clan_tag.as_deref(), &p.name, rank, now));
    name.push(Span::raw(" "));
    let line1 = Line::from([name, vec![
        Span::styled(format!("[{}]", rank.label()), fg(rank_color(rank)).bold()),
        "   MP ".into(),
        Span::styled(format_mp(p.mp), fg(Color::Yellow).bold()),
        "   Total ".into(),
        Span::raw(format_mp(p.lifetime)),
        "   Streak ".into(),
        Span::styled(
            format!("{} (x{:.2})", g.streak, g.streak_mult(&stats)),
            fg(if g.streak > 0 { Color::LightRed } else { Color::Gray }).bold(),
        ),
        Span::styled(format!("   Duelos {}V {}D", p.duels_won, p.duels_lost), fg(Color::Gray)),
    ]].concat());
    f.render_widget(line1, l1);

    let mut line2 = vec![Span::styled(
        format!(
            " Poder {}   Defesa {}   Passivo {}/s   CD {:.1}s ",
            format_mp(stats.attack),
            format_mp(stats.defense),
            format_mp(stats.passive),
            stats.cooldown
        ),
        fg(Color::Gray),
    )];
    for (buff, left) in g.buffs(now) {
        line2.push(Span::styled(format!(" {} {}{} ", buff.label(), buff.effect(), buff_left(left)), buff_style(buff)));
        line2.push(" ".into());
    }
    if now < g.immune_until {
        line2.push(Span::styled(" IMUNE ", Style::new().bg(Color::Red).fg(Color::Black).bold()));
    }
    f.render_widget(Line::from(line2), l2);

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
    f.render_widget(gauge, l3);
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
        Span::styled("  · chance BOM ", fg(Color::Gray)),
        Span::styled(pct(good), fg(odds_color(good)).bold()),
        Span::styled(" · PERFECT ", fg(Color::Gray)),
        Span::styled(pct(perfect), fg(odds_color(perfect)).bold()),
    ]
}

fn draw_center(f: &mut Frame, area: Rect, g: &Game, now: Instant) {
    let [target, button, meter, cooldown, banner] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .areas(area);
    let stats = g.player.stats();
    let ready = g.is_ready(now);

    let target_line = match g.peers.get(g.selected) {
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
    let mut target_block = Block::bordered();
    if let Some(status) = duel::pending_status(g) {
        target_block = target_block.title_bottom(status.centered());
    }
    f.render_widget(Paragraph::new(target_line).centered().block(target_block), target);

    let pulse = ((now - g.started).as_millis() / PULSE_MS).is_multiple_of(2);
    let buffs = g.buffs(now);
    let color = match (ready, pulse, buffs.is_empty()) {
        (false, _, _) => Color::DarkGray,
        (true, true, true) => Color::Magenta,
        (true, false, true) => Color::LightMagenta,
        (true, true, false) => BUFF_GOLD,
        (true, false, false) => Color::LightYellow,
    };
    let mut block = Block::bordered()
        .border_type(if ready { BorderType::Thick } else { BorderType::Plain })
        .border_style(fg(if buffs.is_empty() { color } else { BUFF_GOLD }))
        .title(Line::from(" [ESPAÇO] ").centered());
    if !buffs.is_empty() {
        let mut spans = vec![Span::styled(" ⚡ BUFFADO ", Style::new().bg(BUFF_GOLD).fg(Color::Black).bold()), " ".into()];
        for (buff, left) in &buffs {
            spans.push(Span::styled(format!(" {} {}{} ", buff.label(), buff.effect(), buff_left(*left)), buff_style(*buff)));
            spans.push(" ".into());
        }
        block = block.title_bottom(Line::from(spans).centered());
    }
    let inner = block.inner(button);
    let art: Vec<&str> = if inner.width as usize > BIG_BUTTON[0].chars().count() {
        BIG_BUTTON.to_vec()
    } else {
        vec!["M O G A R"]
    };
    let pad = (inner.height as usize).saturating_sub(art.len()) / 2;
    let mut lines = vec![Line::raw(""); pad];
    lines.extend(art.into_iter().map(|l| Line::styled(l, fg(color).bold())));
    f.render_widget(Paragraph::new(lines).centered().block(block), button);

    let legend: Vec<Span> = TimingTier::ALL
        .iter()
        .flat_map(|t| {
            [
                Span::styled("■ ", fg(tier_color(*t))),
                Span::raw(format!("{} x{}  ", t.label(), decimal(t.reward()))),
            ]
        })
        .collect();
    let meter_block = Block::bordered().title(Line::from([vec![Span::raw(" MP: ")], legend].concat()));
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

    let text = match &g.banner {
        Some(b) if now < b.until => Line::styled(b.text.clone(), fg(tone_color(b.tone)).bold()),
        _ => Line::raw(""),
    };
    f.render_widget(Paragraph::new(text).centered().block(Block::bordered()), banner);
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

fn draw_footer(f: &mut Frame, area: Rect, g: &Game) {
    let key = |k: String| Span::styled(format!(" {k} "), Style::new().bg(Color::DarkGray).fg(Color::White).bold());
    let mut spans = vec![
        key("ESPAÇO".into()), " mogar ".into(),
        key("↑↓".into()), " alvo ".into(),
        key("M".into()), " mewar ".into(),
        key("L".into()), " loja ".into(),
        key("C".into()), " conquistas ".into(),
        key("S".into()), " servidor ".into(),
        key("D".into()), " duelar ".into(),
        key("F".into()), " fila ".into(),
        key(match g.chat.unread() {
            0 => "T".to_string(),
            n => format!("T ({n})"),
        }),
        " chat ".into(),
        key(match g.clan.pending() {
            0 => "G".to_string(),
            n => format!("G ({n})"),
        }),
        " clã ".into(),
        key("?".into()), " ajuda ".into(),
        key("Q".into()), " sair  ".into(),
    ];
    for def in CATALOG.iter().filter(|d| g.player.level(d.id) > 0) {
        if let Some((k, _)) = def.usage {
            spans.push(key(k.to_string()));
            spans.push(format!(" {} x{} ", def.name, g.player.level(def.id)).into());
        }
    }
    f.render_widget(Line::from(spans), area);
}

fn popup_area(area: Rect, pct_x: u16, pct_y: u16) -> Rect {
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

fn draw_popup(f: &mut Frame, title: &str, text: Text) {
    let area = popup_area(f.area(), 70, 70);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(fg(Color::Magenta))
        .title(title.to_string());
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }).block(block), area);
}

fn draw_achievements(f: &mut Frame, g: &Game) {
    let p = &g.player;
    let mut lines: Vec<Line> = Achievement::ALL
        .iter()
        .map(|a| {
            let done = p.achievements.contains(a);
            let (mark, style) = if done { ("[x] ", fg(Color::Yellow).bold()) } else { ("[ ] ", fg(Color::DarkGray)) };
            Line::styled(format!("{mark}{} · {}", a.name(), a.desc()), style)
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!(
            "Mogs {}/{}   Defesas {}/{}   PERFECTs {}   Melhor streak {}",
            p.mogs_won,
            p.mogs_won + p.mogs_lost,
            p.defends_won,
            p.defends_won + p.defends_lost,
            p.perfects,
            p.best_streak
        ),
        fg(Color::Cyan),
    ));
    let title = format!(" Conquistas {}/{} · Esc fecha ", p.achievements.len(), Achievement::ALL.len());
    draw_popup(f, &title, Text::from(lines));
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
    let rows = CATALOG.iter().map(|def| {
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
        Row::new(vec![
            Cell::from(def.name),
            Cell::from(level_text),
            Cell::from(cost_text).style(fg(cost_color)),
            Cell::from(def.min_rank.label()).style(fg(rank_color(def.min_rank))),
            Cell::from(def.summary()),
        ])
        .style(if locked { fg(Color::DarkGray) } else { Style::new() })
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(21),
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Min(20),
        ],
    )
    .header(Row::new(["Item", "Nível", "Custo", "Requer", "Efeito"]).style(fg(Color::Yellow).bold()))
    .block(
        Block::bordered()
            .border_type(BorderType::Double)
            .border_style(fg(Color::Magenta))
            .title(format!(" Loja · {} MP · Enter compra · Esc fecha ", format_mp(p.mp)))
            .title_bottom(shop_status(g)),
    )
    .row_highlight_style(Style::new().bg(Color::Rgb(45, 20, 55)))
    .highlight_symbol("▶ ");
    let mut state = TableState::default().with_selected(Some(g.shop_cursor));
    f.render_stateful_widget(table, area, &mut state);
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

/// Renders a sprite with half blocks: two pixels per terminal cell.
fn sprite_lines(sprite: &Sprite, dim: bool) -> Vec<Line<'static>> {
    let pixel = |c: char| {
        sprites::color(c).map(|(r, g, b)| {
            if dim {
                let gray = ((r as u16 + g as u16 + b as u16) / 6) as u8;
                Color::Rgb(gray, gray, gray)
            } else {
                Color::Rgb(r, g, b)
            }
        })
    };
    sprite
        .chunks(2)
        .map(|pair| {
            let spans: Vec<Span> = pair[0]
                .chars()
                .zip(pair[1].chars())
                .map(|(top, bottom)| match (pixel(top), pixel(bottom)) {
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

    let mut lines = vec![Line::raw("")];
    lines.extend(sprite_lines(sprites::sprite(def.id), locked).into_iter().map(Line::centered));
    lines.push(Line::raw(""));
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
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), area);
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
    let [tabs_area, _, body, hint] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
            .areas(inner);

    let titles = help::PAGES.iter().enumerate().map(|(i, t)| format!("{} {t}", i + 1));
    let tabs = Tabs::new(titles)
        .select(g.help_page)
        .style(fg(Color::Gray))
        .highlight_style(Style::new().fg(Color::Black).bg(Color::Magenta).bold())
        .divider("│");
    f.render_widget(tabs, tabs_area);

    let paragraph = Paragraph::new(help::page(g.help_page, g)).wrap(Wrap { trim: false });
    let max_scroll = (paragraph.line_count(body.width) as u16).saturating_sub(body.height);
    g.help_max_scroll.set(max_scroll);
    let scroll = g.help_scroll.min(max_scroll);
    f.render_widget(paragraph.scroll((scroll, 0)), body);

    let more = if scroll < max_scroll { "  ↓ tem mais abaixo" } else { "" };
    let hint_line = Line::from(vec![
        Span::styled("← → aba   ↑ ↓ rolar   1-6 pular   Esc fecha", fg(Color::DarkGray)),
        Span::styled(more, fg(Color::Yellow)),
    ]);
    f.render_widget(hint_line, hint);
}

fn server_label(app: &App) -> String {
    match (&app.conn, &app.host) {
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
    let [title, body, notice, footer] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Min(8),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let mut title_lines = vec![Line::raw("")];
    title_lines.extend(TITLE[..2].iter().map(|l| Line::styled(*l, fg(Color::Magenta).bold())));
    title_lines.push(Line::styled(TITLE[2], fg(Color::Gray)));
    f.render_widget(Paragraph::new(title_lines).centered(), title);

    let [list_area, info_area] =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(body);
    let rows = app.lobby_rows();
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            LobbyRow::Own => {
                let status = app.host.as_ref().map(|h| h.status()).unwrap_or_default();
                let name = app.host.as_ref().map_or(String::new(), |h| h.name.clone());
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
            LobbyRow::Host => ListItem::new(vec![
                Line::styled("+ Hospedar servidor neste computador", fg(Color::LightMagenta).bold()),
                Line::styled("   liga um servidor aqui e já te conecta nele", fg(Color::DarkGray)),
            ]),
        })
        .collect();
    let searching = app.discovery.is_some() && rows.iter().all(|r| !matches!(r, LobbyRow::Server(_)));
    let title = if searching { " Servidores na rede (procurando...) " } else { " Servidores na rede " };
    let list = List::new(items)
        .block(Block::bordered().border_type(BorderType::Rounded).title(title))
        .highlight_symbol("▶ ")
        .highlight_style(Style::new().bg(Color::Rgb(45, 20, 55)));
    let mut state = ListState::default().with_selected(Some(app.lobby_cursor.min(rows.len().saturating_sub(1))));
    f.render_stateful_widget(list, list_area, &mut state);

    let info = vec![
        Line::styled("VOCÊ", fg(Color::Magenta).bold()),
        Line::from(vec![Span::raw("Nome: "), Span::styled(app.name.clone(), Style::new().bold())]),
        Line::styled(format!("Computador: {}", &app.machine_id[..8.min(app.machine_id.len())]), fg(Color::Gray)),
        Line::styled(
            if app.release > 0 { format!("Versão: {}", crate::update::release_label(app.release)) } else { "Versão: desenvolvimento".into() },
            fg(Color::Gray),
        ),
        Line::raw(""),
        Line::styled("COMO FUNCIONA", fg(Color::Magenta).bold()),
        Line::raw("Seu progresso fica salvo no servidor em que você joga, ligado a este computador."),
        Line::raw(""),
        Line::raw("Para jogar com amigos, um de vocês hospeda e os outros entram pela lista, na mesma rede."),
    ];
    let info_block = Block::bordered().border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(info).wrap(Wrap { trim: true }).block(info_block), info_area);

    if let Some((tone, text)) = &app.notice {
        let line = Line::styled(text.clone(), fg(tone_color(*tone)).bold());
        f.render_widget(Paragraph::new(line).centered().wrap(Wrap { trim: true }).block(Block::bordered()), notice);
    }

    let key = |k: &'static str| Span::styled(format!(" {k} "), Style::new().bg(Color::DarkGray).fg(Color::White).bold());
    let mut keys = vec![key("↑↓"), " escolher ".into(), key("Enter"), " entrar ".into(), key("N"), " trocar nome ".into()];
    if app.host.is_some() {
        keys.extend([key("P"), " parar servidor ".into()]);
    }
    keys.extend([key("Q"), " sair".into()]);
    f.render_widget(Line::from(keys), footer);
}

fn draw_server_panel(f: &mut Frame, app: &App) {
    let area = popup_area(f.area(), 60, 70);
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
    if let Some(host) = &app.host {
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
    lines.push(heading("AÇÕES"));
    lines.push(Line::raw(" D  desconectar e voltar para a tela inicial"));
    if app.host.is_some() {
        lines.push(Line::raw(" P  parar o servidor (todos são desconectados, o progresso fica salvo)"));
    }
    lines.push(Line::raw(" N  trocar seu nome (o progresso continua o mesmo)"));
    let state = if app.notifier.enabled { "ligadas" } else { "desligadas" };
    lines.push(Line::raw(format!(" Notificações do sistema: {state} (B)")));
    lines.push(Line::raw(" Esc  fechar"));
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(fg(Color::Magenta))
        .title(" Servidor ");
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), area);
}

fn small_popup(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

fn draw_name_editor(f: &mut Frame, typed: &str) {
    let area = small_popup(f.area(), 50, 5);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(fg(Color::Magenta))
        .title(" Seu nome · Enter salva · Esc cancela ");
    let line = Line::from(vec![Span::raw(" "), Span::styled(typed.to_string(), Style::new().bold()), Span::styled("█", fg(Color::Magenta))]);
    f.render_widget(Paragraph::new(vec![Line::raw(""), line]).block(block), area);
}

fn draw_confirm(f: &mut Frame, app: &App, question: Confirm) {
    let others = app.host.as_ref().map_or(0, |h| h.status().online.len().saturating_sub(1));
    let text = match question {
        Confirm::QuitWhileHosting => format!("Você está hospedando. Sair desliga o servidor para {others} jogador(es). Sair mesmo?"),
        Confirm::StopServer => format!("Parar o servidor? {others} jogador(es) serão desconectados. O progresso de todos fica salvo."),
        Confirm::Disconnect => "Desconectar e voltar para a tela inicial?".to_string(),
        Confirm::Forfeit => {
            let stake = app.game.duel.as_ref().map_or(0.0, |d| d.stake);
            format!("Desistir do duelo? Você perde a aposta ({} MP).", format_mp(stake))
        }
        Confirm::QuitWhileDueling => "Sair agora conta como desistência do duelo. Sair mesmo?".to_string(),
        Confirm::Update => match app.update_offer() {
            Some((entry, release)) => format!(
                "Versão nova do PMOG ({}) disponível em {}. Atualizar agora? O jogo baixa, confere a assinatura e reabre sozinho.",
                crate::update::release_label(release),
                entry.name
            ),
            None => "A versão nova sumiu da rede. Tente de novo quando o servidor voltar.".to_string(),
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
