//! Fast-forward simulation: the real World plus bot clients on a simulated clock.
//! Checks invariants every tick and prints a report of the economy, mogs and duels.
//!
//! cargo run --release --example sim -- [bots] [minutes]

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use pmog::bot::{self, Duelist};
use pmog::constants::{BOT_ACTION_SECS, STATE_INTERVAL, TICK};
use pmog::duel::{Board, Minigame, Phase};
use pmog::game::{Game, Player, Rank};
use pmog::protocol::{ClientMsg, PlayerId, ServerMsg};
use pmog::world::World;
use rand::Rng;

struct Bot {
    id: PlayerId,
    game: Game,
    duelist: Duelist,
    next_action: Instant,
    online: bool,
    rank_times: BTreeMap<String, f64>,
}

#[derive(Default)]
struct MinigameStats {
    rounds: u32,
    draws: u32,
    durations: Vec<f64>,
    notes: Vec<f64>,
}

#[derive(Default)]
struct Stats {
    mogs: BTreeMap<&'static str, u32>,
    npc_mogs: BTreeMap<&'static str, u32>,
    tiers: BTreeMap<&'static str, u32>,
    mog_mp_won: f64,
    mog_mp_lost: f64,
    purchases: BTreeMap<String, (u32, f64)>,
    matches: u32,
    match_draws: u32,
    match_rounds: Vec<u32>,
    stakes: Vec<f64>,
    duel_lengths: Vec<f64>,
    minigames: BTreeMap<&'static str, MinigameStats>,
    events: BTreeMap<String, u32>,
    violations: Vec<String>,
    invites_sent: u32,
    queue_joins: u32,
    max_state_bytes: usize,
    reconnects: u32,
}

/// Groups event texts by their first words so similar messages count together.
fn event_kind(text: &str) -> String {
    let cut: String = text
        .split_whitespace()
        .filter(|w| !w.chars().any(|c| c.is_ascii_digit()) && !w.starts_with("Bot"))
        .take(4)
        .collect::<Vec<_>>()
        .join(" ");
    cut
}

fn deliver(world: &mut World, bots: &mut [Bot], stats: &mut Stats, now: Instant, all_states: bool) {
    for (to, msg) in std::mem::take(&mut world.outbox) {
        if let ServerMsg::Event { text, banner, .. } = &msg {
            let label = if text.is_empty() { banner.clone().unwrap_or_default() } else { text.clone() };
            *stats.events.entry(event_kind(&label)).or_default() += 1;
        }
        if let Some(b) = bots.iter_mut().find(|b| b.id == to && b.online) {
            b.game.apply(msg, now);
        }
    }
    let mut push = std::mem::take(&mut world.push);
    if all_states {
        push.extend(bots.iter().filter(|b| b.online).map(|b| b.id));
    }
    push.sort_unstable();
    push.dedup();
    for id in push {
        if let Some(view) = world.view(id, now) {
            let size = serde_json::to_vec(&ServerMsg::State(Box::new(view.clone()))).map_or(0, |v| v.len());
            stats.max_state_bytes = stats.max_state_bytes.max(size);
            if let Some(b) = bots.iter_mut().find(|b| b.id == id && b.online) {
                b.game.apply(ServerMsg::State(Box::new(view)), now);
            }
        }
    }
}

fn handle(world: &mut World, id: PlayerId, msg: ClientMsg, now: Instant, stats: &mut Stats) {
    match msg {
        ClientMsg::Mog { target, meter } => world.mog(id, target, meter, now),
        ClientMsg::Mew => world.mew(id, now),
        ClientMsg::Buy { item } => world.buy(id, item),
        ClientMsg::Use { item } => world.use_item(id, item, now),
        ClientMsg::Rename { name } => world.rename(id, &name),
        ClientMsg::Challenge { target, fraction, draft } => {
            stats.invites_sent += 1;
            world.challenge(id, target, fraction, draft, now)
        }
        ClientMsg::Answer { from, accept } => world.answer(id, from, accept, now),
        ClientMsg::CancelChallenge => world.cancel_challenge(id),
        ClientMsg::Queue { fraction, draft } => {
            stats.queue_joins += fraction.is_some() as u32;
            world.queue(id, fraction, draft, now)
        }
        ClientMsg::Duel { key, at_ms } => world.duel_input(id, key, at_ms, 0.0, now),
        ClientMsg::Pong { .. } | ClientMsg::Chat(_) | ClientMsg::Clan(_) => {}
        ClientMsg::Forfeit => world.forfeit(id, now),
        ClientMsg::Hello { .. } => {}
    }
}

/// Watches each running duel from side A to time rounds and collect per-minigame numbers.
#[derive(Default)]
struct DuelWatch {
    started: HashMap<(PlayerId, PlayerId), Instant>,
    phase: HashMap<(PlayerId, PlayerId), (u32, Phase, Instant)>,
}

fn watch_duels(world: &World, watch: &mut DuelWatch, stats: &mut Stats, now: Instant) {
    let mut seen = Vec::new();
    for (id, _) in world.online() {
        let Some(view) = world.view(id, now) else { continue };
        let Some(d) = view.duel else { continue };
        let Some(opp) = world.online().find(|(o, s)| *o != id && s.player.name == d.opponent).map(|(o, _)| o) else {
            continue;
        };
        if id > opp {
            continue;
        }
        let key = (id, opp);
        seen.push(key);
        watch.started.entry(key).or_insert(now);
        let prev = watch.phase.get(&key).copied();
        if prev.is_none_or(|(r, p, _)| r != d.round || p != d.phase) {
            if d.phase == Phase::Play && prev.is_none_or(|(_, p, _)| p != Phase::Show) {
                watch.phase.insert(key, (d.round, d.phase, now));
            } else {
                let play_start = prev.map_or(now, |(_, _, t)| t);
                watch.phase.insert(key, (d.round, d.phase, if d.phase == Phase::Show && prev.is_some_and(|(_, p, _)| p == Phase::Countdown) { now } else { play_start }));
            }
            if d.phase == Phase::RoundOver {
                let m = stats.minigames.entry(d.game.name()).or_default();
                m.rounds += 1;
                m.draws += d.i_won_round.is_none() as u32;
                m.durations.push(now.saturating_duration_since(play_start_of(prev, now)).as_secs_f64());
                match &d.board {
                    Board::Mewing { mine, theirs } => m.notes.push((*mine).max(*theirs) as f64),
                    Board::Angle { mine, theirs, .. } => {
                        let best = |v: &Vec<f64>| v.iter().copied().fold(1.0, f64::min);
                        m.notes.push(best(mine).min(best(theirs)) * 100.0)
                    }
                    Board::Skincare { level, .. } => m.notes.push(*level as f64),
                    Board::Rhythm { mine, theirs } => m.notes.push(mine.score.max(theirs.score)),
                    Board::Hunter { mine, theirs, .. } => m.notes.push(mine.score.max(theirs.score)),
                    Board::Macros { mine, theirs, .. } => m.notes.push(mine.solved.max(theirs.solved) as f64),
                    Board::Typing { .. } => {
                        let first_wpm = d.result.as_deref().and_then(|r| r.split(" ppm").next()).and_then(|r| r.rsplit(' ').next());
                        if let Some(wpm) = first_wpm.and_then(|n| n.parse::<f64>().ok()) {
                            m.notes.push(wpm);
                        }
                    }
                    Board::Jawline { mine, theirs, .. } => m.notes.push(mine.error.min(theirs.error) as f64),
                    Board::Hydration { .. }
                    | Board::Posture { .. }
                    | Board::FitCheck { .. }
                    | Board::Photo { .. }
                    | Board::Status { .. }
                    | Board::Brow { .. }
                    | Board::ReverseMog { .. }
                    | Board::Draft { .. } => {}
                    Board::Hair { mine, theirs, .. } => m.notes.push(mine.score.max(theirs.score)),
                    Board::Staredown { .. } => {
                        if let Some(ms) = d.result.as_deref().and_then(|r| r.split("em ").nth(1)).and_then(|r| r.split(' ').next()).and_then(|n| n.parse::<f64>().ok()) {
                            m.notes.push(ms);
                        }
                    }
                }
            }
        }
    }
    let ended: Vec<_> = watch.started.keys().copied().filter(|k| !seen.contains(k)).collect();
    for key in ended {
        if let Some(start) = watch.started.remove(&key) {
            stats.duel_lengths.push(now.saturating_duration_since(start).as_secs_f64());
        }
        watch.phase.remove(&key);
    }
}

fn play_start_of(prev: Option<(u32, Phase, Instant)>, now: Instant) -> Instant {
    prev.map_or(now, |(_, _, t)| t)
}

fn check(world: &World, bots: &[Bot], stats: &mut Stats, now: Instant, minute: f64) {
    let mut in_duel = 0;
    for (id, slot) in world.online() {
        let p = &slot.player;
        if !p.mp.is_finite() || p.mp < -1e-6 || !p.lifetime.is_finite() {
            stats.violations.push(format!("[{minute:.1} min] {} MP inválido: {} / {}", p.name, p.mp, p.lifetime));
        }
        in_duel += world.in_duel(id) as usize;
        if let Some(view) = world.view(id, now)
            && let Some(out) = &view.outgoing
            && out.expires_ms > pmog::constants::INVITE_TTL.as_millis() as u64
        {
            stats.violations.push(format!("[{minute:.1} min] convite com validade acima do limite"));
        }
    }
    if in_duel > world.duel_count() * 2 {
        stats.violations.push(format!("[{minute:.1} min] {in_duel} jogadores em duelo para {} duelos", world.duel_count()));
    }
    for b in bots.iter().filter(|b| b.online) {
        if let Some(slot) = world.slot(b.id)
            && b.game.player.achievements.len() > slot.player.achievements.len()
        {
            stats.violations.push(format!("[{minute:.1} min] cliente de {} tem conquistas que o servidor não tem", b.game.player.name));
        }
    }
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 }
}

fn pct(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    s[((s.len() - 1) as f64 * p).round() as usize]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: i64 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(8);
    let minutes: f64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(120.0);
    let mut rng = rand::thread_rng();
    let t0 = Instant::now();
    let mut now = t0;
    let mut world = World::new();
    let mut saved: HashMap<PlayerId, Player> = HashMap::new();
    let mut bots: Vec<Bot> = (1..=n)
        .map(|id| {
            let name = format!("Bot{id}");
            world.join(id, format!("m{id}"), Player { name: name.clone(), ..Default::default() }, now).unwrap();
            Bot {
                id,
                game: Game::new(Player { name, ..Default::default() }, now),
                duelist: Duelist::default(),
                next_action: now + Duration::from_secs_f64(rng.gen_range(BOT_ACTION_SECS)),
                online: true,
                rank_times: BTreeMap::new(),
            }
        })
        .collect();
    let mut stats = Stats::default();
    let mut watch = DuelWatch::default();
    let mut last_state = now;
    let mut timeline = Vec::new();
    let end = now + Duration::from_secs_f64(minutes * 60.0);
    let wall = Instant::now();
    let mut last_minute = -1i64;

    while now < end {
        now += TICK;
        let minute = now.saturating_duration_since(t0).as_secs_f64() / 60.0;
        world.tick(now, TICK.as_secs_f64());

        // Clients act and send.
        for b in bots.iter_mut().filter(|b| b.online) {
            if !b.game.invites.is_empty() {
                b.game.answer_invite(true);
            }
            if b.game.duel.is_some() {
                b.duelist.act(&mut b.game, now, &mut rng);
            } else if now >= b.next_action {
                b.duelist = Duelist::default();
                bot::act(&mut b.game, now, &mut rng);
                b.next_action = now + Duration::from_secs_f64(rng.gen_range(BOT_ACTION_SECS));
            }
        }
        for b in bots.iter_mut().filter(|b| b.online) {
            for msg in std::mem::take(&mut b.game.outbox) {
                handle(&mut world, b.id, msg, now, &mut stats);
            }
        }

        // Rare disconnects (with reconnect a bit later).
        for b in bots.iter_mut() {
            if b.online && rng.gen_bool(0.00003) {
                if let Some(p) = world.leave(b.id, now) {
                    saved.insert(b.id, p);
                }
                b.online = false;
                stats.reconnects += 1;
            } else if !b.online && rng.gen_bool(0.002) {
                let p = saved.remove(&b.id).unwrap_or_default();
                world.join(b.id, format!("m{}", b.id), p.clone(), now).unwrap();
                b.game = Game::new(p, now);
                b.online = true;
            }
        }

        let periodic = now.saturating_duration_since(last_state) >= STATE_INTERVAL;
        if periodic {
            last_state = now;
        }
        deliver(&mut world, &mut bots, &mut stats, now, periodic);
        watch_duels(&world, &mut watch, &mut stats, now);

        for d in std::mem::take(&mut world.duels) {
            let map = if d.defender.is_some() { &mut stats.mogs } else { &mut stats.npc_mogs };
            *map.entry(d.outcome).or_default() += 1;
            *stats.tiers.entry(d.tier).or_default() += 1;
            stats.mog_mp_won += d.mp_won;
            stats.mog_mp_lost += d.mp_lost;
        }
        for p in std::mem::take(&mut world.purchases) {
            let e = stats.purchases.entry(p.item).or_default();
            e.0 += 1;
            e.1 += p.cost;
        }
        for m in std::mem::take(&mut world.matches) {
            stats.matches += 1;
            stats.match_draws += m.winner.is_none() as u32;
            stats.match_rounds.push(m.rounds);
            stats.stakes.push(m.stake);
        }
        world.take_dirty();

        if minute as i64 != last_minute {
            last_minute = minute as i64;
            check(&world, &bots, &mut stats, now, minute);
            for b in bots.iter_mut() {
                if let Some(slot) = world.slot(b.id) {
                    b.rank_times.entry(slot.player.rank().label().to_string()).or_insert(minute);
                }
            }
            if last_minute % 10 == 0 {
                let ps: Vec<&Player> = world.online().map(|(_, s)| &s.player).collect();
                let total: f64 = ps.iter().map(|p| p.lifetime).sum();
                let best = ps.iter().map(|p| p.lifetime).fold(0.0, f64::max);
                let worst = ps.iter().map(|p| p.lifetime).fold(f64::MAX, f64::min);
                timeline.push((last_minute, total / ps.len().max(1) as f64, best, worst, ps.iter().map(|p| p.items.values().sum::<u32>()).sum::<u32>()));
            }
        }
    }

    // ---------- Report ----------
    println!("# Simulação: {n} bots, {minutes} min de jogo ({:.1} s reais)\n", wall.elapsed().as_secs_f64());
    println!("## Invariantes: {} violação(ões)", stats.violations.len());
    for v in stats.violations.iter().take(20) {
        println!("- {v}");
    }
    println!("\n## Linha do tempo (a cada 10 min)\n| min | total médio | melhor | pior | itens (soma) |\n|---|---|---|---|---|");
    for (m, avg, best, worst, items) in &timeline {
        println!("| {m} | {avg:.0} | {best:.0} | {worst:.0} | {items} |");
    }
    println!("\n## Jogadores no fim\n| bot | rank | MP | total | mogs V/D | defesas V/D | duelos V/D | itens | chegou em |\n|---|---|---|---|---|---|---|---|---|");
    for b in &bots {
        let p = world.slot(b.id).map(|s| s.player.clone()).or_else(|| saved.get(&b.id).cloned()).unwrap_or_default();
        let ranks: Vec<String> = Rank::ALL
            .iter()
            .filter_map(|r| b.rank_times.get(r.label()).map(|m| format!("{} {:.0}m", r.label(), m)))
            .collect();
        println!(
            "| {} | {} | {:.0} | {:.0} | {}/{} | {}/{} | {}/{} | {} | {} |",
            p.name, p.rank().label(), p.mp, p.lifetime, p.mogs_won, p.mogs_lost, p.defends_won, p.defends_lost,
            p.duels_won, p.duels_lost, p.items.values().sum::<u32>(), ranks.join(", ")
        );
    }
    println!("\n## Mogs\n- contra jogadores: {:?}\n- contra NPC: {:?}\n- timing: {:?}\n- MP ganho em mogs: {:.0}, perdido: {:.0}", stats.mogs, stats.npc_mogs, stats.tiers, stats.mog_mp_won, stats.mog_mp_lost);
    println!("\n## Compras\n| item | qtd | MP gasto |\n|---|---|---|");
    let mut buys: Vec<_> = stats.purchases.iter().collect();
    buys.sort_by(|a, b| b.1.1.total_cmp(&a.1.1));
    for (item, (count, cost)) in buys {
        println!("| {item} | {count} | {cost:.0} |");
    }
    println!(
        "\n## Duelos\n- desafios enviados: {}, entradas na fila: {}\n- duelos: {} (empates {}), rodadas médias {:.1}\n- aposta: média {:.0}, mediana {:.0}, máx {:.0}\n- duração: média {:.0} s, p95 {:.0} s, máx {:.0} s",
        stats.invites_sent, stats.queue_joins, stats.matches, stats.match_draws,
        mean(&stats.match_rounds.iter().map(|r| *r as f64).collect::<Vec<_>>()),
        mean(&stats.stakes), pct(&stats.stakes, 0.5), pct(&stats.stakes, 1.0),
        mean(&stats.duel_lengths), pct(&stats.duel_lengths, 0.95), pct(&stats.duel_lengths, 1.0)
    );
    println!("\n| minigame | rodadas | empates | duração média (s) | máx (s) | métrica (média / p95) |\n|---|---|---|---|---|---|");
    for game in Minigame::ALL {
        let Some(m) = stats.minigames.get(game.name()) else { continue };
        let metric = match game {
            Minigame::Staredown => "reação ms",
            Minigame::Skincare => "nível final",
            Minigame::Mewing => "mewadas do vencedor",
            Minigame::Angle => "melhor distância %",
            Minigame::Typing => "ppm do vencedor",
            Minigame::Rhythm => "pontos do vencedor",
            Minigame::Hunter => "pontos do vencedor",
            Minigame::Macros => "contas do vencedor",
            Minigame::Memory => "nível final",
            Minigame::Jawline => "menor erro",
            Minigame::Hair => "pontos do vencedor",
            _ => "-",
        };
        println!(
            "| {} | {} | {} ({:.0}%) | {:.1} | {:.1} | {metric}: {:.1} / {:.1} |",
            game.name(), m.rounds, m.draws, 100.0 * m.draws as f64 / m.rounds.max(1) as f64,
            mean(&m.durations), pct(&m.durations, 1.0), mean(&m.notes), pct(&m.notes, 0.95)
        );
    }
    println!("\n## Rede\n- maior estado enviado: {} bytes\n- desconexões simuladas: {}", stats.max_state_bytes, stats.reconnects);
    println!("\n## Mensagens mais frequentes");
    let mut events: Vec<_> = stats.events.into_iter().collect();
    events.sort_by(|a, b| b.1.cmp(&a.1));
    for (kind, count) in events.iter().take(25) {
        println!("- {count:>6}  {kind}");
    }
}
