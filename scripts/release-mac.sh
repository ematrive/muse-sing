#!/usr/bin/env bash
# Builds Muse Sing for macOS: universal .app + .dmg (Apple Silicon and Intel), into release/.
# Signed with a Developer ID when APPLE_SIGNING_IDENTITY is set (see connector's
# docs/tecnico/rilascio-macos.md), otherwise ad-hoc: the first launch then needs
#   xattr -cr "/Applications/Muse Sing.app"
set -euo pipefail
cd "$(dirname "$0")/.."

red() { printf '\033[31m%s\033[0m\n' "$*"; }
step() { printf '\n\033[1;34m▸ %s\033[0m\n' "$*"; }
fail() { red "✗ $*"; exit 1; }

[[ "$(uname -s)" == "Darwin" ]] || fail "va eseguito su macOS"
VERSION=$(node -p "require('./package.json').version")
step "Muse Sing $VERSION"

step "Dipendenze e test"
pnpm install
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
(cd src-tauri && cargo test --quiet)

if [[ -f .env.apple ]]; then set -a; source .env.apple; set +a; fi
if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  export APPLE_SIGNING_IDENTITY="-"
  red "! nessun certificato Developer ID: firma ad-hoc (al primo avvio: xattr -cr \"/Applications/Muse Sing.app\")"
fi

step "Build universale (Apple Silicon + Intel)"
pnpm tauri build --target universal-apple-darwin

BUNDLE=src-tauri/target/universal-apple-darwin/release/bundle
APP="$BUNDLE/macos/Muse Sing.app"
DMG=$(ls -t "$BUNDLE"/dmg/*.dmg | head -1)
[[ -d "$APP" && -f "$DMG" ]] || fail "pacchetto non trovato in $BUNDLE"

step "Verifiche"
EXE=$(/usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" "$APP/Contents/Info.plist")
ARCHS=$(lipo -archs "$APP/Contents/MacOS/$EXE")
[[ "$ARCHS" == *arm64* && "$ARCHS" == *x86_64* ]] || fail "l'eseguibile non è universale ($ARCHS)"
echo "architetture: $ARCHS"
codesign --verify --deep --strict "$APP" && echo "firma valida"
hdiutil verify "$DMG" >/dev/null && echo "dmg integro"

mkdir -p release
OUT="release/MuseSing-$VERSION-macOS.dmg"
cp "$DMG" "$OUT"
echo "✓ $OUT ($(du -h "$OUT" | cut -f1))"
