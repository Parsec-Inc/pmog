//! Release signing for self-update. The private key stays in the user's config folder and is never printed.
//!
//! cargo run --example release -- keygen
//! cargo run --example release -- sign <linux|windows|macos> <release> <binary>
//! cargo run --example release -- check <dir>

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::exit;

use ed25519_dalek::SigningKey;
use pmog::update::{Manifest, Shelf, hex_encode, release_label, sidecar};
use rand::rngs::OsRng;

fn key_path() -> PathBuf {
    dirs::config_dir().expect("pasta de configuração").join("pmog-release").join("release.key")
}

fn fail(text: &str) -> ! {
    eprintln!("{text}");
    exit(1);
}

fn keygen() {
    let path = key_path();
    if path.exists() {
        fail(&format!("Já existe uma chave em {}. Não vou sobrescrever.", path.display()));
    }
    let dir = path.parent().expect("pasta da chave");
    fs::create_dir_all(dir).unwrap_or_else(|e| fail(&format!("Não consegui criar {}: {e}", dir.display())));
    let key = SigningKey::generate(&mut OsRng);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    let mut file = options.open(&path).unwrap_or_else(|e| fail(&format!("Não consegui criar a chave: {e}")));
    file.write_all(&key.to_bytes()).unwrap_or_else(|e| fail(&format!("Não consegui gravar a chave: {e}")));
    let public = hex_encode(key.verifying_key().as_bytes());
    fs::write("release-pubkey.txt", format!("{public}\n")).unwrap_or_else(|e| fail(&format!("release-pubkey.txt: {e}")));
    println!("Chave privada criada em {} (só leitura sua; faça um backup offline).", path.display());
    println!("Chave pública gravada em release-pubkey.txt: {public}");
}

fn load_key() -> SigningKey {
    let path = key_path();
    let bytes = fs::read(&path).unwrap_or_else(|_| fail("Sem chave de release. Rode: cargo run --example release -- keygen"));
    let bytes: [u8; 32] = bytes.try_into().unwrap_or_else(|_| fail("Arquivo de chave corrompido."));
    SigningKey::from_bytes(&bytes)
}

fn sign(platform: &str, release: &str, binary: &str) {
    if !["linux", "windows", "macos"].contains(&platform) {
        fail("Plataforma deve ser linux, windows ou macos.");
    }
    let release: u64 = release.parse().unwrap_or_else(|_| fail("Release deve ser um número."));
    let bytes = fs::read(binary).unwrap_or_else(|e| fail(&format!("{binary}: {e}")));
    let key = load_key();
    let manifest = Manifest::sign(&key, platform, release, &bytes);
    let public = hex_encode(key.verifying_key().as_bytes());
    manifest.verify(&public, &bytes).unwrap_or_else(|e| fail(&e));
    let out = sidecar(binary.as_ref());
    fs::write(&out, serde_json::to_vec_pretty(&manifest).expect("json")).unwrap_or_else(|e| fail(&format!("{}: {e}", out.display())));
    println!("{} assinado ({platform}, release {release})", binary);
}

/// Shows what a host running from `dir` would hand out.
fn check(dir: &str) {
    let public = fs::read_to_string("release-pubkey.txt").unwrap_or_else(|_| fail("Falta release-pubkey.txt."));
    let offers = Shelf::scan(dir.as_ref(), public.trim()).offers();
    if offers.is_empty() {
        fail(&format!("Nenhuma build assinada válida em {dir}."));
    }
    for (platform, release) in offers {
        println!("{platform}: {}", release_label(release));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["keygen"] => keygen(),
        ["sign", platform, release, binary] => sign(platform, release, binary),
        ["check", dir] => check(dir),
        _ => fail("uso: release keygen | release sign <linux|windows|macos> <release> <binário> | release check <pasta>"),
    }
}
