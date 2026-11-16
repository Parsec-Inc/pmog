//! Headless bot client for testing with several players on one machine.

use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, Instant};

use rand::Rng;

use crate::connection::{Connection, Incoming, Liveness};
use crate::constants::{ANGLE_ATTEMPTS, BOT_ACTION_SECS, BOT_MEWS_PER_ACTION, BOT_RETRY, STAKE_FRACTIONS, TICK};
use crate::discovery::Discovery;
use crate::duel::{
    Board, BrowTrial, DuelKey, DuelView, Minigame, Phase, Stance, angle_pos, fit_color, hydration_level, photo_flash, rhythm_beat_ms,
    rps_throw,
};
use crate::constants::{
    BOT_DRAFT_MS, BOT_HUNTER_REACT_MS, BOT_HUNTER_SLIP, BOT_MACROS_SECS, BOT_MACROS_SLIP, BOT_RHYTHM_SPREAD_MS, BOT_TYPING_CPS,
    HUNTER_SHOW_MS, HYDRATION_SIP_BIG, JAW_POINTS, PHOTO_LATE_MS, POSTURE_BAND, RHYTHM_BEATS,
};
use crate::game::{Game, Player};
use crate::items::CATALOG;
use crate::machine_id;
use crate::chat::{ChatCmd, Conv};
use crate::clan::ClanCmd;
use crate::protocol::{ClientMsg, PlayerId, ServerMsg};

/// Plays forever on `target` or the first server found, reconnecting when it drops.
pub fn run(name: &str, discovery_port: u16, target: Option<SocketAddr>) {
    let machine_id = machine_id::for_bot(name);
    let mut discovery = if target.is_none() { Discovery::start(discovery_port).ok() } else { None };
    loop {
        let addr = target.or_else(|| discovery.as_mut().and_then(|d| d.wait_for_server(BOT_RETRY)).map(|s| s.addr));
        let Some(addr) = addr else {
            println!("[{name}] procurando servidor...");
            continue;
        };
        match Connection::open(addr, &machine_id, name) {
            Ok(mut conn) => {
                println!("[{name}] conectado em {} ({addr})", conn.server_name);
                play(name, &mut conn);
                println!("[{name}] desconectado");
            }
            Err(reason) => println!("[{name}] {reason}"),
        }
        thread::sleep(BOT_RETRY);
    }
}

/// Plays on one connection until it closes.
fn play(name: &str, conn: &mut Connection) {
    let mut rng = rand::thread_rng();
    let mut game = Game::new(Player { name: name.into(), ..Default::default() }, Instant::now());
    let mut next_action = Instant::now();
    let mut printed = 0usize;
    let mut liveness = Liveness::new(Instant::now());
    let mut duelist = Duelist::default();
    loop {
        let now = Instant::now();
        let incoming = conn.drain();
        if liveness.is_dead(!incoming.is_empty(), now) {
            return;
        }
        for incoming in incoming {
            match incoming {
                Incoming::Msg(ServerMsg::Shutdown | ServerMsg::Rejected { .. }) | Incoming::Closed => return,
                Incoming::Msg(msg) => game.apply(msg, now),
            }
        }
        if !game.invites.is_empty() {
            game.answer_invite(true);
        }
        if game.duel.is_some() {
            duelist.act(&mut game, now, &mut rng);
        } else if now >= next_action {
            duelist = Duelist::default();
            act(&mut game, now, &mut rng);
            next_action = now + Duration::from_secs_f64(rng.gen_range(BOT_ACTION_SECS));
        }
        for msg in std::mem::take(&mut game.outbox) {
            if !conn.send(&msg) {
                return;
            }
        }
        printed = echo_feed(&game, printed);
        thread::sleep(TICK);
    }
}

/// One bot decision: mew, buy the cheapest affordable upgrade and mog someone.
pub fn act(game: &mut Game, now: Instant, rng: &mut impl Rng) {
    for _ in 0..BOT_MEWS_PER_ACTION {
        game.mew();
    }
    let cheapest = CATALOG
        .iter()
        .enumerate()
        .filter(|(_, d)| !d.consumable() && game.player.rank() >= d.min_rank && game.player.level(d.id) < d.max_level)
        .min_by(|(_, a), (_, b)| a.cost(game.player.level(a.id)).total_cmp(&b.cost(game.player.level(b.id))));
    if let Some((index, def)) = cheapest
        && game.player.mp >= def.cost(game.player.level(def.id))
    {
        game.buy(index);
    }
    let humans: Vec<usize> = (0..game.peers.len()).filter(|&i| !game.peers[i].is_npc() && !game.peers[i].in_duel).collect();
    // Only attack players it can plausibly beat; otherwise farm the NPC.
    let attack = game.player.stats().attack;
    let beatable: Vec<usize> = humans.iter().copied().filter(|&i| game.peers[i].immune_left(Instant::now()).is_none() && game.peers[i].defense <= attack * 1.2).collect();
    game.selected = if beatable.is_empty() { 0 } else { beatable[rng.gen_range(0..beatable.len())] };
    let idle = game.duel.is_none() && game.outgoing.is_none() && game.queued.is_none();
    let roll: f64 = rng.r#gen();
    if idle && !humans.is_empty() && roll < BOT_CHALLENGE_CHANCE {
        game.stake_cursor = rng.gen_range(0..3);
        game.open_challenge();
        game.confirm_stake();
    } else if idle && roll < BOT_CHALLENGE_CHANCE + BOT_QUEUE_CHANCE {
        game.stake_cursor = rng.gen_range(0..STAKE_FRACTIONS.len().min(3));
        game.toggle_queue();
        game.confirm_stake();
    } else {
        game.try_mog(now);
    }
    social(game, rng);
}

const BOT_LINES: [&str; 6] = ["bora moggar", "olha esse jawline", "mewing em dia", "quem quer duelo?", "gg", "fui moggado de novo"];

/// Chats now and then, and builds clans: creates one, invites people, accepts invites.
fn social(game: &mut Game, rng: &mut impl Rng) {
    if let Some(invite) = game.clan.invites.first() {
        let clan = invite.clan;
        game.outbox.push(ClientMsg::Clan(ClanCmd::Accept { clan }));
    }
    let line = BOT_LINES[rng.gen_range(0..BOT_LINES.len())].to_string();
    let clan_conv = game.chat.convs.iter().map(|c| c.conv).find(|c| matches!(c, Conv::Clan(_)));
    if rng.gen_bool(BOT_CHAT_CHANCE) {
        let conv = clan_conv.filter(|_| rng.gen_bool(0.5)).unwrap_or(Conv::General);
        game.outbox.push(ClientMsg::Chat(ChatCmd::Send { conv, text: line }));
    }
    match &game.clan.clan {
        None if game.clan.invites.is_empty() && rng.gen_bool(BOT_CLAN_CREATE_CHANCE) => {
            let tag: String = (0..4).map(|_| rng.gen_range(b'A'..=b'Z') as char).collect();
            game.outbox.push(ClientMsg::Clan(ClanCmd::Create { name: format!("Clã {tag}"), tag }));
        }
        Some(clan) if clan.is_leader() && rng.gen_bool(BOT_CLAN_INVITE_CHANCE) => {
            let free: Vec<PlayerId> = game.peers.iter().filter(|p| !p.is_npc() && p.clan_tag.is_none()).map(|p| p.id).collect();
            if !free.is_empty() {
                let target = free[rng.gen_range(0..free.len())];
                game.outbox.push(ClientMsg::Clan(ClanCmd::Invite { target }));
            }
        }
        Some(_) if rng.gen_bool(BOT_CLAN_LEAVE_CHANCE) => game.outbox.push(ClientMsg::Clan(ClanCmd::Leave)),
        _ => {}
    }
}

const BOT_CHALLENGE_CHANCE: f64 = 0.12;
const BOT_QUEUE_CHANCE: f64 = 0.08;
const BOT_CHAT_CHANCE: f64 = 0.1;
const BOT_CLAN_CREATE_CHANCE: f64 = 0.08;
const BOT_CLAN_INVITE_CHANCE: f64 = 0.5;
const BOT_CLAN_LEAVE_CHANCE: f64 = 0.02;

/// Plays duel minigames with human-ish reflexes and the odd mistake.
#[derive(Default)]
pub struct Duelist {
    next_press: Option<Instant>,
    routine: Vec<DuelKey>,
    sent_step: Option<(usize, usize)>,
    last_mew: Option<DuelKey>,
    /// Characters per second this bot types in the current round.
    typing_cps: Option<f64>,
    /// Rhythm beat or hunter face already handled this round.
    handled: Option<usize>,
}

impl Duelist {
    pub fn act(&mut self, game: &mut Game, now: Instant, rng: &mut impl Rng) {
        game.predict_duel(now);
        let Some(d) = game.duel.clone() else { return };
        if d.phase == Phase::Countdown || d.phase == Phase::RoundOver {
            self.next_press = None;
            self.sent_step = None;
            self.typing_cps = None;
            self.handled = None;
        }
        let ms = game.duel_elapsed(now) * 1000.0;
        let due = self.next_press.is_none_or(|t| now >= t);
        match (&d.board, d.phase) {
            (Board::Staredown { i_pressed: false, .. }, Phase::Play) if game.staredown_go(now) => match self.next_press {
                None => self.next_press = Some(now + Duration::from_millis(rng.gen_range(170..450))),
                Some(t) if now >= t => game.duel_key(DuelKey::Action, now),
                _ => {}
            },
            (Board::Skincare { sequence, .. }, Phase::Show) => self.routine = sequence.clone(),
            (Board::Skincare { level, mine, .. }, Phase::Play) if !mine.done && !mine.failed && due => {
                if self.sent_step == Some((*level, mine.steps)) {
                    return;
                }
                let right = self.routine.get(mine.steps).copied().unwrap_or(DuelKey::Up);
                let wrong = match right {
                    DuelKey::Char(c) => DuelKey::Char(if c == '1' { '2' } else { '1' }),
                    _ => *DuelKey::ARROWS.iter().find(|k| **k != right).expect("4 arrows"),
                };
                let key = if rng.gen_bool(0.05) { wrong } else { right };
                game.duel_key(key, now);
                self.sent_step = Some((*level, mine.steps));
                self.next_press = Some(now + Duration::from_millis(rng.gen_range(180..380)));
            }
            (Board::Mewing { .. }, Phase::Play) if due => {
                let key = if self.last_mew == Some(DuelKey::Left) { DuelKey::Right } else { DuelKey::Left };
                game.duel_key(key, now);
                self.last_mew = Some(key);
                self.next_press = Some(now + Duration::from_millis(rng.gen_range(70..160)));
            }
            (Board::Typing { text, .. }, Phase::Play) if due => {
                let Some(mine) = game.typing_mine(now).filter(|m| !m.done) else { return };
                let want = text.chars().nth(mine.typed).unwrap_or(' ');
                let key = if rng.gen_bool(0.04) { if want == 'x' { 'z' } else { 'x' } } else { want };
                game.duel_key(DuelKey::Char(key), now);
                let cps = *self.typing_cps.get_or_insert_with(|| rng.gen_range(BOT_TYPING_CPS));
                self.next_press = Some(now + Duration::from_secs_f64(rng.gen_range(0.6..1.4) / cps));
            }
            (Board::Rhythm { .. }, Phase::Play) => {
                let next = self.handled.map_or(0, |b| b + 1);
                if next >= RHYTHM_BEATS {
                    return;
                }
                let at = *self.next_press.get_or_insert_with(|| {
                    let ahead = rhythm_beat_ms(next) - ms + rng.gen_range(-BOT_RHYTHM_SPREAD_MS..BOT_RHYTHM_SPREAD_MS);
                    now + Duration::from_secs_f64(ahead.max(0.0) / 1000.0)
                });
                if now >= at {
                    game.duel_key(DuelKey::Action, now);
                    self.handled = Some(next);
                    self.next_press = None;
                }
            }
            (Board::Hunter { schedule, .. }, Phase::Play) => {
                let Some(i) = schedule.iter().position(|(at, _)| (*at..at + HUNTER_SHOW_MS).contains(&ms)) else { return };
                if self.handled == Some(i) {
                    return;
                }
                let (start, target) = schedule[i];
                if self.next_press.is_none() && !target && !rng.gen_bool(BOT_HUNTER_SLIP) {
                    self.handled = Some(i);
                    return;
                }
                let at = *self.next_press.get_or_insert_with(|| {
                    let react = rng.gen_range(BOT_HUNTER_REACT_MS);
                    now + Duration::from_secs_f64((start + react - ms).max(0.0) / 1000.0)
                });
                if now >= at {
                    game.duel_key(DuelKey::Action, now);
                    self.handled = Some(i);
                    self.next_press = None;
                }
            }
            (Board::Macros { problems, .. }, Phase::Play) => {
                let Some(mine) = game.macros_mine().filter(|m| !m.done) else { return };
                let step = (mine.solved, mine.errors as usize);
                if self.sent_step == Some(step) {
                    return;
                }
                let at = *self.next_press.get_or_insert_with(|| now + Duration::from_secs_f64(rng.gen_range(BOT_MACROS_SECS)));
                if now >= at {
                    let right = problems[mine.solved].answer();
                    let value = if rng.gen_bool(BOT_MACROS_SLIP) { right + 1 } else { right };
                    game.duel_key(DuelKey::Answer(value), now);
                    self.sent_step = Some(step);
                    self.next_press = None;
                }
            }
            (Board::Angle { target, mine, .. }, Phase::Play) if mine.len() < ANGLE_ATTEMPTS && due => {
                let pos = angle_pos(game.duel_elapsed(now));
                if (pos - target).abs() < rng.gen_range(0.01..0.09) {
                    game.duel_key(DuelKey::Action, now);
                    self.next_press = Some(now + Duration::from_millis(400));
                }
            }
            (_, Phase::Draft | Phase::Play) if due => self.act_more(game, &d, ms, now, rng),
            _ => {}
        }
    }

    /// The draft and the newer minigames; called when no press is pending.
    fn act_more(&mut self, game: &mut Game, d: &DuelView, ms: f64, now: Instant, rng: &mut impl Rng) {
        let wait = |rng: &mut dyn rand::RngCore, range: std::ops::Range<u64>| Some(now + Duration::from_millis(rng.gen_range(range)));
        let key = match &d.board {
            Board::Draft { picks, my_turn: true } => {
                if self.next_press.is_none() {
                    self.next_press = wait(rng, BOT_DRAFT_MS);
                    return;
                }
                let free: Vec<Minigame> = Minigame::ALL.into_iter().filter(|g| !picks.iter().any(|p| p.game == *g)).collect();
                let choice = if rng.gen_bool(0.2) { None } else { free.get(rng.gen_range(0..free.len())).copied() };
                game.outbox.push(crate::protocol::ClientMsg::Duel { key: DuelKey::Pick(choice), at_ms: None });
                self.next_press = wait(rng, 1_000..1_500);
                return;
            }
            Board::Jawline { target, mine, .. } if !mine.done => {
                let c = mine.cursor;
                let off = |i: usize| mine.heights[i] != target[i];
                if off(c) {
                    if mine.heights[c] < target[c] { DuelKey::Up } else { DuelKey::Down }
                } else if (c + 1..JAW_POINTS).any(off) {
                    DuelKey::Right
                } else {
                    DuelKey::Left
                }
            }
            Board::Posture { drift, mine, .. } => {
                let pos = drift.position(&mine.presses, ms);
                if pos > POSTURE_BAND * 0.5 {
                    DuelKey::Left
                } else if pos < -POSTURE_BAND * 0.5 {
                    DuelKey::Right
                } else {
                    return;
                }
            }
            Board::FitCheck { target, offsets, mine, .. } if !mine.done => {
                let piece = mine.locked.len();
                if fit_color(offsets, piece, ms - 120.0) != target[piece] && !rng.gen_bool(0.003) {
                    return;
                }
                DuelKey::Action
            }
            Board::Hydration { target, mine, .. } if !mine.locked => {
                let level = hydration_level(mine.level, mine.at, ms);
                if level <= target - HYDRATION_SIP_BIG {
                    DuelKey::Up
                } else if level < *target {
                    DuelKey::Right
                } else {
                    DuelKey::Action
                }
            }
            Board::Photo { shots, mine, .. } => {
                let Some(i) = (0..shots.len()).find(|i| mine[*i].is_none() && ms <= photo_flash(shots[*i]) + PHOTO_LATE_MS) else { return };
                let at = *self.next_press.get_or_insert_with(|| {
                    let ahead = photo_flash(shots[i]) - ms + rng.gen_range(-90.0..90.0);
                    now + Duration::from_secs_f64(ahead.max(0.0) / 1000.0)
                });
                if now < at || self.handled == Some(i) {
                    return;
                }
                self.handled = Some(i);
                game.duel_key(DuelKey::Action, now);
                self.next_press = None;
                return;
            }
            Board::Status { items, mine, .. } if mine.len() < items.len() => {
                let cost = |name: &String| CATALOG.iter().find(|d| d.name == name).map_or(0.0, |d| d.base_cost);
                let mut order: Vec<usize> = (0..items.len()).filter(|i| !mine.contains(i)).collect();
                order.sort_by(|a, b| cost(&items[*b]).total_cmp(&cost(&items[*a])));
                let i = if rng.gen_bool(0.15) && order.len() > 1 { order[1] } else { order[0] };
                self.next_press = wait(rng, 600..1_400);
                DuelKey::Char(char::from(b'1' + i as u8))
            }
            Board::Hair { strands, mine, .. } => {
                let Some(i) = strands.iter().position(|s| s.showing(ms)) else { return };
                if mine.answered.get(i).copied().unwrap_or(true) || self.handled == Some(i) {
                    return;
                }
                let at = *self.next_press.get_or_insert_with(|| {
                    now + Duration::from_secs_f64((strands[i].at + rng.gen_range(BOT_HUNTER_REACT_MS) - ms).max(0.0) / 1000.0)
                });
                if now < at {
                    return;
                }
                self.handled = Some(i);
                self.next_press = None;
                let right = strands[i].dir;
                let key = if rng.gen_bool(BOT_HUNTER_SLIP) { *DuelKey::ARROWS.iter().find(|k| **k != right).expect("arrows") } else { right };
                game.duel_key(key, now);
                return;
            }
            Board::Brow { trials, mine, .. } => {
                let Some(i) = trials.iter().position(|tr: &BrowTrial| ms <= tr.end()) else { return };
                if mine[i].is_some() || ms < trials[i].raise || self.handled == Some(i) {
                    return;
                }
                let at = *self.next_press.get_or_insert_with(|| {
                    now + Duration::from_secs_f64((trials[i].raise + rng.gen_range(BOT_HUNTER_REACT_MS) - ms).max(0.0) / 1000.0)
                });
                if now < at {
                    return;
                }
                self.handled = Some(i);
                self.next_press = None;
                game.duel_key(DuelKey::Action, now);
                return;
            }
            Board::ReverseMog { mine, .. } => {
                let (i, open) = rps_throw(ms);
                if !open || mine.get(i).is_none_or(Option::is_some) {
                    return;
                }
                if self.handled != Some(i) {
                    self.handled = Some(i);
                    self.next_press = wait(rng, 300..2_000);
                    return;
                }
                [Stance::Mog, Stance::Defend, Stance::Ignore]
                    .map(|s| match s {
                        Stance::Mog => DuelKey::Left,
                        Stance::Defend => DuelKey::Down,
                        Stance::Ignore => DuelKey::Right,
                    })[rng.gen_range(0..3)]
            }
            _ => return,
        };
        game.duel_key(key, now);
        if self.next_press.is_none_or(|t| t <= now) {
            self.next_press = wait(rng, 130..280);
        }
    }
}

fn echo_feed(game: &Game, printed: usize) -> usize {
    let total = game.total_logged();
    let fresh = total.saturating_sub(printed).min(game.feed.len());
    for line in game.feed.iter().take(fresh).rev() {
        println!("[{}] {}", game.player.name, line.text);
    }
    total
}
