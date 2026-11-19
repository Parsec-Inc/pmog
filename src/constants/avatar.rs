//! Avatar size and timing.

use std::time::Duration;

/// Pixels of a face; each terminal row shows two pixel rows.
pub const AVATAR_WIDTH: usize = 25;
pub const AVATAR_HEIGHT: usize = 28;
/// Center column and top row of the number drawn on the forehead.
pub const FOREHEAD_CENTER: usize = 12;
pub const FOREHEAD_TOP: usize = 8;
/// Biggest number that fits on the forehead.
pub const FOREHEAD_MAX: u32 = 999;
/// How long the face keeps a reaction to a mog.
pub const MOOD_DURATION: Duration = Duration::from_secs(3);
/// The face blinks once every this many ms, for `BLINK_MS`.
pub const BLINK_EVERY_MS: u128 = 4_000;
pub const BLINK_MS: u128 = 160;
/// Side panel height from which the own face shows above the feed.
pub const AVATAR_PANEL_MIN_HEIGHT: u16 = 30;
