//! Self-update: hosts on the LAN or the GitHub releases hand out signed builds, players confirm, the executable swaps itself.
//!
//! The LAN request line and reply format below must never change, so any old build can update from any new host.

use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::constants::{CONNECT_TIMEOUT, GITHUB_ACCEPT, GITHUB_LATEST_URL, MAX_UPDATE_BYTES, UPDATE_TIMEOUT, UPDATE_USER_AGENT, WEB_UPDATE_TIMEOUT};
use crate::protocol::{read_line, write_msg};

/// Build number baked in by `scripts/build-dist.sh`; 0 for development builds, which never update.
pub fn release() -> u64 {
    env!("PMOG_RELEASE").parse().unwrap_or(0)
}

/// Public key that release builds must be signed with; empty when none was set up.
pub fn release_pubkey() -> &'static str {
    env!("PMOG_RELEASE_PUBKEY")
}

/// Whether this build takes part in updates at all.
pub fn enabled() -> bool {
    release() > 0 && !release_pubkey().is_empty()
}

pub fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Why a client on `client` (protocol, release) can't join a server on `server`; `None` when they match.
pub fn version_refusal(client: (u32, u64), server: (u32, u64)) -> Option<&'static str> {
    if client == server {
        None
    } else if client.1 > server.1 {
        Some("O servidor está numa versão mais velha que a sua. Peça para quem hospeda atualizar o PMOG.")
    } else if client.1 < server.1 {
        Some("Seu PMOG está desatualizado para este servidor. Atualize pela tela inicial (Enter no servidor ou U).")
    } else {
        Some("Versão do jogo diferente da do servidor. Atualize o PMOG.")
    }
}

/// "build 2026-11-16 18:30" for a release number like 202611161830.
pub fn release_label(release: u64) -> String {
    let s = release.to_string();
    if s.len() != 12 {
        return format!("build {s}");
    }
    format!("build {}-{}-{} {}:{}", &s[0..4], &s[4..6], &s[6..8], &s[8..10], &s[10..12])
}

/// What a signed build is: kept in a `.sig` file next to the executable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub platform: String,
    pub release: u64,
    pub size: u64,
    pub sha256: String,
    pub signature: String,
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect()
}

pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Manifest {
    fn message(platform: &str, release: u64, size: u64, sha256: &str) -> String {
        format!("pmog-release:{platform}:{release}:{size}:{sha256}")
    }

    pub fn sign(key: &SigningKey, platform: &str, release: u64, bytes: &[u8]) -> Manifest {
        let (size, sha256) = (bytes.len() as u64, sha256_hex(bytes));
        let signature = key.sign(Self::message(platform, release, size, &sha256).as_bytes());
        Manifest { platform: platform.into(), release, size, sha256, signature: hex_encode(&signature.to_bytes()) }
    }

    /// Checks that `bytes` are exactly the build this manifest describes and that `pubkey_hex` signed it.
    pub fn verify(&self, pubkey_hex: &str, bytes: &[u8]) -> Result<(), String> {
        let key: [u8; 32] = hex_decode(pubkey_hex).and_then(|k| k.try_into().ok()).ok_or("chave pública de atualização inválida")?;
        let key = VerifyingKey::from_bytes(&key).map_err(|_| "chave pública de atualização inválida")?;
        let signature: [u8; 64] = hex_decode(&self.signature).and_then(|s| s.try_into().ok()).ok_or("assinatura malformada")?;
        if bytes.len() as u64 != self.size || sha256_hex(bytes) != self.sha256 {
            return Err("o arquivo baixado não bate com o anunciado".into());
        }
        key.verify_strict(Self::message(&self.platform, self.release, self.size, &self.sha256).as_bytes(), &Signature::from_bytes(&signature))
            .map_err(|_| "assinatura inválida: essa build não foi publicada pelo dono do jogo".into())
    }
}

/// Where the manifest of `binary` lives.
pub fn sidecar(binary: &Path) -> PathBuf {
    let mut name = binary.as_os_str().to_owned();
    name.push(".sig");
    PathBuf::from(name)
}

/// First line a client sends to ask for a build instead of joining.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateRequest {
    pub pmog_update: UpdateQuery,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateQuery {
    pub platform: String,
    pub have: u64,
}

/// Server reply; an offer is followed by exactly `size` raw bytes.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateReply {
    Offer(Manifest),
    None(String),
}

/// Signed builds a host can hand out.
#[derive(Debug, Default)]
pub struct Shelf {
    builds: Vec<(Manifest, PathBuf)>,
}

impl Shelf {
    /// Collects every build in `dir` whose `.sig` checks out against `pubkey_hex`, keeping the newest per platform.
    pub fn scan(dir: &Path, pubkey_hex: &str) -> Shelf {
        let mut shelf = Shelf::default();
        if pubkey_hex.is_empty() {
            return shelf;
        }
        let Ok(entries) = fs::read_dir(dir) else { return shelf };
        for path in entries.flatten().map(|e| e.path()) {
            let Some(binary) = path.to_str().and_then(|p| p.strip_suffix(".sig")).map(PathBuf::from) else { continue };
            let Some(manifest) = fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Manifest>(&b).ok()) else { continue };
            let Ok(bytes) = fs::read(&binary) else { continue };
            if manifest.verify(pubkey_hex, &bytes).is_err() {
                continue;
            }
            let newer = shelf.best(&manifest.platform).is_none_or(|(m, _)| manifest.release > m.release);
            if newer {
                shelf.builds.retain(|(m, _)| m.platform != manifest.platform);
                shelf.builds.push((manifest, binary));
            }
        }
        shelf
    }

    /// Builds found next to the running executable.
    pub fn beside_exe() -> Shelf {
        let dir = running_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        dir.map_or_else(Shelf::default, |d| Shelf::scan(&d, release_pubkey()))
    }

    pub fn best(&self, platform: &str) -> Option<&(Manifest, PathBuf)> {
        self.builds.iter().find(|(m, _)| m.platform == platform)
    }

    /// (platform, release) pairs for the discovery beacon.
    pub fn offers(&self) -> Vec<(String, u64)> {
        self.builds.iter().map(|(m, _)| (m.platform.clone(), m.release)).collect()
    }

    /// Answers one update request on `out`.
    pub fn serve(&self, query: &UpdateQuery, out: &mut impl Write) -> io::Result<()> {
        let Some((manifest, path)) = self.best(&query.platform).filter(|(m, _)| m.release > query.have) else {
            return write_msg(out, &UpdateReply::None("Este servidor não tem uma versão mais nova para você.".into()));
        };
        let bytes = fs::read(path)?;
        if bytes.len() as u64 != manifest.size {
            return write_msg(out, &UpdateReply::None("A build deste servidor mudou; reinicie o servidor.".into()));
        }
        write_msg(out, &UpdateReply::Offer(manifest.clone()))?;
        out.write_all(&bytes)?;
        out.flush()
    }
}

/// Parses the first line of a connection as an update request, if it is one.
pub fn as_request(line: &[u8]) -> Option<UpdateQuery> {
    serde_json::from_slice::<UpdateRequest>(line).ok().map(|r| r.pmog_update)
}

/// Downloads and checks the newest build for `platform` from the server at `addr`.
pub fn fetch(addr: SocketAddr, platform: &str, have: u64, pubkey_hex: &str) -> Result<(Manifest, Vec<u8>), String> {
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|e| format!("Não consegui conectar em {addr}: {e}"))?;
    let _ = stream.set_read_timeout(Some(UPDATE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(UPDATE_TIMEOUT));
    let request = UpdateRequest { pmog_update: UpdateQuery { platform: platform.into(), have } };
    write_msg(&mut stream, &request).map_err(|e| format!("Falha ao pedir a atualização: {e}"))?;
    let mut reader = BufReader::new(stream);
    let line = read_line(&mut reader).map_err(|e| format!("O servidor não respondeu: {e}"))?.ok_or("O servidor fechou a conexão.")?;
    let manifest = match serde_json::from_slice::<UpdateReply>(&line) {
        Ok(UpdateReply::Offer(m)) => m,
        Ok(UpdateReply::None(reason)) => return Err(reason),
        Err(_) => return Err("Este servidor é de uma versão sem atualização automática.".into()),
    };
    if manifest.platform != platform || manifest.release <= have || manifest.size > MAX_UPDATE_BYTES {
        return Err("O servidor ofereceu uma build que não serve para este computador.".into());
    }
    let mut bytes = Vec::with_capacity(manifest.size as usize);
    reader.by_ref().take(manifest.size).read_to_end(&mut bytes).map_err(|e| format!("Download interrompido: {e}"))?;
    manifest.verify(pubkey_hex, &bytes)?;
    Ok((manifest, bytes))
}

/// File name of a published build, as `scripts/build-dist.sh` writes it.
pub fn dist_name(platform: &str) -> &'static str {
    match platform {
        "windows" => "pmog-windows.exe",
        "macos" => "pmog-macos",
        _ => "pmog-linux",
    }
}

/// A newer signed build published in the GitHub releases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebOffer {
    pub tag: String,
    pub manifest: Manifest,
    pub binary_url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Downloads `url` with the system's curl, so the game needs no TLS stack of its own.
pub fn download(url: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", &WEB_UPDATE_TIMEOUT.as_secs().to_string(), "--max-filesize", &MAX_UPDATE_BYTES.to_string()])
        .args(["-H", GITHUB_ACCEPT, "-A", UPDATE_USER_AGENT, url])
        .output()
        .map_err(|e| format!("Não consegui rodar o curl: {e}"))?;
    if !output.status.success() {
        return Err(format!("Download falhou: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(output.stdout)
}

/// The latest GitHub release when it has a newer build for `platform`; `get` fetches a URL.
pub fn web_offer(platform: &str, have: u64, get: impl Fn(&str) -> Result<Vec<u8>, String>) -> Result<Option<WebOffer>, String> {
    let release: GithubRelease = serde_json::from_slice(&get(GITHUB_LATEST_URL)?).map_err(|e| format!("Resposta inesperada do GitHub: {e}"))?;
    let name = dist_name(platform);
    let url = |wanted: &str| release.assets.iter().find(|a| a.name == wanted).map(|a| a.browser_download_url.clone());
    let (Some(binary_url), Some(sig_url)) = (url(name), url(&format!("{name}.sig"))) else { return Ok(None) };
    let manifest: Manifest = serde_json::from_slice(&get(&sig_url)?).map_err(|e| format!("Assinatura malformada no GitHub: {e}"))?;
    if manifest.platform != platform || manifest.release <= have || manifest.size > MAX_UPDATE_BYTES {
        return Ok(None);
    }
    Ok(Some(WebOffer { tag: release.tag_name, manifest, binary_url }))
}

/// Downloads the offered build and checks it against its signed manifest.
pub fn fetch_web(offer: &WebOffer, pubkey_hex: &str, get: impl Fn(&str) -> Result<Vec<u8>, String>) -> Result<Vec<u8>, String> {
    let bytes = get(&offer.binary_url)?;
    offer.manifest.verify(pubkey_hex, &bytes)?;
    Ok(bytes)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Whether this user can create files in `dir`.
pub fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".pmog-write-test-{}", std::process::id()));
    let ok = fs::OpenOptions::new().write(true).create_new(true).open(&probe).is_ok();
    if ok {
        let _ = fs::remove_file(&probe);
    }
    ok
}

/// Puts a verified build in place of `exe`, keeping its manifest next to it so this computer can pass it on.
/// When a package installed the game where this user can't write, the system asks for the admin password.
pub fn install(exe: &Path, manifest: &Manifest, bytes: &[u8]) -> Result<(), String> {
    let manifest = serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?;
    match exe.parent() {
        Some(dir) if dir.is_dir() && !writable(dir) => install_elevated(exe, &manifest, bytes),
        _ => install_direct(exe, &manifest, bytes),
    }
}

fn install_direct(exe: &Path, manifest: &[u8], bytes: &[u8]) -> Result<(), String> {
    let fail = |e: io::Error| format!("Não consegui trocar o executável em {}: {e}", exe.display());
    let fresh = with_suffix(exe, ".new");
    fs::write(&fresh, bytes).map_err(fail)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&fresh, fs::Permissions::from_mode(0o755)).map_err(fail)?;
    }
    // Windows can't overwrite a running program, but it can rename it out of the way.
    #[cfg(windows)]
    {
        let old = with_suffix(exe, ".old");
        let _ = fs::remove_file(&old);
        fs::rename(exe, &old).map_err(fail)?;
    }
    fs::rename(&fresh, exe).map_err(fail)?;
    fs::write(sidecar(exe), manifest).map_err(fail)
}

/// Stages the build in a temp folder and lets an elevated helper move it into place.
fn install_elevated(exe: &Path, manifest: &[u8], bytes: &[u8]) -> Result<(), String> {
    let staging = std::env::temp_dir().join(format!("pmog-update-{}", std::process::id()));
    let fail = |e: io::Error| format!("Não consegui preparar a atualização em {}: {e}", staging.display());
    fs::create_dir_all(&staging).map_err(fail)?;
    let (binary, signature) = (staging.join("pmog"), staging.join("pmog.sig"));
    fs::write(&binary, bytes).map_err(fail)?;
    fs::write(&signature, manifest).map_err(fail)?;
    let result = run_elevated(&binary, exe, &signature, &sidecar(exe), &staging);
    let _ = fs::remove_dir_all(&staging);
    result
}

/// Asks for the admin password and copies the staged build and manifest over `exe` and `sig`.
fn run_elevated(binary: &Path, exe: &Path, signature: &Path, sig: &Path, staging: &Path) -> Result<(), String> {
    let mut command = elevation_command(binary, exe, signature, sig, staging).map_err(|e| format!("Não consegui preparar a atualização: {e}"))?;
    let status = command.status().map_err(|e| format!("Não consegui pedir a senha de administrador: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("A troca do executável em {} não foi autorizada.", exe.display()))
    }
}

/// Shell steps that copy the new build next to `$2`, swap it in and write the manifest; paths come as `$1`..`$4`.
pub const ELEVATED_SWAP: &str = r#"install -m 755 "$1" "$2.new" && mv -f "$2.new" "$2" && install -m 644 "$3" "$4""#;

/// polkit on Linux, which shows the system password prompt.
#[cfg(all(unix, not(target_os = "macos")))]
fn elevation_command(binary: &Path, exe: &Path, signature: &Path, sig: &Path, _staging: &Path) -> io::Result<Command> {
    let mut command = Command::new("pkexec");
    command.args(linux_elevation_args(binary, exe, signature, sig));
    Ok(command)
}

/// Arguments for `pkexec`: the swap script and the four paths, never spliced into the script.
pub fn linux_elevation_args(binary: &Path, exe: &Path, signature: &Path, sig: &Path) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = ["/bin/sh", "-c", ELEVATED_SWAP, "pmog-update"].map(Into::into).to_vec();
    args.extend([binary, exe, signature, sig].map(|p| p.as_os_str().to_owned()));
    args
}

/// The macOS admin prompt through AppleScript, with the paths passed as `argv` and quoted by AppleScript itself.
#[cfg(target_os = "macos")]
fn elevation_command(binary: &Path, exe: &Path, signature: &Path, sig: &Path, _staging: &Path) -> io::Result<Command> {
    let script = [
        "on run argv",
        "set q to {}",
        "repeat with p in argv",
        "set end of q to quoted form of (p as text)",
        "end repeat",
        r#"do shell script "/bin/sh -c " & quoted form of (system attribute "PMOG_SWAP") & " pmog-update " & (item 1 of q) & " " & (item 2 of q) & " " & (item 3 of q) & " " & (item 4 of q) with administrator privileges"#,
        "end run",
    ];
    let mut command = Command::new("osascript");
    for line in script {
        command.args(["-e", line]);
    }
    command.args([binary, exe, signature, sig]).env("PMOG_SWAP", ELEVATED_SWAP);
    Ok(command)
}

/// UAC on Windows: a batch file in the staging folder does the swap, started elevated through PowerShell.
#[cfg(windows)]
fn elevation_command(binary: &Path, exe: &Path, signature: &Path, sig: &Path, staging: &Path) -> io::Result<Command> {
    let script = staging.join("swap.cmd");
    fs::write(&script, windows_swap_script(binary, exe, signature, sig))?;
    let mut command = crate::utils::hidden_powershell();
    command
        .arg("-Command")
        .arg("$p = Start-Process -FilePath $env:PMOG_SWAP -Verb RunAs -Wait -PassThru -WindowStyle Hidden; exit $p.ExitCode")
        .env("PMOG_SWAP", &script);
    Ok(command)
}

/// Batch steps that move the running exe aside (Windows can't overwrite it) and copy the new one in.
pub fn windows_swap_script(binary: &Path, exe: &Path, signature: &Path, sig: &Path) -> String {
    let quote = |p: &Path| format!("\"{}\"", p.display().to_string().replace('%', "%%"));
    let old = quote(&with_suffix(exe, ".old"));
    let (binary, exe, signature, sig) = (quote(binary), quote(exe), quote(signature), quote(sig));
    format!(
        "@echo off\r\ndel /f /q {old} 2>nul\r\nmove /y {exe} {old} >nul || exit /b 1\r\ncopy /y {binary} {exe} >nul || (move /y {old} {exe} >nul & exit /b 1)\r\ncopy /y {signature} {sig} >nul || exit /b 1\r\n"
    )
}

/// Removes what a previous update left behind on Windows.
pub fn cleanup(exe: &Path) {
    let _ = fs::remove_file(with_suffix(exe, ".old"));
    let _ = fs::remove_file(with_suffix(exe, ".new"));
}

/// Path of the running executable, resolved once so it survives being replaced on disk.
pub fn running_exe() -> Result<&'static Path, String> {
    static EXE: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    EXE.get_or_init(|| std::env::current_exe().map_err(|e| format!("Não achei o executável: {e}"))).as_deref().map_err(Clone::clone)
}

/// Downloads from `addr` and installs over the running executable.
pub fn update_from(addr: SocketAddr) -> Result<u64, String> {
    let exe = running_exe()?;
    let (manifest, bytes) = fetch(addr, platform(), release(), release_pubkey())?;
    install(exe, &manifest, &bytes)?;
    Ok(manifest.release)
}

/// Downloads the GitHub build and installs over the running executable.
pub fn update_from_web(offer: &WebOffer) -> Result<u64, String> {
    let exe = running_exe()?;
    let bytes = fetch_web(offer, release_pubkey(), download)?;
    install(exe, &offer.manifest, &bytes)?;
    Ok(offer.manifest.release)
}

/// Starts the freshly installed executable with the same arguments; never returns.
pub fn restart() -> ! {
    let exe = running_exe().expect("running executable");
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(exe).args(&args).exec();
        eprintln!("Atualizado, mas não consegui reabrir o jogo ({err}). Abra o PMOG de novo.");
        std::process::exit(1);
    }
    #[cfg(not(unix))]
    {
        let code = std::process::Command::new(exe).args(&args).status().ok().and_then(|s| s.code()).unwrap_or(0);
        std::process::exit(code);
    }
}
