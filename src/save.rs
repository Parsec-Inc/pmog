//! Local client settings (the player name). Progress lives on the server.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub name: String,
    /// Whether game events may show system notifications.
    pub notifications: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config { name: String::new(), notifications: true }
    }
}

fn path() -> Option<PathBuf> {
    crate::utils::data_dir().map(|dir| dir.join("config.json"))
}

pub fn load() -> Config {
    path().map(|p| load_from(&p)).unwrap_or_default()
}

/// Reads the settings at `path`, falling back to the defaults.
pub fn load_from(path: &Path) -> Config {
    fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

/// Writes the settings atomically.
pub fn store(config: &Config) {
    if let Some(path) = path() {
        store_to(&path, config);
    }
}

/// Writes the settings atomically at `path`.
pub fn store_to(path: &Path, config: &Config) {
    let Ok(bytes) = serde_json::to_vec_pretty(config) else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, bytes).is_ok() {
        let _ = fs::rename(tmp, path);
    }
}
