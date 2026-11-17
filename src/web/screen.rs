//! Turns the drawn terminal buffer into rows of styled runs for the page.

use ratatui::buffer::{Buffer, Cell};
use ratatui::style::{Color, Modifier};
use serde_json::{Value, json};

/// Page colors used when a cell keeps the terminal default; the page uses the same two.
pub const DEFAULT_FG: &str = "#d4d4d4";
pub const DEFAULT_BG: &str = "#0c0c10";

const BOLD: u8 = 1;
const DIM: u8 = 2;
const ITALIC: u8 = 4;
const UNDERLINE: u8 = 8;
const CROSSED: u8 = 16;

/// The 16 named terminal colors, in ANSI order.
const ANSI: [&str; 16] = [
    "#000000", "#cd3131", "#0dbc79", "#e5e510", "#2472c8", "#bc3fbc", "#11a8cd", "#e5e5e5",
    "#666666", "#f14c4c", "#23d18b", "#f5f543", "#3b8eea", "#d670d6", "#29b8db", "#ffffff",
];

/// CSS color for a terminal color; `None` keeps the page default.
pub fn css(color: Color) -> Option<String> {
    let named = |i: usize| Some(ANSI[i].to_string());
    match color {
        Color::Reset => None,
        Color::Black => named(0),
        Color::Red => named(1),
        Color::Green => named(2),
        Color::Yellow => named(3),
        Color::Blue => named(4),
        Color::Magenta => named(5),
        Color::Cyan => named(6),
        Color::Gray => named(7),
        Color::DarkGray => named(8),
        Color::LightRed => named(9),
        Color::LightGreen => named(10),
        Color::LightYellow => named(11),
        Color::LightBlue => named(12),
        Color::LightMagenta => named(13),
        Color::LightCyan => named(14),
        Color::White => named(15),
        Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        Color::Indexed(i) => Some(indexed(i)),
    }
}

/// xterm 256-color palette entry.
fn indexed(i: u8) -> String {
    match i {
        0..16 => ANSI[i as usize].to_string(),
        16..232 => {
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            let n = i - 16;
            format!("#{:02x}{:02x}{:02x}", level(n / 36), level(n / 6 % 6), level(n % 6))
        }
        _ => {
            let v = 8 + (i - 232) * 10;
            format!("#{v:02x}{v:02x}{v:02x}")
        }
    }
}

fn flags(modifier: Modifier) -> u8 {
    [(Modifier::BOLD, BOLD), (Modifier::DIM, DIM), (Modifier::ITALIC, ITALIC), (Modifier::UNDERLINED, UNDERLINE), (Modifier::CROSSED_OUT, CROSSED)]
        .iter()
        .filter(|(m, _)| modifier.contains(*m))
        .fold(0, |acc, (_, bit)| acc | bit)
}

/// Foreground, background (`None` keeps the page default) and flag bits of a cell.
type CellStyle = (Option<String>, Option<String>, u8);

/// Colors and flags of one cell, with reversed video already applied.
fn style_of(cell: &Cell) -> CellStyle {
    let (fg, bg) = (css(cell.fg), css(cell.bg));
    let (fg, bg) = if cell.modifier.contains(Modifier::REVERSED) {
        (Some(bg.unwrap_or_else(|| DEFAULT_BG.into())), Some(fg.unwrap_or_else(|| DEFAULT_FG.into())))
    } else {
        (fg, bg)
    };
    (fg, bg, flags(cell.modifier))
}

/// One row as `[text, fg, bg, flags]` runs, one grapheme per cell.
pub fn row(buffer: &Buffer, y: u16) -> Value {
    let area = buffer.area;
    let mut runs: Vec<(String, CellStyle)> = Vec::new();
    for x in area.left()..area.right() {
        let cell = &buffer[(x, y)];
        let symbol = if cell.modifier.contains(Modifier::HIDDEN) || cell.symbol().is_empty() { " " } else { cell.symbol() };
        let style = style_of(cell);
        match runs.last_mut() {
            Some((text, last)) if *last == style => text.push_str(symbol),
            _ => runs.push((symbol.to_string(), style)),
        }
    }
    Value::Array(runs.into_iter().map(|(text, (fg, bg, f))| json!([text, fg, bg, f])).collect())
}
