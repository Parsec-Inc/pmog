//! Fairness under lag: a bot on the server's machine duels a bot on slow Wi-Fi, with and without compensation.
//!
//! cargo run --release --example latency -- [duels per mode] [slow one-way ms] [jitter ms]

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use pmog::bot::Duelist;
use pmog::constants::STATE_INTERVAL;
use pmog::duel::Phase;
use pmog::game::{Game, Player};
use pmog::protocol::{ClientMsg, PlayerId, ServerMsg};
use pmog::world::World;
use rand::Rng;

const STEP: Duration = Duration::from_millis(1);

/// One direction of a TCP link: in order, each message delayed by latency plus jitter.
struct Pipe<T> {
    queue: VecDeque<(Instant, T)>,
}

impl<T> Pipe<T> {
    fn new() -> Pipe<T> {
        Pipe { queue: VecDeque::new() }
    }

    fn send(&mut self, msg: T, now: Instant, one_way: f64, jitter: f64, rng: &mut impl Rng) {
        let delay = (one_way + rng.gen_range(-jitter..=jitter)).max(0.1);
        let at = now + Duration::from_secs_f64(delay / 1000.0);
        let at = self.queue.back().map_or(at, |(last, _)| at.max(*last));
        self.queue.push_back((at, msg));
    }

    fn recv(&mut self, now: Instant) -> Vec<T> {
        let mut out = Vec::new();
        while self.queue.front().is_some_and(|(at, _)| *at <= now) {
            out.push(self.queue.pop_front().expect("checked").1);
        }
        out
    }
}

struct Side {
    id: PlayerId,
    game: Game,
    duelist: Duelist,
    one_way: f64,
    jitter: f64,
    up: Pipe<ClientMsg>,
    down: Pipe<ServerMsg>,
}

/// Rounds the slow player won, lost and drew, per minigame.
type Tally = BTreeMap<&'static str, [u32; 3]>;

fn run(duels: u32, slow_ms: f64, jitter: f64, compensate: bool) -> (Tally, u32) {
    let mut rng = rand::thread_rng();
    let mut now = Instant::now();
    let mut world = World::new();
    let mut sides: Vec<Side> = [(1, "Rapido", 0.5, 0.3), (2, "Lento", slow_ms, jitter)]
        .into_iter()
        .map(|(id, name, one_way, jitter)| {
            world.join(id, format!("m{id}"), Player { name: name.into(), ..Default::default() }, now).unwrap();
            let mut game = Game::new(Player { name: name.into(), ..Default::default() }, now);
            game.compensate = compensate;
            Side { id, game, duelist: Duelist::default(), one_way, jitter, up: Pipe::new(), down: Pipe::new() }
        })
        .collect();
    let mut tally = Tally::new();
    let mut guesses = 0;
    let mut done = 0;
    let mut last_state = now;
    let mut seen_round = None;
    while done < duels {
        now += STEP;
        world.tick(now, STEP.as_secs_f64());
        if world.duels().is_empty() && world.view(1, now).is_some_and(|v| v.outgoing.is_none()) {
            world.challenge(1, 2, 0.0, false, now);
            world.answer(2, 1, true, now);
        }
        for s in sides.iter_mut() {
            for msg in s.down.recv(now) {
                s.game.apply(msg, now);
            }
            if s.game.duel.is_some() {
                s.duelist.act(&mut s.game, now, &mut rng);
            } else {
                s.duelist = Duelist::default();
            }
            for msg in std::mem::take(&mut s.game.outbox) {
                s.up.send(msg, now, s.one_way, s.jitter, &mut rng);
            }
            for msg in s.up.recv(now) {
                if let ClientMsg::Duel { key, at_ms } = msg {
                    let rtt_max = 2.0 * (s.one_way + s.jitter);
                    world.duel_input(s.id, key, at_ms, rtt_max, now);
                }
            }
        }
        if let Some(d) = world.duels().first()
            && d.phase == Phase::RoundOver
            && seen_round != Some((done, d.rounds))
        {
            seen_round = Some((done, d.rounds));
            let last = d.last.as_ref().expect("round over");
            guesses += (last.text.contains("chutou") || last.text.contains("piscou")) as u32;
            let slow = d.side(2).expect("in duel");
            let slot = match last.winner {
                Some(w) if w == slow => 0,
                Some(_) => 1,
                None => 2,
            };
            tally.entry(d.game.name()).or_default()[slot] += 1;
        }
        if world.duels().first().is_some_and(|d| d.phase == Phase::Finished) && seen_round.is_some_and(|(n, _)| n == done) {
            done += 1;
        }
        // Server to clients: events and pushed states now, every state on the periodic tick.
        let periodic = now.saturating_duration_since(last_state) >= STATE_INTERVAL;
        if periodic {
            last_state = now;
        }
        let mut push = std::mem::take(&mut world.push);
        for (to, msg) in std::mem::take(&mut world.outbox) {
            if let Some(s) = sides.iter_mut().find(|s| s.id == to) {
                s.down.send(msg, now, s.one_way, s.jitter, &mut rng);
            }
        }
        if periodic {
            push.extend([1, 2]);
        }
        push.sort_unstable();
        push.dedup();
        for id in push {
            let Some(s) = sides.iter_mut().find(|s| s.id == id) else { continue };
            if let Some(mut view) = world.view(id, now) {
                view.rtt_ms = 2.0 * s.one_way;
                s.down.send(ServerMsg::State(Box::new(view)), now, s.one_way, s.jitter, &mut rng);
            }
        }
    }
    (tally, guesses)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let duels: u32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(150);
    let slow: f64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(25.0);
    let jitter: f64 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(10.0);
    println!("# Duelos com lag: Rapido (0,5 ms) x Lento ({slow} ms ±{jitter} ms, só ida), {duels} duelos por modo\n");
    println!("| modo | minigame | rodadas | Lento venceu | Rapido venceu | empates |");
    println!("|---|---|---:|---:|---:|---:|");
    for compensate in [false, true] {
        let mode = if compensate { "com compensação" } else { "sem compensação" };
        let (tally, guesses) = run(duels, slow, jitter, compensate);
        for (game, [w, l, d]) in &tally {
            let n = w + l + d;
            let p = |x: u32| format!("{:.0}%", 100.0 * x as f64 / n.max(1) as f64);
            println!("| {mode} | {game} | {n} | {} | {} | {} |", p(*w), p(*l), p(*d));
        }
        println!("| {mode} | chutes/piscadas | {guesses} | | | |");
    }
}
