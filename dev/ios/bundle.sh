# Sourced by run-sim.sh and run-device.sh: builds an example for $target ($profile) and makes
# $app = target/$target/$profile/<Name>.app (binary, its assets, its icon, Info.plist).
# In: root, target, profile, platform (iPhoneSimulator | iPhoneOS), extra_plist (more keys), and
# from apps.sh: crate (the example), name (shown under the icon), bundle_id, bundle (the .app's
# file name), launch_color (RRGGBB).

# Skia: the prebuilt archive from the DrawnUi/rust-skia release of the pinned drawnui-skia-bindings
# (its default address), or SKIA_BINARIES_URL when the shell sets one (a local
# file://.../skia-binaries-{key}.tar.gz). Forced, so the patched ../rust-skia checkout downloads
# instead of compiling Skia from source.
export FORCE_SKIA_BINARIES_DOWNLOAD=1

if [ "$profile" = release ]; then
    cargo build -p "$crate" --target "$target" --release
else
    cargo build -p "$crate" --target "$target"
fi

out="$root/target/$target/$profile"
app="$out/$bundle.app"
rm -rf "$app"
mkdir -p "$app"
cp "$out/$crate" "$app/$crate"
# The example's own assets (target/<profile>/assets is shared by every example built there).
cp -R "$root/examples/$crate/assets" "$app/assets"

# The icon from the Windows .ico: its largest image, scaled to the iPhone app icon sizes.
icon_src="$out/icon-src.png"
sips -s format png "$root/examples/$crate/icon.ico" --out "$icon_src" >/dev/null
sips -z 120 120 "$icon_src" --out "$app/AppIcon60x60@2x.png" >/dev/null
sips -z 180 180 "$icon_src" --out "$app/AppIcon60x60@3x.png" >/dev/null
rm -f "$icon_src"

# The launch screen's color (`launch_color`): UILaunchScreen takes it from a named color, which only
# an asset catalog holds (compiled into Assets.car).
assets="$out/$bundle-assets.xcassets"
rm -rf "$assets"
mkdir -p "$assets/LaunchBackground.colorset"
printf '{"info":{"version":1,"author":"xcode"}}' > "$assets/Contents.json"
cat > "$assets/LaunchBackground.colorset/Contents.json" <<JSON
{"info":{"version":1,"author":"xcode"},"colors":[{"idiom":"universal","color":{"color-space":"srgb","components":{"red":"0x$(echo "$launch_color" | cut -c1-2)","green":"0x$(echo "$launch_color" | cut -c3-4)","blue":"0x$(echo "$launch_color" | cut -c5-6)","alpha":"1"}}}]}
JSON
case "$platform" in iPhoneSimulator) sdk=iphonesimulator ;; *) sdk=iphoneos ;; esac
xcrun actool --compile "$app" --platform "$sdk" --minimum-deployment-target 14.0 \
    --target-device iphone --target-device ipad --output-format human-readable-text \
    --output-partial-info-plist "$out/$bundle-assets.plist" "$assets" >/dev/null
rm -rf "$assets" "$out/$bundle-assets.plist"

cat > "$app/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>$crate</string>
    <key>CFBundleIdentifier</key><string>$bundle_id</string>
    <key>CFBundleName</key><string>$bundle</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>CFBundleShortVersionString</key><string>0.0.1</string>
    <key>CFBundleSupportedPlatforms</key><array><string>$platform</string></array>
    <key>LSRequiresIPhoneOS</key><true/>
    <key>MinimumOSVersion</key><string>14.0</string>
    <key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
    <key>UILaunchScreen</key><dict><key>UIColorName</key><string>LaunchBackground</string></dict>
    <key>UIRequiresFullScreen</key><true/>
    <!-- ProMotion: up to 120 Hz for the display link (drawnui::set_max_fps caps it). -->
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
$extra_plist
</dict>
</plist>
PLIST
