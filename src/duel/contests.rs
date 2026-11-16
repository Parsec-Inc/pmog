//! Self-contained duel minigames; times are ms since play started.

use std::time::Duration;

use rand::Rng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use super::{Board, DuelKey, HunterProgress, Minigame, mean};
use crate::constants::*;

/// Rules of one minigame round between two sides.
pub trait Contest: Send {
    /// How long play lasts.
    fn play_len(&self) -> Duration;
    /// Applies a key from `side` pressed at `t`.
    fn input(&mut self, side: usize, key: DuelKey, t: f64);
    /// True once nothing left can change the result.
    fn settled(&self) -> bool {
        false
    }
    /// Winner and result text; `end` is when play stopped.
    fn result(&self, names: &[String; 2], end: f64) -> (Option<usize>, String);
    /// What `side` sees at `t`.
    fn board(&self, side: usize, t: f64) -> Board;
    /// Changes whenever time alone reveals something new, so the server pushes it.
    fn stamp(&self, _t: f64) -> u32 {
        0
    }
}

/// The contest that plays `game`, if it is one of these.
pub fn create(game: Minigame, rng: &mut impl Rng) -> Option<Box<dyn Contest>> {
    Some(match game {
        Minigame::Jawline => Box::new(Jawline::new(rng)),
        Minigame::Posture => Box::new(Posture::new(rng)),
        Minigame::FitCheck => Box::new(FitCheck::new(rng)),
        Minigame::Hydration => Box::new(Hydration::new(rng)),
        Minigame::Photo => Box::new(Photo::new(rng)),
        Minigame::Status => Box::new(Status::new(rng)),
        Minigame::Hair => Box::new(Hair::new(rng)),
        Minigame::Brow => Box::new(Brow::new(rng)),
        Minigame::ReverseMog => Box::new(ReverseMog::default()),
        _ => return None,
    })
}

/// Side with the larger value, if they differ.
fn higher(a: f64, b: f64) -> Option<usize> {
    if (a - b).abs() < 1e-9 {
        None
    } else if a > b {
        Some(0)
    } else {
        Some(1)
    }
}

/// Side with the smaller value, if they differ.
fn lower(a: f64, b: f64) -> Option<usize> {
    higher(b, a)
}

fn secs(ms: f64) -> String {
    format!("{:.1} s", ms / 1000.0).replace('.', ",")
}

/// Scoring shared by the minigames where you react to what shows up.
pub(super) struct Reflexes {
    answered: [Vec<bool>; 2],
    reactions: [Vec<f64>; 2],
    score: [f64; 2],
    mistakes: [u32; 2],
}

impl Reflexes {
    pub(super) fn new(stimuli: usize) -> Reflexes {
        Reflexes { answered: [vec![false; stimuli], vec![false; stimuli]], reactions: [Vec::new(), Vec::new()], score: [0.0; 2], mistakes: [0; 2] }
    }

    /// A press on stimulus `i`, `reaction` ms after it showed; a wrong or inhumanly fast one costs `penalty`.
    pub(super) fn press(&mut self, side: usize, i: usize, reaction: f64, right: bool, penalty: f64) {
        if self.answered[side][i] {
            return;
        }
        self.answered[side][i] = true;
        if right && reaction >= HUMAN_REACTION_MIN_MS {
            self.reactions[side].push(reaction);
            self.score[side] += 1.0;
        } else {
            self.miss(side, penalty);
        }
    }

    /// A press with nothing on screen.
    pub(super) fn miss(&mut self, side: usize, penalty: f64) {
        self.score[side] -= penalty;
        self.mistakes[side] += 1;
    }

    pub(super) fn progress(&self, side: usize) -> HunterProgress {
        HunterProgress {
            hits: self.reactions[side].len() as u32,
            mistakes: self.mistakes[side],
            score: self.score[side],
            avg_ms: mean(&self.reactions[side]),
            answered: self.answered[side].clone(),
        }
    }

    /// More points wins, then the faster average; `won` follows the winner's name, `tied` opens a draw.
    pub(super) fn result(&self, names: &[String; 2], won: &str, tied: &str) -> (Option<usize>, String) {
        let avg = [mean(&self.reactions[0]), mean(&self.reactions[1])];
        let line = |s: usize| match avg[s] {
            Some(ms) => format!("{:.0} pontos, {ms:.0} ms em média", self.score[s]),
            None => format!("{:.0} pontos", self.score[s]),
        };
        let winner = higher(self.score[0], self.score[1]).or_else(|| match (avg[0], avg[1]) {
            (Some(a), Some(b)) if (a - b).abs() >= 0.5 => lower(a, b),
            _ => None,
        });
        let text = match winner {
            Some(w) => format!("{} {won}: {} x {}.", names[w], line(w), line(1 - w)),
            None => format!("{tied}: {}. Empate.", line(0)),
        };
        (winner, text)
    }
}

/// One side of the jawline minigame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JawSide {
    pub heights: Vec<u8>,
    pub cursor: usize,
    pub error: u32,
    pub done: bool,
}

pub struct Jawline {
    target: Vec<u8>,
    heights: [Vec<u8>; 2],
    cursor: [usize; 2],
    done_ms: [Option<f64>; 2],
}

impl Jawline {
    fn new(rng: &mut impl Rng) -> Jawline {
        let top = JAW_LEVELS as i32 - 1;
        let mut h = rng.gen_range(1..top);
        let target: Vec<u8> = (0..JAW_POINTS)
            .map(|_| {
                h = (h + rng.gen_range(-1..=1)).clamp(0, top);
                h as u8
            })
            .collect();
        let start: Vec<u8> = target
            .iter()
            .map(|t| loop {
                let v = rng.gen_range(0..JAW_LEVELS);
                if v != *t {
                    break v;
                }
            })
            .collect();
        Jawline { target, heights: [start.clone(), start], cursor: [0, 0], done_ms: [None, None] }
    }

    fn error(&self, side: usize) -> u32 {
        self.heights[side].iter().zip(&self.target).map(|(h, t)| h.abs_diff(*t) as u32).sum()
    }

    fn side(&self, s: usize) -> JawSide {
        JawSide { heights: self.heights[s].clone(), cursor: self.cursor[s], error: self.error(s), done: self.done_ms[s].is_some() }
    }
}

impl Contest for Jawline {
    fn play_len(&self) -> Duration {
        JAW_LIMIT
    }

    fn input(&mut self, side: usize, key: DuelKey, t: f64) {
        if self.done_ms[side].is_some() {
            return;
        }
        let c = self.cursor[side];
        match key {
            DuelKey::Left => self.cursor[side] = c.saturating_sub(1),
            DuelKey::Right => self.cursor[side] = (c + 1).min(JAW_POINTS - 1),
            DuelKey::Up => self.heights[side][c] = (self.heights[side][c] + 1).min(JAW_LEVELS - 1),
            DuelKey::Down => self.heights[side][c] = self.heights[side][c].saturating_sub(1),
            _ => return,
        }
        if self.error(side) == 0 {
            self.done_ms[side] = Some(t.max(0.0));
        }
    }

    fn settled(&self) -> bool {
        self.done_ms.iter().all(Option::is_some)
    }

    fn result(&self, names: &[String; 2], _end: f64) -> (Option<usize>, String) {
        let e = [self.error(0), self.error(1)];
        let done = |s: usize| self.done_ms[s].unwrap_or(f64::MAX);
        let by_error = lower(e[0] as f64, e[1] as f64);
        let winner = by_error.or_else(|| lower(done(0), done(1)));
        let text = match (winner, by_error) {
            (Some(w), Some(_)) if e[w] == 0 => format!("{} deixou o maxilar perfeito; {} ficou a {} de erro.", names[w], names[1 - w], e[1 - w]),
            (Some(w), Some(_)) => format!("{} esculpiu melhor: {} x {} de erro.", names[w], e[w], e[1 - w]),
            (Some(w), None) => format!("Os dois acertaram o maxilar; {} terminou antes.", names[w]),
            (None, _) => format!("Maxilares iguais ({} de erro). Empate.", e[0]),
        };
        (winner, text)
    }

    fn board(&self, side: usize, _t: f64) -> Board {
        Board::Jawline { target: self.target.clone(), mine: self.side(side), theirs: self.side(1 - side) }
    }
}

/// Drift of the posture minigame: two waves, (frequency in rad/s, phase), and a lean per second.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PostureWaves {
    pub waves: [(f64, f64); 2],
    pub lean: f64,
}

impl PostureWaves {
    /// Where the neck would be at `t` with no corrections.
    pub fn drift(&self, t: f64) -> f64 {
        let s = t.max(0.0) / 1000.0;
        let amp = POSTURE_AMP_START + POSTURE_AMP_GROWTH * s;
        let [(f1, p1), (f2, p2)] = self.waves;
        amp * ((f1 * s + p1).sin() + 0.5 * (f2 * s + p2).sin()) / 1.5 + self.lean * s
    }

    /// Where the neck is at `t`, given the presses (time, direction) sorted by time.
    pub fn position(&self, presses: &[(f64, i8)], t: f64) -> f64 {
        self.drift(t) + presses.iter().take_while(|(at, _)| *at <= t).map(|(_, d)| *d as f64 * POSTURE_STEP).sum::<f64>()
    }

    /// Time in the upright band from the start up to `t`, in ms.
    pub fn in_band_ms(&self, presses: &[(f64, i8)], t: f64) -> f64 {
        let mut offset = 0.0;
        let mut next = 0;
        let mut total = 0.0;
        let mut at = 0.0;
        while at <= t {
            while next < presses.len() && presses[next].0 <= at {
                offset += presses[next].1 as f64 * POSTURE_STEP;
                next += 1;
            }
            if (self.drift(at) + offset).abs() <= POSTURE_BAND {
                total += POSTURE_SAMPLE_MS;
            }
            at += POSTURE_SAMPLE_MS;
        }
        total
    }
}

/// One side of the posture minigame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostureSide {
    pub presses: Vec<(f64, i8)>,
    pub in_band_ms: f64,
}

pub struct Posture {
    drift: PostureWaves,
    presses: [Vec<(f64, i8)>; 2],
}

impl Posture {
    fn new(rng: &mut impl Rng) -> Posture {
        let phase = |rng: &mut dyn rand::RngCore| rng.gen_range(0.0..std::f64::consts::TAU);
        let waves = [(rng.gen_range(POSTURE_FREQ_SLOW), phase(rng)), (rng.gen_range(POSTURE_FREQ_FAST), phase(rng))];
        let lean = if rng.gen_bool(0.5) { POSTURE_LEAN } else { -POSTURE_LEAN };
        Posture { drift: PostureWaves { waves, lean }, presses: [Vec::new(), Vec::new()] }
    }

    fn side(&self, s: usize, t: f64) -> PostureSide {
        PostureSide { presses: self.presses[s].clone(), in_band_ms: self.drift.in_band_ms(&self.presses[s], t) }
    }
}

impl Contest for Posture {
    fn play_len(&self) -> Duration {
        POSTURE_LIMIT
    }

    fn input(&mut self, side: usize, key: DuelKey, t: f64) {
        let dir = match key {
            DuelKey::Left => -1,
            DuelKey::Right => 1,
            _ => return,
        };
        let presses = &mut self.presses[side];
        if presses.iter().any(|(at, _)| (at - t).abs() < POSTURE_MIN_GAP_MS) {
            return;
        }
        let i = presses.partition_point(|(at, _)| *at <= t);
        presses.insert(i, (t, dir));
    }

    fn result(&self, names: &[String; 2], end: f64) -> (Option<usize>, String) {
        let band = [0, 1].map(|s| self.drift.in_band_ms(&self.presses[s], end));
        let winner = higher(band[0], band[1]);
        let text = match winner {
            Some(w) => format!("{} ficou mais tempo reto: {} x {}.", names[w], secs(band[w]), secs(band[1 - w])),
            None => format!("Mesma postura ({}). Empate.", secs(band[0])),
        };
        (winner, text)
    }

    fn board(&self, side: usize, t: f64) -> Board {
        Board::Posture { drift: self.drift, mine: self.side(side, t), theirs: self.side(1 - side, t) }
    }
}

/// Color piece `piece` shows at `t`, as an index of `FIT_COLORS`.
pub fn fit_color(offsets: &[u8], piece: usize, t: f64) -> u8 {
    let period = FIT_PERIOD_MS - FIT_PERIOD_STEP_MS * piece as f64;
    (((t.max(0.0) / period).floor() as usize + offsets[piece] as usize) % FIT_COLORS.len()) as u8
}

/// One side of the fit check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FitSide {
    pub locked: Vec<u8>,
    pub done: bool,
}

pub struct FitCheck {
    target: Vec<u8>,
    offsets: Vec<u8>,
    locked: [Vec<u8>; 2],
    done_ms: [Option<f64>; 2],
}

impl FitCheck {
    fn new(rng: &mut impl Rng) -> FitCheck {
        let n = FIT_COLORS.len() as u8;
        let target = (0..FIT_PIECES.len()).map(|_| rng.gen_range(0..n)).collect();
        let offsets = (0..FIT_PIECES.len()).map(|_| rng.gen_range(0..n)).collect();
        FitCheck { target, offsets, locked: [Vec::new(), Vec::new()], done_ms: [None, None] }
    }

    fn matches(&self, side: usize) -> usize {
        self.locked[side].iter().zip(&self.target).filter(|(a, b)| a == b).count()
    }
}

impl Contest for FitCheck {
    fn play_len(&self) -> Duration {
        FIT_LIMIT
    }

    fn input(&mut self, side: usize, key: DuelKey, t: f64) {
        let piece = self.locked[side].len();
        if key != DuelKey::Action || piece >= FIT_PIECES.len() {
            return;
        }
        self.locked[side].push(fit_color(&self.offsets, piece, t));
        if piece + 1 == FIT_PIECES.len() {
            self.done_ms[side] = Some(t.max(0.0));
        }
    }

    fn settled(&self) -> bool {
        self.done_ms.iter().all(Option::is_some)
    }

    fn result(&self, names: &[String; 2], _end: f64) -> (Option<usize>, String) {
        let m = [self.matches(0), self.matches(1)];
        let n = FIT_PIECES.len();
        let by_matches = higher(m[0] as f64, m[1] as f64);
        let done = |s: usize| self.done_ms[s].unwrap_or(f64::MAX);
        let winner = by_matches.or_else(|| lower(done(0), done(1)));
        let text = match (winner, by_matches) {
            (Some(w), Some(_)) => format!("{} montou o fit: {}/{n} x {}/{n} peças certas.", names[w], m[w], m[1 - w]),
            (Some(w), None) => format!("Mesmo fit ({}/{n}); {} fechou o look antes.", m[w], names[w]),
            (None, _) => format!("Fits iguais ({}/{n}). Empate.", m[0]),
        };
        (winner, text)
    }

    fn board(&self, side: usize, _t: f64) -> Board {
        let of = |s: usize| FitSide { locked: self.locked[s].clone(), done: self.done_ms[s].is_some() };
        Board::FitCheck { target: self.target.clone(), offsets: self.offsets.clone(), mine: of(side), theirs: of(1 - side) }
    }
}

/// Bottle level at `t`, from `level` measured at `at`.
pub fn hydration_level(level: i32, at: f64, t: f64) -> i32 {
    let drains = (t.max(at) / HYDRATION_DRAIN_MS).floor() - (at / HYDRATION_DRAIN_MS).floor();
    (level - drains as i32).max(0)
}

/// One side of the hydration minigame; `level` was measured at `at`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HydrationSide {
    pub level: i32,
    pub at: f64,
    pub locked: bool,
}

pub struct Hydration {
    target: i32,
    level: [i32; 2],
    at: [f64; 2],
    locked: [Option<f64>; 2],
}

impl Hydration {
    fn new(rng: &mut impl Rng) -> Hydration {
        Hydration { target: rng.gen_range(HYDRATION_TARGET), level: [0; 2], at: [0.0; 2], locked: [None; 2] }
    }

    /// Final level and when the bottle was closed.
    fn finish(&self, side: usize, end: f64) -> (i32, f64) {
        match self.locked[side] {
            Some(t) => (self.level[side], t),
            None => (hydration_level(self.level[side], self.at[side], end), end),
        }
    }
}

impl Contest for Hydration {
    fn play_len(&self) -> Duration {
        HYDRATION_LIMIT
    }

    fn input(&mut self, side: usize, key: DuelKey, t: f64) {
        if self.locked[side].is_some() {
            return;
        }
        let t = t.max(self.at[side]);
        let level = hydration_level(self.level[side], self.at[side], t);
        let level = match key {
            DuelKey::Up => level + HYDRATION_SIP_BIG,
            DuelKey::Right => level + HYDRATION_SIP_SMALL,
            DuelKey::Action => {
                self.locked[side] = Some(t);
                level
            }
            _ => return,
        };
        self.level[side] = level.min(HYDRATION_MAX);
        self.at[side] = t;
    }

    fn settled(&self) -> bool {
        self.locked.iter().all(Option::is_some)
    }

    fn result(&self, names: &[String; 2], end: f64) -> (Option<usize>, String) {
        let f = [self.finish(0, end), self.finish(1, end)];
        let off = |s: usize| (f[s].0 - self.target).abs();
        let by_off = lower(off(0) as f64, off(1) as f64);
        let winner = by_off.or_else(|| lower(f[0].1, f[1].1));
        let target = self.target;
        let text = match (winner, by_off) {
            (Some(w), Some(_)) => format!("{} acertou a hidratação: {}% x {}% (meta {target}%).", names[w], f[w].0, f[1 - w].0),
            (Some(w), None) => format!("Mesma distância da meta {target}%; {} fechou a garrafa antes.", names[w]),
            (None, _) => format!("Garrafas iguais ({}%, meta {target}%). Empate.", f[0].0),
        };
        (winner, text)
    }

    fn board(&self, side: usize, _t: f64) -> Board {
        let of = |s: usize| HydrationSide { level: self.level[s], at: self.at[s], locked: self.locked[s].is_some() };
        Board::Hydration { target: self.target, mine: of(side), theirs: of(1 - side) }
    }
}

/// When the flash of a photo shot (start, interval) goes off.
pub fn photo_flash(shot: (f64, f64)) -> f64 {
    shot.0 + 3.0 * shot.1
}

pub struct Photo {
    shots: Vec<(f64, f64)>,
    errors: [Vec<Option<f64>>; 2],
}

impl Photo {
    fn new(rng: &mut impl Rng) -> Photo {
        let mut at = PHOTO_FIRST_MS;
        let shots = (0..PHOTO_SHOTS)
            .map(|_| {
                let shot = (at, rng.gen_range(PHOTO_INTERVAL_MS));
                at = photo_flash(shot) + PHOTO_LATE_MS + PHOTO_GAP_MS;
                shot
            })
            .collect();
        Photo { shots, errors: [vec![None; PHOTO_SHOTS], vec![None; PHOTO_SHOTS]] }
    }

    fn average(&self, side: usize) -> f64 {
        mean(&self.errors[side].iter().map(|e| e.map_or(PHOTO_MISS_MS, f64::abs)).collect::<Vec<_>>()).unwrap_or(PHOTO_MISS_MS)
    }
}

impl Contest for Photo {
    fn play_len(&self) -> Duration {
        let last = *self.shots.last().expect("shots");
        Duration::from_secs_f64((photo_flash(last) + PHOTO_LATE_MS + 300.0) / 1000.0)
    }

    fn input(&mut self, side: usize, key: DuelKey, t: f64) {
        if key != DuelKey::Action {
            return;
        }
        let Some(i) = self.shots.iter().position(|s| (s.0..=photo_flash(*s) + PHOTO_LATE_MS).contains(&t)) else { return };
        self.errors[side][i].get_or_insert(t - photo_flash(self.shots[i]));
    }

    fn settled(&self) -> bool {
        self.errors.iter().flatten().all(Option::is_some)
    }

    fn result(&self, names: &[String; 2], _end: f64) -> (Option<usize>, String) {
        let avg = [self.average(0), self.average(1)];
        let winner = if (avg[0] - avg[1]).abs() < 0.5 { None } else { lower(avg[0], avg[1]) };
        let text = match winner {
            Some(w) => format!("{} saiu melhor na foto: {:.0} ms x {:.0} ms do flash, em média.", names[w], avg[w], avg[1 - w]),
            None => format!("Fotos iguais ({:.0} ms do flash). Empate.", avg[0]),
        };
        (winner, text)
    }

    fn board(&self, side: usize, _t: f64) -> Board {
        Board::Photo { shots: self.shots.clone(), mine: self.errors[side].clone(), theirs: self.errors[1 - side].clone() }
    }
}

pub struct Status {
    items: Vec<(&'static str, f64)>,
    order: [Vec<usize>; 2],
    done_ms: [Option<f64>; 2],
}

impl Status {
    fn new(rng: &mut impl Rng) -> Status {
        let mut catalog: Vec<_> = crate::items::CATALOG.iter().map(|d| (d.name, d.base_cost)).collect();
        catalog.shuffle(rng);
        let mut items: Vec<(&str, f64)> = Vec::new();
        for item in catalog {
            if items.len() < STATUS_ITEMS && items.iter().all(|(_, c)| (c - item.1).abs() >= 1.0) {
                items.push(item);
            }
        }
        Status { items, order: [Vec::new(), Vec::new()], done_ms: [None, None] }
    }

    /// Item indexes from the most to the least expensive.
    fn correct(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.items.len()).collect();
        order.sort_by(|a, b| self.items[*b].1.total_cmp(&self.items[*a].1));
        order
    }

    fn score(&self, side: usize) -> usize {
        self.order[side].iter().zip(self.correct()).filter(|(a, b)| **a == *b).count()
    }
}

impl Contest for Status {
    fn play_len(&self) -> Duration {
        STATUS_LIMIT
    }

    fn input(&mut self, side: usize, key: DuelKey, t: f64) {
        let DuelKey::Char(c) = key else { return };
        let Some(i) = c.to_digit(10).and_then(|d| (d as usize).checked_sub(1)).filter(|i| *i < self.items.len()) else { return };
        let order = &mut self.order[side];
        if order.contains(&i) || self.done_ms[side].is_some() {
            return;
        }
        order.push(i);
        if order.len() == self.items.len() {
            self.done_ms[side] = Some(t.max(0.0));
        }
    }

    fn settled(&self) -> bool {
        self.done_ms.iter().all(Option::is_some)
    }

    fn result(&self, names: &[String; 2], _end: f64) -> (Option<usize>, String) {
        let s = [self.score(0), self.score(1)];
        let n = self.items.len();
        let by_score = higher(s[0] as f64, s[1] as f64);
        let done = |side: usize| self.done_ms[side].unwrap_or(f64::MAX);
        let winner = by_score.or_else(|| lower(done(0), done(1)));
        let answer: Vec<&str> = self.correct().iter().map(|i| self.items[*i].0).collect();
        let answer = answer.join(" > ");
        let text = match (winner, by_score) {
            (Some(w), Some(_)) => format!("{} conhece os preços: {}/{n} x {}/{n}. Certo: {answer}.", names[w], s[w], s[1 - w]),
            (Some(w), None) => format!("Mesmos acertos ({}/{n}); {} ordenou antes. Certo: {answer}.", s[w], names[w]),
            (None, _) => format!("Mesmos acertos ({}/{n}). Empate. Certo: {answer}.", s[0]),
        };
        (winner, text)
    }

    fn board(&self, side: usize, _t: f64) -> Board {
        Board::Status {
            items: self.items.iter().map(|(name, _)| name.to_string()).collect(),
            mine: self.order[side].clone(),
            theirs: self.order[1 - side].len(),
        }
    }
}

/// A loose strand: when it shows, for how long, and the arrow that fixes it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Strand {
    pub at: f64,
    pub len: f64,
    pub dir: DuelKey,
}

impl Strand {
    pub fn showing(&self, t: f64) -> bool {
        (self.at..self.at + self.len).contains(&t)
    }
}

pub struct Hair {
    strands: Vec<Strand>,
    reflexes: Reflexes,
}

impl Hair {
    fn new(rng: &mut impl Rng) -> Hair {
        let mut at = HAIR_FIRST_MS;
        let strands: Vec<Strand> = (0..HAIR_STRANDS)
            .map(|i| {
                let k = i as f64 / (HAIR_STRANDS - 1) as f64;
                let len = HAIR_SHOW_START_MS + (HAIR_SHOW_END_MS - HAIR_SHOW_START_MS) * k;
                let strand = Strand { at, len, dir: *DuelKey::ARROWS.choose(rng).expect("arrows") };
                at += len + rng.gen_range(HAIR_GAP_MS);
                strand
            })
            .collect();
        Hair { reflexes: Reflexes::new(strands.len()), strands }
    }
}

impl Contest for Hair {
    fn play_len(&self) -> Duration {
        let last = self.strands.last().expect("strands");
        Duration::from_secs_f64((last.at + last.len + 500.0) / 1000.0)
    }

    fn input(&mut self, side: usize, key: DuelKey, t: f64) {
        if !DuelKey::ARROWS.contains(&key) {
            return;
        }
        match self.strands.iter().position(|s| s.showing(t)) {
            Some(i) => {
                let s = self.strands[i];
                self.reflexes.press(side, i, t - s.at, key == s.dir, HAIR_WRONG_PENALTY);
            }
            None => self.reflexes.miss(side, HAIR_EMPTY_PENALTY),
        }
    }

    fn result(&self, names: &[String; 2], _end: f64) -> (Option<usize>, String) {
        self.reflexes.result(names, "domou o cabelo", "Cabelos iguais")
    }

    fn board(&self, side: usize, _t: f64) -> Board {
        Board::Hair { strands: self.strands.clone(), mine: self.reflexes.progress(side), theirs: self.reflexes.progress(1 - side) }
    }
}

/// One brow trial: fake twitches before the real raise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowTrial {
    pub start: f64,
    pub twitches: Vec<f64>,
    pub raise: f64,
}

impl BrowTrial {
    /// Last moment a press still counts for this trial.
    pub fn end(&self) -> f64 {
        self.raise + BROW_WINDOW_MS
    }
}

/// How one brow trial went for a side.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BrowMark {
    Reaction(f64),
    /// Pressed before the raise, or too fast to be human.
    Early,
}

impl BrowMark {
    pub fn cost(mark: Option<BrowMark>) -> f64 {
        match mark {
            Some(BrowMark::Reaction(ms)) => ms,
            _ => BROW_PENALTY_MS,
        }
    }
}

pub struct Brow {
    trials: Vec<BrowTrial>,
    marks: [Vec<Option<BrowMark>>; 2],
}

impl Brow {
    fn new(rng: &mut impl Rng) -> Brow {
        let mut at = BROW_FIRST_MS;
        let trials: Vec<BrowTrial> = (0..BROW_TRIALS)
            .map(|_| {
                let wait = rng.gen_range(BROW_WAIT_MS);
                let slots = ((wait - 400.0) / (BROW_TWITCH_MS * 2.0)).floor().max(0.0) as usize;
                let mut picks: Vec<usize> = (1..slots).collect();
                picks.shuffle(rng);
                let mut twitches: Vec<f64> =
                    picks.into_iter().take(rng.gen_range(0..=BROW_MAX_TWITCHES)).map(|k| at + k as f64 * BROW_TWITCH_MS * 2.0).collect();
                twitches.sort_by(f64::total_cmp);
                let trial = BrowTrial { start: at, twitches, raise: at + wait };
                at = trial.end() + BROW_GAP_MS;
                trial
            })
            .collect();
        Brow { marks: [vec![None; trials.len()], vec![None; trials.len()]], trials }
    }

    fn total(&self, side: usize) -> f64 {
        self.marks[side].iter().map(|m| BrowMark::cost(*m)).sum()
    }
}

impl Contest for Brow {
    fn play_len(&self) -> Duration {
        Duration::from_secs_f64((self.trials.last().expect("trials").end() + 300.0) / 1000.0)
    }

    fn input(&mut self, side: usize, key: DuelKey, t: f64) {
        if key != DuelKey::Action {
            return;
        }
        let Some(i) = self.trials.iter().position(|tr| (tr.start..=tr.end()).contains(&t)) else { return };
        let reaction = t - self.trials[i].raise;
        let mark = if reaction < HUMAN_REACTION_MIN_MS { BrowMark::Early } else { BrowMark::Reaction(reaction) };
        self.marks[side][i].get_or_insert(mark);
    }

    fn settled(&self) -> bool {
        self.marks.iter().flatten().all(Option::is_some)
    }

    fn result(&self, names: &[String; 2], _end: f64) -> (Option<usize>, String) {
        let total = [self.total(0), self.total(1)];
        let winner = if (total[0] - total[1]).abs() < 0.5 { None } else { lower(total[0], total[1]) };
        let text = match winner {
            Some(w) => format!("{} pegou a sobrancelha: {:.0} ms x {:.0} ms no total.", names[w], total[w], total[1 - w]),
            None => format!("Mesmo tempo ({:.0} ms). Empate.", total[0]),
        };
        (winner, text)
    }

    fn board(&self, side: usize, _t: f64) -> Board {
        Board::Brow { trials: self.trials.clone(), mine: self.marks[side].clone(), theirs: self.marks[1 - side].clone() }
    }
}

/// A throw of the reverse mog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stance {
    Mog,
    Defend,
    Ignore,
}

impl Stance {
    pub fn label(self) -> &'static str {
        match self {
            Stance::Mog => "Mogar",
            Stance::Defend => "Defender",
            Stance::Ignore => "Ignorar",
        }
    }

    pub fn beats(self, other: Stance) -> bool {
        matches!((self, other), (Stance::Mog, Stance::Ignore) | (Stance::Ignore, Stance::Defend) | (Stance::Defend, Stance::Mog))
    }

    pub fn from_key(key: DuelKey) -> Option<Stance> {
        match key {
            DuelKey::Left => Some(Stance::Mog),
            DuelKey::Down => Some(Stance::Defend),
            DuelKey::Right => Some(Stance::Ignore),
            _ => None,
        }
    }
}

/// Which side took a throw, if anyone; no pick loses to any pick.
pub fn throw_winner(a: Option<Stance>, b: Option<Stance>) -> Option<usize> {
    match (a, b) {
        (Some(x), Some(y)) if x.beats(y) => Some(0),
        (Some(x), Some(y)) if y.beats(x) => Some(1),
        (Some(_), None) => Some(0),
        (None, Some(_)) => Some(1),
        _ => None,
    }
}

/// Throw being played at `t` and whether it is still open for picks.
pub fn rps_throw(t: f64) -> (usize, bool) {
    let span = RPS_CHOOSE_MS + RPS_REVEAL_MS;
    let i = (t.max(0.0) / span).floor() as usize;
    (i, t.max(0.0) - i as f64 * span < RPS_CHOOSE_MS)
}

/// Throws already revealed at `t`.
pub fn rps_revealed(t: f64) -> usize {
    let (i, open) = rps_throw(t);
    (if open { i } else { i + 1 }).min(RPS_THROWS)
}

#[derive(Default)]
pub struct ReverseMog {
    picks: [[Option<Stance>; RPS_THROWS]; 2],
}

impl ReverseMog {
    fn wins(&self) -> [u32; 2] {
        let mut wins = [0, 0];
        for i in 0..RPS_THROWS {
            if let Some(w) = throw_winner(self.picks[0][i], self.picks[1][i]) {
                wins[w] += 1;
            }
        }
        wins
    }
}

impl Contest for ReverseMog {
    fn play_len(&self) -> Duration {
        Duration::from_secs_f64(RPS_THROWS as f64 * (RPS_CHOOSE_MS + RPS_REVEAL_MS) / 1000.0)
    }

    fn input(&mut self, side: usize, key: DuelKey, t: f64) {
        let (i, open) = rps_throw(t);
        if let (Some(stance), true, true) = (Stance::from_key(key), open, i < RPS_THROWS) {
            self.picks[side][i].get_or_insert(stance);
        }
    }

    fn result(&self, names: &[String; 2], _end: f64) -> (Option<usize>, String) {
        let w = self.wins();
        let winner = higher(w[0] as f64, w[1] as f64);
        let text = match winner {
            Some(x) => format!("{} leu o rival: {} x {} jogadas.", names[x], w[x], w[1 - x]),
            None => format!("Mesmas jogadas ganhas ({}). Empate.", w[0]),
        };
        (winner, text)
    }

    fn board(&self, side: usize, t: f64) -> Board {
        let shown = rps_revealed(t);
        let (now, _) = rps_throw(t);
        let theirs = self.picks[1 - side].iter().enumerate().map(|(i, p)| if i < shown { *p } else { None }).collect();
        Board::ReverseMog {
            mine: self.picks[side].to_vec(),
            theirs,
            their_ready: self.picks[1 - side].get(now).is_some_and(Option::is_some),
        }
    }

    fn stamp(&self, t: f64) -> u32 {
        rps_revealed(t) as u32
    }
}
