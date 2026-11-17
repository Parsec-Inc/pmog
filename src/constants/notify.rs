//! Tunables for desktop notifications.

use std::time::Duration;

/// Without a key press for this long, a terminal that reports no focus counts as unfocused.
pub const NOTIFY_IDLE_AWAY: Duration = Duration::from_secs(30);
/// Least gap between two notifications with the same title.
pub const NOTIFY_THROTTLE: Duration = Duration::from_secs(5);
/// Distinct notifications kept waiting for their throttle gap.
pub const NOTIFY_MAX_PENDING: usize = 16;
/// Notices the game keeps while nobody drains them.
pub const NOTIFY_QUEUE_MAX: usize = 32;
pub const NOTIFY_TITLE_MAX: usize = 64;
pub const NOTIFY_BODY_MAX: usize = 240;
pub const NOTIFY_PROGRAM: &str = "notify-send";
pub const NOTIFY_APP_NAME: &str = "PMOG";
/// Notifier on macOS.
pub const MACOS_NOTIFY_PROGRAM: &str = "osascript";
/// AppleScript that shows `argv` 1 as the title and `argv` 2 as the body.
pub const MACOS_NOTIFY_SCRIPT: [&str; 3] = ["on run argv", "display notification (item 2 of argv) with title (item 1 of argv)", "end run"];

// Titles; the kind of a notice is told by its title.
pub const NOTIFY_TITLE_MOGGED: &str = "PMOG: você foi moggado";
pub const NOTIFY_TITLE_INVITE: &str = "PMOG: desafio de duelo";
pub const NOTIFY_TITLE_DUEL_WON: &str = "PMOG: vitória no duelo";
pub const NOTIFY_TITLE_DUEL_LOST: &str = "PMOG: derrota no duelo";
pub const NOTIFY_TITLE_DUEL_DRAW: &str = "PMOG: duelo empatado";
pub const NOTIFY_TITLE_RANK: &str = "PMOG: você subiu de rank";
pub const NOTIFY_TITLE_ACHIEVEMENT: &str = "PMOG: conquista desbloqueada";
pub const NOTIFY_TITLE_DISCONNECTED: &str = "PMOG: conexão perdida";

// Banners and texts the server sends with its events.
pub const NOTIFY_BANNER_DUEL_WON: &str = "VITÓRIA NO DUELO";
pub const NOTIFY_BANNER_DUEL_LOST: &str = "DERROTA NO DUELO";
pub const NOTIFY_BANNER_DUEL_DRAW: &str = "EMPATE";
pub const NOTIFY_BANNER_RANK_PREFIX: &str = "RANK UP";
pub const NOTIFY_BANNER_MOGGED_PREFIX: &str = "MOGGADO POR";
pub const NOTIFY_TEXT_ACHIEVEMENT_PREFIX: &str = "Conquista:";
