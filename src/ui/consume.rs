//! One full-screen animation per consumable, painted over blank cells so the UI stays readable:
//! each has its own color, motion and shake. A name banner types itself out over the top.

use ratatui::prelude::*;

use super::burst::hash;
use crate::items::{self, ItemId};

fn color(item: ItemId) -> (u8, u8, u8) {
    match item {
        ItemId::EnergyDrink => (255, 220, 40),
        ItemId::SigmaShield => (80, 200, 255),
        ItemId::AngleShot => (255, 240, 150),
        ItemId::PreWorkout => (230, 70, 230),
        ItemId::PocketMirror => (150, 200, 255),
        ItemId::JawTape => (120, 230, 230),
        ItemId::Cologne => (120, 255, 150),
        ItemId::IcePack => (170, 230, 255),
        ItemId::ComboSixSeven => (255, 90, 90),
        _ => (170, 120, 255), // Aura Farm
    }
}

pub fn duration_ms(item: ItemId) -> u64 {
    match item {
        ItemId::EnergyDrink => 900,
        ItemId::SigmaShield => 1100,
        ItemId::AngleShot => 800,
        ItemId::PreWorkout => 1300,
        ItemId::PocketMirror => 1000,
        ItemId::JawTape => 900,
        ItemId::Cologne => 1500,
        ItemId::IcePack => 1300,
        ItemId::ComboSixSeven => 1200,
        _ => 1800,
    }
}

/// Shake strength: jolting items rattle the screen, calm ones barely do.
pub fn shake_power(item: ItemId) -> f64 {
    match item {
        ItemId::ComboSixSeven => 1.8,
        ItemId::PreWorkout => 1.6,
        ItemId::EnergyDrink => 1.2,
        ItemId::AngleShot => 1.0,
        ItemId::SigmaShield => 0.6,
        ItemId::JawTape => 0.5,
        ItemId::AuraFarm => 0.4,
        ItemId::IcePack => 0.3,
        _ => 0.0,
    }
}

/// Where a cell is, in cell units (`y` counts half rows so distances are round).
pub(super) struct Cell {
    pub col: u16,
    pub row: u16,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub t: f64,
    pub age: u64,
}

pub(super) fn noise(a: u64, b: u64, c: u64) -> f64 {
    hash(a, b, c) as f64 / 97.0
}

/// Glyph and brightness (0..1) for one blank cell, or nothing.
fn paint(item: ItemId, c: &Cell) -> Option<(&'static str, f64)> {
    let (cx, cy) = (c.w / 2.0, c.h);
    let (dx, dy) = (c.x - cx, c.y - cy);
    let d = dx.hypot(dy);
    let t = c.t;
    let phase = c.age as f64;
    let alpha = |from: f64| if t > from { 1.0 - (t - from) / (1.0 - from) } else { 1.0 };
    let (col, row) = (c.col as u64, c.row as u64);
    match item {
        // Lightning bolts zigzag down from the top, flickering.
        ItemId::EnergyDrink => {
            let reach = (t * 2.5).min(1.0) * c.h;
            (0..7u64)
                .any(|i| {
                    let bx = (noise(i, 1, 1) * c.w) as i64 + (hash(row, i, c.age / 70) % 3) as i64 - 1;
                    col as i64 == bx && (c.row as f64) < reach && !(c.age / 50 + i).is_multiple_of(4)
                })
                .then(|| ("║", (1.0 - t).powf(0.8)))
        }
        // A dome snaps shut around the player, its rim shimmering around the circle.
        ItemId::SigmaShield => {
            let r = c.h * 0.85 * (1.0 - (1.0 - (t / 0.35).min(1.0)).powi(3));
            if (d - r).abs() < 1.4 {
                Some(("▓", (0.55 + 0.45 * (dy.atan2(dx) * 3.0 - phase / 90.0).sin()) * alpha(0.65)))
            } else if d < r && t > 0.2 && col % 4 == 0 && (row + col / 4) % 2 == 0 {
                Some(("·", 0.4 * alpha(0.65)))
            } else {
                None
            }
        }
        // Camera flash, then autofocus brackets close in.
        ItemId::AngleShot => {
            if t < 0.22 {
                return Some(("▓", 1.0 - t / 0.22));
            }
            let e = 1.0 - (1.0 - ((t - 0.1) / 0.5).clamp(0.0, 1.0)).powi(2);
            let (hw, hh) = (c.w * (0.42 - 0.26 * e), c.h * (0.42 - 0.24 * e));
            let (ax, ay) = (dx.abs(), (c.row as f64 + 0.5 - c.h / 2.0).abs());
            let k = alpha(0.7);
            if (ax - hw).abs() < 0.6 && ay <= hh && ay >= hh - 1.5 {
                Some(("┃", k))
            } else if (ay - hh).abs() < 0.6 && ax <= hw && ax >= hw - 4.0 {
                Some(("━", k))
            } else {
                None
            }
        }
        // Pump bars: an equalizer that rises from the bottom.
        ItemId::PreWorkout => {
            let frac = 0.25 + 0.75 * (col as f64 * 0.45 + phase / 110.0).sin().abs();
            let up = (c.h - c.row as f64) / (c.h * 0.8);
            (col % 2 == 0 && up <= frac * (t * 4.0).min(1.0)).then(|| ("█", (0.45 + 0.55 * (1.0 - up)) * alpha(0.7)))
        }
        // Light glints sweep diagonally across the glass.
        ItemId::PocketMirror => {
            let v = c.col as f64 + c.row as f64 * 2.0;
            let p = t * (c.w + c.h * 2.0) * 1.2;
            let band = |off: f64, width: f64| (1.0 - (v - (p - off)).abs() / width).max(0.0);
            let k = band(0.0, 3.0).max(band(12.0, 1.5)) * alpha(0.7);
            if k > 0.0 && hash(col, row, c.age / 100) == 0 {
                Some(("✧", 1.0))
            } else {
                (k > 0.0).then_some(("▒", k))
            }
        }
        // Three strips of tape wrap across the screen one after another.
        ItemId::JawTape => {
            let mid = (c.h / 2.0) as i64;
            (0..3i64).find_map(|i| {
                let off = (c.row as i64 - (mid + (i - 1) * 3)).abs();
                let front = (t * 1.5 - i as f64 * 0.12).clamp(0.0, 1.0) * c.w;
                (off <= 1 && c.x < front).then(|| (if off == 0 { "▓" } else { "░" }, 0.9 * alpha(0.75)))
            })
        }
        // Scented bubbles drift up, swaying, over a green haze at the bottom.
        ItemId::Cologne => {
            let hit = (0..36u64).any(|i| {
                let rise = ((t - noise(i, 4, 0) * 0.4).max(0.0) * (0.6 + noise(i, 3, 0) * 0.8) * 1.6) * c.h;
                let y = c.h - rise;
                let x = noise(i, 2, 0) * c.w + 2.0 * (y * 0.6 + i as f64).sin();
                col as i64 == x.round() as i64 && row as i64 == y.round() as i64
            });
            if hit {
                Some((["·", "∘", "○"][(col % 3) as usize], (1.0 - t) * 0.9))
            } else {
                let haze = (c.row as f64 / c.h).powi(3) * 0.35 * (1.0 - t);
                (haze > 0.1).then_some(("░", haze))
            }
        }
        // Frost creeps in from the edges, then melts.
        ItemId::IcePack => {
            let edge = c.col.min(c.w as u16 - 1 - c.col.min(c.w as u16 - 1)) as f64;
            let edge = edge.min((c.row as f64).min(c.h - 1.0 - c.row as f64) * 4.0);
            let band = c.w * 0.22 * (t * 2.5).min(1.0);
            if edge < band * (0.6 + 0.4 * noise(col, row, 3)) {
                let depth = 1.0 - edge / band;
                let g = if noise(col, row, 9) < 0.07 { "✶" } else if depth > 0.5 { "▒" } else { "░" };
                Some((g, (0.4 + 0.6 * depth) * alpha(0.6)))
            } else {
                None
            }
        }
        // 6 and 7 rain down like code.
        ItemId::ComboSixSeven => {
            if !hash(col, 5, 0).is_multiple_of(3) {
                return None;
            }
            let head = phase / 1000.0 * (14.0 + (hash(col, 6, 0) % 10) as f64) - (hash(col, 7, 0) % 12) as f64;
            let behind = head - c.row as f64;
            (0.0..8.0).contains(&behind).then(|| (if (col + row) % 2 == 0 { "6" } else { "7" }, (1.0 - behind / 8.0) * alpha(0.7)))
        }
        // Aura Farm: a slow lavender glow with two lazy waves.
        _ => {
            let r = 10.0 + 14.0 * (t * 2.0).min(1.0);
            let glow = (1.0 - d / r).max(0.0).powi(2) * (0.6 + 0.4 * (phase / 150.0 - d * 0.3).sin());
            let max_r = cx.hypot(cy) + 4.0;
            let ring = (0..2)
                .map(|i| {
                    let front = (t - i as f64 * 0.2).max(0.0) * max_r + 1.5 * (dy.atan2(dx) * 4.0 + phase / 100.0).sin();
                    (1.0 - (d - front).abs() / 2.5).max(0.0)
                })
                .fold(0.0, f64::max);
            let k = (glow * 0.8 + ring * 0.7) * alpha(0.6);
            (k > 0.0).then_some(("░", k)).map(|(g, k)| (if k > 0.5 { "▒" } else { g }, k))
        }
    }
}

pub fn render(buf: &mut Buffer, area: Rect, item: ItemId, age_ms: u64) {
    let def = items::def(item);
    let label = format!("{} · {}", def.name.to_uppercase(), def.usage.map_or("", |u| u.1));
    draw(buf, area, duration_ms(item), age_ms, color(item), &label, |c| paint(item, c));
}

/// Paints `paint`'s glyphs over the blank cells of `area` for `dur` ms, with a typed-out banner on top.
pub(super) fn draw(
    buf: &mut Buffer,
    area: Rect,
    dur: u64,
    age_ms: u64,
    (r, g, b): (u8, u8, u8),
    label: &str,
    paint: impl Fn(&Cell) -> Option<(&'static str, f64)>,
) {
    if age_ms >= dur || area.width < 2 || area.height < 2 {
        return;
    }
    let mut c = Cell { col: 0, row: 0, x: 0.0, y: 0.0, w: area.width as f64, h: area.height as f64, t: age_ms as f64 / dur as f64, age: age_ms };
    for row in 0..area.height {
        for col in 0..area.width {
            (c.col, c.row, c.x, c.y) = (col, row, col as f64 + 0.5, (row as f64 + 0.5) * 2.0);
            let Some((glyph, k)) = paint(&c).filter(|(_, k)| *k > 0.08) else { continue };
            let beside = super::burst::beside_text(buf, area.x + col, area.y + row);
            let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) else { continue };
            if cell.symbol() == " " && !beside {
                let k = k.clamp(0.35, 1.0);
                cell.set_symbol(glyph).set_fg(Color::Rgb((r as f64 * k) as u8, (g as f64 * k) as u8, (b as f64 * k) as u8));
            }
        }
    }

    // Name banner: types itself in, then vanishes with the animation.
    if c.t < 0.85 && !label.is_empty() {
        let text: Vec<char> = format!(" {label} ").chars().take(area.width as usize).collect();
        let shown = ((text.len() as f64 * (c.t / 0.25).min(1.0)) as usize).min(text.len());
        let x0 = area.x + (area.width as usize - text.len()) as u16 / 2;
        for (i, ch) in text.iter().take(shown).enumerate() {
            if let Some(cell) = buf.cell_mut((x0 + i as u16, area.y + area.height / 3)) {
                cell.set_char(*ch).set_style(Style::new().bg(Color::Rgb(r, g, b)).fg(Color::Black).add_modifier(Modifier::BOLD));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::CATALOG;

    #[test]
    fn every_consumable_animates_and_ends() {
        let area = Rect::new(0, 0, 100, 30);
        let consumables: Vec<_> = CATALOG.iter().filter(|d| d.consumable()).collect();
        assert_eq!(consumables.len(), 10);
        for def in consumables {
            let id = def.id;
            let painted = |age| {
                let mut buf = Buffer::empty(area);
                render(&mut buf, area, id, age);
                buf.content().iter().filter(|c| c.symbol() != " ").count()
            };
            let dur = duration_ms(id);
            assert!((1..10).any(|i| painted(dur * i / 10) > 20), "{:?} paints nothing", id);
            assert_eq!(painted(dur), 0, "{:?} never ends", id);
        }
    }
}
