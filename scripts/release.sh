#!/usr/bin/env bash
# Build, sign and publish an Atelier release (see RELEASING.md).
#
#   scripts/release.sh 0.2.0 "What changed, one line per item"
#   ATELIER_DRY_RUN=1 scripts/release.sh 0.2.0 "…"   # build + latest.json only
#
# Needs the updater signing key, from the environment or the gitignored
# .env.release:  TAURI_SIGNING_PRIVATE_KEY_PATH (or TAURI_SIGNING_PRIVATE_KEY)
#                TAURI_SIGNING_PRIVATE_KEY_PASSWORD
set -euo pipefail

REPO="Mukashif/atelier"
cd "$(dirname "$0")/.."

VERSION="${1:-}"
NOTES="${2:-}"
DRY_RUN="${ATELIER_DRY_RUN:-0}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "usage: scripts/release.sh <x.y.z> \"notes\"" >&2; exit 2; }

[[ -f .env.release ]] && set -a && source .env.release && set +a
if [[ -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" && -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  export TAURI_SIGNING_PRIVATE_KEY="$(cat "$TAURI_SIGNING_PRIVATE_KEY_PATH")"
fi
[[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]] || { echo "No signing key. Set TAURI_SIGNING_PRIVATE_KEY_PATH in .env.release (see RELEASING.md)." >&2; exit 1; }

PUBKEY=$(node -p 'require("./src-tauri/tauri.conf.json").plugins.updater.pubkey')
[[ -n "$PUBKEY" ]] || { echo "tauri.conf.json has no updater pubkey. Run scripts/set-pubkey.sh first." >&2; exit 1; }

if [[ "$DRY_RUN" != "1" ]]; then
  [[ -z "$(git status --porcelain)" ]] || { echo "Working tree not clean; commit first." >&2; exit 1; }
  [[ "$(git rev-parse --abbrev-ref HEAD)" == "main" ]] || { echo "Release from main." >&2; exit 1; }
  ! git rev-parse -q --verify "refs/tags/v$VERSION" >/dev/null || { echo "Tag v$VERSION already exists." >&2; exit 1; }
fi

echo "==> Setting version $VERSION"
node -e '
  const fs = require("fs"), v = process.argv[1];
  for (const f of ["package.json", "src-tauri/tauri.conf.json"]) {
    const j = JSON.parse(fs.readFileSync(f, "utf8")); j.version = v;
    fs.writeFileSync(f, JSON.stringify(j, null, 2) + "\n");
  }
  const c = fs.readFileSync("Cargo.toml", "utf8").replace(/(\[workspace\.package\][^\[]*?\nversion = )"[^"]*"/, `$1"${v}"`);
  fs.writeFileSync("Cargo.toml", c);
' "$VERSION"

echo "==> Tests"
cargo test -q -p atelier-core

echo "==> Building signed installer"
npx tauri build --bundles nsis --config '{"bundle":{"createUpdaterArtifacts":true}}'

BUNDLE="target/release/bundle/nsis"
INSTALLER=$(ls "$BUNDLE"/*_"$VERSION"_x64-setup.exe)
SIG="$INSTALLER.sig"
[[ -f "$SIG" ]] || { echo "Missing signature $SIG" >&2; exit 1; }
NAME=$(basename "$INSTALLER")

echo "==> Writing latest.json"
node -e '
  const fs = require("fs");
  const [version, notes, sigFile, name, repo, out] = process.argv.slice(1);
  fs.writeFileSync(out, JSON.stringify({
    version, notes, pub_date: new Date().toISOString(),
    platforms: { "windows-x86_64": {
      signature: fs.readFileSync(sigFile, "utf8").trim(),
      url: `https://github.com/${repo}/releases/download/v${version}/${name}`,
    } },
  }, null, 2) + "\n");
' "$VERSION" "$NOTES" "$SIG" "$NAME" "$REPO" "$BUNDLE/latest.json"

if [[ "$DRY_RUN" == "1" ]]; then
  echo "==> Dry run: nothing committed or published."
  echo "    $INSTALLER"
  echo "    $SIG"
  echo "    $BUNDLE/latest.json"
  git checkout -- package.json src-tauri/tauri.conf.json Cargo.toml 2>/dev/null || true
  exit 0
fi

echo "==> Committing, tagging, publishing"
git add package.json src-tauri/tauri.conf.json Cargo.toml Cargo.lock
git commit -m "Release $VERSION"
git tag -a "v$VERSION" -m "Atelier $VERSION"
git push origin main "v$VERSION"
gh release create "v$VERSION" "$INSTALLER" "$SIG" "$BUNDLE/latest.json" \
  --repo "$REPO" --title "Atelier $VERSION" --notes "${NOTES:-Atelier $VERSION}"
echo "==> Released $VERSION"
