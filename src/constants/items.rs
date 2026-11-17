//! Item effect tuning.

use std::time::Duration;

/// How long the Colônia de Feromônio lasts.
pub const COLOGNE_DURATION: Duration = Duration::from_secs(60);
/// MP multiplier while the cologne is on.
pub const COLOGNE_MULT: f64 = 1.5;
/// MP multiplier of the Combo 6-7 on the next mog that lands.
pub const COMBO_MULT: f64 = 1.67;
/// How long Farmar Aura lasts.
pub const AURA_FARM_DURATION: Duration = Duration::from_secs(45);
/// Passive MP multiplier while farming aura.
pub const AURA_FARM_MULT: f64 = 2.0;
/// Immunity the Bolsa de Gelo gives right away.
pub const ICE_PACK_IMMUNITY: Duration = Duration::from_secs(20);
/// The PERFECT zone never starts before this point of the meter.
pub const MIN_PERFECT_THRESHOLD: f64 = 0.92;

/// How long the shop shows a purchase result.
pub const SHOP_RESULT_SHOW: Duration = Duration::from_secs(4);
