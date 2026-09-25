#!/usr/bin/env bash
# Compila agentboard-mcp y lo deja en src-tauri/binaries/ con el nombre que espera Tauri
# (agentboard-mcp-<target-triple>) para empaquetarlo junto a la app (bundle.externalBin).
# Uso: scripts/build-mcp-sidecar.sh [universal]   (universal = macOS Intel + Apple Silicon)
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"
mkdir -p binaries
EXT=""; [[ "${OS:-}" == "Windows_NT" ]] && EXT=".exe"

if [[ "${1:-}" == "universal" ]]; then
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release -p agentboard-mcp --target "$t"
    cp "target/$t/release/agentboard-mcp" "binaries/agentboard-mcp-$t"
  done
  lipo -create -output binaries/agentboard-mcp-universal-apple-darwin \
    binaries/agentboard-mcp-aarch64-apple-darwin binaries/agentboard-mcp-x86_64-apple-darwin
else
  TRIPLE=$(rustc -vV | sed -n 's/^host: //p')
  cargo build --release -p agentboard-mcp
  cp "target/release/agentboard-mcp$EXT" "binaries/agentboard-mcp-$TRIPLE$EXT"
fi
ls -la binaries/
