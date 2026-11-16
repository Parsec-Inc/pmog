//! Clan window: create, invite, join, members and clan ranking.

use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap};

use super::flair::{name_spans, tag_span};
use super::{fg, rank_color};
use crate::app::App;
use crate::clan::{ClanCmd, ClanView, MemberView, valid_clan_name, valid_clan_tag};
use crate::constants::*;
use crate::game::{Game, Overlay, Rank};
use crate::protocol::{ClientMsg, PlayerId};
use crate::utils::format_mp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Home,
    Ranking,
}

impl Tab {
    fn other(self) -> Tab {
        match self {
            Tab::Home => Tab::Ranking,
            Tab::Ranking => Tab::Home,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Field {
    #[default]
    Name,
    Tag,
}

/// The text boxes of the create form.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CreateForm {
    pub name: String,
    pub tag: String,
    pub field: Field,
}

/// Something that asks for a yes before it is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Leave,
    Disband,
    Kick(PlayerId),
    Promote(PlayerId),
}

impl Action {
    fn question(self) -> &'static str {
        match self {
            Action::Leave => "Sair do clã?",
            Action::Disband => "Desfazer o clã para todos?",
            Action::Kick(_) => "Expulsar o membro selecionado?",
            Action::Promote(_) => "Passar a liderança para o membro selecionado?",
        }
    }

    fn command(self) -> ClanCmd {
        match self {
            Action::Leave => ClanCmd::Leave,
            Action::Disband => ClanCmd::Disband,
            Action::Kick(member) => ClanCmd::Kick { member },
            Action::Promote(member) => ClanCmd::Promote { member },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Browse,
    Create(CreateForm),
    /// Choosing an online player to invite.
    Invite,
    Confirm(Action),
}

/// What the clan window remembers between frames.
#[derive(Debug, Clone, Default)]
pub struct ClanUi {
    pub tab: Tab,
    pub mode: Mode,
    /// Selected member, or selected invitation without a clan.
    pub cursor: usize,
    /// Selected player in the invite picker.
    pub picker: usize,
    /// Selected row of the ranking.
    pub scroll: usize,
}

impl ClanUi {
    /// Keeps every selection inside its list after the data changed.
    pub fn clamp(&mut self, clan: Option<&ClanView>, invites: usize, ranking: usize) {
        let home = clan.map_or(invites, |c| c.members.len());
        self.cursor = self.cursor.min(home.saturating_sub(1));
        self.scroll = self.scroll.min(ranking.saturating_sub(1));
        if clan.is_none() && matches!(self.mode, Mode::Invite | Mode::Confirm(_)) {
            self.mode = Mode::Browse;
        }
    }
}

/// Members as listed: online first, then the leader, then by name.
pub fn sorted_members(clan: &ClanView) -> Vec<&MemberView> {
    let mut members: Vec<&MemberView> = clan.members.iter().collect();
    members.sort_by_key(|m| (!m.online, !m.leader, m.name.to_lowercase()));
    members
}

/// Online players who could be invited.
pub fn candidates(game: &Game) -> Vec<(PlayerId, &str)> {
    game.peers.iter().filter(|p| !p.is_npc() && p.clan_tag.is_none()).map(|p| (p.id, p.name.as_str())).collect()
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let (w, h) = (area.width.min(width), area.height.min(height));
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

pub fn draw(f: &mut Frame, app: &App, now: Instant) {
    let (game, client) = (&app.game, &app.game.clan);
    let area = centered(f.area(), CLAN_POPUP_WIDTH, CLAN_POPUP_HEIGHT);
    f.render_widget(Clear, area);
    let title = match &client.clan {
        Some(c) => format!(" Clã [{}] {} ", c.tag, c.name),
        None => " Clãs ".to_string(),
    };
    let block = Block::bordered().border_type(BorderType::Double).border_style(fg(Color::Magenta)).title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [tabs, body, status, help] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1), Constraint::Length(2)]).areas(inner);

    f.render_widget(tab_bar(client.ui.tab, client.clan.is_some()), tabs);
    match (client.ui.tab, &client.ui.mode) {
        (Tab::Ranking, _) => draw_ranking(f, body, game),
        (Tab::Home, Mode::Create(form)) => draw_form(f, body, form),
        (Tab::Home, Mode::Invite) => draw_picker(f, body, game, now),
        (Tab::Home, _) => match &client.clan {
            Some(clan) => draw_members(f, body, game, clan, now),
            None => draw_invites(f, body, game),
        },
    }
    if let Some(text) = &client.status {
        f.render_widget(Line::styled(text.clone(), fg(Color::Yellow)), status);
    }
    f.render_widget(Paragraph::new(help_line(game)).wrap(Wrap { trim: true }), help);
}

fn tab_bar(tab: Tab, in_clan: bool) -> Line<'static> {
    let label = |t: Tab, text: &str| {
        let style = if t == tab { Style::new().bold().fg(Color::Black).bg(Color::Magenta) } else { fg(Color::Gray) };
        Span::styled(format!(" {text} "), style)
    };
    Line::from(vec![label(Tab::Home, if in_clan { "Membros" } else { "Convites" }), " ".into(), label(Tab::Ranking, "Ranking")])
}

fn help_line(game: &Game) -> Line<'static> {
    let client = &game.clan;
    let text = match (&client.ui.mode, &client.clan) {
        _ if client.ui.tab == Tab::Ranking => "↑↓ rolar · Tab voltar · Esc fecha".to_string(),
        (Mode::Create(_), _) => "Enter avança e confirma · Tab troca de campo · Esc cancela".into(),
        (Mode::Invite, _) => "↑↓ escolher · Enter convida · Esc volta".into(),
        (Mode::Confirm(action), _) => format!("{} S confirma · N cancela", action.question()),
        (Mode::Browse, None) => "C criar clã · A aceitar · R recusar · ↑↓ escolher · Tab ranking · Esc fecha".into(),
        (Mode::Browse, Some(c)) if c.is_leader() => {
            "I convidar · K expulsar · P promover · D desfazer · S sair · Tab ranking · Esc fecha".into()
        }
        (Mode::Browse, Some(_)) => "S sair do clã · Tab ranking · Esc fecha".into(),
    };
    Line::styled(text, fg(Color::DarkGray))
}

fn selectable<'a>(items: Vec<ListItem<'a>>, selected: Option<usize>) -> (List<'a>, ListState) {
    let list = List::new(items).highlight_symbol("▶ ").highlight_style(Style::new().bg(Color::Rgb(45, 20, 55)));
    (list, ListState::default().with_selected(selected))
}

fn draw_list(f: &mut Frame, area: Rect, items: Vec<ListItem>, selected: Option<usize>) {
    let (list, mut state) = selectable(items, selected);
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_form(f: &mut Frame, area: Rect, form: &CreateForm) {
    let line = |label: &str, text: &str, active: bool, hint: String| {
        let caret = if active { "▏" } else { "" };
        let style = if active { fg(Color::Yellow).bold() } else { Style::new() };
        Line::from(vec![
            Span::styled(format!("{label:<6}"), fg(Color::Gray)),
            Span::styled(format!("{text}{caret}"), style),
            Span::styled(format!("   {hint}"), fg(Color::DarkGray)),
        ])
    };
    let lines = vec![
        Line::styled("Novo clã", fg(Color::Magenta).bold()),
        Line::raw(""),
        line("Nome", &form.name, form.field == Field::Name, format!("{CLAN_NAME_MIN} a {CLAN_NAME_MAX} caracteres")),
        line("Tag", &form.tag, form.field == Field::Tag, format!("{CLAN_TAG_MIN} a {CLAN_TAG_MAX} letras ou números")),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_invites(f: &mut Frame, area: Rect, game: &Game) {
    let client = &game.clan;
    let [intro, list] = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(area);
    let lines = vec![
        Line::styled("Você não está em nenhum clã.", Style::new().bold()),
        Line::styled("Aperte C para criar um ou aceite um convite abaixo.", fg(Color::Gray)),
        Line::raw(""),
    ];
    f.render_widget(Paragraph::new(lines), intro);
    if client.invites.is_empty() {
        f.render_widget(Line::styled("Nenhum convite no momento.", fg(Color::DarkGray)), list);
        return;
    }
    let items = client
        .invites
        .iter()
        .map(|i| {
            ListItem::new(Line::from(vec![
                tag_span(&i.tag),
                Span::styled(i.name.clone(), Style::new().bold()),
                Span::styled(format!("  {} membros · convite de {}", i.members, i.from), fg(Color::Gray)),
            ]))
        })
        .collect();
    draw_list(f, list, items, Some(client.ui.cursor));
}

fn draw_members(f: &mut Frame, area: Rect, game: &Game, clan: &ClanView, now: Instant) {
    let [head, list] = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(area);
    let summary = format!("{}/{} membros · {} MP de total somado", clan.members.len(), clan.max_members, format_mp(clan.lifetime()));
    f.render_widget(Paragraph::new(vec![Line::styled(summary, fg(Color::Gray)), Line::raw("")]), head);
    let items = sorted_members(clan)
        .into_iter()
        .map(|m| {
            let rank = Rank::from_lifetime(m.lifetime);
            let (dot, dot_color) = if m.online { ("●", Color::Green) } else { ("○", Color::DarkGray) };
            let mut spans = vec![
                Span::styled(format!("{dot} "), fg(dot_color)),
                Span::styled(if m.leader { "★ " } else { "  " }, fg(Color::Yellow)),
            ];
            spans.extend(name_spans(&m.name, rank, now));
            spans.push(Span::styled(format!("  {}", rank.label()), fg(rank_color(rank))));
            spans.push(Span::styled(format!("  {} MP", format_mp(m.lifetime)), fg(Color::Gray)));
            if m.id == clan.you {
                spans.push(Span::styled("  (você)", fg(Color::DarkGray)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    draw_list(f, list, items, Some(game.clan.ui.cursor));
}

fn draw_picker(f: &mut Frame, area: Rect, game: &Game, now: Instant) {
    let [head, list] = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(area);
    f.render_widget(Line::styled("Quem você quer convidar?", fg(Color::Gray)), head);
    let players = candidates(game);
    if players.is_empty() {
        f.render_widget(Line::styled("Ninguém disponível: só jogadores online sem clã.", fg(Color::DarkGray)), list);
        return;
    }
    let items = players
        .iter()
        .map(|(id, name)| {
            let rank = game.peers.iter().find(|p| p.id == *id).map_or(Rank::Sub5, |p| Rank::from_lifetime(p.lifetime));
            ListItem::new(Line::from(name_spans(name, rank, now)))
        })
        .collect();
    draw_list(f, list, items, Some(game.clan.ui.picker.min(players.len() - 1)));
}

fn draw_ranking(f: &mut Frame, area: Rect, game: &Game) {
    let client = &game.clan;
    let [head, list] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    f.render_widget(Line::styled("  #  Clã                          Membros   MP total", fg(Color::Yellow).bold()), head);
    if client.ranking.is_empty() {
        f.render_widget(Line::styled("Nenhum clã ainda. Crie o primeiro!", fg(Color::DarkGray)), list);
        return;
    }
    let mine = client.clan.as_ref().map(|c| c.id);
    let items = client
        .ranking
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let style = if Some(r.id) == mine { fg(Color::Yellow).bold() } else { Style::new() };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:>3}  ", i + 1), fg(Color::Gray)),
                tag_span(&r.tag),
                Span::styled(r.name.clone(), style),
                Span::styled(format!("   {} membros   {} MP", r.members, format_mp(r.lifetime)), fg(Color::Gray)),
            ]))
        })
        .collect();
    draw_list(f, list, items, Some(client.ui.scroll));
}

/// Every key while this window is open lands here, with its original case.
pub fn handle_key(app: &mut App, key: KeyEvent, now: Instant) {
    if key.kind == KeyEventKind::Release {
        return;
    }
    let game = &mut app.game;
    refresh_when_stale(game, now);
    game.clan.status = None;
    let plain = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    match std::mem::take(&mut game.clan.ui.mode) {
        Mode::Browse => browse(game, key.code, plain),
        Mode::Create(form) => create_key(game, form, key.code, plain),
        Mode::Invite => invite_key(game, key.code),
        Mode::Confirm(action) => confirm_key(game, action, key.code),
    }
}

fn send(game: &mut Game, cmd: ClanCmd) {
    game.outbox.push(ClientMsg::Clan(cmd));
}

/// Asks for fresh data when the last snapshot is old, so the ranking never lags for long.
fn refresh_when_stale(game: &mut Game, now: Instant) {
    let stale = game.clan.refreshed.is_none_or(|at| now.saturating_duration_since(at) >= CLAN_REFRESH);
    if stale {
        game.clan.refreshed = Some(now);
        send(game, ClanCmd::Refresh);
    }
}

fn step(current: usize, delta: isize, len: usize) -> usize {
    (current as isize + delta).clamp(0, len.saturating_sub(1) as isize) as usize
}

fn browse(game: &mut Game, code: KeyCode, plain: bool) {
    match code {
        KeyCode::Esc => game.overlay = Overlay::None,
        KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
            game.clan.ui.tab = game.clan.ui.tab.other();
            if game.clan.ui.tab == Tab::Ranking {
                send(game, ClanCmd::Refresh);
            }
        }
        KeyCode::Up => move_selection(game, -1),
        KeyCode::Down => move_selection(game, 1),
        KeyCode::PageUp => move_selection(game, -(CLAN_PAGE_STEP as isize)),
        KeyCode::PageDown => move_selection(game, CLAN_PAGE_STEP as isize),
        KeyCode::Enter if game.clan.ui.tab == Tab::Home => match game.clan.clan.as_ref().map(ClanView::is_leader) {
            None => answer_invite(game, true),
            Some(true) => open_picker(game),
            Some(false) => {}
        },
        KeyCode::Char(c) if plain && game.clan.ui.tab == Tab::Home => home_hotkey(game, c.to_ascii_lowercase()),
        _ => {}
    }
}

fn move_selection(game: &mut Game, delta: isize) {
    let client = &mut game.clan;
    match client.ui.tab {
        Tab::Ranking => client.ui.scroll = step(client.ui.scroll, delta, client.ranking.len()),
        Tab::Home => {
            let len = client.clan.as_ref().map_or(client.invites.len(), |c| c.members.len());
            client.ui.cursor = step(client.ui.cursor, delta, len);
        }
    }
}

fn home_hotkey(game: &mut Game, c: char) {
    let Some(clan) = game.clan.clan.as_ref() else {
        match c {
            'c' => game.clan.ui.mode = Mode::Create(CreateForm::default()),
            'a' => answer_invite(game, true),
            'r' => answer_invite(game, false),
            _ => {}
        }
        return;
    };
    if c == 's' {
        game.clan.ui.mode = Mode::Confirm(Action::Leave);
    } else if "ikpd".contains(c) && !clan.is_leader() {
        game.clan.status = Some("Só o líder do clã pode fazer isso.".into());
    } else if c == 'i' {
        open_picker(game);
    } else if c == 'd' {
        game.clan.ui.mode = Mode::Confirm(Action::Disband);
    } else if c == 'k' || c == 'p' {
        let selected = sorted_members(clan).get(game.clan.ui.cursor).map(|m| m.id);
        match selected {
            Some(id) if id != clan.you => game.clan.ui.mode = Mode::Confirm(if c == 'k' { Action::Kick(id) } else { Action::Promote(id) }),
            _ => game.clan.status = Some("Escolha outro membro com ↑↓.".into()),
        }
    }
}

fn open_picker(game: &mut Game) {
    if candidates(game).is_empty() {
        game.clan.status = Some("Ninguém disponível: só jogadores online sem clã.".into());
        return;
    }
    game.clan.ui.picker = 0;
    game.clan.ui.mode = Mode::Invite;
}

fn answer_invite(game: &mut Game, accept: bool) {
    let Some(invite) = game.clan.invites.get(game.clan.ui.cursor) else {
        game.clan.status = Some("Nenhum convite selecionado.".into());
        return;
    };
    let clan = invite.clan;
    send(game, if accept { ClanCmd::Accept { clan } } else { ClanCmd::Decline { clan } });
}

fn create_key(game: &mut Game, mut form: CreateForm, code: KeyCode, plain: bool) {
    let mut submit = false;
    match (code, form.field) {
        (KeyCode::Esc, _) => return,
        (KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down, _) => form.field = other_field(form.field),
        (KeyCode::Enter, Field::Name) => form.field = Field::Tag,
        (KeyCode::Enter, Field::Tag) => submit = true,
        (KeyCode::Backspace, Field::Name) => drop(form.name.pop()),
        (KeyCode::Backspace, Field::Tag) => drop(form.tag.pop()),
        (KeyCode::Char(c), Field::Name) if plain && !c.is_control() && form.name.chars().count() < CLAN_NAME_MAX => form.name.push(c),
        (KeyCode::Char(c), Field::Tag) if plain && c.is_ascii_alphanumeric() && form.tag.chars().count() < CLAN_TAG_MAX => {
            form.tag.push(c.to_ascii_uppercase())
        }
        _ => {}
    }
    if submit {
        match valid_clan_name(&form.name).and_then(|name| valid_clan_tag(&form.tag).map(|tag| (name, tag))) {
            Ok((name, tag)) => return send(game, ClanCmd::Create { name, tag }),
            Err(text) => game.clan.status = Some(text),
        }
    }
    game.clan.ui.mode = Mode::Create(form);
}

fn other_field(field: Field) -> Field {
    match field {
        Field::Name => Field::Tag,
        Field::Tag => Field::Name,
    }
}

fn invite_key(game: &mut Game, code: KeyCode) {
    let players: Vec<PlayerId> = candidates(game).iter().map(|(id, _)| *id).collect();
    let picker = &mut game.clan.ui.picker;
    match code {
        KeyCode::Esc => return,
        KeyCode::Up => *picker = step(*picker, -1, players.len()),
        KeyCode::Down => *picker = step(*picker, 1, players.len()),
        KeyCode::Enter => {
            if let Some(&target) = players.get(*picker) {
                return send(game, ClanCmd::Invite { target });
            }
        }
        _ => {}
    }
    game.clan.ui.mode = Mode::Invite;
}

fn confirm_key(game: &mut Game, action: Action, code: KeyCode) {
    match code {
        KeyCode::Char('s' | 'S' | 'y' | 'Y') | KeyCode::Enter => send(game, action.command()),
        KeyCode::Esc | KeyCode::Char('n' | 'N') => {}
        _ => game.clan.ui.mode = Mode::Confirm(action),
    }
}
