//! Rules of the drafted-era minigames: jawline, posture, fit check, memory, hydration, photo, status, hair, brow and reverse mog.

use std::time::{Duration, Instant};

use pmog::constants::*;
use pmog::duel::{
    Board, BrowMark, Duel, DuelKey, Minigame, Phase, Stance, fit_color, hydration_level, photo_flash, rps_throw,
};
use rand::SeedableRng;
use rand::rngs::StdRng;

const STEP: Duration = Duration::from_millis(10);

fn rng() -> StdRng {
    StdRng::seed_from_u64(23)
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

fn board(d: &Duel, side: usize, start: Instant, ms: f64) -> Board {
    d.view(side, 0.0, start + Duration::from_secs_f64(ms / 1000.0)).board
}

#[test]
fn jawline_fixed_first_wins_and_cursor_stays_inside() {
    let now = Instant::now();
    let mut d = duel(Minigame::Jawline, now);
    let t = to_play(&mut d, now);
    let Board::Jawline { target, mine, .. } = board(&d, 0, t, 0.0) else { panic!() };
    assert!(mine.error > 0, "starts off the mold");
    press(&mut d, 0, DuelKey::Left, t, 10.0);
    let mut ms = 20.0;
    for (i, goal) in target.iter().enumerate() {
        let Board::Jawline { mine, .. } = board(&d, 0, t, ms) else { panic!() };
        assert_eq!(mine.cursor, i);
        let key = if mine.heights[i] < *goal { DuelKey::Up } else { DuelKey::Down };
        for _ in 0..mine.heights[i].abs_diff(*goal) {
            press(&mut d, 0, key, t, ms);
            ms += 10.0;
        }
        press(&mut d, 0, DuelKey::Right, t, ms);
        ms += 10.0;
    }
    let Board::Jawline { mine, theirs, .. } = board(&d, 0, t, ms) else { panic!() };
    assert!(mine.done && mine.error == 0 && mine.cursor == JAW_POINTS - 1);
    assert!(!theirs.done);
    assert_eq!(finish(&mut d, t), Some(0));
    assert!(result(&d).contains("Alfa deixou o maxilar perfeito"), "{}", result(&d));
}

#[test]
fn posture_counts_time_in_the_band() {
    let now = Instant::now();
    let mut d = duel(Minigame::Posture, now);
    let t = to_play(&mut d, now);
    let Board::Posture { drift, .. } = board(&d, 0, t, 0.0) else { panic!() };
    // Side 0 corrects every 40 ms like a perfect player; side 1 never does.
    let mut presses: Vec<(f64, i8)> = Vec::new();
    let mut ms = 0.0;
    while ms < POSTURE_LIMIT.as_secs_f64() * 1000.0 {
        let pos = drift.position(&presses, ms);
        if pos.abs() > POSTURE_STEP / 2.0 {
            let key = if pos > 0.0 { DuelKey::Left } else { DuelKey::Right };
            press(&mut d, 0, key, t, ms);
            presses.push((ms, if pos > 0.0 { -1 } else { 1 }));
        }
        ms += 40.0;
    }
    let Board::Posture { mine, theirs, .. } = board(&d, 0, t, ms) else { panic!() };
    assert!(mine.in_band_ms > 0.9 * ms, "kept upright: {}", mine.in_band_ms);
    assert!(theirs.in_band_ms < mine.in_band_ms);
    let before = mine.presses.len();
    press(&mut d, 0, DuelKey::Left, t, 1.0);
    press(&mut d, 0, DuelKey::Left, t, 1.0 + POSTURE_MIN_GAP_MS / 2.0);
    let Board::Posture { mine, .. } = board(&d, 0, t, ms) else { panic!() };
    assert!(mine.presses.len() <= before + 1, "inhuman double press ignored");
    assert!(mine.presses.windows(2).all(|w| w[0].0 <= w[1].0), "kept in time order");
    assert_eq!(finish(&mut d, t), Some(0));
    assert!(result(&d).contains("ficou mais tempo reto"));
}

#[test]
fn fit_check_scores_matches_then_speed() {
    let now = Instant::now();
    let mut d = duel(Minigame::FitCheck, now);
    let t = to_play(&mut d, now);
    let Board::FitCheck { target, offsets, .. } = board(&d, 0, t, 0.0) else { panic!() };
    // Side 0 waits for every right color; side 1 locks everything at once.
    let mut ms = 0.0;
    for (piece, goal) in target.iter().enumerate() {
        while fit_color(&offsets, piece, ms) != *goal {
            ms += 5.0;
        }
        press(&mut d, 0, DuelKey::Action, t, ms + 1.0);
    }
    let wrong = (0..FIT_PIECES.len()).filter(|p| fit_color(&offsets, *p, 0.0) != target[*p]).count();
    for _ in 0..FIT_PIECES.len() + 2 {
        press(&mut d, 1, DuelKey::Action, t, 0.0);
    }
    let Board::FitCheck { mine, theirs, .. } = board(&d, 0, t, ms) else { panic!() };
    assert!(mine.done && theirs.done && theirs.locked.len() == FIT_PIECES.len());
    assert_eq!(d.phase, Phase::RoundOver, "ends when both fits are locked");
    let expected = if wrong == 0 { Some(1) } else { Some(0) };
    assert_eq!(d.last.as_ref().unwrap().winner, expected, "{}", result(&d));
}

#[test]
fn memory_flashes_digits_and_ignores_arrows() {
    let now = Instant::now();
    let mut d = duel(Minigame::Memory, now);
    let t = run_until(&mut d, now, |d| d.phase == Phase::Show);
    let Board::Skincare { sequence, level, .. } = board(&d, 0, t, 0.0) else { panic!() };
    assert_eq!(level, MEMORY_START_LEN);
    assert!(sequence.iter().all(|k| matches!(k, DuelKey::Char('1'..='8'))), "{sequence:?}");
    assert_eq!(d.phase_len, MEMORY_LEAD + MEMORY_FLASH * MEMORY_START_LEN as u32);
    let t = to_play(&mut d, t);
    press(&mut d, 1, DuelKey::Up, t, 50.0);
    let Board::Skincare { theirs, .. } = board(&d, 0, t, 60.0) else { panic!() };
    assert!(!theirs.failed, "an arrow is not a memory step");
    for (i, key) in sequence.iter().enumerate() {
        press(&mut d, 0, *key, t, 100.0 + i as f64 * 50.0);
    }
    let wrong = if sequence[0] == DuelKey::Char('1') { '2' } else { '1' };
    press(&mut d, 1, DuelKey::Char(wrong), t, 100.0);
    assert_eq!(d.last.as_ref().map(|r| r.winner), Some(Some(0)), "{:?}", d.last);
}

#[test]
fn hydration_drains_and_closest_lock_wins() {
    assert_eq!(hydration_level(50, 0.0, HYDRATION_DRAIN_MS * 3.0 + 1.0), 47);
    assert_eq!(hydration_level(2, 0.0, HYDRATION_DRAIN_MS * 10.0), 0, "never below empty");
    let now = Instant::now();
    let mut d = duel(Minigame::Hydration, now);
    let t = to_play(&mut d, now);
    let Board::Hydration { target, .. } = board(&d, 0, t, 0.0) else { panic!() };
    // Both pour fast at the start, before any drain.
    let mut ms = 1.0;
    let mut level = 0;
    while level + HYDRATION_SIP_BIG <= target {
        press(&mut d, 0, DuelKey::Up, t, ms);
        press(&mut d, 1, DuelKey::Up, t, ms);
        level += HYDRATION_SIP_BIG;
        ms += 1.0;
    }
    while level < target {
        press(&mut d, 0, DuelKey::Right, t, ms);
        level += 1;
        ms += 1.0;
    }
    press(&mut d, 0, DuelKey::Action, t, ms);
    press(&mut d, 0, DuelKey::Up, t, ms + 1.0);
    let Board::Hydration { mine, .. } = board(&d, 0, t, ms + 5.0) else { panic!() };
    assert!(mine.locked && mine.level == target, "closed exactly on the line; later sips ignored");
    assert_eq!(finish(&mut d, t), Some(0));
    assert!(result(&d).contains(&format!("{target}%")), "{}", result(&d));
}

#[test]
fn photo_rewards_hitting_the_flash() {
    let now = Instant::now();
    let mut d = duel(Minigame::Photo, now);
    let t = to_play(&mut d, now);
    let Board::Photo { shots, .. } = board(&d, 0, t, 0.0) else { panic!() };
    assert_eq!(shots.len(), PHOTO_SHOTS);
    for shot in &shots {
        let flash = photo_flash(*shot);
        press(&mut d, 0, DuelKey::Action, t, flash + 10.0);
        press(&mut d, 0, DuelKey::Action, t, flash + 20.0);
        press(&mut d, 1, DuelKey::Action, t, flash - 300.0);
    }
    let Board::Photo { mine, theirs, .. } = board(&d, 0, t, 0.0) else { panic!() };
    assert!(mine.iter().all(|e| e.is_some_and(|ms| (ms - 10.0).abs() < 1e-6)), "first press per shot counts: {mine:?}");
    assert!(theirs.iter().all(|e| e.is_some_and(|ms| (ms + 300.0).abs() < 1e-6)));
    assert_eq!(d.phase, Phase::RoundOver, "ends once every shot was taken");
    assert_eq!(d.last.as_ref().unwrap().winner, Some(0));
    assert!(result(&d).contains("10 ms x 300 ms"), "{}", result(&d));
}

#[test]
fn status_needs_the_price_order_and_hides_the_rival_order() {
    let now = Instant::now();
    let mut d = duel(Minigame::Status, now);
    let t = to_play(&mut d, now);
    let Board::Status { items, .. } = board(&d, 0, t, 0.0) else { panic!() };
    assert_eq!(items.len(), STATUS_ITEMS);
    let cost = |name: &String| pmog::items::CATALOG.iter().find(|d| d.name == name).unwrap().base_cost;
    let mut right: Vec<usize> = (0..items.len()).collect();
    right.sort_by(|a, b| cost(&items[*b]).total_cmp(&cost(&items[*a])));
    for (k, i) in right.iter().rev().enumerate() {
        press(&mut d, 1, DuelKey::Char(char::from(b'1' + *i as u8)), t, 10.0 + k as f64);
    }
    for (k, i) in right.iter().enumerate() {
        press(&mut d, 0, DuelKey::Char(char::from(b'1' + *i as u8)), t, 500.0 + k as f64);
        if k == 0 {
            press(&mut d, 0, DuelKey::Char(char::from(b'1' + *i as u8)), t, 501.0);
            press(&mut d, 0, DuelKey::Char('9'), t, 501.0);
            let Board::Status { mine, theirs, .. } = board(&d, 0, t, 600.0) else { panic!() };
            assert_eq!(mine.len(), 1, "repeats and unknown numbers ignored");
            assert_eq!(theirs, STATUS_ITEMS, "only how many the rival placed");
        }
    }
    assert_eq!(d.phase, Phase::RoundOver);
    assert_eq!(d.last.as_ref().unwrap().winner, Some(0), "{}", result(&d));
    assert!(result(&d).contains(&format!("{STATUS_ITEMS}/{STATUS_ITEMS}")) && result(&d).contains(" > "));
}

#[test]
fn hair_wants_the_right_arrow_in_time() {
    let now = Instant::now();
    let mut d = duel(Minigame::Hair, now);
    let t = to_play(&mut d, now);
    let Board::Hair { strands, .. } = board(&d, 0, t, 0.0) else { panic!() };
    assert_eq!(strands.len(), HAIR_STRANDS);
    assert!(strands.windows(2).all(|w| w[1].at >= w[0].at + w[0].len), "one strand at a time");
    assert!(strands.last().unwrap().len < strands[0].len, "gets faster");
    for s in &strands {
        press(&mut d, 0, s.dir, t, s.at + 300.0);
        let wrong = *DuelKey::ARROWS.iter().find(|k| **k != s.dir).unwrap();
        press(&mut d, 1, wrong, t, s.at + 250.0);
    }
    press(&mut d, 0, DuelKey::Up, t, 1.0);
    let Board::Hair { mine, theirs, .. } = board(&d, 0, t, 0.0) else { panic!() };
    assert_eq!(mine.hits as usize, HAIR_STRANDS);
    assert_eq!(mine.mistakes, 1, "an arrow with no strand costs");
    assert_eq!(theirs.hits, 0);
    assert_eq!(finish(&mut d, t), Some(0));
    assert!(result(&d).contains("domou o cabelo"));
}

#[test]
fn brow_punishes_twitches_and_misses() {
    let now = Instant::now();
    let mut d = duel(Minigame::Brow, now);
    let t = to_play(&mut d, now);
    let Board::Brow { trials, .. } = board(&d, 0, t, 0.0) else { panic!() };
    assert_eq!(trials.len(), BROW_TRIALS);
    assert!(trials.iter().all(|tr| tr.twitches.len() <= BROW_MAX_TWITCHES && tr.twitches.iter().all(|w| *w + BROW_TWITCH_MS <= tr.raise)));
    for (i, tr) in trials.iter().enumerate() {
        press(&mut d, 0, DuelKey::Action, t, tr.raise + 250.0);
        if i == 0 {
            press(&mut d, 1, DuelKey::Action, t, tr.raise - 100.0);
        }
    }
    let Board::Brow { mine, theirs, .. } = board(&d, 0, t, 0.0) else { panic!() };
    assert!(mine.iter().all(|m| matches!(m, Some(BrowMark::Reaction(ms)) if (ms - 250.0).abs() < 1e-6)));
    assert_eq!(theirs[0], Some(BrowMark::Early));
    assert_eq!(finish(&mut d, t), Some(0));
    let total = 250.0 * BROW_TRIALS as f64;
    assert!(result(&d).contains(&format!("{total:.0} ms x")), "{}", result(&d));
}

#[test]
fn reverse_mog_rules_and_hidden_picks() {
    assert!(Stance::Mog.beats(Stance::Ignore) && Stance::Ignore.beats(Stance::Defend) && Stance::Defend.beats(Stance::Mog));
    assert!(!Stance::Mog.beats(Stance::Mog));
    let now = Instant::now();
    let mut d = duel(Minigame::ReverseMog, now);
    let t = to_play(&mut d, now);
    let span = RPS_CHOOSE_MS + RPS_REVEAL_MS;
    for i in 0..RPS_THROWS {
        let at = i as f64 * span + 100.0;
        assert_eq!(rps_throw(at), (i, true));
        press(&mut d, 0, DuelKey::Down, t, at);
        press(&mut d, 0, DuelKey::Left, t, at + 1.0);
        press(&mut d, 1, DuelKey::Left, t, at);
        if i == 0 {
            let Board::ReverseMog { mine, theirs, their_ready } = board(&d, 0, t, at + 5.0) else { panic!() };
            assert_eq!(mine[0], Some(Stance::Defend), "first pick sticks");
            assert_eq!(theirs[0], None, "hidden until the reveal");
            assert!(their_ready);
            let Board::ReverseMog { theirs, .. } = board(&d, 0, t, RPS_CHOOSE_MS + 10.0) else { panic!() };
            assert_eq!(theirs[0], Some(Stance::Mog), "revealed after the choosing time");
        }
    }
    press(&mut d, 1, DuelKey::Right, t, RPS_CHOOSE_MS + 100.0);
    assert_eq!(finish(&mut d, t), Some(0));
    assert!(result(&d).contains(&format!("{RPS_THROWS} x 0")), "{}", result(&d));
}

#[test]
fn reverse_mog_reveal_pushes_a_new_state() {
    let now = Instant::now();
    let mut d = duel(Minigame::ReverseMog, now);
    let t = to_play(&mut d, now);
    let before = d.revision(t);
    let after = d.revision(t + Duration::from_secs_f64((RPS_CHOOSE_MS + 1.0) / 1000.0));
    assert_ne!(before, after, "time alone reveals, so the server must push");
}

#[test]
fn every_new_minigame_ends_in_a_draw_when_nobody_plays() {
    for game in [
        Minigame::Jawline,
        Minigame::Posture,
        Minigame::FitCheck,
        Minigame::Memory,
        Minigame::Hydration,
        Minigame::Photo,
        Minigame::Status,
        Minigame::Hair,
        Minigame::Brow,
        Minigame::ReverseMog,
    ] {
        let now = Instant::now();
        let mut d = duel(game, now);
        let t = to_play(&mut d, now);
        assert_eq!(finish(&mut d, t), None, "{game:?}: {}", result(&d));
        assert!(result(&d).contains("Empate"), "{game:?}: {}", result(&d));
    }
}
