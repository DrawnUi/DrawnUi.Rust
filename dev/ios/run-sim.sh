#!/bin/sh
# An example on the iOS simulator: builds for aarch64-apple-ios-sim, makes its .app (binary,
# Info.plist, assets, icon), boots a simulator, installs and launches the app.
#   dev/ios/run-sim.sh [--release] [--example NAME, default hellorust] [simulator name, default "iPhone 17 Pro"]
# Skia: the prebuilt archive of the DrawnUi/rust-skia release (see dev/ios/bundle.sh).
set -eu
cd "$(dirname "$0")/../.."
root="$PWD"

profile=debug
crate=hellorust
while [ $# -gt 0 ]; do
    case "$1" in
        --release) profile=release ;;
        --example) crate="$2"; shift ;;
        *) break ;;
    esac
    shift
done
device="${1:-iPhone 17 Pro}"
. "$root/dev/ios/apps.sh"
target=aarch64-apple-ios-sim

platform=iPhoneSimulator
extra_plist=
. "$root/dev/ios/bundle.sh"

# A simulator app needs no signing; the simulator accepts an ad-hoc signature.
codesign --force --sign - --timestamp=none "$app" >/dev/null 2>&1 || true

# Our own simulator, "DrawnUi <device>", made on the newest iOS runtime the first time: the
# stock ones can lose their data folder (a cleaned ~/Library) and then fail to boot.
sim="DrawnUi $device"
udid=$(xcrun simctl list devices available | grep -F "    $sim (" | tail -1 | sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/')
if [ -z "$udid" ]; then
    runtime=$(xcrun simctl list runtimes available | grep '^iOS' | tail -1 | sed -E 's/.* - (com\.apple\.[^ ]+).*/\1/')
    udid=$(xcrun simctl create "$sim" "$device" "$runtime")
    echo "made simulator $sim ($udid) on $runtime"
fi
device="$udid"
xcrun simctl boot "$device" 2>/dev/null || true
open -a Simulator
xcrun simctl bootstatus "$device" -b >/dev/null
xcrun simctl terminate "$device" "$bundle_id" 2>/dev/null || true
xcrun simctl install "$device" "$app"
echo "$name installed on $device; launching (Ctrl+C stops the console, not the app)"
xcrun simctl launch --console-pty "$device" "$bundle_id"
