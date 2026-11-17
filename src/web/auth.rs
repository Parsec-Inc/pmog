//! Phone access settings: the password set on the PC and the browsers that already entered it.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::constants::WEB_MAX_TRUSTED;

/// Saved with the local config; only salted hashes are kept.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhoneSettings {
    pub salt: String,
    pub password_hash: String,
    /// Hashes of browser tokens that already typed the password, newest last.
    pub trusted: Vec<String>,
    /// When on, a connected phone takes over and the PC only watches.
    pub takeover: bool,
}

fn hash(salt: &str, text: &str) -> String {
    let digest = Sha256::new().chain_update(salt.as_bytes()).chain_update(b"|").chain_update(text.as_bytes()).finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compares without stopping at the first different byte.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl PhoneSettings {
    pub fn has_password(&self) -> bool {
        !self.password_hash.is_empty()
    }

    /// Replaces the password; every browser has to type the new one.
    pub fn set_password(&mut self, password: &str) {
        self.salt = (0..16).map(|_| format!("{:02x}", rand::random::<u8>())).collect();
        self.password_hash = hash(&self.salt, password);
        self.trusted.clear();
    }

    pub fn check_password(&self, password: &str) -> bool {
        self.has_password() && same(&hash(&self.salt, password), &self.password_hash)
    }

    pub fn trusts(&self, token: &str) -> bool {
        self.has_password() && self.trusted.iter().any(|t| same(t, &hash(&self.salt, token)))
    }

    /// Remembers a browser, forgetting the oldest past the limit.
    pub fn trust(&mut self, token: &str) {
        let hashed = hash(&self.salt, token);
        self.trusted.retain(|t| t != &hashed);
        self.trusted.push(hashed);
        let extra = self.trusted.len().saturating_sub(WEB_MAX_TRUSTED);
        self.trusted.drain(..extra);
    }
}
