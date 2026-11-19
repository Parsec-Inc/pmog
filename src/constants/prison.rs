//! Cadeia DLC tuning.

use std::time::Duration;

/// Defense lost while the soap number is fresh on the forehead.
pub const SOAP_DEF_CUT: f64 = 0.20;
pub const SOAP_DURATION: Duration = Duration::from_secs(30);
pub const GREEN_POTION_MULT: f64 = 2.0;
pub const GREEN_POTION_DURATION: Duration = Duration::from_secs(20);
pub const RED_POTION_MULT: f64 = 1.4;
pub const RED_POTION_DURATION: Duration = Duration::from_secs(120);
/// How long whoever drank the Black Potion can't mog the one who gave it.
pub const BLACK_POTION_DURATION: Duration = Duration::from_secs(60);
/// Cooldown added to whoever took Cenoura's pill.
pub const PILL_SLEEP: Duration = Duration::from_secs(10);
/// Marks given (soaps others picked up) needed for each prison title, from X9 to Mil.
pub const PRISON_TITLE_MARKS: [u32; 6] = [1, 5, 15, 40, 100, 250];
/// Marks given that make a clean forehead into Zero (Lucas).
pub const ZERO_TITLE_MARKS: u32 = 67;
