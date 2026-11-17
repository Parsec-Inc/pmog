//! Self-update: only builds signed by the release key are accepted, whatever a host sends.

mod common;

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use common::*;
use ed25519_dalek::SigningKey;
use pmog::app::{App, Confirm, UpdateSource};
use pmog::discovery::Discovery;
use pmog::input::handle_key;
use pmog::protocol::Beacon;
use pmog::server::ServerHandle;
use pmog::constants::GITHUB_LATEST_URL;
use pmog::update::{Manifest, Shelf, UpdateReply, dist_name, fetch, fetch_web, hex_encode, install, release_label, sidecar, web_offer, windows_swap_script};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn public(key: &SigningKey) -> String {
    hex_encode(key.verifying_key().as_bytes())
}

/// A scratch folder removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let path = std::env::temp_dir().join(format!("pmog-update-{tag}-{}-{}", std::process::id(), rand::random::<u32>()));
        fs::create_dir_all(&path).unwrap();
        Dir(path)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Puts a signed build named `name` in `dir`.
fn publish(dir: &Dir, name: &str, signer: &SigningKey, platform: &str, release: u64, bytes: &[u8]) -> PathBuf {
    let path = dir.file(name);
    fs::write(&path, bytes).unwrap();
    let manifest = Manifest::sign(signer, platform, release, bytes);
    fs::write(sidecar(&path), serde_json::to_vec(&manifest).unwrap()).unwrap();
    path
}

#[test]
fn signature_binds_bytes_platform_and_release() {
    let k = key(1);
    let bytes = b"build nova".to_vec();
    let m = Manifest::sign(&k, "linux", 5, &bytes);
    assert_eq!(m.verify(&public(&k), &bytes), Ok(()));

    let mut tampered = bytes.clone();
    tampered[0] ^= 1;
    assert!(m.verify(&public(&k), &tampered).is_err(), "changed bytes");
    assert!(m.verify(&public(&key(2)), &bytes).is_err(), "someone else's key");
    assert!(Manifest { platform: "windows".into(), ..m.clone() }.verify(&public(&k), &bytes).is_err(), "platform swapped");
    assert!(Manifest { release: 99, ..m.clone() }.verify(&public(&k), &bytes).is_err(), "release bumped");
    assert!(Manifest { signature: "00".repeat(64), ..m.clone() }.verify(&public(&k), &bytes).is_err(), "forged signature");
    assert!(Manifest { signature: "zz".into(), ..m.clone() }.verify(&public(&k), &bytes).is_err(), "garbage signature");
    assert!(m.verify("", &bytes).is_err(), "no key, no update");
    // A malicious host can rewrite the manifest to match its payload, but can't sign it.
    let evil = b"malware".to_vec();
    let relabeled = Manifest { size: evil.len() as u64, sha256: Manifest::sign(&key(9), "linux", 5, &evil).sha256, ..m.clone() };
    assert!(relabeled.verify(&public(&k), &evil).is_err());
}

#[test]
fn shelf_only_keeps_builds_signed_by_the_release_key() {
    let dir = Dir::new("shelf");
    let k = key(1);
    publish(&dir, "pmog-linux", &k, "linux", 10, b"linux 10");
    publish(&dir, "pmog-linux-velho", &k, "linux", 7, b"linux 7");
    publish(&dir, "pmog-windows.exe", &key(2), "windows", 50, b"windows de outra chave");
    fs::write(dir.file("solto"), b"sem assinatura").unwrap();
    let tampered = publish(&dir, "adulterado", &k, "linux", 99, b"original");
    fs::write(&tampered, b"trocado depois de assinar").unwrap();
    fs::write(dir.file("lixo.sig"), b"{nada").unwrap();

    let shelf = Shelf::scan(&dir.0, &public(&k));
    assert_eq!(shelf.offers(), vec![("linux".to_string(), 10)]);
    assert!(Shelf::scan(&dir.0, "").offers().is_empty(), "no key, nothing offered");
}

#[test]
fn update_from_a_real_host_installs_and_can_be_passed_on() {
    let dir = Dir::new("host");
    let k = key(1);
    let new_build = vec![7u8; 300_000];
    publish(&dir, "pmog-linux", &k, "linux", 20, &new_build);
    let server = ServerHandle::start_with("Host", None, ports(31), Shelf::scan(&dir.0, &public(&k))).unwrap();

    let player = Dir::new("player");
    let exe = player.file("pmog");
    fs::write(&exe, b"build velha").unwrap();
    let (manifest, bytes) = fetch(server.local_addr(), "linux", 10, &public(&k)).unwrap();
    assert_eq!((manifest.release, bytes.len()), (20, new_build.len()));
    install(&exe, &manifest, &bytes).unwrap();
    assert_eq!(fs::read(&exe).unwrap(), new_build);
    assert!(!player.file("pmog.new").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&exe).unwrap().permissions().mode() & 0o777, 0o755);
    }
    // The updated player can host and hand the same build on.
    assert_eq!(Shelf::scan(&player.0, &public(&k)).offers(), vec![("linux".to_string(), 20)]);

    let none = fetch(server.local_addr(), "linux", 20, &public(&k)).unwrap_err();
    assert!(none.contains("não tem uma versão mais nova"), "{none}");
    let other = fetch(server.local_addr(), "windows", 0, &public(&k)).unwrap_err();
    assert!(other.contains("não tem uma versão mais nova"), "{other}");

    // Update requests don't disturb normal players on the same port.
    let mut a = Client::connect(&server, "Normal");
    assert!(a.wait(|c| c.states > 0), "player still joins");
    server.stop();
}

/// A fake host that answers every update request with `reply` and then `payload`.
fn evil_host(reply: UpdateReply, payload: Vec<u8>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let mut out = stream;
            let _ = writeln!(out, "{}", serde_json::to_string(&reply).unwrap());
            let _ = out.write_all(&payload);
        }
    });
    addr
}

#[test]
fn malicious_hosts_cannot_get_anything_installed() {
    let k = key(1);
    let real = b"build oficial".to_vec();
    let malware = b"#!/bin/sh\nrm -rf ~\n".to_vec();
    let attacker = key(66);
    let cases: Vec<(&str, UpdateReply, Vec<u8>)> = vec![
        ("assinado por outra chave", UpdateReply::Offer(Manifest::sign(&attacker, "linux", 50, &malware)), malware.clone()),
        ("manifesto oficial, bytes trocados", UpdateReply::Offer(Manifest { size: malware.len() as u64, ..Manifest::sign(&k, "linux", 50, &real) }), malware.clone()),
        ("downgrade", UpdateReply::Offer(Manifest::sign(&k, "linux", 5, &real)), real.clone()),
        ("plataforma errada", UpdateReply::Offer(Manifest::sign(&k, "windows", 50, &real)), real.clone()),
        ("tamanho gigante", UpdateReply::Offer(Manifest { size: u64::MAX, ..Manifest::sign(&k, "linux", 50, &real) }), real.clone()),
        ("download cortado", UpdateReply::Offer(Manifest::sign(&k, "linux", 50, &real)), real[..4].to_vec()),
    ];
    for (name, reply, payload) in cases {
        let addr = evil_host(reply, payload);
        let result = fetch(addr, "linux", 10, &public(&k));
        assert!(result.is_err(), "{name} was accepted");
    }
    // Junk instead of a reply.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\n\r\nmalware");
        }
    });
    assert!(fetch(addr, "linux", 10, &public(&k)).is_err());
}

#[test]
fn install_never_touches_the_executable_when_writing_fails() {
    let dir = Dir::new("readonly");
    let exe = dir.file("missing-folder").join("pmog");
    let m = Manifest::sign(&key(1), "linux", 2, b"x");
    assert!(install(&exe, &m, b"x").is_err(), "a missing folder is an error, not a reason to ask for the password");
    assert!(!exe.exists());
}

fn beacon(port: u16, v: u32, updates: Vec<(String, u64)>) {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let release = updates.iter().map(|(_, r)| *r).max().unwrap_or(0);
    let b = Beacon { v, name: "Host Novo".into(), port: 47_790, players: 2, release, updates };
    socket.send_to(&serde_json::to_vec(&b).unwrap(), ("127.0.0.1", port)).unwrap();
}

fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE), Instant::now());
}

#[test]
fn lobby_asks_once_and_u_asks_again() {
    let p = ports(32);
    let discovery = Discovery::start(p.discovery).unwrap();
    let mut app = App::new("Igor".into(), "a".repeat(64), Some(discovery), None, p);
    app.release = 202611160000;
    let offer = app.release + 1;
    let deadline = Instant::now() + DEADLINE;
    while app.confirm.is_none() && Instant::now() < deadline {
        let other = if pmog::update::platform() == "windows" { "linux" } else { "windows" };
        beacon(p.discovery, 1, vec![(other.into(), offer + 5), (pmog::update::platform().into(), offer)]);
        thread::sleep(Duration::from_millis(50));
        app.step(Instant::now());
    }
    assert_eq!(app.confirm, Some(Confirm::Update), "a newer build on the network is offered");
    assert_eq!(app.update_offer().unwrap().1, offer, "only builds for this platform count");
    let Some((UpdateSource::Lan(entry), _)) = app.update_offer() else { panic!("offer comes from the LAN") };
    assert!(!entry.compatible, "old-protocol servers still show up");

    press(&mut app, KeyCode::Char('n'));
    assert_eq!(app.confirm, None);
    for _ in 0..5 {
        app.step(Instant::now());
    }
    assert_eq!(app.confirm, None, "not asked again after saying no");
    press(&mut app, KeyCode::Char('u'));
    assert_eq!(app.confirm, Some(Confirm::Update), "U asks again");
    press(&mut app, KeyCode::Esc);

    let dev = Discovery::start(ports(33).discovery).unwrap();
    let mut dev_app = App::new("Dev".into(), "b".repeat(64), Some(dev), None, ports(33));
    dev_app.release = 0;
    beacon(ports(33).discovery, 1, vec![(pmog::update::platform().into(), offer)]);
    thread::sleep(Duration::from_millis(100));
    dev_app.step(Instant::now());
    assert_eq!(dev_app.confirm, None, "development builds never update");
    assert!(dev_app.update_offer().is_none());
}

#[test]
fn only_the_exact_same_build_can_join() {
    use pmog::update::version_refusal;
    assert_eq!(version_refusal((8, 100), (8, 100)), None);
    assert!(version_refusal((8, 100), (8, 101)).unwrap().contains("desatualizado"));
    assert!(version_refusal((8, 101), (8, 100)).unwrap().contains("mais velha"));
    assert!(version_refusal((7, 100), (8, 100)).is_some(), "protocol mismatch too");
}

#[test]
fn same_protocol_other_build_is_not_joinable() {
    let p = ports(34);
    let mut discovery = Discovery::start(p.discovery).unwrap();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let other = Beacon {
        v: pmog::constants::PROTOCOL_VERSION,
        name: "Outra build".into(),
        port: 47_790,
        players: 1,
        release: pmog::update::release() + 1,
        updates: Vec::new(),
    };
    let deadline = Instant::now() + DEADLINE;
    while discovery.servers().is_empty() && Instant::now() < deadline {
        socket.send_to(&serde_json::to_vec(&other).unwrap(), ("127.0.0.1", p.discovery)).unwrap();
        thread::sleep(Duration::from_millis(50));
        discovery.refresh(Instant::now());
    }
    let entry = discovery.servers().first().expect("server listed").clone();
    assert!(!entry.compatible, "listed, but not joinable");
    assert!(discovery.wait_for_server(Duration::from_millis(200)).is_none(), "bots skip it");
}

#[test]
fn release_labels_read_as_dates() {
    assert_eq!(release_label(202611161830), "build 2026-11-16 18:30");
    assert_eq!(release_label(7), "build 7");
}


/// A fake GitHub: the latest-release JSON plus the files it links to.
fn github(release_json: String, files: Vec<(&'static str, Vec<u8>)>) -> impl Fn(&str) -> Result<Vec<u8>, String> {
    move |url: &str| {
        if url == GITHUB_LATEST_URL {
            return Ok(release_json.clone().into_bytes());
        }
        files.iter().find(|(u, _)| *u == url).map(|(_, b)| b.clone()).ok_or_else(|| format!("404 {url}"))
    }
}

fn release_json(platform: &str) -> String {
    let name = dist_name(platform);
    format!(
        r#"{{"tag_name":"0.3.0","assets":[{{"name":"{name}","browser_download_url":"https://x/bin"}},{{"name":"{name}.sig","browser_download_url":"https://x/sig"}},{{"name":"SHA256SUMS","browser_download_url":"https://x/sums"}}]}}"#
    )
}

#[test]
fn github_release_with_a_newer_signed_build_is_offered_and_verified() {
    let signer = key(7);
    let platform = pmog::update::platform();
    let bytes = b"pmog nova".to_vec();
    let manifest = Manifest::sign(&signer, platform, 202611200000, &bytes);
    let get = github(release_json(platform), vec![("https://x/bin", bytes.clone()), ("https://x/sig", serde_json::to_vec(&manifest).unwrap())]);

    let offer = web_offer(platform, 202611170000, &get).unwrap().expect("newer build offered");
    assert_eq!(offer.tag, "0.3.0");
    assert_eq!(offer.manifest, manifest);
    assert_eq!(fetch_web(&offer, &public(&signer), &get).unwrap(), bytes);
    assert!(fetch_web(&offer, &public(&key(8)), &get).is_err(), "a build signed by another key is refused");

    assert!(web_offer(platform, 202611200000, &get).unwrap().is_none(), "same release is not offered");
    let other = if platform == "linux" { "windows" } else { "linux" };
    assert!(web_offer(other, 0, &get).unwrap().is_none(), "a manifest for another platform is ignored");
}

#[test]
fn github_build_that_does_not_match_its_signature_is_refused() {
    let signer = key(7);
    let platform = pmog::update::platform();
    let manifest = Manifest::sign(&signer, platform, 202611200000, b"build assinada");
    let get = github(release_json(platform), vec![("https://x/bin", b"build trocada".to_vec()), ("https://x/sig", serde_json::to_vec(&manifest).unwrap())]);
    let offer = web_offer(platform, 0, &get).unwrap().unwrap();
    assert!(fetch_web(&offer, &public(&signer), &get).is_err());
}

#[test]
fn github_release_without_this_platform_offers_nothing() {
    let get = github(r#"{"tag_name":"0.3.0","assets":[]}"#.to_string(), vec![]);
    assert!(web_offer(pmog::update::platform(), 0, &get).unwrap().is_none());
    let broken = github("not json".to_string(), vec![]);
    assert!(web_offer(pmog::update::platform(), 0, &broken).is_err());
}

#[test]
fn downloads_go_through_the_system_curl() {
    let dir = Dir::new("curl");
    let path = dir.file("build");
    fs::write(&path, b"bytes do build").unwrap();
    assert_eq!(pmog::update::download(&format!("file://{}", path.display())).unwrap(), b"bytes do build");
    assert!(pmog::update::download(&format!("file://{}", dir.file("missing").display())).is_err());
}

#[test]
fn every_platform_has_its_own_published_name() {
    assert_eq!(dist_name("linux"), "pmog-linux");
    assert_eq!(dist_name("windows"), "pmog-windows.exe");
    assert_eq!(dist_name("macos"), "pmog-macos");
    assert!(["linux", "windows", "macos"].contains(&pmog::update::platform()));
}

#[cfg(unix)]
#[test]
fn read_only_install_folders_are_detected() {
    use pmog::update::writable;
    use std::os::unix::fs::PermissionsExt;
    let dir = Dir::new("readonly");
    assert!(writable(&dir.0));
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o555)).unwrap();
    let root = fs::write(dir.file("probe"), b"x").is_ok();
    assert_eq!(writable(&dir.0), root, "only root writes into a read-only folder");
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn the_elevated_swap_script_replaces_the_build_and_its_manifest() {
    // Runs the same script pkexec would, as the current user, so no password prompt opens.
    use pmog::update::{ELEVATED_SWAP, linux_elevation_args};
    let dir = Dir::new("swap");
    let (binary, exe, signature, sig) = (dir.file("staged"), dir.file("pmog dir/pmog"), dir.file("staged.sig"), dir.file("pmog dir/pmog.sig"));
    fs::create_dir_all(exe.parent().unwrap()).unwrap();
    fs::write(&binary, b"nova").unwrap();
    fs::write(&exe, b"velha").unwrap();
    fs::write(&signature, b"{}").unwrap();
    let status = std::process::Command::new("/bin/sh").args(["-c", ELEVATED_SWAP, "pmog-update"]).args([&binary, &exe, &signature, &sig]).status().unwrap();
    assert!(status.success());
    assert_eq!(fs::read(&exe).unwrap(), b"nova");
    assert_eq!(fs::read(&sig).unwrap(), b"{}");
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(fs::metadata(&exe).unwrap().permissions().mode() & 0o777, 0o755);

    let args = linux_elevation_args(&binary, &exe, &signature, &sig);
    assert_eq!(args[..3], ["/bin/sh", "-c", ELEVATED_SWAP].map(std::ffi::OsString::from));
    assert_eq!(args[4..], [&binary, &exe, &signature, &sig].map(|p| p.as_os_str().to_owned()), "paths travel as arguments");
}

#[test]
fn the_windows_swap_script_quotes_paths_and_keeps_the_old_exe() {
    let p = |s: &str| PathBuf::from(s);
    let script = windows_swap_script(&p(r"C:\Temp\pmog"), &p(r"C:\Program Files\PMOG\pmog.exe"), &p(r"C:\Temp\pmog.sig"), &p(r"C:\Program Files\PMOG\pmog.exe.sig"));
    assert!(script.contains(r#"move /y "C:\Program Files\PMOG\pmog.exe" "C:\Program Files\PMOG\pmog.exe.old""#), "{script}");
    assert!(script.contains(r#"copy /y "C:\Temp\pmog" "C:\Program Files\PMOG\pmog.exe""#), "{script}");
    let odd = windows_swap_script(&p(r"C:\a%b\x"), &p(r"C:\y.exe"), &p(r"C:\s"), &p(r"C:\y.exe.sig"));
    assert!(odd.contains(r"C:\a%%b\x"), "percent signs are escaped for cmd");
}

#[cfg(windows)]
#[test]
fn a_running_exe_is_moved_aside_and_replaced() {
    // Windows refuses to overwrite a program that is running, so install has to rename it out of the way.
    let dir = Dir::new("running");
    let exe = dir.file("pmog.exe");
    let ping = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join(r"System32\PING.EXE");
    fs::copy(ping, &exe).unwrap();
    let mut child = std::process::Command::new(&exe).args(["-n", "30", "127.0.0.1"]).stdout(std::process::Stdio::null()).spawn().unwrap();
    assert!(fs::write(&exe, b"x").is_err(), "a running exe can't be overwritten");
    let result = install(&exe, &Manifest::sign(&key(1), "windows", 20, b"nova"), b"nova");
    let _ = child.kill();
    let _ = child.wait();
    result.unwrap();
    assert_eq!(fs::read(&exe).unwrap(), b"nova");
    assert!(dir.file("pmog.exe.old").exists(), "the running build was kept aside");
    pmog::update::cleanup(&exe);
    assert!(!dir.file("pmog.exe.old").exists());
}

#[cfg(windows)]
#[test]
fn the_windows_swap_script_runs_in_cmd() {
    // Runs the same batch UAC would, as the current user, so no prompt opens.
    let dir = Dir::new("winswap");
    let folder = dir.file("Program Files 100%");
    fs::create_dir_all(&folder).unwrap();
    let (binary, exe, signature, sig) = (dir.file("staged"), folder.join("pmog.exe"), dir.file("staged.sig"), folder.join("pmog.exe.sig"));
    fs::write(&binary, b"nova").unwrap();
    fs::write(&exe, b"velha").unwrap();
    fs::write(&signature, b"{}").unwrap();
    let script = dir.file("swap.cmd");
    fs::write(&script, windows_swap_script(&binary, &exe, &signature, &sig)).unwrap();
    assert!(std::process::Command::new("cmd").arg("/C").arg(&script).status().unwrap().success());
    assert_eq!(fs::read(&exe).unwrap(), b"nova");
    assert_eq!(fs::read(&sig).unwrap(), b"{}");
    assert_eq!(fs::read(folder.join("pmog.exe.old")).unwrap(), b"velha");
}
