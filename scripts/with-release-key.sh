#!/usr/bin/env bash
# CI: grava a chave de release (secret PMOG_RELEASE_KEY, em base64) só enquanto o comando roda.
set -euo pipefail
: "${PMOG_RELEASE_KEY:?defina PMOG_RELEASE_KEY}"
key_dir="${XDG_CONFIG_HOME:-$HOME/.config}/pmog-release"
mkdir -p "$key_dir"
(umask 077 && printf '%s' "$PMOG_RELEASE_KEY" | base64 -d > "$key_dir/release.key")
trap 'rm -f "$key_dir/release.key"' EXIT
"$@"
