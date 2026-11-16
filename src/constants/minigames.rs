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

// Draft: each side picks half of the regulation rounds, alternating.
pub const DRAFT_PICK_TIME: Duration = Duration::from_secs(12);
/// Columns of the minigame grid on the draft screen.
pub const DRAFT_COLUMNS: usize = 3;
/// Bots think this long before a draft pick, in ms.
pub const BOT_DRAFT_MS: Range<u64> = 600..2_000;

// Memory: routine steps flash one at a time, repeat them with the digits.
pub const MEMORY_STEPS: [&str; 8] = ["Limpar", "Esfoliar", "Tônico", "Sérum", "Ácido", "Hidratar", "Protetor", "Gua Sha"];
pub const MEMORY_START_LEN: usize = 4;
pub const MEMORY_MAX_LEN: usize = 14;
pub const MEMORY_LEAD: Duration = Duration::from_millis(700);
pub const MEMORY_FLASH: Duration = Duration::from_millis(650);
pub const MEMORY_INPUT_BASE: Duration = Duration::from_secs(2);
pub const MEMORY_INPUT_STEP: Duration = Duration::from_millis(600);

// Jawline: fix the jaw profile point by point.
pub const JAW_POINTS: usize = 9;
pub const JAW_LEVELS: u8 = 6;
pub const JAW_LIMIT: Duration = Duration::from_secs(14);

// Posture: keep the drifting neck inside the band.
pub const POSTURE_LIMIT: Duration = Duration::from_secs(10);
pub const POSTURE_AMP_START: f64 = 0.25;
/// Drift amplitude added per second.
pub const POSTURE_AMP_GROWTH: f64 = 0.1;
/// Slow lean per second, either way.
pub const POSTURE_LEAN: f64 = 0.06;
pub const POSTURE_FREQ_SLOW: Range<f64> = 1.0..1.8;
pub const POSTURE_FREQ_FAST: Range<f64> = 2.2..3.4;
/// How far one press moves the neck.
pub const POSTURE_STEP: f64 = 0.1;
/// Half width of the upright band.
pub const POSTURE_BAND: f64 = 0.25;
pub const POSTURE_SAMPLE_MS: f64 = 20.0;
/// Presses closer than this are not human; ignored.
pub const POSTURE_MIN_GAP_MS: f64 = 25.0;

// Fit check: lock each piece on the color of the fit of the day.
pub const FIT_PIECES: [&str; 5] = ["Boné", "Camisa", "Jaqueta", "Calça", "Tênis"];
pub const FIT_COLORS: [&str; 6] = ["Preto", "Branco", "Bege", "Verde", "Azul", "Vinho"];
/// Color period of the first piece; each next piece cycles faster.
pub const FIT_PERIOD_MS: f64 = 450.0;
pub const FIT_PERIOD_STEP_MS: f64 = 50.0;
pub const FIT_LIMIT: Duration = Duration::from_secs(15);

// Hydration: fill the bottle up to the line while it drains.
pub const HYDRATION_TARGET: Range<i32> = 45..96;
pub const HYDRATION_SIP_BIG: i32 = 5;
pub const HYDRATION_SIP_SMALL: i32 = 1;
pub const HYDRATION_MAX: i32 = 100;
/// The bottle loses one point every this many ms.
pub const HYDRATION_DRAIN_MS: f64 = 400.0;
pub const HYDRATION_LIMIT: Duration = Duration::from_secs(10);

// Photo: press on the flash after an irregular 3, 2, 1.
pub const PHOTO_SHOTS: usize = 4;
pub const PHOTO_FIRST_MS: f64 = 1_200.0;
pub const PHOTO_INTERVAL_MS: Range<f64> = 450.0..950.0;
pub const PHOTO_GAP_MS: f64 = 1_500.0;
/// A press this long after the flash still counts for it.
pub const PHOTO_LATE_MS: f64 = 600.0;
/// Error charged for a shot with no press.
pub const PHOTO_MISS_MS: f64 = 600.0;
/// How long each count number stays on screen.
pub const PHOTO_BLIP_MS: f64 = 200.0;

// Status: order shop items from most to least expensive.
pub const STATUS_ITEMS: usize = 5;
pub const STATUS_LIMIT: Duration = Duration::from_secs(15);

// Hair: press the arrow of each loose strand before it flies away.
pub const HAIR_STRANDS: usize = 12;
pub const HAIR_FIRST_MS: f64 = 1_200.0;
pub const HAIR_SHOW_START_MS: f64 = 1_000.0;
pub const HAIR_SHOW_END_MS: f64 = 550.0;
pub const HAIR_GAP_MS: Range<f64> = 300.0..800.0;
pub const HAIR_WRONG_PENALTY: f64 = 1.0;
pub const HAIR_EMPTY_PENALTY: f64 = 0.5;

// Brow: react to the real raise, not to the twitches.
pub const BROW_TRIALS: usize = 5;
pub const BROW_FIRST_MS: f64 = 1_000.0;
pub const BROW_WAIT_MS: Range<f64> = 1_200.0..3_200.0;
pub const BROW_MAX_TWITCHES: usize = 2;
pub const BROW_TWITCH_MS: f64 = 250.0;
/// The raise stays up this long.
pub const BROW_WINDOW_MS: f64 = 1_200.0;
pub const BROW_GAP_MS: f64 = 800.0;
/// Cost of reacting to a twitch, guessing or missing, in ms.
pub const BROW_PENALTY_MS: f64 = 1_200.0;

// Reverse mog: rock, paper, scissors with mog, defend and ignore.
pub const RPS_THROWS: usize = 5;
pub const RPS_CHOOSE_MS: f64 = 3_000.0;
pub const RPS_REVEAL_MS: f64 = 1_500.0;
