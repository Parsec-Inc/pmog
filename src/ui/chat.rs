//! Chat window: conversation list, messages and the line being typed.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph};

use super::flair::{name_spans, tag_span};
use crate::app::App;
use crate::chat::{ChatClient, ChatCmd, ChatMode, ChatMsg, Conv, Conversation};
use crate::constants::*;
use crate::game::Overlay;
use crate::protocol::{ClientMsg, PlayerId};

/// Width of one character in terminal cells.
fn char_width(c: char) -> usize {
    if c.is_ascii() { 1 } else { Span::raw(c.to_string()).width() }
}

fn text_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

/// The start of `text` that fits in `width` cells.
fn fit(text: &str, width: usize) -> String {
    let mut used = 0;
    text.chars()
        .take_while(|c| {
            used += char_width(*c);
            used <= width
        })
        .collect()
}

/// The end of `text` that fits in `width` cells.
fn fit_tail(text: &str, width: usize) -> String {
    let mut used = 0;
    let mut tail: Vec<char> = text
        .chars()
        .rev()
        .take_while(|c| {
            used += char_width(*c);
            used <= width
        })
        .collect();
    tail.reverse();
    tail.into_iter().collect()
}

/// Breaks text into lines of at most `width` cells, at spaces when possible.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    for word in text.split(' ') {
        let len = text_width(word);
        let gap = usize::from(!line.is_empty());
        if used + gap + len <= width {
            if gap == 1 {
                line.push(' ');
            }
            line.push_str(word);
            used += gap + len;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            used = 0;
        }
        if len <= width {
            line.push_str(word);
            used = len;
            continue;
        }
        for c in word.chars() {
            let w = char_width(c);
            if used + w > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            line.push(c);
            used += w;
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// How long ago a message was sent, in short form.
fn ago(at: i64) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    match (now - at).max(0) {
        0..=59 => "agora".to_string(),
        s @ 60..=3599 => format!("{} min", s / 60),
        s @ 3600..=86_399 => format!("{} h", s / 3600),
        s => format!("{} d", s / 86_400),
    }
}

fn label(c: &Conversation) -> String {
    match c.conv {
        Conv::General => "Geral".to_string(),
        Conv::Clan(_) => format!("Clã [{}]", c.title),
        Conv::Group(_) => format!("# {}", c.title),
        Conv::Private(_) => format!("@ {}", c.title),
    }
}

fn window_area(area: Rect) -> Rect {
    let size = |full: u16, pct: u16, max: u16| (full as u32 * pct as u32 / 100).min(max as u32) as u16;
    let (w, h) = (size(area.width, CHAT_WINDOW_PCT.0, CHAT_WINDOW_MAX.0), size(area.height, CHAT_WINDOW_PCT.1, CHAT_WINDOW_MAX.1));
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

/// Online players one can start a conversation with.
fn candidates(app: &App) -> Vec<(PlayerId, String)> {
    let me = app.game.chat.me;
    app.game.peers.iter().filter(|p| !p.is_npc() && Some(p.id) != me).map(|p| (p.id, p.name.clone())).collect()
}

pub fn draw(f: &mut Frame, app: &App, now: Instant) {
    let chat = &app.game.chat;
    chat.mark_viewed(now);
    let area = window_area(f.area());
    f.render_widget(Clear, area);
    let main = if area.width >= CHAT_LIST_MIN_WIDTH {
        let width = (area.width / 4).clamp(CHAT_LIST_WIDTH.0, CHAT_LIST_WIDTH.1);
        let [list, main] = Layout::horizontal([Constraint::Length(width), Constraint::Min(0)]).areas(area);
        draw_list(f, list, chat);
        main
    } else {
        area
    };
    draw_main(f, main, chat, now);
    match &chat.mode {
        ChatMode::PickPlayer { cursor } => draw_picker(f, area, " Nova mensagem para ", &candidates(app), *cursor),
        ChatMode::AddPlayer { cursor, .. } => draw_picker(f, area, " Adicionar ao grupo ", &candidates(app), *cursor),
        ChatMode::Talk | ChatMode::NewGroup(_) => {}
    }
}

fn block(title: String) -> Block<'static> {
    Block::bordered().border_type(BorderType::Rounded).border_style(Style::new().fg(Color::Magenta)).title(title)
}

fn draw_list(f: &mut Frame, area: Rect, chat: &ChatClient) {
    let inner_width = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = chat
        .convs
        .iter()
        .map(|c| {
            let badge = match c.unread.get() {
                0 => String::new(),
                n => format!(" ({n})"),
            };
            let text = fit(&label(c), inner_width.saturating_sub(text_width(&badge)));
            let mut spans = vec![Span::raw(text)];
            if !badge.is_empty() {
                spans.push(Span::styled(badge, Style::new().fg(Color::Yellow).bold()));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let mut state = ListState::default().with_selected(chat.convs.iter().position(|c| c.conv == chat.selected));
    let list = List::new(items).block(block(" Conversas ".into())).highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_main(f: &mut Frame, area: Rect, chat: &ChatClient, now: Instant) {
    let current = chat.current();
    let mut title = format!(" {}", label(current));
    if !current.members.is_empty() {
        title.push_str(&format!(" ({})", current.members.join(", ")));
    }
    title.push(' ');
    let outer = block(title);
    let inner = outer.inner(area);
    f.render_widget(outer, area);
    let [messages, status, input] = Layout::vertical([Constraint::Min(0), Constraint::Length(1), Constraint::Length(1)]).areas(inner);
    draw_messages(f, messages, chat, now);
    f.render_widget(Paragraph::new(status_line(chat, inner.width as usize)), status);
    f.render_widget(Paragraph::new(input_line(chat, inner.width as usize)), input);
}

/// Every line of the conversation, oldest first.
fn message_lines(messages: &[ChatMsg], width: usize, now: Instant) -> Vec<Line<'static>> {
    let indent = " ".repeat(CHAT_INDENT.min(width.saturating_sub(1)));
    let mut lines = Vec::new();
    for msg in messages {
        let mut head = vec![Span::styled(format!("{} ", ago(msg.at)), Style::new().fg(Color::DarkGray))];
        if let Some(tag) = &msg.tag {
            head.push(tag_span(tag));
        }
        head.extend(name_spans(&msg.name, msg.rank, now));
        lines.push(Line::from(head));
        for part in wrap(&msg.text, width.saturating_sub(indent.len())) {
            lines.push(Line::raw(format!("{indent}{part}")));
        }
    }
    lines
}

fn draw_messages(f: &mut Frame, area: Rect, chat: &ChatClient, now: Instant) {
    let height = area.height as usize;
    if height == 0 || area.width == 0 {
        return;
    }
    let mut lines = message_lines(&chat.current().messages, area.width as usize, now);
    if lines.is_empty() {
        lines.push(Line::styled("Nenhuma mensagem ainda.", Style::new().fg(Color::DarkGray)));
    }
    let max_scroll = lines.len().saturating_sub(height);
    chat.max_scroll.set(max_scroll);
    let end = lines.len() - chat.scroll.min(max_scroll);
    let start = end.saturating_sub(height);
    let mut shown: Vec<Line> = lines.drain(start..end).collect();
    // Newest at the bottom, even when the history is short.
    let padding = height.saturating_sub(shown.len());
    shown.splice(0..0, std::iter::repeat_n(Line::raw(""), padding));
    f.render_widget(Paragraph::new(shown), area);
}

fn status_line(chat: &ChatClient, width: usize) -> Line<'static> {
    if let Some(text) = &chat.status {
        return Line::styled(fit(text, width), Style::new().fg(Color::Red));
    }
    let hint = match (&chat.mode, chat.selected) {
        (ChatMode::Talk, Conv::Group(_)) => "Enter envia · Tab troca · ^N nova · ^G grupo · ^A adiciona · ^L sai",
        (ChatMode::Talk, _) => "Enter envia · Tab troca · PgUp/PgDn rola · ^N nova · ^G grupo · Esc fecha",
        (ChatMode::NewGroup(_), _) => "Enter cria o grupo · Esc volta",
        _ => "↑↓ escolhe · Enter confirma · Esc volta",
    };
    Line::styled(fit(hint, width), Style::new().fg(Color::DarkGray))
}

fn input_line(chat: &ChatClient, width: usize) -> Line<'static> {
    let (prompt, text) = match &chat.mode {
        ChatMode::NewGroup(name) => ("Nome do grupo: ", name.as_str()),
        _ => ("> ", chat.input.as_str()),
    };
    let room = width.saturating_sub(text_width(prompt) + 1);
    Line::from(vec![
        Span::styled(fit(prompt, width), Style::new().fg(Color::Magenta).bold()),
        Span::raw(fit_tail(text, room)),
        Span::styled("█", Style::new().fg(Color::Gray)),
    ])
}

fn draw_picker(f: &mut Frame, area: Rect, title: &str, players: &[(PlayerId, String)], cursor: usize) {
    let width = area.width.min(36);
    let height = (players.len().max(1) as u16 + 2).min(area.height);
    let popup = Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height);
    f.render_widget(Clear, popup);
    let inner = width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = if players.is_empty() {
        vec![ListItem::new("Ninguém online.")]
    } else {
        players.iter().map(|(_, name)| ListItem::new(fit(name, inner))).collect()
    };
    let selected = (!players.is_empty()).then(|| cursor.min(players.len() - 1));
    let list = List::new(items).block(block(title.to_string())).highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, popup, &mut ListState::default().with_selected(selected));
}

/// Every key while this window is open lands here, with its original case.
pub fn handle_key(app: &mut App, key: KeyEvent, _now: Instant) {
    if key.kind == KeyEventKind::Release {
        return;
    }
    let players = candidates(app);
    let chat = &mut app.game.chat;
    let mut out = Vec::new();
    let close = match chat.mode.clone() {
        ChatMode::Talk => talk(chat, key, &players, &mut out),
        ChatMode::PickPlayer { cursor } => {
            pick(chat, key, &players, cursor, &mut out, |chat, id| {
                chat.pending_private = Some(id);
                ChatCmd::Open { with: id }
            });
            false
        }
        ChatMode::AddPlayer { group, cursor } => {
            pick(chat, key, &players, cursor, &mut out, |_, player| ChatCmd::AddMember { group, player });
            false
        }
        ChatMode::NewGroup(name) => {
            new_group(chat, key, name, &mut out);
            false
        }
    };
    if close {
        app.game.overlay = Overlay::None;
        chat.select(chat.selected);
    }
    out.extend(chat.take_pending());
    app.game.outbox.extend(out.into_iter().map(ClientMsg::Chat));
}

/// Keys while reading and typing; returns true to close the window.
fn talk(chat: &mut ChatClient, key: KeyEvent, players: &[(PlayerId, String)], out: &mut Vec<ChatCmd>) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Esc => return true,
        KeyCode::Enter => {
            let text = chat.input.trim().to_string();
            if !text.is_empty() {
                out.push(ChatCmd::Send { conv: chat.selected, text });
                chat.input.clear();
                chat.scroll = 0;
                chat.status = None;
            }
        }
        KeyCode::Backspace => {
            chat.input.pop();
        }
        KeyCode::Up | KeyCode::BackTab => chat.step(-1),
        KeyCode::Down | KeyCode::Tab => chat.step(1),
        KeyCode::PageUp => chat.scroll = (chat.scroll + CHAT_SCROLL_STEP).min(chat.max_scroll.get()),
        KeyCode::PageDown => chat.scroll = chat.scroll.saturating_sub(CHAT_SCROLL_STEP),
        KeyCode::Char(c) if ctrl => control(chat, c.to_ascii_lowercase(), players, out),
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::ALT) && !c.is_control() && chat.input.chars().count() < CHAT_MAX_CHARS => {
            chat.input.push(c);
        }
        _ => {}
    }
    false
}

/// Ctrl shortcuts: new private message, group, add to group, leave group.
fn control(chat: &mut ChatClient, key: char, players: &[(PlayerId, String)], out: &mut Vec<ChatCmd>) {
    chat.status = None;
    match (key, chat.selected) {
        ('n', _) if players.is_empty() => chat.status = Some("Não há mais ninguém online para conversar.".into()),
        ('n', _) => chat.mode = ChatMode::PickPlayer { cursor: 0 },
        ('g', _) => chat.mode = ChatMode::NewGroup(String::new()),
        ('a', Conv::Group(_)) if players.is_empty() => chat.status = Some("Não há mais ninguém online para adicionar.".into()),
        ('a', Conv::Group(group)) => chat.mode = ChatMode::AddPlayer { group, cursor: 0 },
        ('l', Conv::Group(group)) => out.push(ChatCmd::Leave { group }),
        ('a' | 'l', _) => chat.status = Some("Escolha um grupo na lista primeiro.".into()),
        _ => {}
    }
}

/// Keys in the player picker; `command` builds what Enter sends.
fn pick(chat: &mut ChatClient, key: KeyEvent, players: &[(PlayerId, String)], cursor: usize, out: &mut Vec<ChatCmd>, command: impl FnOnce(&mut ChatClient, PlayerId) -> ChatCmd) {
    let last = players.len().saturating_sub(1);
    let cursor = cursor.min(last);
    let moved = |cursor: usize| match &chat.mode {
        ChatMode::AddPlayer { group, .. } => ChatMode::AddPlayer { group: *group, cursor },
        _ => ChatMode::PickPlayer { cursor },
    };
    match key.code {
        KeyCode::Esc => chat.mode = ChatMode::Talk,
        KeyCode::Up => chat.mode = moved(if cursor == 0 { last } else { cursor - 1 }),
        KeyCode::Down => chat.mode = moved(if cursor >= last { 0 } else { cursor + 1 }),
        KeyCode::Enter => {
            if let Some((id, _)) = players.get(cursor) {
                out.push(command(chat, *id));
                chat.mode = ChatMode::Talk;
            }
        }
        _ => {}
    }
}

/// Keys while typing a group name.
fn new_group(chat: &mut ChatClient, key: KeyEvent, mut name: String, out: &mut Vec<ChatCmd>) {
    match key.code {
        KeyCode::Esc => chat.mode = ChatMode::Talk,
        KeyCode::Enter => {
            let name = name.trim().to_string();
            if !name.is_empty() {
                chat.pending_group = true;
                out.push(ChatCmd::CreateGroup { name });
                chat.mode = ChatMode::Talk;
            }
        }
        KeyCode::Backspace => {
            name.pop();
            chat.mode = ChatMode::NewGroup(name);
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) && !c.is_control() && name.chars().count() < CHAT_GROUP_NAME_LEN => {
            name.push(c);
            chat.mode = ChatMode::NewGroup(name);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_respects_width_and_keeps_accents() {
        for width in 1..30 {
            for line in wrap("ação é ótima, não é? supercalifragilisticoespialidoso", width) {
                assert!(text_width(&line) <= width.max(1) || line.chars().count() == 1, "{line:?} at {width}");
            }
        }
        assert_eq!(wrap("oi tudo bem", 7), vec!["oi tudo", "bem"]);
        assert_eq!(wrap("", 5), vec![""]);
    }

    #[test]
    fn fit_helpers_cut_on_cells() {
        assert_eq!(fit("ação", 3), "açã");
        assert_eq!(fit_tail("ação", 3), "ção");
        assert_eq!(fit_tail("日本語", 4), "本語");
    }

    #[test]
    fn ago_is_short() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        assert_eq!(ago(now), "agora");
        assert_eq!(ago(now - 120), "2 min");
        assert_eq!(ago(now - 7300), "2 h");
        assert_eq!(ago(now - 200_000), "2 d");
        assert_eq!(ago(now + 500), "agora");
    }
}
