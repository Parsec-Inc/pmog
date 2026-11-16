//! Tunables for the chat feature.

use std::time::Duration;

// Messages
/// Longest message text, in characters.
pub const CHAT_MAX_CHARS: usize = 200;
/// Longest group name, in characters.
pub const CHAT_GROUP_NAME_LEN: usize = 24;

// Rate limit: a burst of messages, then a steady refill.
pub const CHAT_RATE_BURST: f64 = 5.0;
pub const CHAT_RATE_PER_SEC: f64 = 1.0;

// Server storage and history sent on login
pub const CHAT_HISTORY: usize = 50;
pub const CHAT_GENERAL_HISTORY: usize = 30;
/// Messages kept in the database per conversation.
pub const CHAT_STORE_CAP: usize = 500;
pub const CHAT_GENERAL_STORE_CAP: usize = 100;
/// Private conversations sent on login, most recent first.
pub const CHAT_PRIVATE_CONVS: usize = 15;
pub const CHAT_GROUPS_PER_PLAYER: usize = 20;
pub const CHAT_GROUP_MEMBERS: usize = 30;

// Client and window
/// Messages kept in memory per conversation on the client.
pub const CHAT_CLIENT_CAP: usize = 200;
/// The window counts as open if it was drawn this recently.
pub const CHAT_VIEW_GRACE: Duration = Duration::from_millis(400);
pub const CHAT_SCROLL_STEP: usize = 5;
/// Window size as a share of the terminal, capped in cells.
pub const CHAT_WINDOW_PCT: (u16, u16) = (92, 86);
pub const CHAT_WINDOW_MAX: (u16, u16) = (120, 44);
/// Below this width the conversation list is hidden.
pub const CHAT_LIST_MIN_WIDTH: u16 = 60;
pub const CHAT_LIST_WIDTH: (u16, u16) = (16, 28);
/// Indent of the message text under its sender.
pub const CHAT_INDENT: usize = 2;
