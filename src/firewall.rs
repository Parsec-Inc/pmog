//! Finds a firewall that keeps other computers from reaching this PMOG and says how to open it.

use std::io::ErrorKind;
use std::net::SocketAddr;
use std::ops::RangeInclusive;
use std::process::{Command, Stdio};

use crate::constants::{
    DISCOVERY_PORT, FIREWALL_HINT, FIREWALL_SERVICE, SERVER_PORT_COUNT, SERVER_PORT_START, WEB_PORT_COUNT, WEB_PORT_START,
};

/// Something in the way, with the command that clears it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// False when the firewall is on but its rules cannot be read without admin rights.
    pub sure: bool,
    pub what: String,
    pub fix: String,
}

/// Ports PMOG listens on, as (range, protocol).
pub fn needed_ports() -> [(RangeInclusive<u16>, &'static str); 3] {
    [
        (DISCOVERY_PORT..=DISCOVERY_PORT, "udp"),
        (SERVER_PORT_START..=SERVER_PORT_START + SERVER_PORT_COUNT - 1, "tcp"),
        (WEB_PORT_START..=WEB_PORT_START + WEB_PORT_COUNT - 1, "tcp"),
    ]
}

fn port_label(range: &RangeInclusive<u16>, proto: &str, separator: char) -> String {
    match range.start() == range.end() {
        true => format!("{}/{proto}", range.start()),
        false => format!("{}{separator}{}/{proto}", range.start(), range.end()),
    }
}

/// Port ranges firewalld leaves closed, given its `--list-services` and `--list-ports` output.
pub fn missing_ports(services: &str, ports: &str) -> Vec<String> {
    if services.split_whitespace().any(|s| s == FIREWALL_SERVICE) {
        return Vec::new();
    }
    let open: Vec<(u16, u16, &str)> = ports
        .split_whitespace()
        .filter_map(|entry| {
            let (span, proto) = entry.split_once('/')?;
            let (low, high) = span.split_once('-').unwrap_or((span, span));
            Some((low.parse().ok()?, high.parse().ok()?, proto))
        })
        .collect();
    needed_ports()
        .into_iter()
        .filter(|(range, proto)| range.clone().any(|p| !open.iter().any(|(low, high, open_proto)| open_proto == proto && (*low..=*high).contains(&p))))
        .map(|(range, proto)| port_label(&range, proto, '-'))
        .collect()
}

/// Verdict for a running firewalld; the fix uses the packaged service when it is installed.
pub fn firewalld_warning(services: &str, ports: &str, service_installed: bool) -> Option<Warning> {
    let missing = missing_ports(services, ports);
    if missing.is_empty() {
        return None;
    }
    let opens = match service_installed {
        true => format!("--add-service={FIREWALL_SERVICE}"),
        false => needed_ports().iter().map(|(range, proto)| format!("--add-port={}", port_label(range, proto, '-'))).collect::<Vec<_>>().join(" "),
    };
    Some(Warning {
        sure: true,
        what: format!("O firewalld está bloqueando o PMOG (portas fechadas: {}).", missing.join(", ")),
        fix: format!("sudo firewall-cmd --permanent {opens} && sudo firewall-cmd --reload"),
    })
}

/// Verdict for ufw from its config file; its rules need root to read, so it can only say "maybe".
pub fn ufw_warning(conf: &str, profile_installed: bool) -> Option<Warning> {
    let enabled = conf.lines().any(|l| l.trim().replace(['"', '\''], "").eq_ignore_ascii_case("ENABLED=yes"));
    if !enabled {
        return None;
    }
    let fix = match profile_installed {
        true => format!("sudo ufw allow {FIREWALL_SERVICE}"),
        false => needed_ports().iter().map(|(range, proto)| format!("sudo ufw allow {}", port_label(range, proto, ':'))).collect::<Vec<_>>().join(" && "),
    };
    Some(Warning { sure: false, what: "O ufw está ligado e pode estar bloqueando o PMOG.".into(), fix })
}

/// Verdict for Windows from the number of block rules naming the exe.
pub fn windows_warning(block_rules: &str, exe: &str) -> Option<Warning> {
    let count: u32 = block_rules.trim().parse().ok()?;
    (count > 0).then(|| Warning {
        sure: true,
        what: "O Firewall do Windows tem uma regra bloqueando o PMOG.".into(),
        fix: format!(
            "No PowerShell como administrador: Get-NetFirewallApplicationFilter -Program \"{exe}\" | Get-NetFirewallRule | Where-Object Action -eq Block | Disable-NetFirewallRule"
        ),
    })
}

/// Verdict for macOS from `socketfilterfw` answers: global state, block-all mode and this app.
pub fn macos_warning(global: &str, block_all: &str, app: &str, exe: &str) -> Option<Warning> {
    let on = |text: &str| {
        let text = text.to_lowercase();
        text.contains("enabled") && !text.contains("disabled")
    };
    if !on(global) {
        return None;
    }
    let program = crate::constants::MACOS_FIREWALL_PROGRAM;
    if on(block_all) {
        return Some(Warning {
            sure: true,
            what: "O firewall do macOS está bloqueando todas as conexões de entrada.".into(),
            fix: format!("sudo {program} --setblockall off"),
        });
    }
    let app = app.to_lowercase();
    (app.contains("blocked") && !app.contains("not blocked") && !app.contains("permitted")).then(|| Warning {
        sure: true,
        what: "O firewall do macOS está bloqueando o PMOG.".into(),
        fix: format!("sudo {program} --unblockapp \"{exe}\""),
    })
}

/// A failed connection worth blaming on a firewall: the server was seen but its port did not answer.
pub fn connect_hint(addr: SocketAddr, kind: ErrorKind) -> Option<&'static str> {
    let blocked = matches!(kind, ErrorKind::TimedOut | ErrorKind::WouldBlock | ErrorKind::ConnectionRefused | ErrorKind::HostUnreachable | ErrorKind::NetworkUnreachable);
    (blocked && !addr.ip().is_loopback()).then_some(FIREWALL_HINT)
}

/// Standard output of a command that succeeded, never asking for input.
fn run(program: &str, args: &[&str], env: Option<(&str, &str)>) -> Option<String> {
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).stderr(Stdio::null());
    if let Some((key, value)) = env {
        command.env(key, value);
    }
    let output = command.output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(any(windows, target_os = "macos"))]
fn exe_path() -> Option<String> {
    Some(std::env::current_exe().ok()?.to_string_lossy().into_owned())
}

#[cfg(all(unix, not(target_os = "macos")))]
/// Looks at this computer's firewall; slow, so run it off the UI thread.
pub fn check() -> Option<Warning> {
    use crate::constants::{FIREWALLD_PROGRAM, FIREWALLD_SERVICE_DIRS, UFW_CONF, UFW_PROFILE};
    use std::path::Path;
    if run(FIREWALLD_PROGRAM, &["--state"], None).is_some_and(|s| s.trim() == "running") {
        let services = run(FIREWALLD_PROGRAM, &["--list-services"], None)?;
        let ports = run(FIREWALLD_PROGRAM, &["--list-ports"], None)?;
        let installed = FIREWALLD_SERVICE_DIRS.iter().any(|dir| Path::new(dir).join(format!("{FIREWALL_SERVICE}.xml")).exists());
        return firewalld_warning(&services, &ports, installed);
    }
    ufw_warning(&std::fs::read_to_string(UFW_CONF).ok()?, Path::new(UFW_PROFILE).exists())
}

#[cfg(windows)]
/// Looks at this computer's firewall; slow, so run it off the UI thread.
pub fn check() -> Option<Warning> {
    let exe = exe_path()?;
    let query = crate::constants::WINDOWS_BLOCK_QUERY;
    let count = run("powershell", &["-NoProfile", "-NonInteractive", "-Command", query], Some(("PMOG_EXE", &exe)))?;
    windows_warning(&count, &exe)
}

#[cfg(target_os = "macos")]
/// Looks at this computer's firewall; slow, so run it off the UI thread.
pub fn check() -> Option<Warning> {
    let program = crate::constants::MACOS_FIREWALL_PROGRAM;
    let exe = exe_path()?;
    let global = run(program, &["--getglobalstate"], None)?;
    let block_all = run(program, &["--getblockall"], None).unwrap_or_default();
    let app = run(program, &["--getappblocked", &exe], None).unwrap_or_default();
    macos_warning(&global, &block_all, &app, &exe)
}
