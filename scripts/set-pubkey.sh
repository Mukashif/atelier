#!/usr/bin/env bash
# Put the updater public key into src-tauri/tauri.conf.json.
#   scripts/set-pubkey.sh [path to .key.pub]   (default: ~/.tauri/atelier.key.pub)
set -euo pipefail
cd "$(dirname "$0")/.."
PUB="${1:-$HOME/.tauri/atelier.key.pub}"
[[ -f "$PUB" ]] || { echo "Public key not found: $PUB" >&2; exit 1; }
node -e '
  const fs = require("fs"), f = "src-tauri/tauri.conf.json";
  const j = JSON.parse(fs.readFileSync(f, "utf8"));
  j.plugins.updater.pubkey = fs.readFileSync(process.argv[1], "utf8").trim();
  fs.writeFileSync(f, JSON.stringify(j, null, 2) + "\n");
  console.log("Public key set in " + f);
' "$PUB"
