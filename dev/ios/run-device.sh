#!/bin/sh
# An example on a connected iPhone: builds for aarch64-apple-ios, makes and signs its .app,
# installs and launches it with its console (the host's frame stats and panics).
#   dev/ios/run-device.sh [--release] [--example NAME, default hellorust]
# From the environment, else found:
#   IOS_DEVICE    devicectl id or UDID; default: the first connected iPhone
#   IOS_IDENTITY  codesigning identity; default: the first "Apple Development" one
#   IOS_PROFILE   provisioning profile; default: the newest development profile that allows
#                 net.drawnui.<example> (exactly or by its team's wildcard) and this device
# Skia comes from target/skia-bin (a source build for aarch64-apple-ios with `metal`).
set -eu
cd "$(dirname "$0")/../.."
root="$PWD"

profile=debug
crate=hellorust
extra_plist=
while [ $# -gt 0 ]; do
    case "$1" in
        --release) profile=release ;;
        --example) crate="$2"; shift ;;
        *) echo "unknown option $1"; exit 1 ;;
    esac
    shift
done
. "$root/dev/ios/apps.sh"
target=aarch64-apple-ios
platform=iPhoneOS
. "$root/dev/ios/bundle.sh"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# The device: devicectl's identifier and its hardware UDID (profiles list UDIDs).
devices="$work/devices.json"
xcrun devicectl list devices --json-output "$devices" >/dev/null
read_device() {
    python3 - "$devices" "${IOS_DEVICE:-}" <<'PY'
import json, sys
wanted = sys.argv[2]
for d in json.load(open(sys.argv[1]))["result"]["devices"]:
    hw, props = d.get("hardwareProperties", {}), d.get("connectionProperties", {})
    if hw.get("deviceType") != "iPhone":
        continue
    ids = (d.get("identifier"), hw.get("udid"))
    if wanted and wanted not in ids:
        continue
    if not wanted and props.get("tunnelState") in ("disconnected", "unavailable"):
        continue
    print(d["identifier"], hw.get("udid"), d.get("deviceProperties", {}).get("name", "?"))
    break
PY
}
set -- $(read_device)
[ $# -ge 2 ] || { echo "no connected iPhone (IOS_DEVICE=${IOS_DEVICE:-})"; exit 1; }
device=$1 udid=$2 name=$3

identity="${IOS_IDENTITY:-$(security find-identity -v -p codesigning | grep -m1 'Apple Development' | sed -E 's/.*"(.*)"/\1/')}"
[ -n "$identity" ] || { echo "no Apple Development identity"; exit 1; }

# The profile: development (get-task-allow), the app's id or its team's wildcard,
# this device listed, not expired; the newest such one.
if [ -z "${IOS_PROFILE:-}" ]; then
    IOS_PROFILE=$(python3 - "$udid" "$bundle_id" <<'PY'
import glob, os, plistlib, subprocess, sys, datetime
udid, bundle_id, best = sys.argv[1], sys.argv[2], None
folders = ["~/Library/Developer/Xcode/UserData/Provisioning Profiles", "~/Library/MobileDevice/Provisioning Profiles"]
for path in (p for f in folders for p in glob.glob(os.path.join(os.path.expanduser(f), "*.mobileprovision"))):
    raw = subprocess.run(["security", "cms", "-D", "-i", path], capture_output=True).stdout
    try:
        p = plistlib.loads(raw)
    except Exception:
        continue
    e = p.get("Entitlements", {})
    app_id = e.get("application-identifier", "")
    team = (p.get("TeamIdentifier") or [""])[0]
    if not e.get("get-task-allow") or udid not in p.get("ProvisionedDevices", []):
        continue
    if app_id not in (f"{team}.{bundle_id}", f"{team}.*"):
        continue
    if p["ExpirationDate"] < datetime.datetime.now():
        continue
    if best is None or p["CreationDate"] > best[0]:
        best = (p["CreationDate"], path)
print(best[1] if best else "")
PY
)
fi
[ -n "$IOS_PROFILE" ] || { echo "no development profile for $bundle_id with this iPhone ($udid)"; exit 1; }

# Entitlements from the profile, the app id narrowed from a wildcard to ours.
security cms -D -i "$IOS_PROFILE" > "$work/profile.plist"
python3 - "$work/profile.plist" "$work/entitlements.plist" "$bundle_id" <<'PY'
import plistlib, sys
p = plistlib.load(open(sys.argv[1], "rb"))
e = p["Entitlements"]
team = p["TeamIdentifier"][0]
out = {
    "application-identifier": f"{team}.{sys.argv[3]}",
    "com.apple.developer.team-identifier": team,
    "get-task-allow": e.get("get-task-allow", True),
}
plistlib.dump(out, open(sys.argv[2], "wb"))
PY
cp "$IOS_PROFILE" "$app/embedded.mobileprovision"
codesign --force --sign "$identity" --entitlements "$work/entitlements.plist" --timestamp=none "$app"

echo "installing on $name ($device), signed by $identity"
xcrun devicectl device install app --device "$device" "$app"
echo "launching (Ctrl+C stops the console; the app keeps running)"
xcrun devicectl device process launch --device "$device" --terminate-existing --console "$bundle_id"
