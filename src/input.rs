//! Keyboard handling for every screen.

use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Confirm, Screen};
use crate::constants::{HELP_PAGE_STEP, MACROS_INPUT_LEN, MAX_NAME_LEN, WEB_MAX_PASSWORD};
use crate::duel::{DuelKey, Minigame, Phase};
use crate::game::{Game, Overlay};
use crate::help::PAGES;
use crate::items::consumable_for_key;
use crate::protocol::ClientMsg;

/// Applies a key press; returns false once the app should exit.
/// Keys typed on the PC: while a phone has taken over, only R (take back) and Ctrl+C work.
pub fn handle_local_key(app: &mut App, key: KeyEvent, now: Instant) -> bool {
    let ctrl_c = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
    if app.phone_controls() && !ctrl_c {
        if matches!(key.code, KeyCode::Char('r' | 'R')) {
            app.drop_phone("O PC retomou o controle.");
        }
        return true;
    }
    handle_key(app, key, now)
}

pub fn handle_key(app: &mut App, key: KeyEvent, now: Instant) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.request_quit();
        return !app.quit;
    }
    if app.confirm.is_some() {
        match key.code {
            KeyCode::Char('s' | 'S' | 'y' | 'Y') | KeyCode::Enter => app.answer(true),
            KeyCode::Char('n' | 'N') | KeyCode::Esc => app.answer(false),
            _ => {}
        }
        return !app.quit;
    }
    if let Some(typed) = app.editing_phone_password.as_mut() {
        match key.code {
            KeyCode::Enter => app.finish_phone_password_edit(),
            KeyCode::Esc => app.editing_phone_password = None,
            KeyCode::Backspace => {
                typed.pop();
            }
            KeyCode::Char(c) if !c.is_control() && typed.chars().count() < WEB_MAX_PASSWORD => typed.push(c),
            _ => {}
        }
        return true;
    }
    if let Some(edit) = app.editing_world.as_mut() {
        match key.code {
            KeyCode::Enter => app.finish_world_edit(),
            KeyCode::Esc => app.editing_world = None,
            KeyCode::Backspace => {
                edit.typed.pop();
            }
            KeyCode::Char(c) if !c.is_control() && edit.typed.chars().count() < MAX_NAME_LEN => edit.typed.push(c),
            _ => {}
        }
        return true;
    }
    if let Some(typed) = app.editing_name.as_mut() {
        match key.code {
            KeyCode::Enter => {
                app.finish_name_edit();
            }
            KeyCode::Esc => app.editing_name = None,
            KeyCode::Backspace => {
                typed.pop();
            }
            KeyCode::Char(c) if !c.is_control() && typed.chars().count() < MAX_NAME_LEN => typed.push(c),
            _ => {}
        }
        return true;
    }

    // Chat and clan windows read raw keys: they have text boxes where case matters.
    if app.screen == Screen::Playing && app.game.duel.is_none() {
        match app.game.overlay {
            Overlay::Chat => {
                crate::ui::chat::handle_key(app, key, now);
                return !app.quit;
            }
            Overlay::Clan => {
                crate::ui::clan::handle_key(app, key, now);
                return !app.quit;
            }
            _ => {}
        }
    }

    // A fast Esc + key arrives as Alt + key: treat it as closing the overlay first.
    if key.modifiers.contains(KeyModifiers::ALT) && app.game.overlay != Overlay::None {
        app.game.overlay = Overlay::None;
    }
    let code = match key.code {
        KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
        other => other,
    };
    match app.screen {
        Screen::Lobby => lobby_key(app, code),
        Screen::Playing if app.game.duel.is_some() => duel_key(app, code),
        Screen::Playing => playing_key(app, code, now),
    }
    !app.quit
}

fn lobby_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Up | KeyCode::Char('k') => app.lobby_move(-1),
        KeyCode::Down | KeyCode::Char('j') => app.lobby_move(1),
        KeyCode::Enter | KeyCode::Char(' ') => app.lobby_enter(),
        KeyCode::Char('n') => app.start_name_edit(),
        KeyCode::Char('c') => app.start_phone_password_edit(),
        KeyCode::Char('e') => app.toggle_phone_mode(),
        KeyCode::Char('x') => app.drop_phone("O PC desconectou o celular."),
        KeyCode::Char('u') => app.ask_update(),
        KeyCode::Char('p') => app.ask_stop_selected(),
        KeyCode::Char('r') => app.start_world_rename(),
        KeyCode::Char('a') | KeyCode::Delete => app.ask_delete_world(),
        KeyCode::Char('q') | KeyCode::Esc => app.request_quit(),
        _ => {}
    }
}

fn duel_key(app: &mut App, code: KeyCode) {
    let decided = app.game.duel.as_ref().is_none_or(|d| d.phase == Phase::Finished);
    if app.game.duel.as_ref().is_some_and(|d| d.phase == Phase::Draft) {
        let game = &mut app.game;
        match code {
            KeyCode::Left | KeyCode::Char('a') => game.move_draft_cursor(-1, 0),
            KeyCode::Right | KeyCode::Char('d') => game.move_draft_cursor(1, 0),
            KeyCode::Up | KeyCode::Char('w') => game.move_draft_cursor(0, -1),
            KeyCode::Down | KeyCode::Char('s') => game.move_draft_cursor(0, 1),
            KeyCode::Enter | KeyCode::Char(' ') => game.pick_minigame(),
            KeyCode::Esc | KeyCode::Char('q') => app.confirm = Some(Confirm::Forfeit),
            _ => {}
        }
        return;
    }
    let typing = app.game.duel.as_ref().is_some_and(|d| {
        d.game == Minigame::Typing && matches!(d.phase, Phase::Countdown | Phase::Show | Phase::Play)
    });
    let macros = app.game.duel.as_ref().is_some_and(|d| {
        d.game == Minigame::Macros && matches!(d.phase, Phase::Countdown | Phase::Show | Phase::Play)
    });
    if macros {
        let input = &mut app.game.macros_input;
        match code {
            KeyCode::Char(c @ ('0'..='9' | '-')) if input.len() < MACROS_INPUT_LEN => input.push(c),
            KeyCode::Backspace => _ = input.pop(),
            KeyCode::Enter | KeyCode::Char(' ') => app.game.submit_macros(Instant::now()),
            KeyCode::Esc => app.confirm = Some(Confirm::Forfeit),
            _ => {}
        }
        return;
    }
    if typing {
        // Every letter types, so only Esc gives up.
        match code {
            KeyCode::Char(c) if !c.is_control() => app.game.duel_key(DuelKey::Char(c), Instant::now()),
            KeyCode::Esc => app.confirm = Some(Confirm::Forfeit),
            _ => {}
        }
        return;
    }
    let key = match code {
        KeyCode::Up | KeyCode::Char('w') => DuelKey::Up,
        KeyCode::Down | KeyCode::Char('s') => DuelKey::Down,
        KeyCode::Left | KeyCode::Char('a') => DuelKey::Left,
        KeyCode::Right | KeyCode::Char('d') => DuelKey::Right,
        KeyCode::Char(' ') | KeyCode::Enter => DuelKey::Action,
        KeyCode::Char(c @ '1'..='9') => DuelKey::Char(c),
        KeyCode::Esc | KeyCode::Char('q') if !decided => {
            app.confirm = Some(Confirm::Forfeit);
            return;
        }
        _ => return,
    };
    app.game.duel_key(key, Instant::now());
}

fn playing_key(app: &mut App, code: KeyCode, now: Instant) {
    let game = &mut app.game;
    if game.overlay == Overlay::None && !game.invites.is_empty() {
        match code {
            KeyCode::Char('a') => return game.answer_invite(true),
            KeyCode::Char('r') | KeyCode::Esc => return game.answer_invite(false),
            _ => {}
        }
    }
    match game.overlay {
        Overlay::Shop => shop_key(game, code),
        Overlay::Help => help_key(game, code),
        Overlay::Achievements | Overlay::Ranks => list_key(game, code),
        Overlay::Chat | Overlay::Clan => {} // handled above with raw keys
        Overlay::Stake => match code {
            KeyCode::Left | KeyCode::Char('h') => game.stake_move(-1),
            KeyCode::Right | KeyCode::Char('l') => game.stake_move(1),
            KeyCode::Up | KeyCode::Char('k') => game.stake_row_move(-1),
            KeyCode::Down | KeyCode::Tab | KeyCode::Char('j') => game.stake_row_move(1),
            KeyCode::Enter | KeyCode::Char(' ') => game.confirm_stake(),
            KeyCode::Esc | KeyCode::Char('q') => game.overlay = Overlay::None,
            _ => {}
        },
        Overlay::Server => match code {
            KeyCode::Char('d') => app.confirm = Some(Confirm::Disconnect),
            KeyCode::Char('p') => {
                if let Some(port) = app.current_host().map(|h| h.port) {
                    app.confirm = Some(Confirm::StopServer(port));
                }
            }
            KeyCode::Char('n') => {
                game.overlay = Overlay::None;
                app.start_name_edit();
            }
            KeyCode::Char('b') => app.toggle_notifications(),
            KeyCode::Char('c') => {
                game.overlay = Overlay::None;
                app.start_phone_password_edit();
            }
            KeyCode::Char('e') => app.toggle_phone_mode(),
            KeyCode::Char('x') => app.drop_phone("O PC desconectou o celular."),
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('s' | 'q') => game.overlay = Overlay::None,
            _ => {}
        },
        Overlay::None => match code {
            KeyCode::Char('q') => app.request_quit(),
            KeyCode::Char(' ') | KeyCode::Enter => game.try_mog(now),
            KeyCode::Up | KeyCode::Left | KeyCode::Char('k') => game.select_move(-1),
            KeyCode::Down | KeyCode::Right | KeyCode::Char('j') => game.select_move(1),
            KeyCode::Char('m') => game.mew(),
            KeyCode::Char('l') => game.overlay = Overlay::Shop,
            KeyCode::Char('c') => game.open_list(Overlay::Achievements),
            KeyCode::Char('p') => game.open_list(Overlay::Ranks),
            KeyCode::Char('s') => game.overlay = Overlay::Server,
            KeyCode::Char('d') => game.open_challenge(),
            KeyCode::Char('f') => game.toggle_queue(),
            KeyCode::Char('t') => game.overlay = Overlay::Chat,
            KeyCode::Char('g') => {
                game.overlay = Overlay::Clan;
                game.outbox.push(ClientMsg::Clan(crate::clan::ClanCmd::Refresh));
            }
            KeyCode::Char('x') if game.outgoing.is_some() => game.outbox.push(ClientMsg::CancelChallenge),
            KeyCode::Char('?' | 'h') => game.overlay = Overlay::Help,
            KeyCode::Char(c) => {
                if let Some(id) = consumable_for_key(c) {
                    game.use_item(id);
                }
            }
            _ => {}
        },
    }
}

fn shop_key(game: &mut Game, code: KeyCode) {
    if game.shop_confirm.is_some() {
        match code {
            KeyCode::Enter | KeyCode::Char(' ' | 's' | 'y') => game.shop_enter(),
            KeyCode::Esc | KeyCode::Char('n' | 'q') => game.shop_back(),
            _ => {}
        }
        return;
    }
    match code {
        KeyCode::Up | KeyCode::Char('k') => {
            game.shop_result = None;
            game.shop_move(-1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            game.shop_result = None;
            game.shop_move(1);
        }
        KeyCode::Left | KeyCode::BackTab | KeyCode::Char('h') => {
            game.shop_result = None;
            game.shop_tab(-1);
        }
        KeyCode::Right | KeyCode::Tab => {
            game.shop_result = None;
            game.shop_tab(1);
        }
        KeyCode::Enter | KeyCode::Char(' ') => game.shop_enter(),
        KeyCode::Esc | KeyCode::Char('l' | 'q') => game.shop_back(),
        _ => {}
    }
}

fn list_key(game: &mut Game, code: KeyCode) {
    match code {
        KeyCode::Up | KeyCode::Char('k') => game.list_scroll_by(-1),
        KeyCode::Down | KeyCode::Char('j') => game.list_scroll_by(1),
        KeyCode::PageUp => game.list_scroll_by(-HELP_PAGE_STEP),
        KeyCode::PageDown | KeyCode::Char(' ') => game.list_scroll_by(HELP_PAGE_STEP),
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('c' | 'p' | '?' | 'h' | 'q') => game.overlay = Overlay::None,
        _ => {}
    }
}

fn help_key(game: &mut Game, code: KeyCode) {
    match code {
        KeyCode::Left | KeyCode::BackTab | KeyCode::Char('h') => game.help_turn(-1),
        KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => game.help_turn(1),
        KeyCode::Up | KeyCode::Char('k') => game.help_scroll_by(-1),
        KeyCode::Down | KeyCode::Char('j') => game.help_scroll_by(1),
        KeyCode::PageUp => game.help_scroll_by(-HELP_PAGE_STEP),
        KeyCode::PageDown | KeyCode::Char(' ') => game.help_scroll_by(HELP_PAGE_STEP),
        KeyCode::Char(c @ '1'..='9') => {
            let page = c as usize - '1' as usize;
            if page < PAGES.len() {
                game.help_page = page;
                game.help_scroll = 0;
            }
        }
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?' | 'q') => game.overlay = Overlay::None,
        _ => {}
    }
}
