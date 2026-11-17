//! Firewall verdicts from each platform's answers and the hint on a failed connection.

use std::io::ErrorKind;

use pmog::constants::FIREWALL_HINT;
use pmog::firewall::{connect_hint, firewalld_warning, macos_warning, missing_ports, ufw_warning, windows_warning};

#[test]
fn firewalld_lists_only_the_ports_still_closed() {
    assert!(missing_ports("ssh pmog dhcpv6-client", "").is_empty(), "the packaged service opens everything");
    assert!(missing_ports("ssh", "47777/udp 47790-47799/tcp 47800-47809/tcp").is_empty());
    assert!(missing_ports("", "47777/udp 47700-47900/tcp").is_empty(), "a wider range covers ours");
    assert_eq!(missing_ports("ssh", "47777/tcp 47790-47795/tcp"), ["47777/udp", "47790-47799/tcp", "47800-47809/tcp"]);
    assert_eq!(missing_ports("", "47777/udp 47790-47809/tcp"), Vec::<String>::new());
}

#[test]
fn firewalld_fix_uses_the_service_when_it_is_installed() {
    assert_eq!(firewalld_warning("pmog", "", true), None);
    let with_service = firewalld_warning("ssh", "", true).unwrap();
    assert!(with_service.sure);
    assert!(with_service.what.contains("47777/udp"), "{}", with_service.what);
    assert_eq!(with_service.fix, "sudo firewall-cmd --permanent --add-service=pmog && sudo firewall-cmd --reload");
    let by_port = firewalld_warning("ssh", "", false).unwrap();
    assert_eq!(
        by_port.fix,
        "sudo firewall-cmd --permanent --add-port=47777/udp --add-port=47790-47799/tcp --add-port=47800-47809/tcp && sudo firewall-cmd --reload"
    );
}

#[test]
fn an_enabled_ufw_is_only_a_maybe() {
    assert_eq!(ufw_warning("# comment\nENABLED=no\n", true), None);
    let warning = ufw_warning("ENABLED=yes\nLOGLEVEL=low\n", true).unwrap();
    assert!(!warning.sure, "rules need root to read");
    assert_eq!(warning.fix, "sudo ufw allow pmog");
    let by_port = ufw_warning("ENABLED=\"yes\"", false).unwrap();
    assert_eq!(by_port.fix, "sudo ufw allow 47777/udp && sudo ufw allow 47790:47799/tcp && sudo ufw allow 47800:47809/tcp");
}

#[test]
fn windows_warns_only_about_block_rules() {
    assert_eq!(windows_warning("0\r\n", "C:\\pmog.exe"), None, "without rules Windows asks by itself");
    assert_eq!(windows_warning("", "C:\\pmog.exe"), None);
    let warning = windows_warning("2\r\n", "C:\\Program Files\\pmog\\pmog.exe").unwrap();
    assert!(warning.sure);
    assert!(warning.fix.contains("-Program \"C:\\Program Files\\pmog\\pmog.exe\""), "{}", warning.fix);
    assert!(warning.fix.contains("Disable-NetFirewallRule"));
}

#[test]
fn macos_warns_about_block_all_and_a_blocked_app() {
    let exe = "/Applications/pmog";
    let on = "Firewall is enabled. (State = 1)";
    assert_eq!(macos_warning("Firewall is disabled. (State = 0)", "", "The application is blocked", exe), None);
    assert_eq!(macos_warning(on, "Block all DISABLED!", "The application /Applications/pmog is permitted", exe), None);
    let all = macos_warning(on, "Firewall has block all state set to enabled.", "", exe).unwrap();
    assert!(all.fix.ends_with("--setblockall off"), "{}", all.fix);
    let app = macos_warning(on, "", "The application /Applications/pmog is blocked from incoming connections", exe).unwrap();
    assert!(app.fix.ends_with("--unblockapp \"/Applications/pmog\""), "{}", app.fix);
}

#[test]
fn unreachable_servers_on_the_network_hint_at_a_firewall() {
    let lan = "192.168.0.20:47790".parse().unwrap();
    for kind in [ErrorKind::TimedOut, ErrorKind::ConnectionRefused, ErrorKind::HostUnreachable] {
        assert_eq!(connect_hint(lan, kind), Some(FIREWALL_HINT));
    }
    assert_eq!(connect_hint(lan, ErrorKind::InvalidData), None);
    assert_eq!(connect_hint("127.0.0.1:47790".parse().unwrap(), ErrorKind::ConnectionRefused), None, "this computer is never blocked");
}

#[test]
fn the_check_finishes_on_this_computer() {
    let started = std::time::Instant::now();
    if let Some(warning) = pmog::firewall::check() {
        assert!(!warning.what.is_empty() && !warning.fix.is_empty(), "{warning:?}");
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(30), "the check must not hang");
}

#[cfg(windows)]
#[test]
fn the_windows_block_query_answers_a_count() {
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", pmog::constants::WINDOWS_BLOCK_QUERY])
        .env("PMOG_EXE", std::env::current_exe().unwrap())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(text.trim().parse::<u32>().is_ok(), "{text:?}");
}

#[cfg(target_os = "macos")]
#[test]
fn socketfilterfw_answers_in_the_words_the_check_reads() {
    let ask = |args: &[&str]| {
        let output = std::process::Command::new(pmog::constants::MACOS_FIREWALL_PROGRAM).args(args).output().unwrap();
        String::from_utf8_lossy(&output.stdout).to_lowercase()
    };
    for args in [["--getglobalstate"].as_slice(), ["--getblockall"].as_slice()] {
        let text = ask(args);
        assert!(text.contains("enabled") || text.contains("disabled"), "{args:?}: {text:?}");
    }
    // This test binary is not in the firewall's list, so its answer must not read as blocked.
    let exe = std::env::current_exe().unwrap().display().to_string();
    let app = ask(&["--getappblocked", &exe]);
    assert_eq!(macos_warning("firewall is enabled", "block all is disabled", &app, &exe), None, "{app:?}");
}
