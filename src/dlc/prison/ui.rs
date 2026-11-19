//! How the Cadeia looks: name tags, the prison track, help, consumable animations and sprites.

use ratatui::prelude::*;

use super::{Profile, Public, PrisonTitle};
use crate::utils::nbsp;
use crate::constants::prison::{PILL_SLEEP, SOAP_DEF_CUT};
use crate::dlc::{Paint, SpriteAsset, UseFx};
use crate::game::Game;
use crate::items::ItemId;
use crate::sprites::sprite_asset;
use crate::ui::burst::hash;
use crate::ui::consume::{Cell, noise};
use crate::ui::fg;

pub const SOAP_RGB: (u8, u8, u8) = (250, 160, 200);
pub const PILL_RGB: (u8, u8, u8) = (245, 130, 35);
pub const BLACK_RGB: (u8, u8, u8) = (150, 60, 210);
const SOAP_PINK: Color = Color::Rgb(SOAP_RGB.0, SOAP_RGB.1, SOAP_RGB.2);

fn title_color(title: PrisonTitle) -> Color {
    match title {
        PrisonTitle::X9 => Color::Gray,
        PrisonTitle::Frangolino => Color::Rgb(230, 210, 160),
        PrisonTitle::SixSeven => Color::Rgb(120, 255, 120),
        PrisonTitle::Cenoura => Color::Rgb(PILL_RGB.0, PILL_RGB.1, PILL_RGB.2),
        PrisonTitle::Pera => Color::Rgb(190, 220, 90),
        PrisonTitle::Mil => Color::Rgb(185, 230, 245),
        PrisonTitle::Zero => Color::Rgb(255, 215, 90),
    }
}

/// Prison title after a name, plus the number on the forehead while it is fresh.
pub fn name_tags(p: &Public, fresh: bool) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if let Some(title) = p.title() {
        spans.push(Span::styled(nbsp(&format!("  «{}»", title.label())), fg(title_color(title))));
    }
    if fresh {
        spans.push(Span::styled(format!("  testa\u{a0}{}", p.forehead), fg(SOAP_PINK).bold()));
    }
    spans
}

pub fn stats_line(p: &Profile) -> Line<'static> {
    let title = p.title().map_or("nenhum", |t| t.label());
    let parts = [format!("Cadeia: {} na testa", p.forehead), format!("Numerinhos marcados {}", p.marks), format!("Título {title}")];
    Line::styled(parts.map(|part| nbsp(&part)).join("   "), fg(SOAP_PINK))
}

/// The Cadeia track under the ranks: each prison title, what it takes and who on the network holds it.
pub fn track_lines(g: &Game) -> Vec<Line<'static>> {
    let p = &g.player.dlc.prison;
    let mine = p.title();
    let mut lines = vec![
        Line::raw(""),
        Line::styled("── TRILHA DA CADEIA ──", fg(SOAP_PINK).bold()),
        Line::styled(
            format!("Você: {} na testa · {} numerinhos marcados. Mog PERFECT (ou com Sabonete) faz o alvo pegar o sabonete.", p.forehead, p.marks),
            fg(Color::Gray),
        ),
    ];
    for title in PrisonTitle::ALL {
        let marker = if mine == Some(title) { "▶ " } else if mine.is_some_and(|m| m > title) { "✓ " } else { "  " };
        let first = vec![
            Span::styled(marker, fg(Color::Magenta).bold()),
            Span::styled(title.label(), fg(title_color(title)).bold()),
            Span::styled(format!("  · {}", title.requirement()), fg(Color::Cyan)),
        ];
        let here: Vec<&str> = g
            .peers
            .iter()
            .filter(|peer| !peer.is_npc() && peer.dlc.prison.title() == Some(title))
            .map(|peer| peer.name.as_str())
            .collect();
        let mut details = vec![title.flavor().to_string()];
        if !here.is_empty() {
            details.push(format!("na rede: {}", here.join(", ")));
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(first));
        lines.push(Line::styled(format!("    {}", details.join(" · ")), fg(Color::Gray)));
    }
    lines
}

pub fn help() -> Vec<String> {
    vec![
        "Baseada na novelinha \"O sabonete\": quem pega o sabonete ganha um número na testa.".into(),
        "Com a DLC, todo mog PERFECT que acertar faz o alvo pegar o sabonete. O Sabonete faz isso em qualquer acerto, mesmo sem a DLC no alvo.".into(),
        format!("Quem pega fica com defesa -{:.0}% enquanto o número está fresco, e o servidor inteiro fica sabendo.", SOAP_DEF_CUT * 100.0),
        format!("Comprimido: o próximo alvo dorme, +{} s de cooldown. Poção Negra: o próximo alvo não consegue te mogar por um tempo.", PILL_SLEEP.as_secs()),
        "Os títulos da cela (do X9 ao Zero) sobem com os numerinhos que você marca; veja a trilha na tela de Ranks.".into(),
    ]
}

pub fn use_fx(item: ItemId) -> Option<UseFx> {
    let (color, duration_ms, shake, paint): (_, _, _, Paint) = match item {
        ItemId::Soap => (SOAP_RGB, 1400, 0.8, soap),
        ItemId::GreenPotion => ((90, 230, 100), 1200, 1.5, green_potion),
        ItemId::RedPotion => ((230, 60, 70), 1500, 0.7, red_potion),
        ItemId::BlackPotion => (BLACK_RGB, 1600, 1.0, black_potion),
        ItemId::CarrotPill => (PILL_RGB, 1600, 0.2, carrot_pill),
        _ => return None,
    };
    Some(UseFx { color, duration_ms, shake, paint })
}

pub fn sprite(item: ItemId) -> Option<SpriteAsset> {
    Some(match item {
        ItemId::PrisonDlc => sprite_asset!("dlc/prison/prison_dlc"),
        ItemId::PullUpBar => sprite_asset!("dlc/prison/pull_up_bar"),
        ItemId::Crutch => sprite_asset!("dlc/prison/crutch"),
        ItemId::CarrotHook => sprite_asset!("dlc/prison/carrot_hook"),
        ItemId::MilEar => sprite_asset!("dlc/prison/mil_ear"),
        ItemId::Folder77 => sprite_asset!("dlc/prison/folder_77"),
        ItemId::PeraUniform => sprite_asset!("dlc/prison/pera_uniform"),
        ItemId::TowerKey => sprite_asset!("dlc/prison/tower_key"),
        ItemId::GreenPotion => sprite_asset!("dlc/prison/green_potion"),
        ItemId::RedPotion => sprite_asset!("dlc/prison/red_potion"),
        ItemId::BlackPotion => sprite_asset!("dlc/prison/black_potion"),
        ItemId::Soap => sprite_asset!("dlc/prison/soap"),
        ItemId::CarrotPill => sprite_asset!("dlc/prison/carrot_pill"),
        _ => return None,
    })
}

/// Fades out over the last part of the animation, from `from` (0..1) on.
fn alpha(t: f64, from: f64) -> f64 {
    if t > from { 1.0 - (t - from) / (1.0 - from) } else { 1.0 }
}

/// The soap drops in the middle and foam bubbles float up around it.
fn soap(c: &Cell) -> Option<(&'static str, f64)> {
    let (t, col, row) = (c.t, c.col as u64, c.row as u64);
    let drop = (t * 2.2).min(1.0) * (c.h - 2.0);
    if row as i64 == drop.round() as i64 && (c.x - c.w / 2.0).abs() < 3.0 {
        return Some(("▀", alpha(t, 0.7)));
    }
    let hit = (0..40u64).any(|i| {
        let rise = ((t - noise(i, 4, 1) * 0.5).max(0.0) * (0.7 + noise(i, 3, 1) * 0.6) * 1.8) * c.h;
        let y = c.h - rise;
        let x = noise(i, 2, 1) * c.w + 1.5 * (y * 0.5 + i as f64).cos();
        col as i64 == x.round() as i64 && row as i64 == y.round() as i64
    });
    hit.then(|| (["o", "°", "O"][(col % 3) as usize], (1.0 - t) * 0.9))
}

/// Green fizz climbs from the bottom, popping bubbles above it.
fn green_potion(c: &Cell) -> Option<(&'static str, f64)> {
    let (t, col, row) = (c.t, c.col as u64, c.row as u64);
    let level = (t * 3.0).min(1.0) * c.h * 0.5;
    let up = c.h - c.row as f64;
    if up < level * (0.7 + 0.3 * (col as f64 * 0.7 + c.age as f64 / 90.0).sin()) {
        Some((if hash(col, row, c.age / 80) < 4 { "∘" } else { "░" }, 0.7 * alpha(t, 0.6)))
    } else {
        (hash(col, row, c.age / 60) < 3 && up < level * 1.8).then(|| ("°", 0.9 * alpha(t, 0.6)))
    }
}

/// Red pulses roll out from the middle like a slow heartbeat.
fn red_potion(c: &Cell) -> Option<(&'static str, f64)> {
    let (cx, cy) = (c.w / 2.0, c.h);
    let d = (c.x - cx).hypot(c.y - cy);
    let max_r = cx.hypot(cy);
    let ring = (0..3)
        .map(|i| {
            let front = (c.t * 1.6 - i as f64 * 0.25).max(0.0) * max_r;
            (1.0 - (d - front).abs() / 1.8).max(0.0)
        })
        .fold(0.0, f64::max);
    (ring > 0.0).then(|| (if ring > 0.6 { "▓" } else { "░" }, ring * alpha(c.t, 0.6)))
}

/// Dark tendrils creep in from the edges.
fn black_potion(c: &Cell) -> Option<(&'static str, f64)> {
    let (dx, dy) = (c.x - c.w / 2.0, c.y - c.h);
    let e = (c.x.min(c.w - c.x) / (c.w / 2.0)).min(c.y.min(c.h * 2.0 - c.y) / c.h);
    let reach = (c.t * 2.0).min(1.0) * 0.55 + 0.12 * (dy.atan2(dx) * 7.0 + c.age as f64 / 200.0).sin();
    (e < reach).then(|| (if e < reach * 0.5 { "▓" } else { "▒" }, (1.0 - e / reach.max(0.01)).max(0.4) * alpha(c.t, 0.7)))
}

/// Sleepy Zs drift up and away.
fn carrot_pill(c: &Cell) -> Option<(&'static str, f64)> {
    let (col, row) = (c.col as i64, c.row as i64);
    (0..40u64).find_map(|i| {
        let k = ((c.t - noise(i, 5, 2) * 0.5) / 0.5).clamp(0.0, 1.0);
        if k <= 0.0 || k >= 1.0 {
            return None;
        }
        let x = c.w * (0.1 + 0.8 * noise(i, 6, 2)) + k * 8.0;
        let y = c.h * (0.6 + 0.35 * noise(i, 7, 2)) - k * c.h * 0.5;
        (col == x.round() as i64 && row == y.round() as i64).then_some((if i % 2 == 0 { "Z" } else { "z" }, 1.0 - k))
    })
}
