//! Saved worlds: one server database per world, each hosted on its own port.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::constants::{LEGACY_SERVER_DB, WORLD_EXTENSION, WORLD_NAME_KEY, WORLD_SIDE_FILES, WORLD_UNNAMED, WORLDS_DIR, WORLDS_TRASH_DIR};
use crate::db::Db;

/// One world saved on this computer.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldSave {
    /// Creation time in milliseconds plus a random suffix, so ids sort by age.
    pub id: u64,
    pub name: String,
    pub path: PathBuf,
}

/// Where worlds live and where deleted ones go.
#[derive(Debug, Clone)]
pub struct WorldStore {
    pub dir: PathBuf,
    pub trash: PathBuf,
}

fn db_error(e: rusqlite::Error) -> io::Error {
    io::Error::other(format!("banco de dados: {e}"))
}

fn file_name(id: u64) -> String {
    format!("{id:016x}.{WORLD_EXTENSION}")
}

/// `path` with a SQLite side-file suffix appended.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Moves a world database and its side files.
fn move_db(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)?;
    for suffix in WORLD_SIDE_FILES {
        let side = with_suffix(from, suffix);
        if side.exists() {
            fs::rename(side, with_suffix(to, suffix))?;
        }
    }
    Ok(())
}

impl WorldStore {
    /// Worlds under the game's data folder.
    pub fn in_data_dir() -> Option<WorldStore> {
        crate::utils::data_dir().map(|d| WorldStore::under(&d))
    }

    pub fn under(base: &Path) -> WorldStore {
        WorldStore { dir: base.join(WORLDS_DIR), trash: base.join(WORLDS_TRASH_DIR) }
    }

    /// Every saved world, oldest first.
    pub fn list(&self) -> Vec<WorldSave> {
        let Ok(entries) = fs::read_dir(&self.dir) else { return Vec::new() };
        let mut worlds: Vec<WorldSave> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension()? != WORLD_EXTENSION {
                    return None;
                }
                let id = u64::from_str_radix(path.file_stem()?.to_str()?, 16).ok()?;
                let name = Db::open(&path).ok().and_then(|db| db.meta(WORLD_NAME_KEY).ok().flatten());
                Some(WorldSave { id, name: name.unwrap_or_else(|| WORLD_UNNAMED.to_string()), path })
            })
            .collect();
        worlds.sort_by_key(|w| w.id);
        worlds
    }

    /// Id and path for a world that does not exist yet.
    fn new_slot(&self) -> io::Result<(u64, PathBuf)> {
        fs::create_dir_all(&self.dir)?;
        let millis = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        let id = (millis << 16) | u64::from(rand::random::<u16>());
        Ok((id, self.dir.join(file_name(id))))
    }

    /// A new empty world called `name`.
    pub fn create(&self, name: &str) -> io::Result<WorldSave> {
        let (id, path) = self.new_slot()?;
        let world = WorldSave { id, name: name.to_string(), path };
        self.rename(&world, name)?;
        Ok(world)
    }

    pub fn rename(&self, world: &WorldSave, name: &str) -> io::Result<()> {
        Db::open(&world.path).and_then(|db| db.set_meta(WORLD_NAME_KEY, name)).map_err(db_error)
    }

    /// Moves a world to the trash folder, where it can still be recovered by hand.
    pub fn trash(&self, world: &WorldSave) -> io::Result<()> {
        fs::create_dir_all(&self.trash)?;
        move_db(&world.path, &self.trash.join(file_name(world.id)))
    }

    /// Turns the single server database from older versions into the first world.
    pub fn import_legacy(&self, base: &Path, name: &str) -> io::Result<Option<WorldSave>> {
        let legacy = base.join(LEGACY_SERVER_DB);
        if !legacy.exists() {
            return Ok(None);
        }
        let (id, path) = self.new_slot()?;
        move_db(&legacy, &path)?;
        let world = WorldSave { id, name: name.to_string(), path };
        self.rename(&world, name)?;
        Ok(Some(world))
    }
}
