#!/usr/bin/env bash
# Gera e assina dist/pmog-linux (musl estático) e, se o mingw estiver instalado, dist/pmog-windows.exe.
# A assinatura usa a chave de ~/.config/pmog-release (crie uma vez com: cargo run --example release -- keygen).
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p dist

if [ ! -s release-pubkey.txt ]; then
  echo "Falta release-pubkey.txt. Rode uma vez: cargo run --example release -- keygen"
  exit 1
fi
export PMOG_RELEASE="${PMOG_RELEASE:-$(date +%Y%m%d%H%M)}"

# Copia por um arquivo temporário e renomeia: funciona mesmo com o executável antigo aberto.
publish() {
  cp "$1" "$2.tmp"
  strip "$2.tmp" 2>/dev/null || x86_64-w64-mingw32-strip "$2.tmp"
  mv -f "$2.tmp" "$2"
  cargo run -q --release --example release -- sign "$3" "$PMOG_RELEASE" "$2"
}

# musl-gcc (musl-tools no Ubuntu) evita o _FORTIFY_SOURCE do gcc do Ubuntu, que puxa __memcpy_chk da glibc
CC_x86_64_unknown_linux_musl="$(command -v musl-gcc || echo gcc)" CFLAGS_x86_64_unknown_linux_musl="-DSQLITE_DISABLE_LFS" \
  cargo build --release --target x86_64-unknown-linux-musl
publish target/x86_64-unknown-linux-musl/release/pmog dist/pmog-linux linux

if command -v x86_64-w64-mingw32-gcc >/dev/null; then
  CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc \
    cargo build --release --target x86_64-pc-windows-gnu
  publish target/x86_64-pc-windows-gnu/release/pmog.exe dist/pmog-windows.exe windows
else
  echo "mingw ausente: instale mingw64-gcc e mingw64-winpthreads-static para gerar o .exe"
fi
echo "release $PMOG_RELEASE pronta em dist/ (o host serve as duas plataformas a partir desta pasta)"
