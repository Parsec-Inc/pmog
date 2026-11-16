//! Bakes the release number and the update signing public key into the binary.

fn main() {
    println!("cargo:rerun-if-env-changed=PMOG_RELEASE");
    println!("cargo:rerun-if-changed=release-pubkey.txt");
    let release = std::env::var("PMOG_RELEASE").ok().filter(|r| r.parse::<u64>().is_ok()).unwrap_or_else(|| "0".into());
    let pubkey = std::fs::read_to_string("release-pubkey.txt").map(|k| k.trim().to_string()).unwrap_or_default();
    println!("cargo:rustc-env=PMOG_RELEASE={release}");
    println!("cargo:rustc-env=PMOG_RELEASE_PUBKEY={pubkey}");
}
