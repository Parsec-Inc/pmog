//! Tunable numbers for networking, combat and economy.

use std::net::Ipv4Addr;
use std::ops::Range;
use std::time::Duration;

// Networking
pub const PROTOCOL_VERSION: u32 = 16;
/// UDP port where servers announce themselves.
pub const DISCOVERY_PORT: u16 = 47_777;
/// First TCP port a server tries; it walks up when busy.
pub const SERVER_PORT_START: u16 = 47_790;
pub const SERVER_PORT_COUNT: u16 = 10;
pub const MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(239, 77, 77, 77);
pub const BEACON_INTERVAL: Duration = Duration::from_millis(1000);
/// A server that stops announcing for this long leaves the lobby list.
pub const SERVER_STALE: Duration = Duration::from_secs(4);
/// A connected client gives up after this long without hearing from the server.
pub const SERVER_SILENCE: Duration = Duration::from_secs(6);
pub const RECV_BUFFER: usize = 4096;
pub const RECV_ERROR_BACKOFF: Duration = Duration::from_millis(100);
pub const MAX_LINE_BYTES: usize = 256 * 1024;
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);
pub const WRITE_TIMEOUT: Duration = Duration::from_millis(500);
pub const ACCEPT_POLL: Duration = Duration::from_millis(20);
pub const STATE_INTERVAL: Duration = Duration::from_millis(200);
pub const DB_FLUSH_INTERVAL: Duration = Duration::from_secs(2);
/// How far the client's meter reading may differ from the server's and still count.
pub const METER_TOLERANCE: f64 = 0.06;
/// Never change: it is part of every player's id, so changing it would orphan all progress.
pub const MACHINE_ID_SALT: &str = "mogger-machine-v1";
/// Folder for config, machine id and server data under the user's data dir.
pub const APP_DIR: &str = "pmog";
/// Folder used before the rename; moved to `APP_DIR` on first run.
pub const LEGACY_APP_DIR: &str = "mogger";

// Loop and UI
pub const TICK: Duration = Duration::from_millis(33);
/// Longest gap credited as passive income (stops pause/suspend farming).
pub const MAX_TICK_SECS: f64 = 1.0;
pub const MAX_NAME_LEN: usize = 20;
pub const FEED_LEN: usize = 100;
pub const BANNER_DURATION: Duration = Duration::from_millis(1800);
pub const HIT_FLASH: Duration = Duration::from_millis(700);
pub const PULSE_MS: u128 = 400;
pub const SHOP_PREVIEW_WIDTH: u16 = 44;
pub const HELP_PAGE_STEP: i32 = 10;
/// Width of the name column in the rank list, crest included.
pub const RANK_NAME_COLUMN: usize = 20;
pub const SHOP_PREVIEW_MIN_WIDTH: u16 = 100;
/// Below this width (phones held upright) the screens stack instead of sitting side by side.
pub const NARROW_WIDTH: u16 = 80;
/// Lobby height from which the title gets its padding and mascot.
pub const LOBBY_TALL: u16 = 30;
/// Shop width below which the table drops the Requer and Efeito columns and shows them under it.
pub const SHOP_FULL_TABLE_WIDTH: u16 = 76;
pub const SHOP_DETAILS_ROWS: u16 = 5;
/// Below this height popups take the whole screen.
pub const SHORT_HEIGHT: u16 = 24;
/// Stacked game body tall enough for the bigger MOGAR button.
pub const NARROW_ROOMY_BODY: u16 = 34;
/// Upright screens at least this tall show the full header (combat numbers too).
pub const NARROW_FULL_HEADER_FROM: u16 = 40;
/// Feed rows kept under a stacked duel or game.
pub const NARROW_FEED_ROWS: u16 = 6;
/// Stacked duel body tall enough to also show the feed.
pub const NARROW_DUEL_FEED_FROM: u16 = 34;

// Combat
pub const BASE_ATTACK: f64 = 10.0;
pub const BASE_DEFENSE: f64 = 10.0;
pub const BASE_COOLDOWN_SECS: f64 = 4.0;
pub const MIN_COOLDOWN_SECS: f64 = 1.0;
pub const IMMUNITY: Duration = Duration::from_secs(5);
pub const METER_SPEED: f64 = 4.5;
pub const METER_STREAK_ACCEL: f64 = 0.05;
pub const PERFECT_THRESHOLD: f64 = 0.97;
pub const PERFECT_MULT: f64 = 1.6;
/// Meter position where the OK and GOOD timing tiers start.
pub const TIMING_OK_FROM: f64 = 0.5;
pub const TIMING_GOOD_FROM: f64 = 0.75;
/// MP reward multiplier per timing tier.
pub const REWARD_BAD: f64 = 0.5;
pub const REWARD_OK: f64 = 1.0;
pub const REWARD_GOOD: f64 = 1.25;
pub const REWARD_PERFECT: f64 = 2.0;
pub const TIMING_FLOOR: f64 = 0.6;
pub const TIMING_SPAN: f64 = 0.6;
pub const DEFENSE_ROLL: Range<f64> = 0.75..1.25;
pub const STREAK_CAP: u32 = 10;
pub const BASE_STREAK_BONUS: f64 = 0.05;
/// Extra share of the defender's stake per streak win the attacker breaks.
pub const STREAK_BOUNTY: f64 = 0.1;
/// Critical chance at equal attack and defense is half of this.
pub const CRIT_CHANCE_SCALE: f64 = 0.2;
/// Critical chance even against a far stronger defense.
pub const CRIT_MIN_CHANCE: f64 = 0.03;
/// MP multiplier of a critical mog.
pub const CRIT_TAKE_MULT: f64 = 1.5;

// Economy
pub const STAKE_FLAT: f64 = 15.0;
pub const STAKE_RATIO: f64 = 0.08;
pub const STAKE_CAP: f64 = 5_000.0;
pub const LOSS_PENALTY_RATIO: f64 = 0.05;
pub const LOSS_PENALTY_FLAT: f64 = 2.0;
pub const COUNTER_BONUS: f64 = 8.0;
pub const COST_GROWTH: f64 = 1.55;
pub const MEW_CAP_PER_SEC: u32 = 5;
pub const MAX_LOSS_SHIELD: f64 = 0.5;
pub const MIN_METER_FACTOR: f64 = 0.7;
pub const PREWORKOUT_DURATION: Duration = Duration::from_secs(30);
pub const PREWORKOUT_FACTOR: f64 = 0.5;
pub const MIRROR_MULT: f64 = 3.0;

// Training bot
pub const NPC_NAME: &str = "NPC de Treino";
pub const NPC_DEFENSE_ROLL: Range<f64> = 0.25..1.05;
pub const NPC_REWARD_FLAT: f64 = 3.0;
pub const NPC_REWARD_RATIO: f64 = 0.15;

// Headless bot
pub const BOT_ACTION_SECS: Range<f64> = 2.0..6.0;
pub const BOT_MEWS_PER_ACTION: u32 = 3;
/// Wait between attempts to find or reach a server.
pub const BOT_RETRY: Duration = Duration::from_secs(2);

// PvP duels
/// Share of the poorer player's MP each side can bet.
pub const STAKE_FRACTIONS: [f64; 5] = [0.0, 0.1, 0.25, 0.5, 1.0];
pub const INVITE_TTL: Duration = Duration::from_secs(15);
/// Regulation rounds; each side drafts half of them.
pub const DUEL_ROUNDS: u32 = 6;
/// Sudden death stops here and a still tied duel is a draw.
pub const DUEL_MAX_ROUNDS: u32 = 9;
pub const DUEL_COUNTDOWN: Duration = Duration::from_secs(5);
pub const DUEL_ROUND_PAUSE: Duration = Duration::from_millis(3500);
pub const DUEL_RESULT_SHOW: Duration = Duration::from_secs(4);
pub const STAREDOWN_WAIT_SECS: Range<f64> = 1.5..4.5;
pub const STAREDOWN_LIMIT: Duration = Duration::from_secs(3);
pub const SKINCARE_START_LEN: usize = 3;
pub const SKINCARE_MAX_LEN: usize = 12;
pub const SKINCARE_SHOW_BASE: Duration = Duration::from_millis(800);
pub const SKINCARE_SHOW_STEP: Duration = Duration::from_millis(550);
pub const SKINCARE_INPUT_BASE: Duration = Duration::from_secs(2);
pub const SKINCARE_INPUT_STEP: Duration = Duration::from_millis(700);
pub const MEWING_DURATION: Duration = Duration::from_secs(5);
/// Faster alternation than this is not human; ignored.
pub const MEWING_MIN_GAP: Duration = Duration::from_millis(30);
pub const ANGLE_ATTEMPTS: usize = 3;
pub const ANGLE_LIMIT: Duration = Duration::from_secs(10);
/// Bar widths per second the angle cursor travels.
pub const ANGLE_SPEED: f64 = 0.9;
pub const ANGLE_TARGET: Range<f64> = 0.15..0.85;
/// After this long in the queue, any opponent will do (otherwise at most one rank apart).
pub const QUEUE_RELAX: Duration = Duration::from_secs(10);
pub const QUEUE_RANK_GAP: usize = 1;
/// Duels betting at least this much are announced to the whole server.
pub const DUEL_ANNOUNCE_MIN: f64 = 100.0;
/// A gap this long between client loops means the client itself was paused (sleep, Ctrl+Z).
pub const SELF_PAUSE: Duration = Duration::from_secs(1);

// Latency compensation
pub const PING_INTERVAL: Duration = Duration::from_secs(1);
/// Round trips kept to estimate the worst recent latency.
pub const RTT_WINDOW: usize = 10;
/// Weight of each new round trip in the running average.
pub const RTT_SMOOTHING: f64 = 0.2;
/// Extra room, in ms, a client's own timing may lag the server's view.
pub const CLIENT_TIME_SLACK_MS: f64 = 25.0;
/// A client's timing may run at most this many ms ahead of what the server saw.
pub const CLIENT_TIME_AHEAD_MS: f64 = 5.0;
/// After a phase ends, late packets still count if they were pressed in time.
pub const INPUT_GRACE: Duration = Duration::from_millis(250);
/// Faster than this is a guess, not a reaction: counts as blinking early.
pub const HUMAN_REACTION_MIN_MS: f64 = 100.0;
/// After the first valid reaction, how long to wait for a faster one still on the wire.
pub const STAREDOWN_DECIDE_SLACK: Duration = Duration::from_millis(200);

// Fair play between players
/// Attacking this many ranks below yourself pays only `PUNCH_DOWN_FACTOR`.
pub const PUNCH_DOWN_GAP: usize = 2;
pub const PUNCH_DOWN_FACTOR: f64 = 0.25;
/// From this many ranks below, a mog takes nothing at all.
pub const PUNCH_DOWN_ZERO_GAP: usize = 3;
/// Each mog suffered in a row multiplies the next immunity by this.
pub const IMMUNITY_GROWTH: f64 = 3.0;
pub const IMMUNITY_MAX: Duration = Duration::from_secs(60);
/// Without being mogged for this long, immunity goes back to the base.
pub const IMMUNITY_RESET: Duration = Duration::from_secs(60);

// Typing battle
/// Time to read the phrase before typing starts.
pub const TYPING_SHOW: Duration = Duration::from_secs(3);
pub const TYPING_LIMIT: Duration = Duration::from_secs(40);
/// After the first player finishes, the other still has this long.
pub const TYPING_CHASE: Duration = Duration::from_secs(4);
pub const TYPING_WORD_COUNT: usize = 10;
/// Keys closer together than this are a held key or a macro, and are ignored.
pub const TYPING_MIN_GAP_MS: f64 = 15.0;
/// Score multiplier for whoever finishes the phrase first.
pub const TYPING_FINISH_BONUS: f64 = 1.1;
/// Characters per "word" when measuring speed (the usual typing-test convention).
pub const CHARS_PER_WORD: f64 = 5.0;
/// Typing speed range for bots, in characters per second (about 40 to 85 words per minute).
pub const BOT_TYPING_CPS: Range<f64> = 3.5..7.0;

// Each social feature keeps its tunables in its own file.
mod chat;
mod clan;
mod firewall;
mod items;
mod minigames;
mod notify;
mod web;
mod worlds;
pub use chat::*;
pub use clan::*;
pub use firewall::*;
pub use items::*;
pub use minigames::*;
pub use notify::*;
pub use web::*;
pub use worlds::*;

/// Largest build an update may download.
pub const MAX_UPDATE_BYTES: u64 = 64 * 1024 * 1024;
/// How long an update download may stall.
pub const UPDATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// Latest published release of the game.
pub const GITHUB_LATEST_URL: &str = "https://api.github.com/repos/Parsec-Inc/pmog/releases/latest";
/// Accept header the GitHub API asks for.
pub const GITHUB_ACCEPT: &str = "Accept: application/vnd.github+json";
/// Generic client name for downloads; carries nothing about the player.
pub const UPDATE_USER_AGENT: &str = "pmog-updater";
/// How long a whole download from the internet may take.
pub const WEB_UPDATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
