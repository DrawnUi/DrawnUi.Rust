#!/usr/bin/env bash
# The Apple Skia archives of a release, on the Mac (dev\release.ps1 makes the rest on Windows):
#   dev/release-apple.sh
# Copies target/skia-bin's macOS, iOS and iOS simulator archives to target/release-apple/ and
# prints their SHA-256, for `gh release upload`. They must be keyed by the commit the pinned
# drawnui-skia-bindings was published from: the sibling ../rust-skia is checked out there (branch
# `drawnui-crates` of DrawnUi/rust-skia) when they are built.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
version=$(sed -n 's/.*package = "drawnui-skia-safe", version = "=\([0-9.]*\)".*/\1/p' "$root/Cargo.toml")
crate=$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/drawnui-skia-bindings-"$version" 2>/dev/null | head -1)
if [ -z "$crate" ]; then
    echo "drawnui-skia-bindings $version is not in the cargo registry: cargo fetch in an app that uses drawnui" >&2
    exit 1
fi
pin=$(sed -n 's/.*"sha1": *"\([0-9a-f]*\)".*/\1/p' "$crate/.cargo_vcs_info.json")
sibling=$(git -C "$root/../rust-skia" rev-parse HEAD)
if [ "$sibling" != "$pin" ]; then
    echo "../rust-skia is at $sibling, drawnui-skia-bindings $version was published from $pin: check out DrawnUi/rust-skia $pin" >&2
    exit 1
fi
key=$(echo "$pin" | cut -c1-20)
features=ganesh-gl-jpegd-jpege-metal-pdf-svg-textlayout
out="$root/target/release-apple"
rm -rf "$out" && mkdir -p "$out"
missing=0
for target in aarch64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim; do
    from="$root/target/skia-bin/skia-binaries-$key-$target-$features.tar.gz"
    if [ -f "$from" ]; then
        cp "$from" "$out/"
    else
        echo "missing: $(basename "$from")"
        missing=1
    fi
done
(cd "$out" && shasum -a 256 ./*.tar.gz 2>/dev/null | sed 's| \./| |')
echo "key $key (DrawnUi/rust-skia $pin) -> $out"
exit $missing
