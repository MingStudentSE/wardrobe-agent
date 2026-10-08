#!/usr/bin/env bash
# Runs INSIDE the Ubuntu build VM. Local reproduction of the Linux leg of
# .github/workflows/media-bundles.yml (Packaged media runtime), ticket #35.
#
#   bootstrap native HEIC decoder (libde265 v1.1.3 + libheif v1.23.1, same refs as CI)
#   -> cargo build --release -p wardrobe-cli with RUNPATH link args
#   -> scripts/prepare-media-distribution.py (portable CLI + private libs + tauri overlay)
#   -> optional: npm ci + npm run tauri build (deb/AppImage installers)
#
# Environment: VM provisioned by cloud-init seed (build-essential, pkg-config,
# cmake, patchelf, libwebkit2gtk-4.1-dev, rustup stable, node 22).
set -euo pipefail

SRC="${WARDROBE_SRC:-$HOME/src}"
MEDIA_ROOT="${WARDROBE_MEDIA_ROOT:-$HOME/wardrobe-media}"
DIST="${WARDROBE_DIST:-$HOME/distribution}"
ARCH="$(dpkg --print-architecture)"          # "arm64" on the local arm64 VM
TARGET_TRIPLE="$(case "$ARCH" in arm64|aarch64) echo aarch64-unknown-linux-gnu;; amd64|x86_64) echo x86_64-unknown-linux-gnu;; *) echo "unsupported arch $ARCH" >&2; exit 1;; esac)"
RUN_TAURI_BUILD="${RUN_TAURI_BUILD:-0}"

cd "$SRC"
source "$HOME/.cargo/env"

echo "== [1/4] native HEIC decoder (bootstrap-media-unix.sh, pinned libde265 v1.1.3 + libheif v1.23.1)"
if [ ! -f "$MEDIA_ROOT/install/lib/libheif.so.1" ]; then
  bash scripts/bootstrap-media-unix.sh "$MEDIA_ROOT"
else
  echo "already built at $MEDIA_ROOT/install"
fi
export PKG_CONFIG_PATH="$MEDIA_ROOT/install/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export LD_LIBRARY_PATH="$MEDIA_ROOT/install/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export RUSTFLAGS='-C link-arg=-Wl,-rpath,$ORIGIN/lib:$ORIGIN/../lib/wardrobe-agent'

echo "== [2/4] cargo build --release -p wardrobe-cli ($TARGET_TRIPLE)"
cargo build --release -p wardrobe-cli

echo "== [3/4] stage distribution (portable CLI + private decoder libs + tauri overlay)"
python3 scripts/prepare-media-distribution.py \
  "$MEDIA_ROOT/install" target/release/wardrobe-cli "$DIST" --target "$TARGET_TRIPLE"

echo "== [4/4] portable CLI smoke (verify-packaged-cli.py, same as CI)"
python3 scripts/verify-packaged-cli.py "$DIST/portable/wardrobe-cli" \
  crates/wardrobe-core/tests/fixtures/generated-colors.heic \
  --report "$DIST/standalone-runtime.json"

if [ "$RUN_TAURI_BUILD" = "1" ]; then
  echo "== [extra] tauri desktop installers (deb/AppImage); needs node 22"
  export npm_config_update_notifier=false
  cd apps/desktop
  npm ci
  npm run tauri build -- --config "$DIST/tauri-media.json"
  cd "$SRC"
fi
echo "PIPELINE-OK target=$TARGET_TRIPLE"
