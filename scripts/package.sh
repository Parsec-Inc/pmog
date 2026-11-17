#!/usr/bin/env bash
# Pacotes da release em dist/, a partir das builds do scripts/build-dist.sh. Local e CI usam este script.
#   scripts/package.sh deb | rpm | aur | msi | macos | brew   (rpm também gera o .src.rpm do COPR)
# VERSION vem do Cargo.toml; o CI passa a versão do bump (e o PMOG_RELEASE, para o macos).
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}
LINUX_BIN=dist/pmog-linux
WINDOWS_BIN=dist/pmog-windows.exe
MACOS_BIN=dist/pmog-macos
MACOS_TAR="dist/pmog-$VERSION-macos-universal.tar.gz"
mkdir -p dist

need() { command -v "$1" >/dev/null || { echo "faltando '$1': $2" >&2; exit 1; }; }
built() { [ -f "$1" ] || { echo "falta $1: rode antes $2" >&2; exit 1; }; }

deb() {
  need cargo-deb "cargo install cargo-deb"
  built "$LINUX_BIN" scripts/build-dist.sh
  cargo deb --no-build --no-strip --deb-version "$VERSION" --output "dist/pmog_${VERSION}_amd64.deb"
}

# .rpm e .src.rpm (o COPR recompila o .src.rpm) com a spec e os arquivos de packaging/linux.
rpm() {
  need rpmbuild "sudo dnf install rpm-build (ou apt install rpm)"
  built "$LINUX_BIN" scripts/build-dist.sh
  local top
  top=$(mktemp -d)
  mkdir -p "$top/SOURCES" "$top/SPECS"
  cp "$LINUX_BIN" packaging/linux/* "$top/SOURCES/"
  sed "s/@VERSION@/$VERSION/" packaging/rpm/pmog.spec > "$top/SPECS/pmog.spec"
  rpmbuild --quiet --define "_topdir $top" --define "dist %{nil}" --target x86_64 -ba "$top/SPECS/pmog.spec"
  find "$top/RPMS" "$top/SRPMS" -name '*.rpm' -exec cp {} dist/ \;
  rm -rf "$top"
}

# PKGBUILD do AUR (pmog-bin) com a versão; as somas o deploy calcula.
aur() {
  mkdir -p dist/aur
  sed "s/@VERSION@/$VERSION/" packaging/aur/PKGBUILD > dist/aur/PKGBUILD
  cp packaging/aur/pmog.install dist/aur/
}

# MSI com exceção no firewall: precisa do WiX 3 (a extensão de firewall não existe no wixl).
msi() {
  need candle "WiX 3 no Windows (choco install wixtoolset)"
  built "$WINDOWS_BIN" scripts/build-dist.sh
  mkdir -p target/wix
  candle -nologo -arch x64 -ext WixFirewallExtension -dVersion="$VERSION" -dExe="$PWD/$WINDOWS_BIN" \
    -out target/wix/pmog.wixobj packaging/windows/pmog.wxs
  light -nologo -spdb -ext WixFirewallExtension -out "dist/pmog-$VERSION-x86_64.msi" target/wix/pmog.wixobj
}

# macOS: binário universal (Apple Silicon + Intel). Só roda num Mac: precisa do SDK da Apple.
# Sem PMOG_RELEASE (o CI passa o do bump) sai uma build de desenvolvimento, sem auto-update.
macos() {
  [ "$(uname -s)" = Darwin ] || { echo "o binário do macOS só builda num Mac (ou no runner macOS do CI)" >&2; exit 1; }
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  cargo build --locked --release --target aarch64-apple-darwin
  cargo build --locked --release --target x86_64-apple-darwin
  lipo -create -output "$MACOS_BIN" target/aarch64-apple-darwin/release/pmog target/x86_64-apple-darwin/release/pmog
  strip -x "$MACOS_BIN"
}

# Fórmula do Homebrew e o tarball que ela baixa (o binário já assinado vai junto com o .sig).
brew() {
  built "$MACOS_BIN" "scripts/package.sh macos"
  local work sum
  work=$(mktemp -d)
  cp "$MACOS_BIN" "$work/pmog"
  [ -f "$MACOS_BIN.sig" ] && cp "$MACOS_BIN.sig" "$work/pmog.sig"
  tar czf "$MACOS_TAR" -C "$work" .
  rm -rf "$work"
  sum=$(sha256sum "$MACOS_TAR" | cut -d' ' -f1)
  mkdir -p dist/brew
  sed "s/@VERSION@/$VERSION/g; s/@SHA256@/$sum/" packaging/homebrew/pmog.rb > dist/brew/pmog.rb
}

[ $# -ge 1 ] || { sed -n '2,4p' "$0"; exit 1; }
for cmd in "$@"; do
  case $cmd in
    deb | rpm | aur | msi | macos | brew) "$cmd" ;;
    *) echo "artefato desconhecido: $cmd" >&2; exit 1 ;;
  esac
done
ls -lh dist
