#!/usr/bin/env bash
# Sobe N bots headless na rede local para testar (padrão: 4). Ctrl+C derruba todos.
set -euo pipefail
count="${1:-4}"
bin="$(dirname "$0")/../target/release/pmog"
[[ -x "$bin" ]] || cargo build --release --manifest-path "$(dirname "$0")/../Cargo.toml"
trap 'kill $(jobs -p) 2>/dev/null' EXIT
for i in $(seq 1 "$count"); do
  "$bin" --bot --name "Bot$i" &
done
wait
