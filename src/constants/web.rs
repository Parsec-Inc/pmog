//! Tunables for the phone client served over HTTP.

use std::time::Duration;

/// Phone sessions one server runs at the same time.
pub const WEB_MAX_SESSIONS: usize = 32;
/// Time the page has to say who it is after the socket opens.
pub const WEB_HELLO_TIMEOUT: Duration = Duration::from_secs(5);
pub const WEB_MIN_COLS: u16 = 20;
pub const WEB_MAX_COLS: u16 = 300;
pub const WEB_MIN_ROWS: u16 = 8;
pub const WEB_MAX_ROWS: u16 = 150;
/// Longest request head the HTTP side reads.
pub const WEB_MAX_HEAD: usize = 8 * 1024;
/// Longest WebSocket message the page may send.
pub const WEB_MAX_MESSAGE: usize = 16 * 1024;
/// Salt in front of the browser token before it becomes a machine id.
pub const WEB_ID_PREFIX: &str = "web-";
pub const WEB_PATH_PAGE: &str = "/";
pub const WEB_PATH_SOCKET: &str = "/ws";
/// Key names the page sends with Ctrl held start with this.
pub const WEB_CTRL_PREFIX: &str = "Ctrl+";
/// Blank modules around the phone QR code.
pub const QR_QUIET_ZONE: usize = 2;
/// Terminal width from which the server panel shows the QR code beside the text.
pub const QR_PANEL_MIN_WIDTH: u16 = 100;
