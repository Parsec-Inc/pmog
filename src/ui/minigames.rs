//! Screens of the drafted duel and of the newer minigames.

use ratatui::prelude::*;

use super::duel::{LOSS_COLOR, WIN_COLOR, hunter_row, seconds_left};
use super::fg;
use crate::constants::*;
use crate::duel::{
    BrowMark, BrowTrial, DraftPick, DuelKey, DuelView, FitSide, HunterProgress, HydrationSide, JawSide, Minigame, PostureSide,
    PostureWaves, Stance, Strand, fit_color, hydration_level, photo_flash, rps_throw, throw_winner,
};
use crate::game::Game;

fn key_style() -> Style {
    Style::new().bold().fg(Color::Black).bg(Color::Yellow)
}

fn side_color(mine: bool) -> Color {
    if mine { WIN_COLOR } else { LOSS_COLOR }
}

/// Picks so far as one line: "1 Encarada · 2 Fit Check ...", colored by who picked.
pub fn lineup_line(lineup: &[DraftPick], current: Option<u32>) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, pick) in lineup.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", fg(Color::DarkGray)));
        }
        let random = if pick.random { "🎲" } else { "" };
        let mut style = fg(side_color(pick.mine));
        if current == Some(i as u32 + 1) {
            style = style.bold().underlined();
        }
        spans.push(Span::styled(format!("{} {}{random}", i + 1, pick.game.name()), style));
    }
    Line::from(spans)
}

/// Draft screen: the lineup, the grid of minigames and the rules of the one under the cursor.
pub fn draft(d: &DuelView, picks: &[DraftPick], my_turn: bool, g: &Game, elapsed: f64, width: usize) -> Vec<Line<'static>> {
    let left = seconds_left(d, elapsed);
    let header = if my_turn {
        Line::styled(format!("SUA VEZ DE ESCOLHER · {left} s (acabou o tempo, sorteia)"), fg(Color::Yellow).bold())
    } else {
        Line::styled(format!("{} está escolhendo... {left} s", d.opponent), fg(Color::Gray).bold())
    };
    let mine = picks.iter().filter(|p| p.mine).count();
    let theirs = picks.len() - mine;
    let half = DUEL_ROUNDS as usize / 2;
    let mut lines = vec![
        header,
        Line::styled(format!("Você escolheu {mine}/{half} · rival {theirs}/{half} · as rodadas seguem esta ordem"), fg(Color::DarkGray)),
        Line::raw(""),
    ];
    if !picks.is_empty() {
        lines.push(lineup_line(picks, None));
        lines.push(Line::raw(""));
    }
    let cell = (width.saturating_sub(4) / DRAFT_COLUMNS).clamp(14, 26);
    let options: Vec<Option<Minigame>> = Minigame::ALL.iter().copied().map(Some).chain([None]).collect();
    for row in options.chunks(DRAFT_COLUMNS).enumerate() {
        let spans: Vec<Span> = row
            .1
            .iter()
            .enumerate()
            .map(|(col, option)| {
                let index = row.0 * DRAFT_COLUMNS + col;
                let taken = option.is_some_and(|game| picks.iter().any(|p| p.game == game));
                let label = option.map_or("🎲 Aleatório".to_string(), |game| game.name().to_string());
                let text = format!(" {:<w$.w$}", label, w = cell - 2);
                let style = if index == g.draft_cursor && my_turn {
                    Style::new().bold().fg(Color::Black).bg(Color::Magenta)
                } else if taken {
                    fg(Color::DarkGray).crossed_out()
                } else {
                    fg(Color::White)
                };
                Span::styled(text + " ", style)
            })
            .collect();
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    let rules = match options.get(g.draft_cursor).copied().flatten() {
        Some(game) => format!("{}: {}", game.name(), game.how_to()),
        None => "Aleatório: o servidor sorteia um minigame que ainda não foi escolhido.".to_string(),
    };
    lines.push(Line::styled(rules, fg(Color::Gray)));
    lines
}

pub fn jawline(target: &[u8], mine: &JawSide, theirs: &JawSide, opponent: &str) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled("ESCULPA O MAXILAR IGUAL AO MOLDE", fg(Color::Yellow).bold()), Line::raw("")];
    for level in (0..JAW_LEVELS).rev() {
        let spans: Vec<Span> = (0..target.len())
            .map(|i| {
                let here = mine.heights.get(i) == Some(&level);
                let goal = target[i] == level;
                let (text, color) = match (here, goal) {
                    (true, true) => (" ◆ ", WIN_COLOR),
                    (true, false) => (" ● ", Color::Yellow),
                    (false, true) => (" ○ ", Color::DarkGray),
                    (false, false) => (" · ", Color::Rgb(60, 60, 60)),
                };
                let style = if i == mine.cursor { fg(color).bold().bg(Color::Rgb(50, 30, 60)) } else { fg(color).bold() };
                Span::styled(text, style)
            })
            .collect();
        lines.push(Line::from(spans));
    }
    let pointer: String = (0..target.len()).map(|i| if i == mine.cursor { " ▲ " } else { "   " }).collect();
    lines.push(Line::styled(pointer, fg(Color::Magenta).bold()));
    lines.push(Line::raw(""));
    let status = |s: &JawSide| if s.done { "perfeito ✓".to_string() } else { format!("{} de erro", s.error) };
    lines.push(Line::from(vec!["Você: ".into(), Span::styled(status(mine), fg(WIN_COLOR).bold())]));
    lines.push(Line::from(vec![format!("{opponent}: ").into(), Span::styled(status(theirs), fg(LOSS_COLOR))]));
    lines.push(Line::styled("● seu ponto · ○ molde · ◆ acertou", fg(Color::DarkGray)));
    lines
}

pub fn posture(drift: &PostureWaves, mine: &PostureSide, theirs: &PostureSide, elapsed: f64, width: usize, opponent: &str) -> Vec<Line<'static>> {
    let t = elapsed * 1000.0;
    let cells = width.saturating_sub(4).clamp(20, 70);
    let range = 1.5;
    let cell_of = |x: f64| ((x.clamp(-range, range) + range) / (2.0 * range) * (cells - 1) as f64).round() as usize;
    let (lo, hi) = (cell_of(-POSTURE_BAND), cell_of(POSTURE_BAND));
    let track = |pos: f64, color: Color, mark: &'static str| {
        let at = cell_of(pos);
        let spans: Vec<Span> = (0..cells)
            .map(|i| {
                if i == at {
                    Span::styled(mark, fg(color).bold())
                } else if (lo..=hi).contains(&i) {
                    Span::styled("▒", fg(Color::Rgb(40, 110, 60)))
                } else {
                    Span::styled("─", fg(Color::DarkGray))
                }
            })
            .collect();
        Line::from(spans)
    };
    let me = drift.position(&mine.presses, t);
    let them = drift.position(&theirs.presses, t);
    let upright = me.abs() <= POSTURE_BAND;
    vec![
        Line::styled(if upright { "RETO! SEGURA AÍ" } else { "TORTO! CORRIJA COM ← →" }, fg(if upright { WIN_COLOR } else { LOSS_COLOR }).bold()),
        Line::raw(""),
        track(me, Color::White, "█"),
        track(them, LOSS_COLOR, "▼"),
        Line::raw(""),
        Line::from(vec![
            "Você reto por ".into(),
            Span::styled(decimal_secs(drift.in_band_ms(&mine.presses, t)), fg(WIN_COLOR).bold()),
            format!(" · {opponent} ").into(),
            Span::styled(decimal_secs(drift.in_band_ms(&theirs.presses, t)), fg(LOSS_COLOR)),
        ]),
        Line::styled("█ você · ▼ rival · ▒ faixa reta", fg(Color::DarkGray)),
    ]
}

/// Milliseconds as "1,8 s".
fn decimal_secs(ms: f64) -> String {
    format!("{:.1} s", ms / 1000.0).replace('.', ",")
}

fn swatch(color: u8) -> Span<'static> {
    let (bg, text) = match color {
        0 => (Color::Rgb(25, 25, 25), Color::White),
        1 => (Color::Rgb(240, 240, 240), Color::Black),
        2 => (Color::Rgb(215, 190, 145), Color::Black),
        3 => (Color::Rgb(40, 150, 70), Color::Black),
        4 => (Color::Rgb(50, 100, 220), Color::White),
        _ => (Color::Rgb(125, 25, 55), Color::White),
    };
    let name = FIT_COLORS.get(color as usize).copied().unwrap_or("?");
    Span::styled(format!(" {name:<6} "), Style::new().bold().fg(text).bg(bg))
}

pub fn fit_check(target: &[u8], offsets: &[u8], mine: &FitSide, theirs: &FitSide, elapsed: f64, opponent: &str) -> Vec<Line<'static>> {
    let t = elapsed * 1000.0;
    let mut lines = vec![Line::styled("TRAVE CADA PEÇA NA COR DO FIT DO DIA (ESPAÇO)", fg(Color::Yellow).bold()), Line::raw("")];
    for (piece, name) in FIT_PIECES.iter().enumerate() {
        let mut spans = vec![Span::raw(format!("{name:<8} ")), swatch(target[piece]), Span::raw("   ")];
        match mine.locked.get(piece) {
            Some(color) => {
                spans.push(swatch(*color));
                let right = *color == target[piece];
                spans.push(Span::styled(if right { " ✓" } else { " ✗" }, fg(side_color(right)).bold()));
            }
            None if piece == mine.locked.len() => {
                spans.push(swatch(fit_color(offsets, piece, t)));
                spans.push(Span::styled(" ◀", fg(Color::Yellow).bold()));
            }
            None => spans.push(Span::styled("   ...    ", fg(Color::DarkGray))),
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("          meta      seu look", fg(Color::DarkGray)));
    let rival = if theirs.done { "fechou o look".to_string() } else { format!("{}/{} peças travadas", theirs.locked.len(), FIT_PIECES.len()) };
    lines.push(Line::from(vec![format!("{opponent}: ").into(), Span::styled(rival, fg(LOSS_COLOR))]));
    lines
}

fn bottle(level: i32, target: i32, width: usize, color: Color) -> Span<'static> {
    let cell = |v: i32| (v.clamp(0, HYDRATION_MAX) as usize * width / HYDRATION_MAX as usize).min(width - 1);
    let (fill, goal) = (cell(level), cell(target));
    let text: String = (0..width)
        .map(|i| match (i == goal, i < fill) {
            (true, _) => '┃',
            (false, true) => '█',
            (false, false) => '░',
        })
        .collect();
    Span::styled(text, fg(color))
}

pub fn hydration(target: i32, mine: &HydrationSide, theirs: &HydrationSide, elapsed: f64, width: usize, opponent: &str) -> Vec<Line<'static>> {
    let t = elapsed * 1000.0;
    let level = |s: &HydrationSide| if s.locked { s.level } else { hydration_level(s.level, s.at, t) };
    let (me, them) = (level(mine), level(theirs));
    let bar_width = width.saturating_sub(30).clamp(10, 50);
    let state = |s: &HydrationSide| if s.locked { " fechada" } else { "" };
    let header = if mine.locked {
        Line::styled(format!("Garrafa fechada em {me}%. Esperando o rival..."), fg(Color::Gray).bold())
    } else {
        Line::styled(format!("ENCHA ATÉ {target}% E FECHE COM ESPAÇO"), fg(Color::Yellow).bold())
    };
    vec![
        header,
        Line::raw(""),
        Line::from(vec![format!("{:<12.12}", "Você").into(), bottle(me, target, bar_width, Color::LightBlue), Span::styled(format!(" {me:>3}%{}", state(mine)), fg(WIN_COLOR).bold())]),
        Line::from(vec![format!("{:<12.12}", opponent).into(), bottle(them, target, bar_width, LOSS_COLOR), Span::styled(format!(" {them:>3}%{}", state(theirs)), fg(LOSS_COLOR))]),
        Line::raw(""),
        Line::styled(format!("┃ meta {target}% · ↑ +{HYDRATION_SIP_BIG} · → +{HYDRATION_SIP_SMALL} · a garrafa esvazia sozinha"), fg(Color::DarkGray)),
    ]
}

fn photo_errors(errors: &[Option<f64>], shots: &[(f64, f64)], t: f64) -> String {
    errors
        .iter()
        .zip(shots)
        .map(|(e, shot)| match e {
            Some(ms) => format!("{ms:+.0} ms"),
            None if t > photo_flash(*shot) + PHOTO_LATE_MS => "perdeu".to_string(),
            None => "  -  ".to_string(),
        })
        .collect::<Vec<_>>()
        .join("  ")
}

pub fn photo(shots: &[(f64, f64)], mine: &[Option<f64>], theirs: &[Option<f64>], elapsed: f64, opponent: &str) -> Vec<Line<'static>> {
    let t = elapsed * 1000.0;
    let current = shots.iter().position(|s| t <= photo_flash(*s) + PHOTO_LATE_MS);
    let big = match current {
        Some(i) => {
            let (start, interval) = shots[i];
            let flash = photo_flash(shots[i]);
            let blip = (0..3).find(|k| (start + *k as f64 * interval..start + *k as f64 * interval + PHOTO_BLIP_MS).contains(&t));
            match blip {
                _ if (flash..flash + 250.0).contains(&t) => {
                    Line::styled("   ✦ FLASH ✦   ", Style::new().bold().fg(Color::Black).bg(Color::White))
                }
                Some(k) => Line::styled(format!("   {}   ", 3 - k), Style::new().bold().fg(Color::Black).bg(Color::Yellow)),
                None if t < start => Line::styled("prepare a pose...", fg(Color::DarkGray)),
                None => Line::styled("   . . .   ", fg(Color::Gray)),
            }
        }
        None => Line::styled("Sessão de fotos encerrada.", fg(Color::Gray)),
    };
    let shot = current.map_or(shots.len(), |i| i + 1);
    vec![
        Line::styled(format!("FOTO {shot}/{} · ESPAÇO NO CLIQUE DO FLASH", shots.len()), fg(Color::Yellow).bold()),
        Line::raw(""),
        big,
        Line::raw(""),
        Line::from(vec![format!("{:<12.12}", "Você").into(), Span::styled(photo_errors(mine, shots, t), fg(WIN_COLOR).bold())]),
        Line::from(vec![format!("{:<12.12}", opponent).into(), Span::styled(photo_errors(theirs, shots, t), fg(LOSS_COLOR))]),
        Line::raw(""),
        Line::styled("a contagem some antes do flash: sinta o ritmo", fg(Color::DarkGray)),
    ]
}

pub fn status(items: &[String], mine: &[usize], theirs: usize, opponent: &str) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled("DO MAIS CARO PARA O MAIS BARATO: APERTE OS NÚMEROS", fg(Color::Yellow).bold()), Line::raw("")];
    for (i, name) in items.iter().enumerate() {
        let place = mine.iter().position(|p| *p == i);
        let (mark, style) = match place {
            Some(k) => (format!("  → {}º", k + 1), fg(Color::DarkGray)),
            None => (String::new(), fg(Color::White).bold()),
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", i + 1), key_style()),
            Span::styled(format!(" {name:<24}"), style),
            Span::styled(mark, fg(WIN_COLOR).bold()),
        ]));
    }
    lines.push(Line::raw(""));
    let order: Vec<&str> = mine.iter().map(|i| items[*i].as_str()).collect();
    if !order.is_empty() {
        lines.push(Line::styled(format!("Sua ordem: {}", order.join(" > ")), fg(WIN_COLOR)));
    }
    lines.push(Line::styled(format!("{opponent}: {theirs}/{} ordenados", items.len()), fg(LOSS_COLOR)));
    lines
}

pub fn hair(strands: &[Strand], mine: &HunterProgress, theirs: &HunterProgress, elapsed: f64, opponent: &str) -> Vec<Line<'static>> {
    let t = elapsed * 1000.0;
    let showing = strands.iter().position(|s| s.showing(t));
    let side = showing.filter(|i| !mine.answered.get(*i).copied().unwrap_or(false)).map(|i| strands[i].dir);
    let strand = |dir: DuelKey, text: &'static str| Span::styled(if side == Some(dir) { text } else { "     " }, fg(Color::Yellow).bold());
    let seen = strands.iter().filter(|s| s.at <= t).count();
    vec![
        Line::styled(format!("SEGURA O CABELO · fio {seen}/{}", strands.len()), fg(Color::Yellow).bold()),
        Line::raw(""),
        Line::from(strand(DuelKey::Up, " ≀≀≀ ")),
        Line::from(vec![strand(DuelKey::Left, "≋≋≋ "), Span::styled(" ( ò_ó ) ", fg(Color::White).bold()), strand(DuelKey::Right, " ≋≋≋")]),
        Line::from(strand(DuelKey::Down, " ≀≀≀ ")),
        Line::raw(""),
        hunter_row("Você", mine, WIN_COLOR),
        hunter_row(opponent, theirs, LOSS_COLOR),
        Line::raw(""),
        Line::styled("aperte a seta do lado do fio solto", fg(Color::DarkGray)),
    ]
}

fn brow_marks(marks: &[Option<BrowMark>], trials: &[BrowTrial], t: f64) -> String {
    marks
        .iter()
        .zip(trials)
        .map(|(m, trial)| match m {
            Some(BrowMark::Reaction(ms)) => format!("{ms:.0} ms"),
            Some(BrowMark::Early) => "caiu!".to_string(),
            None if t > trial.end() => "perdeu".to_string(),
            None => "  -  ".to_string(),
        })
        .collect::<Vec<_>>()
        .join("  ")
}

pub fn brow(trials: &[BrowTrial], mine: &[Option<BrowMark>], theirs: &[Option<BrowMark>], elapsed: f64, opponent: &str) -> Vec<Line<'static>> {
    let t = elapsed * 1000.0;
    let current = trials.iter().position(|tr| t <= tr.end());
    let (brows, color) = match current.map(|i| &trials[i]) {
        Some(tr) if t >= tr.raise => ("▁▁▁▁       ▇▇▇▇", Color::Yellow),
        Some(tr) if tr.twitches.iter().any(|at| (*at..at + BROW_TWITCH_MS).contains(&t)) => ("▁▁▁▁       ▃▃▃▃", Color::Gray),
        _ => ("▁▁▁▁       ▁▁▁▁", Color::Gray),
    };
    let trial = current.map_or(trials.len(), |i| i + 1);
    vec![
        Line::styled(format!("ENCARE · {trial}/{} · ESPAÇO QUANDO A SOBRANCELHA SUBIR", trials.len()), fg(Color::Yellow).bold()),
        Line::raw(""),
        Line::styled(brows, fg(color).bold()),
        Line::styled(" (◉)       (◉) ", fg(Color::White).bold()),
        Line::styled("      ───      ", fg(Color::Gray)),
        Line::raw(""),
        Line::from(vec![format!("{:<12.12}", "Você").into(), Span::styled(brow_marks(mine, trials, t), fg(WIN_COLOR).bold())]),
        Line::from(vec![format!("{:<12.12}", opponent).into(), Span::styled(brow_marks(theirs, trials, t), fg(LOSS_COLOR))]),
        Line::raw(""),
        Line::styled("tremidinha é falsa · apertar nela, antes ou não apertar custa 1,2 s", fg(Color::DarkGray)),
    ]
}

fn stance_text(s: Option<Stance>) -> String {
    s.map_or("—".to_string(), |s| s.label().to_string())
}

pub fn reverse_mog(mine: &[Option<Stance>], theirs: &[Option<Stance>], their_ready: bool, elapsed: f64, opponent: &str) -> Vec<Line<'static>> {
    let t = elapsed * 1000.0;
    let (i, open) = rps_throw(t);
    let i = i.min(RPS_THROWS - 1);
    let span = RPS_CHOOSE_MS + RPS_REVEAL_MS;
    let mut lines = vec![];
    if open {
        let left = ((i as f64 * span + RPS_CHOOSE_MS - t) / 1000.0).max(0.0).ceil();
        lines.push(Line::styled(format!("JOGADA {}/{RPS_THROWS} · {left:.0} s para escolher", i + 1), fg(Color::Yellow).bold()));
        lines.push(Line::raw(""));
        lines.push(match mine.get(i).copied().flatten() {
            Some(s) => Line::styled(format!("Você vai de {}", s.label()), fg(WIN_COLOR).bold()),
            None => Line::from(vec![
                Span::styled(" ← ", key_style()),
                " Mogar   ".into(),
                Span::styled(" ↓ ", key_style()),
                " Defender   ".into(),
                Span::styled(" → ", key_style()),
                " Ignorar".into(),
            ]),
        });
        let rival = if their_ready { "já escolheu" } else { "pensando..." };
        lines.push(Line::styled(format!("{opponent} {rival}"), fg(LOSS_COLOR)));
    } else {
        let (a, b) = (mine.get(i).copied().flatten(), theirs.get(i).copied().flatten());
        let (label, color) = match throw_winner(a, b) {
            Some(0) => ("SUA!", WIN_COLOR),
            Some(_) => ("DO RIVAL", LOSS_COLOR),
            None => ("EMPATE", Color::Yellow),
        };
        lines.push(Line::styled(format!("JOGADA {}/{RPS_THROWS}: {label}", i + 1), fg(color).bold()));
        lines.push(Line::raw(""));
        lines.push(Line::styled(format!("{}  x  {}", stance_text(a), stance_text(b)), fg(Color::White).bold()));
        lines.push(Line::raw(""));
    }
    lines.push(Line::raw(""));
    let history: Vec<Span> = (0..RPS_THROWS)
        .map(|k| {
            let (a, b) = (mine.get(k).copied().flatten(), theirs.get(k).copied().flatten());
            let revealed = k < i || (k == i && !open);
            let (mark, color) = match (revealed, throw_winner(a, b)) {
                (false, _) => ("·", Color::DarkGray),
                (true, Some(0)) => ("✓", WIN_COLOR),
                (true, Some(_)) => ("✗", LOSS_COLOR),
                (true, None) => ("=", Color::Yellow),
            };
            Span::styled(format!(" {mark} "), fg(color).bold())
        })
        .collect();
    lines.push(Line::from(history));
    lines.push(Line::styled("Mogar > Ignorar > Defender > Mogar", fg(Color::DarkGray)));
    lines
}
