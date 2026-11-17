//! Tunables for playing from a phone browser.

use std::time::Duration;

/// First TCP port the phone page tries; it walks up when busy (several PMOGs on one PC).
pub const WEB_PORT_START: u16 = 47_800;
pub const WEB_PORT_COUNT: u16 = 10;
/// How long the phone socket waits for input before sending the next screen.
pub const WEB_POLL: Duration = Duration::from_millis(10);
/// Time the page has to say who it is after the socket opens.
pub const WEB_HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// Time the page has to type the password.
pub const WEB_LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
/// Pause after a wrong password.
pub const WEB_FAIL_DELAY: Duration = Duration::from_secs(1);
/// Wrong passwords in a row before the lockout.
pub const WEB_MAX_FAILURES: u32 = 5;
pub const WEB_LOCKOUT: Duration = Duration::from_secs(60);
pub const WEB_MIN_PASSWORD: usize = 4;
pub const WEB_MAX_PASSWORD: usize = 64;
/// Browsers remembered after typing the password.
pub const WEB_MAX_TRUSTED: usize = 8;
/// Length range of the random token a browser keeps.
pub const WEB_TOKEN_MIN: usize = 16;
pub const WEB_TOKEN_MAX: usize = 64;
pub const WEB_MIN_COLS: u16 = 20;
pub const WEB_MAX_COLS: u16 = 300;
pub const WEB_MIN_ROWS: u16 = 8;
pub const WEB_MAX_ROWS: u16 = 150;
/// Longest request head the HTTP side reads.
pub const WEB_MAX_HEAD: usize = 8 * 1024;
/// Longest WebSocket message the page may send.
pub const WEB_MAX_MESSAGE: usize = 16 * 1024;
pub const WEB_PATH_PAGE: &str = "/";
pub const WEB_PATH_SOCKET: &str = "/ws";
/// Key names the page sends with Ctrl held start with this.
pub const WEB_CTRL_PREFIX: &str = "Ctrl+";
/// Blank modules around the phone QR code.
pub const QR_QUIET_ZONE: usize = 2;
/// Terminal width from which the server panel shows the QR code beside the text.
pub const QR_PANEL_MIN_WIDTH: u16 = 100;
