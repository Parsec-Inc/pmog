//! End to end: a real server with real TCP clients on this machine.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

use common::*;
use pmog::constants::{HANDSHAKE_TIMEOUT, MAX_LINE_BYTES, MEW_CAP_PER_SEC, PROTOCOL_VERSION};
use pmog::connection::Connection;
use pmog::db::Db;
use pmog::discovery::Discovery;
use pmog::items::ItemId;
use pmog::machine_id;
use pmog::protocol::{ClientMsg, NPC_TARGET, ServerMsg, read_msg, write_msg};

#[test]
fn progress_lives_on_the_server_across_reconnects_renames_and_restarts() {
    let db = TempDb::new("persist");
    let server = start_server(0, Some(&db));
    let machine = machine_id::for_bot("persist");

    let mut a = Client::connect_as(&server, "Alfa", &machine);
    for _ in 0..MEW_CAP_PER_SEC {
        a.game.mew();
    }
    assert!(a.wait(|c| c.game.player.mp >= MEW_CAP_PER_SEC as f64), "mews counted by the server");
    a.conn.close();
    thread::sleep(Duration::from_millis(200));

    let mut again = Client::connect_as(&server, "Alfa Renomeado", &machine);
    assert_eq!(again.game.player.mp, MEW_CAP_PER_SEC as f64, "same computer, same progress");
    assert_eq!(again.game.player.name, "Alfa Renomeado");
    again.game.outbox.push(ClientMsg::Rename { name: "Alfa Final".into() });
    assert!(again.wait(|c| c.game.player.name == "Alfa Final"));
    server.stop();
    assert!(again.wait(|c| c.shutdown), "players are told the server stopped");

    let server = start_server(0, Some(&db));
    let back = Client::connect_as(&server, "Alfa Final", &machine);
    assert_eq!(back.game.player.mp, MEW_CAP_PER_SEC as f64, "survives a server restart");
    assert_eq!(server.status().registered, 0, "status is filled on the first flush");
    drop(back);
    server.stop();

    let stored = Db::open(&db.0).unwrap();
    assert_eq!(stored.player_count().unwrap(), 1);
}

#[test]
fn two_players_duel_and_every_duel_is_logged() {
    let db = TempDb::new("duel");
    let server = start_server(1, Some(&db));
    let mut clients = vec![Client::connect(&server, "Alfa"), Client::connect(&server, "Beta")];
    assert!(wait_all(&mut clients, |cs| cs.iter().all(|c| c.game.peers.len() == 2)), "both see each other");
    assert!(clients[1].feed_has("Alfa") || clients[0].feed_has("Beta entrou"));

    // Alfa attacks Beta a few times; the server decides each duel.
    clients[0].select("Beta");
    let mut attacks = 0;
    let start = Instant::now();
    while attacks < 3 && start.elapsed() < DEADLINE * 2 {
        let now = Instant::now();
        if clients[0].game.is_ready(now) {
            clients[0].game.try_mog(now);
            attacks += 1;
        }
        wait_all(&mut clients, |_| true);
        thread::sleep(Duration::from_millis(50));
    }
    assert!(wait_all(&mut clients, |cs| {
        let (a, b) = (&cs[0].game.player, &cs[1].game.player);
        a.mogs_won + a.mogs_lost >= 1 && a.mogs_won == b.defends_lost && a.mogs_lost == b.defends_won
    }), "both sides agree on every duel");
    server.stop();

    let stored = Db::open(&db.0).unwrap();
    let a = &clients[0].game.player;
    let b = &clients[1].game.player;
    let logged = stored.duel_count(None).unwrap();
    assert!(logged >= 1);
    assert_eq!(
        stored.duel_count(Some("won")).unwrap() + stored.duel_count(Some("lost")).unwrap(),
        (a.mogs_won + a.mogs_lost) as i64,
        "every resolved duel is in the history"
    );
    assert_eq!(a.mogs_won, b.defends_lost);
    assert_eq!(a.mogs_lost, b.defends_won);
}

#[test]
fn new_session_from_the_same_computer_replaces_the_old_one() {
    let server = start_server(2, None);
    let machine = machine_id::for_bot("twice");
    let mut first = Client::connect_as(&server, "Um", &machine);
    let mut other = Client::connect(&server, "Outro");
    let mut second = Client::connect_as(&server, "Dois", &machine);
    assert!(first.wait(|c| c.closed), "old session is dropped");
    assert!(first.rejected.as_deref().is_some_and(|r| r.contains("de novo")));
    assert!(second.wait(|c| c.game.peers.len() == 2), "new session plays normally");
    assert!(other.wait(|c| c.game.peers.iter().filter(|p| !p.is_npc()).count() == 1));
    assert!(other.wait(|c| c.game.peers.iter().any(|p| p.name == "Dois")));
}

#[test]
fn bad_handshakes_are_refused() {
    let server = start_server(3, None);
    let addr = server.local_addr();

    let wrong_version = |v: u32, id: &str| {
        let mut s = TcpStream::connect(addr).unwrap();
        write_msg(&mut s, &ClientMsg::Hello { v, machine_id: id.into(), name: "x".into() }).unwrap();
        s.set_read_timeout(Some(DEADLINE)).unwrap();
        read_msg::<ServerMsg>(&mut BufReader::new(s)).unwrap()
    };
    let good = machine_id::for_bot("hs");
    assert!(matches!(wrong_version(PROTOCOL_VERSION + 1, &good), Some(ServerMsg::Rejected { reason }) if reason.contains("Versão")));
    assert!(matches!(wrong_version(PROTOCOL_VERSION, "not-an-id"), Some(ServerMsg::Rejected { reason }) if reason.contains("inválida")));
    assert!(matches!(wrong_version(PROTOCOL_VERSION, &good.to_uppercase()), Some(ServerMsg::Rejected { .. })));

    let closes = |payload: &[u8]| {
        let mut s = TcpStream::connect(addr).unwrap();
        let _ = s.write_all(payload);
        s.set_read_timeout(Some(HANDSHAKE_TIMEOUT * 3)).unwrap();
        let mut line = String::new();
        matches!(BufReader::new(s).read_line(&mut line), Ok(0) | Err(_))
    };
    assert!(closes(b"isto nao e json\n"), "garbage");
    assert!(closes(b"{\"t\":\"Mew\"}\n"), "action before hello");
    assert!(closes(&vec![b'a'; MAX_LINE_BYTES + 10]), "oversized line");
    assert!(closes(b""), "silent client is dropped after the handshake timeout");

    let ok = Connection::open(addr, &good, "Depois");
    assert!(ok.is_ok(), "the server is still healthy");
}

#[test]
fn leaving_is_announced_and_the_target_disappears() {
    let server = start_server(4, None);
    let mut a = Client::connect(&server, "Fica");
    let b = Client::connect(&server, "Sai");
    assert!(a.wait(|c| c.game.peers.len() == 2));
    a.select("Sai");
    drop(b);
    assert!(a.wait(|c| c.feed_has("Sai saiu")));
    assert!(a.wait(|c| c.game.peers.len() == 1));
    assert_eq!(a.game.selected, 0, "selection falls back to the NPC");
}

#[test]
fn shop_and_consumables_round_trip() {
    let server = start_server(5, None);
    let mut c = Client::connect(&server, "Compras");
    let start = Instant::now();
    while c.game.player.mp < 25.0 && start.elapsed() < DEADLINE * 2 {
        for _ in 0..MEW_CAP_PER_SEC {
            c.game.mew();
        }
        c.wait(|_| true);
        thread::sleep(Duration::from_millis(1_050));
    }
    let mewing = pmog::items::CATALOG.iter().position(|d| d.id == ItemId::Mewing).unwrap();
    c.game.buy(mewing);
    assert!(c.wait(|c| c.game.player.level(ItemId::Mewing) == 1), "purchase confirmed by the server");
    assert!(c.feed_has("Comprou"));
    c.game.use_item(ItemId::EnergyDrink);
    assert!(c.game.outbox.is_empty(), "client refuses items it doesn't have");
    c.game.outbox.push(ClientMsg::Use { item: ItemId::EnergyDrink });
    assert!(c.wait(|c| c.feed_has("Você não tem")), "server refuses too");
    c.game.outbox.push(ClientMsg::Mog { target: NPC_TARGET, meter: 0.0 });
    assert!(c.wait(|c| c.game.player.mogs_won + c.game.player.mogs_lost == 1));
}

#[test]
fn discovery_lists_the_server_with_player_count() {
    let p = ports(6);
    let mut discovery = Discovery::start(p.discovery).expect("listen");
    let server = start_server(6, None);
    let _c = Client::connect(&server, "Visto");
    let found = discovery.wait_for_server(DEADLINE).expect("beacon arrives");
    assert_eq!(found.name, "Servidor de Teste");
    assert_eq!(found.addr.port(), server.port);
    let start = Instant::now();
    while start.elapsed() < DEADLINE && discovery.refresh(Instant::now()).first().is_none_or(|s| s.players != 1) {
        thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(discovery.servers()[0].players, 1);
    server.stop();
    let start = Instant::now();
    while start.elapsed() < DEADLINE && !discovery.refresh(Instant::now()).is_empty() {
        thread::sleep(Duration::from_millis(200));
    }
    assert!(discovery.servers().is_empty(), "stopped servers leave the list");
}

#[test]
fn port_in_use_moves_to_the_next_one() {
    let a = start_server(7, None);
    let b = start_server(7, None);
    assert_ne!(a.port, b.port);
    let mut c = Client::connect(&b, "Segundo");
    assert!(c.wait(|c| c.states > 1));
}

#[test]
fn raw_protocol_stays_line_based() {
    let server = start_server(8, None);
    let mut s = TcpStream::connect(server.local_addr()).unwrap();
    let hello = format!("{{\"t\":\"Hello\",\"v\":{PROTOCOL_VERSION},\"machine_id\":\"{}\",\"name\":\"Cru\"}}\n", machine_id::for_bot("raw"));
    s.write_all(hello.as_bytes()).unwrap();
    s.set_read_timeout(Some(DEADLINE)).unwrap();
    let mut reader = BufReader::new(s);
    assert!(matches!(read_msg::<ServerMsg>(&mut reader).unwrap(), Some(ServerMsg::Welcome { .. })));
    assert!(matches!(read_msg::<ServerMsg>(&mut reader).unwrap(), Some(ServerMsg::State(_)) | Some(ServerMsg::Event { .. })));
}

#[test]
fn a_whole_duel_over_tcp_with_escrow_and_history() {
    use pmog::bot::Duelist;
    use pmog::duel::DuelKey;

    let db = TempDb::new("pvp");
    let server = start_server(9, Some(&db));
    let mut clients = vec![Client::connect(&server, "Duelista A"), Client::connect(&server, "Duelista B")];
    assert!(wait_all(&mut clients, |cs| cs.iter().all(|c| c.game.peers.len() == 2)));
    // Earn some MP to bet.
    for _ in 0..3 {
        for c in clients.iter_mut() {
            for _ in 0..MEW_CAP_PER_SEC {
                c.game.mew();
            }
        }
        wait_all(&mut clients, |_| true);
        thread::sleep(Duration::from_millis(1_050));
    }
    assert!(wait_all(&mut clients, |cs| cs.iter().all(|c| c.game.player.mp >= 15.0)));
    let total_before: f64 = clients.iter().map(|c| c.game.player.mp).sum();

    clients[0].select("Duelista B");
    clients[0].game.stake_cursor = 4; // everything
    clients[0].game.open_challenge();
    clients[0].game.confirm_stake();
    assert!(wait_all(&mut clients, |cs| !cs[1].game.invites.is_empty()), "invite arrives");
    assert_eq!(clients[1].game.invites[0].name, "Duelista A");
    clients[1].game.answer_invite(true);
    assert!(wait_all(&mut clients, |cs| cs.iter().all(|c| c.game.duel.is_some())), "duel starts for both");
    assert!(wait_all(&mut clients, |cs| cs.iter().all(|c| c.game.player.mp == 0.0)), "all MP in escrow");

    let mut duelists = [Duelist::default(), Duelist::default()];
    let mut rng = rand::thread_rng();
    let start = Instant::now();
    let mut seen_games = std::collections::HashSet::new();
    // Up to DUEL_MAX_ROUNDS rounds of intro, play and pause.
    while start.elapsed() < Duration::from_secs(180) {
        let now = Instant::now();
        for (c, d) in clients.iter_mut().zip(duelists.iter_mut()) {
            c.pump();
            if let Some(view) = &c.game.duel {
                seen_games.insert(view.game);
            }
            d.act(&mut c.game, now, &mut rng);
        }
        if clients.iter().all(|c| c.game.duel.as_ref().is_some_and(|d| d.i_won.is_some())) {
            break;
        }
        thread::sleep(Duration::from_millis(15));
    }
    let results: Vec<_> = clients.iter().map(|c| c.game.duel.as_ref().and_then(|d| d.i_won)).collect();
    assert!(results.iter().all(Option::is_some), "duel finished: {results:?}, games {seen_games:?}");
    match (results[0].unwrap(), results[1].unwrap()) {
        (Some(a), Some(b)) => assert_ne!(a, b, "exactly one winner"),
        (None, None) => {}
        other => panic!("sides disagree: {other:?}"),
    }
    assert!(wait_all(&mut clients, |cs| cs.iter().all(|c| c.game.duel.is_none())), "result screen goes away");
    let total_after: f64 = clients.iter().map(|c| c.game.player.mp).sum();
    assert!((total_after - total_before).abs() < 1e-6, "MP is conserved: {total_before} -> {total_after}");
    clients[0].game.duel_key(DuelKey::Action, Instant::now());
    clients[0].pump();
    server.stop();
    assert_eq!(Db::open(&db.0).unwrap().match_count().unwrap(), 1);
}

#[test]
fn liveness_ignores_our_own_pauses() {
    use pmog::connection::Liveness;
    use pmog::constants::{SELF_PAUSE, SERVER_SILENCE};
    let t = Instant::now();
    let mut l = Liveness::new(t);
    assert!(!l.is_dead(false, t + SERVER_SILENCE * 3), "a long pause of our own is not the server's fault");
    let mut now = t + SERVER_SILENCE * 3;
    let step = SELF_PAUSE / 4;
    let mut dead = false;
    for _ in 0..200 {
        now += step;
        dead = l.is_dead(false, now);
        if dead {
            break;
        }
    }
    assert!(dead, "but a quiet server while we keep running is detected");
    assert!(now.saturating_duration_since(t + SERVER_SILENCE * 3) <= SERVER_SILENCE + step * 2);
    assert!(!l.is_dead(true, now));
}

#[test]
fn server_measures_round_trip_and_reports_it() {
    let server = start_server(12, None);
    let mut a = Client::connect_as(&server, "Ping", &machine_id::for_bot("ping"));
    assert!(a.wait(|c| c.game.rtt > Duration::ZERO), "ping answered and reported");
    assert!(a.game.rtt > Duration::ZERO && a.game.rtt < Duration::from_millis(200), "rtt {:?}", a.game.rtt);
    server.stop();
}

#[test]
fn old_databases_gain_the_ranked_column() {
    let db = TempDb::new("migrate");
    {
        let old = rusqlite::Connection::open(&db.0).unwrap();
        old.execute_batch(
            "CREATE TABLE matches (id INTEGER PRIMARY KEY AUTOINCREMENT, at INTEGER NOT NULL, a_id INTEGER NOT NULL,
             b_id INTEGER NOT NULL, winner_id INTEGER, stake REAL NOT NULL, rounds INTEGER NOT NULL);
             INSERT INTO matches (at, a_id, b_id, winner_id, stake, rounds) VALUES (1, 1, 2, 1, 10, 2);",
        )
        .unwrap();
    }
    let opened = Db::open(&db.0).expect("old database opens");
    assert_eq!(opened.match_count().unwrap(), 1);
    drop(opened);
    let sql = rusqlite::Connection::open(&db.0).unwrap();
    let ranked: i64 = sql.query_row("SELECT ranked FROM matches", [], |r| r.get(0)).unwrap();
    assert_eq!(ranked, 1, "past duels keep counting");
    assert!(Db::open(&db.0).is_ok(), "opening twice does not add the column again");
}
