//! Many bots fighting on one server, with reconnects; the database must account for every duel.

mod common;

use std::thread;
use std::time::{Duration, Instant};

use common::*;
use pmog::bot;
use pmog::constants::TICK;
use pmog::game::Player;
use rusqlite::Connection as Sql;

const BOTS: usize = 10;
const FIGHT: Duration = Duration::from_secs(12);

#[test]
fn every_duel_is_accounted_for() {
    let db = TempDb::new("stress");
    let server = start_server(10, Some(&db));
    let names: Vec<String> = (0..BOTS).map(|i| format!("Bot{i}")).collect();
    let mut clients: Vec<Client> = names.iter().map(|n| Client::connect(&server, n)).collect();
    assert!(wait_all(&mut clients, |cs| cs.iter().all(|c| c.game.peers.len() == BOTS)), "everyone sees everyone");

    let mut rng = rand::thread_rng();
    let start = Instant::now();
    let mut next_act = [start; BOTS];
    let mut reconnects = 0;
    while start.elapsed() < FIGHT {
        let now = Instant::now();
        for (i, c) in clients.iter_mut().enumerate() {
            if now >= next_act[i] {
                bot::act(&mut c.game, now, &mut rng);
                next_act[i] = now + Duration::from_millis(rand::random::<u64>() % 400 + 100);
            }
            c.pump();
        }
        // Every few seconds one bot drops and comes back.
        if start.elapsed() > Duration::from_secs(3 * (reconnects + 1)) {
            let i = reconnects as usize % BOTS;
            clients[i] = Client::connect(&server, &names[i]);
            reconnects += 1;
        }
        thread::sleep(TICK);
    }
    for c in &clients {
        assert!(!c.closed, "{} lost its connection", c.game.player.name);
    }
    server.stop();

    let sql = Sql::open(&db.0).unwrap();
    let players: Vec<Player> = sql
        .prepare("SELECT profile FROM players")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|json| serde_json::from_str(&json.unwrap()).unwrap())
        .collect();
    assert_eq!(players.len(), BOTS, "one row per computer, reconnects reuse it");
    let count = |sql_text: &str| sql.query_row(sql_text, [], |r| r.get::<_, i64>(0)).unwrap();
    let sum = |f: fn(&Player) -> u32| players.iter().map(|p| f(p) as i64).sum::<i64>();

    let duels = count("SELECT COUNT(*) FROM duels");
    assert!(duels > BOTS as i64, "bots actually fought ({duels} duels)");
    assert_eq!(sum(|p| p.mogs_won), count("SELECT COUNT(*) FROM duels WHERE outcome = 'won'"));
    assert_eq!(sum(|p| p.mogs_lost), count("SELECT COUNT(*) FROM duels WHERE outcome = 'lost'"));
    assert_eq!(sum(|p| p.defends_lost), count("SELECT COUNT(*) FROM duels WHERE outcome = 'won' AND defender_id IS NOT NULL"));
    assert_eq!(sum(|p| p.defends_won), count("SELECT COUNT(*) FROM duels WHERE outcome = 'lost' AND defender_id IS NOT NULL"));
    assert_eq!(sum(|p| p.purchases), count("SELECT COUNT(*) FROM purchases"));
    for p in &players {
        assert!(p.mp.is_finite() && p.mp >= 0.0, "{} has {} MP", p.name, p.mp);
        assert!(p.lifetime >= p.mp - 1e-6);
    }
}
