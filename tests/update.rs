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
use pmog::app::{App, Confirm};
use pmog::discovery::Discovery;
use pmog::input::handle_key;
use pmog::protocol::Beacon;
use pmog::server::ServerHandle;
use pmog::update::{Manifest, Shelf, UpdateReply, fetch, hex_encode, install, release_label, sidecar};
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
    assert!(install(&exe, &m, b"x").is_err());
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
        beacon(p.discovery, 1, vec![("windows".into(), offer + 5), (pmog::update::platform().into(), offer)]);
        thread::sleep(Duration::from_millis(50));
        app.step(Instant::now());
    }
    assert_eq!(app.confirm, Some(Confirm::Update), "a newer build on the network is offered");
    assert_eq!(app.update_offer().unwrap().1, offer, "only builds for this platform count");
    let entry = app.update_offer().unwrap().0;
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

