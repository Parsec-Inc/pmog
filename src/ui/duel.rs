//! Duel screen, bet picker and invitation popup.

use std::time::Instant;

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Clear, Gauge, Paragraph, Wrap};

use super::flair::{clock, tagged_spans};
use super::{KeyHint, fg, hint, small_popup};
use super::minigames;
use crate::constants::{
    ANGLE_ATTEMPTS, DUEL_ROUNDS, HUNTER_SHOW_MS, MEMORY_FLASH, MEMORY_LEAD, MEMORY_STEPS, RHYTHM_BEATS, RHYTHM_INTERVAL_MS, RHYTHM_PERFECT_MS,
    STAKE_FRACTIONS,
};
use crate::duel::{
    Board, DuelKey, DuelView, HunterProgress, MacroProblem, MacrosProgress, Minigame, Phase, Progress, RhythmProgress, TypingProgress,
    angle_pos, rhythm_beat_ms,
};
use crate::game::{Game, Rank};
use crate::utils::format_mp;

pub(super) const WIN_COLOR: Color = Color::Rgb(90, 230, 120);
pub(super) const LOSS_COLOR: Color = Color::Rgb(255, 90, 90);
const DRAW_COLOR: Color = Color::Rgb(250, 210, 80);

fn outcome_color(won: Option<bool>) -> Color {
    match won {
        Some(true) => WIN_COLOR,
        Some(false) => LOSS_COLOR,
        None => DRAW_COLOR,
    }
}

/// Wins as dots out of a majority of the regulation rounds.
fn pips(wins: u32) -> String {
    (0..(DUEL_ROUNDS / 2 + 1).max(wins)).map(|i| if i < wins { '●' } else { '○' }).collect()
}

/// "Rodada 3 de 6 · jogo", "Desempate" or the draft for the duel header; `short` drops the minigame.
fn round_label(d: &DuelView, short: bool) -> String {
    match (d.phase, d.round, short) {
        (Phase::Draft, _, _) => "Escolha dos minigames".to_string(),
        (_, r, true) if r > DUEL_ROUNDS => "Morte súbita".to_string(),
        (_, r, true) => format!("Rodada {r}/{DUEL_ROUNDS}"),
        (_, r, false) if r > DUEL_ROUNDS => format!("Desempate (morte súbita) · {}", d.game.name()),
        (_, r, false) => format!("Rodada {r} de {DUEL_ROUNDS} · {}", d.game.name()),
    }
}

/// Seconds left in the current phase, rounded up.
pub(super) fn seconds_left(d: &DuelView, elapsed: f64) -> u64 {
    (d.phase_len_ms as f64 / 1000.0 - elapsed).max(0.0).ceil() as u64
}

pub fn draw_duel(f: &mut Frame, area: Rect, g: &Game, now: Instant) {
    let Some(d) = &g.duel else { return };
    let elapsed = g.duel_elapsed(now);
    let [score, board, timer] =
        Layout::vertical([Constraint::Length(4), Constraint::Min(8), Constraint::Length(3)]).areas(area);

    let room = score.width.saturating_sub(2) as usize;
    let versus = |tags: bool, gap: &str| {
        let mut line = tagged_spans(g.clan_tag.as_deref().filter(|_| tags), &g.player.name, g.player.rank(), now);
        line.push(Span::styled(format!("{gap}{}{gap}", pips(d.my_wins)), fg(WIN_COLOR).bold()));
        line.push(Span::styled("VS", fg(Color::Magenta).bold()));
        line.push(Span::styled(format!("{gap}{}{gap}", pips(d.their_wins)), fg(LOSS_COLOR).bold()));
        line.extend(tagged_spans(d.opponent_tag.as_deref().filter(|_| tags), &d.opponent, Rank::from_lifetime(d.opponent_lifetime), now));
        Line::from(line)
    };
    // Narrow screens drop the clan tags, then the extra spacing, before cutting names.
    let line = [versus(true, "  "), versus(false, "  "), versus(false, " ")].into_iter().find(|l| l.width() <= room).unwrap_or_else(|| versus(false, " "));
    let stake = match d.stake {
        0.0 => "Amistoso".to_string(),
        stake => format!("Aposta {} MP cada", format_mp(stake)),
    };
    let long = format!("{} · {stake}", round_label(d, false));
    let info = if long.chars().count() <= room { long } else { format!("{} · {stake}", round_label(d, true)) };
    let info = Line::styled(info, fg(Color::Gray));
    let block = Block::bordered().border_type(BorderType::Double).border_style(fg(Color::Magenta)).title(" ⚔ DUELO ");
    f.render_widget(Paragraph::new(vec![line.centered(), info.centered()]).block(block), score);

    let title = if d.phase == Phase::Draft { " Escolha alternada ".to_string() } else { format!(" {} ", d.game.name()) };
    let block = Block::bordered().border_type(BorderType::Rounded).title(Line::from(title).centered());
    let inner = block.inner(board);
    f.render_widget(block, board);
    let lines = match d.phase {
        Phase::Draft => match &d.board {
            Board::Draft { picks, my_turn } => minigames::draft(d, picks, *my_turn, g, elapsed, inner.width as usize),
            _ => Vec::new(),
        },
        Phase::Countdown => countdown(d, elapsed),
        Phase::RoundOver => round_over(d),
        Phase::Finished => finished(d),
        Phase::Show | Phase::Play => playing(d, g, elapsed, inner.width as usize, now),
    };
    let pad = (inner.height as usize).saturating_sub(lines.len()) / 2;
    let mut padded = vec![Line::raw(""); pad];
    padded.extend(lines);
    f.render_widget(Paragraph::new(padded).centered().wrap(Wrap { trim: false }), inner);

    let total = d.phase_len_ms as f64 / 1000.0;
    let ratio = if total > 0.0 { (1.0 - elapsed / total).clamp(0.0, 1.0) } else { 0.0 };
    let gauge = Gauge::default()
        .block(Block::bordered().title(" Tempo "))
        .gauge_style(fg(if ratio < 0.25 { LOSS_COLOR } else { Color::Magenta }))
        .ratio(ratio)
        .label(format!("{} s", seconds_left(d, elapsed)));
    f.render_widget(gauge, timer);
}

fn countdown(d: &DuelView, elapsed: f64) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::styled(format!("PRÓXIMO: {}", d.game.name().to_uppercase()), fg(Color::Yellow).bold()),
        Line::raw(""),
        Line::styled(format!("{}", seconds_left(d, elapsed).max(1)), fg(Color::Magenta).bold()),
        Line::raw(""),
        Line::styled(d.game.how_to(), fg(Color::Gray)),
    ];
    if !d.lineup.is_empty() {
        lines.push(Line::raw(""));
        lines.push(minigames::lineup_line(&d.lineup, Some(d.round)));
    }
    lines
}

fn round_over(d: &DuelView) -> Vec<Line<'static>> {
    let (label, color) = match d.i_won_round {
        Some(true) => ("RODADA SUA!", WIN_COLOR),
        Some(false) => ("RODADA DO RIVAL", LOSS_COLOR),
        None => ("RODADA EMPATADA", DRAW_COLOR),
    };
    vec![
        Line::styled(label, fg(color).bold()),
        Line::raw(""),
        Line::raw(d.result.clone().unwrap_or_default()),
    ]
}

fn finished(d: &DuelView) -> Vec<Line<'static>> {
    let won = d.i_won.flatten();
    let pot = format_mp(d.stake);
    let friendly = d.stake <= 0.0;
    let (label, detail) = match won {
        Some(true) if friendly => ("VITÓRIA!", "Duelo amistoso: só a glória.".to_string()),
        Some(false) if friendly => ("DERROTA", "Duelo amistoso: nenhum MP perdido.".to_string()),
        Some(true) => ("VITÓRIA!", format!("Você levou a aposta: +{pot} MP")),
        Some(false) => ("DERROTA", format!("A aposta ficou com {}: -{pot} MP", d.opponent)),
        None => ("EMPATE", "As apostas voltaram para cada um.".to_string()),
    };
    vec![
        Line::styled(label, fg(outcome_color(won)).bold()),
        Line::raw(""),
        Line::raw(d.result.clone().unwrap_or_default()),
        Line::raw(detail),
        Line::raw(""),
        Line::styled(format!("Placar final {} x {}", d.my_wins, d.their_wins), fg(Color::Gray)),
    ]
}

fn playing(d: &DuelView, g: &Game, elapsed: f64, width: usize, now: Instant) -> Vec<Line<'static>> {
    match &d.board {
        Board::Staredown { i_pressed, .. } => staredown(g.staredown_go(now), *i_pressed, now),
        Board::Skincare { level, sequence, mine, theirs } => skincare(d, *level, sequence, mine, theirs, elapsed),
        Board::Mewing { mine, theirs } => mewing(*mine, *theirs, &d.opponent, width),
        Board::Angle { target, mine, theirs } => angle(*target, mine, theirs, elapsed, width, &d.opponent, g),
        Board::Typing { text, mine, theirs } => {
            let mine = g.typing_mine(now).unwrap_or(*mine);
            typing(d, text, &mine, theirs, elapsed, width)
        }
        Board::Rhythm { mine, theirs } => rhythm(mine, theirs, elapsed, width, &d.opponent),
        Board::Hunter { schedule, mine, theirs } => hunter(schedule, mine, theirs, elapsed, &d.opponent),
        Board::Macros { problems, mine, theirs } => {
            let mine = g.macros_mine().unwrap_or(*mine);
            macros(d, problems, &mine, theirs, &g.macros_input, elapsed, width)
        }
        Board::Draft { .. } => Vec::new(),
        Board::Jawline { target, mine, theirs } => minigames::jawline(target, mine, theirs, &d.opponent),
        Board::Posture { drift, mine, theirs } => minigames::posture(drift, mine, theirs, elapsed, width, &d.opponent),
        Board::FitCheck { target, offsets, mine, theirs } => minigames::fit_check(target, offsets, mine, theirs, elapsed, &d.opponent),
        Board::Hydration { target, mine, theirs } => minigames::hydration(*target, mine, theirs, elapsed, width, &d.opponent),
        Board::Photo { shots, mine, theirs } => minigames::photo(shots, mine, theirs, elapsed, &d.opponent),
        Board::Status { items, mine, theirs } => minigames::status(items, mine, *theirs, &d.opponent),
        Board::Hair { strands, mine, theirs } => minigames::hair(strands, mine, theirs, elapsed, &d.opponent),
        Board::Brow { trials, mine, theirs } => minigames::brow(trials, mine, theirs, elapsed, &d.opponent),
        Board::ReverseMog { mine, theirs, their_ready } => minigames::reverse_mog(mine, theirs, *their_ready, elapsed, &d.opponent),
    }
}

/// Cells per beat on the rhythm track.
const RHYTHM_CELLS_PER_BEAT: f64 = 6.0;

fn rhythm_row(name: &str, p: &RhythmProgress, color: Color) -> Line<'static> {
    Line::from(vec![
        format!("{:<12.12}", name).into(),
        Span::styled(
            format!("{:>5.1} pts · {:>2}/{RHYTHM_BEATS} batidas · {} perfeitas · {} fora", p.score, p.hits, p.perfects, p.misses),
            fg(color).bold(),
        ),
    ])
}

fn rhythm(mine: &RhythmProgress, theirs: &RhythmProgress, elapsed: f64, width: usize, opponent: &str) -> Vec<Line<'static>> {
    let cells = width.saturating_sub(4).clamp(16, 70);
    let hit_col = 3;
    let now_ms = elapsed * 1000.0;
    let ms_per_cell = RHYTHM_INTERVAL_MS / RHYTHM_CELLS_PER_BEAT;
    let mut track: Vec<Span> = (0..cells)
        .map(|i| if i == hit_col { Span::styled("┃", fg(Color::Yellow).bold()) } else { Span::styled("─", fg(Color::DarkGray)) })
        .collect();
    for beat in 0..RHYTHM_BEATS {
        let offset = (rhythm_beat_ms(beat) - now_ms) / ms_per_cell;
        let col = hit_col as f64 + offset.round();
        if !(0.0..cells as f64).contains(&col) {
            continue;
        }
        let span = match mine.judged.get(beat).copied().flatten() {
            Some(e) if e.abs() <= RHYTHM_PERFECT_MS => Span::styled("★", fg(WIN_COLOR).bold()),
            Some(_) => Span::styled("✓", fg(WIN_COLOR)),
            None if offset < -1.0 => Span::styled("✗", fg(LOSS_COLOR)),
            None => Span::styled("●", fg(Color::LightMagenta).bold()),
        };
        track[col as usize] = span;
    }
    let first = rhythm_beat_ms(0) - now_ms;
    let header = if first > 0.0 {
        Line::styled(format!("PRIMEIRA BATIDA EM {:.1} s", first / 1000.0), fg(Color::Magenta).bold())
    } else {
        Line::styled("ESPAÇO QUANDO A NOTA CHEGAR NA LINHA", fg(Color::Yellow).bold())
    };
    vec![
        header,
        Line::raw(""),
        Line::from(track),
        Line::raw(""),
        rhythm_row("Você", mine, WIN_COLOR),
        rhythm_row(opponent, theirs, LOSS_COLOR),
        Line::raw(""),
        Line::styled("★ perfeita · ✓ acertou · ✗ perdeu", fg(Color::DarkGray)),
    ]
}

pub(super) fn hunter_row(name: &str, p: &HunterProgress, color: Color) -> Line<'static> {
    let avg = p.avg_ms.map_or(String::new(), |ms| format!(" · {ms:.0} ms"));
    Line::from(vec![
        format!("{:<12.12}", name).into(),
        Span::styled(format!("{:>4.1} pts · {} acertos · {} erros{avg}", p.score, p.hits, p.mistakes), fg(color).bold()),
    ])
}

fn hunter(schedule: &[(f64, bool)], mine: &HunterProgress, theirs: &HunterProgress, elapsed: f64, opponent: &str) -> Vec<Line<'static>> {
    let now_ms = elapsed * 1000.0;
    let showing = schedule.iter().position(|(at, _)| (*at..at + HUNTER_SHOW_MS).contains(&now_ms));
    let seen = schedule.iter().filter(|(at, _)| *at <= now_ms).count();
    let (face, note) = match showing {
        Some(i) => {
            let target = schedule[i].1;
            let face = if target {
                Span::styled("  ( ◣ _ ◢ )  ", Style::new().bold().fg(Color::Black).bg(LOSS_COLOR))
            } else {
                Span::styled("  ( ◕ ‿ ◕ )  ", Style::new().bold().fg(Color::Black).bg(Color::LightBlue))
            };
            let note = match (mine.answered.get(i).copied().unwrap_or(false), target) {
                (true, _) => Line::styled("apertou", fg(Color::Gray)),
                (false, true) => Line::styled("HUNTER EYES! ESPAÇO!", fg(WIN_COLOR).bold()),
                (false, false) => Line::styled("cachorrinho... segura", fg(Color::LightBlue)),
            };
            (Line::from(face), note)
        }
        None => (Line::styled("  (       )  ", fg(Color::DarkGray)), Line::styled("olho aberto...", fg(Color::DarkGray))),
    };
    vec![
        Line::styled(format!("SÓ OS HUNTER EYES · rosto {}/{}", seen, schedule.len()), fg(Color::Yellow).bold()),
        Line::raw(""),
        face,
        Line::raw(""),
        note,
        Line::raw(""),
        hunter_row("Você", mine, WIN_COLOR),
        hunter_row(opponent, theirs, LOSS_COLOR),
        Line::raw(""),
        Line::styled("◣_◢ aperte · ◕‿◕ não aperte", fg(Color::DarkGray)),
    ]
}

fn macros_row(name: &str, p: &MacrosProgress, total: usize, bar_width: usize, color: Color) -> Line<'static> {
    let status = if p.done { " ✓" } else { "" };
    Line::from(vec![
        format!("{:<12.12}", name).into(),
        bar(p.solved as u32, total as u32, bar_width, color),
        Span::styled(format!(" {}/{total} · {} erros{status}", p.solved, p.errors), fg(color).bold()),
    ])
}

fn macros(
    d: &DuelView,
    problems: &[MacroProblem],
    mine: &MacrosProgress,
    theirs: &MacrosProgress,
    input: &str,
    elapsed: f64,
    width: usize,
) -> Vec<Line<'static>> {
    let total = problems.len();
    let current = if d.phase == Phase::Show {
        Line::styled(format!("PREPARE A CALCULADORA MENTAL · começa em {} s", seconds_left(d, elapsed).max(1)), fg(Color::Magenta).bold())
    } else if let Some(p) = problems.get(mine.solved).filter(|_| !mine.done) {
        Line::from(vec![
            Span::styled(format!("Conta {}/{total}:  {} = ", mine.solved + 1, p.text()), fg(Color::Yellow).bold()),
            Span::styled(format!(" {input:<5} "), Style::new().bold().fg(Color::Black).bg(Color::Yellow)),
        ])
    } else {
        Line::styled("Macros fechados! Esperando o rival...", fg(WIN_COLOR).bold())
    };
    let bar_width = width.saturating_sub(34).clamp(4, 40);
    vec![
        current,
        Line::raw(""),
        macros_row("Você", mine, total, bar_width, WIN_COLOR),
        macros_row(&d.opponent, theirs, total, bar_width, LOSS_COLOR),
        Line::raw(""),
        Line::styled("digite o número e Enter · resposta errada conta erro", fg(Color::DarkGray)),
    ]
}

/// The phrase split into lines of whole words, each char styled by how far `typed` got.
fn typing_text_lines(text: &str, typed: usize, width: usize) -> Vec<Line<'static>> {
    let cursor = Style::new().bold().fg(Color::Black).bg(Color::Yellow);
    let mut lines = Vec::new();
    let mut line: Vec<Span> = Vec::new();
    let mut len = 0;
    let mut index = 0;
    for word in text.split(' ') {
        let n = word.chars().count() + 1;
        if len > 0 && len + n > width {
            lines.push(Line::from(std::mem::take(&mut line)));
            len = 0;
        }
        for c in word.chars().chain(std::iter::once(' ')) {
            if index >= text.chars().count() {
                break;
            }
            let style = match index.cmp(&typed) {
                std::cmp::Ordering::Less => fg(WIN_COLOR),
                std::cmp::Ordering::Equal => cursor,
                std::cmp::Ordering::Greater => fg(Color::Gray),
            };
            let shown = if c == ' ' && index == typed { '·' } else { c };
            line.push(Span::styled(shown.to_string(), style));
            index += 1;
        }
        len += n;
    }
    lines.push(Line::from(line));
    lines
}

fn typing_row(name: &str, p: &TypingProgress, total: usize, bar_width: usize, color: Color) -> Line<'static> {
    let status = if p.done { " ✓".to_string() } else { String::new() };
    Line::from(vec![
        format!("{:<12.12}", name).into(),
        bar(p.typed as u32, total as u32, bar_width, color),
        Span::styled(format!(" {:>3.0} ppm {:>3.0}%{status}", p.wpm, p.accuracy * 100.0), fg(color).bold()),
    ])
}

fn typing(d: &DuelView, text: &str, mine: &TypingProgress, theirs: &TypingProgress, elapsed: f64, width: usize) -> Vec<Line<'static>> {
    let total = text.chars().count();
    let header = if d.phase == Phase::Show {
        Line::styled(format!("LEIA A FRASE · começa em {} s", seconds_left(d, elapsed).max(1)), fg(Color::Magenta).bold())
    } else if mine.done {
        Line::styled("Frase completa! Esperando o rival...", fg(WIN_COLOR).bold())
    } else {
        Line::styled("DIGITE!", fg(Color::Yellow).bold())
    };
    let mut lines = vec![header, Line::raw("")];
    let typed = if d.phase == Phase::Play { mine.typed } else { usize::MAX };
    lines.extend(typing_text_lines(text, typed.min(total + 1), width.saturating_sub(4).clamp(16, 70)));
    lines.push(Line::raw(""));
    let bar_width = width.saturating_sub(34).clamp(4, 40);
    lines.push(typing_row("Você", mine, total, bar_width, WIN_COLOR));
    lines.push(typing_row(&d.opponent, theirs, total, bar_width, LOSS_COLOR));
    lines.push(Line::raw(""));
    lines.push(Line::styled(format!("erros: você {} · rival {}", mine.errors, theirs.errors), fg(Color::DarkGray)));
    lines
}

fn staredown(go: bool, pressed: bool, now: Instant) -> Vec<Line<'static>> {
    let eyes = if go { "(⊙ ‿ ⊙)          (⊙ ‿ ⊙)" } else { "(◉ _ ◉)    ⟷    (◉ _ ◉)" };
    let mut lines = vec![Line::styled(eyes, fg(Color::White).bold()), Line::raw("")];
    if pressed {
        lines.push(Line::styled("Você apertou! Esperando o rival...", fg(Color::Gray)));
    } else if go {
        let blink = ((clock(now) * 1000.0) as u64 / 150).is_multiple_of(2);
        let style = Style::new().bold().fg(Color::Black).bg(if blink { WIN_COLOR } else { Color::Yellow });
        lines.push(Line::styled("   A G O R A !   ", style));
        lines.push(Line::raw(""));
        lines.push(Line::styled("ESPAÇO!", fg(WIN_COLOR).bold()));
    } else {
        lines.push(Line::styled("Encarem-se... não pisque.", fg(LOSS_COLOR)));
        lines.push(Line::styled("Apertar antes do AGORA perde a rodada.", fg(Color::DarkGray)));
    }
    lines
}

fn progress_text(p: &Progress, level: usize) -> (String, Color) {
    if p.failed {
        (format!("errou no passo {}", p.steps + 1), LOSS_COLOR)
    } else if p.done {
        ("rotina completa!".into(), WIN_COLOR)
    } else {
        (format!("{}/{} passos", p.steps, level), Color::Gray)
    }
}

/// Memory step a digit key stands for.
fn memory_step(key: &DuelKey) -> String {
    match key {
        DuelKey::Char(c) => {
            let i = c.to_digit(10).unwrap_or(1).saturating_sub(1) as usize;
            format!("{c} {}", MEMORY_STEPS.get(i).copied().unwrap_or("?"))
        }
        _ => String::new(),
    }
}

fn skincare(d: &DuelView, level: usize, sequence: &[DuelKey], mine: &Progress, theirs: &Progress, elapsed: f64) -> Vec<Line<'static>> {
    let memory = d.game == Minigame::Memory;
    let mut lines = vec![Line::styled(format!("Nível {level}"), fg(Color::Yellow).bold()), Line::raw("")];
    if memory && d.phase == Phase::Show {
        let shown = elapsed - MEMORY_LEAD.as_secs_f64();
        let step = (shown / MEMORY_FLASH.as_secs_f64()).floor();
        let within = shown - step * MEMORY_FLASH.as_secs_f64() < MEMORY_FLASH.as_secs_f64() * 0.8;
        lines.push(Line::styled("DECORE OS PASSOS", fg(Color::Magenta).bold()));
        lines.push(Line::raw(""));
        lines.push(match sequence.get(step.max(0.0) as usize) {
            Some(key) if shown >= 0.0 && within => {
                Line::styled(format!("  {}  ", memory_step(key)), Style::new().bold().fg(Color::Black).bg(Color::LightMagenta))
            }
            _ => Line::raw(""),
        });
        lines.push(Line::styled(format!("passo {}/{}", (step.max(-1.0) + 1.0) as usize, sequence.len()), fg(Color::DarkGray)));
        return lines;
    }
    if d.phase == Phase::Show {
        lines.push(Line::styled("MEMORIZE A ROTINA", fg(Color::Magenta).bold()));
        lines.push(Line::raw(""));
        let arrows: Vec<Span> = sequence
            .iter()
            .map(|k| Span::styled(format!(" {} ", k.arrow()), Style::new().bold().fg(Color::Black).bg(Color::LightMagenta)))
            .flat_map(|s| [s, Span::raw(" ")])
            .collect();
        lines.push(Line::from(arrows));
        let steps: Vec<&str> = sequence.iter().map(|k| k.step_name()).collect();
        lines.push(Line::styled(steps.join(" → "), fg(Color::Gray)));
        return lines;
    }
    let slots: Vec<Span> = (0..level)
        .map(|i| {
            let (text, color) = if i < mine.steps {
                ("✓", WIN_COLOR)
            } else if mine.failed && i == mine.steps {
                ("✗", LOSS_COLOR)
            } else {
                ("?", Color::DarkGray)
            };
            Span::styled(format!(" {text} "), fg(color).bold())
        })
        .collect();
    lines.push(Line::from(slots));
    lines.push(Line::raw(""));
    let (mine_text, mine_color) = progress_text(mine, level);
    let (their_text, their_color) = progress_text(theirs, level);
    lines.push(Line::from(vec!["Você: ".into(), Span::styled(mine_text, fg(mine_color).bold())]));
    lines.push(Line::from(vec![format!("{}: ", d.opponent).into(), Span::styled(their_text, fg(their_color))]));
    lines.push(Line::raw(""));
    let legend = if memory {
        MEMORY_STEPS.iter().enumerate().map(|(i, s)| format!("{} {s}", i + 1)).collect::<Vec<_>>().join("  ")
    } else {
        "↑ Tônico   ↓ Sérum   ← Hidratante   → Protetor".to_string()
    };
    lines.push(Line::styled(legend, fg(Color::DarkGray)));
    lines
}

fn bar(count: u32, max: u32, width: usize, color: Color) -> Span<'static> {
    let filled = if max == 0 { 0 } else { (count as usize * width / max as usize).min(width) };
    Span::styled("█".repeat(filled) + &"░".repeat(width - filled), fg(color))
}

fn mewing(mine: u32, theirs: u32, opponent: &str, width: usize) -> Vec<Line<'static>> {
    let bar_width = width.saturating_sub(24).clamp(4, 50);
    let max = mine.max(theirs).max(30);
    let label = |name: &str| format!("{:<12.12}", name);
    vec![
        Line::styled("ALTERNE  ←  →   (ou A  D)", fg(Color::Yellow).bold()),
        Line::raw(""),
        Line::from(vec![label("Você").into(), bar(mine, max, bar_width, WIN_COLOR), format!(" {mine:>3}").into()]),
        Line::from(vec![label(opponent).into(), bar(theirs, max, bar_width, LOSS_COLOR), format!(" {theirs:>3}").into()]),
        Line::raw(""),
        Line::styled("língua no céu da boca, mandíbula travada", fg(Color::DarkGray)),
    ]
}

fn attempts_text(attempts: &[f64]) -> String {
    (0..ANGLE_ATTEMPTS)
        .map(|i| attempts.get(i).map_or("  -  ".to_string(), |d| format!("{:.1}%", d * 100.0)))
        .collect::<Vec<_>>()
        .join("   ")
}

fn angle(target: f64, mine: &[f64], theirs: &[f64], elapsed: f64, width: usize, opponent: &str, _g: &Game) -> Vec<Line<'static>> {
    let cells = width.saturating_sub(4).clamp(10, 70);
    let cell_of = |x: f64| ((x * (cells - 1) as f64).round() as usize).min(cells - 1);
    let target_cell = cell_of(target);
    let cursor = (mine.len() < ANGLE_ATTEMPTS).then(|| cell_of(angle_pos(elapsed)));
    let track: Vec<Span> = (0..cells)
        .map(|i| {
            if Some(i) == cursor {
                Span::styled("█", fg(Color::White).bold())
            } else if i == target_cell {
                Span::styled("▼", fg(WIN_COLOR).bold())
            } else if i.abs_diff(target_cell) <= 1 {
                Span::styled("▒", fg(WIN_COLOR))
            } else {
                Span::styled("─", fg(Color::DarkGray))
            }
        })
        .collect();
    let mut lines = vec![
        Line::styled("PARE NO ALVO COM ESPAÇO", fg(Color::Yellow).bold()),
        Line::raw(""),
        Line::from(track),
        Line::raw(""),
        Line::from(vec!["Você:  ".into(), Span::styled(attempts_text(mine), fg(WIN_COLOR).bold())]),
        Line::from(vec![format!("{opponent}:  ").into(), Span::styled(attempts_text(theirs), fg(LOSS_COLOR))]),
    ];
    if mine.len() >= ANGLE_ATTEMPTS {
        lines.push(Line::raw(""));
        lines.push(Line::styled("Suas tentativas acabaram. Esperando o rival...", fg(Color::Gray)));
    }
    lines
}

pub fn duel_hints(d: &DuelView) -> Vec<KeyHint> {
    let mut hints = match (d.phase, d.game) {
        (Phase::Finished, _) => vec![hint("", "O resultado some sozinho em instantes.")],
        (Phase::Draft, _) => vec![hint("↑↓←→", "escolher"), hint("Enter", "confirmar")],
        (_, Minigame::Staredown | Minigame::Angle | Minigame::Rhythm | Minigame::Hunter | Minigame::FitCheck | Minigame::Photo | Minigame::Brow) => {
            vec![hint("ESPAÇO", "apertar")]
        }
        (_, Minigame::Macros) => vec![hint("0-9", "resposta"), hint("Enter", "enviar")],
        (_, Minigame::Skincare) => vec![hint("↑↓←→", "passos da rotina")],
        (_, Minigame::Memory) => vec![hint("1-8", "passos da rotina")],
        (_, Minigame::Status) => vec![hint("1-5", "do mais caro ao mais barato")],
        (_, Minigame::Mewing) => vec![hint("← →", "ou"), hint("A D", "mewar alternando")],
        (_, Minigame::Typing) => vec![hint("letras", "digitar a frase")],
        (_, Minigame::Jawline) => vec![hint("← →", "ponto"), hint("↑ ↓", "altura")],
        (_, Minigame::Posture) => vec![hint("← →", "endireitar")],
        (_, Minigame::Hydration) => vec![hint("↑", "+5"), hint("→", "+1"), hint("ESPAÇO", "fechar")],
        (_, Minigame::Hair) => vec![hint("↑↓←→", "lado do fio")],
        (_, Minigame::ReverseMog) => vec![hint("←", "mogar"), hint("↓", "defender"), hint("→", "ignorar")],
    };
    if d.phase != Phase::Finished {
        hints.push(hint("Esc", "desistir"));
    }
    hints
}

/// One choice in a stake picker row; the focused row shows its pick brighter.
fn stake_option(label: String, chosen: bool, focused: bool) -> Span<'static> {
    let style = match (chosen, focused) {
        (true, true) => Style::new().bold().fg(Color::Black).bg(Color::Magenta),
        (true, false) => Style::new().bold().fg(Color::Magenta),
        _ => fg(Color::Gray),
    };
    Span::styled(format!(" {label} "), style)
}

/// Bet picker for a challenge or the queue.
pub fn draw_stake_picker(f: &mut Frame, g: &Game) {
    let area = small_popup(f.area(), 64, 14);
    f.render_widget(Clear, area);
    let target = g.stake_target.and_then(|id| g.peers.iter().find(|p| p.id == id));
    let title = match target {
        Some(peer) => format!(" Desafiar {}{} ", peer.clan_tag.as_ref().map_or(String::new(), |t| format!("[{t}] ")), peer.name),
        None => " Fila de duelo ".to_string(),
    };
    let options: Vec<Span> = STAKE_FRACTIONS
        .iter()
        .enumerate()
        .map(|(i, frac)| {
            let label = match *frac {
                0.0 => "Amistoso".to_string(),
                1.0 => "Tudo".to_string(),
                f => format!("{}%", (f * 100.0) as u32),
            };
            stake_option(label, i == g.stake_cursor, g.stake_row == 0)
        })
        .flat_map(|s| [s, Span::raw("  ")])
        .collect();
    let modes: Vec<Span> = [("Aleatório", false), ("Escolher", true)]
        .into_iter()
        .map(|(label, draft)| stake_option(label.into(), draft == g.stake_draft, g.stake_row == 1))
        .flat_map(|s| [s, Span::raw("  ")])
        .collect();
    let mode_hint = if g.stake_draft {
        format!("Cada um escolhe {} minigames (na fila, só se os dois escolherem).", DUEL_ROUNDS / 2)
    } else {
        "Os minigames são sorteados.".to_string()
    };
    let fraction = STAKE_FRACTIONS[g.stake_cursor.min(STAKE_FRACTIONS.len() - 1)];
    let base = match target {
        Some(peer) => g.player.mp.min(peer.mp),
        None => g.player.mp,
    };
    let stake = (fraction * base.max(0.0)).floor();
    let estimate = match target {
        Some(_) => format!("Cada um aposta {} MP. Quem vencer leva tudo.", format_mp(stake)),
        None => format!("Até {} MP cada (vale a menor aposta e o MP do mais pobre).", format_mp(stake)),
    };
    let lines = vec![
        Line::styled("Quanto do MP do mais pobre vale o duelo?", fg(Color::Gray)),
        Line::raw(""),
        Line::from(options).centered(),
        Line::styled(estimate, fg(Color::Yellow)),
        Line::raw(""),
        Line::styled("Minigames:", fg(Color::Gray)),
        Line::from(modes).centered(),
        Line::styled(mode_hint, fg(Color::Yellow)),
        Line::raw(""),
        Line::styled("←→ escolher · ↑↓ linha · Enter confirmar · Esc cancelar", fg(Color::DarkGray)),
    ];
    let block = Block::bordered().border_type(BorderType::Double).border_style(fg(Color::Magenta)).title(title);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }).block(block), area);
}

/// Pending invitation, shown over the game until answered.
pub fn draw_invite(f: &mut Frame, g: &Game, now: Instant) {
    let Some(invite) = g.invites.first() else { return };
    let peer = g.peers.iter().find(|p| p.id == invite.peer);
    let rank = peer.map_or(Rank::Sub5, |p| Rank::from_lifetime(p.lifetime));
    let mut who = tagged_spans(peer.and_then(|p| p.clan_tag.as_deref()), &invite.name, rank, now);
    who.push(Span::styled(" te desafiou para um duelo!", Style::new().bold()));
    let blink = ((clock(now) * 1000.0) as u64 / 400).is_multiple_of(2);
    let lines = vec![
        Line::from(who).centered(),
        Line::styled(
            format!(
                "Aposta: {} MP cada · {DUEL_ROUNDS} rodadas · {} · expira em {} s",
                format_mp(invite.stake),
                if invite.draft { "cada um escolhe os minigames" } else { "minigames sorteados" },
                invite.expires_ms.div_ceil(1000)
            ),
            fg(Color::Yellow),
        )
        .centered(),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" A ", Style::new().bold().fg(Color::Black).bg(WIN_COLOR)),
            " aceitar     ".into(),
            Span::styled(" R ", Style::new().bold().fg(Color::Black).bg(LOSS_COLOR)),
            " recusar".into(),
        ])
        .centered(),
    ];
    let border = if blink { Color::Yellow } else { Color::Magenta };
    let block = Block::bordered().border_type(BorderType::Double).border_style(fg(border)).title(" ⚔ Desafio ");
    let text = Paragraph::new(lines).wrap(Wrap { trim: true });
    let width = f.area().width.min(60);
    let area = small_popup(f.area(), width, text.line_count(width.saturating_sub(2)) as u16 + 3);
    f.render_widget(Clear, area);
    f.render_widget(text.block(block), area);
}

/// One-line status of a sent challenge or queue wait, if any.
pub fn pending_status(g: &Game) -> Option<Line<'static>> {
    if let Some(out) = &g.outgoing {
        return Some(Line::styled(
            format!(" Desafio para {} · {} MP · expira em {} s · X cancela ", out.name, format_mp(out.stake), out.expires_ms.div_ceil(1000)),
            fg(Color::Yellow),
        ));
    }
    g.queued.map(|f| {
        Line::styled(format!(" Na fila de duelo (aposta {}%) · F sai ", (f * 100.0) as u32), fg(Color::Yellow))
    })
}
