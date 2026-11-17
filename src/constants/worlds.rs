//! Saved worlds on this computer.

/// Folder under the data dir with one database per world.
pub const WORLDS_DIR: &str = "worlds";
/// Folder under the data dir where deleted worlds go, still recoverable.
pub const WORLDS_TRASH_DIR: &str = "lixeira";
/// The single server database from before worlds existed.
pub const LEGACY_SERVER_DB: &str = "server.db";
/// Extension of a world database.
pub const WORLD_EXTENSION: &str = "db";
/// SQLite side files that travel with a world database.
pub const WORLD_SIDE_FILES: [&str; 2] = ["-wal", "-shm"];
/// Key of the world name in the database's meta table.
pub const WORLD_NAME_KEY: &str = "world_name";
/// Shown for a world whose name could not be read.
pub const WORLD_UNNAMED: &str = "Mundo sem nome";
