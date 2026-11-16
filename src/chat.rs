//! Chat between players: Geral, private messages, groups and clan channels, stored on the server.

use std::cell::Cell;
use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::constants::*;
use crate::db::Db;
use crate::game::Rank;
use crate::notify::Notice;
use crate::protocol::{PlayerId, ServerMsg};
use crate::world::World;

/// Tables this feature needs; run when the database opens.
pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS chat_messages (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    kind         INTEGER NOT NULL,
    a            INTEGER NOT NULL,
    b            INTEGER NOT NULL,
    sender_id    INTEGER NOT NULL,
    sender_name  TEXT    NOT NULL,
    sender_tag   TEXT,
    sender_rank  INTEGER NOT NULL,
    text         TEXT    NOT NULL,
    at           INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS chat_messages_scope ON chat_messages (kind, a, b, id);
CREATE TABLE IF NOT EXISTS chat_groups (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL,
    created_by  INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS chat_members (
    group_id   INTEGER NOT NULL REFERENCES chat_groups(id) ON DELETE CASCADE,
    player_id  INTEGER NOT NULL,
    PRIMARY KEY (group_id, player_id)
);
CREATE INDEX IF NOT EXISTS chat_members_player ON chat_members (player_id);
CREATE TABLE IF NOT EXISTS chat_reads (
    player_id  INTEGER NOT NULL,
    kind       INTEGER NOT NULL,
    a          INTEGER NOT NULL,
    b          INTEGER NOT NULL,
    last_id    INTEGER NOT NULL,
    PRIMARY KEY (player_id, kind, a, b)
);
";

/// A conversation as the player sees it: `Private` holds the other player's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Conv {
    General,
    Private(PlayerId),
    Group(i64),
    Clan(i64),
}

/// Where a conversation lives in the database: kind and two ids.
type Scope = (i64, i64, i64);

impl Conv {
    fn scope(self, viewer: PlayerId) -> Scope {
        match self {
            Conv::General => (0, 0, 0),
            Conv::Private(peer) => (1, viewer.min(peer), viewer.max(peer)),
            Conv::Group(id) => (2, id, 0),
            Conv::Clan(id) => (3, id, 0),
        }
    }

    /// Position in the conversation list: Geral, clan, groups, then privates.
    fn order(self) -> u8 {
        match self {
            Conv::General => 0,
            Conv::Clan(_) => 1,
            Conv::Group(_) => 2,
            Conv::Private(_) => 3,
        }
    }
}

/// One message with its sender as they were when it was sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMsg {
    pub id: i64,
    pub sender: PlayerId,
    pub name: String,
    pub tag: Option<String>,
    pub rank: Rank,
    pub text: String,
    /// Wall clock seconds, for display only.
    pub at: i64,
}

/// A conversation with its recent history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConvView {
    pub conv: Conv,
    pub title: String,
    /// Group members, by name.
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub unread: u32,
    #[serde(default)]
    pub messages: Vec<ChatMsg>,
}

/// What a client asks the chat to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChatCmd {
    Send { conv: Conv, text: String },
    /// Everything in the conversation up to now has been read.
    Read { conv: Conv },
    /// Starts (or reopens) a private conversation.
    Open { with: PlayerId },
    CreateGroup { name: String },
    AddMember { group: i64, player: PlayerId },
    Leave { group: i64 },
    /// Refreshes what depends on the player's clan.
    Sync {},
}

/// What the chat tells a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChatEvent {
    /// First event after joining: who the player is; conversations follow.
    Ready { me: PlayerId },
    /// Adds or updates a conversation; without messages it only updates title and members.
    Upsert(ConvView),
    Message { conv: Conv, title: String, msg: ChatMsg },
    Removed { conv: Conv },
    Error { text: String },
}

/// Everything the chat may touch while handling one command.
pub struct ChatCtx<'a> {
    pub db: &'a Db,
    pub world: &'a World,
    /// Messages to send, addressed by player.
    pub out: &'a mut Vec<(PlayerId, ServerMsg)>,
    pub now: Instant,
}

impl ChatCtx<'_> {
    fn tell(&mut self, to: PlayerId, event: ChatEvent) {
        self.out.push((to, ServerMsg::Chat(event)));
    }

    fn online(&self, player: PlayerId) -> bool {
        self.world.slot(player).is_some()
    }
}

/// Text without control characters (line breaks become spaces), trimmed.
pub fn clean_text(raw: &str) -> String {
    let spaced: String = raw.chars().map(|c| if c.is_whitespace() { ' ' } else { c }).filter(|c| !c.is_control()).collect();
    spaced.trim().to_string()
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Result of one server step: the error text is shown to the player.
type Res<T = ()> = Result<T, String>;

fn db<T>(result: rusqlite::Result<T>) -> Res<T> {
    result.map_err(|_| "Erro no banco do servidor.".to_string())
}

const NOT_FOUND: &str = "Jogador não encontrado.";
const NOT_IN_GROUP: &str = "Você não faz parte deste grupo.";
const NOT_IN_CLAN: &str = "Você não faz parte deste clã.";

/// Messages still available to one player: refills over time.
struct Bucket {
    tokens: f64,
    at: Instant,
}

/// Server side of the chat.
#[derive(Default)]
pub struct ChatHub {
    buckets: HashMap<PlayerId, Bucket>,
    /// Clan each online player last saw in the chat.
    clans: HashMap<PlayerId, Option<i64>>,
}

impl ChatHub {
    pub fn load(_db: &Db) -> ChatHub {
        ChatHub::default()
    }

    pub fn handle(&mut self, ctx: &mut ChatCtx, from: PlayerId, cmd: ChatCmd) {
        self.sync_clan(ctx, from);
        let result = match cmd {
            ChatCmd::Sync {} => Ok(()),
            ChatCmd::Read { conv } => self.read(ctx, from, conv),
            _ if !self.allow(from, ctx.now) => Err("Calma, você está enviando rápido demais.".to_string()),
            ChatCmd::Send { conv, text } => self.send(ctx, from, conv, &text),
            ChatCmd::Open { with } => self.open(ctx, from, with),
            ChatCmd::CreateGroup { name } => self.create_group(ctx, from, &name),
            ChatCmd::AddMember { group, player } => self.add_member(ctx, from, group, player),
            ChatCmd::Leave { group } => self.leave(ctx, from, group),
        };
        if let Err(text) = result {
            ctx.tell(from, ChatEvent::Error { text });
        }
    }

    /// A player came online (already in the world, clan already set).
    pub fn joined(&mut self, ctx: &mut ChatCtx, player: PlayerId) {
        self.clans.insert(player, None);
        ctx.tell(player, ChatEvent::Ready { me: player });
        let mut convs = vec![Conv::General];
        if let Ok(groups) = ctx.db.chat_groups_of(player) {
            convs.extend(groups.into_iter().map(|(id, _)| Conv::Group(id)));
        }
        if let Ok(peers) = ctx.db.chat_privates_of(player, CHAT_PRIVATE_CONVS) {
            convs.extend(peers.into_iter().map(Conv::Private));
        }
        for conv in convs {
            if let Ok(view) = self.view(ctx, player, conv) {
                ctx.tell(player, ChatEvent::Upsert(view));
            }
        }
        self.sync_clan(ctx, player);
    }

    /// A player is going offline (still in the world).
    pub fn left(&mut self, _ctx: &mut ChatCtx, player: PlayerId) {
        self.buckets.remove(&player);
        self.clans.remove(&player);
    }

    /// Spends one token; false when the player is sending too fast.
    fn allow(&mut self, player: PlayerId, now: Instant) -> bool {
        let bucket = self.buckets.entry(player).or_insert(Bucket { tokens: CHAT_RATE_BURST, at: now });
        let elapsed = now.saturating_duration_since(bucket.at).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * CHAT_RATE_PER_SEC).min(CHAT_RATE_BURST);
        bucket.at = now;
        if bucket.tokens < 1.0 {
            return false;
        }
        bucket.tokens -= 1.0;
        true
    }

    /// Shows or hides the clan channel when the player's clan changed since last time.
    fn sync_clan(&mut self, ctx: &mut ChatCtx, player: PlayerId) {
        let now_clan = ctx.world.clan_of(player).map(|(id, _)| id);
        let before = self.clans.get(&player).copied().flatten();
        if before == now_clan {
            return;
        }
        self.clans.insert(player, now_clan);
        if let Some(old) = before {
            ctx.tell(player, ChatEvent::Removed { conv: Conv::Clan(old) });
        }
        if let Some(id) = now_clan {
            let conv = Conv::Clan(id);
            let scope = conv.scope(player);
            // History from before joining is not unread.
            if matches!(ctx.db.chat_read_mark(player, scope), Ok(None))
                && let Ok(last) = ctx.db.chat_last_id(scope)
            {
                let _ = ctx.db.chat_set_read(player, scope, last);
            }
            if let Ok(view) = self.view(ctx, player, conv) {
                ctx.tell(player, ChatEvent::Upsert(view));
            }
        }
    }

    /// Checks the player may use the conversation and returns where it is stored.
    fn authorize(&self, ctx: &ChatCtx, player: PlayerId, conv: Conv) -> Res<Scope> {
        match conv {
            Conv::General => {}
            Conv::Private(peer) => {
                if peer == player {
                    return Err("Você não pode enviar mensagem para si mesmo.".into());
                }
                if db(ctx.db.chat_player_name(peer))?.is_none() {
                    return Err(NOT_FOUND.into());
                }
            }
            Conv::Group(id) => {
                if !db(ctx.db.chat_is_member(id, player))? {
                    return Err(NOT_IN_GROUP.into());
                }
            }
            Conv::Clan(id) => {
                if ctx.world.clan_of(player).map(|(c, _)| c) != Some(id) {
                    return Err(NOT_IN_CLAN.into());
                }
            }
        }
        Ok(conv.scope(player))
    }

    /// The conversation as `player` sees it, with history and unread count.
    fn view(&self, ctx: &ChatCtx, player: PlayerId, conv: Conv) -> Res<ConvView> {
        let scope = self.authorize(ctx, player, conv)?;
        let (limit, unread) = match conv {
            Conv::General => (CHAT_GENERAL_HISTORY, 0),
            _ => (CHAT_HISTORY, db(ctx.db.chat_unread(player, scope))?),
        };
        let mut view = self.meta(ctx, player, conv)?;
        view.unread = unread;
        view.messages = db(ctx.db.chat_recent(scope, limit))?;
        Ok(view)
    }

    /// Title and members only.
    fn meta(&self, ctx: &ChatCtx, player: PlayerId, conv: Conv) -> Res<ConvView> {
        let (title, members) = match conv {
            Conv::General => ("Geral".to_string(), Vec::new()),
            Conv::Private(peer) => (db(ctx.db.chat_player_name(peer))?.unwrap_or_else(|| "?".into()), Vec::new()),
            Conv::Group(id) => {
                let name = db(ctx.db.chat_group_name(id))?.ok_or(NOT_IN_GROUP)?;
                (name, db(ctx.db.chat_group_members(id))?.into_iter().map(|(_, n)| n).collect())
            }
            Conv::Clan(_) => (ctx.world.clan_of(player).map_or_else(String::new, |(_, tag)| tag.to_string()), Vec::new()),
        };
        Ok(ConvView { conv, title, members, unread: 0, messages: Vec::new() })
    }

    fn read(&mut self, ctx: &mut ChatCtx, player: PlayerId, conv: Conv) -> Res {
        if conv == Conv::General {
            return Ok(());
        }
        let scope = self.authorize(ctx, player, conv)?;
        let last = db(ctx.db.chat_last_id(scope))?;
        db(ctx.db.chat_set_read(player, scope, last))
    }

    /// Everyone who should get a message sent to `conv` by `from`, with the conversation as they see it.
    fn audience(&self, ctx: &ChatCtx, from: PlayerId, conv: Conv, sender_name: &str) -> Res<Vec<(PlayerId, Conv, String)>> {
        let mut to = Vec::new();
        match conv {
            Conv::General => to.extend(ctx.world.online_names().map(|(id, _)| (id, Conv::General, "Geral".to_string()))),
            Conv::Private(peer) => {
                let peer_name = db(ctx.db.chat_player_name(peer))?.ok_or(NOT_FOUND)?;
                to.push((from, conv, peer_name));
                if ctx.online(peer) {
                    to.push((peer, Conv::Private(from), sender_name.to_string()));
                }
            }
            Conv::Group(id) => {
                let title = db(ctx.db.chat_group_name(id))?.ok_or(NOT_IN_GROUP)?;
                for (member, _) in db(ctx.db.chat_group_members(id))? {
                    if ctx.online(member) {
                        to.push((member, conv, title.clone()));
                    }
                }
            }
            Conv::Clan(id) => {
                for (member, _) in ctx.world.online_names() {
                    if let Some((c, tag)) = ctx.world.clan_of(member)
                        && c == id
                    {
                        to.push((member, conv, tag.to_string()));
                    }
                }
            }
        }
        Ok(to)
    }

    fn send(&mut self, ctx: &mut ChatCtx, from: PlayerId, conv: Conv, raw: &str) -> Res {
        let text = clean_text(raw);
        if text.is_empty() {
            return Err("Escreva algo antes de enviar.".into());
        }
        if text.chars().count() > CHAT_MAX_CHARS {
            return Err(format!("Mensagem longa demais (máximo de {CHAT_MAX_CHARS} caracteres)."));
        }
        let scope = self.authorize(ctx, from, conv)?;
        let slot = ctx.world.slot(from).ok_or("Você não está conectado.")?;
        let (name, rank) = (slot.player.name.clone(), slot.player.rank());
        let audience = self.audience(ctx, from, conv, &name)?;
        let tag = ctx.world.clan_of(from).map(|(_, t)| t.to_string());
        let mut msg = ChatMsg { id: 0, sender: from, name, tag, rank, text, at: now_secs() };
        let cap = if conv == Conv::General { CHAT_GENERAL_STORE_CAP } else { CHAT_STORE_CAP };
        msg.id = db(ctx.db.chat_add(scope, &msg, cap))?;
        if conv != Conv::General {
            db(ctx.db.chat_set_read(from, scope, msg.id))?;
        }
        for (to, seen_as, title) in audience {
            ctx.tell(to, ChatEvent::Message { conv: seen_as, title, msg: msg.clone() });
        }
        Ok(())
    }

    fn open(&mut self, ctx: &mut ChatCtx, from: PlayerId, with: PlayerId) -> Res {
        let view = self.view(ctx, from, Conv::Private(with))?;
        ctx.tell(from, ChatEvent::Upsert(view));
        Ok(())
    }

    fn create_group(&mut self, ctx: &mut ChatCtx, from: PlayerId, raw: &str) -> Res {
        let name: String = clean_text(raw).chars().take(CHAT_GROUP_NAME_LEN).collect();
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("Dê um nome ao grupo.".into());
        }
        if db(ctx.db.chat_groups_of(from))?.len() >= CHAT_GROUPS_PER_PLAYER {
            return Err("Você já está em grupos demais.".into());
        }
        let id = db(ctx.db.chat_group_create(&name, from))?;
        db(ctx.db.chat_add_member(id, from))?;
        let view = self.view(ctx, from, Conv::Group(id))?;
        ctx.tell(from, ChatEvent::Upsert(view));
        Ok(())
    }

    fn add_member(&mut self, ctx: &mut ChatCtx, from: PlayerId, group: i64, player: PlayerId) -> Res {
        self.authorize(ctx, from, Conv::Group(group))?;
        let name = db(ctx.db.chat_player_name(player))?.ok_or(NOT_FOUND)?;
        if db(ctx.db.chat_is_member(group, player))? {
            return Err(format!("{name} já está no grupo."));
        }
        if db(ctx.db.chat_group_members(group))?.len() >= CHAT_GROUP_MEMBERS {
            return Err("O grupo já está cheio.".into());
        }
        if db(ctx.db.chat_groups_of(player))?.len() >= CHAT_GROUPS_PER_PLAYER {
            return Err(format!("{name} já está em grupos demais."));
        }
        db(ctx.db.chat_add_member(group, player))?;
        // History from before joining is not unread.
        let scope = Conv::Group(group).scope(player);
        db(ctx.db.chat_set_read(player, scope, db(ctx.db.chat_last_id(scope))?))?;
        let conv = Conv::Group(group);
        let meta = self.meta(ctx, from, conv)?;
        for (member, _) in db(ctx.db.chat_group_members(group))? {
            if !ctx.online(member) {
                continue;
            }
            let event = if member == player { ChatEvent::Upsert(self.view(ctx, member, conv)?) } else { ChatEvent::Upsert(meta.clone()) };
            ctx.tell(member, event);
        }
        Ok(())
    }

    fn leave(&mut self, ctx: &mut ChatCtx, from: PlayerId, group: i64) -> Res {
        self.authorize(ctx, from, Conv::Group(group))?;
        let conv = Conv::Group(group);
        db(ctx.db.chat_remove_member(group, from))?;
        db(ctx.db.chat_forget_reads(from, conv.scope(from)))?;
        ctx.tell(from, ChatEvent::Removed { conv });
        let members = db(ctx.db.chat_group_members(group))?;
        if members.is_empty() {
            return db(ctx.db.chat_group_delete(group));
        }
        let meta = self.meta(ctx, members[0].0, conv)?;
        for (member, _) in members {
            if ctx.online(member) {
                ctx.tell(member, ChatEvent::Upsert(meta.clone()));
            }
        }
        Ok(())
    }
}

fn rank_index(rank: Rank) -> i64 {
    Rank::ALL.iter().position(|r| *r == rank).unwrap_or(0) as i64
}

impl Db {
    /// Stored chat messages of one kind (0 Geral, 1 private, 2 group, 3 clan), or all of them.
    pub fn chat_message_count(&self, kind: Option<i64>) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM chat_messages WHERE ?1 IS NULL OR kind = ?1", [kind], |r| r.get(0))
    }

    pub fn chat_group_count(&self) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM chat_groups", [], |r| r.get(0))
    }

    fn chat_add(&self, scope: Scope, msg: &ChatMsg, cap: usize) -> rusqlite::Result<i64> {
        let (kind, a, b) = scope;
        self.conn.execute(
            "INSERT INTO chat_messages (kind, a, b, sender_id, sender_name, sender_tag, sender_rank, text, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![kind, a, b, msg.sender, msg.name, msg.tag, rank_index(msg.rank), msg.text, msg.at],
        )?;
        let id = self.conn.last_insert_rowid();
        self.conn.execute(
            "DELETE FROM chat_messages WHERE kind = ?1 AND a = ?2 AND b = ?3 AND id <= (
                 SELECT id FROM chat_messages WHERE kind = ?1 AND a = ?2 AND b = ?3 ORDER BY id DESC LIMIT 1 OFFSET ?4)",
            params![kind, a, b, cap as i64],
        )?;
        Ok(id)
    }

    /// The latest messages of a conversation, oldest first.
    fn chat_recent(&self, scope: Scope, limit: usize) -> rusqlite::Result<Vec<ChatMsg>> {
        let (kind, a, b) = scope;
        let mut stmt = self.conn.prepare(
            "SELECT id, sender_id, sender_name, sender_tag, sender_rank, text, at FROM chat_messages
             WHERE kind = ?1 AND a = ?2 AND b = ?3 ORDER BY id DESC LIMIT ?4",
        )?;
        let rows = stmt.query_map(params![kind, a, b, limit as i64], |r| {
            let rank: i64 = r.get(4)?;
            Ok(ChatMsg {
                id: r.get(0)?,
                sender: r.get(1)?,
                name: r.get(2)?,
                tag: r.get(3)?,
                rank: Rank::ALL.get(rank as usize).copied().unwrap_or(Rank::Sub5),
                text: r.get(5)?,
                at: r.get(6)?,
            })
        })?;
        let mut messages = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        messages.reverse();
        Ok(messages)
    }

    fn chat_last_id(&self, scope: Scope) -> rusqlite::Result<i64> {
        let (kind, a, b) = scope;
        self.conn.query_row("SELECT COALESCE(MAX(id), 0) FROM chat_messages WHERE kind = ?1 AND a = ?2 AND b = ?3", params![kind, a, b], |r| r.get(0))
    }

    fn chat_read_mark(&self, player: PlayerId, scope: Scope) -> rusqlite::Result<Option<i64>> {
        let (kind, a, b) = scope;
        self.conn
            .query_row("SELECT last_id FROM chat_reads WHERE player_id = ?1 AND kind = ?2 AND a = ?3 AND b = ?4", params![player, kind, a, b], |r| r.get(0))
            .optional()
    }

    fn chat_set_read(&self, player: PlayerId, scope: Scope, last: i64) -> rusqlite::Result<()> {
        let (kind, a, b) = scope;
        self.conn.execute(
            "INSERT INTO chat_reads (player_id, kind, a, b, last_id) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (player_id, kind, a, b) DO UPDATE SET last_id = MAX(last_id, excluded.last_id)",
            params![player, kind, a, b, last],
        )?;
        Ok(())
    }

    fn chat_forget_reads(&self, player: PlayerId, scope: Scope) -> rusqlite::Result<()> {
        let (kind, a, b) = scope;
        self.conn.execute("DELETE FROM chat_reads WHERE player_id = ?1 AND kind = ?2 AND a = ?3 AND b = ?4", params![player, kind, a, b])?;
        Ok(())
    }

    /// Messages from others after the player's read mark.
    fn chat_unread(&self, player: PlayerId, scope: Scope) -> rusqlite::Result<u32> {
        let (kind, a, b) = scope;
        let mark = self.chat_read_mark(player, scope)?.unwrap_or(0);
        self.conn.query_row(
            "SELECT COUNT(*) FROM chat_messages WHERE kind = ?1 AND a = ?2 AND b = ?3 AND id > ?4 AND sender_id != ?5",
            params![kind, a, b, mark, player],
            |r| r.get(0),
        )
    }

    fn chat_player_name(&self, id: PlayerId) -> rusqlite::Result<Option<String>> {
        self.conn.query_row("SELECT name FROM players WHERE id = ?1", [id], |r| r.get(0)).optional()
    }

    /// Other players with a private conversation with `player`, latest first.
    fn chat_privates_of(&self, player: PlayerId, limit: usize) -> rusqlite::Result<Vec<PlayerId>> {
        let mut stmt = self.conn.prepare(
            "SELECT CASE WHEN a = ?1 THEN b ELSE a END FROM chat_messages
             WHERE kind = 1 AND (a = ?1 OR b = ?1) GROUP BY a, b ORDER BY MAX(id) DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![player, limit as i64], |r| r.get(0))?;
        rows.collect()
    }

    fn chat_group_create(&self, name: &str, by: PlayerId) -> rusqlite::Result<i64> {
        self.conn.execute("INSERT INTO chat_groups (name, created_by, created_at) VALUES (?1, ?2, ?3)", params![name, by, now_secs()])?;
        Ok(self.conn.last_insert_rowid())
    }

    fn chat_group_name(&self, id: i64) -> rusqlite::Result<Option<String>> {
        self.conn.query_row("SELECT name FROM chat_groups WHERE id = ?1", [id], |r| r.get(0)).optional()
    }

    fn chat_group_members(&self, id: i64) -> rusqlite::Result<Vec<(PlayerId, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.player_id, COALESCE(p.name, '?') FROM chat_members m LEFT JOIN players p ON p.id = m.player_id WHERE m.group_id = ?1 ORDER BY m.rowid",
        )?;
        let rows = stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    fn chat_is_member(&self, group: i64, player: PlayerId) -> rusqlite::Result<bool> {
        self.conn.query_row("SELECT COUNT(*) FROM chat_members WHERE group_id = ?1 AND player_id = ?2", params![group, player], |r| r.get::<_, i64>(0)).map(|n| n > 0)
    }

    fn chat_add_member(&self, group: i64, player: PlayerId) -> rusqlite::Result<()> {
        self.conn.execute("INSERT OR IGNORE INTO chat_members (group_id, player_id) VALUES (?1, ?2)", params![group, player])?;
        Ok(())
    }

    fn chat_remove_member(&self, group: i64, player: PlayerId) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM chat_members WHERE group_id = ?1 AND player_id = ?2", params![group, player])?;
        Ok(())
    }

    fn chat_groups_of(&self, player: PlayerId) -> rusqlite::Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare("SELECT g.id, g.name FROM chat_groups g JOIN chat_members m ON m.group_id = g.id WHERE m.player_id = ?1 ORDER BY g.id")?;
        let rows = stmt.query_map([player], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    /// Removes an empty group with its messages and read marks.
    fn chat_group_delete(&self, group: i64) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM chat_messages WHERE kind = 2 AND a = ?1", [group])?;
        self.conn.execute("DELETE FROM chat_reads WHERE kind = 2 AND a = ?1", [group])?;
        self.conn.execute("DELETE FROM chat_groups WHERE id = ?1", [group])?;
        Ok(())
    }
}

/// What the window is doing besides showing messages.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ChatMode {
    #[default]
    Talk,
    /// Choosing an online player for a new private conversation.
    PickPlayer { cursor: usize },
    /// Typing the name of a new group.
    NewGroup(String),
    /// Choosing an online player to add to a group.
    AddPlayer { group: i64, cursor: usize },
}

/// A conversation on the client.
#[derive(Debug, Clone)]
pub struct Conversation {
    pub conv: Conv,
    pub title: String,
    pub members: Vec<String>,
    pub messages: Vec<ChatMsg>,
    /// A cell because the window marks what it shows as read while drawing.
    pub unread: Cell<u32>,
}

impl Conversation {
    fn new(conv: Conv, title: String) -> Conversation {
        Conversation { conv, title, members: Vec::new(), messages: Vec::new(), unread: Cell::new(0) }
    }

    fn last_id(&self) -> i64 {
        self.messages.last().map_or(0, |m| m.id)
    }
}

/// Client side of the chat: conversations as this player sees them.
#[derive(Debug, Clone)]
pub struct ChatClient {
    pub me: Option<PlayerId>,
    pub convs: Vec<Conversation>,
    pub selected: Conv,
    pub input: String,
    /// Lines scrolled up from the newest message.
    pub scroll: usize,
    /// Last scroll limit computed by the renderer.
    pub max_scroll: Cell<usize>,
    pub mode: ChatMode,
    /// Last error from the server or hint from the window.
    pub status: Option<String>,
    /// Private conversation to jump to once the server confirms it.
    pub pending_private: Option<PlayerId>,
    pub pending_group: bool,
    viewed: Cell<Option<Instant>>,
    read_due: Cell<bool>,
    sync_due: Cell<bool>,
}

impl Default for ChatClient {
    fn default() -> ChatClient {
        ChatClient {
            me: None,
            convs: vec![Conversation::new(Conv::General, "Geral".into())],
            selected: Conv::General,
            input: String::new(),
            scroll: 0,
            max_scroll: Cell::new(0),
            mode: ChatMode::Talk,
            status: None,
            pending_private: None,
            pending_group: false,
            viewed: Cell::new(None),
            read_due: Cell::new(false),
            sync_due: Cell::new(false),
        }
    }
}

impl ChatClient {
    pub fn apply(&mut self, event: ChatEvent, now: Instant, notices: &mut Vec<Notice>) {
        match event {
            ChatEvent::Ready { me } => *self = ChatClient { me: Some(me), mode: std::mem::take(&mut self.mode), input: std::mem::take(&mut self.input), ..ChatClient::default() },
            ChatEvent::Upsert(view) => self.upsert(view),
            ChatEvent::Message { conv, title, msg } => self.message(conv, title, msg, now, notices),
            ChatEvent::Removed { conv } if conv != Conv::General => {
                self.convs.retain(|c| c.conv != conv);
                if self.selected == conv {
                    self.select(Conv::General);
                }
            }
            ChatEvent::Removed { .. } => {}
            ChatEvent::Error { text } => {
                self.status = Some(clean_text(&text));
                self.pending_private = None;
                self.pending_group = false;
            }
        }
    }

    /// Messages not read yet, for the footer badge (Geral is live only and does not count).
    pub fn unread(&self) -> usize {
        self.convs.iter().filter(|c| c.conv != Conv::General).map(|c| c.unread.get() as usize).sum()
    }

    pub fn current(&self) -> &Conversation {
        self.convs.iter().find(|c| c.conv == self.selected).unwrap_or(&self.convs[0])
    }

    /// Selects a conversation and marks it read.
    pub fn select(&mut self, conv: Conv) {
        if !self.convs.iter().any(|c| c.conv == conv) {
            return;
        }
        self.selected = conv;
        self.scroll = 0;
        self.status = None;
        self.current().unread.set(0);
        self.read_due.set(true);
    }

    /// Moves the selection by `step` conversations, wrapping around.
    pub fn step(&mut self, step: isize) {
        let len = self.convs.len() as isize;
        let at = self.convs.iter().position(|c| c.conv == self.selected).unwrap_or(0) as isize;
        let next = (at + step).rem_euclid(len) as usize;
        self.select(self.convs[next].conv);
    }

    /// The window drew this frame: what it shows counts as read.
    pub fn mark_viewed(&self, now: Instant) {
        let reopened = self.viewed.get().is_none_or(|t| now.saturating_duration_since(t) > CHAT_VIEW_GRACE);
        if reopened {
            self.sync_due.set(true);
        }
        self.viewed.set(Some(now));
        if self.current().unread.replace(0) > 0 {
            self.read_due.set(true);
        }
    }

    /// Commands the window owes the server (read marks, refresh after opening).
    pub fn take_pending(&self) -> Vec<ChatCmd> {
        let mut cmds = Vec::new();
        if self.sync_due.replace(false) {
            cmds.push(ChatCmd::Sync {});
        }
        if self.read_due.replace(false) && self.selected != Conv::General {
            cmds.push(ChatCmd::Read { conv: self.selected });
        }
        cmds
    }

    fn is_viewing(&self, now: Instant) -> bool {
        self.viewed.get().is_some_and(|t| now.saturating_duration_since(t) <= CHAT_VIEW_GRACE)
    }

    fn sort(&mut self) {
        self.convs.sort_by_key(|c| (c.conv.order(), std::cmp::Reverse(c.last_id())));
    }

    fn upsert(&mut self, view: ConvView) {
        let title = clean_text(&view.title);
        let members: Vec<String> = view.members.iter().map(|m| clean_text(m)).collect();
        let at = match self.convs.iter().position(|c| c.conv == view.conv) {
            Some(i) => i,
            None => {
                self.convs.push(Conversation::new(view.conv, title.clone()));
                self.convs.len() - 1
            }
        };
        let conversation = &mut self.convs[at];
        conversation.title = title;
        conversation.members = members;
        if !view.messages.is_empty() {
            conversation.messages = view.messages.into_iter().map(sanitize).collect();
            let excess = conversation.messages.len().saturating_sub(CHAT_CLIENT_CAP);
            conversation.messages.drain(..excess);
            conversation.unread.set(view.unread);
        }
        self.sort();
        let wanted = match view.conv {
            Conv::Private(peer) if self.pending_private == Some(peer) => {
                self.pending_private = None;
                true
            }
            Conv::Group(_) if self.pending_group => {
                self.pending_group = false;
                true
            }
            _ => false,
        };
        if wanted {
            self.select(view.conv);
        } else if view.conv == self.selected {
            self.current().unread.set(0);
        }
    }

    fn message(&mut self, conv: Conv, title: String, msg: ChatMsg, now: Instant, notices: &mut Vec<Notice>) {
        let msg = sanitize(msg);
        if !self.convs.iter().any(|c| c.conv == conv) {
            self.convs.push(Conversation::new(conv, clean_text(&title)));
        }
        let viewing = self.is_viewing(now) && self.selected == conv;
        let own = self.me == Some(msg.sender);
        let conversation = self.convs.iter_mut().find(|c| c.conv == conv).expect("just ensured");
        if conversation.messages.last().is_some_and(|m| m.id >= msg.id) {
            return;
        }
        if !own && conv != Conv::General {
            notices.push(notice_for(conv, &conversation.title, &msg));
        }
        if !own && conv != Conv::General && !viewing {
            conversation.unread.set(conversation.unread.get() + 1);
        }
        conversation.messages.push(msg);
        let excess = conversation.messages.len().saturating_sub(CHAT_CLIENT_CAP);
        conversation.messages.drain(..excess);
        if viewing {
            self.read_due.set(true);
        }
        self.sort();
    }
}

/// Never trust text from the wire: strip controls and cap the length.
fn sanitize(mut msg: ChatMsg) -> ChatMsg {
    msg.name = clean_text(&msg.name);
    msg.tag = msg.tag.map(|t| clean_text(&t));
    msg.text = clean_text(&msg.text).chars().take(CHAT_MAX_CHARS).collect();
    msg
}

fn notice_for(conv: Conv, title: &str, msg: &ChatMsg) -> Notice {
    let title = match conv {
        Conv::General | Conv::Private(_) => format!("Mensagem de {}", msg.name),
        Conv::Group(_) => format!("{} em {title}", msg.name),
        Conv::Clan(_) => format!("[{title}] clã"),
    };
    Notice { title, body: msg.text.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: i64, sender: PlayerId, text: &str) -> ChatMsg {
        ChatMsg { id, sender, name: format!("P{sender}"), tag: None, rank: Rank::Sub5, text: text.into(), at: 0 }
    }

    fn client() -> ChatClient {
        let mut c = ChatClient::default();
        c.apply(ChatEvent::Ready { me: 1 }, Instant::now(), &mut Vec::new());
        c
    }

    fn deliver(c: &mut ChatClient, conv: Conv, title: &str, m: ChatMsg) -> Vec<Notice> {
        let mut notices = Vec::new();
        c.apply(ChatEvent::Message { conv, title: title.into(), msg: m }, Instant::now(), &mut notices);
        notices
    }

    #[test]
    fn clean_text_strips_controls_and_breaks() {
        assert_eq!(clean_text("  oi\u{0}\u{7}\n\tmundo \u{1b}[31m "), "oi  mundo [31m");
        assert_eq!(clean_text("\n\t "), "");
        assert_eq!(clean_text("ação é ótima"), "ação é ótima");
    }

    #[test]
    fn private_message_from_others_counts_and_notifies() {
        let mut c = client();
        let notices = deliver(&mut c, Conv::Private(2), "P2", msg(1, 2, "oi"));
        assert_eq!(c.unread(), 1);
        assert_eq!(notices, vec![Notice { title: "Mensagem de P2".into(), body: "oi".into() }]);
    }

    #[test]
    fn own_messages_and_general_do_not_notify_or_count() {
        let mut c = client();
        assert!(deliver(&mut c, Conv::Private(2), "P2", msg(1, 1, "eu")).is_empty());
        assert!(deliver(&mut c, Conv::General, "Geral", msg(2, 3, "geral")).is_empty());
        assert_eq!(c.unread(), 0);
        assert_eq!(c.current().messages.len(), 1);
    }

    #[test]
    fn group_and_clan_notice_titles() {
        let mut c = client();
        let g = deliver(&mut c, Conv::Group(7), "Academia", msg(1, 2, "bora"));
        assert_eq!(g[0].title, "P2 em Academia");
        let k = deliver(&mut c, Conv::Clan(3), "GIGA", msg(2, 2, "salve"));
        assert_eq!(k[0].title, "[GIGA] clã");
    }

    #[test]
    fn selecting_reads_and_asks_the_server() {
        let mut c = client();
        deliver(&mut c, Conv::Private(2), "P2", msg(1, 2, "oi"));
        c.select(Conv::Private(2));
        assert_eq!(c.unread(), 0);
        assert_eq!(c.take_pending(), vec![ChatCmd::Read { conv: Conv::Private(2) }]);
        assert!(c.take_pending().is_empty());
    }

    #[test]
    fn open_window_reads_messages_as_they_arrive() {
        let mut c = client();
        deliver(&mut c, Conv::Private(2), "P2", msg(1, 2, "oi"));
        c.select(Conv::Private(2));
        c.mark_viewed(Instant::now());
        deliver(&mut c, Conv::Private(2), "P2", msg(2, 2, "de novo"));
        assert_eq!(c.unread(), 0);
        assert!(c.take_pending().contains(&ChatCmd::Read { conv: Conv::Private(2) }));
    }

    #[test]
    fn duplicates_and_hostile_text_are_handled() {
        let mut c = client();
        deliver(&mut c, Conv::General, "Geral", msg(5, 2, "a"));
        deliver(&mut c, Conv::General, "Geral", msg(5, 2, "a"));
        let long = "x".repeat(CHAT_MAX_CHARS * 3);
        deliver(&mut c, Conv::General, "Geral", msg(6, 2, &format!("\u{1b}[2J{long}")));
        let last = c.current().messages.last().unwrap();
        assert_eq!(c.current().messages.len(), 2);
        assert_eq!(last.text.chars().count(), CHAT_MAX_CHARS);
        assert!(!last.text.contains('\u{1b}'));
    }

    #[test]
    fn history_is_bounded_and_removed_conversations_fall_back() {
        let mut c = client();
        for i in 1..=(CHAT_CLIENT_CAP as i64 + 20) {
            deliver(&mut c, Conv::General, "Geral", msg(i, 2, "x"));
        }
        assert_eq!(c.current().messages.len(), CHAT_CLIENT_CAP);
        deliver(&mut c, Conv::Group(1), "G", msg(1, 2, "x"));
        c.select(Conv::Group(1));
        c.apply(ChatEvent::Removed { conv: Conv::Group(1) }, Instant::now(), &mut Vec::new());
        assert_eq!(c.selected, Conv::General);
    }

    #[test]
    fn pending_private_selects_when_confirmed() {
        let mut c = client();
        c.pending_private = Some(9);
        let view = ConvView { conv: Conv::Private(9), title: "Ana".into(), members: vec![], unread: 0, messages: vec![] };
        c.apply(ChatEvent::Upsert(view), Instant::now(), &mut Vec::new());
        assert_eq!(c.selected, Conv::Private(9));
    }

    #[test]
    fn events_round_trip_through_the_wire_format() {
        let event = ServerMsg::Chat(ChatEvent::Upsert(ConvView { conv: Conv::Group(3), title: "Acadêmia".into(), members: vec!["Ana".into()], unread: 2, messages: vec![msg(1, 2, "oi")] }));
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(serde_json::from_str::<ServerMsg>(&json).unwrap(), event);
        let cmd = crate::protocol::ClientMsg::Chat(ChatCmd::Sync {});
        let json = serde_json::to_string(&cmd).unwrap();
        assert_eq!(serde_json::from_str::<crate::protocol::ClientMsg>(&json).unwrap(), cmd);
    }
}
