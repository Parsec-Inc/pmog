//! PvP duel rules: the classic minigames, the draft, six rounds with sudden death and the World around it (escrow, invites, queue).

use std::time::{Duration, Instant};

use pmog::constants::*;
use pmog::duel::{Board, Duel, DuelKey, DuelView, Minigame, Phase, angle_pos, rhythm_beat_ms};
use pmog::game::Player;
use pmog::protocol::{NPC_TARGET, PlayerId, ServerMsg};
use pmog::world::World;
use rand::SeedableRng;
use rand::rngs::StdRng;

const STEP: Duration = Duration::from_millis(10);

fn rng() -> StdRng {
    StdRng::seed_from_u64(7)
}

fn duel(games: &[Minigame], now: Instant) -> Duel {
    Duel::with_games([1, 2], ["Alfa".into(), "Beta".into()], 100.0, games, now, &mut rng())
}

/// Ticks in small steps until `done` holds; returns the time reached.
fn run_until(d: &mut Duel, mut now: Instant, done: impl Fn(&Duel, Instant) -> bool) -> Instant {
    let mut r = rng();
    for _ in 0..10_000 {
        if done(d, now) {
            return now;
        }
        now += STEP;
        d.tick(now, &mut r);
    }
    panic!("condition never reached; phase {:?}", d.phase);
}

fn to_play(d: &mut Duel, now: Instant) -> Instant {
    run_until(d, now, |d, _| d.phase == Phase::Play)
}

fn board(d: &Duel, side: usize, now: Instant) -> Board {
    d.view(side, 0.0, now).board
}

fn last_winner(d: &Duel) -> Option<usize> {
    d.last.as_ref().expect("round finished").winner
}

#[test]
fn staredown_false_start_loses() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Staredown], now);
    let t = to_play(&mut d, now);
    assert!(matches!(board(&d, 0, t), Board::Staredown { go: false, .. }));
    d.input(1, DuelKey::Action, None, 0.0, t, &mut rng());
    assert_eq!(d.phase, Phase::RoundOver);
    assert_eq!(last_winner(&d), Some(0));
    assert!(d.last.as_ref().unwrap().text.contains("Beta piscou"));
}

#[test]
fn staredown_first_press_after_go_wins() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Staredown], now);
    let t = to_play(&mut d, now);
    let t = run_until(&mut d, t, |d, t| matches!(board(d, 0, t), Board::Staredown { go: true, .. }));
    d.input(0, DuelKey::Left, None, 0.0, t, &mut rng());
    assert_eq!(d.phase, Phase::Play, "only ESPAÇO counts");
    d.input(1, DuelKey::Action, None, 0.0, t + Duration::from_millis(200), &mut rng());
    d.input(0, DuelKey::Action, None, 0.0, t + Duration::from_millis(250), &mut rng());
    assert_eq!(last_winner(&d), Some(1));
    assert_eq!(d.wins, [0, 1]);
}

#[test]
fn staredown_without_reaction_is_a_draw() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Staredown], now);
    let t = to_play(&mut d, now);
    run_until(&mut d, t, |d, _| d.phase == Phase::RoundOver);
    assert_eq!(last_winner(&d), None);
    assert_eq!(d.wins, [0, 0]);
}

fn routine(d: &Duel, now: Instant) -> Vec<DuelKey> {
    match board(d, 0, now) {
        Board::Skincare { sequence, .. } => sequence,
        other => panic!("not skincare: {other:?}"),
    }
}

fn wrong(key: DuelKey) -> DuelKey {
    if key == DuelKey::Up { DuelKey::Down } else { DuelKey::Up }
}

#[test]
fn skincare_grows_until_someone_slips() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Skincare], now);
    let t = run_until(&mut d, now, |d, _| d.phase == Phase::Show);
    let seq = routine(&d, t);
    assert_eq!(seq.len(), SKINCARE_START_LEN);
    let t = to_play(&mut d, t);
    assert!(routine(&d, t).is_empty(), "routine is hidden while typing");
    d.input(0, DuelKey::Up, None, 0.0, t, &mut rng());
    assert!(matches!(board(&d, 0, t), Board::Skincare { mine, .. } if mine.steps <= 1));

    // Restart cleanly: type the whole routine for both players.
    let mut d = duel(&[Minigame::Skincare], now);
    let t = run_until(&mut d, now, |d, _| d.phase == Phase::Show);
    let seq = routine(&d, t);
    let t = to_play(&mut d, t);
    for key in &seq {
        d.input(0, *key, None, 0.0, t, &mut rng());
        d.input(1, *key, None, 0.0, t, &mut rng());
    }
    assert_eq!(d.phase, Phase::Show, "both passed: next level");
    let longer = routine(&d, t);
    assert_eq!(longer.len(), SKINCARE_START_LEN + 1);
    assert_eq!(&longer[..seq.len()], &seq[..], "the routine keeps its old steps");

    let t = to_play(&mut d, t);
    for (i, key) in longer.iter().enumerate() {
        d.input(0, *key, None, 0.0, t, &mut rng());
        d.input(1, if i == 2 { wrong(*key) } else { *key }, None, 0.0, t, &mut rng());
    }
    assert_eq!(d.phase, Phase::RoundOver);
    assert_eq!(last_winner(&d), Some(0));
}

#[test]
fn skincare_both_slip_farthest_wins() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Skincare], now);
    let t = run_until(&mut d, now, |d, _| d.phase == Phase::Show);
    let seq = routine(&d, t);
    let t = to_play(&mut d, t);
    d.input(0, wrong(seq[0]), None, 0.0, t, &mut rng());
    d.input(1, seq[0], None, 0.0, t, &mut rng());
    assert_eq!(d.phase, Phase::Play, "Beta is still typing");
    d.input(1, wrong(seq[1]), None, 0.0, t, &mut rng());
    assert_eq!(last_winner(&d), Some(1));
}

#[test]
fn skincare_timeout_counts_progress() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Skincare], now);
    let t = run_until(&mut d, now, |d, _| d.phase == Phase::Show);
    let seq = routine(&d, t);
    let t = to_play(&mut d, t);
    d.input(0, seq[0], None, 0.0, t, &mut rng());
    run_until(&mut d, t, |d, _| d.phase == Phase::RoundOver);
    assert_eq!(last_winner(&d), Some(0));
}

#[test]
fn mewing_counts_only_alternation_at_human_speed() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Mewing], now);
    let mut t = to_play(&mut d, now);
    let start = t;
    let keys = [DuelKey::Left, DuelKey::Right];
    for i in 0..20 {
        d.input(0, keys[i % 2], None, 0.0, t, &mut rng());
        d.input(1, DuelKey::Left, None, 0.0, t, &mut rng()); // holding one key
        d.input(1, DuelKey::Up, None, 0.0, t, &mut rng());
        t += Duration::from_millis(60);
    }
    for i in 0..10 {
        d.input(1, keys[i % 2], None, 0.0, start + Duration::from_millis(i as u64), &mut rng()); // macro speed
    }
    match board(&d, 0, t) {
        Board::Mewing { mine, theirs } => {
            assert_eq!(mine, 20);
            assert_eq!(theirs, 1, "same key and inhuman speed don't count");
        }
        other => panic!("{other:?}"),
    }
    run_until(&mut d, t, |d, _| d.phase == Phase::RoundOver);
    assert_eq!(last_winner(&d), Some(0));
    assert!(d.last.as_ref().unwrap().text.contains("20 x 1"));
}

#[test]
fn angle_best_of_three_and_early_finish() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Angle], now);
    let t = to_play(&mut d, now);
    let target = match board(&d, 0, t) {
        Board::Angle { target, .. } => target,
        other => panic!("{other:?}"),
    };
    assert!(ANGLE_TARGET.contains(&target));
    let period = 2.0 / ANGLE_SPEED;
    let on_target = target / ANGLE_SPEED;
    for k in 0..ANGLE_ATTEMPTS {
        let base = k as f64 * period;
        d.input(1, DuelKey::Action, None, 0.0, t + Duration::from_secs_f64(base + 0.0001), &mut rng());
        let hit = t + Duration::from_secs_f64(base + if k == 1 { on_target } else { 0.0 });
        d.input(0, DuelKey::Action, None, 0.0, hit, &mut rng());
    }
    assert!((angle_pos(on_target) - target).abs() < 1e-6);
    assert_eq!(d.phase, Phase::RoundOver, "ends as soon as everyone used the attempts");
    assert_eq!(last_winner(&d), Some(0), "the single good attempt is what counts");
    d.input(0, DuelKey::Action, None, 0.0, t, &mut rng());
}

#[test]
fn angle_without_attempts_is_a_draw() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Angle], now);
    let t = to_play(&mut d, now);
    run_until(&mut d, t, |d, _| d.phase == Phase::RoundOver);
    assert_eq!(last_winner(&d), None);
}

/// Makes `side` win whatever round is being played.
fn win_round(d: &mut Duel, side: usize, now: Instant) -> Instant {
    let other = 1 - side;
    let mut r = rng();
    let t = run_until(d, now, |d, _| matches!(d.phase, Phase::Play | Phase::Show));
    match d.game {
        Minigame::Staredown => d.input(other, DuelKey::Action, None, 0.0, t, &mut r),
        Minigame::Skincare => {
            let seq = routine(d, t);
            let t = to_play(d, t);
            d.input(side, seq[0], None, 0.0, t, &mut r);
            d.input(other, wrong(seq[0]), None, 0.0, t, &mut r);
            run_until(d, t, |d, _| d.phase == Phase::RoundOver);
        }
        Minigame::Mewing => {
            d.input(side, DuelKey::Left, None, 0.0, t, &mut r);
            run_until(d, t, |d, _| d.phase == Phase::RoundOver);
        }
        Minigame::Angle => {
            d.input(side, DuelKey::Action, None, 0.0, t + Duration::from_millis(150), &mut r);
            run_until(d, t, |d, _| d.phase == Phase::RoundOver);
        }
        Minigame::Typing => {
            let first = phrase(d, t).chars().next().unwrap();
            let t = to_play(d, t);
            d.input(side, DuelKey::Char(first), None, 0.0, t + Duration::from_millis(300), &mut r);
            run_until(d, t, |d, _| d.phase == Phase::RoundOver);
        }
        Minigame::Rhythm => {
            d.input(side, DuelKey::Action, None, 0.0, t + Duration::from_secs_f64(rhythm_beat_ms(0) / 1000.0), &mut r);
            run_until(d, t, |d, _| d.phase == Phase::RoundOver);
        }
        Minigame::Hunter => {
            let Board::Hunter { schedule, .. } = d.view(side, 0.0, t).board else { unreachable!() };
            let (at, _) = schedule.iter().find(|(_, target)| *target).copied().unwrap();
            d.input(side, DuelKey::Action, None, 0.0, t + Duration::from_secs_f64((at + 300.0) / 1000.0), &mut r);
            run_until(d, t, |d, _| d.phase == Phase::RoundOver);
        }
        Minigame::Macros => {
            let Board::Macros { problems, .. } = d.view(side, 0.0, t).board else { unreachable!() };
            let t = to_play(d, t);
            d.input(side, DuelKey::Answer(problems[0].answer()), None, 0.0, t + Duration::from_millis(500), &mut r);
            run_until(d, t, |d, _| d.phase == Phase::RoundOver);
        }
        other => panic!("no script to win {other:?}"),
    }
    assert_eq!(last_winner(d), Some(side), "{:?}", d.game);
    run_until(d, t, |d, _| d.phase != Phase::RoundOver)
}

#[test]
fn four_straight_wins_close_the_six_rounds_early() {
    let now = Instant::now();
    let games = [Minigame::Mewing, Minigame::Angle, Minigame::Staredown, Minigame::Mewing, Minigame::Angle, Minigame::Mewing];
    let mut d = duel(&games, now);
    let t = win_round(&mut d, 1, now);
    assert_eq!(d.game, Minigame::Angle, "rounds follow the given order");
    let mut t = t;
    for _ in 0..3 {
        assert_eq!(d.outcome, None);
        t = win_round(&mut d, 0, t);
    }
    assert_eq!(d.outcome, None, "3 x 1 with 2 rounds left is still open");
    let t = win_round(&mut d, 0, t);
    assert_eq!(d.phase, Phase::Finished);
    assert_eq!(d.outcome, Some(Some(0)), "4 x 1 with one round left can't be caught");
    assert_eq!(d.rounds, 5);
    assert_eq!(d.take_settlement(), Some(Some(0)));
    assert_eq!(d.take_settlement(), None, "stakes move only once");

    let view: DuelView = d.view(1, 500.0, t);
    assert_eq!((view.my_wins, view.their_wins), (1, 4));
    assert_eq!(view.i_won, Some(Some(false)));
    assert_eq!(view.opponent, "Alfa");
    assert!(!d.tick(t, &mut rng()));
    assert!(d.tick(t + DUEL_RESULT_SHOW, &mut rng()), "removed after the result screen");
}

#[test]
fn five_draws_end_in_a_draw() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Angle; 5], now);
    let mut t = now;
    for _ in 0..DUEL_MAX_ROUNDS {
        t = run_until(&mut d, t, |d, _| d.phase == Phase::RoundOver);
        t = run_until(&mut d, t, |d, _| d.phase != Phase::RoundOver);
    }
    assert_eq!(d.phase, Phase::Finished);
    assert_eq!(d.outcome, Some(None));
}

#[test]
fn forfeit_ends_at_once() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Mewing], now);
    d.forfeit(0, now);
    assert_eq!(d.outcome, Some(Some(1)));
    assert!(d.last.as_ref().unwrap().text.contains("Alfa desistiu"));
    d.forfeit(1, now);
    assert_eq!(d.outcome, Some(Some(1)), "a decided duel can't change");
}

#[test]
fn input_outside_play_is_ignored() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Staredown], now);
    d.input(0, DuelKey::Action, None, 0.0, now, &mut rng());
    assert_eq!(d.phase, Phase::Countdown);
    d.input(5, DuelKey::Action, None, 0.0, now, &mut rng());
}

// ---------- World: invites, queue, escrow ----------

fn world(n: i64, mp: f64, now: Instant) -> World {
    let mut w = World::new();
    for id in 1..=n {
        let player = Player { name: format!("p{id}"), mp, lifetime: mp + id as f64, ..Default::default() };
        w.join(id, format!("m{id}"), player, now).unwrap();
    }
    w.outbox.clear();
    w
}

fn mp(w: &World, id: PlayerId) -> f64 {
    w.slot(id).unwrap().player.mp
}

fn texts(w: &World, id: PlayerId) -> Vec<String> {
    w.outbox
        .iter()
        .filter(|(to, _)| *to == id)
        .filter_map(|(_, m)| match m {
            ServerMsg::Event { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn challenge_accept_escrows_both_bets() {
    let now = Instant::now();
    let mut w = world(3, 200.0, now);
    w.slot_mut(2).unwrap().player.mp = 80.0;
    w.challenge(1, 2, 0.5, now);
    let view = w.view(2, now).unwrap();
    assert_eq!(view.invites.len(), 1);
    assert_eq!(view.invites[0].stake, 40.0, "half of the poorer player's MP");
    assert_eq!(w.view(1, now).unwrap().outgoing.unwrap().name, "p2");
    assert!(texts(&w, 2).iter().any(|t| t.contains("te desafiou")));

    w.answer(2, 1, true, now);
    assert!(w.in_duel(1) && w.in_duel(2) && !w.in_duel(3));
    assert_eq!(mp(&w, 1), 160.0);
    assert_eq!(mp(&w, 2), 40.0);
    assert!(w.view(1, now).unwrap().duel.is_some());
    assert!(w.view(3, now).unwrap().players.iter().any(|p| p.id == 1 && p.in_duel));

    w.mog(3, 1, 0.5, now);
    assert!(texts(&w, 3).iter().any(|t| t.contains("está num duelo")));
    w.mog(1, NPC_TARGET, 0.5, now);
    assert!(texts(&w, 1).iter().any(|t| t.contains("Termine o duelo")));
    w.challenge(3, 2, 0.1, now);
    assert!(texts(&w, 3).iter().any(|t| t.contains("já está num duelo")));
}

#[test]
fn winner_takes_the_pot_and_it_is_logged() {
    let now = Instant::now();
    let mut w = world(2, 100.0, now);
    let lifetime_before = w.slot(2).unwrap().player.lifetime;
    w.challenge(1, 2, 0.25, now);
    w.answer(2, 1, true, now);
    w.forfeit(1, now);
    assert_eq!(mp(&w, 1), 75.0);
    assert_eq!(mp(&w, 2), 125.0);
    assert_eq!(w.slot(2).unwrap().player.lifetime, lifetime_before, "winning a duel is a transfer, not earned MP");
    assert_eq!(w.slot(2).unwrap().player.duels_won, 0, "walkover before any round does not count");
    assert_eq!(w.slot(1).unwrap().player.duels_lost, 0);
    assert_eq!(w.matches.len(), 1);
    assert_eq!(w.matches[0].winner, Some(2));
    assert!(!w.matches[0].ranked);
    assert!(texts(&w, 2).iter().any(|t| t.contains("Não conta para o placar")));
    assert!(texts(&w, 2).iter().any(|t| t.contains("venceu o duelo")));

    // The result screen keeps them busy until it goes away.
    assert!(w.in_duel(1));
    w.tick(now + DUEL_RESULT_SHOW + STEP, 0.0);
    assert!(!w.in_duel(1) && w.duel_count() == 0);
    w.tick(now + DUEL_RESULT_SHOW * 2, 0.0);
    assert!(!w.slot(2).unwrap().player.achievements.contains(&pmog::game::Achievement::FirstDuel), "a walkover gives no achievement");
}

#[test]
fn a_full_duel_played_through_the_world() {
    let now = Instant::now();
    let mut w = world(2, 100.0, now);
    w.challenge(1, 2, 1.0, now);
    w.answer(2, 1, true, now);
    assert_eq!(mp(&w, 1) + mp(&w, 2), 0.0, "everything is in escrow");
    let mut t = now;
    let mut guard = 0;
    // Player 1 plays well, player 2 does nothing.
    while w.view(1, t).unwrap().duel.is_some_and(|d| d.i_won.is_none()) {
        guard += 1;
        assert!(guard < 100_000, "duel never ended");
        t += STEP;
        w.tick(t, 0.0);
        let d = w.view(1, t).unwrap().duel.unwrap();
        // Player 1 drafts the games it knows; player 2 lets the clock pick.
        if let Board::Draft { ref picks, my_turn: true } = d.board {
            let mine = [Minigame::Staredown, Minigame::Mewing, Minigame::Angle];
            let free = mine.into_iter().find(|g| !picks.iter().any(|p| p.game == *g));
            w.duel_input(1, DuelKey::Pick(free), None, 0.0, t);
        }
        if d.phase != Phase::Play {
            continue;
        }
        match d.board {
            Board::Staredown { go: true, i_pressed: false, go_ms, .. } if d.phase_ms >= go_ms + 250 => {
                w.duel_input(1, DuelKey::Action, None, 0.0, t)
            }
            Board::Mewing { mine: 0, .. } => w.duel_input(1, DuelKey::Left, None, 0.0, t),
            Board::Angle { ref mine, .. } if mine.is_empty() => w.duel_input(1, DuelKey::Action, None, 0.0, t),
            _ => {}
        }
    }
    let d = w.view(1, t).unwrap().duel.unwrap();
    let total = mp(&w, 1) + mp(&w, 2);
    assert!((total - 200.0).abs() < 1e-9, "MP is conserved");
    match d.i_won.unwrap() {
        Some(true) => assert_eq!(mp(&w, 1), 200.0),
        None => assert_eq!(mp(&w, 1), 100.0),
        Some(false) => panic!("the idle player can't win"),
    }
    assert_eq!(w.matches.len(), 1);
}

#[test]
fn decline_cancel_and_expiry() {
    let now = Instant::now();
    let mut w = world(3, 100.0, now);
    w.challenge(1, 2, 0.1, now);
    w.answer(2, 1, false, now);
    assert!(texts(&w, 1).iter().any(|t| t.contains("recusou")));
    assert!(w.view(1, now).unwrap().outgoing.is_none());

    w.challenge(1, 2, 0.1, now);
    w.challenge(1, 3, 0.1, now);
    assert!(w.view(2, now).unwrap().invites.is_empty(), "a new challenge replaces the old one");
    assert_eq!(w.view(3, now).unwrap().invites.len(), 1);
    w.cancel_challenge(1);
    assert!(w.view(3, now).unwrap().invites.is_empty());

    w.challenge(1, 2, 0.1, now);
    w.tick(now + INVITE_TTL, 0.0);
    assert!(w.view(2, now).unwrap().invites.is_empty());
    assert!(texts(&w, 2).iter().any(|t| t.contains("expirou")));
    w.answer(2, 1, true, now);
    assert!(!w.in_duel(1), "an expired invite can't be accepted");
}

#[test]
fn bad_challenges_are_refused() {
    let now = Instant::now();
    let mut w = world(2, 100.0, now);
    w.challenge(1, 1, 0.1, now);
    w.challenge(1, NPC_TARGET, 0.1, now);
    w.challenge(1, 99, 0.1, now);
    w.challenge(1, 2, 0.33, now);
    w.challenge(1, 2, f64::NAN, now);
    assert!(w.view(2, now).unwrap().invites.is_empty());
    assert!(texts(&w, 1).iter().any(|t| t.contains("Aposta inválida")));
}

#[test]
fn queue_pairs_close_ranks_first_then_anyone() {
    let now = Instant::now();
    let mut w = world(4, 100.0, now);
    w.slot_mut(1).unwrap().player.lifetime = 0.0; // Sub-5
    w.slot_mut(2).unwrap().player.lifetime = 1e9; // Ascendido
    w.slot_mut(3).unwrap().player.lifetime = 200.0; // LTN
    w.queue(1, Some(0.5), now);
    assert_eq!(w.view(1, now).unwrap().queued, Some(0.5));
    w.queue(2, Some(0.5), now);
    assert!(!w.in_duel(1) && !w.in_duel(2), "ranks too far apart: keep waiting");
    w.queue(3, Some(0.25), now);
    assert!(w.in_duel(1) && w.in_duel(3), "one rank apart: paired at once");
    assert!(!w.in_duel(2));
    assert_eq!(mp(&w, 1), 75.0, "the smaller bet is used");

    w.queue(4, Some(0.1), now);
    assert!(!w.in_duel(4));
    w.tick(now + QUEUE_RELAX, 0.0);
    assert!(w.in_duel(2) && w.in_duel(4), "after waiting, anyone will do");
}

#[test]
fn leaving_the_queue() {
    let now = Instant::now();
    let mut w = world(2, 100.0, now);
    w.queue(1, Some(0.1), now);
    w.queue(1, None, now);
    assert!(w.view(1, now).unwrap().queued.is_none());
    assert!(texts(&w, 1).iter().any(|t| t.contains("saiu da fila")));
    w.queue(2, Some(0.1), now);
    assert!(!w.in_duel(2));
}

#[test]
fn leaving_mid_duel_is_a_forfeit_and_shutdown_refunds() {
    let now = Instant::now();
    let mut w = world(4, 100.0, now);
    w.challenge(1, 2, 1.0, now);
    w.answer(2, 1, true, now);
    let now = play_one_round(&mut w, now);
    let profile = w.leave(1, now).unwrap();
    assert_eq!(profile.mp, 0.0);
    assert_eq!(profile.duels_lost, 1, "a round was played, so leaving is a counted loss");
    assert_eq!(mp(&w, 2), 200.0);

    w.challenge(3, 4, 0.5, now);
    w.answer(4, 3, true, now);
    assert_eq!(mp(&w, 3), 50.0);
    let logged = w.matches.len();
    w.abort_duels();
    assert_eq!(w.matches.len(), logged, "a called-off duel is not history");
    assert_eq!(mp(&w, 3), 100.0);
    assert_eq!(mp(&w, 4), 100.0);
    assert!(!w.in_duel(3));
}

#[test]
fn leaving_cancels_invites_and_queue() {
    let now = Instant::now();
    let mut w = world(3, 100.0, now);
    w.challenge(1, 2, 0.1, now);
    w.queue(3, Some(0.1), now);
    w.leave(1, now);
    w.leave(3, now);
    assert!(w.view(2, now).unwrap().invites.is_empty());
    assert!(texts(&w, 2).iter().any(|t| t.contains("saiu do servidor")));
    w.queue(2, Some(0.1), now);
    assert!(!w.in_duel(2), "nobody left in the queue to pair with");
}

#[test]
fn friendly_duel_moves_no_mp() {
    let now = Instant::now();
    let mut w = world(2, 100.0, now);
    w.challenge(1, 2, 0.0, now);
    w.answer(2, 1, true, now);
    let now = play_one_round(&mut w, now);
    w.forfeit(2, now);
    assert_eq!((mp(&w, 1), mp(&w, 2)), (100.0, 100.0));
    assert_eq!(w.slot(1).unwrap().player.duels_won, 0, "friendly duels never count");
    assert!(!w.matches[0].ranked);
}

/// Lets the first duel's opening round run out with nobody pressing anything.
fn play_one_round(w: &mut World, mut now: Instant) -> Instant {
    while w.duels()[0].played == 0 {
        now += STEP;
        w.tick(now, STEP.as_secs_f64());
    }
    now
}

#[test]
fn ranked_duel_after_a_round_counts() {
    let now = Instant::now();
    let mut w = world(2, 100.0, now);
    w.challenge(1, 2, 0.25, now);
    w.answer(2, 1, true, now);
    let now = play_one_round(&mut w, now);
    w.forfeit(1, now);
    assert_eq!(w.slot(2).unwrap().player.duels_won, 1);
    assert_eq!(w.slot(1).unwrap().player.duels_lost, 1);
    assert!(w.matches[0].ranked);
    w.tick(now + DUEL_RESULT_SHOW * 2, 0.0);
    assert!(w.slot(2).unwrap().player.achievements.contains(&pmog::game::Achievement::FirstDuel));
}

#[test]
fn friendly_duel_messages_have_no_mp() {
    let now = Instant::now();
    let mut w = world(2, 100.0, now);
    w.challenge(1, 2, 0.0, now);
    w.answer(2, 1, true, now);
    w.outbox.clear();
    w.forfeit(1, now);
    let all: Vec<String> = texts(&w, 1).into_iter().chain(texts(&w, 2)).collect();
    assert!(all.iter().all(|t| !t.contains("MP")), "{all:?}");
    assert!(all.iter().any(|t| t.contains("amistoso")));
}

#[test]
fn mewing_tie_goes_to_who_got_there_first() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Mewing], now);
    let t = to_play(&mut d, now);
    for (i, key) in [DuelKey::Left, DuelKey::Right, DuelKey::Left].into_iter().enumerate() {
        let at = t + Duration::from_millis(100 * i as u64);
        d.input(1, key, None, 0.0, at, &mut rng());
        d.input(0, key, None, 0.0, at + Duration::from_millis(5), &mut rng());
    }
    run_until(&mut d, t, |d, _| d.phase == Phase::RoundOver);
    assert_eq!(last_winner(&d), Some(1));
    assert!(d.last.as_ref().unwrap().text.contains("chegou lá primeiro"));
}

#[test]
fn angle_tie_falls_to_the_other_attempts() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Angle], now);
    let t = to_play(&mut d, now);
    let period = 2.0 / ANGLE_SPEED;
    // Same first press for both; then side 1 lands closer on its second try.
    d.input(0, DuelKey::Action, None, 0.0, t, &mut rng());
    d.input(1, DuelKey::Action, None, 0.0, t, &mut rng());
    d.input(0, DuelKey::Action, None, 0.0, t + Duration::from_secs_f64(period), &mut rng());
    d.input(1, DuelKey::Action, None, 0.0, t + Duration::from_secs_f64(period + 0.05), &mut rng());
    run_until(&mut d, t, |d, _| d.phase == Phase::RoundOver);
    let target = match &d.view(0, 0.0, t).board {
        Board::Angle { target, .. } => *target,
        _ => unreachable!(),
    };
    let expected = if (angle_pos(0.05) - target).abs() < target { 1 } else { 0 };
    assert_eq!(last_winner(&d), Some(expected));
}

#[test]
fn friendly_results_are_not_broadcast() {
    let now = Instant::now();
    let mut w = world(3, 100.0, now);
    w.challenge(1, 2, 0.0, now);
    w.answer(2, 1, true, now);
    w.outbox.clear();
    w.forfeit(1, now);
    assert!(texts(&w, 3).is_empty());
    w.tick(now + DUEL_RESULT_SHOW * 2, 0.0);
    w.slot_mut(1).unwrap().player.mp = 1000.0;
    w.slot_mut(2).unwrap().player.mp = 1000.0;
    w.challenge(1, 2, 0.5, now);
    w.answer(2, 1, true, now);
    w.forfeit(1, now);
    assert!(texts(&w, 3).iter().any(|t| t.contains("num duelo valendo")), "big bets are announced");
}

// ---------- Latency compensation ----------

/// Staredown in Play with its GO time, in ms after the phase started at `t`.
fn staredown_play(now: Instant) -> (Duel, Instant, f64) {
    let mut d = duel(&[Minigame::Staredown], now);
    let t = to_play(&mut d, now);
    let Board::Staredown { go_ms, .. } = board(&d, 0, t) else { panic!("staredown") };
    (d, t, go_ms as f64)
}

fn at(t: Instant, ms: f64) -> Instant {
    t + Duration::from_secs_f64(ms / 1000.0)
}

#[test]
fn client_timing_is_clamped_to_what_the_server_saw() {
    let now = Instant::now();
    let (d, t, _) = staredown_play(now);
    let arrival = at(t, 500.0);
    assert_eq!(d.press_time(None, arrival, 20.0).round(), 500.0, "no stamp: arrival time");
    assert_eq!(d.press_time(Some(480.0), arrival, 20.0).round(), 480.0, "honest stamp kept");
    assert_eq!(d.press_time(Some(-1e6), arrival, 20.0).round(), 500.0 - 20.0 - CLIENT_TIME_SLACK_MS, "cannot claim the past");
    assert_eq!(d.press_time(Some(1e6), arrival, 20.0).round(), 500.0 + CLIENT_TIME_AHEAD_MS, "cannot claim the future");
    assert_eq!(d.press_time(Some(f64::NAN), arrival, 20.0).round(), 500.0, "garbage ignored");
}

#[test]
fn staredown_lagged_player_with_faster_reaction_still_wins() {
    let now = Instant::now();
    let (mut d, t, go) = staredown_play(now);
    // Beta is on the server's machine: 230 ms reaction, arrives at once.
    d.input(1, DuelKey::Action, Some(go + 230.0), 1.0, at(t, go + 231.0), &mut rng());
    assert_eq!(d.phase, Phase::Play, "waits for a faster press still on the wire");
    // Alfa reacted in 200 ms but has 80 ms of round trip.
    d.input(0, DuelKey::Action, Some(go + 200.0), 80.0, at(t, go + 240.0), &mut rng());
    assert_eq!(last_winner(&d), Some(0));
    let text = &d.last.as_ref().unwrap().text;
    assert!(text.contains("Alfa reagiu primeiro") && text.contains(" ms x "), "{text}");
}

#[test]
fn staredown_lying_about_time_does_not_help() {
    let now = Instant::now();
    let (mut d, t, go) = staredown_play(now);
    d.input(1, DuelKey::Action, Some(go + 250.0), 5.0, at(t, go + 252.0), &mut rng());
    // Alfa's packet arrives at +300 claiming +120; with a 10 ms ping it is clamped to +265.
    d.input(0, DuelKey::Action, Some(go + 120.0), 10.0, at(t, go + 300.0), &mut rng());
    assert_eq!(last_winner(&d), Some(1));
}

#[test]
fn staredown_inhumanly_fast_counts_as_guessing() {
    let now = Instant::now();
    let (mut d, t, go) = staredown_play(now);
    d.input(0, DuelKey::Action, Some(go + 40.0), 0.0, at(t, go + 40.0), &mut rng());
    assert_eq!(last_winner(&d), Some(1));
    assert!(d.last.as_ref().unwrap().text.contains("chutou"));
}

#[test]
fn staredown_single_press_is_decided_after_the_slack() {
    let now = Instant::now();
    let (mut d, t, go) = staredown_play(now);
    let pressed = at(t, go + 300.0);
    d.input(0, DuelKey::Action, None, 0.0, pressed, &mut rng());
    d.tick(pressed + STAREDOWN_DECIDE_SLACK / 2, &mut rng());
    assert_eq!(d.phase, Phase::Play);
    d.tick(pressed + STAREDOWN_DECIDE_SLACK, &mut rng());
    assert_eq!(last_winner(&d), Some(0));
}

#[test]
fn late_packets_count_only_if_pressed_in_time() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Mewing], now);
    let t = to_play(&mut d, now);
    let len = MEWING_DURATION.as_secs_f64() * 1000.0;
    let arrival = at(t, len + 100.0);
    d.tick(arrival, &mut rng());
    assert_eq!(d.phase, Phase::Play, "grace period keeps the phase open");
    d.input(0, DuelKey::Left, Some(len - 50.0), 200.0, arrival, &mut rng());
    d.input(1, DuelKey::Left, Some(len + 50.0), 200.0, arrival, &mut rng());
    assert!(matches!(board(&d, 0, arrival), Board::Mewing { mine: 1, theirs: 0 }));
    d.tick(t + MEWING_DURATION + INPUT_GRACE, &mut rng());
    assert_eq!(last_winner(&d), Some(0));
}

// ---------- Typing battle ----------

fn phrase(d: &Duel, now: Instant) -> String {
    let Board::Typing { text, .. } = board(d, 0, now) else { panic!("typing board") };
    text
}

/// Types `text` for `side` at `cps` characters per second starting at `start`, with a wrong key before every `typo_every`-th char.
fn type_out(d: &mut Duel, side: usize, text: &str, start: Instant, cps: f64, typo_every: Option<usize>) {
    let mut t = start;
    for (i, c) in text.chars().enumerate() {
        if typo_every.is_some_and(|n| i % n == n - 1) {
            d.input(side, DuelKey::Char('#'), None, 0.0, t, &mut rng());
            t += Duration::from_secs_f64(1.0 / cps);
        }
        d.input(side, DuelKey::Char(c), None, 0.0, t, &mut rng());
        t += Duration::from_secs_f64(1.0 / cps);
    }
}

#[test]
fn typing_phrase_is_shown_first_and_themed() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Typing], now);
    let t = run_until(&mut d, now, |d, _| d.phase == Phase::Show);
    let text = phrase(&d, t);
    assert_eq!(text.split(' ').count(), TYPING_WORD_COUNT);
    assert!(text.split(' ').all(|w| pmog::duel::TYPING_WORDS.contains(&w)), "{text}");
    assert!(text.is_ascii(), "no accents to fight the keyboard");
    d.input(0, DuelKey::Char(text.chars().next().unwrap()), None, 0.0, t, &mut rng());
    let Board::Typing { mine, .. } = board(&d, 0, t) else { panic!() };
    assert_eq!(mine.typed, 0, "typing only counts once play starts");
}

#[test]
fn typing_wrong_keys_do_not_advance_and_cost_accuracy() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Typing], now);
    let t = to_play(&mut d, now);
    let text = phrase(&d, t);
    let first = text.chars().next().unwrap();
    d.input(0, DuelKey::Char('#'), None, 0.0, t + Duration::from_millis(100), &mut rng());
    d.input(0, DuelKey::Char(first.to_ascii_uppercase()), None, 0.0, t + Duration::from_millis(200), &mut rng());
    d.input(0, DuelKey::Char('#'), None, 0.0, t + Duration::from_millis(205), &mut rng()); // too close: ignored
    d.input(0, DuelKey::Up, None, 0.0, t + Duration::from_millis(300), &mut rng());
    let Board::Typing { mine, .. } = board(&d, 0, t + Duration::from_millis(400)) else { panic!() };
    assert_eq!((mine.typed, mine.errors), (1, 1), "case does not matter; a key 5 ms after another is ignored");
    assert!((mine.accuracy - 0.5).abs() < 1e-9);
}

#[test]
fn typing_fastest_clean_typist_wins_and_ends_the_round() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Typing], now);
    let t = to_play(&mut d, now);
    let text = phrase(&d, t);
    type_out(&mut d, 1, &text, t, 8.0, None);
    assert_eq!(d.phase, Phase::Play, "the other side gets the chase time");
    type_out(&mut d, 0, &text, t, 6.0, None);
    assert_eq!(d.phase, Phase::RoundOver, "both done ends it at once");
    assert_eq!(last_winner(&d), Some(1));
    let result = &d.last.as_ref().unwrap().text;
    assert!(result.contains("Beta digitou melhor e terminou primeiro") && result.contains("ppm e 100% de precisão x"), "{result}");
}

#[test]
fn typing_precision_beats_sloppy_speed() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Typing], now);
    let t = to_play(&mut d, now);
    let text = phrase(&d, t);
    // Beta finishes first, but one key in three is wrong; Alfa is a bit slower and clean.
    type_out(&mut d, 1, &text, t, 9.0, Some(2));
    type_out(&mut d, 0, &text, t, 5.0, None);
    assert_eq!(last_winner(&d), Some(0), "{}", d.last.as_ref().unwrap().text);
}

#[test]
fn typing_chase_time_ends_the_round_after_the_first_finisher() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Typing], now);
    let t = to_play(&mut d, now);
    let text = phrase(&d, t);
    type_out(&mut d, 0, &text, t, 10.0, None);
    let done = t + Duration::from_secs_f64((text.chars().count() - 1) as f64 / 10.0);
    d.tick(done + TYPING_CHASE / 2, &mut rng());
    assert_eq!(d.phase, Phase::Play);
    d.tick(done + TYPING_CHASE, &mut rng());
    assert_eq!(last_winner(&d), Some(0));
}

#[test]
fn typing_with_nobody_typing_is_a_draw() {
    let now = Instant::now();
    let mut d = duel(&[Minigame::Typing], now);
    let t = to_play(&mut d, now);
    run_until(&mut d, t, |d, _| d.phase == Phase::RoundOver);
    assert_eq!(last_winner(&d), None);
}

#[test]
fn typing_stats_are_standard() {
    let (wpm, acc) = pmog::duel::typing_stats(300, 0, 60_000.0);
    assert_eq!((wpm, acc), (60.0, 1.0), "300 chars in a minute = 60 words per minute");
    assert_eq!(pmog::duel::typing_stats(0, 0, 0.0), (0.0, 1.0));
    assert_eq!(pmog::duel::typing_stats(3, 1, 1000.0).1, 0.75);
}

#[test]
fn three_all_goes_to_sudden_death_with_a_new_minigame() {
    let now = Instant::now();
    let games = [Minigame::Mewing, Minigame::Mewing, Minigame::Mewing, Minigame::Angle, Minigame::Angle, Minigame::Angle];
    let mut d = duel(&games, now);
    let mut t = now;
    for side in [0, 0, 0, 1, 1, 1] {
        t = win_round(&mut d, side, t);
    }
    assert_eq!(d.outcome, None, "3 x 3 is not over");
    assert_eq!(d.rounds, 7);
    assert!(!games.contains(&d.game), "sudden death draws a minigame not played yet: {:?}", d.game);
    // Nobody plays the tiebreak: draws keep it going until the cap.
    while d.outcome.is_none() {
        t = run_until(&mut d, t, |d, _| d.phase == Phase::RoundOver);
        t = run_until(&mut d, t, |d, _| d.phase != Phase::RoundOver);
    }
    assert_eq!(d.rounds, DUEL_MAX_ROUNDS);
    assert_eq!(d.outcome, Some(None));
}

#[test]
fn sudden_death_winner_takes_it() {
    let now = Instant::now();
    let games = [Minigame::Mewing, Minigame::Mewing, Minigame::Mewing, Minigame::Angle, Minigame::Angle, Minigame::Angle, Minigame::Mewing];
    let mut d = duel(&games, now);
    let mut t = now;
    for side in [1, 0, 1, 0, 1, 0, 1] {
        t = win_round(&mut d, side, t);
    }
    assert_eq!(d.outcome, Some(Some(1)));
    assert_eq!(d.rounds, 7);
}

/// Picks for whoever's turn it is until the draft is over.
fn draft_all(d: &mut Duel, picks: &[Option<Minigame>], now: Instant) {
    let mut r = rng();
    for pick in picks {
        let side = (0..2).find(|s| matches!(board(d, *s, now), Board::Draft { my_turn: true, .. })).expect("someone's turn");
        d.input(side, DuelKey::Pick(*pick), None, 0.0, now, &mut r);
    }
}

#[test]
fn draft_alternates_and_sets_the_round_order() {
    let now = Instant::now();
    let mut d = Duel::new([1, 2], ["Alfa".into(), "Beta".into()], 50.0, now, &mut rng());
    assert_eq!(d.phase, Phase::Draft);
    let first = (0..2).find(|s| matches!(board(&d, *s, now), Board::Draft { my_turn: true, .. })).unwrap();
    d.input(1 - first, DuelKey::Pick(Some(Minigame::Brow)), None, 0.0, now, &mut rng());
    assert!(matches!(board(&d, first, now), Board::Draft { ref picks, my_turn: true } if picks.is_empty()), "out of turn is ignored");

    draft_all(&mut d, &[Some(Minigame::Brow), Some(Minigame::Brow)], now);
    let Board::Draft { picks, .. } = board(&d, first, now) else { panic!() };
    assert_eq!(picks.len(), 1, "a taken minigame can't be picked again");
    assert!(picks[0].mine && picks[0].game == Minigame::Brow && !picks[0].random);

    draft_all(&mut d, &[Some(Minigame::Hair), None, Some(Minigame::Status), Some(Minigame::Photo), Some(Minigame::Jawline)], now);
    assert_eq!(d.phase, Phase::Countdown, "six picks start the first round");
    assert_eq!(d.game, Minigame::Brow);
    let view = d.view(first, 0.0, now);
    assert_eq!(view.lineup.len(), 6);
    assert_eq!(view.lineup.iter().filter(|p| p.mine).count(), 3, "each side picks half");
    assert!(view.lineup[2].random, "the third pick asked for a random one");
    let games: std::collections::HashSet<_> = view.lineup.iter().map(|p| p.game).collect();
    assert_eq!(games.len(), 6, "no repeats, the random pick included");
    assert_eq!(view.lineup[1].game, Minigame::Hair);
}

#[test]
fn draft_time_out_picks_for_you() {
    let now = Instant::now();
    let mut d = Duel::new([1, 2], ["Alfa".into(), "Beta".into()], 0.0, now, &mut rng());
    let t = run_until(&mut d, now, |d, _| d.phase == Phase::Countdown);
    assert!(t >= now + DRAFT_PICK_TIME * DUEL_ROUNDS);
    let view = d.view(0, 0.0, t);
    assert!(view.lineup.iter().all(|p| p.random) && view.lineup.len() == DUEL_ROUNDS as usize);
}

#[test]
fn every_draft_step_is_a_new_phase_for_clients() {
    let now = Instant::now();
    let mut d = Duel::new([1, 2], ["Alfa".into(), "Beta".into()], 0.0, now, &mut rng());
    let before = d.revision(now);
    draft_all(&mut d, &[None], now);
    assert_ne!(d.revision(now), before);
}
