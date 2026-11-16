//! Tunables for the newer duel minigames.

use std::ops::Range;
use std::time::Duration;

// Rhythm: press on every metronome beat.
/// Time before the first beat.
pub const RHYTHM_LEAD_MS: f64 = 1500.0;
pub const RHYTHM_INTERVAL_MS: f64 = 600.0;
pub const RHYTHM_BEATS: usize = 12;
/// A press this close to a beat counts as a hit.
pub const RHYTHM_WINDOW_MS: f64 = 150.0;
/// A press this close to a beat is perfect.
pub const RHYTHM_PERFECT_MS: f64 = 40.0;
/// Points lost for a press far from any free beat.
pub const RHYTHM_MISS_PENALTY: f64 = 0.5;

// Hunter eyes: press only for the right face.
pub const HUNTER_STIMULI: usize = 8;
pub const HUNTER_MIN_TARGETS: usize = 3;
pub const HUNTER_TARGET_CHANCE: f64 = 0.6;
pub const HUNTER_FIRST_MS: f64 = 1500.0;
pub const HUNTER_SHOW_MS: f64 = 800.0;
pub const HUNTER_GAP_MS: Range<f64> = 500.0..1300.0;
pub const HUNTER_DECOY_PENALTY: f64 = 1.0;
pub const HUNTER_EMPTY_PENALTY: f64 = 0.5;

// Macros: mental math with grams.
pub const MACROS_PROBLEMS: usize = 6;
pub const MACROS_SHOW: Duration = Duration::from_millis(1500);
pub const MACROS_LIMIT: Duration = Duration::from_secs(35);
/// After the first player solves everything, the other still has this long.
pub const MACROS_CHASE: Duration = Duration::from_secs(4);
pub const MACROS_OPERAND: Range<i64> = 10..60;

/// Most digits a macros answer can have.
pub const MACROS_INPUT_LEN: usize = 5;

/// Bots press a rhythm beat up to this far off, in ms.
pub const BOT_RHYTHM_SPREAD_MS: f64 = 80.0;
/// Bot reaction to hunter eyes, in ms.
pub const BOT_HUNTER_REACT_MS: std::ops::Range<f64> = 230.0..480.0;
/// Chance a bot falls for the puppy face.
pub const BOT_HUNTER_SLIP: f64 = 0.04;
/// Seconds a bot takes per macros problem.
pub const BOT_MACROS_SECS: std::ops::Range<f64> = 1.5..3.5;
/// Chance a bot gets a macros answer wrong.
pub const BOT_MACROS_SLIP: f64 = 0.05;
