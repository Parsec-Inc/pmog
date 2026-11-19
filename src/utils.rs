//! Small formatting helpers.

use std::collections::VecDeque;

use crate::constants::{APP_DIR, LEGACY_APP_DIR, MAX_NAME_LEN, RTT_SMOOTHING, RTT_WINDOW, VIRTUAL_BRIDGE_PREFIXES};

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
/// `text` with non-breaking spaces, so it wraps as one word.
pub fn nbsp(text: &str) -> String {
    text.replace(' ', "\u{a0}")
}

pub fn lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((std::net::Ipv4Addr::new(10, 255, 255, 255), 1)).ok()?;
    socket.local_addr().ok().map(|a| a.ip()).filter(|ip| !ip.is_unspecified())
}

/// IPv4 address and broadcast address of each network interface, without loopback and container bridges.
pub fn local_networks() -> Vec<(std::net::Ipv4Addr, std::net::Ipv4Addr)> {
    let interfaces = if_addrs::get_if_addrs().unwrap_or_default();
    let mut networks: Vec<_> = interfaces
        .into_iter()
        .filter(|i| !VIRTUAL_BRIDGE_PREFIXES.iter().any(|p| i.name.starts_with(p)))
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) if !v4.ip.is_loopback() => Some((v4.ip, directed_broadcast(v4.ip, v4.netmask))),
            _ => None,
        })
        .collect();
    networks.sort();
    networks.dedup();
    networks
}

/// Broadcast address of the network `ip` belongs to.
pub fn directed_broadcast(ip: std::net::Ipv4Addr, netmask: std::net::Ipv4Addr) -> std::net::Ipv4Addr {
    (u32::from(ip) | !u32::from(netmask)).into()
}

/// Server address typed by the player: IP or name, with the default port when it has none.
pub fn parse_server_addr(text: &str, default_port: u16) -> Option<std::net::SocketAddr> {
    use std::net::ToSocketAddrs;
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(addr) = text.parse() {
        return Some(addr);
    }
    if let Ok(ip) = text.trim_start_matches('[').trim_end_matches(']').parse::<std::net::IpAddr>() {
        return Some((ip, default_port).into());
    }
    let resolved = if text.contains(':') { text.to_socket_addrs() } else { (text, default_port).to_socket_addrs() };
    resolved.ok()?.next()
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

/// PowerShell on its own hidden console, so it can never hide or close the game's terminal window.
#[cfg(windows)]
pub fn hidden_powershell() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut command = std::process::Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive"]).creation_flags(crate::constants::WINDOWS_CREATE_NO_WINDOW);
    command
}
