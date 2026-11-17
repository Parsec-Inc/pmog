//! Full-screen animations for consumables going off at the moment of impact, sent by the server
//! to whoever is involved: the shield flashes and shatters for the defender while the attacker's
//! mog bounces, the angle flares, the mirror reflects, the combo bursts into 6s and 7s...

use std::f64::consts::TAU;

use ratatui::prelude::*;

use super::consume::{Cell, draw, noise};
use crate::protocol::Fx;

pub fn duration_ms(fx: Fx) -> u64 {
    match fx {
        Fx::ShieldBlocked => 900,
        Fx::ShieldBounced => 800,
        Fx::AngleStrike => 800,
        Fx::MirrorHeld => 1000,
        Fx::MirrorCracked => 900,
        Fx::TapeSaved => 900,
        Fx::ComboHit => 1100,
        Fx::CologneHit => 1200,
        Fx::RushPress => 700,
        Fx::IceBounced => 1000,
    }
}

pub fn shake_power(fx: Fx) -> f64 {
    match fx {
        Fx::ComboHit => 1.8,
        Fx::ShieldBlocked => 1.4,
        Fx::AngleStrike => 1.2,
        Fx::ShieldBounced => 1.0,
        Fx::MirrorCracked | Fx::RushPress => 0.8,
        Fx::MirrorHeld => 0.6,
        Fx::IceBounced => 0.5,
        Fx::TapeSaved => 0.3,
        Fx::CologneHit => 0.0,
    }
}

fn style(fx: Fx) -> ((u8, u8, u8), &'static str) {
    match fx {
        Fx::ShieldBlocked => ((80, 200, 255), "BLOQUEADO!"),
        Fx::ShieldBounced => ((80, 200, 255), "BLOQUEARAM SEU MOG!"),
        Fx::AngleStrike => ((255, 240, 150), "PODER x2!"),
        Fx::MirrorHeld => ((160, 210, 255), "REFLETIDO!"),
        Fx::MirrorCracked => ((190, 190, 205), "O ESPELHO RACHOU"),
        Fx::TapeSaved => ((120, 230, 230), "A FITA SALVOU SEU MP E SEU STREAK"),
        Fx::ComboHit => ((255, 90, 90), "COMBO 6-7 +67%"),
        Fx::CologneHit => ((120, 255, 150), "FEROMÔNIO +50% DE MP"),
        Fx::RushPress => ((230, 70, 230), "COOLDOWN ÷2"),
        Fx::IceBounced => ((170, 230, 255), "IMUNE: GELO"),
    }
}

/// Is the cell on one of `n` rays from the screen center, within `len` cells and `wid` wide?
fn on_ray(c: &Cell, i: u64, n: u64, len: f64, wid: f64) -> bool {
    let a = i as f64 * TAU / n as f64;
    let (dx, dy) = (c.x - c.w / 2.0, c.y - c.h);
    let along = dx * a.cos() + dy * a.sin();
    along > 0.0 && along < len && (-dx * a.sin() + dy * a.cos()).abs() < wid
}

/// Distance from the cell to the segment (ax,ay)-(bx,by), in cell units (y in half rows).
fn seg_dist(c: &Cell, (ax, ay): (f64, f64), (bx, by): (f64, f64)) -> f64 {
    let (vx, vy) = (bx - ax, by - ay);
    let k = (((c.x - ax) * vx + (c.y - ay) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    (c.x - (ax + k * vx)).hypot(c.y - (ay + k * vy))
}

fn paint(fx: Fx, c: &Cell) -> Option<(&'static str, f64)> {
    let (cx, cy) = (c.w / 2.0, c.h);
    let (dx, dy) = (c.x - cx, c.y - cy);
    let d = dx.hypot(dy);
    let ang = dy.atan2(dx);
    let t = c.t;
    let max_r = cx.hypot(cy) + 4.0;
    let (col, row) = (c.col as u64, c.row as u64);
    let fade = 1.0 - t;
    match fx {
        // The dome takes the hit: flash, shockwave outward, then it shatters piece by piece.
        Fx::ShieldBlocked => {
            let r = c.h * 0.85;
            let front = r + t * (max_r - r);
            if (d - r).abs() < 1.4 && noise(col, row, 5) > t * 1.2 - 0.2 {
                Some(("▓", (1.0 - t * 0.7) * (0.6 + 0.4 * (ang * 3.0).sin())))
            } else if t > 0.05 && (d - front).abs() < 1.5 {
                Some(("░", fade))
            } else {
                None
            }
        }
        // The attacker's wave collapses back on itself and sparks scatter.
        Fx::ShieldBounced => {
            let front = (1.0 - t * 1.2).clamp(0.0, 1.0) * max_r + 1.5 * (ang * 5.0 - c.age as f64 / 80.0).sin();
            if (d - front).abs() < 1.5 && t < 0.85 {
                return Some(("▒", (1.0 - t).max(0.4)));
            }
            let reach = t * max_r * 0.8;
            (0..12).any(|i| on_ray(c, i, 12, reach, 0.7) && d > reach - 2.0).then_some(("✧", fade))
        }
        // Anamorphic flare: a streak across the middle plus eight growing spokes.
        Fx::AngleStrike => {
            let mid = (c.row as f64 + 0.5 - c.h / 2.0).abs() < 0.6;
            if mid && dx.abs() < t * 3.0 * cx {
                return Some(("━", fade));
            }
            (0..8).any(|i| on_ray(c, i, 8, t * max_r, 0.6)).then_some((if (col + row) % 2 == 0 { "╲" } else { "╱" }, fade))
        }
        // A beam comes in from the top left, hits the glass and bounces to the top right.
        Fx::MirrorHeld | Fx::MirrorCracked => {
            let cracked = fx == Fx::MirrorCracked;
            if cracked {
                // Cracks: jagged spokes with gaps, bits falling away.
                let jag = (0..8).any(|i| on_ray(c, i, 8, t * max_r * 0.7, 0.6 + 0.6 * noise(row, i, 1)) && noise(col, row, 2) > 0.25);
                return jag.then(|| (if (col + row) % 2 == 0 { "╲" } else { "╱" }, fade.max(0.4)));
            }
            let reveal = (t * 2.2).min(2.0);
            let (a, m, b) = ((0.0, 0.0), (cx, cy), (c.w, 0.0));
            let in_beam = (reveal > 0.0 && c.x / cx.max(1.0) <= reveal && seg_dist(c, a, m) < 0.9)
                || (reveal > 1.0 && (c.x - cx) / cx.max(1.0) <= reveal - 1.0 && seg_dist(c, m, b) < 0.9);
            if in_beam {
                Some(("▓", if t > 0.7 { 1.0 - (t - 0.7) / 0.3 } else { 1.0 }))
            } else if d < 3.0 * (1.0 - t) + 1.0 && t > 0.2 {
                Some(("✧", fade))
            } else {
                None
            }
        }
        // Tape slaps shut from both sides and meets in the middle.
        Fx::TapeSaved => {
            let mid = (c.h / 2.0) as i64;
            let reach = (t * 2.0).min(1.0) * cx;
            (0..3i64).find_map(|i| {
                let off = (c.row as i64 - (mid + (i - 1) * 3)).abs();
                let covered = c.x < reach || c.x > c.w - reach;
                (off <= 1 && covered).then(|| (if off == 0 { "▓" } else { "░" }, if t > 0.7 { (1.0 - t) / 0.3 } else { 1.0 }))
            })
        }
        // 6s and 7s blow out of the center.
        Fx::ComboHit => (0..70u64)
            .any(|i| {
                let a = i as f64 * 2.399;
                let r = (1.0 - fade * fade) * (0.3 + noise(i, 1, 0)) * max_r * 0.8;
                let (px, py) = (cx + r * a.cos(), cy + r * a.sin());
                col as i64 == px.floor() as i64 && row as i64 == (py / 2.0).floor() as i64
            })
            .then(|| (if (col + row) % 2 == 0 { "6" } else { "7" }, fade.max(0.4))),
        // Perfume bubbles puff outward over a soft green glow.
        Fx::CologneHit => {
            let hit = (0..60u64).any(|i| {
                let a = i as f64 * 2.399;
                let r = (1.0 - fade * fade) * (0.2 + noise(i, 2, 0)) * max_r * 0.8;
                col as i64 == (cx + r * a.cos()).round() as i64 && row as i64 == ((cy + r * a.sin()) / 2.0).round() as i64
            });
            if hit {
                Some((["·", "∘", "○"][(col % 3) as usize], fade.max(0.5)))
            } else {
                let glow = (1.0 - d / (t * max_r + 1.0)).max(0.0) * 0.4 * fade;
                (glow > 0.1).then_some(("░", glow))
            }
        }
        // Speed lines streak across the screen.
        Fx::RushPress => {
            let head = (t * 2.2 - noise(row, 2, 0) * 0.5) * c.w;
            (noise(row, 1, 0) < 0.4 && c.x < head && c.x > head - c.w * 0.3).then(|| ("━", fade.max(0.5)))
        }
        // A frost crystal: six spokes tipped with stars inside an expanding ring.
        Fx::IceBounced => {
            let len = t * max_r * 0.7;
            if (d - len).abs() < 1.2 {
                Some(("▒", fade))
            } else if (0..6).any(|i| on_ray(c, i, 6, len, 0.7)) {
                Some((if d > len - 2.0 { "✶" } else { "·" }, fade.max(0.5)))
            } else {
                None
            }
        }
    }
}

pub fn render(buf: &mut Buffer, area: Rect, fx: Fx, age_ms: u64) {
    let (rgb, label) = style(fx);
    draw(buf, area, duration_ms(fx), age_ms, rgb, label, |c| paint(fx, c));
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Fx; 10] = [
        Fx::ShieldBlocked,
        Fx::ShieldBounced,
        Fx::AngleStrike,
        Fx::MirrorHeld,
        Fx::MirrorCracked,
        Fx::TapeSaved,
        Fx::ComboHit,
        Fx::CologneHit,
        Fx::RushPress,
        Fx::IceBounced,
    ];

    #[test]
    fn every_fx_animates_and_ends() {
        let area = Rect::new(0, 0, 100, 30);
        for fx in ALL {
            let painted = |age| {
                let mut buf = Buffer::empty(area);
                render(&mut buf, area, fx, age);
                buf.content().iter().filter(|c| c.symbol() != " ").count()
            };
            let dur = duration_ms(fx);
            assert!((1..10).any(|i| painted(dur * i / 10) > 20), "{fx:?} paints nothing");
            assert_eq!(painted(dur), 0, "{fx:?} never ends");
        }
    }
}
