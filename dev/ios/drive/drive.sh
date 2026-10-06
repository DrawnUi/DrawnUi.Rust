#!/bin/sh
# Drives an app (IOS_APP, a bundle id; default net.drawnui.hellorust) on the connected iPhone, or on
# the simulator IOS_UDID names, through the Drive UI test (see Drive/Drive.swift for the commands),
# and saves its screenshots and texts.
#   dev/ios/drive/drive.sh OUT_DIR 'activate
#   tap 200 400
#   shot after-tap'
# The runner is built once: xcodebuild build-for-testing (see below), signed by the team in
# project.yml.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
out="$1"; script="$2"
udid="${IOS_UDID:-$(xcrun xctrace list devices 2>/dev/null | sed -n '/== Devices ==/,/== Devices Offline ==/p' | grep -E '\([0-9]+\.[0-9.]+\) \(' | head -1 | sed -E 's/.*\(([0-9A-F-]+)\)$/\1/')}"
derived="$root/target/ios-drive/$udid"
if [ ! -d "$derived/Build/Products" ]; then
    if xcrun simctl list devices | grep -q "$udid"; then
        # A simulator: no team, an ad-hoc signature.
        sign="CODE_SIGN_STYLE=Manual CODE_SIGN_IDENTITY=- DEVELOPMENT_TEAM="
    else
        # The team: IOS_TEAM, else that of the newest development profile listing this device.
        team="${IOS_TEAM:-$(python3 - "$udid" <<'PY'
import glob, os, plistlib, subprocess, sys
best = None
for path in glob.glob(os.path.expanduser("~/Library/Developer/Xcode/UserData/Provisioning Profiles/*.mobileprovision")) + glob.glob(os.path.expanduser("~/Library/MobileDevice/Provisioning Profiles/*.mobileprovision")):
    try:
        p = plistlib.loads(subprocess.run(["security", "cms", "-D", "-i", path], capture_output=True).stdout)
    except Exception:
        continue
    if p.get("Entitlements", {}).get("get-task-allow") and sys.argv[1] in p.get("ProvisionedDevices", []):
        if best is None or p["CreationDate"] > best[0]:
            best = (p["CreationDate"], p["TeamIdentifier"][0])
print(best[1] if best else "")
PY
    )}"
        [ -n "$team" ] || { echo "no development team for this device (set IOS_TEAM)"; exit 1; }
        sign="DEVELOPMENT_TEAM=$team"
    fi
    (cd "$here" && xcodegen generate -q)
    # shellcheck disable=SC2086
    xcodebuild build-for-testing -project "$here/Drive.xcodeproj" -scheme Drive -destination "id=$udid" \
        -derivedDataPath "$derived" $sign -quiet
fi
result="$derived/last.xcresult"
rm -rf "$result"
mkdir -p "$out"
TEST_RUNNER_DRIVE="$script" TEST_RUNNER_DRIVE_APP="${IOS_APP:-net.drawnui.hellorust}" xcodebuild test-without-building -project "$here/Drive.xcodeproj" -scheme Drive \
    -destination "id=$udid" -derivedDataPath "$derived" -resultBundlePath "$result" -quiet 2>&1 \
    | grep -v "youmi.cer\|DVTProvisioning\|IDERunDestination" || true
xcrun xcresulttool export attachments --path "$result" --output-path "$out" >/dev/null
# Name the PNGs after their `shot` names.
python3 - "$out" <<'PY'
import json, os, sys
out = sys.argv[1]
manifest = os.path.join(out, "manifest.json")
for test in json.load(open(manifest)):
    for a in test.get("attachments", []):
        src = os.path.join(out, a["exportedFileName"])
        name = a.get("suggestedHumanReadableName", a["exportedFileName"]).split("_0_")[0]
        ext = os.path.splitext(a["exportedFileName"])[1]
        dst = os.path.join(out, name if name.endswith(ext) else name + ext)
        os.replace(src, dst)
        print(dst)
os.remove(manifest)
PY
