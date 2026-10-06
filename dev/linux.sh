#!/usr/bin/env bash
# DrawnUi.Rust on Linux (also inside WSL, where WSLg shows the window).
#   dev/linux.sh skia             -> builds Skia once from ../rust-skia, exports the archive to target/skia-bin
#   dev/linux.sh run [example]    -> release build + run (default hellorust), assets from the example folder
#   dev/linux.sh build [example]  -> release build only
#   dev/linux.sh test             -> cargo test -p drawnui -p hellorust
# Needs: clang 16+, ninja, python3, pkg-config, rsync, the fontconfig / freetype, X11 / xkbcommon /
# wayland and GL / EGL development packages (solved-linux.md 1, 2).
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
cache=${DRAWNUI_LINUX_CACHE:-$HOME/.cache/drawnui-linux}
# The target dir on a Linux filesystem: inside WSL a build on the Windows drive (/mnt/c) is slow.
export CARGO_TARGET_DIR=$cache/target
# .cargo/config.toml names the Windows path of the Skia archives; this checkout's path instead.
export SKIA_BINARIES_URL="file://$repo/target/skia-bin/skia-binaries-{key}.tar.gz"
export FORCE_SKIA_BINARIES_DOWNLOAD=1

# A checkout on the Windows drive (WSL) is built from a copy on the Linux filesystem, synced by
# content: WSL's clock can run ahead of Windows' (a minute here), so a file edited on Windows right
# after a build looks older than the build and cargo would skip it. Changed files get the current
# time in the copy; the sibling checkouts the workspace patches in are linked.
src=$repo
if [[ $repo == /mnt/* ]]; then
  src=$cache/src/$(basename "$repo")
  mkdir -p "$src"
  rsync -rlc --delete --exclude /target/ --exclude /.git/ "$repo/" "$src/"
  for sibling in rust-skia android-activity winit; do
    [[ -e $repo/../$sibling ]] && ln -sfn "$(cd "$repo/../$sibling" && pwd)" "$cache/src/$sibling"
  done
fi
cd "$src"

case "${1:-run}" in
  skia)
    stage=$(mktemp -d)
    FORCE_SKIA_BUILD=1 BUILD_ARTIFACTSTAGINGDIRECTORY="$stage" cargo build -p drawnui --release
    key=$(tr -d '[:space:]' < "$stage/skia-binaries/key.txt")
    mkdir -p "$repo/target/skia-bin"
    tar -czf "$repo/target/skia-bin/skia-binaries-$key.tar.gz" -C "$stage" skia-binaries
    rm -rf "$stage"
    ls -l "$repo/target/skia-bin/skia-binaries-$key.tar.gz"
    ;;
  build)
    cargo build -p "${2:-hellorust}" --release
    ;;
  run)
    example=${2:-hellorust}
    cargo build -p "$example" --release
    cd "$src/examples/$example"
    exec "$CARGO_TARGET_DIR/release/$example"
    ;;
  test)
    cargo test -p drawnui -p hellorust --no-fail-fast
    ;;
  *)
    echo "usage: dev/linux.sh skia | build [example] | run [example] | test" >&2
    exit 2
    ;;
esac
