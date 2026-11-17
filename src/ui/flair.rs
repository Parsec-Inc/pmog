//! Player names styled by rank: higher ranks glow, shimmer and burn.

use std::sync::OnceLock;
use std::time::Instant;

use ratatui::prelude::*;

use crate::game::Rank;

fn epoch() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

/// Seconds on a clock shared by every animated name.
pub fn clock(now: Instant) -> f64 {
    now.saturating_duration_since(epoch()).as_secs_f64()
}

fn lerp(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    Color::Rgb(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// Hue in degrees to a saturated RGB color.
pub fn hue(degrees: f64) -> Color {
    let h = degrees.rem_euclid(360.0) / 60.0;
    let x = ((1.0 - (h % 2.0 - 1.0).abs()) * 255.0) as u8;
    let (r, g, b) = match h as u32 {
        0 => (255, x, 0),
        1 => (x, 255, 0),
        2 => (0, 255, x),
        3 => (0, x, 255),
        4 => (x, 0, 255),
        _ => (255, 0, x),
    };
    Color::Rgb(r, g, b)
}

/// Wave in [0, 1].
fn wave(x: f64) -> f64 {
    (x.sin() + 1.0) / 2.0
}

/// Cheap per-letter flicker, stable within one animation frame.
fn flicker(i: usize, frame: u64) -> u64 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ frame.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h % 3
}

/// Symbol shown before the name from Chad up.
pub fn crest(rank: Rank) -> Option<&'static str> {
    match rank {
        Rank::Chad => Some("✦ "),
        Rank::Gigachad => Some("✸ "),
        Rank::Ascended => Some("♛ "),
        Rank::Sigma => Some("Σ "),
        Rank::AuraLord => Some("◈ "),
        Rank::SixSeven => Some("67 "),
        Rank::Mythic => Some("✧ "),
        Rank::GreekGod => Some("Ω "),
        Rank::Cosmic => Some("✶ "),
        Rank::Transcendent => Some("∞ "),
        _ => None,
    }
}

/// The clan tag shown before a name, like `[GIGA]`.
pub fn tag_span(tag: &str) -> Span<'static> {
    Span::styled(format!("[{tag}] "), Style::new().fg(Color::Yellow).bold())
}

/// The name preceded by the clan tag, when there is one.
pub fn tagged_spans(tag: Option<&str>, name: &str, rank: Rank, now: Instant) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = tag.map(tag_span).into_iter().collect();
    spans.extend(name_spans(name, rank, now));
    spans
}

/// The name as styled spans for its rank at time `now`.
pub fn name_spans(name: &str, rank: Rank, now: Instant) -> Vec<Span<'static>> {
    let t = clock(now);
    let bold = Style::new().bold();
    let per_char = |color: &dyn Fn(usize) -> Color| -> Vec<Span<'static>> {
        name.chars().enumerate().map(|(i, c)| Span::styled(c.to_string(), bold.fg(color(i)))).collect()
    };
    let mut spans = match rank {
        Rank::Sub5 => vec![Span::styled(name.to_string(), Style::new().fg(Color::Gray))],
        Rank::Ltn => vec![Span::styled(name.to_string(), bold.fg(Color::White))],
        Rank::Mtn => vec![Span::styled(name.to_string(), bold.fg(Color::Rgb(110, 230, 120)))],
        Rank::Htn => {
            let color = lerp((0, 170, 255), (170, 245, 255), wave(t * 3.0));
            vec![Span::styled(name.to_string(), bold.fg(color))]
        }
        Rank::Chadlite => per_char(&|i| lerp((60, 110, 255), (200, 90, 255), wave(i as f64 * 0.6 - t * 4.0))),
        Rank::Chad => {
            let len = name.chars().count() as f64;
            let shine = (t * 9.0) % (len + 8.0) - 2.0;
            per_char(&|i| {
                let near = 1.0 - ((i as f64 - shine).abs() / 2.0).min(1.0);
                lerp((225, 170, 30), (255, 252, 220), near)
            })
        }
        Rank::Gigachad => {
            let frame = (t * 12.0) as u64;
            per_char(&|i| match flicker(i, frame) {
                0 => Color::Rgb(255, 60, 20),
                1 => Color::Rgb(255, 150, 0),
                _ => Color::Rgb(255, 225, 60),
            })
        }
        Rank::Ascended => per_char(&|i| hue(i as f64 * 28.0 - t * 200.0)),
        Rank::Sigma => per_char(&|i| lerp((140, 160, 185), (240, 248, 255), wave(i as f64 * 0.9 - t * 2.5))),
        Rank::AuraLord => per_char(&|i| lerp((120, 0, 200), (255, 130, 255), wave(t * 5.0 + i as f64 * 0.35))),
        Rank::SixSeven => {
            // The 6-7 hand wobble: letters swap colors in step.
            let frame = (t * 6.7) as usize;
            per_char(&|i| if (i + frame).is_multiple_of(2) { Color::Rgb(90, 255, 120) } else { Color::Rgb(255, 225, 50) })
        }
        Rank::Mythic => per_char(&|i| lerp((0, 200, 180), (255, 200, 60), wave(i as f64 * 0.5 - t * 3.0))),
        Rank::GreekGod => {
            let len = name.chars().count() as f64;
            let vein = (t * 7.0) % (len + 6.0) - 1.0;
            per_char(&|i| {
                let near = 1.0 - ((i as f64 - vein).abs() / 1.5).min(1.0);
                lerp((235, 232, 220), (255, 190, 30), near)
            })
        }
        Rank::Cosmic => {
            let frame = (t * 8.0) as u64;
            per_char(&|i| match flicker(i, frame) {
                0 => Color::Rgb(100, 60, 220),
                1 => Color::Rgb(40, 130, 255),
                _ => Color::Rgb(240, 240, 255),
            })
        }
        Rank::Transcendent => name
            .chars()
            .enumerate()
            .map(|(i, c)| {
                let h = i as f64 * 28.0 - t * 200.0;
                Span::styled(c.to_string(), bold.fg(hue(h + 180.0)).bg(hue(h)))
            })
            .collect(),
    };
    if let Some(symbol) = crest(rank) {
        let color = match rank {
            Rank::Ascended | Rank::Transcendent => hue(-t * 200.0),
            Rank::Sigma => Color::Rgb(200, 215, 235),
            Rank::AuraLord => Color::Rgb(200, 90, 255),
            Rank::SixSeven => Color::Rgb(90, 255, 120),
            Rank::Mythic => Color::Rgb(0, 210, 190),
            Rank::GreekGod => Color::Rgb(255, 200, 60),
            Rank::Cosmic => Color::Rgb(120, 110, 255),
            Rank::Gigachad => Color::Rgb(255, 120, 0),
            _ => Color::Rgb(255, 215, 80),
        };
        spans.insert(0, Span::styled(symbol, bold.fg(color)));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn colors(rank: Rank, now: Instant) -> Vec<Option<Color>> {
        name_spans("Mogger", rank, now).iter().map(|s| s.style.fg).collect()
    }

    #[test]
    fn low_ranks_are_static_and_high_ranks_animate() {
        let now = Instant::now();
        let later = now + Duration::from_millis(250);
        for rank in [Rank::Sub5, Rank::Ltn, Rank::Mtn] {
            assert_eq!(colors(rank, now), colors(rank, later), "{rank:?} should not move");
        }
        for rank in Rank::ALL.into_iter().filter(|r| *r >= Rank::Htn) {
            let changed = (1..20).any(|k| colors(rank, now) != colors(rank, now + Duration::from_millis(k * 70)));
            assert!(changed, "{rank:?} should animate");
        }
    }

    #[test]
    fn every_rank_looks_different_and_keeps_the_name() {
        let now = Instant::now();
        let looks: Vec<_> = Rank::ALL.iter().map(|r| colors(*r, now)).collect();
        for (i, a) in looks.iter().enumerate() {
            for b in &looks[i + 1..] {
                assert_ne!(a, b);
            }
        }
        for rank in Rank::ALL {
            let text: String = name_spans("Mogger", rank, now).iter().map(|s| s.content.to_string()).collect();
            assert!(text.ends_with("Mogger"));
        }
    }

    #[test]
    fn hue_wraps() {
        assert_eq!(hue(0.0), hue(360.0));
        assert_eq!(hue(-120.0), hue(240.0));
    }
}
