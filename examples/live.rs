//! Live run: a real server plus bot processes over TCP, with faults injected and the database audited.
//!
//! cargo build --release && cargo run --release --example live -- [bots] [seconds] [db path]

use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use pmog::game::Player;
use pmog::server::{Ports, ServerHandle};
use rusqlite::Connection;

fn cpu_seconds() -> f64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let fields: Vec<&str> = stat.rsplit(')').next().unwrap_or("").split_whitespace().collect();
    let ticks = |i: usize| fields.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    (ticks(11) + ticks(12)) / 100.0
}

fn spawn_bot(bin: &str, name: &str, addr: &str, log_dir: &str) -> Child {
    let log = std::fs::File::create(format!("{log_dir}/{name}.log")).expect("log file");
    Command::new(bin)
        .args(["--bot", "--name", name, "--server", addr])
        .stdout(Stdio::from(log))
        .stderr(Stdio::null())
        .spawn()
        .expect("bot starts")
}

fn signal(child: &Child, sig: &str) {
    let _ = Command::new("kill").args([sig, &child.id().to_string()]).status();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(10);
    let secs: u64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(180);
    let db_path = std::path::PathBuf::from(args.get(3).cloned().unwrap_or_else(|| "/tmp/pmog-live.db".into()));
    let log_dir = db_path.with_extension("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_dir = log_dir.display().to_string();
    let bin = "target/release/pmog";

    let server = ServerHandle::start("Servidor Live", Some(db_path.clone()), Ports { discovery: 53_777, tcp_start: 53_790 })
        .expect("server");
    let addr = format!("127.0.0.1:{}", server.port);
    let mut bots: Vec<Child> = (0..n).map(|i| spawn_bot(bin, &format!("Live{i}"), &addr, &log_dir)).collect();
    println!("servidor em {addr}, {n} bots, {secs} s, banco {}", db_path.display());

    let start = Instant::now();
    let mut cpu_prev = cpu_seconds();
    let mut t_prev = start;
    let mut killed = false;
    let mut frozen = false;
    let mut thawed = false;
    while start.elapsed() < Duration::from_secs(secs) {
        thread::sleep(Duration::from_secs(10));
        let el = start.elapsed().as_secs();
        if !killed && el >= secs / 4 {
            signal(&bots[0], "-9");
            let _ = bots[0].wait();
            println!("[{el:>4}s] Live0 morto com kill -9; volta em seguida");
            bots[0] = spawn_bot(bin, "Live0", &addr, &log_dir);
            killed = true;
        }
        if !frozen && el >= secs / 3 {
            signal(&bots[1], "-STOP");
            println!("[{el:>4}s] Live1 congelado (SIGSTOP)");
            frozen = true;
        }
        if frozen && !thawed && el >= secs / 3 + 60 {
            signal(&bots[1], "-CONT");
            println!("[{el:>4}s] Live1 descongelado");
            thawed = true;
        }
        let now = Instant::now();
        let cpu = cpu_seconds();
        let load = (cpu - cpu_prev) / now.duration_since(t_prev).as_secs_f64() * 100.0;
        cpu_prev = cpu;
        t_prev = now;
        let s = server.status();
        println!(
            "[{el:>4}s] online {:>2}  cadastrados {:>2}  mogs {:>5}  cpu do servidor {load:>5.1}%  erro {:?}",
            s.online.len(), s.registered, s.duels, s.error
        );
    }
    if frozen && !thawed {
        signal(&bots[1], "-CONT");
    }
    server.stop();
    thread::sleep(Duration::from_millis(500));
    for b in bots.iter_mut() {
        let _ = b.kill();
        let _ = b.wait();
    }

    // ---------- Audit ----------
    let sql = Connection::open(&db_path).expect("db");
    let count = |q: &str| sql.query_row(q, [], |r| r.get::<_, i64>(0)).unwrap();
    let players: Vec<Player> = sql
        .prepare("SELECT profile FROM players")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|j| serde_json::from_str(&j.unwrap()).unwrap())
        .collect();
    let sum = |f: fn(&Player) -> u32| players.iter().map(|p| f(p) as i64).sum::<i64>();
    let mut problems = Vec::new();
    let mut expect = |what: &str, a: i64, b: i64| {
        let ok = a == b;
        println!("{} {what}: {a} vs {b}", if ok { "ok  " } else { "ERRO" });
        if !ok {
            problems.push(what.to_string());
        }
    };
    println!("\n# Auditoria do banco");
    let integrity: String = sql.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
    println!("integridade: {integrity}");
    expect("jogadores", players.len() as i64, n as i64);
    expect("mogs vencidos = duelos 'won'", sum(|p| p.mogs_won), count("SELECT COUNT(*) FROM duels WHERE outcome='won'"));
    expect("mogs perdidos = duelos 'lost'", sum(|p| p.mogs_lost), count("SELECT COUNT(*) FROM duels WHERE outcome='lost'"));
    expect("defesas perdidas", sum(|p| p.defends_lost), count("SELECT COUNT(*) FROM duels WHERE outcome='won' AND defender_id IS NOT NULL"));
    expect("defesas vencidas", sum(|p| p.defends_won), count("SELECT COUNT(*) FROM duels WHERE outcome='lost' AND defender_id IS NOT NULL"));
    expect("compras", sum(|p| p.purchases), count("SELECT COUNT(*) FROM purchases"));
    expect("vitórias em duelo PvP", sum(|p| p.duels_won), count("SELECT COUNT(*) FROM matches WHERE winner_id IS NOT NULL AND ranked = 1"));
    expect("derrotas em duelo PvP", sum(|p| p.duels_lost), count("SELECT COUNT(*) FROM matches WHERE winner_id IS NOT NULL AND ranked = 1"));
    expect("clãs sem membros", count("SELECT COUNT(*) FROM clans c WHERE NOT EXISTS (SELECT 1 FROM clan_members m WHERE m.clan_id = c.id)"), 0);
    expect(
        "líder fora do próprio clã",
        count("SELECT COUNT(*) FROM clans c WHERE NOT EXISTS (SELECT 1 FROM clan_members m WHERE m.clan_id = c.id AND m.player_id = c.leader_id)"),
        0,
    );
    expect("membro em clã inexistente", count("SELECT COUNT(*) FROM clan_members m WHERE NOT EXISTS (SELECT 1 FROM clans c WHERE c.id = m.clan_id)"), 0);
    println!(
        "clãs: {}, membros: {}, mensagens no chat: {}",
        count("SELECT COUNT(*) FROM clans"),
        count("SELECT COUNT(*) FROM clan_members"),
        count("SELECT COUNT(*) FROM chat_messages")
    );
    let bad_mp = players.iter().filter(|p| !p.mp.is_finite() || p.mp < 0.0).count();
    expect("jogadores com MP inválido", bad_mp as i64, 0);
    println!(
        "duelos PvP: {} (empates {}), mogs: {}, compras: {}",
        count("SELECT COUNT(*) FROM matches"),
        count("SELECT COUNT(*) FROM matches WHERE winner_id IS NULL"),
        count("SELECT COUNT(*) FROM duels"),
        count("SELECT COUNT(*) FROM purchases")
    );

    println!("\n# Logs dos bots");
    for i in 0..n {
        let log = std::fs::read_to_string(format!("{log_dir}/Live{i}.log")).unwrap_or_default();
        let connects = log.matches("conectado em").count();
        let drops = log.matches("] desconectado").count();
        let rejected = log.lines().filter(|l| l.contains("de novo em outra sessão") || l.contains("Versão")).count();
        println!("Live{i}: {connects} conexões, {drops} quedas, {rejected} recusas, {} linhas", log.lines().count());
    }
    println!("\n{}", if problems.is_empty() { "SEM PROBLEMAS".to_string() } else { format!("PROBLEMAS: {problems:?}") });
}
