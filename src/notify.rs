//! Desktop notifications for game events.

use std::collections::HashMap;
use std::io::{self, Write};
#[cfg(not(windows))]
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Instant;

use crate::constants::*;
use crate::utils::format_mp;

/// Something worth telling the player about outside the game window.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

impl Notice {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Notice {
        Notice { title: title.into(), body: body.into() }
    }

    pub fn invite(from: &str, stake: f64) -> Notice {
        Notice::new(NOTIFY_TITLE_INVITE, format!("{from} te desafiou para um duelo valendo {} MP.", format_mp(stake)))
    }

    pub fn disconnected(reason: &str) -> Notice {
        Notice::new(NOTIFY_TITLE_DISCONNECTED, reason)
    }

    /// What a server event means for a player who is not looking, if anything.
    pub fn from_event(text: &str, banner: Option<&str>, hit: bool) -> Option<Notice> {
        let banner = banner.unwrap_or_default();
        let titled = |title: &str| Some(Notice::new(title, text));
        if banner == NOTIFY_BANNER_DUEL_WON {
            titled(NOTIFY_TITLE_DUEL_WON)
        } else if banner == NOTIFY_BANNER_DUEL_LOST {
            titled(NOTIFY_TITLE_DUEL_LOST)
        } else if banner == NOTIFY_BANNER_DUEL_DRAW {
            titled(NOTIFY_TITLE_DUEL_DRAW)
        } else if banner.starts_with(NOTIFY_BANNER_RANK_PREFIX) {
            titled(NOTIFY_TITLE_RANK)
        } else if hit && banner.starts_with(NOTIFY_BANNER_MOGGED_PREFIX) {
            titled(NOTIFY_TITLE_MOGGED)
        } else {
            let achievement = text.strip_prefix(NOTIFY_TEXT_ACHIEVEMENT_PREFIX)?;
            Some(Notice::new(NOTIFY_TITLE_ACHIEVEMENT, achievement.trim()))
        }
    }
}

/// Drops the oldest notices beyond the queue limit.
pub fn bound(queue: &mut Vec<Notice>) {
    let extra = queue.len().saturating_sub(NOTIFY_QUEUE_MAX);
    queue.drain(..extra);
}

/// Kinds of notice, told apart by title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Mogged,
    Invite,
    Duel,
    Progress,
    Disconnected,
    Other,
}

impl Kind {
    pub fn of(notice: &Notice) -> Kind {
        match notice.title.as_str() {
            NOTIFY_TITLE_MOGGED => Kind::Mogged,
            NOTIFY_TITLE_INVITE => Kind::Invite,
            NOTIFY_TITLE_DUEL_WON | NOTIFY_TITLE_DUEL_LOST | NOTIFY_TITLE_DUEL_DRAW => Kind::Duel,
            NOTIFY_TITLE_RANK | NOTIFY_TITLE_ACHIEVEMENT => Kind::Progress,
            NOTIFY_TITLE_DISCONNECTED => Kind::Disconnected,
            _ => Kind::Other,
        }
    }

    pub fn urgency(self) -> Urgency {
        match self {
            Kind::Progress => Urgency::Low,
            Kind::Disconnected => Urgency::Critical,
            _ => Urgency::Normal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

impl Urgency {
    fn flag(self) -> &'static str {
        match self {
            Urgency::Low => "low",
            Urgency::Normal => "normal",
            Urgency::Critical => "critical",
        }
    }
}

/// Where notifications end up.
pub trait Sink {
    fn deliver(&mut self, notice: &Notice, urgency: Urgency);
}

/// Sink that shows nothing.
pub struct NullSink;

impl Sink for NullSink {
    fn deliver(&mut self, _notice: &Notice, _urgency: Urgency) {}
}

/// Shows notices through `notify-send`, or the terminal's own notify escape when it is missing.
#[derive(Default)]
pub struct SystemSink {
    use_terminal: bool,
}

impl SystemSink {
    pub fn new() -> SystemSink {
        SystemSink::default()
    }

    /// Starts the desktop notifier without waiting for it.
    #[cfg(not(windows))]
    fn spawn(notice: &Notice, urgency: Urgency) -> io::Result<()> {
        let mut child = notifier_command(notice, urgency)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }

    /// Shows a Windows toast through PowerShell; the text travels in environment variables, never in the script.
    #[cfg(windows)]
    fn spawn(notice: &Notice, _urgency: Urgency) -> io::Result<()> {
        let mut child = crate::utils::hidden_powershell()
            .args(["-Command", WINDOWS_TOAST_SCRIPT])
            .env("PMOG_TITLE", &notice.title)
            .env("PMOG_BODY", &notice.body)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

/// `notify-send` on Linux and other Unix desktops.
#[cfg(all(unix, not(target_os = "macos")))]
fn notifier_command(notice: &Notice, urgency: Urgency) -> Command {
    let mut command = Command::new(NOTIFY_PROGRAM);
    command.args(notify_send_args(notice, urgency));
    command
}

/// macOS Notification Center through `osascript`; title and body travel as arguments, never in the script.
#[cfg(target_os = "macos")]
fn notifier_command(notice: &Notice, _urgency: Urgency) -> Command {
    let mut command = Command::new(MACOS_NOTIFY_PROGRAM);
    command.args(macos_notify_args(notice));
    command
}

/// Arguments for `osascript`: the fixed script, then title and body as its `argv`.
pub fn macos_notify_args(notice: &Notice) -> Vec<String> {
    let mut args: Vec<String> = MACOS_NOTIFY_SCRIPT.iter().flat_map(|line| ["-e".to_string(), line.to_string()]).collect();
    args.extend([notice.title.clone(), notice.body.clone()]);
    args
}

/// Builds the toast from `PMOG_TITLE` and `PMOG_BODY` as plain text nodes, under PowerShell's own app id.
#[cfg(windows)]
const WINDOWS_TOAST_SCRIPT: &str = "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
$t = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
$n = $t.GetElementsByTagName('text'); \
$n.Item(0).AppendChild($t.CreateTextNode($env:PMOG_TITLE)) > $null; \
$n.Item(1).AppendChild($t.CreateTextNode($env:PMOG_BODY)) > $null; \
$id = '{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe'; \
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($id).Show([Windows.UI.Notifications.ToastNotification]::new($t))";

impl Sink for SystemSink {
    fn deliver(&mut self, notice: &Notice, urgency: Urgency) {
        if !self.use_terminal {
            if SystemSink::spawn(notice, urgency).is_ok() {
                return;
            }
            self.use_terminal = true;
        }
        let mut out = io::stdout();
        let _ = write_terminal_notice(&mut out, notice);
    }
}

/// Removes control and invisible characters and caps the length.
pub fn sanitize(text: &str, max: usize) -> String {
    let clean: String = text
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control() && !is_invisible(*c))
        .collect();
    let clean = clean.trim();
    if clean.chars().count() <= max {
        return clean.to_string();
    }
    let mut cut: String = clean.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// Zero-width and text-direction characters, which can disguise text.
fn is_invisible(c: char) -> bool {
    matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Arguments for `notify-send`; `--` keeps text starting with a dash from being read as an option.
pub fn notify_send_args(notice: &Notice, urgency: Urgency) -> Vec<String> {
    vec![
        "-a".into(),
        NOTIFY_APP_NAME.into(),
        "-u".into(),
        urgency.flag().into(),
        "--".into(),
        escape_markup(&notice.title),
        escape_markup(&notice.body),
    ]
}

/// The terminal notify escape sequence for a notice.
pub fn terminal_sequence(notice: &Notice) -> String {
    let field = |text: &str| text.replace(';', ",");
    format!("\x1b]777;notify;{};{}\x07", field(&notice.title), field(&notice.body))
}

fn write_terminal_notice(out: &mut impl Write, notice: &Notice) -> io::Result<()> {
    out.write_all(terminal_sequence(notice).as_bytes())?;
    out.flush()
}

/// Whether the player is looking at the game window.
#[derive(Debug, Clone, Copy)]
pub struct Focus {
    /// What the terminal last reported; `None` while it never has.
    reported: Option<bool>,
    last_key: Instant,
}

impl Focus {
    pub fn new(now: Instant) -> Focus {
        Focus { reported: None, last_key: now }
    }

    pub fn gained(&mut self) {
        self.reported = Some(true);
    }

    pub fn lost(&mut self) {
        self.reported = Some(false);
    }

    /// A key press proves the window has focus, even when the terminal missed saying so.
    pub fn key_pressed(&mut self, now: Instant) {
        self.last_key = now;
        if self.reported == Some(false) {
            self.reported = Some(true);
        }
    }

    pub fn is_away(&self, now: Instant) -> bool {
        match self.reported {
            Some(focused) => !focused,
            None => now.saturating_duration_since(self.last_key) >= NOTIFY_IDLE_AWAY,
        }
    }
}

/// Notices of one title waiting to go out as a single notification.
struct Pending {
    kind: Kind,
    title: String,
    count: usize,
    last: Notice,
}

impl Pending {
    /// The notification to show: the notice itself, or a summary of a burst.
    fn message(&self) -> Notice {
        if self.count == 1 {
            return self.last.clone();
        }
        let n = self.count;
        let summary = match self.kind {
            Kind::Mogged => format!("Você foi moggado {n} vezes."),
            Kind::Invite => format!("{n} desafios de duelo."),
            _ => format!("{n} novidades."),
        };
        Notice::new(self.title.clone(), format!("{summary} Último: {}", self.last.body))
    }
}

/// Decides whether notices are shown, merges bursts and hands them to a sink.
pub struct Notifier {
    pub enabled: bool,
    pub focus: Focus,
    sink: Box<dyn Sink>,
    pending: Vec<Pending>,
    sent: HashMap<String, Instant>,
}

impl Notifier {
    pub fn new(sink: Box<dyn Sink>, now: Instant) -> Notifier {
        Notifier { enabled: true, focus: Focus::new(now), sink, pending: Vec::new(), sent: HashMap::new() }
    }

    /// A notifier that shows nothing, for tests and headless clients.
    pub fn silent(now: Instant) -> Notifier {
        Notifier::new(Box::new(NullSink), now)
    }

    pub fn set_sink(&mut self, sink: Box<dyn Sink>) {
        self.sink = sink;
    }

    /// Takes new notices and shows whatever is due; call once per frame.
    pub fn pump(&mut self, notices: Vec<Notice>, now: Instant) {
        if !self.enabled || !self.focus.is_away(now) {
            self.pending.clear();
            return;
        }
        for notice in notices {
            self.enqueue(notice);
        }
        self.flush(now);
    }

    fn enqueue(&mut self, notice: Notice) {
        let notice = Notice::new(sanitize(&notice.title, NOTIFY_TITLE_MAX), sanitize(&notice.body, NOTIFY_BODY_MAX));
        if let Some(pending) = self.pending.iter_mut().find(|p| p.title == notice.title) {
            pending.count += 1;
            pending.last = notice;
            return;
        }
        if self.pending.len() >= NOTIFY_MAX_PENDING {
            self.pending.remove(0);
        }
        self.pending.push(Pending { kind: Kind::of(&notice), title: notice.title.clone(), count: 1, last: notice });
    }

    fn flush(&mut self, now: Instant) {
        self.sent.retain(|_, at| now.saturating_duration_since(*at) < NOTIFY_THROTTLE);
        let (due, wait): (Vec<Pending>, Vec<Pending>) =
            std::mem::take(&mut self.pending).into_iter().partition(|p| !self.sent.contains_key(&p.title));
        self.pending = wait;
        for pending in due {
            self.sink.deliver(&pending.message(), pending.kind.urgency());
            self.sent.insert(pending.title, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_escaped_for_notify_send() {
        let args = notify_send_args(&Notice::new("<b>x</b>", "a & b"), Urgency::Low);
        assert_eq!(args[4], "--");
        assert_eq!(args[5], "&lt;b&gt;x&lt;/b&gt;");
        assert_eq!(args[6], "a &amp; b");
    }

    #[test]
    fn terminal_sequence_keeps_fields_apart() {
        let seq = terminal_sequence(&Notice::new("a;b", "c;d"));
        assert_eq!(seq, "\x1b]777;notify;a,b;c,d\x07");
    }

    #[test]
    fn terminal_notice_is_written_whole() {
        let mut out = Vec::new();
        write_terminal_notice(&mut out, &Notice::new("t", "b")).unwrap();
        assert_eq!(out, b"\x1b]777;notify;t;b\x07");
    }

    #[test]
    fn game_queue_drops_oldest() {
        let mut queue: Vec<Notice> = (0..NOTIFY_QUEUE_MAX + 3).map(|i| Notice::new("t", i.to_string())).collect();
        bound(&mut queue);
        assert_eq!(queue.len(), NOTIFY_QUEUE_MAX);
        assert_eq!(queue[0].body, "3");
    }
}
