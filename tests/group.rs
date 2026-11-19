//! Group duels: up to five players, points by placing, winner takes the pot.

use std::time::{Duration, Instant};

mod common;
use common::{mp_of, staked_world};

use pmog::constants::*;
use pmog::duel::{Duel, DuelKey, Minigame, Phase, Standings};
use rand::SeedableRng;
use rand::rngs::StdRng;

const STEP: Duration = Duration::from_millis(10);

fn names(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("p{i}")).collect()
}

#[test]
fn standings_give_one_point_per_rival_beaten_and_ties_share() {
    let s = Standings::new(vec![vec![3.0], vec![1.0], vec![3.0], vec![0.0]], &[]);
    assert_eq!(s.points(), vec![2, 1, 2, 0]);
    assert_eq!(s.winner(), None, "a tie at the top has no winner");
    let s = Standings::new(vec![vec![1.0, 5.0], vec![1.0, 2.0], vec![f64::NAN, 9.0]], &[]);
    assert_eq!(s.points(), vec![1, 0, 2], "a NaN number ties, so the next one decides");
    let s = Standings::new(vec![vec![10.0], vec![10.3]], &[0.5]);
    assert_eq!(s.points(), vec![0, 0], "inside the tolerance is a tie");
}

#[test]
fn a_group_invite_starts_with_whoever_accepted() {
    let now = Instant::now();
    let mut w = staked_world(4, 100.0, now);
    w.challenge(1, vec![2, 3, 4], 0.5, false, now);
    let invite = &w.view(2, now).unwrap().invites[0];
    assert_eq!(invite.stake, 50.0);
    assert_eq!(invite.others.len(), 2, "the invite names the other guests");
    w.answer(2, 1, true, now);
    w.answer(4, 1, false, now);
    assert!(!w.in_duel(1), "waits while someone has not answered");
    w.answer(3, 1, true, now);
    assert!(w.in_duel(1) && w.in_duel(2) && w.in_duel(3) && !w.in_duel(4));
    assert_eq!(w.duels()[0].players.len(), 3);
    assert_eq!([mp_of(&w, 1), mp_of(&w, 2), mp_of(&w, 3), mp_of(&w, 4)], [50.0, 50.0, 50.0, 100.0], "everyone bets the same");
}

#[test]
fn an_expired_group_invite_starts_with_who_accepted_or_drops() {
    let now = Instant::now();
    let mut w = staked_world(4, 100.0, now);
    w.challenge(1, vec![2, 3], 0.25, false, now);
    w.answer(2, 1, true, now);
    w.tick(now + INVITE_TTL + STEP, 0.0);
    assert!(w.in_duel(1) && w.in_duel(2) && !w.in_duel(3), "the silent guest is left out");

    let mut w = staked_world(3, 100.0, now);
    w.challenge(1, vec![2, 3], 0.25, false, now);
    w.tick(now + INVITE_TTL + STEP, 0.0);
    assert!(!w.in_duel(1) && w.view(1, now).unwrap().outgoing.is_none(), "nobody accepted: no duel");
}

#[test]
fn group_queue_starts_at_five_or_after_the_wait_with_three() {
    let now = Instant::now();
    let mut w = staked_world(5, 100.0, now);
    for id in 1..=5 {
        w.queue(id, Some(0.1), false, true, now);
    }
    w.tick(now + STEP, 0.0);
    assert_eq!(w.duel_count(), 1);
    assert_eq!(w.duels()[0].players.len(), 5);

    let mut w = staked_world(3, 100.0, now);
    for id in 1..=3 {
        w.queue(id, Some(0.1), false, true, now);
    }
    w.tick(now + STEP, 0.0);
    assert_eq!(w.duel_count(), 0, "three wait for more players");
    assert!(w.view(1, now).unwrap().queued_group);
    w.tick(now + GROUP_QUEUE_WAIT + STEP, 0.0);
    assert_eq!(w.duels()[0].players.len(), 3);
}

#[test]
fn forfeits_leave_the_pot_to_the_last_one_standing() {
    let now = Instant::now();
    let mut w = staked_world(3, 100.0, now);
    w.challenge(1, vec![2, 3], 0.5, false, now);
    w.answer(2, 1, true, now);
    w.answer(3, 1, true, now);
    w.forfeit(1, now);
    assert!(!w.in_duel(1), "who left a group duel is free while the others play");
    assert!(w.in_duel(2) && w.in_duel(3));
    w.forfeit(2, now);
    assert_eq!([mp_of(&w, 1), mp_of(&w, 2), mp_of(&w, 3)], [50.0, 50.0, 200.0], "the winner takes every bet");
    assert_eq!(w.matches[0].winner, Some(3));
}

#[test]
fn group_duels_never_draw_a_one_on_one_game_nor_draft() {
    let now = Instant::now();
    for seed in 0..300 {
        let mut rng = StdRng::seed_from_u64(seed);
        let d = Duel::new([1, 2, 3], names(3), 10.0, true, now, &mut rng);
        assert_ne!(d.phase, Phase::Draft, "a group skips the draft");
        assert_ne!(d.game, Minigame::ReverseMog);
    }
}

#[test]
fn a_round_gives_points_by_placing() {
    let now = Instant::now();
    let mut rng = StdRng::seed_from_u64(7);
    let mut d = Duel::with_games([1, 2, 3, 4], names(4), 10.0, &[Minigame::Mewing], now, &mut rng);
    let mut t = now;
    while d.phase != Phase::Play {
        t += STEP;
        d.tick(t, &mut rng);
    }
    for (side, presses) in [(0, 3), (1, 2), (2, 1)] {
        for i in 0..presses {
            let key = if i % 2 == 0 { DuelKey::Left } else { DuelKey::Right };
            d.input(side, key, None, 0.0, t + MEWING_MIN_GAP * (i + 1), &mut rng);
        }
    }
    while d.phase == Phase::Play {
        t += STEP;
        d.tick(t, &mut rng);
    }
    assert_eq!(d.wins, vec![3, 2, 1, 0]);
    let text = &d.last.as_ref().unwrap().text;
    assert!(text.starts_with("1º p0"), "{text}");
}
