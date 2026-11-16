//! Tunables for the clan feature.

use std::time::Duration;

pub const CLAN_NAME_MIN: usize = 3;
pub const CLAN_NAME_MAX: usize = 24;
pub const CLAN_TAG_MIN: usize = 2;
pub const CLAN_TAG_MAX: usize = 5;
pub const CLAN_MAX_MEMBERS: usize = 20;
/// Invitations one player can hold at the same time.
pub const CLAN_MAX_PENDING_INVITES: usize = 5;
/// Clans sent in the ranking.
pub const CLAN_RANKING_LIMIT: usize = 50;
/// How old the ranking may get while the clan window is open.
pub const CLAN_REFRESH: Duration = Duration::from_secs(5);
/// Largest size of the clan window; it shrinks to fit smaller terminals.
pub const CLAN_POPUP_WIDTH: u16 = 84;
pub const CLAN_POPUP_HEIGHT: u16 = 28;
/// Rows the ranking moves on PageUp and PageDown.
pub const CLAN_PAGE_STEP: usize = 5;
