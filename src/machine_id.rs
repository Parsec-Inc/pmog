//! Stable per-computer id derived from the hardware UUID, hashed and cached.

use std::fs;
#[cfg(all(unix, not(target_os = "macos")))]
use std::io;
use std::path::PathBuf;
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::constants::MACHINE_ID_SALT;

#[cfg(all(unix, not(target_os = "macos")))]
const PRODUCT_UUID: &str = "/sys/class/dmi/id/product_uuid";
#[cfg(all(unix, not(target_os = "macos")))]
const BOARD_SERIAL: &str = "/sys/class/dmi/id/board_serial";

/// Placeholder UUIDs some vendors ship on every board.
const BOGUS_UUIDS: [&str; 4] = [
    "00000000-0000-0000-0000-000000000000",
    "ffffffff-ffff-ffff-ffff-ffffffffffff",
    "03000200-0400-0500-0006-000700080009",
    "00020003-0004-0005-0006-000700080009",
];

#[derive(Debug)]
pub enum IdError {
    /// Reading needs elevated permission and the user did not grant it.
    Denied(String),
    /// The hardware exposes no usable identifier.
    Unavailable(String),
}

impl std::fmt::Display for IdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdError::Denied(why) => write!(f, "permissão negada para ler o UUID do hardware ({why})"),
            IdError::Unavailable(why) => write!(f, "UUID do hardware indisponível ({why})"),
        }
    }
}

/// Hashes raw hardware identifiers into the public machine id.
pub fn derive(product_uuid: &str, board_serial: &str) -> Result<String, IdError> {
    let uuid = product_uuid.trim().to_ascii_lowercase();
    let serial = board_serial.trim();
    if uuid.is_empty() {
        return Err(IdError::Unavailable("UUID vazio".into()));
    }
    if BOGUS_UUIDS.contains(&uuid.as_str()) && serial.is_empty() {
        return Err(IdError::Unavailable("UUID genérico do fabricante e sem número de série".into()));
    }
    let mut hasher = Sha256::new();
    hasher.update(MACHINE_ID_SALT.as_bytes());
    hasher.update(b"|");
    hasher.update(uuid.as_bytes());
    hasher.update(b"|");
    hasher.update(serial.as_bytes());
    let digest = hasher.finalize();
    Ok(digest.iter().take(16).map(|b| format!("{b:02x}")).collect())
}

/// Accepts only ids in the format `derive` produces.
pub fn is_valid(id: &str) -> bool {
    id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// Id for headless test bots, derived from the bot name only.
pub fn for_bot(name: &str) -> String {
    derive(&format!("bot-{name}"), "bot").expect("non-empty input")
}

fn cache_path() -> Option<PathBuf> {
    crate::utils::data_dir().map(|d| d.join("machine-id"))
}

fn read_cache() -> Option<String> {
    let id = fs::read_to_string(cache_path()?).ok()?;
    let id = id.trim().to_string();
    is_valid(&id).then_some(id)
}

fn write_cache(id: &str) {
    if let Some(path) = cache_path() {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, id);
    }
}

/// Reads the identifiers directly (works as root or on permissive systems).
#[cfg(all(unix, not(target_os = "macos")))]
fn read_direct() -> io::Result<(String, String)> {
    let uuid = fs::read_to_string(PRODUCT_UUID)?;
    let serial = fs::read_to_string(BOARD_SERIAL).unwrap_or_default();
    Ok((uuid, serial))
}

/// Reads the identifiers through polkit, which asks for the admin password.
#[cfg(all(unix, not(target_os = "macos")))]
fn read_elevated() -> Result<(String, String), IdError> {
    let script = format!("cat {PRODUCT_UUID}; echo '|'; cat {BOARD_SERIAL} 2>/dev/null; true");
    let output = Command::new("pkexec")
        .args(["sh", "-c", &script])
        .output()
        .map_err(|e| IdError::Denied(format!("pkexec indisponível: {e}")))?;
    if !output.status.success() {
        return Err(IdError::Denied("autorização recusada".into()));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let (uuid, serial) = text.split_once('|').unwrap_or((&text, ""));
    Ok((uuid.to_string(), serial.to_string()))
}

/// Reads the same SMBIOS UUID and board serial on Windows, which needs no admin rights.
#[cfg(windows)]
fn read_windows() -> Result<(String, String), IdError> {
    let script = "(Get-CimInstance Win32_ComputerSystemProduct).UUID; '|'; (Get-CimInstance Win32_BaseBoard).SerialNumber";
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|e| IdError::Unavailable(format!("PowerShell indisponível: {e}")))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let (uuid, serial) = text.split_once('|').unwrap_or((&text, ""));
    Ok((uuid.trim().to_string(), serial.trim().to_string()))
}

/// Returns the cached id, or reads the hardware (asking for permission once on Linux).
#[cfg(windows)]
pub fn resolve(_allow_prompt: bool) -> Result<String, IdError> {
    if let Some(id) = read_cache() {
        return Ok(id);
    }
    // Wine and some locked-down Windows installs have no working PowerShell; the install's MachineGuid still works.
    let id = read_windows().and_then(|(uuid, serial)| derive(&uuid, &serial)).or_else(|_| {
        let guid = read_machine_guid()?;
        derive(&guid, MACHINE_GUID_TAG)
    })?;
    write_cache(&id);
    Ok(id)
}

#[cfg(windows)]
fn read_machine_guid() -> Result<String, IdError> {
    let output = Command::new("reg")
        .args(["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"])
        .output()
        .map_err(|e| IdError::Unavailable(format!("registro indisponível: {e}")))?;
    reg_value(&String::from_utf8_lossy(&output.stdout), "MachineGuid").ok_or_else(|| IdError::Unavailable("MachineGuid ausente".into()))
}

/// Marks ids derived from the Windows MachineGuid instead of the hardware UUID.
#[cfg(any(windows, test))]
const MACHINE_GUID_TAG: &str = "machine-guid";

/// Value of `name` in `reg query` output.
#[cfg(any(windows, test))]
fn reg_value(text: &str, name: &str) -> Option<String> {
    text.lines()
        .map(str::split_whitespace)
        .find_map(|mut parts| (parts.next() == Some(name)).then(|| parts.nth(1).map(str::to_string)).flatten())
}

/// Returns the cached id, or reads the hardware UUID and serial from IOKit, which needs no admin rights.
#[cfg(target_os = "macos")]
pub fn resolve(_allow_prompt: bool) -> Result<String, IdError> {
    if let Some(id) = read_cache() {
        return Ok(id);
    }
    let output = Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .map_err(|e| IdError::Unavailable(format!("ioreg indisponível: {e}")))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let uuid = ioreg_value(&text, "IOPlatformUUID").ok_or_else(|| IdError::Unavailable("IOPlatformUUID ausente".into()))?;
    let id = derive(&uuid, &ioreg_value(&text, "IOPlatformSerialNumber").unwrap_or_default())?;
    write_cache(&id);
    Ok(id)
}

/// Value of `"key" = "value"` in `ioreg` output.
#[cfg(any(target_os = "macos", test))]
fn ioreg_value(text: &str, key: &str) -> Option<String> {
    let prefix = format!("\"{key}\" = \"");
    text.lines().find_map(|line| line.trim().strip_prefix(prefix.as_str())?.strip_suffix('"').map(str::to_string))
}

/// Returns the cached id, or reads the hardware (asking for permission once).
#[cfg(all(unix, not(target_os = "macos")))]
pub fn resolve(allow_prompt: bool) -> Result<String, IdError> {
    if let Some(id) = read_cache() {
        return Ok(id);
    }
    let (uuid, serial) = match read_direct() {
        Ok(found) => found,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied && allow_prompt => read_elevated()?,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return Err(IdError::Denied(e.to_string())),
        Err(e) => return Err(IdError::Unavailable(e.to_string())),
    };
    let id = derive(&uuid, &serial)?;
    write_cache(&id);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_is_stable_and_case_insensitive() {
        let a = derive("4C4C4544-0042-3510-8051-B4C04F4A4E32\n", "ABC123").unwrap();
        let b = derive("4c4c4544-0042-3510-8051-b4c04f4a4e32", "ABC123\n").unwrap();
        assert_eq!(a, b);
        assert!(is_valid(&a));
    }

    #[test]
    fn machine_guid_is_read_from_reg_output() {
        let text = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Cryptography\r\n    MachineGuid    REG_SZ    a1ff69ec-2f73-462e-b523-d97fe5b81b29\r\n\r\n";
        assert_eq!(reg_value(text, "MachineGuid").as_deref(), Some("a1ff69ec-2f73-462e-b523-d97fe5b81b29"));
        assert_eq!(reg_value("", "MachineGuid"), None);
        assert!(derive("a1ff69ec-2f73-462e-b523-d97fe5b81b29", MACHINE_GUID_TAG).is_ok());
    }

    #[test]
    fn mac_uuid_and_serial_are_read_from_ioreg_output() {
        let text = "+-o J314sAP  <class IOPlatformExpertDevice>\n    {\n      \"IOPlatformSerialNumber\" = \"C02XK1ABCD12\"\n      \"IOPlatformUUID\" = \"0A1B2C3D-1111-2222-3333-444455556666\"\n    }\n";
        assert_eq!(ioreg_value(text, "IOPlatformUUID").as_deref(), Some("0A1B2C3D-1111-2222-3333-444455556666"));
        assert_eq!(ioreg_value(text, "IOPlatformSerialNumber").as_deref(), Some("C02XK1ABCD12"));
        assert_eq!(ioreg_value(text, "missing"), None);
    }

    #[test]
    fn different_hardware_gives_different_ids() {
        let a = derive("4c4c4544-0042-3510-8051-b4c04f4a4e32", "").unwrap();
        let b = derive("4c4c4544-0042-3510-8051-b4c04f4a4e33", "").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn raw_uuid_is_not_exposed() {
        let id = derive("4c4c4544-0042-3510-8051-b4c04f4a4e32", "").unwrap();
        assert!(!id.contains("4c4c4544"));
    }

    #[test]
    fn bogus_uuid_needs_a_serial() {
        assert!(derive("03000200-0400-0500-0006-000700080009", "").is_err());
        let with_serial = derive("03000200-0400-0500-0006-000700080009", "SN1").unwrap();
        let other_serial = derive("03000200-0400-0500-0006-000700080009", "SN2").unwrap();
        assert_ne!(with_serial, other_serial);
        assert!(derive("", "SN1").is_err());
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn this_computer_has_a_stable_id() {
        let id = resolve(false).unwrap();
        assert!(is_valid(&id));
        assert_eq!(resolve(false).unwrap(), id);
    }

    #[test]
    fn validation_rejects_garbage() {
        assert!(!is_valid(""));
        assert!(!is_valid("xyz"));
        assert!(!is_valid(&"A".repeat(32)));
        assert!(!is_valid(&"g".repeat(32)));
        assert!(is_valid(&for_bot("x")));
        assert_ne!(for_bot("x"), for_bot("y"));
    }
}
