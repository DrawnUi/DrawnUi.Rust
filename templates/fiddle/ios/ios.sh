#!/bin/sh
# iOS, on a Mac with Xcode: builds the app, makes its .app (binary, Info.plist, assets, icon) and
# runs it.
#   ios/ios.sh                 -> the iOS simulator (Apple silicon or Intel Mac)
#   ios/ios.sh device          -> the connected iPhone, signed with your Apple developer account
#   ios/ios.sh device --debug  -> a debug build (default: release)
# Simulator: a device type other than the default with IOS_SIMULATOR="iPhone 17".
# Device, from the environment, else found:
#   IOS_DEVICE    devicectl id or UDID; default: the first connected iPhone
#   IOS_IDENTITY  codesigning identity; default: the first "Apple Development" one
#   IOS_PROFILE   provisioning profile; default: the newest development profile that allows
#                 com.example.myapp (exactly or by its team's wildcard) and this device. Xcode makes
#                 one: a new iOS app project with this bundle id, run once on the iPhone.
set -eu
cd "$(dirname "$0")/.."
root="$PWD"

crate=myapp
name="My App"
bundle_id=com.example.myapp
# The launch screen's color, before the first frame (RRGGBB, the app's background).
launch_color=121218

mode=simulator
profile=release
for arg in "$@"; do
    case "$arg" in
        device) mode=device ;;
        simulator) mode=simulator ;;
        --debug) profile=debug ;;
        *) echo "unknown option $arg"; exit 1 ;;
    esac
done

if [ "$mode" = device ]; then
    target=aarch64-apple-ios
    platform=iPhoneOS
    sdk=iphoneos
elif [ "$(uname -m)" = arm64 ]; then
    target=aarch64-apple-ios-sim
    platform=iPhoneSimulator
    sdk=iphonesimulator
else
    target=x86_64-apple-ios
    platform=iPhoneSimulator
    sdk=iphonesimulator
fi
rustup target add "$target" >/dev/null 2>&1 || true

if [ "$profile" = release ]; then
    cargo build --target "$target" --release
else
    cargo build --target "$target"
fi

out="$root/target/$target/$profile"
app="$out/$crate.app"
rm -rf "$app"
mkdir -p "$app"
cp "$out/$crate" "$app/$crate"
cp -R "$root/assets" "$app/assets"

# The icon from icon.ico: its largest image, scaled to the iPhone app icon sizes.
icon_src="$out/icon-src.png"
sips -s format png "$root/icon.ico" --out "$icon_src" >/dev/null
sips -z 120 120 "$icon_src" --out "$app/AppIcon60x60@2x.png" >/dev/null
sips -z 180 180 "$icon_src" --out "$app/AppIcon60x60@3x.png" >/dev/null
rm -f "$icon_src"

# The launch screen's color: UILaunchScreen takes it from a named color, which only an asset
# catalog holds (compiled into Assets.car).
assets="$out/$crate-assets.xcassets"
rm -rf "$assets"
mkdir -p "$assets/LaunchBackground.colorset"
printf '{"info":{"version":1,"author":"xcode"}}' > "$assets/Contents.json"
cat > "$assets/LaunchBackground.colorset/Contents.json" <<JSON
{"info":{"version":1,"author":"xcode"},"colors":[{"idiom":"universal","color":{"color-space":"srgb","components":{"red":"0x$(echo "$launch_color" | cut -c1-2)","green":"0x$(echo "$launch_color" | cut -c3-4)","blue":"0x$(echo "$launch_color" | cut -c5-6)","alpha":"1"}}}]}
JSON
xcrun actool --compile "$app" --platform "$sdk" --minimum-deployment-target 14.0 \
    --target-device iphone --target-device ipad --output-format human-readable-text \
    --output-partial-info-plist "$out/$crate-assets.plist" "$assets" >/dev/null
rm -rf "$assets" "$out/$crate-assets.plist"

cat > "$app/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>$crate</string>
    <key>CFBundleIdentifier</key><string>$bundle_id</string>
    <key>CFBundleName</key><string>$name</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>CFBundleSupportedPlatforms</key><array><string>$platform</string></array>
    <key>LSRequiresIPhoneOS</key><true/>
    <key>MinimumOSVersion</key><string>14.0</string>
    <key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
    <key>UILaunchScreen</key><dict><key>UIColorName</key><string>LaunchBackground</string></dict>
    <key>UIRequiresFullScreen</key><true/>
    <!-- ProMotion: up to 120 Hz. -->
    <key>CADisableMinimumFrameDurationOnPhone</key><true/>
    <key>UISupportedInterfaceOrientations</key>
    <array>
        <string>UIInterfaceOrientationPortrait</string>
        <string>UIInterfaceOrientationLandscapeLeft</string>
        <string>UIInterfaceOrientationLandscapeRight</string>
    </array>
    <key>CFBundleIcons</key>
    <dict>
        <key>CFBundlePrimaryIcon</key>
        <dict>
            <key>CFBundleIconFiles</key><array><string>AppIcon60x60</string></array>
        </dict>
    </dict>
</dict>
</plist>
PLIST

if [ "$mode" = simulator ]; then
    # A simulator app needs no signing; the simulator accepts an ad-hoc signature.
    codesign --force --sign - --timestamp=none "$app" >/dev/null 2>&1 || true
    sim_type="${IOS_SIMULATOR:-iPhone 17 Pro}"
    # That device type on the newest runtime that still has its data: a simulator whose folder was
    # cleaned away (~/Library/Developer/CoreSimulator) is still listed but cannot boot.
    udid=
    for id in $(xcrun simctl list devices available | grep -F "    $sim_type (" | sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/'); do
        if [ -d "$HOME/Library/Developer/CoreSimulator/Devices/$id/data" ]; then udid=$id; fi
    done
    if [ -z "$udid" ]; then
        runtime=$(xcrun simctl list runtimes available | grep '^iOS' | tail -1 | sed -E 's/.* - (com\.apple\.[^ ]+).*/\1/')
        udid=$(xcrun simctl create "$sim_type" "$sim_type" "$runtime")
    fi
    xcrun simctl boot "$udid" 2>/dev/null || true
    open -a Simulator
    xcrun simctl bootstatus "$udid" -b >/dev/null
    xcrun simctl terminate "$udid" "$bundle_id" 2>/dev/null || true
    xcrun simctl install "$udid" "$app"
    echo "$name installed on the simulator; launching (Ctrl+C stops the console, not the app)"
    xcrun simctl launch --console-pty "$udid" "$bundle_id"
    exit 0
fi

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
device=$1 udid=$2 device_name=$3

identity="${IOS_IDENTITY:-$(security find-identity -v -p codesigning | grep -m1 'Apple Development' | sed -E 's/.*"(.*)"/\1/')}"
[ -n "$identity" ] || { echo "no Apple Development identity: sign in to your Apple account in Xcode"; exit 1; }

# The profile: development, the app's id or its team's wildcard, this device listed, not expired;
# the newest such one.
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
[ -n "$IOS_PROFILE" ] || { echo "no development profile for $bundle_id with this iPhone ($udid): see the top of this script"; exit 1; }

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

echo "installing on $device_name, signed by $identity"
xcrun devicectl device install app --device "$device" "$app"
echo "launching (Ctrl+C stops the console; the app keeps running)"
xcrun devicectl device process launch --device "$device" --terminate-existing --console "$bundle_id"
