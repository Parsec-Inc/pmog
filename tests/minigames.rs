//! Rules of the rhythm, hunter eyes and macros duel minigames.

use std::time::{Duration, Instant};

use pmog::constants::*;
use pmog::duel::{Board, Duel, DuelKey, MacroProblem, Minigame, Phase, rhythm_beat_ms};
use rand::SeedableRng;
use rand::rngs::StdRng;

const STEP: Duration = Duration::from_millis(10);

fn rng() -> StdRng {
    StdRng::seed_from_u64(11)
}

fn duel(game: Minigame, now: Instant) -> Duel {
    Duel::with_games([1, 2], ["Alfa".into(), "Beta".into()], 0.0, &[game], now, &mut rng())
}

fn run_until(d: &mut Duel, mut now: Instant, done: impl Fn(&Duel) -> bool) -> Instant {
    let mut r = rng();
    for _ in 0..20_000 {
        if done(d) {
            return now;
        }
        now += STEP;
        d.tick(now, &mut r);
    }
    panic!("condition never reached; phase {:?}", d.phase);
}

fn to_play(d: &mut Duel, now: Instant) -> Instant {
    run_until(d, now, |d| d.phase == Phase::Play)
}

fn finish(d: &mut Duel, now: Instant) -> Option<usize> {
    run_until(d, now, |d| d.phase == Phase::RoundOver);
    d.last.as_ref().expect("round finished").winner
}

/// Presses at `ms` after play started, stamped with that client time.
fn press(d: &mut Duel, side: usize, key: DuelKey, start: Instant, ms: f64) {
    d.input(side, key, Some(ms), 0.0, start + Duration::from_secs_f64(ms / 1000.0), &mut rng());
}

fn result(d: &Duel) -> String {
    d.last.as_ref().unwrap().text.clone()
}

#[test]
fn every_minigame_has_a_name_and_explanation() {
    assert_eq!(Minigame::ALL.len(), 8);
    for g in Minigame::ALL {
        assert!(!g.name().is_empty() && g.how_to().len() > 40, "{g:?}");
    }
}

#[test]
fn intro_and_pause_give_time_to_read() {
    assert!(DUEL_COUNTDOWN >= Duration::from_secs(5));
    assert!(DUEL_ROUND_PAUSE >= Duration::from_millis(3500));
}

#[test]
fn rhythm_starts_without_memorising_and_lasts_all_beats() {
    let now = Instant::now();
    let mut d = duel(Minigame::Rhythm, now);
    let t = to_play(&mut d, now);
    assert!(t.duration_since(now) >= DUEL_COUNTDOWN);
    let Board::Rhythm { mine, .. } = d.view(0, 0.0, t).board else { panic!("rhythm board") };
    assert_eq!(mine.judged.len(), RHYTHM_BEATS);
    assert!(d.view(0, 0.0, t).phase_len_ms as f64 > rhythm_beat_ms(RHYTHM_BEATS - 1));
}

#[test]
fn rhythm_on_beat_beats_sloppy_and_spam() {
    let now = Instant::now();
    let mut d = duel(Minigame::Rhythm, now);
    let t = to_play(&mut d, now);
    for i in 0..RHYTHM_BEATS {
        let beat = rhythm_beat_ms(i);
        press(&mut d, 0, DuelKey::Action, t, beat + 10.0);
        press(&mut d, 1, DuelKey::Action, t, beat + 100.0);
        press(&mut d, 1, DuelKey::Action, t, beat + 300.0);
    }
    let Board::Rhythm { mine, theirs } = d.view(0, 0.0, t + Duration::from_secs(8)).board else { panic!() };
    assert_eq!((mine.hits, mine.perfects, mine.misses), (RHYTHM_BEATS as u32, RHYTHM_BEATS as u32, 0));
    assert_eq!(theirs.hits, RHYTHM_BEATS as u32);
    assert_eq!(theirs.misses, RHYTHM_BEATS as u32, "presses between beats are misses");
    assert!(mine.score > theirs.score);
    assert_eq!(finish(&mut d, t), Some(0));
    assert!(result(&d).contains("Alfa ficou no ritmo"), "{}", result(&d));
}

#[test]
fn rhythm_one_press_counts_for_one_beat_only() {
    let now = Instant::now();
    let mut d = duel(Minigame::Rhythm, now);
    let t = to_play(&mut d, now);
    let beat = rhythm_beat_ms(0);
    press(&mut d, 0, DuelKey::Action, t, beat);
    press(&mut d, 0, DuelKey::Action, t, beat + 5.0);
    press(&mut d, 0, DuelKey::Left, t, beat + 20.0);
    let Board::Rhythm { mine, .. } = d.view(0, 0.0, t).board else { panic!() };
    assert_eq!((mine.hits, mine.misses), (1, 1), "double tap is a miss, arrows are ignored");
}

#[test]
fn rhythm_nobody_playing_is_a_draw() {
    let now = Instant::now();
    let mut d = duel(Minigame::Rhythm, now);
    let t = to_play(&mut d, now);
    assert_eq!(finish(&mut d, t), None);
}

fn hunter_schedule(d: &Duel, t: Instant) -> Vec<(f64, bool)> {
    let Board::Hunter { schedule, .. } = d.view(0, 0.0, t).board else { panic!("hunter board") };
    schedule
}

#[test]
fn hunter_schedule_has_enough_targets_and_time_to_react() {
    for seed in 0..50 {
        let now = Instant::now();
        let mut d = Duel::with_games([1, 2], ["A".into(), "B".into()], 0.0, &[Minigame::Hunter], now, &mut StdRng::seed_from_u64(seed));
        let t = to_play(&mut d, now);
        let s = hunter_schedule(&d, t);
        assert_eq!(s.len(), HUNTER_STIMULI);
        assert!(s.iter().filter(|(_, x)| *x).count() >= HUNTER_MIN_TARGETS);
        assert!(s[0].0 >= HUNTER_FIRST_MS);
        assert!(s.windows(2).all(|w| w[1].0 - w[0].0 >= HUNTER_SHOW_MS), "faces never overlap");
    }
}

#[test]
fn hunter_rewards_targets_and_punishes_decoys_and_guesses() {
    let now = Instant::now();
    let mut d = duel(Minigame::Hunter, now);
    let t = to_play(&mut d, now);
    let s = hunter_schedule(&d, t);
    let targets = s.iter().filter(|(_, x)| *x).count() as f64;
    let decoys = s.len() as f64 - targets;
    for (at, target) in &s {
        if *target {
            press(&mut d, 0, DuelKey::Action, t, at + 300.0);
            press(&mut d, 0, DuelKey::Action, t, at + 350.0); // second press on the same face does nothing
        }
        press(&mut d, 1, DuelKey::Action, t, at + 300.0);
    }
    press(&mut d, 1, DuelKey::Action, t, 100.0); // nothing on screen
    let Board::Hunter { mine, theirs, .. } = d.view(0, 0.0, t).board else { panic!() };
    assert_eq!(mine.score, targets);
    assert_eq!(mine.mistakes, 0);
    assert_eq!(theirs.score, targets - decoys * HUNTER_DECOY_PENALTY - HUNTER_EMPTY_PENALTY);
    assert_eq!(theirs.mistakes as f64, decoys + 1.0);
    assert!((mine.avg_ms.unwrap() - 300.0).abs() < 1.0);
    assert_eq!(finish(&mut d, t), Some(0));
    assert!(result(&d).contains("olhar de caçador"), "{}", result(&d));
}

#[test]
fn hunter_inhuman_reaction_is_a_guess() {
    let now = Instant::now();
    let mut d = duel(Minigame::Hunter, now);
    let t = to_play(&mut d, now);
    let (at, _) = *hunter_schedule(&d, t).iter().find(|(_, x)| *x).unwrap();
    press(&mut d, 0, DuelKey::Action, t, at + 20.0);
    let Board::Hunter { mine, .. } = d.view(0, 0.0, t).board else { panic!() };
    assert_eq!((mine.hits, mine.mistakes), (0, 1));
}

#[test]
fn hunter_equal_score_goes_to_faster_eyes() {
    let now = Instant::now();
    let mut d = duel(Minigame::Hunter, now);
    let t = to_play(&mut d, now);
    for (at, target) in hunter_schedule(&d, t) {
        if target {
            press(&mut d, 0, DuelKey::Action, t, at + 400.0);
            press(&mut d, 1, DuelKey::Action, t, at + 250.0);
        }
    }
    assert_eq!(finish(&mut d, t), Some(1));
}

fn problems(d: &Duel, t: Instant) -> Vec<MacroProblem> {
    let Board::Macros { problems, .. } = d.view(0, 0.0, t).board else { panic!("macros board") };
    problems
}

#[test]
fn macros_problems_are_shown_before_play_and_never_negative() {
    let now = Instant::now();
    let mut d = duel(Minigame::Macros, now);
    let t = run_until(&mut d, now, |d| d.phase == Phase::Show);
    let p = problems(&d, t);
    assert_eq!(p.len(), MACROS_PROBLEMS);
    assert!(p.iter().all(|p| p.answer() >= 0 && p.text().contains('g')));
    press(&mut d, 0, DuelKey::Answer(p[0].answer()), t, 0.0);
    let Board::Macros { mine, .. } = d.view(0, 0.0, t).board else { panic!() };
    assert_eq!(mine.solved, 0, "answers before play do not count");
}

#[test]
fn macros_wrong_answer_is_an_error_and_does_not_advance() {
    let now = Instant::now();
    let mut d = duel(Minigame::Macros, now);
    let t = to_play(&mut d, now);
    let p = problems(&d, t);
    press(&mut d, 0, DuelKey::Answer(p[0].answer() + 1), t, 500.0);
    press(&mut d, 0, DuelKey::Action, t, 600.0);
    press(&mut d, 0, DuelKey::Answer(p[0].answer()), t, 900.0);
    let Board::Macros { mine, .. } = d.view(0, 0.0, t).board else { panic!() };
    assert_eq!((mine.solved, mine.errors), (1, 1));
}

#[test]
fn macros_more_solved_wins_and_both_done_ends_early() {
    let now = Instant::now();
    let mut d = duel(Minigame::Macros, now);
    let t = to_play(&mut d, now);
    let p = problems(&d, t);
    for (i, q) in p.iter().enumerate() {
        press(&mut d, 0, DuelKey::Answer(q.answer()), t, 1000.0 * (i + 1) as f64);
        press(&mut d, 1, DuelKey::Answer(q.answer()), t, 1500.0 * (i + 1) as f64);
    }
    assert_eq!(d.phase, Phase::RoundOver, "ends as soon as both finished");
    assert_eq!(d.last.as_ref().unwrap().winner, Some(0), "same count, earlier finish wins");
    assert!(result(&d).contains("terminou antes"), "{}", result(&d));
}

#[test]
fn macros_chase_time_ends_after_the_first_finisher() {
    let now = Instant::now();
    let mut d = duel(Minigame::Macros, now);
    let t = to_play(&mut d, now);
    let p = problems(&d, t);
    for (i, q) in p.iter().enumerate() {
        press(&mut d, 1, DuelKey::Answer(q.answer()), t, 500.0 * (i + 1) as f64);
    }
    press(&mut d, 0, DuelKey::Answer(p[0].answer()), t, 3500.0);
    let done = t + Duration::from_millis(500 * p.len() as u64);
    let end = run_until(&mut d, done, |d| d.phase == Phase::RoundOver);
    assert!(end.duration_since(done) <= MACROS_CHASE + STEP * 2);
    assert_eq!(d.last.as_ref().unwrap().winner, Some(1));
}

#[test]
fn macros_nobody_answering_is_a_draw() {
    let now = Instant::now();
    let mut d = duel(Minigame::Macros, now);
    let t = to_play(&mut d, now);
    assert_eq!(finish(&mut d, t), None);
}
