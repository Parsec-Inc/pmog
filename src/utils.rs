//! Small formatting helpers.

use std::collections::VecDeque;

use crate::constants::{APP_DIR, LEGACY_APP_DIR, MAX_NAME_LEN, RTT_SMOOTHING, RTT_WINDOW};

/// Formats a point amount with k/M/B suffixes.
/// Whole number with dots between thousands, like `6.767`.
pub fn format_count(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

pub fn format_mp(value: f64) -> String {
    let abs = value.abs();
    let text = if abs >= 1e9 {
        format!("{:.2}B", value / 1e9)
    } else if abs >= 1e6 {
        format!("{:.2}M", value / 1e6)
    } else if abs >= 1e3 {
        format!("{:.1}k", value / 1e3)
    } else {
        format!("{value:.0}")
    };
    text.replace('.', ",")
}

/// Strips control characters and caps the length of a player name.
/// Time left as "42 s" or "2:05".
pub fn format_wait(left: std::time::Duration) -> String {
    let secs = left.as_secs_f64().ceil() as u64;
    if secs < 60 { format!("{secs} s") } else { format!("{}:{:02}", secs / 60, secs % 60) }
}

pub fn clean_name(raw: &str) -> String {
    raw.chars().filter(|c| !c.is_control()).take(MAX_NAME_LEN).collect()
}

/// Replaces NaN/infinite values with zero and negatives with zero.
pub fn sane(value: f64) -> f64 {
    if value.is_finite() { value.max(0.0) } else { 0.0 }
}

/// Decimal with comma and no trailing zeros (pt-BR).
pub fn decimal(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').replace('.', ",")
}

/// This computer's address on the local network (no packet is sent).
pub fn lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((std::net::Ipv4Addr::new(10, 255, 255, 255), 1)).ok()?;
    socket.local_addr().ok().map(|a| a.ip()).filter(|ip| !ip.is_unspecified())
}

/// Running round-trip estimate: smoothed average plus the worst recent sample.
#[derive(Debug, Clone, Default)]
pub struct RttStats {
    avg: Option<f64>,
    recent: VecDeque<f64>,
}

impl RttStats {
    pub fn add(&mut self, ms: f64) {
        self.avg = Some(self.avg.map_or(ms, |a| a + (ms - a) * RTT_SMOOTHING));
        self.recent.push_back(ms);
        if self.recent.len() > RTT_WINDOW {
            self.recent.pop_front();
        }
    }

    pub fn avg(&self) -> f64 {
        self.avg.unwrap_or(0.0)
    }

    pub fn max(&self) -> f64 {
        self.recent.iter().copied().fold(0.0, f64::max)
    }
}

/// The game's data folder, moving the one from before the rename into place the first time.
pub fn data_dir() -> Option<std::path::PathBuf> {
    Some(data_dir_in(&dirs::data_dir()?))
}

/// The game's folder inside `base`, moving the pre-rename one into place when it is missing.
pub fn data_dir_in(base: &std::path::Path) -> std::path::PathBuf {
    let dir = base.join(APP_DIR);
    let legacy = base.join(LEGACY_APP_DIR);
    if !dir.exists() && legacy.is_dir() {
        let _ = std::fs::rename(&legacy, &dir);
    }
    dir
}
