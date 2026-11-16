//! Server storage: players keyed by machine id, plus duel and purchase history.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::game::Player;
use crate::protocol::PlayerId;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS players (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    machine_id  TEXT    NOT NULL UNIQUE,
    name        TEXT    NOT NULL,
    profile     TEXT    NOT NULL,
    created_at  INTEGER NOT NULL,
    last_seen   INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS duels (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    at           INTEGER NOT NULL,
    attacker_id  INTEGER NOT NULL REFERENCES players(id),
    defender_id  INTEGER REFERENCES players(id),
    outcome      TEXT    NOT NULL,
    tier         TEXT    NOT NULL,
    power        REAL    NOT NULL,
    mp_won       REAL    NOT NULL,
    mp_lost      REAL    NOT NULL
);
CREATE TABLE IF NOT EXISTS matches (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    at         INTEGER NOT NULL,
    a_id       INTEGER NOT NULL REFERENCES players(id),
    b_id       INTEGER NOT NULL REFERENCES players(id),
    winner_id  INTEGER REFERENCES players(id),
    stake      REAL    NOT NULL,
    rounds     INTEGER NOT NULL,
    ranked     INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS purchases (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    at         INTEGER NOT NULL,
    player_id  INTEGER NOT NULL REFERENCES players(id),
    item       TEXT    NOT NULL,
    cost       REAL    NOT NULL
);
";

/// One finished duel; `defender` is `None` for the training bot.
#[derive(Debug, Clone, PartialEq)]
pub struct DuelRecord {
    pub attacker: PlayerId,
    pub defender: Option<PlayerId>,
    pub outcome: &'static str,
    pub tier: &'static str,
    pub power: f64,
    pub mp_won: f64,
    pub mp_lost: f64,
}

/// One PvP duel; `winner` is `None` for a draw.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchRecord {
    pub a: PlayerId,
    pub b: PlayerId,
    pub winner: Option<PlayerId>,
    pub stake: f64,
    pub rounds: u32,
    /// Counted for records and achievements (not friendly, not a walkover).
    pub ranked: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PurchaseRecord {
    pub player: PlayerId,
    pub item: String,
    pub cost: f64,
}

pub struct Db {
    /// Open to the crate so each feature can keep its own queries next to its code.
    pub(crate) conn: Connection,
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Db> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> rusqlite::Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Db> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch(crate::chat::SCHEMA)?;
        conn.execute_batch(crate::clan::SCHEMA)?;
        // Databases from before the `ranked` column.
        let has_ranked: bool = conn.query_row("SELECT COUNT(*) FROM pragma_table_info('matches') WHERE name = 'ranked'", [], |r| r.get::<_, i64>(0))? > 0;
        if !has_ranked {
            conn.execute_batch("ALTER TABLE matches ADD COLUMN ranked INTEGER NOT NULL DEFAULT 1")?;
        }
        Ok(Db { conn })
    }

    /// Loads the profile for a machine, creating it on first sight; the name is refreshed.
    pub fn load_or_create(&self, machine_id: &str, name: &str) -> rusqlite::Result<(PlayerId, Player)> {
        let found: Option<(PlayerId, String)> = self
            .conn
            .query_row("SELECT id, profile FROM players WHERE machine_id = ?1", [machine_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        let (id, mut player) = match found {
            Some((id, json)) => (id, serde_json::from_str(&json).unwrap_or_default()),
            None => {
                let player = Player::default();
                let now = now_secs();
                self.conn.execute(
                    "INSERT INTO players (machine_id, name, profile, created_at, last_seen) VALUES (?1, ?2, ?3, ?4, ?4)",
                    params![machine_id, name, profile_json(&player), now],
                )?;
                (self.conn.last_insert_rowid(), player)
            }
        };
        player.name = name.to_string();
        Ok((id, player))
    }

    pub fn save(&self, id: PlayerId, player: &Player) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE players SET name = ?2, profile = ?3, last_seen = ?4 WHERE id = ?1",
            params![id, player.name, profile_json(player), now_secs()],
        )?;
        Ok(())
    }

    pub fn log_duel(&self, d: &DuelRecord) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO duels (at, attacker_id, defender_id, outcome, tier, power, mp_won, mp_lost)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![now_secs(), d.attacker, d.defender, d.outcome, d.tier, d.power, d.mp_won, d.mp_lost],
        )?;
        Ok(())
    }

    pub fn log_match(&self, m: &MatchRecord) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO matches (at, a_id, b_id, winner_id, stake, rounds, ranked) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![now_secs(), m.a, m.b, m.winner, m.stake, m.rounds, m.ranked],
        )?;
        Ok(())
    }

    pub fn match_count(&self) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM matches", [], |r| r.get(0))
    }

    pub fn log_purchase(&self, p: &PurchaseRecord) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO purchases (at, player_id, item, cost) VALUES (?1, ?2, ?3, ?4)",
            params![now_secs(), p.player, p.item, p.cost],
        )?;
        Ok(())
    }

    /// Runs several writes in one transaction.
    pub fn transaction<T>(&mut self, f: impl FnOnce(&Db) -> rusqlite::Result<T>) -> rusqlite::Result<T> {
        self.conn.execute_batch("BEGIN")?;
        match f(self) {
            Ok(value) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn player_count(&self) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM players", [], |r| r.get(0))
    }

    /// Counts duels by outcome, for checks and the server panel.
    pub fn duel_count(&self, outcome: Option<&str>) -> rusqlite::Result<i64> {
        match outcome {
            Some(o) => self.conn.query_row("SELECT COUNT(*) FROM duels WHERE outcome = ?1", [o], |r| r.get(0)),
            None => self.conn.query_row("SELECT COUNT(*) FROM duels", [], |r| r.get(0)),
        }
    }

    pub fn purchase_count(&self) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM purchases", [], |r| r.get(0))
    }
}

fn profile_json(player: &Player) -> String {
    serde_json::to_string(player).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ItemId;

    #[test]
    fn same_machine_keeps_progress_across_renames() {
        let db = Db::in_memory().unwrap();
        let (id, mut p) = db.load_or_create("m1", "Igor").unwrap();
        p.mp = 42.0;
        p.items.insert(ItemId::GuaSha, 2);
        db.save(id, &p).unwrap();

        let (again, loaded) = db.load_or_create("m1", "Igor Novo").unwrap();
        assert_eq!(again, id);
        assert_eq!(loaded.mp, 42.0);
        assert_eq!(loaded.level(ItemId::GuaSha), 2);
        assert_eq!(loaded.name, "Igor Novo");
        assert_eq!(db.player_count().unwrap(), 1);
    }

    #[test]
    fn different_machines_get_different_players() {
        let db = Db::in_memory().unwrap();
        let (a, _) = db.load_or_create("m1", "Igor").unwrap();
        let (b, _) = db.load_or_create("m2", "Igor").unwrap();
        assert_ne!(a, b, "same name, different computer");
    }

    #[test]
    fn history_is_recorded() {
        let mut db = Db::in_memory().unwrap();
        let (a, _) = db.load_or_create("m1", "A").unwrap();
        let (b, _) = db.load_or_create("m2", "B").unwrap();
        db.transaction(|db| {
            db.log_duel(&DuelRecord { attacker: a, defender: Some(b), outcome: "won", tier: "PERFECT", power: 9.0, mp_won: 30.0, mp_lost: 30.0 })?;
            db.log_duel(&DuelRecord { attacker: a, defender: None, outcome: "lost", tier: "RUIM", power: 3.0, mp_won: 0.0, mp_lost: 2.0 })?;
            db.log_purchase(&PurchaseRecord { player: a, item: "GuaSha".into(), cost: 30.0 })
        })
        .unwrap();
        assert_eq!(db.duel_count(None).unwrap(), 2);
        assert_eq!(db.duel_count(Some("won")).unwrap(), 1);
        assert_eq!(db.purchase_count().unwrap(), 1);
    }

    #[test]
    fn data_survives_reopening_the_file() {
        let dir = std::env::temp_dir().join(format!("pmog-db-{}", rand::random::<u64>()));
        let path = dir.join("server.db");
        {
            let db = Db::open(&path).unwrap();
            let (id, mut p) = db.load_or_create("m1", "A").unwrap();
            p.lifetime = 999.0;
            db.save(id, &p).unwrap();
        }
        let db = Db::open(&path).unwrap();
        let (_, p) = db.load_or_create("m1", "A").unwrap();
        assert_eq!(p.lifetime, 999.0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
