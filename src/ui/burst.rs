//! Wave + aura burst painted over the MOGAR button after a press, plus a screen shake.
//! Colored by the timing tier; `MogHit::intensity` (tier x MP won) scales everything: more
//! rings, a wider aura, bigger waves, a harder shake, longer life and, on PERFECT, sparkles.
//! The MP won floats up out of the button.

use ratatui::prelude::*;

use super::tier_color;
use crate::game::{HitTaken, MogHit, TimingTier};
use crate::utils::format_mp;

pub fn duration_ms(h: &MogHit) -> u64 {
    let base = match h.tier {
        TimingTier::Bad => 700.0,
        TimingTier::Ok => 900.0,
        TimingTier::Good => 1100.0,
        TimingTier::Perfect => 1500.0,
    };
    (base * (0.75 + 0.25 * h.intensity())) as u64
}

/// Shifts the whole frame by a decaying random offset, so the entire screen trembles.
pub fn shake(buf: &mut Buffer, power: f64, age_ms: u64) {
    let dur = 500.0 * (0.6 + 0.4 * power);
    let left = 1.0 - age_ms as f64 / dur;
    if left <= 0.0 {
        return;
    }
    let amp = 2.5 * power * left * left;
    let noise = |seed| hash(age_ms / 40, seed, 7) as f64 / 48.0 - 1.0;
    let (dx, dy) = ((amp * noise(1)).round() as i32, (amp * 0.4 * noise(2)).round() as i32);
    if dx == 0 && dy == 0 {
        return;
    }
    let (area, old) = (buf.area, buf.clone());
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let src = ((x as i32 - dx).try_into().ok(), (y as i32 - dy).try_into().ok());
            let cell = match src {
                (Some(sx), Some(sy)) => old.cell((sx, sy)).cloned().unwrap_or_default(),
                _ => Default::default(),
            };
            if let Some(c) = buf.cell_mut((x, y)) {
                *c = cell;
            }
        }
    }
}

/// Cheap deterministic noise for sparkles.
pub(crate) fn hash(a: u64, b: u64, c: u64) -> u64 {
    (a.wrapping_mul(73856093) ^ b.wrapping_mul(19349663) ^ c.wrapping_mul(83492791)) % 97
}

pub fn render(buf: &mut Buffer, area: Rect, h: &MogHit, age_ms: u64) {
    let (tier, power) = (h.tier, h.intensity());
    let dur = duration_ms(h);
    if age_ms >= dur || area.is_empty() {
        return;
    }
    let t = age_ms as f64 / dur as f64;
    // (rings, aura radius, wave amplitude)
    let (rings, aura_r, amp) = match tier {
        TimingTier::Bad => (1, 5.0, 0.5),
        TimingTier::Ok => (1, 8.0, 1.0),
        TimingTier::Good => (2, 11.0, 1.5),
        TimingTier::Perfect => (3, 16.0, 2.5),
    };
    // A big prize adds a ring and widens the glow and the waves.
    let (rings, aura_r, amp) = (rings + (power > 1.8) as usize, aura_r * (0.7 + 0.3 * power), amp * (0.7 + 0.3 * power));
    let Color::Rgb(r, g, b) = tier_color(tier) else { return };
    let scale = |k: f64| Color::Rgb((r as f64 * k) as u8, (g as f64 * k) as u8, (b as f64 * k) as u8);
    let phase = age_ms as f64 / 90.0;
    // Distances use half rows so a cell is ~square.
    let (cx, cy) = (area.width as f64 / 2.0, area.height as f64);
    let max_r = cx.hypot(cy) + 6.0 * rings as f64;
    let fade = (1.0 - t).powf(1.5);

    for row in 0..area.height {
        for col in 0..area.width {
            let dx = col as f64 + 0.5 - cx;
            let dy = (row as f64 + 0.5) * 2.0 - cy;
            let d = dx.hypot(dy);
            let wave = amp * (dy.atan2(dx) * 6.0 + phase).sin();
            let ring = (0..rings)
                .map(|i| {
                    let front = (t - i as f64 * 0.12).max(0.0) * max_r + wave;
                    (1.0 - (d - front).abs() / 2.5).max(0.0) * (1.0 - i as f64 * 0.25)
                })
                .fold(0.0, f64::max);
            let aura = (1.0 - d / aura_r).max(0.0).powi(2) * (0.6 + 0.4 * (phase * 0.7 - d * 0.4).sin());
            let k = (ring * fade + aura * (1.0 - t) * 0.6).min(1.0);
            if k < 0.08 {
                continue;
            }
            let beside = beside_text(buf, area.x + col, area.y + row);
            let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) else { continue };
            if cell.symbol() != " " {
                // The MOGAR letters flash in the tier color.
                cell.set_fg(Color::Rgb(r.saturating_add(70), g.saturating_add(70), b.saturating_add(70)));
                continue;
            }
            if beside {
                continue;
            }
            let sparkle = tier == TimingTier::Perfect && hash(col as u64, row as u64, age_ms / 120) == 0;
            let (glyph, color) = if sparkle {
                ("✦", Color::White)
            } else {
                (if k > 0.75 { "▓" } else if k > 0.5 { "▒" } else if k > 0.25 { "░" } else { "·" }, scale(k.max(0.35)))
            };
            cell.set_symbol(glyph).set_fg(color);
        }
    }

    // The prize floats up from the center and fades out.
    if h.gain > 0.0 && t > 0.08 && t < 0.92 {
        let text = format!("+{} MP", format_mp(h.gain));
        let start = (area.height as f64 / 2.0 - 1.0).max(0.0);
        let row = (start * (1.0 - (t * 1.3).min(1.0))).round() as u16;
        let x0 = area.x + area.width.saturating_sub(text.chars().count() as u16) / 2;
        let color = scale((1.0 - t).max(0.4));
        for (i, ch) in text.chars().enumerate() {
            let Some(cell) = buf.cell_mut((x0 + i as u16, area.y + row)) else { continue };
            cell.set_char(ch).set_fg(if t < 0.5 { Color::White } else { color }).set_style(Style::new().add_modifier(Modifier::BOLD));
        }
    }
}


const HIT_RED: (u8, u8, u8) = (230, 50, 60);

pub fn hit_duration_ms(h: &HitTaken) -> u64 {
    (900.0 * (0.75 + 0.25 * h.intensity())) as u64
}

/// A blank cell squeezed against a letter or digit is a gap inside text; effects leave those alone.
pub(super) fn beside_text(buf: &Buffer, x: u16, y: u16) -> bool {
    let text = |x: Option<u16>| x.and_then(|x| buf.cell((x, y))).is_some_and(|c| c.symbol().chars().next().is_some_and(char::is_alphanumeric));
    text(x.checked_sub(1)) || text(x.checked_add(1))
}

pub(super) fn shade(k: f64) -> &'static str {
    if k > 0.75 { "▓" } else if k > 0.5 { "▒" } else if k > 0.25 { "░" } else { "·" }
}

/// Red vignette that pulses in from the screen edges; only blank cells, so text stays readable.
pub fn vignette(buf: &mut Buffer, area: Rect, h: &HitTaken, age_ms: u64) {
    let dur = hit_duration_ms(h);
    if age_ms >= dur || area.is_empty() {
        return;
    }
    let t = age_ms as f64 / dur as f64;
    let power = h.intensity();
    let band = 4.0 + 5.0 * power;
    let pulse = 0.65 + 0.35 * (age_ms as f64 / 70.0).sin();
    let fade = (1.0 - t).powf(1.3);
    let (r, g, b) = HIT_RED;
    for row in 0..area.height {
        for col in 0..area.width {
            // Rows count double so the band is even on all four sides.
            let edge = (col.min(area.width - 1 - col) as f64).min(row.min(area.height - 1 - row) as f64 * 2.0);
            let k = (1.0 - edge / band).max(0.0).powi(2) * pulse * fade * (0.6 + 0.4 * power);
            if k < 0.1 {
                continue;
            }
            let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) else { continue };
            if cell.symbol() == " " {
                let k = k.min(1.0);
                cell.set_symbol(shade(k)).set_fg(Color::Rgb((r as f64 * k.max(0.35)) as u8, (g as f64 * k.max(0.35)) as u8, (b as f64 * k.max(0.35)) as u8));
            }
        }
    }
}

/// Shockwave rings collapsing from the edges into the center of the button, plus the MP lost
/// dropping out of it. The opposite of the mog burst, which expands.
pub fn impact(buf: &mut Buffer, area: Rect, h: &HitTaken, age_ms: u64) {
    let dur = hit_duration_ms(h);
    if age_ms >= dur || area.is_empty() {
        return;
    }
    let t = age_ms as f64 / dur as f64;
    let power = h.intensity();
    let rings = 2 + (power > 1.5) as usize;
    let amp = 1.5 + power;
    let (r, g, b) = HIT_RED;
    let phase = age_ms as f64 / 70.0;
    let (cx, cy) = (area.width as f64 / 2.0, area.height as f64);
    let max_r = cx.hypot(cy) + 4.0;
    let fade = 1.0 - t * 0.8;
    for row in 0..area.height {
        for col in 0..area.width {
            let dx = col as f64 + 0.5 - cx;
            let dy = (row as f64 + 0.5) * 2.0 - cy;
            let d = dx.hypot(dy);
            let wave = amp * (dy.atan2(dx) * 5.0 - phase).sin();
            let k = (0..rings)
                .map(|i| {
                    let front = (1.0 - (t * 1.4 - i as f64 * 0.15).clamp(0.0, 1.0)) * max_r + wave;
                    (1.0 - (d - front).abs() / 2.5).max(0.0) * (1.0 - i as f64 * 0.2)
                })
                .fold(0.0, f64::max)
                * fade;
            if k < 0.1 {
                continue;
            }
            let beside = beside_text(buf, area.x + col, area.y + row);
            let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) else { continue };
            if cell.symbol() != " " {
                cell.set_fg(Color::Rgb(255, 120, 120));
            } else if !beside {
                let c = k.max(0.35);
                cell.set_symbol(shade(k)).set_fg(Color::Rgb((r as f64 * c) as u8, (g as f64 * c) as u8, (b as f64 * c) as u8));
            }
        }
    }
    if h.loss > 0.0 && t > 0.05 && t < 0.95 {
        let text = format!("-{} MP", format_mp(h.loss));
        let start = (area.height as f64 / 2.0 - 1.0).max(0.0);
        let row = (start + (area.height as f64 - 1.0 - start) * (t * 1.3).min(1.0)).round() as u16;
        let x0 = area.x + area.width.saturating_sub(text.chars().count() as u16) / 2;
        for (i, ch) in text.chars().enumerate() {
            let Some(cell) = buf.cell_mut((x0 + i as u16, area.y + row.min(area.height - 1))) else { continue };
            cell.set_char(ch).set_fg(if t < 0.5 { Color::White } else { Color::Rgb(r, g, b) }).set_style(Style::new().add_modifier(Modifier::BOLD));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn hit(tier: TimingTier, gain: f64) -> MogHit {
        MogHit { at: Instant::now(), tier, bank: 100.0, gain }
    }

    fn painted(h: &MogHit, age: u64) -> usize {
        let area = Rect::new(0, 0, 60, 8);
        let mut buf = Buffer::empty(area);
        render(&mut buf, area, h, age);
        buf.content().iter().filter(|c| c.symbol() != " ").count()
    }

    #[test]
    fn better_tiers_and_prizes_paint_more_and_the_burst_ends() {
        assert!(painted(&hit(TimingTier::Perfect, 0.0), 300) > painted(&hit(TimingTier::Bad, 0.0), 300));
        let (big, small) = (hit(TimingTier::Good, 100.0), hit(TimingTier::Good, 0.0));
        assert!(duration_ms(&big) > duration_ms(&small));
        assert!(painted(&big, duration_ms(&big) * 3 / 10) > painted(&small, duration_ms(&small) * 3 / 10));
        for tier in TimingTier::ALL {
            let h = hit(tier, 50.0);
            assert!(painted(&h, 100) > 0);
            assert_eq!(painted(&h, duration_ms(&h)), 0);
        }
    }

    #[test]
    fn shake_moves_the_frame_and_stops() {
        let area = Rect::new(0, 0, 20, 5);
        let h = hit(TimingTier::Perfect, 100.0);
        let moved = (0..40).any(|i| {
            let mut buf = Buffer::empty(area);
            buf.set_string(10, 2, "X", Style::new());
            shake(&mut buf, h.intensity(), i * 5);
            buf.cell((10, 2)).unwrap().symbol() != "X"
        });
        assert!(moved);
        let mut buf = Buffer::empty(area);
        buf.set_string(10, 2, "X", Style::new());
        shake(&mut buf, h.intensity(), 5000);
        assert_eq!(buf.cell((10, 2)).unwrap().symbol(), "X");
    }

    #[test]
    fn impact_paints_then_ends_and_bigger_losses_last_longer() {
        let at = Instant::now();
        let (small, big) = (HitTaken { at, bank: 100.0, loss: 1.0 }, HitTaken { at, bank: 100.0, loss: 100.0 });
        assert!(hit_duration_ms(&big) > hit_duration_ms(&small));
        for h in [small, big] {
            let area = Rect::new(0, 0, 60, 8);
            let (mut a, mut b, mut c) = (Buffer::empty(area), Buffer::empty(area), Buffer::empty(area));
            impact(&mut a, area, &h, 100);
            vignette(&mut b, area, &h, 100);
            impact(&mut c, area, &h, hit_duration_ms(&h));
            vignette(&mut c, area, &h, hit_duration_ms(&h));
            assert!(a.content().iter().any(|x| x.symbol() != " "));
            assert!(b.content().iter().any(|x| x.symbol() != " "));
            assert!(c.content().iter().all(|x| x.symbol() == " "));
        }
    }
}
