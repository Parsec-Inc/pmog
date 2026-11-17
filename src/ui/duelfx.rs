//! Duel animations, all driven by the phase the server already sends:
//! a themed backdrop per minigame during the countdown, wave/aura/confetti when a round or the
//! whole duel ends (red implosion when you lose), and a heartbeat in the last seconds of play.
//! `board` paints over the board's blank cells; `screen` shakes and tints the whole frame.

use std::f64::consts::TAU;
use std::time::Instant;

use ratatui::prelude::*;

use super::burst;
use super::consume::{Cell, draw, noise};
use crate::duel::{DuelView, Minigame, Phase};
use crate::game::{HitTaken, MogHit, TimingTier};

const LETTERS: [&str; 26] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z",
];
const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
const CONFETTI: [(u8, u8, u8); 5] = [(255, 90, 90), (255, 200, 60), (90, 230, 120), (90, 170, 255), (230, 90, 230)];

fn theme(game: Minigame) -> (u8, u8, u8) {
    match game {
        Minigame::Staredown => (255, 70, 70),
        Minigame::Skincare => (255, 150, 200),
        Minigame::Mewing => (100, 220, 200),
        Minigame::Angle => (255, 200, 60),
        Minigame::Typing => (120, 255, 160),
        Minigame::Rhythm => (230, 80, 230),
        Minigame::Hunter => (255, 120, 60),
        Minigame::Macros => (90, 200, 255),
        Minigame::Jawline => (200, 210, 230),
        Minigame::Posture => (170, 230, 90),
        Minigame::FitCheck => (255, 160, 60),
        Minigame::Memory => (190, 140, 255),
        Minigame::Hydration => (80, 160, 255),
        Minigame::Photo => (255, 240, 160),
        Minigame::Status => (255, 215, 80),
        Minigame::Hair => (210, 140, 90),
        Minigame::Brow => (255, 120, 180),
        Minigame::ReverseMog => (220, 90, 255),
    }
}

type Glyph = Option<(&'static str, f64)>;

/// Glyphs falling in some columns with a fading tail.
fn rain(c: &Cell, glyphs: &[&'static str]) -> Glyph {
    let col = c.col as u64;
    if !col.is_multiple_of(2) || noise(col, 5, 0) > 0.5 {
        return None;
    }
    let span = c.h + 10.0;
    let head = (c.age as f64 / 1000.0 * (8.0 + noise(col, 6, 0) * 10.0) + noise(col, 7, 0) * span) % span;
    let behind = head - c.row as f64;
    (0.0..7.0).contains(&behind).then(|| (glyphs[(noise(col, c.row as u64, c.age / 120) * glyphs.len() as f64) as usize % glyphs.len()], 1.0 - behind / 7.0))
}

/// Rings that repeat every `period` ms, going outward or inward.
fn rings(c: &Cell, period: u64, inward: bool) -> Glyph {
    let (dx, dy) = (c.x - c.w / 2.0, c.y - c.h);
    let max_r = (c.w / 2.0).hypot(c.h) + 2.0;
    let phase = (c.age % period) as f64 / period as f64;
    let front = if inward { 1.0 - phase } else { phase } * max_r;
    ((dx.hypot(dy) - front).abs() < 1.3).then(|| ("▒", 0.4 + 0.6 * (1.0 - front / max_r)))
}

/// Round bubbles drifting up.
fn bubbles(c: &Cell, glyphs: &[&'static str]) -> Glyph {
    (0..30u64)
        .any(|i| {
            let rise = (c.age as f64 / 1000.0 * (0.25 + noise(i, 3, 0) * 0.4) + noise(i, 4, 0)) % 1.0;
            let y = c.h - rise * (c.h + 2.0);
            let x = noise(i, 2, 0) * c.w + 2.0 * (y * 0.6 + i as f64).sin();
            c.col as i64 == x.round() as i64 && c.row as i64 == y.round() as i64
        })
        .then(|| (glyphs[(c.col as usize) % glyphs.len()], 0.8))
}

fn backdrop(game: Minigame, c: &Cell) -> Glyph {
    let (cx, cy) = (c.w / 2.0, c.h);
    let (dx, dy) = (c.x - cx, c.y - cy);
    let d = dx.hypot(dy);
    let ang = dy.atan2(dx);
    let age = c.age as f64;
    let (col, row) = (c.col as u64, c.row as u64);
    match game {
        // Rings close in on you; a stare is a staring contest with the void.
        Minigame::Staredown => rings(c, 900, true),
        Minigame::Skincare => bubbles(c, &["○", "∘", "·"]),
        // Slow tongue-on-palate waves.
        Minigame::Mewing => (0..3).any(|i| {
            let wy = c.h / 2.0 + (i as f64 - 1.0) * 4.0 + 2.5 * (c.x * 0.3 - age / 200.0 + i as f64).sin();
            (c.row as f64 - wy).abs() < 0.6
        }).then_some(("~", 0.8)),
        // A needle sweeps the dial and leaves a trail.
        Minigame::Angle => {
            let behind = (age / 600.0 - ang).rem_euclid(TAU);
            (behind < 0.9 && d < cx.hypot(cy)).then(|| ("▒", 1.0 - behind / 0.9))
        }
        Minigame::Typing => rain(c, &LETTERS),
        // The beat the track will keep: a ring per 600 ms.
        Minigame::Rhythm => rings(c, 600, false),
        // Crosshair with a gap, a lock ring and a scan line.
        Minigame::Hunter => {
            let scan = (c.row as f64 - (age / 25.0) % (c.h + 4.0)).abs() < 0.6;
            if (d - 8.0).abs() < 1.0 || ((dx.abs() < 0.6 || dy.abs() < 0.6) && d > 4.0 && d < 14.0) {
                Some(("▓", 0.9))
            } else {
                scan.then_some(("─", 0.5))
            }
        }
        Minigame::Macros => rain(c, &DIGITS),
        // Chevrons marching down the jaw.
        Minigame::Jawline => ((dx.abs() * 0.7 + c.y - age / 80.0).rem_euclid(8.0) < 1.0).then_some(("▓", 0.6)),
        // Equalizer-like neck bars swaying around the midline.
        Minigame::Posture => {
            let amp = (0.4 + 0.6 * (col as f64 * 0.4 + age / 250.0).sin().abs()) * c.h * 0.6;
            (col % 2 == 0 && dy.abs() < amp).then_some(("█", 0.5))
        }
        Minigame::FitCheck => ((c.col as f64 + 2.0 * c.row as f64 - age / 30.0).rem_euclid(12.0) < 3.0).then_some(("▒", 0.6)),
        // Memory tiles lighting up at random.
        Minigame::Memory => (col % 5 < 4 && row % 3 < 2 && noise(col / 5, row / 3, c.age / 300) < 0.2).then_some(("▓", 0.8)),
        // Water rises with a wavy surface.
        Minigame::Hydration => {
            let surface = c.h * (1.0 - (c.t * 0.9).min(1.0)) + 1.2 * (c.x * 0.4 + age / 150.0).sin();
            let r = c.row as f64;
            if (r - surface).abs() < 0.6 { Some(("~", 1.0)) } else { (r > surface).then_some(("░", 0.5)) }
        }
        // Camera flashes plus viewfinder corners.
        Minigame::Photo => {
            let flash = c.age % 700;
            let (ax, ay) = (c.x.min(c.w - c.x), (c.row as f64).min(c.h - 1.0 - c.row as f64));
            if flash < 90 {
                Some(("▓", 0.8 * (1.0 - flash as f64 / 90.0)))
            } else if (ay - 1.0).abs() < 0.6 && ax < 8.0 && ax > 2.0 {
                Some(("━", 0.8))
            } else {
                ((ax - 2.0).abs() < 0.6 && ay < 4.0 && ay > 0.5).then_some(("┃", 0.8))
            }
        }
        Minigame::Status => bubbles(c, &["✦", "·", "✧"]),
        // Strands swaying from the bottom.
        Minigame::Hair => (0..(c.w as u64 / 3)).any(|i| {
            let x = i as f64 * 3.0 + 1.5 * (c.row as f64 * 0.3 + age / 200.0 + i as f64).sin();
            c.col as i64 == x.round() as i64
        }).then_some(("│", 0.6)),
        // Arches opening above the center.
        Minigame::Brow => (dy < 0.0).then(|| rings(c, 1100, false)).flatten(),
        // A three-armed spiral.
        Minigame::ReverseMog => {
            let arm = (ang / TAU * 3.0 + d * 0.08 - age / 500.0).rem_euclid(1.0);
            (arm < 0.12 && d < cx.hypot(cy)).then_some(("▒", 0.8))
        }
    }
}

/// Paints the board for the current phase: the themed backdrop while counting down, the
/// win/lose burst when a round or the duel ends.
pub fn board(buf: &mut Buffer, area: Rect, d: &DuelView, elapsed_ms: u64) {
    match d.phase {
        Phase::Countdown => {
            // Fades in so the "PRÓXIMO" card doesn't pop over a busy screen.
            let intro = (elapsed_ms as f64 / 400.0).min(1.0);
            draw(buf, area, d.phase_len_ms.max(1), elapsed_ms, theme(d.game), "", |c| backdrop(d.game, c).map(|(g, k)| (g, k * intro)));
        }
        Phase::RoundOver | Phase::Finished => {
            let Some(res) = outcome(d) else { return };
            match res {
                Outcome::Win(h) | Outcome::Draw(h) => {
                    burst::render(buf, area, &h, elapsed_ms);
                    if matches!(res, Outcome::Win(_)) {
                        confetti(buf, area, elapsed_ms, if d.phase == Phase::Finished { 130 } else { 45 });
                    }
                }
                Outcome::Loss(h) => burst::impact(buf, area, &h, elapsed_ms),
            }
        }
        _ => {}
    }
}

/// Shake, red vignette on a loss and the heartbeat of the last seconds of play.
pub fn screen(buf: &mut Buffer, area: Rect, d: &DuelView, elapsed_ms: u64) {
    match d.phase {
        Phase::RoundOver | Phase::Finished => match outcome(d) {
            Some(Outcome::Win(h)) => burst::shake(buf, h.intensity() * 0.8, elapsed_ms),
            Some(Outcome::Draw(h)) => burst::shake(buf, h.intensity() * 0.5, elapsed_ms),
            Some(Outcome::Loss(h)) => {
                burst::vignette(buf, area, &h, elapsed_ms);
                burst::shake(buf, h.intensity() * 1.3, elapsed_ms);
            }
            None => {}
        },
        Phase::Play => {
            let left = d.phase_len_ms.saturating_sub(elapsed_ms);
            if d.phase_len_ms > 4000 && left <= 3000 {
                // One red pulse per second, two per second in the last one.
                let beat = if left <= 1000 { 500 } else { 1000 };
                let h = HitTaken { at: Instant::now(), bank: 100.0, loss: 0.0 };
                burst::vignette(buf, area, &h, (elapsed_ms % beat) * 900 / beat);
            }
        }
        _ => {}
    }
}

enum Outcome {
    Win(MogHit),
    Draw(MogHit),
    Loss(HitTaken),
}

/// What just ended and how big it is: the whole duel counts double, and shows the pot.
fn outcome(d: &DuelView) -> Option<Outcome> {
    let (won, big) = if d.phase == Phase::Finished { (d.i_won?, true) } else { (d.i_won_round, false) };
    let (at, pot) = (Instant::now(), if big { d.stake.max(0.0) } else { 0.0 });
    // `bank == gain` doubles the intensity; with no pot the whole duel just gets the base size.
    let hit = |tier, gain| MogHit { at, tier, bank: if big && pot > 0.0 { pot } else { 100.0 }, gain };
    Some(match won {
        Some(true) => Outcome::Win(hit(TimingTier::Perfect, if big { pot.max(100.0) } else { 0.0 })),
        None => Outcome::Draw(hit(TimingTier::Ok, 0.0)),
        Some(false) => Outcome::Loss(HitTaken { at, bank: if big && pot > 0.0 { pot } else { 100.0 }, loss: if big { pot } else { 0.0 } }),
    })
}

/// Confetti shot from the middle of the board that falls with gravity.
fn confetti(buf: &mut Buffer, area: Rect, age_ms: u64, n: u64) {
    if age_ms >= 1800 || area.is_empty() {
        return;
    }
    let tau = age_ms as f64 / 1500.0;
    let (cx, cy) = (area.width as f64 / 2.0, area.height as f64);
    let reach = cx.hypot(cy);
    for i in 0..n {
        let a = i as f64 * 2.399;
        let s = (0.4 + noise(i, 1, 0)) * reach * 0.9;
        let (px, py) = (cx + a.cos() * s * tau, cy + a.sin() * s * tau + 0.5 * reach * 1.2 * tau * tau);
        let (col, row) = (px.floor(), (py / 2.0).floor());
        if col < 0.0 || row < 0.0 || col >= area.width as f64 || row >= area.height as f64 {
            continue;
        }
        let beside = burst::beside_text(buf, area.x + col as u16, area.y + row as u16);
        let Some(cell) = buf.cell_mut((area.x + col as u16, area.y + row as u16)) else { continue };
        if cell.symbol() == " " && !beside {
            let (r, g, b) = CONFETTI[i as usize % CONFETTI.len()];
            let k = (1.3 - tau).clamp(0.3, 1.0);
            cell.set_symbol(["▪", "▫", "✦", "*"][i as usize % 4]).set_fg(Color::Rgb((r as f64 * k) as u8, (g as f64 * k) as u8, (b as f64 * k) as u8));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted(buf: &Buffer) -> usize {
        buf.content().iter().filter(|c| c.symbol() != " ").count()
    }

    #[test]
    fn every_minigame_has_a_backdrop_that_moves() {
        let area = Rect::new(0, 0, 80, 20);
        for game in Minigame::ALL {
            let frame = |age| {
                let mut buf = Buffer::empty(area);
                let mut c = Cell { col: 0, row: 0, x: 0.0, y: 0.0, w: 80.0, h: 20.0, t: 0.5, age };
                for row in 0..20 {
                    for col in 0..80 {
                        (c.col, c.row, c.x, c.y) = (col, row, col as f64 + 0.5, (row as f64 + 0.5) * 2.0);
                        if let Some((g, k)) = backdrop(game, &c).filter(|(_, k)| *k > 0.08) {
                            buf.cell_mut((col, row)).unwrap().set_symbol(g);
                            let _ = k;
                        }
                    }
                }
                buf
            };
            let (a, b) = (frame(300), frame(700));
            assert!(painted(&a) > 10 && painted(&b) > 10, "{game:?} paints nothing");
            assert!(a != b, "{game:?} is static");
        }
    }
}
