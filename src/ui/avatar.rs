//! The avatar on screen: own face in the corner, the avatar shop and players' profiles.

use std::time::Instant;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use super::{centered, dlc_tags, fg, key_style, popup_area, rank_color, sprite_lines};
use crate::avatar::{self, Avatar, Kind, Mood};
use crate::dlc::{self, DlcPublic};
use crate::game::{Game, Player, Rank};
use crate::protocol::PlayerId;
use crate::utils::format_mp;

/// Columns and rows a face takes on screen.
pub const FACE_COLS: u16 = crate::constants::avatar::AVATAR_WIDTH as u16;
pub const FACE_ROWS: u16 = crate::constants::avatar::AVATAR_HEIGHT.div_ceil(2) as u16;

/// Lines of a face, with the forehead number of the DLCs `viewer` owns; `public` is only built for owners.
fn face(avatar: &Avatar, mood: Mood, viewer: &Player, public: impl FnOnce() -> DlcPublic) -> Vec<Line<'static>> {
    let mut owned = dlc::owned_by(viewer).peekable();
    let forehead = owned.peek().is_some().then(public).and_then(|public| owned.find_map(|d| d.forehead(&public)));
    sprite_lines(&avatar::compose(avatar, mood, forehead), false)
}

/// Height of the corner panel: the face plus its border.
pub fn corner_height() -> u16 {
    FACE_ROWS + 2
}

/// The own face, reacting to the last mog.
pub fn draw_corner(f: &mut Frame, area: Rect, g: &Game, now: Instant) {
    let block = Block::bordered().title(" Você ").title_bottom(Line::from(" [A] avatar ").right_aligned());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = face(&g.player.avatar, g.mood(now), &g.player, || dlc::public(&g.player));
    f.render_widget(Paragraph::new(lines), centered(inner, FACE_COLS, FACE_ROWS));
}

/// Avatar shop: preview on the left, the parts of the current tab on the right.
pub fn draw_editor(f: &mut Frame, g: &Game, now: Instant) {
    let popup = popup_area(f.area(), 80, 80);
    f.render_widget(Clear, popup);
    let block = Block::bordered().title(" Avatar ").title_bottom(
        Line::from(vec![
            Span::styled(" ←→ ", key_style()),
            " parte ".into(),
            Span::styled(" ↑↓ ", key_style()),
            " opção ".into(),
            Span::styled(" Enter ", key_style()),
            " usar/comprar ".into(),
            Span::styled(" Esc ", key_style()),
            " fechar ".into(),
        ])
        .centered(),
    );
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let [left, right] = Layout::horizontal([Constraint::Length(FACE_COLS + 4), Constraint::Min(20)]).areas(inner);

    let editor = &g.avatar_editor;
    let p = &g.player;
    let preview = face(&editor.preview(&p.avatar), g.idle_mood(now), p, || dlc::public(p));
    f.render_widget(Paragraph::new(preview), centered(left, FACE_COLS, FACE_ROWS));

    let tabs: Vec<Span> = Kind::ALL
        .iter()
        .enumerate()
        .flat_map(|(i, k)| {
            let style = if i == editor.tab { Style::new().bg(Color::Magenta).fg(Color::Black).bold() } else { fg(Color::Gray) };
            [Span::styled(format!(" {} ", k.label()), style), Span::raw(" ")]
        })
        .collect();
    let mut lines = vec![Line::from(tabs), Line::raw("")];
    let worn = p.avatar.get(editor.kind());
    for (i, d) in editor.kind().parts().enumerate() {
        let (status, color) = match d.block(p) {
            _ if d.part == worn => ("usando".to_string(), Color::LightGreen),
            Some(avatar::Block::Owned) => (d.block_label(avatar::Block::Owned), Color::Cyan),
            Some(block) => (d.block_label(block), Color::DarkGray),
            None => (format!("{} MP", format_mp(d.cost)), Color::Yellow),
        };
        let here = i == editor.cursor;
        lines.push(Line::from(vec![
            Span::styled(if here { "▶ " } else { "  " }, fg(Color::Magenta).bold()),
            Span::styled(format!("{:<24}", d.name), if here { Style::new().bold() } else { Style::new() }),
            Span::styled(status, fg(color)),
        ]));
    }
    lines.push(Line::raw(""));
    let d = editor.selected();
    let note = if editor.confirm == Some(d.part) {
        Line::styled(format!("Enter de novo para comprar {} por {} MP.", d.name, format_mp(d.cost)), fg(Color::Yellow).bold())
    } else if let Some(why) = d.purchase_block(p).filter(|_| !p.owns_part(d.part)) {
        Line::styled(why, fg(Color::Red))
    } else {
        Line::styled(format!("Você tem {} MP.", format_mp(p.mp)), fg(Color::Gray))
    };
    lines.push(note);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), right);
}

/// A player's profile: face, rank, numbers and DLC tags; `id` `None` is the own one.
pub fn draw_profile(f: &mut Frame, g: &Game, id: Option<PlayerId>, now: Instant) {
    let peer = id.and_then(|id| g.peers.iter().find(|p| p.id == id));
    let own;
    let who = match peer {
        Some(p) => p,
        None => {
            own = g.own_peer(now);
            &own
        }
    };
    let mood = who.reacting(now).unwrap_or_else(|| g.idle_mood(now));
    let popup = popup_area(f.area(), 70, 60);
    f.render_widget(Clear, popup);
    let block = Block::bordered()
        .title(" Perfil ")
        .title_bottom(Line::from(vec![Span::styled(" ↑↓ ", key_style()), " outro jogador ".into(), Span::styled(" Esc ", key_style()), " fechar ".into()]).centered());
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let [left, right] = Layout::horizontal([Constraint::Length(FACE_COLS + 4), Constraint::Min(20)]).areas(inner);
    f.render_widget(Paragraph::new(face(&who.avatar, mood, &g.player, || who.dlc.clone())), centered(left, FACE_COLS, FACE_ROWS));

    let rank = Rank::from_lifetime(who.lifetime);
    let mut title = vec![];
    if let Some(tag) = &who.clan_tag {
        title.push(Span::styled(format!("[{tag}] "), fg(Color::Cyan)));
    }
    title.push(Span::styled(who.name.clone(), Style::new().bold()));
    title.push(Span::styled(format!("  {}", rank.label()), fg(rank_color(rank)).bold()));
    title.extend(dlc_tags(&g.player, &who.dlc, who.statuses.iter().filter(|(_, until)| now < *until).map(|(st, _)| *st)));
    let mut lines = vec![Line::from(title), Line::raw("")];
    let row = |label: &str, value: String| Line::from(vec![Span::styled(format!("{label:<10}"), fg(Color::Gray)), Span::raw(value)]);
    lines.push(row("MP", format_mp(who.mp)));
    lines.push(row("Total", format_mp(who.lifetime)));
    lines.push(row("Poder", format_mp(who.attack)));
    lines.push(row("Defesa", format_mp(who.defense)));
    lines.push(row("Streak", who.streak.to_string()));
    lines.push(Line::raw(""));
    let parts: Vec<&str> = Kind::ALL.iter().map(|k| avatar::def(who.avatar.get(*k)).name).collect();
    lines.push(Line::styled(format!("Usando: {}", parts.join(", ")), fg(Color::DarkGray)));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), right);
}
