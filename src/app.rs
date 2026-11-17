//! Top-level client state: lobby, hosting, connection and the game screen.

use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Instant;

use crate::connection::{Connection, Incoming, Liveness};
use crate::discovery::{Discovery, ServerEntry};
use crate::game::{Game, Overlay, Player, Tone};
use crate::notify::{Notice, Notifier};
use crate::protocol::{ClientMsg, ServerMsg};
use crate::server::{Ports, ServerHandle};
use crate::constants::{PROTOCOL_VERSION, WEB_MIN_PASSWORD};
use crate::update;
use crate::utils::clean_name;
use crate::web::Remote;
use crate::worlds::{WorldSave, WorldStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Lobby,
    Playing,
}

/// Questions that need a yes/no before acting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    QuitWhileHosting,
    /// Stop the world hosted on this port.
    StopServer(u16),
    /// Send this saved world to the trash.
    DeleteWorld(u64),
    Disconnect,
    Forfeit,
    QuitWhileDueling,
    Update,
}

/// One row of the lobby list.
#[derive(Debug, Clone, PartialEq)]
pub enum LobbyRow {
    /// A world this computer is hosting, by port.
    Own(u16),
    Server(ServerEntry),
    /// A saved world that is not running.
    World(u64),
    /// Create a new world and host it.
    Host,
}

/// World name being typed: a new world, or renaming the saved one with this id.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldEdit {
    pub id: Option<u64>,
    pub typed: String,
}

pub struct App {
    pub screen: Screen,
    pub game: Game,
    pub name: String,
    pub machine_id: String,
    pub lobby_cursor: usize,
    /// Name being typed in the lobby, if the editor is open.
    pub editing_name: Option<String>,
    pub notice: Option<(Tone, String)>,
    pub confirm: Option<Confirm>,
    pub conn: Option<Connection>,
    liveness: Liveness,
    /// Worlds this computer is hosting right now.
    pub hosts: Vec<ServerHandle>,
    pub discovery: Option<Discovery>,
    /// Where worlds are saved; `None` keeps new worlds in memory.
    pub store: Option<WorldStore>,
    /// Saved worlds, refreshed after each change.
    pub worlds: Vec<WorldSave>,
    pub editing_world: Option<WorldEdit>,
    pub ports: Ports,
    pub quit: bool,
    /// Set when the name changed and should be written to the local config.
    pub name_changed: bool,
    pub notifier: Notifier,
    /// Set when the notification toggle changed and should be written to the local config.
    pub notifications_changed: bool,
    /// This build's release number; updates are off when it is 0.
    pub release: u64,
    /// Newest release the player said no to, so the question is not repeated.
    pub update_declined: u64,
    update_job: Option<Receiver<Result<u64, String>>>,
    /// Set once a new build is installed; the binary restarts when the UI closes.
    pub restart: bool,
    /// Phone access to this PMOG; `None` in tests and bots.
    pub remote: Option<Remote>,
    /// Phone password being typed, if the editor is open.
    pub editing_phone_password: Option<String>,
}

impl App {
    pub fn new(name: String, machine_id: String, discovery: Option<Discovery>, store: Option<WorldStore>, ports: Ports) -> App {
        let now = Instant::now();
        let notice = discovery.is_none().then(|| {
            (Tone::Bad, "Não consegui escutar a rede: só dá para hospedar ou conectar por aqui mesmo.".to_string())
        });
        App {
            screen: Screen::Lobby,
            game: Game::new(Player { name: name.clone(), ..Default::default() }, now),
            name,
            machine_id,
            lobby_cursor: 0,
            editing_name: None,
            notice,
            confirm: None,
            conn: None,
            liveness: Liveness::new(now),
            hosts: Vec::new(),
            discovery,
            worlds: store.as_ref().map(WorldStore::list).unwrap_or_default(),
            store,
            editing_world: None,
            ports,
            quit: false,
            name_changed: false,
            notifier: Notifier::silent(now),
            notifications_changed: false,
            release: if update::enabled() { update::release() } else { 0 },
            update_declined: 0,
            update_job: None,
            restart: false,
            remote: None,
            editing_phone_password: None,
        }
    }

    /// Hosted worlds, servers found on the network, saved worlds and the "new world" option.
    pub fn lobby_rows(&self) -> Vec<LobbyRow> {
        let mut rows: Vec<LobbyRow> = self.hosts.iter().map(|h| LobbyRow::Own(h.port)).collect();
        if let Some(d) = self.discovery.as_ref() {
            rows.extend(
                d.servers()
                    .iter()
                    .filter(|s| !self.hosts.iter().any(|h| s.addr.port() == h.port && s.name == h.name))
                    .cloned()
                    .map(LobbyRow::Server),
            );
        }
        rows.extend(self.worlds.iter().filter(|w| self.hosted_world(w).is_none()).map(|w| LobbyRow::World(w.id)));
        rows.push(LobbyRow::Host);
        rows
    }

    pub fn host_on(&self, port: u16) -> Option<&ServerHandle> {
        self.hosts.iter().find(|h| h.port == port)
    }

    /// The running server of a saved world, if it is hosted.
    pub fn hosted_world(&self, world: &WorldSave) -> Option<&ServerHandle> {
        self.hosts.iter().find(|h| h.db_path.as_ref() == Some(&world.path))
    }

    pub fn world(&self, id: u64) -> Option<&WorldSave> {
        self.worlds.iter().find(|w| w.id == id)
    }

    /// The hosted world this client is playing in, if any.
    pub fn current_host(&self) -> Option<&ServerHandle> {
        let addr = self.conn.as_ref()?.addr;
        self.hosts.iter().find(|h| addr.ip().is_loopback() && addr.port() == h.port)
    }

    pub fn selected_row(&self) -> Option<LobbyRow> {
        self.lobby_rows().get(self.lobby_cursor).cloned()
    }

    pub fn lobby_move(&mut self, delta: isize) {
        let len = self.lobby_rows().len().max(1) as isize;
        self.lobby_cursor = (self.lobby_cursor as isize + delta).rem_euclid(len) as usize;
    }

    /// Acts on the selected lobby row.
    pub fn lobby_enter(&mut self) {
        match self.lobby_rows().get(self.lobby_cursor).cloned() {
            Some(LobbyRow::Own(port)) => {
                if let Some(addr) = self.host_on(port).map(|h| h.local_addr()) {
                    self.connect(addr);
                }
            }
            Some(LobbyRow::World(id)) => {
                if let Some(world) = self.world(id).cloned() {
                    self.host_world(&world);
                }
            }
            Some(LobbyRow::Server(entry)) if !entry.compatible && self.offered(&entry).is_some() => {
                self.update_declined = 0;
                self.confirm = Some(Confirm::Update);
            }
            Some(LobbyRow::Server(entry)) if !entry.compatible => {
                let reason = update::version_refusal((PROTOCOL_VERSION, self.release), (PROTOCOL_VERSION, entry.release));
                let text = reason.unwrap_or("Esse servidor está com outra versão do PMOG e não tem atualização para você.");
                self.notice = Some((Tone::Bad, text.into()));
            }
            Some(LobbyRow::Server(entry)) => self.connect(entry.addr),
            Some(LobbyRow::Host) => self.editing_world = Some(WorldEdit { id: None, typed: format!("Servidor de {}", self.name) }),
            None => {}
        }
    }

    /// Starts a server for `name` saved at `path` (in memory without one) and joins it.
    fn host(&mut self, name: &str, path: Option<std::path::PathBuf>) {
        match ServerHandle::start(name, path, self.ports) {
            Ok(handle) => {
                let addr = handle.local_addr();
                self.hosts.push(handle);
                self.connect(addr);
            }
            Err(e) => self.notice = Some((Tone::Bad, format!("Não consegui hospedar: {e}"))),
        }
    }

    pub fn host_world(&mut self, world: &WorldSave) {
        self.host(&world.name, Some(world.path.clone()));
    }

    /// Creates a world called `name`, saved when there is a store, and hosts it.
    pub fn host_new_world(&mut self, name: &str) {
        let Some(store) = self.store.clone() else {
            self.host(name, None);
            return;
        };
        match store.create(name) {
            Ok(world) => {
                self.refresh_worlds();
                self.host_world(&world);
            }
            Err(e) => self.notice = Some((Tone::Bad, format!("Não consegui criar o mundo: {e}"))),
        }
    }

    pub fn refresh_worlds(&mut self) {
        self.worlds = self.store.as_ref().map(WorldStore::list).unwrap_or_default();
    }

    /// Opens the name editor to rename the selected saved world.
    pub fn start_world_rename(&mut self) {
        match self.selected_row() {
            Some(LobbyRow::World(id)) => {
                let typed = self.world(id).map_or(String::new(), |w| w.name.clone());
                self.editing_world = Some(WorldEdit { id: Some(id), typed });
            }
            Some(LobbyRow::Own(_)) => self.notice = Some((Tone::Info, "Pare o mundo (P) para renomear.".into())),
            _ => {}
        }
    }

    /// Creates or renames a world with the typed name.
    pub fn finish_world_edit(&mut self) {
        let Some(edit) = self.editing_world.take() else { return };
        let name = clean_name(edit.typed.trim());
        if name.is_empty() {
            return;
        }
        let Some(id) = edit.id else {
            self.host_new_world(&name);
            return;
        };
        let result = match (self.store.as_ref(), self.world(id)) {
            (Some(store), Some(world)) => store.rename(world, &name),
            _ => return,
        };
        if let Err(e) = result {
            self.notice = Some((Tone::Bad, format!("Não consegui renomear: {e}")));
        }
        self.refresh_worlds();
    }

    /// Asks before sending the selected saved world to the trash.
    pub fn ask_delete_world(&mut self) {
        match self.selected_row() {
            Some(LobbyRow::World(id)) => self.confirm = Some(Confirm::DeleteWorld(id)),
            Some(LobbyRow::Own(_)) => self.notice = Some((Tone::Info, "Pare o mundo (P) antes de apagar.".into())),
            _ => {}
        }
    }

    fn delete_world(&mut self, id: u64) {
        let result = match (self.store.as_ref(), self.world(id)) {
            (Some(store), Some(world)) => store.trash(world).map(|()| (world.name.clone(), store.trash.display().to_string())),
            _ => return,
        };
        self.notice = Some(match result {
            Ok((name, trash)) => (Tone::Info, format!("{name} foi para a lixeira do PMOG ({trash}).")),
            Err(e) => (Tone::Bad, format!("Não consegui apagar: {e}")),
        });
        self.refresh_worlds();
        self.lobby_cursor = self.lobby_cursor.min(self.lobby_rows().len().saturating_sub(1));
    }

    /// Asks before stopping the selected hosted world.
    pub fn ask_stop_selected(&mut self) {
        if let Some(LobbyRow::Own(port)) = self.selected_row() {
            self.confirm = Some(Confirm::StopServer(port));
        }
    }

    pub fn connect(&mut self, addr: SocketAddr) {
        match Connection::open(addr, &self.machine_id, &self.name) {
            Ok(conn) => {
                let now = Instant::now();
                self.game = Game::new(Player { name: self.name.clone(), ..Default::default() }, now);
                self.game.log(Tone::Info, format!("Conectado em {}. Aperte ? para ver as regras.", conn.server_name));
                self.conn = Some(conn);
                self.liveness = Liveness::new(now);
                self.screen = Screen::Playing;
                self.notice = None;
            }
            Err(reason) => self.notice = Some((Tone::Bad, reason)),
        }
    }

    /// Leaves the server but keeps hosting if this computer is the host.
    pub fn disconnect(&mut self, notice: Option<(Tone, String)>) {
        if let Some(conn) = self.conn.take() {
            conn.close();
        }
        self.screen = Screen::Lobby;
        self.game.overlay = Overlay::None;
        self.lobby_cursor = 0;
        self.notice = notice;
    }

    /// Stops the world hosted on `port`, leaving it first when playing there.
    pub fn stop_hosting(&mut self, port: u16) {
        let notice = Some((Tone::Info, "Servidor parado. O progresso de todos ficou salvo.".into()));
        if self.current_host().is_some_and(|h| h.port == port) {
            self.disconnect(notice);
        } else {
            self.notice = notice;
        }
        if let Some(i) = self.hosts.iter().position(|h| h.port == port) {
            self.hosts.remove(i).stop();
        }
        self.lobby_cursor = self.lobby_cursor.min(self.lobby_rows().len().saturating_sub(1));
    }

    /// Players other than this one online across every hosted world.
    pub fn hosted_others(&self) -> usize {
        let mine = usize::from(self.current_host().is_some());
        self.hosts.iter().map(|h| h.status().online.len()).sum::<usize>().saturating_sub(mine)
    }

    /// Asks before anything that would kick other players.
    pub fn request_quit(&mut self) {
        if self.dueling() {
            self.confirm = Some(Confirm::QuitWhileDueling);
            return;
        }
        if self.hosted_others() > 0 {
            self.confirm = Some(Confirm::QuitWhileHosting);
        } else {
            self.quit = true;
        }
    }

    pub fn answer(&mut self, yes: bool) {
        let Some(question) = self.confirm.take() else { return };
        if !yes {
            if let (Confirm::Update, Some((_, release))) = (question, self.update_offer()) {
                self.update_declined = release;
                self.notice = Some((Tone::Info, "Atualização adiada. Aperte U aqui no lobby quando quiser.".into()));
            }
            return;
        }
        match question {
            Confirm::QuitWhileHosting | Confirm::QuitWhileDueling => self.quit = true,
            Confirm::Forfeit => self.game.outbox.push(ClientMsg::Forfeit),
            Confirm::StopServer(port) => self.stop_hosting(port),
            Confirm::DeleteWorld(id) => self.delete_world(id),
            Confirm::Disconnect => self.disconnect(None),
            Confirm::Update => self.start_update(),
        }
    }

    /// Release `entry` offers that is newer than this build.
    fn offered(&self, entry: &ServerEntry) -> Option<u64> {
        entry.update.filter(|r| self.release > 0 && *r > self.release)
    }

    /// The server with the newest build for this computer, if any is newer.
    pub fn update_offer(&self) -> Option<(ServerEntry, u64)> {
        let servers = self.discovery.as_ref().map_or(&[][..], |d| d.servers());
        servers.iter().filter_map(|s| self.offered(s).map(|r| (s.clone(), r))).max_by_key(|(_, r)| *r)
    }

    /// Opens the update question again after the player said no.
    pub fn ask_update(&mut self) {
        if self.update_offer().is_some() && self.update_job.is_none() {
            self.update_declined = 0;
            self.confirm = Some(Confirm::Update);
        }
    }

    fn start_update(&mut self) {
        let Some((entry, release)) = self.update_offer() else { return };
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(update::update_from(entry.addr));
        });
        self.update_job = Some(rx);
        self.update_declined = release;
        self.notice = Some((Tone::Info, format!("Baixando a versão nova de {}...", entry.name)));
    }

    /// Offers a newer build once per release and follows a running download.
    fn step_update(&mut self) {
        if let Some(job) = &self.update_job {
            match job.try_recv() {
                Ok(Ok(release)) => {
                    self.notice = Some((Tone::Good, format!("Atualizado para {}. Reabrindo...", update::release_label(release))));
                    self.restart = true;
                    self.update_job = None;
                }
                Ok(Err(reason)) => {
                    self.notice = Some((Tone::Bad, format!("Atualização falhou: {reason}")));
                    self.update_job = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.update_job = None,
            }
            return;
        }
        let pending = self.update_offer().filter(|(_, r)| *r > self.update_declined);
        if pending.is_some() && self.screen == Screen::Lobby && self.confirm.is_none() && self.editing_name.is_none() && self.editing_world.is_none() {
            self.confirm = Some(Confirm::Update);
        }
    }

    /// True while a duel is still being played (not just its result screen).
    pub fn dueling(&self) -> bool {
        self.game.duel.as_ref().is_some_and(|d| d.i_won.is_none())
    }

    pub fn start_name_edit(&mut self) {
        self.editing_name = Some(self.name.clone());
    }

    /// Applies the typed name here and on the server.
    pub fn finish_name_edit(&mut self) -> Option<String> {
        let typed = self.editing_name.take()?;
        let name = clean_name(typed.trim());
        if name.is_empty() {
            return None;
        }
        self.name = name.clone();
        self.name_changed = true;
        self.game.player.name = name.clone();
        if let Some(conn) = self.conn.as_mut() {
            conn.send(&ClientMsg::Rename { name: name.clone() });
        }
        Some(name)
    }

    /// True while a phone runs this PMOG and the PC only watches.
    pub fn phone_controls(&self) -> bool {
        self.remote.as_ref().is_some_and(|r| r.controls())
    }

    pub fn start_phone_password_edit(&mut self) {
        if self.remote.as_ref().is_some_and(|r| r.port > 0) {
            self.editing_phone_password = Some(String::new());
        } else {
            self.notice = Some((Tone::Bad, "Nenhuma porta livre para o celular (47800 a 47809).".into()));
        }
    }

    /// Saves the typed phone password, signing every phone out.
    pub fn finish_phone_password_edit(&mut self) {
        let Some(typed) = self.editing_phone_password.take() else { return };
        let Some(remote) = self.remote.as_mut() else { return };
        let (tone, text) = if typed.chars().count() < WEB_MIN_PASSWORD {
            (Tone::Bad, format!("A senha do celular precisa de pelo menos {WEB_MIN_PASSWORD} caracteres."))
        } else {
            remote.set_password(&typed);
            (Tone::Good, "Senha do celular salva. Os celulares precisam digitar ela de novo.".to_string())
        };
        self.say(tone, text);
    }

    pub fn toggle_phone_mode(&mut self) {
        if let Some(remote) = self.remote.as_mut() {
            remote.toggle_takeover();
            let text = if remote.takeover() { "Celular agora assume o jogo: o PC só assiste enquanto ele estiver conectado." } else { "Celular agora espelha o jogo: PC e celular controlam juntos." };
            self.say(Tone::Info, text.to_string());
        }
    }

    /// Disconnects the phone so the PC is in control again.
    pub fn drop_phone(&mut self, reason: &str) {
        if let Some(remote) = self.remote.as_mut().filter(|r| r.connected()) {
            remote.kick(reason);
            self.say(Tone::Info, "Celular desconectado.".to_string());
        }
    }

    /// Shows a message in the feed while playing, or as the lobby notice.
    fn say(&mut self, tone: Tone, text: String) {
        if self.screen == Screen::Playing {
            self.game.log(tone, text);
        } else {
            self.notice = Some((tone, text));
        }
    }

    pub fn toggle_notifications(&mut self) {
        self.notifier.enabled = !self.notifier.enabled;
        self.notifications_changed = true;
    }

    /// Network in, actions out; call once per frame.
    pub fn step(&mut self, now: Instant) {
        crate::web::pump(self, now);
        self.notifier.pump(std::mem::take(&mut self.game.notices), now);
        if let Some(discovery) = self.discovery.as_mut() {
            discovery.refresh(now);
        }
        let rows = self.lobby_rows().len();
        if self.lobby_cursor >= rows {
            self.lobby_cursor = rows.saturating_sub(1);
        }
        self.step_update();

        let Some(conn) = self.conn.as_mut() else { return };
        let mut lost = None;
        let incoming = conn.drain();
        if self.liveness.is_dead(!incoming.is_empty(), now) {
            lost = Some("O servidor parou de responder.".to_string());
        }
        for incoming in incoming {
            match incoming {
                Incoming::Msg(ServerMsg::Shutdown) => {
                    lost = Some("O servidor foi desligado. Seu progresso ficou salvo nele.".to_string());
                }
                Incoming::Msg(ServerMsg::Rejected { reason }) => lost = Some(reason),
                Incoming::Msg(msg) => self.game.apply(msg, now),
                Incoming::Closed => {
                    lost.get_or_insert_with(|| "A conexão com o servidor caiu.".to_string());
                }
            }
        }
        self.game.predict_duel(now);
        self.game.note_expired_buffs(now);
        for msg in std::mem::take(&mut self.game.outbox) {
            if !conn.send(&msg) {
                lost.get_or_insert_with(|| "A conexão com o servidor caiu.".to_string());
                break;
            }
        }
        if let Some(reason) = lost {
            self.game.notices.push(Notice::disconnected(&reason));
            self.disconnect(Some((Tone::Bad, reason)));
        }
    }

    /// Stops hosting (saving everything) before the program ends.
    pub fn shutdown(&mut self) {
        self.remote = None;
        if let Some(conn) = self.conn.take() {
            conn.close();
        }
        for host in self.hosts.drain(..) {
            host.stop();
        }
    }
}
