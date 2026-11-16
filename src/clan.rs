//! Clans: a tag next to the name, members, and no friendly fire.

use std::collections::BTreeMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::constants::*;
use crate::db::Db;
use crate::game::Tone;
use crate::notify::Notice;
use crate::protocol::{PlayerId, ServerMsg};
use crate::ui::clan::ClanUi;
use crate::world::World;

/// Tables this feature needs; run when the database opens.
pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS clans (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL,
    tag         TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    leader_id   INTEGER NOT NULL REFERENCES players(id),
    created_at  INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS clan_members (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id  INTEGER NOT NULL UNIQUE REFERENCES players(id),
    clan_id    INTEGER NOT NULL REFERENCES clans(id) ON DELETE CASCADE,
    joined_at  INTEGER NOT NULL
);
";

/// What a client asks the clan system to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "c")]
pub enum ClanCmd {
    Create { name: String, tag: String },
    /// Leader only: invites an online player.
    Invite { target: PlayerId },
    Accept { clan: i64 },
    Decline { clan: i64 },
    Leave,
    /// Leader only.
    Kick { member: PlayerId },
    /// Leader only: hands the leadership to a member.
    Promote { member: PlayerId },
    /// Leader only.
    Disband,
    /// Asks for a fresh snapshot, ranking included.
    Refresh,
}

/// One member as the clan window lists them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberView {
    pub id: PlayerId,
    pub name: String,
    pub leader: bool,
    pub online: bool,
    pub lifetime: f64,
}

/// The player's own clan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClanView {
    pub id: i64,
    pub name: String,
    pub tag: String,
    /// The viewing player.
    pub you: PlayerId,
    pub leader: PlayerId,
    pub max_members: usize,
    pub members: Vec<MemberView>,
}

impl ClanView {
    pub fn is_leader(&self) -> bool {
        self.you == self.leader
    }

    pub fn lifetime(&self) -> f64 {
        self.members.iter().map(|m| m.lifetime).sum()
    }
}

/// An invitation waiting for an answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClanInviteView {
    pub clan: i64,
    pub name: String,
    pub tag: String,
    pub from: String,
    pub members: usize,
}

/// One line of the clan ranking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankRow {
    pub id: i64,
    pub name: String,
    pub tag: String,
    pub members: usize,
    pub lifetime: f64,
}

/// Everything the clan window shows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClanSnapshot {
    pub clan: Option<ClanView>,
    pub invites: Vec<ClanInviteView>,
    pub ranking: Vec<RankRow>,
}

/// What the clan system tells a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "c")]
pub enum ClanEvent {
    Snapshot { snapshot: Box<ClanSnapshot> },
    /// Answer to the last command, for the window's status line.
    Status { text: String },
    Invited { clan: String, tag: String, from: String },
    Kicked { clan: String, tag: String },
    /// Someone accepted the leader's invitation.
    InviteAccepted { name: String },
    Disbanded { clan: String, tag: String },
}

/// Everything the clan system may touch while handling one command.
pub struct ClanCtx<'a> {
    pub db: &'a Db,
    pub world: &'a mut World,
    /// Messages to send, addressed by player.
    pub out: &'a mut Vec<(PlayerId, ServerMsg)>,
    pub now: Instant,
}

/// A refusal or a failure, already in the player's language.
type Fallible<T = ()> = Result<T, String>;

fn db_failed(e: rusqlite::Error) -> String {
    format!("Erro no banco do servidor: {e}")
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Checks and trims a clan name.
pub fn valid_clan_name(raw: &str) -> Fallible<String> {
    let name = raw.trim();
    let len = name.chars().count();
    if !(CLAN_NAME_MIN..=CLAN_NAME_MAX).contains(&len) || name.chars().any(char::is_control) {
        return Err(format!("O nome do clã deve ter de {CLAN_NAME_MIN} a {CLAN_NAME_MAX} caracteres."));
    }
    Ok(name.to_string())
}

/// Checks a tag and returns it in uppercase.
pub fn valid_clan_tag(raw: &str) -> Fallible<String> {
    let tag = raw.trim();
    let len = tag.chars().count();
    if !(CLAN_TAG_MIN..=CLAN_TAG_MAX).contains(&len) || !tag.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("A tag deve ter de {CLAN_TAG_MIN} a {CLAN_TAG_MAX} letras ou números."));
    }
    Ok(tag.to_ascii_uppercase())
}

#[derive(Debug, Clone)]
struct ClanRow {
    id: i64,
    name: String,
    tag: String,
    leader: PlayerId,
}

/// One membership joined with the player's saved data.
struct MemberRow {
    clan: i64,
    clan_name: String,
    tag: String,
    player: PlayerId,
    name: String,
    lifetime: f64,
}

/// The only part of a saved profile the ranking needs.
#[derive(Deserialize, Default)]
#[serde(default)]
struct SavedLifetime {
    lifetime: f64,
}

impl Db {
    fn clan_from_row(r: &rusqlite::Row) -> rusqlite::Result<ClanRow> {
        Ok(ClanRow { id: r.get(0)?, name: r.get(1)?, tag: r.get(2)?, leader: r.get(3)? })
    }

    fn clan_row(&self, id: i64) -> rusqlite::Result<Option<ClanRow>> {
        self.conn.query_row("SELECT id, name, tag, leader_id FROM clans WHERE id = ?1", [id], Self::clan_from_row).optional()
    }

    fn clan_row_of(&self, player: PlayerId) -> rusqlite::Result<Option<ClanRow>> {
        self.conn
            .query_row(
                "SELECT c.id, c.name, c.tag, c.leader_id FROM clans c JOIN clan_members m ON m.clan_id = c.id WHERE m.player_id = ?1",
                [player],
                Self::clan_from_row,
            )
            .optional()
    }

    fn clan_tag_taken(&self, tag: &str) -> rusqlite::Result<bool> {
        self.conn.query_row("SELECT COUNT(*) FROM clans WHERE tag = ?1", [tag], |r| r.get::<_, i64>(0)).map(|n| n > 0)
    }

    /// Creates a clan and makes its founder the first member.
    fn clan_insert(&self, name: &str, tag: &str, leader: PlayerId) -> rusqlite::Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let now = now_secs();
        tx.execute("INSERT INTO clans (name, tag, leader_id, created_at) VALUES (?1, ?2, ?3, ?4)", params![name, tag, leader, now])?;
        let id = tx.last_insert_rowid();
        tx.execute("INSERT INTO clan_members (player_id, clan_id, joined_at) VALUES (?1, ?2, ?3)", params![leader, id, now])?;
        tx.commit()?;
        Ok(id)
    }

    fn clan_add_member(&self, clan: i64, player: PlayerId) -> rusqlite::Result<()> {
        self.conn.execute("INSERT INTO clan_members (player_id, clan_id, joined_at) VALUES (?1, ?2, ?3)", params![player, clan, now_secs()])?;
        Ok(())
    }

    fn clan_remove_member(&self, player: PlayerId) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM clan_members WHERE player_id = ?1", [player])?;
        Ok(())
    }

    fn clan_set_leader(&self, clan: i64, player: PlayerId) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE clans SET leader_id = ?2 WHERE id = ?1", params![clan, player])?;
        Ok(())
    }

    fn clan_delete(&self, clan: i64) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM clan_members WHERE clan_id = ?1", [clan])?;
        tx.execute("DELETE FROM clans WHERE id = ?1", [clan])?;
        tx.commit()
    }

    /// Memberships of one clan, or of every clan, oldest member first.
    fn clan_member_rows(&self, clan: Option<i64>) -> rusqlite::Result<Vec<MemberRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.clan_id, c.name, c.tag, m.player_id, p.name, p.profile
             FROM clan_members m JOIN clans c ON c.id = m.clan_id JOIN players p ON p.id = m.player_id
             WHERE (?1 IS NULL OR m.clan_id = ?1) ORDER BY m.clan_id, m.id",
        )?;
        let rows = stmt.query_map([clan], |r| {
            let profile: String = r.get(5)?;
            let saved: SavedLifetime = serde_json::from_str(&profile).unwrap_or_default();
            Ok(MemberRow { clan: r.get(0)?, clan_name: r.get(1)?, tag: r.get(2)?, player: r.get(3)?, name: r.get(4)?, lifetime: saved.lifetime })
        })?;
        rows.collect()
    }

    fn clan_member_ids(&self, clan: i64) -> rusqlite::Result<Vec<PlayerId>> {
        Ok(self.clan_member_rows(Some(clan))?.into_iter().map(|m| m.player).collect())
    }

    fn saved_name(&self, player: PlayerId) -> rusqlite::Result<Option<String>> {
        self.conn.query_row("SELECT name FROM players WHERE id = ?1", [player], |r| r.get(0)).optional()
    }
}

/// An invitation waiting for the invitee's answer.
struct Invite {
    clan: i64,
    from: PlayerId,
    to: PlayerId,
}

/// Server side of clans.
#[derive(Default)]
pub struct ClanHub {
    /// Invitations live until answered or until their player goes offline.
    invites: Vec<Invite>,
}

fn say(ctx: &mut ClanCtx, to: PlayerId, tone: Tone, text: impl Into<String>) {
    let text = text.into();
    ctx.out.push((to, ServerMsg::Event { tone, text: text.clone(), banner: None, hit: false }));
    ctx.out.push((to, ServerMsg::Clan(ClanEvent::Status { text })));
}

fn tell(ctx: &mut ClanCtx, to: PlayerId, event: ClanEvent) {
    ctx.out.push((to, ServerMsg::Clan(event)));
}

fn name_of(ctx: &ClanCtx, player: PlayerId) -> String {
    match ctx.world.slot(player) {
        Some(slot) => slot.player.name.clone(),
        None => ctx.db.saved_name(player).ok().flatten().unwrap_or_else(|| "?".into()),
    }
}

fn lifetime_of(ctx: &ClanCtx, row: &MemberRow) -> f64 {
    ctx.world.slot(row.player).map_or(row.lifetime, |s| s.player.lifetime)
}

fn display_clan(row: &ClanRow) -> String {
    format!("[{}] {}", row.tag, row.name)
}

impl ClanHub {
    pub fn load(_db: &Db) -> ClanHub {
        ClanHub::default()
    }

    pub fn handle(&mut self, ctx: &mut ClanCtx, from: PlayerId, cmd: ClanCmd) {
        if let Err(text) = self.run(ctx, from, cmd) {
            say(ctx, from, Tone::Info, text);
        }
    }

    fn run(&mut self, ctx: &mut ClanCtx, from: PlayerId, cmd: ClanCmd) -> Fallible {
        match cmd {
            ClanCmd::Create { name, tag } => self.create(ctx, from, &name, &tag),
            ClanCmd::Invite { target } => self.invite(ctx, from, target),
            ClanCmd::Accept { clan } => self.accept(ctx, from, clan),
            ClanCmd::Decline { clan } => self.decline(ctx, from, clan),
            ClanCmd::Leave => self.leave(ctx, from),
            ClanCmd::Kick { member } => self.kick(ctx, from, member),
            ClanCmd::Promote { member } => self.promote(ctx, from, member),
            ClanCmd::Disband => self.disband(ctx, from),
            ClanCmd::Refresh => self.broadcast(ctx, &[from], None),
        }
    }

    /// A player came online (already in the world); must set their clan with `World::set_clan`.
    pub fn joined(&mut self, ctx: &mut ClanCtx, player: PlayerId) {
        let _ = self.sync_world(ctx, player);
        let mut audience = vec![player];
        if let Ok(Some(clan)) = ctx.db.clan_row_of(player) {
            audience.extend(ctx.db.clan_member_ids(clan.id).unwrap_or_default());
        }
        let _ = self.broadcast(ctx, &audience, None);
    }

    /// A player is going offline (still in the world).
    pub fn left(&mut self, ctx: &mut ClanCtx, player: PlayerId) {
        self.invites.retain(|i| i.to != player);
        let Ok(Some(clan)) = ctx.db.clan_row_of(player) else { return };
        let audience = ctx.db.clan_member_ids(clan.id).unwrap_or_default();
        let _ = self.broadcast(ctx, &audience, Some(player));
    }

    fn require_member(&self, ctx: &ClanCtx, player: PlayerId) -> Fallible<ClanRow> {
        ctx.db.clan_row_of(player).map_err(db_failed)?.ok_or_else(|| "Você não está em um clã.".to_string())
    }

    fn require_leader(&self, ctx: &ClanCtx, player: PlayerId) -> Fallible<ClanRow> {
        let clan = self.require_member(ctx, player)?;
        if clan.leader != player {
            return Err("Só o líder do clã pode fazer isso.".into());
        }
        Ok(clan)
    }

    /// A member of the leader's clan other than the leader.
    fn require_other_member(&self, ctx: &ClanCtx, clan: &ClanRow, leader: PlayerId, member: PlayerId) -> Fallible {
        let same = member != leader && ctx.db.clan_row_of(member).map_err(db_failed)?.is_some_and(|c| c.id == clan.id);
        if same { Ok(()) } else { Err("Esse jogador não é outro membro do seu clã.".into()) }
    }

    /// Puts the player's clan, or its absence, in the world; offline players have no entry there.
    fn sync_world(&self, ctx: &mut ClanCtx, player: PlayerId) -> Fallible {
        if ctx.world.slot(player).is_none() {
            return Ok(());
        }
        let clan = ctx.db.clan_row_of(player).map_err(db_failed)?;
        ctx.world.set_clan(player, clan.map(|c| (c.id, c.tag)));
        Ok(())
    }

    fn drop_invites_to(&mut self, player: PlayerId) {
        self.invites.retain(|i| i.to != player);
    }

    fn create(&mut self, ctx: &mut ClanCtx, from: PlayerId, name: &str, tag: &str) -> Fallible {
        if ctx.db.clan_row_of(from).map_err(db_failed)?.is_some() {
            return Err("Você já está em um clã. Saia dele antes de criar outro.".into());
        }
        let (name, tag) = (valid_clan_name(name)?, valid_clan_tag(tag)?);
        if ctx.db.clan_tag_taken(&tag).map_err(db_failed)? {
            return Err(format!("A tag {tag} já está em uso."));
        }
        ctx.db.clan_insert(&name, &tag, from).map_err(db_failed)?;
        self.drop_invites_to(from);
        self.sync_world(ctx, from)?;
        say(ctx, from, Tone::Epic, format!("Clã [{tag}] {name} criado! Convide jogadores com I na janela do clã."));
        self.broadcast(ctx, &[from], None)
    }

    fn invite(&mut self, ctx: &mut ClanCtx, from: PlayerId, target: PlayerId) -> Fallible {
        let clan = self.require_leader(ctx, from)?;
        if target == from || ctx.world.slot(target).is_none() {
            return Err("Escolha outro jogador que esteja no servidor.".into());
        }
        let name = name_of(ctx, target);
        if ctx.db.clan_row_of(target).map_err(db_failed)?.is_some() {
            return Err(format!("{name} já está em um clã."));
        }
        if ctx.db.clan_member_ids(clan.id).map_err(db_failed)?.len() >= CLAN_MAX_MEMBERS {
            return Err("O clã está cheio.".into());
        }
        if self.invites.iter().any(|i| i.clan == clan.id && i.to == target) {
            return Err(format!("{name} já tem um convite do seu clã."));
        }
        if self.invites.iter().filter(|i| i.to == target).count() >= CLAN_MAX_PENDING_INVITES {
            return Err(format!("{name} já tem convites demais esperando resposta."));
        }
        self.invites.push(Invite { clan: clan.id, from, to: target });
        let leader = name_of(ctx, from);
        say(ctx, from, Tone::Good, format!("Convite enviado para {name}."));
        let text = format!("{leader} convidou você para o clã {}. Abra o clã com G.", display_clan(&clan));
        ctx.out.push((target, ServerMsg::Event { tone: Tone::Epic, text, banner: Some("CONVITE DE CLÃ".into()), hit: false }));
        tell(ctx, target, ClanEvent::Invited { clan: clan.name.clone(), tag: clan.tag.clone(), from: leader });
        self.broadcast(ctx, &[target], None)
    }

    fn take_invite(&mut self, clan: i64, to: PlayerId) -> Fallible<Invite> {
        let pos = self.invites.iter().position(|i| i.clan == clan && i.to == to);
        pos.map(|p| self.invites.remove(p)).ok_or_else(|| "Esse convite não existe mais.".to_string())
    }

    fn accept(&mut self, ctx: &mut ClanCtx, from: PlayerId, clan_id: i64) -> Fallible {
        self.take_invite(clan_id, from)?;
        if ctx.db.clan_row_of(from).map_err(db_failed)?.is_some() {
            return Err("Você já está em um clã.".into());
        }
        let clan = ctx.db.clan_row(clan_id).map_err(db_failed)?.ok_or("Esse clã não existe mais.")?;
        let members = ctx.db.clan_member_ids(clan.id).map_err(db_failed)?;
        if members.len() >= CLAN_MAX_MEMBERS {
            return Err("O clã está cheio.".into());
        }
        ctx.db.clan_add_member(clan.id, from).map_err(db_failed)?;
        self.drop_invites_to(from);
        self.sync_world(ctx, from)?;
        let name = name_of(ctx, from);
        say(ctx, from, Tone::Epic, format!("Você entrou no clã {}!", display_clan(&clan)));
        for member in members {
            if member == clan.leader {
                tell(ctx, member, ClanEvent::InviteAccepted { name: name.clone() });
            }
            say(ctx, member, Tone::Good, format!("{name} entrou no clã."));
        }
        let audience = ctx.db.clan_member_ids(clan.id).map_err(db_failed)?;
        self.broadcast(ctx, &audience, None)
    }

    fn decline(&mut self, ctx: &mut ClanCtx, from: PlayerId, clan: i64) -> Fallible {
        let invite = self.take_invite(clan, from)?;
        let name = name_of(ctx, from);
        say(ctx, from, Tone::Info, "Convite recusado.");
        if ctx.world.slot(invite.from).is_some() {
            say(ctx, invite.from, Tone::Info, format!("{name} recusou o convite do clã."));
        }
        self.broadcast(ctx, &[from], None)
    }

    fn leave(&mut self, ctx: &mut ClanCtx, from: PlayerId) -> Fallible {
        let clan = self.require_member(ctx, from)?;
        let members = ctx.db.clan_member_ids(clan.id).map_err(db_failed)?;
        let Some(&heir) = members.iter().find(|m| **m != from) else {
            self.dissolve(ctx, &clan, from)?;
            say(ctx, from, Tone::Info, "Você saiu do clã e, sem ninguém, ele foi desfeito.");
            return self.broadcast(ctx, &[from], None);
        };
        let name = name_of(ctx, from);
        if clan.leader == from {
            ctx.db.clan_set_leader(clan.id, heir).map_err(db_failed)?;
        }
        ctx.db.clan_remove_member(from).map_err(db_failed)?;
        self.sync_world(ctx, from)?;
        say(ctx, from, Tone::Info, format!("Você saiu do clã {}.", display_clan(&clan)));
        for member in members.iter().filter(|m| **m != from) {
            say(ctx, *member, Tone::Info, format!("{name} saiu do clã."));
        }
        if clan.leader == from {
            let heir_name = name_of(ctx, heir);
            for member in members.iter().filter(|m| **m != from) {
                say(ctx, *member, Tone::Epic, format!("{heir_name} agora lidera o clã."));
            }
        }
        self.broadcast(ctx, &members, None)
    }

    fn kick(&mut self, ctx: &mut ClanCtx, from: PlayerId, member: PlayerId) -> Fallible {
        let clan = self.require_leader(ctx, from)?;
        self.require_other_member(ctx, &clan, from, member)?;
        let members = ctx.db.clan_member_ids(clan.id).map_err(db_failed)?;
        let name = name_of(ctx, member);
        ctx.db.clan_remove_member(member).map_err(db_failed)?;
        self.sync_world(ctx, member)?;
        say(ctx, member, Tone::Bad, format!("Você foi expulso do clã {}.", display_clan(&clan)));
        tell(ctx, member, ClanEvent::Kicked { clan: clan.name.clone(), tag: clan.tag.clone() });
        for other in members.iter().filter(|m| **m != member) {
            say(ctx, *other, Tone::Info, format!("{name} foi expulso do clã."));
        }
        self.broadcast(ctx, &members, None)
    }

    fn promote(&mut self, ctx: &mut ClanCtx, from: PlayerId, member: PlayerId) -> Fallible {
        let clan = self.require_leader(ctx, from)?;
        self.require_other_member(ctx, &clan, from, member)?;
        ctx.db.clan_set_leader(clan.id, member).map_err(db_failed)?;
        let name = name_of(ctx, member);
        let members = ctx.db.clan_member_ids(clan.id).map_err(db_failed)?;
        for other in &members {
            say(ctx, *other, Tone::Epic, format!("{name} agora lidera o clã."));
        }
        self.broadcast(ctx, &members, None)
    }

    fn disband(&mut self, ctx: &mut ClanCtx, from: PlayerId) -> Fallible {
        let clan = self.require_leader(ctx, from)?;
        let members = ctx.db.clan_member_ids(clan.id).map_err(db_failed)?;
        self.dissolve(ctx, &clan, from)?;
        say(ctx, from, Tone::Info, format!("Você desfez o clã {}.", display_clan(&clan)));
        self.broadcast(ctx, &members, None)
    }

    /// Deletes the clan and tells every member but `by`.
    fn dissolve(&mut self, ctx: &mut ClanCtx, clan: &ClanRow, by: PlayerId) -> Fallible {
        let members = ctx.db.clan_member_ids(clan.id).map_err(db_failed)?;
        ctx.db.clan_delete(clan.id).map_err(db_failed)?;
        self.invites.retain(|i| i.clan != clan.id);
        for member in members {
            self.sync_world(ctx, member)?;
            if member != by {
                say(ctx, member, Tone::Bad, format!("O clã {} foi desfeito.", display_clan(clan)));
                tell(ctx, member, ClanEvent::Disbanded { clan: clan.name.clone(), tag: clan.tag.clone() });
            }
        }
        Ok(())
    }

    /// Sends a fresh snapshot to each of these players who is online; `leaving` counts as offline.
    fn broadcast(&self, ctx: &mut ClanCtx, players: &[PlayerId], leaving: Option<PlayerId>) -> Fallible {
        let rows = ctx.db.clan_member_rows(None).map_err(db_failed)?;
        let ranking = ranking(ctx, &rows);
        let mut sent: Vec<PlayerId> = Vec::new();
        for &player in players {
            if sent.contains(&player) || ctx.world.slot(player).is_none() {
                continue;
            }
            sent.push(player);
            let snapshot = self.snapshot_for(ctx, player, &rows, &ranking, leaving)?;
            tell(ctx, player, ClanEvent::Snapshot { snapshot: Box::new(snapshot) });
        }
        Ok(())
    }

    fn snapshot_for(&self, ctx: &ClanCtx, player: PlayerId, rows: &[MemberRow], ranking: &[RankRow], leaving: Option<PlayerId>) -> Fallible<ClanSnapshot> {
        let clan = match ctx.db.clan_row_of(player).map_err(db_failed)? {
            Some(c) => {
                let members = rows
                    .iter()
                    .filter(|m| m.clan == c.id)
                    .map(|m| MemberView {
                        id: m.player,
                        name: ctx.world.slot(m.player).map_or_else(|| m.name.clone(), |s| s.player.name.clone()),
                        leader: m.player == c.leader,
                        online: ctx.world.slot(m.player).is_some() && Some(m.player) != leaving,
                        lifetime: lifetime_of(ctx, m),
                    })
                    .collect();
                Some(ClanView { id: c.id, name: c.name, tag: c.tag, you: player, leader: c.leader, max_members: CLAN_MAX_MEMBERS, members })
            }
            None => None,
        };
        let invites = self
            .invites
            .iter()
            .filter(|i| i.to == player)
            .filter_map(|i| {
                let first = rows.iter().find(|m| m.clan == i.clan)?;
                let members = rows.iter().filter(|m| m.clan == i.clan).count();
                Some(ClanInviteView { clan: i.clan, name: first.clan_name.clone(), tag: first.tag.clone(), from: name_of(ctx, i.from), members })
            })
            .collect();
        Ok(ClanSnapshot { clan, invites, ranking: ranking.to_vec() })
    }
}

/// Clans by the sum of their members' lifetime MP, richest first.
fn ranking(ctx: &ClanCtx, rows: &[MemberRow]) -> Vec<RankRow> {
    let mut clans: BTreeMap<i64, RankRow> = BTreeMap::new();
    for m in rows {
        let row = clans.entry(m.clan).or_insert_with(|| RankRow { id: m.clan, name: m.clan_name.clone(), tag: m.tag.clone(), members: 0, lifetime: 0.0 });
        row.members += 1;
        row.lifetime += lifetime_of(ctx, m);
    }
    let mut ranked: Vec<RankRow> = clans.into_values().collect();
    ranked.sort_by(|a, b| b.lifetime.total_cmp(&a.lifetime).then(b.members.cmp(&a.members)).then_with(|| a.tag.cmp(&b.tag)));
    ranked.truncate(CLAN_RANKING_LIMIT);
    ranked
}

/// Client side of the clan system: the player's clan and invitations.
#[derive(Debug, Clone, Default)]
pub struct ClanClient {
    pub clan: Option<ClanView>,
    pub invites: Vec<ClanInviteView>,
    pub ranking: Vec<RankRow>,
    /// Last answer from the server, shown in the window.
    pub status: Option<String>,
    /// When the last snapshot arrived.
    pub refreshed: Option<Instant>,
    /// State of the clan window itself.
    pub ui: ClanUi,
}

fn notice(title: &str, body: String) -> Notice {
    Notice { title: title.into(), body }
}

impl ClanClient {
    pub fn apply(&mut self, event: ClanEvent, now: Instant, notices: &mut Vec<Notice>) {
        match event {
            ClanEvent::Snapshot { snapshot } => {
                let ClanSnapshot { clan, invites, ranking } = *snapshot;
                (self.clan, self.invites, self.ranking, self.refreshed) = (clan, invites, ranking, Some(now));
                self.ui.clamp(self.clan.as_ref(), self.invites.len(), self.ranking.len());
            }
            ClanEvent::Status { text } => self.status = Some(text),
            ClanEvent::Invited { clan, tag, from } => {
                notices.push(notice("Convite de clã", format!("{from} convidou você para [{tag}] {clan}.")));
            }
            ClanEvent::Kicked { clan, tag } => {
                notices.push(notice("Expulso do clã", format!("Você foi expulso de [{tag}] {clan}.")));
            }
            ClanEvent::InviteAccepted { name } => {
                notices.push(notice("Novo membro no clã", format!("{name} aceitou o convite e entrou no clã.")));
            }
            ClanEvent::Disbanded { clan, tag } => {
                notices.push(notice("Clã desfeito", format!("O clã [{tag}] {clan} foi desfeito.")));
            }
        }
    }

    /// Pending clan invitations, for the footer badge.
    pub fn pending(&self) -> usize {
        self.invites.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_uppercased_and_checked() {
        assert_eq!(valid_clan_tag(" giga ").unwrap(), "GIGA");
        assert!(valid_clan_tag("a").is_err());
        assert!(valid_clan_tag("abcdef").is_err());
        assert!(valid_clan_tag("a-b").is_err());
        assert!(valid_clan_tag("çã").is_err());
    }

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(valid_clan_name("  Mewers  ").unwrap(), "Mewers");
        assert!(valid_clan_name("ab").is_err());
        assert!(valid_clan_name(&"x".repeat(CLAN_NAME_MAX + 1)).is_err());
        assert!(valid_clan_name("bad\nname").is_err());
        assert!(valid_clan_name(&"ã".repeat(CLAN_NAME_MAX)).is_ok());
    }

    #[test]
    fn commands_and_events_survive_json() {
        for cmd in [ClanCmd::Leave, ClanCmd::Disband, ClanCmd::Refresh, ClanCmd::Kick { member: 3 }] {
            let msg = crate::protocol::ClientMsg::Clan(cmd.clone());
            let back: crate::protocol::ClientMsg = serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
            assert_eq!(back, msg);
        }
        let event = ServerMsg::Clan(ClanEvent::Snapshot { snapshot: Box::default() });
        let back: ServerMsg = serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(back, event);
    }

    fn apply(event: ClanEvent) -> (ClanClient, Vec<Notice>) {
        let mut client = ClanClient::default();
        let mut notices = Vec::new();
        client.apply(event, Instant::now(), &mut notices);
        (client, notices)
    }

    #[test]
    fn notices_for_invite_kick_accept_and_disband() {
        let events = [
            (ClanEvent::Invited { clan: "Mewers".into(), tag: "MEW".into(), from: "Ana".into() }, "Ana"),
            (ClanEvent::Kicked { clan: "Mewers".into(), tag: "MEW".into() }, "expulso"),
            (ClanEvent::InviteAccepted { name: "Beto".into() }, "Beto"),
            (ClanEvent::Disbanded { clan: "Mewers".into(), tag: "MEW".into() }, "desfeito"),
        ];
        for (event, needle) in events {
            let (_, notices) = apply(event);
            assert_eq!(notices.len(), 1);
            assert!(notices[0].body.contains(needle), "{:?}", notices[0]);
        }
    }

    #[test]
    fn status_and_snapshot_are_silent_and_update_the_client() {
        let (client, notices) = apply(ClanEvent::Status { text: "ok".into() });
        assert!(notices.is_empty());
        assert_eq!(client.status.as_deref(), Some("ok"));
        let invite = ClanInviteView { clan: 1, name: "Mewers".into(), tag: "MEW".into(), from: "Ana".into(), members: 2 };
        let snapshot = ClanSnapshot { invites: vec![invite], ..Default::default() };
        let (client, notices) = apply(ClanEvent::Snapshot { snapshot: Box::new(snapshot) });
        assert!(notices.is_empty());
        assert_eq!(client.pending(), 1);
        assert!(client.refreshed.is_some());
    }
}
